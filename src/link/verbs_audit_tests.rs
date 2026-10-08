//! **The register verbs are audited as the host's** (Spec 2.13, act 11's
//! design section 10): each of the five writes its first record before it
//! acts and an outcome naming it after, and a verb whose first record
//! cannot be written does not act. Against the real store and a lab
//! authority; no test reaches an agent.

use super::Plane;
use super::authority::Authority;
use super::register::CredentialState;
use super::tests::{Lab, lab_config};
use crate::store::Store;
use crate::store::audit::FAIL_FIRST_RECORD;
use serde_json::Value;

/// The first record an answer names and the outcome naming it, as the
/// store holds them: (principal, claimed author, target kind, target id,
/// action) of the first, and the outcome.
async fn audited(
    s: &Store,
    answer: &Value,
) -> (
    (String, Option<String>, String, Option<String>, String),
    String,
) {
    let first = answer["audit"]
        .as_str()
        .unwrap_or_else(|| panic!("the answer names its first record: {answer}"))
        .to_owned();
    let record: (String, Option<String>, String, Option<String>, String) = sqlx::query_as(
        "SELECT principal, claimed_author, target_kind, target_id, action FROM audit \
         WHERE audit_id = $1 AND answers IS NULL AND outcome IS NULL AND refusal IS NULL",
    )
    .bind(&first)
    .fetch_one(&s.pool)
    .await
    .expect("the first record stands");
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM audit WHERE answers = $1")
        .bind(&first)
        .fetch_one(&s.pool)
        .await
        .expect("an outcome names the first record");
    (record, outcome)
}

fn host(
    kind: &str,
    id: Option<&str>,
    action: &str,
) -> (String, Option<String>, String, Option<String>, String) {
    (
        "host".into(),
        Some("lab".into()),
        kind.into(),
        id.map(str::to_owned),
        action.into(),
    )
}

/// **Each of the five register verbs writes its first record and an outcome
/// naming it**: `authority init` and `authority rotate` on the server's
/// authority, `register`, `revoke` and `rotate` on the agent's row, each
/// with the host's `--author` claim and the verb as its action, and the
/// outcome `ok` as the answer is.
#[tokio::test]
async fn every_register_verb_is_audited_as_the_hosts() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);

    let fresh = tempfile::tempdir().unwrap();
    let mut init_cfg = cfg.clone();
    init_cfg.authority_dir = fresh.path().join("authority");
    let answer = super::verbs::authority_init(&lab.store, &init_cfg, &[], Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(
        audited(&lab.store, &answer.value).await,
        (host("authority", None, "authority init"), "ok".into())
    );

    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let authority = Authority::load(lab.authority.dir()).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let agent = answer.value["agent"].as_str().unwrap().to_owned();
    assert_eq!(
        audited(&lab.store, &answer.value).await,
        (host("agent", Some(&agent), "register"), "ok".into())
    );

    let answer = super::verbs::revoke(&lab.store, &agent, Plane::Gate, Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(
        audited(&lab.store, &answer.value).await,
        (host("agent", Some(&agent), "revoke"), "ok".into())
    );

    let answer = super::verbs::rotate(
        &lab.store,
        &cfg,
        &authority,
        &agent,
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(
        audited(&lab.store, &answer.value).await,
        (host("agent", Some(&agent), "rotate"), "ok".into())
    );

    let answer = super::verbs::authority_rotate(&lab.store, &cfg, &[], Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(
        audited(&lab.store, &answer.value).await,
        (host("authority", None, "authority rotate"), "ok".into())
    );
}

/// **A verb whose first record cannot be written does not act**: with the
/// first record refused, each of the five answers why and leaves what it
/// would have changed as it was: no authority created, no row registered
/// and no config written, no credential revoked, no pair rotated, and the
/// authority not replaced.
#[tokio::test]
async fn a_verb_whose_first_record_cannot_be_written_does_not_act() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let refuse = || FAIL_FIRST_RECORD.with(|f| f.set(true));
    let unaudited = |answer: &super::verbs::Answer| {
        assert!(!answer.ok, "{}", answer.value);
        assert!(
            answer.value["error"]
                .as_str()
                .is_some_and(|e| e.contains("first record could not be written")),
            "{}",
            answer.value
        );
    };

    let fresh = tempfile::tempdir().unwrap();
    let mut init_cfg = cfg.clone();
    init_cfg.authority_dir = fresh.path().join("authority");
    refuse();
    let answer = super::verbs::authority_init(&lab.store, &init_cfg, &[], Some("lab")).await;
    unaudited(&answer);
    assert!(
        Authority::load(&init_cfg.authority_dir).is_err(),
        "no authority was created"
    );

    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let authority = Authority::load(lab.authority.dir()).unwrap();
    refuse();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    unaudited(&answer);
    assert!(
        lab.store
            .resolve_agent(&format!("{}/karl", r#box))
            .await
            .is_err(),
        "no row was registered"
    );
    let configs = out.path().join(&r#box).join("karl");
    assert!(
        std::fs::read_dir(&configs).map_or(true, |mut d| d.next().is_none()),
        "no config was written"
    );

    // A registration that is audited, so the three verbs on a row have one.
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let agent = answer.value["agent"].as_str().unwrap().to_owned();
    let before = lab.store.resolve_agent(&agent).await.unwrap();

    refuse();
    let answer = super::verbs::revoke(&lab.store, &agent, Plane::Gate, Some("lab")).await;
    unaudited(&answer);
    let after = lab.store.resolve_agent(&agent).await.unwrap();
    assert_eq!(
        after.gate.state,
        CredentialState::Live,
        "nothing was revoked"
    );

    refuse();
    let answer = super::verbs::rotate(
        &lab.store,
        &cfg,
        &authority,
        &agent,
        out.path(),
        Some("lab"),
    )
    .await;
    unaudited(&answer);
    let after = lab.store.resolve_agent(&agent).await.unwrap();
    assert_eq!(
        (after.gate.fingerprint, after.admin.fingerprint),
        (
            before.gate.fingerprint.clone(),
            before.admin.fingerprint.clone()
        ),
        "nothing was rotated"
    );

    refuse();
    let answer = super::verbs::authority_rotate(&lab.store, &cfg, &[], Some("lab")).await;
    unaudited(&answer);
    assert_eq!(
        Authority::load(lab.authority.dir()).unwrap().fingerprint(),
        authority.fingerprint(),
        "the authority was not replaced"
    );
    let after = lab.store.resolve_agent(&agent).await.unwrap();
    assert_eq!(
        after.admin.state,
        CredentialState::Live,
        "no credential was revoked"
    );
}
