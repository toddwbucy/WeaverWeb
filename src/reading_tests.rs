//! Reading an agent (`src/surfaces/reading.rs`, `src/store/access.rs`;
//! design sections 6 and 9; Spec 2.12 and 2.13): the list holds only the
//! agents a person holds a grant on; another agent's page and live view
//! answer exactly as an unknown agent's; the page shows the row's facts;
//! and the live view relays the listener's window and closes within its
//! re-check when the grant, the person or the session ends. Each test runs
//! on a store of its own with a real listener. No name is a real one.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tokio_stream::StreamExt;
use tower::ServiceExt;

use crate::admin_tests::{admin, send_as, signed_in};
use crate::agents_tests::{Rig, TEST_RECHECK, rig};
use crate::host;
use crate::passkeys_tests::{ORIGIN, authenticator};
use crate::traceview::TraceEvent;

const UNKNOWN: &str = "ag-0123456789abcdef";

/// A rig with an admin's two agents, `box-a/karl` and `box-b/jane`, and a
/// person, bea, signed in and holding the observer role on karl alone.
async fn granted() -> Option<(Rig, String, String, String, String, String)> {
    let rig = rig().await?;
    let s = rig.store();
    let (_, admin_session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (karl, _) = rig.register(&admin_session, "box-a", "karl").await;
    let (jane, _) = rig.register(&admin_session, "box-b", "jane").await;
    let (bea, session) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let granted = host::grant_add(s, &bea, "observer", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    Some((rig, admin_session, karl, jane, bea, session))
}

/// **The list holds the agents the person holds a grant on, and no
/// other**; the page and the live view of an agent without a grant answer
/// exactly as an unknown agent's; a malformed identity is refused first.
#[tokio::test]
async fn a_person_reads_only_the_agents_they_hold_a_grant_on() {
    let Some((rig, admin_session, karl, jane, _, session)) = granted().await else {
        return;
    };
    let (status, _, list) = send_as(&rig.app, "GET", "/agents", None, &session).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    assert!(list.contains("box-a/karl"), "{list}");
    assert!(!list.contains("jane") && !list.contains(&jane), "{list}");

    let unknown = send_as(
        &rig.app,
        "GET",
        &format!("/agents/{UNKNOWN}"),
        None,
        &session,
    )
    .await;
    assert_eq!(unknown.0, StatusCode::NOT_FOUND);
    for uri in [format!("/agents/{jane}"), format!("/agents/{jane}/trace")] {
        let (status, _, body) = send_as(&rig.app, "GET", &uri, None, &session).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(body, unknown.2, "{uri}: identical to an unknown agent's");
    }
    let (status, _, _) = send_as(&rig.app, "GET", "/agents/box-b%2Fjane", None, &session).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // The admin holds no grant on either: nothing here, and the register
    // page shows the rows without their state.
    let (_, _, list) = send_as(&rig.app, "GET", "/agents", None, &admin_session).await;
    assert!(!list.contains("karl") && !list.contains("jane"), "{list}");
    let (status, _, _) = send_as(
        &rig.app,
        "GET",
        &format!("/agents/{karl}"),
        None,
        &admin_session,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, _, register) = send_as(&rig.app, "GET", "/admin/agents", None, &admin_session).await;
    assert!(register.contains("box-a/karl"), "{register}");
    assert!(
        !register.contains("Load state") && !register.contains("Trace door"),
        "the register page shows no agent's state"
    );
}

/// **The page shows the register row's facts** to a person holding a
/// grant: presence, the ceiling, the door, the load state with its source
/// and date, the tuple and the constituents, and the person's roles.
#[tokio::test]
async fn the_page_shows_the_rows_facts_and_the_persons_roles() {
    let Some((rig, _, karl, _, _, session)) = granted().await else {
        return;
    };
    sqlx::query(
        "UPDATE agent SET admin_ceiling = ARRAY['show','load'], admin_ceiling_at = now(), \
         trace_door = true, trace_door_at = now(), load_state = 'loaded', \
         state_source = 'show', load_state_at = now(), \
         tuple = '{\"model\": \"m-1\"}'::jsonb, tuple_source = 'show', tuple_at = now(), \
         constituents = ARRAY[4242, 4343], constituents_at = now() WHERE agent_id = $1",
    )
    .bind(&karl)
    .execute(&rig.store().pool)
    .await
    .unwrap();
    let (status, _, page) =
        send_as(&rig.app, "GET", &format!("/agents/{karl}"), None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    for fact in [
        "box-a/karl",
        "your roles: observer",
        "show load",
        "open",
        "loaded, by show",
        "m-1",
        "4242 4343",
        "not a record",
    ] {
        assert!(page.contains(fact), "{fact}: {page}");
    }
}

/// The live view's stream, opened under `session`.
async fn open(rig: &Rig, agent: &str, session: &str) -> axum::body::BodyDataStream {
    let response = rig
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/agents/{agent}/trace"))
                .header(header::ORIGIN, ORIGIN)
                .header(header::COOKIE, format!("__Host-weaver_session={session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.into_body().into_data_stream()
}

/// Read the stream until `needle` arrives, or answer what was read and
/// whether it ended, within `bound`.
async fn until(
    stream: &mut axum::body::BodyDataStream,
    needle: &str,
    bound: Duration,
) -> (String, bool) {
    let mut read = String::new();
    let deadline = tokio::time::Instant::now() + bound;
    loop {
        match tokio::time::timeout_at(deadline, stream.next()).await {
            Ok(Some(Ok(chunk))) => {
                read.push_str(&String::from_utf8_lossy(&chunk));
                if read.contains(needle) {
                    return (read, false);
                }
            }
            Ok(Some(Err(_))) | Ok(None) => return (read, true),
            Err(_) => return (read, false),
        }
    }
}

fn event(seq: u64, text: &str) -> TraceEvent {
    TraceEvent {
        seq,
        mark: None,
        run: Some("run-1".into()),
        turn: None,
        kind: Some("note".into()),
        raw: serde_json::json!({ "kind": "note", "text": text }),
    }
}

/// **The live view relays the window and closes within its re-check when
/// the grant ends**: it says it is the live window, the window's events
/// reach it, and the host's revocation of the person's grant closes it
/// with its reason within the interval, the stream then ending.
#[tokio::test]
async fn the_live_view_relays_the_window_and_closes_when_the_grant_ends() {
    let Some((rig, _, karl, _, bea, session)) = granted().await else {
        return;
    };
    rig.listener
        .windows()
        .ingest(&karl, event(1, "held before"));
    let mut stream = open(&rig, &karl, &session).await;
    let (read, _) = until(&mut stream, "held before", Duration::from_secs(2)).await;
    assert!(
        read.contains("not a record"),
        "the view says what it is: {read}"
    );
    rig.listener
        .windows()
        .ingest(&karl, event(2, "arrived after"));
    let (read, _) = until(&mut stream, "arrived after", Duration::from_secs(2)).await;
    assert!(read.contains("arrived after"), "{read}");

    let removed = host::grant_remove(rig.store(), &bea, "observer", Some(&karl), None).await;
    assert!(removed.ok, "{}", removed.value);
    let (read, ended) = until(
        &mut stream,
        "the grant on this agent ended",
        TEST_RECHECK * 5,
    )
    .await;
    assert!(read.contains("event: closed"), "{read}");
    let (_, ended_after) = if ended {
        (String::new(), true)
    } else {
        until(&mut stream, "\u{0}", Duration::from_secs(2)).await
    };
    assert!(ended_after, "the stream ends");
}

/// **Disabling the person, or closing the session, closes the live view**
/// within its re-check: bea's view closes when she is disabled, and
/// cara's when her session is signed out.
#[tokio::test]
async fn the_live_view_closes_when_the_person_or_the_session_ends() {
    let Some((rig, _, karl, _, bea, session)) = granted().await else {
        return;
    };
    let s = rig.store();
    let (cara, cara_session) = signed_in(&rig.app, s, &mut authenticator(), "cara").await;
    let granted = host::grant_add(s, &cara, "observer", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let mut beas = open(&rig, &karl, &session).await;
    let mut caras = open(&rig, &karl, &cara_session).await;
    until(&mut beas, "not a record", Duration::from_secs(2)).await;
    until(&mut caras, "not a record", Duration::from_secs(2)).await;

    sqlx::query("UPDATE person SET enabled = false WHERE person_id = $1")
        .bind(&bea)
        .execute(&s.pool)
        .await
        .unwrap();
    let (read, _) = until(&mut beas, "the session ended", TEST_RECHECK * 5).await;
    assert!(
        read.contains("event: closed"),
        "a disabled person's view closes: {read}"
    );

    let (status, _, _) = send_as(&rig.app, "POST", "/sign-out", None, &cara_session).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (read, _) = until(&mut caras, "the session ended", TEST_RECHECK * 5).await;
    assert!(
        read.contains("event: closed"),
        "a signed-out session's view closes: {read}"
    );
}
