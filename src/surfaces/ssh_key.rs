//! conforms: web-an-ssh-keys-line-is-checked-for-its-form
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//!
//! **The operator's SSH public keys** (Spec 6.2): a console setting, for a
//! session whose person holds a live admin grant, among IAM's sub-tabs.
//! `GET /admin/ssh-key` shows every key held by its SHA-256 fingerprint,
//! OpenSSH's form, with its type, its comment, who added it and when. Two
//! writes, `POST /admin/ssh-key` (an add) and `POST
//! /admin/ssh-key/{sk}/remove`, each in the admin writes' order (Spec
//! 2.13): the form parsed to its shape, the admin gate with its one refusal
//! record, the key resolved where the write names one, then the audited
//! write with the admin as principal by `session`.
//!
//! **The console checks the line's form alone**: a key type of OpenSSH's
//! public-key format, base64 that decodes, and the type name inside the
//! decoded blob equal to the line's, a line of at most 16 KiB. Which types
//! a box's SSH door admits is the image's. **The line's text enters no
//! audit record**: the audit names the key's identity, `sk-`.

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::identity::is_ssh_key_id;
use crate::surfaces::admin::{
    NOT_AN_ADMIN, admin_session, fault, first_of, landed, refused, settled, step_four, writer,
};
use crate::surfaces::gate::Policy;

const PAGE: &str = "/admin/ssh-key";

/// The most bytes a key's line runs to.
pub(crate) const LINE_MOST: usize = 16 * 1024;

/// **The key types of OpenSSH's public-key format**, `ssh-keygen`'s and
/// `authorized_keys`' plain key types, a certificate being no key. DSA's
/// `ssh-dss` is not among them: OpenSSH 10.0 removed it.
const KEY_TYPES: [&str; 7] = [
    "ssh-ed25519",
    "ssh-rsa",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "sk-ssh-ed25519@openssh.com",
    "sk-ecdsa-sha2-nistp256@openssh.com",
];

/// A line whose form holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicKey {
    /// The line, trimmed.
    pub(crate) line: String,
    pub(crate) key_type: String,
    /// `SHA256:` and the blob's digest in base64 without padding.
    pub(crate) fingerprint: String,
    pub(crate) comment: String,
}

/// **A line checked for its form**, or why it does not hold: a type of
/// OpenSSH's public-key format, base64 that decodes, and the decoded blob's
/// own type name, its first string, equal to the line's.
pub(crate) fn parse(given: &str) -> Result<PublicKey, &'static str> {
    let line = given.trim();
    if line.is_empty() {
        return Err("a key's line is required");
    }
    if line.len() > LINE_MOST {
        return Err("a key's line is 16 KiB at most");
    }
    if line.chars().any(|c| c.is_control() && c != '\t') {
        return Err("a key is one line, holding no control character");
    }
    let blank = |c: char| c == ' ' || c == '\t';
    let Some((key_type, rest)) = line.split_once(blank) else {
        return Err("a key's line is its type, then its base64, then a comment if any");
    };
    if !KEY_TYPES.contains(&key_type) {
        return Err("the line's type is not one of OpenSSH's public-key types");
    }
    let rest = rest.trim_start_matches(blank);
    let (encoded, comment) = rest.split_once(blank).unwrap_or((rest, ""));
    let blob = STANDARD
        .decode(encoded)
        .map_err(|_| "the key's base64 does not decode")?;
    let named = blob
        .get(..4)
        .map(|n| u32::from_be_bytes([n[0], n[1], n[2], n[3]]) as usize)
        .and_then(|n| blob.get(4..4usize.checked_add(n)?))
        .ok_or("the decoded key names no type")?;
    if named != key_type.as_bytes() {
        return Err("the type named inside the key is not the line's");
    }
    Ok(PublicKey {
        line: line.to_owned(),
        key_type: key_type.to_owned(),
        fingerprint: format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(&blob))),
        comment: comment.trim().to_owned(),
    })
}

pub fn routes(policy: Policy) -> Router<Store> {
    let page_policy = policy.clone();
    let add_policy = policy.clone();
    Router::new()
        .route(
            PAGE,
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let policy = page_policy.clone();
                async move { page(store, &policy, headers).await }
            })
            .post(
                move |State(store): State<Store>, headers: HeaderMap, Form(ask): Form<AddAsk>| {
                    let policy = add_policy.clone();
                    async move { add(store, &policy, headers, ask).await }
                },
            ),
        )
        .route(
            "/admin/ssh-key/{sk}/remove",
            post(
                move |State(store): State<Store>, headers: HeaderMap, Path(sk): Path<String>| {
                    let policy = policy.clone();
                    async move { remove(store, &policy, headers, &sk).await }
                },
            ),
        )
}

pub(crate) struct KeyView {
    pub(crate) key_id: String,
    pub(crate) fingerprint: String,
    pub(crate) key_type: String,
    pub(crate) comment: String,
    pub(crate) added_by: String,
    pub(crate) added_at: String,
}

/// Every key held, as a page shows it: by its fingerprint, never its line.
pub(crate) async fn held(store: &Store) -> anyhow::Result<Vec<KeyView>> {
    Ok(store
        .ssh_keys()
        .await?
        .into_iter()
        .map(|k| {
            let (key_type, comment) = match parse(&k.line) {
                Ok(key) => (key.key_type, key.comment),
                Err(_) => ("unreadable".to_owned(), String::new()),
            };
            KeyView {
                key_id: k.key_id,
                fingerprint: k.fingerprint,
                key_type,
                comment,
                added_by: k.added_by,
                added_at: k.added_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            }
        })
        .collect())
}

#[derive(Template)]
#[template(path = "admin_ssh_key.html")]
struct SshKeyPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    keys: Vec<KeyView>,
    most: i64,
}

async fn page(store: Store, policy: &Policy, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let chrome = match super::Chrome::of(&store, &session.person_id).await {
        Ok(chrome) => chrome,
        Err(e) => return fault(e),
    };
    let keys = match held(&store).await {
        Ok(keys) => keys,
        Err(e) => return fault(e),
    };
    let page = SshKeyPage {
        here: "ssh-key",
        who: session.name.clone(),
        admin: true,
        chrome,
        keys,
        most: crate::store::ssh_key::KEYS_MOST,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct AddAsk {
    line: String,
}

async fn add(store: Store, policy: &Policy, headers: HeaderMap, ask: AddAsk) -> Response {
    const ACTION: &str = "ssh-key add";
    let key = match parse(&ask.line) {
        Ok(key) => key,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let key_id = match store.mint_key("sk").await {
        Ok(key_id) => key_id,
        Err(e) => return fault(e),
    };
    let target = Target::SshKey(&key_id);
    let session = match writer(&store, policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    step_four(async move {
        let first = match first_of(&store, &session, Target::SshKey(&key_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .add_ssh_key(
                &session.person_id,
                session.session_id,
                &first,
                &key_id,
                &key.line,
                &key.fingerprint,
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, PAGE)
    })
    .await
}

async fn remove(store: Store, policy: &Policy, headers: HeaderMap, key_id: &str) -> Response {
    const ACTION: &str = "ssh-key remove";
    if !is_ssh_key_id(key_id) {
        return (
            StatusCode::BAD_REQUEST,
            "a key's identity is sk- and sixteen lowercase hex",
        )
            .into_response();
    }
    let target = Target::SshKey(key_id);
    let session = match writer(&store, policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.ssh_key_held(key_id).await {
        Ok(true) => {}
        Ok(false) => return refused(&Refusal::NoSuchKey),
        Err(e) => return fault(e),
    }
    let key_id = key_id.to_owned();
    step_four(async move {
        let first = match first_of(&store, &session, Target::SshKey(&key_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .remove_ssh_key(&session.person_id, session.session_id, &first, &key_id)
            .await;
        settled(&store, &first, &written).await;
        landed(written, PAGE)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A blob of OpenSSH's wire form: the type as a string, then `rest`.
    /// No blob here is a real key.
    pub(crate) fn blob(key_type: &str, rest: &[u8]) -> String {
        let mut b = (key_type.len() as u32).to_be_bytes().to_vec();
        b.extend_from_slice(key_type.as_bytes());
        b.extend_from_slice(rest);
        STANDARD.encode(b)
    }

    #[test]
    fn a_line_of_each_type_holds_and_is_fingerprinted() {
        for key_type in KEY_TYPES {
            let encoded = blob(key_type, &[7u8; 32]);
            let key = parse(&format!("  {key_type} {encoded} ada@example.test \n")).unwrap();
            assert_eq!(key.key_type, key_type);
            assert_eq!(key.comment, "ada@example.test");
            assert_eq!(key.line, format!("{key_type} {encoded} ada@example.test"));
            let digest = Sha256::digest(STANDARD.decode(&encoded).unwrap());
            assert_eq!(
                key.fingerprint,
                format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
            );
        }
    }

    #[test]
    fn a_line_whose_form_does_not_hold_is_refused() {
        let ed = blob("ssh-ed25519", &[7u8; 32]);
        let long = format!("ssh-ed25519 {ed} {}", "c".repeat(LINE_MOST));
        let short_blob = STANDARD.encode([0u8, 0, 0, 40, b's', b's', b'h']);
        for (line, why) in [
            (String::new(), "a key's line is required"),
            ("ssh-ed25519".to_owned(), "a key's line is its type"),
            (
                format!("ssh-dss {}", blob("ssh-dss", &[1; 8])),
                "OpenSSH's public-key types",
            ),
            ("ssh-ed25519 !!!!".to_owned(), "does not decode"),
            (format!("ssh-rsa {ed}"), "is not the line's"),
            (format!("ssh-ed25519 {short_blob}"), "names no type"),
            (long, "16 KiB"),
            (
                format!("ssh-ed25519 {ed} a\nssh-ed25519 {ed} b"),
                "one line",
            ),
        ] {
            let refused = parse(&line).expect_err(&line);
            assert!(refused.contains(why), "{line:?}: {refused}");
        }
    }
}
