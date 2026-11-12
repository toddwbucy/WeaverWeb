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
                replayed: false,
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

/// A live trace event of `kind` at `seq`.
fn live(seq: u64, kind: &str) -> FromClient {
    FromClient::Event {
        position: Position {
            generation: "g-1".into(),
            offset: seq * 100,
            digest: format!("d{seq}"),
        },
        replayed: false,
        event: TraceEvent {
            seq,
            mark: None,
            run: Some("run-1".into()),
            turn: Some("t-1".into()),
            kind: Some(kind.into()),
            raw: json!({"kind": kind, "run": "run-1", "wall_ms": seq, "payload": {}}),
            replayed: false,
        },
    }
}

/// Wait for karl's row to meet `cond`.
async fn wait_row(o: &Operated, what: &str, cond: impl Fn(&Agent) -> bool) -> Agent {
    let id: AgentId = o.karl.parse().unwrap();
    let until = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let row = o.rig.store().agent(&id).await.unwrap().unwrap();
        if cond(&row) {
            return row;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "never {what}: {row:#?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// **A turn's close moves the state and keeps the transition**: a graceful
/// unload waiting for a turn (S3) answers `show` in transition, the turn's
/// close writes `idle` and the transition still stands, and the unload's
/// event then ends it.
///
/// Perturbation: let a turn's close end the transition, and the page reads
/// `idle` with no transition while the unload still runs.
#[tokio::test]
async fn a_turns_close_keeps_the_transition_and_the_unload_ends_it() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let row = shown(&mut o, json!({"kind": "in_transition"}), 0).await;
    let since = row.in_transition_at.expect("a transition stands");
    o.admin_con.send(live(1, "turn.started")).await;
    wait_row(&o, "active", |r| r.load_state.as_deref() == Some("active")).await;
    o.admin_con.send(live(2, "turn.closed")).await;
    let row = wait_row(&o, "idle", |r| {
        r.load_state.as_deref() == Some("idle") && r.state_source.as_deref() == Some("event")
    })
    .await;
    assert_eq!(
        row.in_transition_at,
        Some(since),
        "the turn keeps the transition"
    );
    assert!(page(&o).await.contains("another command is in progress"));
    o.admin_con.send(live(3, "unload")).await;
    let row = wait_row(&o, "unloaded", |r| {
        r.load_state.as_deref() == Some("unloaded")
    })
    .await;
    assert_eq!(row.in_transition_at, None, "the unload ends it");
}

/// The warning the agent page puts beside `verb`'s form, `None` where the
/// form stands bare; panics where the verb is not offered.
fn warning_beside(page: &str, karl: &str, verb: &str) -> Option<String> {
    let form = format!(
        "<form method=\"post\" action=\"/agents/{karl}/verb\"><input type=\"hidden\" name=\"verb\" value=\"{verb}\"><button type=\"submit\" class=\"verb\">{verb}</button></form>"
    );
    let at = page
        .find(&form)
        .unwrap_or_else(|| panic!("{verb} is not offered: {page}"));
    let rest = &page[at + form.len()..];
    let open = "<span class=\"status hold warning\">";
    rest.strip_prefix(open)
        .map(|w| w[..w.find("</span>").unwrap()].to_owned())
}

/// **A verb admin's state table refuses in the row's last reading is warned
/// of, and offered still** (Spec 2.13; the admin-operator contract's
/// section 4.1): read idle, `load` is warned `AgentRunning` and `save-point`
/// and `unload` are not; read active, `load` is warned `AgentRunning` and
/// `save-point` `ActivityNotAtRest`; read in transition, both are warned
/// with the lock held and free told apart, dated by the transition; read
/// unloaded, `save-point` alone is warned `OutOfOrder`. Each warning names
/// the reading and its date, every form stays, the pane names the table's
/// source at its pin while a warning shows, and `unload` is never warned.
///
/// Perturbation: drop the warning from the template, change a cell, or
/// ignore a standing transition, and the case fails.
#[tokio::test]
async fn the_agent_page_warns_of_a_verb_the_state_table_refuses() {
    let Some(mut o) = operated(&["show", "load", "save-point", "unload"]).await else {
        return;
    };
    let id: AgentId = o.karl.parse().unwrap();
    let date = |at: chrono::DateTime<chrono::Utc>| at.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let source = format!(
        "admin's state table (<code>weaver-admin-operator-contract</code> section 4.1 at WeaverAgent <span class=\"mono\">{}</span>)",
        crate::surfaces::admin_table::STATE_TABLE_READ_AT
    );
    let karl = o.karl.clone();
    let reads = |page: &str, verb: &str| warning_beside(page, &karl, verb);

    // Idle, as the admission's `show` answered.
    let row = o.rig.store().agent(&id).await.unwrap().unwrap();
    assert_eq!(row.load_state.as_deref(), Some("idle"));
    let at = date(row.load_state_at.unwrap());
    let p = page(&o).await;
    assert_eq!(
        reads(&p, "load").as_deref(),
        Some(
            format!("admin refuses this while the agent reads idle: AgentRunning. Read idle {at}.")
                .as_str()
        ),
        "{p}"
    );
    assert_eq!(reads(&p, "save-point"), None, "S2 and S9 take a save point");
    assert_eq!(reads(&p, "unload"), None);
    assert!(p.contains(&source), "{p}");

    // Active.
    let row = shown(&mut o, json!({"kind": "state", "state": "active"}), 0).await;
    let at = date(row.load_state_at.unwrap());
    let p = page(&o).await;
    assert_eq!(
        reads(&p, "load").as_deref(),
        Some(
            format!(
                "admin refuses this while the agent reads active: AgentRunning. Read active {at}."
            )
            .as_str()
        ),
    );
    assert_eq!(
        reads(&p, "save-point").as_deref(),
        Some(format!("admin refuses this while the agent reads active: ActivityNotAtRest. Read active {at}.").as_str()),
    );
    assert_eq!(reads(&p, "unload"), None);

    // In transition: the transition is the reading while it stands, dated
    // by itself.
    let row = shown(&mut o, json!({"kind": "in_transition"}), 0).await;
    assert_eq!(
        row.load_state.as_deref(),
        Some("active"),
        "no state claimed"
    );
    let at = date(row.in_transition_at.unwrap());
    let p = page(&o).await;
    assert_eq!(
        reads(&p, "load").as_deref(),
        Some(format!("admin refuses this while the agent reads in transition: InvocationInFlight where a command holds the agent, AgentRunning where none does. Read in transition {at}.").as_str()),
    );
    assert_eq!(
        reads(&p, "save-point").as_deref(),
        Some(format!("admin refuses this while the agent reads in transition: InvocationInFlight where a command holds the agent, OutOfOrder where none does. Read in transition {at}.").as_str()),
    );
    assert_eq!(reads(&p, "unload"), None);

    // Unloaded.
    let row = shown(&mut o, json!({"kind": "state", "state": "unloaded"}), 0).await;
    assert_eq!(row.in_transition_at, None);
    let at = date(row.load_state_at.unwrap());
    let p = page(&o).await;
    assert_eq!(reads(&p, "load"), None, "S0 answers with the load");
    assert_eq!(
        reads(&p, "save-point").as_deref(),
        Some(
            format!(
                "admin refuses this while the agent reads unloaded: OutOfOrder. Read unloaded {at}."
            )
            .as_str()
        ),
    );
    assert_eq!(reads(&p, "unload"), None);
    assert!(p.contains(&source), "{p}");
}
