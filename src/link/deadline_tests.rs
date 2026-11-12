//! **The listener's bounded audit writes are never sent once their deadline
//! has come** (Spec 2.13, 7.2; `crate::deadline`): the five sites the sweep
//! of 2026-11-12 found awaiting a store write under tokio's timeouts. Each
//! test holds one write just before the statement that commits it until
//! the very deadline its site bounds it by (`deadline::hold`), drives the
//! site through the real listener and fake admin-cons, takes the caller's
//! report of the bound passing, its answer or its error line, and reads the
//! audit.
//!
//! Every perturbation is the same: bound the site's write with tokio's
//! `timeout_at` at the same deadline again. The held statement then goes
//! out at the deadline's wake, which polls the write before the bound reads
//! its deadline, and its row lands while the caller reports it unwritten.

use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;

use super::frames::FromClient;
use super::listener::{Act, VerbError};
use super::server_asks_tests::{admitted, hello, take_show};
use super::tests::{Lab, SOON, operator, show_answer};
use crate::agents_tests::Captured;
use crate::deadline::hold::{self, Point};
use crate::store::AgentId;
use crate::store::audit::{FIRST_FAULT, FirstFault};
use crate::verbs_tests::global_capture;

/// The silence bound, and so every site's bound: short, so each held
/// write waits seconds and not a minute.
const BOUND: Duration = Duration::from_secs(4);

/// The first records on the agent, oldest first.
async fn firsts(lab: &Lab, agent: &AgentId) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT audit_id FROM audit WHERE target_id = $1 AND answers IS NULL \
         AND refusal IS NULL ORDER BY at, audit_id",
    )
    .bind(agent.as_str())
    .fetch_all(&lab.store.pool)
    .await
    .unwrap()
}

/// The outcome answering a first, where one stands.
async fn outcome_of(lab: &Lab, first: &str) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT outcome FROM audit WHERE answers = $1")
        .bind(first)
        .fetch_optional(&lab.store.pool)
        .await
        .unwrap()
        .flatten()
}

/// The refusal records on the agent.
async fn refusals(lab: &Lab, agent: &AgentId) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM audit WHERE target_id = $1 AND refusal IS NOT NULL")
        .bind(agent.as_str())
        .fetch_one(&lab.store.pool)
        .await
        .unwrap()
}

/// **The audit's verdict on a held write**: where the hold was let go at
/// its deadline, its statement went out and commits a moment later, so the
/// read waits for the row, which the assertion then shows; where it was
/// not, nothing is in flight and the read is final.
async fn settled<T, F, Fut>(held: &hold::Armed, read: F, landed: impl Fn(&T) -> bool) -> T
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
{
    let until = Instant::now() + SOON;
    loop {
        let now = read().await;
        if !held.let_go() || landed(&now) || Instant::now() >= until {
            return now;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// **The caller's error line**: one line of the process's capture, written
/// since `from`, holding every needle, within a bound and the slack past it.
async fn logged(capture: &Captured, from: usize, needles: &[&str]) -> bool {
    let until = Instant::now() + BOUND + SOON;
    loop {
        let found = {
            let bytes = capture.0.lock().unwrap();
            String::from_utf8_lossy(&bytes[from.min(bytes.len())..])
                .lines()
                .any(|line| needles.iter().all(|needle| line.contains(needle)))
        };
        if found {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Where the capture stands now: the lines a test reads come after it.
fn mark(capture: &Captured) -> usize {
    capture.0.lock().unwrap().len()
}

/// **A person's refusal not recorded by the ask's deadline was never sent**
/// (Spec 2.13, `listener.rs`, `Inner::ask_held`): bea holds `operator` on
/// karl, whose admin-con is not connected, so her `load` is refused past
/// the grant check as not connected. The refusal's record is held to the
/// ask's one deadline, and the ask answers the bound's failure, 503 on the
/// page; the audit holds no refusal.
///
/// Perturbation: bound the refusal's record with tokio's `timeout_at`. The
/// person is told the ask was not made within its bound, and the audit
/// holds the refusal naming not connected.
#[tokio::test]
async fn a_persons_refusal_unrecorded_at_the_asks_deadline_was_never_sent() {
    let Some(lab) = Lab::open_with(BOUND).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let bea = operator(&lab.store, &karl.id).await;
    let held = hold::arm(Point::Refusal);
    let admission = lab
        .listener
        .admit_ask(&bea.person_id)
        .expect("within bea's share");
    let asked = tokio::time::timeout(
        BOUND + SOON,
        lab.listener
            .ask_as_person(admission, &bea, &karl.id, Act::Verb("load".into())),
    )
    .await
    .expect("the ask answered within its bound");
    assert!(
        matches!(asked, Err(VerbError::Stalled)),
        "expected the bound's failure, got {:?}",
        asked.err()
    );
    let refusals = settled(&held, || refusals(&lab, &karl.id), |n| *n > 0).await;
    assert_eq!(
        refusals, 0,
        "the audit holds a refusal the person was told was not made"
    );
    assert!(
        !held.let_go(),
        "the refusal's statement went out at the deadline"
    );
}

/// **The server's refusal not recorded by its deadline was never sent**
/// (Spec 2.13, `Listener::verb`): the server's `load` is refused before any
/// frame, its record held to the silence bound, and the verb answers the
/// audit's failure; the audit holds no refusal.
///
/// Perturbation: bound the refusal's record with tokio's `timeout_at`. The
/// verb answers that the audit could not record it, and the audit holds
/// "the server asks show only".
#[tokio::test]
async fn the_servers_refusal_unrecorded_at_its_deadline_was_never_sent() {
    let Some(lab) = Lab::open_with(BOUND).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let held = hold::arm(Point::Refusal);
    let refused = tokio::time::timeout(BOUND + SOON, lab.listener.verb(&karl.id, "load"))
        .await
        .expect("the verb answered within its bound");
    assert!(
        matches!(refused, Err(VerbError::Audit)),
        "expected the audit's failure, got {refused:?}"
    );
    let refusals = settled(&held, || refusals(&lab, &karl.id), |n| *n > 0).await;
    assert_eq!(
        refusals, 0,
        "the audit holds a refusal the verb answered as unrecorded"
    );
    assert!(
        !held.let_go(),
        "the refusal's statement went out at the deadline"
    );
}

/// **A verb's outcome not written by its deadline was never sent** (Spec
/// 2.13, `Inner::record_outcome`): the admission's `show` is answered, its
/// outcome held to the silence bound, and the listener logs the outcome as
/// not written; its first stands alone, the audit's unknown.
///
/// Perturbation: bound the outcome with tokio's `timeout_at`. The line says
/// the outcome was not written, and the audit holds it, `ok`.
///
/// The Planner named the listener's `delay_next_outcome` seam for this
/// site. It sleeps before the write takes a connection, which then pings
/// and prepares, two round trips, so the outcome goes out past its deadline
/// only where one of them is answered across it: a rate, not a verdict.
/// The hold sleeps after both, at the statement.
#[tokio::test]
async fn an_outcome_unwritten_at_its_deadline_was_never_sent() {
    let Some(lab) = Lab::open_with(BOUND).await else {
        return;
    };
    let capture = global_capture();
    let karl = lab.register("karl").await;
    let mut admin = hello(&lab, &karl, &["show"]).await;
    let id = take_show(&mut admin).await;
    let first = firsts(&lab, &karl.id).await;
    assert_eq!(first.len(), 1, "the admission's first: {first:?}");
    let first = first[0].clone();
    let from = mark(capture);
    let held = hold::arm(Point::Outcome);
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    assert!(
        logged(
            capture,
            from,
            &[karl.id.as_str(), &first, "was not written within"]
        )
        .await,
        "the listener never logged the outcome as not written"
    );
    let outcome = settled(&held, || outcome_of(&lab, &first), Option::is_some).await;
    assert_eq!(
        outcome, None,
        "the audit holds an outcome the listener logged as not written"
    );
    assert!(
        !held.let_go(),
        "the outcome's statement went out at the deadline"
    );
}

/// **A first record not written by its deadline was never committed**
/// (Spec 2.13, `Inner::record_ask`): the server's `show` takes its record
/// step, whose insert is held to its own bound before its commit; the
/// asker answers the audit's failure, the step logs the first as not
/// written, and the audit holds no first for the ask and so no outcome.
///
/// Perturbation: bound the insert with tokio's `timeout_at`. The step logs
/// the first as not written, and the audit holds it, answered `failed` by
/// the guard the step dropped.
#[tokio::test]
async fn a_first_unwritten_at_its_deadline_was_never_committed() {
    let Some(lab) = Lab::open_with(BOUND).await else {
        return;
    };
    let capture = global_capture();
    let karl = lab.register("karl").await;
    let _admin = admitted(&lab, &karl, &["show"]).await;
    let before = firsts(&lab, &karl.id).await.len();
    let from = mark(capture);
    let held = hold::arm(Point::FirstRecord);
    let asked = tokio::time::timeout(BOUND + SOON, lab.listener.verb(&karl.id, "show"))
        .await
        .expect("the ask answered within its bound");
    assert!(
        matches!(asked, Err(VerbError::Audit)),
        "expected the audit's failure, got {asked:?}"
    );
    assert!(
        logged(
            capture,
            from,
            &[
                karl.id.as_str(),
                "the first record",
                "was not written within"
            ]
        )
        .await,
        "the record step never logged the first as not written"
    );
    let firsts = settled(&held, || firsts(&lab, &karl.id), |f| f.len() > before).await;
    assert_eq!(
        firsts.len(),
        before,
        "the audit holds a first the record step logged as not written: {firsts:?}"
    );
    assert!(
        !held.let_go(),
        "the first's commit went out at the deadline"
    );
}

/// **A `failed` outcome not written by its deadline was never committed**
/// (Spec 2.13, `Unsent`): the server's `show` has its first committed and
/// its answer lost, so the guard answers the first `failed`, its write held
/// to the silence bound before its commit; the guard logs it as not
/// written, and the first stands alone.
///
/// Perturbation: bound the `failed` write with tokio's `timeout_at`. The
/// guard logs it as not written, and the audit holds it.
#[tokio::test]
async fn a_failed_outcome_unwritten_at_its_deadline_was_never_committed() {
    let Some(lab) = Lab::open_with(BOUND).await else {
        return;
    };
    let capture = global_capture();
    let karl = lab.register("karl").await;
    let _admin = admitted(&lab, &karl, &["show"]).await;
    let before = firsts(&lab, &karl.id).await.len();
    let from = mark(capture);
    let held = hold::arm(Point::Unsent);
    FIRST_FAULT.with(|f| f.set(Some(FirstFault::LoseAnswer)));
    assert!(matches!(
        lab.listener.verb(&karl.id, "show").await,
        Err(VerbError::Audit)
    ));
    let firsts = firsts(&lab, &karl.id).await;
    assert_eq!(
        firsts.len(),
        before + 1,
        "the lost answer's first committed"
    );
    let first = firsts.last().unwrap().clone();
    assert!(
        logged(
            capture,
            from,
            &[
                karl.id.as_str(),
                &first,
                "its frame never sent, was not written within"
            ]
        )
        .await,
        "the guard never logged the failed outcome as not written"
    );
    let outcome = settled(&held, || outcome_of(&lab, &first), Option::is_some).await;
    assert_eq!(
        outcome, None,
        "the audit holds a failed outcome the guard logged as not written"
    );
    assert!(
        !held.let_go(),
        "the failed outcome's commit went out at the deadline"
    );
}
