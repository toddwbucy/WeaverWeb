//! **No sign-in, for development** (the operator's ruling of 2026-11-21, the
//! config's `no_sign_in_as`): the surfaces driven in process with no cookie,
//! the standing person carried in the `Policy` they are mounted with. Each
//! test runs on a database of its own.

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt;

use crate::admin_tests::form;
use crate::config::SignInMethod;
use crate::host;
use crate::passkeys_tests::{ORIGIN, passkeys};
use crate::sign_in_tests::rows;
use crate::sign_in_token_tests::{cfg, person, policy};
use crate::store::Store;
use crate::surfaces::gate;

const TOKEN: &[SignInMethod] = &[SignInMethod::Token];

/// The policy the server builds from a config naming `name` as its
/// standing person, the test's limits and origin otherwise.
fn standing_policy(name: &str) -> gate::Policy {
    let mut config = cfg(TOKEN);
    config.no_sign_in_as = Some(name.to_owned());
    gate::Policy {
        standing: gate::Policy::from_config(&config).standing,
        ..policy()
    }
}

/// The surfaces as the server mounts them, under the script policy and the
/// `Origin` check, every one with `policy`.
fn app(s: &Store, policy: gate::Policy) -> axum::Router {
    let passkeys = Some(passkeys());
    gate::guard(
        crate::surfaces::routes(policy.clone())
            .merge(crate::surfaces::script_policy(
                crate::surfaces::enroll::routes(passkeys.clone()),
            ))
            .merge(crate::surfaces::script_policy(
                crate::surfaces::sign_in::routes(passkeys, TOKEN),
            ))
            .merge(crate::surfaces::script_policy(
                crate::surfaces::admin::routes(policy.clone(), 24),
            ))
            .with_state(s.clone()),
        policy,
    )
}

/// A request from the server's own origin carrying **no cookie**, a form's
/// body where given.
async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<String>,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(body)
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

/// **The policy carries a standing person exactly where the config names
/// one**: none on a config as before, the named one otherwise.
#[test]
fn the_policy_carries_a_standing_person_only_where_one_is_named() {
    assert!(gate::Policy::from_config(&cfg(TOKEN)).standing.is_none());
    let standing = standing_policy("ada").standing.expect("a standing person");
    assert_eq!(standing.name(), "ada");
}

/// **With no cookie, the pages serve the standing person**: `/` goes on to
/// `/pair`, the standing person holding the admin grant, The Record shows
/// their name, the persons page serves a
/// standing admin, and an admin write lands with them as its principal by
/// `session`, as any signed-in admin's does. No cookie is set.
#[tokio::test]
async fn with_no_cookie_the_pages_serve_the_standing_person() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let granted = host::grant_add(s, &ada, "admin", None, None).await;
    assert!(granted.ok, "{}", granted.value);
    let bea = person(s, "bea").await;
    let app = app(s, standing_policy("ada"));

    let (status, headers, _) = send(&app, "GET", "/", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/pair");

    let (status, headers, page) = send(&app, "GET", "/record", None).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        page.contains(r#"<span class="who">ada &middot; admin</span>"#),
        "The Record shows the standing person: {page}"
    );
    assert!(
        headers.get(header::SET_COOKIE).is_none(),
        "no cookie is set"
    );

    let (status, _, page) = send(&app, "GET", "/admin/persons", None).await;
    assert_eq!(status, StatusCode::OK, "{page}");

    let version: i64 = sqlx::query_scalar("SELECT version FROM person WHERE person_id = $1")
        .bind(&bea)
        .fetch_one(&s.pool)
        .await
        .unwrap();
    let (status, _, answer) = send(
        &app,
        "POST",
        "/admin/persons/rename",
        Some(form(&[
            ("person", &bea),
            ("version", &version.to_string()),
            ("name", "bee"),
        ])),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(s.person(&bea).await.unwrap().unwrap().name, "bee");
    let audited = rows(
        s,
        &format!(
            "SELECT count(*) FROM audit WHERE principal = 'person' AND person_id = '{ada}' \
             AND method = 'session' AND target_id = '{bea}' AND action = 'person rename' \
             AND outcome = 'ok'"
        ),
    )
    .await;
    assert_eq!(audited, 1, "the write is the standing person's, by session");
    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        1,
        "one standing session"
    );
}

/// **With no standing person, nothing changes**: The Record asked with no
/// cookie links to sign-in, and no session opens.
#[tokio::test]
async fn with_no_person_named_the_record_links_to_sign_in() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    person(s, "ada").await;
    let app = app(s, policy());
    let (_, _, page) = send(&app, "GET", "/record", None).await;
    assert!(
        page.contains(r#"<a href="/sign-in">Sign in</a>"#),
        "The Record links to sign-in: {page}"
    );
    let (status, headers, _) = send(&app, "GET", "/", None).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[header::LOCATION], "/sign-in");
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 0);
}

/// **A fragment's re-fetch reads the standing person too**, with no cookie,
/// through the background read the 15 s polls use; with no standing person
/// it reads none.
#[tokio::test]
async fn the_background_read_serves_the_standing_person() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let none = HeaderMap::new();
    let served = gate::session_in_background(s, &standing_policy("ada"), &none)
        .await
        .unwrap()
        .expect("the poll is served as ada");
    assert_eq!(served.person_id, ada);
    assert_eq!(
        gate::session_in_background(s, &policy(), &none)
            .await
            .unwrap(),
        None
    );
}

/// **The start refuses a person who cannot stand**: a name no person
/// holds, and a disabled person, each refused with the remedy named; an
/// enabled person holding no grant passes.
#[tokio::test]
async fn the_start_refuses_a_person_who_cannot_stand() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let unknown = standing_policy("nobody").standing.unwrap();
    let refused = unknown.ready(s).await.unwrap_err().to_string();
    assert!(refused.contains("no person holds that name"), "{refused}");

    let ada = person(s, "ada").await;
    let standing = standing_policy("ada").standing.unwrap();
    standing.ready(s).await.expect("an enabled person stands");

    sqlx::query("UPDATE person SET enabled = false WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    let refused = standing.ready(s).await.unwrap_err().to_string();
    assert!(refused.contains("who is disabled"), "{refused}");
}

/// **A disabled person writes no audit at each use**: twenty uses while
/// the standing person is disabled serve no one and leave the audit's row
/// count as it was; the next use after the person is enabled is served.
#[tokio::test]
async fn a_disabled_person_writes_no_audit_at_each_use() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    sqlx::query("UPDATE person SET enabled = false WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    let policy = standing_policy("ada");
    let standing = policy.standing.clone().unwrap();
    let before = rows(s, "SELECT count(*) FROM audit").await;
    for _ in 0..20 {
        assert_eq!(standing.session(s, &policy, true).await.unwrap(), None);
    }
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before,
        "twenty refused uses write nothing"
    );
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 0);

    sqlx::query("UPDATE person SET enabled = true WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    let served = standing
        .session(s, &policy, true)
        .await
        .unwrap()
        .expect("served once enabled");
    assert_eq!(served.person_id, ada);
}

/// **A person named by identity is served too**: the redemption matches
/// the person's canonical name, read from the store, not the config's
/// spelling.
#[tokio::test]
async fn a_person_named_by_identity_is_served() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let policy = standing_policy(&ada);
    let standing = policy.standing.clone().unwrap();
    standing.ready(s).await.expect("ada stands by identity");
    let served = standing
        .session(s, &policy, true)
        .await
        .unwrap()
        .expect("served by identity");
    assert_eq!(served.person_id, ada);
}

/// **An opening ends the person's other tokens**, as the doc says: a live
/// enrollment token the host issued ends `revoked`, and a sign-in token the
/// host printed ends `superseded`. A pin of the stated behaviour: it fails
/// where the standing session opens by any path but the host's issue.
#[tokio::test]
async fn an_opening_ends_the_persons_other_tokens() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let printed = host::sign_in_token(s, &cfg(TOKEN), &ada, None, Some("lab")).await;
    assert!(printed.ok, "{}", printed.value);
    let enrollment = host::token(s, &cfg(TOKEN), &ada, None, Some("lab")).await;
    assert!(enrollment.ok, "{}", enrollment.value);
    let digest = |token: &serde_json::Value| {
        crate::store::identity::digest(token["token"].as_str().unwrap())
    };
    let (printed, enrollment) = (digest(&printed.value), digest(&enrollment.value));
    assert_eq!(
        ended(s, "enrollment_token", &enrollment).await,
        None,
        "live before the opening"
    );

    let policy = standing_policy("ada");
    let standing = policy.standing.clone().unwrap();
    standing
        .session(s, &policy, true)
        .await
        .unwrap()
        .expect("served");
    assert_eq!(
        ended(s, "enrollment_token", &enrollment).await.as_deref(),
        Some("revoked")
    );
    assert_eq!(
        ended(s, "sign_in_token", &printed).await.as_deref(),
        Some("superseded")
    );
}

/// How a token ended, by its digest, or `None` while it is live.
async fn ended(s: &Store, table: &str, digest: &str) -> Option<String> {
    let query = format!("SELECT ended FROM {table} WHERE token_digest = $1");
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .bind(digest)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **A served use waits on no opening**: with a session held, a use
/// answers while an opening holds the opening lock.
#[tokio::test]
async fn a_served_use_does_not_wait_on_an_opening() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let policy = standing_policy("ada");
    let standing = policy.standing.clone().unwrap();
    standing
        .session(s, &policy, true)
        .await
        .unwrap()
        .expect("the first use opens the session");
    let _opening = standing.hold_opening().await;
    let served = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        standing.session(s, &policy, true),
    )
    .await
    .expect("a served use does not wait on the opening lock")
    .unwrap()
    .expect("served");
    assert_eq!(served.person_id, ada);
}

/// **Eight first uses at once open one session**: each that finds none
/// serving waits on the opening lock, and every one after the first finds
/// the session the first opened.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn eight_first_uses_open_one_session() {
    let Some(fresh) = crate::store::read::tests::fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let policy = standing_policy("ada");
    let standing = policy.standing.clone().unwrap();
    let uses: Vec<_> = (0..8)
        .map(|_| {
            let (store, policy, standing) = (s.clone(), policy.clone(), standing.clone());
            tokio::spawn(async move { standing.session(&store, &policy, true).await })
        })
        .collect();
    for used in uses {
        let served = used.await.unwrap().unwrap().expect("served");
        assert_eq!(served.person_id, ada);
    }
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 1);
}
