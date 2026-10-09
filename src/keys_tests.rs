//! A person's own passkeys (`src/surfaces/keys.rs`; design sections 6, 7
//! and 10): adding through the fresh assertion and its one-time grant,
//! each way a grant is refused, a failed registration spending its grant,
//! a possible clone at the add, removing never the last and ending the
//! removed passkey's sessions, the removal race, and the page showing only
//! one's own. Each test runs on a database of its own.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use serde_json::{Value, json};
use tower::ServiceExt;
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;

use crate::passkeys::{CEREMONY_LIFETIME, Ceremonies, Ceremony};
use crate::passkeys_tests::{ORIGIN, app, authenticator, count, passkeys, register};
use crate::sign_in_tests::{bearer, enrolled, passkey_of, restore, rows, sign_in};
use crate::store::Store;
use crate::store::identity::{REMOVE_HOLD, Removed};
use crate::store::read::tests::fresh_store;

/// A request under a session's cookie, a JSON body or a form's.
async fn send_as(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    form: Option<String>,
    session: &str,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, format!("__Host-weaver_session={session}"));
    let body = match (body, form) {
        (Some(json), _) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(json.to_string())
        }
        (None, Some(form)) => {
            request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            Body::from(form)
        }
        (None, None) => Body::empty(),
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

/// A session's row identity, by its bearer.
async fn session_id_of(s: &Store, bearer: &str) -> i64 {
    sqlx::query_scalar("SELECT session_id FROM session WHERE bearer_digest = $1")
        .bind(crate::surfaces::gate::digest(bearer))
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// A person enrolled with `key` and signed in: their identity, the passkey's
/// credential ID, and the session's bearer.
async fn signed_in(
    app: &axum::Router,
    s: &Store,
    key: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> (String, String, String) {
    let (person, credential) = enrolled(app, s, key, name).await;
    let (status, headers, answer) = sign_in(app, key, name).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    (person, credential, bearer(&headers))
}

/// Another sign-in of the same person with `key`: a second session.
async fn another_session(
    app: &axum::Router,
    key: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> String {
    let (status, headers, answer) = sign_in(app, key, name).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    bearer(&headers)
}

/// The fresh assertion with `key` under `session`: its status and answer.
async fn fresh_assertion(
    app: &axum::Router,
    key: &mut WebauthnAuthenticator<SoftPasskey>,
    session: &str,
) -> (StatusCode, String) {
    let (status, _, options) = send_as(
        app,
        "POST",
        "/passkeys/add/assert/options",
        Some(json!({})),
        None,
        session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{options}");
    let (ceremony, credential) = crate::sign_in_tests::assert_with(key, &options);
    let (status, _, answer) = send_as(
        app,
        "POST",
        "/passkeys/add/assert/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential })),
        None,
        session,
    )
    .await;
    (status, answer)
}

/// The grant a fresh assertion with `key` earns under `session`.
async fn grant(
    app: &axum::Router,
    key: &mut WebauthnAuthenticator<SoftPasskey>,
    session: &str,
) -> String {
    let (status, answer) = fresh_assertion(app, key, session).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    serde_json::from_str::<Value>(&answer).unwrap()["grant"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// A registration's start under `session` with `grant`.
async fn add_options(app: &axum::Router, grant: &str, session: &str) -> (StatusCode, String) {
    let (status, _, answer) = send_as(
        app,
        "POST",
        "/passkeys/add/options",
        Some(json!({ "grant": grant })),
        None,
        session,
    )
    .await;
    (status, answer)
}

/// A whole addition with `device` under `session`, spending `grant`.
async fn add(
    app: &axum::Router,
    device: &mut WebauthnAuthenticator<SoftPasskey>,
    grant: &str,
    session: &str,
    label: &str,
) -> (StatusCode, String) {
    let (status, options) = add_options(app, grant, session).await;
    if status != StatusCode::OK {
        return (status, options);
    }
    let (ceremony, credential) = register(device, &options);
    let (status, _, answer) = send_as(
        app,
        "POST",
        "/passkeys/add/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential, "label": label })),
        None,
        session,
    )
    .await;
    (status, answer)
}

/// **A second passkey is added after a fresh assertion, and signs in**
/// (design section 7): the grant from the assertion starts a registration,
/// the passkey stands under its own identity with its label, the audit
/// holds the fresh assertion's records (on the asserting passkey) and the
/// addition's (on the new one), both by `passkey assertion`, and the new
/// passkey signs its person in.
#[tokio::test]
async fn a_second_passkey_is_added_after_a_fresh_assertion_and_signs_in() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential, session) = signed_in(&app, s, &mut key, "ada").await;
    let first = passkey_of(s, &credential).await;
    let earned = grant(&app, &mut key, &session).await;
    let mut phone = authenticator();
    let (status, answer) = add(&app, &mut phone, &earned, &session, "phone").await;
    assert_eq!(status, StatusCode::OK, "{answer}");

    let added: (String, Option<String>) = sqlx::query_as(
        "SELECT passkey_id, label FROM passkey WHERE person_id = $1 AND passkey_id <> $2",
    )
    .bind(&person)
    .bind(&first)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(added.1.as_deref(), Some("phone"));
    let records: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT action, method, target_id, outcome FROM audit \
         WHERE action IN ('add grant', 'passkey add') ORDER BY at, answers NULLS FIRST",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    let record = |action: &str, on: &str, outcome: Option<&str>| {
        (
            action.to_owned(),
            "passkey assertion".to_owned(),
            Some(on.to_owned()),
            outcome.map(str::to_owned),
        )
    };
    assert_eq!(
        records,
        [
            record("add grant", &first, None),
            record("add grant", &first, Some("ok")),
            record("passkey add", &added.0, None),
            record("passkey add", &added.0, Some("ok")),
        ]
    );
    let (status, _, answer) = sign_in(&app, &mut phone, "ada").await;
    assert_eq!(status, StatusCode::OK, "the new passkey signs in: {answer}");
}

/// **A grant serves one registration from the session that earned it**
/// (design section 7): another session of the same person cannot use it,
/// and the attempt spends it, so the earning session cannot either; a
/// grant used twice is refused the second time; an unknown grant is
/// refused; and without a session nothing starts.
#[tokio::test]
async fn a_grant_serves_one_registration_from_the_session_that_earned_it() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, _, session) = signed_in(&app, s, &mut key, "ada").await;
    let other = another_session(&app, &mut key, "ada").await;

    let earned = grant(&app, &mut key, &session).await;
    let (status, answer) = add_options(&app, &earned, &other).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "another session: {answer}");
    let (status, _) = add_options(&app, &earned, &session).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the refused attempt spent it"
    );

    let earned = grant(&app, &mut key, &session).await;
    let (status, answer) = add_options(&app, &earned, &session).await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let (status, _) = add_options(&app, &earned, &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a grant used twice");

    let (status, _) = add_options(&app, "no-such-grant", &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "no grant");
    let (status, _) = add_options(&app, "no-such-grant", "no-such-session").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "no session");
}

/// **A grant expires with the ceremony window** (design section 6), on a
/// paused clock: one past five minutes is refused.
#[tokio::test(start_paused = true)]
async fn a_grant_expires_with_the_ceremony_window() {
    let table = Ceremonies::default();
    let grant = table
        .start(Ceremony::AddGrant {
            person_id: "pe-0000000000000000".to_owned(),
            session_id: 1,
            earned_by: "pk-0000000000000000".to_owned(),
        })
        .unwrap();
    tokio::time::advance(CEREMONY_LIFETIME).await;
    assert!(table.take(&grant).is_none(), "a grant past five minutes");
}

/// **A registration that fails has spent its grant** (design section 7): a
/// credential ID already held refuses the finish, the ceremony cannot be
/// finished again, the grant cannot start another, and a fresh assertion
/// earns the next.
#[tokio::test]
async fn a_registration_that_fails_has_spent_its_grant() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, _, session) = signed_in(&app, s, &mut key, "ada").await;
    let (bea, _, _) = signed_in(&app, s, &mut authenticator(), "bea").await;

    let earned = grant(&app, &mut key, &session).await;
    let (status, options) = add_options(&app, &earned, &session).await;
    assert_eq!(status, StatusCode::OK, "{options}");
    let mut phone = authenticator();
    let (ceremony, credential) = register(&mut phone, &options);
    sqlx::query("INSERT INTO passkey (credential_id, person_id, credential) VALUES ($1, $2, '{}')")
        .bind(credential["rawId"].as_str().unwrap())
        .bind(&bea)
        .execute(&s.pool)
        .await
        .unwrap();
    let finish = json!({ "ceremony": ceremony, "credential": credential, "label": "phone" });
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/passkeys/add/finish",
        Some(finish.clone()),
        None,
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/passkeys/add/finish",
        Some(finish),
        None,
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "the ceremony is spent");
    let (status, _) = add_options(&app, &earned, &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the grant is spent");

    let earned = grant(&app, &mut key, &session).await;
    let (status, answer) = add(&app, &mut authenticator(), &earned, &session, "laptop").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a fresh assertion earns the next: {answer}"
    );
}

/// **A counter that did not rise at the add's assertion grants nothing**
/// (design section 6): audited as a possible cloned credential on the
/// asserting passkey, and no grant answered.
#[tokio::test]
async fn a_possible_clone_at_the_adds_assertion_grants_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential, session) = signed_in(&app, s, &mut key, "ada").await;
    restore(s, &credential, |c| c.counter = 1_000).await;
    let (status, answer) = fresh_assertion(&app, &mut key, &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(!answer.contains("grant"), "{answer}");
    let audited: Vec<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT person_id, target_id, refusal FROM audit WHERE action = 'add grant'",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    assert_eq!(audited.len(), 1, "{audited:?}");
    assert_eq!(audited[0].0.as_deref(), Some(person.as_str()));
    assert_eq!(
        audited[0].1.as_deref(),
        Some(passkey_of(s, &credential).await.as_str())
    );
    assert!(
        audited[0]
            .2
            .as_deref()
            .unwrap()
            .contains("possible cloned credential")
    );
}

/// **A person's last passkey is never removed, and removing another ends
/// its sessions** (design section 7): the last is refused; with two, the
/// one another session was opened with is removed and that session ends at
/// its next use; and removing the one this session was opened with ends
/// this session.
#[tokio::test]
async fn the_last_is_never_removed_and_removing_another_ends_its_sessions() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, credential, session) = signed_in(&app, s, &mut key, "ada").await;
    let first = passkey_of(s, &credential).await;
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/passkeys/remove",
        None,
        Some(format!("passkey={first}")),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "the last: {answer}");

    let mut phone = authenticator();
    let earned = grant(&app, &mut key, &session).await;
    add(&app, &mut phone, &earned, &session, "phone").await;
    let phones = another_session(&app, &mut phone, "ada").await;
    let phone_id: String =
        sqlx::query_scalar("SELECT passkey_id FROM passkey WHERE label = 'phone'")
            .fetch_one(&s.pool)
            .await
            .unwrap();
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/passkeys/remove",
        None,
        Some(format!("passkey={phone_id}")),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, _) = send_as(&app, "GET", "/passkeys", None, None, &phones).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the phone's session ended"
    );
    let (status, _, _) = send_as(&app, "GET", "/passkeys", None, None, &session).await;
    assert_eq!(status, StatusCode::OK, "this session stands");

    let earned = grant(&app, &mut key, &session).await;
    add(&app, &mut authenticator(), &earned, &session, "laptop").await;
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/passkeys/remove",
        None,
        Some(format!("passkey={first}")),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, _) = send_as(&app, "GET", "/passkeys", None, None, &session).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "this session, opened with the removed passkey, ended"
    );
}

/// **Two concurrent removals of a person's last two passkeys cannot both
/// land** (design section 7): from a session that stands throughout, the
/// first held after its count while the second starts; the second counts
/// after the first's removal, and is refused as the last.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_removals_of_the_last_two_leave_one() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential, session) = signed_in(&app, s, &mut key, "ada").await;
    let earned = grant(&app, &mut key, &session).await;
    let mut phone = authenticator();
    add(&app, &mut phone, &earned, &session, "phone").await;
    let first = passkey_of(s, &credential).await;
    let second: String = sqlx::query_scalar("SELECT passkey_id FROM passkey WHERE label = 'phone'")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    // Both removals from the phone's session, which stands until its own
    // passkey goes: the first removes the other passkey, the second the
    // phone's own, so only the count can refuse the second.
    let session = another_session(&app, &mut phone, "ada").await;

    let (read, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    REMOVE_HOLD
        .lock()
        .unwrap()
        .push((first.clone(), read.clone(), release.clone()));
    let session_id = session_id_of(s, &session).await;
    let held = tokio::spawn({
        let (s, person, first) = (s.clone(), person.clone(), first.clone());
        async move { s.remove_passkey(&person, session_id, &first).await.unwrap() }
    });
    tokio::time::timeout(Duration::from_secs(10), read.notified())
        .await
        .expect("the first removal counted");
    let other = tokio::spawn({
        let (s, person, second) = (s.clone(), person.clone(), second.clone());
        async move {
            s.remove_passkey(&person, session_id, &second)
                .await
                .unwrap()
        }
    });
    // Long enough for the second to count, were it not excluded.
    tokio::time::sleep(Duration::from_millis(300)).await;
    release.notify_one();
    let (first, second) = (held.await.unwrap(), other.await.unwrap());
    assert_eq!((first, second), (Removed::Removed, Removed::Last));
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM passkey WHERE person_id = $1",
            &person
        )
        .await,
        1,
        "one passkey stands"
    );
}

/// **The page lists the session's person's own passkeys alone**, marking
/// the one this session was opened with; another person's passkeys and
/// labels are not on it, and without a session it is not served.
#[tokio::test]
async fn the_page_lists_only_ones_own_passkeys() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, credential, session) = signed_in(&app, s, &mut key, "ada").await;
    let earned = grant(&app, &mut key, &session).await;
    add(&app, &mut authenticator(), &earned, &session, "ada-phone").await;
    let (bea, bea_credential, _) = signed_in(&app, s, &mut authenticator(), "bea").await;
    sqlx::query("UPDATE passkey SET label = 'bea-key' WHERE person_id = $1")
        .bind(&bea)
        .execute(&s.pool)
        .await
        .unwrap();

    let (status, _, page) = send_as(&app, "GET", "/passkeys", None, None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("ada-phone"), "{page}");
    assert!(page.contains(&passkey_of(s, &credential).await), "{page}");
    assert!(page.contains("this session"), "{page}");
    assert!(!page.contains("bea-key"), "another person's label: {page}");
    assert!(
        !page.contains(&passkey_of(s, &bea_credential).await),
        "another person's passkey: {page}"
    );
    assert_eq!(rows(s, "SELECT count(*) FROM passkey").await, 3);
    let (status, _, _) = send_as(&app, "GET", "/passkeys", None, None, "no-such-session").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// **A write a session authorizes re-checks its authority inside its own
/// transaction** (Spec 2.13): an addition held after its session read, a
/// host reset committing meanwhile (the person's passkeys cleared, a
/// recovery token issued), then released, is refused as its authority
/// gone, audited as failed: no passkey lands, so the reset's token still
/// redeems.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_host_reset_during_an_addition_refuses_it() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, _, session) = signed_in(&app, s, &mut key, "ada").await;
    let earned = grant(&app, &mut key, &session).await;
    let (status, options) = add_options(&app, &earned, &session).await;
    assert_eq!(status, StatusCode::OK, "{options}");
    let (ceremony, credential) = register(&mut authenticator(), &options);

    let (read, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    crate::surfaces::keys::ADD_HOLD.lock().unwrap().push((
        person.clone(),
        read.clone(),
        release.clone(),
    ));
    let finishing = tokio::spawn({
        let (app, session) = (app.clone(), session.clone());
        async move {
            send_as(
                &app,
                "POST",
                "/passkeys/add/finish",
                Some(json!({ "ceremony": ceremony, "credential": credential, "label": "late" })),
                None,
                &session,
            )
            .await
        }
    });
    tokio::time::timeout(Duration::from_secs(10), read.notified())
        .await
        .expect("the addition read its session");
    let reset = crate::host::reset(s, &cfg(), &person, None, Some("lab")).await;
    assert!(reset.ok, "{}", reset.value);
    let token = reset.value["token"].as_str().unwrap().to_owned();
    release.notify_one();
    let (status, _, answer) = finishing.await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM passkey WHERE person_id = $1",
            &person
        )
        .await,
        0,
        "the device the reset cut off was planted"
    );
    assert!(
        s.redeemable(&crate::store::identity::digest(&token))
            .await
            .unwrap()
            .is_some(),
        "the reset's token is stranded"
    );
    let outcome: Option<String> = sqlx::query_scalar(
        "SELECT outcome FROM audit WHERE action = 'passkey add' AND answers IS NOT NULL",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(outcome.as_deref(), Some("failed"));
}

/// A server config for the host's commands.
fn cfg() -> crate::config::ServerConfig {
    crate::config::ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:0".into(),
        database: String::new(),
        authority_dir: std::path::PathBuf::new(),
        silence_bound_secs: 60,
        link_address: None,
        server_name: "weaver-web".into(),
        admins: Vec::new(),
        agent_hop_budget: 8,
        providers: Vec::new(),
        enrollment_token_hours: 24,
        origin: None,
        tls_certificate: None,
        tls_key: None,
        rp_id: None,
        session_idle_secs: 3600,
        session_absolute_secs: 43200,
    }
}

/// **The passkey that earned a grant must still stand when the addition
/// lands**: the session stands throughout, opened with the first passkey,
/// while the grant is earned by a second, which is removed before the
/// registration finishes; the addition is refused as its authority gone.
#[tokio::test]
async fn an_addition_whose_earning_passkey_was_removed_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, _, session) = signed_in(&app, s, &mut key, "ada").await;
    let mut phone = authenticator();
    let earned = grant(&app, &mut key, &session).await;
    add(&app, &mut phone, &earned, &session, "phone").await;
    let phone_id: String =
        sqlx::query_scalar("SELECT passkey_id FROM passkey WHERE label = 'phone'")
            .fetch_one(&s.pool)
            .await
            .unwrap();

    let earned = grant(&app, &mut phone, &session).await;
    let (status, options) = add_options(&app, &earned, &session).await;
    assert_eq!(status, StatusCode::OK, "{options}");
    let (ceremony, credential) = register(&mut authenticator(), &options);
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/passkeys/remove",
        None,
        Some(format!("passkey={phone_id}")),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "the earning passkey removed");
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/passkeys/add/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential, "label": "laptop" })),
        None,
        &session,
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
        "the addition landed on a removed passkey's grant"
    );
}

/// **A removal re-checks its session inside its own transaction**: asked
/// under a session that no longer stands, it is refused as its authority
/// gone and removes nothing.
#[tokio::test]
async fn a_removal_under_a_session_that_no_longer_stands_removes_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential, session) = signed_in(&app, s, &mut key, "ada").await;
    let earned = grant(&app, &mut key, &session).await;
    add(&app, &mut authenticator(), &earned, &session, "phone").await;
    let session_id = session_id_of(s, &session).await;
    sqlx::query("UPDATE session SET closed_at = now() WHERE session_id = $1")
        .bind(session_id)
        .execute(&s.pool)
        .await
        .unwrap();
    let removed = s
        .remove_passkey(&person, session_id, &passkey_of(s, &credential).await)
        .await
        .unwrap();
    assert_eq!(removed, Removed::AuthorityGone);
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM passkey WHERE person_id = $1",
            &person
        )
        .await,
        2,
        "nothing removed"
    );
}

/// **No person removes another person's passkey** (Spec 2.13: a person
/// writes only their own authentication material): an admin holding two
/// passkeys, so the last-passkey rule does not answer first, asks to remove
/// another person's, which is refused as no passkey of theirs and stands.
#[tokio::test]
async fn another_persons_passkey_is_never_removed() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, _, session) = signed_in(&app, s, &mut key, "ada").await;
    let earned = grant(&app, &mut key, &session).await;
    add(&app, &mut authenticator(), &earned, &session, "phone").await;
    let (_, theirs) = enrolled(&app, s, &mut authenticator(), "bea").await;
    let theirs = passkey_of(s, &theirs).await;

    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/passkeys/remove",
        None,
        Some(format!("passkey={theirs}")),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{answer}");
    let stands: i64 = sqlx::query_scalar("SELECT count(*) FROM passkey WHERE passkey_id = $1")
        .bind(&theirs)
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert_eq!(stands, 1, "bea's passkey stands");
}
