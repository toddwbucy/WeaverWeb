//! **The server's own asks, audited** (Spec 2.13; act 11, PR 6b1): every
//! verb frame the server sends has its first record, principal and method
//! `server`, committed before the frame enters the channel, and the outcome
//! naming it when the answer lands. Fake admin-cons drive a real listener,
//! as `link::tests` does, and the audit is read back from the store.
//!
//! Every perturbation below names its guard, and the act that landed it
//! showed the test failing with the guard removed, per Spec section 9.

use super::frames::{FromClient, Plane, Principal, Refusal, ToClient, VerbFault, VerbOutcome};
use super::listener::VerbError;
use super::tests::{Fake, Lab, Registered, SOON, position, position_in, show_answer, trace_event};
use crate::store::AgentId;
use crate::store::audit::{FAIL_FIRST_RECORD, FAIL_REFUSAL, FIRST_FAULT, FirstFault};
use serde_json::json;
use std::time::Duration;

/// One audit record, as the tests read it.
#[derive(Debug, Clone)]
struct Record {
    audit_id: String,
    principal: String,
    person_id: Option<String>,
    method: String,
    target_kind: String,
    action: String,
    answers: Option<String>,
    outcome: Option<String>,
    refusal: Option<String>,
}

type Row = (
    String,
    String,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Every audit record on the agent, oldest first.
async fn records(lab: &Lab, agent: &AgentId) -> Vec<Record> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT audit_id, principal, person_id, method, target_kind, action, answers, \
         outcome, refusal FROM audit WHERE target_id = $1 ORDER BY at, audit_id",
    )
    .bind(agent.as_str())
    .fetch_all(&lab.store.pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(
            |(
                audit_id,
                principal,
                person_id,
                method,
                target_kind,
                action,
                answers,
                outcome,
                refusal,
            )| {
                Record {
                    audit_id,
                    principal,
                    person_id,
                    method,
                    target_kind,
                    action,
                    answers,
                    outcome,
                    refusal,
                }
            },
        )
        .collect()
}

fn firsts(records: &[Record]) -> Vec<Record> {
    records
        .iter()
        .filter(|r| r.answers.is_none() && r.refusal.is_none())
        .cloned()
        .collect()
}

fn outcome_of<'a>(records: &'a [Record], first: &Record) -> Option<&'a str> {
    records
        .iter()
        .find(|r| r.answers.as_deref() == Some(first.audit_id.as_str()))
        .and_then(|r| r.outcome.as_deref())
}

/// Poll the agent's audit until a condition holds, or fail with it.
async fn wait_records(
    lab: &Lab,
    agent: &AgentId,
    what: &str,
    cond: impl Fn(&[Record]) -> bool,
) -> Vec<Record> {
    let until = tokio::time::Instant::now() + SOON;
    loop {
        let records = records(lab, agent).await;
        if cond(&records) {
            return records;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the audit never read {what}: {records:#?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// A first record of the server's `show` on the agent.
fn assert_servers_show(first: &Record) {
    assert_eq!(first.principal, "server");
    assert_eq!(first.method, "server");
    assert_eq!(first.person_id, None);
    assert_eq!(first.target_kind, "agent");
    assert_eq!(first.action, "show");
}

/// Connect admin-con with the ceiling given and take the hello's answer.
async fn hello(lab: &Lab, agent: &Registered, ceiling: &[&str]) -> Fake {
    let mut fake = lab.connect(&agent.admin).await;
    fake.send(FromClient::Hello {
        agent: agent.name.clone(),
        plane: Plane::Admin,
        tail: Some(position(100)),
        ceiling: Some(ceiling.iter().map(|v| (*v).to_owned()).collect()),
        door: Some(true),
    })
    .await;
    match fake.recv().await {
        Some(ToClient::HelloAnswer { .. }) => {}
        other => panic!("expected the hello answer, got {other:?}"),
    }
    fake
}

/// The next frame is the server's `show`: its ask's id.
async fn take_show(fake: &mut Fake) -> u64 {
    match fake.recv().await {
        Some(ToClient::Verb {
            id,
            verb,
            principal,
        }) if verb == "show" => {
            assert_eq!(principal, Principal::Server);
            id
        }
        other => panic!("expected the server's show, got {other:?}"),
    }
}

/// Answer the admission's `show` with a state, take the server's landing
/// and say the replay is caught up: an admission complete.
async fn admitted(lab: &Lab, agent: &Registered, ceiling: &[&str]) -> Fake {
    let mut fake = hello(lab, agent, ceiling).await;
    let id = take_show(&mut fake).await;
    fake.send(FromClient::Verb {
        id,
        outcome: Some(show_answer(&agent.name, "idle", None)),
        error: None,
    })
    .await;
    assert!(matches!(fake.recv().await, Some(ToClient::Landed { id: landed }) if landed == id));
    fake.send(FromClient::CaughtUp).await;
    lab.wait_for(&agent.id, "connected", |a| a.admin.connected)
        .await;
    fake
}

/// admin's refusal of a verb: exit 1 and a `lifecycle-refusal`.
fn refusal_answer(agent: &str) -> VerbOutcome {
    VerbOutcome {
        verb: "show".into(),
        agent: agent.into(),
        exit_code: Some(1),
        answer: Some(json!({ "kind": "unauthorized" })),
        raw_stdout: None,
        stderr: None,
    }
}

async fn disconnected(lab: &Lab, agent: &Registered) {
    lab.wait_for(&agent.id, "admin down", |a| !a.admin.connected)
        .await;
}

/// **Each answer to the server's `show` writes the outcome its contract
/// shape decides, naming the first** (Spec 2.13): `ok` for admin's answer
/// (exit 0), `failed` for admin's refusal (exit 1) and for admin-con's
/// fault on a verb that never ran (`busy`), and no outcome at all for a
/// fault that leaves the act unknown (`unknown`), the first standing alone.
/// The first record is written, and committed, before the frame reaches the
/// connector, with no outcome until the answer lands.
#[tokio::test]
async fn each_answer_to_the_servers_show_writes_its_outcome() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;

    let mut admin = hello(&lab, &karl, &["show"]).await;
    let id = take_show(&mut admin).await;
    let at_the_frame = records(&lab, &karl.id).await;
    assert_eq!(at_the_frame.len(), 1, "{at_the_frame:#?}");
    assert_servers_show(&at_the_frame[0]);
    assert_eq!(outcome_of(&at_the_frame, &at_the_frame[0]), None);
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Landed { .. })));
    let answered = wait_records(&lab, &karl.id, "the ok outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &answered[0]), Some("ok"));
    drop(admin);
    disconnected(&lab, &karl).await;

    // admin's refusal, then admin-con's fault on a verb never run: each
    // closes the admission as incomplete, and each is `failed`.
    let answers = [
        (Some(refusal_answer("karl")), None),
        (
            None,
            Some(VerbFault {
                kind: VerbFault::BUSY.into(),
                message: "busy".into(),
            }),
        ),
    ];
    for (outcome, error) in answers {
        let mut admin = hello(&lab, &karl, &["show"]).await;
        let id = take_show(&mut admin).await;
        admin.send(FromClient::Verb { id, outcome, error }).await;
        admin.expect_refusal(Refusal::AdmissionIncomplete).await;
        disconnected(&lab, &karl).await;
        let records = records(&lab, &karl.id).await;
        let last = firsts(&records).pop().unwrap();
        assert_servers_show(&last);
        assert_eq!(outcome_of(&records, &last), Some("failed"));
    }

    // A fault that leaves the act unknown writes no outcome.
    let mut admin = hello(&lab, &karl, &["show"]).await;
    let id = take_show(&mut admin).await;
    admin
        .send(FromClient::Verb {
            id,
            outcome: None,
            error: Some(VerbFault {
                kind: VerbFault::UNKNOWN.into(),
                message: "past its bound".into(),
            }),
        })
        .await;
    admin.expect_refusal(Refusal::AdmissionIncomplete).await;
    disconnected(&lab, &karl).await;
    let records = records(&lab, &karl.id).await;
    let last = firsts(&records).pop().unwrap();
    assert_eq!(outcome_of(&records, &last), None, "{records:#?}");
    assert_eq!(firsts(&records).len(), 4);
    assert_eq!(records.len(), 7, "four firsts and three outcomes");
}

/// **A ceiling without `show` asks nothing and records nothing** (Spec 7.2,
/// 8): admission completes at `caught_up` with no frame and no record.
#[tokio::test]
async fn a_ceiling_without_show_asks_and_records_nothing() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = hello(&lab, &karl, &["load"]).await;
    admin.send(FromClient::CaughtUp).await;
    lab.wait_for(&karl.id, "connected", |a| a.admin.connected)
        .await;
    assert!(admin.recv().await.is_none(), "no frame follows the answer");
    assert!(lab.listener.connected(&karl.id, Plane::Admin));
    assert!(records(&lab, &karl.id).await.is_empty());
}

/// **Each opening of the trace door asks its own `show` with its own first
/// record, and each answer is matched to its own record only**: the
/// admission's `show` is left in flight while the door closes and opens
/// again, the opening's `show` is answered first with admin's answer and
/// the admission's after it with admin's refusal, and each outcome names
/// the first its ask's id was kept against.
///
/// Perturbation: take the oldest entry of the connection's map instead of
/// the one the answer's id names. The opening's answer then lands as the
/// admission's outcome, `ok` where `failed` is owed.
#[tokio::test]
async fn each_opening_of_the_door_asks_its_own_show_and_each_answer_names_its_own_first() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = hello(&lab, &karl, &["show"]).await;
    let admission = take_show(&mut admin).await;
    admin
        .send(FromClient::Door {
            open: false,
            wall_ms: 1_790_000_000_000,
            tail: None,
        })
        .await;
    admin
        .send(FromClient::Door {
            open: true,
            wall_ms: 1_790_000_000_001,
            tail: Some(position(200)),
        })
        .await;
    let opening = take_show(&mut admin).await;
    assert_ne!(admission, opening);
    let both = wait_records(&lab, &karl.id, "two firsts", |r| firsts(r).len() == 2).await;
    let (first_admission, first_opening) = (both[0].clone(), both[1].clone());
    assert_servers_show(&first_admission);
    assert_servers_show(&first_opening);

    admin
        .send(FromClient::Verb {
            id: opening,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    assert!(
        matches!(admin.recv().await, Some(ToClient::Landed { id }) if id == opening),
        "the opening's show landed"
    );
    admin
        .send(FromClient::Verb {
            id: admission,
            outcome: Some(refusal_answer("karl")),
            error: None,
        })
        .await;
    let answered = wait_records(&lab, &karl.id, "two outcomes", |r| r.len() == 4).await;
    assert_eq!(outcome_of(&answered, &first_opening), Some("ok"));
    assert_eq!(outcome_of(&answered, &first_admission), Some("failed"));
    assert!(lab.listener.connected(&karl.id, Plane::Admin));
}

/// **A first record the audit refuses sends no frame** (Spec 2.13): the
/// admission's `show` is not asked, the connection closes refused as the
/// store unavailable, the row reads disconnected and not admitted, and the
/// reconnect's admission asks again with its record.
///
/// Perturbations: (1) enqueue the frame and write the record after it; (2)
/// enqueue the frame where the record failed. Either way the connector
/// receives the `show` the audit holds no record of.
#[tokio::test]
async fn a_first_record_the_audit_refuses_sends_no_show_and_closes_the_connection() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;

    FAIL_FIRST_RECORD.with(|f| f.set(true));
    let mut admin = hello(&lab, &karl, &["show"]).await;
    admin.expect_refusal(Refusal::StoreUnavailable).await;
    disconnected(&lab, &karl).await;
    assert!(!lab.listener.connected(&karl.id, Plane::Admin));
    assert!(records(&lab, &karl.id).await.is_empty());

    let _again = admitted(&lab, &karl, &["show"]).await;
    let records = records(&lab, &karl.id).await;
    assert_eq!(firsts(&records).len(), 1);
    assert_eq!(outcome_of(&records, &records[0]), Some("ok"));
}

/// **A first record that stalls holds the admission no longer than the
/// silence bound** (Spec 2.13, 8): the store never answers, no `show` is
/// sent, and the connection closes refused as the store unavailable within
/// the bound, as a record refused would.
///
/// Perturbation: write the record unbounded (drop the record step's
/// timeout and the connection task's bound around it). The connection then
/// waits on the store past the bound, and no refusal arrives within it.
#[tokio::test]
async fn a_first_record_that_stalls_closes_the_admission_within_the_bound() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    lab.listener.stall_next_first();
    let mut admin = hello(&lab, &karl, &["show"]).await;
    admin.expect_refusal(Refusal::StoreUnavailable).await;
    disconnected(&lab, &karl).await;
    assert!(records(&lab, &karl.id).await.is_empty());
}

/// **A connection that ends with the `show` in flight writes no outcome**
/// (Spec 2.13): the first stands alone, the audit's "unknown". Both ways a
/// connection ends with it so: the admission's `show` passing its deadline,
/// and the connector gone.
#[tokio::test]
async fn a_connection_that_ends_with_the_show_in_flight_leaves_its_first_alone() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;

    let mut admin = hello(&lab, &karl, &["show"]).await;
    let _unanswered = take_show(&mut admin).await;
    admin.expect_refusal(Refusal::AdmissionIncomplete).await;
    disconnected(&lab, &karl).await;

    let mut admin = hello(&lab, &karl, &["show"]).await;
    let _dropped = take_show(&mut admin).await;
    drop(admin);
    disconnected(&lab, &karl).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    let records = records(&lab, &karl.id).await;
    assert_eq!(firsts(&records).len(), 2, "{records:#?}");
    assert_eq!(records.len(), 2, "no outcome: {records:#?}");
}

/// **The server asks `show` only, and its `show` through `Listener::verb` is
/// audited** (Spec 2.13): `load`, inside the connection's ceiling, is
/// refused before any frame, as one refusal record naming the server; a
/// refusal the audit cannot record answers the server's failure; and the
/// server's `show` writes its first before its frame and its `ok` outcome
/// after the answer.
///
/// Perturbation: drop the show-only refusal in `Listener::verb`. The `load`
/// is asked, its frame reaches admin-con, and no refusal is recorded.
#[tokio::test]
async fn the_server_asks_show_only_and_its_show_is_audited() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = admitted(&lab, &karl, &["show", "load"]).await;
    let before = records(&lab, &karl.id).await.len();

    match lab.listener.verb(&karl.id, "load").await {
        Err(VerbError::ServerAsksShowOnly { verb }) => assert_eq!(verb, "load"),
        other => panic!("expected the show-only refusal, got {other:?}"),
    }
    let refused = records(&lab, &karl.id).await;
    assert_eq!(refused.len(), before + 1);
    let refusal = refused.last().unwrap();
    assert_eq!(refusal.principal, "server");
    assert_eq!(refusal.method, "server");
    assert_eq!(refusal.action, "load");
    assert_eq!(
        refusal.refusal.as_deref(),
        Some("the server asks show only")
    );

    FAIL_REFUSAL.with(|f| f.set(true));
    assert!(matches!(
        lab.listener.verb(&karl.id, "load").await,
        Err(VerbError::Audit)
    ));
    assert!(admin.recv().await.is_none(), "no frame left the server");
    assert_eq!(records(&lab, &karl.id).await.len(), before + 1);

    let listener = lab.listener.clone();
    let asked = karl.id.clone();
    let shown = tokio::spawn(async move { listener.verb(&asked, "show").await });
    let id = take_show(&mut admin).await;
    let at_the_frame = records(&lab, &karl.id).await;
    let first = firsts(&at_the_frame).pop().unwrap();
    assert_servers_show(&first);
    assert_eq!(outcome_of(&at_the_frame, &first), None);
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    shown.await.unwrap().expect("the show was answered");
    let answered = records(&lab, &karl.id).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));
}

/// **The server's ask through `Listener::verb` sends no frame without its
/// first record** (Spec 2.13): a record the audit refuses answers the
/// audit's error at once, and one that stalls answers it within the
/// silence bound; no `show` reaches admin-con either way.
///
/// Perturbations: (1) enqueue before the record step in `Inner::ask`; (2)
/// enqueue where the record failed. The connector receives a `show`. (3)
/// Drop the record step's timeout: the stalled ask is not answered within
/// the bound.
#[tokio::test]
async fn the_servers_ask_sends_no_frame_without_its_first_record() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let mut admin = admitted(&lab, &karl, &["show"]).await;
    let before = records(&lab, &karl.id).await.len();

    FAIL_FIRST_RECORD.with(|f| f.set(true));
    assert!(matches!(
        lab.listener.verb(&karl.id, "show").await,
        Err(VerbError::Audit)
    ));
    lab.listener.stall_next_first();
    let stalled =
        tokio::time::timeout(Duration::from_secs(6), lab.listener.verb(&karl.id, "show")).await;
    assert!(
        matches!(stalled, Ok(Err(VerbError::Audit))),
        "answered the audit's error within the bound: {stalled:?}"
    );
    // Whatever the connection said since, the silence bound's refusal among
    // it, no `show` was asked.
    while let Some(frame) = admin.recv().await {
        assert!(
            !matches!(frame, ToClient::Verb { .. }),
            "a verb frame left without its record: {frame:?}"
        );
    }
    assert_eq!(records(&lab, &karl.id).await.len(), before);
}

/// **An answer in hand has its outcome written whatever closes the
/// connection** (Spec 2.13): the outcome's write is slow, the credential is
/// revoked while it is in flight, and the `ok` outcome still lands, the
/// connection closed by the revocation.
///
/// Perturbation: race the outcome's write against the close signal again.
/// The revocation cancels it, and the answered ask's first stands alone.
#[tokio::test]
async fn an_answered_show_has_its_outcome_though_the_connection_closes_mid_write() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = hello(&lab, &karl, &["show"]).await;
    let id = take_show(&mut admin).await;
    lab.listener.delay_next_outcome(Duration::from_secs(2));
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    // The outcome's write is in flight when the revocation closes the link.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let row = lab.agent(&karl.id).await;
    lab.store
        .revoke_credential(&row, Plane::Admin, Some("lab"))
        .await
        .unwrap();
    disconnected(&lab, &karl).await;
    let records = wait_records(&lab, &karl.id, "the answered show's outcome", |r| {
        firsts(r)
            .first()
            .is_some_and(|f| outcome_of(r, f).is_some())
    })
    .await;
    let first = firsts(&records).remove(0);
    assert_servers_show(&first);
    assert_eq!(outcome_of(&records, &first), Some("ok"));
}

/// **An ask dropped while its frame waits on a full channel leaves nothing
/// behind** (Spec 2.13): admin-con stops reading, acks fill the server's
/// write path, and the server's `show` is cancelled by its caller while its
/// send waits. Its entries leave both of the connection's maps and its
/// first record is answered `failed`, the frame never having left.
///
/// Perturbation: disarm the `Unsent` guard as soon as it is installed. The
/// cancelled ask's entries stay for the connection's life and its first
/// stands alone.
#[tokio::test]
async fn an_ask_dropped_before_its_frame_leaves_answers_its_first_failed() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = admitted(&lab, &karl, &["show"]).await;
    // Acks that outrun every buffer between the server's queue and this
    // reader, which never reads them, so the write channel stays full.
    let generation = "g".repeat(4096);
    let events = 5_000u64;
    for n in 1..=events {
        admin
            .send(FromClient::Event {
                position: position_in(&generation, 100 + n),
                replayed: false,
                event: trace_event(n, "turn", json!({})),
            })
            .await;
    }
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&karl.id).map(|p| p.offset) != Some(100 + events) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the last"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(lab.listener.held_asks(&karl.id), Some((0, 0)));

    let cancelled = tokio::time::timeout(
        Duration::from_millis(500),
        lab.listener.verb(&karl.id, "show"),
    )
    .await;
    assert!(
        cancelled.is_err(),
        "the send waited on the full channel: {cancelled:?}"
    );
    assert_eq!(
        lab.listener.held_asks(&karl.id),
        Some((0, 0)),
        "the cancelled ask left an entry behind"
    );
    let records = wait_records(&lab, &karl.id, "the cancelled show's outcome", |r| {
        let firsts = firsts(r);
        firsts.len() == 2 && outcome_of(r, &firsts[1]).is_some()
    })
    .await;
    let cancelled = firsts(&records).remove(1);
    assert_servers_show(&cancelled);
    assert_eq!(outcome_of(&records, &cancelled), Some("failed"));
}

/// Admitted with `show` granted, the admission's own records written: the
/// count to read the server's next ask against.
async fn ready(lab: &Lab, karl: &Registered) -> (Fake, usize) {
    let admin = admitted(lab, karl, &["show"]).await;
    let before = records(lab, &karl.id).await.len();
    (admin, before)
}

/// The newest first record on the agent and its outcome, once it has one.
async fn newest_answered(lab: &Lab, agent: &AgentId, what: &str) -> (Record, Option<String>) {
    let records = wait_records(lab, agent, what, |r| {
        firsts(r).last().is_some_and(|f| outcome_of(r, f).is_some()) && firsts(r).len() >= 2
    })
    .await;
    let first = firsts(&records).pop().unwrap();
    let outcome = outcome_of(&records, &first).map(str::to_owned);
    (first, outcome)
}

/// **A first record that commits after its asker left is answered
/// `failed`** (Spec 2.13): the insert runs in a task the asker cannot
/// cancel, its answer is held past the asker's own timeout, and the guard
/// the task hands back finds no one to take it and answers the first,
/// whose frame never left.
#[tokio::test]
async fn a_first_committed_after_its_asker_left_is_answered_failed() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let (mut admin, _) = ready(&lab, &karl).await;
    FIRST_FAULT.with(|f| f.set(Some(FirstFault::DelayAnswer(Duration::from_secs(1)))));
    let left = tokio::time::timeout(
        Duration::from_millis(200),
        lab.listener.verb(&karl.id, "show"),
    )
    .await;
    assert!(left.is_err(), "the asker left before the answer: {left:?}");
    let (first, outcome) = newest_answered(&lab, &karl.id, "the left ask's outcome").await;
    assert_servers_show(&first);
    assert_eq!(outcome.as_deref(), Some("failed"));
    assert!(admin.recv().await.is_none(), "no frame left the server");
    assert_eq!(lab.listener.held_asks(&karl.id), Some((0, 0)));
}

/// **A first record whose answer is lost after its commit is answered
/// `failed`**: the guard was armed before the insert, so the task that
/// lost the answer still holds it, and its drop answers the first.
///
/// Perturbation: arm the guard only once the insert has answered. The lost
/// answer leaves it unarmed, and the committed first stands alone.
#[tokio::test]
async fn a_first_whose_answer_is_lost_after_its_commit_is_answered_failed() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let (mut admin, _) = ready(&lab, &karl).await;
    FIRST_FAULT.with(|f| f.set(Some(FirstFault::LoseAnswer)));
    assert!(matches!(
        lab.listener.verb(&karl.id, "show").await,
        Err(VerbError::Audit)
    ));
    let (first, outcome) = newest_answered(&lab, &karl.id, "the lost answer's outcome").await;
    assert_servers_show(&first);
    assert_eq!(outcome.as_deref(), Some("failed"));
    assert!(admin.recv().await.is_none(), "no frame left the server");
}

/// **A `failed` outcome waits for the first's transaction to end**: the
/// first's answer is lost while its COMMIT is still to land, as a
/// connection that fails while its backend completes the commit; the
/// `failed` write takes the first's outcome lock, so it runs only once the
/// commit has landed, and then finds the first and answers it.
///
/// Perturbation: drop the lock from the `failed` write. It runs at once,
/// finds no first yet, writes nothing, and the first that commits after it
/// stands alone: Codex's interleaving, staged.
#[tokio::test]
async fn a_failed_outcome_waits_for_its_firsts_transaction_to_end() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let (_admin, before) = ready(&lab, &karl).await;
    FIRST_FAULT.with(|f| f.set(Some(FirstFault::CommitLate(Duration::from_secs(1)))));
    assert!(matches!(
        lab.listener.verb(&karl.id, "show").await,
        Err(VerbError::Audit)
    ));
    let at_the_answer = records(&lab, &karl.id).await;
    assert_eq!(at_the_answer.len(), before, "nothing committed yet");
    let (first, outcome) = newest_answered(&lab, &karl.id, "the late commit's outcome").await;
    assert_servers_show(&first);
    assert_eq!(outcome.as_deref(), Some("failed"));
}

/// **A first record that never commits gets no outcome and nothing else**:
/// its guard's `failed` write finds no first and lands nothing.
#[tokio::test]
async fn a_first_that_never_commits_leaves_nothing() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let (mut admin, before) = ready(&lab, &karl).await;
    FIRST_FAULT.with(|f| f.set(Some(FirstFault::Abort)));
    assert!(matches!(
        lab.listener.verb(&karl.id, "show").await,
        Err(VerbError::Audit)
    ));
    assert!(admin.recv().await.is_none(), "no frame left the server");
    assert_eq!(records(&lab, &karl.id).await.len(), before);
    assert_eq!(lab.listener.held_asks(&karl.id), Some((0, 0)));
}

/// **A verb answer carrying both an outcome and a fault, or neither, is
/// the peer's protocol violation** (Spec 7.2, 8): the answer is classified
/// once, and one of neither shape is refused as malformed, the connection
/// closed; the asker never reads it as a success, and its first record has
/// no outcome, since what admin did cannot be said.
///
/// Perturbation: classify a frame with both as admin's outcome. The asker
/// gets a success and the audit `ok` for a contradictory answer.
#[tokio::test]
async fn a_verb_answer_with_both_or_neither_shape_is_refused_as_malformed() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let (mut admin, _) = ready(&lab, &karl).await;
    let listener = lab.listener.clone();
    let asked = karl.id.clone();
    let shown = tokio::spawn(async move { listener.verb(&asked, "show").await });
    let id = take_show(&mut admin).await;
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer("karl", "idle", None)),
            error: Some(VerbFault {
                kind: VerbFault::BUSY.into(),
                message: "busy".into(),
            }),
        })
        .await;
    admin.expect_refusal(Refusal::Malformed).await;
    match shown.await.unwrap() {
        Err(VerbError::Unanswered) => {}
        other => panic!("expected no answer for a contradictory frame, got {other:?}"),
    }
    disconnected(&lab, &karl).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after_both = records(&lab, &karl.id).await;
    let both = firsts(&after_both).pop().unwrap();
    assert_servers_show(&both);
    assert_eq!(outcome_of(&after_both, &both), None, "{after_both:#?}");

    // An admission's show answered with neither.
    let mut admin = hello(&lab, &karl, &["show"]).await;
    let id = take_show(&mut admin).await;
    admin
        .send(FromClient::Verb {
            id,
            outcome: None,
            error: None,
        })
        .await;
    admin.expect_refusal(Refusal::Malformed).await;
    disconnected(&lab, &karl).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let after_neither = records(&lab, &karl.id).await;
    let neither = firsts(&after_neither).pop().unwrap();
    assert_ne!(neither.audit_id, both.audit_id);
    assert_eq!(
        outcome_of(&after_neither, &neither),
        None,
        "{after_neither:#?}"
    );
}

/// **Only evidence writes an outcome**: a fault writes `failed` only where
/// its kind proves the verb never ran. One this server does not know, as
/// a connector of another version might send, and `not_landed`, which is
/// the server's own and never admin-con's word, each leave the first with
/// no second.
///
/// Perturbation: go back to a deny-list of the unknown kinds. The invented
/// kind writes `failed`.
#[tokio::test]
async fn a_fault_of_a_kind_that_proves_nothing_writes_no_outcome() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    for kind in ["gremlin", VerbFault::NOT_LANDED] {
        let mut admin = hello(&lab, &karl, &["show"]).await;
        let id = take_show(&mut admin).await;
        admin
            .send(FromClient::Verb {
                id,
                outcome: None,
                error: Some(VerbFault {
                    kind: kind.into(),
                    message: "no evidence".into(),
                }),
            })
            .await;
        admin.expect_refusal(Refusal::AdmissionIncomplete).await;
        disconnected(&lab, &karl).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let records = records(&lab, &karl.id).await;
        let first = firsts(&records).pop().unwrap();
        assert_eq!(outcome_of(&records, &first), None, "{kind}: {records:#?}");
    }
}

/// **The admission's deadline is read at the await it bounds**: the
/// outcome's write takes the admission past its deadline, and the landing
/// then has nothing left, so the admission fails at its deadline rather
/// than completing late.
///
/// Perturbation: compute what remains of the deadline before the outcome's
/// write. The landing gets the stale remainder and the admission completes
/// after its deadline.
#[tokio::test]
async fn an_admission_answered_near_its_deadline_does_not_complete_past_it() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let mut admin = hello(&lab, &karl, &["show"]).await;
    let id = take_show(&mut admin).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    lab.listener.delay_next_outcome(Duration::from_millis(2500));
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    admin.expect_refusal(Refusal::AdmissionIncomplete).await;
    disconnected(&lab, &karl).await;
    // The answer's outcome still landed: the answer was in hand.
    let records = records(&lab, &karl.id).await;
    let first = firsts(&records).pop().unwrap();
    assert_eq!(outcome_of(&records, &first), Some("ok"));
    assert_ne!(
        lab.agent(&karl.id).await.state_source.as_deref(),
        Some("show"),
        "the late show never landed on the row"
    );
}

/// **Only the contract's evidence writes an outcome, in every field the
/// audit reads**: admin's exit status is 0 for an answer and 1 for a
/// refusal, and an outcome with no status or any other one is a fault by
/// the contract, so each leaves the first with no second.
///
/// Perturbation: go back to `exit_code == Some(0)` as the only test. Exit 2
/// writes `failed`.
#[tokio::test]
async fn an_outcome_whose_exit_status_proves_nothing_writes_no_outcome() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let (mut admin, _) = ready(&lab, &karl).await;
    for exit_code in [None, Some(2)] {
        let listener = lab.listener.clone();
        let asked = karl.id.clone();
        let shown = tokio::spawn(async move { listener.verb(&asked, "show").await });
        let id = take_show(&mut admin).await;
        let mut outcome = show_answer("karl", "idle", None);
        outcome.exit_code = exit_code;
        admin
            .send(FromClient::Verb {
                id,
                outcome: Some(outcome),
                error: None,
            })
            .await;
        shown
            .await
            .unwrap()
            .expect("the asker reads admin's answer");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let records = records(&lab, &karl.id).await;
        let first = firsts(&records).pop().unwrap();
        assert_servers_show(&first);
        assert_eq!(
            outcome_of(&records, &first),
            None,
            "exit {exit_code:?}: {records:#?}"
        );
    }
}
