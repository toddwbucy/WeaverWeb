//! conforms: web-a-hellos-admission-is-bounded-from-its-connections-accept-and-leaves-no-live-entry-past-its-deadline
//!
//! **A hello's admission is bounded from its connection's accept** (Spec 8,
//! #151): a committing write never polled past its deadline, refused
//! `store_unavailable` within the connector's own `HELLO_SECS`, leaving no
//! live entry, so the connector's next hello is admitted. Each test drives
//! the real listener and store with a fake connector's hello.
//!
//! The hold on the admission's commit is thread-local (`deadline::hold`),
//! and these tests run on a current-thread runtime, where the listener's
//! tasks run on the test's own thread.

use std::time::Duration;

use super::client::HELLO_SECS;
use super::frames::{FromClient, Plane, Refusal};
use super::tests::{Lab, Registered};
use crate::deadline::hold::{self, Point};

/// The hello answer bound in these tests, the listener's own being
/// `HELLO_ANSWER_SECS` from the accept.
const BOUND: Duration = Duration::from_millis(2000);

/// What a test spends of the bound before the admission begins, once
/// before the connection's task starts and once more before the hello.
const SPENT: Duration = Duration::from_millis(600);

/// How late past the bound a refusal may arrive and still be the bound's:
/// under each step of `SPENT`, so a deadline made at either later instant
/// arrives past it.
const SLACK: Duration = Duration::from_millis(500);

/// gate-con's hello for `agent`.
fn gate_hello(agent: &Registered) -> FromClient {
    FromClient::Hello {
        agent: agent.name.clone(),
        plane: Plane::Gate,
        tail: None,
        ceiling: None,
        door: None,
        capabilities: None,
    }
}

/// **A hello whose admission waits on its agent's row is refused within the
/// connector's bound, counted from the accept**: karl's row is held for
/// update by another transaction, as a stalled store holds it, so the
/// admission waits at its first statement. The connection's task starts
/// `SPENT` after its accept, as a busy runtime would start it, and the
/// hello comes `SPENT` after the handshake. The hello is refused
/// `store_unavailable` at the bound from the accept, well within the
/// connector's `HELLO_SECS`, and no entry stands for the credential. Once
/// the hold goes, the row reads disconnected and the next hello is
/// admitted.
///
/// Perturbations: await the admission with no bound, and no answer comes
/// within the fake's wait. Take the deadline's instant where the task
/// starts, or where the admission begins, and the refusal arrives `SPENT`
/// or twice `SPENT` past the bound.
#[tokio::test]
async fn a_hello_whose_admission_waits_on_a_held_row_is_refused_within_the_connectors_bound() {
    let Some(lab) = Lab::open().await else { return };
    lab.listener.set_hello_answer_bound(BOUND);
    let karl = lab.register("karl").await;

    let mut holder = lab.store.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM agent WHERE agent_id = $1 FOR UPDATE")
        .bind(karl.id.as_str())
        .execute(&mut *holder)
        .await
        .unwrap();

    lab.listener.start_next_late(SPENT);
    let dialed = tokio::time::Instant::now();
    let mut fake = lab.connect(&karl.gate).await;
    tokio::time::sleep(SPENT).await;
    fake.send(gate_hello(&karl)).await;
    fake.expect_refusal_frame(Refusal::StoreUnavailable).await;
    let took = dialed.elapsed();
    assert!(
        fake.closed().await,
        "the connection closes after the refusal"
    );
    assert!(
        took >= BOUND,
        "refused at the bound and not before: {took:?}"
    );
    assert!(
        took < BOUND + SLACK,
        "refused at the bound from the accept, not from a later instant: {took:?}"
    );
    assert!(
        took < Duration::from_secs(HELLO_SECS),
        "refused within the connector's own bound: {took:?}"
    );
    assert!(
        !lab.listener.connected(&karl.id, Plane::Gate),
        "no entry stands for the credential"
    );

    holder.rollback().await.unwrap();
    assert!(!lab.agent(&karl.id).await.gate.connected);
    let _again = lab.admit(&karl, Plane::Gate).await;
}

/// **A hello whose admission has not committed by its deadline leaves no
/// live entry**: karl's admission is held just before its commit until the
/// very deadline its site bounds it by, its entry installed. The hello is
/// refused `store_unavailable`, the entry is gone, the commit was never
/// sent, so the row reads disconnected once the hold goes, and the next
/// hello is admitted rather than refused `already_connected`.
///
/// Perturbations: bound the admission with tokio's `timeout_at` at the same
/// deadline, and the held commit goes out at the deadline's wake, the hold
/// let go and the row reading connected. Leave the installed entry in past
/// the deadline, and it stands after the refusal.
#[tokio::test]
async fn a_hello_whose_admission_commits_past_its_deadline_leaves_no_live_entry() {
    let Some(lab) = Lab::open().await else { return };
    lab.listener.set_hello_answer_bound(BOUND);
    let karl = lab.register("karl").await;

    let held = hold::arm(Point::Admit);
    let mut fake = lab.connect(&karl.gate).await;
    fake.send(gate_hello(&karl)).await;
    fake.expect_refusal(Refusal::StoreUnavailable).await;
    assert!(
        !lab.listener.connected(&karl.id, Plane::Gate),
        "no entry stands for the credential after the refusal"
    );
    assert!(
        !held.let_go(),
        "the commit was never sent: the hold was not let go at the deadline"
    );
    let row = held
        .settled(|| lab.agent(&karl.id), |agent| agent.gate.connected)
        .await;
    assert!(!row.gate.connected, "the commit was never sent: {row:?}");
    drop(held);

    let _again = lab.admit(&karl, Plane::Gate).await;
}
