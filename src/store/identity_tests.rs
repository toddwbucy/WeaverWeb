//! The identity rows and their rules at the store (migration `0015`,
//! `src/store/identity.rs`). The pure rules run without a database; the
//! store-bound ones each run on a database of their own (`fresh_store`),
//! and every statement that would change a row is rolled back.

use super::identity::{bearer, name_key};
use super::read::tests::fresh_store;

/// **A name's canonical form folds case, width and composition** (design
/// section 6): the compatibility caseless form of D146, full case folding
/// included, so `Ada`, `ADA`, `Ada` in full-width letters, and a name
/// with an accent composed or decomposed are each one name, and surrounding
/// white space is no part of it.
#[test]
fn a_names_canonical_form_folds_case_width_and_composition() {
    assert_eq!(name_key("Ada"), name_key("ADA"));
    assert_eq!(
        name_key("Ada"),
        name_key("\u{FF21}\u{FF44}\u{FF41}"),
        "full-width"
    );
    assert_eq!(
        name_key("Ren\u{E9}e"),
        name_key("Rene\u{301}e"),
        "composition"
    );
    assert_eq!(
        name_key("Stra\u{DF}e"),
        name_key("STRASSE"),
        "full case folding"
    );
    assert_eq!(name_key("  Ada\t"), name_key("Ada"), "white space trimmed");
    assert_ne!(name_key("Ada"), name_key("Adb"));
}

/// **A bearer is the operating system's randomness, not a counter or a
/// clock** (design section 6): consecutive bearers share no prefix of eight
/// bytes, which a counter's or a clock's high bytes would, and the chance
/// of two random ones doing so is one in 2^64.
#[test]
fn consecutive_bearers_share_no_prefix() {
    let drawn: Vec<[u8; 32]> = (0..16).map(|_| bearer()).collect();
    for pair in drawn.windows(2) {
        assert_ne!(pair[0][..8], pair[1][..8], "two bearers share a prefix");
    }
}

/// **Every new check is false, never unknown, on a missing value**
/// (0014's rule carried to 0015): for each check guarding a member that may
/// be missing, the row with that member missing is refused by that check by
/// name.
#[tokio::test]
async fn every_identity_check_refuses_its_member_missing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    sqlx::query(
        "INSERT INTO person (person_id, name, name_key) VALUES ('pe-0000000000000001', 'a', 'a')",
    )
    .execute(&s.pool)
    .await
    .unwrap();
    let cases: [(&str, &str, &str); 6] = [
        (
            "a token ended with no reason",
            "INSERT INTO enrollment_token (token_digest, person_id, expires_at, ended_at) \
             VALUES (repeat('a', 64), 'pe-0000000000000001', now() + interval '1 hour', now())",
            "enrollment_token_ends_with_a_reason",
        ),
        (
            "a role's verb missing",
            "INSERT INTO role (name, scope, verbs) VALUES ('watcher', 'agent', ARRAY['show', NULL])",
            "role_verbs_are_the_vocabulary",
        ),
        (
            "a per-agent grant with no agent",
            "INSERT INTO role_grant (person_id, role) VALUES ('pe-0000000000000001', 'observer')",
            "role_grant_admin_is_server_wide_alone",
        ),
        (
            "a person target with no identity",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('host', 'host', 'person', 'test')",
            "audit_a_person_target_names_its_row",
        ),
        (
            "a grant target with no identity",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('host', 'host', 'grant', 'test')",
            "audit_a_grant_target_names_its_row",
        ),
        (
            "a role target with no name",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('host', 'host', 'role', 'test')",
            "audit_a_role_target_names_one",
        ),
    ];
    for (what, statement, check) in cases {
        let mut tx = s.pool.begin().await.unwrap();
        let result = sqlx::query(statement).execute(&mut *tx).await;
        tx.rollback().await.unwrap();
        let error = result
            .err()
            .unwrap_or_else(|| panic!("{what} landed where {check} should refuse it"));
        assert!(error.to_string().contains(check), "{what}: {error}");
    }
}

/// **Every target the audit writes is checked by its kind** (0018's
/// sweep): a passkey target out of its `pk-` shape, an authority target
/// naming a row, and a target of no kind the writer knows are each refused
/// by their check's name.
#[tokio::test]
async fn every_audit_target_is_checked_by_its_kind() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let cases: [(&str, &str, &str); 4] = [
        (
            "a passkey target out of shape",
            "INSERT INTO audit (principal, method, target_kind, target_id, action) \
             VALUES ('host', 'host', 'passkey', 'pk-0123', 'test')",
            "audit_a_passkey_target_names_its_row",
        ),
        (
            "a passkey target with no identity",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('host', 'host', 'passkey', 'test')",
            "audit_a_passkey_target_names_its_row",
        ),
        (
            "an authority target naming a row",
            "INSERT INTO audit (principal, method, target_kind, target_id, action) \
             VALUES ('host', 'host', 'authority', 'ag-0123456789abcdef', 'test')",
            "audit_an_authority_target_names_no_row",
        ),
        (
            "a target of no known kind",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('host', 'host', 'session', 'test')",
            "audit_a_target_is_a_known_kind",
        ),
    ];
    for (what, statement, check) in cases {
        let mut tx = s.pool.begin().await.unwrap();
        let result = sqlx::query(statement).execute(&mut *tx).await;
        tx.rollback().await.unwrap();
        let error = result
            .err()
            .unwrap_or_else(|| panic!("{what} landed where {check} should refuse it"));
        assert!(error.to_string().contains(check), "{what}: {error}");
    }
}

/// **The audit's person is a person** (0015's foreign key): a record naming
/// a person no row holds is refused.
#[tokio::test]
async fn the_audit_names_no_person_that_is_not_one() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let mut tx = fresh.store.pool.begin().await.unwrap();
    let result = sqlx::query(
        "INSERT INTO audit (principal, person_id, method, target_kind, action) \
         VALUES ('person', 'pe-0000000000000009', 'session', 'authority', 'test')",
    )
    .execute(&mut *tx)
    .await;
    tx.rollback().await.unwrap();
    let error = result.expect_err("a record naming no person is refused");
    assert!(
        error.to_string().contains("audit_person_is_a_person"),
        "{error}"
    );
}

/// **The admin role is fixed by the store** (Spec 2.13): an update or a
/// delete of its row is refused by the trigger, whatever statement tries.
#[tokio::test]
async fn the_admin_role_is_fixed_by_the_store() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    for statement in [
        "UPDATE role SET verbs = '{show}' WHERE name = 'admin'",
        "DELETE FROM role WHERE name = 'admin'",
    ] {
        let mut tx = fresh.store.pool.begin().await.unwrap();
        let result = sqlx::query(statement).execute(&mut *tx).await;
        tx.rollback().await.unwrap();
        let error = result.expect_err(statement);
        assert!(
            error.to_string().contains("fixed by the store"),
            "{statement}: {error}"
        );
    }
}

/// **Migration 0016 widens the seeded `operator` role and no other**
/// (WeaverAgent's A3.2, the operator's ruling of 2026-10-08): on a fresh
/// store `operator` carries the three save-point verbs at version 2 and
/// `observer` is as seeded; the migration run again over an `operator` an
/// admin narrowed leaves it as it stands, and over 0015's seed written in
/// another order widens it.
#[tokio::test]
async fn the_seeded_operator_gains_the_save_point_verbs_and_an_edited_one_is_left() {
    const MIGRATION: &str = include_str!("../../migrations/0016_the_save_point_verbs.sql");
    const WIDENED: [&str; 9] = [
        "show",
        "validate",
        "load",
        "unload",
        "stop",
        "save-point",
        "restore",
        "force-unload",
        "turn",
    ];
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let operator = s.role("operator").await.unwrap().unwrap();
    assert_eq!(operator.verbs, WIDENED);
    assert_eq!(operator.version, 2);
    let observer = s.role("observer").await.unwrap().unwrap();
    assert_eq!(observer.verbs, ["show"]);
    assert_eq!(observer.version, 1);

    sqlx::query(
        "UPDATE role SET verbs = '{show,turn}', version = version + 1 WHERE name = 'operator'",
    )
    .execute(&s.pool)
    .await
    .unwrap();
    sqlx::raw_sql(MIGRATION).execute(&s.pool).await.unwrap();
    let operator = s.role("operator").await.unwrap().unwrap();
    assert_eq!(
        operator.verbs,
        ["show", "turn"],
        "an operator role an admin narrowed was left as it stands"
    );
    assert_eq!(operator.version, 3);

    sqlx::query(
        "UPDATE role SET verbs = '{turn,stop,unload,load,validate,show}', version = version + 1 WHERE name = 'operator'",
    )
    .execute(&s.pool)
    .await
    .unwrap();
    sqlx::raw_sql(MIGRATION).execute(&s.pool).await.unwrap();
    let operator = s.role("operator").await.unwrap().unwrap();
    assert_eq!(operator.verbs, WIDENED);
    assert_eq!(operator.version, 5);
}
