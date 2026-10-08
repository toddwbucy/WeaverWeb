//! **The host's identity commands** (`src/host.rs`), each against a
//! database of its own, since the last-admin rule is store-wide and would
//! read another test's admins. No test name, token or digest is a real
//! person's.

use crate::config::ServerConfig;
use crate::host;
use crate::link::verbs::Answer;
use crate::store::Store;
use crate::store::audit::FAIL_FIRST_RECORD;
use crate::store::identity::digest;
use crate::store::read::tests::fresh_store;
use serde_json::Value;
use std::sync::Arc;

fn cfg() -> ServerConfig {
    ServerConfig {
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
    }
}

const LAB: Option<&str> = Some("lab");

fn ok(answer: &Answer) -> &Value {
    assert!(answer.ok, "{}", answer.value);
    &answer.value
}

fn refused<'a>(answer: &'a Answer, why: &str) -> &'a str {
    assert!(!answer.ok, "{}", answer.value);
    let error = answer.value["error"].as_str().unwrap();
    assert!(error.contains(why), "{error}");
    error
}

/// The first record an answer names, as (target kind, target id, action),
/// with the outcome naming it.
async fn audited(s: &Store, answer: &Value) -> ((String, Option<String>, String), String) {
    let first = answer["audit"]
        .as_str()
        .expect("the answer names its record");
    let record: (String, Option<String>, String) = sqlx::query_as(
        "SELECT target_kind, target_id, action FROM audit \
         WHERE audit_id = $1 AND principal = 'host' AND claimed_author = 'lab' AND answers IS NULL",
    )
    .bind(first)
    .fetch_one(&s.pool)
    .await
    .expect("the first record stands, the host's");
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM audit WHERE answers = $1")
        .bind(first)
        .fetch_one(&s.pool)
        .await
        .expect("an outcome names it");
    (record, outcome)
}

fn on(kind: &str, id: &str, action: &str) -> ((String, Option<String>, String), String) {
    ((kind.into(), Some(id.into()), action.into()), "ok".into())
}

/// An agent row of the register to grant a role on.
async fn agent(s: &Store) -> String {
    let mut conn = s.pool.acquire().await.unwrap();
    let id = Store::mint_agent_id_on(&mut conn).await.unwrap();
    let fp = |seed: &[u8]| crate::link::authority::fingerprint(seed);
    Store::register_agent_on(
        &mut conn,
        &id,
        "box",
        "karl",
        LAB,
        &fp(b"gate"),
        &fp(b"admin"),
        &fp(b"authority"),
    )
    .await
    .unwrap();
    id.as_str().to_owned()
}

async fn count(s: &Store, sql: &'static str, bind: &str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(bind)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **Every host identity command is audited as the host's**: bootstrap,
/// token and reset on the person, grant add and remove on the grant, role
/// set on the role, each with its first record and an outcome naming it.
#[tokio::test]
async fn every_host_identity_command_is_audited() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let a = host::bootstrap(s, &cfg(), "ada", None, LAB).await;
    let ada = ok(&a)["person"].as_str().unwrap().to_owned();
    assert_eq!(
        audited(s, &a.value).await,
        on("person", &ada, "person bootstrap")
    );
    let b = host::bootstrap(s, &cfg(), "bea", None, LAB).await;
    ok(&b);

    let t = host::token(s, &cfg(), "ada", None, LAB).await;
    assert_eq!(audited(s, ok(&t)).await, on("person", &ada, "person token"));
    let r = host::reset(s, &cfg(), &ada, None, LAB).await;
    assert_eq!(audited(s, ok(&r)).await, on("person", &ada, "person reset"));

    let agent = agent(s).await;
    let g = host::grant_add(s, "ada", "observer", Some(&agent), LAB).await;
    let grant = ok(&g)["grant"].as_str().unwrap().to_owned();
    assert_eq!(audited(s, &g.value).await, on("grant", &grant, "grant add"));
    let admin_grant = a.value["grant"].as_str().unwrap();
    let rm = host::grant_remove(s, "ada", "admin", None, LAB).await;
    assert_eq!(
        audited(s, ok(&rm)).await,
        on("grant", admin_grant, "grant remove")
    );

    let role = host::role_set(s, "operator", &["show".into(), "turn".into()], LAB).await;
    assert_eq!(
        audited(s, ok(&role)).await,
        on("role", "operator", "role set")
    );
}

/// **A refused first record leaves the store as it was**: with the record
/// refused, each command answers why and writes nothing: no person, no
/// token, no passkey cleared, no grant written or revoked, no verb changed.
#[tokio::test]
async fn a_refused_first_record_leaves_the_identity_rows_as_they_were() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let refuse = || FAIL_FIRST_RECORD.with(|f| f.set(true));
    let unaudited = |answer: &Answer| refused(answer, "first record could not be written").len();

    refuse();
    unaudited(&host::bootstrap(s, &cfg(), "ada", None, LAB).await);
    assert!(s.person("ada").await.unwrap().is_none(), "no person");

    let a = host::bootstrap(s, &cfg(), "ada", None, LAB).await;
    let ada = ok(&a)["person"].as_str().unwrap().to_owned();
    host::bootstrap(s, &cfg(), "bea", None, LAB).await;
    sqlx::query(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('cred-1', $1, '{}')",
    )
    .bind(&ada)
    .execute(&s.pool)
    .await
    .unwrap();
    let tokens = "SELECT count(*) FROM enrollment_token WHERE person_id = $1";
    let before = count(s, tokens, &ada).await;

    refuse();
    unaudited(&host::reset(s, &cfg(), &ada, None, LAB).await);
    assert_eq!(
        count(s, "SELECT count(*) FROM passkey WHERE person_id = $1", &ada).await,
        1,
        "no passkey cleared"
    );
    assert_eq!(count(s, tokens, &ada).await, before, "no token issued");

    let bea = s.person("bea").await.unwrap().unwrap().person_id;
    refuse();
    unaudited(&host::token(s, &cfg(), &bea, None, LAB).await);
    assert_eq!(count(s, tokens, &bea).await, 1, "no token issued");

    let agent = agent(s).await;
    refuse();
    unaudited(&host::grant_add(s, &ada, "observer", Some(&agent), LAB).await);
    assert!(
        s.live_grant(&ada, "observer", Some(&agent))
            .await
            .unwrap()
            .is_none(),
        "no grant"
    );

    refuse();
    unaudited(&host::grant_remove(s, &ada, "admin", None, LAB).await);
    assert!(
        s.live_grant(&ada, "admin", None).await.unwrap().is_some(),
        "no grant revoked"
    );

    refuse();
    unaudited(&host::role_set(s, "observer", &["turn".into()], LAB).await);
    assert_eq!(
        s.role("observer").await.unwrap().unwrap().verbs,
        vec!["show"],
        "no verb changed"
    );
}

/// **Names differing only by case, width or composition are one name**: a
/// second person under any such variant is refused, by name.
#[tokio::test]
async fn names_differing_by_case_width_or_composition_are_one() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    ok(&host::bootstrap(s, &cfg(), "Ada", None, LAB).await);
    ok(&host::bootstrap(s, &cfg(), "Ren\u{E9}e", None, LAB).await);
    for variant in [
        "ADA",
        "\u{FF21}\u{FF44}\u{FF41}",
        " ada ",
        "Rene\u{301}e",
        "REN\u{C9}E",
    ] {
        let answer = host::bootstrap(s, &cfg(), variant, None, LAB).await;
        refused(&answer, "canonical form");
    }
    assert_eq!(
        count(s, "SELECT count(*) FROM person WHERE $1 = $1", "").await,
        2,
        "two persons and no variant"
    );
}

/// **A token is never issued for a person holding a passkey**, until the
/// host reset clears them; and never for longer than seven days.
#[tokio::test]
async fn a_token_is_issued_only_for_a_person_holding_no_passkey_and_at_most_seven_days() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let a = host::bootstrap(s, &cfg(), "ada", None, LAB).await;
    let ada = ok(&a)["person"].as_str().unwrap().to_owned();
    sqlx::query(
        "INSERT INTO passkey (credential_id, person_id, credential) VALUES ('cred-1', $1, '{}')",
    )
    .bind(&ada)
    .execute(&s.pool)
    .await
    .unwrap();
    refused(
        &host::token(s, &cfg(), &ada, None, LAB).await,
        "holds a passkey",
    );
    let r = host::reset(s, &cfg(), &ada, None, LAB).await;
    assert_eq!(ok(&r)["passkeys_cleared"], 1);
    ok(&host::token(s, &cfg(), &ada, None, LAB).await);

    refused(
        &host::token(s, &cfg(), &ada, Some(169), LAB).await,
        "seven days",
    );
    let mut tx = s.pool.begin().await.unwrap();
    let past = sqlx::query(
        "INSERT INTO enrollment_token (token_digest, person_id, expires_at) \
         VALUES (repeat('b', 64), $1, now() + interval '8 days')",
    )
    .bind(&ada)
    .execute(&mut *tx)
    .await;
    tx.rollback().await.unwrap();
    assert!(
        past.unwrap_err()
            .to_string()
            .contains("enrollment_token_lives_at_most_seven_days"),
        "the store refuses a token past seven days"
    );
}

/// **A token is kept only as its digest**, printed once in the answer: its
/// digest stands in the token table, and its value in no column of that
/// table or of the audit. A newer token ends the person's earlier one.
#[tokio::test]
async fn a_token_is_kept_only_as_its_digest() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let a = host::bootstrap(s, &cfg(), "ada", None, LAB).await;
    let token = ok(&a)["token"].as_str().unwrap().to_owned();
    assert_eq!(token.len(), 64, "32 bytes, in hex");
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM enrollment_token WHERE token_digest = $1",
            &digest(&token)
        )
        .await,
        1
    );
    for table in [
        "SELECT count(*) FROM enrollment_token t WHERE to_jsonb(t)::text LIKE '%' || $1 || '%'",
        "SELECT count(*) FROM audit a WHERE to_jsonb(a)::text LIKE '%' || $1 || '%'",
    ] {
        assert_eq!(
            count(s, table, &token).await,
            0,
            "the value is stored nowhere"
        );
    }
    let person = a.value["person"].as_str().unwrap();
    ok(&host::token(s, &cfg(), person, None, LAB).await);
    assert_eq!(
        count(
            s,
            "SELECT count(*) FROM enrollment_token WHERE person_id = $1 AND ended_at IS NULL",
            person
        )
        .await,
        1,
        "one live token, the newest"
    );
}

/// **The last enabled admin's grant is never revoked**, by the host either.
#[tokio::test]
async fn the_last_enabled_admins_grant_is_never_revoked() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    ok(&host::bootstrap(s, &cfg(), "ada", None, LAB).await);
    refused(
        &host::grant_remove(s, "ada", "admin", None, LAB).await,
        "last enabled admin",
    );
    let ada = s.person("ada").await.unwrap().unwrap().person_id;
    assert!(s.live_grant(&ada, "admin", None).await.unwrap().is_some());
}

/// **Two removals of the last two admins leave one** (the identity
/// exclusion): the first held between its count and its revocation, the
/// second asked meanwhile; under the exclusion the second waits, counts
/// after the first has committed, and is refused.
#[tokio::test]
async fn two_removals_of_the_last_two_admins_leave_one() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = fresh.store.clone();
    ok(&host::bootstrap(&s, &cfg(), "ada", None, LAB).await);
    ok(&host::bootstrap(&s, &cfg(), "bea", None, LAB).await);
    let ada = s.person("ada").await.unwrap().unwrap().person_id;
    let held = s.live_grant(&ada, "admin", None).await.unwrap().unwrap();
    let (locked, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    *host::REMOVE_HOLD.lock().unwrap() = Some((held, locked.clone(), release.clone()));

    let first = tokio::spawn({
        let s = s.clone();
        async move { host::grant_remove(&s, "ada", "admin", None, LAB).await }
    });
    locked.notified().await;
    let second = tokio::spawn({
        let s = s.clone();
        async move { host::grant_remove(&s, "bea", "admin", None, LAB).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    release.notify_one();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(
        [first.ok, second.ok].iter().filter(|done| **done).count(),
        1,
        "one removal lands: {} {}",
        first.value,
        second.value
    );
    assert_eq!(
        count(
            &s,
            "SELECT count(*) FROM role_grant WHERE role = 'admin' AND revoked_at IS NULL AND $1 = $1",
            ""
        )
        .await,
        1,
        "one admin remains"
    );
}

/// **A role's verbs are written within the vocabulary, and never admin's**:
/// the command refuses a verb outside it and the admin role, by name, and
/// the store refuses both on its own.
#[tokio::test]
async fn a_roles_verbs_stay_in_the_vocabulary_and_admins_are_never_written() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    refused(
        &host::role_set(s, "admin", &["show".into()], LAB).await,
        "admin is fixed",
    );
    refused(
        &host::role_set(s, "observer", &["fly".into()], LAB).await,
        "not in the vocabulary",
    );
    let mut tx = s.pool.begin().await.unwrap();
    let outside = sqlx::query("UPDATE role SET verbs = '{fly}' WHERE name = 'observer'")
        .execute(&mut *tx)
        .await;
    tx.rollback().await.unwrap();
    assert!(
        outside
            .unwrap_err()
            .to_string()
            .contains("role_verbs_are_the_vocabulary")
    );
    assert_eq!(
        s.role("observer").await.unwrap().unwrap().verbs,
        vec!["show"]
    );
}

/// **A per-agent role is granted on an agent, and admin on none**: each the
/// other way is refused before anything is written.
#[tokio::test]
async fn a_grant_names_an_agent_exactly_where_its_role_is_per_agent() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    ok(&host::bootstrap(s, &cfg(), "ada", None, LAB).await);
    let agent = agent(s).await;
    refused(
        &host::grant_add(s, "ada", "observer", None, LAB).await,
        "--agent",
    );
    refused(
        &host::grant_add(s, "ada", "admin", Some(&agent), LAB).await,
        "server-wide",
    );
    ok(&host::grant_add(s, "ada", "observer", Some(&agent), LAB).await);
}
