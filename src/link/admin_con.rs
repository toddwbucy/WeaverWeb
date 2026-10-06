//! admin-con, the management plane's connector (Spec sections 7.2 and 8):
//! beside the agent as its own unprivileged service user, a client of the
//! server's listener over the link with the admin credential the register
//! verb minted. It reads the agent's trace through the relay's door
//! (`link::relay`), relays every event with its position, replays from the
//! acknowledged position at every opening of the door, and marks every
//! discontinuity; it declares its ceiling in the hello, exactly what
//! its invoker's `grants` answers; and it answers verb asks through that
//! invoker, one at a time, each answer placed in the stream at its
//! invocation.
//!
//! **There is no privileged code here.** The verbs reach admin behind the
//! [`Invoker`] trait, whose service implementation is the sudo invoker of
//! `link::sudo_invoker`, the one privileged invocation in this crate. This
//! module owns the order around it: the process-wide invocation slot, the
//! verb bound, and the orderly stop's `unload` (Spec 7.2, 8). No file is
//! read: the record reaches admin-con through the relay alone.
//!
//! **One task owns the door and the connection's outbound stream**, so
//! everything admin-con sends, trace events and verb answers alike, is one
//! ordered stream (Spec 7.2): an answer is emitted after a drain to the
//! relay's heartbeat and before anything read after the invocation, and
//! only one verb runs at a time.

use crate::link::client::{
    self, Backoff, Connection, Ended, Incoming, Link, LinkConfig, LinkStatus,
};
use crate::link::frames::{
    FromClient, LINE_BOUND, Plane, Position, Principal, ToClient, VerbFault, VerbOutcome,
};
use crate::link::relay::{self, Dial, Read};
use crate::traceview::{TraceEvent, parse_line};
use futures::StreamExt;
use serde::Deserialize;
use std::collections::{BTreeSet, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

/// The tail relayed after a server restart, when the config names none
/// (Spec 7.2).
pub const DEFAULT_BACKFILL_BYTES: u64 = 1024 * 1024;
/// The most a config may name, so a backfill stays bounded.
pub const MAX_BACKFILL_BYTES: u64 = 256 * 1024 * 1024;
pub use crate::link::relay::RECORD_BOUND;
/// Verb asks waiting behind the one in flight.
pub const VERB_QUEUE: usize = 16;
/// **The floor under an opening's boundary** (Spec 7.2), covering the
/// opening whole, its dials and its verification included, the last
/// `VERIFY_RESERVE` of it kept for the verification: where no heartbeat
/// comes this long, less that reserve, after the opening's start, as under
/// a writer that never idles, the boundary is taken at the position read so
/// far, and a dial that reaches no header by then leaves the door closed,
/// retried on the backoff. The heartbeat stays the measure; this is what
/// keeps the link up without it.
pub const BOUNDARY_BOUND: Duration = Duration::from_secs(30);
/// **The last part of an opening's bound, kept for verifying the server's
/// position** past the boundary: the opening's read takes its fallback
/// boundary this much before the bound, so the verification always has
/// time. A quarter of the bound where the bound is shorter than four of
/// these. The server's position is unknown when the read starts, so the
/// reserve is always kept.
pub const VERIFY_RESERVE: Duration = Duration::from_secs(5);
/// **The floor under the drain before a verb** (Spec 7.2): where no
/// heartbeat dated at or after the drain's start comes this long after it,
/// the verb is invoked anyway, so a writer that never idles cannot hold an
/// `unload` back.
pub const DRAIN_BOUND: Duration = Duration::from_secs(10);
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
/// filled at install with no default (the trace relay's socket and the
/// absolute path of `weaver-admin` the box's sudo rule names), the backfill
/// bound for the first connection after a server restart, and the verb's
/// bound and the stop's grace (Spec 7.2, 8).
#[derive(Debug, Clone, Deserialize)]
pub struct AdminConConfig {
    #[serde(flatten)]
    pub link: LinkConfig,
    /// The trace relay's socket, absolute: the door the agent's start step
    /// opens for admin-con's user alone (Spec 7.2).
    pub trace_socket: PathBuf,
    /// The absolute path of `weaver-admin` the box's sudo rule names, the
    /// one config value in a privileged command line beside the agent's
    /// name (Spec 7.2).
    pub weaver_admin: PathBuf,
    /// How much of the file's tail is relayed after a server restart, and
    /// how much of what an opening read it holds to replay without reading
    /// again.
    #[serde(default = "default_backfill")]
    pub backfill_bytes: u64,
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
    /// `BOUNDARY_BOUND`. Not a config member: settable in code so a test
    /// can reach it.
    #[serde(skip, default = "default_boundary_bound")]
    pub boundary_bound: Duration,
    /// `DRAIN_BOUND`. Not a config member: settable in code so a test can
    /// reach it.
    #[serde(skip, default = "default_drain_bound")]
    pub drain_bound: Duration,
}

fn default_backfill() -> u64 {
    DEFAULT_BACKFILL_BYTES
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

fn default_boundary_bound() -> Duration {
    BOUNDARY_BOUND
}

fn default_drain_bound() -> Duration {
    DRAIN_BOUND
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
    "trace_socket",
    "weaver_admin",
    "backfill_bytes",
    "verb_bound_secs",
    "stop_grace_secs",
];

impl AdminConConfig {
    /// Read the config under the trust rule of `client::read_private`, and
    /// refuse one minted for the gate plane, one missing a member, a
    /// backfill past `MAX_BACKFILL_BYTES`, or a `weaver_admin` or
    /// `trace_socket` that is not absolute: the box's sudo rule names the
    /// one absolutely, and the other is a box fact never resolved against
    /// admin-con's working directory.
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
        if !cfg.trace_socket.is_absolute() {
            anyhow::bail!(
                "{}: trace_socket {} is not an absolute path",
                path.display(),
                cfg.trace_socket.display()
            );
        }
        Ok(cfg)
    }
}

// ---------- the door ----------

/// What the door hands the connection.
enum Item {
    Record { position: Position, line: Vec<u8> },
    Mark { position: Position, reason: String },
}

impl Item {
    fn position(&self) -> &Position {
        match self {
            Item::Record { position, .. } | Item::Mark { position, .. } => position,
        }
    }
}

/// The mark for a record past `RECORD_BOUND`, at the position after it.
fn oversized(position: Position, len: u64) -> Item {
    let start = position.offset - len;
    Item::Mark {
        reason: format!(
            "a record at offset {start} of {len} bytes passed the {RECORD_BOUND} byte bound and was not relayed"
        ),
        position,
    }
}

/// The mark at the front of a backfill that starts past offset zero.
fn backfill_mark(position: Position) -> Item {
    Item::Mark {
        reason: format!(
            "the server holds no acknowledged position (a first connection, or a server restart): backfill starts {} bytes into the file, and the bytes before it were not relayed",
            position.offset
        ),
        position,
    }
}

/// Milliseconds since the epoch on this box's clock, which is the relay's.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// **What an opening read behind its boundary, the last `cap` bytes of
/// it**, so a replay whose span it holds is sent without reading the
/// file through the relay a second time. Bounded by the backfill bound,
/// so a long outage costs a second read and never memory.
struct Ring {
    /// The position the first kept item starts at.
    start: Position,
    /// Whether anything before `start` was read and let go.
    dropped: bool,
    items: VecDeque<(Item, u64)>,
    bytes: u64,
    cap: u64,
}

impl Ring {
    fn new(start: Position, cap: u64) -> Self {
        Self {
            start,
            dropped: false,
            items: VecDeque::new(),
            bytes: 0,
            cap,
        }
    }

    /// Keep one item of `len` file bytes, letting the oldest go past `cap`.
    /// **What is kept is exactly the records from the first record boundary
    /// within `cap` bytes of the end**: an item is let go only while the
    /// span from its start to the end passes `cap`, so the first kept
    /// item starts at the boundary a backfill of `cap` bytes starts at.
    fn push(&mut self, item: Item, len: u64) {
        self.items.push_back((item, len));
        self.bytes += len;
        while self.bytes > self.cap
            && let Some((item, len)) = self.items.pop_front()
        {
            self.bytes -= len;
            self.start = item.position().clone();
            self.dropped = true;
        }
    }

    /// The items after `from`, where the ring holds everything after it.
    fn after(self, from: &Position) -> Option<Vec<Item>> {
        if *from == self.start {
            return Some(self.items.into_iter().map(|(item, _)| item).collect());
        }
        let at = self
            .items
            .iter()
            .position(|(item, _)| item.position() == from)?;
        Some(
            self.items
                .into_iter()
                .skip(at + 1)
                .map(|(item, _)| item)
                .collect(),
        )
    }
}

/// An opening's read through the relay to its boundary.
struct Opened {
    /// The stream, standing at the boundary.
    stream: relay::Stream,
    boundary: Position,
    ring: Ring,
    /// **What the read met on its way**, in order: a position the relay
    /// refused, a file other than the one admin-con last read, a truncation
    /// it read through, one mark for each. Sent at the front of the replay
    /// in every case, so the window tells what the file did and nothing is
    /// smoothed.
    marks: Vec<Item>,
    /// **The stream `verify` opened at the server's position past the
    /// boundary**, handed to the door, so the position the door resumes
    /// from and the stream it reads came from one dial.
    resumed: Option<relay::Stream>,
    /// The opening's one deadline, taken before its first dial: its
    /// verification gets only what remains of it.
    until: tokio::time::Instant,
}

/// **An opening's boundary is the position at the first heartbeat after
/// its request** (Spec 7.2), this act's election while the relay names no
/// length (`toddwbucy/WeaverAgent#88`): the relay heartbeats only while
/// idle, once everything the file held has been sent, so the position
/// there is everything the file held at that moment. The read starts at
/// `from`, admin-con's last relayed position, or at offset zero where the
/// relay refuses it or serves another file, and keeps the last `cap` bytes
/// of what it read. **One deadline, `bound` from the opening's start,
/// covers the opening whole**, its dials and reads alike: a dial that
/// reaches no header by then is a closed door, retried on the backoff, and
/// where no heartbeat comes by then, as under a writer that never idles or
/// a file that ends inside a record, the boundary is taken at the position
/// read so far: an earlier
/// boundary only makes more of the backlog live, the agent's own record in
/// order, so the row converges to the trace's tail and the opening's `show`
/// re-establishes it. The header's length (#88) would make the boundary
/// exact.
async fn measure(
    socket: &Path,
    from: &Position,
    cap: u64,
    bound: Duration,
) -> Result<Opened, String> {
    // The opening's one deadline, and the read's: the read stops short of
    // the deadline by the reserve, so verifying the server's position after
    // it always has time.
    let deadline = tokio::time::Instant::now() + bound;
    let until = deadline - VERIFY_RESERVE.min(bound / 4);
    let mut marks = Vec::new();
    let dial = async |from: &Position, marks: &mut Vec<Item>| match tokio::time::timeout_at(
        until,
        connect(socket, from, marks),
    )
    .await
    {
        Ok(connected) => connected,
        Err(_) => Err(format!(
            "the trace relay gave no header within {bound:?} of the opening's start"
        )),
    };
    let (mut stream, mut at) = dial(from, &mut marks).await?;
    let mut ring = Ring::new(at.clone(), cap);
    loop {
        // The stream's read is cancel-safe, so the bound drops nothing.
        let Ok(read) = tokio::time::timeout_at(until, stream.next()).await else {
            tracing::warn!(
                "no heartbeat from the trace relay within {bound:?} of the opening's start; the boundary is taken at {}:{}, read so far",
                at.generation,
                at.offset
            );
            return Ok(Opened {
                stream,
                boundary: at,
                ring,
                marks,
                resumed: None,
                until: deadline,
            });
        };
        match read {
            Read::Record { line, digest } => {
                let len = line.len() as u64;
                at.offset += len;
                at.digest = digest;
                ring.push(
                    Item::Record {
                        position: at.clone(),
                        line,
                    },
                    len,
                );
            }
            Read::Oversized { len, digest } => {
                at.offset += len;
                at.digest = digest;
                ring.push(oversized(at.clone(), len), len);
            }
            Read::Heartbeat { .. } => {
                return Ok(Opened {
                    stream,
                    boundary: at,
                    ring,
                    marks,
                    resumed: None,
                    until: deadline,
                });
            }
            Read::Truncated { size } => {
                let reason = format!(
                    "the file was truncated to {size} bytes, below offset {}, while an opening read it; relayed from its start",
                    at.offset
                );
                tracing::info!("{reason}");
                marks.push(Item::Mark {
                    position: relay::zero(&at.generation),
                    reason,
                });
                (stream, at) = dial(&relay::zero(&at.generation), &mut marks).await?;
                ring = Ring::new(at.clone(), cap);
            }
            Read::Ended(why) => return Err(why),
        }
    }
}

/// Dial from `from`, or from offset zero where the relay refuses it or
/// serves another file than `from` names, **each marked** at offset zero of
/// the file now served: the stream and its start.
async fn connect(
    socket: &Path,
    from: &Position,
    marks: &mut Vec<Item>,
) -> Result<(relay::Stream, Position), String> {
    let mut at = from.clone();
    loop {
        match relay::dial(socket, &at).await {
            // **Another file than the position names is a replacement at
            // any offset**, zero included: only the empty generation, the
            // position before any header was read, names no file. A request
            // from zero already reads the new file from its start, so its
            // stream is kept; one past zero is dialed again from zero.
            Dial::Open(stream) if !at.generation.is_empty() && stream.identity != at.generation => {
                let reason = format!(
                    "the relay serves {}, not {} where admin-con last read; the new file is relayed from its start",
                    stream.identity, at.generation
                );
                tracing::info!("{reason}");
                let from_zero = at.offset == 0;
                at = relay::zero(&stream.identity);
                marks.push(Item::Mark {
                    position: at.clone(),
                    reason,
                });
                if from_zero {
                    return Ok((*stream, at));
                }
            }
            Dial::Open(stream) => {
                at.generation = stream.identity.clone();
                return Ok((*stream, at));
            }
            Dial::Refused if at.offset > 0 => {
                let reason = format!(
                    "the relay refused offset {} of {}: truncated or rewritten below it while admin-con was not reading; relayed from its start",
                    at.offset, at.generation
                );
                tracing::info!("{reason}");
                at = relay::zero(&at.generation);
                marks.push(Item::Mark {
                    position: at.clone(),
                    reason,
                });
            }
            Dial::Refused => {
                return Err("the trace relay refused a request from offset zero".into());
            }
            Dial::Closed(why) => return Err(why),
        }
    }
}

/// Where an opening's replay starts, by the server's word.
#[derive(Debug, Clone)]
enum Resume {
    /// The position the server holds: what it acknowledged, or what this
    /// connection sent since, which it will.
    Ack(Position),
    /// The server holds none: a first connection, or a server restart.
    Backfill,
}

/// **A failed verification still moves the next opening forward**: the
/// cursor, where the next opening's read starts, becomes the boundary this
/// one measured, so each attempt reads onward toward the server's position
/// instead of again from the same start, and once a boundary passes it no
/// verification is needed. The cursor is then a measured position and not
/// a relayed one, which is safe because the relay verifies it at the next
/// dial, refusing it where the file changed below it.
fn progress(shared: &mut Shared, opened: &Opened) {
    shared.cursor = opened.boundary.clone();
}

/// **A position the server holds past the boundary is verified inside the
/// opening, before anything is sent** (Spec 7.2): the relay is dialed at
/// it. Where it answers in the boundary's file, the opening is caught up
/// and the door reads that very stream. Where it refuses, the position names
/// nothing the file still holds: a mark goes at the replay's front and the
/// replay is a backfill to the boundary, so the file's history stays behind
/// the boundary and the opening's `show` re-establishes the row. Anything
/// else fails the opening, the door left closed on the backoff. Every other
/// position is planned as it stands.
async fn verify(socket: &Path, opened: &mut Opened, resume: &Resume) -> Result<Resume, String> {
    let acked = match resume {
        // An opening whose read already marked a discontinuity replays from
        // zero of its file, per `begin`, and verifies nothing here.
        Resume::Ack(acked)
            if opened.marks.is_empty()
                && acked.generation == opened.boundary.generation
                && acked.offset > opened.boundary.offset =>
        {
            acked
        }
        other => return Ok(other.clone()),
    };
    // **The verification dial gets only what remains of the opening's one
    // deadline**: where nothing remains it fails at once, a closed door on
    // the backoff, as a dial past the deadline does.
    match tokio::time::timeout_at(opened.until, relay::dial(socket, acked)).await {
        Ok(Dial::Open(stream)) if stream.identity == opened.boundary.generation => {
            opened.resumed = Some(*stream);
            Ok(resume.clone())
        }
        Ok(Dial::Open(stream)) => Err(format!(
            "the trace relay serves {}, not {}, at the acknowledged position",
            stream.identity, opened.boundary.generation
        )),
        Ok(Dial::Refused) => {
            opened.marks.push(Item::Mark {
                position: relay::zero(&opened.boundary.generation),
                reason: format!(
                    "the relay refused the acknowledged offset {}: truncated or rewritten below it; the file is relayed from its start, replayed to the boundary",
                    acked.offset
                ),
            });
            Ok(Resume::Backfill)
        }
        Ok(Dial::Closed(why)) => Err(why),
        Err(_) => Err(
            "the trace relay gave no header at the acknowledged position within the opening's bound"
                .to_owned(),
        ),
    }
}

/// An opening's replay behind its boundary.
struct Replay {
    /// Sent first, as replayed.
    front: Vec<Item>,
    /// Where the ring does not hold the whole span, the second read: from
    /// where, keeping only records that start at or after the offset.
    again: Option<(Position, u64)>,
    /// Where the server already holds a position past the boundary, the
    /// replay is empty and the stream resumes live from that position.
    live_from: Option<Position>,
}

/// **The replay an opening relays behind its boundary** (Spec 7.2): from
/// the server's acknowledged position, or after a server restart a
/// bounded tail from the first record boundary within `backfill` bytes of
/// the boundary, marked at its front. **An acknowledged file the relay no
/// longer serves is a discontinuity**, marked, and the current file is
/// relayed from its start: the relay holds only the run's file. Taken from
/// what the opening read where that holds the span, else read again.
fn plan(ring: Ring, boundary: &Position, resume: &Resume, backfill: u64) -> Replay {
    match resume {
        Resume::Backfill if ring.dropped || ring.start.offset == 0 => {
            let mut front = Vec::new();
            if ring.start.offset > 0 {
                front.push(backfill_mark(ring.start.clone()));
            }
            front.extend(ring.items.into_iter().map(|(item, _)| item));
            Replay {
                front,
                again: None,
                live_from: None,
            }
        }
        Resume::Backfill => Replay {
            front: Vec::new(),
            again: Some((
                relay::zero(&boundary.generation),
                boundary.offset.saturating_sub(backfill),
            )),
            live_from: None,
        },
        // **A position the server holds past the boundary is caught up
        // already**: a boundary taken at its bound can stand short of what
        // a previous connection relayed. Nothing is behind it, `caught_up`
        // goes at once, and the stream resumes live from the server's
        // position, which the relay verifies as any resumption.
        Resume::Ack(acked)
            if acked.generation == boundary.generation && acked.offset > boundary.offset =>
        {
            Replay {
                front: Vec::new(),
                again: None,
                live_from: Some(acked.clone()),
            }
        }
        Resume::Ack(acked) if acked.generation == boundary.generation => match ring.after(acked) {
            Some(front) => Replay {
                front,
                again: None,
                live_from: None,
            },
            None => Replay {
                front: Vec::new(),
                again: Some((acked.clone(), 0)),
                live_from: None,
            },
        },
        Resume::Ack(acked) => {
            let zero = relay::zero(&boundary.generation);
            let mark = Item::Mark {
                position: zero.clone(),
                reason: format!(
                    "the acknowledged file ({}) was replaced, and its tail past {} is not readable through the relay, which serves only the current file; the new file is relayed from its start",
                    acked.generation, acked.offset
                ),
            };
            match ring.after(&zero) {
                Some(items) => Replay {
                    front: std::iter::once(mark).chain(items).collect(),
                    again: None,
                    live_from: None,
                },
                None => Replay {
                    front: vec![mark],
                    again: Some((zero, 0)),
                    live_from: None,
                },
            }
        }
    }
}

/// An opening's second read: its boundary, and where a backfill keeps from.
struct Target {
    boundary: Position,
    keep_from: u64,
    /// No record kept yet, so a backfill's mark is still owed.
    first: bool,
}

/// What one read of the door came to, in the order it is sent.
#[derive(Default)]
struct Step {
    /// Behind the opening's boundary, sent as replayed.
    replayed: Vec<Item>,
    /// The replay reached its boundary or ended: `caught_up` follows.
    caught_up: bool,
    /// After any `caught_up`, sent live.
    live: Vec<Item>,
    /// The door closed; a closing frame follows.
    closed: bool,
}

/// **The trace door as one connection holds it** (Spec 7.2): open while
/// admin-con holds the relay's stream, closed while no relay answers, which
/// is the normal state while the agent is unloaded. A stream that ends with
/// the door open is redialed from its position at the next read, so a
/// relay that dropped admin-con while it was not reading, during a verb,
/// costs nothing: the position is verified and the stream resumes.
struct Door {
    socket: PathBuf,
    backoff: Backoff,
    open: bool,
    /// The stream, while the door is open; none with the door open is a
    /// redial owed from `at`.
    stream: Option<relay::Stream>,
    /// The stream's position: after the last record read from it.
    at: Position,
    /// During an opening's second read, its boundary.
    target: Option<Target>,
    /// The newest heartbeat's time, for the drain.
    heartbeat: Option<u64>,
    /// A redialed stream that has carried nothing yet: one that ends so is
    /// a relay that admits and drops at once, taken as a closed door on the
    /// backoff rather than redialed without pause.
    fresh: bool,
    /// Openings failed since the last success, for the backoff.
    failures: u32,
    next_try: tokio::time::Instant,
}

impl Door {
    fn closed(socket: PathBuf, backoff: Backoff, at: Position) -> Self {
        Self {
            socket,
            backoff,
            open: false,
            stream: None,
            at,
            target: None,
            heartbeat: None,
            fresh: false,
            failures: 0,
            next_try: tokio::time::Instant::now(),
        }
    }

    /// The door closed: the next opening is tried at once where a new
    /// file stands behind the relay, else on the backoff.
    fn close(&mut self, at_once: bool) {
        self.open = false;
        self.stream = None;
        self.target = None;
        self.heartbeat = None;
        self.failures = if at_once { 0 } else { self.failures.max(1) };
        self.next_try = tokio::time::Instant::now()
            + if at_once {
                Duration::ZERO
            } else {
                self.backoff.delay(self.failures)
            };
    }

    /// **Hold an opening**: its replay's front, sent as replayed, the marks
    /// of what the opening's read met first, and the frames that follow. Where the ring holds the span the stream stands
    /// at the boundary and `caught_up` follows the front; else the second
    /// read reaches it.
    fn begin(&mut self, opened: Opened, resume: &Resume, backfill: u64) -> Vec<Item> {
        let Opened {
            stream,
            boundary,
            ring,
            marks,
            resumed,
            ..
        } = opened;
        // **One mark per discontinuity**: where the opening's read already
        // marked one and started the file again from zero, the server's
        // position stands before that same discontinuity, so the replay
        // starts at zero of the file now served with no second mark.
        let resume = match resume {
            Resume::Ack(_) if !marks.is_empty() => Resume::Ack(relay::zero(&boundary.generation)),
            other => other.clone(),
        };
        let replay = plan(ring, &boundary, &resume, backfill);
        self.open = true;
        self.failures = 0;
        self.heartbeat = None;
        match (replay.again, replay.live_from) {
            // The server's position, verified by the opening: the door
            // reads the stream that verified it, so position and stream
            // came from one dial. Without one, a redial owed from the
            // position; only an opening that skipped `verify` reaches that,
            // which nothing in service does.
            (None, Some(from)) => {
                drop(stream);
                self.stream = resumed;
                self.at = from;
                self.target = None;
            }
            (None, None) => {
                self.stream = Some(stream);
                self.at = boundary;
                self.target = None;
            }
            (Some((from, keep_from)), _) => {
                drop(stream);
                self.stream = None;
                self.at = from;
                self.target = Some(Target {
                    boundary,
                    keep_from,
                    first: true,
                });
            }
        }
        // The read's own marks lead, ahead of the plan's and of any
        // second read.
        marks.into_iter().chain(replay.front).collect()
    }

    /// **One read of the door**, cancel-safe: the stream's read keeps a
    /// partial record for the next call, and a redial dropped mid-way
    /// leaves the redial owed, nothing having moved.
    async fn read(&mut self) -> Step {
        let mut step = Step::default();
        let Some(stream) = &mut self.stream else {
            self.redial(&mut step).await;
            return step;
        };
        let before = self.at.clone();
        let read = stream.next().await;
        let fresh = std::mem::take(&mut self.fresh);
        match read {
            Read::Record { line, digest } => {
                let len = line.len() as u64;
                self.at.offset += len;
                self.at.digest = digest;
                let item = Item::Record {
                    position: self.at.clone(),
                    line,
                };
                self.place(item, before, &mut step);
            }
            Read::Oversized { len, digest } => {
                self.at.offset += len;
                self.at.digest = digest;
                let item = oversized(self.at.clone(), len);
                self.place(item, before, &mut step);
            }
            Read::Heartbeat { wall_ms } => {
                self.heartbeat = Some(wall_ms);
                if let Some(target) = &self.target
                    && self.at.offset < target.boundary.offset
                {
                    let reason = format!(
                        "the file ends at {} short of the replay's boundary at {}: it was truncated or rewritten since the boundary was taken; relayed live from its start",
                        self.at.offset, target.boundary.offset
                    );
                    self.restart(reason, &mut step);
                }
            }
            Read::Truncated { size } => {
                let reason = if self.target.is_some() {
                    format!(
                        "the file shrank to {size} bytes, below offset {}, during the replay; relayed live from its start",
                        self.at.offset
                    )
                } else {
                    format!(
                        "the file was truncated to {size} bytes, below offset {}; relayed from its start",
                        self.at.offset
                    )
                };
                self.restart(reason, &mut step);
            }
            Read::Ended(why) if fresh => {
                tracing::info!(
                    "the trace relay ended a redialed stream before it carried anything ({why}); the door is taken as closed"
                );
                self.close(false);
                step.closed = true;
            }
            Read::Ended(why) => {
                tracing::info!(
                    "the trace relay's stream ended ({why}); redialing from {}",
                    self.at.offset
                );
                self.stream = None;
            }
        }
        step
    }

    /// **Truncation and rewrite are marked, and nothing is smoothed** (Spec
    /// 7.2): the file is relayed from its start, live, any replay ended.
    fn restart(&mut self, reason: String, step: &mut Step) {
        let zero = relay::zero(&self.at.generation);
        if self.target.take().is_some() {
            step.caught_up = true;
        }
        step.live.push(Item::Mark {
            position: zero.clone(),
            reason,
        });
        self.at = zero;
        self.stream = None;
    }

    /// One item read: behind the opening's boundary as replayed, else live.
    fn place(&mut self, item: Item, before: Position, step: &mut Step) {
        let Some(target) = &mut self.target else {
            step.live.push(item);
            return;
        };
        let end = self.at.offset;
        if end > target.boundary.offset {
            // **The boundary no longer ends a record**: the file was
            // rewritten since it was taken, so what follows is live.
            step.replayed.push(Item::Mark {
                reason: format!(
                    "the bytes from {} to the replay's boundary at {} no longer end a record: the file was rewritten since the boundary was taken; relayed live from here",
                    before.offset, target.boundary.offset
                ),
                position: before,
            });
            self.target = None;
            step.caught_up = true;
            step.live.push(item);
            return;
        }
        if before.offset >= target.keep_from {
            if std::mem::take(&mut target.first) && target.keep_from > 0 {
                step.replayed.push(backfill_mark(before));
            }
            step.replayed.push(item);
        }
        if end < target.boundary.offset {
            return;
        }
        // **The boundary is checked by its digest before `caught_up`**: a
        // file rewritten in place and regrown past the boundary keeps its
        // identity and hides in its length, and what was replayed of it is
        // not what the boundary named.
        if self.at.digest != target.boundary.digest {
            let reason = format!(
                "the record before the replay's boundary at {end} no longer matches its digest: the file was truncated or rewritten since the boundary was taken, so what was replayed to it is not what the boundary named; relayed live from its start"
            );
            self.restart(reason, step);
            return;
        }
        if target.first && target.keep_from > 0 {
            step.replayed.push(backfill_mark(self.at.clone()));
        }
        self.target = None;
        step.caught_up = true;
    }

    /// **The redial owed by an ended stream**, from the position: the same
    /// file resumes; a refused position, the file truncated or rewritten
    /// below it while admin-con was not reading, is marked and read from
    /// its start; and a relay that serves another file, or none, closed
    /// the door, so the next opening is an admission of the trace.
    async fn redial(&mut self, step: &mut Step) {
        match relay::dial(&self.socket, &self.at).await {
            Dial::Open(stream) if stream.identity == self.at.generation => {
                self.stream = Some(*stream);
                self.fresh = true;
            }
            Dial::Open(stream) => {
                tracing::info!(
                    "the trace relay now serves {}, not {}: a new run; the door is taken as closed and opened again",
                    stream.identity,
                    self.at.generation
                );
                self.close(true);
                step.closed = true;
            }
            Dial::Refused if self.at.offset > 0 => {
                let mark = Item::Mark {
                    position: relay::zero(&self.at.generation),
                    reason: format!(
                        "the relay refused offset {}: the file was truncated or rewritten below it while admin-con was not reading; relayed from its start",
                        self.at.offset
                    ),
                };
                if self.target.is_some() {
                    step.replayed.push(mark);
                } else {
                    step.live.push(mark);
                }
                self.at = relay::zero(&self.at.generation);
            }
            Dial::Refused => {
                tracing::error!(
                    "the trace relay refused a request from offset zero; the door is taken as closed"
                );
                self.close(false);
                step.closed = true;
            }
            Dial::Closed(why) => {
                tracing::info!("the trace door closed: {why}");
                self.close(false);
                step.closed = true;
            }
        }
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

/// **What waits to be sent**: an item read from the door, made a frame only
/// as it goes, so a replay held in memory costs its records' bytes and
/// never their parsed events; or a frame of admin-con's own.
enum Out {
    Item { item: Item, replayed: bool },
    Frame(FromClient),
}

fn outs(items: Vec<Item>, replayed: bool) -> impl Iterator<Item = Out> {
    items
        .into_iter()
        .map(move |item| Out::Item { item, replayed })
}

/// One item as the frame that carries it, none for a blank record. **A
/// record is measured as the frame that carries it** and replaced by a mark
/// where that frame would pass the link's line bound: re-encoding can grow
/// a record past its raw length (a NUL becomes six bytes, a quote two, an
/// invalid byte three), so `RECORD_BOUND` on the raw bytes alone does not
/// keep a frame under `LINE_BOUND`.
fn frame_of(item: Item, replayed: bool, seq: &mut u64) -> Option<FromClient> {
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
}

/// An out as its frame, none for a blank record.
fn frame_out(out: Out, seq: &mut u64) -> Option<FromClient> {
    match out {
        Out::Item { item, replayed } => frame_of(item, replayed, seq),
        Out::Frame(frame) => Some(frame),
    }
}

/// What one attempt prepared before its hello: the ceiling it declares,
/// and the opening of the door it reports, none where the door is closed.
struct Prepared {
    ceiling: BTreeSet<String>,
    opened: Option<Opened>,
    /// When the hello left for the server: the server connection's time
    /// from here is not the opening's.
    measured: tokio::time::Instant,
}

/// State shared between the hellos and the serves of the process.
struct Shared {
    prepared: Option<Prepared>,
    /// The next event's sequence number for the server's window.
    seq: u64,
    /// **The last position sent**, on any connection: where the next
    /// opening's read starts, so a living admin-con reads only what is new.
    /// Offset zero of no file at the process's start.
    cursor: Position,
    /// Where this connection's next opening replays from: the server's
    /// acknowledged position at the hello, then the last position sent.
    resume: Resume,
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
        prepared: None,
        seq: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            * 1_000_000,
        cursor: relay::zero(""),
        resume: Resume::Backfill,
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
        socket: cfg.trace_socket.clone(),
        backoff,
        backfill: cfg.backfill_bytes,
        boundary_bound: cfg.boundary_bound,
        drain_bound: cfg.drain_bound,
        verb_bound: cfg.verb_bound,
        stop_grace: cfg.stop_grace,
        slot: slot.clone(),
    };
    let grants_bound = cfg.grants_bound;
    let socket = cfg.trace_socket.clone();
    let backfill = cfg.backfill_bytes;
    let boundary_bound = cfg.boundary_bound;
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
            let socket = socket.clone();
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
                // **The hello reports the door, and an open door's boundary**
                // (Spec 7.2): an opening taken now, before any verb of this
                // connection, its boundary the position at the relay's first
                // heartbeat. A door that does not open is reported closed,
                // with no boundary, and redialed on the backoff.
                let mut shared = shared.lock().await;
                let from = shared.cursor.clone();
                let opened = match measure(&socket, &from, backfill, boundary_bound).await {
                    Ok(opened) => Some(opened),
                    Err(why) => {
                        tracing::info!("the trace door is closed at the hello: {why}");
                        None
                    }
                };
                let tail = opened.as_ref().map(|o| o.boundary.clone());
                shared.prepared = Some(Prepared {
                    ceiling: ceiling.clone(),
                    opened,
                    measured: tokio::time::Instant::now(),
                });
                Ok(FromClient::Hello {
                    agent,
                    plane: Plane::Admin,
                    door: Some(tail.is_some()),
                    tail,
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
        invoker,
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
///
/// **Every wait here ends by the stop's deadline**: the ceiling's ask by
/// the earlier of the deadline and its own bound, so a slot freed just
/// before the deadline cannot carry the stop past it. **The `unload` runs
/// in a task of its own**, which owns the child and both its readers to the
/// end and reaps it, as an ordinary invocation does: at the deadline the
/// stop stops waiting and admin-con exits, and the child, in its own
/// session and never killed, runs on. Its pipes close when admin-con exits,
/// which the contract covers: admin ignores the broken pipe, finishes, and
/// records the outcome in its own log on the box.
async fn orderly_stop<I: Invoker>(
    invoker: Arc<I>,
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
    let asked_until = deadline.min(tokio::time::Instant::now() + grants_bound);
    let granted = match tokio::time::timeout_at(asked_until, invoker.grants()).await {
        Ok(Ok(verbs)) => verbs.iter().any(|v| v == "unload"),
        Ok(Err(e)) => {
            tracing::warn!("{agent}: the ceiling could not be read at the stop ({e:#})");
            false
        }
        Err(_) => {
            tracing::warn!(
                "{agent}: the ceiling was not read within its bound or the stop's grace; the stop issues no unload"
            );
            false
        }
    };
    if !granted {
        tracing::warn!("{agent}: the ceiling grants no unload; the stop issues none");
        return;
    }
    tracing::info!("{agent}: the orderly stop unloads the agent");
    let unload = tokio::spawn({
        let agent = agent.to_owned();
        async move {
            let ran = invoker.run(&agent, "unload", &Principal::Server).await;
            drop(permit);
            ran
        }
    });
    match tokio::time::timeout_at(deadline, unload).await {
        Ok(Ok(Ok(outcome))) => tracing::info!(
            "{agent}: the stop's unload answered, exit {:?}: {}",
            outcome.exit_code,
            outcome.answer.map(|a| a.to_string()).unwrap_or_default()
        ),
        Ok(Ok(Err(fault))) => tracing::error!(
            "{agent}: the stop's unload faulted ({}): {}",
            fault.kind,
            fault.message
        ),
        Ok(Err(e)) => tracing::error!("{agent}: the stop's unload ended without an answer: {e}"),
        Err(_) => tracing::error!(
            "{agent}: the stop's unload runs on past the stop's grace; its outcome is in the box's admin.log"
        ),
    }
}

#[derive(Clone)]
struct ServeOptions {
    agent: String,
    /// The trace relay's socket.
    socket: PathBuf,
    /// The door's redial, the connector's own backoff.
    backoff: Backoff,
    backfill: u64,
    boundary_bound: Duration,
    drain_bound: Duration,
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

/// What one read of the door sends, in order: what is behind the boundary
/// as replayed, `caught_up` where the replay ended, what is live, and the
/// closing where the door closed.
fn step_outs(step: Step) -> Vec<Out> {
    let mut sent: Vec<Out> = outs(step.replayed, true).collect();
    if step.caught_up {
        sent.push(Out::Frame(FromClient::CaughtUp));
    }
    sent.extend(outs(step.live, false));
    if step.closed {
        sent.push(Out::Frame(FromClient::Door {
            open: false,
            wall_ms: now_ms(),
            tail: None,
        }));
    }
    sent
}

/// **What a sent frame moves**: an event's position becomes the last
/// position sent, where the next opening's read starts and this
/// connection's next opening replays from, since the server will hold it.
fn sent(shared: &mut Shared, frame: &FromClient) {
    if let FromClient::Event { position, .. } = frame {
        shared.cursor = position.clone();
        shared.resume = Resume::Ack(position.clone());
    }
}

/// Send in order, each frame moving what it moves.
async fn send_outs(conn: &Connection, shared: &mut Shared, outs: Vec<Out>) -> Result<(), Ended> {
    for out in outs {
        let Some(frame) = frame_out(out, &mut shared.seq) else {
            continue;
        };
        conn.send(frame.clone()).await.map_err(Ended::Lost)?;
        sent(shared, &frame);
    }
    Ok(())
}

/// **An opening of the door on the live connection** (Spec 7.2), taken only
/// while no invocation is in flight: the read to the boundary, raced
/// against the connection, whose asks queue meanwhile, and the stop. An
/// opening that cannot be taken leaves the door closed on the backoff.
/// Answers the frames that report it and replay behind it, none where the
/// door stayed closed.
async fn take_opening(
    conn: &mut Connection,
    door: &mut Door,
    shared: &mut Shared,
    queue: &mut Asks,
    ceiling: &BTreeSet<String>,
    opts: &ServeOptions,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<Option<Vec<Out>>, Ended> {
    let from = shared.cursor.clone();
    let read = measure(&opts.socket, &from, opts.backfill, opts.boundary_bound);
    tokio::pin!(read);
    let measured = loop {
        tokio::select! {
            measured = &mut read => break measured,
            incoming = conn.recv() => {
                if let Some(end) = handle(incoming, conn, queue, ceiling, &opts.agent).await {
                    return Err(end);
                }
            }
            _ = shutdown.changed() => {
                decline_waiting(conn, queue).await;
                return Err(Ended::Shutdown);
            }
        }
    };
    let opened = match measured {
        Ok(opened) => opened,
        Err(why) => {
            door.failures = door.failures.saturating_add(1);
            door.next_try = tokio::time::Instant::now() + door.backoff.delay(door.failures);
            tracing::debug!("{}: the trace door stays closed: {why}", opts.agent);
            return Ok(None);
        }
    };
    tracing::info!(
        "{}: the trace door opened at {}:{}",
        opts.agent,
        opened.boundary.generation,
        opened.boundary.offset
    );
    let mut opened = opened;
    // The verification races the connection as the read did: asks queue
    // meanwhile, and the stop is heard.
    let verified = {
        let check = verify(&opts.socket, &mut opened, &shared.resume);
        tokio::pin!(check);
        loop {
            tokio::select! {
                verified = &mut check => break verified,
                incoming = conn.recv() => {
                    if let Some(end) = handle(incoming, conn, queue, ceiling, &opts.agent).await {
                        return Err(end);
                    }
                }
                _ = shutdown.changed() => {
                    decline_waiting(conn, queue).await;
                    return Err(Ended::Shutdown);
                }
            }
        }
    };
    let resume = match verified {
        Ok(resume) => resume,
        Err(why) => {
            progress(shared, &opened);
            door.failures = door.failures.saturating_add(1);
            door.next_try = tokio::time::Instant::now() + door.backoff.delay(door.failures);
            tracing::info!(
                "{}: the opening failed at the acknowledged position: {why}",
                opts.agent
            );
            return Ok(None);
        }
    };
    let boundary = opened.boundary.clone();
    let front = door.begin(opened, &resume, opts.backfill);
    let mut sent = vec![Out::Frame(FromClient::Door {
        open: true,
        wall_ms: now_ms(),
        tail: Some(boundary),
    })];
    sent.extend(outs(front, true));
    if door.target.is_none() {
        sent.push(Out::Frame(FromClient::CaughtUp));
    }
    Ok(Some(sent))
}

async fn relay<I: Invoker>(
    conn: &mut Connection,
    shared: &mut Shared,
    invoker: Arc<I>,
    opts: &ServeOptions,
    shutdown: &mut watch::Receiver<bool>,
) -> Ended {
    let Some(prepared) = shared.prepared.take() else {
        return Ended::Lost("the attempt prepared no hello".to_owned());
    };
    let Prepared {
        ceiling,
        mut opened,
        measured,
    } = prepared;
    // **The server connection's time is not the opening's**: the hello's
    // opening was measured before the handshake and the hello, and its
    // deadline moves forward by what they took, so the reserve the
    // verification gets is what the measurement left it. Only this path
    // needs it: a later opening runs on a live connection, with no
    // handshake between its measurement and its verification.
    if let Some(opened) = &mut opened {
        opened.until += measured.elapsed();
    }
    // **Where this connection's openings replay from** (Spec 7.2): the
    // server's acknowledged position, or a backfill where it holds none.
    shared.resume = match conn.acknowledged.clone() {
        Some(acknowledged) => Resume::Ack(acknowledged),
        None => Resume::Backfill,
    };
    let mut door = Door::closed(opts.socket.clone(), opts.backoff, shared.cursor.clone());
    // **The replay's frames wait here and go out one at a time**, the
    // connection read and any ask served between each (Spec 7.2), so an ask
    // waits behind at most one frame and never behind a whole step, which
    // on a slow link could outlast the admission `show`'s deadline. The
    // frame that ends the replay is the last one queued.
    let mut outbox: VecDeque<Out> = VecDeque::new();
    // **A closed door's hello has no replay**: the server takes `caught_up`
    // as sent, and the door is redialed on the backoff.
    let mut replaying = false;
    // **The hello's verification runs beside the connection**: the server's
    // admission `show` and its deadline start at admission, so the
    // connection is read meanwhile and that `show` is served under the
    // replay's rule, the boundary being measured already; ordinary asks
    // wait for the replay as ever. The task ends with the connection.
    let mut verifying: Option<Verifying> = None;
    match opened {
        Some(mut opened) => {
            let socket = opts.socket.clone();
            let resume = shared.resume.clone();
            verifying = Some(Verifying(tokio::spawn(async move {
                let verified = verify(&socket, &mut opened, &resume).await;
                (opened, verified)
            })));
            replaying = true;
        }
        None => door.close(false),
    }
    let mut queue = Asks::new();
    let mut in_flight = futures::stream::FuturesUnordered::new();
    // Whether a waiting ask's hold behind a detached process was logged.
    let mut held_logged = false;
    // **An opening on the live connection serves its own `show` first,
    // and holds every ordinary ask until that `show` is answered** (Spec
    // 7.2), **at the hello as at every opening**, since the hello is an
    // admission and its `show` is owed whatever the door's state: where the
    // ceiling grants `show`, ordinary asks wait from the hello or the
    // opening until the `show` the server asks has been answered with a
    // `state` answer and that answer sent. **The hold runs on no clock of
    // admin-con's**: the server always asks after a door frame where the
    // ceiling grants `show`, and if it does not, or the `show` faults or
    // answers anything but a state, its own admission deadline closes the
    // connection, which ends the hold. That deadline is the only clock, and
    // it is the right one, since no deadline admin-con computes can be
    // proven to outlast it.
    let mut opening_show = owes_show(&ceiling);
    // The `show` taken under the hold, whose answer may release it.
    let mut hold_show: Option<u64> = None;
    loop {
        if *shutdown.borrow_and_update() {
            decline_waiting(conn, &mut queue).await;
            return Ended::Shutdown;
        }
        // Only a `show` the server asked is served while this holds.
        let restricted = replaying || opening_show;
        // **A verb, one at a time, its answer placed at the invocation**
        // (Spec 7.2): drain the door to the relay's heartbeat, invoke with
        // the door unread, emit the answer, then read on. A second ask
        // waits its turn in arrival order; `VERBS_IN_FLIGHT` is the rule's
        // one number.
        while in_flight.len() < VERBS_IN_FLIGHT
            && queue.servable(restricted)
            && let Ok(permit) = opts.slot.permit.clone().try_acquire_owned()
            && let Some(ask) = queue.next(restricted)
        {
            if opening_show && ask.served_during_the_replay() {
                hold_show = Some(ask.id);
            }
            // **During the replay only a `show` the server asked is served,
            // at once and with no drain** (Spec 7.2): the admission's held
            // behind a long backfill would miss its deadline. Its snapshot
            // is taken after the boundary and every live event written
            // before its invocation is relayed after its answer, in order,
            // so the row may briefly read older than the snapshot but
            // converges to it.
            // Every other ask waits for `caught_up` and takes the drain: a
            // record appended after the boundary is live and merely unread,
            // and an ordinary verb answered ahead of it would invert around
            // a person's load or stop.
            //
            // **A verb counts as started only once its invocation begins**
            // (Spec 7.2): one taken from the queue and still draining is
            // answered `not_started` at a stop, so the drain races the stop.
            // Dropping the drain is safe: the door's read is cancel-safe and
            // the connection's writer takes a frame whole or not at all.
            if !restricted {
                let stopped = tokio::select! {
                    biased;
                    _ = shutdown.changed() => true,
                    drained = drain(conn, &mut door, shared, opts.drain_bound) => match drained {
                        Ok(()) => false,
                        Err(end) => return end,
                    },
                };
                if stopped || *shutdown.borrow() {
                    drop(permit);
                    decline(conn, &ask).await;
                    decline_waiting(conn, &mut queue).await;
                    return Ended::Shutdown;
                }
            }
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
        let held = in_flight.is_empty() && queue.servable(restricted) && !opts.slot.free();
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
            // seen; the door is not, so nothing written during the
            // invocation is emitted ahead of its answer.
            tokio::select! {
                Some((id, outcome)) = in_flight.next() => {
                    let releases = hold_show == Some(id) && answered_a_state(&outcome);
                    if let Err(why) = conn.send(answer(id, outcome, opts.verb_bound)).await {
                        return Ended::Lost(why);
                    }
                    // The opening's `show` answered with a state and sent:
                    // the hold ends. Answered otherwise, the hold stands
                    // until the server closes the connection.
                    if hold_show == Some(id) {
                        hold_show = None;
                        if releases {
                            opening_show = false;
                        }
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
            // The hello's verification, its connection read meanwhile.
            if let Some(pending) = &mut verifying {
                tokio::select! {
                    verified = &mut pending.0 => {
                        verifying = None;
                        match verified {
                            Ok((opened, Ok(resume))) => {
                                let front = door.begin(opened, &resume, opts.backfill);
                                outbox.extend(outs(front, true));
                                if door.target.is_none() {
                                    outbox.push_back(Out::Frame(FromClient::CaughtUp));
                                }
                            }
                            // The hello reported the door open; it closes
                            // before anything is behind it.
                            failed => {
                                let why = match failed {
                                    Ok((opened, Err(why))) => {
                                        progress(shared, &opened);
                                        why
                                    }
                                    Err(e) => format!("the verification's task ended: {e}"),
                                    Ok((_, Ok(_))) => unreachable!("matched above"),
                                };
                                tracing::info!(
                                    "{}: the opening failed at the acknowledged position: {why}",
                                    opts.agent
                                );
                                door.close(false);
                                outbox.push_back(Out::Frame(FromClient::Door {
                                    open: false,
                                    wall_ms: now_ms(),
                                    tail: None,
                                }));
                            }
                        }
                    }
                    incoming = conn.recv() => {
                        if let Some(end) = handle(incoming, conn, &mut queue, &ceiling, &opts.agent).await {
                            return end;
                        }
                    }
                    _ = shutdown.changed() => {
                        decline_waiting(conn, &mut queue).await;
                        return Ended::Shutdown;
                    }
                }
                continue;
            }
            if let Some(out) = outbox.pop_front() {
                let Some(frame) = frame_out(out, &mut shared.seq) else {
                    continue;
                };
                let ends_the_replay = matches!(
                    frame,
                    FromClient::CaughtUp | FromClient::Door { open: false, .. }
                );
                if let Err(why) = conn.send(frame.clone()).await {
                    return Ended::Lost(why);
                }
                sent(shared, &frame);
                if ends_the_replay {
                    replaying = false;
                }
                continue;
            }
            // **The opening's second read**, where what the opening read
            // did not hold the span: the door is read toward its boundary,
            // the connection and the stop still heard.
            tokio::select! {
                step = door.read() => outbox.extend(step_outs(step)),
                incoming = conn.recv() => {
                    if let Some(end) = handle(incoming, conn, &mut queue, &ceiling, &opts.agent).await {
                        return end;
                    }
                }
                _ = shutdown.changed() => {
                    decline_waiting(conn, &mut queue).await;
                    return Ended::Shutdown;
                }
            }
            continue;
        }
        // Anything queued past the replay's end is live and goes out first.
        if !outbox.is_empty() {
            let queued: Vec<Out> = outbox.drain(..).collect();
            if let Err(end) = send_outs(conn, shared, queued).await {
                return end;
            }
            continue;
        }
        // **Live**: the door, an ask, the stop, the slot freed by a
        // timed-out verb's process ending while an ask or an opening waits
        // for it, or the door's next opening, taken only while nothing is
        // in flight or waiting to start.
        // **No new opening while an opening's hold stands**: the hold
        // resolves first, its `show` answered or the connection ended, so a
        // `show` from an earlier opening never clears a later one's hold.
        let opening_due =
            !door.open && !opening_show && opts.slot.free() && !queue.servable(restricted);
        let next_try = door.next_try;
        tokio::select! {
            _ = shutdown.changed() => {
                decline_waiting(conn, &mut queue).await;
                return Ended::Shutdown;
            }
            // The slot freed by a timed-out verb's process ending wakes an
            // ask waiting for it, and a closed door's opening owed after it.
            () = opts.slot.freed(), if !opts.slot.free() && (queue.servable(restricted) || !door.open) => {}
            step = door.read(), if door.open => {
                if let Err(end) = send_outs(conn, shared, step_outs(step)).await {
                    return end;
                }
            }
            () = tokio::time::sleep_until(next_try), if opening_due => {
                match take_opening(conn, &mut door, shared, &mut queue, &ceiling, opts, shutdown).await {
                    Ok(Some(queued)) => {
                        outbox.extend(queued);
                        replaying = true;
                        opening_show = owes_show(&ceiling);
                    }
                    Ok(None) => {}
                    Err(end) => return end,
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

/// The hello's verification, a task the connection's loop waits on beside
/// its reads; aborted where the connection ends first.
struct Verifying(tokio::task::JoinHandle<(Opened, Result<Resume, String>)>);

impl Drop for Verifying {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// **Whether an admission or an opening owes a `show`, and so holds
/// ordinary asks until it is answered**: where the ceiling grants it. The
/// one place the hold is decided, at the hello and at every opening.
fn owes_show(ceiling: &BTreeSet<String>) -> bool {
    ceiling.contains("show")
}

/// Whether an invocation answered with a `state` answer, the one that ends
/// an opening's hold.
fn answered_a_state(invocation: &Invocation) -> bool {
    matches!(
        invocation,
        Invocation::Ran(Some(Ok(outcome)))
            if outcome
                .answer
                .as_ref()
                .and_then(|a| a.get("kind"))
                .and_then(|k| k.as_str())
                == Some("state")
    )
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

/// **The drain before a verb** (Spec 7.2): every record the file held when
/// the drain began is emitted ahead of the answer, so an unread older event
/// cannot follow a newer answer. **The measure is the relay's heartbeat**,
/// this act's election while the relay names no length
/// (`toddwbucy/WeaverAgent#88`): a heartbeat dated at or after the drain's
/// start says everything written before it was sent, so the door is read
/// until one comes. A heartbeat already in flight, dated before, says
/// nothing of what was written since. **Where none comes within `bound`**,
/// as under a writer that never idles, the verb is invoked anyway: what is
/// unread then is read after the answer and ordered behind it. The
/// inversion that matters, an unload written before a `show`'s snapshot and
/// relayed after its answer, cannot pass through this gap, since an unload
/// holds the box's invocation lock for its whole run and a `show` meeting
/// it answers `InTransition`, which claims no state; a turn's event ordered
/// behind the answer is a transient the next event corrects. With the door
/// closed there is nothing to drain.
async fn drain(
    conn: &Connection,
    door: &mut Door,
    shared: &mut Shared,
    bound: Duration,
) -> Result<(), Ended> {
    let since = now_ms();
    let until = tokio::time::Instant::now() + bound;
    while door.open {
        // The door's read is cancel-safe, so the bound drops nothing.
        let Ok(step) = tokio::time::timeout_at(until, door.read()).await else {
            tracing::warn!(
                "no heartbeat from the trace relay within {bound:?} of the drain's start; the verb is invoked with what was read"
            );
            return Ok(());
        };
        send_outs(conn, shared, step_outs(step)).await?;
        if door.heartbeat.is_some_and(|at| at >= since) {
            return Ok(());
        }
    }
    Ok(())
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
            } else if !(verb == "show" && principal == Principal::Server)
                && queue
                    .waiting
                    .iter()
                    .filter(|ask| !ask.served_during_the_replay())
                    .count()
                    >= VERB_QUEUE
            {
                // **The bound is on ordinary asks**: a `show` the server
                // asked is never answered `busy`, since it is what completes
                // an admission or an opening and the server asks at most one
                // for each, so the reserve is that one observation ask and
                // never a second queue.
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
