//! conforms: web-an-ssh-keys-line-is-checked-for-its-form
//!
//! The operator's SSH public keys (Spec 6.2, `src/surfaces/ssh_key.rs`,
//! `src/store/ssh_key.rs`). A line whose type, base64 or inner type name
//! does not hold is refused, saving nothing and recorded nowhere. A stored
//! key renders by its fingerprint, on its page and on the drives page, and
//! its line enters no audit row. A person without an admin grant is refused
//! with one record. A key is held once, and sixteen at most. A removal
//! takes the key. Each test runs on a store of its own. No key is a real
//! one.

use axum::http::StatusCode;
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use sha2::{Digest, Sha256};

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::passkeys_tests::authenticator;
use crate::setup_tests::{begin, page, rig};
use crate::store::Store;

/// A blob of OpenSSH's wire form naming `inner`, its body drawn from
/// `seed`. No blob here is a real key.
fn blob(inner: &str, seed: u8) -> Vec<u8> {
    let mut b = (inner.len() as u32).to_be_bytes().to_vec();
    b.extend_from_slice(inner.as_bytes());
    b.extend_from_slice(&[0, 0, 0, 32]);
    b.extend_from_slice(&[seed; 32]);
    b
}

/// A line of `key_type` whose blob names the same type, drawn from `seed`.
pub(crate) fn line(key_type: &str, seed: u8) -> String {
    format!(
        "{key_type} {} ada@example.test",
        STANDARD.encode(blob(key_type, seed))
    )
}

/// The fingerprint OpenSSH gives the key `line` names, computed here.
fn fingerprint(line: &str) -> String {
    let encoded = line.split(' ').nth(1).unwrap();
    let digest = Sha256::digest(STANDARD.decode(encoded).unwrap());
    format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
}

async fn keys(s: &Store) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM operator_ssh_key")
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The audit's records about keys for `action`: first records, `ok`
/// outcomes, `failed` outcomes and refusals.
async fn audited(s: &Store, action: &str) -> (i64, i64, i64, i64) {
    let count = |clause: &'static str| {
        let query = format!(
            "SELECT count(*) FROM audit WHERE target_kind = 'ssh_key' \
             AND target_id ~ '^sk-[0-9a-f]{{16}}$' AND action = $1 AND {clause}"
        );
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
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
        count("refusal IS NOT NULL").await,
    )
}

/// Every audit row as text, for a search.
async fn audit_text(s: &Store) -> String {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT string_agg(row_to_json(a)::text, E'\\n') FROM audit a",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap()
    .unwrap_or_default()
}

async fn add(app: &axum::Router, session: &str, line: &str) -> (StatusCode, String) {
    let (status, _, body) = send_as(
        app,
        "POST",
        "/admin/ssh-key",
        Some(form(&[("line", line)])),
        session,
    )
    .await;
    (status, body)
}

/// **A line whose form does not hold is refused, saving nothing**: a type
/// outside OpenSSH's public-key format, base64 that does not decode, a blob
/// whose own type name is not the line's, a blob naming no type, and a line
/// past 16 KiB, each at 400 before any record.
#[tokio::test]
async fn a_line_whose_form_does_not_hold_is_refused_saving_nothing() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let ed = STANDARD.encode(blob("ssh-ed25519", 1));
    for (given, why) in [
        (
            format!("ssh-foo {}", STANDARD.encode(blob("ssh-foo", 1))),
            "OpenSSH's public-key types",
        ),
        ("ssh-ed25519 AAAA@@@@".to_owned(), "does not decode"),
        (format!("ssh-rsa {ed}"), "is not the line's"),
        (
            format!("ssh-ed25519 {}", STANDARD.encode([0u8, 0, 1, 0])),
            "names no type",
        ),
        (
            format!("ssh-ed25519 {ed} {}", "c".repeat(16 * 1024)),
            "16 KiB",
        ),
    ] {
        let (status, body) = add(&rig.app, &ada, &given).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{given:.60}: {body}");
        assert!(body.contains(why), "{given:.60}: {body}");
        assert_eq!(keys(s).await, 0, "{given:.60} saves nothing");
        assert_eq!(audited(s, "ssh-key add").await, (0, 0, 0, 0));
    }
}

/// **A stored key renders by its fingerprint and its line enters no audit
/// row**: the page and the drives page show the fingerprint OpenSSH gives
/// it, neither shows the key's base64, and no audit row holds it.
#[tokio::test]
async fn a_stored_key_renders_by_its_fingerprint_and_its_line_enters_no_audit_row() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let given = line("ssh-ed25519", 7);
    let encoded = given.split(' ').nth(1).unwrap().to_owned();
    let (status, body) = add(&rig.app, &ada, &format!("  {given}\n")).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(keys(s).await, 1);
    assert_eq!(audited(s, "ssh-key add").await, (1, 1, 0, 0));
    let held = s.ssh_keys().await.unwrap();
    assert_eq!(held[0].line, given, "the line is kept trimmed, in clear");
    assert_eq!(held[0].fingerprint, fingerprint(&given));

    let keys_page = page(&rig.app, "/admin/ssh-key", &ada).await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let drives = page(&rig.app, &format!("/setup/{}/drives", setup.unwrap()), &ada).await;
    for shown in [&keys_page, &drives] {
        assert!(shown.contains(&fingerprint(&given)), "{shown}");
        assert!(!shown.contains(&encoded), "the line is not shown: {shown}");
    }
    let audit = audit_text(s).await;
    assert!(!audit.is_empty());
    assert!(
        !audit.contains(&encoded),
        "the line enters no audit row: {audit}"
    );
    assert!(!audit.contains("ada@example.test"), "{audit}");
}

/// **A person without an admin grant is refused**: the page answers 403,
/// and each write is one refusal record, writing nothing.
#[tokio::test]
async fn a_person_without_an_admin_grant_is_refused() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let (status, body) = add(&rig.app, &ada, &line("ssh-ed25519", 1)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let key_id = s.ssh_keys().await.unwrap()[0].key_id.clone();

    let (status, _, body) = send_as(&rig.app, "GET", "/admin/ssh-key", None, &bea).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = add(&rig.app, &bea, &line("ssh-ed25519", 2)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, _, body) = send_as(
        &rig.app,
        "POST",
        &format!("/admin/ssh-key/{key_id}/remove"),
        Some(String::new()),
        &bea,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(keys(s).await, 1, "nothing written");
    assert_eq!(audited(s, "ssh-key add").await, (1, 1, 0, 1));
    assert_eq!(audited(s, "ssh-key remove").await, (0, 0, 0, 1));
}

/// **A key is held once, and sixteen at most**: the same key again is
/// refused at 409, as is a seventeenth, each first record's outcome failed.
#[tokio::test]
async fn a_key_is_held_once_and_sixteen_at_most() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (status, _) = add(&rig.app, &ada, &line("ssh-ed25519", 0)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, body) = add(&rig.app, &ada, &line("ssh-ed25519", 0)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("held already"), "{body}");
    for seed in 1..16 {
        let (status, body) = add(&rig.app, &ada, &line("ecdsa-sha2-nistp256", seed)).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{seed}: {body}");
    }
    assert_eq!(keys(s).await, 16);
    let (status, body) = add(&rig.app, &ada, &line("ssh-rsa", 99)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("16 SSH keys"), "{body}");
    assert_eq!(keys(s).await, 16);
    assert_eq!(audited(s, "ssh-key add").await, (18, 16, 2, 0));
}

/// **A removal takes the key**, audited, and a key that is not held is not
/// found, a malformed one refused, each recorded nowhere.
#[tokio::test]
async fn a_removal_takes_the_key_and_an_unknown_key_is_not_found() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (status, _) = add(&rig.app, &ada, &line("ssh-ed25519", 3)).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let key_id = s.ssh_keys().await.unwrap()[0].key_id.clone();
    let remove = |key: String| {
        let app = rig.app.clone();
        let ada = ada.clone();
        async move {
            send_as(
                &app,
                "POST",
                &format!("/admin/ssh-key/{key}/remove"),
                Some(String::new()),
                &ada,
            )
            .await
        }
    };
    let (status, _, body) = remove(key_id.clone()).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(keys(s).await, 0);
    assert_eq!(audited(s, "ssh-key remove").await, (1, 1, 0, 0));
    let (status, _, body) = remove(key_id).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (status, _, body) = remove("sk-12".to_owned()).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(audited(s, "ssh-key remove").await, (1, 1, 0, 0));
}
