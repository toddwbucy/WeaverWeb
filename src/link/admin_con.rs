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
//! **There is no privileged code here.** The verbs reach admin behind the
//! [`Invoker`] trait, whose service implementation is the sudo invoker of
//! `link::sudo_invoker`, the one privileged invocation in this crate. This
//! module owns the order around it: the process-wide invocation slot, the
//! verb bound, and the orderly stop's `unload` (Spec 7.2, 8). The trace
//! file is read through group read access and never written.
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
pub const READ_BUDGET: usize = 1024 * 1024;
/// Verb asks waiting behind the one in flight.
pub const VERB_QUEUE: usize = 16;
/// The bound on one invocation when the config names none (Spec 7.2): it
/// must exceed the box's own load bound, 900 seconds unless the agent's
/// root names another, so a load that answers in time is never answered
/// `unknown`.
pub const VERB_BOUND: Duration = Duration::from_secs(960);
/// Verbs running at once on one connection (Spec 7.2): one, so invocation
/// spans never overlap and an older answer can never follow a newer one.
pub const VERBS_IN_FLIGHT: usize = 1;
/// The orderly stop's grace when the config names none (Spec 8): the box's
/// load bound, 900 seconds, plus the unload's 105, plus a margin, so a stop
/// that meets a `load` in flight still unloads.
pub const STOP_GRACE: Duration = Duration::from_secs(1080);

// ---------- the invoker ----------

/// **The verb plane's one reach to admin** (Spec 7.2, 8): `grants` answers
/// which verbs the box grants admin-con on this agent, and `run` invokes
/// one. The one privileged implementation is
/// [`crate::link::sudo_invoker::SudoInvoker`].
pub trait Invoker: Send + Sync + 'static {
    /// The verbs the box grants on this agent, read-only: the ceiling.
    fn grants(&self) -> impl Future<Output = anyhow::Result<Vec<String>>> + Send;
    /// Run one verb on this agent for a principal, answering admin's object
    /// or a typed fault.
    ///
    /// **A verb runs to completion** (Spec 7.2): the future is driven to its
    /// end by a task of its own, which holds admin-con's invocation slot
    /// until it ends, and admin-con answers `unknown` at its bound without
    /// dropping it. An implementation therefore owns whatever it starts
    /// until that ends, and never kills it.
    fn run(
        &self,
        agent: &str,
        verb: &str,
        principal: &Principal,
    ) -> impl Future<Output = Result<VerbOutcome, VerbFault>> + Send;
}

/// **An invoker that grants nothing**: an empty `grants`, so the ceiling is
/// empty and the server asks nothing (Spec 8), and a `run` never reached,
/// since nothing is inside the ceiling. The tests' invoker where no verb is
/// meant to run.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoVerbs;

impl Invoker for NoVerbs {
    async fn grants(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn run(
        &self,
        _agent: &str,
        verb: &str,
        _principal: &Principal,
    ) -> Result<VerbOutcome, VerbFault> {
        Err(VerbFault {
            kind: VerbFault::NOT_STARTED.into(),
            message: format!("this invoker grants nothing and runs nothing, {verb} included"),
        })
    }
}

// ---------- the config ----------

/// admin-con's config: what `weaver-web register` wrote, plus the box facts
/// filled at install with no default (the trace file's path and the
/// absolute path of `weaver-admin` the box's sudo rule names), the backfill
/// bound for the first connection after a server restart, and the verb's
/// bound and the stop's grace (Spec 7.2, 8).
#[derive(Debug, Clone, Deserialize)]
pub struct AdminConConfig {
    #[serde(flatten)]
    pub link: LinkConfig,
    /// The agent's trace file, read by group read and never written.
    pub trace_file: PathBuf,
    /// The absolute path of `weaver-admin` the box's sudo rule names, the
    /// one config value in a privileged command line beside the agent's
    /// name (Spec 7.2).
    pub weaver_admin: PathBuf,
    /// How much of the file's tail is relayed after a server restart.
    #[serde(default = "default_backfill")]
    pub backfill_bytes: u64,
    /// How often the tailer looks at the file. Not a config member:
    /// `DEFAULT_POLL`, settable in code so a test can reach it.
    #[serde(skip, default = "default_poll")]
    pub poll: Duration,
    /// The bound on one invocation, `verb_bound_secs` in the file
    /// (`VERB_BOUND` by default): it must exceed the box's load bound, so
    /// the install sets it above that (Spec 7.2). Past it admin-con answers
    /// `unknown` and the invocation runs on, holding the slot.
    #[serde(
        rename = "verb_bound_secs",
        default = "default_verb_bound",
        deserialize_with = "seconds"
    )]
    pub verb_bound: Duration,
    /// The orderly stop's grace, `stop_grace_secs` in the file
    /// (`STOP_GRACE` by default): it covers a verb in flight, a load at its
    /// bound, and the `unload` after it (Spec 8).
    #[serde(
        rename = "stop_grace_secs",
        default = "default_stop_grace",
        deserialize_with = "seconds"
    )]
    pub stop_grace: Duration,
    /// The bound on the `grants` ask at each hello. Not a config member:
    /// the hello's own bound, `client::HELLO_SECS`, settable in code so a
    /// test can reach it.
    #[serde(skip, default = "default_grants_bound")]
    pub grants_bound: Duration,
    /// A pause before each chunk of a scan that may cross a record of any
    /// length. Not a config member: zero, settable in code so a test can
    /// make a scan take time without a file of gigabytes.
    #[serde(skip)]
    pub scan_delay: Duration,
}

fn default_backfill() -> u64 {
    DEFAULT_BACKFILL_BYTES
}

fn default_poll() -> Duration {
    DEFAULT_POLL
}

fn default_verb_bound() -> Duration {
    VERB_BOUND
}

fn default_stop_grace() -> Duration {
    STOP_GRACE
}

/// A whole number of seconds, at least one.
fn seconds<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
    let secs = u64::deserialize(d)?;
    if secs == 0 {
        return Err(serde::de::Error::custom("a bound of zero seconds"));
    }
    Ok(Duration::from_secs(secs))
}

fn default_grants_bound() -> Duration {
    Duration::from_secs(client::HELLO_SECS)
}

/// The members an admin-con config may carry: the only names a parse error
/// may print.
const MEMBERS: &[&str] = &[
    "server",
    "server_name",
    "agent",
    "agent_id",
    "plane",
    "server_certificate",
    "certificate",
    "key",
    "trace_file",
    "weaver_admin",
    "backfill_bytes",
    "verb_bound_secs",
    "stop_grace_secs",
];

impl AdminConConfig {
    /// Read the config under the trust rule of `client::read_private`, and
    /// refuse one minted for the gate plane, one missing a member, a
    /// backfill past `MAX_BACKFILL_BYTES`, or a `weaver_admin` that is not
    /// absolute, since the box's sudo rule names it absolutely.
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
        if !cfg.weaver_admin.is_absolute() {
            anyhow::bail!(
                "{}: weaver_admin {} is not an absolute path",
                path.display(),
                cfg.weaver_admin.display()
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

/// What stands at the trace path, seen without following a symlink.
enum AtPath {
    Missing,
    File(String),
    /// A symlink or anything but a regular file: refused, never followed.
    Refused(&'static str),
}

/// **A symlinked sink is not supported, and the path is never followed
/// through one**: anyone who can write the trace's directory could
/// otherwise point it at admin-con's own config and have the key relayed
/// as events. The path is seen with `lstat` and opened with `O_NOFOLLOW`.
fn at_path(path: &Path) -> std::io::Result<AtPath> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Ok(AtPath::Refused(
            "the trace path is a symlink, and a symlinked sink is not supported: nothing is read through it",
        )),
        Ok(meta) if meta.file_type().is_file() => Ok(AtPath::File(generation_of(&meta))),
        Ok(_) => Ok(AtPath::Refused(
            "the trace path is not a regular file: nothing is read from it",
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AtPath::Missing),
        Err(e) => Err(e),
    }
}

/// Open the trace file read-only, without following a symlink and without
/// blocking (a FIFO at the path cannot hang the tailer), and only where it
/// is a regular file. `None` where nothing openable stands.
fn open_trace(path: &Path) -> std::io::Result<Option<Held>> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    if !meta.file_type().is_file() {
        return Ok(None);
    }
    let generation = generation_of(&meta);
    Ok(Some(Held { file, generation }))
}

/// The bytes a scan reads at a time.
const SCAN_CHUNK: usize = 64 * 1024;

/// **A scan that may cross a record of any length runs off the
/// connection's task and stops when it is no longer wanted**: the work is
/// file reads, synchronous and unbounded by the record bound, so it runs
/// under `spawn_blocking`, and the future awaiting it can be dropped by a
/// stop or the shutdown that ends a hello, which tells the scan to stop at
/// its next chunk. The task stays responsive and a stop is honoured within
/// its grace. `delay` is a test's throttle on each chunk, zero in service.
async fn off_task<T: Send + 'static>(
    delay: Duration,
    scan: impl FnOnce(&Scan) -> std::io::Result<T> + Send + 'static,
) -> std::io::Result<T> {
    struct StopOnDrop(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for StopOnDrop {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let _stop_on_drop = StopOnDrop(stop.clone());
    let scan_state = Scan { stop, delay };
    tokio::task::spawn_blocking(move || scan(&scan_state))
        .await
        .map_err(std::io::Error::other)?
}

/// What a scan checks between chunks.
struct Scan {
    stop: Arc<std::sync::atomic::AtomicBool>,
    delay: Duration,
}

impl Scan {
    fn next_chunk(&self) -> std::io::Result<()> {
        if self.stop.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "the scan was stopped",
            ));
        }
        if !self.delay.is_zero() {
            std::thread::sleep(self.delay);
        }
        Ok(())
    }
}

/// The end of the last complete record in the file, its tail (the byte
/// after the last delimiter, 0 where there is none), and the file's length.
/// **Found at any distance**, scanned back in bounded chunks, so a long
/// unterminated fragment at the end never stops a hello; run through
/// `off_task`.
fn tail_of(file: &std::fs::File, scan: &Scan) -> std::io::Result<(u64, u64)> {
    let len = file.metadata()?.len();
    let mut end = len;
    let mut buf = vec![0u8; SCAN_CHUNK];
    while end > 0 {
        scan.next_chunk()?;
        let start = end.saturating_sub(buf.len() as u64);
        let n = (end - start) as usize;
        file.read_exact_at(&mut buf[..n], start)?;
        if let Some(i) = buf[..n].iter().rposition(|&b| b == b'\n') {
            return Ok((start + i as u64 + 1, len));
        }
        end = start;
    }
    Ok((0, len))
}

/// `tail_of` off the connection's task.
async fn tail_of_off_task(file: &std::fs::File, delay: Duration) -> std::io::Result<(u64, u64)> {
    let file = file.try_clone()?;
    off_task(delay, move |scan| tail_of(&file, scan)).await
}

/// The first record boundary at or after `start` and before `end`, found
/// at any distance scanning forward in bounded chunks, or `end` where there
/// is none; run through `off_task`.
fn first_boundary(file: &std::fs::File, start: u64, end: u64, scan: &Scan) -> std::io::Result<u64> {
    if start == 0 {
        return Ok(0);
    }
    let mut at = start - 1;
    let mut buf = vec![0u8; SCAN_CHUNK];
    loop {
        if at >= end {
            return Ok(end);
        }
        scan.next_chunk()?;
        let n = ((end - at) as usize).min(buf.len());
        file.read_exact_at(&mut buf[..n], at)?;
        if let Some(i) = buf[..n].iter().position(|&b| b == b'\n') {
            return Ok(at + i as u64 + 1);
        }
        at += n as u64;
    }
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

/// A record past `RECORD_BOUND` being skipped through its delimiter: where
/// it began, and its last `DIGEST_WINDOW` bytes so far, so the mark at its
/// end carries the digest the file has there.
struct Skip {
    start: u64,
    tail: Vec<u8>,
}

impl Skip {
    fn take(&mut self, bytes: &[u8]) {
        self.tail.extend_from_slice(bytes);
        let excess = self.tail.len().saturating_sub(DIGEST_WINDOW);
        self.tail.drain(..excess);
    }
}

/// The tailer: a held generation, the next record boundary to read from,
/// and the digest of the record that ends there.
struct Tailer {
    path: PathBuf,
    held: Option<Held>,
    offset: u64,
    digest: String,
    /// Inside a record longer than `RECORD_BOUND`, waiting for its end.
    skip: Option<Skip>,
    /// **A generation replaced while the link was down, still held**: its
    /// tail past the acknowledged position is relayed before the new file,
    /// since admin-con still has it (Spec 7.2). Only the resume clears it,
    /// so a hello attempt that fails does not lose it.
    previous: Option<Held>,
    /// While the previous generation's tail is relayed, the current file
    /// waiting behind it (`Some(None)` where nothing openable stands at the
    /// path).
    pending: Option<Option<Held>>,
    /// What was refused at the path and already marked, so a refusal is
    /// marked once and not at every poll.
    refused: Option<&'static str>,
    /// The held file's length and modification time when it was last
    /// checked, so the digest is read again only where the file changed.
    seen: Option<(u64, Option<std::time::SystemTime>)>,
    /// Marks found at the hello, sent at the front of the replay.
    notes: Vec<String>,
    /// A test's throttle on each chunk of a long scan, zero in service.
    scan_delay: Duration,
    /// How many times the position has gone back to 0: see `restart`.
    restarts: u64,
}

impl Tailer {
    fn new(path: PathBuf, scan_delay: Duration) -> Self {
        Self {
            scan_delay,
            restarts: 0,
            path,
            held: None,
            offset: 0,
            digest: String::new(),
            skip: None,
            previous: None,
            pending: None,
            refused: None,
            seen: None,
            notes: Vec::new(),
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
        if held.is_some() {
            self.refused = None;
        }
        self.held = held;
        self.restart();
    }

    /// **Read the held file from its start, its next poll checked afresh**:
    /// the one place a position goes back to 0, whether the file was
    /// switched (rotation, a file appearing, a refusal) or restarted in
    /// place (truncation, rewrite). Each counts in `restarts`, which is how
    /// a drain knows its target named a file that is no longer the one
    /// being read.
    fn restart(&mut self) {
        self.offset = 0;
        self.digest = String::new();
        self.skip = None;
        self.seen = None;
        self.restarts += 1;
    }

    fn held_len(&self) -> std::io::Result<u64> {
        match &self.held {
            Some(held) => Ok(held.file.metadata()?.len()),
            None => Ok(0),
        }
    }

    /// The file's current tail as a position (Spec 7.2): the hello's
    /// boundary. The held generation, where the path's file replaced it, is
    /// kept aside as `previous` (once: an older one already kept is the one
    /// a reconnection most likely acknowledged) and the current file held,
    /// so the boundary is always the current file's. Notes what it refused
    /// and an unterminated fragment past any record's bound.
    async fn current_tail(&mut self) -> anyhow::Result<Position> {
        self.pending = None;
        self.notes.clear();
        match at_path(&self.path)? {
            AtPath::File(generation) => {
                let replaced = self
                    .held
                    .as_ref()
                    .is_some_and(|h| h.generation != generation);
                if replaced || self.held.is_none() {
                    if replaced && self.previous.is_none() {
                        self.previous = self.held.take();
                    }
                    let held = open_trace(&self.path)?;
                    self.reset_to(held);
                }
            }
            AtPath::Refused(why) => {
                if self.held.is_some() && self.previous.is_none() {
                    self.previous = self.held.take();
                }
                self.reset_to(None);
                self.refused = Some(why);
                self.notes.push(why.to_owned());
            }
            // A deleted file still held is still readable; nothing held and
            // nothing at the path is the absent generation.
            AtPath::Missing => {}
        }
        let Some(held) = &self.held else {
            return Ok(Position {
                generation: ABSENT.to_owned(),
                offset: 0,
                digest: String::new(),
            });
        };
        let (tail, len) = tail_of_off_task(&held.file, self.scan_delay).await?;
        if len - tail > RECORD_BOUND as u64 {
            self.notes.push(format!(
                "an unterminated fragment of {} bytes follows the tail at {tail}; it passes the {RECORD_BOUND} byte bound and will be marked, not relayed, once it ends",
                len - tail
            ));
        }
        let digest = digest_before(&held.file, tail)?.unwrap_or_default();
        Ok(Position {
            generation: held.generation.clone(),
            offset: tail,
            digest,
        })
    }

    /// What stands at the path in place of the held file, **sampled and
    /// never acted on**: `None` where the path is the held file, or holds
    /// nothing (a deleted file still held is still read), or holds what was
    /// already refused and marked.
    fn replacement(&self) -> std::io::Result<Option<AtPath>> {
        Ok(match at_path(&self.path)? {
            AtPath::Missing => None,
            AtPath::File(generation)
                if self
                    .held
                    .as_ref()
                    .is_some_and(|h| h.generation == generation) =>
            {
                None
            }
            AtPath::Refused(why) if self.held.is_none() && self.refused == Some(why) => None,
            other => Some(other),
        })
    }

    /// The held file was truncated below the position: marked, and read
    /// again from its start. **Wherever the file's length or modification
    /// time changed since the last look, the record before the position is
    /// checked by its digest, as a reconnection checks it**, so a copy and
    /// truncate that regrows past the position between two polls is caught
    /// though its length hides it. While a record past the bound is being
    /// skipped the position is inside it, so the bytes the skip kept, the
    /// record's last up to `DIGEST_WINDOW` read so far, are compared with
    /// the file's bytes before the position instead.
    fn truncated(&mut self) -> std::io::Result<Option<Item>> {
        let Some(held) = &self.held else {
            return Ok(None);
        };
        let meta = held.file.metadata()?;
        let seen = (meta.len(), meta.modified().ok());
        if self.seen == Some(seen) {
            return Ok(None);
        }
        self.seen = Some(seen);
        let shrunk = meta.len() < self.offset;
        let rewritten = !shrunk
            && match &self.skip {
                None => {
                    digest_before(&held.file, self.offset)?.as_deref() != Some(self.digest.as_str())
                }
                Some(skip) => {
                    let kept = skip.tail.len() as u64;
                    let mut there = vec![0u8; skip.tail.len()];
                    held.file
                        .read_exact_at(&mut there, self.offset.saturating_sub(kept))?;
                    there != skip.tail
                }
            };
        if !shrunk && !rewritten {
            return Ok(None);
        }
        let at = self.offset;
        self.restart();
        let reason = if shrunk {
            format!("the file was truncated below offset {at}; relayed from its start")
        } else {
            format!(
                "the file was truncated or rewritten below offset {at}: the record before it no longer matches its digest; relayed from its start"
            )
        };
        Ok(Some(Item::Mark {
            position: self.position(),
            reason,
        }))
    }

    /// Switch to what replaced the held file, once its tail is read.
    /// `None` where what stood at the sample is gone again by the open, so
    /// the next step samples afresh.
    fn switch_to(&mut self, change: AtPath) -> anyhow::Result<Option<Item>> {
        match change {
            AtPath::File(_) => {
                let Some(held) = open_trace(&self.path)? else {
                    return Ok(None);
                };
                let why = if self.held.is_none() {
                    "the trace file appeared; relayed from its start"
                } else {
                    "file replaced: rotation; the new file is relayed from its start"
                };
                self.switch(Some(held), why).map(Some)
            }
            AtPath::Refused(why) => {
                let mark = self.switch(None, why)?;
                self.refused = Some(why);
                Ok(Some(mark))
            }
            AtPath::Missing => Ok(None),
        }
    }

    /// Switch to `to` from its start, with a mark naming why; an
    /// unterminated fragment left in the file switched from, or a record
    /// past the bound still being skipped there, is named too.
    fn switch(&mut self, to: Option<Held>, why: &str) -> anyhow::Result<Item> {
        let from = self.skip.as_ref().map_or(self.offset, |s| s.start);
        let left = self.held_len()?.saturating_sub(from);
        self.reset_to(to);
        let reason = if left > 0 {
            format!(
                "{why}; the file switched from ended in an unterminated fragment of {left} bytes, not relayed"
            )
        } else {
            why.to_owned()
        };
        Ok(Item::Mark {
            position: self.position(),
            reason,
        })
    }

    /// **Read complete records from the position, at most `budget` bytes and
    /// never past `until`**, each a record boundary (Spec 7.2): a record
    /// still unterminated is left for a later read. A record past
    /// `RECORD_BOUND` is skipped through its delimiter, budget by budget,
    /// and marked with its real digest. Answers whether the position moved.
    fn read(&mut self, until: Option<u64>, budget: usize) -> anyhow::Result<(Vec<Item>, bool)> {
        let Some(held) = &self.held else {
            return Ok((Vec::new(), false));
        };
        let len = held.file.metadata()?.len();
        let end = until.map_or(len, |u| u.min(len));
        let mut items = Vec::new();
        if end <= self.offset {
            return Ok((items, false));
        }
        let span = if self.skip.is_some() {
            budget
        } else {
            budget.max(RECORD_BOUND + 1)
        };
        let want = ((end - self.offset) as usize).min(span);
        let mut buf = vec![0u8; want];
        held.file.read_exact_at(&mut buf, self.offset)?;
        let from = self.offset;
        let mut consumed = 0usize;
        while consumed < buf.len() {
            let rest = &buf[consumed..];
            let newline = rest.iter().position(|&b| b == b'\n');
            if let Some(skip) = &mut self.skip {
                match newline {
                    Some(i) => {
                        skip.take(&rest[..=i]);
                        self.offset += i as u64 + 1;
                        consumed += i + 1;
                        self.digest = digest_of(&skip.tail);
                        let start = skip.start;
                        self.skip = None;
                        items.push(Item::Mark {
                            position: self.position(),
                            reason: format!(
                                "a record at offset {start} of {} bytes passed the {RECORD_BOUND} byte bound and was not relayed",
                                self.offset - start
                            ),
                        });
                        continue;
                    }
                    None => {
                        skip.take(rest);
                        self.offset += rest.len() as u64;
                        break;
                    }
                }
            }
            match newline {
                Some(i) => {
                    let line = &rest[..=i];
                    let start = self.offset;
                    self.offset += line.len() as u64;
                    consumed += line.len();
                    self.digest = digest_of(line);
                    if line.len() > RECORD_BOUND {
                        items.push(Item::Mark {
                            position: self.position(),
                            reason: format!(
                                "a record at offset {start} of {} bytes passed the {RECORD_BOUND} byte bound and was not relayed",
                                line.len()
                            ),
                        });
                        continue;
                    }
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
                    // past the bound, in which case it is skipped.
                    if rest.len() > RECORD_BOUND {
                        let mut skip = Skip {
                            start: self.offset,
                            tail: Vec::new(),
                        };
                        skip.take(rest);
                        self.skip = Some(skip);
                        self.offset += rest.len() as u64;
                    }
                    break;
                }
            }
        }
        Ok((items, self.offset != from))
    }
}

// ---------- the connection ----------

fn mark_event(seq: u64, reason: String) -> TraceEvent {
    TraceEvent {
        seq,
        mark: Some(reason),
        run: None,
        turn: None,
        kind: None,
        raw: serde_json::Value::Null,
    }
}

/// The events and marks a step sends, as frames. **A record is measured as
/// the frame that carries it** and replaced by a mark where that frame
/// would pass the link's line bound: re-encoding can grow a record past
/// its raw length (a NUL becomes six bytes, a quote two, an invalid byte
/// three), so `RECORD_BOUND` on the raw bytes alone does not keep a frame
/// under `LINE_BOUND`.
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
                    let frame = FromClient::Event {
                        position,
                        replayed,
                        event,
                    };
                    let encoded = serde_json::to_vec(&frame).map_or(usize::MAX, |v| v.len());
                    if encoded <= LINE_BOUND {
                        return Some(frame);
                    }
                    let FromClient::Event { position, .. } = frame else {
                        unreachable!("built as an event above")
                    };
                    let at = position.offset - line.len() as u64;
                    Some(FromClient::Event {
                        position,
                        replayed,
                        event: mark_event(
                            *seq,
                            format!(
                                "a record at offset {at} of {} bytes encodes to a frame of {encoded} bytes, past the {LINE_BOUND} byte line bound, and was not relayed",
                                line.len()
                            ),
                        ),
                    })
                }
                Item::Mark { position, reason } => Some(FromClient::Event {
                    position,
                    replayed,
                    event: mark_event(*seq, reason),
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
        tailer: Tailer::new(cfg.trace_file.clone(), cfg.scan_delay),
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
    let slot = Arc::new(Slot::new());
    let opts = ServeOptions {
        agent: cfg.link.agent.clone(),
        backfill: cfg.backfill_bytes,
        poll: cfg.poll,
        verb_bound: cfg.verb_bound,
        stop_grace: cfg.stop_grace,
        slot: slot.clone(),
    };
    let grants_bound = cfg.grants_bound;
    // The stop's grace runs from the moment the stop is asked, whatever
    // the link is doing then.
    let stop_asked = {
        let mut signal = shutdown.clone();
        tokio::spawn(async move {
            let _ = signal.wait_for(|stop| *stop).await;
            tokio::time::Instant::now()
        })
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
                // **The ask is bounded**, by the hello's own bound: an
                // invoker that never answers declares the empty ceiling and
                // the hello goes on, so a hung `grants` cannot hold the
                // link down.
                let ceiling: BTreeSet<String> =
                    match tokio::time::timeout(grants_bound, invoker.grants()).await {
                        Ok(Ok(verbs)) => verbs.into_iter().collect(),
                        Ok(Err(e)) => {
                            tracing::warn!(
                                "the grants ask failed ({e:#}); declaring an empty ceiling"
                            );
                            BTreeSet::new()
                        }
                        Err(_) => {
                            tracing::warn!(
                                "the grants ask passed its {grants_bound:?} bound; declaring an empty ceiling"
                            );
                            BTreeSet::new()
                        }
                    };
                let mut shared = shared.lock().await;
                let boundary = shared
                    .tailer
                    .current_tail()
                    .await
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
    let asked = stop_asked
        .await
        .unwrap_or_else(|_| tokio::time::Instant::now());
    orderly_stop(
        &*invoker,
        &slot,
        &cfg.link.agent,
        asked + cfg.stop_grace,
        grants_bound,
    )
    .await;
    Ok(())
}

/// **The orderly stop unloads the agent first** (Spec 8), admin-con's one
/// act on its own initiative: the link's half (asks still waiting answered
/// `not_started`, the verb in flight's answer sent) is done by the time
/// this runs; here the verb in flight is waited for until its process exits
/// and is reaped, then `unload` runs through the invoker where the ceiling
/// grants it, all within the stop's grace. A process that outlasts the
/// grace means the `unload` is not issued, and the kill that follows is an
/// unclean stop, reset at the next load. The stop is the shutdown signal
/// alone: a lost link never reaches here, since the client loop returns
/// only on the stop. The `unload`'s answer is logged; its events reach the
/// server through the trace when admin-con next connects.
async fn orderly_stop<I: Invoker>(
    invoker: &I,
    slot: &Slot,
    agent: &str,
    deadline: tokio::time::Instant,
    grants_bound: Duration,
) {
    let permit = match tokio::time::timeout_at(deadline, slot.permit.clone().acquire_owned()).await
    {
        Ok(Ok(permit)) => permit,
        Ok(Err(_)) => return,
        Err(_) => {
            let running = slot.running.lock().unwrap().clone();
            tracing::error!(
                "{agent}: {} still runs at the end of the stop's grace; unload not issued, and the stop is unclean",
                running.as_deref().unwrap_or("a verb")
            );
            return;
        }
    };
    let granted = match tokio::time::timeout(grants_bound, invoker.grants()).await {
        Ok(Ok(verbs)) => verbs.iter().any(|v| v == "unload"),
        Ok(Err(e)) => {
            tracing::warn!("{agent}: the ceiling could not be read at the stop ({e:#})");
            false
        }
        Err(_) => {
            tracing::warn!("{agent}: the ceiling was not read within {grants_bound:?} at the stop");
            false
        }
    };
    if !granted {
        tracing::warn!("{agent}: the ceiling grants no unload; the stop issues none");
        return;
    }
    tracing::info!("{agent}: the orderly stop unloads the agent");
    match tokio::time::timeout_at(deadline, invoker.run(agent, "unload", &Principal::Server)).await
    {
        Ok(Ok(outcome)) => tracing::info!(
            "{agent}: the stop's unload answered, exit {:?}: {}",
            outcome.exit_code,
            outcome.answer.map(|a| a.to_string()).unwrap_or_default()
        ),
        Ok(Err(fault)) => tracing::error!(
            "{agent}: the stop's unload faulted ({}): {}",
            fault.kind,
            fault.message
        ),
        Err(_) => tracing::error!(
            "{agent}: the stop's unload outlasted the stop's grace; its outcome is unknown"
        ),
    }
    drop(permit);
}

#[derive(Clone)]
struct ServeOptions {
    agent: String,
    backfill: u64,
    poll: Duration,
    verb_bound: Duration,
    stop_grace: Duration,
    slot: Arc<Slot>,
}

/// **admin-con's invocation slot, one per process** (Spec 7.2): held from
/// an invocation's start until its process exits and is reaped, across
/// connection attempts, so a verb asked on a fresh connection while a
/// timed-out one still runs queues behind it and never runs beside it. The
/// box's own guard stands beside this one, weaver-admin's invocation lock;
/// this slot is what keeps this crate's ordering of answers against the
/// trace.
struct Slot {
    permit: Arc<tokio::sync::Semaphore>,
    /// The verb that holds the slot, for the stop's log.
    running: std::sync::Mutex<Option<String>>,
}

impl Slot {
    fn new() -> Self {
        Self {
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
            running: std::sync::Mutex::new(None),
        }
    }

    fn free(&self) -> bool {
        self.permit.available_permits() > 0
    }

    /// Wait until the slot is free, without taking it.
    async fn freed(&self) {
        if let Ok(permit) = self.permit.acquire().await {
            drop(permit);
        }
    }
}

/// A verb ask waiting its turn.
struct Ask {
    id: u64,
    verb: String,
    principal: Principal,
}

impl Ask {
    /// **A `show` asked by the server principal is served during the
    /// replay** (Spec 7.2): the server may ask only observation verbs, and
    /// a re-confirmation acts on nothing, so the live events before its
    /// snapshot follow its answer in order and the row converges on it.
    /// The admission's `show` is one; the frame says so by its principal.
    fn served_during_the_replay(&self) -> bool {
        self.verb == "show" && self.principal == Principal::Server
    }
}

/// The asks waiting their turn.
struct Asks {
    waiting: VecDeque<Ask>,
}

impl Asks {
    fn new() -> Self {
        Self {
            waiting: VecDeque::new(),
        }
    }

    /// The next ask to serve: during the replay a `show` the server asked,
    /// at once, every other waiting for `caught_up`; after it, the oldest.
    fn next(&mut self, replaying: bool) -> Option<Ask> {
        if !replaying {
            return self.waiting.pop_front();
        }
        let at = self
            .waiting
            .iter()
            .position(Ask::served_during_the_replay)?;
        self.waiting.remove(at)
    }

    fn servable(&self, replaying: bool) -> bool {
        if replaying {
            self.waiting.iter().any(Ask::served_during_the_replay)
        } else {
            !self.waiting.is_empty()
        }
    }
}

async fn serve<I: Invoker>(
    mut conn: Connection,
    shared: Arc<tokio::sync::Mutex<Shared>>,
    invoker: Arc<I>,
    opts: ServeOptions,
    mut shutdown: watch::Receiver<bool>,
) -> Ended {
    let mut shared = shared.lock().await;
    // **The whole stop is bounded by one grace**, from the moment shutdown
    // is seen: the relay gets the grace to finish (a verb in flight its
    // answer, a send to a server that stopped taking bytes no more than
    // what remains), and the close only what is left of it, never a
    // further cadence. A send inside the relay is bounded by a cadence of
    // its own, so without this a server that stopped reading would hold
    // SIGTERM past the grace it was promised.
    let mut signal = shutdown.clone();
    let mut grace_ends = None;
    let ended = {
        let relay = relay(&mut conn, &mut shared, invoker, &opts, &mut shutdown);
        tokio::pin!(relay);
        tokio::select! {
            ended = &mut relay => ended,
            () = async { let _ = signal.wait_for(|stop| *stop).await; } => {
                let ends = tokio::time::Instant::now() + opts.stop_grace;
                grace_ends = Some(ends);
                tokio::time::timeout_at(ends, &mut relay)
                    .await
                    .unwrap_or(Ended::Shutdown)
            }
        }
    };
    drop(shared);
    match grace_ends {
        Some(ends) => {
            conn.close_within(ends.saturating_duration_since(tokio::time::Instant::now()))
                .await
        }
        None if matches!(ended, Ended::Shutdown) => conn.close_within(opts.stop_grace).await,
        None => conn.close().await,
    }
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
/// answer (Spec 7.2). **The one place `previous` is cleared**: a hello
/// attempt that fails before its answer leaves the replaced generation
/// held for the next.
async fn resume(
    tailer: &mut Tailer,
    acknowledged: Option<&Position>,
    boundary: &Position,
    backfill: u64,
) -> anyhow::Result<Vec<Item>> {
    let mut marks = Vec::new();
    tailer.skip = None;
    match acknowledged {
        // **A server that restarted answers with no position**: a bounded
        // tail of the file is relayed, a mark at the front saying what was
        // not.
        None => {
            let start = boundary.offset.saturating_sub(backfill);
            let Some(held) = &tailer.held else {
                tailer.reset_to(None);
                tailer.previous = None;
                return Ok(front(tailer, marks));
            };
            // **The first record boundary at or after the start, found at
            // any distance** in bounded chunks: a start inside a record
            // longer than the bound is carried through that record to its
            // delimiter, never to the boundary, so the complete records
            // after it are relayed and the record itself is marked. Off
            // the connection's task, since it may cross a record of any
            // length.
            let aligned = {
                let file = held.file.try_clone()?;
                let end = boundary.offset;
                off_task(tailer.scan_delay, move |scan| {
                    first_boundary(&file, start, end, scan)
                })
                .await?
            };
            tailer.offset = aligned;
            tailer.digest = digest_before(&held.file, aligned)?.unwrap_or_default();
            if aligned > 0 {
                marks.push(Item::Mark {
                    position: tailer.position(),
                    reason: format!(
                        "the server holds no acknowledged position (a first connection, or a server restart): backfill starts {aligned} bytes into the file, and the bytes before it were not relayed"
                    ),
                });
            }
            if aligned - start > RECORD_BOUND as u64 {
                marks.push(Item::Mark {
                    position: tailer.position(),
                    reason: format!(
                        "the backfill's start at {start} fell inside a record past the {RECORD_BOUND} byte bound, ending at {aligned}; it was not relayed"
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
            } else {
                tailer.restart();
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
                tailer.pending = Some(tailer.held.take());
                tailer.held = Some(previous);
                tailer.offset = acked.offset;
                tailer.digest = acked.digest.clone();
                return Ok(front(tailer, marks));
            }
            tailer.restart();
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
    Ok(front(tailer, marks))
}

/// The resume's marks, then the hello's notes, at the resume point.
fn front(tailer: &mut Tailer, mut marks: Vec<Item>) -> Vec<Item> {
    for reason in std::mem::take(&mut tailer.notes) {
        marks.push(Item::Mark {
            position: tailer.position(),
            reason,
        });
    }
    marks
}

/// Where the replay stands after one of its steps, with what to send.
enum Replay {
    Going(Vec<Item>),
    /// Finished: what to send before `caught_up`.
    Done(Vec<Item>),
}

/// **The replay's end is a fact about the file, never about a step's
/// yield** (Spec 7.2): it is finished only where the tailer is in the
/// boundary's generation at or past the boundary's offset, or where that
/// file shrank below the boundary, which is marked and relayed live from
/// its start. A step that sent nothing (a record past the bound being
/// skipped, a frame replaced by a mark) is not the end.
fn replay_state(tailer: &mut Tailer, boundary: &Position, moved: bool) -> anyhow::Result<Replay> {
    let mut items = Vec::new();
    let mut moved = moved;
    if tailer.generation() != boundary.generation {
        // **The previous generation's tail is relayed through its end**,
        // then the current file from its start, marked; its end is a read
        // that moved nothing.
        if moved {
            return Ok(Replay::Going(items));
        }
        let Some(current) = tailer.pending.take() else {
            // Nothing waits behind this generation, so nothing remains to
            // reach the boundary through; never spun on.
            return Ok(Replay::Done(items));
        };
        items.push(tailer.switch(
            current,
            "file replaced: rotation; the new file is relayed from its start",
        )?);
        if tailer.generation() != boundary.generation {
            return Ok(Replay::Done(items));
        }
        moved = true;
    }
    if tailer.held_len()? < boundary.offset {
        let at = tailer.offset;
        tailer.restart();
        items.push(Item::Mark {
            position: tailer.position(),
            reason: format!(
                "the file shrank below the replay's boundary at {} while it was replayed from {at}; relayed live from its start",
                boundary.offset
            ),
        });
        return Ok(Replay::Done(items));
    }
    if tailer.offset >= boundary.offset || !moved {
        // **The boundary is checked by its digest before `caught_up`**: a
        // copy and truncate regrown past the boundary between the hello and
        // the replay's end keeps the file's identity and hides in its
        // length, and what the replay read of it is the new content
        // labelled replayed, which never lands. A mismatch means the file
        // was rewritten after the hello, so its content is post-hello: it
        // is marked and relayed live from its start.
        let same = match &tailer.held {
            Some(held) => {
                digest_before(&held.file, boundary.offset)?.as_deref()
                    == Some(boundary.digest.as_str())
            }
            None => true,
        };
        if !same {
            let at = tailer.offset;
            tailer.restart();
            items.push(Item::Mark {
                position: tailer.position(),
                reason: format!(
                    "the record before the replay's boundary at {} no longer matches its digest: the file was truncated or rewritten since the hello, so what was replayed to {at} is not what the hello named; relayed live from its start",
                    boundary.offset
                ),
            });
            return Ok(Replay::Done(items));
        }
        if tailer.offset >= boundary.offset {
            return Ok(Replay::Done(items));
        }
        // The boundary still ends the record the hello named, and the
        // bytes from here to it end none: rewritten in place before it.
        items.push(Item::Mark {
            position: tailer.position(),
            reason: format!(
                "the bytes from {} to the replay's boundary at {} no longer end a record: the file was rewritten since the hello; relayed live from here",
                tailer.offset, boundary.offset
            ),
        });
        return Ok(Replay::Done(items));
    }
    Ok(Replay::Going(items))
}

async fn relay<I: Invoker>(
    conn: &mut Connection,
    shared: &mut Shared,
    invoker: Arc<I>,
    opts: &ServeOptions,
    shutdown: &mut watch::Receiver<bool>,
) -> Ended {
    let Some(prepared) = shared.prepared.take() else {
        return Ended::Lost("the attempt prepared no boundary".to_owned());
    };
    let Prepared { boundary, ceiling } = prepared;
    let lost = |e: anyhow::Error| Ended::Lost(format!("reading the trace file: {e:#}"));
    // The tailer is at the boundary's generation, or at the previous one
    // with the boundary's waiting behind it: `current_tail` opened or kept
    // it. A generation replaced since is caught by the live reads.
    let acknowledged = conn.acknowledged.clone();
    let front = match resume(
        &mut shared.tailer,
        acknowledged.as_ref(),
        &boundary,
        opts.backfill,
    )
    .await
    {
        Ok(marks) => marks,
        Err(e) => return lost(e),
    };
    let mut seq = shared.seq;
    let mut replaying = true;
    // **The replay's frames wait here and go out one at a time**, the
    // connection read and any ask served between each (Spec 7.2), so an ask
    // waits behind at most one frame and never behind a whole step, which
    // on a slow link could outlast the admission `show`'s deadline. The
    // frame that ends the replay is the last one queued.
    let mut outbox: VecDeque<FromClient> = frames_of(front, true, &mut seq).into();
    let mut queue = Asks::new();
    let mut in_flight = futures::stream::FuturesUnordered::new();
    // Whether a waiting ask's hold behind a detached process was logged.
    let mut held_logged = false;
    let mut tick = tokio::time::interval(opts.poll);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        shared.seq = seq;
        if *shutdown.borrow_and_update() {
            decline_waiting(conn, &mut queue).await;
            return Ended::Shutdown;
        }
        // **A verb, one at a time, its answer placed at the invocation**
        // (Spec 7.2): drain the file to its tail, invoke with the tailer
        // paused, emit the answer, then resume reading or replaying. A second ask waits
        // its turn in arrival order; `VERBS_IN_FLIGHT` is the rule's one
        // number.
        while in_flight.len() < VERBS_IN_FLIGHT
            && queue.servable(replaying)
            && let Ok(permit) = opts.slot.permit.clone().try_acquire_owned()
            && let Some(ask) = queue.next(replaying)
        {
            // **During the replay only a `show` the server asked is served,
            // at once and with no drain** (Spec 7.2): the admission's held
            // behind a long backfill would miss its deadline. Its snapshot
            // is taken after the boundary and every live event written
            // before its invocation is relayed after its answer, in order,
            // so the row may briefly read older than the snapshot but
            // converges to it.
            // Every other ask waits for `caught_up` and takes the drain: a
            // record appended after the hello is live and merely unread,
            // and an ordinary verb answered ahead of it would invert around
            // a person's load or stop.
            //
            // **A verb counts as started only once its invocation begins**
            // (Spec 7.2): one taken from the queue and still draining is
            // answered `not_started` at a stop, so the drain races the stop.
            // Dropping the drain is safe: it gives up between whole frames,
            // since the connection's writer takes a frame whole or not at
            // all, and a tailer left ahead of what was sent is reset by the
            // next connection's resume, which starts from the server's
            // acknowledged position.
            if !replaying {
                let stopped = tokio::select! {
                    biased;
                    _ = shutdown.changed() => true,
                    drained = drain(conn, &mut shared.tailer, &mut seq) => match drained {
                        Ok(()) => false,
                        Err(end) => return end,
                    },
                };
                if stopped || *shutdown.borrow() {
                    shared.seq = seq;
                    drop(permit);
                    decline(conn, &ask).await;
                    decline_waiting(conn, &mut queue).await;
                    return Ended::Shutdown;
                }
            }
            shared.seq = seq;
            let agent = opts.agent.clone();
            let bound = opts.verb_bound;
            let stop = shutdown.clone();
            let invoker = invoker.clone();
            let slot = opts.slot.clone();
            in_flight.push(Box::pin(async move {
                // **The stop is checked as the invocation's first act**, so
                // a verb pushed but not yet polled when the stop came, by
                // the select below or by the grace's loop, never begins:
                // only a verb whose run began is waited for.
                if *stop.borrow() {
                    drop(permit);
                    return (ask.id, Invocation::NotStarted(ask.verb));
                }
                // **The invocation runs to its end in a task of its own,
                // which holds the slot until then** (Spec 7.2): at the bound
                // the answer is `unknown` and the task runs on, the slot
                // still held, so no verb runs beside it on this connection
                // or the next.
                *slot.running.lock().unwrap() = Some(ask.verb.clone());
                let task = tokio::spawn({
                    let verb = ask.verb.clone();
                    let principal = ask.principal.clone();
                    let slot = slot.clone();
                    async move {
                        let ran = invoker.run(&agent, &verb, &principal).await;
                        *slot.running.lock().unwrap() = None;
                        drop(permit);
                        ran
                    }
                });
                let outcome = match tokio::time::timeout(bound, task).await {
                    Ok(Ok(ran)) => Some(ran),
                    Ok(Err(e)) => Some(Err(VerbFault {
                        kind: VerbFault::FAULT.into(),
                        message: format!("the invocation's task ended without an answer: {e}"),
                    })),
                    Err(_) => None,
                };
                (ask.id, Invocation::Ran(outcome))
            }));
        }
        // **An ask held behind a process an earlier invocation left running
        // is logged once, with that verb's name**: on a fresh connection it
        // can be the admission's `show`, and an operator reading admissions
        // that keep closing `admission_incomplete` reads why here.
        let held = in_flight.is_empty() && queue.servable(replaying) && !opts.slot.free();
        if held && !held_logged {
            let running = opts.slot.running.lock().unwrap().clone();
            tracing::warn!(
                "{}: an ask waits for the invocation slot, held by {} still running past its bound",
                opts.agent,
                running.as_deref().unwrap_or("a verb")
            );
        }
        held_logged = held;
        if !in_flight.is_empty() {
            // The connection is still read, so asks queue and a refusal is
            // seen; the file is not, so nothing written during the
            // invocation is emitted ahead of its answer.
            tokio::select! {
                Some((id, outcome)) = in_flight.next() => {
                    if let Err(why) = conn.send(answer(id, outcome, opts.verb_bound)).await {
                        return Ended::Lost(why);
                    }
                }
                incoming = conn.recv() => {
                    if let Some(end) = handle(incoming, conn, &mut queue, &ceiling, &opts.agent).await {
                        return end;
                    }
                }
                _ = shutdown.changed() => {
                    decline_waiting(conn, &mut queue).await;
                    // **The stop waits for the verb in flight within its
                    // grace** (Spec 8), its answer still going out in order;
                    // the process it started is waited for until reaped by
                    // the orderly stop after the link closes.
                    let finished = tokio::time::timeout(opts.stop_grace, async {
                        while let Some((id, outcome)) = in_flight.next().await {
                            if conn.send(answer(id, outcome, opts.verb_bound)).await.is_err() {
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
        // **The replay first**, from the resume point through the boundary,
        // each event marked replayed; then the frame that ends it.
        if replaying {
            // **The connection is read before every frame, the first
            // included**, so a `show` the server asked is served before the
            // next frame and waits behind at most the one being sent, and
            // the server's acknowledgements never back up behind the replay.
            if let Some(end) = take_incoming(conn, &mut queue, &ceiling, &opts.agent).await {
                return end;
            }
            if queue.servable(replaying) && opts.slot.free() {
                continue;
            }
            if let Some(frame) = outbox.pop_front() {
                let ends_the_replay = matches!(frame, FromClient::CaughtUp);
                if let Err(why) = conn.send(frame).await {
                    return Ended::Lost(why);
                }
                if ends_the_replay {
                    replaying = false;
                }
                continue;
            }
            let until =
                (shared.tailer.generation() == boundary.generation).then_some(boundary.offset);
            let (items, moved) = match shared.tailer.read(until, READ_BUDGET) {
                Ok(read) => read,
                Err(e) => return lost(e),
            };
            outbox.extend(frames_of(items, true, &mut seq));
            let (items, done) = match replay_state(&mut shared.tailer, &boundary, moved) {
                Ok(Replay::Going(items)) => (items, false),
                Ok(Replay::Done(items)) => (items, true),
                Err(e) => return lost(e),
            };
            outbox.extend(frames_of(items, true, &mut seq));
            if done {
                outbox.push_back(FromClient::CaughtUp);
            }
            continue;
        }
        // **Live**: wait for the poll, an ask, shutdown, or the slot freed
        // by a timed-out verb's process ending while an ask waits for it.
        tokio::select! {
            _ = shutdown.changed() => {
                decline_waiting(conn, &mut queue).await;
                return Ended::Shutdown;
            }
            () = opts.slot.freed(), if queue.servable(replaying) && !opts.slot.free() => {}
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

/// How an invocation ended: it ran, answering admin's object or passing
/// its bound, or it was declined before its run began because a stop came.
enum Invocation {
    /// The invocation's answer or fault, or `None` where it passed its
    /// bound and runs on.
    Ran(Option<Result<VerbOutcome, VerbFault>>),
    NotStarted(String),
}

/// The answer frame for an invocation, or the typed reason it has none.
/// **A verb that passed its bound answers `unknown`**: the invocation was
/// ended, and whether admin acted before it was is not known here. **A verb
/// declined before its run began answers `not_started`.**
fn answer(id: u64, invocation: Invocation, bound: Duration) -> FromClient {
    match invocation {
        Invocation::NotStarted(verb) => not_started(id, &verb),
        Invocation::Ran(Some(Ok(outcome))) => FromClient::Verb {
            id,
            outcome: Some(outcome),
            error: None,
        },
        Invocation::Ran(Some(Err(fault))) => FromClient::Verb {
            id,
            outcome: None,
            error: Some(fault),
        },
        Invocation::Ran(None) => FromClient::Verb {
            id,
            outcome: None,
            error: Some(VerbFault {
                kind: VerbFault::UNKNOWN.into(),
                message: format!(
                    "the invocation passed its {bound:?} bound and runs on; whether the verb took effect is unknown until the next show"
                ),
            }),
        },
    }
}

/// **Asks still waiting at shutdown are answered `not_started`**, while
/// the link still stands and before a verb in flight gets its grace: none
/// was invoked, so its caller is told it definitely did not run and may be
/// asked again, which a verb lost with the link, whose outcome is unknown,
/// cannot be (Spec 7.2). The whole stop stays under `serve`'s one grace.
async fn decline_waiting(conn: &Connection, queue: &mut Asks) {
    for ask in queue.waiting.drain(..) {
        if conn.send(not_started(ask.id, &ask.verb)).await.is_err() {
            return;
        }
    }
}

/// One ask taken from the queue and not yet invoked when the stop came,
/// answered as the waiting ones are.
async fn decline(conn: &Connection, ask: &Ask) {
    let _ = conn.send(not_started(ask.id, &ask.verb)).await;
}

/// The `not_started` frame for an ask never invoked.
fn not_started(id: u64, verb: &str) -> FromClient {
    FromClient::Verb {
        id,
        outcome: None,
        error: Some(VerbFault {
            kind: VerbFault::NOT_STARTED.into(),
            message: format!("admin-con stopped before {verb} was invoked; it did not run"),
        }),
    }
}

/// One live read; answers whether it moved the position. A truncation is
/// marked and read from the start. **What replaces the held file is
/// sampled before the read, and the switch waits for a read that moved
/// nothing**, so a replacement landing between the read and the switch is
/// never acted on before the held file's tail is read: the next step
/// samples it, reads the old file again, and switches only once that read
/// finds nothing more.
async fn live_step(conn: &Connection, tailer: &mut Tailer, seq: &mut u64) -> Result<bool, Ended> {
    let lost = |e: anyhow::Error| Ended::Lost(format!("reading the trace file: {e:#}"));
    let replacement = tailer.replacement().map_err(|e| lost(e.into()))?;
    let mut items = Vec::new();
    if let Some(mark) = tailer.truncated().map_err(|e| lost(e.into()))? {
        items.push(mark);
    }
    let (read, moved) = tailer.read(None, READ_BUDGET).map_err(lost)?;
    items.extend(read);
    if !moved
        && let Some(change) = replacement
        && let Some(mark) = tailer.switch_to(change).map_err(lost)?
    {
        items.push(mark);
    }
    send_all(conn, frames_of(items, false, seq)).await?;
    Ok(moved)
}

/// **The drain before a verb** (Spec 7.2): every complete record up to the
/// file's tail is emitted ahead of the answer, so an unread older event
/// cannot follow a newer answer. Bounded by the backlog, which is the
/// tailer's lag and not the file.
async fn drain(conn: &Connection, tailer: &mut Tailer, seq: &mut u64) -> Result<(), Ended> {
    // **The tail is recorded first and the drain runs to it**, so a file
    // written continuously cannot hold the verb forever. A live step reads
    // whole records up to its budget, so its last may carry the drain past
    // the target: those records were in the file at the read and go out
    // ahead of the answer too. A record appended after that read is read
    // after the answer; it was written before the snapshot, so the
    // snapshot already reflects it, and applying it after the answer
    // leaves the row at the same state.
    //
    // **A switch inside the drain records the new file's tail and drains
    // to it too**: records already in the replacing file were written
    // before the invocation as surely as the old file's tail was, so they
    // go out ahead of the answer. Each switch needs a replacement the
    // agent made, so the drain stays bounded by what was written before.
    async fn tail(tailer: &Tailer) -> Result<u64, Ended> {
        match &tailer.held {
            Some(held) => tail_of_off_task(&held.file, tailer.scan_delay)
                .await
                .map(|(tail, _)| tail)
                .map_err(|e| Ended::Lost(format!("reading the trace file: {e:#}"))),
            None => Ok(0),
        }
    }
    //
    // **Whenever the held file restarts during the drain, by rotation,
    // truncation or rewrite, the drain retargets to that file's current
    // tail before deciding it is complete**: the target recorded first
    // named the file as it stood, and a restart reads from 0 a file whose
    // complete records may run past it.
    let mut restarts = tailer.restarts;
    let mut target = tail(tailer).await?;
    loop {
        let moved = live_step(conn, tailer, seq).await?;
        if tailer.restarts != restarts {
            restarts = tailer.restarts;
            target = tail(tailer).await?;
            continue;
        }
        if !moved || tailer.offset >= target {
            // **The drain ends only where no replacement is pending**: a
            // switch waits for a read that moves nothing, so a held file
            // whose last read moved reaches its target with the new file
            // still unread. The next step reads it out and switches, and
            // the new file's tail becomes the target.
            if moved
                && tailer
                    .replacement()
                    .map_err(|e| Ended::Lost(format!("reading the trace file: {e:#}")))?
                    .is_some()
            {
                continue;
            }
            return Ok(());
        }
    }
}

/// Read whatever the server has sent without waiting, during the replay.
async fn take_incoming(
    conn: &mut Connection,
    queue: &mut Asks,
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
    queue: &mut Asks,
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
            } else if queue.waiting.len() >= VERB_QUEUE {
                Some(VerbFault {
                    kind: VerbFault::BUSY.into(),
                    message: format!("admin-con holds {VERB_QUEUE} asks waiting, its bound"),
                })
            } else {
                queue.waiting.push_back(Ask {
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
