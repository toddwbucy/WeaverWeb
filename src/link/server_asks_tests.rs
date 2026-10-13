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
use super::tests::{Fake, Lab, Registered, SOON, position, show_answer};
use crate::store::AgentId;
use crate::store::audit::{FAIL_FIRST_RECORD, FAIL_REFUSAL};
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
