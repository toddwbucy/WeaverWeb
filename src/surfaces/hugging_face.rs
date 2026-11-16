//! conforms: web-a-person-without-an-admin-grant-causes-no-request-to-hugging-face
//! conforms: web-the-hugging-face-token-is-held-sealed
//!
//! **The Hugging Face account** (Spec 2.15): a console setting, for a
//! session whose person holds a live admin grant. `GET /admin/hugging-face`
//! shows the account, its token's role, who set it and when, when it was
//! last checked, whether Hugging Face last refused it and whether its seal
//! opens. Three writes, connect, check and disconnect.
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
use axum::extract::{Form, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::hugging_face::seal::SealKey;
use crate::hugging_face::{Client, Token, Whoami};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::hugging_face::Checked;
use crate::surfaces::admin::{
    NOT_AN_ADMIN, admin_session, fault, first_of, landed, refused, settled, writer,
};
use crate::surfaces::gate::{Policy, Session};

const PAGE: &str = "/admin/hugging-face";

/// What the surface takes beside the store: the session policy, the client
/// (whose base is the test seam), and the directory the seal key stands in.
#[derive(Clone)]
pub struct Seams {
    pub policy: Policy,
    pub client: Client,
    pub authority_dir: PathBuf,
}

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
            "/admin/hugging-face/connect",
            post(write!(connect, ConnectAsk)),
        )
        .route("/admin/hugging-face/check", post(write!(check, VersionAsk)))
        .route(
            "/admin/hugging-face/disconnect",
            post(write!(disconnect, VersionAsk)),
        )
}

fn when(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
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
    let first = match first_of(&store, &session, Target::HuggingFace, ACTION).await {
        Ok(first) => first,
        Err(answer) => return answer,
    };
    let written = store
        .hugging_face_disconnect(&session.person_id, session.session_id, &first, ask.version)
        .await;
    settled(&store, &first, &written).await;
    landed(written, PAGE)
}
