//! **Sign-in by token** (Spec 2.8 and 2.13, the operator's ruling of
//! 2026-11-10): the host's `person sign-in-token`, the token's redemption at
//! `POST /sign-in/token`, the session it opens, what a server configured as
//! before keeps, the store's checks, and `GET /`. Each test runs on a
//! database of its own.

use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt;

use crate::admin_tests::{admin, form, send_as, version};
use crate::config::{ServerConfig, SignInMethod};
use crate::host;
use crate::passkeys_tests::{ORIGIN, authenticator, passkeys};
use crate::sign_in_tests::{bearer, rows};
use crate::store::Store;
use crate::store::identity;
use crate::store::read::tests::fresh_store;
use crate::surfaces::gate::{self, Ended};

const BOTH: &[SignInMethod] = &[SignInMethod::Passkey, SignInMethod::Token];
const PASSKEY: &[SignInMethod] = &[SignInMethod::Passkey];

/// The one answer every token that cannot sign in gets.
const NO_TOKEN: &str = "no sign-in by token is open for that name and token";

fn policy() -> gate::Policy {
    gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
    }
}

/// The surfaces as the server mounts them, under the script policy and the
/// `Origin` check, with `sign_in` as the config would give it.
fn app(s: &Store, sign_in: &[SignInMethod]) -> axum::Router {
    let policy = policy();
    let passkeys = Some(passkeys());
    gate::guard(
        crate::surfaces::routes(policy.clone())
            .merge(crate::surfaces::script_policy(
                crate::surfaces::enroll::routes(passkeys.clone()),
            ))
            .merge(crate::surfaces::script_policy(
                crate::surfaces::sign_in::routes(passkeys, sign_in),
            ))
            .merge(crate::surfaces::script_policy(
                crate::surfaces::admin::routes(policy.clone(), 24),
            ))
            .with_state(s.clone()),
        policy,
    )
}

fn cfg(sign_in: &[SignInMethod]) -> ServerConfig {
    ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:0".into(),
        database: String::new(),
        authority_dir: std::path::PathBuf::new(),
        silence_bound_secs: 60,
        link_address: None,
        server_name: "weaver-web".into(),
        enrollment_token_hours: 24,
        origin: None,
        tls_certificate: None,
        tls_key: None,
        rp_id: None,
        session_idle_secs: 3600,
        session_absolute_secs: 43200,
        operations_log: None,
        sign_in: sign_in.to_vec(),
    }
}

/// A person, enabled and holding nothing.
async fn person(s: &Store, name: &str) -> String {
    sqlx::query_scalar("INSERT INTO person (name, name_key) VALUES ($1, $2) RETURNING person_id")
        .bind(name)
        .bind(identity::name_key(name))
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The host's sign-in token for `person`, its value as printed.
async fn token_for(s: &Store, person: &str) -> String {
    let answer = host::sign_in_token(s, &cfg(BOTH), person, None, Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    answer.value["token"].as_str().unwrap().to_owned()
}

/// A request from the server's own origin, a form's body and a cookie where
/// given.
async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<String>,
    session: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN);
    if let Some(session) = session {
        request = request.header(header::COOKIE, format!("__Host-weaver_session={session}"));
    }
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

/// The token form's post.
async fn post_token(
    app: &axum::Router,
    name: &str,
    token: &str,
) -> (StatusCode, HeaderMap, String) {
    send(
        app,
        "POST",
        "/sign-in/token",
        Some(form(&[("name", name), ("token", token)])),
        None,
    )
    .await
}

fn location(headers: &HeaderMap) -> &str {
    headers.get(header::LOCATION).unwrap().to_str().unwrap()
}

/// **A server configured as before issues no sign-in token and serves no
/// route for one** (Spec 2.13): the host's command refuses before any
/// record, the token's post is no route, and the sign-in page carries no
/// token pane; with `token` named, the page carries it.
#[tokio::test]
async fn a_server_configured_as_before_has_no_sign_in_token() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;

    let answer = host::sign_in_token(s, &cfg(PASSKEY), &ada, None, Some("lab")).await;
    assert!(!answer.ok, "{}", answer.value);
    let error = answer.value["error"].as_str().unwrap();
    assert!(
        error.contains("does not admit the sign-in token"),
        "{error}"
    );
    assert_eq!(rows(s, "SELECT count(*) FROM sign_in_token").await, 0);
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action = 'person sign-in-token'"
        )
        .await,
        0,
        "refused before any record"
    );

    let before = app(s, PASSKEY);
    let (status, _, _) = post_token(&before, "ada", &"0".repeat(64)).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "no route for the token");
    let (status, _, page) = send(&before, "GET", "/sign-in", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !page.contains("/sign-in/token") && !page.contains("sign-in token"),
        "{page}"
    );
    assert!(
        page.contains("Give your name, then answer your passkey.</p>"),
        "{page}"
    );

    let (status, _, page) = send(&app(s, BOTH), "GET", "/sign-in", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"action="/sign-in/token""#), "{page}");
    assert!(
        page.contains(r#"id="sign-in""#),
        "the passkey's pane stands beside it: {page}"
    );
    crate::surfaces::assert_framed(&page);

    let (status, _, page) = send(
        &app(s, &[SignInMethod::Token]),
        "GET",
        "/sign-in",
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains(r#"action="/sign-in/token""#), "{page}");
    assert!(
        !page.contains(r#"id="sign-in""#) && !page.contains("passkeys.js"),
        "no passkey's pane where the config admits the token alone: {page}"
    );
}

/// **The host issues a sign-in token for an enabled person**, ten minutes by
/// default and never past an hour, kept as its digest, the person's one live
/// token, audited as the host's; a disabled or unknown person is refused.
#[tokio::test]
async fn the_host_issues_a_short_lived_token_kept_as_its_digest() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;

    let answer = host::sign_in_token(s, &cfg(BOTH), "ada", None, Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(answer.value["person"], ada.as_str());
    let first = answer.value["token"].as_str().unwrap().to_owned();
    assert_eq!(first.len(), 64);
    let lived: f64 = sqlx::query_scalar(
        "SELECT extract(epoch FROM expires_at - issued_at)::float8 FROM sign_in_token \
         WHERE token_digest = $1",
    )
    .bind(identity::digest(&first))
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert!(
        (599.0..=601.0).contains(&lived),
        "ten minutes by default: {lived}"
    );
    assert_eq!(
        rows(
            s,
            &format!("SELECT count(*) FROM sign_in_token WHERE token_digest = '{first}'")
        )
        .await,
        0,
        "the token is not stored in the clear"
    );
    let audited: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE answers IS NULL), count(*) FILTER (WHERE outcome = 'ok') \
         FROM audit WHERE principal = 'host' AND action = 'person sign-in-token' \
         AND target_kind = 'person' AND target_id = $1",
    )
    .bind(&ada)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(audited, (1, 1));

    let second = host::sign_in_token(s, &cfg(BOTH), &ada, Some(60), Some("lab")).await;
    assert!(second.ok, "an hour is the longest: {}", second.value);
    let ended: Option<String> =
        sqlx::query_scalar("SELECT ended FROM sign_in_token WHERE token_digest = $1")
            .bind(identity::digest(&first))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(ended.as_deref(), Some("superseded"));
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM sign_in_token WHERE ended_at IS NULL"
        )
        .await,
        1
    );

    let records = rows(s, "SELECT count(*) FROM audit").await;
    for minutes in [0, 61] {
        let answer = host::sign_in_token(s, &cfg(BOTH), &ada, Some(minutes), Some("lab")).await;
        assert!(!answer.ok, "{minutes}: {}", answer.value);
    }
    let answer = host::sign_in_token(s, &cfg(BOTH), "nobody", None, Some("lab")).await;
    assert!(!answer.ok, "{}", answer.value);
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        records,
        "refused before any record"
    );

    sqlx::query("UPDATE person SET enabled = false WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    let answer = host::sign_in_token(s, &cfg(BOTH), &ada, None, Some("lab")).await;
    assert!(!answer.ok, "{}", answer.value);
    assert!(answer.value["error"].as_str().unwrap().contains("disabled"));
}

/// **A token signs its person in once** (Spec 2.8): the name in any case,
/// the browser sent to Record with the cookie, the session naming its
/// method and no passkey, the opening audited by `sign-in token` on the
/// person with its outcome, the token ended `redeemed`; the same token again
/// gets the one answer and no record.
#[tokio::test]
async fn a_token_signs_its_person_in_once() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, BOTH);
    let ada = person(s, "Ada").await;
    let token = token_for(s, &ada).await;

    let (status, headers, body) = post_token(&app, " ADA ", &token).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(location(&headers), "/record");
    let session = bearer(&headers);
    let row: (String, Option<String>, String) = sqlx::query_as(
        "SELECT person_id, passkey_id, method FROM session WHERE bearer_digest = $1",
    )
    .bind(gate::digest(&session))
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(row, (ada.clone(), None, "sign-in token".to_owned()));
    let audited: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE answers IS NULL), count(*) FILTER (WHERE outcome = 'ok') \
         FROM audit WHERE principal = 'person' AND person_id = $1 AND method = 'sign-in token' \
         AND target_kind = 'person' AND target_id = $1 AND action = 'session open'",
    )
    .bind(&ada)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(audited, (1, 1));
    let ended: Option<String> =
        sqlx::query_scalar("SELECT ended FROM sign_in_token WHERE token_digest = $1")
            .bind(identity::digest(&token))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(ended.as_deref(), Some("redeemed"));

    for _ in 0..2 {
        let served = gate::at_use(s, &policy(), &session).await.unwrap().unwrap();
        assert_eq!(
            (served.person_id.as_str(), served.passkey_id),
            (ada.as_str(), None)
        );
    }
    let (status, _, page) = send(&app, "GET", "/record", None, Some(&session)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page.contains("Ada"), "{page}");

    let (status, _, body) = post_token(&app, "ada", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body.contains(NO_TOKEN), "{body}");
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 1);
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action = 'session open'"
        )
        .await,
        2,
        "the second post wrote no record"
    );
}

/// **One answer refuses every token that cannot sign in**, unaudited:
/// another person's name, a value of no token's shape, a token no one was
/// issued, an expired token and a disabled person's.
#[tokio::test]
async fn a_token_that_cannot_sign_in_gets_one_answer_and_no_record() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, BOTH);
    let ada = person(s, "ada").await;
    person(s, "bea").await;
    let token = token_for(s, &ada).await;

    let mut answers = Vec::new();
    for (name, value) in [
        ("bea", token.as_str()),
        ("ada", "not a token"),
        ("ada", &"ab".repeat(32)),
    ] {
        answers.push(post_token(&app, name, value).await);
    }
    sqlx::query(
        "UPDATE sign_in_token SET issued_at = now() - interval '20 minutes', \
         expires_at = now() - interval '10 minutes' WHERE person_id = $1",
    )
    .bind(&ada)
    .execute(&s.pool)
    .await
    .unwrap();
    answers.push(post_token(&app, "ada", &token).await);
    let fresh_token = token_for(s, &ada).await;
    sqlx::query("UPDATE person SET enabled = false WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    answers.push(post_token(&app, "ada", &fresh_token).await);

    for (status, headers, body) in &answers {
        assert_eq!(*status, StatusCode::FORBIDDEN, "{body}");
        assert!(headers.get(header::SET_COOKIE).is_none());
        assert!(body.contains(NO_TOKEN), "{body}");
    }
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 0);
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action = 'session open'"
        )
        .await,
        0
    );
}

/// **Of two redemptions of one token at once, one opens a session**: the
/// opening's conditional update matches a token still live.
#[tokio::test]
async fn two_redemptions_at_once_open_one_session() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, BOTH);
    let ada = person(s, "ada").await;
    let token = token_for(s, &ada).await;

    let (one, two) = tokio::join!(
        post_token(&app, "ada", &token),
        post_token(&app, "ada", &token)
    );
    let mut statuses = [one.0, two.0];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::SEE_OTHER, StatusCode::FORBIDDEN]);
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 1);
}

/// **A session a token opened has no passkey to lose** (Spec 2.8): a
/// passkey of its person's removed leaves it serving, it stands for a write,
/// and its person's disable ends it.
#[tokio::test]
async fn a_token_session_serves_and_ends_as_a_session_does() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, BOTH);
    let ada = person(s, "ada").await;
    sqlx::query(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('c-ada', $1, '{}')",
    )
    .bind(&ada)
    .execute(&s.pool)
    .await
    .unwrap();
    let token = token_for(s, &ada).await;
    let (status, headers, body) = post_token(&app, "ada", &token).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let session = bearer(&headers);

    let served = gate::at_use(s, &policy(), &session).await.unwrap().unwrap();
    sqlx::query("DELETE FROM passkey WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    assert!(
        gate::at_use(s, &policy(), &session).await.unwrap().is_ok(),
        "a passkey's removal leaves a token's session standing"
    );
    let mut tx = s.identity_transaction().await.unwrap();
    assert!(
        identity::session_stands(&mut tx, served.session_id, &ada)
            .await
            .unwrap(),
        "a token's session stands for a write"
    );
    tx.rollback().await.unwrap();

    sqlx::query("UPDATE person SET enabled = false WHERE person_id = $1")
        .bind(&ada)
        .execute(&s.pool)
        .await
        .unwrap();
    assert_eq!(
        gate::at_use(s, &policy(), &session).await.unwrap(),
        Err(Some(Ended::PersonDisabled))
    );
}

/// **A disable revokes the person's sign-in token** (Spec 2.13), so one
/// printed before it opens nothing after an enable.
#[tokio::test]
async fn a_disable_revokes_the_sign_in_token() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, BOTH);
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let bea = person(s, "bea").await;
    let token = token_for(s, &bea).await;

    for write in ["disable", "enable"] {
        let v = version(s, &bea).await;
        let (status, _, answer) = send_as(
            &app,
            "POST",
            &format!("/admin/persons/{write}"),
            Some(form(&[("person", &bea), ("version", &v)])),
            &session,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{write}: {answer}");
        if write == "disable" {
            let ended: Option<String> =
                sqlx::query_scalar("SELECT ended FROM sign_in_token WHERE token_digest = $1")
                    .bind(identity::digest(&token))
                    .fetch_one(&s.pool)
                    .await
                    .unwrap();
            assert_eq!(ended.as_deref(), Some("revoked"));
        }
    }
    let (status, _, body) = post_token(&app, "bea", &token).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

/// **The store refuses a session or a sign-in token out of shape**, each
/// insert in a transaction of its own and rolled back: a passkey's method
/// naming no passkey, a token's naming one, a method no one knows; a token
/// past an hour, ended without a reason or for one no one knows, or not a
/// digest. A token of exactly an hour lands.
#[tokio::test]
async fn the_store_refuses_a_session_or_token_out_of_shape() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let ada = person(s, "ada").await;
    let passkey: String = sqlx::query_scalar(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('c-ada', $1, '{}') \
         RETURNING passkey_id",
    )
    .bind(&ada)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    let digest = identity::digest("a token");

    let session =
        "INSERT INTO session (bearer_digest, person_id, passkey_id, method) VALUES ($1, $2, $3, ";
    for (method, with_passkey, lands) in [
        ("'passkey assertion')", true, true),
        ("'passkey assertion')", false, false),
        ("'sign-in token')", false, true),
        ("'sign-in token')", true, false),
        ("'password')", false, false),
    ] {
        let passkey = with_passkey.then_some(passkey.as_str());
        let statement = format!("{session}{method}");
        assert_eq!(
            landed(s, &statement, &digest, &ada, passkey).await,
            lands,
            "{method} {with_passkey}"
        );
    }
    for (statement, lands) in [
        (
            "INSERT INTO sign_in_token (token_digest, person_id, expires_at) \
             VALUES ($1, $2, now() + interval '1 hour')",
            true,
        ),
        (
            "INSERT INTO sign_in_token (token_digest, person_id, expires_at) \
             VALUES ($1, $2, now() + interval '61 minutes')",
            false,
        ),
        (
            "INSERT INTO sign_in_token (token_digest, person_id, expires_at, ended_at) \
             VALUES ($1, $2, now() + interval '1 minute', now())",
            false,
        ),
        (
            "INSERT INTO sign_in_token (token_digest, person_id, expires_at, ended_at, ended) \
             VALUES ($1, $2, now() + interval '1 minute', now(), 'used')",
            false,
        ),
        (
            "INSERT INTO sign_in_token (token_digest, person_id, expires_at) \
             VALUES (upper($1), $2, now() + interval '1 minute')",
            false,
        ),
    ] {
        assert_eq!(
            landed(s, statement, &digest, &ada, None).await,
            lands,
            "{statement}"
        );
    }
}

/// Whether `statement` lands, binding the digest, the person and, where it
/// takes a third, the passkey; in a transaction of its own, rolled back.
async fn landed(
    s: &Store,
    statement: &str,
    digest: &str,
    person: &str,
    passkey: Option<&str>,
) -> bool {
    let mut tx = s.pool.begin().await.unwrap();
    let mut query = sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
        .bind(digest)
        .bind(person);
    if statement.contains("$3") {
        query = query.bind(passkey);
    }
    let landed = query.execute(&mut *tx).await.is_ok();
    tx.rollback().await.ok();
    landed
}

/// **`GET /` sends a browser on** (Spec 2.8): to sign-in with no cookie or
/// one naming no session, to Record with a live session.
#[tokio::test]
async fn the_root_sends_a_browser_on() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, BOTH);
    for session in [None, Some("no-such-session")] {
        let (status, headers, _) = send(&app, "GET", "/", None, session).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{session:?}");
        assert_eq!(location(&headers), "/sign-in", "{session:?}");
    }
    let ada = person(s, "ada").await;
    let token = token_for(s, &ada).await;
    let (_, headers, _) = post_token(&app, "ada", &token).await;
    let (status, headers, _) = send(&app, "GET", "/", None, Some(&bearer(&headers))).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(location(&headers), "/record");
}
