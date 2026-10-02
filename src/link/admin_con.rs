//! admin-con, the management plane's connector (Spec sections 7.2 and 8):
//! beside the agent as its own unprivileged service user, a client of the
//! server's listener over the link with the admin credential the register
//! verb minted. It tails the agent's trace file, relays every event with its
//! position, replays from the acknowledged position on reconnect, and marks
//! every discontinuity; it declares its ceiling in the hello, exactly what
//! its invoker's `grants` answers; and it answers verb asks through that
//! invoker, one at a time, each answer placed in the stream at its
//! invocation.
//!
//! **There is no privileged code here.** The verbs reach admin through the
//! interface `toddwbucy/WeaverAgent#50` settles, behind the [`Invoker`]
//! trait; the only implementation this crate ships, [`NoVerbs`], answers an
//! empty `grants` and runs nothing, so the ceiling is empty, the server asks
//! nothing, and admission completes at `caught_up` (Spec 8). The trace file
//! is read through group read access and never written.
//!
//! **One task owns the tailer and the connection's outbound stream**, so
//! everything admin-con sends, file events and verb answers alike, is one
//! ordered stream (Spec 7.2): an answer is emitted after a drain to the
//! file's tail and before anything read after the invocation, and only one
//! verb runs at a time.

use crate::link::client::{
    self, Backoff, Connection, Ended, Incoming, Link, LinkConfig, LinkStatus,
};
use crate::link::frames::{
    FromClient, LINE_BOUND, Plane, Position, Principal, ToClient, VerbFault, VerbOutcome,
};
use crate::traceview::{TraceEvent, parse_line};
use futures::StreamExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// The tail relayed after a server restart, when the config names none
/// (Spec 7.2).
pub const DEFAULT_BACKFILL_BYTES: u64 = 1024 * 1024;
/// The most a config may name, so a backfill stays bounded.
pub const MAX_BACKFILL_BYTES: u64 = 256 * 1024 * 1024;
/// How often the tailer looks at the file when nothing is pending.
pub const DEFAULT_POLL: Duration = Duration::from_millis(250);
/// A record longer than this is not relayed: its event, re-encoded inside a
/// frame, could pass the link's line bound. It is marked instead.
pub const RECORD_BOUND: usize = LINE_BOUND / 2;
/// The digest covers the record ending at an offset, or its last this many
/// bytes where it is longer (Spec 7.2: the bytes immediately before the
/// offset).
pub const DIGEST_WINDOW: usize = 64 * 1024;
/// Bytes of records read per step, so a long backlog interleaves with the
/// connection's reads and the server's acknowledgements.
const READ_BUDGET: usize = 1024 * 1024;
/// Verb asks waiting behind the one in flight.
pub const VERB_QUEUE: usize = 16;
/// The bound on one invocation: the invoker's, which caps the pause and
/// the wait (Spec 7.2).
pub const VERB_BOUND: Duration = Duration::from_secs(300);
/// Verbs running at once on one connection (Spec 7.2): one, so invocation
/// spans never overlap and an older answer can never follow a newer one.
pub const VERBS_IN_FLIGHT: usize = 1;
/// How long shutdown lets a verb in flight finish.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

// ---------- the invoker ----------

/// **The verb plane's one reach to admin** (Spec 7.2, 8): `grants` answers
/// which verbs admin-con's role permits on this agent, and `run` invokes
/// one. The real implementation waits on WeaverAgent #50; this crate carries
/// no privilege code.
pub trait Invoker: Send + Sync + 'static {
    /// The verbs the caller's role permits on this agent, read-only.
    fn grants(&self) -> impl Future<Output = anyhow::Result<Vec<String>>> + Send;
    /// Run one verb on this agent for a principal, answering admin's object.
    fn run(
        &self,
        agent: &str,
        verb: &str,
        principal: &Principal,
    ) -> impl Future<Output = VerbOutcome> + Send;
}

/// **The only invoker this crate ships**: an empty `grants`, so the
/// ceiling is empty and the server asks nothing (Spec 8). It is the honest
/// declaration of what the box grants until WeaverAgent #50 lands, and its
/// `run` is never reached, since nothing is inside the ceiling.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoVerbs;

impl Invoker for NoVerbs {
    async fn grants(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn run(&self, agent: &str, verb: &str, _principal: &Principal) -> VerbOutcome {
        VerbOutcome {
            verb: verb.to_owned(),
            agent: agent.to_owned(),
            exit_code: None,
            answer: None,
            raw_stdout: None,
            stderr: Some("no verb runs until WeaverAgent #50 lands".to_owned()),
            timed_out: false,
        }
    }
}

// ---------- the config ----------

/// admin-con's config: what `weaver-web register` wrote, plus the trace
/// file's path, a box fact filled at install with no default, and the
/// backfill bound for the first connection after a server restart.
#[derive(Debug, Clone, Deserialize)]
pub struct AdminConConfig {
    #[serde(flatten)]
    pub link: LinkConfig,
    /// The agent's trace file, read by group read and never written.
    pub trace_file: PathBuf,
    /// How much of the file's tail is relayed after a server restart.
    #[serde(default = "default_backfill")]
    pub backfill_bytes: u64,
    /// How often the tailer looks at the file. Not a config member:
    /// `DEFAULT_POLL`, settable in code so a test can reach it.
    #[serde(skip, default = "default_poll")]
    pub poll: Duration,
}

fn default_backfill() -> u64 {
    DEFAULT_BACKFILL_BYTES
}

fn default_poll() -> Duration {
    DEFAULT_POLL
}

/// The members an admin-con config may carry: the only names a parse error
/// may print.
const MEMBERS: &[&str] = &[
    "server",
    "server_name",
    "agent",
    "plane",
    "server_certificate",
    "certificate",
    "key",
    "trace_file",
    "backfill_bytes",
];

impl AdminConConfig {
    /// Read the config under the trust rule of `client::read_private`, and
    /// refuse one minted for the gate plane, one missing a member, or a
    /// backfill past `MAX_BACKFILL_BYTES`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = client::read_private(path)?;
        let cfg: Self = client::parse_config(path, &content, MEMBERS)?;
        cfg.link
            .expect_plane(Plane::Admin)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        if cfg.backfill_bytes > MAX_BACKFILL_BYTES {
            anyhow::bail!(
                "{}: backfill_bytes is {}, over the {MAX_BACKFILL_BYTES} a backfill may relay",
                path.display(),
                cfg.backfill_bytes
            );
        }
        Ok(cfg)
    }
}

// ---------- the tailer ----------

/// The digest of the record that ends at an offset: SHA-256 over its last
/// `DIGEST_WINDOW` bytes, its delimiter included. Empty at offset zero.
fn digest_of(record: &[u8]) -> String {
    let tail = &record[record.len().saturating_sub(DIGEST_WINDOW)..];
    format!("{:x}", Sha256::digest(tail))
}

/// **The generation, from the file's durable identity and never from
/// process state** (Spec 7.2): device, inode, and birth time where the
/// filesystem reports it, so a restarted admin-con derives the same
/// generation for the same file and a reused inode after a rotation reads
/// as a new one. The form is admin-con's own and carries no trace field.
fn generation_of(meta: &std::fs::Metadata) -> String {
    let birth = meta
        .created()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| format!("{}.{:09}", d.as_secs(), d.subsec_nanos()))
        .unwrap_or_else(|| "-".to_owned());
    format!("{:x}.{:x}.{birth}", meta.dev(), meta.ino())
}

/// The generation an absent file stands under: the agent has never written
/// its trace, or the sink moved. A file that appears later is a new
/// generation, marked.
const ABSENT: &str = "absent";

/// One open generation of the trace file.
struct Held {
    file: std::fs::File,
    generation: String,
}

/// Open the trace file read-only, refusing anything but a regular file and
/// opening without blocking, so a FIFO at the path cannot hang the tailer.
fn open_trace(path: &Path) -> std::io::Result<Option<Held>> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    if !meta.file_type().is_file() {
        return Err(std::io::Error::other(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    let generation = generation_of(&meta);
    Ok(Some(Held { file, generation }))
}

/// The end of the last complete record in the file, its tail: the byte
/// after the last delimiter. `None` where the file ends in a fragment longer
/// than any record could be, which has no boundary within reach.
fn tail_of(file: &std::fs::File) -> std::io::Result<Option<u64>> {
    let len = file.metadata()?.len();
    let mut end = len;
    let floor = len.saturating_sub(RECORD_BOUND as u64 + 1);
    let mut buf = vec![0u8; 64 * 1024];
    while end > floor {
        let start = end.saturating_sub(buf.len() as u64).max(floor);
        let n = (end - start) as usize;
        file.read_exact_at(&mut buf[..n], start)?;
        if let Some(i) = buf[..n].iter().rposition(|&b| b == b'\n') {
            return Ok(Some(start + i as u64 + 1));
        }
        end = start;
    }
    Ok(if floor == 0 { Some(0) } else { None })
}

/// The digest of the record ending at `offset`, or `None` where `offset` is
/// past the file's end or not a record boundary.
fn digest_before(file: &std::fs::File, offset: u64) -> std::io::Result<Option<String>> {
    if offset == 0 {
        return Ok(Some(String::new()));
    }
    if offset > file.metadata()?.len() {
        return Ok(None);
    }
    let start = offset.saturating_sub(DIGEST_WINDOW as u64 + 1);
    let mut window = vec![0u8; (offset - start) as usize];
    file.read_exact_at(&mut window, start)?;
    if window.last() != Some(&b'\n') {
        return Ok(None);
    }
    let body = &window[..window.len() - 1];
    let record_start = body.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    Ok(Some(digest_of(&window[record_start..])))
}

/// What the tailer hands the connection.
enum Item {
    Record { position: Position, line: Vec<u8> },
    Mark { position: Position, reason: String },
}

/// The tailer: a held generation, the next record boundary to read from,
/// and the digest of the record that ends there.
struct Tailer {
    path: PathBuf,
    held: Option<Held>,
    offset: u64,
    digest: String,
    /// Inside a record longer than `RECORD_BOUND`, waiting for its end.
    skipping: bool,
    /// **A generation replaced while the link was down, still held**: its
    /// tail past the acknowledged position is relayed before the new file,
    /// since admin-con still has it (Spec 7.2).
    previous: Option<Held>,
    /// While the previous generation's tail is relayed, the current file
    /// waiting behind it.
    pending: Option<Held>,
}

impl Tailer {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            held: None,
            offset: 0,
            digest: String::new(),
            skipping: false,
            previous: None,
            pending: None,
        }
    }

    fn generation(&self) -> &str {
        self.held.as_ref().map_or(ABSENT, |h| h.generation.as_str())
    }

    fn position(&self) -> Position {
        Position {
            generation: self.generation().to_owned(),
            offset: self.offset,
            digest: self.digest.clone(),
        }
    }

    fn reset_to(&mut self, held: Option<Held>) {
        self.held = held;
        self.offset = 0;
        self.digest = String::new();
        self.skipping = false;
    }

    /// The file's current tail as a position (Spec 7.2): the hello's
    /// boundary. Opens the file where nothing is held yet, and where the
    /// held generation was replaced keeps it aside as `previous` and holds
    /// the current file, so the boundary is always the current file's.
    fn current_tail(&mut self) -> anyhow::Result<Position> {
        self.pending = None;
        if self.held.is_none() {
            self.held = open_trace(&self.path)?;
            self.previous = None;
        } else if self.replaced() {
            self.previous = self.held.take();
            self.held = open_trace(&self.path)?;
        } else {
            self.previous = None;
        }
        let Some(held) = &self.held else {
            return Ok(Position {
                generation: ABSENT.to_owned(),
                offset: 0,
                digest: String::new(),
            });
        };
        let tail = tail_of(&held.file)?.ok_or_else(|| {
            anyhow::anyhow!(
                "the trace file ends in an unterminated fragment longer than any record; not relaying until it terminates"
            )
        })?;
        let digest = digest_before(&held.file, tail)?.unwrap_or_default();
        Ok(Position {
            generation: held.generation.clone(),
            offset: tail,
            digest,
        })
    }

    /// **A rotation is a different file, not a path followed blindly**: the
    /// path is stat'd and its identity compared with the generation held.
    /// A replaced file is drained to its end first (the caller reads what
    /// remains), then switched to, from its start, with a mark; a file
    /// shrunk below the position in place is a truncation, marked and read
    /// from its start. Answers the marks, in order.
    fn check_identity(&mut self) -> anyhow::Result<Option<Item>> {
        let current = match std::fs::metadata(&self.path) {
            Ok(meta) if meta.file_type().is_file() => Some(generation_of(&meta)),
            Ok(_) => None,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        match (&self.held, current) {
            (None, Some(_)) => {
                let held = open_trace(&self.path)?;
                self.reset_to(held);
                Ok(Some(Item::Mark {
                    position: self.position(),
                    reason: "the trace file appeared; relayed from its start".to_owned(),
                }))
            }
            (Some(held), Some(generation)) if held.generation != generation => {
                let held = open_trace(&self.path)?;
                self.reset_to(held);
                Ok(Some(Item::Mark {
                    position: self.position(),
                    reason: "file replaced: rotation; the new file is relayed from its start"
                        .to_owned(),
                }))
            }
            (Some(held), _) => {
                let len = held.file.metadata()?.len();
                if len < self.offset {
                    self.offset = 0;
                    self.digest = String::new();
                    self.skipping = false;
                    Ok(Some(Item::Mark {
                        position: self.position(),
                        reason: "file shrank below the relayed position: truncation; relayed from its start"
                            .to_owned(),
                    }))
                } else {
                    Ok(None)
                }
            }
            (None, None) => Ok(None),
        }
    }

    /// Whether the held generation is no longer the path's file.
    fn replaced(&self) -> bool {
        let Some(held) = &self.held else { return false };
        match std::fs::metadata(&self.path) {
            Ok(meta) => meta.file_type().is_file() && generation_of(&meta) != held.generation,
            Err(_) => false,
        }
    }

    /// **Read complete records from the position, at most `budget` bytes and
    /// never past `until`**, each a record boundary (Spec 7.2): a record
    /// still unterminated is left for a later read. A record past
    /// `RECORD_BOUND` is skipped through its delimiter and marked.
    fn read(&mut self, until: Option<u64>, budget: usize) -> anyhow::Result<Vec<Item>> {
        let Some(held) = &self.held else {
            return Ok(Vec::new());
        };
        let len = held.file.metadata()?.len();
        let end = until.map_or(len, |u| u.min(len));
        let mut items = Vec::new();
        if end <= self.offset {
            return Ok(items);
        }
        let want = ((end - self.offset) as usize).min(budget.max(RECORD_BOUND + 1));
        let mut buf = vec![0u8; want];
        held.file.read_exact_at(&mut buf, self.offset)?;
        let mut consumed = 0usize;
        while consumed < buf.len() {
            let rest = &buf[consumed..];
            match rest.iter().position(|&b| b == b'\n') {
                Some(i) => {
                    let line = &rest[..=i];
                    let start = self.offset;
                    self.offset += line.len() as u64;
                    consumed += line.len();
                    if self.skipping || line.len() > RECORD_BOUND {
                        self.skipping = false;
                        self.digest = String::new();
                        items.push(Item::Mark {
                            position: self.position(),
                            reason: format!(
                                "a record at offset {start} passed the {RECORD_BOUND} byte bound and was not relayed"
                            ),
                        });
                        continue;
                    }
                    self.digest = digest_of(line);
                    items.push(Item::Record {
                        position: self.position(),
                        line: line.to_vec(),
                    });
                    if consumed >= budget {
                        break;
                    }
                }
                None => {
                    // No delimiter in what was read: an unterminated
                    // record, left for a later read, unless it is already
                    // past the bound, in which case its bytes are skipped.
                    if rest.len() > RECORD_BOUND {
                        self.skipping = true;
                        self.offset += rest.len() as u64;
                    }
                    break;
                }
            }
        }
        Ok(items)
    }
}

// ---------- the connection ----------

/// The events and marks a step sends, as frames.
fn frames_of(items: Vec<Item>, replayed: bool, seq: &mut u64) -> Vec<FromClient> {
    items
        .into_iter()
        .filter_map(|item| {
            *seq += 1;
            match item {
                Item::Record { position, line } => {
                    if line.iter().all(|b| b.is_ascii_whitespace()) {
                        return None;
                    }
                    let event = match std::str::from_utf8(&line) {
                        Ok(text) => parse_line(*seq, text.trim_end()),
                        Err(e) => TraceEvent {
                            seq: *seq,
                            mark: Some(format!("record is not UTF-8: {e}")),
                            run: None,
                            turn: None,
                            kind: None,
                            raw: serde_json::Value::String(
                                String::from_utf8_lossy(&line).trim_end().to_owned(),
                            ),
                        },
                    };
                    Some(FromClient::Event {
                        position,
                        replayed,
                        event,
                    })
                }
                Item::Mark { position, reason } => Some(FromClient::Event {
                    position,
                    replayed,
                    event: TraceEvent {
                        seq: *seq,
                        mark: Some(reason),
                        run: None,
                        turn: None,
                        kind: None,
                        raw: serde_json::Value::Null,
                    },
                }),
            }
        })
        .collect()
}

/// What one attempt prepared before its hello: the boundary the hello
/// names and the ceiling it declares.
struct Prepared {
    boundary: Position,
    ceiling: BTreeSet<String>,
}

/// State shared between the hello and the serve of one attempt.
struct Shared {
    tailer: Tailer,
    prepared: Option<Prepared>,
    /// The next event's sequence number for the server's window.
    seq: u64,
}

/// Run admin-con until `shutdown` is set: the client loop of `client::run`
/// with this plane's hello and serve. `source` is the config's path, re-read
/// before each retry at the cap.
pub async fn run<I: Invoker>(
    cfg: AdminConConfig,
    invoker: Arc<I>,
    source: Option<PathBuf>,
    backoff: Backoff,
    shutdown: watch::Receiver<bool>,
    status: &watch::Sender<LinkStatus>,
) -> anyhow::Result<()> {
    let link = Link::new(cfg.link.clone())?;
    let shared = Arc::new(tokio::sync::Mutex::new(Shared {
        tailer: Tailer::new(cfg.trace_file.clone()),
        prepared: None,
        seq: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            * 1_000_000,
    }));
    let reload = || {
        let path = source.as_ref()?;
        match AdminConConfig::load(path) {
            Ok(fresh) => Some(fresh.link),
            Err(e) => {
                tracing::warn!("re-reading the config: {e:#}; keeping the credential in hand");
                None
            }
        }
    };
    let hello_shared = shared.clone();
    let hello_invoker = invoker.clone();
    let serve_shutdown = shutdown.clone();
    let opts = ServeOptions {
        agent: cfg.link.agent.clone(),
        backfill: cfg.backfill_bytes,
        poll: cfg.poll,
    };
    client::run(
        link,
        |link: &LinkConfig| {
            let agent = link.agent.clone();
            let shared = hello_shared.clone();
            let invoker = hello_invoker.clone();
            async move {
                // **The ceiling is exactly what `grants` answers** (Spec 8);
                // an ask that fails declares nothing rather than guessing.
                let ceiling: BTreeSet<String> = match invoker.grants().await {
                    Ok(verbs) => verbs.into_iter().collect(),
                    Err(e) => {
                        tracing::warn!("the grants ask failed ({e:#}); declaring an empty ceiling");
                        BTreeSet::new()
                    }
                };
                let mut shared = shared.lock().await;
                let boundary = shared
                    .tailer
                    .current_tail()
                    .map_err(|e| format!("reading the trace file's tail: {e:#}"))?;
                shared.prepared = Some(Prepared {
                    boundary: boundary.clone(),
                    ceiling: ceiling.clone(),
                });
                Ok(FromClient::Hello {
                    agent,
                    plane: Plane::Admin,
                    tail: Some(boundary),
                    ceiling: Some(ceiling.into_iter().collect()),
                })
            }
        },
        reload,
        backoff,
        shutdown,
        status,
        |conn| {
            serve(
                conn,
                shared.clone(),
                invoker.clone(),
                opts.clone(),
                serve_shutdown.clone(),
            )
        },
    )
    .await;
    Ok(())
}

#[derive(Clone)]
struct ServeOptions {
    agent: String,
    backfill: u64,
    poll: Duration,
}

/// A verb ask waiting its turn.
struct Ask {
    id: u64,
    verb: String,
    principal: Principal,
}

async fn serve<I: Invoker>(
    mut conn: Connection,
    shared: Arc<tokio::sync::Mutex<Shared>>,
    invoker: Arc<I>,
    opts: ServeOptions,
    mut shutdown: watch::Receiver<bool>,
) -> Ended {
    let mut shared = shared.lock().await;
    let ended = relay(&mut conn, &mut shared, &*invoker, &opts, &mut shutdown).await;
    drop(shared);
    conn.close().await;
    ended
}

/// Send frames in order, bounded per frame by the connection's cadence.
async fn send_all(conn: &Connection, frames: Vec<FromClient>) -> Result<(), Ended> {
    for frame in frames {
        conn.send(frame).await.map_err(Ended::Lost)?;
    }
    Ok(())
}

/// Where the replay starts, and the marks in front of it, from the hello's
/// answer (Spec 7.2).
fn resume(
    tailer: &mut Tailer,
    acknowledged: Option<&Position>,
    boundary: &Position,
    backfill: u64,
) -> anyhow::Result<Vec<Item>> {
    let mut marks = Vec::new();
    match acknowledged {
        // **A server that restarted answers with no position**: a bounded
        // tail of the file is relayed, a mark at the front saying what was
        // not.
        None => {
            let start = boundary.offset.saturating_sub(backfill);
            let Some(held) = &tailer.held else {
                tailer.reset_to(None);
                return Ok(marks);
            };
            let aligned = if start == 0 {
                0
            } else {
                // The first record boundary at or after the start.
                let mut buf = vec![0u8; (RECORD_BOUND + 1).min((boundary.offset - start) as usize)];
                held.file.read_exact_at(&mut buf, start - 1)?;
                match buf.iter().position(|&b| b == b'\n') {
                    Some(i) => start - 1 + i as u64 + 1,
                    None => boundary.offset,
                }
            };
            tailer.offset = aligned;
            tailer.digest = digest_before(&held.file, aligned)?.unwrap_or_default();
            tailer.skipping = false;
            if aligned > 0 {
                marks.push(Item::Mark {
                    position: tailer.position(),
                    reason: format!(
                        "the server holds no acknowledged position (a first connection, or a server restart): backfill starts {aligned} bytes into the file, and the bytes before it were not relayed"
                    ),
                });
            }
        }
        Some(acked) if acked.generation == tailer.generation() => {
            let ok = match &tailer.held {
                Some(held) => {
                    acked.offset <= boundary.offset
                        && digest_before(&held.file, acked.offset)?.as_deref()
                            == Some(acked.digest.as_str())
                }
                None => acked.offset == 0,
            };
            if ok {
                tailer.offset = acked.offset;
                tailer.digest = acked.digest.clone();
                tailer.skipping = false;
            } else {
                tailer.offset = 0;
                tailer.digest = String::new();
                tailer.skipping = false;
                marks.push(Item::Mark {
                    position: tailer.position(),
                    reason: format!(
                        "the file was truncated or rewritten below the acknowledged position {}; relayed from its start",
                        acked.offset
                    ),
                });
            }
        }
        Some(acked) => {
            // The acknowledged generation is still held: its tail past the
            // acknowledged position is relayed first, then the new file.
            if let Some(previous) = tailer.previous.take()
                && previous.generation == acked.generation
                && digest_before(&previous.file, acked.offset)?.as_deref()
                    == Some(acked.digest.as_str())
            {
                tailer.pending = tailer.held.take();
                tailer.held = Some(previous);
                tailer.offset = acked.offset;
                tailer.digest = acked.digest.clone();
                tailer.skipping = false;
                return Ok(marks);
            }
            tailer.offset = 0;
            tailer.digest = String::new();
            tailer.skipping = false;
            marks.push(Item::Mark {
                position: tailer.position(),
                reason: if acked.generation == ABSENT {
                    "the trace file appeared while the link was down; relayed from its start"
                        .to_owned()
                } else {
                    format!(
                        "the acknowledged file ({}) was replaced while the link was down, and admin-con no longer holds its tail past {}; the new file is relayed from its start",
                        acked.generation, acked.offset
                    )
                },
            });
        }
    }
    tailer.previous = None;
    Ok(marks)
}

async fn relay<I: Invoker>(
    conn: &mut Connection,
    shared: &mut Shared,
    invoker: &I,
    opts: &ServeOptions,
    shutdown: &mut watch::Receiver<bool>,
) -> Ended {
    let Some(prepared) = shared.prepared.take() else {
        return Ended::Lost("the attempt prepared no boundary".to_owned());
    };
    let Prepared { boundary, ceiling } = prepared;
    // The tailer is at the boundary's generation: `current_tail` opened or
    // kept it. A generation replaced since is caught by the live reads.
    let acknowledged = conn.acknowledged.clone();
    let front = match resume(
        &mut shared.tailer,
        acknowledged.as_ref(),
        &boundary,
        opts.backfill,
    ) {
        Ok(marks) => marks,
        Err(e) => return Ended::Lost(format!("reading the trace file: {e:#}")),
    };
    let mut seq = shared.seq;
    let mut replaying = true;
    if let Err(end) = send_all(conn, frames_of(front, true, &mut seq)).await {
        return end;
    }
    let mut queue: VecDeque<Ask> = VecDeque::new();
    let mut in_flight = futures::stream::FuturesUnordered::new();
    let mut tick = tokio::time::interval(opts.poll);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        shared.seq = seq;
        if *shutdown.borrow_and_update() {
            return Ended::Shutdown;
        }
        // **The replay first**, from the resume point through the boundary,
        // each event marked replayed; then the frame that ends it.
        if replaying {
            let until =
                (shared.tailer.generation() == boundary.generation).then_some(boundary.offset);
            let items = match shared.tailer.read(until, READ_BUDGET) {
                Ok(items) => items,
                Err(e) => return Ended::Lost(format!("reading the trace file: {e:#}")),
            };
            let exhausted = items.is_empty();
            if let Err(end) = send_all(conn, frames_of(items, true, &mut seq)).await {
                return end;
            }
            // The previous generation's tail is relayed through its end,
            // then the current file from its start, marked.
            if exhausted && shared.tailer.pending.is_some() {
                let current = shared.tailer.pending.take();
                shared.tailer.reset_to(current);
                let mark = Item::Mark {
                    position: shared.tailer.position(),
                    reason: "file replaced: rotation; the new file is relayed from its start"
                        .to_owned(),
                };
                if let Err(end) = send_all(conn, frames_of(vec![mark], true, &mut seq)).await {
                    return end;
                }
                continue;
            }
            // Every byte before the boundary existed at the hello, so a read
            // that finds nothing more is the replay's end, even where the file
            // shrank since: the live reads then mark the truncation.
            let done = exhausted;
            if done {
                if let Err(why) = conn.send(FromClient::CaughtUp).await {
                    return Ended::Lost(why);
                }
                replaying = false;
            }
            // Between replay steps the connection is read, so the server's
            // acknowledgements never back up behind the replay.
            if let Some(end) = take_incoming(conn, &mut queue, &ceiling, &opts.agent).await {
                return end;
            }
            continue;
        }
        // **A verb, one at a time, its answer placed at the invocation**
        // (Spec 7.2): drain the file to its tail, invoke with the tailer
        // paused, emit the answer, then resume reading. A second ask waits
        // its turn in arrival order; `VERBS_IN_FLIGHT` is the rule's one
        // number.
        while in_flight.len() < VERBS_IN_FLIGHT
            && let Some(ask) = queue.pop_front()
        {
            if let Err(end) = drain(conn, &mut shared.tailer, &mut seq).await {
                return end;
            }
            shared.seq = seq;
            let agent = opts.agent.as_str();
            in_flight.push(Box::pin(async move {
                let outcome =
                    tokio::time::timeout(VERB_BOUND, invoker.run(agent, &ask.verb, &ask.principal))
                        .await
                        .ok();
                (ask.id, outcome)
            }));
        }
        if !in_flight.is_empty() {
            // The connection is still read, so asks queue and a refusal is
            // seen; the file is not, so nothing written during the
            // invocation is emitted ahead of its answer.
            tokio::select! {
                Some((id, outcome)) = in_flight.next() => {
                    if let Err(why) = conn.send(answer(id, outcome)).await {
                        return Ended::Lost(why);
                    }
                }
                incoming = conn.recv() => {
                    if let Some(end) = handle(incoming, conn, &mut queue, &ceiling, &opts.agent).await {
                        return end;
                    }
                }
                _ = shutdown.changed() => {
                    // **Shutdown lets a verb in flight finish within a
                    // grace**, its answer still going out in order.
                    let finished = tokio::time::timeout(SHUTDOWN_GRACE, async {
                        while let Some((id, outcome)) = in_flight.next().await {
                            if conn.send(answer(id, outcome)).await.is_err() {
                                return;
                            }
                        }
                    })
                    .await;
                    if finished.is_err() {
                        tracing::warn!("the verb in flight did not finish within the grace");
                    }
                    return Ended::Shutdown;
                }
            }
            continue;
        }
        // **Live**: wait for the poll, an ask, or shutdown.
        tokio::select! {
            _ = shutdown.changed() => return Ended::Shutdown,
            _ = tick.tick() => {
                if let Err(end) = live_step(conn, &mut shared.tailer, &mut seq).await {
                    return end;
                }
            }
            incoming = conn.recv() => {
                if let Some(end) = handle(incoming, conn, &mut queue, &ceiling, &opts.agent).await {
                    return end;
                }
            }
        }
    }
}

/// The answer frame for an invocation, or the typed reason it has none.
fn answer(id: u64, outcome: Option<VerbOutcome>) -> FromClient {
    match outcome {
        Some(outcome) => FromClient::Verb {
            id,
            outcome: Some(outcome),
            error: None,
        },
        None => FromClient::Verb {
            id,
            outcome: None,
            error: Some(VerbFault {
                kind: VerbFault::NOT_RUN.into(),
                message: format!("the invocation passed its {VERB_BOUND:?} bound"),
            }),
        },
    }
}

/// One live read: a rotation or truncation is checked, a replaced file is
/// drained before the switch, and up to the budget of new records is sent.
async fn live_step(conn: &Connection, tailer: &mut Tailer, seq: &mut u64) -> Result<(), Ended> {
    let lost = |e: anyhow::Error| Ended::Lost(format!("reading the trace file: {e:#}"));
    // A replaced file is read to its end before the switch, so nothing of
    // its tail is lost while admin-con still holds it.
    let draining = tailer.replaced();
    let items = tailer.read(None, READ_BUDGET).map_err(lost)?;
    let finished = items.is_empty();
    send_all(conn, frames_of(items, false, seq)).await?;
    if (!draining || finished)
        && let Some(mark) = tailer.check_identity().map_err(lost)?
    {
        send_all(conn, frames_of(vec![mark], false, seq)).await?;
    }
    Ok(())
}

/// **The drain before a verb** (Spec 7.2): every complete record up to the
/// file's tail is emitted ahead of the answer, so an unread older event
/// cannot follow a newer answer. Bounded by the backlog, which is the
/// tailer's lag and not the file.
async fn drain(conn: &Connection, tailer: &mut Tailer, seq: &mut u64) -> Result<(), Ended> {
    // **The tail is recorded first and the drain runs to it**, so a file
    // written continuously cannot hold the verb forever. A record appended
    // after the recording is read after the answer; it was written before
    // the snapshot, so the snapshot already reflects it, and applying it
    // after the answer leaves the row at the same state.
    let generation = tailer.generation().to_owned();
    let target = match &tailer.held {
        Some(held) => tail_of(&held.file)
            .map_err(|e| Ended::Lost(format!("reading the trace file: {e:#}")))?
            .unwrap_or(tailer.offset),
        None => 0,
    };
    loop {
        let before = (tailer.generation().to_owned(), tailer.offset);
        live_step(conn, tailer, seq).await?;
        let after = (tailer.generation().to_owned(), tailer.offset);
        if after == before || after.0 != generation || after.1 >= target {
            return Ok(());
        }
    }
}

/// Read whatever the server has sent without waiting, during the replay.
async fn take_incoming(
    conn: &mut Connection,
    queue: &mut VecDeque<Ask>,
    ceiling: &BTreeSet<String>,
    agent: &str,
) -> Option<Ended> {
    loop {
        let incoming = tokio::select! {
            biased;
            incoming = conn.recv() => incoming,
            _ = std::future::ready(()) => return None,
        };
        if let Some(end) = handle(incoming, conn, queue, ceiling, agent).await {
            return Some(end);
        }
    }
}

/// One frame from the server.
async fn handle(
    incoming: Incoming,
    conn: &Connection,
    queue: &mut VecDeque<Ask>,
    ceiling: &BTreeSet<String>,
    agent: &str,
) -> Option<Ended> {
    match incoming {
        Incoming::Frame(ToClient::Ack { .. }) => None,
        Incoming::Frame(ToClient::Verb {
            id,
            verb,
            principal,
        }) => {
            // **An ask outside the ceiling is the server's own defect**
            // (Spec 8): answered with a typed error naming the ceiling,
            // logged, nothing run, the connection kept.
            let fault = if !ceiling.contains(&verb) {
                tracing::error!(
                    "{agent}: the server asked {verb}, outside the declared ceiling; answered and not run"
                );
                Some(VerbFault {
                    kind: VerbFault::OUTSIDE_CEILING.into(),
                    message: format!(
                        "{verb} is outside the ceiling admin-con declared ({})",
                        if ceiling.is_empty() {
                            "empty".to_owned()
                        } else {
                            ceiling.iter().cloned().collect::<Vec<_>>().join(", ")
                        }
                    ),
                })
            } else if queue.len() >= VERB_QUEUE {
                Some(VerbFault {
                    kind: VerbFault::BUSY.into(),
                    message: format!("admin-con holds {VERB_QUEUE} asks waiting, its bound"),
                })
            } else {
                queue.push_back(Ask {
                    id,
                    verb,
                    principal,
                });
                None
            };
            match fault {
                Some(fault) => conn
                    .send(FromClient::Verb {
                        id,
                        outcome: None,
                        error: Some(fault),
                    })
                    .await
                    .err()
                    .map(Ended::Lost),
                None => None,
            }
        }
        Incoming::Frame(other) => {
            let what = client::to_client_name(&other);
            tracing::error!("protocol fault: the server sent a {what} frame on the admin plane");
            Some(Ended::Lost(format!(
                "protocol fault: a {what} frame on the admin plane"
            )))
        }
        Incoming::Refused(reason) => Some(Ended::Refused(reason)),
        Incoming::Lost(why) => Some(Ended::Lost(why)),
    }
}
