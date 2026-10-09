//! The session as a person's (`src/surfaces/gate.rs`, Spec 2.8, design
//! section 6): the cookie, each end at use, the last-used refresh, the
//! `Origin` check, sign-out and the author member. Sessions are opened
//! directly in the store, the ceremonies being the sign-in pull request's;
//! each test runs on a database of its own.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

use super::gate::{self, Ended, Policy};
use crate::store::Store;
use crate::store::identity::{self, Author};
use crate::store::read::tests::fresh_store;

/// The policy at the design's limits, the origin a reserved test name.
pub(crate) fn policy() -> Policy {
    Policy {
        origin: Some("https://weaver.test".to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
    }
}

/// A person, a passkey and a session opened with it, directly in the
/// store: the bearer a browser would hold, the person's identity and the
/// passkey's credential ID.
pub(crate) async fn open(store: &Store, name: &str) -> (String, String, String) {
    let person: String = sqlx::query_scalar(
        "INSERT INTO person (name, name_key) VALUES ($1, $2) RETURNING person_id",
    )
    .bind(name)
    .bind(identity::name_key(name))
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let credential = format!("cred-{}", uuid::Uuid::new_v4().simple());
    let passkey = enroll(store, &person, &credential).await;
    let bearer = uuid::Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO session (bearer_digest, person_id, passkey_id) VALUES ($1, $2, $3)")
        .bind(gate::digest(&bearer))
        .bind(&person)
        .bind(&passkey)
        .execute(&store.pool)
        .await
        .unwrap();
    (bearer, person, credential)
}

/// A passkey enrolled for a person under a credential ID, directly in the
/// store; answers the passkey's own identity.
async fn enroll(store: &Store, person: &str, credential: &str) -> String {
    sqlx::query_scalar(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ($1, $2, '{}') \
         RETURNING passkey_id",
    )
    .bind(credential)
    .bind(person)
    .fetch_one(&store.pool)
    .await
    .unwrap()
}

/// The headers a browser sends with its session's cookie.
fn with_cookie(name: &str, bearer: &str) -> axum::http::HeaderMap {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::COOKIE, format!("{name}={bearer}").parse().unwrap());
    headers
}

async fn closed(store: &Store, bearer: &str) -> bool {
    sqlx::query_scalar("SELECT closed_at IS NOT NULL FROM session WHERE bearer_digest = $1")
        .bind(gate::digest(bearer))
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn last_used(store: &Store, bearer: &str) -> chrono::DateTime<chrono::Utc> {
    sqlx::query_scalar("SELECT last_used_at FROM session WHERE bearer_digest = $1")
        .bind(gate::digest(bearer))
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

async fn set(store: &Store, bearer: &str, assignment: &str) {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE session SET {assignment} WHERE bearer_digest = $1"
    )))
    .bind(gate::digest(bearer))
    .execute(&store.pool)
    .await
    .unwrap();
}

/// **The cookie is `__Host-weaver_session`, `Secure`, `HttpOnly`,
/// `SameSite=Strict`, `Path=/` and no `Domain`**, set and cleared alike, and
/// a request carrying only the claimed-name session's old name finds no
/// session.
#[tokio::test]
async fn the_cookie_is_the_hosts_alone_and_the_old_name_finds_nothing() {
    for value in [gate::set_cookie("b"), gate::cleared_cookie()] {
        let value = value.to_str().unwrap().to_owned();
        assert!(value.starts_with("__Host-weaver_session="), "{value}");
        for attribute in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
            assert!(
                value.split("; ").any(|a| a == attribute),
                "{attribute} missing from {value}"
            );
        }
        assert!(!value.contains("Domain"), "{value}");
    }
    assert!(
        gate::cleared_cookie()
            .to_str()
            .unwrap()
            .ends_with("; Max-Age=0")
    );

    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let (bearer, person, _) = open(s, "ada").await;
    let found = gate::session(s, &policy(), &with_cookie(gate::COOKIE, &bearer))
        .await
        .unwrap()
        .expect("the session under its cookie");
    assert_eq!(
        (found.person_id.as_str(), found.name.as_str()),
        (person.as_str(), "ada")
    );
    assert_eq!(
        gate::session(s, &policy(), &with_cookie("weaver_session", &bearer))
            .await
            .unwrap(),
        None,
        "the old name names no session"
    );
}

/// **Each end is checked at every use** (design section 6): a session
/// closed, idle past its limit, open past its absolute limit, its person
/// disabled, or its passkey removed is not served, and the use that finds
/// it ended closes its row; a session that has ended none is served.
#[tokio::test]
async fn each_end_is_seen_at_the_next_use_and_closes_the_row() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let p = policy();

    let (live, ..) = open(s, "live").await;
    assert!(gate::at_use(s, &p, &live).await.unwrap().is_ok());

    let (bearer, ..) = open(s, "closed").await;
    set(s, &bearer, "closed_at = now()").await;
    assert_eq!(
        gate::at_use(s, &p, &bearer).await.unwrap(),
        Err(Some(Ended::Closed))
    );

    let (bearer, ..) = open(s, "idle").await;
    set(
        s,
        &bearer,
        "opened_at = now() - interval '2 hours', last_used_at = now() - interval '61 minutes'",
    )
    .await;
    assert_eq!(
        gate::at_use(s, &p, &bearer).await.unwrap(),
        Err(Some(Ended::Idle))
    );
    assert!(closed(s, &bearer).await, "the idle session's row closed");

    let (bearer, ..) = open(s, "expired").await;
    set(
        s,
        &bearer,
        "opened_at = now() - interval '12 hours 1 minute', last_used_at = now()",
    )
    .await;
    assert_eq!(
        gate::at_use(s, &p, &bearer).await.unwrap(),
        Err(Some(Ended::Expired))
    );
    assert!(closed(s, &bearer).await, "the expired session's row closed");

    let (bearer, person, _) = open(s, "disabled").await;
    sqlx::query("UPDATE person SET enabled = false, version = version + 1 WHERE person_id = $1")
        .bind(&person)
        .execute(&s.pool)
        .await
        .unwrap();
    assert_eq!(
        gate::at_use(s, &p, &bearer).await.unwrap(),
        Err(Some(Ended::PersonDisabled))
    );
    assert!(
        closed(s, &bearer).await,
        "the disabled person's session closed"
    );

    let (bearer, _, credential) = open(s, "removed").await;
    sqlx::query("DELETE FROM passkey WHERE credential_id = $1")
        .bind(&credential)
        .execute(&s.pool)
        .await
        .unwrap();
    assert_eq!(
        gate::at_use(s, &p, &bearer).await.unwrap(),
        Err(Some(Ended::PasskeyRemoved))
    );
    assert!(
        closed(s, &bearer).await,
        "the removed passkey's session closed"
    );

    assert_eq!(
        gate::at_use(s, &p, "no-such-bearer").await.unwrap(),
        Err(None)
    );
    assert!(
        gate::at_use(s, &p, &live).await.unwrap().is_ok(),
        "the live one still serves"
    );
}

/// **The last use is written at most once a minute and never moves
/// backwards** (design section 6): two uses within a minute write once, a
/// use a minute later writes again, and a stored time ahead of the
/// database's clock is left where it is.
#[tokio::test]
async fn the_last_use_is_refreshed_at_most_once_a_minute_and_never_backwards() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let p = policy();
    let (bearer, ..) = open(s, "reader").await;

    set(s, &bearer, "last_used_at = now() - interval '2 minutes'").await;
    let before = last_used(s, &bearer).await;
    gate::at_use(s, &p, &bearer).await.unwrap().unwrap();
    let first = last_used(s, &bearer).await;
    assert!(first > before, "a use two minutes on refreshes");
    tokio::time::sleep(Duration::from_millis(20)).await;
    gate::at_use(s, &p, &bearer).await.unwrap().unwrap();
    assert_eq!(
        last_used(s, &bearer).await,
        first,
        "a second use within the minute writes nothing"
    );

    set(s, &bearer, "last_used_at = now() - interval '61 seconds'").await;
    let stale = last_used(s, &bearer).await;
    gate::at_use(s, &p, &bearer).await.unwrap().unwrap();
    assert!(
        last_used(s, &bearer).await > stale,
        "a use a minute later refreshes again"
    );

    set(s, &bearer, "last_used_at = now() + interval '1 hour'").await;
    let ahead = last_used(s, &bearer).await;
    gate::at_use(s, &p, &bearer).await.unwrap().unwrap();
    assert_eq!(
        last_used(s, &bearer).await,
        ahead,
        "it never moves backwards"
    );
}

/// The surfaces under the `Origin` check, as the server mounts them.
fn app(s: &Store, policy: Policy) -> axum::Router {
    gate::guard(super::routes(policy.clone()).with_state(s.clone()), policy)
}

async fn send(
    app: axum::Router,
    method: &str,
    uri: &str,
    origin: Option<&str>,
    bearer: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap) {
    let mut request = Request::builder().method(method).uri(uri);
    if let Some(origin) = origin {
        request = request.header(header::ORIGIN, origin);
    }
    if let Some(bearer) = bearer {
        request = request.header(header::COOKIE, format!("{}={bearer}", gate::COOKIE));
    }
    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    (response.status(), response.headers().clone())
}

/// **Every request that changes state carries the configured origin or is
/// refused before its handler** (design section 6): a POST from another
/// origin, with none, or with two is refused and changes nothing, one with
/// the configured origin is served, and a GET with none is served. **With no
/// origin configured, every POST is refused.**
#[tokio::test]
async fn a_request_that_changes_state_comes_from_the_origin_or_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let (bearer, ..) = open(s, "ada").await;
    for origin in [
        Some("https://elsewhere.test"),
        None,
        Some("https://weaver.test/"),
    ] {
        let (status, _) = send(app(s, policy()), "POST", "/sign-out", origin, Some(&bearer)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{origin:?}");
        assert!(
            !closed(s, &bearer).await,
            "a refused sign-out changed nothing ({origin:?})"
        );
    }
    let mut twice = Request::builder()
        .method("POST")
        .uri("/sign-out")
        .header(header::COOKIE, format!("{}={bearer}", gate::COOKIE))
        .body(Body::empty())
        .unwrap();
    for origin in ["https://weaver.test", "https://elsewhere.test"] {
        twice
            .headers_mut()
            .append(header::ORIGIN, origin.parse().unwrap());
    }
    let response = app(s, policy()).oneshot(twice).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN, "two origins");

    let (status, _) = send(app(s, policy()), "GET", "/record", None, Some(&bearer)).await;
    assert_eq!(status, StatusCode::OK, "a GET needs no origin");

    let mut unconfigured = policy();
    unconfigured.origin = None;
    let (status, _) = send(
        app(s, unconfigured),
        "POST",
        "/sign-out",
        Some("https://weaver.test"),
        Some(&bearer),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no origin configured");

    let (status, _) = send(
        app(s, policy()),
        "POST",
        "/sign-out",
        Some("https://weaver.test"),
        Some(&bearer),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NO_CONTENT,
        "the configured origin is served"
    );
}

/// **Sign-out closes the session's row and clears the cookie**, and the
/// next request with that cookie finds no session.
#[tokio::test]
async fn sign_out_closes_the_row_and_the_next_use_finds_no_session() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let (bearer, ..) = open(s, "ada").await;
    let (status, headers) = send(
        app(s, policy()),
        "POST",
        "/sign-out",
        Some("https://weaver.test"),
        Some(&bearer),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        headers.get(header::SET_COOKIE),
        Some(&gate::cleared_cookie()),
        "the cookie is cleared"
    );
    assert!(closed(s, &bearer).await, "the row is closed");
    assert_eq!(
        gate::at_use(s, &policy(), &bearer).await.unwrap(),
        Err(Some(Ended::Closed))
    );
    let (status, _) = send(app(s, policy()), "GET", "/record", None, Some(&bearer)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the next request finds no session"
    );
}

/// **An authored row written in a session names its person** (Spec 3.2,
/// design section 6): the author member takes the person's identity, and
/// renders as their current name after a rename; a member written before
/// persons stood renders as a claim, and a null as no author.
#[tokio::test]
async fn an_authored_row_names_its_person_and_renders_their_current_name() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let (bearer, person, _) = open(s, "ada").await;
    let session = gate::at_use(s, &policy(), &bearer).await.unwrap().unwrap();
    assert_eq!(session.author(), person, "the identity, never the name");

    sqlx::query(
        "INSERT INTO run (run_id, record_identity, sampler, boundary_set) \
         VALUES ('authored-parent', 'AUTHOR-REC', '{}', '[]')",
    )
    .execute(&s.pool)
    .await
    .unwrap();
    let plan: crate::store::PlanId = sqlx::query_scalar::<_, String>(
        "INSERT INTO plan (parent_run_id, author) VALUES ('authored-parent', $1) RETURNING plan_id",
    )
    .bind(session.author())
    .fetch_one(&s.pool)
    .await
    .unwrap()
    .parse()
    .unwrap();
    let written = s.plan(&plan).await.unwrap().unwrap();
    assert_eq!(written.author.as_deref(), Some(person.as_str()));
    assert_eq!(
        s.author(written.author.as_deref()).await.unwrap(),
        Author::Person {
            person_id: person.clone(),
            name: "ada".to_owned()
        }
    );

    sqlx::query(
        "UPDATE person SET name = 'Ada Lovelace', name_key = $2, version = version + 1 \
         WHERE person_id = $1",
    )
    .bind(&person)
    .bind(identity::name_key("Ada Lovelace"))
    .execute(&s.pool)
    .await
    .unwrap();
    assert_eq!(
        s.author(written.author.as_deref()).await.unwrap(),
        Author::Person {
            person_id: person.clone(),
            name: "Ada Lovelace".to_owned()
        },
        "the current name after a rename"
    );
    assert_eq!(
        s.author(Some("todd")).await.unwrap(),
        Author::Claim("todd".to_owned())
    );
    assert_eq!(
        s.author(Some("pe-0000000000000000")).await.unwrap(),
        Author::Claim("pe-0000000000000000".to_owned()),
        "an identity's shape that resolves to no person is a claim"
    );
    assert_eq!(s.author(None).await.unwrap(), Author::Absent);
}

/// **A session in active use stays open at the shortest idle limit**: used
/// every 50 seconds under the 300-second floor, across eight minutes, it is
/// served at every use and never closed. The clock is the database's, so
/// each step moves the session's own times back 50 seconds rather than
/// sleeping; the refresh's one-minute grain leaves its last use at most
/// about two minutes stale, well inside the limit.
#[tokio::test]
async fn a_session_used_every_fifty_seconds_stays_open_at_the_idle_floor() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let mut p = policy();
    p.idle = Duration::from_secs(crate::config::SESSION_IDLE_FLOOR_SECS);
    let (bearer, ..) = open(s, "reader").await;
    for step in 0..10 {
        set(
            s,
            &bearer,
            "opened_at = opened_at - interval '50 seconds', \
             last_used_at = last_used_at - interval '50 seconds'",
        )
        .await;
        assert!(
            gate::at_use(s, &p, &bearer).await.unwrap().is_ok(),
            "a use {}s on was refused",
            (step + 1) * 50
        );
    }
    assert!(!closed(s, &bearer).await, "the session stayed open");
}

/// **A close decided on a stale read moves nothing** (design section 6,
/// one writer): a use reads a session as idle and is held before its
/// close; another request's use refreshes the session meanwhile; the held
/// use, released, closes nothing, since its close restates the idle reason,
/// reads the session again, and serves it. The session stays open.
#[tokio::test]
async fn a_session_refreshed_between_a_read_and_its_close_stays_open() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let (bearer, ..) = open(s, "reader").await;
    set(
        s,
        &bearer,
        "opened_at = now() - interval '2 hours', last_used_at = now() - interval '61 minutes'",
    )
    .await;

    let (read, release) = (
        std::sync::Arc::new(tokio::sync::Notify::new()),
        std::sync::Arc::new(tokio::sync::Notify::new()),
    );
    *gate::CLOSE_HOLD.lock().unwrap() =
        Some((gate::digest(&bearer), read.clone(), release.clone()));
    let held = tokio::spawn({
        let (s, bearer) = (s.clone(), bearer.clone());
        async move { gate::at_use(&s, &policy(), &bearer).await.unwrap() }
    });
    tokio::time::timeout(Duration::from_secs(10), read.notified())
        .await
        .expect("the held use read the session as idle");

    // The other request's refresh, as its use writes it.
    set(s, &bearer, "last_used_at = now()").await;
    release.notify_one();

    let answered = held.await.unwrap();
    assert!(
        answered.is_ok(),
        "the refreshed session was refused on a stale read: {answered:?}"
    );
    assert!(
        !closed(s, &bearer).await,
        "the refreshed session stayed open"
    );
}

/// **A session ends with its passkey, a credential re-enrolled after a
/// reset notwithstanding** (design section 6, Codex on #29): the session
/// names the passkey's own identity, never reused, so the host reset's
/// removal of the passkey, followed by enrolling the same credential ID
/// again, leaves the old session naming a passkey that is gone, and it is
/// refused at its next use; a session opened on the new passkey serves.
#[tokio::test]
async fn a_credential_re_enrolled_after_a_reset_revives_no_session() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let (bearer, person, credential) = open(s, "ada").await;
    assert!(gate::at_use(s, &policy(), &bearer).await.unwrap().is_ok());

    // The host reset, then the same authenticator enrolled again.
    sqlx::query("DELETE FROM passkey WHERE person_id = $1")
        .bind(&person)
        .execute(&s.pool)
        .await
        .unwrap();
    let again = enroll(s, &person, &credential).await;

    assert_eq!(
        gate::at_use(s, &policy(), &bearer).await.unwrap(),
        Err(Some(Ended::PasskeyRemoved)),
        "a session opened before the reset revived on the re-enrolled credential"
    );
    let bearer = uuid::Uuid::new_v4().simple().to_string();
    sqlx::query("INSERT INTO session (bearer_digest, person_id, passkey_id) VALUES ($1, $2, $3)")
        .bind(gate::digest(&bearer))
        .bind(&person)
        .bind(&again)
        .execute(&s.pool)
        .await
        .unwrap();
    assert!(
        gate::at_use(s, &policy(), &bearer).await.unwrap().is_ok(),
        "a session on the new passkey serves"
    );
}
