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
use crate::agents_tests::{Connector, Rig, TEST_RECHECK, rig};
use crate::host;
use crate::link::frames::{FromClient, Position, ToClient};
use crate::passkeys_tests::{ORIGIN, authenticator};
use crate::traceview::TraceEvent;
use crate::verbs_tests::answer;

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
        replayed: false,
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

/// **A viewer gone ends the view at once**: the stream dropped on a quiet
/// agent, whose session and grant stand, releases its subscription to the
/// window within moments, not at the session's or the grant's end.
#[tokio::test]
async fn a_dropped_view_releases_its_subscription() {
    let Some((rig, _, karl, _, _, session)) = granted().await else {
        return;
    };
    let mut stream = open(&rig, &karl, &session).await;
    until(&mut stream, "not a record", Duration::from_secs(2)).await;
    assert_eq!(rig.listener.windows().watchers(&karl), 1);
    drop(stream);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while rig.listener.windows().watchers(&karl) > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        rig.listener.windows().watchers(&karl),
        0,
        "the view's task ended with its viewer"
    );
}

/// **Nothing reaches the reader after the cut**: a reader reads the first
/// events and then stops reading, the view's channel filling behind it;
/// the host revokes the grant; the view closes within its re-check
/// (releasing its subscription) though its reader is not reading; and when
/// the reader resumes, it receives the closing line and the end, and none
/// of the events queued before the cut nor any arriving after.
#[tokio::test]
async fn nothing_reaches_the_reader_after_the_cut() {
    let Some((rig, _, karl, _, bea, session)) = granted().await else {
        return;
    };
    let mut stream = open(&rig, &karl, &session).await;
    for seq in 1..=100 {
        rig.listener
            .windows()
            .ingest(&karl, event(seq, &format!("before-{seq}")));
    }
    let (read, _) = until(&mut stream, "before-3\"", Duration::from_secs(2)).await;
    assert!(
        read.contains("before-1"),
        "the reader read the first: {read}"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    let removed = host::grant_remove(rig.store(), &bea, "observer", Some(&karl), None).await;
    assert!(removed.ok, "{}", removed.value);
    for seq in 101..=110 {
        rig.listener
            .windows()
            .ingest(&karl, event(seq, &format!("after-{seq}")));
    }
    let deadline = tokio::time::Instant::now() + TEST_RECHECK * 8;
    while rig.listener.windows().watchers(&karl) > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        rig.listener.windows().watchers(&karl),
        0,
        "the view closed within its re-check though its reader was not reading"
    );
    let (resumed, ended) = until(&mut stream, "\u{0}", Duration::from_secs(2)).await;
    assert!(ended, "the stream ends");
    assert!(
        resumed.contains("event: closed") && resumed.contains("the grant on this agent ended"),
        "the closing line, first: {resumed}"
    );
    assert!(
        !resumed.contains("event: trace"),
        "no queued event after the cut: {resumed}"
    );
}

/// karl registered by ada, bea signed in holding `observer` on him, and his
/// admin-con dialed on its real config with its admission's `show`
/// answered and landed, **not yet caught up**: the hello's boundary is
/// offset 0 of generation `g`.
async fn replaying() -> Option<(Rig, String, String, String, Connector)> {
    let rig = rig().await?;
    let s = rig.store().clone();
    let (_, ada_session) = admin(&rig.app, &s, &mut authenticator(), "ada").await;
    let (karl, handles) = rig.register(&ada_session, "box-a", "karl").await;
    let (_, _, config) = rig.take(&handles[1], &ada_session).await;
    let (bea, session) = signed_in(&rig.app, &s, &mut authenticator(), "bea").await;
    let granted = host::grant_add(&s, &bea, "observer", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let admin_con = admitted_replaying(&config).await;
    Some((rig, karl, session, config, admin_con))
}

/// An admin-con dialed and admitted up to `caught_up`, which is left to the
/// test.
async fn admitted_replaying(config: &str) -> Connector {
    let mut admin_con = Connector::dial_with(config, &["show"]).await;
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
            outcome: Some(answer(
                "show",
                serde_json::json!({"kind": "state", "state": "idle"}),
                0,
            )),
            error: None,
        })
        .await;
    assert!(matches!(
        admin_con.recv().await,
        Some(ToClient::Landed { .. })
    ));
    admin_con
}

/// Send one event frame and take its acknowledgement.
async fn relay(admin_con: &mut Connector, position: Position, flagged: bool, event: TraceEvent) {
    admin_con
        .send(FromClient::Event {
            position,
            replayed: flagged,
            event,
        })
        .await;
    match admin_con.recv().await {
        Some(ToClient::Ack { .. }) => {}
        other => panic!("expected an ack, got {other:?}"),
    }
}

fn at(generation: &str, offset: u64) -> Position {
    Position {
        generation: generation.into(),
        offset,
        digest: "d".into(),
    }
}

fn mark(seq: u64, reason: &str) -> TraceEvent {
    TraceEvent {
        seq,
        mark: Some(reason.into()),
        run: None,
        turn: None,
        kind: None,
        raw: serde_json::Value::Null,
        replayed: false,
    }
}

/// Every `data:` line of the stream that is a JSON object: the events as
/// the live view sent them.
fn sent(read: &str) -> Vec<serde_json::Value> {
    read.lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter_map(|d| serde_json::from_str::<serde_json::Value>(d).ok())
        .filter(|v| v.is_object())
        .collect()
}

/// The one event the view sent whose text or mark contains `needle`.
fn one<'a>(events: &'a [serde_json::Value], needle: &str) -> &'a serde_json::Value {
    let found: Vec<_> = events
        .iter()
        .filter(|e| {
            e["raw"]["text"]
                .as_str()
                .is_some_and(|t| t.contains(needle))
                || e["mark"].as_str().is_some_and(|m| m.contains(needle))
        })
        .collect();
    assert_eq!(found.len(), 1, "one event for {needle}: {events:?}");
    found[0]
}

/// **A replayed event reaches the live view saying so** (Spec 2.13, 7.2):
/// through the real listener, an event sent before `caught_up` arrives on
/// the SSE stream with `"replayed":true` though admin-con flagged it live,
/// and one sent after with `"replayed":false`. The view carries the
/// listener's verdict, never admin-con's flag.
///
/// Perturbation: set the member from admin-con's flag in `land_event`, and
/// the event the frame placed behind the boundary reads live.
#[tokio::test]
async fn a_replayed_event_reaches_the_live_view_saying_so() {
    let Some((rig, karl, session, _, mut admin_con)) = replaying().await else {
        return;
    };
    let mut stream = open(&rig, &karl, &session).await;
    until(&mut stream, "not a record", Duration::from_secs(2)).await;
    // Flagged live by admin-con, sent before `caught_up`: the replay's.
    relay(
        &mut admin_con,
        at("old", 10),
        false,
        event(1, "sent before caught up"),
    )
    .await;
    admin_con.send(FromClient::CaughtUp).await;
    relay(
        &mut admin_con,
        at("g", 20),
        false,
        event(2, "sent after caught up"),
    )
    .await;
    let (read, _) = until(&mut stream, "sent after caught up", Duration::from_secs(2)).await;
    let events = sent(&read);
    assert_eq!(
        one(&events, "sent before caught up")["replayed"],
        true,
        "{read}"
    );
    assert_eq!(
        one(&events, "sent after caught up")["replayed"],
        false,
        "{read}"
    );
}

/// **A mark behind the boundary is replayed, and the server's own marks
/// are not** (Spec 2.13, 7.2): a backfill's front mark sent before
/// `caught_up` arrives `replayed: true`, as a record there would; the
/// marks the server writes itself, the link's loss and the reconnection,
/// arrive `replayed: false`.
///
/// Perturbation: default marks to `false` in `land_event`, and the
/// backfill's mark reads as live.
#[tokio::test]
async fn a_mark_behind_the_boundary_is_replayed_and_the_servers_marks_are_not() {
    let Some((rig, karl, session, config, mut admin_con)) = replaying().await else {
        return;
    };
    let mut stream = open(&rig, &karl, &session).await;
    until(&mut stream, "not a record", Duration::from_secs(2)).await;
    relay(
        &mut admin_con,
        at("old", 0),
        true,
        mark(1, "backfill starts 10 bytes into the file"),
    )
    .await;
    relay(
        &mut admin_con,
        at("old", 10),
        true,
        event(2, "behind the boundary"),
    )
    .await;
    admin_con.send(FromClient::CaughtUp).await;
    relay(
        &mut admin_con,
        at("g", 20),
        false,
        event(3, "live after it"),
    )
    .await;
    // The stream is read in three parts; the events are taken from all.
    let (mut read, _) = until(&mut stream, "live after it", Duration::from_secs(2)).await;

    drop(admin_con);
    read += &until(
        &mut stream,
        "link to admin-con lost",
        Duration::from_secs(5),
    )
    .await
    .0;
    let _again = admitted_replaying(&config).await;
    read += &until(&mut stream, "admin-con reconnected", Duration::from_secs(5))
        .await
        .0;
    let events = sent(&read);
    assert_eq!(one(&events, "backfill starts")["replayed"], true, "{read}");
    assert_eq!(
        one(&events, "behind the boundary")["replayed"],
        true,
        "{read}"
    );
    assert_eq!(one(&events, "live after it")["replayed"], false, "{read}");
    assert_eq!(
        one(&events, "link to admin-con lost")["replayed"],
        false,
        "{read}"
    );
    assert_eq!(
        one(&events, "admin-con reconnected")["replayed"],
        false,
        "{read}"
    );
}
