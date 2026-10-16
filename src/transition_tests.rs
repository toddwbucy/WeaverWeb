//! A `show` in transition (Spec 2.12, 7.2; `toddwbucy/WeaverWeb#38`, #46
//! item 2): admin's `in_transition` claims no state, so the row keeps its
//! last state and records the transition beside it, and the next
//! observation that writes a state ends it. Against the real listener and a
//! fake admin-con, each test on a store of its own. No name is a real one.

use serde_json::json;

use crate::admin_tests::send_as;
use crate::agents_tests::Connector;
use crate::link::frames::{FromClient, Position, ToClient};
use crate::link::register::Agent;
use crate::store::AgentId;
use crate::traceview::TraceEvent;
use crate::verbs_tests::{Operated, answer, operated, take_verb};

/// The server asks `show` of karl, and admin-con answers it with `object`
/// and `exit`; the row as it stands after.
async fn shown(o: &mut Operated, object: serde_json::Value, exit: i32) -> Agent {
    let id: AgentId = o.karl.parse().unwrap();
    let listener = o.rig.listener.clone();
    let asked = {
        let id = id.clone();
        tokio::spawn(async move { listener.verb(&id, "show").await })
    };
    let (ask, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "show");
    o.admin_con
        .send(FromClient::Verb {
            id: ask,
            outcome: Some(answer("show", object, exit)),
            error: None,
        })
        .await;
    let _ = asked.await.unwrap();
    o.rig.store().agent(&id).await.unwrap().unwrap()
}

/// The agent page as bea reads it.
async fn page(o: &Operated) -> String {
    send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}", o.karl),
        None,
        &o.bea_session,
    )
    .await
    .2
}

/// **`in_transition` claims no state, and the next state ends it**: from
/// `Idle`, a `show` answering `in_transition` keeps the state, its source
/// and its tuple and records a transition with no constituents, which the
/// page says is another command in progress; one naming constituents
/// records them, the page naming them and picking neither reading; a `show`
/// answering `Unloaded` ends it, its constituents none.
///
/// Perturbation: write the load state from an `in_transition` answer, and
/// the row reads a state admin never claimed.
#[tokio::test]
async fn in_transition_claims_no_state_and_the_next_state_ends_it() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let before = o
        .rig
        .store()
        .agent(&o.karl.parse().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.load_state.as_deref(), Some("idle"));

    let row = shown(&mut o, json!({"kind": "in_transition"}), 0).await;
    assert_eq!(row.load_state.as_deref(), Some("idle"), "no state claimed");
    assert_eq!(row.state_source.as_deref(), Some("show"));
    assert_eq!(
        row.load_state_at, before.load_state_at,
        "the state's date kept"
    );
    assert_eq!(row.tuple, before.tuple, "the tuple kept");
    assert!(row.in_transition_at.is_some(), "a transition stands");
    assert_eq!(row.constituents, None);
    let shown_page = page(&o).await;
    assert!(
        shown_page.contains(": another command is in progress"),
        "{shown_page}"
    );
    assert!(
        !shown_page.to_lowercase().contains("loading"),
        "{shown_page}"
    );

    let row = shown(
        &mut o,
        json!({"kind": "in_transition", "constituents": [4100, 4101]}),
        0,
    )
    .await;
    assert_eq!(row.load_state.as_deref(), Some("idle"));
    assert_eq!(row.constituents, Some(vec![4100, 4101]));
    let shown_page = page(&o).await;
    assert!(
        shown_page.contains("processes 4100 4101 still stand: a run starting or ending, or an ended run&#39;s processes still being ended"),
        "{shown_page}"
    );
    assert!(
        !shown_page.to_lowercase().contains("loading"),
        "{shown_page}"
    );

    let row = shown(&mut o, json!({"kind": "state", "state": "unloaded"}), 0).await;
    assert_eq!(row.load_state.as_deref(), Some("unloaded"));
    assert_eq!(row.in_transition_at, None, "the state ends the transition");
    assert_eq!(row.constituents, None);
    let shown_page = page(&o).await;
    assert!(!shown_page.contains("In transition"), "{shown_page}");
}

/// **A live unload event ends a transition**: it writes a state.
///
/// Perturbation: let a state leave the transition standing, and the page
/// shows a transition over a newer state.
#[tokio::test]
async fn a_live_unload_ends_a_transition() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let row = shown(&mut o, json!({"kind": "in_transition"}), 0).await;
    assert!(row.in_transition_at.is_some());
    o.admin_con
        .send(FromClient::Event {
            position: Position {
                generation: "g-1".into(),
                offset: 100,
                digest: "d".into(),
            },
            replayed: false,
            event: TraceEvent {
                seq: 1,
                mark: None,
                run: Some("run-1".into()),
                turn: None,
                kind: Some("unload".into()),
                raw: json!({"kind": "unload", "run": "run-1", "wall_ms": 1, "payload": {"forced": false}}),
            },
        })
        .await;
    let id: AgentId = o.karl.parse().unwrap();
    let until = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row = o.rig.store().agent(&id).await.unwrap().unwrap();
        if row.load_state.as_deref() == Some("unloaded") {
            assert_eq!(row.in_transition_at, None, "the unload ends the transition");
            break;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the unload never landed"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let shown_page = page(&o).await;
    assert!(!shown_page.contains("In transition"), "{shown_page}");
}

/// **A refusal is never admin's word on the state**: a `show` refused (exit
/// 1) whose object says `in_transition` lands nothing.
#[tokio::test]
async fn a_refusal_saying_in_transition_lands_nothing() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let row = shown(&mut o, json!({"kind": "in_transition"}), 1).await;
    assert_eq!(row.in_transition_at, None);
    assert_eq!(row.load_state.as_deref(), Some("idle"));
}

/// **An admission whose `show` answers `in_transition` completes**, the row
/// recording the transition, where before it closed `admission_incomplete`
/// and readmitted for as long as the transition stood.
///
/// Perturbation: drop the constituents from the transition, and the
/// leftovers' pids vanish from the row and the page.
#[tokio::test]
async fn an_admission_meeting_a_transition_completes() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let Operated {
        rig,
        karl,
        admin_config,
        admin_con,
        bea_session,
        ..
    } = o;
    drop(admin_con);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut c = Connector::dial_with(&admin_config, &["show"]).await;
    assert!(matches!(c.recv().await, Some(ToClient::HelloAnswer { .. })));
    let ask = match c.recv().await {
        Some(ToClient::Verb { id, verb, .. }) if verb == "show" => id,
        other => panic!("expected the admission's show, got {other:?}"),
    };
    c.send(FromClient::Verb {
        id: ask,
        outcome: Some(answer(
            "show",
            json!({"kind": "in_transition", "constituents": [4100]}),
            0,
        )),
        error: None,
    })
    .await;
    assert!(
        matches!(c.recv().await, Some(ToClient::Landed { .. })),
        "the admission's show landed"
    );
    let id: AgentId = karl.parse().unwrap();
    let row = rig.store().agent(&id).await.unwrap().unwrap();
    assert!(row.in_transition_at.is_some());
    assert_eq!(row.constituents, Some(vec![4100]));
    let shown_page = send_as(
        &rig.app,
        "GET",
        &format!("/agents/{karl}"),
        None,
        &bea_session,
    )
    .await
    .2;
    assert!(
        shown_page.contains("processes 4100 still stand"),
        "{shown_page}"
    );
    let (_, _, list) = send_as(&rig.app, "GET", "/agents", None, &bea_session).await;
    assert!(list.contains("processes 4100 still stand"), "{list}");
}
