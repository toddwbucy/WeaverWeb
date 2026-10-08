//! The audit's table and its one writer, against a live PostgreSQL named by
//! `DATABASE_URL`; without it the store-bound tests pass by not running and
//! say so. Each statement that would change the table runs in a
//! transaction the test rolls back, so a perturbation that drops a trigger
//! never empties the shared scratch store's audit.

use super::Store;
use super::audit::{Principal, Target};
use super::read::tests::store;

const HOST: Principal<'static> = Principal::Host {
    author: Some("lab"),
};

async fn first(s: &Store) -> String {
    s.audit_first(HOST, Target::Authority, "audit test")
        .await
        .expect("the first record lands")
}

/// The error of `statement`, bound to the record `id` where given, run in a
/// transaction that is rolled back whatever happens; `None` where the
/// statement succeeded.
async fn refusal_of(s: &Store, statement: &'static str, id: Option<&str>) -> Option<String> {
    let mut tx = s.pool.begin().await.unwrap();
    let mut query = sqlx::query(statement);
    if let Some(id) = id {
        query = query.bind(id.to_owned());
    }
    let result = query.execute(&mut *tx).await;
    tx.rollback().await.unwrap();
    result.err().map(|e| e.to_string())
}

/// **The audit is append-only against this crate's statements**
/// (migration `0014`): an update and a delete of a record are refused by
/// the row trigger, and a truncate by the statement trigger, which no row
/// trigger sees.
#[tokio::test]
async fn an_audit_record_is_never_updated_deleted_or_truncated() {
    let Some(s) = store().await else { return };
    let id = first(&s).await;
    let update = refusal_of(
        &s,
        "UPDATE audit SET action = 'rewritten' WHERE audit_id = $1",
        Some(&id),
    )
    .await;
    assert!(
        update.as_deref().is_some_and(|e| e.contains("append-only")),
        "an update is refused: {update:?}"
    );
    let delete = refusal_of(&s, "DELETE FROM audit WHERE audit_id = $1", Some(&id)).await;
    assert!(
        delete.as_deref().is_some_and(|e| e.contains("append-only")),
        "a delete is refused: {delete:?}"
    );
    let truncate = refusal_of(&s, "TRUNCATE audit", None).await;
    assert!(
        truncate
            .as_deref()
            .is_some_and(|e| e.contains("append-only")),
        "a truncate is refused: {truncate:?}"
    );
    let still: i64 = sqlx::query_scalar("SELECT count(*) FROM audit WHERE audit_id = $1")
        .bind(&id)
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert_eq!(still, 1, "the record stands");
}

/// **A record's principal and its method agree** (design section 10): each
/// method belongs to exactly one principal, so a host record carrying a
/// person's method is refused by the store.
#[tokio::test]
async fn a_principal_and_its_method_never_disagree() {
    let Some(s) = store().await else { return };
    let mut tx = s.pool.begin().await.unwrap();
    let refused = sqlx::query(
        "INSERT INTO audit (principal, method, target_kind, action) \
         VALUES ('host', 'session', 'authority', 'audit test')",
    )
    .execute(&mut *tx)
    .await;
    tx.rollback().await.unwrap();
    let error = refused.expect_err("a host record with a session method is refused");
    assert!(
        error
            .to_string()
            .contains("audit_principal_and_method_agree"),
        "{error}"
    );
}

/// **The outcome names its first and copies what it was about**: the same
/// principal, claim, target and action, with `ok` or `failed`; and a first
/// record is answered once.
#[tokio::test]
async fn an_outcome_names_its_first_once() {
    let Some(s) = store().await else { return };
    let id = first(&s).await;
    let outcome = s.audit_outcome(&id, false).await.unwrap();
    let row: (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT principal, method, claimed_author, target_kind, action, outcome \
         FROM audit WHERE audit_id = $1 AND answers = $2",
    )
    .bind(&outcome)
    .bind(&id)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "host".into(),
            "host".into(),
            Some("lab".into()),
            "authority".into(),
            "audit test".into(),
            Some("failed".into())
        )
    );
    assert!(
        s.audit_outcome(&id, true).await.is_err(),
        "a second outcome for one first record is refused"
    );
    assert!(
        s.audit_outcome(&outcome, true).await.is_err(),
        "an outcome answers a first record, not another outcome"
    );
}

/// **Nothing but the audit's writer inserts into it**: every source file
/// outside `src/store/audit.rs` and this test is read for an insert into
/// the table, so the one call graph that writes the audit stays one.
#[test]
fn only_the_writer_inserts_into_the_audit() {
    fn walk(dir: &std::path::Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).unwrap().to_lowercase();
                let name = path.to_string_lossy().replace('\\', "/");
                let own = name.ends_with("src/store/audit.rs")
                    || name.ends_with("src/store/audit_tests.rs");
                if !own && text.contains("insert into audit") {
                    found.push(name);
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(
        found.is_empty(),
        "inserts into the audit outside its writer: {found:?}"
    );
}

/// **Every check of the audit is false, never unknown, on a missing value**
/// (Codex on PR #25): a CHECK passes when its expression is NULL, so for
/// each check guarding a member that may be missing, the row with that
/// member missing is inserted, in a transaction rolled back, and refused by
/// that check by name.
#[tokio::test]
async fn every_check_refuses_its_member_missing() {
    let Some(s) = store().await else { return };
    let first = first(&s).await;
    let cases: [(&str, &str, &str); 3] = [
        (
            "an outcome record with no outcome",
            "INSERT INTO audit (principal, method, target_kind, action, answers) \
             VALUES ('host', 'host', 'authority', 'audit test', $1)",
            "audit_a_record_is_first_outcome_or_refusal",
        ),
        (
            "an agent target with no identity",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('host', 'host', 'agent', 'audit test') RETURNING $1::text",
            "audit_an_agent_target_names_its_row",
        ),
        (
            "a person with no identity",
            "INSERT INTO audit (principal, method, target_kind, action) \
             VALUES ('person', 'session', 'authority', 'audit test') RETURNING $1::text",
            "audit_principal_and_method_agree",
        ),
    ];
    for (what, statement, check) in cases {
        let mut tx = s.pool.begin().await.unwrap();
        let result = sqlx::query(statement).bind(&first).execute(&mut *tx).await;
        tx.rollback().await.unwrap();
        let error = result
            .err()
            .unwrap_or_else(|| panic!("{what} landed where {check} should refuse it"));
        assert!(error.to_string().contains(check), "{what}: {error}");
    }
}
