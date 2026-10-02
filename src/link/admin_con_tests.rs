//! Act 6: admin-con against a temporary trace file and the real listener,
//! with a fake invoker where a verb must run, and against a fake server
//! where the real one cannot be made to ask outside the ceiling. No agent is
//! reached and no verb is invoked for real.

use super::admin_con::{self, AdminConConfig, Invoker, NoVerbs};
use super::client::{Backoff, LinkStatus};
use super::client_tests::{FAST, FakeServer};
use super::frames::{
    FromClient, Line, Plane, Position, Principal, ToClient, VerbFault, VerbOutcome,
};
use super::listener::{Listener, VerbError};
use super::tests::{Fake, Lab, SILENCE, SOON, lab_config};
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

const POLL: Duration = Duration::from_millis(50);

/// A trace file in a temporary directory.
struct Trace {
    _dir: tempfile::TempDir,
    path: PathBuf,
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
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.ndjson");
        std::fs::write(&path, "").unwrap();
        Self { _dir: dir, path }
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
}

impl FakeInvoker {
    fn new(grants: &[&str], state: &str) -> Arc<Self> {
        Arc::new(Self {
            grants: grants.iter().map(|g| (*g).to_owned()).collect(),
            state: Mutex::new(state.to_owned()),
            steps: Mutex::new(VecDeque::new()),
            ran: Mutex::new(Vec::new()),
        })
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

    async fn run(&self, agent: &str, verb: &str, _principal: &Principal) -> VerbOutcome {
        self.ran.lock().unwrap().push(verb.to_owned());
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
        let load = if snapshot == "idle" {
            json!({"declaration": "sha-show"})
        } else {
            serde_json::Value::Null
        };
        VerbOutcome {
            verb: verb.to_owned(),
            agent: agent.to_owned(),
            exit_code: Some(0),
            answer: Some(json!({"kind": "state", "state": snapshot, "load": load})),
            raw_stdout: None,
            stderr: None,
        }
    }
}

/// admin-con run in-process.
struct Running {
    stop: watch::Sender<bool>,
    status: watch::Receiver<LinkStatus>,
    task: JoinHandle<anyhow::Result<()>>,
}

impl Running {
    fn start<I: Invoker>(cfg: AdminConConfig, invoker: Arc<I>) -> Self {
        Self::start_with(cfg, invoker, FAST)
    }

    fn start_with<I: Invoker>(cfg: AdminConConfig, invoker: Arc<I>, backoff: Backoff) -> Self {
        Self::launch(cfg, invoker, None, backoff)
    }

    /// From the installed config at its path, re-read at a capped retry.
    fn from_file(path: &Path, poll: Duration) -> Self {
        Self::launch(
            config(path, poll),
            Arc::new(NoVerbs),
            Some(path.to_owned()),
            FAST,
        )
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

    async fn wait(&mut self, what: &str, cond: impl Fn(&LinkStatus) -> bool) -> LinkStatus {
        let reached = tokio::time::timeout(SOON, self.status.wait_for(|s| cond(s)))
            .await
            .ok()
            .and_then(|r| r.ok().map(|s| s.clone()));
        match reached {
            Some(s) => s,
            None => panic!("admin-con never reached {what}: {:?}", self.status()),
        }
    }

    async fn stop(self) {
        let _ = self.stop.send(true);
        tokio::time::timeout(SOON * 2, self.task)
            .await
            .expect("admin-con stops on shutdown")
            .unwrap()
            .unwrap();
    }
}

/// An agent registered by the verbs, and its admin config as `register`
/// wrote it with `trace_file` (and a backfill bound, where given) added.
async fn installed(
    lab: &Lab,
    trace: &Path,
    out: &Path,
    backfill: Option<u64>,
) -> (AgentId, PathBuf) {
    installed_as(lab, "karl", trace, out, backfill).await
}

async fn installed_as(
    lab: &Lab,
    name: &str,
    trace: &Path,
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
        "trace_file = {}\n",
        toml::Value::String(trace.display().to_string())
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

fn config(path: &Path, poll: Duration) -> AdminConConfig {
    let mut cfg = AdminConConfig::load(path).unwrap();
    cfg.poll = poll;
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

async fn verb(listener: &Listener, id: &AgentId, verb: &str) -> Result<VerbOutcome, VerbError> {
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path, POLL), invoker.clone());
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
        trace_file: trace.path.clone(),
        backfill_bytes: admin_con::DEFAULT_BACKFILL_BYTES,
        poll: POLL,
        verb_bound: admin_con::VERB_BOUND,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    trace.append(4, "turn");
    trace.append(5, "turn");
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "all five once", |e| ns(e) == [1, 2, 3, 4, 5]).await;
    assert!(
        marks(&window(&lab, &id)).is_empty(),
        "an unchanged file reads as the same generation: {:?}",
        marks(&window(&lab, &id))
    );
    con.stop().await;
}

/// **A file rotated while the link was down is a new generation**, marked,
/// and relayed from its start, even where it grew past the old offset.
#[tokio::test]
async fn a_rotation_while_the_link_is_down_is_marked_and_relayed_from_the_start() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;

    std::fs::rename(&trace.path, trace.path.with_extension("1")).unwrap();
    std::fs::write(&trace.path, "").unwrap();
    for n in 10..=16 {
        trace.append(n, "turn");
    }
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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
/// digest**: same device, inode and birth time, so only the digest of the
/// record before the offset tells; it is marked and relayed from its start.
#[tokio::test]
async fn a_truncation_regrown_past_the_offset_is_marked() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;

    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&trace.path)
        .unwrap();
    for n in 20..=27 {
        trace.append(n, "turn");
    }
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "the regrown file whole", |e| {
        ns(e).ends_with(&[20, 21, 22, 23, 24, 25, 26, 27])
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
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(bound)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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
    let address = lab.listener.address();
    lab.listener.stop().await;
    trace.append(31, "turn");
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

/// **A record is relayed only whole**: an unterminated record at the tail
/// is left until its delimiter lands, then relayed once, and never as half
/// a record that fails to parse.
#[tokio::test]
async fn an_unterminated_record_is_relayed_only_whole() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let second = record(2, "turn");
    let (head, rest) = second.as_bytes().split_at(second.len() / 2);
    trace.append_raw(head);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    tokio::time::sleep(POLL * 4).await;
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path, POLL), invoker.clone());
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
/// tailer has not read yet is emitted ahead of the answer, so a `show` taken
/// after an unload cannot be followed by the older load.
#[tokio::test]
async fn the_drain_runs_before_the_invocation() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "unloaded");
    // A slow poll, so the tailer does not read the load on its own before
    // the ask arrives.
    let mut con = Running::start(config(&path, Duration::from_secs(3)), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    trace.append(1, "load");
    verb(&lab.listener, &id, "show").await.unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut con = Running::start(config(&path, POLL), invoker.clone());
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
/// file, or names a backfill past the bound is refused at start.**
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
    let good = write("good.toml", text(Plane::Admin, "trace_file = \"/trace\"\n"));
    let cfg = AdminConConfig::load(&good).unwrap();
    assert_eq!(cfg.backfill_bytes, admin_con::DEFAULT_BACKFILL_BYTES);
    let refused = |path: &Path| AdminConConfig::load(path).unwrap_err().to_string();
    let gate = write("gate.toml", text(Plane::Gate, "trace_file = \"/trace\"\n"));
    assert!(refused(&gate).contains("gate plane"));
    let missing = write("missing.toml", text(Plane::Admin, ""));
    assert!(refused(&missing).contains("trace_file"));
    let unnamed = write(
        "unnamed.toml",
        text(Plane::Admin, "trace_file = \"/trace\"\n")
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
                "trace_file = \"/trace\"\nbackfill_bytes = {}\n",
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
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(16 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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
/// there**: acknowledged at the mark, a reconnection verifies the digest
/// and resumes, raising no false truncation.
#[tokio::test]
async fn a_reconnection_at_a_marked_record_resumes_without_a_false_truncation() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    trace.append_raw(&oversized(admin_con::RECORD_BOUND + 70 * 1024));
    wait_acknowledged(&lab, &id, &trace).await;
    con.stop().await;
    lab.wait_for(&id, "admin down", |a| !a.admin.connected)
        .await;

    trace.append(2, "turn");
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted again", |s| s.admitted).await;
    wait_window(&lab, &id, "the second", |e| ns(e) == [1, 2]).await;
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
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(16 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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

/// **An unterminated fragment past the record bound at the file's end
/// does not stop the hello**: the tail is found at any distance, the
/// fragment is noted at the front, and once it ends it is marked and
/// relaying goes on.
#[tokio::test]
async fn a_long_unterminated_fragment_does_not_stop_the_hello() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let fragment = oversized(admin_con::RECORD_BOUND + 1024 * 1024);
    trace.append_raw(&fragment[..fragment.len() - 1]);
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(16 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;
    assert!(
        marks(&window(&lab, &id))
            .iter()
            .any(|m| m.contains("unterminated fragment")),
        "{:?}",
        marks(&window(&lab, &id))
    );
    trace.append_raw(b"\n");
    trace.append(2, "turn");
    wait_window(&lab, &id, "the second", |e| ns(e) == [1, 2]).await;
    assert!(
        marks(&window(&lab, &id))
            .iter()
            .any(|m| m.contains("passed the")),
        "{:?}",
        marks(&window(&lab, &id))
    );
    con.stop().await;
}

/// Hold the admin slot with a fake connection on admin-con's own
/// credential, so admin-con's attempts are refused as already connected.
async fn hold_the_slot(lab: &Lab, path: &Path) -> Fake {
    let link = AdminConConfig::load(path).unwrap().link;
    let credential = super::authority::ClientCredential {
        fingerprint: String::new(),
        certificate_pem: link.certificate.clone(),
        key_pem: link.key.clone(),
    };
    let until = tokio::time::Instant::now() + SOON;
    loop {
        let mut fake = Fake::try_connect(
            lab.listener.address(),
            lab.authority.certificate_pem(),
            &credential,
        )
        .await
        .unwrap();
        fake.send(FromClient::Hello {
            agent: link.agent.clone(),
            plane: Plane::Admin,
            tail: Some(Position {
                generation: "holder".into(),
                offset: 0,
                digest: String::new(),
            }),
            ceiling: Some(Vec::new()),
        })
        .await;
        match fake.recv().await {
            Some(ToClient::HelloAnswer { .. }) => {
                fake.send(FromClient::CaughtUp).await;
                return fake;
            }
            other => {
                assert!(
                    tokio::time::Instant::now() < until,
                    "the slot was never free: {other:?}"
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
}

/// **Only the resume clears the replaced generation**: a file rotated
/// during an outage, a reconnection refused as already connected and a
/// second refused, and the old file's tail past the acknowledged position
/// is still relayed before the new file once admin-con is admitted.
#[tokio::test]
async fn a_refused_reconnection_keeps_the_replaced_files_tail() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let slow = Backoff {
        base: Duration::from_secs(2),
        cap: Duration::from_secs(2),
    };
    let mut con = Running::start_with(config(&path, POLL), Arc::new(NoVerbs), slow);
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    wait_acknowledged(&lab, &id, &trace).await;

    // The outage: the store refuses the next landing, a live load, and the
    // server closes the link with that event unacknowledged.
    lab.listener.fail_next_land();
    trace.append(4, "load");
    con.wait("the link down", |s| !s.admitted).await;
    let holder = hold_the_slot(&lab, &path).await;

    // Rotated while the link is down; the old file is written once more
    // through the agent's still-open handle.
    let old = trace.path.with_extension("1");
    std::fs::rename(&trace.path, &old).unwrap();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&old)
        .unwrap()
        .write_all(record(5, "turn").as_bytes())
        .unwrap();
    std::fs::write(&trace.path, "").unwrap();
    trace.append(10, "turn");
    trace.append(11, "turn");

    let attempts = con.status().attempts;
    let until = tokio::time::Instant::now() + Duration::from_secs(20);
    while con.status().attempts < attempts + 2 {
        assert!(tokio::time::Instant::now() < until, "{:?}", con.status());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        con.status().last_refusal,
        Some(super::frames::Refusal::AlreadyConnected)
    );
    drop(holder);
    let until = tokio::time::Instant::now() + Duration::from_secs(20);
    while !con.status().admitted {
        assert!(tokio::time::Instant::now() < until, "{:?}", con.status());
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    wait_window(&lab, &id, "the old tail, then the new file", |e| {
        ns(e).ends_with(&[4, 5, 10, 11])
    })
    .await;
    let marks = marks(&window(&lab, &id));
    assert!(
        !marks.iter().any(|m| m.contains("no longer holds")),
        "the old tail was lost: {marks:?}"
    );
    assert!(marks.iter().any(|m| m.contains("rotation")), "{marks:?}");
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
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(2 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let mut cfg = config(&path, POLL);
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
        async fn run(&self, _: &str, _: &str, _: &Principal) -> VerbOutcome {
            unreachable!("nothing is inside the empty ceiling")
        }
    }
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut cfg = config(&path, POLL);
    cfg.grants_bound = Duration::from_millis(300);
    let mut con = Running::start(cfg, Arc::new(Hung));
    con.wait("admitted", |s| s.admitted).await;
    assert_eq!(lab.agent(&id).await.ceiling, Some(Vec::new()));
    con.stop().await;
}

/// **A symlinked sink is not supported**: a symlink at the trace path is
/// refused and marked, nothing is read through it, and a regular file put
/// in its place is relayed from its start.
#[tokio::test]
async fn a_symlink_at_the_trace_path_is_refused_and_marked() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let target = trace.path.with_extension("target");
    std::fs::write(&target, record(99, "load")).unwrap();
    std::fs::remove_file(&trace.path).unwrap();
    std::os::unix::fs::symlink(&target, &trace.path).unwrap();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the refusal's mark", |e| {
        marks(e).iter().any(|m| m.contains("symlink"))
    })
    .await;
    tokio::time::sleep(POLL * 6).await;
    assert!(ns(&window(&lab, &id)).is_empty(), "read through the link");
    assert_eq!(
        marks(&window(&lab, &id))
            .iter()
            .filter(|m| m.contains("symlink"))
            .count(),
        1,
        "marked once"
    );

    std::fs::remove_file(&trace.path).unwrap();
    std::fs::write(&trace.path, record(1, "turn")).unwrap();
    wait_window(&lab, &id, "the regular file", |e| ns(e) == [1]).await;
    assert_eq!(lab.agent(&id).await.load_state, None);
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let twin_trace = Trace::new();
    let twin_out = tempfile::tempdir().unwrap();
    let (twin, twin_path) =
        installed_as(&lab, "karl", &twin_trace.path, twin_out.path(), None).await;
    let other_trace = Trace::new();
    let other_out = tempfile::tempdir().unwrap();
    let (other, other_path) =
        installed_as(&lab, "kevin", &other_trace.path, other_out.path(), None).await;
    let mut con = Running::from_file(&path, POLL);
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

/// **A copy and truncate regrown past the position between two polls is
/// caught by the digest**: the file changed, so the record before the held
/// offset is checked as a reconnection checks it, found not to match, and
/// the file is marked and relayed from its start rather than resumed
/// mid-record.
#[tokio::test]
async fn a_truncation_regrown_between_two_polls_is_marked() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    for n in 1..=3 {
        trace.append(n, "turn");
    }
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let mut con = Running::start(config(&path, Duration::from_secs(2)), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first three", |e| ns(e) == [1, 2, 3]).await;
    // A poll has just read this one, so the next is a poll period away.
    trace.append(4, "turn");
    wait_window(&lab, &id, "the fourth", |e| ns(e) == [1, 2, 3, 4]).await;

    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&trace.path)
        .unwrap();
    for n in 20..=27 {
        trace.append(n, "turn");
    }
    wait_window(&lab, &id, "the regrown file whole", |e| {
        ns(e).ends_with(&[20, 21, 22, 23, 24, 25, 26, 27])
    })
    .await;
    let events = window(&lab, &id);
    let marks = marks(&events);
    assert!(marks.iter().any(|m| m.contains("truncated")), "{marks:?}");
    assert_eq!(ns(&events), [1, 2, 3, 4, 20, 21, 22, 23, 24, 25, 26, 27]);
    con.stop().await;
}

/// **A rotation just before a verb is drained through the new file too**:
/// a load written to the replacing file before the ask is emitted ahead of
/// the answer, so the `show` taken after it is the row's last word.
#[tokio::test]
async fn a_rotation_just_before_a_verb_is_drained_ahead_of_its_answer() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "unloaded");
    // A slow poll, so the tailer does not see the rotation on its own
    // before the ask arrives.
    let mut con = Running::start(config(&path, Duration::from_secs(3)), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    std::fs::rename(&trace.path, trace.path.with_extension("1")).unwrap();
    std::fs::write(&trace.path, record(1, "load")).unwrap();
    verb(&lab.listener, &id, "show").await.unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("show"));
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
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(8 * 1024 * 1024)).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    let started = tokio::time::Instant::now();
    let mut con = Running::start(config(&path, POLL), invoker.clone());
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

/// **A copy and truncate regrown past the position while a record past
/// the bound is being skipped is caught**: the bytes the skip kept are
/// compared with the file's before the position, found different, and the
/// file is marked and relayed from its start rather than skipped on from
/// inside another record.
#[tokio::test]
async fn a_truncation_regrown_during_a_skip_is_marked() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    trace.append(1, "turn");
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let poll = Duration::from_secs(1);
    let mut con = Running::start(config(&path, poll), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the first", |e| ns(e) == [1]).await;

    // A record, then an unterminated fragment of 3 MiB: the poll that reads
    // the record leaves the fragment; the next opens the skip; the one after
    // carries it to the file's end.
    let mut fragment = record(2, "turn").into_bytes();
    fragment.extend(std::iter::repeat_n(b'x', 3 * 1024 * 1024));
    trace.append_raw(&fragment);
    wait_window(&lab, &id, "the second", |e| ns(e) == [1, 2]).await;
    tokio::time::sleep(poll * 2 + poll / 2).await;

    // Truncated and regrown past the position between two polls, the
    // regrown bytes where the skip stands differing from what it kept.
    std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&trace.path)
        .unwrap();
    let mut regrown = Vec::new();
    for n in 20..=23 {
        regrown.extend(record(n, "turn").into_bytes());
    }
    let mut pad = br#"{"kind":"turn","payload":{"pad":""#.to_vec();
    pad.extend(std::iter::repeat_n(b'y', 4 * 1024 * 1024));
    pad.extend(b"\"}}\n");
    regrown.extend(pad);
    regrown.extend(record(24, "turn").into_bytes());
    trace.append_raw(&regrown);

    wait_window(&lab, &id, "the regrown file whole", |e| {
        ns(e).ends_with(&[20, 21, 22, 23, 24])
    })
    .await;
    let marks = marks(&window(&lab, &id));
    assert!(marks.iter().any(|m| m.contains("truncated")), "{marks:?}");
    con.stop().await;
}

/// **The boundary is checked by its digest before `caught_up`**: the file
/// is rewritten in place, longer than before, between the hello and the
/// replay's end, so it never shrinks below the boundary and keeps its
/// identity. The replay's end finds the record before the boundary no
/// longer the hello's, marks it, and relays the new content live, so the
/// load in it reaches the row.
#[tokio::test]
async fn a_rewrite_between_the_hello_and_the_replays_end_is_relayed_live() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let small = |from: u64, bytes: usize| {
        let mut body = String::new();
        let mut n = from;
        while body.len() < bytes {
            body.push_str(&format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n"));
            n += 1;
        }
        body
    };
    trace.append_raw(small(1, 6 * 1024 * 1024).as_bytes());
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(16 * 1024 * 1024)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    assert_ne!(
        lab.listener.acknowledged(&id).map(|p| p.offset),
        Some(trace.len()),
        "the replay must still be running for this test to say anything"
    );

    // Rewritten in place from its start, never truncated, so the length
    // never falls below the boundary.
    let mut rewritten = record(1, "load").into_bytes();
    rewritten.extend(small(1_000_000, 7 * 1024 * 1024).into_bytes());
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(&trace.path)
            .unwrap();
        f.write_all(&rewritten).unwrap();
        f.sync_all().unwrap();
    }

    // The replay of the old file's 6 MiB runs past the usual wait first.
    let until = tokio::time::Instant::now() + Duration::from_secs(90);
    let row = loop {
        let row = lab.agent(&id).await;
        if row.load_state.as_deref() == Some("idle") {
            break row;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the row never took the rewritten file's load: {:?}",
            marks(&window(&lab, &id))
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(row.state_source.as_deref(), Some("event"));
    let marks = marks(&window(&lab, &id));
    assert!(
        marks
            .iter()
            .any(|m| m.contains("no longer matches its digest")),
        "{marks:?}"
    );
    con.stop().await;
}

/// **A backfill whose start falls inside a record past the bound starts at
/// that record's delimiter**, found at any distance: the record is marked,
/// and the complete records after it arrive.
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
    let (id, path) = installed(&lab, &trace.path, out.path(), Some(backfill)).await;
    let mut con = Running::start(config(&path, POLL), Arc::new(NoVerbs));
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the records after it", |e| ns(e) == [4, 5, 6]).await;
    let marks = marks(&window(&lab, &id));
    assert!(
        marks
            .iter()
            .any(|m| m.contains("fell inside a record past")),
        "{marks:?}"
    );
    con.stop().await;
}

/// **An ask during the replay waits behind at most one frame, never a
/// step, and behind no more than the link's unsent bound**: a fake server
/// with a small receive buffer reads the replay slowly, a millisecond every
/// four lines, so one step's frames take longer to cross than the shortest
/// admission deadline, and asks `show` once the step is under way. The
/// answer comes back within a tenth of a step and inside that deadline.
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
    let per_step =
        admin_con::READ_BUDGET / format!("{{\"wall_ms\":1,\"payload\":{{\"n\":{n}}}}}\n").len();
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
        trace_file: trace.path.clone(),
        backfill_bytes: 16 * 1024 * 1024,
        poll: POLL,
        verb_bound: admin_con::VERB_BOUND,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
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
        trace_file: trace.path.clone(),
        backfill_bytes: 4 * 1024 * 1024,
        poll: Duration::from_secs(30),
        verb_bound: admin_con::VERB_BOUND,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
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
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "idle");
    invoker.script(Step {
        first_append: Some((trace.path.clone(), record(2, "load"))),
        ..Step::default()
    });
    let mut con = Running::start(config(&path, POLL), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    wait_window(&lab, &id, "the load, live", |e| ns(e) == [1, 2]).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("idle"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("event"), "{row:?}");
    assert_eq!(invoker.ran(), ["show"]);
    con.stop().await;
}

/// **The drain ends only where no replacement is pending**: an unread
/// record in the old file, a rotation already visible, and a load in the
/// new file when a verb is asked. The drain reads the old record, finds
/// the replacement pending, switches and drains the new file too, so the
/// load precedes the answer and the `show` taken after it is the row's last
/// word.
#[tokio::test]
async fn a_pending_rotation_with_an_unread_record_is_drained_ahead_of_the_answer() {
    let Some(lab) = Lab::open().await else { return };
    let trace = Trace::new();
    let out = tempfile::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
    let invoker = FakeInvoker::new(&["show"], "unloaded");
    // A slow poll, so the tailer sees neither the record nor the rotation
    // on its own before the ask arrives.
    let mut con = Running::start(config(&path, Duration::from_secs(3)), invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    lab.wait_for(&id, "the admission's show", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    trace.append(1, "turn");
    std::fs::rename(&trace.path, trace.path.with_extension("1")).unwrap();
    std::fs::write(&trace.path, record(2, "load")).unwrap();
    verb(&lab.listener, &id, "show").await.unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
    let row = lab.agent(&id).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("show"));
    assert_eq!(ns(&window(&lab, &id)), [1, 2]);
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
        trace_file: trace.path.clone(),
        backfill_bytes: 32 * 1024 * 1024,
        poll: POLL,
        verb_bound: admin_con::VERB_BOUND,
        grants_bound: Duration::from_secs(super::client::HELLO_SECS),
    };
    let con = Running::start(cfg, Arc::new(NoVerbs));
    // Admitted on a cadence far past the grace, then never read: the
    // replay fills the path and its sends wait.
    let (reader, write) = server.admit(60).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let Running { stop, task, .. } = con;
    let started = tokio::time::Instant::now();
    let _ = stop.send(true);
    let finished = tokio::time::timeout(admin_con::SHUTDOWN_GRACE * 2, task).await;
    let took = started.elapsed();
    assert!(
        finished.is_ok(),
        "the stop was still waiting after {took:?}"
    );
    assert!(
        took <= admin_con::SHUTDOWN_GRACE + Duration::from_millis(500),
        "the stop took {took:?}, past the {:?} grace",
        admin_con::SHUTDOWN_GRACE
    );
    drop(reader);
    drop(write);
}
