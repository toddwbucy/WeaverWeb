//! **The row is true when a run ends** (Spec 2.12, 7.2): the server asks
//! `show` at every close of the trace door where the ceiling grants it, as
//! its own audited ask holding nothing, and asks again, a bounded number of
//! times, while the door stays closed and `show` answers `in_transition`.
//! Against the real listener, the real admin-con, a fake relay
//! (`fake_relay`) and a fake invoker; no agent is reached, and no name is a
//! real one.
//!
//! A run's end closes the door however it came: an `unload` whose event the
//! relay carried, one whose event it ended before carrying (measured on
//! 2026-11-13: the row kept the wind-down turn's `idle`), a run ended from
//! the box's own shell, or a crash, which writes no `unload` at all. Only
//! `show` and a live event write the state, so the close's `show` is what
//! makes the row true in each.
//!
//! Every perturbation below names its guard, and the act that landed it
//! showed the test failing with the guard removed, per Spec section 9.

use super::admin_con_tests::{
    Running, Service, Step, Trace, config, installed, ns, record, verb, window,
};
use super::tests::{Lab, SOON};
use crate::store::AgentId;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// The re-ask's gap in these tests, where the service's 5 s would hold
/// each test for seconds.
const GAP: Duration = Duration::from_millis(300);

/// One agent's run: the lab, its trace and relay, admin-con running with a
/// fake invoker, and the admission's `show` landed where it is granted.
struct Run {
    lab: Lab,
    trace: Trace,
    id: AgentId,
    invoker: Arc<Service>,
    con: Running,
    _out: tempfile::TempDir,
}

async fn running(grants: &[&str]) -> Option<Run> {
    let lab = Lab::open().await?;
    let trace = Trace::new();
    let out = crate::test_dir::tempdir().unwrap();
    let (id, path) = installed(&lab, &trace.socket, out.path(), None).await;
    let invoker = Service::new(grants, "idle");
    let mut cfg = config(&path);
    // A held relay sends no heartbeat, so a verb's drain would wait its
    // whole bound before the invocation.
    cfg.drain_bound = Duration::from_millis(300);
    let mut con = Running::start(cfg, invoker.clone());
    con.wait("admitted", |s| s.admitted).await;
    if grants.contains(&"show") {
        lab.wait_for(&id, "the admission's show landed", |a| {
            a.state_source.as_deref() == Some("show")
        })
        .await;
    }
    Some(Run {
        lab,
        trace,
        id,
        invoker,
        con,
        _out: out,
    })
}

/// A `show` the server asked of its own: its first record's date, and the
/// outcome naming it and that record's date, the answer's, if one has
/// landed.
#[derive(Debug, sqlx::FromRow)]
struct Shown {
    at: DateTime<Utc>,
    outcome: Option<String>,
    answered: Option<DateTime<Utc>>,
}

/// The server's own `show` asks the audit holds on `id`, oldest first.
async fn server_shows(lab: &Lab, id: &AgentId) -> Vec<Shown> {
    sqlx::query_as::<_, Shown>(
        "SELECT f.at, o.outcome, o.at AS answered FROM audit f LEFT JOIN audit o \
         ON o.answers = f.audit_id WHERE f.target_id = $1 AND f.action = 'show' \
         AND f.principal = 'server' AND f.answers IS NULL AND f.refusal IS NULL \
         ORDER BY f.at, f.audit_id",
    )
    .bind(id.as_str())
    .fetch_all(&lab.store.pool)
    .await
    .unwrap()
}

/// Wait until the audit holds `n` of the server's `show` asks, each
/// answered.
async fn wait_shows(lab: &Lab, id: &AgentId, n: usize) -> Vec<Shown> {
    let until = tokio::time::Instant::now() + SOON;
    loop {
        let shows = server_shows(lab, id).await;
        if shows.len() == n && shows.iter().all(|s| s.outcome.is_some()) {
            return shows;
        }
        assert!(
            shows.len() <= n,
            "more than {n} of the server's shows: {shows:?}"
        );
        assert!(
            tokio::time::Instant::now() < until,
            "never {n} answered shows of the server's: {shows:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// **A run ended with its `unload` relayed**: the event writes `unloaded`
/// as it always did, and the close that follows asks `show`, audited as
/// the server's with its outcome, whose answer now holds the row.
///
/// Perturbation: drop the close's `show`, and the row stays on the event
/// and the audit holds the admission's `show` alone.
#[tokio::test]
async fn a_run_ended_with_its_unload_relayed_is_read_by_show_at_the_close() {
    let Some(mut r) = running(&["show", "unload"]).await else {
        return;
    };
    r.trace.append(1, "load");
    r.lab
        .wait_for(&r.id, "idle by the trace", |a| {
            a.state_source.as_deref() == Some("event")
        })
        .await;
    r.invoker.script(Step {
        then_append: Some((r.trace.path.clone(), record(2, "unload"))),
        then_state: Some("unloaded".into()),
        ..Default::default()
    });
    verb(&r.lab.listener, &r.id, "unload").await.unwrap();
    r.lab
        .wait_for(&r.id, "unloaded by the trace", |a| {
            a.load_state.as_deref() == Some("unloaded")
                && a.state_source.as_deref() == Some("event")
        })
        .await;
    r.trace.relay.stop().await;
    let row = r
        .lab
        .wait_for(&r.id, "read by the close's show", |a| {
            a.trace_door == Some(false) && a.state_source.as_deref() == Some("show")
        })
        .await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    let shows = wait_shows(&r.lab, &r.id, 2).await;
    assert_eq!(
        shows[1].outcome.as_deref(),
        Some("ok"),
        "the close's show is the server's, its outcome named: {shows:?}"
    );
    assert_eq!(r.invoker.ran(), ["show", "unload", "show"]);
    r.con.stop().await;
}

/// **A run ended without its `unload` relayed** (measured on 2026-11-13):
/// the relay ends before carrying the event, so the row's last word is the
/// wind-down turn's `idle` by event; the close's `show` reads `unloaded`.
///
/// Perturbation: drop the close's `show`, and the row reads `idle` by the
/// turn's close while the agent is unloaded.
#[tokio::test]
async fn a_run_ended_without_its_unload_relayed_is_read_by_show_at_the_close() {
    let Some(mut r) = running(&["show", "unload"]).await else {
        return;
    };
    r.trace.append(1, "load");
    r.trace.append(2, "turn.closed");
    let (lab, id) = (&r.lab, &r.id);
    let until = tokio::time::Instant::now() + SOON;
    while ns(&window(lab, id)) != [1, 2] {
        assert!(tokio::time::Instant::now() < until, "never both events");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    lab.wait_for(id, "idle by the turn's close", |a| {
        a.load_state.as_deref() == Some("idle") && a.state_source.as_deref() == Some("event")
    })
    .await;
    // The relay carries nothing from here, and ends with the run.
    r.trace.relay.hold(true);
    r.invoker.script(Step {
        then_append: Some((r.trace.path.clone(), record(3, "unload"))),
        then_state: Some("unloaded".into()),
        ..Default::default()
    });
    verb(&r.lab.listener, &r.id, "unload").await.unwrap();
    r.trace.relay.stop().await;
    let row = r
        .lab
        .wait_for(&r.id, "read by the close's show", |a| {
            a.trace_door == Some(false) && a.state_source.as_deref() == Some("show")
        })
        .await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    assert!(
        !ns(&window(&r.lab, &r.id)).contains(&3),
        "the unload's event never reached the server"
    );
    wait_shows(&r.lab, &r.id, 2).await;
    r.con.stop().await;
}

/// **A crashed run writes no `unload` at all**: the row's last word is a
/// turn's start, `active` by event, and the relay ends with the run; the
/// close's `show` reads what admin says now.
///
/// Perturbation: drop the close's `show`, and the row reads `active` for
/// good.
#[tokio::test]
async fn a_crashed_run_is_read_by_show_at_the_close() {
    let Some(mut r) = running(&["show"]).await else {
        return;
    };
    r.trace.append(1, "load");
    r.trace.append(2, "turn.started");
    r.lab
        .wait_for(&r.id, "active by the trace", |a| {
            a.load_state.as_deref() == Some("active")
        })
        .await;
    r.invoker.set_state("unloaded");
    r.trace.relay.stop().await;
    let row = r
        .lab
        .wait_for(&r.id, "read by the close's show", |a| {
            a.trace_door == Some(false) && a.state_source.as_deref() == Some("show")
        })
        .await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"), "{row:?}");
    wait_shows(&r.lab, &r.id, 2).await;
    r.con.stop().await;
}

/// **A relay's hiccup**: the door closes and opens again on the same run.
/// The close asks its `show` and the reopening its own, each audited, and
/// nothing asks after them.
///
/// Perturbation: ask the close's `show` at an opening too, and a third is
/// asked for the one reopening.
#[tokio::test]
async fn a_relay_hiccup_asks_show_at_its_close_and_again_at_the_reopening() {
    let Some(mut r) = running(&["show"]).await else {
        return;
    };
    r.lab.listener.set_close_reask(GAP);
    r.trace.relay.stop().await;
    r.lab
        .wait_for(&r.id, "the door closed", |a| a.trace_door == Some(false))
        .await;
    wait_shows(&r.lab, &r.id, 2).await;
    r.trace.relay.start();
    r.lab
        .wait_for(&r.id, "the door open again", |a| a.trace_door == Some(true))
        .await;
    wait_shows(&r.lab, &r.id, 3).await;
    tokio::time::sleep(GAP * 4).await;
    assert_eq!(
        server_shows(&r.lab, &r.id).await.len(),
        3,
        "the admission's, the close's and the reopening's"
    );
    let row = r.lab.agent(&r.id).await;
    assert_eq!(row.load_state.as_deref(), Some("idle"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("show"));
    r.con.stop().await;
}

/// The door closes on a run whose `show` answers `in_transition` `n`
/// times before its state, the state `unloaded`; the run as it then stands.
async fn closed_in_transition(n: usize) -> Option<Run> {
    closed_in_transition_answering_after(n, Duration::ZERO).await
}

/// As [`closed_in_transition`], each `show` from the close on answered
/// `delay` after it is asked.
async fn closed_in_transition_answering_after(n: usize, delay: Duration) -> Option<Run> {
    let mut r = running(&["show"]).await?;
    r.lab.listener.set_close_reask(GAP);
    if !delay.is_zero() {
        r.invoker.slow("show", delay);
    }
    r.trace.append(1, "load");
    r.lab
        .wait_for(&r.id, "idle by the trace", |a| {
            a.state_source.as_deref() == Some("event")
        })
        .await;
    r.invoker.set_state("unloaded");
    r.invoker.in_transition_for(n);
    r.trace.relay.stop().await;
    Some(r)
}

/// **`in_transition` once**: the close's `show` answers it, the server asks
/// again after the gap, and the second answers the state, which ends the
/// transition; nothing is asked after.
///
/// Perturbation: drop the re-ask, and the row keeps `idle` by event with a
/// transition beside it.
#[tokio::test]
async fn a_close_show_in_transition_once_is_asked_again_and_reads_the_state() {
    let Some(r) = closed_in_transition(1).await else {
        return;
    };
    let row = r
        .lab
        .wait_for(&r.id, "read by the re-ask", |a| {
            a.state_source.as_deref() == Some("show") && a.load_state.as_deref() == Some("unloaded")
        })
        .await;
    assert_eq!(row.in_transition_at, None, "the state ends the transition");
    wait_shows(&r.lab, &r.id, 3).await;
    tokio::time::sleep(GAP * 4).await;
    assert_eq!(
        server_shows(&r.lab, &r.id).await.len(),
        3,
        "nothing asked once a state answered"
    );
    r.con.stop().await;
}

/// **`in_transition` twice**: two re-asks, the second answering the state.
///
/// Perturbation: bound the re-asks at one, and the row keeps the
/// transition.
#[tokio::test]
async fn a_close_show_in_transition_twice_is_asked_twice_more() {
    let Some(r) = closed_in_transition(2).await else {
        return;
    };
    r.lab
        .wait_for(&r.id, "read by the second re-ask", |a| {
            a.state_source.as_deref() == Some("show") && a.load_state.as_deref() == Some("unloaded")
        })
        .await;
    wait_shows(&r.lab, &r.id, 4).await;
    tokio::time::sleep(GAP * 4).await;
    assert_eq!(server_shows(&r.lab, &r.id).await.len(), 4);
    r.con.stop().await;
}

/// **`in_transition` past the bound**: the close's `show` and its three
/// re-asks all answer it, each re-ask asked no sooner than the gap after
/// the answer before it; then nothing is asked, and the row keeps its last
/// state with the transition beside it, dated. Each answer is held a gap
/// after its ask, so a gap measured from the ask would re-ask at the
/// answer: the gap is read from the answer's outcome record.
///
/// Perturbations: (1) re-ask without a bound, and a fifth `show` is asked;
/// (2) re-ask at once, and a re-ask stands closer than the gap to the
/// answer before it; (3) measure the gap from the ask, and the same.
#[tokio::test]
async fn a_close_show_in_transition_past_the_bound_leaves_the_transition() {
    let Some(r) = closed_in_transition_answering_after(10, GAP).await else {
        return;
    };
    let shows = wait_shows(&r.lab, &r.id, 5).await;
    for pair in shows[1..].windows(2) {
        let answered = pair[0].answered.expect("each show answered");
        let apart = (pair[1].at - answered).to_std().unwrap_or_default();
        assert!(
            apart >= GAP,
            "a re-ask stands {apart:?} after the answer before it: {shows:?}"
        );
    }
    tokio::time::sleep(GAP * 5).await;
    assert_eq!(
        server_shows(&r.lab, &r.id).await.len(),
        5,
        "the admission's, the close's and three re-asks, then nothing"
    );
    let row = r.lab.agent(&r.id).await;
    assert_eq!(row.load_state.as_deref(), Some("idle"), "{row:?}");
    assert_eq!(row.state_source.as_deref(), Some("event"));
    assert!(
        row.in_transition_at.is_some(),
        "the transition stands, dated"
    );
    assert_eq!(row.constituents, Some(vec![4100]));
    r.con.stop().await;
}

/// **An opening ends the re-asks**: the close's `show` answers
/// `in_transition`, the door opens again before the gap has passed, and
/// the opening's own `show` is the last asked.
///
/// Perturbation: keep the re-ask past an opening, and a fourth `show` is
/// asked after the opening's.
#[tokio::test]
async fn an_opening_ends_the_close_shows_re_asks() {
    let Some(mut r) = running(&["show"]).await else {
        return;
    };
    let gap = Duration::from_secs(3);
    r.lab.listener.set_close_reask(gap);
    r.invoker.in_transition_for(1);
    r.trace.relay.stop().await;
    wait_shows(&r.lab, &r.id, 2).await;
    r.trace.relay.start();
    r.lab
        .wait_for(&r.id, "the door open again", |a| a.trace_door == Some(true))
        .await;
    wait_shows(&r.lab, &r.id, 3).await;
    tokio::time::sleep(gap + Duration::from_secs(1)).await;
    assert_eq!(
        server_shows(&r.lab, &r.id).await.len(),
        3,
        "the admission's, the close's and the opening's"
    );
    r.con.stop().await;
}

/// **A ceiling without `show` asks nothing at the close**: the door closes
/// and the row records it, no `show` is asked or audited, and the
/// connection stands.
///
/// Perturbation: ask at the close whatever the ceiling, and a `show`
/// outside it is asked and audited.
#[tokio::test]
async fn a_ceiling_without_show_asks_nothing_at_the_close() {
    let Some(mut r) = running(&["unload"]).await else {
        return;
    };
    r.trace.relay.stop().await;
    r.lab
        .wait_for(&r.id, "the door closed", |a| a.trace_door == Some(false))
        .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(server_shows(&r.lab, &r.id).await.is_empty());
    assert!(r.invoker.ran().is_empty(), "{:?}", r.invoker.ran());
    assert!(r.con.status().admitted);
    r.con.stop().await;
}

/// **A `show` met while an opening is measured never holds the verbs**
/// (Spec 7.2; Codex on #113): admin-con queues the server's asks while it
/// measures and verifies an opening, and serves the queued `show` first
/// under the opening's hold. A close's re-ask that arrives then is one; the
/// hold ends at the landing of any `show` it served, so the opening's own
/// landing ends it, a person's verb is answered, and the door opens again
/// after a later close.
///
/// Perturbation: keep the hold for the first `show` it served alone, and
/// the verb is never answered.
#[tokio::test]
async fn a_show_met_while_an_opening_is_measured_never_holds_the_verbs() {
    let Some(mut r) = running(&["show", "load"]).await else {
        return;
    };
    let gap = Duration::from_millis(600);
    r.lab.listener.set_close_reask(gap);
    r.trace.append(1, "load");
    r.lab
        .wait_for(&r.id, "idle by the trace", |a| {
            a.state_source.as_deref() == Some("event")
        })
        .await;
    r.invoker.set_state("unloaded");
    r.invoker.in_transition_for(1);
    // The next run holds each header three gaps, so the close's re-ask
    // reaches admin-con while it measures the opening.
    let held = (gap * 3).as_millis() as u64;
    r.trace
        .relay
        .counts
        .header_delay_ms
        .store(held, Ordering::SeqCst);
    r.trace.relay.stop().await;
    r.lab
        .wait_for(&r.id, "the door closed", |a| a.trace_door == Some(false))
        .await;
    // The admission's, and the close's, answered in transition.
    wait_shows(&r.lab, &r.id, 2).await;
    let sized = r.trace.relay.counts.sized.load(Ordering::SeqCst);
    r.trace.relay.start();
    let until = tokio::time::Instant::now() + SOON;
    while r.trace.relay.counts.sized.load(Ordering::SeqCst) == sized {
        assert!(
            tokio::time::Instant::now() < until,
            "admin-con never asked the relay"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let measured_from = Utc::now();
    r.lab
        .wait_for(&r.id, "the door open again", |a| a.trace_door == Some(true))
        .await;
    // The re-ask, then the opening's own.
    let shows = wait_shows(&r.lab, &r.id, 4).await;
    assert!(
        shows[2].at > measured_from,
        "the re-ask reached admin-con while it measured the opening: {shows:?}"
    );
    verb(&r.lab.listener, &r.id, "load").await.unwrap();
    assert_eq!(r.invoker.ran().last().map(String::as_str), Some("load"));
    // A later close and opening: the door is not held shut.
    r.trace
        .relay
        .counts
        .header_delay_ms
        .store(0, Ordering::SeqCst);
    r.trace.relay.stop().await;
    r.lab
        .wait_for(&r.id, "the door closed again", |a| {
            a.trace_door == Some(false)
        })
        .await;
    r.trace.relay.start();
    r.lab
        .wait_for(&r.id, "the door open once more", |a| {
            a.trace_door == Some(true)
        })
        .await;
    r.con.stop().await;
}
