//! Sign-in (`src/surfaces/sign_in.rs`, the counter rule in
//! `src/passkeys.rs`, the opening in `src/surfaces/gate.rs`; design
//! section 6): end to end through the soft passkey; the refusals; the
//! counter rule against a possible clone, a concurrent pair, a zero pair and
//! a backup upgrade; the counter persisting where the opening fails; and
//! the session bearer's randomness. Each test runs on a database of its own.

use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};
use webauthn_authenticator_rs::WebauthnAuthenticator;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_rs::prelude::{Credential, Passkey, RequestChallengeResponse, Url};

use crate::passkeys::{self, Assertion, COUNT_HOLD, Counted};
use crate::passkeys_tests::{
    ORIGIN, app, authenticator, count, passkeys, person_with_token, register, send,
};
use crate::store::Store;
use crate::store::audit::{FAIL_FIRST_RECORD, FAIL_REFUSAL};
use crate::store::read::tests::fresh_store;

/// A person enrolled through the token's redemption with this
/// authenticator: their identity and the passkey's credential ID.
pub(crate) async fn enrolled(
    app: &axum::Router,
    s: &Store,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> (String, String) {
    let (person, token) = person_with_token(s, name).await;
    let (_, _, options) = send(
        app,
        "POST",
        "/enroll/options",
        Some(json!({ "token": token })),
    )
    .await;
    let (ceremony, credential) = register(authenticator, &options);
    let (status, _, answer) = send(
        app,
        "POST",
        "/enroll/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    (person, credential["rawId"].as_str().unwrap().to_owned())
}

/// The authenticator's answer to the sign-in options the server gave: the
/// ceremony's identity and the assertion, as the browser module sends it.
pub(crate) fn assert_with(
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    options: &str,
) -> (String, Value) {
    let options: Value = serde_json::from_str(options).unwrap();
    let ceremony = options["ceremony"].as_str().unwrap().to_owned();
    let challenge: RequestChallengeResponse =
        serde_json::from_value(options["options"].clone()).unwrap();
    let asserted = authenticator
        .do_authentication(Url::parse(ORIGIN).unwrap(), challenge)
        .unwrap();
    let mut credential = serde_json::to_value(&asserted).unwrap();
    credential.as_object_mut().unwrap().remove("extensions");
    (ceremony, credential)
}

/// A whole sign-in: the options for `name`, the assertion, the finish.
pub(crate) async fn sign_in(
    app: &axum::Router,
    authenticator: &mut WebauthnAuthenticator<SoftPasskey>,
    name: &str,
) -> (StatusCode, HeaderMap, String) {
    let (status, _, options) = send(
        app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": name })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{options}");
    let (ceremony, credential) = assert_with(authenticator, &options);
    send(
        app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": credential })),
    )
    .await
}

pub(crate) fn bearer(headers: &HeaderMap) -> String {
    let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    cookie
        .strip_prefix("__Host-weaver_session=")
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

/// The stored passkey of a credential, as the library holds it.
pub(crate) async fn stored(s: &Store, credential_id: &str) -> Credential {
    let value: Value =
        sqlx::query_scalar("SELECT credential FROM passkey WHERE credential_id = $1")
            .bind(credential_id)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    Credential::from(serde_json::from_value::<Passkey>(value).unwrap())
}

/// The stored passkey of a credential, changed by `change` and written back.
pub(crate) async fn restore(s: &Store, credential_id: &str, change: impl FnOnce(&mut Credential)) {
    let mut credential = stored(s, credential_id).await;
    change(&mut credential);
    sqlx::query("UPDATE passkey SET credential = $2 WHERE credential_id = $1")
        .bind(credential_id)
        .bind(serde_json::to_value(Passkey::from(credential)).unwrap())
        .execute(&s.pool)
        .await
        .unwrap();
}

/// An audit record's method, person, target kind and identity, and outcome.
type AuditRow = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
);

/// The passkey's own identity, by its credential ID.
pub(crate) async fn passkey_of(s: &Store, credential_id: &str) -> String {
    sqlx::query_scalar("SELECT passkey_id FROM passkey WHERE credential_id = $1")
        .bind(credential_id)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

pub(crate) async fn rows(s: &Store, query: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(query.to_owned()))
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **A person enrolled signs in, and the session serves** (design section
/// 6): the finish sets the `__Host-` cookie, the session names the person
/// and the passkey's `pk-` identity, the cookie reads Record under the
/// person's name, the stored counter rose to the authenticator's, and the
/// audit holds the opening's first record (the person, by passkey
/// assertion) and its `ok` outcome.
#[tokio::test]
async fn a_person_enrolled_signs_in_and_the_session_serves() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential) = enrolled(&app, s, &mut key, "ada").await;

    let (status, headers, answer) = sign_in(&app, &mut key, "ada").await;
    assert_eq!(status, StatusCode::OK, "{answer}");
    let cookie = headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(cookie.starts_with("__Host-weaver_session="), "{cookie}");
    let passkey_id: String =
        sqlx::query_scalar("SELECT passkey_id FROM passkey WHERE person_id = $1")
            .bind(&person)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    let session: String = sqlx::query_scalar(
        "SELECT passkey_id FROM session WHERE person_id = $1 AND bearer_digest = $2",
    )
    .bind(&person)
    .bind(crate::surfaces::gate::digest(&bearer(&headers)))
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(
        session, passkey_id,
        "the session names the passkey's own identity"
    );
    assert!(stored(s, &credential).await.counter > 0, "the counter rose");

    let request = axum::http::Request::builder()
        .uri("/record")
        .header(
            header::COOKIE,
            format!("__Host-weaver_session={}", bearer(&headers)),
        )
        .body(axum::body::Body::empty())
        .unwrap();
    let response = tower::ServiceExt::oneshot(app.clone(), request)
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the session serves Record"
    );

    let opened: Vec<AuditRow> = sqlx::query_as(
        "SELECT method, person_id, target_kind, target_id, outcome FROM audit \
             WHERE action = 'session open' ORDER BY answers NULLS FIRST",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    let on_the_passkey = |outcome: Option<&str>| {
        (
            "passkey assertion".to_owned(),
            Some(person.clone()),
            "passkey".to_owned(),
            Some(passkey_id.clone()),
            outcome.map(str::to_owned),
        )
    };
    assert_eq!(
        opened,
        [on_the_passkey(None), on_the_passkey(Some("ok"))],
        "the opening is audited on the passkey it began with"
    );
}

/// **A sign-in that cannot proceed opens nothing**: an unknown name, a
/// disabled person and a person holding no passkey get one answer at the
/// options and start no ceremony; a failed signature is refused and not
/// audited; a ceremony is finished once.
#[tokio::test]
async fn a_sign_in_that_cannot_proceed_opens_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let keys = passkeys();
    let app = app(s, Some(keys.clone()));
    let mut key = authenticator();
    enrolled(&app, s, &mut key, "ada").await;
    let (disabled, _) = enrolled(&app, s, &mut authenticator(), "bea").await;
    sqlx::query("UPDATE person SET enabled = false, version = version + 1 WHERE person_id = $1")
        .bind(&disabled)
        .execute(&s.pool)
        .await
        .unwrap();
    person_with_token(s, "carl").await;
    for name in ["nobody", "bea", "carl"] {
        let (status, _, answer) = send(
            &app,
            "POST",
            "/sign-in/options",
            Some(json!({ "name": name })),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{name}: {answer}");
        assert_eq!(answer, "no passkey sign-in is open for that name", "{name}");
    }
    assert_eq!(keys.ceremonies.held(), 0, "no ceremony was started");

    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, mut credential) = assert_with(&mut key, &options);
    let signature = credential["response"]["signature"]
        .as_str()
        .unwrap()
        .to_owned();
    let flipped = if signature.starts_with('A') { "B" } else { "A" };
    credential["response"]["signature"] = Value::String(format!("{flipped}{}", &signature[1..]));
    let finish = json!({ "ceremony": ceremony, "credential": credential });
    let (status, _, _) = send(&app, "POST", "/sign-in/finish", Some(finish.clone())).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a failed signature is refused"
    );
    assert_eq!(rows(s, "SELECT count(*) FROM session").await, 0);
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action IN ('sign in', 'session open')"
        )
        .await,
        0,
        "a failed signature is not audited"
    );
    let (status, _, _) = send(&app, "POST", "/sign-in/finish", Some(finish)).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a ceremony is finished once"
    );
}

/// **A counter that did not rise refuses the sign-in as a possible clone**
/// (design section 6), whether the library sees it against the copy loaded
/// at the options or the counter rule against the copy it locks: each is
/// audited as a possible cloned credential, opens nothing, and leaves the
/// passkey standing.
#[tokio::test]
async fn a_counter_that_did_not_rise_is_refused_as_a_possible_clone() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential) = enrolled(&app, s, &mut key, "ada").await;

    // The library's own check: the stored counter is above what the
    // authenticator returns before the options load it.
    restore(s, &credential, |c| c.counter = 100).await;
    let (status, headers, answer) = sign_in(&app, &mut key, "ada").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(answer.contains("may be a copy"), "{answer}");
    assert!(headers.get(header::SET_COOKIE).is_none());

    // The counter rule's: the counter rises past the authenticator's between
    // the options and the finish.
    restore(s, &credential, |c| c.counter = 0).await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);
    restore(s, &credential, |c| c.counter = 100).await;
    let (status, _, answer) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": asserted })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");

    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        0,
        "nothing opened"
    );
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM passkey WHERE person_id = $1",
            &person
        )
        .await,
        1,
        "the passkey stands"
    );
    let audited: Vec<(String, Option<String>, String)> =
        sqlx::query_as("SELECT method, person_id, refusal FROM audit WHERE action = 'sign in'")
            .fetch_all(&s.pool)
            .await
            .unwrap();
    assert_eq!(audited.len(), 2, "{audited:?}");
    for (method, who, refusal) in audited {
        assert_eq!(
            (method.as_str(), who.as_deref()),
            ("passkey assertion", Some(person.as_str()))
        );
        assert!(refusal.contains("possible cloned credential"), "{refusal}");
    }
}

/// An assertion as the counter rule reads it, with a counter and an upgrade
/// the soft authenticator cannot produce.
struct Fake {
    counter: u32,
    upgrades_backup: bool,
}

impl Assertion for Fake {
    fn counter(&self) -> u32 {
        self.counter
    }

    fn merge_into(&self, passkey: &mut Passkey) {
        let mut credential = Credential::from(passkey.clone());
        if self.counter > credential.counter {
            credential.counter = self.counter;
        }
        if self.upgrades_backup {
            credential.backup_eligible = true;
        }
        *passkey = Passkey::from(credential);
    }
}

/// Two assertions of one passkey through the counter rule, the first held
/// after its locked read while the second starts, then released.
async fn concurrently(
    s: &Store,
    person: &str,
    credential: &str,
    first: Fake,
    second: Fake,
) -> (Counted, Counted) {
    // The rule counts, and the hold is keyed, by the passkey's own identity.
    let credential = passkey_of(s, credential).await;
    let credential = credential.as_str();
    let (read, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    COUNT_HOLD
        .lock()
        .unwrap()
        .push((credential.to_owned(), read.clone(), release.clone()));
    let held = tokio::spawn({
        let (s, person, credential) = (s.clone(), person.to_owned(), credential.to_owned());
        async move {
            passkeys::count(&s, &credential, &person, &first)
                .await
                .unwrap()
        }
    });
    tokio::time::timeout(Duration::from_secs(10), read.notified())
        .await
        .expect("the first assertion read the passkey");
    let other = tokio::spawn({
        let (s, person, credential) = (s.clone(), person.to_owned(), credential.to_owned());
        async move {
            passkeys::count(&s, &credential, &person, &second)
                .await
                .unwrap()
        }
    });
    // Long enough for the second to read the passkey, were it not locked.
    tokio::time::sleep(Duration::from_millis(300)).await;
    release.notify_one();
    (held.await.unwrap(), other.await.unwrap())
}

/// **The counter rule serializes on the passkey's row** (design section 6):
/// two concurrent assertions of one nonzero-counter passkey with one
/// counter (a clone's), the first held after its locked read: the second
/// reads the first's write, not the copy before it, and is refused.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn of_two_concurrent_assertions_with_one_counter_the_second_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (person, credential) = enrolled(&app, s, &mut authenticator(), "ada").await;
    restore(s, &credential, |c| c.counter = 3).await;
    let fake = || Fake {
        counter: 5,
        upgrades_backup: false,
    };
    let (first, second) = concurrently(s, &person, &credential, fake(), fake()).await;
    assert!(matches!(first, Counted::Admitted { .. }), "{first:?}");
    assert!(
        matches!(second, Counted::PossibleClone { .. }),
        "{second:?}"
    );
    assert_eq!(stored(s, &credential).await.counter, 5);
}

/// **A zero counter has nothing to race** (design section 6): two
/// concurrent assertions of a passkey whose stored and returned counters
/// are zero both succeed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_zero_counter_assertions_both_succeed() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (person, credential) = enrolled(&app, s, &mut authenticator(), "ada").await;
    restore(s, &credential, |c| c.counter = 0).await;
    let zero = || Fake {
        counter: 0,
        upgrades_backup: false,
    };
    let (first, second) = concurrently(s, &person, &credential, zero(), zero()).await;
    assert!(matches!(first, Counted::Admitted { .. }), "{first:?}");
    assert!(matches!(second, Counted::Admitted { .. }), "{second:?}");
}

/// **No assertion's write erases another's upgrade** (design section 6):
/// the first assertion, held after its locked read, merges nothing; the
/// second, started meanwhile, upgrades the backup eligibility. Merging into
/// the copy each reads under the lock, the upgrade stands; writing back a
/// copy read before the other's write would erase it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_backup_upgrade_is_not_erased_by_a_concurrent_assertion() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (person, credential) = enrolled(&app, s, &mut authenticator(), "ada").await;
    restore(s, &credential, |c| {
        c.counter = 0;
        c.backup_eligible = false;
    })
    .await;
    let (first, second) = concurrently(
        s,
        &person,
        &credential,
        Fake {
            counter: 0,
            upgrades_backup: false,
        },
        Fake {
            counter: 0,
            upgrades_backup: true,
        },
    )
    .await;
    assert!(matches!(first, Counted::Admitted { .. }), "{first:?}");
    assert!(matches!(second, Counted::Admitted { .. }), "{second:?}");
    assert!(
        stored(s, &credential).await.backup_eligible,
        "the concurrent assertion erased the upgrade"
    );
}

/// **The counter persists where the session's opening then fails** (design
/// section 6): the counter rule commits before the opening begins, so an
/// opening refused at its first audit record leaves the passkey as the
/// authenticator advanced it, and no session.
#[tokio::test]
async fn the_counter_persists_where_the_opening_then_fails() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, credential) = enrolled(&app, s, &mut key, "ada").await;
    let before = stored(s, &credential).await.counter;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);
    FAIL_FIRST_RECORD.with(|f| f.set(true));
    let (status, headers, _) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": asserted })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(headers.get(header::SET_COOKIE).is_none());
    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        0,
        "no session"
    );
    assert!(
        stored(s, &credential).await.counter > before,
        "the counter rolled back with the failed opening"
    );
}

/// **A session bearer is drawn as every bearer the server issues is**: two
/// sign-ins' bearers are 64 hex and share no prefix.
#[tokio::test]
async fn session_bearers_share_no_prefix() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    enrolled(&app, s, &mut key, "ada").await;
    let mut bearers = Vec::new();
    for _ in 0..4 {
        let (status, headers, answer) = sign_in(&app, &mut key, "ada").await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        bearers.push(bearer(&headers));
    }
    for pair in bearers.windows(2) {
        assert_eq!(pair[0].len(), 64);
        assert_ne!(
            pair[0][..16],
            pair[1][..16],
            "two session bearers share a prefix"
        );
    }
}

/// **One person never signs in as another**: another person's
/// authenticator answering this person's challenge opens nothing. Three
/// guards hold it, each alone: the ceremony's options allow this person's
/// passkeys only, so the other authenticator finds nothing to answer with;
/// the counter rule finds the credential among this person's passkeys only;
/// and the opening checks the passkey is this person's.
#[tokio::test]
async fn an_assertion_by_another_persons_passkey_opens_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    enrolled(&app, s, &mut authenticator(), "ada").await;
    let mut bea = authenticator();
    enrolled(&app, s, &mut bea, "bea").await;

    let (_, _, ada_options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let options: Value = serde_json::from_str(&ada_options).unwrap();
    let challenge: RequestChallengeResponse =
        serde_json::from_value(options["options"].clone()).unwrap();
    let Ok(answered) = bea.do_authentication(Url::parse(ORIGIN).unwrap(), challenge) else {
        // The options allowed none of the other person's passkeys.
        assert_eq!(rows(s, "SELECT count(*) FROM session").await, 0);
        return;
    };
    let mut credential = serde_json::to_value(&answered).unwrap();
    credential.as_object_mut().unwrap().remove("extensions");
    let (status, headers, answer) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": options["ceremony"], "credential": credential })),
    )
    .await;
    assert_ne!(status, StatusCode::OK, "{answer}");
    assert!(headers.get(header::SET_COOKIE).is_none());
    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        0,
        "nothing opened"
    );
}

/// **The session's opening checks its person again** (design section 6):
/// a person disabled after the counter rule read their passkey, and before
/// the session opens, gets no session, the opening's own check under a share
/// lock refusing it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_person_disabled_during_sign_in_gets_no_session() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential) = enrolled(&app, s, &mut key, "ada").await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);
    let (read, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    let held = passkey_of(s, &credential).await;
    COUNT_HOLD
        .lock()
        .unwrap()
        .push((held, read.clone(), release.clone()));
    let finishing = tokio::spawn({
        let app = app.clone();
        async move {
            send(
                &app,
                "POST",
                "/sign-in/finish",
                Some(json!({ "ceremony": ceremony, "credential": asserted })),
            )
            .await
        }
    });
    tokio::time::timeout(Duration::from_secs(10), read.notified())
        .await
        .expect("the counter rule read the passkey");
    sqlx::query("UPDATE person SET enabled = false, version = version + 1 WHERE person_id = $1")
        .bind(&person)
        .execute(&s.pool)
        .await
        .unwrap();
    release.notify_one();
    let (status, headers, answer) = finishing.await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(headers.get(header::SET_COOKIE).is_none());
    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        0,
        "no session"
    );
}

/// Each of a person's passkeys removed, as the host reset removes them.
pub(crate) async fn reset(s: &Store, person: &str) {
    sqlx::query("DELETE FROM passkey WHERE person_id = $1")
        .bind(person)
        .execute(&s.pool)
        .await
        .unwrap();
}

/// **A passkey removed between the options and the finish signs no one
/// in** (design section 6): the finish counts and opens on the passkey the
/// ceremony challenged, by its own identity, and that row is gone, so the
/// sign-in is refused as its passkey removed. A verified assertion the
/// library calls a possible clone is still audited, against the challenged
/// identity.
#[tokio::test]
async fn a_passkey_removed_between_options_and_finish_signs_no_one_in() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, _) = enrolled(&app, s, &mut key, "ada").await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);
    reset(s, &person).await;
    let (status, headers, answer) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": asserted })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(answer.contains("has been removed"), "{answer}");
    assert!(headers.get(header::SET_COOKIE).is_none());

    // A possible clone, its passkey removed before the finish: still
    // audited against the identity the ceremony challenged.
    let (person, credential) = enrolled(&app, s, &mut key, "bea").await;
    let old = passkey_of(s, &credential).await;
    restore(s, &credential, |c| c.counter = 1_000).await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "bea" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);
    reset(s, &person).await;
    let (status, _, answer) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": asserted })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    let audited: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT target_kind, target_id FROM audit WHERE action = 'sign in' AND person_id = $1",
    )
    .bind(&person)
    .fetch_all(&s.pool)
    .await
    .unwrap();
    assert_eq!(audited, [("passkey".to_owned(), Some(old))]);
    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        0,
        "nothing opened"
    );
}

/// **A credential re-enrolled between the options and the finish opens
/// nothing** (design section 6): the reset removes the challenged passkey,
/// and the same credential enrolled again is a new row under a new
/// identity the ceremony never challenged, so the finish counts nothing on
/// it and opens no session.
#[tokio::test]
async fn a_credential_re_enrolled_between_options_and_finish_opens_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (person, credential) = enrolled(&app, s, &mut key, "ada").await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);

    let kept: Value = sqlx::query_scalar("SELECT credential FROM passkey WHERE credential_id = $1")
        .bind(&credential)
        .fetch_one(&s.pool)
        .await
        .unwrap();
    reset(s, &person).await;
    sqlx::query("INSERT INTO passkey (credential_id, person_id, credential) VALUES ($1, $2, $3)")
        .bind(&credential)
        .bind(&person)
        .bind(&kept)
        .execute(&s.pool)
        .await
        .unwrap();
    let renewed = passkey_of(s, &credential).await;

    let (status, headers, answer) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": asserted })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(headers.get(header::SET_COOKIE).is_none());
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM session WHERE passkey_id = $1",
            &renewed
        )
        .await,
        0,
        "a session opened on the re-enrolled row"
    );
    assert_eq!(
        stored(s, &credential).await.counter,
        Credential::from(serde_json::from_value::<Passkey>(kept).unwrap()).counter,
        "the re-enrolled row was counted"
    );
}

/// **A refusal whose record cannot be written is the server's failure**:
/// with the possible clone's refusal record failing, the sign-in answers
/// 500, never the ordinary refusal, and opens nothing.
#[tokio::test]
async fn a_clone_refusal_that_cannot_be_recorded_answers_the_servers_failure() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let mut key = authenticator();
    let (_, credential) = enrolled(&app, s, &mut key, "ada").await;
    restore(s, &credential, |c| c.counter = 1_000).await;
    let (_, _, options) = send(
        &app,
        "POST",
        "/sign-in/options",
        Some(json!({ "name": "ada" })),
    )
    .await;
    let (ceremony, asserted) = assert_with(&mut key, &options);
    FAIL_REFUSAL.with(|f| f.set(true));
    let (status, headers, answer) = send(
        &app,
        "POST",
        "/sign-in/finish",
        Some(json!({ "ceremony": ceremony, "credential": asserted })),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{answer}");
    assert!(answer.contains("could not be recorded"), "{answer}");
    assert!(headers.get(header::SET_COOKIE).is_none());
    assert_eq!(
        rows(s, "SELECT count(*) FROM session").await,
        0,
        "nothing opened"
    );
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit WHERE action = 'sign in'").await,
        0,
        "the hook refused the record"
    );
}
