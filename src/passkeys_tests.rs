//! Enrollment by token (`src/surfaces/enroll.rs`, `src/passkeys.rs`; design
//! sections 4, 6 and 7): a token's redemption end to end, each refusal, the
//! race the transaction's re-check answers, the ceremony table, and the
//! browser module and page. The authenticator is `webauthn-authenticator-rs`'s
//! soft passkey, a dev-dependency; each test runs on a database of its own.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_rs::prelude::{CreationChallengeResponse, Url, Uuid, WebauthnBuilder};

use crate::passkeys::{
    CEREMONIES_IN_FLIGHT, CEREMONY_LIFETIME, Ceremonies, Ceremony, Full, Passkeys,
};
use crate::store::Store;
use crate::store::identity::{self, Supersedes};
use crate::store::read::tests::fresh_store;
use crate::surfaces::{enroll, gate};

/// The origin and relying party, reserved test names.
pub(crate) const ORIGIN: &str = "https://weaver.test";

pub(crate) fn passkeys() -> Passkeys {
    let webauthn = WebauthnBuilder::new("weaver.test", &Url::parse(ORIGIN).unwrap())
        .unwrap()
        .build()
        .unwrap();
    Passkeys {
        webauthn: Arc::new(webauthn),
        ceremonies: Arc::new(Ceremonies::default()),
    }
}

/// The enrollment surface and the others, as the server mounts them: under
/// the script policy and the `Origin` check.
pub(crate) fn app(s: &Store, passkeys: Option<Passkeys>) -> axum::Router {
    let policy = gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
    };
    gate::guard(
        crate::surfaces::routes(policy.clone())
            .merge(crate::surfaces::script_policy(enroll::routes(
                passkeys.clone(),
            )))
            .merge(crate::surfaces::script_policy(
                crate::surfaces::sign_in::routes(passkeys.clone()),
            ))
            .merge(crate::surfaces::script_policy(
                crate::surfaces::admin::routes(policy.clone(), 24),
            ))
            .merge(match passkeys {
                Some(passkeys) => crate::surfaces::script_policy(crate::surfaces::keys::routes(
                    policy.clone(),
                    passkeys,
                )),
                None => axum::Router::new(),
            })
            .with_state(s.clone()),
        policy,
    )
}

/// A person and an enrollment token issued for them, the token's value
/// as the host printed it.
pub(crate) async fn person_with_token(s: &Store, name: &str) -> (String, String) {
    let person: String = sqlx::query_scalar(
        "INSERT INTO person (name, name_key) VALUES ($1, $2) RETURNING person_id",
    )
    .bind(name)
    .bind(identity::name_key(name))
    .fetch_one(&s.pool)
    .await
    .unwrap();
    let mut tx = s.identity_transaction().await.unwrap();
    let issued = identity::issue_token(&mut tx, &person, 24, Supersedes::Issue)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (person, issued.value)
}

pub(crate) async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
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

/// The authenticator's answer to the options the server gave: the
/// ceremony's identity and the credential, as the browser module sends it
/// (its members and no extension output).
pub(crate) fn register(
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    options: &str,
) -> (String, Value) {
    let options: Value = serde_json::from_str(options).unwrap();
    let ceremony = options["ceremony"].as_str().unwrap().to_owned();
    let challenge: CreationChallengeResponse =
        serde_json::from_value(options["options"].clone()).unwrap();
    let registered = authenticator
        .do_registration(Url::parse(ORIGIN).unwrap(), challenge)
        .unwrap();
    let mut credential = serde_json::to_value(&registered).unwrap();
    credential.as_object_mut().unwrap().remove("extensions");
    (ceremony, credential)
}

pub(crate) fn authenticator() -> WebauthnAuthenticator<SoftPasskey> {
    WebauthnAuthenticator::new(SoftPasskey::new(true))
}

/// An audit record's principal, person, method, target, action and outcome.
type AuditRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    String,
    Option<String>,
);

pub(crate) async fn count(s: &Store, query: &str, bind: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(query.to_owned()))
        .bind(bind)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **A token registers its person's first passkey and opens no session**
/// (Spec 2.13, design section 7): the options and the finish answer, the
/// passkey stands under a `pk-` identity with the credential's ID, the token
/// ends `redeemed`, no cookie is set and no session row exists, and the
/// audit holds the first record (the person, by enrollment token, on the
/// passkey) and its outcome. The ceremony is used once.
#[tokio::test]
async fn a_token_registers_one_passkey_and_opens_no_session() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (person, token) = person_with_token(s, "ada").await;

    let (status, headers, options) = send(
        &app,
        "POST",
        "/enroll/options",
        Some(json!({ "token": token })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{options}");
    assert!(
        headers.get(header::SET_COOKIE).is_none(),
        "no cookie at the options"
    );
    let (ceremony, credential) = register(&mut authenticator(), &options);
    let finish = json!({ "ceremony": ceremony, "credential": credential });
    let (status, headers, answer) =
        send(&app, "POST", "/enroll/finish", Some(finish.clone())).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    assert!(
        headers.get(header::SET_COOKIE).is_none(),
        "no cookie at the finish"
    );

    let (passkey_id, credential_id): (String, String) =
        sqlx::query_as("SELECT passkey_id, credential_id FROM passkey WHERE person_id = $1")
            .bind(&person)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert!(
        passkey_id.len() == 19
            && passkey_id.starts_with("pk-")
            && passkey_id[3..].bytes().all(|b| b.is_ascii_hexdigit()),
        "{passkey_id}"
    );
    assert_eq!(Some(credential_id.as_str()), credential["rawId"].as_str());
    let ended: Option<String> =
        sqlx::query_scalar("SELECT ended FROM enrollment_token WHERE person_id = $1")
            .bind(&person)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(ended.as_deref(), Some("redeemed"));
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM session WHERE person_id = $1",
            &person
        )
        .await,
        0
    );

    let records: Vec<AuditRow> = sqlx::query_as(
        "SELECT principal, person_id, method, target_id, action, outcome FROM audit \
             WHERE target_kind = 'passkey' ORDER BY answers NULLS FIRST",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    let first = (
        "person".to_owned(),
        Some(person.clone()),
        "enrollment token".to_owned(),
        Some(passkey_id.clone()),
        "passkey enroll".to_owned(),
    );
    assert_eq!(records.len(), 2, "{records:?}");
    for (i, record) in records.iter().enumerate() {
        assert_eq!(
            (
                record.0.clone(),
                record.1.clone(),
                record.2.clone(),
                record.3.clone(),
                record.4.clone()
            ),
            first
        );
        assert_eq!(record.5.as_deref(), [None, Some("ok")][i]);
    }

    let (status, _, _) = send(&app, "POST", "/enroll/finish", Some(finish)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a ceremony is used once");
}

/// **A token that cannot redeem starts no ceremony**: an expired one, a
/// used one, one whose person is disabled, one whose person already holds
/// a passkey, and an unknown one are each refused at the options with one
/// answer, and nothing is written or held.
#[tokio::test]
async fn a_token_that_cannot_redeem_starts_no_ceremony() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let keys = passkeys();
    let app = app(s, Some(keys.clone()));
    let mut tokens = vec!["no-such-token".to_owned()];
    for (name, change) in [
        (
            "expired",
            "UPDATE enrollment_token SET issued_at = now() - interval '2 days', \
             expires_at = now() - interval '1 day' WHERE person_id = $1",
        ),
        (
            "used",
            "UPDATE enrollment_token SET ended_at = now(), ended = 'redeemed' WHERE person_id = $1",
        ),
        (
            "disabled",
            "UPDATE person SET enabled = false, version = version + 1 WHERE person_id = $1",
        ),
        (
            "holder",
            "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('held-cred', $1, '{}')",
        ),
    ] {
        let (person, token) = person_with_token(s, name).await;
        sqlx::query(sqlx::AssertSqlSafe(change.to_owned()))
            .bind(&person)
            .execute(&s.pool)
            .await
            .unwrap();
        tokens.push(token);
    }
    for token in tokens {
        let (status, _, answer) = send(
            &app,
            "POST",
            "/enroll/options",
            Some(json!({ "token": token })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
        assert!(answer.contains("cannot enroll a passkey"), "{answer}");
    }
    assert_eq!(keys.ceremonies.held(), 0, "no ceremony was started");
    let passkeys: i64 = sqlx::query_scalar("SELECT count(*) FROM passkey")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert_eq!(passkeys, 1, "only the holder's own passkey stands");
}

/// **The transaction's re-check answers the race**: a passkey that appears
/// for the person between the options and the finish refuses the finish,
/// and the authenticator's passkey is not inserted, the token stays live,
/// and the outcome record says `failed`.
#[tokio::test]
async fn a_passkey_appearing_between_options_and_finish_refuses_the_finish() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (person, token) = person_with_token(s, "ada").await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/enroll/options",
        Some(json!({ "token": token })),
    )
    .await;
    let (ceremony, credential) = register(&mut authenticator(), &options);
    sqlx::query(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('raced', $1, '{}')",
    )
    .bind(&person)
    .execute(&s.pool)
    .await
    .unwrap();
    let (status, _, answer) = send(
        &app,
        "POST",
        "/enroll/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM passkey WHERE person_id = $1",
            &person
        )
        .await,
        1,
        "only the raced passkey stands"
    );
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM enrollment_token WHERE person_id = $1 AND ended_at IS NULL",
            &person
        )
        .await,
        1,
        "the token stays live"
    );
    let outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome FROM audit WHERE target_kind = 'passkey' AND answers IS NOT NULL",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(outcome.as_deref(), Some("failed"));
}

/// **A credential ID already held is refused**: the finish of a
/// registration whose credential ID another person's passkey holds inserts
/// nothing and leaves the token live.
#[tokio::test]
async fn a_credential_already_held_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (person, token) = person_with_token(s, "bea").await;
    let (other, _) = person_with_token(s, "carl").await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/enroll/options",
        Some(json!({ "token": token })),
    )
    .await;
    let (ceremony, credential) = register(&mut authenticator(), &options);
    sqlx::query("INSERT INTO passkey (credential_id, person_id, credential) VALUES ($1, $2, '{}')")
        .bind(credential["rawId"].as_str().unwrap())
        .bind(&other)
        .execute(&s.pool)
        .await
        .unwrap();
    let (status, _, answer) = send(
        &app,
        "POST",
        "/enroll/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM passkey WHERE person_id = $1",
            &person
        )
        .await,
        0
    );
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM enrollment_token WHERE person_id = $1 AND ended_at IS NULL",
            &person
        )
        .await,
        1,
        "the token stays live"
    );
}

/// **The ceremony table** (design section 6): identities are 32 random
/// bytes, distinct, and no two consecutive ones share a prefix, as a
/// bearer the server issues is drawn; the sixty-fifth ceremony in flight is refused; a
/// ceremony is taken once; one past five minutes is refused, on a paused
/// clock; and expired ceremonies free their places.
#[tokio::test(start_paused = true)]
async fn the_ceremony_table_caps_expires_and_is_used_once() {
    let keys = passkeys();
    let ceremony = || Ceremony::Enrollment {
        state: keys
            .webauthn
            .start_passkey_registration(Uuid::nil(), "a", "a", None)
            .unwrap()
            .1,
        person_id: "pe-0000000000000000".to_owned(),
        token_digest: "d".to_owned(),
    };
    let table = Ceremonies::default();
    let started: Vec<String> = (0..CEREMONIES_IN_FLIGHT)
        .map(|_| table.start(ceremony()).unwrap())
        .collect();
    assert!(
        started
            .iter()
            .all(|id| id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit()))
    );
    for pair in started.windows(2) {
        assert_ne!(
            pair[0][..16],
            pair[1][..16],
            "two ceremony identities share a prefix, drawn as no bearer is"
        );
    }
    let mut distinct = started.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), started.len(), "every identity is its own");
    assert!(
        matches!(table.start(ceremony()), Err(Full)),
        "the sixty-fifth is refused"
    );

    assert!(table.take(&started[0]).is_some());
    assert!(
        table.take(&started[0]).is_none(),
        "a ceremony is taken once"
    );
    table
        .start(ceremony())
        .expect("a finished ceremony frees its place");

    tokio::time::advance(CEREMONY_LIFETIME).await;
    assert!(
        table.take(&started[1]).is_none(),
        "a ceremony past five minutes is refused"
    );
    for _ in 0..CEREMONIES_IN_FLIGHT {
        table
            .start(ceremony())
            .expect("expired ceremonies free their places");
    }
}

/// **The module and its page** (design section 4): the module is served
/// as JavaScript under 4 KiB; the page and the module carry `script-src
/// 'self'`; the page's every script is a file, none inline; and its token
/// input has no name and its form posts, so nothing carries the token in a
/// URL. Without passkeys configured the page says so and loads no script.
#[tokio::test]
async fn the_module_is_small_and_the_page_has_no_inline_script() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (status, headers, module) = send(&app, "GET", "/assets/surfaces/passkeys.js", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    assert!(module.len() < 4096, "{} bytes", module.len());
    assert_eq!(
        headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
        "script-src 'self'"
    );

    let (status, headers, page) = send(&app, "GET", "/enroll", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CONTENT_SECURITY_POLICY).unwrap(),
        "script-src 'self'"
    );
    let scripts: Vec<&str> = page.split("<script").skip(1).collect();
    assert_eq!(scripts.len(), 1, "{page}");
    for script in scripts {
        let (tag, rest) = script.split_once('>').unwrap();
        assert!(
            tag.contains(" src=\"/assets/surfaces/passkeys.js\""),
            "{tag}"
        );
        assert!(rest.starts_with("</script>"), "an inline script: {rest}");
    }
    assert!(
        page.contains(r#"<form id="enroll" method="post""#),
        "{page}"
    );
    assert!(
        page.contains(r#"<a href="/sign-in""#),
        "no session: a sign-in link"
    );
    assert!(
        !page.contains(r#"<a href="/passkeys""#),
        "no session: no passkeys link"
    );
    let input = page
        .split("<input")
        .nth(1)
        .unwrap()
        .split('>')
        .next()
        .unwrap();
    assert!(
        !input.contains("name="),
        "the token input is named: {input}"
    );

    let (_, _, unconfigured) = send(&app_without(s), "GET", "/enroll", None).await;
    assert!(unconfigured.contains("not configured"), "{unconfigured}");
    assert!(!unconfigured.contains("<script"), "{unconfigured}");
}

fn app_without(s: &Store) -> axum::Router {
    app(s, None)
}
