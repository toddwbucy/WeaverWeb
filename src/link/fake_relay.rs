//! A fake trace relay for tests: a Unix socket server in a temporary
//! directory speaking the relay's wire (`weaver-types-Spec` section 3.1), as
//! `weaver-trace-relay` behaves. It holds the run's file by descriptor, opened
//! at its start, so a rotation is seen only by the next run, a restart of the
//! fake; it verifies a position by the record's digest before a byte is sent,
//! refusing by closing before a header; it sends whole lines, a heartbeat
//! while idle, and `truncated` then the end where the file shrank below the
//! stream's position; and the newest connection replaces the old. No agent
//! is reached: the file is a temporary one the test writes.

use sha2::{Digest, Sha256};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinHandle;

/// The relay's waits, shortened for tests.
#[derive(Clone, Copy)]
struct Timing {
    /// Idle time before a heartbeat, the relay's five seconds shortened.
    heartbeat: Duration,
    /// How long the reader has to take a write before it is dropped.
    write: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            heartbeat: Duration::from_millis(60),
            write: Duration::from_secs(5),
        }
    }
}

const TICK: Duration = Duration::from_millis(10);
const CHUNK: usize = 64 * 1024;

/// What the fake counts, for tests that assert how the door was read.
#[derive(Default)]
pub(super) struct Counts {
    /// Connections accepted.
    pub(super) connections: AtomicUsize,
    /// Requests refused before a header.
    pub(super) refused: AtomicUsize,
    /// Bytes of the file sent, over every stream.
    pub(super) sent: AtomicUsize,
    /// While set, the relay sends nothing, neither the file nor a
    /// heartbeat: a relay behind on its reading, as a busy box's is.
    pub(super) held: std::sync::atomic::AtomicBool,
    /// While set, every stream ends right after its header: a relay that
    /// admits a reader and drops it at once.
    pub(super) end_after_header: std::sync::atomic::AtomicBool,
}

/// The fake relay for one trace file.
pub(super) struct FakeRelay {
    _dir: tempfile::TempDir,
    pub(super) socket: PathBuf,
    trace: PathBuf,
    timing: Timing,
    /// Whether the header names a birth time, as most filesystems report.
    birth: bool,
    pub(super) counts: Arc<Counts>,
    run: Option<JoinHandle<()>>,
}

impl FakeRelay {
    /// A fake relay for the trace at `trace`, not yet running: the door is
    /// closed until `start`.
    pub(super) fn new(trace: &Path) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("trace.sock");
        Self {
            _dir: dir,
            socket,
            trace: trace.to_owned(),
            timing: Timing::default(),
            birth: true,
            counts: Arc::new(Counts::default()),
            run: None,
        }
    }

    /// A header that names no birth time, as a filesystem that reports none.
    pub(super) fn without_birth(mut self) -> Self {
        self.birth = false;
        self
    }

    /// **A run starts**: the file at the trace's path is opened and held,
    /// and the door opens.
    pub(super) fn start(&mut self) {
        assert!(self.run.is_none(), "the relay already runs");
        let file = Arc::new(std::fs::File::open(&self.trace).unwrap());
        let _ = std::fs::remove_file(&self.socket);
        let listener = UnixListener::bind(&self.socket).unwrap();
        let timing = self.timing;
        let birth = self.birth;
        let counts = self.counts.clone();
        self.run = Some(tokio::spawn(async move {
            // The run's streams end with the run: the set aborts them when
            // the run's task is aborted and drops it.
            let mut follower = tokio::task::JoinSet::new();
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                counts.connections.fetch_add(1, Ordering::Relaxed);
                // **The newest connection replaces the old.**
                follower.abort_all();
                follower.spawn(serve(stream, file.clone(), timing, birth, counts.clone()));
            }
        }));
    }

    /// **The run ends**: the door closes, the socket is gone, and every
    /// stream ends.
    pub(super) async fn stop(&mut self) {
        if let Some(run) = self.run.take() {
            run.abort();
            let _ = run.await;
        }
        let _ = std::fs::remove_file(&self.socket);
    }

    /// Hold the stream, or let it go on.
    pub(super) fn hold(&self, held: bool) {
        self.counts.held.store(held, Ordering::SeqCst);
    }

    /// A new run: the file at the path is opened afresh.
    pub(super) async fn restart(&mut self) {
        self.stop().await;
        self.start();
    }
}

impl Drop for FakeRelay {
    fn drop(&mut self) {
        if let Some(run) = self.run.take() {
            run.abort();
        }
    }
}

fn line(control: serde_json::Value) -> Vec<u8> {
    let mut line = serde_json::to_vec(&serde_json::json!({ "trace_stream": control })).unwrap();
    line.push(b'\n');
    line
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// **A position verifies** where it is zero with no digest, or falls just
/// after a newline inside the file with the record ending there hashing to
/// its digest, the record's bytes from its line's start through its newline.
fn verify(file: &std::fs::File, offset: u64, digest: Option<&str>) -> bool {
    let size = file.metadata().unwrap().len();
    match (offset, digest) {
        (0, None) => true,
        (0, Some(_)) | (_, None) => false,
        (offset, Some(digest)) if offset <= size => {
            let mut before = vec![0u8; offset as usize];
            if file.read_exact_at(&mut before, 0).is_err() || before.last() != Some(&b'\n') {
                return false;
            }
            let body = &before[..before.len() - 1];
            let start = body.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
            format!("{:x}", Sha256::digest(&before[start..])) == digest
        }
        _ => false,
    }
}

async fn serve(
    stream: UnixStream,
    file: Arc<std::fs::File>,
    timing: Timing,
    birth: bool,
    counts: Arc<Counts>,
) {
    let mut stream = BufReader::new(stream);
    let mut request = Vec::new();
    let read = tokio::time::timeout(
        Duration::from_secs(5),
        stream.read_until(b'\n', &mut request),
    )
    .await;
    let parsed: Option<serde_json::Value> = match read {
        Ok(Ok(n)) if n > 0 && n <= 4096 && request.ends_with(b"\n") => {
            serde_json::from_slice(&request).ok()
        }
        _ => None,
    };
    let Some(request) = parsed else {
        counts.refused.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let offset = request["offset"].as_u64().unwrap_or(u64::MAX);
    let digest = request.get("prior_digest").and_then(|d| d.as_str());
    if !verify(&file, offset, digest) {
        counts.refused.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let meta = file.metadata().unwrap();
    let mut header = serde_json::json!({"device": meta.dev(), "inode": meta.ino()});
    if birth && let Ok(created) = meta.created() {
        let ns = created
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i128;
        header["birth_ns"] = serde_json::json!(ns);
    }
    let mut stream = stream.into_inner();
    let send = async |stream: &mut UnixStream, bytes: &[u8]| -> bool {
        matches!(
            tokio::time::timeout(timing.write, stream.write_all(bytes)).await,
            Ok(Ok(()))
        )
    };
    if !send(&mut stream, &line(serde_json::json!({ "header": header }))).await
        || counts.end_after_header.load(Ordering::SeqCst)
    {
        return;
    }
    let mut position = offset;
    let mut last_write = tokio::time::Instant::now();
    // Inside a line that outran a chunk: nothing of the relay's own is
    // written until it ends.
    let mut mid_line = false;
    loop {
        if counts.held.load(Ordering::SeqCst) {
            tokio::time::sleep(TICK).await;
            last_write = tokio::time::Instant::now();
            continue;
        }
        let size = file.metadata().unwrap().len();
        if size < position {
            // A truncation met mid-line closes without the line.
            if mid_line {
                return;
            }
            let _ = send(
                &mut stream,
                &line(serde_json::json!({"truncated": {"size": size}})),
            )
            .await;
            return;
        }
        if position < size {
            let want = ((size - position) as usize).min(CHUNK);
            let mut buf = vec![0u8; want];
            let n = file.read_at(&mut buf, position).unwrap();
            buf.truncate(n);
            // Whole lines only, unless one line outruns a chunk, or the
            // rest of a line that did.
            let take = match buf.iter().rposition(|&b| b == b'\n') {
                Some(last) => Some(last + 1),
                None if n == CHUNK || mid_line => Some(n),
                None => None,
            };
            if let Some(take) = take {
                if !send(&mut stream, &buf[..take]).await {
                    return;
                }
                counts.sent.fetch_add(take, Ordering::Relaxed);
                position += take as u64;
                last_write = tokio::time::Instant::now();
                mid_line = buf[take - 1] != b'\n';
                // A backlog goes out at the pace the reader takes it.
                tokio::task::yield_now().await;
                continue;
            }
        } else if !mid_line && last_write.elapsed() >= timing.heartbeat {
            if !send(
                &mut stream,
                &line(serde_json::json!({"heartbeat": {"wall_ms": now_ms()}})),
            )
            .await
            {
                return;
            }
            last_write = tokio::time::Instant::now();
        }
        tokio::time::sleep(TICK).await;
    }
}
