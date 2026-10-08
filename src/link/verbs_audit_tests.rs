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
    assert!(
        !out.path().join(&r#box).exists(),
        "no directory was created, and so no config written"
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

/// A private directory, as a verb's checks require it.
fn private_dir(path: &std::path::Path) {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .recursive(true)
        .create(path)
        .unwrap();
}

/// **A refused first record leaves the agent's directory exactly as it was**
/// (Codex on PR #25): the inspection before the first record reads and
/// never makes or unlinks. Half a staged pair, which a verb that acts
/// discards, stands untouched after `register` and after `rotate` whose
/// first record was refused.
#[tokio::test]
async fn a_refused_first_record_leaves_the_agents_directory_as_it_was() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let authority = Authority::load(lab.authority.dir()).unwrap();
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let dir = out.path().join(&r#box).join("karl");
    private_dir(&dir);
    let half = dir.join("gate-con.toml.staging");
    std::fs::write(&half, "a half pair a crashed run left").unwrap();
    let listing = |dir: &std::path::Path| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    };
    let before = listing(&dir);

    FAIL_FIRST_RECORD.with(|f| f.set(true));
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
    assert!(!answer.ok, "{}", answer.value);
    assert_eq!(
        listing(&dir),
        before,
        "register left the directory as it was"
    );

    // An agent of the same box and name, registered elsewhere, so `rotate`
    // has a row; its directory here still holds only the half pair.
    let elsewhere = tempfile::tempdir().unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &authority,
        &r#box,
        "karl",
        elsewhere.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let agent = answer.value["agent"].as_str().unwrap().to_owned();
    FAIL_FIRST_RECORD.with(|f| f.set(true));
    let answer = super::verbs::rotate(
        &lab.store,
        &cfg,
        &authority,
        &agent,
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    assert_eq!(listing(&dir), before, "rotate left the directory as it was");
    assert!(half.exists(), "the half pair was not discarded");
}

/// **A rotation's recovery is audited on the row whose pair it publishes**
/// (Codex on PR #25): an agent registered, then registered again from
/// another directory, which retires the first row; the replacement's
/// configs staged under the first directory, as a run whose answer was
/// lost leaves them. A `rotate` asked by the retired row's identity
/// publishes the replacement's pair, and its record names the replacement.
#[tokio::test]
async fn a_rotations_recovery_names_the_row_it_publishes() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let authority = Authority::load(lab.authority.dir()).unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let register = |out: std::path::PathBuf| {
        let (store, cfg, authority, r#box) = (
            lab.store.clone(),
            cfg.clone(),
            Authority::load(lab.authority.dir()).unwrap(),
            r#box.clone(),
        );
        async move {
            let answer =
                super::verbs::register(&store, &cfg, &authority, &r#box, "karl", &out, Some("lab"))
                    .await;
            assert!(answer.ok, "{}", answer.value);
            answer.value["agent"].as_str().unwrap().to_owned()
        }
    };
    let first_out = tempfile::tempdir().unwrap();
    let retired = register(first_out.path().to_owned()).await;
    let second_out = tempfile::tempdir().unwrap();
    let replacement = register(second_out.path().to_owned()).await;
    assert_ne!(retired, replacement);

    let from = second_out.path().join(&r#box).join("karl");
    let to = first_out.path().join(&r#box).join("karl");
    for config in ["gate-con.toml", "admin-con.toml"] {
        std::fs::copy(from.join(config), to.join(format!("{config}.staging"))).unwrap();
    }
    let answer = super::verbs::rotate(
        &lab.store,
        &cfg,
        &authority,
        &retired,
        first_out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(answer.value["agent"], Value::String(replacement.clone()));
    assert_eq!(
        audited(&lab.store, &answer.value).await,
        (host("agent", Some(&replacement), "rotate"), "ok".into())
    );
}

/// **A host's `--author` claim of a person's identity is refused before any
/// record** (Spec 3.2): each register verb, asked with the author
/// `pe-0123456789abcdef`, refuses saying an identity is never a claim, and
/// leaves what it would have changed as it was, on the same setups as the
/// refused first record above.
#[tokio::test]
async fn a_register_verb_asked_by_an_identitys_shape_does_not_act() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let unaudited = |answer: &super::verbs::Answer| {
        assert!(!answer.ok, "{}", answer.value);
        assert!(
            answer.value["error"]
                .as_str()
                .is_some_and(|e| e.contains("an identity is never a claim")),
            "{}",
            answer.value
        );
    };

    let fresh = tempfile::tempdir().unwrap();
    let mut init_cfg = cfg.clone();
    init_cfg.authority_dir = fresh.path().join("authority");
    let answer =
        super::verbs::authority_init(&lab.store, &init_cfg, &[], Some("pe-0123456789abcdef")).await;
    unaudited(&answer);
    assert!(
        Authority::load(&init_cfg.authority_dir).is_err(),
        "no authority was created"
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
        Some("pe-0123456789abcdef"),
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
    assert!(
        !out.path().join(&r#box).exists(),
        "no directory was created, and so no config written"
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
    let answer =
        super::verbs::revoke(&lab.store, &agent, Plane::Gate, Some("pe-0123456789abcdef")).await;
    unaudited(&answer);
    let after = lab.store.resolve_agent(&agent).await.unwrap();
    assert_eq!(
        after.gate.state,
        CredentialState::Live,
        "nothing was revoked"
    );
    let answer = super::verbs::rotate(
        &lab.store,
        &cfg,
        &authority,
        &agent,
        out.path(),
        Some("pe-0123456789abcdef"),
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
    let answer =
        super::verbs::authority_rotate(&lab.store, &cfg, &[], Some("pe-0123456789abcdef")).await;
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
