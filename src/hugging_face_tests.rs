//! conforms: web-a-person-without-an-admin-grant-causes-no-request-to-hugging-face
//! conforms: web-the-hugging-face-token-is-held-sealed
//!
//! The Hugging Face account (Spec 2.15, `src/surfaces/hugging_face.rs`,
//! `src/store/hugging_face.rs`, `src/hugging_face/`): a connect keeps the
//! token sealed and shows the account and role Hugging Face answered. A
//! token Hugging Face refuses saves nothing and is one refusal record. A
//! person without an admin grant causes no request to Hugging Face. The
//! token enters no audit row, log line, page or error. A token Hugging Face
//! later refuses is shown refused and stands until a disconnect. A lost key
//! says to connect again. Each write checks the version read. The tests hold
//! the gated path and `authority seal-key` too. Every test runs against a
//! fake Hugging Face served in-process, and the DB-backed ones each on a
//! store of their own. No token, account or digest is a real one, and no
//! test token takes Hugging Face's own prefix.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde_json::json;

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::hugging_face::seal::{SEAL_KEY, SealKey, write_beside};
use crate::hugging_face::{Access, Client, Gated, ModelAt, RepoId, Token};
use crate::link::authority::Authority;
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys};
use crate::store::Store;
use crate::store::read::tests::{Fresh, fresh_store};
use crate::surfaces::gate;
use crate::surfaces::hugging_face::{Seams, routes};
use crate::test_log::Captured;

/// Three tokens the fake knows, one of each role, and one it does not.
/// None is of Hugging Face's own form.
const READ: &str = "test-token-read-0001";
const WRITE: &str = "test-token-write-0002";
const FINE: &str = "test-token-fine-0003";
const UNKNOWN: &str = "test-token-unknown-0004";

/// The commit every revision of the fake names.
const COMMIT: &str = "27b350a791e81d9a4d1ddca1c49282e9ec533768";
const CONFIG_BLOB: &str = "0123456789abcdef0123456789abcdef01234567";
const MODEL_BLOB: &str = "89abcdef0123456789abcdef0123456789abcdef";
const MODEL_SHA: &str = "c214aa19c214aa19c214aa19c214aa19c214aa19c214aa19c214aa19c214aa19";

/// **The fake Hugging Face**: `whoami-v2` by bearer, two repositories'
/// listings (`org/open` and the gated `org/gated`), and a file's `HEAD`
/// through its commit. It counts every request it is sent.
#[derive(Default)]
pub(crate) struct Fake {
    requests: AtomicUsize,
    refused: std::sync::Mutex<HashSet<String>>,
    granted: std::sync::Mutex<HashSet<String>>,
    broken: AtomicBool,
}

impl Fake {
    pub(crate) fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    fn refuse(&self, token: &str) {
        self.refused.lock().unwrap().insert(token.to_owned());
    }

    fn accept(&self, token: &str) {
        self.refused.lock().unwrap().remove(token);
    }

    fn grant(&self, token: &str) {
        self.granted.lock().unwrap().insert(token.to_owned());
    }

    /// The request counted, and its bearer.
    fn bearer(&self, headers: &HeaderMap) -> Option<String> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::to_owned)
    }
}

fn with_code(status: StatusCode, code: &'static str) -> Response {
    (status, [("x-error-code", code)], "").into_response()
}

async fn whoami(State(fake): State<Arc<Fake>>, headers: HeaderMap) -> Response {
    let bearer = fake.bearer(&headers);
    if fake.broken.load(Ordering::SeqCst) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let roles: HashMap<&str, &str> = [(READ, "read"), (WRITE, "write"), (FINE, "fineGrained")]
        .into_iter()
        .collect();
    match bearer.as_deref() {
        Some(token) if !fake.refused.lock().unwrap().contains(token) => match roles.get(token) {
            Some(role) => axum::Json(json!({
                "type": "user",
                "name": "ada-hf",
                "auth": {
                    "type": "access_token",
                    "accessToken": { "displayName": "test", "role": role },
                },
            }))
            .into_response(),
            None => StatusCode::UNAUTHORIZED.into_response(),
        },
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

async fn listing(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path((owner, name, revision)): Path<(String, String, String)>,
) -> Response {
    fake.bearer(&headers);
    let gated = match (owner.as_str(), name.as_str()) {
        ("org", "open") => json!(false),
        ("org", "gated") => json!("manual"),
        _ => return with_code(StatusCode::NOT_FOUND, "RepoNotFound"),
    };
    if revision != "main" && revision != COMMIT {
        return with_code(StatusCode::NOT_FOUND, "RevisionNotFound");
    }
    axum::Json(json!({
        "id": format!("{owner}/{name}"),
        "sha": COMMIT,
        "gated": gated,
        "private": false,
        "siblings": [
            { "rfilename": "config.json", "size": 2, "blobId": CONFIG_BLOB },
            { "rfilename": "model.gguf", "size": 5, "blobId": MODEL_BLOB,
              "lfs": { "sha256": MODEL_SHA, "size": 5, "pointerSize": 130 } },
        ],
    }))
    .into_response()
}

async fn resolve(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    Path((owner, name, commit, _path)): Path<(String, String, String, String)>,
) -> Response {
    let bearer = fake.bearer(&headers);
    if commit != COMMIT {
        return with_code(StatusCode::NOT_FOUND, "RevisionNotFound");
    }
    let found = (
        StatusCode::FOUND,
        [(header::LOCATION, "http://cdn.invalid/blob")],
    );
    match (owner.as_str(), name.as_str()) {
        ("org", "open") => found.into_response(),
        ("org", "gated") => match bearer {
            Some(token) if fake.granted.lock().unwrap().contains(&token) => found.into_response(),
            Some(_) => with_code(StatusCode::FORBIDDEN, "GatedRepo"),
            None => with_code(StatusCode::UNAUTHORIZED, "GatedRepo"),
        },
        _ => with_code(StatusCode::NOT_FOUND, "RepoNotFound"),
    }
}

/// The fake served on a port of its own, and its base.
pub(crate) async fn fake_hugging_face() -> (Arc<Fake>, String) {
    let fake = Arc::new(Fake::default());
    let router = axum::Router::new()
        .route("/api/whoami-v2", get(whoami))
        .route(
            "/api/models/{owner}/{name}/revision/{revision}",
            get(listing),
        )
        .route("/{owner}/{name}/resolve/{commit}/{*path}", get(resolve))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await });
    (fake, base)
}

struct Rig {
    fresh: Fresh,
    dir: tempfile::TempDir,
    fake: Arc<Fake>,
    app: axum::Router,
}

impl Rig {
    fn store(&self) -> &Store {
        &self.fresh.store
    }

    fn authority_dir(&self) -> std::path::PathBuf {
        self.dir.path().join("authority")
    }
}

async fn rig() -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = crate::test_dir::tempdir().unwrap();
    Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
    let (fake, base) = fake_hugging_face().await;
    let policy = gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
        write_bound: Duration::from_secs(60),
    };
    let surface = gate::guard(
        crate::surfaces::script_policy(routes(Seams {
            policy: policy.clone(),
            client: Client::with_base(&base).unwrap(),
            authority_dir: dir.path().join("authority"),
            puller: crate::hugging_face::pull::Puller::start(
                fresh.store.clone(),
                Client::with_base(&base).unwrap(),
                None,
                dir.path().join("authority"),
                crate::hugging_face::pull::Settings::default(),
            ),
        }))
        .with_state(fresh.store.clone()),
        policy,
    );
    let app = app(&fresh.store, Some(passkeys())).merge(surface);
    Some(Rig {
        fresh,
        dir,
        fake,
        app,
    })
}

async fn connect(rig: &Rig, session: &str, token: &str, version: &str) -> (StatusCode, String) {
    let (status, headers, body) = send_as(
        &rig.app,
        "POST",
        "/admin/hugging-face/connect",
        Some(form(&[("token", token), ("version", version)])),
        session,
    )
    .await;
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap()),
        Some("no-store"),
        "every answer of the surface is no-store"
    );
    (status, body)
}

async fn post(rig: &Rig, session: &str, verb: &str, version: &str) -> (StatusCode, String) {
    let (status, _, body) = send_as(
        &rig.app,
        "POST",
        &format!("/admin/hugging-face/{verb}"),
        Some(form(&[("version", version)])),
        session,
    )
    .await;
    (status, body)
}

async fn page(rig: &Rig, session: &str) -> (StatusCode, String) {
    let (status, headers, body) =
        send_as(&rig.app, "GET", "/admin/hugging-face", None, session).await;
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .map(|v| v.to_str().unwrap()),
        Some("no-store")
    );
    (status, body)
}

/// The audit's records about the account for `action`: first records,
/// `ok` outcomes, `failed` outcomes and refusals.
async fn audited(s: &Store, action: &str) -> (i64, i64, i64, i64) {
    let count = |clause: &'static str| {
        let query = format!(
            "SELECT count(*) FROM audit WHERE target_kind = 'hugging_face' \
             AND target_id IS NULL AND action = $1 AND {clause}"
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

fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// **A connect keeps the token sealed**: the row names the account and the
/// role Hugging Face answered, no column of it holds the token's bytes or
/// their hex, the seal opens under the authority's key to the token, and
/// the connect is a first record and its `ok` outcome.
#[tokio::test]
async fn a_connect_keeps_the_token_sealed() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (status, body) = connect(&rig, &ada, READ, "0").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");

    let row = s.hugging_face_account().await.unwrap().unwrap();
    assert_eq!(
        (row.account.as_str(), row.role.as_str()),
        ("ada-hf", "read")
    );
    let as_text: String =
        sqlx::query_scalar("SELECT row_to_json(h)::text FROM hugging_face_account h")
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert!(
        !as_text.contains(READ),
        "the row holds the token: {as_text}"
    );
    assert!(
        !as_text.contains(&crate::store::identity::hex(READ.as_bytes())),
        "the row holds the token's hex"
    );
    assert!(!holds(&row.sealed.ciphertext, READ.as_bytes()));
    assert_eq!(row.sealed.nonce.len(), 12);
    let key = SealKey::load(&rig.authority_dir()).unwrap().unwrap();
    assert_eq!(key.open(&row.sealed).unwrap(), Token::parse(READ).unwrap());
    assert_eq!(audited(s, "hugging-face connect").await, (1, 1, 0, 0));
    assert_eq!(rig.fake.requests(), 1, "one whoami");
}

/// **A token Hugging Face refuses saves nothing**: 422 with the words to act
/// on, no row, and one refusal record and no first record.
#[tokio::test]
async fn a_token_hugging_face_refuses_saves_nothing_and_is_one_refusal_record() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (status, body) = connect(&rig, &ada, UNKNOWN, "0").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("Hugging Face refused this token"), "{body}");
    assert!(s.hugging_face_account().await.unwrap().is_none());
    assert_eq!(audited(s, "hugging-face connect").await, (0, 0, 0, 1));
}

/// **A person without an admin grant causes no request to Hugging Face**:
/// the page and each write answer 403 at the admin gate, each write with
/// its one refusal record, and the fake counts no request. An admin's
/// connect, by contrast, is counted, so the counter counts.
#[tokio::test]
async fn a_person_without_an_admin_grant_causes_no_request_to_hugging_face() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let (status, body) = connect(&rig, &ada, READ, "0").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let before = rig.fake.requests();
    assert_eq!(before, 1);

    assert_eq!(page(&rig, &bea).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        connect(&rig, &bea, WRITE, "1").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&rig, &bea, "check", "1").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        post(&rig, &bea, "disconnect", "1").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        rig.fake.requests(),
        before,
        "a non-admin reached Hugging Face"
    );
    for action in [
        "hugging-face connect",
        "hugging-face check",
        "hugging-face disconnect",
    ] {
        let (_, _, _, refusals) = audited(s, action).await;
        assert_eq!(refusals, 1, "{action}");
    }
    let row = s.hugging_face_account().await.unwrap().unwrap();
    assert_eq!((row.role.as_str(), row.version), ("read", 1));
}

/// **The token enters no audit row, log line, page or error**: after a
/// connect, a check, a refused connect, a page and the answers of a
/// Hugging Face that fails, none of them carries the token, every log line
/// at every level included.
#[tokio::test]
async fn the_token_enters_no_audit_row_log_line_page_or_error() {
    let captured = Captured::default();
    let _logging = crate::test_log::scoped(
        tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish(),
    );
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let mut bodies = Vec::new();
    let (status, body) = connect(&rig, &ada, READ, "0").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    bodies.push(body);
    let (status, body) = post(&rig, &ada, "check", "1").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    bodies.push(body);
    let (status, body) = connect(&rig, &ada, UNKNOWN, "2").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    bodies.push(body);
    rig.fake.broken.store(true, Ordering::SeqCst);
    let (status, body) = connect(&rig, &ada, WRITE, "2").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert!(body.contains("nothing was saved"), "{body}");
    bodies.push(body);
    let (status, body) = post(&rig, &ada, "check", "2").await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    bodies.push(body);
    let (status, body) = page(&rig, &ada).await;
    assert_eq!(status, StatusCode::OK);
    bodies.push(body);

    let audit = audit_text(s).await;
    let logged = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(!logged.is_empty(), "the capture caught nothing");
    for token in [READ, UNKNOWN, WRITE] {
        assert!(!audit.contains(token), "the audit holds a token");
        assert!(!logged.contains(token), "a log line holds a token");
        for body in &bodies {
            assert!(!body.contains(token), "an answer holds a token: {body}");
        }
    }
}

/// **The role is shown as Hugging Face names it**: read, write, and a
/// fine-grained token as fine-grained. A connect over a row takes the
/// version read, and a stale one is refused.
#[tokio::test]
async fn the_role_is_shown_as_hugging_face_names_it() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    for (token, version, role) in [
        (READ, "0", "read"),
        (WRITE, "1", "write"),
        (FINE, "2", "fine-grained"),
    ] {
        let (status, body) = connect(&rig, &ada, token, version).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
        let (_, body) = page(&rig, &ada).await;
        assert!(
            body.contains(&format!("<tr><th>Token role</th><td>{role}</td></tr>")),
            "{role}: {body}"
        );
        assert!(body.contains("ada-hf"));
    }
    let (status, body) = connect(&rig, &ada, READ, "2").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (status, body) = connect(&rig, &ada, READ, "0").await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a connect over a row names its version: {body}"
    );
    assert_eq!(audited(s, "hugging-face connect").await, (5, 3, 2, 0));
}

/// **A token Hugging Face later refuses is shown refused, and the setting
/// stands until a disconnect**: a check records the refusal, a check it
/// passes clears it, and a disconnect at the version read removes the row.
#[tokio::test]
async fn a_token_later_refused_is_shown_refused_and_stands_until_disconnect() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    assert_eq!(
        connect(&rig, &ada, READ, "0").await.0,
        StatusCode::SEE_OTHER
    );
    rig.fake.refuse(READ);
    let (status, body) = post(&rig, &ada, "check", "1").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let (_, body) = page(&rig, &ada).await;
    assert!(body.contains("Hugging Face refused this token"), "{body}");
    let row = s.hugging_face_account().await.unwrap().unwrap();
    assert!(row.refused_at.is_some());
    assert_eq!(row.version, 2);

    rig.fake.accept(READ);
    assert_eq!(
        post(&rig, &ada, "check", "2").await.0,
        StatusCode::SEE_OTHER
    );
    let (_, body) = page(&rig, &ada).await;
    assert!(!body.contains("Hugging Face refused this token"), "{body}");
    assert_eq!(audited(s, "hugging-face check").await, (2, 2, 0, 0));

    let (status, body) = post(&rig, &ada, "disconnect", "2").await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale disconnect: {body}");
    let (status, body) = post(&rig, &ada, "disconnect", "3").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert!(s.hugging_face_account().await.unwrap().is_none());
    let (status, body) = post(&rig, &ada, "disconnect", "3").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (_, body) = page(&rig, &ada).await;
    assert!(body.contains("No account is connected"), "{body}");
}

/// **A version is never repeated across a disconnect**: a page read before
/// a disconnect and a connect again holds a version the new row never
/// takes, so its connect, check and disconnect each answer stale, and the
/// new account stands as connected.
#[tokio::test]
async fn a_version_read_before_a_disconnect_is_stale_after_a_connect_again() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = admin(&rig.app, s, &mut authenticator(), "bea").await;
    assert_eq!(
        connect(&rig, &ada, READ, "0").await.0,
        StatusCode::SEE_OTHER
    );
    let read = s.hugging_face_account().await.unwrap().unwrap().version;
    let read = read.to_string();
    assert_eq!(
        post(&rig, &bea, "disconnect", &read).await.0,
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        connect(&rig, &bea, FINE, "0").await.0,
        StatusCode::SEE_OTHER
    );
    let standing = s.hugging_face_account().await.unwrap().unwrap();

    let (status, body) = connect(&rig, &ada, WRITE, &read).await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale connect: {body}");
    let (status, body) = post(&rig, &ada, "check", &read).await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale check: {body}");
    let (status, body) = post(&rig, &ada, "disconnect", &read).await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale disconnect: {body}");

    let row = s.hugging_face_account().await.unwrap().unwrap();
    assert_eq!(row.account, standing.account);
    assert_eq!(row.role, standing.role);
    assert_eq!(row.version, standing.version);
    assert_eq!(row.checked_at, standing.checked_at);
}

/// **A lost key means the token is entered again**: under a new key the
/// page says the stored token does not open, a check is refused saying so
/// before any request, and a connect with the token restores the setting.
#[tokio::test]
async fn a_lost_key_says_to_connect_again() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    assert_eq!(
        connect(&rig, &ada, READ, "0").await.0,
        StatusCode::SEE_OTHER
    );
    std::fs::remove_file(rig.authority_dir().join(SEAL_KEY)).unwrap();
    write_beside(&rig.authority_dir()).unwrap();

    let (_, body) = page(&rig, &ada).await;
    assert!(
        body.contains("does not open under the console's seal key"),
        "{body}"
    );
    let before = rig.fake.requests();
    let (status, body) = post(&rig, &ada, "check", "1").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("connect the account again"), "{body}");
    assert_eq!(rig.fake.requests(), before);

    assert_eq!(
        connect(&rig, &ada, READ, "1").await.0,
        StatusCode::SEE_OTHER
    );
    let (_, body) = page(&rig, &ada).await;
    assert!(!body.contains("does not open"), "{body}");
}

/// **No seal key, no token kept**: the page says how a key is written and
/// offers no connect, and a connect is refused before any request.
#[tokio::test]
async fn without_a_seal_key_no_token_is_kept() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    std::fs::remove_file(rig.authority_dir().join(SEAL_KEY)).unwrap();
    let (_, body) = page(&rig, &ada).await;
    assert!(body.contains("authority seal-key"), "{body}");
    assert!(!body.contains("/admin/hugging-face/connect"), "{body}");
    let (status, body) = connect(&rig, &ada, READ, "0").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(rig.fake.requests(), 0);
    assert!(s.hugging_face_account().await.unwrap().is_none());
}

/// **The gated path, and a repository's listing**: the listing answers the
/// commit, the gate and every file with its size, blob id and, for an LFS
/// file, its SHA-256. A gated file the account lacks answers gated with the
/// repository's page to accept on, and once the fake grants it, granted. A
/// repository that does not stand answers missing. A branch or a path
/// out of its shape is refused before any request.
#[tokio::test]
async fn a_gated_repository_answers_gated_until_the_account_is_granted() {
    let (fake, base) = fake_hugging_face().await;
    let client = Client::with_base(&base).unwrap();
    let token = Token::parse(READ).unwrap();
    let gated = RepoId::parse("org/gated").unwrap();

    let ModelAt::Model(model) = client.model_at(Some(&token), &gated, "main").await.unwrap() else {
        panic!("the listing answered no model");
    };
    assert_eq!(model.commit, COMMIT);
    assert_eq!(model.gated, Gated::Manual);
    assert_eq!(model.files.len(), 2);
    assert_eq!(model.files[0].sha256, None);
    assert_eq!(model.files[0].blob_id, CONFIG_BLOB);
    assert_eq!(model.files[1].sha256.as_deref(), Some(MODEL_SHA));
    assert_eq!(model.files[1].size, 5);
    let open = RepoId::parse("org/open").unwrap();
    let ModelAt::Model(model) = client.model_at(None, &open, COMMIT).await.unwrap() else {
        panic!("the listing answered no model");
    };
    assert_eq!(model.gated, Gated::No);
    let none = RepoId::parse("org/none").unwrap();
    assert_eq!(
        client.model_at(Some(&token), &none, "main").await.unwrap(),
        ModelAt::Missing
    );

    let accept = Access::Gated {
        accept: format!("{base}org/gated"),
    };
    assert_eq!(
        client
            .access(Some(&token), &gated, COMMIT, "model.gguf")
            .await
            .unwrap(),
        accept
    );
    assert_eq!(
        client
            .access(None, &gated, COMMIT, "model.gguf")
            .await
            .unwrap(),
        accept
    );
    fake.grant(READ);
    assert_eq!(
        client
            .access(Some(&token), &gated, COMMIT, "model.gguf")
            .await
            .unwrap(),
        Access::Granted
    );
    assert_eq!(
        client
            .access(None, &open, COMMIT, "config.json")
            .await
            .unwrap(),
        Access::Granted
    );
    assert_eq!(
        client
            .access(Some(&token), &none, COMMIT, "x")
            .await
            .unwrap(),
        Access::Missing
    );
    let before = fake.requests();
    assert!(
        client
            .access(Some(&token), &gated, "main", "model.gguf")
            .await
            .is_err()
    );
    assert!(
        client
            .access(Some(&token), &gated, COMMIT, "../x")
            .await
            .is_err()
    );
    assert_eq!(fake.requests(), before, "a refused name reached the fake");
}

/// A config for the host's commands, the store's and the authority's.
fn config(dir: &std::path::Path) -> crate::config::ServerConfig {
    crate::config::ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:0".into(),
        database: String::new(),
        authority_dir: dir.join("authority"),
        silence_bound_secs: 60,
        link_address: None,
        server_name: "weaver-web".into(),
        enrollment_token_hours: 24,
        origin: None,
        tls_certificate: None,
        tls_key: None,
        rp_id: None,
        session_idle_secs: 3600,
        session_absolute_secs: 43200,
        operations_log: None,
        model_cache: None,
        sign_in: vec![crate::config::SignInMethod::Passkey],
    }
}

/// **`authority seal-key`**, audited as the host's: refused where no
/// authority stands, refused where a key stands (the key unchanged), and a
/// key written beside an authority without one. `authority init` writes a
/// key, and `authority rotate` carries it into the new set.
#[tokio::test]
async fn authority_seal_key_refuses_to_overwrite() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let dir = crate::test_dir::tempdir().unwrap();
    let cfg = config(dir.path());
    let key_path = cfg.authority_dir.join(SEAL_KEY);

    let answer = crate::link::verbs::authority_seal_key(s, &cfg, None).await;
    assert!(!answer.ok, "{}", answer.value);
    assert!(
        answer.value["error"]
            .as_str()
            .unwrap()
            .contains("no authority stands")
    );

    Authority::init(&cfg.authority_dir, "weaver-web", &[]).unwrap();
    let minted = std::fs::read_to_string(&key_path).unwrap();
    assert_eq!(minted.len(), 65);
    let answer = crate::link::verbs::authority_seal_key(s, &cfg, None).await;
    assert!(!answer.ok, "{}", answer.value);
    assert!(
        answer.value["error"]
            .as_str()
            .unwrap()
            .contains("a seal key stands")
    );
    assert_eq!(std::fs::read_to_string(&key_path).unwrap(), minted);

    Authority::rotate(&cfg.authority_dir, "weaver-web", &[]).unwrap();
    assert_eq!(
        std::fs::read_to_string(&key_path).unwrap(),
        minted,
        "a rotation carries the key"
    );

    std::fs::remove_file(&key_path).unwrap();
    let answer = crate::link::verbs::authority_seal_key(s, &cfg, None).await;
    assert!(answer.ok, "{}", answer.value);
    let written = std::fs::read_to_string(&key_path).unwrap();
    assert_ne!(written, minted);
    assert!(SealKey::load(&cfg.authority_dir).unwrap().is_some());

    let records: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE answers IS NULL AND refusal IS NULL), \
         count(*) FILTER (WHERE outcome = 'ok'), count(*) FILTER (WHERE outcome = 'failed') \
         FROM audit WHERE principal = 'host' AND target_kind = 'authority' \
         AND action = 'authority seal-key'",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(records, (3, 1, 2));
}

/// **`authority seal-key` writes under the authority lock**, which
/// `authority rotate` holds across its switch, so a rotation never puts
/// live a set it staged without the key this command writes: with the lock
/// held, the command waits and writes nothing, and once the lock is
/// released the key stands.
#[tokio::test]
async fn authority_seal_key_waits_for_the_authority_lock() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = fresh.store.clone();
    let dir = crate::test_dir::tempdir().unwrap();
    let cfg = config(dir.path());
    let key_path = cfg.authority_dir.join(SEAL_KEY);
    Authority::init(&cfg.authority_dir, "weaver-web", &[]).unwrap();
    std::fs::remove_file(&key_path).unwrap();

    // Held by this test, as a concurrent rotation would hold it.
    let held = s.authority_lock().await.unwrap();
    let writing = tokio::spawn({
        let (s, cfg) = (s.clone(), cfg.clone());
        async move { crate::link::verbs::authority_seal_key(&s, &cfg, None).await }
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(!key_path.exists(), "a key was written under a held lock");
    assert!(!writing.is_finished(), "seal-key ran under a held lock");
    drop(held);
    let answer = writing.await.unwrap();
    assert!(answer.ok, "{}", answer.value);
    assert!(SealKey::load(&cfg.authority_dir).unwrap().is_some());
}

/// **Each write to the account runs from its first record to its outcome
/// in a task the request cannot cancel** (Spec 2.13, #150): a connect, a
/// check and a disconnect, each request dropped while its first record is
/// held, still land, each first record answered by its `ok` outcome, and
/// the page then reads the account as it stands. A cancel's own test is in
/// `model_pull_tests`.
///
/// Perturbation: run step four in the request's own future
/// (`admin::step_four` awaiting it in place), and the first write's
/// outcome never comes.
#[tokio::test]
async fn each_write_to_the_account_dropped_at_its_first_record_still_lands() {
    use crate::admin_tests::{dropped_at_the_hold, outcome_of};
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (person, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;

    dropped_at_the_hold(
        &rig.app,
        &person,
        &ada,
        "/admin/hugging-face/connect",
        form(&[("token", READ), ("version", "0")]),
    )
    .await;
    assert_eq!(outcome_of(s, &person, "hugging-face connect").await, "ok");
    let row = s
        .hugging_face_account()
        .await
        .unwrap()
        .expect("the connect landed");
    assert_eq!(row.version, 1);
    let (status, body) = page(&rig, &ada).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains(r#"<tr><th>Account</th><td class="mono">ada-hf</td></tr>"#),
        "the page shows the connected account: {body}"
    );
    assert!(!body.contains("No account is connected"), "{body}");

    rig.fake.refuse(READ);
    dropped_at_the_hold(
        &rig.app,
        &person,
        &ada,
        "/admin/hugging-face/check",
        form(&[("version", "1")]),
    )
    .await;
    assert_eq!(outcome_of(s, &person, "hugging-face check").await, "ok");
    assert!(
        s.hugging_face_account()
            .await
            .unwrap()
            .unwrap()
            .refused_at
            .is_some()
    );
    let (status, body) = page(&rig, &ada).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Hugging Face refused this token"), "{body}");

    dropped_at_the_hold(
        &rig.app,
        &person,
        &ada,
        "/admin/hugging-face/disconnect",
        form(&[("version", "2")]),
    )
    .await;
    assert_eq!(
        outcome_of(s, &person, "hugging-face disconnect").await,
        "ok"
    );
    assert!(s.hugging_face_account().await.unwrap().is_none());
    let (status, body) = page(&rig, &ada).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("No account is connected"), "{body}");
}
