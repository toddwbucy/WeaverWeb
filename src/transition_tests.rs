//! A `show` in transition (Spec 2.12, 7.2; `toddwbucy/WeaverWeb#38`, #46
//! item 2): admin's `in_transition` claims no state, so the row keeps its
//! last state and records the transition beside it, and the next
//! observation that writes a state ends it. Against the real listener and a
//! fake admin-con, each test on a store of its own. No name is a real one.

use serde_json::json;

use crate::admin_tests::send_as;
use crate::agents_tests::Connector;
use crate::link::frames::{FromClient, Position, Refusal, ToClient};
use crate::link::register::Agent;
use crate::store::AgentId;
use crate::traceview::TraceEvent;
use crate::verbs_tests::{
    Operated, answer, no_frame, operated, operated_with, readmitted_older_admin, take_verb,
};

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
    let mut c = Connector::redial(&admin_config, &["show"], false).await;
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
        "<form method=\"post\" action=\"/agents/{karl}/verb\" id=\"verb-{verb}\" hx-preserve hx-post=\"/agents/{karl}/verb\" hx-target=\"#answer-{verb}\" hx-swap=\"innerHTML\" hx-sync=\"this:drop\" hx-disabled-elt=\"find button\" hx-indicator=\"#asked-{verb}\"><input type=\"hidden\" name=\"verb\" value=\"{verb}\"><button type=\"submit\" class=\"verb\">{verb}</button></form>"
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

/// **Run's two panes re-read by themselves** (Spec 2.13): the agent page
/// holds the load state and the verbs as one fragment, `GET
/// /agents/{ag}/run`, which the vendored htmx re-fetches every 15 s. The
/// fragment is the page's own panes, kept from every cache; a state written
/// between two fetches shows at the second, its warnings with it; a person
/// holding no grant is answered as for an unknown agent, a malformed
/// identity 400, and an ended session 401. That the fetch leaves the
/// session's last use as it was is `a_poll_leaves_the_sessions_last_use_as_it_was`.
///
/// Perturbation: drop the trigger from the page, drop the grant check, or
/// render the fragment from another view, and the case fails.
#[tokio::test]
async fn the_run_panes_re_read_by_themselves() {
    let Some(mut o) = operated(&["show", "load", "save-point", "unload"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (_, dan) = crate::admin_tests::signed_in(
        &o.rig.app,
        &s,
        &mut crate::passkeys_tests::authenticator(),
        "dan",
    )
    .await;
    let uri = format!("/agents/{}/run", o.karl);
    let karl = o.karl.clone();
    let (app, bea) = (o.rig.app.clone(), o.bea_session.clone());

    let p = page(&o).await;
    assert!(
        p.contains(&format!(
            "<div class=\"stack\" id=\"run\" hx-get=\"{uri}\" hx-trigger=\"every 15s\" hx-swap=\"innerHTML settle:0ms\" hx-sync=\"this:abort\">"
        )),
        "{p}"
    );
    assert!(p.contains("/assets/surfaces/htmx.min.js"), "{p}");

    let (status, headers, first) = send_as(&app, "GET", &uri, None, &bea).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{first}");
    assert_eq!(
        headers
            .get(axum::http::header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap()),
        Some("no-store")
    );
    assert!(
        p.contains(first.trim()),
        "the fragment is the page's own panes: {first}"
    );
    assert!(first.contains("<h2>Load state</h2>"), "{first}");
    assert!(first.contains("<td>idle, by show (since "), "{first}");
    assert!(warning_beside(&first, &karl, "load").is_some(), "{first}");
    assert_eq!(warning_beside(&first, &karl, "save-point"), None);

    // A state written between two fetches shows at the second.
    shown(&mut o, json!({"kind": "state", "state": "unloaded"}), 0).await;
    let (status, _, second) = send_as(&app, "GET", &uri, None, &bea).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{second}");
    assert!(second.contains("<td>unloaded, by show (since "), "{second}");
    assert_eq!(warning_beside(&second, &karl, "load"), None, "{second}");
    assert!(
        warning_beside(&second, &karl, "save-point")
            .is_some_and(|w| w.contains("reads unloaded: OutOfOrder")),
        "{second}"
    );

    let (status, _, theirs) = send_as(&app, "GET", &uri, None, &dan).await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{theirs}");
    let (status, _, _) = send_as(&app, "GET", "/agents/not-an-agent/run", None, &bea).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);

    let (status, _, _) = send_as(&app, "POST", "/sign-out", None, &bea).await;
    assert!(status.is_redirection() || status.is_success(), "{status}");
    let (status, _, ended) = send_as(&app, "GET", &uri, None, &bea).await;
    assert_eq!(status, axum::http::StatusCode::UNAUTHORIZED, "{ended}");
}

/// A live trace event of `kind` at `seq`, dated `wall_ms`.
fn live_at(seq: u64, kind: &str, wall_ms: i64) -> FromClient {
    let mut event = live(seq, kind);
    if let FromClient::Event { event, .. } = &mut event {
        event.raw["wall_ms"] = json!(wall_ms);
    }
    event
}

/// A date as the page writes one to the millisecond.
fn to_the_ms(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string()
}

/// **A state by event under a door that closed after it says so** (Spec
/// 2.12; the Planner's ruling of 2026-11-14): no event can move it once
/// the door is closed, so the load state, the warning beside `load` and the
/// constituents each say when they were read and when the door closed
/// after them, both to the millisecond, and that `show` reads them. The
/// close's own `show` is asked, and while it is unanswered the page says
/// this; its answer holds the state, and the line goes.
///
/// Perturbations: drop the line from the load state, the warning or the
/// constituents, or date it to the second, and the page says the state as
/// if it were current.
#[tokio::test]
async fn a_state_by_event_under_a_closed_door_says_so() {
    let Some(mut o) = operated(&["show", "load", "save-point"]).await else {
        return;
    };
    shown(
        &mut o,
        json!({"kind": "state", "state": "idle", "load": {"declaration": "sha-x"},
               "constituents": [4100, 4101]}),
        0,
    )
    .await;
    let state_ms = chrono::Utc::now().timestamp_millis();
    o.admin_con.send(live_at(1, "turn.closed", state_ms)).await;
    wait_row(&o, "idle by the turn's close", |r| {
        r.state_source.as_deref() == Some("event")
    })
    .await;
    let door_ms = state_ms + 560;
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: door_ms as u64,
            tail: None,
        })
        .await;
    let row = wait_row(&o, "the door closed", |r| r.trace_door == Some(false)).await;
    // The close's own `show`, asked and not yet answered.
    let (ask, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "show", "the close asks show");
    let state_at = to_the_ms(row.load_state_at.unwrap());
    let door_at = to_the_ms(row.trace_door_at.unwrap());
    let read_at = to_the_ms(row.constituents_at.unwrap());
    assert!(state_at.ends_with(&format!(".{:03} UTC", state_ms % 1000)));
    let shown_page = page(&o).await;
    let said = format!(
        "<tr><th>Load state</th><td>idle, by event at {state_at}. The trace door closed after it, at {door_at}: no event has moved it since, and show reads it.</td></tr>"
    );
    assert!(shown_page.contains(&said), "{said}: {shown_page}");
    let warning = warning_beside(&shown_page, &o.karl, "load").expect("load is warned");
    let after = format!(
        "Read idle at {state_at}, by event. The trace door closed after it, at {door_at}: no event has moved it since, and show reads it."
    );
    assert!(warning.contains(&after), "{after}: {warning}");
    let pids = format!(
        "<tr><th>Constituents</th><td class=\"mono\">4100 4101, by show at {read_at}. The trace door closed after it, at {door_at}: nothing has read them since, and show reads them.</td></tr>"
    );
    assert!(shown_page.contains(&pids), "{pids}: {shown_page}");

    // The close's show answers: the state is show's, read after the close.
    o.admin_con
        .send(FromClient::Verb {
            id: ask,
            outcome: Some(answer(
                "show",
                json!({"kind": "state", "state": "unloaded", "load": null}),
                0,
            )),
            error: None,
        })
        .await;
    wait_row(&o, "read by the close's show", |r| {
        r.state_source.as_deref() == Some("show") && r.load_state.as_deref() == Some("unloaded")
    })
    .await;
    let read_page = page(&o).await;
    assert!(
        !read_page.contains("The trace door closed after"),
        "the line goes once show reads: {read_page}"
    );
}

/// **Where the live ceiling does not grant `show`, the line does not say
/// that `show` reads it** (Spec 2.12): no `show` is asked at the close, so
/// the load state and the warning say that `show` is outside the ceiling.
///
/// Perturbation: say "show reads it" whatever the ceiling, and the page
/// claims a read nothing makes.
#[tokio::test]
async fn a_state_by_event_under_a_closed_door_without_show_says_nothing_reads_it() {
    let Some(o) = operated(&["show", "load", "save-point"]).await else {
        return;
    };
    // admin-con reconnects declaring a ceiling without `show`: its
    // admission asks nothing and completes at `caught_up`.
    let Operated {
        rig,
        karl,
        gate_config,
        admin_config,
        ada,
        ada_session,
        bea,
        bea_session,
        admin_con,
    } = o;
    drop(admin_con);
    let mut c = Connector::redial(&admin_config, &["load", "save-point"], false).await;
    c.send(FromClient::CaughtUp).await;
    let mut o = Operated {
        rig,
        karl,
        gate_config,
        admin_config,
        ada,
        ada_session,
        bea,
        bea_session,
        admin_con: c,
    };
    wait_row(&o, "the ceiling without show", |r| {
        r.admin.connected
            && r.ceiling.as_deref() == Some(&["load".to_owned(), "save-point".to_owned()][..])
    })
    .await;
    let state_ms = chrono::Utc::now().timestamp_millis();
    o.admin_con.send(live_at(1, "turn.closed", state_ms)).await;
    wait_row(&o, "idle by the turn's close", |r| {
        r.state_source.as_deref() == Some("event")
    })
    .await;
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: (state_ms + 40) as u64,
            tail: None,
        })
        .await;
    let row = wait_row(&o, "the door closed", |r| r.trace_door == Some(false)).await;
    let state_at = to_the_ms(row.load_state_at.unwrap());
    let door_at = to_the_ms(row.trace_door_at.unwrap());
    let shown_page = page(&o).await;
    let said = format!(
        "<tr><th>Load state</th><td>idle, by event at {state_at}. The trace door closed after it, at {door_at}: no event has moved it since, and show, which would read it, is outside the live ceiling.</td></tr>"
    );
    assert!(shown_page.contains(&said), "{said}: {shown_page}");
    let warning = warning_beside(&shown_page, &o.karl, "load").expect("load is warned");
    assert!(
        warning.contains("no event has moved it since, and show, which would read it, is outside the live ceiling."),
        "{warning}"
    );
    assert!(!shown_page.contains("show reads it"), "{shown_page}");
}

/// **A close's re-ask never stretches the silence bound** (Spec 7.2): the
/// re-ask resumes the connection's read without starting its silence
/// window again, so an admin-con that goes silent after answering
/// `in_transition` is closed at the bound from its last frame, not from
/// the re-ask.
///
/// Perturbation: start the window again at the re-ask, and the silent
/// connection stands a re-ask's gap past the bound.
#[tokio::test]
async fn a_close_re_ask_never_stretches_the_silence_bound() {
    // The least bound the cadence rule admits, and a gap that ends inside
    // it, so a window started again at the re-ask stands well past it.
    let silence = std::time::Duration::from_secs(4);
    let Some(mut o) = operated_with(&["show"], silence).await else {
        return;
    };
    o.rig
        .listener
        .set_close_reask(std::time::Duration::from_secs(3));
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: chrono::Utc::now().timestamp_millis() as u64,
            tail: None,
        })
        .await;
    let (ask, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "show", "the close asks show");
    o.admin_con
        .send(FromClient::Verb {
            id: ask,
            outcome: Some(answer(
                "show",
                json!({"kind": "in_transition", "constituents": [4100]}),
                0,
            )),
            error: None,
        })
        .await;
    let answered = std::time::Instant::now();
    let (_, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "show", "the re-ask");
    // admin-con says nothing more: the silence bound closes it.
    let refused = loop {
        match o.admin_con.recv().await {
            Some(ToClient::Refusal { reason }) => break reason,
            Some(_) => continue,
            None => panic!("closed without a refusal"),
        }
    };
    let took = answered.elapsed();
    assert_eq!(refused, Refusal::Silence);
    assert!(
        took < silence + std::time::Duration::from_millis(1500),
        "closed {took:?} after its last frame, past the {silence:?} bound"
    );
}

/// **A transition read after the close takes no line on the warning or the
/// constituents** (Spec 2.12): the close's `show` answers `in_transition`,
/// so the warning's reading is the transition, by `show`, dated as it
/// always is, and the constituents it names were read after the close. The
/// load state, still by event under the closed door, keeps its line.
///
/// Perturbations: let the warning take the line under a transition, and it
/// dates the transition by the event; let constituents read after the close
/// take it, and the page says nothing has read them since.
#[tokio::test]
async fn a_transition_read_after_the_close_takes_no_line_on_the_warning_or_the_constituents() {
    let Some(mut o) = operated(&["show", "load", "save-point"]).await else {
        return;
    };
    let state_ms = chrono::Utc::now().timestamp_millis();
    o.admin_con.send(live_at(1, "turn.closed", state_ms)).await;
    wait_row(&o, "idle by the turn's close", |r| {
        r.state_source.as_deref() == Some("event")
    })
    .await;
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: (state_ms + 40) as u64,
            tail: None,
        })
        .await;
    let (ask, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "show", "the close asks show");
    // The door is dated by the client's clock, the transition and its
    // constituents by the listener's at the answer: answer only once that
    // clock has passed the door, so they are read after the close.
    while chrono::Utc::now().timestamp_millis() <= state_ms + 40 {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    o.admin_con
        .send(FromClient::Verb {
            id: ask,
            outcome: Some(answer(
                "show",
                json!({"kind": "in_transition", "constituents": [4200]}),
                0,
            )),
            error: None,
        })
        .await;
    let row = wait_row(&o, "the close's transition", |r| {
        r.in_transition_at.is_some() && r.constituents.as_deref() == Some(&[4200][..])
    })
    .await;
    let shown_page = page(&o).await;
    let state_at = to_the_ms(row.load_state_at.unwrap());
    let door_at = to_the_ms(row.trace_door_at.unwrap());
    assert!(
        shown_page.contains(&format!(
            "<tr><th>Load state</th><td>idle, by event at {state_at}. The trace door closed after it, at {door_at}: no event has moved it since, and show reads it.</td></tr>"
        )),
        "{shown_page}"
    );
    let warning = warning_beside(&shown_page, &o.karl, "load").expect("load is warned");
    let transition_at = row
        .in_transition_at
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    assert!(
        warning.contains(&format!("Read in transition {transition_at}.")),
        "{warning}"
    );
    assert!(!warning.contains("by event"), "{warning}");
    let constituents_at = row
        .constituents_at
        .unwrap()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();
    assert!(
        shown_page.contains(&format!(
            "<tr><th>Constituents</th><td class=\"mono\">4200 (since {constituents_at})</td></tr>"
        )),
        "{shown_page}"
    );
}

/// karl's admin-con admitted again as one older than the first capability
/// (Spec 8): its hello states none, its ceiling `ceiling`.
async fn readmitted_older(o: Operated, ceiling: &[&str]) -> Operated {
    let Operated {
        rig,
        karl,
        gate_config,
        admin_config,
        ada,
        ada_session,
        bea,
        bea_session,
        admin_con,
    } = o;
    drop(admin_con);
    let admin_con = readmitted_older_admin(&admin_config, ceiling).await;
    let o = Operated {
        rig,
        karl,
        gate_config,
        admin_config,
        ada,
        ada_session,
        bea,
        bea_session,
        admin_con,
    };
    wait_row(&o, "the older admin-con admitted", |r| {
        r.admin.connected && r.state_source.as_deref() == Some("show")
    })
    .await;
    o
}

/// **An admin-con older than the first capability is asked no `show` at a
/// close** (Spec 7.2, 8; Codex on #113): its hello states no `close_show`,
/// so its opening hold may take a close's `show` and never end. The close
/// lands the door's state alone, as before the close's `show` existed.
///
/// Perturbation: ask the close's `show` whatever the hello says, and a
/// `show` reaches the older admin-con at the close.
#[tokio::test]
async fn an_admin_con_stating_no_capability_is_asked_no_show_at_a_close() {
    let Some(o) = operated(&["show", "load", "save-point"]).await else {
        return;
    };
    let mut o = readmitted_older(o, &["show", "load", "save-point"]).await;
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: chrono::Utc::now().timestamp_millis() as u64,
            tail: None,
        })
        .await;
    wait_row(&o, "the door closed", |r| r.trace_door == Some(false)).await;
    no_frame(&mut o.admin_con).await;
}

/// **Where the live admin-con is asked no `show` at a close, the line says
/// nothing reads the state** (Spec 2.13): an admin-con older than the
/// first capability, its ceiling granting `show`, leaves a state by event
/// under the closed door standing until the door opens or admin-con is
/// admitted again, and the page says so rather than claim a read.
///
/// Perturbation: drop the third wording, and the page says `show` reads a
/// state nothing will read.
#[tokio::test]
async fn a_state_by_event_under_a_door_an_older_admin_con_closed_says_nothing_reads_it() {
    let Some(o) = operated(&["show", "load", "save-point"]).await else {
        return;
    };
    let o = readmitted_older(o, &["show", "load", "save-point"]).await;
    let mut o = o;
    let state_ms = chrono::Utc::now().timestamp_millis();
    o.admin_con.send(live_at(1, "turn.closed", state_ms)).await;
    wait_row(&o, "idle by the turn's close", |r| {
        r.state_source.as_deref() == Some("event")
    })
    .await;
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: (state_ms + 40) as u64,
            tail: None,
        })
        .await;
    let row = wait_row(&o, "the door closed", |r| r.trace_door == Some(false)).await;
    let state_at = to_the_ms(row.load_state_at.unwrap());
    let door_at = to_the_ms(row.trace_door_at.unwrap());
    let shown_page = page(&o).await;
    let said = format!(
        "<tr><th>Load state</th><td>idle, by event at {state_at}. The trace door closed after it, at {door_at}: no event has moved it since, and this admin-con is asked no show at a close, so nothing reads it until the door opens or admin-con is admitted again.</td></tr>"
    );
    assert!(shown_page.contains(&said), "{said}: {shown_page}");
    let warning = warning_beside(&shown_page, &o.karl, "load").expect("load is warned");
    assert!(
        warning.contains("no event has moved it since, and this admin-con is asked no show at a close, so nothing reads it until the door opens or admin-con is admitted again."),
        "{warning}"
    );
    assert!(!shown_page.contains("show reads it"), "{shown_page}");
}
