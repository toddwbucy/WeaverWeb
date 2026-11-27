//! conforms: web-a-verb-asks-the-service-and-reads-its-run-after-a-drop
//!
//! admin-con's invoker against the fake weaver-admin service
//! (`fake_service`, written from `weaver-admin-operator-contract` section
//! 8.10): every word that can answer a verb or the grants, each row of the
//! drop table, and, against the real listener, the invocation slot and the
//! orderly stop. No agent is reached and no program is run.

use super::admin_con::Invoker;
use super::admin_con_tests::{
    GRACE, Running, Service, Trace, config, installed, restart_on_its_address, verb,
};
use super::admin_service::{Purpose, Refused, word};
use super::entries::Entries;
use super::fake_service::{Act, Cut, FakeService, Grants, Script};
use super::frames::{Principal, VerbFault, VerbOutcome};
use super::listener::VerbError;
use super::service_invoker::ServiceInvoker;
use super::tests::{Lab, SOON};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const AGENT: &str = "karl";

fn person() -> Principal {
    Principal::Person {
        person_id: "pe-0123456789abcdef".to_owned(),
    }
}

/// An invoker of the fake, its entries attached, its poll short.
fn invoker(fake: &FakeService, bound: Duration) -> (ServiceInvoker, Entries) {
    let mut invoker = ServiceInvoker::new(&fake.socket, AGENT, bound);
    invoker.poll = Duration::from_millis(50);
    let entries = Entries::new("admin-con");
    invoker.attach(&entries);
    (invoker, entries)
}

/// The kinds of the connector failure entries raised so far.
fn kinds(entries: &Entries) -> Vec<String> {
    entries
        .pending()
        .into_iter()
        .filter_map(|(_, e)| e["kind"].as_str().map(str::to_owned))
        .collect()
}

async fn run(invoker: &ServiceInvoker, verb: &str) -> Result<VerbOutcome, VerbFault> {
    tokio::time::timeout(SOON, invoker.run(AGENT, verb, &person()))
        .await
        .expect("the verb was answered in time")
}

/// **Every word that can answer a verb's control connection, read as the
/// brief's table places it**: the six of the opening and the eight of the
/// verb's own, each with its kind and its entry or none.
///
/// Perturbation (measured 2026-11-27): read the service's `busy` as
/// `fault`, and the `busy` row is red.
#[tokio::test]
async fn every_word_at_a_verb_is_placed() {
    // (word, the kind admin-con answers, an entry raised)
    let opening = [
        (word::OPENING_MALFORMED, VerbFault::NOT_STARTED, true),
        (word::OPENING_LATE, VerbFault::NOT_STARTED, false),
        (word::VERSION_UNKNOWN, VerbFault::NOT_STARTED, true),
        (word::PURPOSE_UNKNOWN, VerbFault::NOT_STARTED, true),
        (word::NAME_REFUSED, VerbFault::NOT_STARTED, true),
        (word::FULL, VerbFault::NOT_STARTED, false),
    ];
    let mut placed = 0;
    for (w, kind, entry) in opening {
        let fake = FakeService::start(&["load"], "idle");
        fake.refuse(Some(Purpose::Control), w);
        let (invoker, entries) = invoker(&fake, SOON);
        let fault = run(&invoker, "load").await.unwrap_err();
        assert_eq!(fault.kind, kind, "{w}");
        assert!(fault.message.contains(w), "{w}: {}", fault.message);
        assert_eq!(
            kinds(&entries),
            if entry {
                vec![format!("service_{w}")]
            } else {
                vec![]
            },
            "{w}"
        );
        assert!(fake.with(|s| s.ran.is_empty()), "{w}: nothing ran");
        placed += 1;
    }
    let asked = [
        (word::ASK_UNKNOWN, VerbFault::NOT_STARTED, true),
        (word::ASK_MALFORMED, VerbFault::NOT_STARTED, true),
        (word::ASK_OUTSTANDING, VerbFault::NOT_STARTED, true),
        (word::VERB_UNKNOWN, VerbFault::NOT_STARTED, true),
        (word::NOT_GRANTED, VerbFault::NOT_GRANTED, false),
        (word::GRANT_UNREAD, VerbFault::NOT_STARTED, false),
        (word::BUSY, VerbFault::BUSY, false),
        (word::NOT_STARTED, VerbFault::NOT_STARTED, false),
    ];
    for (w, kind, entry) in asked {
        let fake = FakeService::start(&["load"], "idle");
        let mut refusal = Refused::new(w, format!("the fake refuses {w}"));
        if w == word::BUSY {
            refusal.run = Some("0123456789abcdef".into());
        }
        fake.script(Script::of(Some("load"), Act::Refuse(refusal)));
        let (invoker, entries) = invoker(&fake, SOON);
        let fault = run(&invoker, "load").await.unwrap_err();
        assert_eq!(fault.kind, kind, "{w}");
        match w {
            word::BUSY => assert!(
                fault.message.contains("0123456789abcdef"),
                "{}",
                fault.message
            ),
            word::NOT_GRANTED | word::ASK_OUTSTANDING => {}
            _ => assert!(fault.message.contains(w), "{w}: {}", fault.message),
        }
        assert_eq!(
            kinds(&entries),
            if entry {
                vec![format!("service_{w}")]
            } else {
                vec![]
            },
            "{w}"
        );
        assert!(fake.with(|s| s.ran.is_empty()), "{w}: nothing ran");
        placed += 1;
    }
    assert_eq!(placed, 14, "the six of the opening and the eight of a verb");
}

/// **The service's own judgments reach the same places**: a verb outside
/// the eight is `verb_unknown`, a grant read again at the verb is
/// `not_granted`, a run standing is `busy` naming it, and a start that
/// fails is `not_started`, none with an entry but the first.
#[tokio::test]
async fn the_services_own_judgments_are_read() {
    let fake = FakeService::start(&["load", "validate"], "idle");
    let (invoker, entries) = invoker(&fake, SOON);
    let fault = run(&invoker, "dance").await.unwrap_err();
    assert_eq!(fault.kind, VerbFault::NOT_STARTED);
    assert_eq!(kinds(&entries), ["service_verb_unknown"]);
    fake.with(|s| s.granted = Some(vec!["validate".into()]));
    assert_eq!(
        run(&invoker, "load").await.unwrap_err().kind,
        VerbFault::NOT_GRANTED
    );
    fake.with(|s| s.granted = None);
    let release = Arc::new(Notify::new());
    let mut held = Script::answer(Some("validate"), json!({"kind": "valid"}));
    held.release = Some(release.clone());
    fake.script(held);
    let first = {
        let invoker = invoker.clone();
        tokio::spawn(async move { invoker.run(AGENT, "validate", &person()).await })
    };
    while fake.with(|s| s.ran.is_empty()) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let busy = run(&invoker, "load").await.unwrap_err();
    assert_eq!(busy.kind, VerbFault::BUSY);
    release.notify_one();
    assert_eq!(
        first.await.unwrap().unwrap().answer,
        Some(json!({"kind": "valid"}))
    );
    fake.script(Script::of(Some("load"), Act::StartFails));
    assert_eq!(
        run(&invoker, "load").await.unwrap_err().kind,
        VerbFault::NOT_STARTED
    );
    assert_eq!(
        kinds(&entries),
        ["service_verb_unknown"],
        "one entry, for the defect"
    );
}

/// **A verb's final lines**: `ran` is the outcome, its exit code and
/// object and standard error carried; `fault` is admin-con's `fault`; the
/// principal reaches the service's record.
#[tokio::test]
async fn a_final_line_is_the_answer() {
    let fake = FakeService::start(&["load", "unload"], "idle");
    fake.script(Script::of(
        Some("load"),
        Act::Answer {
            exit_code: 1,
            answer: json!({"kind": "refused", "refusal": "busy"}),
            stderr: "why".into(),
            stderr_cut: true,
        },
    ));
    fake.script(Script::of(Some("unload"), Act::Fault("signal 9".into())));
    let (invoker, entries) = invoker(&fake, SOON);
    let ran = run(&invoker, "load").await.unwrap();
    assert_eq!(
        (
            ran.verb.as_str(),
            ran.agent.as_str(),
            ran.exit_code,
            ran.stderr.as_deref()
        ),
        ("load", AGENT, Some(1), Some("why"))
    );
    assert_eq!(
        ran.answer,
        Some(json!({"kind": "refused", "refusal": "busy"}))
    );
    let fault = run(&invoker, "unload").await.unwrap_err();
    assert_eq!(fault.kind, VerbFault::FAULT);
    assert!(fault.message.contains("signal 9"), "{}", fault.message);
    assert_eq!(
        fake.with(|s| s.ran.clone()),
        [
            ("load".to_owned(), person()),
            ("unload".to_owned(), person())
        ]
    );
    assert!(kinds(&entries).is_empty());
}

/// **The grants**: the list the service names, among the eight; every
/// refusal, of the opening or of the ask, an error naming its word.
///
/// Perturbation (measured 2026-11-27): answer the eight verbs whatever the
/// service names, and a verb it does not grant is declared.
#[tokio::test]
async fn the_grants_are_the_services_list_and_every_refusal_names_its_word() {
    let fake = FakeService::start(&["show", "load", "fly"], "idle");
    let (invoker, _) = invoker(&fake, SOON);
    assert_eq!(invoker.grants().await.unwrap(), ["show", "load"]);
    fake.with(|s| s.grants = Grants::Unread);
    let e = invoker.grants().await.unwrap_err().to_string();
    assert!(e.contains(word::GRANTS_UNREAD), "{e}");
    for w in word::OPENING {
        fake.refuse(Some(Purpose::Control), w);
        let e = invoker.grants().await.unwrap_err().to_string();
        assert!(e.contains(w), "{e}");
    }
}

/// **Dropped with its run standing, read until it ends** (the drop table's
/// first two rows): the connection closes after `started`, the run stands
/// and the invocation polls on, and once it ends its final line is the
/// answer.
#[tokio::test]
async fn a_drop_while_running_is_answered_once_its_run_ends() {
    let fake = FakeService::start(&["load"], "idle");
    let release = Arc::new(Notify::new());
    let mut script = Script::answer(Some("load"), json!({"kind": "loaded"}));
    script.release = Some(release.clone());
    script.cut = Cut::WhileRunning;
    fake.script(script);
    let (invoker, entries) = invoker(&fake, SOON);
    let running = {
        let invoker = invoker.clone();
        tokio::spawn(async move { invoker.run(AGENT, "load", &person()).await })
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(!running.is_finished(), "a standing run is polled on");
    release.notify_one();
    let ran = tokio::time::timeout(SOON, running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(ran.answer, Some(json!({"kind": "loaded"})));
    assert!(kinds(&entries).is_empty());
}

/// **Dropped while its run stands unstarted, no id held**: the run stands
/// from its grant with `started_ms` null, is read by its verb, and is
/// answered when it ends; a `fault` it ends in is admin-con's `fault`.
#[tokio::test]
async fn a_drop_before_started_reads_the_run_by_its_verb() {
    let fake = FakeService::start(&["load"], "idle");
    let start = Arc::new(Notify::new());
    let mut script = Script::of(Some("load"), Act::Fault("ended by a signal".into()));
    script.hold_start = Some(start.clone());
    script.cut = Cut::Unstarted;
    fake.script(script);
    let (invoker, _) = invoker(&fake, SOON);
    let running = {
        let invoker = invoker.clone();
        tokio::spawn(async move { invoker.run(AGENT, "load", &person()).await })
    };
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !running.is_finished(),
        "an unstarted standing run is polled on"
    );
    start.notify_one();
    let fault = tokio::time::timeout(SOON, running)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(fault.kind, VerbFault::FAULT);
}

/// **Nothing of its own after a drop, and the last run ended set aside**
/// (the drop table's third row): an earlier `load` ended and is the
/// service's last ended; a second `load` whose connection drops with
/// nothing standing reads that run as not its own, by `last_known`, and
/// answers `not_started`.
///
/// Perturbation (measured 2026-11-27): drop `last_known` from the reading
/// with no id held, and the second `load` answers the first one's outcome.
#[tokio::test]
async fn a_drop_with_nothing_of_its_own_is_not_started() {
    let fake = FakeService::start(&["load"], "idle");
    let (invoker, entries) = invoker(&fake, SOON);
    assert!(run(&invoker, "load").await.is_ok());
    fake.script(Script::of(
        Some("load"),
        Act::Refuse(Refused::new(word::ASK_OUTSTANDING, "as if an ask stood")),
    ));
    let fault = run(&invoker, "load").await.unwrap_err();
    assert_eq!(fault.kind, VerbFault::NOT_STARTED, "{}", fault.message);
    assert_eq!(kinds(&entries), ["service_ask_outstanding"]);
}

/// **Another `instance` after a drop is `unknown`** (the fourth row): the
/// service restarted while the run went on, so what came of it is the next
/// `show`'s to say.
///
/// Perturbation (measured 2026-11-27): read another `instance` as "did not
/// run", and the verb answers `not_started`.
#[tokio::test]
async fn a_restart_after_a_drop_is_unknown() {
    let fake = FakeService::start(&["load"], "idle");
    let release = Arc::new(Notify::new());
    let mut script = Script::answer(Some("load"), json!({"kind": "loaded"}));
    script.release = Some(release.clone());
    script.cut = Cut::WhileRunning;
    fake.script(script);
    let (invoker, _) = invoker(&fake, SOON);
    let running = {
        let invoker = invoker.clone();
        tokio::spawn(async move { invoker.run(AGENT, "load", &person()).await })
    };
    while fake.with(|s| s.ran.is_empty()) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // Polled standing at least once, then the service restarts.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!running.is_finished());
    fake.restart();
    let fault = tokio::time::timeout(SOON, running)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(fault.kind, VerbFault::UNKNOWN, "{}", fault.message);
    release.notify_one();
}

/// **No socket answering for the verb's bound after a drop is `unknown`**
/// (the fifth row), and the invocation ends, so the slot is freed.
#[tokio::test]
async fn no_socket_for_the_bound_after_a_drop_is_unknown() {
    let fake = FakeService::start(&["load"], "idle");
    let mut script = Script::answer(Some("load"), json!({"kind": "loaded"}));
    script.release = Some(Arc::new(Notify::new()));
    script.cut = Cut::WhileRunning;
    fake.script(script);
    let (invoker, _) = invoker(&fake, Duration::from_millis(600));
    let running = {
        let invoker = invoker.clone();
        tokio::spawn(async move { invoker.run(AGENT, "load", &person()).await })
    };
    while fake.with(|s| s.ran.is_empty()) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    std::fs::remove_file(&fake.socket).unwrap();
    let fault = tokio::time::timeout(SOON, running)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(fault.kind, VerbFault::UNKNOWN, "{}", fault.message);
    assert!(fault.message.contains("no socket"), "{}", fault.message);
}

/// **A word that ends a poll is `unknown`, with its entry**, and a word
/// that may pass waits for the next poll.
#[tokio::test]
async fn a_poll_refused_for_good_is_unknown_and_a_passing_word_waits() {
    let fake = FakeService::start(&["load"], "idle");
    let release = Arc::new(Notify::new());
    let mut script = Script::answer(Some("load"), json!({"kind": "loaded"}));
    script.release = Some(release.clone());
    script.cut = Cut::WhileRunning;
    fake.script(script);
    let (invoker, entries) = invoker(&fake, SOON);
    let running = {
        let invoker = invoker.clone();
        tokio::spawn(async move { invoker.run(AGENT, "load", &person()).await })
    };
    while fake.with(|s| s.ran.is_empty()) {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    fake.refuse(Some(Purpose::Control), word::OPENING_LATE);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!running.is_finished(), "a passing word waits");
    fake.refuse(Some(Purpose::Control), word::NAME_REFUSED);
    let fault = tokio::time::timeout(SOON, running)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(fault.kind, VerbFault::UNKNOWN);
    assert_eq!(kinds(&entries), ["service_name_refused"]);
    release.notify_one();
}

// ---------- against the real listener: the slot and the stop ----------

/// admin-con run with the service's invoker over the fake, for the slot
/// and the stop, its verb bound short so a run passes it.
struct Con {
    lab: Lab,
    service: Arc<Service>,
    id: crate::store::AgentId,
    path: PathBuf,
    _trace: Trace,
    _out: tempfile::TempDir,
}

const BOUND: Duration = Duration::from_millis(300);

impl Con {
    async fn open(grants: &[&str]) -> Option<Self> {
        let lab = Lab::open().await?;
        let service = Service::new(grants, "unloaded");
        let trace = Trace::new();
        let out = crate::test_dir::tempdir().unwrap();
        let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
        Some(Self {
            lab,
            service,
            id,
            path,
            _trace: trace,
            _out: out,
        })
    }

    fn runs(&self) -> Vec<String> {
        self.service.fake.with(|s| s.runs.clone())
    }

    async fn start(&self, grace: Duration) -> Running {
        let mut cfg = config(&self.path);
        cfg.verb_bound = BOUND;
        cfg.stop_grace = grace;
        let mut con = Running::start(cfg, self.service.clone());
        con.wait("admitted", |s| s.admitted).await;
        self.lab
            .wait_for(&self.id, "the admission's show", |a| {
                a.state_source.as_deref() == Some("show")
            })
            .await;
        con
    }

    async fn wait_runs(&self, n: usize) {
        let until = tokio::time::Instant::now() + SOON;
        while self.runs().len() < n {
            assert!(
                tokio::time::Instant::now() < until,
                "the service never reached {n} runs: {:?}",
                self.runs()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Ask `verb`, past its bound: the answer is `unknown`, the run goes on.
    async fn ask_past_the_bound(&self, verb: &str) {
        match super::admin_con_tests::verb(&self.lab.listener, &self.id, verb).await {
            Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::UNKNOWN),
            other => panic!("expected the unknown fault, got {other:?}"),
        }
        assert!(
            !self.runs().contains(&format!("end {verb}")),
            "{verb} ended inside its bound: {:?}",
            self.runs()
        );
    }
}

const SHOW_VALIDATE_SHOW: [&str; 6] = [
    "start show",
    "end show",
    "start validate",
    "end validate",
    "start show",
    "end show",
];

/// **A verb past its bound holds the slot until its final line**: the
/// answer is `unknown` and the run goes on; the next verb waits until that
/// run has ended, and never runs beside it.
#[tokio::test]
async fn a_verb_past_its_bound_holds_the_slot_until_its_final_line() {
    let Some(con) = Con::open(&["show", "validate"]).await else {
        return;
    };
    con.service.slow("validate", Duration::from_millis(1500));
    let running = con.start(GRACE).await;
    con.ask_past_the_bound("validate").await;
    let asked = tokio::time::Instant::now();
    let shown = verb(&con.lab.listener, &con.id, "show").await.unwrap();
    assert_eq!(shown.exit_code, Some(0));
    assert!(
        asked.elapsed() >= Duration::from_millis(900),
        "show waited for the run in flight: {:?}",
        asked.elapsed()
    );
    assert_eq!(con.runs(), SHOW_VALIDATE_SHOW);
    running.stop().await;
}

/// **The slot is admin-con's own, held across a reconnection**: a verb
/// past its bound runs on while the server restarts, and the fresh
/// connection's admission `show` waits for that run and never runs beside
/// it.
///
/// Perturbation (measured 2026-11-27): free the slot as the invocation's
/// task begins rather than at its end, and the admission's `show` runs
/// beside the `validate`.
#[tokio::test]
async fn the_slot_is_held_across_a_reconnection() {
    let Some(mut con) = Con::open(&["show", "validate"]).await else {
        return;
    };
    con.service.slow("validate", Duration::from_millis(2000));
    let mut running = con.start(GRACE).await;
    con.ask_past_the_bound("validate").await;
    restart_on_its_address(&mut con.lab, || {}).await;
    running
        .wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    con.wait_runs(6).await;
    assert_eq!(con.runs(), SHOW_VALIDATE_SHOW);
    running.stop().await;
}

/// **The stop invokes nothing, though the grants name `unload`** (Spec 8;
/// the operator's ruling of 2026-10-14): admin-con is a conduit, and a
/// shutdown leaves the agent exactly as it stands.
#[tokio::test]
async fn the_stop_invokes_nothing_though_the_grants_name_unload() {
    let Some(con) = Con::open(&["show", "unload"]).await else {
        return;
    };
    let running = con.start(GRACE).await;
    running.stop().await;
    assert_eq!(con.runs(), ["start show", "end show"]);
}

/// **The stop waits for the verb in flight until its run ends**, within its
/// grace, and invokes nothing after it.
#[tokio::test]
async fn the_stop_waits_for_the_run_in_flight_and_invokes_nothing() {
    let Some(con) = Con::open(&["show", "validate", "unload"]).await else {
        return;
    };
    con.service.slow("validate", Duration::from_millis(1500));
    let running = con.start(GRACE).await;
    con.ask_past_the_bound("validate").await;
    running.stop().await;
    assert_eq!(
        con.runs(),
        ["start show", "end show", "start validate", "end validate"]
    );
}

/// **A run that outlasts the stop's grace goes on**: the stop ends at its
/// grace and the run ends at the service after admin-con has stopped, with
/// nothing asked after it.
#[tokio::test]
async fn a_run_that_outlasts_the_grace_goes_on() {
    let Some(con) = Con::open(&["show", "validate", "unload"]).await else {
        return;
    };
    con.service.slow("validate", Duration::from_millis(3000));
    let grace = Duration::from_millis(1000);
    let running = con.start(grace).await;
    con.ask_past_the_bound("validate").await;
    let stopped = tokio::time::Instant::now();
    running.stop().await;
    assert!(
        stopped.elapsed() < Duration::from_millis(2500),
        "the stop kept to its grace: {:?}",
        stopped.elapsed()
    );
    con.wait_runs(4).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        con.runs(),
        ["start show", "end show", "start validate", "end validate"]
    );
}

/// **A `show` lands its constituents on the row** with its date (Spec
/// 2.12), and one naming none writes none.
#[tokio::test]
async fn a_show_lands_its_constituents_on_the_row() {
    let Some(con) = Con::open(&["show"]).await else {
        return;
    };
    con.service.fake.script(Script::answer(
        Some("show"),
        json!({"kind": "state", "state": "idle", "load": {"declaration": "sha-1"},
               "constituents": [4101, 4102, 4103]}),
    ));
    let running = con.start(GRACE).await;
    let row = con
        .lab
        .wait_for(&con.id, "the constituents", |a| a.constituents.is_some())
        .await;
    assert_eq!(row.constituents, Some(vec![4101, 4102, 4103]));
    let first = row.constituents_at.expect("dated");
    verb(&con.lab.listener, &con.id, "show").await.unwrap();
    let row = con
        .lab
        .wait_for(&con.id, "no constituents", |a| a.constituents.is_none())
        .await;
    assert!(row.constituents_at.expect("dated") > first);
    assert_eq!(row.load_state.as_deref(), Some("unloaded"));
    running.stop().await;
}

/// **`not_granted` is audited as never run** (Spec 2.13, 2.14): the server
/// writes the outcome `failed` and raises a failure, never "whether admin
/// acted is unknown".
///
/// Perturbation (measured 2026-11-27): leave `not_granted` out of the
/// listener's `NEVER_RAN`, and the outcome stays unwritten.
#[tokio::test]
async fn not_granted_is_audited_as_never_run() {
    let Some(con) = Con::open(&["show", "load"]).await else {
        return;
    };
    let running = con.start(GRACE).await;
    con.service
        .fake
        .with(|s| s.granted = Some(vec!["show".into()]));
    match verb(&con.lab.listener, &con.id, "load").await {
        Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::NOT_GRANTED),
        other => panic!("expected not_granted, got {other:?}"),
    }
    let until = tokio::time::Instant::now() + SOON;
    let outcome = loop {
        let outcome: Option<Option<String>> = sqlx::query_scalar(
            "SELECT a.outcome FROM audit a JOIN audit f ON a.answers = f.audit_id \
             WHERE f.target_id = $1 AND f.action = 'load'",
        )
        .bind(con.id.as_str())
        .fetch_optional(&con.lab.store.pool)
        .await
        .unwrap();
        if let Some(outcome) = outcome {
            break outcome;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "no answer record on load"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(outcome.as_deref(), Some("failed"));
    let kind = loop {
        let kind: Option<String> =
            sqlx::query_scalar("SELECT kind FROM alert WHERE agent_id = $1 AND subject = 'load'")
                .bind(con.id.as_str())
                .fetch_optional(&con.lab.store.pool)
                .await
                .unwrap();
        if let Some(kind) = kind {
            break kind;
        }
        assert!(tokio::time::Instant::now() < until, "no alert on load");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert_eq!(kind, "failed");
    running.stop().await;
}
