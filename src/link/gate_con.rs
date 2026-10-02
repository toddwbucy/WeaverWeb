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

/// **Asks waiting behind the in-flight bound are bounded too.** Each holds a
/// request no longer than the gate's line bound (an ask past it is answered
/// at once), so the queue costs at most this many lines. An ask arriving to
/// a full queue is answered at once with the fault `busy`, gate-con's own
/// kind rather than the gate's.
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
}

fn default_turns_in_flight() -> usize {
    DEFAULT_TURNS_IN_FLIGHT
}

impl GateConConfig {
    /// Read the config under the trust rule of `client::read_private`, and
    /// refuse one minted for the admin plane, one missing a member, or one
    /// naming no turn in flight.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = client::read_private(path)?;
        let cfg: Self =
            toml::from_str(&content).map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        cfg.link
            .expect_plane(Plane::Gate)
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
        if cfg.turns_in_flight == 0 {
            anyhow::bail!(
                "{}: turns_in_flight is 0, and at least one turn must be relayed",
                path.display()
            );
        }
        Ok(cfg)
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
/// `client::run` with this plane's serve.
pub async fn run(
    cfg: GateConConfig,
    backoff: Backoff,
    shutdown: watch::Receiver<bool>,
    status: &watch::Sender<LinkStatus>,
) -> anyhow::Result<()> {
    let link = Link::new(cfg.link.clone())?;
    let adapter = GateAdapter::new(&cfg.gate_socket);
    let agent = cfg.link.agent.clone();
    let bound = cfg.turns_in_flight;
    let serve_shutdown = shutdown.clone();
    client::run(
        &link,
        || FromClient::Hello {
            agent: agent.clone(),
            plane: Plane::Gate,
            tail: None,
        },
        backoff,
        shutdown,
        status,
        |conn| serve(conn, adapter.clone(), bound, serve_shutdown.clone()),
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
    mut shutdown: watch::Receiver<bool>,
) -> Ended {
    let mut in_flight: JoinSet<FromClient> = JoinSet::new();
    let mut waiting: VecDeque<(u64, String)> = VecDeque::new();
    let ended = relay(
        &mut conn,
        &adapter,
        bound,
        &mut shutdown,
        &mut in_flight,
        &mut waiting,
    )
    .await;
    if matches!(ended, Ended::Shutdown) {
        // **Shutdown lets turns in flight finish within a grace**, their
        // answers sent while the link still stands; asks still waiting are
        // dropped, and the server answers their callers as not connected
        // when the link closes.
        let finished = tokio::time::timeout(SHUTDOWN_GRACE, async {
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
    conn.close().await;
    ended
}

async fn relay(
    conn: &mut Connection,
    adapter: &GateAdapter,
    bound: usize,
    shutdown: &mut watch::Receiver<bool>,
    in_flight: &mut JoinSet<FromClient>,
    waiting: &mut VecDeque<(u64, String)>,
) -> Ended {
    if *shutdown.borrow_and_update() {
        return Ended::Shutdown;
    }
    loop {
        // **At most `bound` turns in flight, the rest waiting in arrival
        // order.** The gate serializes turns for the agent anyway; the
        // bound is this process's protection against an unbounded task set.
        while in_flight.len() < bound
            && let Some((id, text)) = waiting.pop_front()
        {
            let adapter = adapter.clone();
            in_flight.spawn(async move { answer(id, adapter.turn(&text).await) });
        }
        tokio::select! {
            _ = shutdown.changed() => return Ended::Shutdown,
            Some(done) = in_flight.join_next(), if !in_flight.is_empty() => {
                let frame = match done {
                    Ok(frame) => frame,
                    // A failed exchange loses its ask's id: the connection
                    // ends, so the server fails that ask rather than holding
                    // it for an answer that cannot come.
                    Err(e) => return Ended::Lost(format!("a gate exchange failed: {e}")),
                };
                if let Err(why) = conn.send(frame).await {
                    return Ended::Lost(why);
                }
            }
            incoming = conn.recv() => match incoming {
                Incoming::Frame(ToClient::Turn { id, text }) => {
                    // An ask past the gate's bound is answered at once and
                    // never queued, so a waiting entry costs one gate line.
                    if let Err(e) = gate::request_line(&text) {
                        if let Err(why) = conn.send(answer(id, Err(e))).await {
                            return Ended::Lost(why);
                        }
                    } else if waiting.len() >= WAITING_BOUND {
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
                        if let Err(why) = conn.send(busy).await {
                            return Ended::Lost(why);
                        }
                    } else {
                        waiting.push_back((id, text));
                    }
                }
                Incoming::Frame(other) => {
                    let what = match other {
                        ToClient::HelloAnswer { .. } => "hello_answer",
                        ToClient::Verb { .. } => "verb",
                        ToClient::Ack { .. } => "ack",
                        ToClient::Turn { .. } | ToClient::Refusal { .. } => "turn",
                    };
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
