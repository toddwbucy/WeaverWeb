//! conforms: web-link-refuses-a-credential-not-live-before-the-roster
//! conforms: web-link-identity-is-the-certificates-binding-never-the-roster
//! conforms: web-one-live-connection-per-credential
//! conforms: web-link-state-is-reset-when-the-listener-starts
//! conforms: web-agent-present-only-when-both-planes-match-one-row
//! conforms: web-tuple-is-admins-word-and-never-gate-cons
//! conforms: web-nothing-crosses-the-link-in-the-clear
//! conforms: web-the-servers-asks-are-audited
//! conforms: web-verb-refused-before-an-ask-unless-granted
//!
//! The listener (Spec section 8): the mutual-TLS accept loop, admission
//! under the row's lock, the heartbeat, the incarnations that bind every
//! link-state write to its connection, the asks the server routes to a
//! connector, and the landing of what the admin plane reports (Spec 2.12
//! and the clauses of 7.2 the server enacts). The live window is the seed's
//! server half of `traceview.rs`, one ring per agent.
//!
//! **The live map is mutated under the row's lock on every path**, the
//! admission's install, the teardown's uninstall, and the cleanup after a
//! store error, through `Store::with_row_lock`. The one exception is a
//! store that cannot be reached at all, where the lock cannot be taken and
//! the entry is removed without it and the exception logged, so a
//! credential is not stranded as connected in memory by a database outage.
//!
//! **A store failure while landing anything on a connection closes it**
//! with a typed refusal, through the normal teardown, the acknowledged
//! position standing at the last success (Spec 7.2). The connector
//! reconnects under the heartbeat's rule, the hello's answer names that
//! position so the replay resends the failed event and everything after,
//! and the admission's `show` is asked again; a pending ask whose answer
//! could not be landed answers an error and never the outcome. So nothing
//! is acknowledged that did not land and no lifecycle update is lost.

use crate::adapters::gate::GateClose;
use crate::alerting::{Alerts, Failure};
use crate::link::authority::{Authority, fingerprint};
use crate::link::frames::{
    FromClient, Line, LineReader, Plane, Position, Principal, Refusal, ToClient, TurnFault,
    VerbFault, VerbOutcome,
};
use crate::link::register::{
    ConstituentsWrite, CredentialState, Observation, REVOCATION_CHANNEL, TupleWrite,
};
use crate::store::access::{shared_hold, verbs_granted_in};
use crate::store::alerts::{AlertKind, Raise, bounded as cut};
use crate::store::audit::{self, PersonMethod, Target};
use crate::store::identity::session_stands;
use crate::store::{AgentId, Store};
use crate::traceview::{TraceEvent, TraceViews};
use chrono::{DateTime, Utc};
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_rustls::TlsAcceptor;

/// How long a peer has to complete the handshake, and then to say hello. A
/// peer that speaks plaintext never completes the handshake and is dropped
/// here; nothing it sent was read as a frame.
const HANDSHAKE_SECS: u64 = 10;

/// The least silence bound the cadence rule admits: the cadence is the
/// bound divided by four, so a bound under four seconds would have a
/// cadence of zero.
pub const LEAST_SILENCE_SECS: u64 = 4;

/// **One listener per store, held at the store** (Spec 8): the key of the
/// session-level advisory lock a listener takes before its epoch and
/// reset, and holds for its life on a connection of its own. **A constant
/// of this crate's and never a value from config**, because the point is
/// that every weaver-web process against one store contends for the same
/// lock whatever its config says; two listeners on one store would each
/// admit the same credential, and the second's reset would mark the
/// first's connections disconnected.
pub const LISTENER_LOCK_KEY: i64 = i64::from_be_bytes(*b"weaverwb");

/// **The authority lock** (Spec 8): the key of the session-level advisory
/// lock every verb that mints a credential or replaces the authority holds
/// across its store transaction and its file switch, so a registration
/// cannot mint under an authority a concurrent rotation is retiring and
/// commit fingerprints that die at the next restart. A constant of this
/// crate's for the same reason as the listener's: every process against
/// one store must contend for the same lock whatever its config says. A
/// second key rather than the listener's, since the listener holds its own
/// for its life and a verb must not wait on the server.
pub const AUTHORITY_LOCK_KEY: i64 = i64::from_be_bytes(*b"weaverca");

/// A live connection as the listener holds it: the credential it was
/// admitted on, the write path, the incarnation that every link-state write
/// about it names, the asks it has not answered, and the close the revoking
/// act pulls.
struct LiveConnection {
    fingerprint: String,
    incarnation: i64,
    tx: mpsc::Sender<ToClient>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<FromClient>>>>,
    close: watch::Sender<bool>,
    /// Set by an ask whose enqueue could not complete within the bound,
    /// so the close that follows is named as the peer gone silent.
    silent: Arc<std::sync::atomic::AtomicBool>,
    /// False from the install until the admission committed and the hello
    /// answer was enqueued; an ask treats a not-ready connection as absent,
    /// so no frame of its own enters the ordered channel ahead of the
    /// answer.
    ready: bool,
    /// **On the admin plane, the ceiling this connection's hello declared**
    /// (Spec 8), held with the connection and fixed for its life: every
    /// verb ask is checked against it under the live map's lock, so a
    /// reconnection that narrows the ceiling cannot race an ask. The row's
    /// copy is what surfaces read and never the authorization input.
    ceiling: Option<Arc<BTreeSet<String>>>,
    /// **Each verb ask's record, by its ask** (Spec 2.13, 7.2): the verb
    /// asked and its first record, where the ask is audited. The
    /// connection task's own, filled before the frame enters the channel
    /// and taken by the answer the frame's id names, which is checked
    /// against it, so an answer is matched to its own ask and record only.
    /// It goes with the connection, and an entry still in it then is a
    /// first record with no second, the audit's "unknown".
    asks: Asks,
    /// The places of `PERSON_SLOTS` on this connection.
    person_slots: Arc<tokio::sync::Semaphore>,
    /// **When the server last read a frame from this connection**, any
    /// frame, heartbeats included (Spec 2.14): in memory alone, never
    /// written per heartbeat, for the surfaces' "last heard".
    heard: Arc<Mutex<std::time::Instant>>,
}

/// What the connection holds of one verb ask until its answer.
struct Asked {
    verb: String,
    first: Option<String>,
    /// The act's principal, `server` or a person's `pe-`, for an alert's
    /// operations-log line.
    principal: String,
}

/// The connection's map from an ask's id to its record.
type Asks = Arc<Mutex<HashMap<u64, Asked>>>;

/// The connection's map from an ask's id to its asker's reply.
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<FromClient>>>>;

/// **From an ask's first record to its frame in the channel** (Spec 2.13):
/// whatever ends the ask in between, an error or its future dropped while
/// the send waits on a full channel, removes the ask's entries and answers
/// its first record `failed`, since the frame never left. Armed by the
/// record step before the record's insert and disarmed only once the send
/// has completed; the one cleanup of `Inner::ask` and `ask_show` alike.
/// **Drop cannot await**, so the outcome's write is spawned, held to the
/// silence bound, and waits on the first's outcome lock for its insert's
/// transaction to end (`Store::audit_unsent`).
struct Unsent {
    id: u64,
    agent: AgentId,
    first: Option<String>,
    store: Store,
    silence: Duration,
    pending: Option<Pending>,
    asks: Option<Asks>,
    armed: bool,
}

impl Unsent {
    /// The frame is in the channel: the ask's entries now belong to its
    /// answer, and its first to the outcome the answer writes.
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for Unsent {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(pending) = &self.pending {
            pending.lock().unwrap().remove(&self.id);
        }
        if let Some(asks) = &self.asks {
            asks.lock().unwrap().remove(&self.id);
        }
        let Some(first) = self.first.take() else {
            return;
        };
        let (store, silence, agent) = (self.store.clone(), self.silence, self.agent.clone());
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            tracing::error!("{agent}: no runtime to answer {first} failed, its frame never sent");
            return;
        };
        runtime.spawn(async move {
            match tokio::time::timeout(silence, store.audit_unsent(&first)).await {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => {
                    tracing::info!("{agent}: {first} never landed, so it has no outcome to answer")
                }
                Ok(Err(e)) => tracing::error!(
                    "{agent}: the failed outcome of {first}, its frame never sent, could not be written: {e:#}"
                ),
                Err(_) => tracing::error!(
                    "{agent}: the failed outcome of {first}, its frame never sent, was not written within {silence:?}"
                ),
            }
        });
    }
}

/// Why an ask did not reach a connection.
enum AskError {
    /// No ready connection on the plane, or the ask could not be enqueued
    /// on it: nothing left the server.
    NotConnected,
    /// The ask was enqueued on the connection, and the connection ended
    /// before an answer came back.
    Unanswered,
    /// The verb is outside the connection's ceiling: refused on the server
    /// before any frame left it (Spec 8).
    OutsideCeiling(Vec<String>),
    /// The verb's first record could not be written: nothing was asked.
    Audit,
}

/// Why a verb was not answered with an outcome.
#[derive(Debug)]
pub enum VerbError {
    /// No admin-con is connected for this agent: the verb never left the
    /// server.
    NotConnected,
    /// **The verb was sent and never answered: its outcome is unknown
    /// here.** The connection ended with the verb in flight, and it may
    /// have run on the box, which `toddwbucy/WeaverAgent#60` makes
    /// dangerous to guess about. admin's trace and its next `show` hold
    /// the outcome. It is never "not connected", which would say the verb
    /// did not run.
    Unanswered,
    /// The verb is outside the ceiling the connection declared; refused on
    /// the server before any frame left it (Spec 8).
    OutsideCeiling { verb: String, ceiling: Vec<String> },
    /// admin-con's typed reason it did not run the verb.
    Fault(VerbFault),
    /// **The server asks `show` only** (Spec 2.13): any other verb asked
    /// as the server is refused before any frame leaves, as one refusal
    /// record.
    ServerAsksShowOnly { verb: String },
    /// **The audit could not record the ask**, its first record or its
    /// refusal: the server's failure, and nothing was asked.
    Audit,
    /// The person holds no grant on the agent, or their session no longer
    /// stands: refused with one record, and answered to them as an agent
    /// that does not exist.
    NoGrant,
    /// The person's grants on the agent do not carry the verb: refused
    /// with one record.
    NotGranted { verb: String },
    /// The connection ended after the first record and before the frame
    /// left: nothing was asked, and the first is answered `failed`.
    Lost,
    /// The store could not be read or written for the ask: the server's
    /// failure, and nothing was asked.
    Server(String),
    /// The connection's slots for persons' asks are all taken: refused
    /// with one record.
    Busy,
    /// The ask did not reach its enqueue within its bound, a store or a
    /// queue of asks too slow: the server's failure, and nothing was asked.
    Stalled,
    /// The server is taking no more asks now, or none more of this
    /// person's: nothing was asked and nothing recorded.
    Shed,
}

impl std::fmt::Display for VerbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerbError::NotConnected => write!(f, "no admin-con is connected for this agent"),
            VerbError::Unanswered => write!(
                f,
                "the verb was sent and the connection ended before its answer came back: its outcome is unknown here, and admin's trace and its next show hold it"
            ),
            VerbError::OutsideCeiling { verb, ceiling } => write!(
                f,
                "{verb} is outside the ceiling admin-con declared ({}), so it was not asked",
                if ceiling.is_empty() {
                    "empty".to_owned()
                } else {
                    ceiling.join(", ")
                }
            ),
            VerbError::Fault(fault) => write!(f, "{}: {}", fault.kind, fault.message),
            VerbError::ServerAsksShowOnly { verb } => write!(
                f,
                "the server asks show only, so {verb} was refused and not asked"
            ),
            VerbError::Audit => write!(
                f,
                "the audit could not record the ask, so nothing was asked"
            ),
            VerbError::NoGrant => write!(f, "no grant on the agent"),
            VerbError::NotGranted { verb } => {
                write!(f, "the roles held on the agent do not carry {verb}")
            }
            VerbError::Lost => write!(
                f,
                "the connection ended before the verb left; it was not asked"
            ),
            VerbError::Server(why) => write!(f, "the server could not ask: {why}"),
            VerbError::Busy => write!(
                f,
                "admin-con's slots for persons' asks are all taken; ask again"
            ),
            VerbError::Stalled => write!(
                f,
                "the ask was not made within its bound; nothing was asked"
            ),
            VerbError::Shed => write!(
                f,
                "the server is taking no more asks now; nothing was asked, ask again"
            ),
        }
    }
}

impl std::error::Error for VerbError {}

/// **What a person asks of an agent** (Spec 2.13): a lifecycle verb on the
/// admin plane, or a turn on the gate plane. One mechanism serves both;
/// what differs is the verb the person's grant must carry (`turn` for a
/// turn), the plane, the ceiling (a verb's alone: none applies to a turn,
/// the gate's own admission being the box's), and the frame.
#[derive(Debug)]
pub enum Act {
    Verb(String),
    Turn(TurnText),
}

/// **A turn's text, which goes into its frame and nowhere else** (Spec
/// 2.13: the agent's trace on its box is the record of what was said): not
/// the audit, not a log line, not an error. Its `Debug` names no part of it.
pub struct TurnText(String);

impl TurnText {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

impl std::fmt::Debug for TurnText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TurnText(..)")
    }
}

impl Act {
    /// The plane the act goes on.
    fn plane(&self) -> Plane {
        match self {
            Act::Verb(_) => Plane::Admin,
            Act::Turn(_) => Plane::Gate,
        }
    }

    /// **The verb the person's grant must carry**, which is also the first
    /// record's action: the verb asked, or `turn`.
    fn granted(&self) -> &str {
        match self {
            Act::Verb(verb) => verb,
            Act::Turn(_) => "turn",
        }
    }

    /// The verb the connection's ceiling must grant: a verb's alone.
    fn ceilinged(&self) -> Option<&str> {
        match self {
            Act::Verb(verb) => Some(verb),
            Act::Turn(_) => None,
        }
    }

    /// The act's frame: a verb names its person by `pe-` identity, a claim
    /// the box never reads; a turn carries its text, which it alone does.
    fn frame(self, id: u64, person_id: &str) -> ToClient {
        match self {
            Act::Verb(verb) => ToClient::Verb {
                id,
                verb,
                principal: Principal::Person {
                    person_id: person_id.to_owned(),
                },
            },
            Act::Turn(TurnText(text)) => ToClient::Turn { id, text },
        }
    }
}

/// **Who asks a verb through the server** (Spec 2.13): the session that
/// asks and its person, checked again under the ask's hold.
#[derive(Debug, Clone)]
pub struct Asker {
    pub session_id: i64,
    pub person_id: String,
}

/// **A person's verb in the channel**, its first record durable before it:
/// the record's identity, and the answer to wait for. Waiting is the
/// caller's alone and bounds nothing of the ask; the outcome is written at
/// the answer whatever the caller does.
pub struct Enqueued {
    pub first: String,
    verb: String,
    reply: oneshot::Receiver<FromClient>,
    _unask: Unask,
}

impl Enqueued {
    /// admin's answer, or why there is none.
    pub async fn answer(self) -> Result<VerbOutcome, VerbError> {
        match self.reply.await {
            Ok(FromClient::Verb {
                outcome: Some(o), ..
            }) => Ok(o),
            Ok(FromClient::Verb { error: Some(e), .. }) => Err(VerbError::Fault(e)),
            Ok(_) => Err(VerbError::NotConnected),
            Err(_) => Err(VerbError::Unanswered),
        }
    }

    /// The gate's close of a turn, or why there is none.
    pub async fn close(self) -> Result<GateClose, TurnError> {
        match self.reply.await {
            Ok(FromClient::Turn { close: Some(c), .. }) => Ok(c),
            Ok(FromClient::Turn { error: Some(e), .. }) => Err(TurnError::Gate(e)),
            Ok(_) => Err(TurnError::NotConnected),
            Err(_) => Err(TurnError::Unanswered),
        }
    }

    /// The verb asked, or `turn`.
    pub fn verb(&self) -> &str {
        &self.verb
    }
}

/// **A cancelled ask removes its own entry**: run when the asker's wait is
/// dropped, under the pending map's own lock, so a hung connector that still
/// heartbeats does not collect one entry per cancelled ask. An answered ask
/// finds it gone.
struct Unask(Pending, u64);

impl Drop for Unask {
    fn drop(&mut self) {
        self.0.lock().unwrap().remove(&self.1);
    }
}

/// **How many persons' asks hold the identity exclusion at once.** A held
/// ask keeps one of the store's connections for its hold and takes a second
/// for its first record's insert or its refusal's record, so with every
/// connection held by a hold no insert could be had, and the asks would
/// wait together to the pool's own timeout: eight persons asking at once
/// against the store's eight connections. Three held asks take at most six
/// and leave two for every other writer. An ask waits for its place after
/// its slot is reserved and before its hold, a wait on other asks' store
/// writes and never on a peer; a lifecycle verb is a person's rare act.
const HELD_ASKS: usize = 3;

/// **How many persons' asks the server holds outstanding at once**, from
/// their admission to the end of their task, on every agent. An ask's task
/// is detached so a dropped request cannot cut it, and detached work is
/// admitted against a bound before it is detached: past this the server
/// takes no more asks until some end, so a client posting and disconnecting
/// faster than the held asks drain piles nothing up. Each outstanding ask
/// lives at most two silence bounds and holds a few kilobytes; sixty-four is
/// four connections' worth of `PERSON_SLOTS`, far past what persons asking
/// verbs by hand ever hold at once.
const OUTSTANDING_ASKS: usize = 64;

/// **One person's share of `OUTSTANDING_ASKS`**, so one session cannot
/// starve every other person's verbs: a person asks a verb at a time from
/// a page, and four covers a few tabs with a slow agent behind them, while
/// sixteen persons at their share still fit.
const PERSON_SHARE: usize = 4;

/// **An ask admitted against `OUTSTANDING_ASKS` and its person's share**,
/// taken before its task is detached and carried into it, released when
/// the task ends. `Listener::ask_as_person` takes one, so no person's ask
/// is detached without it.
pub struct Admission {
    _slot: tokio::sync::OwnedSemaphorePermit,
    shares: Arc<Mutex<HashMap<String, usize>>>,
    person: String,
}

impl Drop for Admission {
    fn drop(&mut self) {
        let mut shares = self.shares.lock().unwrap();
        if let Some(held) = shares.get_mut(&self.person) {
            *held -= 1;
            if *held == 0 {
                shares.remove(&self.person);
            }
        }
    }
}

/// **How many persons' asks may hold a slot on one connection at once**,
/// from their reservation to their enqueue. A slot is capacity only the
/// peer can drain, and the write channel holds 64 frames: persons' asks
/// parked on the server's own waits (a place among `HELD_ASKS`, the store)
/// with their slots held would otherwise fill it, and the connection task's
/// own frames, the `landed` that ends admin-con's hold and an opening's
/// `show`, would time out as a silent peer and close a healthy connection.
/// **As many as admin-con's own queue holds** (`admin_con::VERB_QUEUE`,
/// 16), so the server never refuses as busy an ask admin-con would still
/// take, and forty-eight frames stay for the server's own. An ask past the
/// cap takes no slot and decides nothing: it goes into the hold like one
/// with no connection, where the grant is checked first, and is refused as
/// busy.
const PERSON_SLOTS: usize = crate::link::admin_con::VERB_QUEUE;

/// What an ask's reservation found.
enum Slot {
    /// A slot on the connection, and the incarnation it is on.
    Reserved(Reserved),
    /// The connection's persons' slots are all taken.
    Busy,
    /// No ready connection, or none whose channel took a reservation.
    None,
}

/// A slot reserved on a connection, its place among `PERSON_SLOTS`, and
/// the incarnation it is on.
struct Reserved {
    incarnation: i64,
    permit: mpsc::OwnedPermit<ToClient>,
    _place: tokio::sync::OwnedSemaphorePermit,
}

/// Where a test may stop a person's ask (`Listener::pause_ask_at`).
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskPoint {
    /// The slot reserved, the hold not yet taken.
    BeforeHold,
    /// Under the hold, the grants about to be read, inside the deadline.
    AtGrants,
    /// Under the hold, a refusal's record about to be written.
    AtRefusal,
    /// Under the hold, the first record durable, the frame not yet sent.
    UnderHold,
    /// Not an ask's: a connection's teardown written and its row released,
    /// its in-flight acts not yet raised.
    AfterTeardown,
}

/// A test's stop in a person's ask: reached, then released.
#[cfg(test)]
type Pause = (AskPoint, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>);

/// The most verbs a ceiling may name, and the longest name: the hello is
/// bounded like every frame's content (Spec 8, eight verbs today).
const CEILING_BOUND: usize = 16;
const VERB_NAME_BOUND: usize = 32;

/// What landing a verb's answer did: wrote an observation on the row,
/// had nothing to write (no answer, or not a state), or failed at the
/// store.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Landing {
    Observation,
    Nothing,
    Failed,
}

struct Inner {
    store: Store,
    acceptor: TlsAcceptor,
    epoch: i64,
    arrival: AtomicI64,
    silence: Duration,
    live: Mutex<HashMap<(AgentId, Plane), LiveConnection>>,
    windows: TraceViews,
    acknowledged: Mutex<HashMap<AgentId, Position>>,
    next_ask: AtomicU64,
    next_incarnation: AtomicI64,
    address: SocketAddr,
    /// The connection holding the listener's advisory lock, dropped by
    /// `stop` or with the listener; its session ending releases the lock.
    /// The backend holding the listener's advisory lock: what the monitor
    /// pings, and what a test ends to see the listener halt.
    lock_pid: i32,
    /// Asks to the monitor, which owns the lock's connection, to ping it
    /// now and answer (see `prove_lock`).
    prove: mpsc::Sender<oneshot::Sender<Result<(), String>>>,
    cadence: Duration,
    /// Set once, with why, when the listener halted itself; what the
    /// binary awaits so the process exits for its supervisor to restart.
    halted: tokio::sync::watch::Sender<Option<String>>,
    /// The accept loop and the notification task, aborted by `stop` and
    /// by `halt`.
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    /// Every connection task, handshaking or admitted, so `halt` and
    /// `stop` abort and drain them all before the lock's session ends: a
    /// replacement listener must not take the lock while an old task can
    /// still admit the same credential.
    connections: Mutex<tokio::task::JoinSet<()>>,
    /// Set under the set's lock when the listener quiesces, so an accept
    /// that completed a moment before refuses to spawn into a set that
    /// has been drained and would go untracked.
    closed: std::sync::atomic::AtomicBool,
    /// The monitor task, which owns the lock's connection; aborted last.
    monitor: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// Set when the listener quiesces, so a connection still handshaking
    /// or waiting for its hello ends on its own rather than by abort.
    halting: watch::Sender<bool>,
    /// Teardowns whose disconnected write the store refused: the row still
    /// records the incarnation as connected, so each is retried until it
    /// lands or the incarnation is superseded (a reconciliation, not a
    /// forgetting). The startup reset covers a restart.
    failed_teardowns: Mutex<Vec<(AgentId, Plane, i64)>>,
    /// Whether the one reconciliation worker runs; set and cleared under
    /// the list's lock, so the worker exits exactly when the list empties
    /// and a new entry starts exactly one.
    reconciling: std::sync::atomic::AtomicBool,
    /// The places of `HELD_ASKS`.
    held: tokio::sync::Semaphore,
    /// **The alerts' raising half** (Spec 2.14): never waited on.
    alerts: Alerts,
    /// The places of `OUTSTANDING_ASKS`, and each person's count of them.
    outstanding: Arc<tokio::sync::Semaphore>,
    shares: Arc<Mutex<HashMap<String, usize>>>,
    #[cfg(test)]
    fault_next_teardown: std::sync::atomic::AtomicBool,
    /// A test's lever: the next entry's take fails once.
    #[cfg(test)]
    fault_next_entry: std::sync::atomic::AtomicBool,
    /// A test's one lever on the store: the next landing fails once, so
    /// the ack's dependence on the write can be watched.
    #[cfg(test)]
    fault_next_land: std::sync::atomic::AtomicBool,
    #[cfg(test)]
    stall_next_land: std::sync::atomic::AtomicBool,
    /// The same lever on a verb's first record: the next one never
    /// completes, so its bound can be watched.
    #[cfg(test)]
    stall_next_first: std::sync::atomic::AtomicBool,
    /// And on a verb's outcome: the next one waits this long first.
    #[cfg(test)]
    delay_next_outcome: Mutex<Option<Duration>>,
    /// The stops a test set in persons' asks.
    #[cfg(test)]
    pauses: Mutex<Vec<Pause>>,
}

/// The server's handle on the link: the asks it routes and the window it
/// reads. Clones share one listener.
#[derive(Clone)]
pub struct Listener {
    inner: Arc<Inner>,
}

/// What a turn ask can fail with on the server side.
#[derive(Debug)]
pub enum TurnError {
    /// The gate's own typed error, relayed.
    Gate(TurnFault),
    /// No gate-con is connected for this agent: the turn never left the
    /// server.
    NotConnected,
    /// **The turn was sent and never answered: its outcome is unknown
    /// here.** The connection ended with the turn in flight, at gate-con's
    /// shutdown or a link loss. A turn whose request crossed to the agent
    /// runs to its end whatever becomes of its caller
    /// (`toddwbucy/WeaverAgent#59`), so it may have been answered, and the
    /// agent's trace, relayed by admin-con, is where its outcome is read.
    /// It is never "not connected", which would say the turn did not run.
    Unanswered,
    /// The server refused the turn before any frame left, or did not take
    /// it: the person's ask's own answer.
    Refused(VerbError),
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnError::Gate(e) => write!(f, "{}", e.message),
            TurnError::NotConnected => write!(f, "no gate-con is connected for this agent"),
            TurnError::Unanswered => write!(
                f,
                "the turn was sent and the connection ended before its close came back: its outcome is unknown here, and the agent's trace holds it"
            ),
            TurnError::Refused(refused) => write!(f, "{refused}"),
        }
    }
}

impl Listener {
    /// **Start** (Spec 8): the authority is already loaded, so this binds,
    /// then in one transaction increments the epoch and resets every plane
    /// recorded as connected, and only then accepts. The notification
    /// channel for revocations is subscribed before the first accept, so
    /// no revocation committed after this returns can be missed.
    pub async fn start(
        store: Store,
        authority: &Authority,
        listen: &str,
        silence: Duration,
    ) -> anyhow::Result<Self> {
        Self::start_with(store, authority, listen, silence, None).await
    }

    /// **Start, writing the operations log** (Spec 2.14) at the path the
    /// config names: each failure the listener raises as an alert is also a
    /// line there.
    pub async fn start_with(
        store: Store,
        authority: &Authority,
        listen: &str,
        silence: Duration,
        operations_log: Option<std::path::PathBuf>,
    ) -> anyhow::Result<Self> {
        if silence.as_secs() < LEAST_SILENCE_SECS {
            anyhow::bail!(
                "the silence bound is {silence:?}, under the {LEAST_SILENCE_SECS} s the cadence rule admits"
            );
        }
        let acceptor = TlsAcceptor::from(authority.server_tls()?);
        let tcp = TcpListener::bind(listen).await?;
        let address = tcp.local_addr()?;
        // **The store's lock first** (Spec 8): a session-level advisory
        // lock on a connection detached from the pool, so it lives with
        // this listener and with nothing the pool hands out later. A
        // predecessor's session may still be closing, so the try is
        // repeated briefly before the refusal.
        let mut lock = store.pool.acquire().await?.detach();
        let mut held = false;
        for _ in 0..20 {
            held = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
                .bind(LISTENER_LOCK_KEY)
                .fetch_one(&mut lock)
                .await?;
            if held {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if !held {
            anyhow::bail!(
                "another weaver-web listener holds this store (advisory lock {LISTENER_LOCK_KEY}): one listener per store, and this one refuses to start beside it"
            );
        }
        let (prove_tx, mut prove_rx) = mpsc::channel::<oneshot::Sender<Result<(), String>>>(16);
        let lock_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut lock)
            .await?;
        let (halted, _) = tokio::sync::watch::channel(None);
        let epoch = store.listener_start().await?;
        let mut notifications = sqlx::postgres::PgListener::connect_with(&store.pool).await?;
        notifications.listen(REVOCATION_CHANNEL).await?;
        let alerts = Alerts::start(store.clone(), operations_log);
        let listener = Self {
            inner: Arc::new(Inner {
                store,
                acceptor,
                epoch,
                arrival: AtomicI64::new(0),
                silence,
                live: Mutex::new(HashMap::new()),
                windows: TraceViews::new(),
                acknowledged: Mutex::new(HashMap::new()),
                next_ask: AtomicU64::new(1),
                next_incarnation: AtomicI64::new(1),
                address,
                lock_pid,
                prove: prove_tx,
                cadence: silence / 4,
                halted,
                tasks: Mutex::new(Vec::new()),
                connections: Mutex::new(tokio::task::JoinSet::new()),
                closed: std::sync::atomic::AtomicBool::new(false),
                monitor: Mutex::new(None),
                halting: watch::channel(false).0,
                failed_teardowns: Mutex::new(Vec::new()),
                reconciling: std::sync::atomic::AtomicBool::new(false),
                held: tokio::sync::Semaphore::new(HELD_ASKS),
                alerts,
                outstanding: Arc::new(tokio::sync::Semaphore::new(OUTSTANDING_ASKS)),
                shares: Arc::new(Mutex::new(HashMap::new())),
                #[cfg(test)]
                fault_next_teardown: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                fault_next_entry: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                fault_next_land: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                stall_next_land: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                stall_next_first: std::sync::atomic::AtomicBool::new(false),
                #[cfg(test)]
                delay_next_outcome: Mutex::new(None),
                #[cfg(test)]
                pauses: Mutex::new(Vec::new()),
            }),
        };
        tracing::info!("link listening on {address} under epoch {epoch}");

        let weak = Arc::downgrade(&listener.inner);
        let accept = tokio::spawn(async move {
            loop {
                let (stream, peer) = match tcp.accept().await {
                    Ok(x) => x,
                    Err(e) => {
                        tracing::error!("link accept failed: {e}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                let Some(inner) = weak.upgrade() else { return };
                let mut set = inner.connections.lock().unwrap();
                if inner.closed.load(Ordering::Acquire) {
                    tracing::info!("link from {peer}: the listener is closing, refused");
                    drop(stream);
                    continue;
                }
                while set.try_join_next().is_some() {}
                set.spawn(serve_connection(inner.clone(), stream, peer));
            }
        });

        // **The revocation channel outlives its errors, and the sweep
        // follows the re-subscription.** A notification raised while the
        // connection to the store is down is lost, so once the subscription
        // stands again the live map is swept against the register and
        // anything revoked is closed. The order is the point: a revocation
        // committed after a sweep but before LISTEN is back is neither
        // swept nor delivered, while with LISTEN back first every
        // revocation is one or the other. `try_recv` answers `None` only
        // after a lost connection was re-established and re-LISTENed
        // (sqlx's eager reconnect), so a sweep after `None` is after the
        // re-subscription; where the re-establishment itself fails the
        // channel is re-listened explicitly until it stands, and then
        // swept. (sqlx records the channel again on each explicit listen;
        // a duplicate LISTEN is a no-op, one entry per outage survived.)
        let weak = Arc::downgrade(&listener.inner);
        let notify = tokio::spawn(async move {
            loop {
                match notifications.try_recv().await {
                    Ok(Some(notification)) => {
                        let Some(inner) = weak.upgrade() else { return };
                        inner.close_fingerprint(notification.payload());
                        continue;
                    }
                    Ok(None) => {
                        tracing::warn!(
                            "the revocation channel was lost and stands again; sweeping the live map"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "the revocation channel failed: {e}; re-establishing it before the sweep"
                        );
                        loop {
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            if weak.upgrade().is_none() {
                                return;
                            }
                            match notifications.listen(REVOCATION_CHANNEL).await {
                                Ok(()) => break,
                                Err(e) => {
                                    tracing::error!("re-establishing the revocation channel: {e}");
                                }
                            }
                        }
                    }
                }
                let Some(inner) = weak.upgrade() else { return };
                inner.sweep_revoked().await;
                inner.reconcile_teardowns().await;
            }
        });
        // **The lock's session is monitored, and its loss halts the
        // listener.** The session holding the advisory lock is pinged at
        // the heartbeat's cadence; when the ping fails the lock is gone
        // with the session, another listener may already hold the store,
        // and reacquiring after a gap would be that listener's chance
        // already taken. So this one stops: every connection closed,
        // nothing accepted, and `halted` set for the binary to exit on, so
        // the operator's supervisor restarts it into a clean start.
        let weak = Arc::downgrade(&listener.inner);
        let cadence = silence / 4;
        // The monitor owns the lock's connection, so the connection ends
        // with the monitor's task, and pings on its own cadence or on an
        // admission's ask, whichever comes first.
        let monitor = tokio::spawn(async move {
            let mut lock = lock;
            loop {
                let ask = tokio::select! {
                    _ = tokio::time::sleep(cadence) => None,
                    ask = prove_rx.recv() => match ask {
                        Some(ask) => Some(ask),
                        // Every asker is gone with the listener's state.
                        None => return,
                    },
                };
                let ping =
                    tokio::time::timeout(cadence, sqlx::query("SELECT 1").execute(&mut lock)).await;
                let lost = match ping {
                    Ok(Ok(_)) => None,
                    Ok(Err(e)) => Some(e.to_string()),
                    Err(_) => Some("the ping did not answer within the cadence".to_owned()),
                };
                if let Some(ask) = ask {
                    let _ = ask.send(lost.clone().map_or(Ok(()), Err));
                }
                if let Some(why) = lost {
                    let Some(inner) = weak.upgrade() else { return };
                    inner
                        .halt(format!(
                            "the session holding the listener's lock was lost ({why}); another listener may already hold the store, so this one stops rather than reacquiring"
                        ))
                        .await;
                    return;
                }
            }
        });
        listener
            .inner
            .tasks
            .lock()
            .unwrap()
            .extend([accept, notify]);
        *listener.inner.monitor.lock().unwrap() = Some(monitor);
        Ok(listener)
    }

    /// The backend holding the listener's advisory lock.
    pub fn lock_pid(&self) -> i32 {
        self.inner.lock_pid
    }

    /// Resolves with why, once the listener has halted itself.
    pub async fn halted(&self) -> String {
        let mut rx = self.inner.halted.subscribe();
        loop {
            if let Some(why) = rx.borrow().clone() {
                return why;
            }
            if rx.changed().await.is_err() {
                return "the listener is gone".into();
            }
        }
    }

    /// Stop accepting and release the store's lock, so another listener
    /// may start against the store. Connections already admitted run to
    /// their own close; the process that owned them is, in the real case,
    /// gone.
    pub async fn stop(&self) {
        // Nothing more accepted, every connection task aborted and
        // drained, and only then the monitor, whose dropped connection
        // ends the session and releases the lock.
        self.inner.quiesce().await;
        if let Some(monitor) = self.inner.monitor.lock().unwrap().take() {
            monitor.abort();
        }
    }

    /// `stop` for a place that cannot await: everything aborted, nothing
    /// drained. The lock's release may then precede a connection task's
    /// end by the moment it takes to drop.
    pub fn stop_now(&self) {
        for task in self.inner.tasks.lock().unwrap().drain(..) {
            task.abort();
        }
        self.inner.connections.lock().unwrap().abort_all();
        if let Some(monitor) = self.inner.monitor.lock().unwrap().take() {
            monitor.abort();
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.inner.address
    }

    pub fn epoch(&self) -> i64 {
        self.inner.epoch
    }

    /// **The alerts' raising half**, for what a surface raises itself.
    pub fn alerts(&self) -> &Alerts {
        &self.inner.alerts
    }

    /// **How long since the server last read a frame from the agent's live
    /// connection on the plane** (Spec 2.14), any frame, heartbeats
    /// included; `None` where no admitted connection stands.
    pub fn last_heard(&self, agent: &AgentId, plane: Plane) -> Option<Duration> {
        let live = self.inner.live.lock().unwrap();
        live.get(&(agent.clone(), plane))
            .filter(|c| c.ready)
            .map(|c| c.heard.lock().unwrap().elapsed())
    }

    /// **The silence bound**, which a caller waiting for a verb's answer
    /// holds its own wait to.
    pub fn silence(&self) -> Duration {
        self.inner.silence
    }

    /// Stop the next person's ask at `point`: the first notifier fires when
    /// it is reached, and the ask goes on once the second is notified.
    #[cfg(test)]
    pub fn pause_ask_at(
        &self,
        point: AskPoint,
    ) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
        let (reached, release) = (
            Arc::new(tokio::sync::Notify::new()),
            Arc::new(tokio::sync::Notify::new()),
        );
        self.inner
            .pauses
            .lock()
            .unwrap()
            .push((point, reached.clone(), release.clone()));
        (reached, release)
    }

    /// The store the listener writes, for a test's fixtures.
    #[cfg(test)]
    pub fn store(&self) -> &Store {
        &self.inner.store
    }

    /// The live window: one ring per agent, fed by the admin plane.
    pub fn windows(&self) -> &TraceViews {
        &self.inner.windows
    }

    /// The acknowledged position this process holds for an agent's trace
    /// (Spec 7.2), or none.
    pub fn acknowledged(&self, agent: &AgentId) -> Option<Position> {
        self.inner.acknowledged.lock().unwrap().get(agent).cloned()
    }

    /// Whether a connection is installed for the agent's plane.
    pub fn connected(&self, agent: &AgentId, plane: Plane) -> bool {
        self.inner
            .live
            .lock()
            .unwrap()
            .contains_key(&(agent.clone(), plane))
    }

    /// Make the next store write of an observation fail, once.
    #[cfg(test)]
    pub fn fail_next_land(&self) {
        self.inner.fault_next_land.store(true, Ordering::Relaxed);
    }

    /// Make the next store write of an observation never complete, once.
    #[cfg(test)]
    pub fn stall_next_land(&self) {
        self.inner.stall_next_land.store(true, Ordering::Relaxed);
    }

    /// Make the next verb's first record never complete, once.
    #[cfg(test)]
    pub fn stall_next_first(&self) {
        self.inner.stall_next_first.store(true, Ordering::Relaxed);
    }

    /// How many asks the agent's admin connection holds: its pending
    /// replies and its first records.
    #[cfg(test)]
    pub fn held_asks(&self, agent: &AgentId) -> Option<(usize, usize)> {
        let live = self.inner.live.lock().unwrap();
        live.get(&(agent.clone(), Plane::Admin)).map(|c| {
            (
                c.pending.lock().unwrap().len(),
                c.asks.lock().unwrap().len(),
            )
        })
    }

    /// Make the next verb's outcome wait `delay` before its write, once.
    #[cfg(test)]
    pub fn delay_next_outcome(&self, delay: Duration) {
        *self.inner.delay_next_outcome.lock().unwrap() = Some(delay);
    }

    /// Make the next entry's take fail, once.
    #[cfg(test)]
    pub fn fail_next_entry(&self) {
        self.inner.fault_next_entry.store(true, Ordering::Relaxed);
    }

    /// Make the next teardown's store write fail, once.
    #[cfg(test)]
    pub fn fail_next_teardown(&self) {
        self.inner
            .fault_next_teardown
            .store(true, Ordering::Relaxed);
    }

    /// Retry every teardown the store refused, once each; what the retry
    /// task does every second while any stands, and what a sweep does.
    pub async fn reconcile_teardowns(&self) {
        self.inner.reconcile_teardowns().await
    }

    /// How many teardowns still wait for the store.
    pub fn failed_teardowns(&self) -> usize {
        self.inner.failed_teardowns.lock().unwrap().len()
    }

    /// Close every live connection whose credential the register no longer
    /// holds live: what the notification task does after an error, and
    /// what an operator's tooling may ask for.
    pub async fn sweep_revoked(&self) {
        self.inner.sweep_revoked().await
    }

    /// **The server's own verb across the link** (Spec 7.2, 2.13): routed
    /// to the agent's admin-con, whose answer lands on the row where it is
    /// `show` before it is answered here, and **audited as the server's**,
    /// its first record committed before the frame enters the channel and
    /// its outcome written when the answer lands. **The server asks `show`
    /// only**: any other verb is refused here, before any frame leaves, as
    /// one refusal record, and a refusal the audit cannot record answers
    /// the server's failure instead. **A verb outside the ceiling the live
    /// connection declared is refused here** too, before any record.
    pub async fn verb(&self, agent: &AgentId, verb: &str) -> Result<VerbOutcome, VerbError> {
        if verb != "show" {
            let refused = tokio::time::timeout(
                self.inner.silence,
                self.inner.store.audit_refusal(
                    audit::Principal::Server,
                    Target::Agent(agent.as_str()),
                    verb,
                    "the server asks show only",
                ),
            )
            .await;
            return match refused {
                Ok(Ok(_)) => Err(VerbError::ServerAsksShowOnly {
                    verb: verb.to_owned(),
                }),
                Ok(Err(e)) => {
                    tracing::error!(
                        "{agent}: the server's refusal of {verb} could not be recorded: {e:#}"
                    );
                    Err(VerbError::Audit)
                }
                Err(_) => {
                    tracing::error!(
                        "{agent}: the server's refusal of {verb} was not recorded within {:?}",
                        self.inner.silence
                    );
                    Err(VerbError::Audit)
                }
            };
        }
        self.ask_verb(agent, verb).await
    }

    /// **A person's ask admitted, or not**: a place among
    /// `OUTSTANDING_ASKS` and within the person's `PERSON_SHARE`, taken
    /// without waiting. `None` is the server taking no more asks now, which
    /// is no refusal of the person's authority and writes no record.
    pub fn admit_ask(&self, person_id: &str) -> Option<Admission> {
        let mut shares = self.inner.shares.lock().unwrap();
        if shares.get(person_id).copied().unwrap_or(0) >= PERSON_SHARE {
            return None;
        }
        let slot = self.inner.outstanding.clone().try_acquire_owned().ok()?;
        *shares.entry(person_id.to_owned()).or_insert(0) += 1;
        Some(Admission {
            _slot: slot,
            shares: self.inner.shares.clone(),
            person: person_id.to_owned(),
        })
    }

    /// How many persons' slots the agent's connection on the plane holds.
    #[cfg(test)]
    pub fn slots_taken(&self, agent: &AgentId, plane: Plane) -> Option<usize> {
        let live = self.inner.live.lock().unwrap();
        live.get(&(agent.clone(), plane))
            .map(|c| PERSON_SLOTS - c.person_slots.available_permits())
    }

    /// How many persons' asks are outstanding.
    #[cfg(test)]
    pub fn outstanding_asks(&self) -> usize {
        OUTSTANDING_ASKS - self.inner.outstanding.available_permits()
    }

    /// **A person's verb, asked through the server** (Spec 2.13, 8): the
    /// whole ask, from the reservation of its slot to its enqueue and its
    /// refusals' records, runs in a task its caller cannot cancel, which
    /// `Inner::ask_held` describes, **admitted before it is detached** and
    /// carrying its admission until it ends; this waits for that task and
    /// hands back the frame in the channel, or the refusal.
    pub async fn ask_as_person(
        &self,
        admission: Admission,
        asker: &Asker,
        agent: &AgentId,
        act: Act,
    ) -> Result<Enqueued, VerbError> {
        self.ask_under(admission, asker, agent, act).await.1
    }

    /// **[`Listener::ask_as_person`], its admission handed back once the
    /// ask's task ends** (Spec 2.14), so a caller that goes on waiting, a
    /// decision's sequence through its answers and its next ask, holds its
    /// one place throughout and asks again under it rather than admitting
    /// twice. `None` where the ask's task itself ended abnormally, the
    /// admission going with it.
    pub async fn ask_under(
        &self,
        admission: Admission,
        asker: &Asker,
        agent: &AgentId,
        act: Act,
    ) -> (Option<Admission>, Result<Enqueued, VerbError>) {
        let (inner, asker_, agent_) = (self.inner.clone(), asker.clone(), agent.clone());
        let (plane, subject) = (act.plane(), act.granted().to_owned());
        let task = async move {
            let mut authorized = false;
            let asked = inner.ask_held(&asker_, &agent_, act, &mut authorized).await;
            (asked, authorized, admission)
        };
        let (asked, authorized, admission) = match tokio::spawn(task).await {
            Ok((asked, authorized, admission)) => (asked, authorized, Some(admission)),
            Err(e) => {
                tracing::error!("{agent}: a person's {subject} was not asked: its task ended: {e}");
                (
                    Err(VerbError::Server(format!("the ask's task ended: {e}"))),
                    false,
                    None,
                )
            }
        };
        // **A refusal for a system reason is an alert** (Spec 2.14), **and
        // only where the act's authorization stood**: the session standing
        // and a grant carrying the act. A failure before that check is the
        // server's log line at error and never an alert, so a person holding
        // no grant on an agent raises nothing on it. Not an authorization
        // refusal either, which is the audit's, and not a shed request,
        // which by design leaves no trace. NotConnected, Busy,
        // OutsideCeiling and Lost are reached only past the check.
        if authorized
            && let Err(refused) = &asked
            && matches!(
                refused,
                VerbError::NotConnected
                    | VerbError::Busy
                    | VerbError::Stalled
                    | VerbError::Audit
                    | VerbError::Lost
                    | VerbError::OutsideCeiling { .. }
                    | VerbError::Server(_)
            )
        {
            self.inner.alerts.raise(failure_of(
                agent,
                plane,
                &subject,
                None,
                asker.person_id.clone(),
                Raised {
                    kind: AlertKind::Refused,
                    summary: format!("{subject} was not asked: {refused}"),
                    detail: String::new(),
                    reason: serde_json::json!({ "refusal": refused.to_string() }),
                    options: Vec::new(),
                    outcome: "refused",
                },
            ));
        }
        (admission, asked)
    }

    /// A person's verb, admitted, asked, and its answer awaited without a
    /// bound.
    pub async fn verb_for_person(
        &self,
        asker: &Asker,
        agent: &AgentId,
        verb: &str,
    ) -> Result<VerbOutcome, VerbError> {
        let admission = self.admit_ask(&asker.person_id).ok_or(VerbError::Shed)?;
        self.ask_as_person(admission, asker, agent, Act::Verb(verb.to_owned()))
            .await?
            .answer()
            .await
    }

    /// **A person's turn** (Spec 7.1, 2.13), admitted, placed through the
    /// same held ask as a verb, and its close awaited without a bound. **No
    /// deadline on the close**: the gate serializes turns and a queued turn
    /// legitimately waits; a connector that drops with the turn in flight
    /// answers it `Unanswered`, its outcome unknown.
    pub async fn turn_for_person(
        &self,
        asker: &Asker,
        agent: &AgentId,
        text: &str,
    ) -> Result<GateClose, TurnError> {
        let admission = self
            .admit_ask(&asker.person_id)
            .ok_or(TurnError::Refused(VerbError::Shed))?;
        match self
            .ask_as_person(admission, asker, agent, Act::Turn(TurnText::new(text)))
            .await
        {
            Ok(enqueued) => enqueued.close().await,
            Err(VerbError::NotConnected) => Err(TurnError::NotConnected),
            Err(refused) => Err(TurnError::Refused(refused)),
        }
    }

    /// The server's own verb across the link, its frame's principal and its
    /// first record's the server.
    async fn ask_verb(&self, agent: &AgentId, verb: &str) -> Result<VerbOutcome, VerbError> {
        let v = verb.to_owned();
        match self
            .inner
            .ask(agent, verb, audit::Principal::Server, |id| ToClient::Verb {
                id,
                verb: v,
                principal: Principal::Server,
            })
            .await
        {
            Ok(FromClient::Verb {
                outcome: Some(o), ..
            }) => Ok(o),
            Ok(FromClient::Verb { error: Some(e), .. }) => Err(VerbError::Fault(e)),
            Err(AskError::OutsideCeiling(ceiling)) => Err(VerbError::OutsideCeiling {
                verb: verb.to_owned(),
                ceiling,
            }),
            Err(AskError::Unanswered) => Err(VerbError::Unanswered),
            Err(AskError::Audit) => Err(VerbError::Audit),
            _ => Err(VerbError::NotConnected),
        }
    }
}

impl Inner {
    /// **The record step** (Spec 2.13): a verb's first record, written
    /// before its frame enters the channel, for `Inner::ask` and `ask_show`
    /// alike. **The duty to answer the first `failed` is owned from before
    /// its insert until its frame is in the channel**:
    ///
    /// - the record's identity is drawn here and the `Unsent` guard armed
    ///   with it before the insert, which takes the record's outcome lock;
    /// - the insert runs in a task of its own, which its asker cannot
    ///   cancel, and hands the armed guard back; a guard dropped anywhere,
    ///   in the task on an insert error or a lost answer, in the hand-over
    ///   where the asker is gone, or in the ask before its send completes,
    ///   answers the first `failed` under the same lock, so only once the
    ///   insert's transaction has ended, and only where it landed;
    /// - the asker waits to the silence bound, so a store that stalls never
    ///   holds an ask, or the connection task asking its own `show`, past
    ///   it; the task finishes on its own and its guard answers.
    ///
    /// 6b2 holds the identity exclusion shared from the person's check
    /// across this step to the enqueue.
    async fn record_ask(
        &self,
        ask: u64,
        agent: &AgentId,
        verb: &str,
        principal: audit::Principal<'_>,
    ) -> anyhow::Result<Unsent> {
        let first = audit::draw_audit_id();
        let record = audit::FirstRecord::new(principal, Target::Agent(agent.as_str()), verb);
        let unsent = Unsent {
            id: ask,
            agent: agent.clone(),
            first: Some(first.clone()),
            store: self.store.clone(),
            silence: self.silence,
            pending: None,
            asks: None,
            armed: true,
        };
        let (store, silence, owner) = (self.store.clone(), self.silence, agent.clone());
        #[cfg(test)]
        let stall = self.stall_next_first.swap(false, Ordering::Relaxed);
        let (hand_over, handed) = oneshot::channel();
        tokio::spawn(async move {
            let insert = async {
                #[cfg(test)]
                if stall {
                    std::future::pending::<()>().await;
                }
                store.audit_first_as(&first, &record).await
            };
            match tokio::time::timeout(silence, insert).await {
                // A receiver gone hands the guard back here, and it drops.
                Ok(Ok(())) => {
                    let _ = hand_over.send(unsent);
                }
                Ok(Err(e)) => {
                    tracing::error!("{owner}: the first record {first} was not written: {e:#}");
                }
                Err(_) => tracing::error!(
                    "{owner}: the first record {first} was not written within {silence:?}"
                ),
            }
        });
        match tokio::time::timeout(self.silence, handed).await {
            Ok(Ok(unsent)) => Ok(unsent),
            Ok(Err(_)) => anyhow::bail!("not written"),
            Err(_) => anyhow::bail!("not written within {:?}", self.silence),
        }
    }

    /// **A verb's outcome, naming its first** (Spec 2.13), bounded. One that
    /// cannot be written is logged at error and changes nothing else: the
    /// act already happened, or never left.
    async fn record_outcome(&self, agent: &AgentId, first: &str, ok: bool) {
        let outcome = async {
            #[cfg(test)]
            {
                let delay = self.delay_next_outcome.lock().unwrap().take();
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
            }
            self.store.audit_outcome(first, ok).await
        };
        match tokio::time::timeout(self.silence, outcome).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                tracing::error!("{agent}: the outcome of {first} could not be written: {e:#}")
            }
            Err(_) => tracing::error!(
                "{agent}: the outcome of {first} was not written within {:?}",
                self.silence
            ),
        }
    }

    /// **The server's own ask** (Spec 2.13): its `show`, on the admin
    /// plane, its first record the server's. Every other frame either plane
    /// carries is a person's, through `Inner::ask_held`, so no frame leaves
    /// the server without its first record.
    async fn ask(
        &self,
        agent: &AgentId,
        verb: &str,
        principal: audit::Principal<'_>,
        make: impl FnOnce(u64) -> ToClient,
    ) -> Result<FromClient, AskError> {
        let id = self.next_ask.fetch_add(1, Ordering::Relaxed);
        let plane = Plane::Admin;
        // **Checked before any record**: no ready connection, or a verb
        // outside its ceiling, asks nothing and writes nothing.
        let incarnation = {
            let live = self.live.lock().unwrap();
            let conn = live
                .get(&(agent.clone(), plane))
                .filter(|c| c.ready)
                .ok_or(AskError::NotConnected)?;
            // **The ceiling is the live connection's own**, checked under
            // the same lock that found the connection (Spec 8).
            let ceiling = conn.ceiling.as_deref().cloned().unwrap_or_default();
            if !ceiling.contains(verb) {
                return Err(AskError::OutsideCeiling(ceiling.into_iter().collect()));
            }
            conn.incarnation
        };
        // **The first record commits before the frame enters the channel**
        // (Spec 2.13), outside the live map's lock since it awaits the
        // store.
        // **Armed from before the record until the frame is in the
        // channel.**
        let mut unsent = match self.record_ask(id, agent, verb, principal).await {
            Ok(unsent) => unsent,
            Err(e) => {
                tracing::error!(
                    "{agent} ({plane}): the first record of {verb} could not be written, nothing asked: {e:#}"
                );
                return Err(AskError::Audit);
            }
        };
        let first = unsent.first.clone();
        let (reply_tx, reply_rx) = oneshot::channel();
        // **The sender is inserted in the same critical section that found
        // the connection**: a teardown then either sees the entry and drops
        // its pending map, failing this await, or has already removed it and
        // this finds no connection. Inserted outside the lock, the sender
        // could land in a map nothing reads and wait forever. **Found again
        // as the incarnation checked above**, so the ceiling that admitted
        // the verb is the one it is sent under.
        let found = {
            let live = self.live.lock().unwrap();
            live.get(&(agent.clone(), plane))
                .filter(|c| c.ready && c.incarnation == incarnation)
                .map(|conn| {
                    conn.pending.lock().unwrap().insert(id, reply_tx);
                    // **Every ask is recorded with its verb**, so its
                    // answer is checked against it.
                    conn.asks.lock().unwrap().insert(
                        id,
                        Asked {
                            verb: verb.to_owned(),
                            first: first.clone(),
                            principal: "server".to_owned(),
                        },
                    );
                    (
                        conn.tx.clone(),
                        conn.pending.clone(),
                        conn.asks.clone(),
                        conn.close.clone(),
                        conn.silent.clone(),
                    )
                })
        };
        // **A frame that never entered the channel is an act that never
        // happened**: each return below, and a drop of this future at the
        // send, leaves `unsent` to answer its first `failed`.
        let Some((tx, pending, asks, close, silent)) = found else {
            return Err(AskError::NotConnected);
        };
        unsent.pending = Some(pending.clone());
        unsent.asks = Some(asks);
        // **Bounded like every enqueue, and a bound that passes is the
        // peer gone silent**: a connector that stopped reading fills the
        // write channel, and the connection task, which on the gate plane
        // sends nothing of its own, would otherwise live on while every
        // ask failed. The ask fails and the connection is closed as silent.
        // A send that does not complete leaves nothing in the channel.
        match tokio::time::timeout(self.silence, tx.send(make(id))).await {
            Ok(Ok(())) => unsent.disarm(),
            Ok(Err(_)) => return Err(AskError::NotConnected),
            Err(_) => {
                tracing::warn!(
                    "{agent} ({plane}): an ask could not be enqueued within the bound, the peer stopped reading"
                );
                silent.store(true, Ordering::Relaxed);
                let _ = close.send(true);
                return Err(AskError::NotConnected);
            }
        }
        let _unask = Unask(pending.clone(), id);
        // Enqueued: from here an ended connection leaves the ask's outcome
        // unknown rather than never sent.
        reply_rx.await.map_err(|_| AskError::Unanswered)
    }

    /// **A person's verb, from its slot to its enqueue** (Spec 2.13, as
    /// built on 2026-10-13), run by `Listener::ask_as_person` in a task its
    /// caller cannot cancel, so a request dropped mid-ask leaves it to
    /// finish: refused with its record, or enqueued with its first.
    ///
    /// 1. **The slot, reserved before the hold**, held to the silence
    ///    bound with the silent-peer close: no wait on a peer runs under
    ///    the hold. The reservation decides nothing: an ask with none goes
    ///    into the hold all the same, where the grant is checked first, so
    ///    a person with no grant meets the unknown agent's answer and never
    ///    learns the agent's presence.
    /// 2. **A place among `HELD_ASKS`**, so held asks never take every
    ///    connection the store has, then **the hold**, the identity
    ///    exclusion shared, in one transaction:
    ///    the session standing and the person's grants on the agent (the
    ///    per-agent ones alone), then the live connection the slot was
    ///    reserved on, ready, and its ceiling. A refusal is one record,
    ///    written under the hold, and asks nothing.
    /// 3. **The first record**, durable through the record step, still
    ///    under the hold.
    /// 4. **The enqueue**, through the permit on the incarnation the
    ///    ceiling was read from; then the hold's commit.
    ///
    /// **From the place to the commit, one deadline**, the silence bound
    /// read at each await: a store or a queue of asks that stalls answers
    /// the ask as the server's failure within it and frees its place. The
    /// whole task is so bounded by the slot's wait and this deadline.
    ///
    /// A revocation or a disable either commits before the check or waits
    /// for the enqueue. Store I/O runs under the hold (the checks, the
    /// records); peer I/O never does.
    /// **`authorized` is set once the act's authorization stands**, the
    /// session standing and a grant carrying the act, so the caller raises
    /// an alert only for a failure past that check (Spec 2.14).
    async fn ask_held(
        &self,
        asker: &Asker,
        agent: &AgentId,
        act: Act,
        authorized: &mut bool,
    ) -> Result<Enqueued, VerbError> {
        let id = self.next_ask.fetch_add(1, Ordering::Relaxed);
        let plane = act.plane();
        // **The verb the grant must carry and the first record's action**:
        // the verb, or `turn`; never a turn's text, which only the frame
        // carries.
        let granted = act.granted().to_owned();
        let verb = granted.as_str();
        let reserved = self.reserve(agent, plane).await;
        // **One deadline from the place to the commit** (6b1's rule: nothing
        // waits unboundedly on the store), an instant read at each await.
        // Passed before the first record, the ask is the server's failure
        // with nothing asked and nothing recorded; passed in the record
        // step, `Unsent` answers its first `failed`.
        let deadline = tokio::time::Instant::now() + self.silence;
        let stalled = |what: &str| {
            tracing::error!(
                "{agent}: a person's {verb} was not asked: {what} did not complete within {:?}",
                self.silence
            );
            VerbError::Stalled
        };
        let server = |what: &str, e: anyhow::Error| {
            tracing::error!("{agent}: a person's {verb} was not asked: {what}: {e:#}");
            VerbError::Server(format!("{what}: {e:#}"))
        };
        // A place among the held asks, after the slot and before the hold;
        // the semaphore is never closed.
        let Ok(_held) = tokio::time::timeout_at(deadline, self.held.acquire()).await else {
            return Err(stalled("a place among the held asks"));
        };
        #[cfg(test)]
        self.pause_at(AskPoint::BeforeHold).await;
        let mut hold = match tokio::time::timeout_at(deadline, shared_hold(&self.store)).await {
            Ok(hold) => hold.map_err(|e| server("the hold", e))?,
            Err(_) => return Err(stalled("the hold")),
        };
        let principal = audit::Principal::Person {
            person_id: &asker.person_id,
            method: PersonMethod::Session,
        };
        let stands = match tokio::time::timeout_at(
            deadline,
            session_stands(&mut hold, asker.session_id, &asker.person_id),
        )
        .await
        {
            Ok(stands) => stands.map_err(|e| server("the session", e))?,
            Err(_) => return Err(stalled("the session's read")),
        };
        let verbs = if stands {
            let granted = async {
                #[cfg(test)]
                self.pause_at(AskPoint::AtGrants).await;
                verbs_granted_in(&mut hold, &asker.person_id, agent).await
            };
            match tokio::time::timeout_at(deadline, granted).await {
                Ok(verbs) => verbs.map_err(|e| server("the grants", e))?,
                Err(_) => return Err(stalled("the grants' read")),
            }
        } else {
            None
        };
        let refusal = match &verbs {
            None => Some((VerbError::NoGrant, "no grant on the agent")),
            Some(verbs) if !verbs.contains(verb) => Some((
                VerbError::NotGranted {
                    verb: verb.to_owned(),
                },
                "the roles held on the agent do not carry the verb",
            )),
            Some(_) => {
                *authorized = true;
                self.connection_refusal(agent, plane, &reserved, act.ceilinged())
            }
        };
        if let Some((answer, why)) = refusal {
            #[cfg(test)]
            self.pause_at(AskPoint::AtRefusal).await;
            let recorded = tokio::time::timeout_at(
                deadline,
                self.store
                    .audit_refusal(principal, Target::Agent(agent.as_str()), verb, why),
            )
            .await;
            return match recorded {
                Ok(Ok(_)) => Err(answer),
                Ok(Err(e)) => {
                    tracing::error!(
                        "{agent}: a person's refusal of {verb} could not be recorded: {e:#}"
                    );
                    Err(VerbError::Audit)
                }
                // **A refusal that cannot be recorded is the server's
                // failure**, answered as the bound passed.
                Err(_) => Err(stalled("the refusal's record")),
            };
        }
        let mut unsent = match tokio::time::timeout_at(
            deadline,
            self.record_ask(id, agent, verb, principal),
        )
        .await
        {
            Ok(Ok(unsent)) => unsent,
            Ok(Err(e)) => {
                tracing::error!(
                    "{agent}: the first record of a person's {verb} could not be written, nothing asked: {e:#}"
                );
                return Err(VerbError::Audit);
            }
            // The record step's hand-over finds no one to take its guard,
            // which answers the first `failed` where it landed.
            Err(_) => return Err(stalled("the first record")),
        };
        #[cfg(test)]
        self.pause_at(AskPoint::UnderHold).await;
        let Slot::Reserved(Reserved {
            incarnation,
            permit,
            _place,
        }) = reserved
        else {
            return Err(VerbError::Lost);
        };
        let first = unsent.first.clone().unwrap_or_default();
        let (reply_tx, reply_rx) = oneshot::channel();
        // **Inserted where the ceiling was read**, the same incarnation
        // under the live map's lock: a teardown since leaves nothing to
        // send on, and the first is answered `failed`.
        let found = {
            let live = self.live.lock().unwrap();
            live.get(&(agent.clone(), plane))
                .filter(|c| c.ready && c.incarnation == incarnation)
                .map(|conn| {
                    conn.pending.lock().unwrap().insert(id, reply_tx);
                    conn.asks.lock().unwrap().insert(
                        id,
                        Asked {
                            verb: verb.to_owned(),
                            first: Some(first.clone()),
                            principal: asker.person_id.clone(),
                        },
                    );
                    (conn.pending.clone(), conn.asks.clone())
                })
        };
        let Some((pending, asks)) = found else {
            return Err(VerbError::Lost);
        };
        unsent.pending = Some(pending.clone());
        unsent.asks = Some(asks);
        // **The enqueue, through the slot reserved before the hold**: no
        // wait, so nothing of the peer runs under the hold.
        permit.send(act.frame(id, &asker.person_id));
        unsent.disarm();
        // **The commit is bounded too**: past the deadline the transaction
        // is dropped, its lock ending with it, and the ask stands enqueued.
        match tokio::time::timeout_at(deadline, hold.commit()).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(
                "{agent}: the hold of a person's {verb} did not commit after its enqueue, its lock ending with its transaction: {e:#}"
            ),
            Err(_) => tracing::warn!(
                "{agent}: the hold of a person's {verb} did not commit within its bound after its enqueue; dropped, its lock ending with it"
            ),
        }
        Ok(Enqueued {
            first,
            verb: verb.to_owned(),
            reply: reply_rx,
            _unask: Unask(pending, id),
        })
    }

    /// **A slot on the agent's admin connection**, ready, reserved before
    /// any hold and held to the silence bound; a bound that passes is the
    /// peer gone silent and closes it, as an enqueue's does. `None` where
    /// there is no slot to be had, which decides nothing by itself.
    async fn reserve(&self, agent: &AgentId, plane: Plane) -> Slot {
        let found = {
            let live = self.live.lock().unwrap();
            live.get(&(agent.clone(), plane))
                .filter(|c| c.ready)
                .map(|conn| {
                    (
                        conn.incarnation,
                        conn.tx.clone(),
                        conn.close.clone(),
                        conn.silent.clone(),
                        conn.person_slots.clone(),
                    )
                })
        };
        let Some((incarnation, tx, close, silent, places)) = found else {
            return Slot::None;
        };
        // **A place among the connection's persons' slots, never waited
        // for**: past the cap the ask takes no slot.
        let Ok(place) = places.try_acquire_owned() else {
            return Slot::Busy;
        };
        match tokio::time::timeout(self.silence, tx.reserve_owned()).await {
            Ok(Ok(permit)) => Slot::Reserved(Reserved {
                incarnation,
                permit,
                _place: place,
            }),
            Ok(Err(_)) => Slot::None,
            Err(_) => {
                tracing::warn!(
                    "{agent} ({plane}): a slot could not be reserved within the bound, the peer stopped reading"
                );
                silent.store(true, Ordering::Relaxed);
                let _ = close.send(true);
                Slot::None
            }
        }
    }

    /// **The connection's half of the check**, under the hold: the
    /// connection the slot was reserved on still live and ready, and its
    /// ceiling granting the verb.
    fn connection_refusal(
        &self,
        agent: &AgentId,
        plane: Plane,
        reserved: &Slot,
        ceilinged: Option<&str>,
    ) -> Option<(VerbError, &'static str)> {
        let not_connected = Some((
            VerbError::NotConnected,
            match plane {
                Plane::Admin => "admin-con is not connected",
                Plane::Gate => "gate-con is not connected",
            },
        ));
        let reserved = match reserved {
            Slot::Reserved(reserved) => reserved,
            Slot::Busy => {
                return Some((
                    VerbError::Busy,
                    "the connection's slots for persons' asks are all taken",
                ));
            }
            Slot::None => return not_connected,
        };
        let live = self.live.lock().unwrap();
        let Some(conn) = live
            .get(&(agent.clone(), plane))
            .filter(|c| c.ready && c.incarnation == reserved.incarnation)
        else {
            return not_connected;
        };
        // **A turn has no ceiling**: the gate's own admission is the box's.
        let verb = ceilinged?;
        let ceiling = conn.ceiling.as_deref().cloned().unwrap_or_default();
        if ceiling.contains(verb) {
            None
        } else {
            Some((
                VerbError::OutsideCeiling {
                    verb: verb.to_owned(),
                    ceiling: ceiling.into_iter().collect(),
                },
                "outside the connection's ceiling",
            ))
        }
    }

    /// A test's stop, where one was set for this point.
    #[cfg(test)]
    async fn pause_at(&self, point: AskPoint) {
        let pause = {
            let mut pauses = self.pauses.lock().unwrap();
            pauses
                .iter()
                .position(|(at, ..)| *at == point)
                .map(|at| pauses.remove(at))
        };
        if let Some((_, reached, release)) = pause {
            reached.notify_one();
            release.notified().await;
        }
    }

    /// The revoking act's close (Spec 8): a notification names a
    /// fingerprint, and the connection installed on it, if any, is closed
    /// now rather than at the next hello. **Resolved in the live map and
    /// not in the register**: a rotation has already replaced the row's
    /// fingerprints by the time its notification arrives, so a lookup in
    /// the register would find nothing to close and leave the old
    /// connections relaying.
    fn close_fingerprint(&self, fingerprint: &str) {
        let closes: Vec<(AgentId, Plane, watch::Sender<bool>)> = {
            let live = self.live.lock().unwrap();
            live.iter()
                .filter(|(_, c)| c.fingerprint == fingerprint)
                .map(|((agent, plane), c)| (agent.clone(), *plane, c.close.clone()))
                .collect()
        };
        for (agent, plane, close) in closes {
            tracing::info!(
                "credential {} of {agent} ({plane}) revoked: closing its live connection",
                &fingerprint[..12]
            );
            let _ = close.send(true);
        }
    }

    /// Retry each failed teardown's disconnected write; one that lands, or
    /// finds its incarnation superseded, leaves the list.
    async fn reconcile_teardowns(&self) {
        let pending: Vec<(AgentId, Plane, i64)> = self.failed_teardowns.lock().unwrap().clone();
        for (agent, plane, incarnation) in pending {
            match self.store.teardown(&agent, plane, incarnation, || {}).await {
                Ok(landed) => {
                    tracing::info!(
                        "{agent} ({plane}) incarnation {incarnation}: the deferred teardown {}",
                        if landed { "landed" } else { "was superseded" }
                    );
                    self.failed_teardowns
                        .lock()
                        .unwrap()
                        .retain(|(a, p, i)| !(*a == agent && *p == plane && *i == incarnation));
                }
                Err(e) => tracing::warn!(
                    "{agent} ({plane}) incarnation {incarnation}: the deferred teardown still fails: {e:#}"
                ),
            }
        }
    }

    /// Record a teardown the store refused. **One worker** retries the list
    /// every second while any entry stands and exits when it empties: it is
    /// started when the list goes from empty to non-empty, under the list's
    /// lock, so N dropped links during an outage give one worker and not N.
    fn defer_teardown(self: &Arc<Self>, agent: AgentId, plane: Plane, incarnation: i64) {
        let start = {
            let mut list = self.failed_teardowns.lock().unwrap();
            list.push((agent, plane, incarnation));
            !self.reconciling.swap(true, Ordering::AcqRel)
        };
        if !start {
            return;
        }
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let Some(inner) = weak.upgrade() else { return };
                inner.reconcile_teardowns().await;
                let list = inner.failed_teardowns.lock().unwrap();
                if list.is_empty() {
                    inner.reconciling.store(false, Ordering::Release);
                    return;
                }
            }
        });
    }

    /// Close every live connection whose credential is no longer the live
    /// one on its row, whether revoked or replaced by a rotation.
    async fn sweep_revoked(&self) {
        let held: Vec<(AgentId, Plane, String)> = {
            let live = self.live.lock().unwrap();
            live.iter()
                .map(|((agent, plane), c)| (agent.clone(), *plane, c.fingerprint.clone()))
                .collect()
        };
        for (agent, plane, fp) in held {
            let standing = match self.store.agent(&agent).await {
                Ok(Some(row)) => {
                    let credential = row.credential(plane);
                    credential.fingerprint == fp && credential.state == CredentialState::Live
                }
                Ok(None) => false,
                Err(e) => {
                    tracing::error!("sweep could not read {agent}: {e:#}");
                    continue;
                }
            };
            if !standing {
                self.close_fingerprint(&fp);
            }
        }
    }

    /// **Admission is coupled to ownership of the lock.** Between one of
    /// the monitor's pings and the next, a lock session PostgreSQL
    /// terminated would let a second listener take the key while this one
    /// still admits; so every admission pings the lock's connection, under
    /// the per-credential exclusion and before the install, and a failed
    /// ping refuses the admission and halts the listener as the monitor
    /// would. The window that remains, a ping that succeeded and a session
    /// lost before the install, is one round trip, the same bound the
    /// verbs accept, and the replacement listener's startup reset is the
    /// designed recovery for it. Admissions are rare, so the round trip
    /// costs nothing that matters.
    async fn prove_lock(&self) -> Result<(), String> {
        let (reply, answer) = oneshot::channel();
        if self.prove.send(reply).await.is_err() {
            return Err("the lock's monitor is gone".to_owned());
        }
        // The monitor's ping is bounded by the cadence; one more for the
        // ask to reach it behind a ping already in flight.
        match tokio::time::timeout(self.cadence * 2, answer).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("the lock's monitor is gone".to_owned()),
            Err(_) => Err("the lock's monitor did not answer within two cadences".to_owned()),
        }
    }

    /// Stop everything: no more accepts, every live connection closed,
    /// and `halted` set with why. Nothing is reacquired.
    async fn halt(&self, why: String) {
        tracing::error!("the listener halts: {why}");
        self.quiesce().await;
        let _ = self.halted.send(Some(why));
    }

    /// Stop accepting and notifying, tell every connection task to end
    /// (the admitted ones tear down and write their disconnected state,
    /// the handshaking ones return), wait for them, and abort whatever is
    /// still running after a grace period, so the lock's session ends only
    /// once no task of this listener can admit anything.
    async fn quiesce(&self) {
        // The accept and notification tasks are aborted and **joined**
        // before the set is taken: an accept that had just completed
        // could otherwise spawn into the replacement set after this one
        // was drained, and the lock would be released with it untracked.
        let tasks: Vec<_> = self.tasks.lock().unwrap().drain(..).collect();
        for task in tasks {
            task.abort();
            let _ = task.await;
        }
        let _ = self.halting.send(true);
        let closes: Vec<watch::Sender<bool>> = self
            .live
            .lock()
            .unwrap()
            .values()
            .map(|c| c.close.clone())
            .collect();
        for close in closes {
            let _ = close.send(true);
        }
        let mut set = {
            let mut guard = self.connections.lock().unwrap();
            self.closed.store(true, Ordering::Release);
            std::mem::take(&mut *guard)
        };
        let graceful = tokio::time::timeout(Duration::from_secs(5), async {
            while set.join_next().await.is_some() {}
        })
        .await;
        if graceful.is_err() {
            tracing::warn!("connection tasks still running after the grace period are aborted");
            set.abort_all();
            while set.join_next().await.is_some() {}
        }
    }

    fn next_arrival(&self) -> i64 {
        self.arrival.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn acknowledged(&self, agent: &AgentId) -> Option<Position> {
        self.acknowledged.lock().unwrap().get(agent).cloned()
    }

    /// Remove the connection from the live map if it is still the one
    /// installed for its plane. Answers whether it was.
    fn remove_if_mine(&self, key: &(AgentId, Plane), incarnation: i64) -> bool {
        let mut live = self.live.lock().unwrap();
        if live.get(key).map(|c| c.incarnation) == Some(incarnation) {
            live.remove(key);
            true
        } else {
            false
        }
    }

    /// The cleanup after a store error on a path that already installed:
    /// under the row's lock where the store can be reached, and without it,
    /// logged, where it cannot (the module header's one exception).
    /// `on_removed` runs where the entry comes out, under the lock where
    /// it is held.
    async fn remove_after_error(
        &self,
        key: &(AgentId, Plane),
        incarnation: i64,
        on_removed: &(dyn Fn() + Sync),
    ) -> bool {
        let removed = std::sync::atomic::AtomicBool::new(false);
        let locked = self
            .store
            .with_row_lock(&key.0, || {
                let mine = self.remove_if_mine(key, incarnation);
                removed.store(mine, Ordering::Relaxed);
                if mine {
                    on_removed();
                }
            })
            .await;
        if let Err(e) = locked {
            tracing::error!(
                "{} ({}): the row's lock could not be taken for the cleanup, removing the connection without it: {e:#}",
                key.0,
                key.1
            );
            let mine = self.remove_if_mine(key, incarnation);
            removed.store(mine, Ordering::Relaxed);
            if mine {
                on_removed();
            }
        }
        removed.load(Ordering::Relaxed)
    }
}

/// A best-effort send for a refusal or an answer on a connection that is
/// about to close: bounded, so a peer that stopped reading cannot hold the
/// task on it.
async fn send(tx: &mpsc::Sender<ToClient>, frame: ToClient) {
    let _ = tokio::time::timeout(Duration::from_secs(5), tx.send(frame)).await;
}

/// Why a connection closes when its `show`'s first record could not be
/// written: the audit's, named so.
const AUDIT_UNWRITTEN: &str = "the audit could not record the server's show";

/// **The server's own `show`, audited** (Spec 2.13): its first record,
/// principal and method `server`, committed through the record step before
/// the frame enters the channel, and the ask's id kept against it in the
/// connection's map. A record that cannot be written sends no frame, and
/// the connection closes with the audit's reason, refused as the store
/// unavailable so admin-con reconnects and the next admission asks again;
/// a frame that cannot be enqueued has its first answered `failed`, by the
/// `Unsent` guard.
async fn ask_show(
    inner: &Inner,
    agent: &AgentId,
    tx: &mpsc::Sender<ToClient>,
    asks: &Asks,
    close: &mut watch::Receiver<bool>,
) -> Result<u64, &'static str> {
    let id = inner.next_ask.fetch_add(1, Ordering::Relaxed);
    let mut unsent = match bounded(
        inner.record_ask(id, agent, "show", audit::Principal::Server),
        inner.silence,
        close,
    )
    .await
    {
        Bounded::Done(Ok(unsent)) => unsent,
        Bounded::Done(Err(e)) => {
            tracing::error!("{agent}: the server's show was not asked: its first record: {e:#}");
            send(
                tx,
                ToClient::Refusal {
                    reason: Refusal::StoreUnavailable,
                },
            )
            .await;
            return Err(AUDIT_UNWRITTEN);
        }
        // The record step holds its own bound; this one is the select's.
        Bounded::TimedOut => {
            tracing::error!("{agent}: the server's show was not asked: its first record stalled");
            send(
                tx,
                ToClient::Refusal {
                    reason: Refusal::StoreUnavailable,
                },
            )
            .await;
            return Err(AUDIT_UNWRITTEN);
        }
        Bounded::Closed => return Err("closed by revocation, rotation or halt"),
    };
    let first = unsent
        .first
        .clone()
        .expect("the record step names its first");
    asks.lock().unwrap().insert(
        id,
        Asked {
            verb: "show".into(),
            first: Some(first),
            principal: "server".to_owned(),
        },
    );
    unsent.asks = Some(asks.clone());
    let frame = ToClient::Verb {
        id,
        verb: "show".into(),
        principal: Principal::Server,
    };
    enqueue(tx, frame, inner.silence, close).await?;
    unsent.disarm();
    Ok(id)
}

/// **A verb's answer, classified once**: admin's outcome, or a fault,
/// admin-con's or the invocation's. A frame outside the contract's shape
/// is no answer and has no classification.
enum Answer {
    Outcome(VerbOutcome),
    Fault(VerbFault),
}

impl Answer {
    /// **The door**: a verb frame checked against the contract and the ask
    /// it answers, the verb asked and the connection's agent, or why it is
    /// outside them. Exactly one of an outcome and a fault. An outcome
    /// names the verb asked and the connection's agent, and has the shape
    /// the box's invocation answers (`weaver-admin-operator-contract`,
    /// built by `link::sudo_invoker`): exit 0 or 1 and one answer object,
    /// with no raw output beside it, which is kept only where no object
    /// parsed. A fault's kind stays free on the wire; what it means is the
    /// audit's allow-list.
    fn of(
        outcome: Option<VerbOutcome>,
        error: Option<VerbFault>,
        asked: &str,
        agent: &str,
    ) -> Result<Self, &'static str> {
        match (outcome, error) {
            (Some(outcome), None) => {
                if outcome.verb != asked {
                    Err("its outcome names another verb than the one asked")
                } else if outcome.agent != agent {
                    Err("its outcome names another agent than the connection's")
                } else if !matches!(outcome.exit_code, Some(0 | 1)) {
                    Err("its exit status is neither an answer's nor a refusal's")
                } else if !outcome.answer.as_ref().is_some_and(|a| a.is_object()) {
                    Err("its outcome carries no answer object")
                } else if outcome.raw_stdout.is_some() {
                    Err("its outcome carries raw output beside its answer object")
                } else {
                    Ok(Answer::Outcome(outcome))
                }
            }
            (None, Some(fault)) => Ok(Answer::Fault(fault)),
            _ => Err("it carries both an outcome and a fault, or neither"),
        }
    }

    /// **What the answer says of the act**, for its outcome record, and
    /// **only the contract's evidence writes one**, in every field the
    /// audit reads from the peer's frame: `weaver-admin-operator-contract`'s
    /// exit status, 0 a `lifecycle-answer` and so `ok`, 1 a
    /// `lifecycle-refusal` and so `failed`; any other status, or none, is a
    /// fault by the contract and writes no outcome, a shape the door
    /// refuses and kept here as defence. A fault writes `failed` only
    /// where its kind proves the verb never ran, by an allow-list: `busy`,
    /// `not_started`, `outside_ceiling`. **Every other kind, known or not,
    /// writes no outcome** (`None`): `unknown` and `fault`, which leave the
    /// act unknown, `not_landed`, which is this server's own, and any kind
    /// a connector of another version invents. The first then stands
    /// alone, which is the audit's "unknown", as where the connection ends
    /// with the frame in flight.
    fn audited(&self) -> Option<bool> {
        const NEVER_RAN: [&str; 3] = [
            VerbFault::BUSY,
            VerbFault::NOT_STARTED,
            VerbFault::OUTSIDE_CEILING,
        ];
        match self {
            Answer::Outcome(outcome) => match outcome.exit_code {
                Some(0) => Some(true),
                Some(1) => Some(false),
                _ => None,
            },
            Answer::Fault(fault) if NEVER_RAN.contains(&fault.kind.as_str()) => Some(false),
            Answer::Fault(_) => None,
        }
    }

    /// The asker's answer, the one classification as a frame.
    fn into_frame(self, id: u64) -> FromClient {
        match self {
            Answer::Outcome(outcome) => FromClient::Verb {
                id,
                outcome: Some(outcome),
                error: None,
            },
            Answer::Fault(fault) => FromClient::Verb {
                id,
                outcome: None,
                error: Some(fault),
            },
        }
    }
}

/// **A turn's answer, classified once**: the gate's close, or gate-con's
/// fault. A frame outside the contract's shape is no answer.
enum TurnAnswer {
    Close(GateClose),
    Fault(TurnFault),
}

impl TurnAnswer {
    /// **The door**: a turn frame checked against the ask it answers, which
    /// must be a turn, and the contract's shape, exactly one of a close and
    /// a fault. A close's kind and a fault's stay free on the wire; what
    /// they mean is `audited`'s allow-lists.
    fn of(
        close: Option<GateClose>,
        error: Option<TurnFault>,
        asked: &str,
    ) -> Result<Self, &'static str> {
        if asked != "turn" {
            return Err("it answers an ask that is no turn");
        }
        match (close, error) {
            (Some(close), None) => Ok(TurnAnswer::Close(close)),
            (None, Some(fault)) => Ok(TurnAnswer::Fault(fault)),
            _ => Err("it carries both a close and a fault, or neither"),
        }
    }

    /// **What the answer says of the turn**, for its outcome record, and
    /// **only the contract's evidence writes one**, by allow-lists:
    /// `weaver-gate-world-contract`'s close kind decides a close, `answered`
    /// `ok`, and `stopped` or `refused` `failed`, the gate's own word that
    /// the turn ended without its answer. A fault's kind decides a fault:
    /// `failed` where it proves the turn never reached the gate (`busy` and
    /// `not_started`, gate-con's own; `unloaded`, no gate to reach;
    /// `line_too_long`, never sent). **Everything else writes no outcome**:
    /// `delivery_lost`, `bad_close`, `close_too_long`, which cannot say
    /// whether the agent answered, any kind on no list, and, at the
    /// connection, a link that ends with the turn in flight.
    fn audited(&self) -> Option<bool> {
        const ENDED_UNANSWERED: [&str; 2] = ["stopped", "refused"];
        const NEVER_REACHED: [&str; 4] = [
            "busy",
            crate::link::gate_con::NOT_STARTED,
            "unloaded",
            "line_too_long",
        ];
        match self {
            TurnAnswer::Close(close) if close.kind == "answered" => Some(true),
            TurnAnswer::Close(close) if ENDED_UNANSWERED.contains(&close.kind.as_str()) => {
                Some(false)
            }
            TurnAnswer::Close(_) => None,
            TurnAnswer::Fault(fault) if NEVER_REACHED.contains(&fault.kind.as_str()) => Some(false),
            TurnAnswer::Fault(_) => None,
        }
    }

    /// The asker's answer, the one classification as a frame.
    fn into_frame(self, id: u64) -> FromClient {
        match self {
            TurnAnswer::Close(close) => FromClient::Turn {
                id,
                close: Some(close),
                error: None,
            },
            TurnAnswer::Fault(fault) => FromClient::Turn {
                id,
                close: None,
                error: Some(fault),
            },
        }
    }
}

/// **The teardown reasons that are the server's own act**, which raise no
/// lost contact (Spec 2.14): a revocation, a rotation or a retirement
/// closing the credential's connection, the listener's halt, and a row that
/// names another connection, a rotation landed before its close. A
/// duplicate's refusal never admits, so it has no teardown to raise from.
/// Every other reason, the connector dropping, a silent peer, a malformed
/// frame, a store unavailable, an admission incomplete, is lost contact.
const SERVERS_OWN_CLOSES: [&str; 2] = [
    "closed by revocation, rotation or halt",
    "the row names another connection",
];

/// **What an alert says of a failure**, before it is raised.
struct Raised {
    kind: AlertKind,
    summary: String,
    detail: String,
    reason: serde_json::Value,
    options: Vec<String>,
    /// `failed`, `unknown`, `refused` or `lost_contact`, for the log line.
    outcome: &'static str,
}

/// A failure on the agent's plane, as the alerts' queue takes it.
fn failure_of(
    agent: &AgentId,
    plane: Plane,
    subject: &str,
    first: Option<String>,
    principal: String,
    raised: Raised,
) -> Failure {
    Failure {
        raise: Raise {
            agent: agent.clone(),
            kind: raised.kind,
            subject: subject.to_owned(),
            summary: raised.summary,
            detail: raised.detail,
            first,
            options: raised.options,
            source: "server",
            once: false,
        },
        plane: plane.as_str(),
        principal,
        outcome: raised.outcome,
        reason: raised.reason,
    }
}

/// **The sources an entry may name, by plane** (Spec 2.14): the box's
/// `admin.log` and admin-con's own failures on the admin plane, gate-con's
/// own on the gate plane.
const ADMIN_ENTRY_SOURCES: [&str; 2] = [crate::link::admin_log::SOURCE, "admin-con"];
const GATE_ENTRY_SOURCES: [&str; 1] = ["gate-con"];

/// **The kinds of admin's answer** (`weaver-types` `LifecycleAnswer`, at
/// `toddwbucy/WeaverAgent` `ba3eb4e`), disjoint from the refusal's by the
/// floor's own rule: a box line whose outcome is one of these is a success
/// and the log's alone, and any other is a refusal.
const ANSWER_KINDS: [&str; 14] = [
    "ready",
    "left",
    "save_point_taken",
    "restore_named",
    "turn_aborted",
    "at_rest",
    "admitted",
    "released",
    "gate_ready",
    "gate_stopped",
    "gate_quiesced",
    "validated",
    "in_transition",
    "state",
];

/// What an entry is, as the door checked it.
#[derive(Debug)]
enum EntryShape {
    /// A line of `admin.log`, as admin wrote it, and whether admin-con found
    /// it its own.
    Line { own: bool },
    /// A discontinuity of the file, or a line past the bound.
    Marked,
    /// A connector's own failure: its kind, the alert's subject.
    Failure { kind: String, message: String },
}

/// **An entry the door checked** (Spec 2.14).
#[derive(Debug)]
struct CheckedEntry {
    source: &'static str,
    shape: EntryShape,
    entry: serde_json::Value,
}

impl CheckedEntry {
    /// The door's one check: the run drawn as a connector draws it, a
    /// sequence from one, the source on the plane's allow-list, an object
    /// within the bound, and the source's own shape.
    fn of(
        plane: Plane,
        run: &str,
        seq: u64,
        source: &str,
        entry: serde_json::Value,
    ) -> Result<Self, &'static str> {
        if run.len() != 16 || !run.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
            return Err("a run not drawn as a connector draws one");
        }
        if seq == 0 || i64::try_from(seq).is_err() {
            return Err("a sequence outside its range");
        }
        let allowed: &[&str] = match plane {
            Plane::Admin => &ADMIN_ENTRY_SOURCES,
            Plane::Gate => &GATE_ENTRY_SOURCES,
        };
        let Some(source) = allowed.iter().copied().find(|s| *s == source) else {
            return Err("a source not on the plane's allow-list");
        };
        let Some(object) = entry.as_object() else {
            return Err("an entry that is not an object");
        };
        if serde_json::to_string(&entry).map_or(usize::MAX, |s| s.len())
            > crate::link::frames::ENTRY_BOUND
        {
            return Err("an entry past the bound");
        }
        let text = |member: &str| object.get(member).and_then(|v| v.as_str());
        let shape = if source == crate::link::admin_log::SOURCE {
            match (
                object.get("line"),
                object.get("own").and_then(|o| o.as_bool()),
                text("discontinuity"),
                text("oversized"),
            ) {
                (Some(line), Some(own), None, None) if line.is_object() || line.is_string() => {
                    EntryShape::Line { own }
                }
                (None, _, Some(_), None) | (None, _, None, Some(_)) => EntryShape::Marked,
                _ => return Err("a line of admin.log outside its shape"),
            }
        } else {
            match (text("kind"), text("message")) {
                (Some(kind), Some(message))
                    if !kind.is_empty()
                        && kind.len() <= 64
                        && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') =>
                {
                    EntryShape::Failure {
                        kind: kind.to_owned(),
                        message: message.to_owned(),
                    }
                }
                _ => return Err("a connector's failure outside its shape"),
            }
        };
        Ok(Self {
            source,
            shape,
            entry,
        })
    }

    /// **The entry as the operations log takes it, with the alert it
    /// raises** (Spec 2.14): every entry is a line; an alert only for a
    /// line of `admin.log` that is not admin-con's own, an act on the box
    /// that did not come through WeaverWeb, classified by the doors'
    /// allow-lists, and for a connector's own failure. **Never twice**: an
    /// act asked through WeaverWeb raised its alert at the door, and its
    /// own line raises nothing here.
    fn into_entry(self, agent: &AgentId, plane: Plane) -> crate::alerting::Entry {
        let raise = |kind, subject: &str, summary: String, detail: String, options, source| Raise {
            agent: agent.clone(),
            kind,
            subject: subject.to_owned(),
            summary,
            detail,
            first: None,
            options,
            source,
            once: false,
        };
        let (own, raise) = match &self.shape {
            EntryShape::Line { own } => (
                Some(*own),
                if *own {
                    None
                } else {
                    self.entry
                        .get("line")
                        .and_then(box_line_raised)
                        .map(|(subject, raised)| {
                            raise(
                                raised.kind,
                                &subject,
                                format!("on the box, not through WeaverWeb: {}", raised.summary),
                                raised.detail,
                                raised.options,
                                "admin",
                            )
                        })
                },
            ),
            EntryShape::Marked => (None, None),
            EntryShape::Failure { kind, message } => (
                None,
                Some(raise(
                    AlertKind::Failed,
                    kind,
                    format!("{}: {message}", self.source),
                    String::new(),
                    Vec::new(),
                    self.source,
                )),
            ),
        };
        crate::alerting::Entry {
            agent: agent.clone(),
            plane: plane.as_str(),
            source: self.source.to_owned(),
            own,
            entry: self.entry,
            raise,
        }
    }
}

/// **What a line of `admin.log` that is not admin-con's own raises**, by
/// allow-list (Spec 2.14): an invocation's refusal, by the doors' own rules
/// (the save-point prompt where an `unload` stopped at its save point with
/// the run at rest); a load rolled back or a verb refused before its
/// answer; and the relay refusing a peer, another party at the trace's
/// door. An answer, any other line, and a line of no known shape raise
/// nothing. The subject and what is raised.
fn box_line_raised(line: &serde_json::Value) -> Option<(String, Raised)> {
    let text = |member: &str| line.get(member).and_then(|v| v.as_str());
    if text("actor") == Some("relay") {
        if text("event") != Some("refused") {
            return None;
        }
        let peer = line
            .get("peer_uid")
            .and_then(|u| u.as_u64())
            .map_or_else(|| "a peer".to_owned(), |u| format!("uid {u}"));
        return Some((
            "trace door".to_owned(),
            Raised {
                kind: AlertKind::Failed,
                summary: format!("the trace relay refused {peer} at the trace's door"),
                detail: cut(&line.to_string(), 2048),
                reason: serde_json::json!({ "line": line }),
                options: Vec::new(),
                outcome: "failed",
            },
        ));
    }
    let verb = text("verb")?;
    let outcome = text("outcome")?;
    match serde_json::from_str::<serde_json::Value>(outcome) {
        Ok(object) if object.is_object() => {
            let kind = object.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if ANSWER_KINDS.contains(&kind) {
                return None;
            }
            Some((verb.to_owned(), refusal_raised(verb, &object, "")))
        }
        _ if outcome.starts_with("refused") || outcome.starts_with("rolled back") => Some((
            verb.to_owned(),
            Raised {
                kind: AlertKind::Failed,
                summary: format!("{verb}: {}", cut(outcome, 512)),
                detail: cut(&line.to_string(), 2048),
                reason: serde_json::json!({ "line": line }),
                options: Vec::new(),
                outcome: "failed",
            },
        )),
        _ => None,
    }
}

/// **The legs of a save point an unload can miss with the run left standing
/// at rest, its gate lowered** (`weaver-admin-operator-contract` section 5,
/// S9, at `toddwbucy/WeaverAgent` `55fbc66`), by allow-list: a miss naming
/// any of these is the decision prompt's; `published` ends the run, and a
/// leg on no list is a plain failure.
const LEGS_AT_REST: [&str; 3] = ["answer", "finished", "member_dead"];

/// **The leg an `unload` stopped at its save point missed**, the run left
/// standing at rest (S9): admin's refusal `save_point_not_taken` to an
/// `unload`, naming a leg of [`LEGS_AT_REST`]. The door's prompt and the
/// verb page read it alike.
pub fn stopped_at_save_point<'a>(verb: &str, outcome: &'a VerbOutcome) -> Option<&'a str> {
    if outcome.exit_code != Some(1) {
        return None;
    }
    missed_at_rest(verb, outcome.answer.as_ref()?)
}

/// The leg of an `unload`'s refusal object that left the run at rest.
fn missed_at_rest<'a>(verb: &str, object: &'a serde_json::Value) -> Option<&'a str> {
    let leg = object.get("missed").and_then(|m| m.as_str())?;
    (verb == "unload"
        && object.get("kind").and_then(|k| k.as_str()) == Some("save_point_not_taken")
        && LEGS_AT_REST.contains(&leg))
    .then_some(leg)
}

/// **What admin's refusal of `verb` raises**, by allow-list (Spec 2.14):
/// the decision prompt where an `unload` stopped at its save point with the
/// run at rest, an explaining alert where a refusal could have lost data
/// (`save_point_not_taken` naming `published`, `boundary_unverified`), and
/// a failure otherwise, each carrying the refusal object and admin's
/// stderr, bounded. The door's and a box line's alike.
fn refusal_raised(verb: &str, object: &serde_json::Value, stderr: &str) -> Raised {
    let kind = object
        .get("kind")
        .and_then(|k| k.as_str())
        .unwrap_or("unnamed")
        .to_owned();
    let missed = object.get("missed").and_then(|m| m.as_str());
    let stderr = cut(stderr, 2048);
    let detail = if stderr.is_empty() {
        object.to_string()
    } else {
        format!("{object}\nstderr: {stderr}")
    };
    let reason = serde_json::json!({ "refusal": object, "stderr": stderr });
    let (alert, summary, options) = match (missed_at_rest(verb, object), kind.as_str(), missed) {
        (Some(leg), _, _) => (
            AlertKind::SavePointMissed,
            format!(
                "unload stopped at its save point (missed: {leg}): the run stands at rest, its gate lowered, and nothing is lost yet"
            ),
            vec![SAVE_POINT_THEN_UNLOAD.to_owned(), FORCE_UNLOAD.to_owned()],
        ),
        (None, "save_point_not_taken", Some("published")) => (
            AlertKind::DataAtRisk,
            format!(
                "{verb} refused: a save point was taken and did not publish; what it holds is not in the published save points"
            ),
            Vec::new(),
        ),
        (None, "boundary_unverified", _) => (
            AlertKind::DataAtRisk,
            format!(
                "{verb} refused: a save point's boundary could not be verified; it is cleared on the box"
            ),
            Vec::new(),
        ),
        _ => (
            AlertKind::Failed,
            format!("{verb} refused by admin: {kind}"),
            Vec::new(),
        ),
    };
    Raised {
        kind: alert,
        summary,
        detail,
        reason,
        options,
        outcome: "failed",
    }
}

/// The decision prompt's two options for an unload stopped at its save point.
pub const SAVE_POINT_THEN_UNLOAD: &str = "save_point_then_unload";
pub const FORCE_UNLOAD: &str = "force_unload";

impl Answer {
    /// **What admin's answer raises** (Spec 2.14): nothing for its answer
    /// (exit 0); for its refusal (exit 1), the decision prompt where an
    /// `unload` stopped at its save point with the run at rest, an
    /// explaining alert where a refusal could have lost data
    /// (`save_point_not_taken` naming `published`, `boundary_unverified`),
    /// and a failure otherwise, each carrying the refusal object and
    /// admin's stderr, bounded; for a fault, a failure or an unknown
    /// outcome by the door's own allow-list.
    fn raised(&self, verb: &str) -> Option<Raised> {
        match self {
            Answer::Outcome(outcome) if outcome.exit_code == Some(0) => None,
            Answer::Outcome(outcome) => {
                let object = outcome.answer.clone().unwrap_or(serde_json::Value::Null);
                Some(refusal_raised(
                    verb,
                    &object,
                    outcome.stderr.as_deref().unwrap_or(""),
                ))
            }
            Answer::Fault(fault) => {
                let reason = serde_json::json!({ "fault": fault.kind, "message": fault.message });
                Some(match self.audited() {
                    Some(_) => Raised {
                        kind: AlertKind::Failed,
                        summary: format!("{verb} was not run: {}", fault.kind),
                        detail: fault.message.clone(),
                        reason,
                        options: Vec::new(),
                        outcome: "failed",
                    },
                    None => Raised {
                        kind: AlertKind::Unknown,
                        summary: format!("{verb}: whether admin acted is unknown ({})", fault.kind),
                        detail: fault.message.clone(),
                        reason,
                        options: Vec::new(),
                        outcome: "unknown",
                    },
                })
            }
        }
    }
}

impl TurnAnswer {
    /// **What the gate's answer raises** (Spec 2.14): nothing for an
    /// `answered` close; a failure for a `stopped` or `refused` close, with
    /// the agent's reason, and for a fault proving the turn never reached
    /// the gate; an unknown outcome otherwise. Never the turn's text, which
    /// no answer carries.
    fn raised(&self) -> Option<Raised> {
        match self {
            TurnAnswer::Close(close) if close.kind == "answered" => None,
            TurnAnswer::Close(close) => {
                let reason_text = close.reason.clone().unwrap_or_default();
                let reason = serde_json::json!({ "close": close.kind, "reason": reason_text });
                Some(match self.audited() {
                    Some(_) => Raised {
                        kind: AlertKind::Failed,
                        summary: format!("the turn was {}: {reason_text}", close.kind),
                        detail: reason_text,
                        reason,
                        options: Vec::new(),
                        outcome: "failed",
                    },
                    None => Raised {
                        kind: AlertKind::Unknown,
                        summary: format!(
                            "the turn closed as {}, which says nothing of it",
                            close.kind
                        ),
                        detail: reason_text,
                        reason,
                        options: Vec::new(),
                        outcome: "unknown",
                    },
                })
            }
            TurnAnswer::Fault(fault) => {
                let reason = serde_json::json!({ "fault": fault.kind, "message": fault.message });
                Some(match self.audited() {
                    Some(_) => Raised {
                        kind: AlertKind::Failed,
                        summary: format!("the turn was not placed: {}", fault.kind),
                        detail: fault.message.clone(),
                        reason,
                        options: Vec::new(),
                        outcome: "failed",
                    },
                    None => Raised {
                        kind: AlertKind::Unknown,
                        summary: format!(
                            "the turn: whether the agent answered is unknown ({})",
                            fault.kind
                        ),
                        detail: fault.message.clone(),
                        reason,
                        options: Vec::new(),
                        outcome: "unknown",
                    },
                })
            }
        }
    }
}

/// **A send to a peer that stopped reading is the peer gone silent.** The
/// write channel behind the socket is bounded, so a peer that stops reading
/// fills it and a plain send would suspend the connection task where the
/// silence timeout, which wraps only the read, never fires. Every enqueue
/// is held to the silence bound and watches the close signal, and one that
/// cannot complete answers the reason the connection ends for.
async fn enqueue(
    tx: &mpsc::Sender<ToClient>,
    frame: ToClient,
    bound: Duration,
    close: &mut watch::Receiver<bool>,
) -> Result<(), &'static str> {
    tokio::select! {
        sent = tokio::time::timeout(bound, tx.send(frame)) => match sent {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err("the write path is gone"),
            Err(_) => Err("silent for the bound: the peer stopped reading"),
        },
        _ = close.changed() => Err("closed by revocation, rotation or halt"),
    }
}

/// How a bounded store landing ended.
enum Bounded<T> {
    Done(T),
    TimedOut,
    Closed,
}

/// **A store landing is awaited against the bound and the close signal,
/// as reads and enqueues are.** A stalled store would otherwise hold the
/// task past the silence timer and the close signal, so a revocation could
/// not finish closing the connection until the store answered. A landing
/// that does not complete within the bound is treated as not landed: the
/// connection closes as `store_unavailable` with no ack, and the replay
/// resends. The landing's future is dropped where it is cut off, so its
/// write may or may not have reached the row; the resend or the next
/// admission's show settles it.
async fn bounded<T>(
    landing: impl Future<Output = T>,
    bound: Duration,
    close: &mut watch::Receiver<bool>,
) -> Bounded<T> {
    bounded_at(landing, tokio::time::Instant::now() + bound, close).await
}

/// `bounded` to an instant rather than a span: **a deadline kept as an
/// instant is read at the await it bounds**, and one already past fires
/// at the first poll, where a remainder of zero computed earlier would
/// still wait for the timer's next tick.
async fn bounded_at<T>(
    landing: impl Future<Output = T>,
    deadline: tokio::time::Instant,
    close: &mut watch::Receiver<bool>,
) -> Bounded<T> {
    tokio::select! {
        done = tokio::time::timeout_at(deadline, landing) => match done {
            Ok(value) => Bounded::Done(value),
            Err(_) => Bounded::TimedOut,
        },
        _ = close.changed() => Bounded::Closed,
    }
}

/// Wait for the writer to drain what it holds, bounded: a peer that stopped
/// reading would otherwise hold the task on the socket's send buffer.
async fn drain_writer(mut writer: tokio::task::JoinHandle<()>) {
    if tokio::time::timeout(Duration::from_secs(5), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
    }
}

/// Refuse, close the write path, and wait for it to drain.
async fn refuse(tx: mpsc::Sender<ToClient>, writer: tokio::task::JoinHandle<()>, reason: Refusal) {
    send(&tx, ToClient::Refusal { reason }).await;
    drop(tx);
    drain_writer(writer).await;
}

async fn serve_connection(inner: Arc<Inner>, stream: TcpStream, peer: SocketAddr) {
    let mut halting = inner.halting.subscribe();
    if *halting.borrow() {
        return;
    }
    // **The handshake is the only door**: a peer that does not complete
    // mutual TLS with a certificate this authority signed is dropped here,
    // and nothing it wrote was read as a frame.
    let handshake = tokio::select! {
        h = tokio::time::timeout(Duration::from_secs(HANDSHAKE_SECS), inner.acceptor.accept(stream)) => h,
        _ = halting.changed() => {
            tracing::info!("link from {peer}: the listener is halting, the handshake is dropped");
            return;
        }
    };
    let tls = match handshake {
        Ok(Ok(tls)) => tls,
        Ok(Err(e)) => {
            tracing::warn!("link handshake from {peer} failed, refused in the clear: {e}");
            return;
        }
        Err(_) => {
            tracing::warn!("link handshake from {peer} did not complete, refused");
            return;
        }
    };
    let Some(fp) = tls
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| certs.first())
        .map(|c| fingerprint(c.as_ref()))
    else {
        tracing::warn!("link from {peer} completed the handshake with no client certificate");
        return;
    };

    let (read_half, mut write_half) = tokio::io::split(tls);
    let (tx, mut rx) = mpsc::channel::<ToClient>(64);
    let (close_tx, mut close_rx) = watch::channel(false);
    let silent = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let heard = Arc::new(Mutex::new(std::time::Instant::now()));
    // **A write that cannot complete within the silence bound is the peer
    // gone silent**, whatever fills the channel: persons' asks hold few
    // slots and send through permits that never wait, so a peer that
    // heartbeats and never reads is found here, at the socket, and the
    // connection closes as silent.
    let (write_bound, writer_close, writer_silent) =
        (inner.silence, close_tx.clone(), silent.clone());
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let Ok(mut line) = serde_json::to_string(&frame) else {
                continue;
            };
            line.push('\n');
            match tokio::time::timeout(write_bound, write_half.write_all(line.as_bytes())).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) => return,
                Err(_) => {
                    tracing::warn!(
                        "link from {peer}: a write did not complete within {write_bound:?}, the peer stopped reading"
                    );
                    writer_silent.store(true, Ordering::Relaxed);
                    let _ = writer_close.send(true);
                    return;
                }
            }
        }
        let _ = write_half.shutdown().await;
    });
    let mut reader = LineReader::new(read_half);

    // **The lookup comes before the roster** (Spec 8): the fingerprint is
    // looked up in the register and a credential that is absent or
    // revoked is refused here, before any byte of the hello is read.
    let (agent, plane) = match inner.store.agent_by_fingerprint(&fp).await {
        Ok(Some((agent, plane))) if agent.credential(plane).state == CredentialState::Live => {
            (agent, plane)
        }
        Ok(Some((agent, plane))) => {
            tracing::warn!(
                "link from {peer}: credential {} of {} ({plane}) is revoked, refused before its roster",
                &fp[..12],
                agent.agent_id
            );
            refuse(tx, writer, Refusal::NotLive).await;
            return;
        }
        Ok(None) => {
            tracing::warn!(
                "link from {peer}: credential {} is not in the register, refused before its roster",
                &fp[..12]
            );
            refuse(tx, writer, Refusal::NotLive).await;
            return;
        }
        Err(e) => {
            tracing::error!("link from {peer}: the register could not be read: {e:#}");
            drop(tx);
            drain_writer(writer).await;
            return;
        }
    };

    // The hello has the handshake's bound, not the silence bound: a peer
    // that authenticated and then says nothing is not a connector yet.
    let hello = tokio::select! {
        h = tokio::time::timeout(Duration::from_secs(HANDSHAKE_SECS), reader.next()) => h,
        _ = halting.changed() => {
            tracing::info!("link from {peer}: the listener is halting before the hello, dropped");
            drop(tx);
            drain_writer(writer).await;
            return;
        }
    };
    let (name, said_plane, tail, ceiling, door) = match hello {
        Ok(Line::Frame(line)) => match serde_json::from_str::<FromClient>(&line) {
            Ok(FromClient::Hello {
                agent,
                plane,
                tail,
                ceiling,
                door,
            }) => (agent, plane, tail, ceiling, door),
            _ => {
                tracing::warn!("link from {peer}: the first frame was not a hello, refused");
                refuse(tx, writer, Refusal::Malformed).await;
                return;
            }
        },
        Ok(Line::Malformed(why)) => {
            tracing::warn!("link from {peer}: {why} before the hello, refused");
            refuse(tx, writer, Refusal::Malformed).await;
            return;
        }
        _ => {
            drop(tx);
            drain_writer(writer).await;
            return;
        }
    };

    // **Identity is the certificate's binding** (Spec 8): the roster is a
    // check and never a source, and a hello naming another agent or plane
    // is refused as a mismatch and logged against the row.
    if name != agent.name || said_plane != plane {
        tracing::warn!(
            "link from {peer}: hello named {name} ({said_plane}) on a credential bound to {} ({plane}) of {}, refused as a mismatch",
            agent.name,
            agent.agent_id
        );
        refuse(tx, writer, Refusal::RosterMismatch).await;
        return;
    }
    // **The replay boundary is fixed from the hello where the trace door
    // is open** (Spec 7.2): an open door's hello carries the boundary
    // admin-con took at the opening, and **a closed door's carries none**,
    // its replay ended at once, `caught_up` taken as sent, so the server
    // admits it and serves verbs per the ceiling with their answers landing
    // at receipt. An admin hello whose door and boundary disagree, or that
    // names no door, is refused; a gate hello carries no door.
    let (mut boundary, door) = match (plane, door, tail) {
        (Plane::Admin, Some(true), Some(tail)) => (Some(tail), Some(true)),
        (Plane::Admin, Some(false), None) => (None, Some(false)),
        (Plane::Admin, door, tail) => {
            tracing::warn!(
                "link from {peer}: admin-con of {} said hello with door {door:?} and {} boundary, refused",
                agent.agent_id,
                if tail.is_some() { "a" } else { "no" }
            );
            refuse(tx, writer, Refusal::Malformed).await;
            return;
        }
        (Plane::Gate, None, _) => (None, None),
        (Plane::Gate, Some(_), _) => {
            tracing::warn!(
                "link from {peer}: gate-con of {} said hello with a trace door, refused",
                agent.agent_id
            );
            refuse(tx, writer, Refusal::Malformed).await;
            return;
        }
    };
    // **The ceiling is the admin plane's and is declared in its hello**
    // (Spec 8): an admin hello without one, a gate hello with one, or a
    // ceiling past its bound is refused as malformed.
    let ceiling: Option<Arc<BTreeSet<String>>> = match (plane, ceiling) {
        (Plane::Admin, Some(verbs))
            if verbs.len() <= CEILING_BOUND && verbs.iter().all(|v| v.len() <= VERB_NAME_BOUND) =>
        {
            Some(Arc::new(verbs.into_iter().collect()))
        }
        (Plane::Gate, None) => None,
        (plane, _) => {
            tracing::warn!(
                "link from {peer}: {} ({plane}) said hello with a ceiling the plane does not carry or past its bound, refused",
                agent.agent_id
            );
            refuse(tx, writer, Refusal::Malformed).await;
            return;
        }
    };
    let ceiling_copy: Option<Vec<String>> = ceiling.as_deref().map(|c| c.iter().cloned().collect());

    let incarnation = (inner.epoch << 32) | inner.next_incarnation.fetch_add(1, Ordering::Relaxed);
    let pending = Arc::new(Mutex::new(HashMap::new()));
    let asks: Asks = Arc::new(Mutex::new(HashMap::new()));
    let key = (agent.agent_id.clone(), plane);
    let admitted = inner
        .store
        .admit(
            &agent.agent_id,
            plane,
            &fp,
            incarnation,
            &peer.to_string(),
            ceiling_copy.as_deref(),
            door,
            // The monitor halts the listener on the failed ping; this
            // admission is refused.
            async || match inner.prove_lock().await {
                Ok(()) => Ok(()),
                Err(why) => {
                    tracing::error!(
                        "link from {peer}: admission refused, the listener's lock could not be proven ({why})"
                    );
                    Err(Refusal::StoreUnavailable)
                }
            },
            || {
                // **One live connection per credential** (Spec 8), decided
                // under the row's lock: a second connection is refused rather
                // than replacing the first.
                let mut live = inner.live.lock().unwrap();
                if live.contains_key(&key) {
                    return Err(Refusal::AlreadyConnected);
                }
                live.insert(
                    key.clone(),
                    LiveConnection {
                        fingerprint: fp.clone(),
                        incarnation,
                        tx: tx.clone(),
                        pending: pending.clone(),
                        close: close_tx.clone(),

                        silent: silent.clone(),
                        ready: false,
                        ceiling: ceiling.clone(),
                        asks: asks.clone(),
                        person_slots: Arc::new(tokio::sync::Semaphore::new(PERSON_SLOTS)),
                        heard: heard.clone(),
                    },
                );
                Ok(())
            },
        )
        .await;
    match admitted {
        Ok(Ok(())) => {}
        Ok(Err(refusal)) => {
            tracing::warn!(
                "link from {peer}: {} ({plane}) refused at admission: {refusal}",
                agent.agent_id
            );
            refuse(tx, writer, refusal).await;
            return;
        }
        Err(e) => {
            // The install closure may have run before the write failed:
            // the entry comes out under the lock, so the map agrees with
            // the row that rolled back.
            tracing::error!("link from {peer}: admission could not be written: {e:#}");
            // Never admitted, so no contact was had to lose.
            inner.remove_after_error(&key, incarnation, &|| {}).await;
            // The commit may have landed before its answer was lost, leaving
            // the row connected under this incarnation with no socket: an
            // incarnation-bound teardown is deferred to the reconciliation,
            // which writes disconnected where the row names this incarnation
            // and drops the entry where it does not.
            inner.defer_teardown(agent.agent_id.clone(), plane, incarnation);
            drop(tx);
            drain_writer(writer).await;
            return;
        }
    }
    tracing::info!(
        "link from {peer}: {} ({plane}) admitted as incarnation {incarnation}",
        agent.agent_id
    );
    inner.alerts.restored(&agent.agent_id, plane.as_str());

    // The hello is answered, then on the admin plane `show` is asked
    // before the connection's first event is read (Spec 7.2, 2.12).
    let acknowledged = inner.acknowledged(&agent.agent_id);
    // Why the connection ended, for the window's mark.
    let mut reason = "dropped by admin-con";
    'serve: {
        if let Err(why) = enqueue(
            &tx,
            ToClient::HelloAnswer {
                cadence_secs: inner.silence.as_secs() / 4,
                silence_secs: inner.silence.as_secs(),
                acknowledged,
            },
            inner.silence,
            &mut close_rx,
        )
        .await
        {
            reason = why;
            break 'serve;
        }
        // **The admission's `show` is the connection's initialisation.**
        // Until it answers with a usable observation the row's tuple and
        // load state are from before the reconnect; an admin-con that
        // errors, answers without a state, or only heartbeats would leave
        // the connection admitted on stale word. So the ask's id and a
        // deadline at the silence bound are held, and a connection whose
        // show has not landed by then, or answers with anything else, is
        // closed with a typed refusal so the reconnect asks again.
        let mut admission_show: Option<(u64, tokio::time::Instant)> = None;
        if plane == Plane::Admin {
            inner.windows.ensure(agent.agent_id.as_str());
            if inner.windows.has_events(agent.agent_id.as_str()) {
                inner.windows.mark(
                    agent.agent_id.as_str(),
                    "admin-con reconnected: a replay follows",
                );
            }
            // **Only where the ceiling grants `show`** (Spec 7.2, 8): where
            // it does not, nothing is asked, admission completes at
            // `caught_up`, and the row's state stands on live events.
            if ceiling.as_deref().is_some_and(|c| c.contains("show")) {
                match ask_show(&inner, &agent.agent_id, &tx, &asks, &mut close_rx).await {
                    Ok(id) => {
                        admission_show = Some((id, tokio::time::Instant::now() + inner.silence))
                    }
                    Err(why) => {
                        reason = why;
                        break 'serve;
                    }
                }
            }
        }
        // The hello's answer and, on the admin plane, the admission's show
        // are in the channel ahead of anything an ask could add: the
        // connection is ready for asks from here and not before, since a
        // verb ask entering the channel ahead of the show would hold the
        // show behind a slow verb and close the connection as incomplete.
        if let Some(conn) = inner.live.lock().unwrap().get_mut(&key)
            && conn.incarnation == incarnation
        {
            conn.ready = true;
        }

        // Whether admin-con's replay has reached the boundary (Spec 7.2): the
        // frame that decides what is replayed and what is live. **A closed
        // door's hello has no replay**, so it is taken as sent.
        let mut caught_up = door == Some(false);
        loop {
            // The read waits to the silence bound, or to the admission
            // show's deadline where that is sooner.
            let wait = match admission_show {
                Some((_, deadline)) => deadline
                    .saturating_duration_since(tokio::time::Instant::now())
                    .min(inner.silence),
                None => inner.silence,
            };
            let line = tokio::select! {
                l = tokio::time::timeout(wait, reader.next()) => l,
                _ = close_rx.changed() => {
                    if *close_rx.borrow() {
                        tracing::info!(
                            "link from {peer}: {} ({plane}) closed by revocation, rotation or halt",
                            agent.agent_id
                        );
                        reason = if silent.load(Ordering::Relaxed) {

                            "silent for the bound: the peer stopped reading"

                        } else {

                            "closed by revocation, rotation or halt"

                        };
                        break;
                    }
                    continue;
                }
            };
            let line = match line {
                Ok(Line::Frame(line)) => {
                    *heard.lock().unwrap() = std::time::Instant::now();
                    line
                }
                Ok(Line::Closed) => break,
                Ok(Line::Malformed(why)) => {
                    tracing::warn!(
                        "link from {peer}: {} ({plane}) sent {why}, refused",
                        agent.agent_id
                    );
                    send(
                        &tx,
                        ToClient::Refusal {
                            reason: Refusal::Malformed,
                        },
                    )
                    .await;
                    reason = "refused as malformed";
                    break;
                }
                Err(_)
                    if admission_show
                        .is_some_and(|(_, deadline)| tokio::time::Instant::now() >= deadline) =>
                {
                    tracing::warn!(
                        "link from {peer}: {} (admin) did not answer the admission's show within {:?}, closed so the reconnect asks again",
                        agent.agent_id,
                        inner.silence
                    );
                    send(
                        &tx,
                        ToClient::Refusal {
                            reason: Refusal::AdmissionIncomplete,
                        },
                    )
                    .await;
                    reason = "the admission's show did not answer";
                    break;
                }
                Err(_) => {
                    tracing::info!(
                        "link from {peer}: {} ({plane}) silent for {:?}, closed",
                        agent.agent_id,
                        inner.silence
                    );
                    send(
                        &tx,
                        ToClient::Refusal {
                            reason: Refusal::Silence,
                        },
                    )
                    .await;
                    reason = "silent for the bound";
                    break;
                }
            };
            let frame: FromClient = match serde_json::from_str(&line) {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!("link from {peer}: a frame did not parse, closed: {e}");
                    send(
                        &tx,
                        ToClient::Refusal {
                            reason: Refusal::Malformed,
                        },
                    )
                    .await;
                    reason = "refused as malformed";
                    break;
                }
            };
            match (plane, frame) {
                (_, FromClient::Heartbeat) => {}
                (Plane::Admin, FromClient::CaughtUp) => {
                    caught_up = true;
                }
                (_, FromClient::Hello { .. }) => {
                    tracing::warn!(
                        "link from {peer}: {} ({plane}) said hello again mid-stream, refused",
                        agent.agent_id
                    );
                    send(
                        &tx,
                        ToClient::Refusal {
                            reason: Refusal::Malformed,
                        },
                    )
                    .await;
                    reason = "refused as malformed";
                    break;
                }
                (Plane::Gate, FromClient::Turn { id, close, error }) => {
                    // **A turn's answer is checked once, here at the door,
                    // against the contract and the ask it answers** (Spec
                    // 2.13, 7.1), as a verb's is: an answer to no ask, or
                    // with both a close and a fault or neither, is the
                    // peer's protocol violation, refused as malformed, its
                    // first given no outcome and its asker no answer.
                    // **A refused answer leaves its ask in flight** (Spec
                    // 2.14): the entry goes back, so the teardown's drain
                    // raises the act's unknown like any act left in flight.
                    let asked = asks.lock().unwrap().remove(&id);
                    let checked = match asked {
                        None => Err("it answers no ask"),
                        Some(asked) => match TurnAnswer::of(close, error, &asked.verb) {
                            Ok(answer) => Ok((asked.first, asked.principal, answer)),
                            Err(why) => {
                                asks.lock().unwrap().insert(id, asked);
                                Err(why)
                            }
                        },
                    };
                    let (first, principal, answer) = match checked {
                        Ok(checked) => checked,
                        Err(why) => {
                            tracing::warn!(
                                "link from {peer}: {} (gate) answered ask {id} outside the contract's shape ({why}), refused",
                                agent.agent_id
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::Malformed,
                                },
                            )
                            .await;
                            reason = "refused as malformed";
                            break;
                        }
                    };
                    // **The outcome when the answer lands**, by the
                    // contract's evidence alone, written whatever closes
                    // the connection, as a verb's.
                    if let Some((first, ok)) = first.clone().zip(answer.audited()) {
                        inner.record_outcome(&agent.agent_id, &first, ok).await;
                    }
                    // **A failed or unknown outcome is an alert**, raised
                    // without waiting (Spec 2.14).
                    if let Some(raised) = answer.raised() {
                        inner.alerts.raise(failure_of(
                            &agent.agent_id,
                            plane,
                            "turn",
                            first,
                            principal,
                            raised,
                        ));
                    }
                    resolve(&pending, id, answer.into_frame(id));
                }
                (Plane::Admin, FromClient::Verb { id, outcome, error }) => {
                    // **An answer's shape is checked once, here at the door,
                    // against the contract and the ask it answers** (Spec
                    // 2.13, 7.2, 8), and the audit, the landing and the
                    // asker all read that one classification. An answer
                    // outside it, to no ask or of the wrong shape, is the
                    // peer's protocol violation, refused as malformed as
                    // any frame of the wrong shape is; its first record
                    // gets no outcome, since what admin did cannot be said,
                    // and the asker gets no answer, its ask ending with the
                    // connection. Nothing past the door sees it.
                    // **A refused answer leaves its ask in flight** (Spec
                    // 2.14): the entry goes back, so the teardown's drain
                    // raises the act's unknown, the server's own `show`
                    // excepted, like any act left in flight.
                    let asked = asks.lock().unwrap().remove(&id);
                    let checked = match asked {
                        None => Err("it answers no ask"),
                        Some(asked) => match Answer::of(outcome, error, &asked.verb, &agent.name) {
                            Ok(answer) => Ok((asked.first, asked.principal, asked.verb, answer)),
                            Err(why) => {
                                asks.lock().unwrap().insert(id, asked);
                                Err(why)
                            }
                        },
                    };
                    let (first, principal, verb, answer) = match checked {
                        Ok(checked) => checked,
                        Err(why) => {
                            tracing::warn!(
                                "link from {peer}: {} (admin) answered ask {id} outside the contract's shape ({why}), refused",
                                agent.agent_id
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::Malformed,
                                },
                            )
                            .await;
                            reason = "refused as malformed";
                            break;
                        }
                    };
                    let is_admission = admission_show.is_some_and(|(ask, _)| ask == id);
                    // **The outcome when the answer lands, naming its own
                    // first** (Spec 2.13): the entry this answer's id names
                    // was taken at the door, so a second answer to one ask
                    // finds none.
                    // Written ahead of the landing, which a store failure
                    // can cut short after admin has acted. **Once the
                    // answer is in hand its outcome is written whatever
                    // closes the connection**: not raced against the close
                    // signal, since a revocation, rotation or halt landing
                    // mid-write would leave an answered ask's first alone,
                    // reading as unknown. Held to the record step's own
                    // silence bound, and not the admission's deadline, so an
                    // answer near the deadline still has its outcome. What
                    // the close costs: it waits at most one silence bound on
                    // a stalled store, a halt within its grace and aborted
                    // past it.
                    let owed = first.clone().zip(answer.audited());
                    if let Some((first, ok)) = owed {
                        inner.record_outcome(&agent.agent_id, &first, ok).await;
                    }
                    // **A failed or unknown outcome is an alert**, and a
                    // refusal that could lose data a prompt or an
                    // explaining alert, raised without waiting (Spec 2.14).
                    if let Some(raised) = answer.raised(&verb) {
                        inner.alerts.raise(failure_of(
                            &agent.agent_id,
                            plane,
                            &verb,
                            first,
                            principal,
                            raised,
                        ));
                    }
                    // The admission's show is bounded by what remains of
                    // its deadline, not by a fresh bound: a show answered
                    // just before the deadline whose landing stalls would
                    // otherwise leave the connection admitted on stale
                    // facts for almost two bounds. **The deadline is kept as
                    // an instant and read here, at the await it bounds**: a
                    // remainder computed before the outcome's write above
                    // would be stale by up to a silence bound.
                    let until = match admission_show {
                        Some((ask, deadline)) if ask == id => deadline,
                        _ => tokio::time::Instant::now() + inner.silence,
                    };
                    let landing = match &answer {
                        Answer::Outcome(outcome) => match bounded_at(
                            inner.land_verb(&agent.agent_id, outcome),
                            until,
                            &mut close_rx,
                        )
                        .await
                        {
                            Bounded::Done(landing) => landing,
                            Bounded::TimedOut if is_admission => {
                                tracing::warn!(
                                    "{}: the admission's show answered but its landing did not complete by the deadline, closed so the reconnect asks again",
                                    agent.agent_id
                                );
                                send(
                                    &tx,
                                    ToClient::Refusal {
                                        reason: Refusal::AdmissionIncomplete,
                                    },
                                )
                                .await;
                                reason = "the admission's show did not answer";
                                break;
                            }
                            Bounded::TimedOut => {
                                tracing::error!(
                                    "{}: admin's {} answer was not landed within the bound",
                                    agent.agent_id,
                                    outcome.verb
                                );
                                Landing::Failed
                            }
                            Bounded::Closed => {
                                reason = if silent.load(Ordering::Relaxed) {
                                    "silent for the bound: the peer stopped reading"
                                } else {
                                    "closed by revocation, rotation or halt"
                                };
                                break;
                            }
                        },
                        Answer::Fault(_) => Landing::Nothing,
                    };
                    if landing == Landing::Failed {
                        // The answer could not be landed: the ask answers the
                        // failure and never the outcome, and the connection
                        // closes so the admission's `show` is asked again.
                        tracing::error!(
                            "{}: admin's {} answer could not be landed, closing the connection",
                            agent.agent_id,
                            match &answer {
                                Answer::Outcome(o) => o.verb.as_str(),
                                Answer::Fault(_) => "show",
                            }
                        );
                        resolve(
                            &pending,
                            id,
                            FromClient::Verb {
                                id,
                                outcome: None,
                                error: Some(VerbFault {
                                    kind: VerbFault::NOT_LANDED.into(),
                                    message: "the store could not land admin's answer; the connection is closed and the ask is owed again".into(),
                                }),
                            },
                        );
                        send(
                            &tx,
                            ToClient::Refusal {
                                reason: Refusal::StoreUnavailable,
                            },
                        )
                        .await;
                        reason = "the store was unavailable";
                        break;
                    }
                    if is_admission {
                        if landing == Landing::Observation {
                            admission_show = None;
                            // **The server's acceptance is the one signal**
                            // that ends admin-con's hold (Spec 7.2): sent
                            // only once the store took the observation, and
                            // **delivered or the connection closes**, as an
                            // ask is. Dropped on a full write path, it would
                            // leave admin-con holding every ordinary verb for
                            // the connection's life; closed, the hold ends
                            // with it and the reconnect admits again.
                            if let Err(why) =
                                enqueue(&tx, ToClient::Landed { id }, inner.silence, &mut close_rx)
                                    .await
                            {
                                tracing::warn!(
                                    "{}: the show's landing could not be reported ({why}), closed so admin-con's hold ends with the connection",
                                    agent.agent_id
                                );
                                reason = why;
                                break;
                            }
                        } else {
                            tracing::warn!(
                                "{}: the admission's show answered without a usable observation ({}), closed so the reconnect asks again",
                                agent.agent_id,
                                match &answer {
                                    Answer::Fault(e) => e.message.as_str(),
                                    Answer::Outcome(_) => "no state in the answer",
                                }
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::AdmissionIncomplete,
                                },
                            )
                            .await;
                            reason = "the admission's show did not answer";
                            break;
                        }
                    }
                    resolve(&pending, id, answer.into_frame(id));
                }
                (
                    Plane::Admin,
                    FromClient::Door {
                        open,
                        wall_ms,
                        tail,
                    },
                ) => {
                    // **Every opening of the door is an admission of the
                    // trace** (Spec 7.2): it carries its boundary, events
                    // until the next `caught_up` are its replay, and `show`
                    // is asked where the ceiling grants it, the opening
                    // complete only when both have arrived. A closing ends
                    // any replay: nothing is behind a closed door. A door
                    // and boundary that disagree are malformed.
                    let tail = match (open, tail) {
                        (true, Some(tail)) => Some(tail),
                        (false, None) => None,
                        _ => {
                            tracing::warn!(
                                "link from {peer}: {} (admin) sent a door frame whose state and boundary disagree, refused",
                                agent.agent_id
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::Malformed,
                                },
                            )
                            .await;
                            reason = "refused as malformed";
                            break;
                        }
                    };
                    let at = i64::try_from(wall_ms)
                        .ok()
                        .and_then(DateTime::<Utc>::from_timestamp_millis)
                        .unwrap_or_else(Utc::now);
                    // **Bound to the connection like a link-state write**
                    // (Spec 2.12): it lands only while this incarnation is
                    // the row's live admin connection.
                    match bounded(
                        inner
                            .store
                            .land_door(&agent.agent_id, incarnation, open, at),
                        inner.silence,
                        &mut close_rx,
                    )
                    .await
                    {
                        Bounded::Done(Ok(true)) => {}
                        // **The row names another connection**: a rotation,
                        // revocation or replacement committed and its
                        // notification has not closed this socket yet. That
                        // is definitive, so this connection closes now, as a
                        // credential no longer live, before anything more it
                        // carries can write the row.
                        Bounded::Done(Ok(false)) => {
                            tracing::warn!(
                                "{}: the door's state was not landed: the row names another connection, closed",
                                agent.agent_id
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::NotLive,
                                },
                            )
                            .await;
                            reason = "the row names another connection";
                            break;
                        }
                        Bounded::Done(Err(_)) | Bounded::TimedOut => {
                            tracing::error!(
                                "{}: the door's state could not be landed, closing the connection",
                                agent.agent_id
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::StoreUnavailable,
                                },
                            )
                            .await;
                            reason = "the store was unavailable";
                            break;
                        }
                        Bounded::Closed => {
                            reason = if silent.load(Ordering::Relaxed) {
                                "silent for the bound: the peer stopped reading"
                            } else {
                                "closed by revocation, rotation or halt"
                            };
                            break;
                        }
                    }
                    caught_up = !open;
                    boundary = tail;
                    if open && ceiling.as_deref().is_some_and(|c| c.contains("show")) {
                        match ask_show(&inner, &agent.agent_id, &tx, &asks, &mut close_rx).await {
                            Ok(id) => {
                                admission_show =
                                    Some((id, tokio::time::Instant::now() + inner.silence))
                            }
                            Err(why) => {
                                reason = why;
                                break;
                            }
                        }
                    }
                }
                (
                    Plane::Admin,
                    FromClient::Event {
                        position,
                        replayed,
                        event,
                    },
                ) => {
                    // **Acknowledged only once every write the event owed the
                    // register landed** (Spec 7.2): an ack is the server's word
                    // that it holds the event, and a reconnection resumes past
                    // it. An observation the store refused is not acknowledged;
                    // admin-con resends from its last acknowledged position,
                    // which is the replay doing its job.
                    let landed = match bounded(
                        inner.land_event(
                            &agent.agent_id,
                            boundary.as_ref(),
                            caught_up,
                            &position,
                            replayed,
                            event,
                        ),
                        inner.silence,
                        &mut close_rx,
                    )
                    .await
                    {
                        Bounded::Done(Ok(landed)) => landed,
                        Bounded::TimedOut => {
                            tracing::error!(
                                "{}: the event at {}:{} was not landed within the bound",
                                agent.agent_id,
                                position.generation,
                                position.offset
                            );
                            false
                        }
                        Bounded::Closed => {
                            reason = if silent.load(Ordering::Relaxed) {
                                "silent for the bound: the peer stopped reading"
                            } else {
                                "closed by revocation, rotation or halt"
                            };
                            break;
                        }
                        Bounded::Done(Err(refusal)) => {
                            tracing::warn!(
                                "{}: an event beyond the boundary arrived before the replay was caught up, refused",
                                agent.agent_id
                            );
                            send(&tx, ToClient::Refusal { reason: refusal }).await;
                            break;
                        }
                    };
                    if landed {
                        inner
                            .acknowledged
                            .lock()
                            .unwrap()
                            .insert(agent.agent_id.clone(), position.clone());
                        // **An ack never blocks the read loop** (Spec 7.2):
                        // admin-con reads acks only between its replay steps,
                        // so a backfill of many small records fills the write
                        // queue with acks while it is still sending, and an
                        // enqueue that waited would stop this loop reading
                        // the events that would let it drain: both ends
                        // blocked until the silence bound. An ack is dropped
                        // where the queue is full. Nothing is lost by it:
                        // each ack names a later position than the last, the
                        // position the hello's answer resumes from is the one
                        // recorded above and not the frames sent, and
                        // admin-con keeps no state from acks.
                        if let Err(mpsc::error::TrySendError::Closed(_)) =
                            tx.try_send(ToClient::Ack { position })
                        {
                            reason = "the write path is gone";
                            break;
                        }
                    } else {
                        // Closed rather than skipped: a later ack would name a
                        // position past the event the store does not hold.
                        tracing::error!(
                            "{}: the event at {}:{} was not landed on the row, closing the connection at the last acknowledged position",
                            agent.agent_id,
                            position.generation,
                            position.offset
                        );
                        send(
                            &tx,
                            ToClient::Refusal {
                                reason: Refusal::StoreUnavailable,
                            },
                        )
                        .await;
                        reason = "the store was unavailable";
                        break;
                    }
                }
                (
                    plane,
                    FromClient::Entry {
                        run,
                        seq,
                        source,
                        entry,
                    },
                ) => {
                    // **An entry is checked once, here at the door** (Spec
                    // 2.14): its identity, its source on the plane's
                    // allow-list and its shape and bound; anything else is
                    // the peer's protocol violation, refused as malformed
                    // as any frame of the wrong shape is. Nothing past the
                    // door sees an unchecked entry.
                    let checked = match CheckedEntry::of(plane, &run, seq, &source, entry) {
                        Ok(checked) => checked,
                        Err(why) => {
                            tracing::warn!(
                                "link from {peer}: {} ({plane}) sent an entry outside its shape ({why}), refused",
                                agent.agent_id
                            );
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::Malformed,
                                },
                            )
                            .await;
                            reason = "refused as malformed";
                            break;
                        }
                    };
                    // **Taken once** (Spec 2.14): a resend, after a lost
                    // acknowledgement, a reconnection or a server restart,
                    // is acknowledged and nothing more. **An entry the
                    // store did not take closes the connection**, as an
                    // event's failed landing does: a later entry taken past
                    // it would move the run's mark beyond it, and its
                    // resend would be taken for a duplicate. Closed, every
                    // entry not acknowledged is resent in order.
                    let take = async {
                        #[cfg(test)]
                        if inner.fault_next_entry.swap(false, Ordering::Relaxed) {
                            return Err(anyhow::anyhow!(
                                "a test fault made this entry's take fail"
                            ));
                        }
                        inner
                            .store
                            .take_entry(&run, &agent.agent_id, plane.as_str(), seq)
                            .await
                    };
                    let taken = tokio::time::timeout(inner.silence, take).await;
                    match taken {
                        Ok(Ok(new)) => {
                            if new {
                                inner
                                    .alerts
                                    .entry(checked.into_entry(&agent.agent_id, plane));
                            }
                            // **An acknowledgement never blocks the read
                            // loop**, as an event's: acknowledgements are
                            // cumulative, so a dropped one is covered by
                            // the next, or by the resend and its dedupe
                            // after a reconnection.
                            if let Err(mpsc::error::TrySendError::Closed(_)) =
                                tx.try_send(ToClient::EntryAck { run, seq })
                            {
                                reason = "the write path is gone";
                                break;
                            }
                        }
                        failed => {
                            match failed {
                                Ok(Err(e)) => tracing::error!(
                                    "{}: an entry could not be taken, closing so it is resent in order: {e:#}",
                                    agent.agent_id
                                ),
                                _ => tracing::error!(
                                    "{}: an entry was not taken within {:?}, closing so it is resent in order",
                                    agent.agent_id,
                                    inner.silence
                                ),
                            }
                            send(
                                &tx,
                                ToClient::Refusal {
                                    reason: Refusal::StoreUnavailable,
                                },
                            )
                            .await;
                            reason = "the store was unavailable";
                            break;
                        }
                    }
                }
                (_, other) => {
                    // **A frame on the wrong plane is refused and logged
                    // against the row** (Spec 8): a tuple or load state can
                    // only arrive by admin-con, and a turn only by gate-con.
                    tracing::warn!(
                        "link from {peer}: {} ({plane}) sent a {} frame that belongs to the other plane, refused",
                        agent.agent_id,
                        frame_name(&other)
                    );
                    send(
                        &tx,
                        ToClient::Refusal {
                            reason: Refusal::WrongPlane,
                        },
                    )
                    .await;
                    reason = "refused on the wrong plane";
                    break;
                }
            }
        }
    }
    if reason.starts_with("silent") {
        send(
            &tx,
            ToClient::Refusal {
                reason: Refusal::Silence,
            },
        )
        .await;
    }

    // **Contact lost, raised where the entry comes out, under the row's
    // lock** (Spec 2.14): a new admission of this plane is decided under
    // that lock too, so its restoration is queued after this loss and the
    // one worker applies them in that order. Not for the server's own
    // closes.
    let removed = std::sync::atomic::AtomicBool::new(false);
    let lose = || {
        if SERVERS_OWN_CLOSES.contains(&reason) {
            return;
        }
        let connector = match plane {
            Plane::Admin => "admin-con",
            Plane::Gate => "gate-con",
        };
        inner.alerts.raise(failure_of(
            &agent.agent_id,
            plane,
            plane.as_str(),
            None,
            "server".to_owned(),
            Raised {
                kind: AlertKind::LostContact,
                summary: format!(
                    "contact with {connector} lost ({reason}); the agent may still be running"
                ),
                detail: String::new(),
                reason: serde_json::json!({ "teardown": reason }),
                options: Vec::new(),
                outcome: "lost_contact",
            },
        ));
    };
    let uninstall = || {
        let mine = inner.remove_if_mine(&key, incarnation);
        removed.store(mine, Ordering::Relaxed);
        if mine {
            lose();
        }
    };
    // **Teardown bound to the incarnation, under the row's lock** (Spec 8):
    // the uninstall runs under the lock on every outcome, and the
    // disconnected write lands only while this incarnation is the live one.
    #[cfg(test)]
    let torn = if inner.fault_next_teardown.swap(false, Ordering::Relaxed) {
        Err(anyhow::anyhow!(
            "a test fault made this teardown's store write fail"
        ))
    } else {
        inner
            .store
            .teardown(&agent.agent_id, plane, incarnation, uninstall)
            .await
    };
    #[cfg(not(test))]
    let torn = inner
        .store
        .teardown(&agent.agent_id, plane, incarnation, uninstall)
        .await;
    match torn {
        Ok(true) => {}
        Ok(false) => {
            // A revocation or a replacement already wrote this plane; the
            // entry came out under the lock and nothing else is owed.
        }
        Err(e) => {
            // The row may still record this incarnation as connected: the
            // entry comes out of the live map, and the disconnected write
            // is reconciled rather than forgotten.
            tracing::error!(
                "link from {peer}: teardown could not be written, deferred until the store answers: {e:#}"
            );
            if inner.remove_after_error(&key, incarnation, &lose).await {
                removed.store(true, Ordering::Relaxed);
            }
            inner.defer_teardown(agent.agent_id.clone(), plane, incarnation);
        }
    }
    #[cfg(test)]
    inner.pause_at(AskPoint::AfterTeardown).await;
    // **The window is marked whenever this connection actually left the
    // live map**, whatever the row's write answered: a revoked or rotated
    // credential has its incarnation cleared before the close, and a store
    // error removes the entry without a write, and in both the link is
    // gone and the window owes its reader the discontinuity.
    if plane == Plane::Admin && removed.load(Ordering::Relaxed) {
        inner.windows.mark(
            agent.agent_id.as_str(),
            &format!("link to admin-con lost: {reason}"),
        );
    }
    // **An act left in flight is an alert, its outcome unknown here**, and
    // **contact lost for any reason but the server's own act** is one too,
    // the agent possibly still running (Spec 2.14), each raised without
    // waiting. **The server's own `show` is left out**: it acts on nothing,
    // so its outcome cannot matter, and the teardown's lost contact already
    // stands for the connection.
    let in_flight: Vec<Asked> = asks
        .lock()
        .unwrap()
        .drain()
        .map(|(_, asked)| asked)
        .filter(|asked| !(asked.principal == "server" && asked.verb == "show"))
        .collect();
    for asked in in_flight {
        inner.alerts.raise(failure_of(
            &agent.agent_id,
            plane,
            &asked.verb,
            asked.first,
            asked.principal,
            Raised {
                kind: AlertKind::Unknown,
                summary: format!(
                    "{}: the connection ended with it in flight, so whether it ran is unknown here",
                    asked.verb
                ),
                detail: reason.to_owned(),
                reason: serde_json::json!({ "teardown": reason }),
                options: Vec::new(),
                outcome: "unknown",
            },
        ));
    }
    pending.lock().unwrap().clear();
    drop(tx);
    drain_writer(writer).await;
    tracing::info!(
        "link from {peer}: {} ({plane}) incarnation {incarnation} closed",
        agent.agent_id
    );
}

fn frame_name(frame: &FromClient) -> &'static str {
    match frame {
        FromClient::Hello { .. } => "hello",
        FromClient::Heartbeat => "heartbeat",
        FromClient::CaughtUp => "caught_up",
        FromClient::Turn { .. } => "turn",
        FromClient::Verb { .. } => "verb",
        FromClient::Door { .. } => "door",
        FromClient::Event { .. } => "event",
        FromClient::Entry { .. } => "entry",
    }
}

fn resolve(
    pending: &Arc<Mutex<HashMap<u64, oneshot::Sender<FromClient>>>>,
    id: u64,
    answer: FromClient,
) {
    if let Some(reply) = pending.lock().unwrap().remove(&id) {
        let _ = reply.send(answer);
    }
}

/// A trace event's own time, from the envelope's `wall_ms`.
fn event_time(event: &TraceEvent) -> Option<DateTime<Utc>> {
    event
        .raw
        .get("wall_ms")
        .and_then(|v| v.as_i64())
        .and_then(DateTime::<Utc>::from_timestamp_millis)
}

impl Inner {
    /// **A `show` answer is admin's word** (Spec 7.2, 2.12) and lands on
    /// the row under the arrival sequence, naming `show` as its source. Any
    /// other verb's answer lands nothing. **The date is the receipt's**, a
    /// gap section 2.12 names: admin's answer carries no time of its own.
    async fn land_verb(&self, agent: &AgentId, outcome: &VerbOutcome) -> Landing {
        // **Only admin's answer is an observation**: a refusal (exit 1) is
        // never admin's word on the state, whatever its object says.
        let Some(answer) = outcome
            .answer
            .as_ref()
            .filter(|_| outcome.exit_code == Some(0))
        else {
            return Landing::Nothing;
        };
        let summary = match outcome.verb.as_str() {
            "show" if answer.get("kind").and_then(|k| k.as_str()) == Some("state") => {
                Some(answer.clone())
            }
            _ => None,
        };
        let Some(summary) = summary else {
            return Landing::Nothing;
        };
        let observation = Observation {
            load_state: summary
                .get("state")
                .and_then(|s| s.as_str())
                .map(str::to_owned),
            tuple: TupleWrite::Write(summary.get("load").cloned().filter(|l| !l.is_null())),
            at: Utc::now(),
            source: "show",
            // **The run's constituents, as this answer names them** (Spec
            // 2.12): admin's pids of the worker, the state member and the
            // relay where a run holds the lock, absent otherwise. A pid that
            // does not fit the column is not a pid, and the answer's
            // constituents are then recorded as none rather than guessed.
            constituents: ConstituentsWrite::Write(Self::constituents_of(&summary)),
        };
        if self.land(agent, observation).await {
            Landing::Observation
        } else {
            Landing::Failed
        }
    }

    /// The constituents a `show` answer names, `None` where it names none or
    /// names one that is not a pid the column can hold.
    fn constituents_of(summary: &serde_json::Value) -> Option<Vec<i32>> {
        let pids = summary.get("constituents")?.as_array()?;
        if pids.is_empty() {
            return None;
        }
        pids.iter()
            .map(|p| p.as_u64().and_then(|p| i32::try_from(p).ok()))
            .collect()
    }

    /// **An event feeds the window, and only a live load or unload writes
    /// the row** (Spec 7.2, 2.12). **The `caught_up` frame classifies, and
    /// the client's flag is a check**: an event before it is the replay's,
    /// in whatever generation, and one after it is live. In the boundary's
    /// generation the offset rule checks the frame, and a disagreement
    /// with the flag is logged.
    async fn land_event(
        &self,
        agent: &AgentId,
        boundary: Option<&Position>,
        caught_up: bool,
        position: &Position,
        replayed: bool,
        event: TraceEvent,
    ) -> Result<bool, Refusal> {
        // **The frame decides** (Spec 7.2): before `caught_up` an event is
        // the replay's, after it live. The boundary's offset rule and the
        // client's flag are checks: an event beyond the boundary in its
        // generation before the frame is a protocol fault and refused,
        // and any other disagreement is logged. The stream's order alone
        // could not decide, since a file rotated after the hello before
        // any event of the boundary's generation reached the boundary would
        // leave every live event of the new generation looking like an
        // older generation's tail.
        let behind = !caught_up;
        // **Marks are held to the offset rule as records are**: a mark
        // carries the position relaying resumes at, never past the
        // boundary before `caught_up`.
        if let Some(b) = boundary
            && b.generation == position.generation
        {
            // **A position is the byte after its record** (Spec 7.2), so the
            // event whose position equals the boundary is the last one
            // behind it, and one beyond the boundary is one whose position
            // is past it.
            let offset_says_behind = position.offset <= b.offset;
            if !caught_up && !offset_says_behind {
                return Err(Refusal::Malformed);
            }
            if caught_up && offset_says_behind {
                tracing::warn!(
                    "{agent}: an event at {}:{} is behind the boundary but arrived after caught_up, taken as live",
                    position.generation,
                    position.offset
                );
            }
        }
        if behind != replayed {
            tracing::warn!(
                "{agent}: admin-con flagged an event at {}:{} as {}, the boundary says {}",
                position.generation,
                position.offset,
                if replayed { "replayed" } else { "live" },
                if behind { "replayed" } else { "live" }
            );
        }
        let kind = event.kind.clone();
        let payload = event.raw.get("payload").cloned();
        // **A loss that already happened** (Spec 2.14, the operator's ruling
        // of 2026-10-15): the trace's `load` event carries `reset` where the
        // agent's last run did not end in a clean unload, the load having
        // reset it to its latest save point (`weaver-trace` `event.rs` at
        // `toddwbucy/WeaverAgent` `ba3eb4e`). Read by allow-list, live or
        // replayed, and raised **once ever per prior run**, so a replay or a
        // backfill of the same event raises nothing again.
        if kind.as_deref() == Some("load")
            && let Some(reset) = payload.as_ref().and_then(|p| p.get("reset"))
            && let Some(prior) = reset.get("prior_run").and_then(|r| r.as_str())
        {
            let why = reset
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("unnamed");
            let said = match why {
                "forced-unload" => {
                    "the prior run was ended by a forced unload, without its leave save point"
                }
                "no-clean-unload" => "the prior run did not end in a clean unload",
                _ => "the prior run did not end cleanly",
            };
            self.alerts.raise(Failure {
                raise: Raise {
                    agent: agent.clone(),
                    kind: AlertKind::DataLost,
                    subject: format!("reset of {}", cut(prior, 48)),
                    summary: format!(
                        "the load reset the agent to its latest save point ({why}): {said}, and its state since that save point is lost"
                    ),
                    detail: reset.to_string(),
                    first: None,
                    options: Vec::new(),
                    source: "trace",
                    once: true,
                },
                plane: "admin",
                principal: "server".to_owned(),
                outcome: "data_lost",
                reason: serde_json::json!({ "reset": reset }),
            });
        }
        // **Admin's date** (Spec 2.12): the event's own time, and the
        // receipt's only where the record carries none.
        let at = event_time(&event).unwrap_or_else(|| {
            tracing::warn!("{agent}: an event carried no wall_ms, dated at receipt");
            Utc::now()
        });
        let observation = match kind.as_deref() {
            // A load event means the agent was admitted and stands idle;
            // its payload is the declared tuple the trace carries, the
            // declaration's digest among it.
            Some("load") if !behind => Some(Observation {
                load_state: Some("idle".into()),
                tuple: TupleWrite::Write(payload),
                at,
                source: "event",
                constituents: ConstituentsWrite::Keep,
            }),
            Some("unload") if !behind => Some(Observation {
                load_state: Some("unloaded".into()),
                tuple: TupleWrite::Write(None),
                at,
                source: "event",
                constituents: ConstituentsWrite::Keep,
            }),
            // **A turn's start and close refresh the load state** (Spec
            // 2.12): an unclean stop writes no `unload`, so a state from an
            // event can outlive its process, and these two keep it no older
            // than the agent's last turn. Same rules as a load: never from
            // behind the boundary, the event's own date, source `event`. A
            // turn says nothing of the tuple, so the row keeps its own.
            Some("turn.started") if !behind => Some(Observation {
                load_state: Some("active".into()),
                tuple: TupleWrite::Keep,
                at,
                source: "event",
                constituents: ConstituentsWrite::Keep,
            }),
            Some("turn.closed") if !behind => Some(Observation {
                load_state: Some("idle".into()),
                tuple: TupleWrite::Keep,
                at,
                source: "event",
                constituents: ConstituentsWrite::Keep,
            }),
            _ => None,
        };
        // **A lifecycle event enters the window only after its landing
        // succeeds**: landed first and then ingested, a failed write that
        // closes the connection would leave the event in the window and the
        // replay would ingest it a second time around the reconnect mark.
        // Every other event enters as it arrives.
        match observation {
            Some(observation) => {
                if !self.land(agent, observation).await {
                    return Ok(false);
                }
                self.windows.ingest(agent.as_str(), event);
                Ok(true)
            }
            None => {
                self.windows.ingest(agent.as_str(), event);
                Ok(true)
            }
        }
    }

    /// Whether the store took the observation or ordered it below the one
    /// it holds; false only where the write failed.
    async fn land(&self, agent: &AgentId, observation: Observation) -> bool {
        #[cfg(test)]
        if self.fault_next_land.swap(false, Ordering::Relaxed) {
            tracing::error!("{agent}: a test fault made this store write fail");
            return false;
        }
        #[cfg(test)]
        if self.stall_next_land.swap(false, Ordering::Relaxed) {
            tracing::error!("{agent}: a test fault made this store write stall");
            std::future::pending::<()>().await;
        }
        let arrival = self.next_arrival();
        match self
            .store
            .land_observation(agent, &observation, self.epoch, arrival)
            .await
        {
            Ok(true) => true,
            Ok(false) => {
                tracing::debug!("{agent}: an observation ordered below the stored one");
                true
            }
            Err(e) => {
                tracing::error!("{agent}: an observation could not be landed: {e:#}");
                false
            }
        }
    }
}
