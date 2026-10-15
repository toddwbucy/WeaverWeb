//! Alerts and the operator's decisions (Spec 2.14; act 12, PR 1;
//! `src/alerting.rs`, `src/store/alerts.rs`, `src/surfaces/alerts.rs`): what
//! the listener raises and what it does not, the operations log, the
//! decision prompt and its sequence, and the acknowledgement. Each test runs
//! on a store of its own against a real listener and fake connectors
//! dialing it with minted configs. No name is a real one.

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;

use crate::admin_tests::{form, send_as, signed_in};
use crate::agents_tests::Rig;
use crate::alerting::Failure;
use crate::host;
use crate::link::frames::{FromClient, Plane, TurnFault, VerbOutcome};
use crate::link::listener::AskPoint;
use crate::passkeys_tests::authenticator;
use crate::store::AgentId;
use crate::store::alerts::{Alert, AlertKind, Raise};
use crate::verbs_tests::{
    Operated, SAID, admitted_admin, ask, close, dial_gate, no_frame, operated, operated_with,
    place, take_turn, take_verb,
};

/// The agent's alerts, open first, newest first.
async fn alerts(rig: &Rig, karl: &str) -> Vec<Alert> {
    let id: AgentId = karl.parse().unwrap();
    rig.store().alerts_on(&id, 50).await.unwrap()
}

/// Poll the agent's alerts until a condition holds.
async fn wait_alerts(
    rig: &Rig,
    karl: &str,
    what: &str,
    cond: impl Fn(&[Alert]) -> bool,
) -> Vec<Alert> {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let found = alerts(rig, karl).await;
        if cond(&found) {
            return found;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the alerts never read {what}: {found:#?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Poll the operations log until it holds `lines` lines.
async fn wait_log(rig: &Rig, lines: usize) -> Vec<serde_json::Value> {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let log = rig.operations_log();
        if log.len() >= lines {
            return log;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the operations log never held {lines} lines: {log:#?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// admin's refusal of a verb on karl, with its stderr.
fn refused(verb: &str, object: serde_json::Value, stderr: Option<&str>) -> VerbOutcome {
    VerbOutcome {
        verb: verb.into(),
        agent: "karl".into(),
        exit_code: Some(1),
        answer: Some(object),
        raw_stdout: None,
        stderr: stderr.map(Into::into),
    }
}

/// admin's answer to a verb on karl.
fn answered(verb: &str, object: serde_json::Value) -> VerbOutcome {
    VerbOutcome {
        verb: verb.into(),
        agent: "karl".into(),
        exit_code: Some(0),
        answer: Some(object),
        raw_stdout: None,
        stderr: None,
    }
}

/// bea asks `verb` from the page and admin-con answers it with `outcome`;
/// the page's status.
async fn asked_and_answered(o: &mut Operated, verb: &str, outcome: VerbOutcome) -> StatusCode {
    asked_and_shown(o, verb, outcome).await.0
}

/// Bea asks the verb, admin-con answers it, and the verb page is shown.
async fn asked_and_shown(
    o: &mut Operated,
    verb: &str,
    outcome: VerbOutcome,
) -> (StatusCode, String) {
    let asked = ask(&o.rig, &o.bea_session, &o.karl, verb);
    let (id, got, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(got, verb);
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome),
            error: None,
        })
        .await;
    let (status, _, page) = asked.await.unwrap();
    (status, page)
}

/// **A refused `load` raises one alert carrying admin's kind, its message
/// and stderr, and one operations-log line; an identical refusal again
/// updates the same alert**, its count two and its version moved.
///
/// Perturbation: raise without resolving the open alert's conflict, and a
/// repeat adds a second alert.
#[tokio::test]
async fn a_refused_load_raises_one_alert_and_one_log_line() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    for round in 1..=2 {
        let refusal = refused(
            "load",
            json!({"kind": "config_invalid", "message": "the declaration is not whole"}),
            Some("the loader said why"),
        );
        assert_eq!(
            asked_and_answered(&mut o, "load", refusal).await,
            StatusCode::OK
        );
        let found = wait_alerts(&o.rig, &o.karl, "the refusal's alert", |a| {
            a.iter().any(|x| x.occurrences == round)
        })
        .await;
        assert_eq!(found.len(), 1, "one open alert: {found:#?}");
        let alert = &found[0];
        assert_eq!(
            (alert.kind.as_str(), alert.subject.as_str()),
            ("failed", "load")
        );
        assert!(
            alert.summary.contains("config_invalid"),
            "{}",
            alert.summary
        );
        assert!(
            alert.detail.contains("the declaration is not whole"),
            "{}",
            alert.detail
        );
        assert!(
            alert.detail.contains("the loader said why"),
            "{}",
            alert.detail
        );
        assert_eq!(alert.version, i64::from(round));
        let log = wait_log(&o.rig, round as usize).await;
        let line = log.last().unwrap();
        assert_eq!(line["agent"], o.karl.as_str());
        assert_eq!(line["act"], "load");
        assert_eq!(line["outcome"], "failed");
        assert_eq!(line["principal"], o.bea.as_str());
        assert_eq!(line["alert"], alert.alert_id.as_str());
        assert!(line["first"].as_str().is_some_and(|f| f.starts_with("au-")));
    }
}

/// **Raisers at once leave one open alert counting them all**: sixteen raises
/// of one agent, kind and subject, run concurrently on the store, leave one
/// alert whose count and version are sixteen.
///
/// Perturbation: drop the open alert's index and the insert's conflict
/// clause, and the raisers leave sixteen alerts.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn raisers_at_once_leave_one_alert_counting_them_all() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let id: AgentId = o.karl.parse().unwrap();
    let raisers: Vec<_> = (0..16)
        .map(|_| {
            let (store, agent) = (o.rig.store().clone(), id.clone());
            tokio::spawn(async move {
                store
                    .raise_alert(&Raise {
                        agent,
                        kind: AlertKind::Unknown,
                        subject: "stop".into(),
                        summary: "raised at once".into(),
                        detail: String::new(),
                        first: None,
                        options: Vec::new(),
                    })
                    .await
            })
        })
        .collect();
    for raiser in raisers {
        raiser.await.unwrap().unwrap();
    }
    let found = alerts(&o.rig, &o.karl).await;
    assert_eq!(found.len(), 1, "one open alert: {found:#?}");
    assert_eq!((found[0].occurrences, found[0].version), (16, 16));
}

/// **A link cut with a verb in flight raises an unknown outcome, and lost
/// contact**, the agent possibly still running.
#[tokio::test]
async fn a_link_cut_with_a_verb_in_flight_raises_unknown_and_lost_contact() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    let _ = take_verb(&mut o.admin_con).await;
    let Operated {
        rig,
        karl,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    let found = wait_alerts(&rig, &karl, "unknown and lost contact", |a| a.len() == 2).await;
    let kinds: Vec<_> = found
        .iter()
        .map(|a| (a.kind.as_str(), a.subject.as_str()))
        .collect();
    assert!(kinds.contains(&("unknown", "load")), "{kinds:?}");
    assert!(kinds.contains(&("lost_contact", "admin")), "{kinds:?}");
    let (_, _, page) = asked.await.unwrap();
    assert!(page.contains("unknown here"), "{page}");
}

/// **The server's own `show` left in flight at a teardown raises no
/// `unknown`**: it acts on nothing, and the teardown's lost contact stands
/// for the connection.
///
/// Perturbation: keep the server's `show` among the in-flight unknowns, and
/// the cut raises an `unknown` for it.
#[tokio::test]
async fn the_servers_show_in_flight_at_a_cut_raises_no_unknown() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (listener, id) = (o.rig.listener.clone(), o.karl.parse::<AgentId>().unwrap());
    let shown = tokio::spawn(async move { listener.verb(&id, "show").await });
    let (_, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "show");
    let Operated {
        rig,
        karl,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    let _ = shown.await.unwrap();
    let found = wait_alerts(&rig, &karl, "lost contact", |a| {
        a.iter().any(|x| x.kind == "lost_contact")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let found = if found.len() == 1 {
        alerts(&rig, &karl).await
    } else {
        found
    };
    let kinds: Vec<_> = found
        .iter()
        .map(|a| (a.kind.as_str(), a.subject.as_str()))
        .collect();
    assert_eq!(kinds, [("lost_contact", "admin")]);
}

/// **Lost contact raises one alert per plane, a flapping link one and not
/// many, its restoration noted; a revocation, the server's own act, raises
/// none.**
///
/// Perturbation: drop the dedupe, and a flapping link raises an alert per
/// loss.
#[tokio::test]
async fn lost_contact_raises_one_alert_and_a_revocation_none() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let gate = dial_gate(&o).await;
    drop(gate);
    let found = wait_alerts(&o.rig, &o.karl, "the gate's lost contact", |a| {
        a.iter()
            .any(|x| x.kind == "lost_contact" && x.subject == "gate")
    })
    .await;
    assert!(
        found[0].summary.contains("may still be running"),
        "{}",
        found[0].summary
    );
    let gate = dial_gate(&o).await;
    wait_alerts(&o.rig, &o.karl, "the restoration noted", |a| {
        a.iter()
            .any(|x| x.subject == "gate" && x.restored_at.is_some())
    })
    .await;
    drop(gate);
    let found = wait_alerts(&o.rig, &o.karl, "the second loss counted", |a| {
        a.iter().any(|x| x.subject == "gate" && x.occurrences == 2)
    })
    .await;
    assert_eq!(found.len(), 1, "one alert for a flapping link: {found:#?}");
    assert!(found[0].restored_at.is_none(), "lost again");

    let s = o.rig.store().clone();
    let id: AgentId = o.karl.parse().unwrap();
    let row = s.agent(&id).await.unwrap().unwrap();
    s.revoke_credential(&row, Plane::Admin, Some("lab"))
        .await
        .unwrap();
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    while o.rig.listener.connected(&id, Plane::Admin) {
        assert!(
            tokio::time::Instant::now() < until,
            "the revocation closed it"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let found = alerts(&o.rig, &o.karl).await;
    assert!(
        !found.iter().any(|a| a.subject == "admin"),
        "a revocation raises no lost contact: {found:#?}"
    );
}

/// **An `unload` stopped at its save point raises the decision prompt**,
/// shown on the agent page as an open dialog with its two options; one
/// whose save point did not publish raises an explaining alert with none.
#[tokio::test]
async fn an_unload_stopped_at_its_save_point_raises_the_prompt() {
    let Some(mut o) = operated(&["show", "unload", "save-point", "force-unload"]).await else {
        return;
    };
    let refusal = refused(
        "unload",
        json!({"kind": "save_point_not_taken", "missed": "finished"}),
        None,
    );
    let (_, shown) = asked_and_shown(&mut o, "unload", refusal).await;
    assert!(
        shown.contains("<dialog open>"),
        "the verb page states the prompt: {shown}"
    );
    assert!(shown.contains("missed: finished"), "{shown}");
    let found = wait_alerts(&o.rig, &o.karl, "the prompt", |a| {
        a.iter().any(|x| x.kind == "save_point_missed")
    })
    .await;
    let prompt = found
        .iter()
        .find(|a| a.kind == "save_point_missed")
        .unwrap();
    assert_eq!(prompt.options, ["save_point_then_unload", "force_unload"]);
    let (_, _, page) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}", o.karl),
        None,
        &o.bea_session,
    )
    .await;
    assert!(page.contains("<dialog open>"), "{page}");
    assert!(page.contains("value=\"save_point_then_unload\""), "{page}");
    assert!(page.contains("value=\"force_unload\""), "{page}");

    let refusal = refused(
        "unload",
        json!({"kind": "save_point_not_taken", "missed": "published"}),
        None,
    );
    let (_, shown) = asked_and_shown(&mut o, "unload", refusal).await;
    assert!(
        !shown.contains("<dialog"),
        "no prompt where the run ended: {shown}"
    );
    let found = wait_alerts(&o.rig, &o.karl, "the explaining alert", |a| {
        a.iter().any(|x| x.kind == "data_at_risk")
    })
    .await;
    let explaining = found.iter().find(|a| a.kind == "data_at_risk").unwrap();
    assert!(explaining.options.is_empty());
}

/// The open prompt's identity, raised by an unload stopped at `finished`.
async fn a_prompt(o: &mut Operated) -> String {
    let refusal = refused(
        "unload",
        json!({"kind": "save_point_not_taken", "missed": "finished"}),
        None,
    );
    asked_and_answered(o, "unload", refusal).await;
    let found = wait_alerts(&o.rig, &o.karl, "the prompt", |a| {
        a.iter().any(|x| x.kind == "save_point_missed")
    })
    .await;
    found
        .iter()
        .find(|a| a.kind == "save_point_missed")
        .unwrap()
        .alert_id
        .clone()
}

/// bea chooses an option of the prompt; the request runs as its own task.
fn choose(
    o: &Operated,
    alert: &str,
    option: &str,
) -> tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)> {
    let (app, session, uri, body) = (
        o.rig.app.clone(),
        o.bea_session.clone(),
        format!("/agents/{}/alerts/{alert}/choose", o.karl),
        form(&[("option", option)]),
    );
    tokio::spawn(async move { send_as(&app, "POST", &uri, Some(body), &session).await })
}

/// **"Save point, then unload" asks `unload` only on a hash-confirmed save
/// point**: `save_point_taken` with its digest leads to the `unload`; a
/// `save_point_not_taken`, or a `save_point_taken` with no report, sends no
/// `unload`, the last raising its own alert. Choosing leaves the prompt
/// open.
///
/// Perturbation: ask `unload` on any `save-point` answer, and a missed save
/// point is followed by an unload.
#[tokio::test]
async fn save_point_then_unload_asks_unload_only_on_a_confirmed_save_point() {
    let Some(mut o) = operated(&["show", "unload", "save-point", "force-unload"]).await else {
        return;
    };
    let prompt = a_prompt(&mut o).await;

    let chosen = choose(&o, &prompt, "save_point_then_unload");
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "save-point");
    let report = json!({"save_point": "d0c0ffee", "name": "n-1", "run": "r-1",
        "sequence": 1, "turn": 1, "event_run": "r-1", "position": 9});
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answered(
                "save-point",
                json!({"kind": "save_point_taken", "report": report}),
            )),
            error: None,
        })
        .await;
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "unload", "the unload follows the confirmation");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answered(
                "unload",
                json!({"kind": "state", "state": "unloaded"}),
            )),
            error: None,
        })
        .await;
    let (status, _, page) = chosen.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains("d0c0ffee") && page.contains("admin&#39;s answer"),
        "{page}"
    );
    let still = alerts(&o.rig, &o.karl).await;
    assert!(
        still
            .iter()
            .any(|a| a.alert_id == prompt && a.acknowledged_at.is_none()),
        "choosing does not acknowledge"
    );

    for (case, outcome) in [
        (
            "a missed save point",
            refused(
                "save-point",
                json!({"kind": "save_point_not_taken", "missed": "answer"}),
                None,
            ),
        ),
        (
            "no report",
            answered("save-point", json!({"kind": "save_point_taken"})),
        ),
    ] {
        let chosen = choose(&o, &prompt, "save_point_then_unload");
        let (id, verb, _) = take_verb(&mut o.admin_con).await;
        assert_eq!(verb, "save-point", "{case}");
        o.admin_con
            .send(FromClient::Verb {
                id,
                outcome: Some(outcome),
                error: None,
            })
            .await;
        let (status, _, page) = chosen.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{case}: {page}");
        assert!(page.contains("no unload sent"), "{case}: {page}");
        no_frame(&mut o.admin_con).await;
    }
    let found = alerts(&o.rig, &o.karl).await;
    assert!(
        found
            .iter()
            .any(|a| a.subject == "save-point" && a.summary.contains("hash-confirmed")),
        "the unconfirmed save point raised its own alert: {found:#?}"
    );
}

/// **One recovery holds exactly one place for its whole life**: bea holds
/// three asks' places and chooses "save point, then unload"; the choice
/// takes the fourth and its save-point and its unload are both asked under
/// it, bea never holding more than her share of four, and the place is
/// given back once the sequence ends.
///
/// Perturbation: admit again inside the sequence, and with bea's share
/// full the save-point is shed.
#[tokio::test]
async fn one_recovery_holds_one_place_for_its_whole_life() {
    let Some(mut o) = operated(&["show", "unload", "save-point", "force-unload"]).await else {
        return;
    };
    let prompt = a_prompt(&mut o).await;
    let listener = o.rig.listener.clone();
    let held: Vec<_> = (0..3)
        .map(|_| listener.admit_ask(&o.bea).expect("a place of bea's share"))
        .collect();

    let chosen = choose(&o, &prompt, "save_point_then_unload");
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "save-point", "the save-point is asked, not shed");
    assert!(
        listener.admit_ask(&o.bea).is_none(),
        "the choice holds the fourth"
    );
    let report = json!({"save_point": "d0c0ffee", "name": "n-1", "run": "r-1",
        "sequence": 1, "turn": 1, "event_run": "r-1", "position": 9});
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answered(
                "save-point",
                json!({"kind": "save_point_taken", "report": report}),
            )),
            error: None,
        })
        .await;
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "unload", "the unload is asked under the same place");
    assert!(
        listener.admit_ask(&o.bea).is_none(),
        "the place is held across the wait"
    );
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answered(
                "unload",
                json!({"kind": "state", "state": "unloaded"}),
            )),
            error: None,
        })
        .await;
    let (status, _, page) = chosen.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("d0c0ffee"), "{page}");
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    while listener.admit_ask(&o.bea).is_none() {
        assert!(
            tokio::time::Instant::now() < until,
            "the place is given back once the sequence ends"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    drop(held);
}

/// **Choosing without a grant answers as an unknown agent**, an alert that
/// stands and one that does not alike, and asks nothing, so the page is no
/// oracle for alert identities.
///
/// Perturbation: resolve the alert before the grant check, and a real
/// prompt answers otherwise than an unknown one.
#[tokio::test]
async fn choosing_without_a_grant_answers_as_unknown() {
    let Some(mut o) = operated(&["show", "unload", "save-point", "force-unload"]).await else {
        return;
    };
    let prompt = a_prompt(&mut o).await;
    let s = o.rig.store().clone();
    let (_, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    for alert in [prompt.as_str(), "al-0000000000000000"] {
        let (status, _, _) = send_as(
            &o.rig.app,
            "POST",
            &format!("/agents/{}/alerts/{alert}/choose", o.karl),
            Some(form(&[("option", "force_unload")])),
            &dan_session,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{alert}");
    }
    no_frame(&mut o.admin_con).await;
}

/// The audit's records on alerts, as (person, action, outcome, refusal).
async fn alert_records(rig: &Rig) -> Vec<(Option<String>, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT person_id, outcome, refusal FROM audit WHERE target_kind = 'alert' \
         ORDER BY at, audit_id",
    )
    .fetch_all(&rig.store().pool)
    .await
    .unwrap()
}

/// **Who sees an alert and who acknowledges it**: an observer sees it and
/// is refused acknowledging (403, one record); a person with no grant is
/// answered as an unknown agent (404, one record); an `operator` holder's
/// acknowledgement naming a stale version is refused (409) and one naming
/// the current version lands with its audit pair.
///
/// Perturbation: drop the acknowledgement's version, and a repeat landing
/// after the page was read is acknowledged unseen.
#[tokio::test]
async fn an_alert_is_seen_by_any_grant_and_acknowledged_by_an_operator_at_its_version() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let refusal = || refused("load", json!({"kind": "config_invalid"}), None);
    asked_and_answered(&mut o, "load", refusal()).await;
    let found = wait_alerts(&o.rig, &o.karl, "the alert", |a| a.len() == 1).await;
    let (alert, read) = (found[0].alert_id.clone(), found[0].version);

    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (_, _, page) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}", o.karl),
        None,
        &cara_session,
    )
    .await;
    assert!(
        page.contains("config_invalid"),
        "the observer sees it: {page}"
    );
    assert!(
        !page.contains("/ack\""),
        "and is offered no acknowledgement"
    );
    let (app_, karl_) = (o.rig.app.clone(), o.karl.clone());
    let ack = |session: &str, version: i64| {
        let (app, session, uri, body) = (
            app_.clone(),
            session.to_owned(),
            format!("/agents/{karl_}/alerts/{alert}/ack"),
            form(&[("version", &version.to_string())]),
        );
        async move { send_as(&app, "POST", &uri, Some(body), &session).await }
    };
    assert_eq!(ack(&cara_session, read).await.0, StatusCode::FORBIDDEN);
    let (_, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    assert_eq!(ack(&dan_session, read).await.0, StatusCode::NOT_FOUND);
    let records = alert_records(&o.rig).await;
    assert_eq!(records.len(), 2, "one refusal record each: {records:#?}");
    assert!(records.iter().all(|(_, _, refusal)| refusal.is_some()));

    // A repeat after bea read the page moves the version.
    asked_and_answered(&mut o, "load", refusal()).await;
    wait_alerts(&o.rig, &o.karl, "the repeat", |a| a[0].occurrences == 2).await;
    let (status, _, body) = ack(&o.bea_session, read).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let current = alerts(&o.rig, &o.karl).await[0].version;
    let (status, _, _) = ack(&o.bea_session, current).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let acked = alerts(&o.rig, &o.karl).await;
    assert_eq!(acked[0].acknowledged_by.as_deref(), Some(o.bea.as_str()));
    let records = alert_records(&o.rig).await;
    let beas: Vec<_> = records
        .iter()
        .filter(|(person, _, _)| person.as_deref() == Some(o.bea.as_str()))
        .collect();
    assert_eq!(beas.len(), 4, "two firsts, failed and ok: {beas:#?}");
    assert!(
        beas.iter()
            .any(|(_, outcome, _)| outcome.as_deref() == Some("failed"))
    );
    assert!(
        beas.iter()
            .any(|(_, outcome, _)| outcome.as_deref() == Some("ok"))
    );
}

/// **An authorization refusal raises no alert**: it is the audit's, not an
/// operational failure.
///
/// Perturbation: raise an alert for an authorization refusal, and the
/// observer's refused `stop` raises one.
#[tokio::test]
async fn an_authorization_refusal_raises_no_alert() {
    let Some(o) = operated(&["show", "stop"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, _, _) = ask(&o.rig, &cara_session, &o.karl, "stop").await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(alerts(&o.rig, &o.karl).await.is_empty());
    assert!(o.rig.operations_log().is_empty());
}

/// **A person holding no grant raises no alert, even where the server
/// fails them before the check**: dan's ask, held before the hold past a
/// four-second silence bound, is the server's stall, and nothing is raised
/// on dan's behalf on an agent dan cannot see, no alert and no
/// operations-log line.
///
/// Perturbation: raise a system refusal whether or not the authorization
/// stood, and dan's stalled ask raises a `refused` alert on karl.
#[tokio::test]
async fn a_person_without_a_grant_raises_nothing_though_the_server_stalls() {
    let Some(o) = operated_with(&["show", "load"], Duration::from_secs(4)).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (_, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::BeforeHold);
    let asked = ask(&o.rig, &dan_session, &o.karl, "load");
    reached.notified().await;
    tokio::time::sleep(Duration::from_millis(4500)).await;
    release.notify_one();
    let (status, _, _) = asked.await.unwrap();
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "the stall answered"
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    // The fake admin-con, quiet past the four-second bound, is closed as
    // a silent peer meanwhile: that lost contact is the server's own
    // observation and may stand. Nothing is raised on dan's behalf.
    let found = alerts(&o.rig, &o.karl).await;
    assert!(found.iter().all(|a| a.kind == "lost_contact"), "{found:#?}");
    let dan: Vec<_> = o
        .rig
        .operations_log()
        .into_iter()
        .filter(|l| l["principal"] != "server")
        .collect();
    assert!(dan.is_empty(), "{dan:?}");
}

/// **A turn's text reaches no alert and no operations-log line**: a turn
/// the gate stops raises its alert, with the agent's reason and nothing of
/// the person's text.
#[tokio::test]
async fn a_turns_text_reaches_no_alert_and_no_log_line() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let placed = place(&o.rig, &o.bea_session, &o.karl, SAID);
    let (id, _) = take_turn(&mut gate).await;
    gate.send(FromClient::Turn {
        id,
        close: Some(close(
            "stopped",
            None,
            Some("a hole in the structure"),
            None,
        )),
        error: None,
    })
    .await;
    assert_eq!(placed.await.unwrap().0, StatusCode::OK);
    let found = wait_alerts(&o.rig, &o.karl, "the stopped turn's alert", |a| {
        a.iter().any(|x| x.subject == "turn")
    })
    .await;
    let log = wait_log(&o.rig, 1).await;
    let everything = format!("{found:?} {log:?}");
    assert!(everything.contains("a hole in the structure"));
    for part in [SAID, "zebra", "kumquat"] {
        assert!(
            !everything.contains(part),
            "the text reached an alert or a line"
        );
    }
}

/// **A full queue drops and counts, and never holds the door**: with the
/// worker held and its queue filled, a refused verb's answer still reaches
/// its asker at once, and the drops are counted.
///
/// Perturbation: wait on the queue in the raise instead of dropping, and
/// the door holds the answer behind the stalled worker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_full_queue_drops_and_counts_and_never_holds_the_door() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let id: AgentId = o.karl.parse().unwrap();
    let alerts_ = o.rig.listener.alerts().clone();
    let held = alerts_.stall_worker().await;
    // Raised from a task of its own and bounded, so a raise that waited on
    // the full queue fails here rather than holding the test.
    let fill = |count: usize| {
        let filler = alerts_.clone();
        let agent = id.clone();
        tokio::time::timeout(
            Duration::from_secs(5),
            tokio::spawn(async move {
                for n in 0..count {
                    filler.raise(Failure {
                        raise: Raise {
                            agent: agent.clone(),
                            kind: AlertKind::Failed,
                            subject: format!("filler-{n}"),
                            summary: "filler".into(),
                            detail: String::new(),
                            first: None,
                            options: Vec::new(),
                        },
                        plane: "admin",
                        principal: "server".into(),
                        outcome: "failed",
                        reason: json!({}),
                    });
                }
            }),
        )
    };
    assert!(fill(300).await.is_ok(), "raising waited on the full queue");
    assert!(alerts_.dropped() > 0, "the full queue dropped");
    // The worker takes one note into its hand and stalls there, freeing a
    // slot; let it, then fill that slot, so the queue stands full.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let before = alerts_.dropped();
    assert!(fill(2).await.is_ok(), "raising waited on the full queue");
    assert!(alerts_.dropped() > before, "the queue stands full");
    let before = alerts_.dropped();
    let answered = tokio::time::timeout(
        Duration::from_secs(5),
        asked_and_answered(
            &mut o,
            "load",
            refused("load", json!({"kind": "config_invalid"}), None),
        ),
    )
    .await
    .expect("the door answered at once with the worker held");
    assert_eq!(answered, StatusCode::OK);
    assert!(
        alerts_.dropped() > before,
        "the door's failure was dropped and counted"
    );
    drop(held);
}

/// Wait until a person's fetch of `uri` answers a body for which `cond`
/// holds.
async fn wait_body(
    app: &axum::Router,
    session: &str,
    uri: &str,
    what: &str,
    cond: impl Fn(&str) -> bool,
) -> String {
    let until = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let (_, _, body) = send_as(app, "GET", uri, None, session).await;
        if cond(&body) {
            return body;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "{uri} never read {what}: {body}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **A plane the server cannot reach is a banner on every page of a person
/// holding a grant on the agent, and on no other person's**: gate-con never
/// connected shows "turns unavailable, gate-con down" on a page that is not
/// the agent's; admin-con cut shows "agent unavailable, admin-con down" in
/// the fragment the page re-fetches; each clears once its plane is
/// admitted again. A person holding no grant sees none of it.
///
/// Perturbation: drop the grant scoping of the unavailable planes, and a
/// person holding no grant sees karl's banners.
#[tokio::test]
async fn an_unreachable_plane_is_a_banner_for_its_grant_holders_alone() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (_, dan) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    let (_, _, page) = send_as(&o.rig.app, "GET", "/passkeys", None, &o.bea_session).await;
    assert!(
        page.contains("box-a/karl</a>: turns unavailable, gate-con down (never connected)"),
        "{page}"
    );
    assert!(!page.contains("admin-con down"), "{page}");
    let (_, _, theirs) = send_as(&o.rig.app, "GET", "/passkeys", None, &dan).await;
    assert!(!theirs.contains("unavailable"), "{theirs}");

    let gate = dial_gate(&o).await;
    let app = o.rig.app.clone();
    let bea = o.bea_session.clone();
    wait_body(&app, &bea, "/availability", "the gate banner gone", |b| {
        !b.contains("gate-con down")
    })
    .await;
    let Operated {
        rig,
        admin_config,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    let down = wait_body(&app, &bea, "/availability", "admin-con down", |b| {
        b.contains("box-a/karl</a>: agent unavailable, admin-con down (since ")
    })
    .await;
    assert!(!down.contains("<html"), "a fragment: {down}");
    let (status, _, theirs) = send_as(&app, "GET", "/availability", None, &dan).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!theirs.contains("unavailable"), "{theirs}");
    let again = admitted_admin(&admin_config, &["show"]).await;
    wait_body(&app, &bea, "/availability", "the admin banner gone", |b| {
        !b.contains("admin-con down")
    })
    .await;
    drop((gate, again, rig));
}

/// **The agent's connectors as the listener knows them, re-fetched by its
/// page**: "last heard" counts up while admin-con sends nothing and falls
/// back when it sends a heartbeat, then reads "admin-con down" once the
/// link is cut, each by the fragment alone; a person holding no grant is
/// answered as for an unknown agent.
///
/// Perturbation: drop the grant check from the fragment, and a person
/// holding no grant reads karl's connectors.
#[tokio::test]
async fn the_agents_connectors_show_last_heard_and_the_cut() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (_, dan) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    let uri = format!("/agents/{}/availability", o.karl);
    let (app, bea) = (o.rig.app.clone(), o.bea_session.clone());
    let (_, _, first) = send_as(&app, "GET", &uri, None, &bea).await;
    assert!(
        first.contains("admin-con: connected, last heard"),
        "{first}"
    );
    assert!(
        first.contains("gate-con: turns unavailable, gate-con down, never connected"),
        "{first}"
    );
    let (_, _, page) = send_as(&app, "GET", &format!("/agents/{}", o.karl), None, &bea).await;
    assert!(page.contains(&format!("hx-get=\"{uri}\"")), "{page}");
    assert!(page.contains("/assets/surfaces/htmx.min.js"), "{page}");
    tokio::time::sleep(Duration::from_millis(2200)).await;
    let (_, _, quiet) = send_as(&app, "GET", &uri, None, &bea).await;
    assert!(
        quiet.contains("last heard 2 s ago") || quiet.contains("last heard 3 s ago"),
        "{quiet}"
    );
    o.admin_con.send(FromClient::Heartbeat).await;
    wait_body(&app, &bea, &uri, "last heard again", |b| {
        b.contains("last heard 0 s ago")
    })
    .await;
    let (status, _, theirs) = send_as(&app, "GET", &uri, None, &dan).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{theirs}");
    let Operated { rig, admin_con, .. } = o;
    drop(admin_con);
    wait_body(&app, &bea, &uri, "admin-con down", |b| {
        b.contains("admin-con: agent unavailable, admin-con down since ")
    })
    .await;
    drop(rig);
}

/// **A re-fetch is not the person's use**: the fragments leave a session's
/// last use as it was, so an open tab never holds an idle session open;
/// a page refreshes it.
///
/// Perturbation: refresh the last use at a background read, and the poll
/// moves it.
#[tokio::test]
async fn a_poll_leaves_the_sessions_last_use_as_it_was() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let pool = &o.rig.store().pool;
    sqlx::query(
        "UPDATE session SET last_used_at = now() - interval '10 minutes' WHERE person_id = $1",
    )
    .bind(&o.bea)
    .execute(pool)
    .await
    .unwrap();
    let last = || async {
        sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
            "SELECT max(last_used_at) FROM session WHERE person_id = $1",
        )
        .bind(&o.bea)
        .fetch_one(pool)
        .await
        .unwrap()
    };
    let before = last().await;
    for uri in [
        "/availability".to_owned(),
        format!("/agents/{}/availability", o.karl),
    ] {
        let (status, _, _) = send_as(&o.rig.app, "GET", &uri, None, &o.bea_session).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
    }
    assert_eq!(last().await, before, "a poll moved the last use");
    let (status, _, _) = send_as(&o.rig.app, "GET", "/availability", None, "no-such-bearer").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "an ended session's poll");
    send_as(&o.rig.app, "GET", "/agents", None, &o.bea_session).await;
    assert!(last().await > before, "a page refreshes it");
}

/// **An answer refused at the door leaves its act's outcome unknown, and
/// raises its alert**: admin-con answers bea's `load` naming another verb,
/// the server refuses it as malformed and closes, and the `load` is an
/// `unknown` alert naming its first record, beside the lost contact. The
/// same on the gate plane, a turn answered with both a close and a fault.
///
/// Perturbation: leave the refused answer's ask out of the teardown's
/// drain, and only the lost contact stands.
#[tokio::test]
async fn an_answer_refused_at_the_door_raises_its_acts_unknown() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answered("stop", json!({"kind": "state", "state": "idle"}))),
            error: None,
        })
        .await;
    let found = wait_alerts(&o.rig, &o.karl, "the load's unknown", |a| {
        a.iter().any(|x| x.kind == "unknown" && x.subject == "load")
    })
    .await;
    let load = found
        .iter()
        .find(|x| x.kind == "unknown" && x.subject == "load")
        .unwrap();
    assert!(
        load.first_record
            .as_deref()
            .is_some_and(|f| f.starts_with("au-")),
        "{load:#?}"
    );
    assert!(
        found
            .iter()
            .any(|x| x.kind == "lost_contact" && x.subject == "admin"),
        "{found:#?}"
    );
    let _ = asked.await.unwrap();

    let placed = place(&o.rig, &o.bea_session, &o.karl, "a turn of the test's");
    let (id, _) = take_turn(&mut gate).await;
    gate.send(FromClient::Turn {
        id,
        close: Some(close("answered", Some("an answer"), None, Some("stop"))),
        error: Some(TurnFault {
            kind: "busy".into(),
            message: "both at once".into(),
        }),
    })
    .await;
    let found = wait_alerts(&o.rig, &o.karl, "the turn's unknown", |a| {
        a.iter().any(|x| x.kind == "unknown" && x.subject == "turn")
    })
    .await;
    let turn = found
        .iter()
        .find(|x| x.kind == "unknown" && x.subject == "turn")
        .unwrap();
    assert!(
        turn.first_record
            .as_deref()
            .is_some_and(|f| f.starts_with("au-")),
        "{turn:#?}"
    );
    assert!(
        found
            .iter()
            .any(|x| x.kind == "lost_contact" && x.subject == "gate"),
        "{found:#?}"
    );
    let _ = placed.await.unwrap();
}

/// **A plane's loss is ordered before the restoration that follows it**:
/// admin-con is cut, its teardown held after the row is released, and a
/// new admin-con admitted in that gap; the lost-contact alert ends
/// restored, the loss having been raised where the entry came out.
///
/// Perturbation: raise the loss after the teardown, and the restoration
/// is applied to no alert first and the loss raised unrestored after it.
#[tokio::test]
async fn a_loss_is_ordered_before_the_restoration_after_it() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::AfterTeardown);
    let Operated {
        rig,
        karl,
        admin_config,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    reached.notified().await;
    let again = admitted_admin(&admin_config, &["show"]).await;
    release.notify_one();
    let found = wait_alerts(&rig, &karl, "the loss restored", |a| {
        a.iter()
            .any(|x| x.kind == "lost_contact" && x.restored_at.is_some())
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let found = if found.len() == 1 {
        alerts(&rig, &karl).await
    } else {
        found
    };
    assert!(
        found
            .iter()
            .filter(|x| x.kind == "lost_contact")
            .all(|x| x.restored_at.is_some()),
        "{found:#?}"
    );
    drop(again);
}
