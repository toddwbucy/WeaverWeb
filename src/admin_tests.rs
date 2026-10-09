//! An admin's writes to persons (`src/surfaces/admin.rs`,
//! `src/store/admin.rs`; Spec 2.13): the page and every write refused
//! without a live admin grant, each write end to end with its audit pair,
//! each refusal, the authority re-checked inside the write's transaction,
//! two admins disabling each other at once, and a disable revoking the
//! outstanding token. Each test runs on a database of its own, since the
//! last-admin rule is store-wide. No name, token or digest is a real one.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde_json::json;
use tower::ServiceExt;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;

use crate::host;
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys, person_with_token, send};
use crate::sign_in_tests::{bearer, enrolled, rows, sign_in};
use crate::store::Store;
use crate::store::admin::{INSIDE_HOLD, Refusal};
use crate::store::identity;
use crate::store::read::tests::fresh_store;
use crate::surfaces::admin::READ_HOLD;

/// A request under a session's cookie, a form's body or none.
async fn send_as(
    app: &axum::Router,
    method: &str,
    uri: &str,
    form: Option<String>,
    session: &str,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, format!("__Host-weaver_session={session}"));
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

/// A form's body, its values encoded.
fn form(fields: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields)
        .finish()
}

/// A person enrolled with `key` and signed in: their identity and the
/// session's bearer.
async fn signed_in(
    app: &axum::Router,
    s: &Store,
    key: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> (String, String) {
    let (person, _) = enrolled(app, s, key, name).await;
    let (status, headers, answer) = sign_in(app, key, name).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    (person, bearer(&headers))
}

/// A person signed in and holding the admin grant, the host's.
async fn admin(
    app: &axum::Router,
    s: &Store,
    key: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> (String, String) {
    let (person, session) = signed_in(app, s, key, name).await;
    let granted = host::grant_add(s, &person, "admin", None, None).await;
    assert!(granted.ok, "{}", granted.value);
    (person, session)
}

async fn version(s: &Store, person: &str) -> String {
    let version: i64 = sqlx::query_scalar("SELECT version FROM person WHERE person_id = $1")
        .bind(person)
        .fetch_one(&s.pool)
        .await
        .unwrap();
    version.to_string()
}

async fn enabled(s: &Store, person: &str) -> bool {
    sqlx::query_scalar("SELECT enabled FROM person WHERE person_id = $1")
        .bind(person)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The admin's audit records of `action` on `target`: the first records,
/// the `ok` outcomes, and the `failed` ones, each by `session`.
async fn audited(s: &Store, admin: &str, target: &str, action: &str) -> (i64, i64, i64) {
    let count = |clause: &'static str| {
        let query = format!(
            "SELECT count(*) FROM audit WHERE principal = 'person' AND person_id = $1 \
             AND method = 'session' AND target_kind = 'person' AND target_id = $2 \
             AND action = $3 AND {clause}"
        );
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
                .bind(admin)
                .bind(target)
                .bind(action)
                .fetch_one(&s.pool)
                .await
                .unwrap()
        }
    };
    (
        count("answers IS NULL AND refusal IS NULL").await,
        count("outcome = 'ok'").await,
        count("outcome = 'failed'").await,
    )
}

/// The token a shown-once answer carries.
fn token_in(page: &str) -> String {
    let at = page.find("<code>").unwrap() + "<code>".len();
    page[at..at + 64].to_owned()
}

/// A hold's two signals, registered under `key` in `holds`.
fn hold(
    holds: &std::sync::Mutex<Vec<crate::store::admin::AdminHold>>,
    key: &str,
) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let (read, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    holds
        .lock()
        .unwrap()
        .push((key.to_owned(), read.clone(), release.clone()));
    (read, release)
}

/// **A session whose person holds no live admin grant is refused at the
/// page and at every write**, each write's refusal one audit record
/// carrying it and nothing written; a request with no session is asked to
/// sign in.
#[tokio::test]
async fn a_session_without_the_admin_grant_is_refused_at_the_page_and_every_write() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, _) = signed_in(&app, s, &mut authenticator(), "ada").await;
    let (bea, session) = signed_in(&app, s, &mut authenticator(), "bea").await;
    // Bea held the admin grant and the host revoked it: a grant that is not
    // live authorizes nothing.
    let granted = host::grant_add(s, &bea, "admin", None, None).await;
    assert!(granted.ok);
    let bootstrap = host::grant_add(s, &ada, "admin", None, None).await;
    assert!(bootstrap.ok);
    let removed = host::grant_remove(s, &bea, "admin", None, None).await;
    assert!(removed.ok, "{}", removed.value);

    let (status, _, page) = send_as(&app, "GET", "/admin/persons", None, &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{page}");
    assert!(!page.contains("ada"), "the page names nobody: {page}");
    let v = version(s, &ada).await;
    let writes = [
        ("/admin/persons/enroll", form(&[("name", "cara")])),
        ("/admin/persons/token", form(&[("person", &ada)])),
        (
            "/admin/persons/disable",
            form(&[("person", &ada), ("version", &v)]),
        ),
        (
            "/admin/persons/enable",
            form(&[("person", &ada), ("version", &v)]),
        ),
        (
            "/admin/persons/rename",
            form(&[("person", &ada), ("version", &v), ("name", "abe")]),
        ),
    ];
    for (uri, body) in &writes {
        let (status, _, answer) = send_as(&app, "POST", uri, Some(body.clone()), &session).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {answer}");
    }
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE refusal = 'not an admin'"
        )
        .await,
        5,
        "each write's refusal is one record"
    );
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action LIKE 'person %' AND refusal IS NULL"
        )
        .await,
        0,
        "no write began"
    );
    assert_eq!(
        rows(s, "SELECT count(*) FROM person").await,
        2,
        "nobody enrolled"
    );
    assert!(enabled(s, &ada).await);

    let (status, _, _) = send(&app, "GET", "/admin/persons", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/admin/persons/enroll",
        Some(form(&[("name", "cara")])),
        "no-such-session",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// **The page lists every person**, enabled or not, whether they hold a
/// passkey and whether a token is outstanding, and offers no write on the
/// admin's own row; the navigation reaches it for an admin alone.
#[tokio::test]
async fn the_page_lists_every_person_and_no_write_on_ones_own_row() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (_, observer) = signed_in(&app, s, &mut authenticator(), "dot").await;
    let (cara, _) = person_with_token(s, "cara").await;
    let v = version(s, &cara).await;
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/admin/persons/disable",
        Some(form(&[("person", &cara), ("version", &v)])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, eve_token) = person_with_token(s, "eve").await;
    assert!(!eve_token.is_empty());

    let (status, headers, page) = send_as(&app, "GET", "/admin/persons", None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        headers[header::CONTENT_SECURITY_POLICY],
        "script-src 'self'"
    );
    for name in ["ada", "cara", "dot", "eve"] {
        assert!(
            page.contains(&format!("<td>{name}")),
            "{name} listed: {page}"
        );
    }
    assert!(page.contains("disabled"), "cara disabled: {page}");
    assert!(page.contains("outstanding"), "eve's token: {page}");
    assert!(page.contains("held"), "a passkey held: {page}");
    assert_eq!(
        page.matches(&format!("name=\"person\" value=\"{ada}\""))
            .count(),
        0,
        "no write on one's own row"
    );
    assert!(
        page.contains("href=\"/admin/persons\""),
        "the navigation reaches it"
    );
    let (_, _, passkeys_page) = send_as(&app, "GET", "/passkeys", None, &session).await;
    assert!(passkeys_page.contains("href=\"/admin/persons\""));
    let (_, _, theirs) = send_as(&app, "GET", "/passkeys", None, &observer).await;
    assert!(
        !theirs.contains("href=\"/admin/persons\""),
        "hidden from a non-admin"
    );
}

/// **Each write lands with its audit pair**: an enrollment and its token
/// shown once under `no-store` and held only as a digest, a token issued
/// superseding the first, a rename, a disable and an enable, each a first
/// record before and an `ok` outcome after, the admin by `session` as the
/// principal; no audit record holds the token.
#[tokio::test]
async fn each_write_lands_with_its_audit_pair() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;

    let (status, headers, shown) = send_as(
        &app,
        "POST",
        "/admin/persons/enroll",
        Some(form(&[("name", " bea ")])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(headers.get(header::LOCATION).is_none(), "never in a URL");
    let first_token = token_in(&shown);
    let bea = s.person("bea").await.unwrap().unwrap().person_id;
    assert_eq!(audited(s, &ada, &bea, "person enroll").await, (1, 1, 0));
    let author: Option<String> =
        sqlx::query_scalar("SELECT author FROM person WHERE person_id = $1")
            .bind(&bea)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(
        author.as_deref(),
        Some(ada.as_str()),
        "the admin authors the row"
    );
    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM enrollment_token WHERE person_id = $1 AND token_digest = $2 \
         AND ended_at IS NULL",
    )
    .bind(&bea)
    .bind(identity::digest(&first_token))
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(held, 1, "the token held as its digest");

    let (status, headers, shown) = send_as(
        &app,
        "POST",
        "/admin/persons/token",
        Some(form(&[("person", &bea)])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{shown}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    let second_token = token_in(&shown);
    assert_ne!(first_token, second_token);
    assert_eq!(audited(s, &ada, &bea, "person token").await, (1, 1, 0));
    let superseded: Option<String> =
        sqlx::query_scalar("SELECT ended FROM enrollment_token WHERE token_digest = $1")
            .bind(identity::digest(&first_token))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(superseded.as_deref(), Some("superseded"));

    let v = version(s, &bea).await;
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/admin/persons/rename",
        Some(form(&[("person", &bea), ("version", &v), ("name", "bee")])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(s.person(&bea).await.unwrap().unwrap().name, "bee");
    assert_eq!(audited(s, &ada, &bea, "person rename").await, (1, 1, 0));

    let v = version(s, &bea).await;
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/admin/persons/disable",
        Some(form(&[("person", &bea), ("version", &v)])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(!enabled(s, &bea).await);
    assert_eq!(audited(s, &ada, &bea, "person disable").await, (1, 1, 0));

    let v = version(s, &bea).await;
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/admin/persons/enable",
        Some(form(&[("person", &bea), ("version", &v)])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(enabled(s, &bea).await);
    assert_eq!(audited(s, &ada, &bea, "person enable").await, (1, 1, 0));

    for token in [&first_token, &second_token] {
        let holding: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit a WHERE strpos(row_to_json(a)::text, $1) > 0 \
             OR strpos(row_to_json(a)::text, $2) > 0",
        )
        .bind(token)
        .bind(identity::digest(token))
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert_eq!(holding, 0, "no record holds a token or its digest");
    }
}

/// **A disable revokes the outstanding token in the same write**: the
/// token ends `revoked`, and redeeming it is refused, even once the person
/// is enabled again.
#[tokio::test]
async fn a_disable_revokes_the_outstanding_token() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, token) = person_with_token(s, "bea").await;

    let v = version(s, &bea).await;
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/admin/persons/disable",
        Some(form(&[("person", &bea), ("version", &v)])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    let ended: Option<String> =
        sqlx::query_scalar("SELECT ended FROM enrollment_token WHERE token_digest = $1")
            .bind(identity::digest(&token))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(ended.as_deref(), Some("revoked"));

    let v = version(s, &bea).await;
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/admin/persons/enable",
        Some(form(&[("person", &bea), ("version", &v)])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, answer) = send(
        &app,
        "POST",
        "/enroll/options",
        Some(json!({ "token": token })),
    )
    .await;
    assert_ne!(
        status,
        StatusCode::OK,
        "a revoked token redeems nothing: {answer}"
    );
}

/// **Each refusal, nothing written**: a taken name in case and width
/// variants at enrollment and at rename, an identity-shaped name, a token
/// for a person holding a passkey, a stale version, and an admin's write to
/// their own row, which is refused before any record.
#[tokio::test]
async fn each_refusal_writes_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, _) = signed_in(&app, s, &mut authenticator(), "bea").await;
    let post = |uri: &'static str, body: String| {
        let (app, session) = (app.clone(), session.clone());
        async move { send_as(&app, "POST", uri, Some(body), &session).await }
    };

    for taken in ["ADA", "\u{FF41}\u{FF44}\u{FF41}", " Bea "] {
        let (status, _, answer) = post("/admin/persons/enroll", form(&[("name", taken)])).await;
        assert_eq!(status, StatusCode::CONFLICT, "{taken}: {answer}");
    }
    assert_eq!(rows(s, "SELECT count(*) FROM person").await, 2);
    let v = version(s, &bea).await;
    for taken in ["ADA", "\u{FF41}\u{FF44}\u{FF41}"] {
        let (status, _, answer) = post(
            "/admin/persons/rename",
            form(&[("person", &bea), ("version", &v), ("name", taken)]),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{taken}: {answer}");
    }
    let shaped = "pe-0123456789abcdef";
    let (status, _, _) = post("/admin/persons/enroll", form(&[("name", shaped)])).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = post(
        "/admin/persons/rename",
        form(&[("person", &bea), ("version", &v), ("name", shaped)]),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    // Renaming a person to their own name in another case is no conflict.
    let (status, _, answer) = post(
        "/admin/persons/rename",
        form(&[("person", &bea), ("version", &v), ("name", "Bea")]),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    let (status, _, _) = post(
        "/admin/persons/rename",
        form(&[("person", &bea), ("version", &v), ("name", "bee")]),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale version");
    assert_eq!(s.person(&bea).await.unwrap().unwrap().name, "Bea");

    let (status, _, answer) = post("/admin/persons/token", form(&[("person", &bea)])).await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM enrollment_token WHERE ended_at IS NULL"
        )
        .await,
        0,
        "no token for a passkey holder"
    );

    let before = rows(s, "SELECT count(*) FROM audit").await;
    let v = version(s, &ada).await;
    for (uri, body) in [
        (
            "/admin/persons/disable",
            form(&[("person", &ada), ("version", &v)]),
        ),
        (
            "/admin/persons/enable",
            form(&[("person", &ada), ("version", &v)]),
        ),
        (
            "/admin/persons/rename",
            form(&[("person", &ada), ("version", &v), ("name", "abe")]),
        ),
        ("/admin/persons/token", form(&[("person", &ada)])),
    ] {
        let (status, _, answer) = post(uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {answer}");
    }
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before,
        "before any record"
    );
    assert!(enabled(s, &ada).await);
    assert_eq!(s.person(&ada).await.unwrap().unwrap().name, "ada");
}

/// **The last enabled admin is never disabled**: the store refuses it.
/// Through the surface the write cannot be formed, since the admin asking
/// is enabled and holds the grant, and an admin's own row is refused before
/// the store; the store's rule stands beneath both, so this asks the store
/// directly with the admin's own row.
#[tokio::test]
async fn the_last_enabled_admin_is_never_disabled() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let session_id: i64 =
        sqlx::query_scalar("SELECT session_id FROM session WHERE bearer_digest = $1")
            .bind(crate::surfaces::gate::digest(&session))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    let v: i64 = version(s, &ada).await.parse().unwrap();
    assert_eq!(
        s.admin_disable(&ada, session_id, &ada, v).await.unwrap(),
        Err(Refusal::LastAdmin)
    );
    assert!(enabled(s, &ada).await);
}

/// **The authority is re-checked inside the write's transaction**: an
/// admin whose grant the host revokes between the surface's read and the
/// write is refused as its authority gone, audited as failed, and nothing
/// lands.
#[tokio::test]
async fn a_write_whose_admin_grant_was_revoked_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, _) = admin(&app, s, &mut authenticator(), "bea").await;
    let (cara, _) = person_with_token(s, "cara").await;
    let v = version(s, &cara).await;

    let (read, release) = hold(&READ_HOLD, &ada);
    let disable = tokio::spawn({
        let (app, session, body) = (
            app.clone(),
            session.clone(),
            form(&[("person", &cara), ("version", &v)]),
        );
        async move { send_as(&app, "POST", "/admin/persons/disable", Some(body), &session).await }
    });
    read.notified().await;
    let removed = host::grant_remove(s, &ada, "admin", None, None).await;
    assert!(removed.ok, "{}", removed.value);
    release.notify_one();
    let (status, _, answer) = disable.await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(answer.contains("no longer stands"), "{answer}");
    assert!(enabled(s, &cara).await, "nothing landed");
    assert_eq!(audited(s, &ada, &cara, "person disable").await, (1, 0, 1));
    assert!(s.is_admin(&bea).await.unwrap());
}

/// **Two admins disabling each other at once leave one**: the identity
/// exclusion serializes the two writes, and the second re-checks its
/// authority after the first commits, finding its own person disabled.
#[tokio::test]
async fn two_admins_disabling_each_other_leave_one() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, ada_session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, bea_session) = admin(&app, s, &mut authenticator(), "bea").await;
    let (va, vb) = (version(s, &ada).await, version(s, &bea).await);

    let (inside, release) = hold(&INSIDE_HOLD, &ada);
    let first = tokio::spawn({
        let (app, body) = (app.clone(), form(&[("person", &bea), ("version", &vb)]));
        async move {
            send_as(
                &app,
                "POST",
                "/admin/persons/disable",
                Some(body),
                &ada_session,
            )
            .await
        }
    });
    inside.notified().await;
    let second = tokio::spawn({
        let (app, body) = (app.clone(), form(&[("person", &ada), ("version", &va)]));
        async move {
            send_as(
                &app,
                "POST",
                "/admin/persons/disable",
                Some(body),
                &bea_session,
            )
            .await
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    release.notify_one();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(first.0, StatusCode::SEE_OTHER, "{}", first.2);
    assert_eq!(second.0, StatusCode::FORBIDDEN, "{}", second.2);
    assert!(enabled(s, &ada).await, "one admin remains");
    assert!(!enabled(s, &bea).await);
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM person p WHERE p.enabled AND EXISTS (SELECT 1 FROM role_grant g \
             WHERE g.person_id = p.person_id AND g.role = 'admin' AND g.revoked_at IS NULL)"
        )
        .await,
        1
    );
}

/// **A submitted identity is parsed into its exact shape before any
/// record**: a malformed person at each admin write, from an admin and from
/// a session holding no grant, and a malformed passkey at the removal, are
/// refused as the ask's fault with no audit row written, where the audit's
/// target check would have made each the server's failure; a well-shaped
/// person naming nobody goes on to the store's answer.
#[tokio::test]
async fn a_malformed_identity_is_refused_before_any_record() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (_, other) = signed_in(&app, s, &mut authenticator(), "bea").await;
    let before = rows(s, "SELECT count(*) FROM audit").await;
    for person in [
        "bea",
        "pe-0123456789ABCDEF",
        "pe-0123",
        "pe-0123456789abcdef0",
        "",
    ] {
        for (uri, body) in [
            ("/admin/persons/token", form(&[("person", person)])),
            (
                "/admin/persons/disable",
                form(&[("person", person), ("version", "1")]),
            ),
            (
                "/admin/persons/enable",
                form(&[("person", person), ("version", "1")]),
            ),
            (
                "/admin/persons/rename",
                form(&[("person", person), ("version", "1"), ("name", "cara")]),
            ),
        ] {
            for asking in [&session, &other] {
                let (status, _, answer) =
                    send_as(&app, "POST", uri, Some(body.clone()), asking).await;
                assert_eq!(
                    status,
                    StatusCode::BAD_REQUEST,
                    "{uri} {person:?}: {answer}"
                );
            }
        }
    }
    for passkey in ["pk-0123", "pe-0123456789abcdef", "label"] {
        let (status, _, answer) = send_as(
            &app,
            "POST",
            "/passkeys/remove",
            Some(form(&[("passkey", passkey)])),
            &session,
        )
        .await;
        assert_eq!(
            rows(s, "SELECT count(*) FROM audit").await,
            before,
            "{passkey}: a malformed passkey lands no audit row"
        );
        assert_eq!(status, StatusCode::BAD_REQUEST, "{passkey}: {answer}");
    }
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before,
        "no record of a malformed ask"
    );

    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/admin/persons/disable",
        Some(form(&[("person", "pe-0123456789abcdef"), ("version", "1")])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
}
