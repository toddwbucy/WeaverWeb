//! The link (Spec section 8): NDJSON frames over one TCP connection
//! the connector dials. Seven services - turn, verb, trace, status,
//! declaration, trace_runs, trace_run - ask and answer correlated by
//! id where the shape is ask-answer, the trace streaming unasked. Link
//! loss is marked, never smoothed: pending asks fail typed, and the
//! server inserts discontinuity marks into every trace view.
//!
//! **Section 8 charters the link and names none of its frames.** The
//! words hello, backfill and discontinuity appear nowhere in that Spec,
//! and roster appears once in another sense, so every citation of
//! section 8 in this module names the link and never the frame. The
//! frames are this module's, and a later act that charters them is
//! where those citations stop needing this paragraph.

use crate::adapters::gate::{GateAdapter, GateClose, GateError};
use crate::config::ConnectorConfig;
use crate::lifecycle::{self, VerbOutcome};
use crate::traceview::{self, TraceEvent};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};

/// Frames the server sends the connector: the ask half of the
/// ask-answer services.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "svc", rename_all = "snake_case")]
pub enum ToConnector {
    Turn {
        id: u64,
        agent: String,
        text: String,
    },
    Verb {
        id: u64,
        agent: String,
        verb: String,
    },
    Status {
        id: u64,
    },
    Declaration {
        id: u64,
        agent: String,
    },
    TraceRuns {
        id: u64,
        agent: String,
    },
    TraceRun {
        id: u64,
        agent: String,
        run: String,
    },
}

/// One run as the sink file carries it: the inventory row of the
/// `trace_runs` service. Spec section 8 charters the link and names no
/// service, so this module is where its services are named.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub run: String,
    pub turns: u32,
    pub events: u32,
    pub first_wall_ms: Option<i64>,
}

/// The gate adapter's error, carried over the link with its typing
/// intact (Spec section 8: section 7.1's variants verbatim in kind).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGateError {
    pub kind: String,
    pub message: String,
}

impl From<&GateError> for WireGateError {
    fn from(e: &GateError) -> Self {
        let kind = match e {
            GateError::Unloaded => "unloaded",
            GateError::LineTooLong(_) => "line_too_long",
            GateError::DeliveryLost(_) => "delivery_lost",
            GateError::BadClose(_) => "bad_close",
            GateError::CloseTooLong(_) => "close_too_long",
        };
        Self {
            kind: kind.into(),
            message: e.to_string(),
        }
    }
}

/// Frames the connector sends the server: the hello, the answers, and
/// the unasked trace stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "svc", rename_all = "snake_case")]
pub enum ToServer {
    /// First frame on every connection: the box's agent roster by
    /// name. Paths stay on the box (Spec section 8).
    Hello {
        agents: Vec<String>,
    },
    Turn {
        id: u64,
        close: Option<GateClose>,
        error: Option<WireGateError>,
    },
    Verb {
        id: u64,
        outcome: Option<VerbOutcome>,
        error: Option<String>,
    },
    Status {
        id: u64,
        agents: HashMap<String, bool>,
    },
    Declaration {
        id: u64,
        path: String,
        content: String,
    },
    TraceRuns {
        id: u64,
        runs: Vec<RunSummary>,
    },
    TraceRun {
        id: u64,
        events: Vec<serde_json::Value>,
        /// True when the cap cut the answer short - stated, never
        /// silent (Spec section 8).
        truncated: bool,
    },
    Trace {
        agent: String,
        event: TraceEvent,
    },
}

/// What a turn ask can fail with on the server side: the gate's own
/// typed error relayed, or the link itself being down.
#[derive(Debug, Clone)]
pub enum TurnError {
    Gate(WireGateError),
    LinkDown,
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnError::Gate(e) => write!(f, "{}", e.message),
            TurnError::LinkDown => {
                write!(f, "agent unreachable: the link to the agents' box is down")
            }
        }
    }
}

/// How long the server waits on the small asks (status, declaration)
/// before reporting the link unresponsive. Turns and verbs carry no
/// server-side deadline: the gate serializes turns and a queued turn
/// legitimately waits (Spec section 7.1), and the verb's 300 s ceiling
/// is the connector's (Spec section 7.2).
const SMALL_ASK_TIMEOUT_SECS: u64 = 10;

/// Events the link surfaces to the composition root.
#[derive(Debug, Clone)]
pub enum LinkEvent {
    /// A connector said hello: the agents this hello admitted (names
    /// already homed to another live box are skipped, first wins).
    Hello(Vec<String>),
    /// A trace event or mark arrived for an agent.
    Trace { agent: String, event: TraceEvent },
    /// A box's connection dropped: these agents are now unreachable.
    /// Their pending asks have already failed.
    Down(Vec<String>),
}

type ConnId = u64;

struct LinkInner {
    /// Every live connection's write path, one per box.
    connections: Mutex<HashMap<ConnId, mpsc::Sender<ToConnector>>>,
    /// Which connection each agent answers on. First hello wins a
    /// name; a collision from another box is skipped and logged.
    homes: std::sync::RwLock<HashMap<String, ConnId>>,
    /// Each connection's admitted roster, in announcement order, for
    /// the union roster and for teardown.
    rosters: Mutex<Vec<(ConnId, Vec<String>)>>,
    pending: Mutex<HashMap<u64, (ConnId, oneshot::Sender<ToServer>)>>,
    next_ask: AtomicU64,
    next_conn: AtomicU64,
}

/// The server's handle on the links: routed asks, the union roster,
/// and liveness. One server, many boxes, each box's connector dialing
/// in with its own roster - the fleet shape.
#[derive(Clone)]
pub struct Link {
    inner: Arc<LinkInner>,
}

impl Link {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(LinkInner {
                connections: Mutex::new(HashMap::new()),
                homes: std::sync::RwLock::new(HashMap::new()),
                rosters: Mutex::new(Vec::new()),
                pending: Mutex::new(HashMap::new()),
                next_ask: AtomicU64::new(1),
                next_conn: AtomicU64::new(1),
            }),
        }
    }

    pub fn is_up(&self) -> bool {
        !self.inner.connections.lock().unwrap().is_empty()
    }

    /// The union roster, box by box in announcement order.
    pub async fn roster(&self) -> Vec<String> {
        self.inner
            .rosters
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, r)| r.iter().cloned())
            .collect()
    }

    pub async fn has_agent(&self, name: &str) -> bool {
        self.inner.homes.read().unwrap().contains_key(name)
    }

    fn conn_for(&self, agent: &str) -> Option<(ConnId, mpsc::Sender<ToConnector>)> {
        let conn = *self.inner.homes.read().unwrap().get(agent)?;
        let tx = self.inner.connections.lock().unwrap().get(&conn)?.clone();
        Some((conn, tx))
    }

    /// One ask, routed to the connection that homes the agent.
    async fn ask(
        &self,
        agent: &str,
        make: impl FnOnce(u64) -> ToConnector,
    ) -> Result<ToServer, ()> {
        let (conn, sender) = self.conn_for(agent).ok_or(())?;
        self.ask_on(conn, sender, make).await
    }

    async fn ask_on(
        &self,
        conn: ConnId,
        sender: mpsc::Sender<ToConnector>,
        make: impl FnOnce(u64) -> ToConnector,
    ) -> Result<ToServer, ()> {
        let id = self.inner.next_ask.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .unwrap()
            .insert(id, (conn, reply_tx));
        // The guard makes every exit from this frame remove the entry:
        // an answered ask already removed it (the remove is then a
        // no-op), and a timeout or a cancelled caller drops this future
        // at an await point, where nothing else would clean up - a
        // hung-but-connected box would otherwise grow one stranded
        // entry per timed-out ask until its connection finally died.
        struct Cleanup<'a>(&'a LinkInner, u64);
        impl Drop for Cleanup<'_> {
            fn drop(&mut self) {
                self.0.pending.lock().unwrap().remove(&self.1);
            }
        }
        let _cleanup = Cleanup(&self.inner, id);
        if sender.send(make(id)).await.is_err() {
            return Err(());
        }
        // A dropped connection fails its own pending asks and no
        // other box's, so this await ends when the answer or that
        // box's disconnect does.
        reply_rx.await.map_err(|_| ())
    }

    /// One turn across the link: the agent's own connector dials the
    /// gate per turn (Spec section 7.1) and answers with the close or
    /// the typed error.
    pub async fn turn(&self, agent: &str, text: &str) -> Result<GateClose, TurnError> {
        let (a, text) = (agent.to_owned(), text.to_owned());
        match self
            .ask(agent, |id| ToConnector::Turn { id, agent: a, text })
            .await
        {
            Ok(ToServer::Turn { close: Some(c), .. }) => Ok(c),
            Ok(ToServer::Turn { error: Some(e), .. }) => Err(TurnError::Gate(e)),
            _ => Err(TurnError::LinkDown),
        }
    }

    /// One verb invocation across the link (Spec section 7.2).
    pub async fn verb(&self, agent: &str, verb: &str) -> anyhow::Result<VerbOutcome> {
        let (a, verb) = (agent.to_owned(), verb.to_owned());
        match self
            .ask(agent, |id| ToConnector::Verb { id, agent: a, verb })
            .await
        {
            Ok(ToServer::Verb {
                outcome: Some(o), ..
            }) => Ok(o),
            Ok(ToServer::Verb { error: Some(e), .. }) => anyhow::bail!("{e}"),
            _ => anyhow::bail!("the link to this agent's box is down"),
        }
    }

    /// The load-state observable, merged across every box: each live
    /// connection answers for its own agents. An agent absent from
    /// the merge has an unresponsive box, which the caller renders as
    /// unreachable. None when no box is connected at all.
    pub async fn status(&self) -> Option<HashMap<String, bool>> {
        let conns: Vec<(ConnId, mpsc::Sender<ToConnector>)> = self
            .inner
            .connections
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        if conns.is_empty() {
            return None;
        }
        // Every box is asked concurrently: the fleet shape makes N
        // greater than one, and a page awaiting this must wait one
        // timeout, not one per unresponsive box.
        let asks = conns.into_iter().map(|(conn, tx)| async move {
            tokio::time::timeout(
                std::time::Duration::from_secs(SMALL_ASK_TIMEOUT_SECS),
                self.ask_on(conn, tx, |id| ToConnector::Status { id }),
            )
            .await
        });
        let mut merged = HashMap::new();
        for answer in futures::future::join_all(asks).await {
            if let Ok(Ok(ToServer::Status { agents, .. })) = answer {
                merged.extend(agents);
            }
        }
        Some(merged)
    }

    async fn small_ask(
        &self,
        agent: &str,
        make: impl FnOnce(u64) -> ToConnector,
    ) -> Option<ToServer> {
        let fut = self.ask(agent, make);
        match tokio::time::timeout(std::time::Duration::from_secs(SMALL_ASK_TIMEOUT_SECS), fut)
            .await
        {
            Ok(Ok(answer)) => Some(answer),
            _ => None,
        }
    }

    /// The agent declaration, read on its own box: (path, content).
    pub async fn declaration(&self, agent: &str) -> Option<(String, String)> {
        let a = agent.to_owned();
        match self
            .small_ask(agent, |id| ToConnector::Declaration { id, agent: a })
            .await
        {
            Some(ToServer::Declaration { path, content, .. }) => Some((path, content)),
            _ => None,
        }
    }

    /// The run inventory from the agent's sink file, this module's
    /// `trace_runs` service over the link that Spec section 8 charters.
    pub async fn trace_runs(&self, agent: &str) -> Option<Vec<RunSummary>> {
        let a = agent.to_owned();
        match self
            .small_ask(agent, |id| ToConnector::TraceRuns { id, agent: a })
            .await
        {
            Some(ToServer::TraceRuns { runs, .. }) => Some(runs),
            _ => None,
        }
    }

    /// One run's events from the sink file, capped with the truncation
    /// stated, this module's `trace_run` service over the link that Spec
    /// section 8 charters.
    pub async fn trace_run(
        &self,
        agent: &str,
        run: &str,
    ) -> Option<(Vec<serde_json::Value>, bool)> {
        let (a, run) = (agent.to_owned(), run.to_owned());
        match self
            .small_ask(agent, |id| ToConnector::TraceRun { id, agent: a, run })
            .await
        {
            Some(ToServer::TraceRun {
                events, truncated, ..
            }) => Some((events, truncated)),
            _ => None,
        }
    }

    /// Drop one connection and everything homed to it. Returns the
    /// agents that just became unreachable.
    fn teardown(&self, conn: ConnId) -> Vec<String> {
        self.inner.connections.lock().unwrap().remove(&conn);
        self.inner
            .pending
            .lock()
            .unwrap()
            .retain(|_, (c, _)| *c != conn);
        let mut lost = Vec::new();
        self.inner.rosters.lock().unwrap().retain(|(c, r)| {
            if *c == conn {
                lost = r.clone();
                false
            } else {
                true
            }
        });
        let mut homes = self.inner.homes.write().unwrap();
        for a in &lost {
            homes.remove(a);
        }
        lost
    }

    /// Admit a hello's roster for a connection: first hello wins each
    /// name, a collision from another live box is skipped and logged.
    /// Returns the admitted names.
    fn admit(&self, conn: ConnId, announced: Vec<String>) -> Vec<String> {
        let mut homes = self.inner.homes.write().unwrap();
        // A repeated hello on the same connection re-announces: clear
        // this connection's prior homes first.
        homes.retain(|_, c| *c != conn);
        let mut admitted = Vec::new();
        for a in announced {
            if let Some(other) = homes.get(&a) {
                tracing::warn!(
                    "agent '{a}' already homed to connection {other}, skipping (first wins)"
                );
                continue;
            }
            homes.insert(a.clone(), conn);
            admitted.push(a);
        }
        drop(homes);
        let mut rosters = self.inner.rosters.lock().unwrap();
        rosters.retain(|(c, _)| *c != conn);
        rosters.push((conn, admitted.clone()));
        admitted
    }
}

impl Default for Link {
    fn default() -> Self {
        Self::new()
    }
}

/// The server's accept loop: one connection per box, any number of
/// boxes, each served concurrently. A box's drop tears down exactly
/// its own agents, pending asks, and roster - the other boxes never
/// notice.
pub async fn serve(link: Link, listener: TcpListener, events: mpsc::Sender<LinkEvent>) {
    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::error!("link accept failed: {e}");
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        let conn = link.inner.next_conn.fetch_add(1, Ordering::Relaxed);
        tracing::info!("link {conn} connected from {peer}");
        let (link, events) = (link.clone(), events.clone());
        tokio::spawn(async move {
            serve_connection(&link, conn, stream, &events).await;
            let lost = link.teardown(conn);
            tracing::info!(
                "link {conn} from {peer} closed, {} agent(s) lost",
                lost.len()
            );
            if !lost.is_empty() {
                let _ = events.send(LinkEvent::Down(lost)).await;
            }
        });
    }
}

async fn serve_connection(
    link: &Link,
    conn: ConnId,
    stream: TcpStream,
    events: &mpsc::Sender<LinkEvent>,
) {
    let (read_half, mut write_half) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<ToConnector>(64);
    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let Ok(mut line) = serde_json::to_string(&frame) else {
                continue;
            };
            line.push('\n');
            if write_half.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
    });

    let mut reader = BufReader::new(read_half).lines();
    let mut said_hello = false;
    while let Ok(Some(line)) = reader.next_line().await {
        let frame: ToServer = match serde_json::from_str(&line) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("link {conn} frame did not parse, closing: {e}");
                break;
            }
        };
        // The first frame must be the hello (Spec section 8); only a
        // greeted connection gets the write path.
        if !said_hello {
            match frame {
                ToServer::Hello { agents } => {
                    said_hello = true;
                    link.inner
                        .connections
                        .lock()
                        .unwrap()
                        .insert(conn, tx.clone());
                    let admitted = link.admit(conn, agents);
                    let _ = events.send(LinkEvent::Hello(admitted)).await;
                    continue;
                }
                _ => {
                    tracing::error!("link {conn} spoke before hello, closing");
                    break;
                }
            }
        }
        match frame {
            ToServer::Hello { agents } => {
                // A repeated hello re-announces this box's roster.
                let admitted = link.admit(conn, agents);
                let _ = events.send(LinkEvent::Hello(admitted)).await;
            }
            ToServer::Trace { agent, event } => {
                let _ = events.send(LinkEvent::Trace { agent, event }).await;
            }
            ToServer::Turn { id, .. }
            | ToServer::Verb { id, .. }
            | ToServer::Status { id, .. }
            | ToServer::Declaration { id, .. }
            | ToServer::TraceRuns { id, .. }
            | ToServer::TraceRun { id, .. } => {
                if let Some((_, reply)) = link.inner.pending.lock().unwrap().remove(&id) {
                    let _ = reply.send(frame);
                }
            }
        }
    }
    writer.abort();
}

/// The connector's whole life: dial the server, say hello, tail the
/// traces, answer asks, and redial with backoff when the link drops.
/// Reconnection re-runs the hello and a fresh trace backfill (Spec
/// section 8, and the frame rule in this module's header).
pub async fn connector_run(cfg: Arc<ConnectorConfig>) {
    let gates: Arc<HashMap<String, GateAdapter>> = Arc::new(
        cfg.agents
            .iter()
            .map(|a| (a.name.clone(), GateAdapter::new(&a.gate)))
            .collect(),
    );
    let mut backoff = 1u64;
    loop {
        match TcpStream::connect(&cfg.server).await {
            Ok(stream) => {
                tracing::info!("link established to {}", cfg.server);
                backoff = 1;
                connector_connection(&cfg, gates.clone(), stream).await;
                tracing::warn!("link to {} lost", cfg.server);
            }
            Err(e) => {
                tracing::warn!("link dial to {} failed: {e}", cfg.server);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connector_connection(
    cfg: &ConnectorConfig,
    gates: Arc<HashMap<String, GateAdapter>>,
    stream: TcpStream,
) {
    let (read_half, mut write_half) = stream.into_split();
    let (tx, mut rx) = mpsc::channel::<ToServer>(256);

    let writer = tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let Ok(mut line) = serde_json::to_string(&frame) else {
                continue;
            };
            line.push('\n');
            if write_half.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
    });

    let hello = ToServer::Hello {
        agents: cfg.agents.iter().map(|a| a.name.clone()).collect(),
    };
    if tx.send(hello).await.is_err() {
        writer.abort();
        return;
    }

    // Fresh tailers per connection: fresh backfill (Spec section 8,
    // and this module's frame rule). The server marks the reconnect,
    // so the re-read is bracketed.
    let mut tasks: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    for a in &cfg.agents {
        let (agent, path, tx) = (a.name.clone(), a.trace.clone(), tx.clone());
        tasks.push(tokio::spawn(async move {
            let (ev_tx, mut ev_rx) = mpsc::channel::<TraceEvent>(256);
            tokio::spawn(traceview::tail_task(path, ev_tx));
            while let Some(event) = ev_rx.recv().await {
                if tx
                    .send(ToServer::Trace {
                        agent: agent.clone(),
                        event,
                    })
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }));
    }

    let trace_paths: Arc<HashMap<String, std::path::PathBuf>> = Arc::new(
        cfg.agents
            .iter()
            .map(|a| (a.name.clone(), a.trace.clone()))
            .collect(),
    );

    let mut reader = BufReader::new(read_half).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        let frame: ToConnector = match serde_json::from_str(&line) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("link ask did not parse, closing: {e}");
                break;
            }
        };
        // Every ask runs concurrently: a turn holds the gate for
        // minutes while status must answer now. Answers serialize
        // through the writer's channel.
        let (gates, cfg_decl, tx) = (gates.clone(), cfg.agent_declarations.clone(), tx.clone());
        let (admin_bin, admin_config, admin_env) = (
            cfg.admin_bin.clone(),
            cfg.admin_config.clone(),
            cfg.admin_env,
        );
        let trace_paths = trace_paths.clone();
        // Answered asks' handles are dropped as new asks arrive, so a
        // long-lived connection does not accumulate them.
        tasks.retain(|t| !t.is_finished());
        tasks.push(tokio::spawn(async move {
            let answer = match frame {
                ToConnector::Turn { id, agent, text } => match gates.get(&agent) {
                    Some(gate) => match gate.turn(&text).await {
                        Ok(close) => ToServer::Turn {
                            id,
                            close: Some(close),
                            error: None,
                        },
                        Err(e) => ToServer::Turn {
                            id,
                            close: None,
                            error: Some(WireGateError::from(&e)),
                        },
                    },
                    None => ToServer::Turn {
                        id,
                        close: None,
                        error: Some(WireGateError {
                            kind: "no_such_agent".into(),
                            message: format!("no agent '{agent}' on this box"),
                        }),
                    },
                },
                ToConnector::Verb { id, agent, verb } => {
                    match lifecycle::run_verb(&verb, &agent, &admin_bin, &admin_config, admin_env)
                        .await
                    {
                        Ok(outcome) => ToServer::Verb {
                            id,
                            outcome: Some(outcome),
                            error: None,
                        },
                        Err(e) => ToServer::Verb {
                            id,
                            outcome: None,
                            error: Some(e.to_string()),
                        },
                    }
                }
                ToConnector::Status { id } => ToServer::Status {
                    id,
                    agents: gates
                        .iter()
                        .map(|(name, gate)| (name.clone(), gate.socket_exists()))
                        .collect(),
                },
                ToConnector::Declaration { id, agent } => {
                    let path = cfg_decl.join(format!("{agent}.yaml"));
                    let content = match tokio::fs::read_to_string(&path).await {
                        Ok(c) => c,
                        Err(e) => format!("could not read the declaration: {e}"),
                    };
                    ToServer::Declaration {
                        id,
                        path: path.display().to_string(),
                        content,
                    }
                }
                // Both record reads stream the sink line by line: the
                // file grows for as long as the agent lives, and a
                // whole-file read would hold it all at once.
                ToConnector::TraceRuns { id, agent } => {
                    let mut runs: Vec<RunSummary> = Vec::new();
                    if let Some(path) = trace_paths.get(&agent)
                        && let Ok(file) = tokio::fs::File::open(path).await
                    {
                        let mut lines = BufReader::new(file).lines();
                        let mut order: Vec<String> = Vec::new();
                        let mut map: HashMap<String, RunSummary> = HashMap::new();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                                continue;
                            };
                            let Some(run) = v.get("run").and_then(|r| r.as_str()) else {
                                continue;
                            };
                            let entry = map.entry(run.to_owned()).or_insert_with(|| {
                                order.push(run.to_owned());
                                RunSummary {
                                    run: run.to_owned(),
                                    turns: 0,
                                    events: 0,
                                    first_wall_ms: None,
                                }
                            });
                            entry.events += 1;
                            if entry.first_wall_ms.is_none() {
                                entry.first_wall_ms = v.get("wall_ms").and_then(|w| w.as_i64());
                            }
                            if v.get("kind").and_then(|k| k.as_str()) == Some("turn.closed") {
                                entry.turns += 1;
                            }
                        }
                        runs = order.into_iter().filter_map(|r| map.remove(&r)).collect();
                    }
                    ToServer::TraceRuns { id, runs }
                }
                ToConnector::TraceRun { id, agent, run } => {
                    const RUN_CAP: usize = 10_000;
                    let mut events = Vec::new();
                    let mut truncated = false;
                    if let Some(path) = trace_paths.get(&agent)
                        && let Ok(file) = tokio::fs::File::open(path).await
                    {
                        let mut lines = BufReader::new(file).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                                continue;
                            };
                            if v.get("run").and_then(|r| r.as_str()) == Some(run.as_str()) {
                                if events.len() == RUN_CAP {
                                    truncated = true;
                                    break;
                                }
                                events.push(v);
                            }
                        }
                    }
                    ToServer::TraceRun {
                        id,
                        events,
                        truncated,
                    }
                }
            };
            let _ = tx.send(answer).await;
        }));
    }

    // The connection is gone: everything serving it dies with it. An
    // in-flight turn's gate connection drops, and the record holds the
    // close (Spec section 7.1).
    for t in &tasks {
        t.abort();
    }
    writer.abort();
}
