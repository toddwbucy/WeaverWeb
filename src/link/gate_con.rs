//! gate-con, the data plane's connector (Spec sections 7.1 and 8): on the
//! agent's box, a client of the server's listener over the link with the
//! gate credential the register verb minted, relaying each turn the server
//! asks for to the agent's gate socket through `adapters::gate` and its
//! close back.
//!
//! **It learns nothing of the interior and reports nothing of it.** What
//! crosses is the close the gate returned, verbatim through `GateClose`, or
//! the adapter's typed fault, and nothing else: no tuple, no load state, no
//! inference from the socket's existence.
//!
//! **The in-flight set and the connection agree on every exit path.** The
//! turns in flight belong to the connection their asks arrived on: they
//! are held in a set the connection's serve owns, their answers go out on
//! that connection and no later one, and every way the connection ends
//! (a refusal, a loss, a failed exchange, shutdown) aborts and joins the
//! set before the connection is closed, so no gate exchange outlives the
//! link that asked for it.

use crate::adapters::gate::{self, GateAdapter, GateError};
use crate::link::client::{
    self, Backoff, Connection, Ended, Incoming, Link, LinkConfig, LinkStatus,
};
use crate::link::frames::{FromClient, Plane, ToClient, TurnFault};
use serde::Deserialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinSet;

/// Turns in flight at once when the config names no number.
pub const DEFAULT_TURNS_IN_FLIGHT: usize = 4;

/// The most turns in flight a config may name, so the in-flight set is
/// bounded whatever the config says.
pub const MAX_TURNS_IN_FLIGHT: usize = 64;

/// **Asks waiting behind the in-flight bound are bounded too.** Each holds a
/// request no longer than the gate's line bound (an ask past it is answered
/// at once), so the queue costs at most this many lines. **An ask arriving
/// to a full queue is answered at once with the fault `busy`**, gate-con's
/// own back-pressure and not one of the gate's kinds, as Spec 8 names it.
pub const WAITING_BOUND: usize = 64;

/// How long shutdown lets turns in flight finish before aborting them.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// gate-con's config: what `weaver-web register` wrote, plus the gate
/// socket's path, a box fact filled at install, with no default.
#[derive(Debug, Clone, Deserialize)]
pub struct GateConConfig {
    #[serde(flatten)]
    pub link: LinkConfig,
    /// The agent's gate socket.
    pub gate_socket: PathBuf,
    /// Turns relayed at once; further asks wait in arrival order.
    #[serde(default = "default_turns_in_flight")]
    pub turns_in_flight: usize,
    /// Asks that may wait behind the in-flight bound. Not a config member:
    /// `WAITING_BOUND`, settable in code so a test can reach it.
    #[serde(skip, default = "default_waiting_bound")]
    pub waiting_bound: usize,
}

fn default_turns_in_flight() -> usize {
    DEFAULT_TURNS_IN_FLIGHT
}

fn default_waiting_bound() -> usize {
    WAITING_BOUND
}

impl GateConConfig {
    /// Read the config under the trust rule of `client::read_private`, and
    /// refuse one minted for the admin plane, one missing a member, or one
    /// naming no turn in flight or more than `MAX_TURNS_IN_FLIGHT`. **A
    /// parse error names its line and never prints it**: the file carries a
    /// key, and a corrupted PEM line is the key's own bytes.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = client::read_private(path)?;
        let cfg: Self = client::parse_config(path, &content, MEMBERS)?;
        cfg.link
            .expect_plane(Plane::Gate)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        if cfg.turns_in_flight == 0 || cfg.turns_in_flight > MAX_TURNS_IN_FLIGHT {
            anyhow::bail!(
                "{}: turns_in_flight is {}, outside 1 to {MAX_TURNS_IN_FLIGHT}",
                path.display(),
                cfg.turns_in_flight
            );
        }
        Ok(cfg)
    }
}

/// The members a gate-con config may carry: the only names a parse error
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
    "gate_socket",
    "turns_in_flight",
];

/// **The fault for an ask that never reached the gate**, gate-con's own
/// like `busy` and not one of the gate's kinds: answered at shutdown to
/// every ask still waiting behind the in-flight bound, so its caller knows
/// the turn did not run.
pub const NOT_STARTED: &str = "not_started";

fn not_started(id: u64) -> FromClient {
    FromClient::Turn {
        id,
        close: None,
        error: Some(TurnFault {
            kind: NOT_STARTED.to_owned(),
            message: "gate-con stopped before this turn reached the gate; it did not run"
                .to_owned(),
        }),
    }
}

/// The adapter's error as the link carries it.
pub fn fault(e: &GateError) -> TurnFault {
    TurnFault {
        kind: e.kind().to_owned(),
        message: e.to_string(),
    }
}

fn answer(id: u64, relayed: Result<gate::GateClose, GateError>) -> FromClient {
    match relayed {
        Ok(close) => FromClient::Turn {
            id,
            close: Some(close),
            error: None,
        },
        Err(e) => FromClient::Turn {
            id,
            close: None,
            error: Some(fault(&e)),
        },
    }
}

/// Run gate-con until `shutdown` is set: the client loop of
/// `client::run` with this plane's serve. **`source` is the config's path**,
/// re-read before each retry at the cap so a re-installed credential is
/// picked up without a restart; its link members are what change at a
/// re-install, and `gate_socket` and the bounds stay as started.
pub async fn run(
    cfg: GateConConfig,
    source: Option<PathBuf>,
    backoff: Backoff,
    shutdown: watch::Receiver<bool>,
    status: &watch::Sender<LinkStatus>,
) -> anyhow::Result<()> {
    let link = Link::new(cfg.link.clone())?;
    let adapter = GateAdapter::new(&cfg.gate_socket);
    let bound = cfg.turns_in_flight;
    let waiting_bound = cfg.waiting_bound;
    let serve_shutdown = shutdown.clone();
    let reload = || {
        let path = source.as_ref()?;
        match GateConConfig::load(path) {
            Ok(fresh) => Some(fresh.link),
            Err(e) => {
                tracing::warn!("re-reading the config: {e:#}; keeping the credential in hand");
                None
            }
        }
    };
    client::run(
        link,
        |link: &LinkConfig| {
            std::future::ready(Ok(FromClient::Hello {
                agent: link.agent.clone(),
                plane: Plane::Gate,
                tail: None,
                ceiling: None,
            }))
        },
        reload,
        backoff,
        shutdown,
        status,
        |conn| {
            serve(
                conn,
                adapter.clone(),
                bound,
                waiting_bound,
                serve_shutdown.clone(),
            )
        },
    )
    .await;
    Ok(())
}

/// Serve one admitted connection, and on every exit path abort and join
/// the turns in flight before closing it.
async fn serve(
    mut conn: Connection,
    adapter: GateAdapter,
    bound: usize,
    waiting_bound: usize,
    mut shutdown: watch::Receiver<bool>,
) -> Ended {
    let mut in_flight: JoinSet<FromClient> = JoinSet::new();
    let mut waiting: VecDeque<(u64, String)> = VecDeque::new();
    let ended = relay(
        &mut conn,
        &adapter,
        bound,
        waiting_bound,
        &mut shutdown,
        &mut in_flight,
        &mut waiting,
    )
    .await;
    let grace_ends = tokio::time::Instant::now() + SHUTDOWN_GRACE;
    if matches!(ended, Ended::Shutdown) {
        // **Asks still waiting are answered `not_started` first**, while the
        // link still stands: none reached the gate, so its caller is told it
        // definitely did not run and may be asked again, which a turn lost
        // with the link, whose outcome is unknown, cannot be (Spec 7.1).
        let declined = tokio::time::timeout_at(grace_ends, async {
            for (id, _) in waiting.drain(..) {
                if conn.send(not_started(id)).await.is_err() {
                    return;
                }
            }
        })
        .await;
        if declined.is_err() {
            tracing::warn!("the asks still waiting were not all answered within the grace");
        }
        // **Shutdown lets turns in flight finish within a grace**, their
        // answers sent while the link still stands.
        let finished = tokio::time::timeout_at(grace_ends, async {
            while let Some(done) = in_flight.join_next().await {
                if let Ok(frame) = done
                    && conn.send(frame).await.is_err()
                {
                    return;
                }
            }
        })
        .await;
        if finished.is_err() {
            tracing::warn!(
                "{} turns still in flight after the {SHUTDOWN_GRACE:?} grace are aborted",
                in_flight.len()
            );
        }
    }
    in_flight.abort_all();
    while in_flight.join_next().await.is_some() {}
    waiting.clear();
    // **The whole stop is bounded by the grace**: the writer's drain gets
    // what remains of it, not a further cadence, so a server that stopped
    // taking bytes cannot hold SIGTERM past the grace it was promised.
    if matches!(ended, Ended::Shutdown) {
        conn.close_within(grace_ends.saturating_duration_since(tokio::time::Instant::now()))
            .await;
    } else {
        conn.close().await;
    }
    ended
}

/// An answer to the server that **watches shutdown while it waits**: a
/// send to a server that stopped taking bytes waits up to a cadence, and a
/// stop must not wait behind it.
async fn send(
    conn: &Connection,
    frame: FromClient,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<(), Ended> {
    tokio::select! {
        sent = conn.send(frame) => sent.map_err(Ended::Lost),
        _ = shutdown.changed() => Err(Ended::Shutdown),
    }
}

async fn relay(
    conn: &mut Connection,
    adapter: &GateAdapter,
    bound: usize,
    waiting_bound: usize,
    shutdown: &mut watch::Receiver<bool>,
    in_flight: &mut JoinSet<FromClient>,
    waiting: &mut VecDeque<(u64, String)>,
) -> Ended {
    loop {
        // **A stop is checked before any waiting turn is promoted**: a turn
        // still waiting when the stop began must stay waiting, to be
        // answered `not_started` by `serve`, and never reach the gate in
        // the grace. A stop seen while a send waited lands here.
        if *shutdown.borrow_and_update() {
            return Ended::Shutdown;
        }
        // **At most `bound` turns in flight, the rest waiting in arrival
        // order.** The gate serializes turns for the agent anyway; the
        // bound is this process's protection against an unbounded task set.
        while in_flight.len() < bound
            && let Some((id, text)) = waiting.pop_front()
        {
            let adapter = adapter.clone();
            in_flight.spawn(async move { answer(id, adapter.turn(&text).await) });
        }
        // **Biased, the stop first**: ready together with a completion or
        // a frame, the stop wins, so the next pass cannot promote a turn
        // that was waiting when it began.
        tokio::select! {
            biased;
            _ = shutdown.changed() => return Ended::Shutdown,
            Some(done) = in_flight.join_next(), if !in_flight.is_empty() => {
                let frame = match done {
                    Ok(frame) => frame,
                    // A failed exchange loses its ask's id: the connection
                    // ends, so the server fails that ask rather than holding
                    // it for an answer that cannot come.
                    Err(e) => return Ended::Lost(format!("a gate exchange failed: {e}")),
                };
                if let Err(end) = send(conn, frame, shutdown).await {
                    return end;
                }
            }
            incoming = conn.recv() => match incoming {
                Incoming::Frame(ToClient::Turn { id, text }) => {
                    // An ask past the gate's bound is answered at once and
                    // never queued, so a waiting entry costs one gate line.
                    if let Err(e) = gate::request_line(&text) {
                        if let Err(end) = send(conn, answer(id, Err(e)), shutdown).await {
                            return end;
                        }
                    } else if waiting.len() >= waiting_bound {
                        let busy = FromClient::Turn {
                            id,
                            close: None,
                            error: Some(TurnFault {
                                kind: "busy".to_owned(),
                                message: format!(
                                    "gate-con holds {} turns in flight and {} waiting, its bounds",
                                    in_flight.len(),
                                    waiting.len()
                                ),
                            }),
                        };
                        if let Err(end) = send(conn, busy, shutdown).await {
                            return end;
                        }
                    } else {
                        waiting.push_back((id, text));
                    }
                }
                Incoming::Frame(other) => {
                    let what = client::to_client_name(&other);
                    tracing::error!("protocol fault: the server sent a {what} frame on the gate plane");
                    return Ended::Lost(format!(
                        "protocol fault: a {what} frame on the gate plane"
                    ));
                }
                Incoming::Refused(reason) => return Ended::Refused(reason),
                Incoming::Lost(why) => return Ended::Lost(why),
            },
        }
    }
}
