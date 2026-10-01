//! conforms: web-link-refuses-a-credential-not-live-before-the-roster
//! conforms: web-link-identity-is-the-certificates-binding-never-the-roster
//! conforms: web-one-live-connection-per-credential
//! conforms: web-link-state-is-reset-when-the-listener-starts
//! conforms: web-agent-present-only-when-both-planes-match-one-row
//! conforms: web-tuple-is-admins-word-and-never-gate-cons
//! conforms: web-nothing-crosses-the-link-in-the-clear
//!
//! The listener (Spec section 8): the mutual-TLS accept loop, admission
//! under the row's lock, the heartbeat, the incarnations that bind every
//! link-state write to its connection, the asks the server routes to a
//! connector, and the landing of what the admin plane reports (Spec 2.12
//! and the clauses of 7.2 the server enacts). The live window is the seed's
//! server half of `traceview.rs`, one ring per agent.

use crate::adapters::gate::GateClose;
use crate::lifecycle::VerbOutcome;
use crate::link::authority::{Authority, fingerprint};
use crate::link::frames::{FromClient, LINE_BOUND, Plane, Position, Refusal, ToClient, TurnFault};
use crate::link::register::{Observation, REVOCATION_CHANNEL};
use crate::store::{AgentId, Store};
use crate::traceview::{TraceEvent, TraceViews};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_rustls::TlsAcceptor;

/// How long a peer has to complete the handshake and to say hello. A peer
/// that speaks plaintext never completes the handshake and is dropped
/// here; nothing it sent was read as a frame.
const HANDSHAKE_SECS: u64 = 10;

/// A live connection as the listener holds it: the write path, the
/// incarnation that every link-state write about it names, the asks it has
/// not answered, and the close the revoking act pulls.
struct LiveConnection {
    incarnation: i64,
    tx: mpsc::Sender<ToClient>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<FromClient>>>>,
    close: watch::Sender<bool>,
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
    /// No gate-con is connected for this agent, or it dropped mid-ask.
    NotConnected,
}

impl std::fmt::Display for TurnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnError::Gate(e) => write!(f, "{}", e.message),
            TurnError::NotConnected => write!(f, "no gate-con is connected for this agent"),
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
        let acceptor = TlsAcceptor::from(authority.server_tls()?);
        let tcp = TcpListener::bind(listen).await?;
        let address = tcp.local_addr()?;
        let epoch = store.listener_start().await?;
        let mut notifications = sqlx::postgres::PgListener::connect_with(&store.pool).await?;
        notifications.listen(REVOCATION_CHANNEL).await?;
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
            }),
        };
        tracing::info!("link listening on {address} under epoch {epoch}");

        let weak = Arc::downgrade(&listener.inner);
        tokio::spawn(async move {
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
                tokio::spawn(serve_connection(inner, stream, peer));
            }
        });

        let weak = Arc::downgrade(&listener.inner);
        tokio::spawn(async move {
            loop {
                let notification = match notifications.recv().await {
                    Ok(n) => n,
                    Err(e) => {
                        tracing::error!("the revocation channel failed: {e}");
                        return;
                    }
                };
                let Some(inner) = weak.upgrade() else { return };
                inner.close_fingerprint(notification.payload()).await;
            }
        });
        Ok(listener)
    }

    pub fn address(&self) -> SocketAddr {
        self.inner.address
    }

    pub fn epoch(&self) -> i64 {
        self.inner.epoch
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

    /// One turn across the link (Spec 7.1): routed to the agent's gate-con.
    pub async fn turn(&self, agent: &AgentId, text: &str) -> Result<GateClose, TurnError> {
        let text = text.to_owned();
        match self
            .inner
            .ask(agent, Plane::Gate, |id| ToClient::Turn { id, text })
            .await
        {
            Some(FromClient::Turn { close: Some(c), .. }) => Ok(c),
            Some(FromClient::Turn { error: Some(e), .. }) => Err(TurnError::Gate(e)),
            _ => Err(TurnError::NotConnected),
        }
    }

    /// One verb across the link (Spec 7.2): routed to the agent's
    /// admin-con, whose answer lands on the row where it is `show` or
    /// `list` before it is answered here.
    pub async fn verb(&self, agent: &AgentId, verb: &str) -> anyhow::Result<VerbOutcome> {
        let v = verb.to_owned();
        match self
            .inner
            .ask(agent, Plane::Admin, |id| ToClient::Verb { id, verb: v })
            .await
        {
            Some(FromClient::Verb {
                outcome: Some(o), ..
            }) => Ok(o),
            Some(FromClient::Verb { error: Some(e), .. }) => anyhow::bail!("{e}"),
            _ => anyhow::bail!("no admin-con is connected for this agent"),
        }
    }
}

impl Inner {
    async fn ask(
        &self,
        agent: &AgentId,
        plane: Plane,
        make: impl FnOnce(u64) -> ToClient,
    ) -> Option<FromClient> {
        let id = self.next_ask.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        let (tx, pending) = {
            let live = self.live.lock().unwrap();
            let conn = live.get(&(agent.clone(), plane))?;
            (conn.tx.clone(), conn.pending.clone())
        };
        pending.lock().unwrap().insert(id, reply_tx);
        if tx.send(make(id)).await.is_err() {
            pending.lock().unwrap().remove(&id);
            return None;
        }
        // A torn-down connection drops its pending map, which fails this
        // await; a cancelled caller leaves an entry that teardown drops.
        reply_rx.await.ok()
    }

    /// The revoking act's close (Spec 8): a notification names a
    /// fingerprint, and the connection installed for it, if any, is closed
    /// now rather than at the next hello.
    async fn close_fingerprint(&self, fingerprint: &str) {
        let Ok(Some((agent, plane))) = self.store.agent_by_fingerprint(fingerprint).await else {
            return;
        };
        let close = {
            let live = self.live.lock().unwrap();
            live.get(&(agent.agent_id.clone(), plane))
                .map(|c| c.close.clone())
        };
        if let Some(close) = close {
            tracing::info!(
                "credential {} of {} ({plane}) revoked: closing its live connection",
                &fingerprint[..12],
                agent.agent_id
            );
            let _ = close.send(true);
        }
    }

    fn next_arrival(&self) -> i64 {
        self.arrival.fetch_add(1, Ordering::Relaxed) + 1
    }
}

/// Read one frame line under the bound. `None` is the end of the
/// connection or a line that left the framing.
async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Option<String> {
    buf.clear();
    let mut taken = 0usize;
    loop {
        let n = reader.read_until(b'\n', buf).await.ok()?;
        if n == 0 {
            return None;
        }
        taken += n;
        if buf.last() == Some(&b'\n') {
            break;
        }
        if taken > LINE_BOUND {
            return None;
        }
    }
    if buf.len() > LINE_BOUND + 1 {
        return None;
    }
    let s = std::str::from_utf8(buf).ok()?;
    Some(s.trim_end().to_owned())
}

async fn send(tx: &mpsc::Sender<ToClient>, frame: ToClient) {
    let _ = tx.send(frame).await;
}

async fn serve_connection(inner: Arc<Inner>, stream: TcpStream, peer: SocketAddr) {
    // **The handshake is the only door**: a peer that does not complete
    // mutual TLS with a certificate this authority signed is dropped here,
    // and nothing it wrote was read as a frame.
    let tls = match tokio::time::timeout(
        Duration::from_secs(HANDSHAKE_SECS),
        inner.acceptor.accept(stream),
    )
    .await
    {
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
        let _ = write_half.shutdown().await;
    });
    let mut reader = BufReader::new(read_half);
    let mut buf = Vec::new();

    // **The lookup comes before the roster** (Spec 8): the fingerprint is
    // looked up in the register and a credential that is absent or
    // revoked is refused here, before any byte of the hello is read.
    let (agent, plane) = match inner.store.agent_by_fingerprint(&fp).await {
        Ok(Some((agent, plane)))
            if agent.credential(plane).state == crate::link::register::CredentialState::Live =>
        {
            (agent, plane)
        }
        Ok(Some((agent, plane))) => {
            tracing::warn!(
                "link from {peer}: credential {} of {} ({plane}) is revoked, refused before its roster",
                &fp[..12],
                agent.agent_id
            );
            send(
                &tx,
                ToClient::Refusal {
                    reason: Refusal::NotLive,
                },
            )
            .await;
            drop(tx);
            let _ = writer.await;
            return;
        }
        Ok(None) => {
            tracing::warn!(
                "link from {peer}: credential {} is not in the register, refused before its roster",
                &fp[..12]
            );
            send(
                &tx,
                ToClient::Refusal {
                    reason: Refusal::NotLive,
                },
            )
            .await;
            drop(tx);
            let _ = writer.await;
            return;
        }
        Err(e) => {
            tracing::error!("link from {peer}: the register could not be read: {e:#}");
            drop(tx);
            let _ = writer.await;
            return;
        }
    };

    let hello = tokio::time::timeout(inner.silence, read_line(&mut reader, &mut buf)).await;
    let (name, said_plane, tail) = match hello {
        Ok(Some(line)) => match serde_json::from_str::<FromClient>(&line) {
            Ok(FromClient::Hello { agent, plane, tail }) => (agent, plane, tail),
            _ => {
                send(
                    &tx,
                    ToClient::Refusal {
                        reason: Refusal::Malformed,
                    },
                )
                .await;
                drop(tx);
                let _ = writer.await;
                return;
            }
        },
        _ => {
            drop(tx);
            let _ = writer.await;
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
        send(
            &tx,
            ToClient::Refusal {
                reason: Refusal::RosterMismatch,
            },
        )
        .await;
        drop(tx);
        let _ = writer.await;
        return;
    }

    let incarnation = (inner.epoch << 32) | inner.next_incarnation.fetch_add(1, Ordering::Relaxed);
    let pending = Arc::new(Mutex::new(HashMap::new()));
    let (close_tx, mut close_rx) = watch::channel(false);
    let key = (agent.agent_id.clone(), plane);
    let admitted = inner
        .store
        .admit(
            &agent.agent_id,
            plane,
            incarnation,
            &peer.to_string(),
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
                        incarnation,
                        tx: tx.clone(),
                        pending: pending.clone(),
                        close: close_tx.clone(),
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
            send(&tx, ToClient::Refusal { reason: refusal }).await;
            drop(tx);
            let _ = writer.await;
            return;
        }
        Err(e) => {
            tracing::error!("link from {peer}: admission could not be written: {e:#}");
            drop(tx);
            let _ = writer.await;
            return;
        }
    }
    tracing::info!(
        "link from {peer}: {} ({plane}) admitted as incarnation {incarnation}",
        agent.agent_id
    );

    // **The replay boundary is fixed from the hello before anything else
    // is read** (Spec 7.2, 2.12), then the hello is answered, then `show`
    // is asked, before the connection's first event is accepted.
    let boundary = tail.clone();
    let acknowledged = inner.acknowledged(&agent.agent_id);
    send(
        &tx,
        ToClient::HelloAnswer {
            cadence_secs: (inner.silence.as_secs() / 4).max(1),
            acknowledged,
        },
    )
    .await;
    if plane == Plane::Admin {
        inner.windows.ensure(agent.agent_id.as_str());
        if inner.windows.has_events(agent.agent_id.as_str()) {
            inner.windows.mark(
                agent.agent_id.as_str(),
                "admin-con reconnected: a replay follows",
            );
        }
        let id = inner.next_ask.fetch_add(1, Ordering::Relaxed);
        send(
            &tx,
            ToClient::Verb {
                id,
                verb: "show".into(),
            },
        )
        .await;
    }

    loop {
        let line = tokio::select! {
            l = tokio::time::timeout(inner.silence, read_line(&mut reader, &mut buf)) => l,
            _ = close_rx.changed() => {
                if *close_rx.borrow() {
                    tracing::info!("link from {peer}: {} ({plane}) closed by revocation", agent.agent_id);
                    break;
                }
                continue;
            }
        };
        let line = match line {
            Ok(Some(line)) => line,
            Ok(None) => break,
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
                break;
            }
        };
        match (plane, frame) {
            (_, FromClient::Heartbeat) => {}
            (Plane::Gate, FromClient::Turn { id, close, error }) => {
                resolve(&pending, id, FromClient::Turn { id, close, error });
            }
            (Plane::Admin, FromClient::Verb { id, outcome, error }) => {
                if let Some(outcome) = &outcome {
                    inner.land_verb(&agent.agent_id, &agent.name, outcome).await;
                }
                resolve(&pending, id, FromClient::Verb { id, outcome, error });
            }
            (
                Plane::Admin,
                FromClient::Event {
                    position,
                    replayed,
                    event,
                },
            ) => {
                inner
                    .land_event(
                        &agent.agent_id,
                        boundary.as_ref(),
                        &position,
                        replayed,
                        event,
                    )
                    .await;
                send(&tx, ToClient::Ack { position }).await;
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
                break;
            }
        }
    }

    // **Teardown bound to the incarnation, under the row's lock** (Spec 8).
    let torn = inner
        .store
        .teardown(&agent.agent_id, plane, incarnation, || {
            let mut live = inner.live.lock().unwrap();
            if live.get(&key).map(|c| c.incarnation) == Some(incarnation) {
                live.remove(&key);
            }
        })
        .await;
    match torn {
        Ok(true) => {
            if plane == Plane::Admin {
                inner
                    .windows
                    .mark(agent.agent_id.as_str(), "link to admin-con lost");
            }
        }
        Ok(false) => {
            // Not the live incarnation any more: a revocation or a
            // replacement already wrote this plane, and this write would
            // have marked it missing while it relays.
            let mut live = inner.live.lock().unwrap();
            if live.get(&key).map(|c| c.incarnation) == Some(incarnation) {
                live.remove(&key);
            }
        }
        Err(e) => tracing::error!("link from {peer}: teardown could not be written: {e:#}"),
    }
    pending.lock().unwrap().clear();
    drop(tx);
    let _ = writer.await;
    tracing::info!(
        "link from {peer}: {} ({plane}) incarnation {incarnation} closed",
        agent.agent_id
    );
}

fn frame_name(frame: &FromClient) -> &'static str {
    match frame {
        FromClient::Hello { .. } => "hello",
        FromClient::Heartbeat => "heartbeat",
        FromClient::Turn { .. } => "turn",
        FromClient::Verb { .. } => "verb",
        FromClient::Event { .. } => "event",
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

impl Inner {
    fn acknowledged(&self, agent: &AgentId) -> Option<Position> {
        self.acknowledged.lock().unwrap().get(agent).cloned()
    }

    /// **A `show` or `list` answer is admin's word** (Spec 7.2, 2.12) and
    /// lands on the row under the arrival sequence. Of a `list` answer only
    /// the summary for this connection's own row lands; the others write
    /// nothing. Any other verb's answer lands nothing.
    async fn land_verb(&self, agent: &AgentId, name: &str, outcome: &VerbOutcome) {
        let Some(answer) = &outcome.answer else {
            return;
        };
        let summary = match outcome.verb.as_str() {
            "show" if answer.get("kind").and_then(|k| k.as_str()) == Some("state") => {
                Some(answer.clone())
            }
            "list" if answer.get("kind").and_then(|k| k.as_str()) == Some("agents") => answer
                .get("agents")
                .and_then(|a| a.as_array())
                .and_then(|rows| {
                    rows.iter()
                        .find(|r| r.get("name").and_then(|n| n.as_str()) == Some(name))
                })
                .cloned(),
            _ => None,
        };
        let Some(summary) = summary else { return };
        let observation = Observation {
            load_state: summary
                .get("state")
                .and_then(|s| s.as_str())
                .map(str::to_owned),
            tuple: summary.get("load").cloned().filter(|l| !l.is_null()),
            at: chrono::Utc::now(),
        };
        self.land(agent, observation).await;
    }

    /// **An event feeds the window, and only a live load or unload writes
    /// the row** (Spec 7.2, 2.12). The server classifies by its own
    /// boundary, the hello's tail: behind it is replayed whatever the
    /// client's flag says, and a replayed event writes no member of the
    /// row. An event of another generation than the boundary's is live
    /// unless the client marked it replayed, a rotation after the hello
    /// being live and a backfill from an older file being history.
    async fn land_event(
        &self,
        agent: &AgentId,
        boundary: Option<&Position>,
        position: &Position,
        replayed: bool,
        event: TraceEvent,
    ) {
        let behind = match boundary {
            Some(b) if b.generation == position.generation => position.offset < b.offset,
            Some(_) => replayed,
            None => replayed,
        };
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
        self.windows.ingest(agent.as_str(), event);
        self.acknowledged
            .lock()
            .unwrap()
            .insert(agent.clone(), position.clone());
        if behind {
            return;
        }
        let observation = match kind.as_deref() {
            // A load event means the agent was admitted and stands idle;
            // its payload is the declared tuple the trace carries, the
            // declaration's digest among it.
            Some("load") => Observation {
                load_state: Some("idle".into()),
                tuple: payload,
                at: chrono::Utc::now(),
            },
            Some("unload") => Observation {
                load_state: Some("unloaded".into()),
                tuple: None,
                at: chrono::Utc::now(),
            },
            _ => return,
        };
        self.land(agent, observation).await;
    }

    async fn land(&self, agent: &AgentId, observation: Observation) {
        let arrival = self.next_arrival();
        match self
            .store
            .land_observation(agent, &observation, self.epoch, arrival)
            .await
        {
            Ok(true) => {}
            Ok(false) => tracing::debug!("{agent}: an observation ordered below the stored one"),
            Err(e) => tracing::error!("{agent}: an observation could not be landed: {e:#}"),
        }
    }
}
