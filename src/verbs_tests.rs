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

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::agents_tests::{Connector, Rig, rig_silenced};
use crate::host;
use crate::link::frames::{FromClient, Position, Principal, ToClient, VerbOutcome};
use crate::link::listener::{AskPoint, Asker, VerbError};
use crate::passkeys_tests::authenticator;
use crate::store::AgentId;
use crate::store::audit::FAIL_FIRST_RECORD;
use crate::traceview::TraceEvent;

const UNKNOWN: &str = "ag-0123456789abcdef";

/// The bound a refusal is answered within: far under the silence bound, so
/// an ask that leaves instead of being refused fails here and not later.
const REFUSED_WITHIN: Duration = Duration::from_secs(10);

/// An agent, `box-a/karl`, registered by ada, its admin-con connected and
/// admitted with the ceiling given; and bea, signed in and holding
/// `operator` on it.
struct Operated {
    rig: Rig,
    karl: String,
    ada: String,
    ada_session: String,
    bea: String,
    bea_session: String,
    admin_con: Connector,
}

async fn operated(ceiling: &[&str]) -> Option<Operated> {
    operated_with(ceiling, Duration::from_secs(60)).await
}

async fn operated_with(ceiling: &[&str], silence: Duration) -> Option<Operated> {
    let rig = rig_silenced(silence).await?;
    let s = rig.store().clone();
    let (ada, ada_session) = admin(&rig.app, &s, &mut authenticator(), "ada").await;
    let (karl, handles) = rig.register(&ada_session, "box-a", "karl").await;
    let (_, _, config) = rig.take(&handles[1], &ada_session).await;
    let mut admin_con = Connector::dial_with(&config, ceiling).await;
    assert!(matches!(
        admin_con.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));
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
    let (bea, bea_session) = signed_in(&rig.app, &s, &mut authenticator(), "bea").await;
    let granted = host::grant_add(&s, &bea, "operator", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    Some(Operated {
        rig,
        karl,
        ada,
        ada_session,
        bea,
        bea_session,
        admin_con,
    })
}

/// admin's answer to a verb on karl, in the invocation's shape.
fn answer(verb: &str, object: serde_json::Value, exit: i32) -> VerbOutcome {
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
fn ask(
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
async fn refused(rig: &Rig, session: &str, agent: &str, verb: &str) -> (StatusCode, String) {
    let (status, _, body) = tokio::time::timeout(REFUSED_WITHIN, ask(rig, session, agent, verb))
        .await
        .unwrap_or_else(|_| panic!("{verb} was not answered at once: it was asked"))
        .unwrap();
    (status, body)
}

/// The next verb frame admin-con receives.
async fn take_verb(c: &mut Connector) -> (u64, String, Principal) {
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
async fn no_frame(c: &mut Connector) {
    while let Some(frame) = c.recv().await {
        assert!(
            !matches!(frame, ToClient::Verb { .. }),
            "a verb frame left the server: {frame:?}"
        );
    }
}

/// One person-principal audit record.
#[derive(Debug, Clone)]
struct Record {
    audit_id: String,
    person_id: Option<String>,
    method: String,
    action: String,
    answers: Option<String>,
    outcome: Option<String>,
    refusal: Option<String>,
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
async fn records(o: &Operated) -> Vec<Record> {
    records_on(&o.rig, &o.karl).await
}

/// Every audit record a person wrote on the agent named, oldest first.
async fn records_on(rig: &Rig, karl: &str) -> Vec<Record> {
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT audit_id, person_id, method, action, answers, outcome, refusal FROM audit \
         WHERE principal = 'person' AND target_kind = 'agent' AND target_id = $1 \
         AND action IN ('show', 'validate', 'load', 'unload', 'stop', 'save-point', \
         'restore', 'force-unload') ORDER BY at, audit_id",
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
async fn wait_records(o: &Operated, what: &str, cond: impl Fn(&[Record]) -> bool) -> Vec<Record> {
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

fn outcome_of<'a>(records: &'a [Record], first: &Record) -> Option<&'a str> {
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
    assert!(page.contains("admin&#39;s answer"), "{page}");
    assert!(
        page.contains("loaded") && page.contains(&first.audit_id),
        "{page}"
    );
    let answered = wait_records(&o, "the ok outcome", |r| r.len() == 2).await;
    assert_eq!(outcome_of(&answered, &first), Some("ok"));
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
async fn bea_asker(o: &Operated) -> Asker {
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
/// still go out: an opening of the trace door has its `show` asked and its
/// `landed` reported, and the connection stands.
///
/// Perturbation: drop the per-connection cap. The parked asks' slots take
/// the channel, the opening's `show` waits on it, and the healthy
/// connection would close as a silent peer.
#[tokio::test]
async fn persons_asks_never_fill_the_connections_channel() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let asker = bea_asker(&o).await;
    let id: AgentId = o.karl.parse().unwrap();
    let pauses: Vec<_> = (0..3)
        .map(|_| o.rig.listener.pause_ask_at(AskPoint::UnderHold))
        .collect();
    for _ in 0..73 {
        let (listener, asker, id) = (o.rig.listener.clone(), asker.clone(), id.clone());
        tokio::spawn(async move { listener.ask_as_person(&asker, &id, "load").await });
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
            tokio::spawn(async move { listener.ask_as_person(&asker, &id, "load").await })
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
        rig.listener.ask_as_person(&asker, &id, "load"),
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
