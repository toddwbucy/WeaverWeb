//! A person's lifecycle verbs, asked through the server (Spec 2.13; act
//! 11, PR 6b2; `src/surfaces/reading.rs` over `Listener::ask_as_person`):
//! the grant on the agent, the session, the connection and its ceiling
//! checked under the identity exclusion held shared to the enqueue, each
//! refusal one record, the first record durable before the frame, and the
//! whole ask run where a dropped request cannot cut it. Each test runs on a
//! store of its own against a real listener and a fake admin-con dialing it
//! with a minted config. No name is a real one.

use std::time::Duration;

use axum::http::StatusCode;
use serde_json::json;

use crate::adapters::gate::GateClose;
use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::agents_tests::{Captured, Connector, Rig, rig_silenced};
use crate::host;
use crate::link::frames::{
    FromClient, Position, Principal, Refusal, ToClient, TurnFault, VerbOutcome,
};
use crate::link::listener::{Act, AskPoint, Asker, TurnError, TurnText, VerbError};
use crate::passkeys_tests::authenticator;
use crate::store::AgentId;
use crate::store::audit::FAIL_FIRST_RECORD;
use crate::traceview::TraceEvent;

pub(crate) const UNKNOWN: &str = "ag-0123456789abcdef";

/// The bound a refusal is answered within: far under the silence bound, so
/// an ask that leaves instead of being refused fails here and not later.
pub(crate) const REFUSED_WITHIN: Duration = Duration::from_secs(10);

/// An agent, `box-a/karl`, registered by ada, its admin-con connected and
/// admitted with the ceiling given; and bea, signed in and holding
/// `operator` on it.
pub(crate) struct Operated {
    pub(crate) rig: Rig,
    pub(crate) karl: String,
    pub(crate) gate_config: String,
    pub(crate) admin_config: String,
    pub(crate) ada: String,
    pub(crate) ada_session: String,
    pub(crate) bea: String,
    pub(crate) bea_session: String,
    pub(crate) admin_con: Connector,
}

pub(crate) async fn operated(ceiling: &[&str]) -> Option<Operated> {
    operated_with(ceiling, Duration::from_secs(60)).await
}

pub(crate) async fn operated_with(ceiling: &[&str], silence: Duration) -> Option<Operated> {
    let rig = rig_silenced(silence).await?;
    let s = rig.store().clone();
    let (ada, ada_session) = admin(&rig.app, &s, &mut authenticator(), "ada").await;
    let (karl, handles) = rig.register(&ada_session, "box-a", "karl").await;
    let (_, _, config) = rig.take(&handles[1], &ada_session).await;
    let (_, _, gate_config) = rig.take(&handles[0], &ada_session).await;
    let admin_con = admitted_admin(&config, ceiling).await;
    let (bea, bea_session) = signed_in(&rig.app, &s, &mut authenticator(), "bea").await;
    let granted = host::grant_add(&s, &bea, "operator", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    Some(Operated {
        rig,
        karl,
        gate_config,
        admin_config: config,
        ada,
        ada_session,
        bea,
        bea_session,
        admin_con,
    })
}

/// An admin-con dialed with the config and admitted: the hello, the
/// admission's `show` answered idle, and `caught_up`.
pub(crate) async fn admitted_admin(config: &str, ceiling: &[&str]) -> Connector {
    admitted(Connector::dial_with(config, ceiling).await).await
}

/// As [`admitted_admin`], dialed again past the teardown of the
/// connection just dropped ([`Connector::redial`]).
pub(crate) async fn readmitted_admin(config: &str, ceiling: &[&str]) -> Connector {
    admission(Connector::redial(config, ceiling, false).await).await
}

/// As [`readmitted_admin`], an admin-con older than the first capability
/// (Spec 8), its hello stating none.
pub(crate) async fn readmitted_older_admin(config: &str, ceiling: &[&str]) -> Connector {
    admission(Connector::redial(config, ceiling, true).await).await
}

async fn admitted(mut admin_con: Connector) -> Connector {
    assert!(matches!(
        admin_con.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));
    admission(admin_con).await
}

/// The admission after the hello's answer: its `show` answered idle, and
/// `caught_up`.
async fn admission(mut admin_con: Connector) -> Connector {
    let show = match admin_con.recv().await {
        Some(ToClient::Verb { id, verb, .. }) if verb == "show" => id,
        other => panic!("expected the admission's show, got {other:?}"),
    };
    admin_con
        .send(FromClient::Verb {
            id: show,
            outcome: Some(answer("show", json!({"kind": "state", "state": "idle"}), 0)),
            error: None,
        })
        .await;
    assert!(matches!(
        admin_con.recv().await,
        Some(ToClient::Landed { .. })
    ));
    admin_con.send(FromClient::CaughtUp).await;
    admin_con
}

/// admin's answer to a verb on karl, in the invocation's shape.
pub(crate) fn answer(verb: &str, object: serde_json::Value, exit: i32) -> VerbOutcome {
    VerbOutcome {
        verb: verb.into(),
        agent: "karl".into(),
        exit_code: Some(exit),
        answer: Some(object),
        raw_stdout: None,
        stderr: None,
    }
}

/// A verb asked from the page, its request running as its own task.
pub(crate) fn ask(
    rig: &Rig,
    session: &str,
    agent: &str,
    verb: &str,
) -> tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)> {
    let (app, session, uri, body) = (
        rig.app.clone(),
        session.to_owned(),
        format!("/agents/{agent}/verb"),
        form(&[("verb", verb)]),
    );
    tokio::spawn(async move { send_as(&app, "POST", &uri, Some(body), &session).await })
}

/// A verb asked and its answer awaited, which a refusal gives at once.
pub(crate) async fn refused(
    rig: &Rig,
    session: &str,
    agent: &str,
    verb: &str,
) -> (StatusCode, String) {
    let (status, _, body) = tokio::time::timeout(REFUSED_WITHIN, ask(rig, session, agent, verb))
        .await
        .unwrap_or_else(|_| panic!("{verb} was not answered at once: it was asked"))
        .unwrap();
    (status, body)
}

/// The next verb frame admin-con receives.
pub(crate) async fn take_verb(c: &mut Connector) -> (u64, String, Principal) {
    loop {
        match c.recv().await {
            Some(ToClient::Verb {
                id,
                verb,
                principal,
            }) => return (id, verb, principal),
            Some(ToClient::Ack { .. }) => {}
            other => panic!("expected a verb frame, got {other:?}"),
        }
    }
}

/// No verb frame reaches admin-con within the receive bound.
pub(crate) async fn no_frame(c: &mut Connector) {
    while let Some(frame) = c.recv().await {
        assert!(
            !matches!(frame, ToClient::Verb { .. }),
            "a verb frame left the server: {frame:?}"
        );
    }
}

/// One person-principal audit record.
#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub(crate) audit_id: String,
    pub(crate) person_id: Option<String>,
    pub(crate) method: String,
    pub(crate) action: String,
    pub(crate) answers: Option<String>,
    pub(crate) outcome: Option<String>,
    pub(crate) refusal: Option<String>,
}

type Row = (
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Every audit record of a person's verb on the agent, oldest first: the
/// register's own records on it are the admin's acts, not verbs.
pub(crate) async fn records(o: &Operated) -> Vec<Record> {
    records_on(&o.rig, &o.karl).await
}

/// Every audit record a person wrote on the agent named, oldest first.
pub(crate) async fn records_on(rig: &Rig, karl: &str) -> Vec<Record> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT audit_id, person_id, method, action, answers, outcome, refusal FROM audit \
         WHERE principal = 'person' AND target_kind = 'agent' AND target_id = $1 \
         AND action IN ('show', 'validate', 'load', 'unload', 'stop', 'save-point', \
         'restore', 'force-unload', 'turn') ORDER BY at, audit_id",
    )
    .bind(karl)
    .fetch_all(&rig.store().pool)
    .await
    .unwrap();
    rows.into_iter()
        .map(
            |(audit_id, person_id, method, action, answers, outcome, refusal)| Record {
                audit_id,
                person_id,
                method,
                action,
                answers,
                outcome,
                refusal,
            },
        )
        .collect()
}

/// Poll the person's records until a condition holds.
pub(crate) async fn wait_records(
    o: &Operated,
    what: &str,
    cond: impl Fn(&[Record]) -> bool,
) -> Vec<Record> {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let records = records(o).await;
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

/// The one refusal record the person's records hold, as `refused` left it.
fn one_refusal<'a>(records: &'a [Record], person: &str, verb: &str) -> &'a Record {
    assert_eq!(records.len(), 1, "one record and no more: {records:#?}");
    let refusal = &records[0];
    assert_eq!(refusal.person_id.as_deref(), Some(person));
    assert_eq!(refusal.method, "session");
    assert_eq!(refusal.action, verb);
    assert!(refusal.refusal.is_some(), "{refusal:?}");
    refusal
}

pub(crate) fn outcome_of<'a>(records: &'a [Record], first: &Record) -> Option<&'a str> {
    records
        .iter()
        .find(|r| r.answers.as_deref() == Some(first.audit_id.as_str()))
        .and_then(|r| r.outcome.as_deref())
}

/// **A person holding `operator` asks `load`**: one first record (the
/// person, by session, on the agent, `load`) written before the frame, the
/// frame naming the person by `pe-` identity, admin's answer writing `ok`
/// naming it, and the page showing the answer and the record.
#[tokio::test]
async fn an_operator_asks_load_and_the_page_shows_admins_answer() {
    let Some(mut o) = operated(&["show", "load", "stop"]).await else {
        return;
    };
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, verb, principal) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load");
    assert_eq!(
        principal,
        Principal::Person {
            person_id: o.bea.clone()
        }
    );
    let at_the_frame = records(&o).await;
    assert_eq!(at_the_frame.len(), 1, "{at_the_frame:#?}");
    let first = at_the_frame[0].clone();
    assert_eq!(first.person_id.as_deref(), Some(o.bea.as_str()));
    assert_eq!(
        (first.method.as_str(), first.action.as_str()),
        ("session", "load")
    );
    assert!(first.answers.is_none() && first.refusal.is_none());
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded", "run": "r-1"}), 0)),
            error: None,
        })
        .await;
    let (status, _, page) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    crate::surfaces::assert_framed(&page);
    assert!(page.contains("admin&#39;s answer"), "{page}");
    assert!(
        page.contains("loaded") && page.contains(&first.audit_id),
        "{page}"
    );
    let answered = wait_records(&o, "the ok outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));
}

/// **The answer page shows admin's own words** (Spec 2.13): a refused
/// `load` whose stderr carries markup and runs past the bound shows that
/// stderr escaped, cut at a character boundary within 2048 bytes and saying
/// so, beside the refusal object, never read for meaning; and a blank stderr
/// is not shown.
#[tokio::test]
async fn the_answer_page_shows_admins_own_words() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    // The words fill 2047 bytes; a two-byte character starts at byte 2047,
    // so the cut falls before it, and what follows it is never shown.
    let head = "weaver-admin: restore names <sp-447> & \"sp-448\"; {\"kind\":\"loaded\"} ";
    let mut stderr = head.to_owned();
    stderr.push_str(&"a".repeat(2047 - head.len()));
    assert_eq!(stderr.len(), 2047);
    stderr.push('\u{e9}');
    stderr.push_str(" TAIL-PAST-THE-BOUND");
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    let mut outcome = answer(
        "load",
        json!({"kind": "config_invalid", "field": "restore"}),
        1,
    );
    outcome.stderr = Some(stderr.clone());
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome),
            error: None,
        })
        .await;
    let (status, _, page) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    crate::surfaces::assert_framed(&page);
    assert!(page.contains("admin&#39;s refusal"), "{page}");
    assert!(
        page.contains("config_invalid"),
        "the refusal object: {page}"
    );
    // askama escapes by numeric reference.
    let shown = "weaver-admin: restore names &#60;sp-447&#62; &#38; &#34;sp-448&#34;; \
                 {&#34;kind&#34;:&#34;loaded&#34;} ";
    assert!(page.contains(shown), "the words, escaped: {page}");
    assert!(!page.contains("<sp-447>"), "never unescaped: {page}");
    assert!(
        page.contains(&format!("{}</pre>", "a".repeat(64))),
        "the words run to the cut: {page}"
    );
    assert!(
        !page.contains('\u{e9}') && !page.contains("TAIL-PAST-THE-BOUND"),
        "nothing past the bound: {page}"
    );
    // The template's own words are its text, not a value, so unescaped.
    assert!(
        page.contains("admin's stderr, its first 2048 bytes"),
        "the cut is said: {page}"
    );

    // A refusal whose stderr is blank shows no stderr.
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    let mut outcome = answer("load", json!({"kind": "agent_running"}), 1);
    outcome.stderr = Some("  \n".into());
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome),
            error: None,
        })
        .await;
    let (status, _, page) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("agent_running"), "{page}");
    assert!(
        !page.contains("admin's stderr"),
        "blank stderr is not shown: {page}"
    );
}

/// **A grant without the verb is refused before an ask**: an observer's
/// `stop` answers 403 with one refusal record, and no frame leaves.
///
/// Perturbations: drop the grant check, and the observer's `stop` leaves
/// the server; drop the refusal's record, and the refused verb leaves no
/// trace.
#[tokio::test]
async fn an_observers_stop_is_refused_with_one_record() {
    let Some(mut o) = operated(&["show", "load", "stop"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, body) = refused(&o.rig, &cara_session, &o.karl, "stop").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    one_refusal(&records(&o).await, &cara, "stop");
    no_frame(&mut o.admin_con).await;
}

/// **No grant on the agent answers exactly as an unknown agent**: the same
/// status and body, so the page is no oracle; the refusal is recorded, and
/// an agent the register does not hold records nothing.
///
/// Perturbation: answer no grant with 403, and the page tells a stranger
/// the agent exists.
#[tokio::test]
async fn no_grant_is_answered_as_an_unknown_agent() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (dan, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    let unknown = refused(&o.rig, &dan_session, UNKNOWN, "load").await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
    assert!(
        records(&o).await.is_empty(),
        "an unknown agent records nothing"
    );
    let stranger = refused(&o.rig, &dan_session, &o.karl, "load").await;
    assert_eq!(stranger, unknown, "identical to an unknown agent's answer");
    one_refusal(&records(&o).await, &dan, "load");
    no_frame(&mut o.admin_con).await;
}

/// **The server-wide admin grant authorizes nothing on an agent**: ada,
/// who registered karl and holds no grant on it, asking `show` is answered
/// as an unknown agent, with a refusal record and no frame.
///
/// Perturbation: let the admin grant carry every verb on every agent, and
/// ada's `show` leaves the server.
#[tokio::test]
async fn the_admin_grant_authorizes_no_verb() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (status, _) = refused(&o.rig, &o.ada_session, &o.karl, "show").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let ada = o.ada.clone();
    one_refusal(&records(&o).await, &ada, "show");
    no_frame(&mut o.admin_con).await;
}

/// **The session is checked again under the hold**: bea is disabled after
/// the gate read her session and before the ask's check, and the ask is
/// refused under the hold with its record, no frame leaving.
///
/// Perturbation: drop the session's re-check under the hold, and the
/// disabled person's ask leaves.
#[tokio::test]
async fn a_person_disabled_before_the_check_is_refused_under_the_hold() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::BeforeHold);
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    reached.notified().await;
    // The disable a page or the host would write, landed while the ask
    // holds nothing.
    sqlx::query("UPDATE person SET enabled = false, version = version + 1 WHERE person_id = $1")
        .bind(&o.bea)
        .execute(&o.rig.store().pool)
        .await
        .unwrap();
    release.notify_one();
    let (status, _, _) = tokio::time::timeout(REFUSED_WITHIN, asked)
        .await
        .expect("refused at once")
        .unwrap();
    assert_eq!(status, StatusCode::NOT_FOUND);
    let bea = o.bea.clone();
    one_refusal(&records(&o).await, &bea, "load");
    no_frame(&mut o.admin_con).await;
}

/// **The connection's ceiling, and its presence, are checked under the
/// hold**: a verb outside the ceiling is 403 naming it, with a refusal
/// record and no frame; with admin-con gone, a verb is 503 with its record.
#[tokio::test]
async fn the_ceiling_and_the_connection_refuse_with_their_records() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (status, body) = refused(&o.rig, &o.bea_session, &o.karl, "stop").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("load, show"), "{body}");
    let bea = o.bea.clone();
    one_refusal(&records(&o).await, &bea, "stop");
    no_frame(&mut o.admin_con).await;

    let Operated {
        rig,
        karl,
        bea_session,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    let id: AgentId = karl.parse().unwrap();
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    while rig
        .listener
        .connected(&id, crate::link::frames::Plane::Admin)
    {
        assert!(tokio::time::Instant::now() < until, "admin-con never left");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let (status, body) = refused(&rig, &bea_session, &karl, "load").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let after = records_on(&rig, &karl).await;
    assert_eq!(after.len(), 2, "{after:#?}");
    assert_eq!(after[1].action, "load");
    assert!(after[1].refusal.is_some());
}

/// **A malformed verb or identity is refused before anything**: 400, and
/// no record.
#[tokio::test]
async fn a_malformed_ask_is_400_with_no_record() {
    let Some(o) = operated(&["show", "load"]).await else {
        return;
    };
    let (status, _) = refused(&o.rig, &o.bea_session, &o.karl, "dance").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = refused(&o.rig, &o.bea_session, &o.karl, "turn").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a turn is not a verb");
    let (status, _) = refused(&o.rig, &o.bea_session, "box-a%2Fkarl", "load").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(records(&o).await.is_empty());
}

/// **An answer past the bound is 202 naming the first**, and the outcome
/// lands when admin answers, the page's wait having bounded nothing.
#[tokio::test]
async fn an_answer_past_the_bound_is_202_and_lands_later() {
    let Some(mut o) = operated_with(&["show", "load"], Duration::from_secs(4)).await else {
        return;
    };
    let mut asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    let (status, _, page) = loop {
        tokio::select! {
            done = &mut asked => break done.unwrap(),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                o.admin_con.send(FromClient::Heartbeat).await;
            }
        }
    };
    assert_eq!(status, StatusCode::ACCEPTED, "{page}");
    let first = records(&o).await[0].clone();
    assert!(page.contains(&first.audit_id), "{page}");
    assert_eq!(outcome_of(&records(&o).await, &first), None);
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    let answered = wait_records(&o, "the late outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));
}

/// **The hold runs from the check to the enqueue**: the ask is stopped
/// under its hold, its first durable and its frame not yet sent; a
/// revocation of the grant does not commit until the frame is in the
/// channel; and the next ask is refused.
///
/// Perturbation: release the hold before the enqueue, and the revocation
/// commits while the frame of the ask it revoked is still to be sent.
#[tokio::test]
async fn a_revocation_waits_for_the_enqueue_of_an_ask_under_the_hold() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::UnderHold);
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    reached.notified().await;
    let (s, bea, karl) = (o.rig.store().clone(), o.bea.clone(), o.karl.clone());
    let revoke =
        tokio::spawn(
            async move { host::grant_remove(&s, &bea, "operator", Some(&karl), None).await },
        );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !revoke.is_finished(),
        "the revocation committed under the ask's hold"
    );
    release.notify_one();
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load");
    let revoked = tokio::time::timeout(Duration::from_secs(5), revoke)
        .await
        .expect("the revocation commits once the frame is in the channel")
        .unwrap();
    assert!(revoked.ok, "{}", revoked.value);
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    assert_eq!(asked.await.unwrap().0, StatusCode::OK);
    let (status, _) = refused(&o.rig, &o.bea_session, &o.karl, "load").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the revoked grant asks nothing"
    );
}

/// **No wait on a peer runs under the hold**: admin-con stops reading and
/// its write channel fills; an ask then waits for its slot before taking
/// the hold, and a grant's removal meanwhile commits at once.
///
/// Perturbation: reserve the slot under the hold, and the removal waits on
/// the stalled connector.
#[tokio::test]
async fn an_ask_waiting_on_a_full_channel_holds_nothing() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    // Acks that outrun every buffer between the server's queue and this
    // connector, which never reads them, so the channel stays full.
    let generation = "g".repeat(4096);
    let events = 5_000u64;
    for n in 1..=events {
        o.admin_con
            .send(FromClient::Event {
                position: Position {
                    generation: generation.clone(),
                    offset: n,
                    digest: format!("d{n}"),
                },
                replayed: false,
                event: TraceEvent {
                    seq: n,
                    mark: None,
                    run: Some("run-1".into()),
                    turn: None,
                    kind: Some("turn".into()),
                    raw: json!({"kind": "turn", "run": "run-1", "payload": {}}),
                    replayed: false,
                },
            })
            .await;
    }
    let id: AgentId = o.karl.parse().unwrap();
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while o.rig.listener.acknowledged(&id).map(|p| p.offset) != Some(events) {
        assert!(tokio::time::Instant::now() < until, "never acknowledged");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _waiting = ask(&o.rig, &o.bea_session, &o.karl, "load");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let s = o.rig.store().clone();
    let removed = tokio::time::timeout(
        Duration::from_secs(5),
        host::grant_remove(&s, &o.bea, "operator", Some(&o.karl), None),
    )
    .await
    .expect("the removal commits while the ask waits on its slot");
    assert!(removed.ok, "{}", removed.value);
}

/// **A dropped request does not cut its ask**: dropped while the ask is
/// stopped under its hold, the frame still reaches admin-con with its first
/// and the answer writes its outcome; dropped while a refusal's record is
/// about to be written, the refusal is still recorded.
///
/// Perturbation: run the ask in the request's own future. The dropped
/// request then leaves an authorized ask's first answered `failed` and its
/// frame never sent, and a refusal with no record.
#[tokio::test]
async fn a_dropped_request_leaves_its_ask_to_finish() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::UnderHold);
    let asked = ask(&o.rig, &o.bea_session, &o.karl, "load");
    reached.notified().await;
    asked.abort();
    let _ = asked.await;
    release.notify_one();
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load");
    let first = records(&o).await[0].clone();
    assert!(first.refusal.is_none() && first.answers.is_none());
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    let answered = wait_records(&o, "the dropped ask's outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));

    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::AtRefusal);
    let asked = ask(&o.rig, &cara_session, &o.karl, "load");
    reached.notified().await;
    asked.abort();
    let _ = asked.await;
    release.notify_one();
    let after = wait_records(&o, "the dropped refusal's record", |r| r.len() == 3).await;
    assert_eq!(after[2].person_id.as_deref(), Some(cara.as_str()));
    assert!(after[2].refusal.is_some());
}

/// **No frame without its first**: the audit refuses a person's first
/// record, and the ask answers the server's failure with no frame and no
/// record.
///
/// Perturbation: send the frame before the record step. It reaches
/// admin-con though the audit holds no record of it.
#[tokio::test]
async fn a_first_record_the_audit_refuses_sends_no_frame() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    FAIL_FIRST_RECORD.with(|f| f.set(true));
    let (status, _) = refused(&o.rig, &o.bea_session, &o.karl, "load").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(records(&o).await.is_empty());
    no_frame(&mut o.admin_con).await;
}

/// bea's open session, as the listener's asker.
pub(crate) async fn bea_asker(o: &Operated) -> Asker {
    let session_id: i64 = sqlx::query_scalar(
        "SELECT session_id FROM session WHERE person_id = $1 AND closed_at IS NULL \
         ORDER BY session_id DESC LIMIT 1",
    )
    .bind(&o.bea)
    .fetch_one(&o.rig.store().pool)
    .await
    .unwrap();
    Asker {
        session_id,
        person_id: o.bea.clone(),
    }
}

/// **Persons' asks never fill the connection's channel**: three asks stop
/// under their holds and take every place, and seventy more park behind
/// them, each having tried for a slot first. The connection's own frames
/// still go out: a close of the trace door has its `show` asked, an
/// opening its own `show` and its `landed` reported, and the connection
/// stands. The asks name three verbs,
/// since a person's second identical verb in flight is held at its check
/// and would never stop under its hold.
///
/// Perturbation: drop the per-connection cap. The parked asks' slots take
/// the channel, the opening's `show` waits on it, and the healthy
/// connection would close as a silent peer.
#[tokio::test]
async fn persons_asks_never_fill_the_connections_channel() {
    let Some(mut o) = operated(&["show", "load", "stop", "validate"]).await else {
        return;
    };
    let asker = bea_asker(&o).await;
    let id: AgentId = o.karl.parse().unwrap();
    let pauses: Vec<_> = (0..3)
        .map(|_| o.rig.listener.pause_ask_at(AskPoint::UnderHold))
        .collect();
    // As many asks as the server admits outstanding, each admitted under
    // an identity of its own so no person's share stops them; the
    // admission is a place, and the ask is bea's.
    for n in 0..73 {
        let Some(admission) = o.rig.listener.admit_ask(&format!("pe-{n:016x}")) else {
            continue;
        };
        let (listener, asker, id) = (o.rig.listener.clone(), asker.clone(), id.clone());
        let verb = ["load", "stop", "validate"][n % 3];
        tokio::spawn(async move {
            listener
                .ask_as_person(admission, &asker, &id, Act::Verb(verb.into()))
                .await
        });
    }
    for (reached, _) in &pauses {
        reached.notified().await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let tail = Position {
        generation: "g".into(),
        offset: 0,
        digest: "d".into(),
    };
    o.admin_con
        .send(FromClient::Door {
            open: false,
            wall_ms: 1_790_000_000_000,
            tail: None,
        })
        .await;
    match o.admin_con.recv().await {
        Some(ToClient::Verb { verb, .. }) if verb == "show" => {}
        other => panic!("the close's show did not go out: {other:?}"),
    }
    o.admin_con
        .send(FromClient::Door {
            open: true,
            wall_ms: 1_790_000_000_001,
            tail: Some(tail),
        })
        .await;
    let show = match o.admin_con.recv().await {
        Some(ToClient::Verb { id, verb, .. }) if verb == "show" => id,
        other => panic!("the opening's show did not go out: {other:?}"),
    };
    o.admin_con
        .send(FromClient::Verb {
            id: show,
            outcome: Some(answer("show", json!({"kind": "state", "state": "idle"}), 0)),
            error: None,
        })
        .await;
    assert!(
        matches!(o.admin_con.recv().await, Some(ToClient::Landed { id }) if id == show),
        "the server's landed went out"
    );
    assert!(
        o.rig
            .listener
            .connected(&id, crate::link::frames::Plane::Admin)
    );
}

/// **Nothing from the place to the commit waits unboundedly on the
/// store**: three asks stall under their holds at the grants' read, each
/// answers the server's failure within the bound and frees its place, and
/// the next ask takes a place and is enqueued.
///
/// Perturbation: read the grants without the deadline. The stalled asks
/// never answer, and every place stays taken.
#[tokio::test]
async fn a_stalled_store_answers_within_the_bound_and_frees_its_place() {
    let Some(o) = operated_with(&["show", "load"], Duration::from_secs(4)).await else {
        return;
    };
    let asker = bea_asker(&o).await;
    let id: AgentId = o.karl.parse().unwrap();
    let Operated { rig, admin_con, .. } = o;
    // admin-con keeps its link alive while the asks wait.
    let (stop, mut stopped) = tokio::sync::oneshot::channel::<()>();
    let beating = tokio::spawn(async move {
        let mut admin_con = admin_con;
        loop {
            tokio::select! {
                _ = &mut stopped => return admin_con,
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    admin_con.send(FromClient::Heartbeat).await;
                }
            }
        }
    });
    let pauses: Vec<_> = (0..3)
        .map(|_| rig.listener.pause_ask_at(AskPoint::AtGrants))
        .collect();
    let stalled: Vec<_> = (0..3)
        .map(|_| {
            let (listener, asker, id) = (rig.listener.clone(), asker.clone(), id.clone());
            let admission = listener
                .admit_ask(&asker.person_id)
                .expect("within bea's share");
            tokio::spawn(async move {
                listener
                    .ask_as_person(admission, &asker, &id, Act::Verb("load".into()))
                    .await
            })
        })
        .collect();
    for (reached, _) in &pauses {
        reached.notified().await;
    }
    for ask in stalled {
        match tokio::time::timeout(Duration::from_secs(6), ask).await {
            Ok(Ok(Err(VerbError::Stalled))) => {}
            Ok(Ok(other)) => panic!("expected the bound's failure, got {:?}", other.err()),
            other => panic!(
                "the stalled ask did not answer within the bound: {:?}",
                other.is_ok()
            ),
        }
    }
    let next = tokio::time::timeout(
        Duration::from_secs(6),
        rig.listener.ask_as_person(
            rig.listener
                .admit_ask(&asker.person_id)
                .expect("the stalled asks' admissions were released"),
            &asker,
            &id,
            Act::Verb("load".into()),
        ),
    )
    .await
    .expect("a place was free");
    let enqueued = next.expect("the next ask is enqueued");
    let _ = stop.send(());
    let mut admin_con = beating.await.unwrap();
    let (_, verb, _) = take_verb(&mut admin_con).await;
    assert_eq!(verb, "load");
    drop(enqueued);
}

/// **Detached work is admitted against a bound before it is detached**:
/// with every held place taken, bea's asks past her share answer 503 at
/// once and record nothing, and the asks outstanding stay at her share
/// (her asks name the eight lifecycle verbs, since a person's second
/// identical verb in flight is held at its check);
/// cara still gets in within hers; past the server's bound, a request is
/// shed before its agent is resolved, a known agent and an unknown one
/// alike; and once released, the held asks finish normally.
///
/// Perturbation: admit every ask. The excess is detached, bea's asks are
/// not answered at once, and the asks outstanding pass her share.
#[tokio::test]
async fn asks_past_the_bound_are_shed_with_no_record() {
    const LIFECYCLE: [&str; 8] = [
        "show",
        "validate",
        "load",
        "unload",
        "stop",
        "save-point",
        "restore",
        "force-unload",
    ];
    let Some(mut o) = operated(&LIFECYCLE).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "operator", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (_, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;

    let pauses: Vec<_> = (0..3)
        .map(|_| o.rig.listener.pause_ask_at(AskPoint::UnderHold))
        .collect();
    let beas: Vec<_> = LIFECYCLE
        .iter()
        .map(|verb| ask(&o.rig, &o.bea_session, &o.karl, verb))
        .collect();
    for (reached, _) in &pauses {
        reached.notified().await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let (shed, admitted): (Vec<_>, Vec<_>) = beas.into_iter().partition(|a| a.is_finished());
    assert_eq!(shed.len(), LIFECYCLE.len() - 4, "bea's share is four");
    for answer in shed {
        let (status, _, body) = answer.await.unwrap();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    }
    assert_eq!(o.rig.listener.outstanding_asks(), 4);
    assert_eq!(
        records(&o).await.len(),
        3,
        "the held asks' firsts, and nothing shed"
    );

    let caras = ask(&o.rig, &cara_session, &o.karl, "load");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!caras.is_finished(), "cara is admitted within her share");
    assert_eq!(o.rig.listener.outstanding_asks(), 5);

    // The server's bound, filled by others: a request past it is shed
    // before its agent is resolved, so a known one and an unknown one
    // answer alike.
    let mut others = Vec::new();
    while let Some(admission) = o
        .rig
        .listener
        .admit_ask(&format!("pe-{:016x}", 0x1000 + others.len()))
    {
        others.push(admission);
    }
    assert_eq!(others.len() + 5, 64);
    let known = refused(&o.rig, &dan_session, &o.karl, "load").await;
    let unknown = refused(&o.rig, &dan_session, UNKNOWN, "load").await;
    assert_eq!(known.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(known, unknown, "shed before the agent is resolved");
    assert_eq!(records(&o).await.len(), 3, "a shed request records nothing");
    drop(others);

    for (_, release) in &pauses {
        release.notify_one();
    }
    for _ in 0..5 {
        let (id, verb, _) = take_verb(&mut o.admin_con).await;
        assert!(LIFECYCLE.contains(&verb.as_str()), "{verb}");
        o.admin_con
            .send(FromClient::Verb {
                id,
                outcome: Some(answer(&verb, json!({"kind": "agent_running"}), 1)),
                error: None,
            })
            .await;
    }
    for asked in admitted.into_iter().chain([caras]) {
        assert_eq!(asked.await.unwrap().0, StatusCode::OK);
    }
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    while o.rig.listener.outstanding_asks() > 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the admissions were released"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---- A person's turns (act 11, PR 6c): the same held ask on the gate plane.

/// A turn's text distinct enough that no other string in a record or a log
/// line carries it.
pub(crate) const SAID: &str = "zebra-kumquat-7731";

/// The gate's close of a turn, in the contract's shape.
pub(crate) fn close(
    kind: &str,
    text: Option<&str>,
    reason: Option<&str>,
    finish: Option<&str>,
) -> GateClose {
    GateClose {
        kind: kind.into(),
        run: Some("run-1".into()),
        turn: Some("t-1".into()),
        text: text.map(Into::into),
        finish: finish.map(Into::into),
        reason: reason.map(Into::into),
        raw: json!({ "kind": kind }),
    }
}

/// gate-con dialed with karl's minted gate config and answered.
pub(crate) async fn dial_gate(o: &Operated) -> Connector {
    let mut gate = Connector::dial_with(&o.gate_config, &[]).await;
    assert!(matches!(
        gate.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));
    gate
}

/// A turn placed from the page, its request running as its own task.
pub(crate) fn place(
    rig: &Rig,
    session: &str,
    agent: &str,
    text: &str,
) -> tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)> {
    let (app, session, uri, body) = (
        rig.app.clone(),
        session.to_owned(),
        format!("/agents/{agent}/turn"),
        form(&[("text", text)]),
    );
    tokio::spawn(async move { send_as(&app, "POST", &uri, Some(body), &session).await })
}

/// A turn placed and its answer awaited, which a refusal gives at once.
async fn turn_refused(rig: &Rig, session: &str, agent: &str, text: &str) -> (StatusCode, String) {
    let (status, _, body) = tokio::time::timeout(REFUSED_WITHIN, place(rig, session, agent, text))
        .await
        .unwrap_or_else(|_| panic!("the turn was not answered at once: it was placed"))
        .unwrap();
    (status, body)
}

/// The next turn frame gate-con receives: its id and text.
pub(crate) async fn take_turn(gate: &mut Connector) -> (u64, String) {
    match gate.recv().await {
        Some(ToClient::Turn { id, text }) => (id, text),
        other => panic!("expected a turn frame, got {other:?}"),
    }
}

/// No turn frame reaches gate-con within the receive bound.
async fn no_turn(gate: &mut Connector) {
    while let Some(frame) = gate.recv().await {
        assert!(
            !matches!(frame, ToClient::Turn { .. }),
            "a turn frame left the server"
        );
    }
}

/// **A person places a turn, and its record holds no part of its text**:
/// one first record (the person, by session, on the agent, `turn`) whose
/// whole row names nothing of the text, the frame carrying the text to
/// gate-con, an `answered` close writing `ok` and shown on the page, and a
/// close cut at the token limit shown as truncated.
///
/// Perturbation: put the text in the first record, and the row carries it.
#[tokio::test]
async fn a_person_places_a_turn_and_its_record_holds_no_text() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let placed = place(&o.rig, &o.bea_session, &o.karl, SAID);
    let (id, text) = take_turn(&mut gate).await;
    assert_eq!(text, SAID, "the frame carries the text");
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT row_to_json(a)::text FROM audit a WHERE target_id = $1 AND person_id = $2",
    )
    .bind(&o.karl)
    .bind(&o.bea)
    .fetch_all(&o.rig.store().pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1, "{rows:#?}");
    // The text whole and its words; its digits alone would meet a
    // timestamp or an identity's hex.
    for part in [SAID, "zebra", "kumquat"] {
        assert!(
            !rows[0].contains(part),
            "the record holds the text: {}",
            rows[0]
        );
    }
    let first = records(&o).await[0].clone();
    assert_eq!(first.person_id.as_deref(), Some(o.bea.as_str()));
    assert_eq!(
        (first.method.as_str(), first.action.as_str()),
        ("session", "turn")
    );
    gate.send(FromClient::Turn {
        id,
        close: Some(close("answered", Some("the tide turns at six"), None, None)),
        error: None,
    })
    .await;
    let (status, _, page) = placed.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains("the agent&#39;s answer") && page.contains("the tide turns at six"),
        "{page}"
    );
    assert!(
        !page.contains(SAID),
        "the page shows the answer and not the text"
    );
    let answered = wait_records(&o, "the ok outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));

    let placed = place(&o.rig, &o.bea_session, &o.karl, "a long one");
    let (id, _) = take_turn(&mut gate).await;
    gate.send(FromClient::Turn {
        id,
        close: Some(close("answered", Some("and then"), None, Some("length"))),
        error: None,
    })
    .await;
    let (_, _, page) = placed.await.unwrap();
    assert!(page.contains("truncated at the turn"), "{page}");
}

/// A case of a turn's answer: its name, the close or the fault, and the
/// outcome owed.
type Evidence = (
    &'static str,
    Option<GateClose>,
    Option<TurnFault>,
    Option<&'static str>,
);

/// **Only the gate contract's evidence writes a turn's outcome**: `stopped`
/// and `refused` closes and the faults of a turn that never reached the
/// gate (`busy`, `not_started`) write `failed`; `delivery_lost`, a close of
/// a kind on no list, and a link that ends with the turn in flight write
/// none.
///
/// Perturbations: write `ok` for any close, and the stopped turn reads
/// `ok`; write `failed` for `delivery_lost`, and an unknown outcome reads
/// `failed`.
#[tokio::test]
async fn each_close_and_fault_writes_only_its_evidence() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let asker = bea_asker(&o).await;
    let id: AgentId = o.karl.parse().unwrap();
    let fault = |kind: &str| TurnFault {
        kind: kind.into(),
        message: "a fault".into(),
    };
    let cases: Vec<Evidence> = vec![
        (
            "stopped",
            Some(close("stopped", None, Some("a hole"), None)),
            None,
            Some("failed"),
        ),
        (
            "refused",
            Some(close("refused", None, Some("not now"), None)),
            None,
            Some("failed"),
        ),
        ("busy", None, Some(fault("busy")), Some("failed")),
        (
            "not_started",
            None,
            Some(fault("not_started")),
            Some("failed"),
        ),
        ("delivery_lost", None, Some(fault("delivery_lost")), None),
        (
            "a kind on no list",
            Some(close("dreamt", Some("?"), None, None)),
            None,
            None,
        ),
    ];
    for (case, closed, faulted, owed) in cases {
        let (listener, asker, agent) = (o.rig.listener.clone(), asker.clone(), id.clone());
        let placed =
            tokio::spawn(async move { listener.turn_for_person(&asker, &agent, "go").await });
        let (turn, _) = take_turn(&mut gate).await;
        gate.send(FromClient::Turn {
            id: turn,
            close: closed,
            error: faulted,
        })
        .await;
        let _ = placed.await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        let records = records(&o).await;
        let first = records
            .iter()
            .rev()
            .find(|r| r.answers.is_none() && r.refusal.is_none())
            .unwrap()
            .clone();
        assert_eq!(outcome_of(&records, &first), owed, "{case}: {records:#?}");
    }
    // A link that ends with the turn in flight.
    let (listener, agent) = (o.rig.listener.clone(), id.clone());
    let placed = tokio::spawn(async move { listener.turn_for_person(&asker, &agent, "go").await });
    let _ = take_turn(&mut gate).await;
    drop(gate);
    assert!(matches!(placed.await.unwrap(), Err(TurnError::Unanswered)));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let records = records(&o).await;
    let last = records.last().unwrap();
    assert!(
        last.answers.is_none() && last.refusal.is_none(),
        "{records:#?}"
    );
}

/// **A turn's answer outside the contract's shape is refused at the
/// door**: one carrying both a close and a fault, or answering no ask, is
/// malformed, the connection torn down and the first given no outcome.
///
/// Perturbation: drop the door's shape check, and a both-members answer
/// writes an outcome.
#[tokio::test]
async fn a_turn_answer_outside_the_shape_is_refused_at_the_door() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let asker = bea_asker(&o).await;
    let id: AgentId = o.karl.parse().unwrap();
    for case in ["both", "no ask"] {
        let mut gate = dial_gate(&o).await;
        let (listener, asker, agent) = (o.rig.listener.clone(), asker.clone(), id.clone());
        let placed =
            tokio::spawn(async move { listener.turn_for_person(&asker, &agent, "go").await });
        let (turn, _) = take_turn(&mut gate).await;
        let frame = match case {
            "both" => FromClient::Turn {
                id: turn,
                close: Some(close("answered", Some("yes"), None, None)),
                error: Some(TurnFault {
                    kind: "busy".into(),
                    message: "busy".into(),
                }),
            },
            _ => FromClient::Turn {
                id: turn + 1000,
                close: Some(close("answered", Some("yes"), None, None)),
                error: None,
            },
        };
        gate.send(frame).await;
        assert!(
            matches!(
                gate.recv().await,
                Some(ToClient::Refusal {
                    reason: Refusal::Malformed
                })
            ),
            "{case}: refused as malformed"
        );
        assert!(
            placed.await.unwrap().is_err(),
            "{case}: the asker reads no answer"
        );
        let until = tokio::time::Instant::now() + Duration::from_secs(5);
        while o
            .rig
            .listener
            .connected(&id, crate::link::frames::Plane::Gate)
        {
            assert!(tokio::time::Instant::now() < until, "{case}: torn down");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        let records = records(&o).await;
        let first = records.last().unwrap();
        assert!(first.answers.is_none() && first.refusal.is_none());
        assert_eq!(outcome_of(&records, first), None, "{case}: {records:#?}");
    }
}

/// **A turn takes a grant carrying `turn`, checked as a verb's is**: an
/// observer is refused 403 with one record; no grant answers as an unknown
/// agent, with one record; the server-wide admin grant alone is no grant;
/// and no frame leaves for any of them.
///
/// Perturbation: drop the grant check, and the observer's turn is placed.
#[tokio::test]
async fn a_turn_without_its_grant_is_refused_with_one_record() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (dan, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;

    let (status, _) = turn_refused(&o.rig, &cara_session, &o.karl, SAID).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let unknown = turn_refused(&o.rig, &dan_session, UNKNOWN, SAID).await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
    let stranger = turn_refused(&o.rig, &dan_session, &o.karl, SAID).await;
    assert_eq!(stranger, unknown, "identical to an unknown agent's answer");
    let (status, _) = turn_refused(&o.rig, &o.ada_session, &o.karl, SAID).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "the admin grant is no grant");
    let records = records(&o).await;
    assert_eq!(records.len(), 3, "{records:#?}");
    let who: Vec<_> = records
        .iter()
        .map(|r| r.person_id.clone().unwrap())
        .collect();
    assert_eq!(who, [cara, dan, o.ada.clone()]);
    assert!(
        records
            .iter()
            .all(|r| r.refusal.is_some() && r.action == "turn")
    );
    no_turn(&mut gate).await;
}

/// **A turn's text is parsed first**: empty, or past the gate's line bound,
/// is 400 with no record and nothing at gate-con; and with no gate-con
/// connected a turn is 503 with one record.
#[tokio::test]
async fn a_malformed_or_unconnected_turn_is_refused() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let (status, _) = turn_refused(&o.rig, &o.bea_session, &o.karl, "   ").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let long = "x".repeat(crate::adapters::gate::LINE_BOUND);
    let (status, body) = turn_refused(&o.rig, &o.bea_session, &o.karl, &long).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.contains("xxx"), "the text is not answered back");
    assert!(records(&o).await.is_empty());
    let (status, body) = turn_refused(&o.rig, &o.bea_session, &o.karl, SAID).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    let records = records(&o).await;
    assert_eq!(records.len(), 1);
    assert!(records[0].refusal.is_some());
    let mut gate = dial_gate(&o).await;
    no_turn(&mut gate).await;
}

/// **Where a turn is placed, the page says the conversation is shared**:
/// shown where more than one enabled person holds `turn` on the agent, not
/// with one, and not once the second is disabled.
#[tokio::test]
async fn the_turn_form_says_when_the_conversation_is_shared() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let _gate = dial_gate(&o).await;
    let note = "your turns interleave";
    let page = || async {
        let (status, _, page) = send_as(
            &o.rig.app,
            "GET",
            &format!("/agents/{}", o.karl),
            None,
            &o.bea_session,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(page.contains("/turn\""), "the turn form: {page}");
        page
    };
    assert!(!page().await.contains(note), "one holder: no note");
    let s = o.rig.store().clone();
    let (cara, _) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "operator", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    assert!(page().await.contains(note), "two holders: the note");
    sqlx::query("UPDATE person SET enabled = false, version = version + 1 WHERE person_id = $1")
        .bind(&cara)
        .execute(&s.pool)
        .await
        .unwrap();
    assert!(!page().await.contains(note), "the second disabled: no note");
}

/// **One capture for every thread of the test binary**, installed once as
/// the process's default: a capture scoped to this test's thread misses
/// lines when other tests run beside it (measured 2026-10-14: the stall's
/// own line was absent in every parallel run and present alone), while the
/// process's default receives every line no scoped capture takes. The
/// text checked is this test's alone, so every other test's lines in it
/// only widen the check.
pub(crate) fn global_capture() -> &'static Captured {
    static CAPTURE: std::sync::OnceLock<Captured> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let captured = Captured::default();
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(captured.clone())
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .finish(),
        );
        captured
    })
}

/// **A turn's text appears in no log line**: across a placed turn, a
/// refused one and a stalled one, the process's capture at every level
/// holds nothing of it, and does hold the stall's own line for this agent.
///
/// Perturbation: log the text at debug in the held section, and the
/// capture finds it.
#[tokio::test]
async fn a_turns_text_is_in_no_log_line() {
    let captured = global_capture();
    let Some(o) = operated_with(&["show"], Duration::from_secs(4)).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let placed = place(&o.rig, &o.bea_session, &o.karl, SAID);
    let (id, _) = take_turn(&mut gate).await;
    gate.send(FromClient::Turn {
        id,
        close: Some(close("answered", Some("noted"), None, None)),
        error: None,
    })
    .await;
    assert_eq!(placed.await.unwrap().0, StatusCode::OK);

    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    assert_eq!(
        turn_refused(&o.rig, &cara_session, &o.karl, SAID).await.0,
        StatusCode::FORBIDDEN
    );

    let (reached, _release) = o.rig.listener.pause_ask_at(AskPoint::AtGrants);
    let stalled = place(&o.rig, &o.bea_session, &o.karl, SAID);
    reached.notified().await;
    let (status, _, _) = tokio::time::timeout(Duration::from_secs(8), stalled)
        .await
        .expect("the stall answers within its bound")
        .unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    let output = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(
        output.contains(&format!("{}: a person's turn was not asked", o.karl)),
        "the capture holds this agent's stall line"
    );
    // The text whole and its words; its digits alone would meet a
    // timestamp or a port in the process's lines.
    for part in [SAID, "zebra", "kumquat"] {
        assert!(!output.contains(part), "a log line holds the text: {part}");
    }
}

/// **A turn's hold runs from its check to its enqueue**, the same held ask
/// as a verb's: a revocation of the grant waits until the turn's frame is
/// in the channel, and the next turn is refused.
///
/// Perturbation: release the hold before the enqueue, and the revocation
/// commits while the turn is still to be sent.
#[tokio::test]
async fn a_turns_hold_runs_to_its_enqueue() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::UnderHold);
    let placed = place(&o.rig, &o.bea_session, &o.karl, SAID);
    reached.notified().await;
    let (s, bea, karl) = (o.rig.store().clone(), o.bea.clone(), o.karl.clone());
    let revoke =
        tokio::spawn(
            async move { host::grant_remove(&s, &bea, "operator", Some(&karl), None).await },
        );
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !revoke.is_finished(),
        "the revocation committed under the turn's hold"
    );
    release.notify_one();
    let (id, _) = take_turn(&mut gate).await;
    let revoked = tokio::time::timeout(Duration::from_secs(5), revoke)
        .await
        .expect("the revocation commits once the frame is in the channel")
        .unwrap();
    assert!(revoked.ok, "{}", revoked.value);
    gate.send(FromClient::Turn {
        id,
        close: Some(close("answered", Some("yes"), None, None)),
        error: None,
    })
    .await;
    assert_eq!(placed.await.unwrap().0, StatusCode::OK);
    let (status, _) = turn_refused(&o.rig, &o.bea_session, &o.karl, SAID).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// **A turn is admitted, and its slot capped, as a verb's**: with bea's
/// three turns stopped under their holds and a fourth of hers parked, her
/// fifth is shed 503 with no record; and with many more parked under
/// identities of their own, the gate connection's persons' slots stay at
/// their cap.
#[tokio::test]
async fn turns_are_admitted_and_their_slots_capped() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let _gate = dial_gate(&o).await;
    let id: AgentId = o.karl.parse().unwrap();
    let pauses: Vec<_> = (0..3)
        .map(|_| o.rig.listener.pause_ask_at(AskPoint::UnderHold))
        .collect();
    let _held: Vec<_> = (0..4)
        .map(|_| place(&o.rig, &o.bea_session, &o.karl, "held"))
        .collect();
    for (reached, _) in &pauses {
        reached.notified().await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let before = records(&o).await.len();
    let (status, _) = turn_refused(&o.rig, &o.bea_session, &o.karl, SAID).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "bea's fifth is shed"
    );
    assert_eq!(
        records(&o).await.len(),
        before,
        "a shed turn records nothing"
    );

    let asker = bea_asker(&o).await;
    for n in 0..70 {
        let Some(admission) = o.rig.listener.admit_ask(&format!("pe-{n:016x}")) else {
            continue;
        };
        let (listener, asker, id) = (o.rig.listener.clone(), asker.clone(), id.clone());
        tokio::spawn(async move {
            listener
                .ask_as_person(admission, &asker, &id, Act::Turn(TurnText::new("parked")))
                .await
        });
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        o.rig
            .listener
            .slots_taken(&id, crate::link::frames::Plane::Gate),
        Some(crate::link::admin_con::VERB_QUEUE),
        "the gate connection's persons' slots at their cap"
    );
}

/// **A turn stalled on the store answers within the bound**, the same
/// deadline as a verb's: 503, and nothing placed.
#[tokio::test]
async fn a_stalled_turn_answers_within_the_bound() {
    let Some(o) = operated_with(&["show"], Duration::from_secs(4)).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let (reached, _release) = o.rig.listener.pause_ask_at(AskPoint::AtGrants);
    let stalled = place(&o.rig, &o.bea_session, &o.karl, SAID);
    reached.notified().await;
    let (status, _, _) = tokio::time::timeout(Duration::from_secs(8), stalled)
        .await
        .expect("answered within the bound")
        .unwrap();
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(records(&o).await.is_empty());
    no_turn(&mut gate).await;
}

// ---- The answer in place (Spec 2.13; brief 2026-11-13) ----

/// A request as the page's htmx sends it: under the session's cookie, with
/// `HX-Request: true`, a form's body or none.
pub(crate) async fn send_hx(
    app: &axum::Router,
    method: &str,
    uri: &str,
    form: Option<String>,
    session: &str,
) -> (StatusCode, axum::http::HeaderMap, String) {
    use axum::body::Body;
    use axum::http::{Request, header};
    use tower::ServiceExt;
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, crate::passkeys_tests::ORIGIN)
        .header(header::COOKIE, format!("__Host-weaver_session={session}"))
        .header("hx-request", "true");
    let body = match form {
        Some(form) => {
            request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(form)
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// A verb asked from the page through htmx, its request its own task.
pub(crate) fn ask_hx(
    rig: &Rig,
    session: &str,
    agent: &str,
    verb: &str,
) -> tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)> {
    let (app, session, uri, body) = (
        rig.app.clone(),
        session.to_owned(),
        format!("/agents/{agent}/verb"),
        form(&[("verb", verb)]),
    );
    tokio::spawn(async move { send_hx(&app, "POST", &uri, Some(body), &session).await })
}

/// A turn placed from the page through htmx, its request its own task.
fn place_hx(
    rig: &Rig,
    session: &str,
    agent: &str,
    text: &str,
) -> tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)> {
    let (app, session, uri, body) = (
        rig.app.clone(),
        session.to_owned(),
        format!("/agents/{agent}/turn"),
        form(&[("text", text)]),
    );
    tokio::spawn(async move { send_hx(&app, "POST", &uri, Some(body), &session).await })
}

/// An ask through htmx answered at once, which a refusal and a held ask are.
async fn at_once(
    asked: tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)>,
) -> (StatusCode, String) {
    let (status, _, body) = tokio::time::timeout(REFUSED_WITHIN, asked)
        .await
        .expect("answered at once, not asked")
        .unwrap();
    (status, body)
}

/// **An answer drawn in its pane is a fragment**, never a framed page: no
/// document, no masthead, no stylesheet link.
pub(crate) fn assert_fragment(body: &str) {
    for mark in [
        "<!doctype",
        "<html",
        r#"<nav class="masthead">"#,
        "instrument.css",
    ] {
        assert!(!body.contains(mark), "a fragment carries no {mark}: {body}");
    }
}

/// **An ask through htmx is answered in its pane with everything the page
/// shows** (Spec 2.13): admin's refusal object pretty-printed and escaped,
/// the exit status as a mark, "admin's refusal", admin's stderr cut at 2048
/// bytes and saying so, and the first record's identity; as a fragment.
///
/// Perturbations: answer the htmx ask with the framed page, and the pane
/// draws a page inside a page; drop the exit mark, the pretty-printing or
/// the cut's words from the fragment, and the pane loses what the page
/// shows.
#[tokio::test]
async fn an_htmx_ask_is_answered_in_its_pane_with_all_of_admins_answer() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let asked = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    let first = records(&o).await[0].clone();
    let mut outcome = answer(
        "load",
        json!({"kind": "config_invalid", "field": "<restore>"}),
        1,
    );
    outcome.stderr = Some(format!("<b>{}", "x".repeat(3000)));
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome),
            error: None,
        })
        .await;
    let (status, _, body) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_fragment(&body);
    assert!(body.contains("admin&#39;s refusal"), "{body}");
    assert!(
        body.contains(r#"<span class="status fail">exit 1</span>"#),
        "the exit mark: {body}"
    );
    assert!(
        body.contains("{\n  &#34;field&#34;: &#34;&#60;restore&#62;&#34;,"),
        "pretty-printed and escaped: {body}"
    );
    assert!(
        body.contains("admin's stderr, its first 2048 bytes"),
        "the cut is said: {body}"
    );
    assert!(
        body.contains("&#60;b&#62;xxx") && !body.contains("<b>x"),
        "the words escaped: {body}"
    );
    assert!(body.contains(&first.audit_id), "the first record: {body}");
}

/// **An `unload` stopped at its save point draws its decision in the pane**
/// (Spec 2.14), as the page states it.
///
/// Perturbation: leave the decision out of the fragment, and the pane
/// drops the prompt the page shows.
#[tokio::test]
async fn an_unload_stopped_at_its_save_point_draws_its_decision_in_the_pane() {
    let Some(mut o) = operated(&["show", "unload", "save-point", "force-unload"]).await else {
        return;
    };
    let asked = ask_hx(&o.rig, &o.bea_session, &o.karl, "unload");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer(
                "unload",
                json!({"kind": "save_point_not_taken", "missed": "finished"}),
                1,
            )),
            error: None,
        })
        .await;
    let (status, _, body) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_fragment(&body);
    assert!(
        body.contains("<dialog open>") && body.contains("missed: finished"),
        "{body}"
    );
}

/// **A turn placed through htmx is answered in its pane**: the close's text
/// escaped, marked truncated at `finish = length`, and its first record;
/// a stopped close's reason; never the turn's own text.
///
/// Perturbation: answer the htmx turn with the framed page, and the pane
/// draws a page inside a page.
#[tokio::test]
async fn a_turn_placed_through_htmx_is_answered_in_its_pane() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let placed = place_hx(&o.rig, &o.bea_session, &o.karl, SAID);
    let (id, _) = take_turn(&mut gate).await;
    let first = records(&o).await[0].clone();
    gate.send(FromClient::Turn {
        id,
        close: Some(close(
            "answered",
            Some("<i>and then</i>"),
            None,
            Some("length"),
        )),
        error: None,
    })
    .await;
    let (status, _, body) = placed.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_fragment(&body);
    assert!(body.contains("truncated at the turn"), "{body}");
    assert!(
        body.contains("&#60;i&#62;and then") && !body.contains("<i>"),
        "{body}"
    );
    assert!(body.contains(&first.audit_id), "{body}");
    assert!(!body.contains(SAID), "never the turn's text: {body}");

    let placed = place_hx(&o.rig, &o.bea_session, &o.karl, SAID);
    let (id, _) = take_turn(&mut gate).await;
    gate.send(FromClient::Turn {
        id,
        close: Some(close("stopped", None, Some("the gate is lowered"), None)),
        error: None,
    })
    .await;
    let (_, _, body) = placed.await.unwrap();
    assert_fragment(&body);
    assert!(
        body.contains("the agent stopped the turn") && body.contains("the gate is lowered"),
        "{body}"
    );
}

/// **A refusal of an htmx ask is drawn in its pane at its own status**
/// (Spec 2.8, 2.13): a grant without the verb 403, no grant 404 and the
/// same body as an unknown agent's, a turn with no gate-con 503 naming
/// gate-con, a turn past the gate's line bound 400; each a fragment saying
/// nothing was asked.
///
/// Perturbations: answer an htmx refusal with its bare text, and the pane
/// draws words with no frame; answer the unconnected turn in admin-con's
/// words, and the pane names the wrong connector.
#[tokio::test]
async fn a_refusal_of_an_htmx_ask_is_drawn_in_its_pane_at_its_status() {
    let Some(o) = operated(&["show", "load", "stop"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, body) = at_once(ask_hx(&o.rig, &cara_session, &o.karl, "stop")).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_fragment(&body);
    assert!(
        body.contains("not asked") && body.contains("do not carry stop"),
        "{body}"
    );

    let (_, dan_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "dan").await;
    let (status, no_grant) = at_once(ask_hx(&o.rig, &dan_session, &o.karl, "load")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{no_grant}");
    assert_fragment(&no_grant);
    assert!(no_grant.contains("not asked"), "{no_grant}");
    let (status, unknown) = at_once(ask_hx(&o.rig, &dan_session, UNKNOWN, "load")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        no_grant.replace(&o.karl, UNKNOWN),
        unknown,
        "no grant answers as an unknown agent"
    );

    let (status, body) = at_once(place_hx(&o.rig, &o.bea_session, &o.karl, SAID)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_fragment(&body);
    assert!(
        body.contains("gate-con") && !body.contains("admin-con"),
        "the unconnected turn names its own connector: {body}"
    );
    let long = "x".repeat(crate::adapters::gate::LINE_BOUND);
    let (status, body) = at_once(place_hx(&o.rig, &o.bea_session, &o.karl, &long)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_fragment(&body);
    assert!(
        body.contains("line bound") && !body.contains("xxx"),
        "{body}"
    );
}

/// **The console holds a second identical verb itself** (Spec 2.13, 2.14):
/// while bea's `load` is in flight, her second `load` on the agent is
/// answered at once, 409, naming when the first was asked and its first
/// record; no frame leaves, one refusal record `already_asked` naming the
/// first, and no alert. Once the first is answered, the hold is gone.
///
/// Perturbations: drop the hold, and the second load reaches admin-con;
/// keep the hold past the answer, and the third load is held; raise an
/// alert for it, and a person's second press is an alert; record no
/// refusal, and the held ask leaves no trace.
#[tokio::test]
async fn a_second_identical_verb_is_held_while_the_first_is_in_flight() {
    let Some(mut o) = operated(&["show", "load", "stop"]).await else {
        return;
    };
    let asked = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    let first = records(&o).await[0].clone();
    let (status, body) = at_once(ask_hx(&o.rig, &o.bea_session, &o.karl, "load")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_fragment(&body);
    assert!(
        body.contains("already asked") && body.contains(&first.audit_id) && body.contains(" UTC"),
        "when and as what the first was asked: {body}"
    );
    // The plain post is held the same way, at the same status.
    let (status, plain) = refused(&o.rig, &o.bea_session, &o.karl, "load").await;
    assert_eq!(status, StatusCode::CONFLICT, "{plain}");
    let held = records(&o).await;
    assert_eq!(held.len(), 3, "the first and two refusals: {held:#?}");
    for refusal in &held[1..] {
        assert_eq!(refusal.action, "load");
        assert_eq!(refusal.person_id.as_deref(), Some(o.bea.as_str()));
        let why = refusal.refusal.as_deref().unwrap_or_default();
        assert!(
            why.starts_with("already_asked") && why.contains(&first.audit_id),
            "{refusal:?}"
        );
    }
    no_frame(&mut o.admin_con).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let raised = crate::alerts_tests::alerts(&o.rig, &o.karl).await;
    assert!(raised.is_empty(), "a held ask raises nothing: {raised:#?}");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    assert_eq!(asked.await.unwrap().0, StatusCode::OK);
    let again = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load", "the hold ends with the first's answer");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "agent_running"}), 1)),
            error: None,
        })
        .await;
    assert_eq!(again.await.unwrap().0, StatusCode::OK);
}

/// **A second ask is held from the first's check, before its frame**: the
/// first stopped under its hold, its first record durable and its frame not
/// yet sent; the second is held naming that record, and one frame leaves.
///
/// Perturbation: claim the ask only at its enqueue, and two identical asks
/// under their holds both reach admin-con.
#[tokio::test]
async fn a_second_ask_is_held_from_the_firsts_check() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let (reached, release) = o.rig.listener.pause_ask_at(AskPoint::UnderHold);
    let asked = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    reached.notified().await;
    let first = records(&o).await[0].clone();
    let (status, body) = at_once(ask_hx(&o.rig, &o.bea_session, &o.karl, "load")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains(&first.audit_id), "{body}");
    release.notify_one();
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load");
    no_frame(&mut o.admin_con).await;
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    assert_eq!(asked.await.unwrap().0, StatusCode::OK);
}

/// **Not held: a different verb, or another person's same verb** (Spec
/// 2.13): bea's `stop` while her `load` is in flight, and cara's `load`
/// beside it, each reach admin-con.
///
/// Perturbations: hold by verb alone, and cara's load is held; hold by
/// person alone, and bea's stop is held.
#[tokio::test]
async fn a_different_verb_and_another_persons_same_verb_are_not_held() {
    let Some(mut o) = operated(&["show", "load", "stop"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (cara, cara_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(&s, &cara, "operator", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let mut asked = vec![ask_hx(&o.rig, &o.bea_session, &o.karl, "load")];
    let mut ids = vec![take_verb(&mut o.admin_con).await];
    asked.push(ask_hx(&o.rig, &o.bea_session, &o.karl, "stop"));
    ids.push(take_verb(&mut o.admin_con).await);
    asked.push(ask_hx(&o.rig, &cara_session, &o.karl, "load"));
    ids.push(take_verb(&mut o.admin_con).await);
    let verbs: Vec<_> = ids.iter().map(|(_, verb, _)| verb.as_str()).collect();
    assert_eq!(verbs, ["load", "stop", "load"]);
    for (id, verb, _) in &ids {
        o.admin_con
            .send(FromClient::Verb {
                id: *id,
                outcome: Some(answer(verb, json!({"kind": "agent_running"}), 1)),
                error: None,
            })
            .await;
    }
    for asked in asked {
        assert_eq!(asked.await.unwrap().0, StatusCode::OK);
    }
}

/// **The hold ends with its connection**: the connection ends with bea's
/// `load` in flight, admin-con connects again, and her next `load` is
/// asked.
///
/// Perturbation: keep the holds beyond the connection, and the load after
/// the reconnection is held.
#[tokio::test]
async fn the_hold_ends_with_its_connection() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let asked = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    let _ = take_verb(&mut o.admin_con).await;
    let Operated {
        rig,
        karl,
        admin_config,
        bea_session,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    let (_, _, body) = asked.await.unwrap();
    assert!(body.contains("unknown here"), "{body}");
    let mut admin_con = admitted_admin(&admin_config, &["show", "load"]).await;
    let again = ask_hx(&rig, &bea_session, &karl, "load");
    let (id, verb, _) = take_verb(&mut admin_con).await;
    assert_eq!(verb, "load", "the hold went with the connection");
    admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    assert_eq!(again.await.unwrap().0, StatusCode::OK);
}

/// **A claim ends with its ask**: bea's `load`, claimed at its check, ends
/// before its frame when the audit refuses its first record, and leaves no
/// hold, so her next `load` on the same connection is asked.
///
/// Perturbation: leave the claim on the connection when its ask ends
/// before the enqueue, and the next load is held for an ask that never
/// left.
#[tokio::test]
async fn a_claim_ends_with_an_ask_that_ends_before_its_frame() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    FAIL_FIRST_RECORD.with(|f| f.set(true));
    let (status, body) = at_once(ask_hx(&o.rig, &o.bea_session, &o.karl, "load")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    let again = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load", "the claim went with its ask");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    assert_eq!(again.await.unwrap().0, StatusCode::OK);
}

/// **Past the silence bound an htmx ask says so in its pane** (Spec 2.13):
/// 202, a fragment naming when it was asked, its first record, the bound,
/// that the connection still holds it, and where its outcome goes; a turn
/// the same in gate-con's words. The outcome still lands.
///
/// Perturbations: drop the time or the first record from the line, and
/// the pane cannot say what is outstanding.
#[tokio::test]
async fn past_the_bound_an_htmx_ask_says_so_in_its_pane() {
    let Some(mut o) = operated_with(&["show", "load"], Duration::from_secs(4)).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let mut asked = ask_hx(&o.rig, &o.bea_session, &o.karl, "load");
    let (id, _, _) = take_verb(&mut o.admin_con).await;
    let (status, _, body) = loop {
        tokio::select! {
            done = &mut asked => break done.unwrap(),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                o.admin_con.send(FromClient::Heartbeat).await;
                gate.send(FromClient::Heartbeat).await;
            }
        }
    };
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_fragment(&body);
    let first = records(&o).await[0].clone();
    for said in [
        first.audit_id.as_str(),
        " UTC",
        "not answered within 4 s",
        "admin-con still holds it",
        "written to the audit",
    ] {
        assert!(body.contains(said), "the line says {said}: {body}");
    }
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(answer("load", json!({"kind": "loaded"}), 0)),
            error: None,
        })
        .await;
    let answered = wait_records(&o, "the late outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));

    let mut placed = place_hx(&o.rig, &o.bea_session, &o.karl, SAID);
    let _ = take_turn(&mut gate).await;
    let (status, _, body) = loop {
        tokio::select! {
            done = &mut placed => break done.unwrap(),
            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                o.admin_con.send(FromClient::Heartbeat).await;
                gate.send(FromClient::Heartbeat).await;
            }
        }
    };
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    assert_fragment(&body);
    assert!(
        body.contains("gate-con still holds it") && body.contains(" UTC"),
        "{body}"
    );
    assert!(!body.contains(SAID), "never the turn's text: {body}");
}

/// **The agent page asks in place** (Spec 2.13, the Controls guideline):
/// each verb form and the turn form post through htmx and still post
/// without it; each verb is on a queue of its own, dropping only its own
/// second submit, and is answered in a slot of its own outside `#run`,
/// which the poll's fragment never carries; each verb form carries its id
/// and `hx-preserve`, so the poll's swap keeps a pending form; every
/// lifecycle verb has its slot, offered or not; the asking line names the
/// page's wait; the page-wide configuration swaps the refusals'
/// statuses; and no surface evaluates an attribute.
///
/// Perturbations: put a slot inside `#run`, and the poll redraws it; drop
/// the poll's `abort`, a form's id or `hx-preserve`, and the poll replaces
/// a pending form; put the verbs back on one queue, and a verb waits
/// behind another's request; drop a status from the configuration, and
/// its refusal draws nothing.
#[tokio::test]
async fn the_agent_page_asks_in_place() {
    let Some(o) = operated(&["show", "load", "stop"]).await else {
        return;
    };
    let _gate = dial_gate(&o).await;
    let karl = &o.karl;
    let (status, _, page) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{karl}"),
        None,
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains(&format!(
            r##"<div class="stack" id="run" hx-get="/agents/{karl}/run" hx-trigger="every 15s" hx-swap="innerHTML settle:0ms" hx-sync="this:abort">"##
        )),
        "the poll is abortable, dropped only behind its own re-fetch, and settles at once: {page}"
    );
    let verb_form = |verb: &str| {
        format!(
            r##"<form method="post" action="/agents/{karl}/verb" id="verb-{verb}" hx-preserve hx-post="/agents/{karl}/verb" hx-target="#answer-{verb}" hx-swap="innerHTML" hx-sync="this:drop" hx-disabled-elt="find button" hx-indicator="#asked-{verb}"><input type="hidden" name="verb" value="{verb}">"##
        )
    };
    for verb in ["show", "load", "stop"] {
        assert!(
            page.contains(&verb_form(verb)),
            "{verb} asks in place on its own queue, kept through the poll: {page}"
        );
    }
    assert!(
        !page.contains("closest #run") && !page.contains("#run form button"),
        "no verb waits behind another's request: {page}"
    );
    assert!(
        page.contains(&format!(
            r##"<form method="post" action="/agents/{karl}/turn" class="field" hx-post="/agents/{karl}/turn" hx-target="#turn-answer" hx-swap="innerHTML" hx-sync="this:drop" hx-disabled-elt="find button" hx-indicator="#turn-asked">"##
        )),
        "the turn asks in place: {page}"
    );
    // Every lifecycle verb's slot, offered or not: `unload` and
    // `force-unload` are not offered here.
    for verb in [
        "show",
        "validate",
        "load",
        "unload",
        "stop",
        "save-point",
        "restore",
        "force-unload",
    ] {
        for slot in [
            format!(r#"<div class="asked" id="asked-{verb}">"#),
            format!(r#"<div class="answer" id="answer-{verb}" aria-live="polite"></div>"#),
        ] {
            assert_eq!(page.matches(&slot).count(), 1, "{slot}: {page}");
        }
    }
    for slot in [
        r#"<div class="asked" id="turn-asked">"#,
        r#"<div class="answer" id="turn-answer" aria-live="polite"></div>"#,
    ] {
        assert!(page.contains(slot), "{slot}: {page}");
    }
    assert!(
        page.contains("within the page&#39;s wait of 60 s")
            || page.contains("within the page's wait of 60 s"),
        "the asking line names the wait: {page}"
    );
    for code in [
        r#"{"code":"40[01349]","swap":true}"#,
        r#"{"code":"503","swap":true}"#,
    ] {
        assert!(page.contains(code), "the page swaps {code}: {page}");
    }
    assert!(!page.contains("hx-on"), "no attribute is evaluated");
    let (status, _, run) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{karl}/run"),
        None,
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    for verb in ["show", "load", "stop"] {
        assert!(
            run.contains(&verb_form(verb)),
            "the poll's fragment asks in place too, its {verb} form kept by its id"
        );
    }
    assert!(
        !run.contains(r#"id="answer"#) && !run.contains(r#"id="asked"#),
        "the slots stand outside #run: {run}"
    );
    let css = include_str!("../assets/surfaces/instrument.css");
    assert!(
        css.contains(
            ".asked:not(.htmx-request) > .asking,\n.asked.htmx-request > .answer { display: none; }"
        ),
        "the asking line's one rule"
    );
}

/// **The polls draw their refusal in their pane** (Spec 2.8): a person
/// whose grant is gone gets 404 from `/run` and `/availability` as a
/// fragment saying so, the page-wide configuration swapping it in.
///
/// Perturbation: answer a poll's 404 with bare text, and the pane draws
/// words with no element.
#[tokio::test]
async fn the_polls_draw_their_refusal_in_their_pane() {
    let Some(o) = operated(&["show", "load"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let removed = host::grant_remove(&s, &o.bea, "operator", Some(&o.karl), None).await;
    assert!(removed.ok, "{}", removed.value);
    for poll in ["run", "availability"] {
        let (status, _, body) = send_hx(
            &o.rig.app,
            "GET",
            &format!("/agents/{}/{poll}", o.karl),
            None,
            &o.bea_session,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{poll}: {body}");
        assert_fragment(&body);
        assert!(
            body.starts_with(r#"<p class="note read" role="status">"#)
                && body.contains("no such agent"),
            "{poll}: {body}"
        );
    }
}
