//! **The alerts' bounded writes are never sent once their deadline has
//! come** (Spec 2.14, 7.2; `crate::deadline`): the four sites of
//! `src/alerting.rs` the sweep of 2026-11-12 found awaiting a store write
//! under tokio's timeouts. Each test holds one write just before the
//! statement that commits it until the very deadline its site bounds it by
//! (`deadline::hold`), drives the site through the alerts' own worker or
//! drainer on a store of its own, takes the caller's report of the bound
//! passing, its operations-log line or its error line, and reads the store.
//!
//! Every perturbation is the same: bound the site's write with tokio's
//! `timeout_at` at the same deadline again. The held statement then goes
//! out at the deadline's wake, which polls the write before the bound reads
//! its deadline, and its effect lands while the caller reports it unmade.

use std::future::Future;
use std::path::Path;
use std::time::Duration;

use serde_json::json;
use tokio::time::Instant;

use crate::agents_tests::Captured;
use crate::alerting::{Alerts, Failure, STORE_BOUND};
use crate::deadline::hold::{self, Point};
use crate::store::alerts::{AlertKind, Raise, TakenEntry};
use crate::store::read::tests::fresh_store;
use crate::store::{AgentId, Store};
use crate::verbs_tests::global_capture;

/// The slack past a bound before a test gives up on what it waits for.
const SOON: Duration = Duration::from_secs(5);

/// A run as a connector draws one.
const RUN: &str = "0123456789abcdef";

/// An agent on the store, its fingerprints a fixture's alone.
async fn agent(store: &Store) -> AgentId {
    let (id, _) = store
        .register_agent(
            "box-a",
            "karl",
            Some("lab"),
            &"a".repeat(64),
            &"b".repeat(64),
            &"c".repeat(64),
        )
        .await
        .unwrap();
    id
}

fn raise(agent: &AgentId, kind: AlertKind, subject: &str) -> Raise {
    Raise {
        agent: agent.clone(),
        kind,
        subject: subject.into(),
        summary: format!("{subject} failed"),
        detail: String::new(),
        first: None,
        options: Vec::new(),
        source: "server",
        once: false,
    }
}

/// **The audit's verdict on a held write**, as `link::deadline_tests` reads
/// it: where the hold was let go at its deadline, its statement went out
/// and commits a moment later, so the read waits for its effect; where it
/// was not, nothing is in flight and the read is final. A verdict stands
/// only where the write reached its hold: one that stalled before it until
/// the deadline lands nothing in either order, so that run fails here.
async fn settled<T, F, Fut>(held: &hold::Armed, read: F, landed: impl Fn(&T) -> bool) -> T
where
    F: Fn() -> Fut,
    Fut: Future<Output = T>,
{
    let until = Instant::now() + SOON;
    loop {
        let now = read().await;
        if !held.let_go() || landed(&now) || Instant::now() >= until {
            assert!(held.reached(), "the write never reached its hold");
            return now;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// **The caller's error line**: one line of the process's capture, written
/// since `from`, holding every needle, within the bound and the slack.
async fn logged(capture: &Captured, from: usize, needles: &[&str]) -> bool {
    let until = Instant::now() + STORE_BOUND + SOON;
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

fn mark(capture: &Captured) -> usize {
    capture.0.lock().unwrap().len()
}

/// The held write has reached its hold: the drainer is inside the row's
/// work, past the look that claimed it.
async fn reached(held: &hold::Armed) {
    let until = Instant::now() + SOON;
    while !held.reached() {
        assert!(Instant::now() < until, "the write never reached its hold");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The operations log's first line, once written.
async fn first_line(log: &Path) -> serde_json::Value {
    let until = Instant::now() + STORE_BOUND + SOON;
    loop {
        if let Ok(text) = std::fs::read_to_string(log)
            && let Some(line) = text.lines().next()
        {
            return serde_json::from_str(line).expect("a JSON line");
        }
        assert!(Instant::now() < until, "the operations log stayed empty");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// A connector's own failure, taken into the inbox: a row that raises a
/// `failed` alert when applied. Its identity.
async fn inbox_row(store: &Store, agent: &AgentId) -> i64 {
    let entry = json!({"kind": "probe", "message": "held"});
    assert!(
        store
            .take_entry(&TakenEntry {
                run: RUN,
                seq: 1,
                agent,
                plane: "admin",
                source: "admin-con",
                entry: &entry,
            })
            .await
            .unwrap()
    );
    sqlx::query_scalar("SELECT inbox_id FROM entry_inbox WHERE run_id = $1")
        .bind(RUN)
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

/// The row's back-offs, `None` where the row is gone.
async fn attempts(store: &Store, inbox_id: i64) -> Option<i32> {
    sqlx::query_scalar("SELECT attempts FROM entry_inbox WHERE inbox_id = $1")
        .bind(inbox_id)
        .fetch_optional(&store.pool)
        .await
        .unwrap()
}

/// **An alert not raised by the worker's bound was never sent, and the
/// operations log and the store agree** (Spec 2.14, `alerting::raised`): a
/// failure is raised, its write held to the bound, and the worker logs the
/// raise as not made and appends the line naming no alert; the store holds
/// no alert.
///
/// Perturbation: bound the raise with tokio's `timeout_at`. The line names
/// no alert, and the store holds one.
#[tokio::test]
async fn an_alert_unraised_at_its_bound_was_never_sent_and_its_line_names_none() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let store = fresh.store.clone();
    let karl = agent(&store).await;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("operations.log");
    let capture = global_capture();
    let from = mark(capture);
    let held = hold::arm(Point::Raise);
    let (alerts, tasks) = Alerts::start(store.clone(), Some(log.clone()));
    alerts.raise(Failure {
        raise: raise(&karl, AlertKind::Failed, "load"),
        plane: "admin",
        principal: "server".into(),
        outcome: "failed",
        reason: json!({}),
    });
    let line = first_line(&log).await;
    assert_eq!(line["agent"], karl.as_str());
    assert!(line["alert"].is_null(), "the line names an alert: {line}");
    assert!(
        logged(
            capture,
            from,
            &[karl.as_str(), "an alert was not raised within"]
        )
        .await,
        "the worker never logged the raise as not made"
    );
    let raised = settled(
        &held,
        || async { store.alerts_on(&karl, 10).await.unwrap() },
        |a| !a.is_empty(),
    )
    .await;
    assert!(
        raised.is_empty(),
        "the store holds an alert the operations log says was not raised: {raised:#?}"
    );
    assert!(
        !held.let_go(),
        "the raise's statement went out at the bound"
    );
    tasks.abort();
}

/// **A restoration not noted by the worker's bound was never sent** (Spec
/// 2.14, the worker's restoration): the open lost-contact alert stands,
/// contact is restored, the note is held to the bound, and the worker logs
/// it as not noted; the alert carries no restoration.
///
/// Perturbation: bound the note with tokio's `timeout_at`. The line says
/// the restoration was not noted, and the alert carries it.
#[tokio::test]
async fn a_restoration_unnoted_at_its_bound_was_never_sent() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let store = fresh.store.clone();
    let karl = agent(&store).await;
    let lost = store
        .raise_alert(&raise(&karl, AlertKind::LostContact, "admin"))
        .await
        .unwrap()
        .expect("the loss raised");
    let capture = global_capture();
    let from = mark(capture);
    let held = hold::arm(Point::Restored);
    let (alerts, tasks) = Alerts::start(store.clone(), None);
    alerts.restored(&karl, "admin");
    assert!(
        logged(
            capture,
            from,
            &[karl.as_str(), "contact's restoration was not noted within"]
        )
        .await,
        "the worker never logged the restoration as not noted"
    );
    let restored = settled(
        &held,
        || async { store.alert(&lost).await.unwrap().unwrap().restored_at },
        Option::is_some,
    )
    .await;
    assert_eq!(
        restored, None,
        "the alert carries a restoration the worker logged as not noted"
    );
    assert!(!held.let_go(), "the note's statement went out at the bound");
    tasks.abort();
}

/// **An inbox row not applied by the drainer's bound was never committed**
/// (Spec 2.14, the drainer): a connector's failure is taken, its row's
/// claim held to the bound before its commit, and the drainer logs the row
/// as not applied; the row stands, its raise rolled back with the claim.
/// The drainer is stalled at its next look, so the row's retry does not
/// apply it before the read.
///
/// Perturbation: bound the row's work with tokio's `timeout_at`. The line
/// says the row was not applied, and the row is gone and its alert raised.
#[tokio::test]
async fn an_inbox_row_unapplied_at_its_bound_was_never_committed() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let store = fresh.store.clone();
    let karl = agent(&store).await;
    let inbox_id = inbox_row(&store, &karl).await;
    let dir = tempfile::tempdir().unwrap();
    let capture = global_capture();
    let from = mark(capture);
    let held = hold::arm(Point::Apply);
    let (alerts, tasks) = Alerts::start(store.clone(), Some(dir.path().join("operations.log")));
    reached(&held).await;
    let _stalled = alerts.stall_worker().await;
    let not_applied = format!("an inbox row ({inbox_id}) was not applied within");
    assert!(
        logged(capture, from, &[&not_applied]).await,
        "the drainer never logged the row as not applied"
    );
    let (standing, raised) = settled(
        &held,
        || async {
            (
                attempts(&store, inbox_id).await,
                store.alerts_on(&karl, 10).await.unwrap().len(),
            )
        },
        |(standing, raised)| standing.is_none() || *raised > 0,
    )
    .await;
    assert!(
        standing.is_some() && raised == 0,
        "the store applied a row the drainer logged as not applied: row {standing:?}, {raised} alerts"
    );
    assert!(!held.let_go(), "the claim's commit went out at the bound");
    tasks.abort();
}

/// **A back-off not written by its bound was never sent** (Spec 2.14, the
/// drainer): a row of a run whose every row fails is taken, its back-off
/// held to the bound, and the drainer logs it as not backed off; the row
/// stands as it was, never backed off. The drainer is stalled at its next
/// look, so the row's retry does not back it off before the read.
///
/// Perturbation: bound the back-off with tokio's `timeout_at`. The line
/// says the row was not backed off, and the row was.
#[tokio::test]
async fn a_back_off_unwritten_at_its_bound_was_never_sent() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let store = fresh.store.clone();
    let karl = agent(&store).await;
    let inbox_id = inbox_row(&store, &karl).await;
    let capture = global_capture();
    let from = mark(capture);
    let held = hold::arm(Point::InboxFailed);
    let (alerts, tasks) = Alerts::start(store.clone(), None);
    // Before the drainer first runs: on this runtime it runs at the test's
    // first await.
    alerts.poison_run(RUN);
    reached(&held).await;
    let _stalled = alerts.stall_worker().await;
    let not_backed_off = format!("an inbox row ({inbox_id}) was not backed off within");
    assert!(
        logged(capture, from, &[&not_backed_off]).await,
        "the drainer never logged the row as not backed off"
    );
    let backed_off = settled(
        &held,
        || attempts(&store, inbox_id),
        |a| a.is_some_and(|a| a > 0),
    )
    .await;
    assert_eq!(
        backed_off,
        Some(0),
        "the store backed off a row the drainer logged as not backed off"
    );
    assert!(
        !held.let_go(),
        "the back-off's statement went out at the bound"
    );
    tasks.abort();
}
