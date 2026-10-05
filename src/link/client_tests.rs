//! Act 3: gate-con against a fake gate and the real listener, and the
//! client half against a fake server where the real one cannot be made to
//! misbehave. No agent is reached: the gate is a `UnixListener` in a
//! temporary directory speaking the gate's line shapes.

use super::Authority;
use super::client::{Backoff, Connect, Incoming, Link, LinkConfig, LinkStatus};
use super::frames::{FromClient, LineReader, Plane, Refusal, ToClient};
use super::gate_con::{self, GateConConfig};
use super::listener::{Listener, TurnError};
use super::tests::{Lab, SILENCE, SOON, lab_config};
use crate::adapters::gate::{CLOSE_BOUND, GateClose};
use crate::store::AgentId;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, UnixListener};
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// A backoff fast enough for a test: the cap is what a credential refusal
/// waits, so it is short and still distinguishable from the base.
pub(super) const FAST: Backoff = Backoff {
    base: Duration::from_millis(20),
    cap: Duration::from_millis(400),
};

/// What the fake gate saw.
#[derive(Default)]
struct GateSeen {
    /// Exchanges open now, and the most at once.
    now: AtomicUsize,
    max: AtomicUsize,
    /// Request texts in the order their exchanges reached the gate.
    started: Mutex<Vec<String>>,
    /// Held exchanges whose client closed before the answer.
    abandoned: AtomicUsize,
}

/// A fake gate: one request line in, one close line out, the shape chosen
/// by the request's text.
struct FakeGate {
    _dir: tempfile::TempDir,
    path: PathBuf,
    seen: Arc<GateSeen>,
    task: JoinHandle<()>,
}

impl Drop for FakeGate {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn close_line(text: &str) -> Vec<u8> {
    let mut line = serde_json::to_vec(
        &json!({"kind": "answered", "run": "run-1", "turn": "turn-1", "text": text}),
    )
    .unwrap();
    line.push(b'\n');
    line
}

impl FakeGate {
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gate.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let seen = Arc::new(GateSeen::default());
        let state = seen.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let state = state.clone();
                tokio::spawn(async move {
                    let (read, mut write) = stream.into_split();
                    let mut reader = BufReader::new(read);
                    let mut line = String::new();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    let request: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
                    let text = request["text"].as_str().unwrap().to_owned();
                    state.started.lock().unwrap().push(text.clone());
                    let n = state.now.fetch_add(1, Ordering::SeqCst) + 1;
                    state.max.fetch_max(n, Ordering::SeqCst);
                    let reply: Option<Vec<u8>> = match text.as_str() {
                        "length" => {
                            let mut l = serde_json::to_vec(&json!({
                                "kind": "answered", "run": "run-1", "turn": "turn-2",
                                "text": "cut", "finish": "length"
                            }))
                            .unwrap();
                            l.push(b'\n');
                            Some(l)
                        }
                        "stopped" => {
                            let mut l = serde_json::to_vec(&json!({
                                "kind": "stopped",
                                "reason": "the working structure holds a hole"
                            }))
                            .unwrap();
                            l.push(b'\n');
                            Some(l)
                        }
                        "malformed" => Some(b"not json\n".to_vec()),
                        "nonutf8" => Some(b"\xff\xfe{}\n".to_vec()),
                        "cut" => Some(
                            br#"{"kind":"answered","run":"run-1","turn":"turn-1","text":"cut"}"#
                                .to_vec(),
                        ),
                        "hangup" => None,
                        "toolong" => Some(vec![b'x'; CLOSE_BOUND + 16]),
                        "big" => Some(close_line(&"y".repeat(900 * 1024))),
                        t if t.starts_with("hold:") => {
                            let ms: u64 = t.split(':').nth(1).unwrap().parse().unwrap();
                            let mut probe = [0u8; 1];
                            tokio::select! {
                                _ = tokio::time::sleep(Duration::from_millis(ms)) => Some(close_line(t)),
                                _ = reader.read(&mut probe) => {
                                    state.abandoned.fetch_add(1, Ordering::SeqCst);
                                    None
                                }
                            }
                        }
                        other => Some(close_line(other)),
                    };
                    if let Some(bytes) = reply {
                        let _ = write.write_all(&bytes).await;
                    }
                    state.now.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        Self {
            _dir: dir,
            path,
            seen,
            task,
        }
    }

    fn now(&self) -> usize {
        self.seen.now.load(Ordering::SeqCst)
    }
}

/// gate-con run in-process.
struct Running {
    stop: watch::Sender<bool>,
    status: watch::Receiver<LinkStatus>,
    task: JoinHandle<anyhow::Result<()>>,
}

impl Running {
    fn start(cfg: GateConConfig, source: Option<PathBuf>, backoff: Backoff) -> Self {
        let (stop, shutdown) = watch::channel(false);
        let (status_tx, status) = watch::channel(LinkStatus::default());
        let task =
            tokio::spawn(
                async move { gate_con::run(cfg, source, backoff, shutdown, &status_tx).await },
            );
        Self { stop, status, task }
    }

    /// From the installed config at its path, re-read at a capped retry.
    fn from_file(path: &Path, backoff: Backoff) -> Self {
        Self::start(
            GateConConfig::load(path).unwrap(),
            Some(path.to_owned()),
            backoff,
        )
    }

    async fn wait(&mut self, what: &str, cond: impl Fn(&LinkStatus) -> bool) -> LinkStatus {
        let reached = tokio::time::timeout(SOON, self.status.wait_for(|s| cond(s)))
            .await
            .ok()
            .and_then(|r| r.ok().map(|s| s.clone()));
        match reached {
            Some(s) => s,
            None => panic!("gate-con never reached {what}: {:?}", self.status()),
        }
    }

    fn status(&self) -> LinkStatus {
        self.status.borrow().clone()
    }

    async fn stop(self) {
        let _ = self.stop.send(true);
        tokio::time::timeout(SOON * 2, self.task)
            .await
            .expect("gate-con stops on shutdown")
            .unwrap()
            .unwrap();
    }
}

/// An agent registered by the verbs, and its gate config as `register`
/// wrote it with `gate_socket` (and a bound, where given) added the way an
/// install would.
async fn installed(
    lab: &Lab,
    name: &str,
    gate: &Path,
    out: &Path,
    turns_in_flight: Option<usize>,
) -> (AgentId, PathBuf) {
    let cfg = lab_config(lab);
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        name,
        out,
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let id: AgentId = answer.value["agent"].as_str().unwrap().parse().unwrap();
    let path = PathBuf::from(answer.value["configs"][0].as_str().unwrap());
    assert!(path.ends_with("gate-con.toml"));
    let mut extra = format!(
        "gate_socket = {}\n",
        toml::Value::String(gate.display().to_string())
    );
    if let Some(n) = turns_in_flight {
        extra.push_str(&format!("turns_in_flight = {n}\n"));
    }
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut file, extra.as_bytes()).unwrap();
    (id, path)
}

/// A turn through the listener, bounded so a test that would hang fails
/// instead, holding no serial guard forever.
async fn turn(listener: &Listener, id: &AgentId, text: &str) -> Result<GateClose, TurnError> {
    tokio::time::timeout(Duration::from_secs(20), listener.turn(id, text))
        .await
        .unwrap_or_else(|_| panic!("the turn {text:?} was never answered"))
}

/// The first turn after an admission: the server routes asks only to a
/// connection whose hello it has answered, which the client can see a
/// moment before the server marks it, so a not-connected answer is asked
/// again within the bound.
async fn first_turn(lab: &Lab, id: &AgentId, text: &str) -> Result<GateClose, TurnError> {
    let until = tokio::time::Instant::now() + SOON;
    loop {
        match turn(&lab.listener, id, text).await {
            Err(TurnError::NotConnected) if tokio::time::Instant::now() < until => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            other => return other,
        }
    }
}

fn fault_kind(result: Result<GateClose, TurnError>) -> String {
    match result {
        Err(TurnError::Gate(f)) => f.kind,
        other => panic!("expected a typed fault, got {other:?}"),
    }
}

/// **A turn crosses the link through gate-con and the close comes back
/// intact, and each gate failure comes back as its typed fault.** The
/// answered close keeps its members and the whole line as `raw`; a cut one
/// carries `finish`; the agent's stop naming no turn carries its `reason`;
/// a malformed close, a hang-up, a close past the buffer
/// cap, a request past the gate's bound and an absent socket each answer
/// their kind. Nothing of the interior is reported: the row's tuple and
/// load state stay empty.
#[tokio::test]
async fn gate_con_relays_a_turn_and_each_gate_failure_as_its_typed_fault() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;

    let close = first_turn(&lab, &id, "hello there").await.unwrap();
    assert_eq!(close.kind, "answered");
    assert_eq!(close.run.as_deref(), Some("run-1"));
    assert_eq!(close.turn.as_deref(), Some("turn-1"));
    assert_eq!(close.text.as_deref(), Some("hello there"));
    assert_eq!(close.finish, None);
    assert_eq!(
        close.raw,
        json!({"kind": "answered", "run": "run-1", "turn": "turn-1", "text": "hello there"})
    );

    let cut = turn(&lab.listener, &id, "length").await.unwrap();
    assert_eq!(cut.finish.as_deref(), Some("length"));
    assert_eq!(cut.turn.as_deref(), Some("turn-2"));

    // The agent's own stop, naming no turn, carries its `reason` across the
    // link in place of `text` (WeaverAgent#59).
    let stopped = turn(&lab.listener, &id, "stopped").await.unwrap();
    assert_eq!(stopped.kind, "stopped");
    assert_eq!(stopped.turn, None);
    assert_eq!(stopped.text, None);
    assert_eq!(
        stopped.reason.as_deref(),
        Some("the working structure holds a hole")
    );

    for (text, kind) in [
        ("malformed", "bad_close"),
        ("nonutf8", "bad_close"),
        ("cut", "bad_close"),
        ("hangup", "delivery_lost"),
        ("toolong", "close_too_long"),
    ] {
        assert_eq!(
            fault_kind(turn(&lab.listener, &id, text).await),
            kind,
            "{text}"
        );
    }
    let long = "x".repeat(40 * 1024);
    assert_eq!(
        fault_kind(turn(&lab.listener, &id, &long).await),
        "line_too_long"
    );
    assert!(
        !gate.seen.started.lock().unwrap().contains(&long),
        "a request past the bound never reaches the gate"
    );
    std::fs::remove_file(&gate.path).unwrap();
    assert_eq!(
        fault_kind(turn(&lab.listener, &id, "answer").await),
        "unloaded"
    );

    let row = lab.agent(&id).await;
    assert!(row.tuple.is_none(), "gate-con reports no tuple");
    assert!(row.load_state.is_none(), "gate-con reports no load state");
    con.stop().await;
}

/// **A revoked credential retries at the cap and never faster, and gate-con
/// does not exit.** The revocation closes the link; the reconnection is
/// refused `not_live`; every attempt after it waits at least the cap.
#[tokio::test]
async fn a_revoked_gate_con_retries_at_the_cap() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;

    let row = lab.agent(&id).await;
    lab.store
        .revoke_credential(&row, Plane::Gate, Some("lab"))
        .await
        .unwrap();
    let status = con
        .wait("refused not_live", |s| {
            s.last_refusal == Some(Refusal::NotLive)
        })
        .await;
    assert!(!status.admitted);
    assert!(
        status.next_delay.unwrap() >= FAST.cap,
        "{:?}",
        status.next_delay
    );
    let before = con.status().attempts;
    tokio::time::sleep(FAST.cap * 3 + FAST.cap / 4).await;
    let after = con.status();
    let attempts = after.attempts - before;
    assert!(
        (1..=3).contains(&attempts),
        "{attempts} attempts in three caps: at the cap, never faster"
    );
    assert_eq!(after.last_refusal, Some(Refusal::NotLive));
    assert!(
        after.next_delay.unwrap() >= FAST.cap,
        "{:?}",
        after.next_delay
    );
    assert!(!con.task.is_finished(), "gate-con never exits on a refusal");
    con.stop().await;
}

/// **A restarted listener is reconnected to and answers the next turn, and
/// the turn in flight when the link dropped does not outlive it**: its
/// gate exchange is closed, which the fake gate sees as its client gone.
#[tokio::test]
async fn gate_con_reconnects_after_the_listener_restarts() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "answer").await.unwrap();

    let listener = lab.listener.clone();
    let asked = id.clone();
    let pending = tokio::spawn(async move { turn(&listener, &asked, "hold:5000").await });
    let until = tokio::time::Instant::now() + SOON;
    while gate.now() == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the held turn reached the gate"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // The server goes with the turn in flight.
    // Stopped rather than crashed: its connections are torn down, so the
    // ask in flight fails rather than waiting on a process that is gone,
    // and it fails as sent and unanswered, its outcome unknown.
    let address = lab.listener.address();
    lab.listener.stop().await;
    assert!(matches!(pending.await.unwrap(), Err(TurnError::Unanswered)));
    let until = tokio::time::Instant::now() + SOON;
    while gate.seen.abandoned.load(Ordering::SeqCst) == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the gate exchange outlived its link"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // The same address again, as a supervisor's restart would bring it.
    let until = tokio::time::Instant::now() + SOON;
    lab.listener = loop {
        match Listener::start(
            lab.store.clone(),
            &lab.authority,
            &address.to_string(),
            SILENCE,
        )
        .await
        {
            Ok(listener) => break listener,
            Err(e) if tokio::time::Instant::now() < until => {
                let _ = e;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("the listener restarts on its address: {e:#}"),
        }
    };
    con.wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    let close = first_turn(&lab, &id, "after the restart").await.unwrap();
    assert_eq!(close.text.as_deref(), Some("after the restart"));
    con.stop().await;
}

/// **A second gate-con on one credential is refused `already_connected`
/// while the first stands, and keeps retrying rather than exiting**; the
/// first still relays.
#[tokio::test]
async fn a_second_gate_con_on_one_credential_is_refused_and_keeps_retrying() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut first = Running::from_file(&path, FAST);
    first.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "answer").await.unwrap();

    let mut second = Running::from_file(&path, FAST);
    second
        .wait("refused already_connected", |s| {
            s.last_refusal == Some(Refusal::AlreadyConnected)
        })
        .await;
    let before = second.status().attempts;
    second
        .wait("a further attempt", |s| s.attempts > before)
        .await;
    assert!(!second.task.is_finished(), "never exits on a refusal");
    assert_eq!(second.status().admissions, 0);
    let close = turn(&lab.listener, &id, "still the first").await.unwrap();
    assert_eq!(close.text.as_deref(), Some("still the first"));
    second.stop().await;
    first.stop().await;
}

/// **Turns past the in-flight bound wait in arrival order and all answer.**
/// Eight held asks against a bound of two: at most two reach the gate at
/// once, and they reach it in the order they were asked.
#[tokio::test]
async fn turns_past_the_in_flight_bound_wait_in_arrival_order() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), Some(2)).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "warm").await.unwrap();
    gate.seen.started.lock().unwrap().clear();
    gate.seen.max.store(0, Ordering::SeqCst);

    let mut asks = Vec::new();
    let mut texts = Vec::new();
    for i in 0..8 {
        let text = format!("hold:250:{i}");
        texts.push(text.clone());
        let listener = lab.listener.clone();
        let asked = id.clone();
        asks.push(tokio::spawn(
            async move { turn(&listener, &asked, &text).await },
        ));
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    for (ask, text) in asks.into_iter().zip(&texts) {
        let close = ask.await.unwrap().unwrap();
        assert_eq!(close.text.as_deref(), Some(text.as_str()));
    }
    assert_eq!(gate.seen.max.load(Ordering::SeqCst), 2, "the bound held");
    assert_eq!(*gate.seen.started.lock().unwrap(), texts, "arrival order");
    con.stop().await;
}

/// **Shutdown lets a turn in flight finish, then closes the link.**
#[tokio::test]
async fn shutdown_lets_a_turn_in_flight_finish() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "answer").await.unwrap();

    let listener = lab.listener.clone();
    let asked = id.clone();
    let pending = tokio::spawn(async move { turn(&listener, &asked, "hold:600").await });
    let until = tokio::time::Instant::now() + SOON;
    while gate.now() == 0 {
        assert!(tokio::time::Instant::now() < until);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let _ = con.stop.send(true);
    let close = pending.await.unwrap().unwrap();
    assert_eq!(close.text.as_deref(), Some("hold:600"));
    tokio::time::timeout(SOON, con.task)
        .await
        .expect("gate-con returns after the grace")
        .unwrap()
        .unwrap();
    assert_eq!(gate.seen.abandoned.load(Ordering::SeqCst), 0);
    lab.wait_for(&id, "gate down", |a| !a.gate.connected).await;
}

/// **A turn abandoned at gate-con's shutdown has an unknown outcome**: a
/// turn held past the grace is aborted when gate-con stops, and its caller
/// is answered `Unanswered`, never `NotConnected`, since its request
/// crossed to the gate and the agent runs it to its end.
#[tokio::test]
async fn a_turn_abandoned_at_shutdown_is_answered_as_unknown() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "answer").await.unwrap();

    let listener = lab.listener.clone();
    let asked = id.clone();
    let held = format!(
        "hold:{}",
        (gate_con::SHUTDOWN_GRACE + Duration::from_secs(5)).as_millis()
    );
    let pending = tokio::spawn(async move { turn(&listener, &asked, &held).await });
    let until = tokio::time::Instant::now() + SOON;
    while gate.now() == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the turn reached the gate"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let _ = con.stop.send(true);
    let answered = tokio::time::timeout(SOON * 2, pending)
        .await
        .expect("the caller is answered after the grace")
        .unwrap();
    assert!(
        matches!(answered, Err(TurnError::Unanswered)),
        "{answered:?}"
    );
    tokio::time::timeout(SOON, con.task)
        .await
        .expect("gate-con returns after the grace")
        .unwrap()
        .unwrap();
}

/// **An ask still waiting at shutdown is answered `not_started`**: one
/// turn in flight fills the bound and one waits behind it; gate-con stops,
/// the waiting caller is told its turn never reached the gate, and the
/// turn in flight finishes within the grace with its close.
#[tokio::test]
async fn a_turn_still_waiting_at_shutdown_is_answered_not_started() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), Some(1)).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "answer").await.unwrap();

    let listener = lab.listener.clone();
    let asked = id.clone();
    let running = tokio::spawn(async move { turn(&listener, &asked, "hold:800").await });
    let until = tokio::time::Instant::now() + SOON;
    while gate.now() == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the turn reached the gate"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let listener = lab.listener.clone();
    let asked = id.clone();
    let queued = tokio::spawn(async move { turn(&listener, &asked, "never").await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = con.stop.send(true);

    match queued.await.unwrap() {
        Err(TurnError::Gate(fault)) => assert_eq!(fault.kind, gate_con::NOT_STARTED),
        other => panic!("expected not_started, got {other:?}"),
    }
    let close = running.await.unwrap().unwrap();
    assert_eq!(close.text.as_deref(), Some("hold:800"));
    assert!(
        !gate
            .seen
            .started
            .lock()
            .unwrap()
            .contains(&"never".to_owned()),
        "the waiting turn never reached the gate"
    );
    tokio::time::timeout(SOON, con.task)
        .await
        .expect("gate-con returns after the grace")
        .unwrap()
        .unwrap();
}

/// A fake server: the authority's TLS, one connection, the hello read and
/// answered with the given cadence, and the rest left to the test.
pub(super) struct FakeServer {
    pub(super) _dir: tempfile::TempDir,
    pub(super) authority: Authority,
    pub(super) listener: TcpListener,
}

pub(super) type ServerStream = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;

impl FakeServer {
    pub(super) async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        Self {
            _dir: dir,
            authority,
            listener,
        }
    }

    pub(super) fn link(&self, plane: Plane) -> LinkConfig {
        let credential = self.authority.mint_client("karl", plane).unwrap();
        LinkConfig {
            server: self.listener.local_addr().unwrap().to_string(),
            server_name: "weaver-web".into(),
            agent: "karl".into(),
            agent_id: "ag-0000000000000001".into(),
            plane,
            server_certificate: self.authority.certificate_pem().to_owned(),
            certificate: credential.certificate_pem,
            key: credential.key_pem,
        }
    }

    /// Accept one connection, read its hello, answer it.
    pub(super) async fn admit(
        &self,
        cadence_secs: u64,
    ) -> (
        LineReader<tokio::io::ReadHalf<ServerStream>>,
        tokio::io::WriteHalf<ServerStream>,
    ) {
        self.admit_with(cadence_secs, None).await
    }

    /// As `admit`, with the accepted socket's receive buffer set, so a
    /// slow reader stands for a slow link and the bytes the path holds are
    /// few.
    pub(super) async fn admit_with(
        &self,
        cadence_secs: u64,
        receive_buffer: Option<usize>,
    ) -> (
        LineReader<tokio::io::ReadHalf<ServerStream>>,
        tokio::io::WriteHalf<ServerStream>,
    ) {
        let acceptor = tokio_rustls::TlsAcceptor::from(self.authority.server_tls().unwrap());
        let (tcp, _) = self.listener.accept().await.unwrap();
        if let Some(bytes) = receive_buffer {
            use std::os::fd::AsRawFd;
            let value = bytes as libc::c_int;
            // SAFETY: a valid descriptor and a c_int option of its size.
            let set = unsafe {
                libc::setsockopt(
                    tcp.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_RCVBUF,
                    &value as *const libc::c_int as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                )
            };
            assert_eq!(set, 0, "SO_RCVBUF");
        }
        let stream = acceptor.accept(tcp).await.unwrap();
        let (read, mut write) = tokio::io::split(stream);
        let mut reader = LineReader::new(read);
        match reader.next().await {
            super::frames::Line::Frame(line) => {
                assert!(matches!(
                    serde_json::from_str::<FromClient>(&line).unwrap(),
                    FromClient::Hello { .. }
                ));
            }
            other => panic!("expected the hello, got {other:?}"),
        }
        let mut answer = serde_json::to_vec(&ToClient::HelloAnswer {
            cadence_secs,
            acknowledged: None,
        })
        .unwrap();
        answer.push(b'\n');
        write.write_all(&answer).await.unwrap();
        write.flush().await.unwrap();
        (reader, write)
    }
}

/// **A server line past the bound ends the connection, without the client
/// buffering it**: the fake server streams bytes with no delimiter for as
/// long as the client takes them, and the client refuses the line having
/// taken little more than the bound.
#[tokio::test]
async fn a_server_line_past_the_bound_ends_the_connection() {
    let server = FakeServer::start().await;
    let link = Link::new(server.link(Plane::Gate)).unwrap();
    let streamed = async {
        let (_reader, mut write) = server.admit(15).await;
        let chunk = vec![b'x'; 64 * 1024];
        let mut total = 0usize;
        while total < 64 * 1024 * 1024 {
            if tokio::time::timeout(Duration::from_secs(5), write.write_all(&chunk))
                .await
                .map(|r| r.is_err())
                .unwrap_or(true)
            {
                break;
            }
            total += chunk.len();
        }
        total
    };
    let client = async {
        let Connect::Admitted(mut conn) = link
            .connect(FromClient::Hello {
                agent: "karl".into(),
                plane: Plane::Gate,
                tail: None,
                ceiling: None,
                door: None,
            })
            .await
        else {
            panic!("admitted by the fake server");
        };
        let ended = tokio::time::timeout(Duration::from_secs(20), conn.recv())
            .await
            .expect("the client ends the connection");
        conn.close().await;
        ended
    };
    let (total, ended) = tokio::join!(streamed, client);
    match ended {
        Incoming::Lost(why) => assert!(why.contains("passed the bound"), "{why}"),
        other => panic!("expected the connection lost, got {other:?}"),
    }
    assert!(
        total < 32 * 1024 * 1024,
        "the server streamed {total} bytes before the client stopped taking them"
    );
}

/// **A write the server does not take within the cadence ends the
/// connection.** The fake server answers the hello with a one-second
/// cadence, asks for turns whose closes are large, and never reads again:
/// once the socket's buffers fill, a write stalls past the cadence and
/// gate-con ends the connection for that reason rather than hanging.
#[tokio::test]
async fn a_write_the_server_does_not_take_within_the_cadence_ends_the_connection() {
    let server = FakeServer::start().await;
    let gate = FakeGate::start();
    let cfg = GateConConfig {
        link: server.link(Plane::Gate),
        gate_socket: gate.path.clone(),
        turns_in_flight: 4,
        waiting_bound: gate_con::WAITING_BOUND,
    };
    let slow = Backoff {
        base: Duration::from_secs(30),
        cap: Duration::from_secs(60),
    };
    let mut con = Running::start(cfg, None, slow);
    let (_reader, mut write) = server.admit(1).await;
    for id in 0..48u64 {
        let mut ask = serde_json::to_vec(&ToClient::Turn {
            id,
            text: "big".into(),
        })
        .unwrap();
        ask.push(b'\n');
        write.write_all(&ask).await.unwrap();
    }
    write.flush().await.unwrap();
    let reached = tokio::time::timeout(
        Duration::from_secs(20),
        con.status.wait_for(|s| s.last_end.is_some()),
    )
    .await
    .ok()
    .and_then(|r| r.ok().map(|s| s.clone()));
    let Some(status) = reached else {
        panic!("the connection never ended: {:?}", con.status());
    };
    // The writer's own bound ends it: the queue behind it, which the
    // heartbeat and the answers enqueue into under the same cadence, is a
    // second layer that would only fire after the queue filled.
    let why = status.last_end.unwrap();
    assert!(
        why.contains("a write did not complete within the cadence"),
        "{why}"
    );
    assert!(!status.admitted);
    con.stop().await;
}

fn config_text(server: &FakeServer, plane: Plane, gate: Option<&str>, extra: &str) -> String {
    let link = server.link(plane);
    let mut table = toml::Table::new();
    table.insert("server".into(), link.server.into());
    table.insert("server_name".into(), link.server_name.into());
    table.insert("agent".into(), link.agent.into());
    table.insert("agent_id".into(), link.agent_id.into());
    table.insert("plane".into(), plane.as_str().into());
    table.insert("server_certificate".into(), link.server_certificate.into());
    table.insert("certificate".into(), link.certificate.into());
    table.insert("key".into(), link.key.into());
    if let Some(gate) = gate {
        table.insert("gate_socket".into(), gate.into());
    }
    format!("{}{extra}", toml::to_string(&table).unwrap())
}

fn write_mode(path: &Path, content: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, content).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// **A config that is not private, not a regular file, a symlink, minted
/// for the admin plane, or missing a member is refused at start**, each
/// naming what was found. (A config owned by another uid is refused by the
/// same descriptor check, which a test without root cannot stage.)
#[tokio::test]
async fn a_config_that_is_not_trusted_is_refused_at_start() {
    let server = FakeServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("gate-con.toml");
    write_mode(
        &good,
        &config_text(&server, Plane::Gate, Some("/gate"), ""),
        0o600,
    );
    let cfg = GateConConfig::load(&good).unwrap();
    assert_eq!(cfg.turns_in_flight, gate_con::DEFAULT_TURNS_IN_FLIGHT);
    assert_eq!(cfg.gate_socket, PathBuf::from("/gate"));
    assert!(
        !format!("{cfg:?}").contains("PRIVATE KEY"),
        "the key is never printed"
    );

    let refused = |path: &Path| GateConConfig::load(path).unwrap_err().to_string();
    for mode in [0o640, 0o604, 0o700] {
        let path = dir.path().join(format!("mode-{mode:o}.toml"));
        write_mode(
            &path,
            &config_text(&server, Plane::Gate, Some("/gate"), ""),
            mode,
        );
        let why = refused(&path);
        assert!(why.contains(&format!("{mode:04o}")), "{why}");
    }

    let link = dir.path().join("linked.toml");
    std::os::unix::fs::symlink(&good, &link).unwrap();
    assert!(refused(&link).contains("symlink"));

    let directory = dir.path().join("a-directory.toml");
    std::fs::create_dir(&directory).unwrap();
    assert!(refused(&directory).contains("not a regular file"));

    let admin = dir.path().join("admin.toml");
    write_mode(
        &admin,
        &config_text(&server, Plane::Admin, Some("/gate"), ""),
        0o600,
    );
    assert!(refused(&admin).contains("admin plane"));

    let missing = dir.path().join("missing.toml");
    write_mode(
        &missing,
        &config_text(&server, Plane::Gate, None, ""),
        0o600,
    );
    assert!(refused(&missing).contains("gate_socket"));

    let many = dir.path().join("many-in-flight.toml");
    write_mode(
        &many,
        &config_text(
            &server,
            Plane::Gate,
            Some("/gate"),
            "turns_in_flight = 65\n",
        ),
        0o600,
    );
    assert!(refused(&many).contains("outside 1 to 64"));

    // A corrupted line names its number and never its bytes: the file
    // carries a key.
    let corrupt = dir.path().join("corrupt.toml");
    write_mode(
        &corrupt,
        &config_text(
            &server,
            Plane::Gate,
            Some("/gate"),
            "SECRETMARKERnotakey = = \"MIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n",
        ),
        0o600,
    );
    let why = refused(&corrupt);
    assert!(why.contains("line "), "{why}");
    assert!(
        !why.contains("SECRETMARKER") && !why.contains("MIIEvQ"),
        "{why}"
    );

    // A key pasted into an integer member is named by its member and line,
    // and its value never appears.
    let pasted = dir.path().join("pasted.toml");
    write_mode(
        &pasted,
        &config_text(
            &server,
            Plane::Gate,
            Some("/gate"),
            "turns_in_flight = \"MIIEvQIBADANBgkqhkiG9w0BAQEFAASCSECRETMARKER\"\n",
        ),
        0o600,
    );
    let why = refused(&pasted);
    assert!(why.contains("turns_in_flight"), "{why}");
    assert!(why.contains("line "), "{why}");
    assert!(
        !why.contains("SECRETMARKER") && !why.contains("MIIEvQ"),
        "{why}"
    );

    let none = dir.path().join("none-in-flight.toml");
    write_mode(
        &none,
        &config_text(&server, Plane::Gate, Some("/gate"), "turns_in_flight = 0\n"),
        0o600,
    );
    assert!(refused(&none).contains("turns_in_flight"));
}

/// **The heartbeat keeps an idle link admitted past the server's silence
/// bound.** With a four-second bound the cadence is one second; gate-con
/// stays on one admission through six idle seconds and still relays.
#[tokio::test]
async fn heartbeats_keep_an_idle_link_up() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let status = con.status();
    assert!(status.admitted, "{status:?}");
    assert_eq!(status.admissions, 1, "one admission throughout: {status:?}");
    let close = turn(&lab.listener, &id, "still up").await.unwrap();
    assert_eq!(close.text.as_deref(), Some("still up"));
    con.stop().await;
}

/// **A config re-installed at its path is picked up at the next retry at
/// the cap, with no restart.** The credential is revoked and gate-con
/// retries at the cap; the agent is rotated and its new config, with the
/// gate socket added as an install would, is written over the file; the
/// next capped attempt reads it and is admitted.
#[tokio::test]
async fn a_reinstalled_config_is_picked_up_at_the_next_capped_retry() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;

    let row = lab.agent(&id).await;
    lab.store
        .revoke_credential(&row, Plane::Gate, Some("lab"))
        .await
        .unwrap();
    con.wait("refused not_live", |s| {
        s.last_refusal == Some(Refusal::NotLive)
    })
    .await;

    let rotated = tempfile::tempdir().unwrap();
    let answer = super::verbs::rotate(
        &lab.store,
        &lab_config(&lab),
        &lab.authority,
        id.as_str(),
        rotated.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let fresh = PathBuf::from(answer.value["configs"][0].as_str().unwrap());
    let content = format!(
        "{}gate_socket = {}\n",
        std::fs::read_to_string(&fresh).unwrap(),
        toml::Value::String(gate.path.display().to_string())
    );
    let staged = path.with_extension("installing");
    write_mode(&staged, &content, 0o600);
    std::fs::rename(&staged, &path).unwrap();

    con.wait("admitted on the re-installed credential", |s| {
        s.admitted && s.admissions >= 2
    })
    .await;
    let close = first_turn(&lab, &id, "on the new credential")
        .await
        .unwrap();
    assert_eq!(close.text.as_deref(), Some("on the new credential"));
    con.stop().await;
}

/// **A config for another agent re-installed at the path is refused**:
/// only the link's members change at a re-install, and the agent is told
/// by its row's identity. gate-con's credential is revoked and it retries
/// at the cap; another box's agent of the same name, then another name,
/// has its gate config written over the file; the capped retries read
/// each, refuse it, and keep dialing with the credential in hand, so
/// neither other row is ever connected through a gate-con bound to this
/// one's socket.
#[tokio::test]
async fn a_reinstalled_config_for_another_agent_is_refused() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), None).await;
    let twin_out = tempfile::tempdir().unwrap();
    let (twin, twin_path) = installed(&lab, "karl", &gate.path, twin_out.path(), None).await;
    let other_out = tempfile::tempdir().unwrap();
    let (other, other_path) = installed(&lab, "kevin", &gate.path, other_out.path(), None).await;
    let mut con = Running::from_file(&path, FAST);
    con.wait("admitted", |s| s.admitted).await;

    let row = lab.agent(&id).await;
    lab.store
        .revoke_credential(&row, Plane::Gate, Some("lab"))
        .await
        .unwrap();
    con.wait("refused not_live", |s| {
        s.last_refusal == Some(Refusal::NotLive)
    })
    .await;
    for (foreign, foreign_path) in [(&twin, &twin_path), (&other, &other_path)] {
        let staged = path.with_extension("installing");
        write_mode(
            &staged,
            &std::fs::read_to_string(foreign_path).unwrap(),
            0o600,
        );
        std::fs::rename(&staged, &path).unwrap();
        let attempts = con.status().attempts;
        con.wait("three capped retries", |s| s.attempts >= attempts + 3)
            .await;
        let status = con.status();
        assert!(!status.admitted, "{status:?}");
        assert_eq!(status.last_refusal, Some(Refusal::NotLive), "{status:?}");
        assert_eq!(status.admissions, 1, "{status:?}");
        assert!(!lab.agent(foreign).await.gate.connected);
    }
    con.stop().await;
}

/// **An ask past the waiting bound is answered `busy` at once**, gate-con's
/// own back-pressure: one turn in flight and two waiting, so of five held
/// asks three answer and two are refused `busy` without reaching the gate.
#[tokio::test]
async fn asks_past_the_waiting_bound_are_answered_busy() {
    let Some(lab) = Lab::open().await else { return };
    let gate = FakeGate::start();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, "karl", &gate.path, out.path(), Some(1)).await;
    let mut cfg = GateConConfig::load(&path).unwrap();
    cfg.waiting_bound = 2;
    let mut con = Running::start(cfg, Some(path.clone()), FAST);
    con.wait("admitted", |s| s.admitted).await;
    first_turn(&lab, &id, "warm").await.unwrap();
    gate.seen.started.lock().unwrap().clear();

    let mut asks = Vec::new();
    for i in 0..5 {
        let text = format!("hold:400:{i}");
        let listener = lab.listener.clone();
        let asked = id.clone();
        asks.push(tokio::spawn(
            async move { turn(&listener, &asked, &text).await },
        ));
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    let mut kinds = Vec::new();
    for ask in asks {
        kinds.push(match ask.await.unwrap() {
            Ok(_) => "answered".to_owned(),
            Err(TurnError::Gate(f)) => f.kind,
            Err(e) => panic!("{e}"),
        });
    }
    assert_eq!(
        kinds,
        ["answered", "answered", "answered", "busy", "busy"],
        "one in flight, two waiting, the rest busy"
    );
    assert_eq!(gate.seen.started.lock().unwrap().len(), 3);
    con.stop().await;
}

/// **A hello's answer naming a cadence past a day is a protocol fault**,
/// not a timer that overflows the heartbeat.
#[tokio::test]
async fn a_hello_answer_naming_an_absurd_cadence_is_a_protocol_fault() {
    let server = FakeServer::start().await;
    let link = Link::new(server.link(Plane::Gate)).unwrap();
    let (_, attempt) = tokio::join!(
        server.admit(super::frames::CADENCE_MAX_SECS + 1),
        link.connect(FromClient::Hello {
            agent: "karl".into(),
            plane: Plane::Gate,
            tail: None,
            ceiling: None,
            door: None,
        })
    );
    match attempt {
        Connect::Failed(why) => assert!(why.contains("cadence"), "{why}"),
        Connect::Admitted(_) => panic!("admitted on an absurd cadence"),
        _ => panic!("expected a protocol fault"),
    }
}

/// **A server whose certificate the pinned authority did not sign is
/// retried at the cap**, as a credential refusal is: the server's authority
/// was rotated, and only a re-installed config can cure it.
#[tokio::test]
async fn a_server_of_another_authority_is_retried_at_the_cap() {
    let server = FakeServer::start().await;
    let other = FakeServer::start().await;
    let gate = FakeGate::start();
    // Pinned to the other authority, dialing this server.
    let mut link = other.link(Plane::Gate);
    link.server = server.listener.local_addr().unwrap().to_string();
    let acceptor = tokio_rustls::TlsAcceptor::from(server.authority.server_tls().unwrap());
    let FakeServer { listener, .. } = &server;
    let accept = async {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let _ = acceptor.accept(tcp).await;
        }
    };
    let cfg = GateConConfig {
        link,
        gate_socket: gate.path.clone(),
        turns_in_flight: 4,
        waiting_bound: gate_con::WAITING_BOUND,
    };
    let mut con = Running::start(cfg, None, FAST);
    let watched = async {
        let status = con.wait("a capped retry", |s| s.next_delay.is_some()).await;
        let why = status.last_end.unwrap();
        assert!(why.contains("authority"), "{why}");
        assert!(
            status.next_delay.unwrap() >= FAST.cap,
            "{:?}",
            status.next_delay
        );
        assert_eq!(status.admissions, 0);
    };
    tokio::select! {
        _ = accept => panic!("the accept loop ended"),
        _ = watched => {}
    }
    con.stop().await;
}

/// **A stop finishes within the grace even when the server stopped taking
/// bytes.** The fake server answers the hello with a fifteen-second cadence,
/// asks for turns with large closes and never reads again, so gate-con's
/// sends and its writer back up. A shutdown then returns within the grace,
/// not a cadence after it: the relay's sends watch shutdown, and the
/// writer's drain gets only what remains of the grace.
#[tokio::test]
async fn a_stop_finishes_within_the_grace_when_the_server_stopped_reading() {
    let server = FakeServer::start().await;
    let gate = FakeGate::start();
    let cfg = GateConConfig {
        link: server.link(Plane::Gate),
        gate_socket: gate.path.clone(),
        turns_in_flight: 4,
        waiting_bound: gate_con::WAITING_BOUND,
    };
    let slow = Backoff {
        base: Duration::from_secs(30),
        cap: Duration::from_secs(60),
    };
    let con = Running::start(cfg, None, slow);
    let (_reader, mut write) = server.admit(15).await;
    for id in 0..48u64 {
        let mut ask = serde_json::to_vec(&ToClient::Turn {
            id,
            text: "big".into(),
        })
        .unwrap();
        ask.push(b'\n');
        write.write_all(&ask).await.unwrap();
    }
    write.flush().await.unwrap();
    // Long enough for the socket's buffers to fill and the sends to back up.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let started = tokio::time::Instant::now();
    let _ = con.stop.send(true);
    tokio::time::timeout(Duration::from_secs(30), con.task)
        .await
        .expect("gate-con stops")
        .unwrap()
        .unwrap();
    let took = started.elapsed();
    assert!(
        took < gate_con::SHUTDOWN_GRACE + Duration::from_secs(2),
        "the stop took {took:?}, past the {:?} grace",
        gate_con::SHUTDOWN_GRACE
    );
}
