//! admin-con against a fake trace relay serving a temporary trace file
//! (`fake_relay`) and the real listener, with a fake invoker where a verb
//! must run, and against a fake server where the real one cannot be made to
//! ask outside the ceiling. No agent is reached and no verb is invoked for
//! real.

use super::admin_con::{self, AdminConConfig, Invoker, NoVerbs};
use super::client::{Backoff, LinkStatus};
use super::client_tests::{FAST, FakeServer};
use super::fake_relay::FakeRelay;
use super::frames::{FromClient, Line, Plane, Principal, ToClient, VerbFault, VerbOutcome};
use super::listener::{Listener, VerbError};
use super::tests::{Lab, SILENCE, SOON, lab_config};
use crate::store::AgentId;
use crate::traceview::TraceEvent;
use serde_json::json;
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Long enough for anything admin-con would wrongly send to have arrived.
const SETTLE: Duration = Duration::from_millis(400);

/// The stop's grace in these tests, where the service's default covers a
/// load at the box's bound and would hold every stop for minutes.
pub(super) const GRACE: Duration = Duration::from_secs(5);

/// A `weaver_admin` for configs whose invoker never runs a line: absolute,
/// as the config requires, and naming nothing.
pub(super) const NO_WEAVER_ADMIN: &str = "/nonexistent/weaver-admin";

/// A trace file in a temporary directory, and the fake relay that serves
/// it: the door admin-con reads it through.
pub(super) struct Trace {
    _dir: tempfile::TempDir,
    pub(super) path: PathBuf,
    pub(super) relay: FakeRelay,
    /// The relay's socket, the config's `trace_socket`.
    pub(super) socket: PathBuf,
}

/// One trace record: its kind, and `n` in its payload to tell events apart.
fn record(n: u64, kind: &str) -> String {
    format!(
        "{}\n",
        json!({
            "kind": kind, "run": "run-1", "wall_ms": 1_790_000_000_000i64 + n as i64,
            "payload": {"n": n, "declaration": format!("sha-{n}")}
        })
    )
}

impl Trace {
    /// An empty trace whose relay runs: the door open.
    pub(super) fn new() -> Self {
        Self::with(|relay| relay)
    }

    /// An empty trace whose relay is shaped by `shape` and runs.
    pub(super) fn with(shape: impl FnOnce(FakeRelay) -> FakeRelay) -> Self {
        let mut trace = Self::closed_with(shape);
        trace.relay.start();
        trace
    }

    /// An empty trace whose relay does not run: the door closed.
    pub(super) fn closed() -> Self {
        Self::closed_with(|relay| relay)
    }

    fn closed_with(shape: impl FnOnce(FakeRelay) -> FakeRelay) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.ndjson");
        std::fs::write(&path, "").unwrap();
        let relay = shape(FakeRelay::new(&path));
        let socket = relay.socket.clone();
        Self {
            _dir: dir,
            path,
            relay,
            socket,
        }
    }

    fn append_raw(&self, bytes: &[u8]) {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.path)
            .unwrap();
        f.write_all(bytes).unwrap();
        f.sync_all().unwrap();
    }

    fn append(&self, n: u64, kind: &str) {
        self.append_raw(record(n, kind).as_bytes());
    }

    fn len(&self) -> u64 {
        std::fs::metadata(&self.path).unwrap().len()
    }
}

/// One scripted invocation of the fake invoker.
#[derive(Default)]
struct Step {
    delay: Duration,
    /// Written to the trace file before the snapshot, as admin would have
    /// written it just ahead of the invocation.
    first_append: Option<(PathBuf, String)>,
    /// Written to the trace file after the snapshot, as admin would while
    /// the verb ran.
    then_append: Option<(PathBuf, String)>,
    then_state: Option<String>,
}

/// A fake invoker: permits a configured set, answers `show` with the state
/// it holds at the invocation's start, and records what ran.
struct FakeInvoker {
    grants: Vec<String>,
    state: Mutex<String>,
    steps: Mutex<VecDeque<Step>>,
    ran: Mutex<Vec<String>>,
    /// A delay for every run of one verb, whatever its order.
    slow: Mutex<Option<(String, Duration)>>,
    /// A verb whose every run faults.
    fails: Mutex<Option<String>>,
}

impl FakeInvoker {
    fn new(grants: &[&str], state: &str) -> Arc<Self> {
        Arc::new(Self {
            grants: grants.iter().map(|g| (*g).to_owned()).collect(),
            state: Mutex::new(state.to_owned()),
            steps: Mutex::new(VecDeque::new()),
            ran: Mutex::new(Vec::new()),
            slow: Mutex::new(None),
            fails: Mutex::new(None),
        })
    }

    fn fail(&self, verb: &str) {
        *self.fails.lock().unwrap() = Some(verb.to_owned());
    }

    fn slow(&self, verb: &str, delay: Duration) {
        *self.slow.lock().unwrap() = Some((verb.to_owned(), delay));
    }

    fn script(&self, step: Step) {
        self.steps.lock().unwrap().push_back(step);
    }

    fn set_state(&self, state: &str) {
        *self.state.lock().unwrap() = state.to_owned();
    }

    fn ran(&self) -> Vec<String> {
        self.ran.lock().unwrap().clone()
    }
}

impl Invoker for FakeInvoker {
    async fn grants(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.grants.clone())
    }

    async fn run(
        &self,
        agent: &str,
        verb: &str,
        _principal: &Principal,
    ) -> Result<VerbOutcome, VerbFault> {
        self.ran.lock().unwrap().push(verb.to_owned());
        if self.fails.lock().unwrap().as_deref() == Some(verb) {
            return Err(VerbFault {
                kind: VerbFault::FAULT.into(),
                message: format!("{verb} faulted"),
            });
        }
        let step = self.steps.lock().unwrap().pop_front().unwrap_or_default();
        if let Some((path, line)) = step.first_append {
            let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            f.write_all(line.as_bytes()).unwrap();
            f.sync_all().unwrap();
        }
        let snapshot = self.state.lock().unwrap().clone();
        if let Some((path, line)) = step.then_append {
            let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
            f.write_all(line.as_bytes()).unwrap();
            f.sync_all().unwrap();
        }
        if let Some(state) = step.then_state {
            self.set_state(&state);
        }
        tokio::time::sleep(step.delay).await;
        let slow = self.slow.lock().unwrap().clone();
        if let Some((slow, delay)) = slow
            && slow == verb
        {
            tokio::time::sleep(delay).await;
        }
        let load = if snapshot == "idle" {
            json!({"declaration": "sha-show"})
        } else {
            serde_json::Value::Null
        };
        Ok(VerbOutcome {
            verb: verb.to_owned(),
            agent: agent.to_owned(),
            exit_code: Some(0),
            answer: Some(json!({"kind": "state", "state": snapshot, "load": load})),
            raw_stdout: None,
            stderr: None,
        })
    }
}

/// admin-con run in-process.
pub(super) struct Running {
    stop: watch::Sender<bool>,
    status: watch::Receiver<LinkStatus>,
    task: JoinHandle<anyhow::Result<()>>,
}

impl Running {
    pub(super) fn start<I: Invoker>(cfg: AdminConConfig, invoker: Arc<I>) -> Self {
        Self::start_with(cfg, invoker, FAST)
    }

    fn start_with<I: Invoker>(cfg: AdminConConfig, invoker: Arc<I>, backoff: Backoff) -> Self {
        Self::launch(cfg, invoker, None, backoff)
    }

    /// From the installed config at its path, re-read at a capped retry.
    fn from_file(path: &Path) -> Self {
        Self::launch(config(path), Arc::new(NoVerbs), Some(path.to_owned()), FAST)
    }

    fn launch<I: Invoker>(
        cfg: AdminConConfig,
        invoker: Arc<I>,
        source: Option<PathBuf>,
        backoff: Backoff,
    ) -> Self {
        let (stop, shutdown) = watch::channel(false);
        let (status_tx, status) = watch::channel(LinkStatus::default());
        let task = tokio::spawn(async move {
            admin_con::run(cfg, invoker, source, backoff, shutdown, &status_tx).await
        });
        Self { stop, status, task }
    }

    fn status(&self) -> LinkStatus {
        self.status.borrow().clone()
    }

    pub(super) async fn wait(
        &mut self,
        what: &str,
        cond: impl Fn(&LinkStatus) -> bool,
    ) -> LinkStatus {
        self.wait_for(SOON, what, cond).await
    }

    /// As `wait`, for up to `bound`.
    pub(super) async fn wait_for(
        &mut self,
        bound: Duration,
        what: &str,
        cond: impl Fn(&LinkStatus) -> bool,
    ) -> LinkStatus {
        let reached = tokio::time::timeout(bound, self.status.wait_for(|s| cond(s)))
            .await
            .ok()
            .and_then(|r| r.ok().map(|s| s.clone()));
        match reached {
            Some(s) => s,
            None => panic!("admin-con never reached {what}: {:?}", self.status()),
        }
    }

    pub(super) async fn stop(self) {
        let _ = self.stop.send(true);
        tokio::time::timeout(SOON * 2, self.task)
            .await
            .expect("admin-con stops on shutdown")
            .unwrap()
            .unwrap();
    }
}

/// An agent registered by the verbs, and its admin config as `register`
/// wrote it with `trace_socket` (and a backfill bound, where given) added.
pub(super) async fn installed(
    lab: &Lab,
    socket: &Path,
    out: &Path,
    backfill: Option<u64>,
) -> (AgentId, PathBuf) {
    installed_as(lab, "karl", socket, out, backfill).await
}

async fn installed_as(
    lab: &Lab,
    name: &str,
    socket: &Path,
    out: &Path,
    backfill: Option<u64>,
) -> (AgentId, PathBuf) {
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let answer = super::verbs::register(
        &lab.store,
        &lab_config(lab),
        &lab.authority,
        &r#box,
        name,
        out,
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let id: AgentId = answer.value["agent"].as_str().unwrap().parse().unwrap();
    let path = PathBuf::from(answer.value["configs"][1].as_str().unwrap());
    assert!(path.ends_with("admin-con.toml"));
    let mut extra = format!(
        "trace_socket = {}\nweaver_admin = {}\n",
        toml::Value::String(socket.display().to_string()),
        toml::Value::String(NO_WEAVER_ADMIN.to_owned())
    );
    if let Some(bytes) = backfill {
        extra.push_str(&format!("backfill_bytes = {bytes}\n"));
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(extra.as_bytes())
        .unwrap();
    (id, path)
}

pub(super) fn config(path: &Path) -> AdminConConfig {
    let mut cfg = AdminConConfig::load(path).unwrap();
    cfg.stop_grace = GRACE;
    cfg
}

fn window(lab: &Lab, id: &AgentId) -> Vec<TraceEvent> {
    lab.listener
        .windows()
        .snapshot(id.as_str())
        .unwrap_or_default()
}

/// The `n` of every relayed record, in window order.
fn ns(events: &[TraceEvent]) -> Vec<u64> {
    events
        .iter()
        .filter(|e| e.mark.is_none())
        .filter_map(|e| e.raw["payload"]["n"].as_u64())
        .collect()
}

/// admin-con's own marks: the server's reconnect and link-loss marks are
/// left out.
fn marks(events: &[TraceEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| e.mark.clone())
        .filter(|m| !m.starts_with("admin-con reconnected") && !m.starts_with("link to admin-con"))
        .collect()
}

async fn wait_window(lab: &Lab, id: &AgentId, what: &str, cond: impl Fn(&[TraceEvent]) -> bool) {
    let until = tokio::time::Instant::now() + SOON;
    loop {
        let events = window(lab, id);
        if cond(&events) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the window never reached {what}: {:?} marks {:?}",
            ns(&events),
            marks(&events)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Wait until the server has acknowledged through the file's current end.
async fn wait_acknowledged(lab: &Lab, id: &AgentId, trace: &Trace) {
    let until = tokio::time::Instant::now() + SOON;
    while lab.listener.acknowledged(id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub(super) async fn verb(
    listener: &Listener,
    id: &AgentId,
    verb: &str,
) -> Result<VerbOutcome, VerbError> {
    tokio::time::timeout(
        Duration::from_secs(20),
        listener.verb(id, verb, Principal::Server),
    )
    .await
    .unwrap_or_else(|_| panic!("the verb {verb} was never answered"))
}

/// **admin-con relays the trace, and a live load or unload lands on the
/// row** with the trace as its source; the row records the empty ceiling
/// the shipped invoker declares.
#[tokio::test]
async fn admin_con_relays_the_trace_and_lands_live_events() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;

    trace.append(4, "load");
    let row = lab
        .wait_for(&id, "idle from the trace", |a| {
            a.load_state.as_deref() == Some("idle")
        })
        .await;
    assert_eq!(row.state_source.as_deref(), Some("event"));
    assert_eq!(row.tuple.unwrap()["n"], 4);
    trace.append(5, "unload");
    lab.wait_for(&id, "unloaded from the trace", |a| {
        a.load_state.as_deref() == Some("unloaded")
    })
    .await;
    let row = lab.agent(&id).await;
    assert_eq!(
        row.ceiling,
        Some(Vec::new()),
        "the empty ceiling is recorded"
    );
    assert!(row.ceiling_at.is_some());
    assert!(marks(&window(&lab, &id)).is_empty());
    con.stop().await;
}

/// **The admission's `show` is asked only where the ceiling grants it**:
/// with the empty ceiling the shipped invoker declares, nothing is asked,
/// the connection stays admitted past the silence bound on one admission,
/// and every verb is refused on the server before a frame leaves it.
#[tokio::test]
async fn an_empty_ceiling_admits_without_show_and_refuses_every_verb() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let status = con.status();
    assert!(status.admitted, "{status:?}");
    assert_eq!(status.admissions, 1, "one admission throughout: {status:?}");
    assert!(lab.agent(&id).await.admin.connected);
    assert_eq!(lab.agent(&id).await.state_source, None, "no show was asked");
    for v in ["show", "stop", "load"] {
        match verb(&lab.listener, &id, v).await {
            Err(VerbError::OutsideCeiling { ceiling, .. }) => assert!(ceiling.is_empty()),
            other => panic!("{v}: expected the server's refusal, got {other:?}"),
        }
    }
    con.stop().await;
}

/// **The server never asks a verb outside the ceiling, and asks `show` at
/// admission where it is granted.** With `show` granted, the admission's
/// `show` runs and lands as the row's source; `stop` is refused on the
/// server and never reaches the invoker; a second `show` runs.
#[tokio::test]
async fn the_server_never_asks_a_verb_outside_the_ceiling() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    let row = lab
        .wait_for(&id, "the admission's show landed", |a| {
            a.state_source.as_deref() == Some("show")
        })
        .await;
    assert_eq!(row.load_state.as_deref(), Some("idle"));
    assert_eq!(row.ceiling, Some(vec!["show".to_owned()]));
    assert_eq!(invoker.ran(), ["show"]);

    match verb(&lab.listener, &id, "stop").await {
        Err(VerbError::OutsideCeiling { ceiling, .. }) => assert_eq!(ceiling, ["show"]),
        other => panic!("expected the server's refusal, got {other:?}"),
    }
    assert_eq!(invoker.ran(), ["show"], "stop never reached the invoker");
    verb(&lab.listener, &id, "show").await.unwrap();
    assert_eq!(invoker.ran(), ["show", "show"]);
    con.stop().await;
}

/// **admin-con answers an ask outside its ceiling with a typed error, runs
/// nothing, and keeps the connection** (Spec 8): a fake server asks `stop`
/// of a connector whose ceiling is `show`, then asks `show`.
#[tokio::test]
async fn admin_con_answers_an_ask_outside_its_ceiling_and_keeps_the_connection() {
    let server = FakeServer::start().await;
    let trace = Trace::new();
    let invoker = FakeInvoker::new(&["show"], "idle");
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit(15).await;
    let ask = |id: u64, verb: &str| {
        let mut line = serde_json::to_vec(&ToClient::Verb {
            id,
            verb: verb.to_owned(),
            principal: Principal::Server,
        })
        .unwrap();
        line.push(b'\n');
        line
    };
    let answer = async |reader: &mut super::frames::LineReader<_>, want: u64| loop {
        match tokio::time::timeout(SOON, reader.next())
            .await
            .expect("an answer")
        {
            Line::Frame(line) => {
                if let FromClient::Verb { id, outcome, error } =
                    serde_json::from_str::<FromClient>(&line).unwrap()
                    && id == want
                {
                    return (outcome, error);
                }
            }
            other => panic!("the connection ended: {other:?}"),
        }
    };
    write.write_all(&ask(1, "stop")).await.unwrap();
    let (outcome, error) = answer(&mut reader, 1).await;
    assert!(outcome.is_none());
    let error = error.expect("a typed error");
    assert_eq!(error.kind, VerbFault::OUTSIDE_CEILING);
    assert!(error.message.contains("show"), "{}", error.message);
    write.write_all(&ask(2, "show")).await.unwrap();
    let (outcome, error) = answer(&mut reader, 2).await;
    assert!(
        error.is_none() && outcome.is_some(),
        "the connection was kept"
    );
    assert_eq!(invoker.ran(), ["show"], "stop ran nothing");
    con.stop().await;
}

/// **The trace is replayed from the acknowledged position**: admin-con
/// stops after the server acknowledged three events, two more land while it
/// is down, and the restarted admin-con relays exactly those two, marking
/// nothing: same file, same generation, no hole and no repeat.
#[tokio::test]
async fn the_trace_is_replayed_from_the_acknowledged_position() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    trace.append(4, "turn");
    trace.append(5, "turn");
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "all five once", |e| ns(e) == [1, 2, 3, 4, 5]).await;
    assert!(
        marks(&window(&lab, &id)).is_empty(),
        "an unchanged file reads as the same generation: {:?}",
        marks(&window(&lab, &id))
    );
    con.stop().await;
}

/// **A file rotated between runs is a new identity**, named by the next
/// run's header: marked, and relayed from its start, even where it grew
/// past the old offset. The relay holds the run's file by descriptor, so
/// the rotation is seen at the next run, a restart of the relay.
#[tokio::test]
async fn a_rotation_while_the_link_is_down_is_marked_and_relayed_from_the_start() {
    let Some(lab) = Lab::open().await else { return };
    let mut trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;

    std::fs::rename(&trace.path, trace.path.with_extension("1")).unwrap();
    std::fs::write(&trace.path, "").unwrap();
    for n in 10..=16 {
        trace.append(n, "turn");
    }
    trace.relay.restart().await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "the new file whole", |e| {
        ns(e).ends_with(&[10, 11, 12, 13, 14, 15, 16])
    })
    .await;
    let marks = marks(&window(&lab, &id));
    assert!(marks.iter().any(|m| m.contains("replaced")), "{marks:?}");
    con.stop().await;
}

/// **A file truncated in place and regrown past the offset is caught by the
/// digest**: same device, inode and birth time, and records of the same
/// lengths, so a record ends at the acknowledged offset again and only the
/// digest of the record before it tells; it is marked and relayed from its
/// start.
#[tokio::test]
async fn a_truncation_regrown_past_the_offset_is_marked() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;

    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&trace.path)
        .unwrap();
    for n in 4..=9 {
        trace.append(n, "turn");
    }
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "the regrown file whole", |e| {
        ns(e) == [1, 2, 3, 4, 5, 6, 7, 8, 9]
    })
    .await;
    let marks = marks(&window(&lab, &id));
    assert!(
        marks.iter().any(|m| m.contains("truncated or rewritten")),
        "{marks:?}"
    );
    con.stop().await;
}

/// **After a server restart the hello's answer names no position**, and
/// admin-con relays a bounded tail of the file with a mark at its front
/// saying what was not relayed, never resuming from a position the new
/// server never acknowledged.
#[tokio::test]
async fn a_server_restart_relays_a_bounded_tail_with_a_mark() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let trace = Trace::new();
    for n in 1..=30 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let bound = 5 * record(30, "turn").len() as u64;
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(bound)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the bounded tail", |e| ns(e).last() == Some(&30)).await;
    let first = window(&lab, &id);
    assert!(!ns(&first).contains(&1), "{:?}", ns(&first));
    assert!(
        marks(&first).iter().any(|m| m.contains("backfill starts")),
        "{:?}",
        marks(&first)
    );
    wait_acknowledged(&lab, &id, &trace).await;

    // The server goes and comes back on its address; its window is new.
    restart_on_its_address(&mut lab, || trace.append(31, "turn")).await;
    con.wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    wait_window(&lab, &id, "the bounded tail again", |e| {
        ns(e).last() == Some(&31)
    })
    .await;
    let second = window(&lab, &id);
    assert!(
        marks(&second).iter().any(|m| m.contains("backfill starts")),
        "the restart's window begins at a marked discontinuity: {:?}",
        marks(&second)
    );
    assert!(
        ns(&second).len() >= 4,
        "a bounded tail, not only what came after: {:?}",
        ns(&second)
    );
    con.stop().await;
}

/// The server stopped and started again on its address, so admin-con's
/// config still reaches it; `between` runs while it is down.
pub(super) async fn restart_on_its_address(lab: &mut Lab, between: impl FnOnce()) {
    let address = lab.listener.address();
    lab.listener.stop().await;
    between();
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
            Err(_) if tokio::time::Instant::now() < until => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("the listener restarts on its address: {e:#}"),
        }
    };
}

/// **A record is relayed only whole**: an unterminated record at the tail
/// is left until its delimiter lands, then relayed once, and never as half
/// a record that fails to parse. The record is longer than the relay's
/// chunk, so the relay sends its first part mid-line and admin-con's reader
/// holds it. (The relay heartbeats only at the file's end, so an opening
/// taken while the file ends in a fragment waits for the fragment's end:
/// the record is written here once the link stands.)
#[tokio::test]
async fn an_unterminated_record_is_relayed_only_whole() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let second = format!(
        "{}\n",
        json!({
            "kind": "turn", "run": "run-1", "wall_ms": 1_790_000_000_002i64,
            "payload": {"n": 2, "pad": "x".repeat(100 * 1024)}
        })
    );
    let (head, rest) = second.as_bytes().split_at(70 * 1024);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    trace.append_raw(head);
    tokio::time::sleep(SETTLE).await;
    assert_eq!(ns(&window(&lab, &id)), [1], "the half record waits");
    trace.append_raw(rest);
    wait_window(&lab, &id, "the second, whole", |e| ns(e) == [1, 2]).await;
    let marks = marks(&window(&lab, &id));
    assert!(
        marks.is_empty(),
        "no half record, no parse failure: {marks:?}"
    );
    con.stop().await;
}

/// **An answer takes its place in the stream at the invocation** (Spec
/// 7.2): an unload written while `show` runs is read after the answer, so
/// the older snapshot cannot overwrite it and the row reads unloaded.
#[tokio::test]
async fn an_answer_takes_its_place_at_the_invocation() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    invoker.script(Step {
        delay: Duration::from_millis(500),
        then_append: Some((trace.path.clone(), record(1, "unload"))),
        then_state: Some("unloaded".into()),
        ..Step::default()
    });
    verb(&lab.listener, &id, "show").await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("event"));
    con.stop().await;
}

/// **The drain runs before the invocation** (Spec 7.2): a load event the
/// relay has not sent yet is read to the relay's next heartbeat and
/// emitted ahead of the answer, so a `show` taken after an unload cannot be
/// followed by the older load. The relay holds its stream while the ask
/// arrives, as a busy box's relay is behind.
#[tokio::test]
async fn the_drain_runs_before_the_invocation() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "unloaded");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    trace.relay.hold(true);
    trace.append(1, "load");
    let listener = lab.listener.clone();
    let asked = id.clone();
    let shown = tokio::spawn(async move { verb(&listener, &asked, "show").await });
    tokio::time::sleep(SETTLE).await;
    trace.relay.hold(false);
    shown.await.unwrap().unwrap();
    tokio::time::sleep(SETTLE).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("show"));
    con.stop().await;
}

/// **One verb at a time per connection** (Spec 7.2): a slow `show` taken
/// while the agent was idle, then an unload, then a fast `show`; run one at
/// a time the newer answer lands last and the row reads unloaded.
#[tokio::test]
async fn one_verb_runs_at_a_time() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    invoker.script(Step {
        delay: Duration::from_millis(700),
        ..Step::default()
    });
    let listener = lab.listener.clone();
    let asked = id.clone();
    let slow = tokio::spawn(async move { verb(&listener, &asked, "show").await });
    tokio::time::sleep(Duration::from_millis(150)).await;
    invoker.set_state("unloaded");
    let fast = verb(&lab.listener, &id, "show").await;
    slow.await.unwrap().unwrap();
    fast.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    con.stop().await;
}

/// **An admin-con config that is minted for the gate plane, lacks its trace
/// socket or names it relatively, or names a backfill past the bound is
/// refused at start.**
#[tokio::test]
async fn an_admin_con_config_that_is_not_trusted_is_refused_at_start() {
    use std::os::unix::fs::PermissionsExt;
    let server = FakeServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let text = |plane: Plane, extra: &str| {
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
        format!("{}{extra}", toml::to_string(&table).unwrap())
    };
    let write = |name: &str, content: String| {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path
    };
    let good = write(
        "good.toml",
        text(
            Plane::Admin,
            "trace_socket = \"/trace.sock\"\nweaver_admin = \"/w\"\n",
        ),
    );
    let cfg = AdminConConfig::load(&good).unwrap();
    assert_eq!(cfg.backfill_bytes, admin_con::DEFAULT_BACKFILL_BYTES);
    let refused = |path: &Path| AdminConConfig::load(path).unwrap_err().to_string();
    let gate = write(
        "gate.toml",
        text(
            Plane::Gate,
            "trace_socket = \"/trace.sock\"\nweaver_admin = \"/w\"\n",
        ),
    );
    assert!(refused(&gate).contains("gate plane"));
    let missing = write("missing.toml", text(Plane::Admin, ""));
    assert!(refused(&missing).contains("trace_socket"));
    let relative = write(
        "relative.toml",
        text(
            Plane::Admin,
            "trace_socket = \"trace.sock\"\nweaver_admin = \"/w\"\n",
        ),
    );
    assert!(refused(&relative).contains("trace_socket"));
    let unnamed = write(
        "unnamed.toml",
        text(
            Plane::Admin,
            "trace_socket = \"/trace.sock\"\nweaver_admin = \"/w\"\n",
        )
        .lines()
        .filter(|l| !l.starts_with("agent_id"))
        .map(|l| format!("{l}\n"))
        .collect(),
    );
    assert!(refused(&unnamed).contains("agent_id"));
    let big = write(
        "big.toml",
        text(
            Plane::Admin,
            &format!(
                "trace_socket = \"/trace.sock\"\nweaver_admin = \"/w\"\nbackfill_bytes = {}\n",
                admin_con::MAX_BACKFILL_BYTES + 1
            ),
        ),
    );
    assert!(refused(&big).contains("backfill_bytes"));
}

/// A record past `RECORD_BOUND`: one JSON line of `bytes` bytes, delimiter
/// included.
fn oversized(bytes: usize) -> Vec<u8> {
    let head = br#"{"kind":"turn","payload":{"pad":""#;
    let tail = b"\"}}\n";
    let mut line = head.to_vec();
    line.resize(bytes - tail.len(), b'x');
    line.extend_from_slice(tail);
    line
}

/// **A record past the bound before the boundary does not end the replay**
/// (Spec 7.2): it is skipped and marked, and the load event behind it is
/// still replayed, so it writes nothing to the row. Finished derives from
/// the file's position against the boundary, never from a step that sent
/// nothing.
#[tokio::test]
async fn a_record_past_the_bound_before_the_boundary_does_not_end_the_replay() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    trace.append_raw(&oversized(admin_con::RECORD_BOUND + 4096));
    trace.append(2, "load");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(16 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the replay", |e| ns(e) == [1, 2]).await;
    trace.append(3, "turn");
    wait_window(&lab, &id, "a live event", |e| ns(e) == [1, 2, 3]).await;
    let row = lab.agent(&id).await;
    assert_eq!(
        row.load_state, None,
        "the replayed load wrote the row: {row:?}"
    );
    let marks = marks(&window(&lab, &id));
    assert!(
        marks.iter().any(|m| m.contains("passed the")),
        "the record past the bound is marked: {marks:?}"
    );
    con.stop().await;
}

/// **The mark for a record past the bound carries the digest the file has
/// there**, over the whole record: acknowledged at the mark, a reconnection
/// whose opening cannot hold the outage asks the relay for that position,
/// the relay verifies the digest, and the replay resumes with no false
/// truncation.
#[tokio::test]
async fn a_reconnection_at_a_marked_record_resumes_without_a_false_truncation() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let bound = 4 * record(10, "turn").len() as u64;
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(bound)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    trace.append_raw(&oversized(admin_con::RECORD_BOUND + 70 * 1024));
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    for n in 2..=12 {
        trace.append(n, "turn");
    }
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "the outage", |e| {
        ns(e) == (1..=12).collect::<Vec<u64>>()
    })
    .await;
    let marks = marks(&window(&lab, &id));
    assert!(
        !marks.iter().any(|m| m.contains("truncated or rewritten")),
        "a false truncation: {marks:?}"
    );
    assert_eq!(
        marks.iter().filter(|m| m.contains("passed the")).count(),
        1,
        "{marks:?}"
    );
    con.stop().await;
}

/// **A record is measured as the frame that carries it**: a 1 MiB record
/// of NUL bytes is under the record bound raw and six times larger encoded,
/// so it is marked rather than sent past the line bound, and the link
/// stands.
#[tokio::test]
async fn a_record_that_encodes_past_the_line_bound_is_marked() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let mut nul = vec![0u8; 1024 * 1024];
    nul.push(b'\n');
    trace.append_raw(&nul);
    trace.append(2, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(16 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "both records", |e| ns(e) == [1, 2]).await;
    let marks = marks(&window(&lab, &id));
    assert!(
        marks.iter().any(|m| m.contains("encodes to a frame")),
        "{marks:?}"
    );
    let status = con.status();
    assert_eq!(status.admissions, 1, "the link stood: {status:?}");
    con.stop().await;
}

/// **An ack never blocks the server's read**: a backfill of many small
/// records, its acks outrunning admin-con's reads between replay steps,
/// is landed on one admission with no stall to the silence bound.
#[tokio::test]
async fn a_backfill_of_small_records_lands_on_one_admission() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(2 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end: {:?}",
            con.status()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.attempts, 1, "{status:?}");
    assert_eq!(ns(&window(&lab, &id)).last(), Some(&n));
    con.stop().await;
}

/// **A verb that passes its bound answers `unknown`**: the invocation was
/// ended and whether admin acted is not known, one shape for one fact.
#[tokio::test]
async fn a_verb_past_its_bound_answers_unknown() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut cfg = config(&path);
    cfg.verb_bound = Duration::from_millis(300);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    invoker.script(Step {
        delay: Duration::from_secs(2),
        ..Step::default()
    });
    match verb(&lab.listener, &id, "show").await {
        Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::UNKNOWN),
        other => panic!("expected the unknown fault, got {other:?}"),
    }
    con.stop().await;
}

/// **The `grants` ask is bounded**: an invoker that never answers declares
/// the empty ceiling at the hello's bound, and the link comes up.
#[tokio::test]
async fn a_grants_ask_that_never_answers_declares_the_empty_ceiling() {
    struct Hung;
    impl Invoker for Hung {
        async fn grants(&self) -> anyhow::Result<Vec<String>> {
            std::future::pending().await
        }
        async fn run(&self, _: &str, _: &str, _: &Principal) -> Result<VerbOutcome, VerbFault> {
            unreachable!("nothing is inside the empty ceiling")
        }
    }
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut cfg = config(&path);
    cfg.grants_bound = Duration::from_millis(300);
    let mut con = Running::start(cfg, Arc::new(Hung));
    con.wait("admitted", |s| s.admitted).await;
    assert_eq!(lab.agent(&id).await.ceiling, Some(Vec::new()));
    con.stop().await;
}

/// **A config for another agent re-installed at the path is refused**, as
/// gate-con refuses it, the agent told by its row's identity: admin-con's
/// tailer and invoker stay bound to the agent it started for, so dialing as
/// another would file this trace under that agent's row. Another box's
/// agent of the same name, then another name: the capped retries refuse
/// each and keep the credential in hand, and neither window gets an event.
#[tokio::test]
async fn a_reinstalled_config_for_another_agent_is_refused() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let twin_trace = Trace::new();
    let twin_out = tempfile::tempdir().unwrap();
    let (twin, twin_path) =
        installed_as(&lab, "karl", &twin_trace.socket, twin_out.path(), None).await;
    let other_trace = Trace::new();
    let other_out = tempfile::tempdir().unwrap();
    let (other, other_path) =
        installed_as(&lab, "kevin", &other_trace.socket, other_out.path(), None).await;
    let mut con = Running::from_file(&path);
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;

    let row = lab.agent(&id).await;
    lab.store
        .revoke_credential(&row, Plane::Admin, Some("lab"))
        .await
        .unwrap();
    con.wait("refused not_live", |s| {
        s.last_refusal == Some(super::frames::Refusal::NotLive)
    })
    .await;
    trace.append(2, "load");
    for (foreign, foreign_path) in [(&twin, &twin_path), (&other, &other_path)] {
        let staged = path.with_extension("installing");
        std::fs::write(&staged, std::fs::read_to_string(foreign_path).unwrap()).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        std::fs::rename(&staged, &path).unwrap();
        let attempts = con.status().attempts;
        con.wait("three capped retries", |s| s.attempts >= attempts + 3)
            .await;
        let status = con.status();
        assert!(!status.admitted, "{status:?}");
        assert_eq!(status.admissions, 1, "{status:?}");
        assert!(!lab.agent(foreign).await.admin.connected);
        assert!(
            window(&lab, foreign).is_empty(),
            "this trace filed under another row"
        );
        assert_eq!(lab.agent(foreign).await.load_state, None);
    }
    con.stop().await;
}

/// **An ask that arrives during the replay is served at once**, between
/// replay steps and with no drain: with `show` granted and a backfill that
/// outlasts the silence bound, the admission's `show` is answered within
/// its deadline, the admission completes once, and the replay goes on to
/// the file's end.
#[tokio::test]
async fn the_admissions_show_is_answered_during_a_long_replay() {
    let silence = Duration::from_secs(4);
    let Some(lab) = Lab::open_with(silence).await else {
        return;
    };
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 6 * 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(8 * 1024 * 1024)).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let started = tokio::time::Instant::now();
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    let until = started + Duration::from_secs(120);
    while lab.listener.acknowledged(&id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end: {:?}",
            con.status()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        started.elapsed() > silence,
        "the replay must outlast the silence bound for this test to say anything: {:?}",
        started.elapsed()
    );
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.last_refusal, None, "{status:?}");
    let row = lab.agent(&id).await;
    assert_eq!(row.state_source.as_deref(), Some("show"), "{row:?}");
    assert_eq!(row.load_state.as_deref(), Some("idle"));
    assert_eq!(invoker.ran(), ["show"]);
    con.stop().await;
}

/// **A backfill whose start falls inside a record past the bound starts at
/// that record's delimiter**, the first record boundary within the bound of
/// the end: the backfill's mark names it, the record before it is not
/// relayed, and the complete records after it arrive.
#[tokio::test]
async fn a_backfill_starting_inside_a_record_past_the_bound_keeps_what_follows() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let head = trace.len();
    trace.append_raw(&oversized(admin_con::RECORD_BOUND + 64 * 1024));
    for n in 4..=6 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let backfill = trace.len() - (head + 100);
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(backfill)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the records after it", |e| ns(e) == [4, 5, 6]).await;
    let starts = format!(
        "backfill starts {} bytes",
        trace.len() - 3 * record(4, "turn").len() as u64
    );
    let marks = marks(&window(&lab, &id));
    assert!(
        marks.iter().any(|m| m.contains(&starts)),
        "{starts}: {marks:?}"
    );
    assert!(!marks.iter().any(|m| m.contains("passed the")), "{marks:?}");
    con.stop().await;
}

/// **An ask during the replay waits behind at most one frame, never a
/// step, and behind no more than the link's unsent bound**: a fake server
/// with a small receive buffer reads the replay slowly, a millisecond every
/// four lines, so a megabyte of records, the step a reader might batch,
/// takes longer to cross than the shortest admission deadline, and asks
/// `show` once the replay is under way. The answer comes back within a
/// tenth of such a step and inside that deadline.
#[tokio::test]
async fn an_ask_during_the_replay_waits_behind_one_frame_not_a_step() {
    let server = FakeServer::start().await;
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 12 * 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let per_step = 1024 * 1024 / format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n").len();
    // A millisecond every four lines: slow enough that a step outlasts the
    // deadline, fast enough that admin-con's own write bound, a cadence,
    // never takes the reader for a server that is gone.
    let throttle = Duration::from_millis(1);
    let throttle_every = 4usize;
    let deadline = Duration::from_secs(4);
    assert!(
        throttle * (per_step / throttle_every) as u32 > deadline,
        "one step must outlast the shortest deadline for this test to say anything"
    );
    let invoker = FakeInvoker::new(&["show"], "idle");
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: 16 * 1024 * 1024,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let status = con.status.clone();
    let (mut reader, mut write) = server.admit_with(15, Some(64 * 1024)).await;
    let lines = std::cell::Cell::new(0usize);
    let next = async |reader: &mut super::frames::LineReader<_>| {
        let line = match tokio::time::timeout(SOON * 4, reader.next())
            .await
            .expect("a frame")
        {
            Line::Frame(line) => line,
            other => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                panic!(
                    "the connection ended: {other:?} {:?}",
                    status.borrow().clone()
                )
            }
        };
        lines.set(lines.get() + 1);
        if lines.get().is_multiple_of(throttle_every) {
            tokio::time::sleep(throttle).await;
        }
        serde_json::from_str::<FromClient>(&line).unwrap()
    };
    // The step is under way: its first frames have crossed.
    let mut read = 0usize;
    while read < 200 {
        if let FromClient::Event { .. } = next(&mut reader).await {
            read += 1;
        }
    }
    let mut ask = serde_json::to_vec(&ToClient::Verb {
        id: 1,
        verb: "show".into(),
        principal: Principal::Server,
    })
    .unwrap();
    ask.push(b'\n');
    write.write_all(&ask).await.unwrap();
    let asked = tokio::time::Instant::now();
    let mut behind = 0usize;
    loop {
        match next(&mut reader).await {
            FromClient::Event { .. } => behind += 1,
            FromClient::Verb { id: 1, outcome, .. } => {
                assert!(outcome.is_some());
                break;
            }
            _ => {}
        }
        assert!(
            behind < per_step,
            "the answer waits behind the step: {behind} events since the ask"
        );
    }
    let answered = asked.elapsed();
    assert!(
        behind < per_step / 10,
        "{behind} events crossed ahead of the answer, of a step of {per_step}"
    );
    assert!(answered < deadline, "answered after {answered:?}");
    assert_eq!(invoker.ran(), ["show"]);
    drop(reader);
    drop(write);
    con.stop().await;
}

/// **An ordinary verb asked during the replay waits for `caught_up` and the
/// drain**: only a `show` the server asked is served during the replay. A
/// fake server asks the admission's `show` and then a `show` for a person
/// right behind the hello's answer, and a record is appended after the
/// hello, live and unread: the admission's answer comes before `caught_up`,
/// and the ordinary answer after `caught_up` and after that record.
#[tokio::test]
async fn an_ordinary_verb_asked_during_the_replay_waits_for_caught_up_and_the_drain() {
    let server = FakeServer::start().await;
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 2 * 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let invoker = FakeInvoker::new(&["show"], "idle");
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: 4 * 1024 * 1024,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit(15).await;
    // Live and unread: after the hello, beyond the boundary, and the poll
    // too slow to read it on its own.
    trace.append(999_999, "unload");
    let mut asks = Vec::new();
    let person = Principal::Person { name: "ada".into() };
    for (id, principal) in [(1u64, Principal::Server), (2, person)] {
        asks.extend(
            serde_json::to_vec(&ToClient::Verb {
                id,
                verb: "show".into(),
                principal,
            })
            .unwrap(),
        );
        asks.push(b'\n');
    }
    write.write_all(&asks).await.unwrap();

    let mut order = Vec::new();
    while !order.contains(&"ordinary") {
        let line = match tokio::time::timeout(SOON * 4, reader.next())
            .await
            .expect("a frame")
        {
            Line::Frame(line) => line,
            other => panic!("the connection ended: {other:?}"),
        };
        match serde_json::from_str::<FromClient>(&line).unwrap() {
            FromClient::CaughtUp => order.push("caught_up"),
            FromClient::Verb { id: 1, .. } => order.push("admission"),
            FromClient::Verb { id: 2, .. } => order.push("ordinary"),
            FromClient::Event { event, .. } if event.raw["payload"]["n"] == 999_999 => {
                order.push("live")
            }
            _ => {}
        }
    }
    assert_eq!(order, ["admission", "caught_up", "live", "ordinary"]);
    drop(reader);
    drop(write);
    con.stop().await;
}

/// **The admission's `show` converges on its snapshot**: a load written
/// after the hello and before the `show`'s invocation is live, relayed
/// after the answer, and the row ends on the snapshot's state.
#[tokio::test]
async fn the_admissions_show_converges_on_its_snapshot() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "unload");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    invoker.script(Step {
        first_append: Some((trace.path.clone(), record(2, "load"))),
        ..Step::default()
    });
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the load, live", |e| ns(e) == [1, 2]).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("idle"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("event"), "{row:?}");
    assert_eq!(invoker.ran(), ["show"]);
    con.stop().await;
}

/// **A stop is bounded by its grace even against a server that stopped
/// reading**: a fake server admits admin-con and never reads, the replay's
/// sends back up, and the stop returns within the grace rather than after
/// a send's cadence and a close's cadence more.
#[tokio::test]
async fn a_stop_against_a_server_that_stopped_reading_returns_within_the_grace() {
    let server = FakeServer::start().await;
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 16 * 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: 32 * 1024 * 1024,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, Arc::new(NoVerbs));
    // Admitted on a cadence far past the grace, then never read: the
    // replay fills the path and its sends wait.
    let (reader, write) = server.admit(60).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let Running { stop, task, .. } = con;
    let started = tokio::time::Instant::now();
    let _ = stop.send(true);
    let finished = tokio::time::timeout(GRACE * 2, task).await;
    let took = started.elapsed();
    assert!(
        finished.is_ok(),
        "the stop was still waiting after {took:?}"
    );
    assert!(
        took <= GRACE + Duration::from_millis(500),
        "the stop took {took:?}, past the {:?} grace",
        GRACE
    );
    drop(reader);
    drop(write);
}

/// **An opening that waits on its heartbeat is interrupted by a stop**:
/// the relay sends its header and then nothing, so the hello's opening
/// waits, within its bound; the runtime stays free, and a stop asked
/// meanwhile returns within the grace, nothing admitted.
#[tokio::test]
async fn a_stop_during_an_opening_that_waits_on_its_heartbeat_returns_within_the_grace() {
    let server = FakeServer::start().await;
    let trace = Trace::new();
    trace.append(1, "turn");
    trace.relay.hold(true);
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let begun = std::time::Instant::now();
    let con = Running::start(cfg, Arc::new(NoVerbs));
    tokio::time::sleep(Duration::from_secs(1)).await;
    // The runtime stayed free while the opening waited.
    let asked = begun.elapsed();
    assert!(
        asked < Duration::from_secs(2),
        "the runtime was held for {asked:?} by the opening"
    );
    let Running {
        stop, status, task, ..
    } = con;
    let started = tokio::time::Instant::now();
    let _ = stop.send(true);
    let finished = tokio::time::timeout(GRACE * 3, task).await;
    let took = started.elapsed();
    assert!(
        finished.is_ok(),
        "the stop was still waiting after {took:?}"
    );
    assert!(
        took < GRACE,
        "the stop took {took:?}, past the {:?} grace",
        GRACE
    );
    assert_eq!(
        status.borrow().admissions,
        0,
        "the opening was still waiting"
    );
    assert!(
        trace
            .relay
            .counts
            .connections
            .load(std::sync::atomic::Ordering::Relaxed)
            >= 1
    );
}

/// **A verb whose link ends with it in flight has an unknown outcome**: a
/// slow invocation is still running when admin-con stops and its grace
/// ends, the connection closes with the verb sent and unanswered, and the
/// caller is answered `Unanswered`, never `NotConnected`: the verb may have
/// run on the box, and admin's trace and next `show` hold what it did.
#[tokio::test]
async fn a_verb_whose_link_ends_in_flight_is_answered_as_unknown() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    invoker.script(Step {
        delay: GRACE + Duration::from_secs(5),
        ..Step::default()
    });
    let listener = lab.listener.clone();
    let asked = id.clone();
    let pending = tokio::spawn(async move { verb(&listener, &asked, "show").await });
    let until = tokio::time::Instant::now() + SOON;
    while invoker.ran().len() < 2 {
        assert!(tokio::time::Instant::now() < until, "the verb never ran");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    con.stop().await;
    match pending.await.unwrap() {
        Err(VerbError::Unanswered) => {}
        other => panic!("expected the unknown outcome, got {other:?}"),
    }
}

/// **A verb still waiting at shutdown is answered `not_started`**: a slow
/// verb runs and one waits behind it; admin-con stops, the waiting caller
/// is told its verb was never invoked, and the verb running finishes within
/// the grace with its answer.
#[tokio::test]
async fn a_verb_still_waiting_at_shutdown_is_answered_not_started() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    invoker.script(Step {
        delay: Duration::from_millis(1500),
        ..Step::default()
    });
    let listener = lab.listener.clone();
    let asked = id.clone();
    let running = tokio::spawn(async move { verb(&listener, &asked, "show").await });
    let until = tokio::time::Instant::now() + SOON;
    while invoker.ran().len() < 2 {
        assert!(tokio::time::Instant::now() < until, "the verb never ran");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let listener = lab.listener.clone();
    let asked = id.clone();
    let queued = tokio::spawn(async move { verb(&listener, &asked, "show").await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    con.stop().await;

    match queued.await.unwrap() {
        Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::NOT_STARTED),
        other => panic!("expected not_started, got {other:?}"),
    }
    running.await.unwrap().unwrap();
    assert_eq!(invoker.ran().len(), 2, "the waiting verb was never invoked");
}

/// **A verb taken from the queue and still draining is answered
/// `not_started` at a stop**: a verb counts as started only once its
/// invocation begins. The relay holds its stream, so the drain waits on a
/// heartbeat that does not come; a stop comes during it, and the caller is
/// told at once that the verb did not run, and the invoker never runs it.
#[tokio::test]
async fn a_verb_still_draining_at_a_stop_is_answered_not_started() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let con = Running::start(config(&path), invoker.clone());
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.agent(&id).await.state_source.as_deref() != Some("show") {
        assert!(
            tokio::time::Instant::now() < until,
            "the admission's show never landed"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    trace.relay.hold(true);
    let listener = lab.listener.clone();
    let asked = id.clone();
    let pending = tokio::spawn(async move { verb(&listener, &asked, "show").await });
    tokio::time::sleep(Duration::from_millis(500)).await;
    let Running { stop, task, .. } = con;
    let _ = stop.send(true);
    match tokio::time::timeout(SOON, pending)
        .await
        .expect("the caller is answered within the grace")
        .unwrap()
    {
        Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::NOT_STARTED),
        other => panic!("expected not_started, got {other:?}"),
    }
    assert_eq!(invoker.ran(), ["show"], "only the admission's show ran");
    tokio::time::timeout(SOON * 2, task)
        .await
        .expect("admin-con stops")
        .unwrap()
        .unwrap();
}

/// Wait until the row reads the trace door as `open`.
async fn wait_door(lab: &Lab, id: &AgentId, open: bool) -> super::register::Agent {
    lab.wait_for(
        id,
        if open {
            "the door open"
        } else {
            "the door closed"
        },
        |a| a.trace_door == Some(open),
    )
    .await
}

/// **A file truncated while the stream follows it is marked by the relay's
/// `truncated`** and relayed from its start; nothing is smoothed.
#[tokio::test]
async fn a_live_truncation_is_marked_and_relayed_from_the_start() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;

    std::fs::write(&trace.path, record(7, "turn")).unwrap();
    wait_window(&lab, &id, "the shrunk file from its start", |e| {
        ns(e) == [1, 2, 3, 7]
    })
    .await;
    let marks = marks(&window(&lab, &id));
    assert!(
        marks.iter().any(|m| m.contains("was truncated to")),
        "{marks:?}"
    );
    con.stop().await;
}

/// **A header with no birth time reads as the same file across admin-con's
/// restarts**: the identity is the header's, `-` for the birth time, never
/// zero and never process state, so nothing is marked.
#[tokio::test]
async fn a_header_without_a_birth_time_resumes_without_a_mark() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::with(FakeRelay::without_birth);
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    let acked = lab.listener.acknowledged(&id).unwrap();
    assert!(acked.generation.ends_with(":-"), "{acked:?}");
    con.stop().await;

    trace.append(2, "turn");
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "both once", |e| ns(e) == [1, 2]).await;
    assert!(marks(&window(&lab, &id)).is_empty());
    con.stop().await;
}

/// **A stream the relay replaced resumes from its position**: another
/// connection from the reader replaces admin-con's, admin-con redials from
/// where it read, replacing that one in turn, and nothing is lost, repeated
/// or marked; the door stays open throughout.
#[tokio::test]
async fn a_replaced_stream_resumes_from_its_position() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    let opened_at = wait_door(&lab, &id, true).await.trace_door_at;
    let before = trace
        .relay
        .counts
        .connections
        .load(std::sync::atomic::Ordering::Relaxed);

    let mut other = tokio::net::UnixStream::connect(&trace.socket)
        .await
        .unwrap();
    other.write_all(b"{\"offset\":0}\n").await.unwrap();
    tokio::time::sleep(SETTLE).await;
    trace.append(2, "turn");
    wait_window(&lab, &id, "the second", |e| ns(e) == [1, 2]).await;
    assert!(
        trace
            .relay
            .counts
            .connections
            .load(std::sync::atomic::Ordering::Relaxed)
            >= before + 2,
        "admin-con redialed after it was replaced"
    );
    assert!(marks(&window(&lab, &id)).is_empty());
    let row = lab.agent(&id).await;
    assert_eq!(row.trace_door, Some(true));
    assert_eq!(row.trace_door_at, opened_at, "the door never closed");
    con.stop().await;
}

/// **An outage longer than what an opening holds is replayed by a second
/// read** from the acknowledged position: the opening's read keeps only
/// the backfill bound's worth, so it cannot hold the span, and the replay
/// reads again from the position the server holds, losing nothing.
#[tokio::test]
async fn a_long_outage_is_replayed_by_a_second_read() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let bound = 4 * record(10, "turn").len() as u64;
    let (id, path) = installed(&lab, &trace.socket, out.path(), Some(bound)).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    for n in 4..=20 {
        trace.append(n, "turn");
    }
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "everything once", |e| {
        ns(e) == (1..=20).collect::<Vec<u64>>()
    })
    .await;
    assert!(
        marks(&window(&lab, &id)).is_empty(),
        "{:?}",
        marks(&window(&lab, &id))
    );
    con.stop().await;
}

/// **A closed door is admitted at once** (Spec 7.2): the hello carries no
/// boundary and the server takes `caught_up` as sent, the admission's
/// `show` lands at receipt, the row reads the door closed, and `load` is
/// askable while the agent is unloaded. **Its opening is an admission of
/// the trace**: the relay comes up, the row reads the door open, and `show`
/// is asked again at the opening.
#[tokio::test]
async fn a_closed_door_is_admitted_at_once_and_its_opening_asks_show() {
    let Some(lab) = Lab::open().await else { return };
    let mut trace = Trace::closed();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show", "load"], "unloaded");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    let row = lab
        .wait_for(&id, "the admission's show", |a| {
            a.state_source.as_deref() == Some("show")
        })
        .await;
    assert_eq!(row.trace_door, Some(false), "{row:?}");
    assert!(row.trace_door_at.is_some());
    let loaded = verb(&lab.listener, &id, "load").await.unwrap();
    assert_eq!(loaded.verb, "load");

    invoker.set_state("idle");
    trace.relay.start();
    let row = wait_door(&lab, &id, true).await;
    assert!(row.trace_door_at > Some(row.registered_at));
    let until = tokio::time::Instant::now() + SOON;
    while invoker.ran() != ["show", "load", "show"] {
        assert!(
            tokio::time::Instant::now() < until,
            "the opening never asked show: {:?}",
            invoker.ran()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    lab.wait_for(&id, "the opening's show", |a| {
        a.load_state.as_deref() == Some("idle")
    })
    .await;
    let status = con.status();
    assert_eq!(status.admissions, 1, "one admission throughout: {status:?}");
    con.stop().await;
}

/// **Nothing written while the door was closed writes the row** (Spec
/// 7.2): with a ceiling that grants no `show`, so the row stands on live
/// events alone, a load written while the door is closed is replayed at
/// the opening, feeding the window and never the row; a record written
/// after the opening is live and does.
#[tokio::test]
async fn an_opening_replays_the_closed_interval_and_writes_no_row() {
    let Some(lab) = Lab::open().await else { return };
    let mut trace = Trace::closed();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_door(&lab, &id, false).await;

    trace.append(1, "load");
    trace.relay.start();
    wait_window(&lab, &id, "the closed interval, replayed", |e| ns(e) == [1]).await;
    wait_door(&lab, &id, true).await;
    tokio::time::sleep(SETTLE).await;
    let row = lab.agent(&id).await;
    assert_eq!(
        row.load_state, None,
        "an event from the closed interval wrote the row: {row:?}"
    );

    trace.append(2, "unload");
    lab.wait_for(&id, "the live unload", |a| {
        a.load_state.as_deref() == Some("unloaded")
    })
    .await;
    con.stop().await;
}

/// **The door's closing lands on the row**, dated, and a later run opens
/// it again on the same connection, relaying what the new run wrote.
#[tokio::test]
async fn the_doors_closing_and_reopening_land_on_the_row() {
    let Some(lab) = Lab::open().await else { return };
    let mut trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    let opened = wait_door(&lab, &id, true).await.trace_door_at;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;

    trace.relay.stop().await;
    let closed = wait_door(&lab, &id, false).await.trace_door_at;
    assert!(closed > opened, "{closed:?} after {opened:?}");
    trace.append(2, "turn");
    trace.relay.start();
    wait_door(&lab, &id, true).await;
    wait_window(&lab, &id, "the second", |e| ns(e) == [1, 2]).await;
    assert!(marks(&window(&lab, &id)).is_empty());
    assert_eq!(con.status().admissions, 1);
    con.stop().await;
}

/// **An opening is taken only while no invocation is in flight** (Spec
/// 7.2): a `load` passes its bound and runs on, holding the slot, and the
/// relay comes up during it. The opening waits for the load's process to
/// end, so its `show` is asked only once the slot is free and the
/// connection is never closed `admission_incomplete`, though the load
/// outlasts the silence bound.
#[tokio::test]
async fn an_opening_waits_for_the_invocation_in_flight() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let mut trace = Trace::closed();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show", "load"], "unloaded");
    let mut cfg = config(&path);
    cfg.verb_bound = Duration::from_millis(500);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    invoker.script(Step {
        delay: Duration::from_secs(7),
        then_state: Some("idle".into()),
        ..Step::default()
    });
    match verb(&lab.listener, &id, "load").await {
        Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::UNKNOWN),
        other => panic!("expected the unknown fault, got {other:?}"),
    }
    trace.relay.start();
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(
        lab.agent(&id).await.trace_door,
        Some(false),
        "the opening was taken while the load ran"
    );
    wait_door(&lab, &id, true).await;
    lab.wait_for(&id, "the opening's show after the load", |a| {
        a.load_state.as_deref() == Some("idle")
    })
    .await;
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.last_refusal, None, "{status:?}");
    con.stop().await;
}

/// **A relay that admits and drops every stream is backed off**: a stream
/// redialed after its run's relay restarted ends before it carries
/// anything, so the door is taken as closed and redialed on the connector's
/// backoff, never in a loop without pause.
#[tokio::test]
async fn a_relay_that_drops_every_stream_is_redialed_on_the_backoff() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let mut trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_door(&lab, &id, true).await;

    trace
        .relay
        .counts
        .end_after_header
        .store(true, Ordering::SeqCst);
    trace.relay.restart().await;
    wait_door(&lab, &id, false).await;
    let before = trace.relay.counts.connections.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let dialed = trace.relay.counts.connections.load(Ordering::Relaxed) - before;
    assert!(dialed < 30, "{dialed} dials in a second and a half");
    con.stop().await;
}

/// A writer that never idles: a record every 20 ms, faster than the fake
/// relay's heartbeat, so the relay never heartbeats while it runs.
fn never_idle(trace: &Trace) -> JoinHandle<()> {
    let path = trace.path.clone();
    tokio::spawn(async move {
        for n in 1_000.. {
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            f.write_all(record(n, "turn").as_bytes()).unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
}

/// **A writer that never idles is admitted at the boundary's bound** (Spec
/// 7.2): no heartbeat comes, the opening takes its boundary at the position
/// read so far once its bound passes, and the link comes up with the trace
/// relayed live behind it.
#[tokio::test]
async fn a_never_idle_writer_is_admitted_at_the_boundarys_bound() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let writer = never_idle(&trace);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(1);
    let mut con = Running::start(cfg, Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_door(&lab, &id, true).await;
    let seen = ns(&window(&lab, &id)).len();
    wait_window(&lab, &id, "live records past the boundary", |e| {
        ns(e).len() > seen + 10
    })
    .await;
    writer.abort();
    con.stop().await;
}

/// **A verb behind a writer that never idles runs at the drain's bound**
/// (Spec 7.2): no heartbeat dated after the drain's start comes, and the
/// verb is invoked once the bound passes rather than held behind the
/// writer, so an operator can still unload a runaway agent.
#[tokio::test]
async fn a_verb_behind_a_never_idle_writer_runs_at_the_drains_bound() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let writer = never_idle(&trace);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show", "unload"], "idle");
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(1);
    cfg.drain_bound = Duration::from_secs(1);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    let unloaded = tokio::time::timeout(
        Duration::from_secs(8),
        lab.listener.verb(&id, "unload", Principal::Server),
    )
    .await
    .expect("the unload ran behind the writer");
    assert_eq!(unloaded.unwrap().verb, "unload");
    assert!(invoker.ran().contains(&"unload".to_owned()));
    writer.abort();
    con.stop().await;
}

/// **A truncation met during an opening's read is marked** (Spec 7.2,
/// nothing is smoothed): with no position on the server, the relay answers
/// `truncated` as the opening reaches the file's end, the opening reads
/// the file again from its start, and the backfill it relays carries the
/// truncation's mark at its front.
#[tokio::test]
async fn a_truncation_during_an_opening_is_marked_at_the_replays_front() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    trace
        .relay
        .counts
        .truncate_next
        .store(true, Ordering::SeqCst);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the file", |e| ns(e) == [1, 2, 3]).await;
    let events = window(&lab, &id);
    let marks = marks(&events);
    assert!(
        marks.iter().any(|m| m.contains("while an opening read it")),
        "{marks:?}"
    );
    assert!(
        events.first().is_some_and(|e| e.mark.is_some()),
        "the mark leads the replay: {events:?}"
    );
    con.stop().await;
}

/// Stop the server, run `between` once admin-con has seen the link go, and
/// start the server again on its address with no positions held.
async fn restart_after(lab: &mut Lab, con: &mut Running, between: impl AsyncFnOnce()) {
    let address = lab.listener.address();
    lab.listener.stop().await;
    con.wait("the link down", |s| !s.admitted).await;
    between().await;
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
            Err(_) if tokio::time::Instant::now() < until => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(e) => panic!("the listener restarts on its address: {e:#}"),
        }
    };
}

/// **A position the relay refuses at an opening is marked** under a server
/// that holds no position: the file is rewritten in place while the link
/// is down, the opening's read from admin-con's last position is refused
/// and starts again from zero, and the window's first event is the mark.
#[tokio::test]
async fn a_refused_position_at_an_opening_is_marked_under_no_server_position() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    restart_after(&mut lab, &mut con, async || {
        std::fs::write(&trace.path, "").unwrap();
        for n in 4..=6 {
            trace.append(n, "turn");
        }
    })
    .await;
    con.wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    wait_window(&lab, &id, "the rewritten file", |e| ns(e) == [4, 5, 6]).await;
    let events = window(&lab, &id);
    let first = events
        .first()
        .and_then(|e| e.mark.clone())
        .unwrap_or_default();
    assert!(
        first.contains("the relay refused offset"),
        "{:?}",
        marks(&events)
    );
    assert_eq!(marks(&events).len(), 1, "one mark: {:?}", marks(&events));
    con.stop().await;
}

/// **A new file at an opening is marked** under a server that holds no
/// position: while the link is down the file is replaced by a copy grown
/// past it, a new file whose bytes still verify admin-con's last position,
/// and a new run's relay serves it; the header names the new identity, the
/// opening reads it from zero, and the window's first event is the mark.
#[tokio::test]
async fn a_new_file_at_an_opening_is_marked_under_no_server_position() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let mut trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    restart_after(&mut lab, &mut con, async || {
        let old = trace.path.with_extension("1");
        std::fs::rename(&trace.path, &old).unwrap();
        std::fs::copy(&old, &trace.path).unwrap();
        for n in 7..=9 {
            trace.append(n, "turn");
        }
        trace.relay.restart().await;
    })
    .await;
    con.wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    wait_window(&lab, &id, "the new file", |e| ns(e) == [1, 2, 3, 7, 8, 9]).await;
    let events = window(&lab, &id);
    let first = events
        .first()
        .and_then(|e| e.mark.clone())
        .unwrap_or_default();
    assert!(first.contains("the relay serves"), "{:?}", marks(&events));
    assert_eq!(marks(&events).len(), 1, "one mark: {:?}", marks(&events));
    con.stop().await;
}

/// **The boundary's bound covers the opening whole, its dials included**:
/// a relay that holds its header past the bound leaves the hello's door
/// closed at the bound, so admin-con is admitted then, not when the header
/// comes.
#[tokio::test]
async fn a_header_held_past_the_bound_leaves_the_door_closed() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    trace
        .relay
        .counts
        .header_delay_ms
        .store(6_000, Ordering::SeqCst);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(1);
    let started = tokio::time::Instant::now();
    let mut con = Running::start(cfg, Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "admitted after {:?}",
        started.elapsed()
    );
    assert_eq!(lab.agent(&id).await.trace_door, Some(false));
    con.stop().await;
}

/// **One mark per discontinuity**: the server keeps its position while the
/// link drops, the file was rewritten in place while admin-con was not
/// reading, the opening's read is refused at admin-con's last position and
/// marks it, and the replay from the server's position, standing before the
/// same discontinuity, adds no second mark.
#[tokio::test]
async fn a_refused_position_is_marked_once_under_the_servers_position() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut cfg = config(&path);
    cfg.drain_bound = Duration::from_millis(300);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    // The relay sends nothing while the file is rewritten, and the show's
    // answer cannot land, so the server closes the link holding its
    // position.
    trace.relay.hold(true);
    std::fs::write(&trace.path, "").unwrap();
    for n in 4..=6 {
        trace.append(n, "turn");
    }
    lab.listener.fail_next_land();
    let _ = verb(&lab.listener, &id, "show").await;
    trace.relay.hold(false);
    con.wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    wait_window(&lab, &id, "the rewritten file", |e| {
        ns(e) == [1, 2, 3, 4, 5, 6]
    })
    .await;
    tokio::time::sleep(SETTLE).await;
    let refused: Vec<String> = marks(&window(&lab, &id))
        .into_iter()
        .filter(|m| m.contains("refused offset"))
        .collect();
    assert_eq!(refused.len(), 1, "{refused:?}");
    con.stop().await;
}

/// **An opening serves its own `show` before any ordinary verb** (Spec
/// 7.2): a person's slow `validate` is queued while the opening reads to
/// its boundary, and the replay behind it is empty. The opening's `show`
/// is served first, inside the opening's deadline, then the `validate`;
/// both answers land and the connection is never closed
/// `admission_incomplete`.
#[tokio::test]
async fn an_opening_serves_its_show_before_a_verb_queued_at_it() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let mut trace = Trace::closed();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show", "validate"], "unloaded");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    // The relay comes up holding its stream, so the opening waits on its
    // heartbeat while the person's ask arrives.
    trace.relay.hold(true);
    trace.relay.start();
    let until = tokio::time::Instant::now() + SOON;
    while trace.relay.counts.connections.load(Ordering::Relaxed) == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the opening never dialed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // The `validate` takes longer than the opening's deadline.
    invoker.slow("validate", Duration::from_secs(6));
    let listener = lab.listener.clone();
    let asked = id.clone();
    let person = Principal::Person { name: "ada".into() };
    let validated = tokio::spawn(async move {
        tokio::time::timeout(
            Duration::from_secs(20),
            listener.verb(&asked, "validate", person),
        )
        .await
    });
    tokio::time::sleep(SETTLE).await;
    trace.relay.hold(false);

    let answered = validated.await.unwrap().expect("the validate was answered");
    assert_eq!(answered.unwrap().verb, "validate");
    assert_eq!(invoker.ran(), ["show", "show", "validate"]);
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.last_refusal, None, "{status:?}");
    con.stop().await;
}

/// **A `show` the server asks is never answered `busy`** (Spec 7.2): the
/// queue's bound of ordinary asks fills while an opening reads to its
/// boundary, and the opening's `show` is still taken, served first, and
/// completes the opening; every queued ask is answered after it.
#[tokio::test]
async fn an_openings_show_is_taken_past_a_full_queue() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let mut trace = Trace::closed();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show", "validate"], "unloaded");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;

    trace.relay.hold(true);
    trace.relay.start();
    let until = tokio::time::Instant::now() + SOON;
    while trace.relay.counts.connections.load(Ordering::Relaxed) == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the opening never dialed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let asks: Vec<_> = (0..admin_con::VERB_QUEUE)
        .map(|n| {
            let listener = lab.listener.clone();
            let asked = id.clone();
            let person = Principal::Person {
                name: format!("ada-{n}"),
            };
            tokio::spawn(async move {
                tokio::time::timeout(
                    Duration::from_secs(20),
                    listener.verb(&asked, "validate", person),
                )
                .await
            })
        })
        .collect();
    tokio::time::sleep(SETTLE).await;
    trace.relay.hold(false);

    for ask in asks {
        let answered = ask.await.unwrap().expect("the ask was answered");
        assert_eq!(answered.unwrap().verb, "validate");
    }
    let ran = invoker.ran();
    assert_eq!(&ran[..2], ["show", "show"], "{ran:?}");
    assert_eq!(ran.len(), 2 + admin_con::VERB_QUEUE, "{ran:?}");
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.last_refusal, None, "{status:?}");
    con.stop().await;
}

/// **A position the server holds past a boundary taken at its bound is
/// caught up already** (Spec 7.2): admin-con relays a long trace and stops;
/// restarted against a relay slower than the opening's bound, it takes its
/// boundary short of what the server acknowledged, and the opening sends
/// `caught_up` at once and resumes live from the server's position. The
/// connection is admitted once, a verb runs, and a record written after is
/// relayed.
#[tokio::test]
async fn an_acknowledgement_past_a_bounded_boundary_is_caught_up() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 2 * 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    // A chunk every 200 ms: the opening's 500 ms bound reads a few chunks.
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(200, Ordering::SeqCst);
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_millis(500);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted again", |s| s.admitted).await;
    trace.relay.counts.chunk_pause_ms.store(0, Ordering::SeqCst);
    verb(&lab.listener, &id, "show").await.unwrap();
    trace.append(n + 1, "turn");
    wait_window(&lab, &id, "the record after", |e| {
        ns(e).last() == Some(&(n + 1))
    })
    .await;
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.last_refusal, None, "{status:?}");
    con.stop().await;
}

/// **Only a clean end before a header is a refused position**: a relay
/// that sends half a header and drops, at a redial from a position past
/// zero, is a broken door, not a verdict on the position. The door closes
/// and reopens as an admission, and nothing is marked refused or relayed
/// again from zero.
#[tokio::test]
async fn a_half_header_at_a_redial_closes_the_door_rather_than_refusing() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    let opened = wait_door(&lab, &id, true).await.trace_door_at;

    trace
        .relay
        .counts
        .half_header_next
        .store(true, Ordering::SeqCst);
    trace.relay.kick();
    lab.wait_for(&id, "the door closed and opened again", |a| {
        a.trace_door == Some(true) && a.trace_door_at > opened
    })
    .await;
    trace.append(4, "turn");
    wait_window(&lab, &id, "the fourth", |e| ns(e) == [1, 2, 3, 4]).await;
    let marks = marks(&window(&lab, &id));
    assert!(!marks.iter().any(|m| m.contains("refused")), "{marks:?}");
    con.stop().await;
}

/// **A position the server holds past a bounded boundary is verified before
/// the replay ends** (Spec 7.2): the file is rewritten in place below the
/// acknowledged position while admin-con is down, the restarted opening's
/// boundary falls short of that position, the relay refuses it, and the
/// replacement is replayed behind the boundary with the refusal's mark at
/// its front: its old load writes nothing, and the row stands on the
/// opening's `show`.
#[tokio::test]
async fn a_rewritten_file_below_an_acknowledgement_past_the_boundary_is_replayed() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let filler = |from: u64, bytes: usize| {
        let mut body = String::new();
        let mut n = from;
        while body.len() < bytes {
            body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
            n += 1;
        }
        body
    };
    trace.append_raw(filler(1, 2 * 1024 * 1024).as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "unloaded");
    let mut con = Running::start(config(&path), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    // Rewritten in place, same file: a load at its front, then more than
    // the bound reads, all of it inside the window's ring.
    std::fs::write(&trace.path, record(9_000_000, "load")).unwrap();
    trace.append_raw(filler(10_000_000, 250 * 1024).as_bytes());
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(200, Ordering::SeqCst);
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_millis(500);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted again", |s| s.admitted).await;
    trace.relay.counts.chunk_pause_ms.store(0, Ordering::SeqCst);
    wait_window(&lab, &id, "the rewritten file", |e| {
        ns(e).contains(&9_000_000)
    })
    .await;
    tokio::time::sleep(SETTLE).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("show"), "{row:?}");
    let marks = marks(&window(&lab, &id));
    assert!(
        marks
            .iter()
            .any(|m| m.contains("refused the acknowledged offset")),
        "{marks:?}"
    );
    con.stop().await;
}

/// **The verification gets only what remains of the opening's bound**: a
/// restarted admin-con's opening uses its whole bound reading a slow relay,
/// its boundary falls short of the server's position, and the relay then
/// holds the verification dial's header. The verification fails at once,
/// the door reported closed within one bound of the start, not two.
#[tokio::test]
async fn the_verification_gets_only_what_remains_of_the_openings_bound() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < 2 * 1024 * 1024 {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    // The opening reads a chunk every 400 ms and gets no heartbeat in its
    // 2 s bound; the verification dial after it meets a held header.
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(400, Ordering::SeqCst);
    let dialed = trace.relay.counts.connections.load(Ordering::SeqCst);
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(2);
    let started = tokio::time::Instant::now();
    let con = Running::start(cfg, Arc::new(NoVerbs));
    while trace.relay.counts.connections.load(Ordering::SeqCst) == dialed {
        assert!(started.elapsed() < SOON, "the opening never dialed");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    trace
        .relay
        .counts
        .header_delay_ms
        .store(10_000, Ordering::SeqCst);
    let row = lab
        .wait_for(&id, "the door reported closed", |a| {
            a.admin.connected && a.trace_door == Some(false)
        })
        .await;
    assert!(
        started.elapsed() < Duration::from_millis(3200),
        "the opening took {:?}: {row:?}",
        started.elapsed()
    );
    con.stop().await;
}

/// **No new opening while an opening's hold stands**: a fake server takes
/// an opening's door frame and holds back its `show`; the relay closes and
/// comes back, and admin-con does not open the door again until that
/// `show` is asked and served, so the earlier opening's `show` can never
/// clear a later opening's hold.
#[tokio::test]
async fn no_opening_is_taken_while_an_openings_hold_stands() {
    let server = FakeServer::start().await;
    let mut trace = Trace::closed();
    let invoker = FakeInvoker::new(&["show"], "idle");
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit(15).await;
    // The next frame that is not a heartbeat, or none within `wait`.
    let next = async |reader: &mut super::frames::LineReader<_>, wait: Duration| loop {
        match tokio::time::timeout(wait, reader.next()).await {
            Err(_) => return None,
            Ok(Line::Frame(line)) => match serde_json::from_str::<FromClient>(&line).unwrap() {
                FromClient::Heartbeat => continue,
                frame => return Some(frame),
            },
            Ok(other) => panic!("the connection ended: {other:?}"),
        }
    };

    trace.relay.start();
    match next(&mut reader, SOON).await {
        Some(FromClient::Door { open: true, .. }) => {}
        other => panic!("expected the door opening, got {other:?}"),
    }
    // The opening's `show` is not asked yet. The relay closes and returns.
    trace.relay.stop().await;
    loop {
        match next(&mut reader, SOON).await {
            Some(FromClient::Door { open: false, .. }) => break,
            Some(FromClient::CaughtUp) => continue,
            other => panic!("expected the door closing, got {other:?}"),
        }
    }
    trace.relay.start();
    let early = next(&mut reader, Duration::from_millis(1500)).await;
    assert!(
        early.is_none(),
        "the door opened again while the hold stood: {early:?}"
    );

    let mut ask = serde_json::to_vec(&ToClient::Verb {
        id: 1,
        verb: "show".into(),
        principal: Principal::Server,
    })
    .unwrap();
    ask.push(b'\n');
    write.write_all(&ask).await.unwrap();
    match next(&mut reader, SOON).await {
        Some(FromClient::Verb { id: 1, outcome, .. }) => assert!(outcome.is_some()),
        other => panic!("expected the show's answer, got {other:?}"),
    }
    match next(&mut reader, SOON).await {
        Some(FromClient::Door { open: true, .. }) => {}
        other => panic!("expected the door opening again, got {other:?}"),
    }
    drop(reader);
    drop(write);
    con.stop().await;
}

/// A trace of `bytes` of small records, acknowledged through its end by an
/// admin-con that then stops: the server holds a position far ahead of
/// where a restarted admin-con's first opening can read to.
async fn acknowledged_long_trace(
    lab: &Lab,
    trace: &Trace,
    bytes: usize,
) -> (AgentId, PathBuf, tempfile::TempDir) {
    let mut body = String::new();
    let mut n = 0u64;
    while body.len() < bytes {
        n += 1;
        body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
    }
    trace.append_raw(body.as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&id).map(|p| p.offset) != Some(trace.len()) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the end"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;
    (id, path, out)
}

/// **The opening's bound keeps a reserve for verifying the server's
/// position**: a restarted admin-con's opening reads a relay too slow to
/// reach the server's far position within the bound, takes its boundary
/// short of it before the reserve, and verifies that position inside what
/// remains, so the door opens on the first attempt and a verb runs.
#[tokio::test]
async fn the_openings_bound_keeps_a_reserve_for_the_verification() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let (id, path, _out) = acknowledged_long_trace(&lab, &trace, 2 * 1024 * 1024).await;
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(300, Ordering::SeqCst);
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(2);
    let started = tokio::time::Instant::now();
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted again", |s| s.admitted).await;
    // A record written now reaches the window live only once an opening has
    // caught up at the server's position.
    trace.append(9_999_999, "turn");
    wait_window(&lab, &id, "the record after the restart", |e| {
        ns(e).contains(&9_999_999)
    })
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "caught up after {:?}",
        started.elapsed()
    );
    trace.relay.counts.chunk_pause_ms.store(0, Ordering::SeqCst);
    verb(&lab.listener, &id, "show").await.unwrap();
    con.stop().await;
}

/// **A failed verification moves the next opening forward**: every dial's
/// header is held past the reserve, so each verification fails, but each
/// opening reads on from the boundary the last one measured, until a
/// boundary reaches the server's position and the door opens with nothing
/// to verify; a verb runs.
#[tokio::test]
async fn failed_verifications_move_the_openings_toward_the_servers_position() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let (id, path, _out) = acknowledged_long_trace(&lab, &trace, 512 * 1024).await;
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(100, Ordering::SeqCst);
    trace
        .relay
        .counts
        .header_delay_ms
        .store(1_000, Ordering::SeqCst);
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(2);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted again", |s| s.admitted).await;
    // A record written now reaches the window live only once an opening has
    // caught up at the server's position.
    trace.append(9_999_999, "turn");
    let until = tokio::time::Instant::now() + Duration::from_secs(20);
    while !ns(&window(&lab, &id)).contains(&9_999_999) {
        assert!(
            tokio::time::Instant::now() < until,
            "never caught up after {} dials",
            trace.relay.counts.connections.load(Ordering::SeqCst)
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    trace
        .relay
        .counts
        .header_delay_ms
        .store(0, Ordering::SeqCst);
    trace.relay.counts.chunk_pause_ms.store(0, Ordering::SeqCst);
    verb(&lab.listener, &id, "show").await.unwrap();
    con.stop().await;
}

/// **An opening's `show` asked late is still served first**: a fake server
/// on a one-second cadence takes the door frame, asks a person's slow
/// `validate` at once, and asks the opening's `show` 4.5 s after the frame.
/// The hold on ordinary asks runs on no clock of admin-con's, so the `show`
/// is served first.
#[tokio::test]
async fn an_openings_show_asked_late_is_still_served_first() {
    let server = FakeServer::start().await;
    let mut trace = Trace::closed();
    let invoker = FakeInvoker::new(&["show", "validate"], "idle");
    invoker.slow("validate", Duration::from_secs(3));
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit(1).await;
    let next = async |reader: &mut super::frames::LineReader<_>, wait: Duration| loop {
        match tokio::time::timeout(wait, reader.next()).await {
            Err(_) => return None,
            Ok(Line::Frame(line)) => match serde_json::from_str::<FromClient>(&line).unwrap() {
                FromClient::Heartbeat => continue,
                frame => return Some(frame),
            },
            Ok(other) => panic!("the connection ended: {other:?}"),
        }
    };
    let ask = |id: u64, verb: &str, principal: Principal| {
        let mut line = serde_json::to_vec(&ToClient::Verb {
            id,
            verb: verb.into(),
            principal,
        })
        .unwrap();
        line.push(b'\n');
        line
    };

    trace.relay.start();
    match next(&mut reader, SOON).await {
        Some(FromClient::Door { open: true, .. }) => {}
        other => panic!("expected the door opening, got {other:?}"),
    }
    let opened = tokio::time::Instant::now();
    let person = Principal::Person { name: "ada".into() };
    write.write_all(&ask(2, "validate", person)).await.unwrap();
    match next(&mut reader, SOON).await {
        Some(FromClient::CaughtUp) => {}
        other => panic!("expected caught_up, got {other:?}"),
    }
    tokio::time::sleep_until(opened + Duration::from_millis(4500)).await;
    write
        .write_all(&ask(1, "show", Principal::Server))
        .await
        .unwrap();
    let mut answered = Vec::new();
    while answered.len() < 2 {
        match next(&mut reader, SOON * 2).await {
            Some(FromClient::Verb { id, .. }) => answered.push(id),
            Some(_) => {}
            None => panic!("the answers never came: {answered:?}"),
        }
    }
    assert_eq!(answered, [1, 2], "the opening's show was served first");
    drop(reader);
    drop(write);
    con.stop().await;
}

/// **A new file is marked at a position standing at offset zero**: a live
/// truncation leaves admin-con's position at zero of the file, the link
/// drops and the server restarts holding no position, and meanwhile the
/// file is rotated and a new run's relay serves it before any record is
/// read. The header names a file the position does not, so the window's
/// first event is the replaced-file mark.
#[tokio::test]
async fn a_new_file_at_offset_zero_is_marked() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let mut trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let mut con = Running::start(config(&path), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    std::fs::write(&trace.path, "").unwrap();
    wait_window(&lab, &id, "the truncation's mark", |e| {
        marks(e).iter().any(|m| m.contains("was truncated to"))
    })
    .await;
    restart_after(&mut lab, &mut con, async || {
        std::fs::rename(&trace.path, trace.path.with_extension("1")).unwrap();
        std::fs::write(&trace.path, "").unwrap();
        for n in 7..=9 {
            trace.append(n, "turn");
        }
        trace.relay.restart().await;
    })
    .await;
    con.wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    wait_window(&lab, &id, "the new file", |e| ns(e) == [7, 8, 9]).await;
    let events = window(&lab, &id);
    let first = events
        .first()
        .and_then(|e| e.mark.clone())
        .unwrap_or_default();
    assert!(first.contains("the relay serves"), "{:?}", marks(&events));
    con.stop().await;
}

/// **The opening's hold covers whatever deadline the server keeps**: a
/// fake server with a seven-second silence bound, so a one-second cadence,
/// takes the door frame, asks a person's slow `validate`, and asks the
/// opening's `show` six seconds after the frame. No bound admin-con could
/// compute from the cadence holds that long; the hold has none, and the
/// `show` is served first.
#[tokio::test]
async fn the_openings_hold_covers_the_servers_deadline() {
    let server = FakeServer::start().await;
    let mut trace = Trace::closed();
    let invoker = FakeInvoker::new(&["show", "validate"], "idle");
    invoker.slow("validate", Duration::from_secs(3));
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit_answering(1, Some(7), None).await;
    let next = async |reader: &mut super::frames::LineReader<_>, wait: Duration| loop {
        match tokio::time::timeout(wait, reader.next()).await {
            Err(_) => return None,
            Ok(Line::Frame(line)) => match serde_json::from_str::<FromClient>(&line).unwrap() {
                FromClient::Heartbeat => continue,
                frame => return Some(frame),
            },
            Ok(other) => panic!("the connection ended: {other:?}"),
        }
    };
    let ask = |id: u64, verb: &str, principal: Principal| {
        let mut line = serde_json::to_vec(&ToClient::Verb {
            id,
            verb: verb.into(),
            principal,
        })
        .unwrap();
        line.push(b'\n');
        line
    };

    trace.relay.start();
    match next(&mut reader, SOON).await {
        Some(FromClient::Door { open: true, .. }) => {}
        other => panic!("expected the door opening, got {other:?}"),
    }
    let opened = tokio::time::Instant::now();
    let person = Principal::Person { name: "ada".into() };
    write.write_all(&ask(2, "validate", person)).await.unwrap();
    tokio::time::sleep_until(opened + Duration::from_secs(6)).await;
    write
        .write_all(&ask(1, "show", Principal::Server))
        .await
        .unwrap();
    let mut answered = Vec::new();
    while answered.len() < 2 {
        match next(&mut reader, SOON * 2).await {
            Some(FromClient::Verb { id, .. }) => answered.push(id),
            Some(_) => {}
            None => panic!("the answers never came: {answered:?}"),
        }
    }
    assert_eq!(answered, [1, 2], "the opening's show was served first");
    drop(reader);
    drop(write);
    con.stop().await;
}

/// **The server connection's time is not charged to the hello's opening**:
/// the opening reads a slow relay for its whole read bound, its boundary
/// falling short of the server's position, and the server answers the hello
/// only after a delay longer than the reserve. The verification still has
/// what the measurement left it, so the door stays open and the replay
/// ends, rather than the door closing at once after admission.
#[tokio::test]
async fn a_hello_answered_late_still_leaves_the_verification_its_reserve() {
    use std::os::unix::fs::MetadataExt;
    use std::sync::atomic::Ordering;
    let server = FakeServer::start().await;
    let trace = Trace::new();
    let mut body = String::new();
    let mut n = 0u64;
    let mut last = String::new();
    while body.len() < 1024 * 1024 {
        n += 1;
        last = format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n");
        body.push_str(&last);
    }
    trace.append_raw(body.as_bytes());
    // The server holds the file's end, as the relay names it.
    let meta = std::fs::metadata(&trace.path).unwrap();
    let birth = meta
        .created()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let acknowledged = super::frames::Position {
        generation: format!("{}:{}:{birth}", meta.dev(), meta.ino()),
        offset: trace.len(),
        digest: super::relay::digest(last.as_bytes()),
    };
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(300, Ordering::SeqCst);
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: Duration::from_secs(2),
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, Arc::new(NoVerbs));
    let (mut reader, write) = server
        .admit_late(15, Some(acknowledged), Duration::from_secs(1))
        .await;
    let mut frames = Vec::new();
    let until = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(line) = tokio::time::timeout_at(until, reader.next()).await {
        match line {
            Line::Frame(line) => match serde_json::from_str::<FromClient>(&line).unwrap() {
                FromClient::Heartbeat => {}
                frame => frames.push(frame),
            },
            other => panic!("the connection ended: {other:?}"),
        }
    }
    assert!(
        frames.iter().any(|f| matches!(f, FromClient::CaughtUp)),
        "the replay never ended: {frames:?}"
    );
    assert!(
        !frames
            .iter()
            .any(|f| matches!(f, FromClient::Door { open: false, .. })),
        "the door closed after admission: {frames:?}"
    );
    drop(reader);
    drop(write);
    con.stop().await;
}

/// **The hello's verification runs beside the connection**: the server
/// asks its admission `show` at once, with the silence bound as its
/// deadline, while admin-con verifies a server position past a boundary
/// taken at its bound and the relay holds that dial's header longer than
/// the bound. The `show` is served meanwhile, the admission completes, and
/// the connection is never closed `admission_incomplete`.
#[tokio::test]
async fn the_hellos_verification_runs_beside_the_admissions_show() {
    use std::sync::atomic::Ordering;
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let trace = Trace::new();
    let (id, path, _out) = acknowledged_long_trace(&lab, &trace, 6 * 1024 * 1024).await;
    // The opening reads a chunk every 300 ms through its 15 s read, short
    // of the server's position; the relay then holds the verification's
    // header for 5 s, past the 4 s bound.
    trace
        .relay
        .counts
        .chunk_pause_ms
        .store(300, Ordering::SeqCst);
    trace
        .relay
        .counts
        .resume_header_delay_ms
        .store(5_000, Ordering::SeqCst);
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut cfg = config(&path);
    cfg.boundary_bound = Duration::from_secs(20);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait_for(Duration::from_secs(25), "admitted again", |s| s.admitted)
        .await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let status = con.status();
    assert_eq!(status.admissions, 1, "{status:?}");
    assert_eq!(status.last_refusal, None, "{status:?}");
    con.stop().await;
}

/// **A `show` that faults keeps the opening's hold**: the opening's `show`
/// answers with a fault, which the listener will not land, so it is about
/// to close the connection; a person's `load` queued at the opening waits
/// rather than running in that window, and the hold ends only with the
/// connection.
#[tokio::test]
async fn a_faulting_openings_show_keeps_the_hold() {
    let server = FakeServer::start().await;
    let mut trace = Trace::closed();
    let invoker = FakeInvoker::new(&["show", "load"], "idle");
    invoker.fail("show");
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit(15).await;
    let next = async |reader: &mut super::frames::LineReader<_>, wait: Duration| loop {
        match tokio::time::timeout(wait, reader.next()).await {
            Err(_) => return None,
            Ok(Line::Frame(line)) => match serde_json::from_str::<FromClient>(&line).unwrap() {
                FromClient::Heartbeat => continue,
                frame => return Some(frame),
            },
            Ok(other) => panic!("the connection ended: {other:?}"),
        }
    };
    let ask = |id: u64, verb: &str, principal: Principal| {
        let mut line = serde_json::to_vec(&ToClient::Verb {
            id,
            verb: verb.into(),
            principal,
        })
        .unwrap();
        line.push(b'\n');
        line
    };

    trace.relay.start();
    match next(&mut reader, SOON).await {
        Some(FromClient::Door { open: true, .. }) => {}
        other => panic!("expected the door opening, got {other:?}"),
    }
    let person = Principal::Person { name: "ada".into() };
    write.write_all(&ask(2, "load", person)).await.unwrap();
    write
        .write_all(&ask(1, "show", Principal::Server))
        .await
        .unwrap();
    let mut answered = Vec::new();
    while let Some(frame) = next(&mut reader, Duration::from_secs(2)).await {
        if let FromClient::Verb { id, .. } = frame {
            answered.push(id);
        }
    }
    assert_eq!(answered, [1], "only the faulting show was answered");
    assert_eq!(invoker.ran(), ["show"], "the load ran under the hold");
    drop(reader);
    drop(write);
    con.stop().await;
}

/// **A server slow to ask the opening's `show` still finds it first**: the
/// server lands the door slowly and asks the `show` seven seconds after the
/// frame, past any bound of a four-second silence's, with a person's verb
/// queued at the frame. The verb waits.
#[tokio::test]
async fn a_late_openings_show_still_runs_before_a_queued_verb() {
    let server = FakeServer::start().await;
    let mut trace = Trace::closed();
    let invoker = FakeInvoker::new(&["show", "validate"], "idle");
    let cfg = AdminConConfig {
        link: server.link(Plane::Admin),
        trace_socket: trace.socket.clone(),
        weaver_admin: PathBuf::from(NO_WEAVER_ADMIN),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        verb_bound: admin_con::VERB_BOUND,
        stop_grace: GRACE,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
        boundary_bound: admin_con::BOUNDARY_BOUND,
        drain_bound: admin_con::DRAIN_BOUND,
    };
    let con = Running::start(cfg, invoker.clone());
    let (mut reader, mut write) = server.admit(1).await;
    let next = async |reader: &mut super::frames::LineReader<_>, wait: Duration| loop {
        match tokio::time::timeout(wait, reader.next()).await {
            Err(_) => return None,
            Ok(Line::Frame(line)) => match serde_json::from_str::<FromClient>(&line).unwrap() {
                FromClient::Heartbeat => continue,
                frame => return Some(frame),
            },
            Ok(other) => panic!("the connection ended: {other:?}"),
        }
    };
    let ask = |id: u64, verb: &str, principal: Principal| {
        let mut line = serde_json::to_vec(&ToClient::Verb {
            id,
            verb: verb.into(),
            principal,
        })
        .unwrap();
        line.push(b'\n');
        line
    };

    trace.relay.start();
    match next(&mut reader, SOON).await {
        Some(FromClient::Door { open: true, .. }) => {}
        other => panic!("expected the door opening, got {other:?}"),
    }
    let opened = tokio::time::Instant::now();
    let person = Principal::Person { name: "ada".into() };
    write.write_all(&ask(2, "validate", person)).await.unwrap();
    tokio::time::sleep_until(opened + Duration::from_secs(7)).await;
    assert_eq!(
        invoker.ran(),
        Vec::<String>::new(),
        "the verb ran under the hold"
    );
    write
        .write_all(&ask(1, "show", Principal::Server))
        .await
        .unwrap();
    let mut answered = Vec::new();
    while answered.len() < 2 {
        match next(&mut reader, SOON).await {
            Some(FromClient::Verb { id, .. }) => answered.push(id),
            Some(_) => {}
            None => panic!("the answers never came: {answered:?}"),
        }
    }
    assert_eq!(answered, [1, 2], "the opening's show was served first");
    drop(reader);
    drop(write);
    con.stop().await;
}
