//! conforms: web-a-person-without-an-admin-grant-causes-no-request-to-hugging-face
//! conforms: web-the-hugging-face-token-is-held-sealed
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//!
//! **The Hugging Face account** (Spec 2.15): a console setting, for a
//! session whose person holds a live admin grant. `GET /admin/hugging-face`
//! shows the account, its token's role, who set it and when, when it was
//! last checked, whether Hugging Face last refused it and whether its seal
//! opens. Three writes, connect, check and disconnect. **The cache's
//! section** shows whether `model_cache` is set, its free space, and the
//! latest pulls with their record and progress, re-fetched every 15 s by
//! the page itself (`GET /admin/hugging-face/pulls`) through the session's
//! background read and the admin check, a running pull with its cancel
//! (`POST /admin/hugging-face/pulls/{mp}/cancel`), an admin's write that
//! keeps the pull's partials for a later pull to resume. The page that asks
//! a pull is the models page's, a later act.
//!
//! **Each write takes the admin writes' order** (Spec 2.13): the form parsed
//! to its shape, the admin gate with its one refusal record before anything
//! is looked up, what the write needs resolved (the seal key, the row, and
//! Hugging Face's answer for the token), then the audited write with the
//! admin as principal by `session`. **Hugging Face is asked after the admin
//! gate**, so a person without an admin grant causes no request to it, and
//! **before the audited write**, so a token it refuses saves nothing. That
//! refusal is one record, as the admin gate's is.
//!
//! **The token enters no page, log line, audit record or error**: the form
//! that carries it is the only place it travels in clear, sealed before the
//! store sees it, and every answer of this surface is `no-store`.

use std::path::PathBuf;

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::hugging_face::cache::Cache;
use crate::hugging_face::pull::Puller;
use crate::hugging_face::seal::SealKey;
use crate::hugging_face::{Client, Token, Whoami};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::hugging_face::Checked;
use crate::store::model_pull::Pull;
use crate::surfaces::admin::{
    NOT_AN_ADMIN, admin_session, fault, first_of, landed, refused, settled, step_four, writer,
};
use crate::surfaces::gate::{self, Policy, Session};

const PAGE: &str = "/admin/hugging-face";

/// What the surface takes beside the store: the session policy, the client
/// (whose base is the test seam), the directory the seal key stands in, and
/// the puller, whose cache the section reads.
#[derive(Clone)]
pub struct Seams {
    pub policy: Policy,
    pub client: Client,
    pub authority_dir: PathBuf,
    pub puller: Puller,
}

/// How many pulls the cache's section shows, the newest first.
const PULLS_SHOWN: i64 = 20;

/// Every answer of this surface is kept by no cache.
fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub fn routes(seams: Seams) -> Router<Store> {
    macro_rules! write {
        ($handler:ident, $ask:ty) => {{
            let seams = seams.clone();
            move |State(store): State<Store>, headers: HeaderMap, Form(ask): Form<$ask>| {
                let seams = seams.clone();
                async move { no_store($handler(store, &seams, headers, ask).await) }
            }
        }};
    }
    let page_seams = seams.clone();
    Router::new()
        .route(
            PAGE,
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let seams = page_seams.clone();
                async move { no_store(page(store, &seams, headers).await) }
            }),
        )
        .route(
            "/admin/hugging-face/pulls",
            get({
                let seams = seams.clone();
                move |State(store): State<Store>, headers: HeaderMap| {
                    let seams = seams.clone();
                    async move { no_store(pulls(store, &seams, headers).await) }
                }
            }),
        )
        .route(
            "/admin/hugging-face/connect",
            post(write!(connect, ConnectAsk)),
        )
        .route("/admin/hugging-face/check", post(write!(check, VersionAsk)))
        .route(
            "/admin/hugging-face/disconnect",
            post(write!(disconnect, VersionAsk)),
        )
        .route(
            "/admin/hugging-face/pulls/{mp}/cancel",
            post(
                move |State(store): State<Store>, headers: HeaderMap, Path(mp): Path<String>| {
                    let seams = seams.clone();
                    async move { no_store(cancel(store, &seams, headers, &mp).await) }
                },
            ),
        )
}

fn when(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// A count of bytes as a person reads it, in powers of 1024.
pub(crate) fn bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if n < 1024 {
        return format!("{n} B");
    }
    let mut value = n as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.2} {}", UNITS[unit])
}

struct FileView {
    path: String,
    size: String,
    done: String,
    sha256: String,
    /// `downloaded`, `found`, or empty while the file is not verified.
    source: String,
}

struct PullView {
    pull_id: String,
    repository: String,
    commit: String,
    state: String,
    /// The state's status hue: `ok`, `warn` while it runs or once it is
    /// cancelled, `fail`.
    hue: &'static str,
    reason: Option<String>,
    /// Whether it runs, so the section offers its cancel.
    running: bool,
    /// The admin whose cancel ended it.
    cancelled_by: Option<String>,
    account: String,
    asked_by: String,
    asked_at: String,
    finished_at: String,
    done: String,
    size: String,
    files: Vec<FileView>,
}

/// The cache's section: whether `model_cache` is set, its free space, and
/// the latest pulls with their files.
struct CacheView {
    set: bool,
    free: String,
    pulls: Vec<PullView>,
}

async fn cache_view(store: &Store, cache: Option<&Cache>) -> anyhow::Result<CacheView> {
    let free = match cache {
        Some(cache) => match Cache::free_space(cache.root()) {
            Ok(free) => bytes(free),
            Err(e) => {
                tracing::warn!("the model cache's free space could not be read: {e}");
                "unknown".to_owned()
            }
        },
        None => String::new(),
    };
    let mut pulls = Vec::new();
    for pull in store.pulls(PULLS_SHOWN).await? {
        pulls.push(pull_view(store, pull).await?);
    }
    Ok(CacheView {
        set: cache.is_some(),
        free,
        pulls,
    })
}

async fn pull_view(store: &Store, pull: Pull) -> anyhow::Result<PullView> {
    let files = store.pull_files(&pull.pull_id).await?;
    let size: u64 = files.iter().map(|f| f.file.size).sum();
    let done: u64 = files
        .iter()
        .map(|f| u64::try_from(f.bytes_done).unwrap_or(0))
        .sum();
    Ok(PullView {
        hue: match pull.state.as_str() {
            "verified" => "ok",
            "running" | "cancelled" => "warn",
            _ => "fail",
        },
        running: pull.state == "running",
        cancelled_by: pull.cancelled_by,
        pull_id: pull.pull_id,
        repository: pull.repository,
        commit: pull.commit,
        state: pull.state,
        reason: pull.reason,
        account: pull.account.unwrap_or_else(|| "none".to_owned()),
        asked_by: pull.asked_by,
        asked_at: when(pull.asked_at),
        finished_at: pull.finished_at.map(when).unwrap_or_default(),
        done: bytes(done),
        size: bytes(size),
        files: files
            .into_iter()
            .map(|f| FileView {
                done: bytes(u64::try_from(f.bytes_done).unwrap_or(0)),
                size: bytes(f.file.size),
                sha256: f.file.sha256.unwrap_or_default(),
                source: f.source.unwrap_or_default(),
                path: f.file.path,
            })
            .collect(),
    })
}

#[derive(Template)]
#[template(path = "hugging_face_pulls.html")]
struct PullsFragment {
    cache: CacheView,
}

/// **The cache's section, re-fetched by the page itself** every 15 s: the
/// session read in the background, so a poll never holds an idle session
/// open, and the admin check as at the page's load.
async fn pulls(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match gate::session_in_background(&store, &seams.policy, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return crate::surfaces::record::NoSession.into_response(),
        Err(e) => return fault(e),
    };
    match store.is_admin(&session.person_id).await {
        Ok(true) => {}
        Ok(false) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(e) => return fault(e),
    }
    let cache = match cache_view(&store, seams.puller.cache()).await {
        Ok(cache) => cache,
        Err(e) => return fault(e),
    };
    match (PullsFragment { cache }).render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

struct AccountView {
    account: String,
    role: String,
    set_by: String,
    set_at: String,
    checked_at: String,
    refused_at: Option<String>,
    opens: bool,
}

#[derive(Template)]
#[template(path = "admin_hugging_face.html")]
struct HuggingFacePage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    account: Option<AccountView>,
    version: i64,
    /// Why no token can be kept, where none can.
    no_key: Option<&'static str>,
    /// Hugging Face's page where an account makes a token.
    tokens: String,
    cache: CacheView,
}

async fn page(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, &seams.policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let row = match store.hugging_face_account().await {
        Ok(row) => row,
        Err(e) => return fault(e),
    };
    let (key, no_key) = match SealKey::load(&seams.authority_dir) {
        Ok(Some(key)) => (Some(key), None),
        Ok(None) => (
            None,
            Some(
                "The console holds no seal key, so it can keep no token. The host's `authority seal-key` writes one beside the authority, and the next connect uses it.",
            ),
        ),
        Err(e) => {
            tracing::error!("the seal key could not be read: {e:#}");
            (
                None,
                Some(
                    "The console's seal key could not be read, so no token can be kept or opened. The operations log says why.",
                ),
            )
        }
    };
    let chrome = match super::Chrome::of(&store, &session.person_id).await {
        Ok(chrome) => chrome,
        Err(e) => return fault(e),
    };
    let cache = match cache_view(&store, seams.puller.cache()).await {
        Ok(cache) => cache,
        Err(e) => return fault(e),
    };
    let version = row.as_ref().map_or(0, |row| row.version);
    let account = row.map(|row| AccountView {
        opens: key
            .as_ref()
            .is_some_and(|key| key.open(&row.sealed).is_some()),
        account: row.account,
        role: row.role,
        set_by: row.set_by,
        set_at: when(row.set_at),
        checked_at: when(row.checked_at),
        refused_at: row.refused_at.map(when),
    });
    let page = HuggingFacePage {
        here: "hugging-face",
        who: session.name.clone(),
        admin: true,
        chrome,
        tokens: format!("{}settings/tokens", seams.client.base()),
        account,
        version,
        no_key,
        cache,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// Hugging Face's answer for a token, or the write answered where it could
/// not be had. Its error names no token (`hugging_face::scrubbed`).
async fn whoami(seams: &Seams, token: &Token) -> Result<Whoami, Response> {
    seams.client.whoami(token).await.map_err(|e| {
        tracing::warn!("a Hugging Face check could not be made: {e:#}");
        (
            StatusCode::BAD_GATEWAY,
            format!("{e:#}; nothing was saved, and the page may be posted again"),
        )
            .into_response()
    })
}

#[derive(Deserialize)]
pub struct ConnectAsk {
    token: String,
    version: i64,
}

async fn connect(store: Store, seams: &Seams, headers: HeaderMap, ask: ConnectAsk) -> Response {
    const ACTION: &str = "hugging-face connect";
    let Some(token) = Token::parse(&ask.token) else {
        return (
            StatusCode::BAD_REQUEST,
            "a token is 1 to 256 visible characters, with no space",
        )
            .into_response();
    };
    if ask.version < 0 {
        return (StatusCode::BAD_REQUEST, "a version is never negative").into_response();
    }
    let session = match writer(&store, &seams.policy, &headers, Target::HuggingFace, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let key = match SealKey::load(&seams.authority_dir) {
        Ok(Some(key)) => key,
        Ok(None) => return refused(&Refusal::NoSealKey),
        Err(e) => return fault(e),
    };
    let account = match whoami(seams, &token).await {
        Ok(Whoami::Account(account)) => account,
        Ok(Whoami::Refused) => return refusal(&store, &session, ACTION).await,
        Err(answer) => return answer,
    };
    step_four(async move {
        let first = match first_of(&store, &session, Target::HuggingFace, ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let sealed = key.seal(&token);
        let written = store
            .hugging_face_connect(
                &session.person_id,
                session.session_id,
                &first,
                &account,
                &sealed,
                ask.version,
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, PAGE)
    })
    .await
}

/// **Hugging Face's refusal of a token, as one record**, the admin gate's
/// shape: the console asked and was refused, and nothing was written.
async fn refusal(store: &Store, session: &Session, action: &str) -> Response {
    let principal = Principal::Person {
        person_id: &session.person_id,
        method: PersonMethod::Session,
    };
    match store
        .audit_refusal(
            principal,
            Target::HuggingFace,
            action,
            "Hugging Face refused the token",
        )
        .await
    {
        Ok(_) => refused(&Refusal::TokenRefused),
        Err(e) => {
            tracing::error!("a refusal of {action} was not recorded: {e:#}");
            fault(e)
        }
    }
}

#[derive(Deserialize)]
pub struct VersionAsk {
    version: i64,
}

async fn check(store: Store, seams: &Seams, headers: HeaderMap, ask: VersionAsk) -> Response {
    const ACTION: &str = "hugging-face check";
    let session = match writer(&store, &seams.policy, &headers, Target::HuggingFace, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let row = match store.hugging_face_account().await {
        Ok(Some(row)) => row,
        Ok(None) => return refused(&Refusal::NotConnected),
        Err(e) => return fault(e),
    };
    let key = match SealKey::load(&seams.authority_dir) {
        Ok(Some(key)) => key,
        Ok(None) => return refused(&Refusal::NoSealKey),
        Err(e) => return fault(e),
    };
    let Some(token) = key.open(&row.sealed) else {
        return refused(&Refusal::SealDoesNotOpen);
    };
    let checked = match whoami(seams, &token).await {
        Ok(Whoami::Account(account)) => Checked::Accepted(account),
        Ok(Whoami::Refused) => Checked::Refused,
        Err(answer) => return answer,
    };
    drop(token);
    step_four(async move {
        let first = match first_of(&store, &session, Target::HuggingFace, ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .hugging_face_checked(
                &session.person_id,
                session.session_id,
                &first,
                &checked,
                ask.version,
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, PAGE)
    })
    .await
}

/// **An admin's cancel of a running pull**, in the admin writes' order: the
/// identity parsed to its `mp-` shape, the admin gate, the pull resolved
/// (unknown: not found, ended: the ask's fault, each recorded nowhere), then
/// the audited write, and the puller told once it has landed. The pull's
/// partials stay on disk for a later pull to resume.
async fn cancel(store: Store, seams: &Seams, headers: HeaderMap, pull_id: &str) -> Response {
    const ACTION: &str = "model pull cancel";
    if !crate::store::identity::is_pull_id(pull_id) {
        return (
            StatusCode::BAD_REQUEST,
            "a pull's identity is mp- and sixteen lowercase hex",
        )
            .into_response();
    }
    let target = Target::ModelPull(pull_id);
    let session = match writer(&store, &seams.policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.pull(pull_id).await {
        Ok(Some(pull)) if pull.state == "running" => {}
        Ok(Some(_)) => return refused(&Refusal::PullNotRunning(pull_id.to_owned())),
        Ok(None) => return (StatusCode::NOT_FOUND, "no such pull").into_response(),
        Err(e) => return fault(e),
    }
    let (pull_id, puller) = (pull_id.to_owned(), seams.puller.clone());
    step_four(async move {
        let first = match first_of(&store, &session, Target::ModelPull(&pull_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .cancel_pull(&session.person_id, session.session_id, &first, &pull_id)
            .await;
        settled(&store, &first, &written).await;
        // Only a definite refusal goes unsignalled: a cancel whose answer was
        // lost may have landed, and a signal for a pull the store still runs
        // makes the puller take it again from its partial.
        if !matches!(written, Ok(Err(_))) {
            puller.cancel(&pull_id);
        }
        landed(written, PAGE)
    })
    .await
}

async fn disconnect(store: Store, seams: &Seams, headers: HeaderMap, ask: VersionAsk) -> Response {
    const ACTION: &str = "hugging-face disconnect";
    let session = match writer(&store, &seams.policy, &headers, Target::HuggingFace, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.hugging_face_account().await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(&Refusal::NotConnected),
        Err(e) => return fault(e),
    }
    step_four(async move {
        let first = match first_of(&store, &session, Target::HuggingFace, ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .hugging_face_disconnect(&session.person_id, session.session_id, &first, ask.version)
            .await;
        settled(&store, &first, &written).await;
        landed(written, PAGE)
    })
    .await
}
