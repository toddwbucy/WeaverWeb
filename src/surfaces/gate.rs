//! conforms: web-session-carries-a-person
//!
//! **The session**, per `weaver-web-Spec` section 2.8 and
//! `docs/project/design-2026-10-07-iam.md` section 6: the person a
//! request's cookie names, checked at every use, its last use refreshed at
//! most once a minute, closed at sign-out; and the `Origin` check every
//! request that changes state passes before its handler.
//!
//! **A surface reads the store and nothing else, and a surface that holds a
//! seam takes it as its own argument** (`surfaces/mod.rs`): the session's
//! [`Policy`], its two limits and the configured origin, is that argument.
//!
//! **Nothing but the surface writes a session**: the use that finds a
//! session ended closes it, a use refreshes its last use, and sign-out
//! closes it. Disabling a person, removing a passkey and the host reset
//! change the person or the passkey, and the session sees that here, at
//! its next use, so no end needs a write to every session.

use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::config::ServerConfig;
use crate::store::Store;

/// **The session's cookie** (design section 6). The `__Host-` prefix makes
/// a browser refuse it unless it is `Secure`, has `Path=/` and has no
/// `Domain`, so no other host can set or read it. Measured on 2026-10-08,
/// Chromium 153 and Firefox 156 keep it on `http://localhost`, which they
/// treat as a secure context, and drop it on a plain origin elsewhere, so
/// this one name serves every origin the server may run on.
pub const COOKIE: &str = "__Host-weaver_session";

/// The attributes the cookie is always set with: `Secure`, `HttpOnly` (no
/// script reads it), `SameSite=Strict` (no other site's request carries it)
/// and `Path=/` with no `Domain`, as the prefix requires.
const ATTRIBUTES: &str = "Secure; HttpOnly; SameSite=Strict; Path=/";

/// **What a session is held to**: how long it may sit unused and how long
/// it may stand at all, and the origin a request that changes state must
/// come from. The surfaces take it as their own argument.
#[derive(Debug, Clone)]
pub struct Policy {
    /// The configured origin; with none, nothing that changes state is
    /// served, since nothing can sign in without one.
    pub origin: Option<String>,
    pub idle: Duration,
    pub absolute: Duration,
}

impl Policy {
    pub fn from_config(cfg: &ServerConfig) -> Self {
        Self {
            origin: cfg.origin.clone(),
            idle: Duration::from_secs(cfg.session_idle_secs),
            absolute: Duration::from_secs(cfg.session_absolute_secs),
        }
    }
}

/// **A live session**: the person it is and the passkey it was opened with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: i64,
    pub person_id: String,
    /// The person's current name, for a page to show.
    pub name: String,
    pub credential_id: String,
}

impl Session {
    /// **The author member a row written in this session takes** (Spec
    /// 3.2): the person's identity and never their name, since a person can
    /// be renamed and an author must not move with them.
    pub fn author(&self) -> &str {
        &self.person_id
    }
}

/// **Why a session no longer serves**, in the order it is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Signed out, or ended at an earlier use.
    Closed,
    /// Its person is disabled.
    PersonDisabled,
    /// The passkey it was opened with is gone.
    PasskeyRemoved,
    /// Open past the absolute limit.
    Expired,
    /// Unused past the idle limit.
    Idle,
}

/// The bearer a browser sent under the session's cookie, if it sent one.
/// **Only that name is read**: a cookie under the claimed-name session's
/// old `weaver_session` names nothing.
fn bearer(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .find_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name.trim() == COOKIE).then(|| value.trim().to_string())
        })
}

/// **The bearer is hashed before it is looked up**, per section 2.8, so a
/// read of the session table is not a set of live bearers and a leaked
/// dump is not a set of usable cookies.
pub fn digest(bearer: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bearer.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// The `Set-Cookie` value that hands a browser its bearer, for the sign-in
/// that opens a session.
pub fn set_cookie(bearer: &str) -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}={bearer}; {ATTRIBUTES}"))
        .expect("a bearer is a header-safe token")
}

/// The `Set-Cookie` value that clears it, with the same attributes, since a
/// browser matches a cookie to clear by its name, path and prefix.
pub fn cleared_cookie() -> HeaderValue {
    HeaderValue::from_str(&format!("{COOKIE}=; {ATTRIBUTES}; Max-Age=0"))
        .expect("a constant header")
}

/// **The session a request's cookie names, at this use**, or `None` where
/// it names none that serves. **An absent session is absent and never a
/// default**: a request with no live session is one this crate cannot name.
pub async fn session(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
) -> anyhow::Result<Option<Session>> {
    let Some(bearer) = bearer(headers) else {
        return Ok(None);
    };
    Ok(at_use(store, policy, &bearer).await?.ok())
}

/// **A session checked at this use** (design section 6): the session where
/// it serves, why it ended where it has, or `Err(None)` where the bearer
/// names no session. **Each end is checked at every use**: closed, its
/// person disabled, its passkey removed, open past the absolute limit,
/// unused past the idle limit, in one read on the database's clock. A use
/// that finds it serving refreshes its last use; one that finds it ended
/// closes it, **the close restating its reason** so it is atomic with what
/// it decides on. Where that close moves no row, another request refreshed
/// or closed the session since this read, so it is read once more and
/// served or refused on that read.
pub(crate) async fn at_use(
    store: &Store,
    policy: &Policy,
    bearer: &str,
) -> anyhow::Result<Result<Session, Option<Ended>>> {
    let digest = digest(bearer);
    for again in [false, true] {
        let Some((session, ended)) = read(store, policy, &digest).await? else {
            return Ok(Err(None));
        };
        let Some(ended) = ended else {
            refresh(store, session.session_id).await?;
            return Ok(Ok(session));
        };
        if ended == Ended::Closed {
            return Ok(Err(Some(Ended::Closed)));
        }
        #[cfg(test)]
        hold_before_close(&digest).await;
        if close(store, policy, session.session_id, ended).await? || again {
            return Ok(Err(Some(ended)));
        }
    }
    unreachable!("the second read returns")
}

/// One read of a session by its bearer's digest: the session, and the first
/// end it has met, in the order [`Ended`] lists them.
async fn read(
    store: &Store,
    policy: &Policy,
    digest: &str,
) -> anyhow::Result<Option<(Session, Option<Ended>)>> {
    let Some(row) = sqlx::query(
        "SELECT s.session_id, s.person_id, p.name, s.credential_id, \
         s.closed_at IS NOT NULL AS closed, \
         NOT p.enabled AS disabled, \
         NOT EXISTS (SELECT 1 FROM passkey k \
           WHERE k.credential_id = s.credential_id AND k.person_id = s.person_id) AS removed, \
         s.opened_at <= now() - make_interval(secs => $2) AS expired, \
         s.last_used_at <= now() - make_interval(secs => $3) AS idle \
         FROM session s JOIN person p ON p.person_id = s.person_id \
         WHERE s.bearer_digest = $1",
    )
    .bind(digest)
    .bind(policy.absolute.as_secs_f64())
    .bind(policy.idle.as_secs_f64())
    .fetch_optional(&store.pool)
    .await?
    else {
        return Ok(None);
    };
    let ended = [
        ("closed", Ended::Closed),
        ("disabled", Ended::PersonDisabled),
        ("removed", Ended::PasskeyRemoved),
        ("expired", Ended::Expired),
        ("idle", Ended::Idle),
    ]
    .into_iter()
    .find_map(|(column, ended)| match row.try_get::<bool, _>(column) {
        Ok(true) => Some(Ok(ended)),
        Ok(false) => None,
        Err(e) => Some(Err(e)),
    })
    .transpose()?;
    let session = Session {
        session_id: row.try_get("session_id")?,
        person_id: row.try_get("person_id")?,
        name: row.try_get("name")?,
        credential_id: row.try_get("credential_id")?,
    };
    Ok(Some((session, ended)))
}

/// **A session closed for the end a read found, where that end still holds**:
/// the reason is restated in the update's own condition, beside the session
/// being open, so a close decided on a read that another request has since
/// overtaken (a refresh above all) moves no row. Whether it closed the row.
async fn close(
    store: &Store,
    policy: &Policy,
    session_id: i64,
    ended: Ended,
) -> anyhow::Result<bool> {
    // Each reason as its own condition, with the one limit it compares, so
    // no statement binds a parameter it does not use.
    let (reason, limit) = match ended {
        Ended::Closed => return Ok(false),
        Ended::PersonDisabled => (
            "NOT (SELECT p.enabled FROM person p WHERE p.person_id = session.person_id)",
            None,
        ),
        Ended::PasskeyRemoved => (
            "NOT EXISTS (SELECT 1 FROM passkey k \
             WHERE k.credential_id = session.credential_id AND k.person_id = session.person_id)",
            None,
        ),
        Ended::Expired => (
            "opened_at <= now() - make_interval(secs => $2)",
            Some(policy.absolute),
        ),
        Ended::Idle => (
            "last_used_at <= now() - make_interval(secs => $2)",
            Some(policy.idle),
        ),
    };
    let statement = format!(
        "UPDATE session SET closed_at = now() \
         WHERE session_id = $1 AND closed_at IS NULL AND {reason}"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(statement)).bind(session_id);
    if let Some(limit) = limit {
        query = query.bind(limit.as_secs_f64());
    }
    let closed = query.execute(&store.pool).await?;
    Ok(closed.rows_affected() == 1)
}

/// A hold between a use's read and its close, keyed by the bearer's digest
/// so no other test's use takes it: the use signals `read` and waits on
/// `release`, which lets a test act between the two.
#[cfg(test)]
pub(crate) type CloseHold = (
    String,
    std::sync::Arc<tokio::sync::Notify>,
    std::sync::Arc<tokio::sync::Notify>,
);

#[cfg(test)]
pub(crate) static CLOSE_HOLD: std::sync::Mutex<Option<CloseHold>> = std::sync::Mutex::new(None);

#[cfg(test)]
async fn hold_before_close(digest: &str) {
    let hold = {
        let mut slot = CLOSE_HOLD.lock().unwrap();
        match slot.as_ref() {
            Some((key, ..)) if key == digest => slot.take(),
            _ => None,
        }
    };
    if let Some((_, read, release)) = hold {
        read.notify_one();
        release.notified().await;
    }
}

/// **The last-used refresh** (design section 6): one conditional update on
/// the database's clock, written where the stored time is a minute old or
/// more, so concurrent uses refresh it once, the writes stay bounded
/// whatever a page polls, and it never moves backwards.
async fn refresh(store: &Store, session_id: i64) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE session SET last_used_at = now() \
         WHERE session_id = $1 AND closed_at IS NULL \
         AND last_used_at <= now() - interval '1 minute'",
    )
    .bind(session_id)
    .execute(&store.pool)
    .await?;
    Ok(())
}

/// **Sign-out**: the session's row closed and the cookie cleared. A request
/// naming no session is answered the same, since there is nothing to end.
pub async fn sign_out(State(store): State<Store>, headers: HeaderMap) -> Response {
    if let Some(bearer) = bearer(&headers)
        && let Err(e) = sqlx::query(
            "UPDATE session SET closed_at = now() WHERE bearer_digest = $1 AND closed_at IS NULL",
        )
        .bind(digest(&bearer))
        .execute(&store.pool)
        .await
    {
        return crate::fault::Fault::from(anyhow::Error::from(e)).into_response();
    }
    (
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, cleared_cookie())],
    )
        .into_response()
}

/// **The `Origin` check** (design section 6), before any handler: every
/// request but `GET` and `HEAD` carries exactly one `Origin` header equal to
/// the configured origin, or it is refused. `SameSite=Strict` keeps the
/// cookie off another site's request; this keeps another site's request
/// from changing anything at all. **With no origin configured, nothing that
/// changes state is served**, since nothing can sign in without one.
pub async fn origin_check(State(policy): State<Policy>, request: Request, next: Next) -> Response {
    if matches!(*request.method(), Method::GET | Method::HEAD) {
        return next.run(request).await;
    }
    let Some(origin) = &policy.origin else {
        return (
            StatusCode::FORBIDDEN,
            "no origin is configured, so this server serves no request that changes state",
        )
            .into_response();
    };
    let mut sent = request.headers().get_all(header::ORIGIN).iter();
    match (sent.next(), sent.next()) {
        (Some(only), None) if only.as_bytes() == origin.as_bytes() => next.run(request).await,
        _ => (
            StatusCode::FORBIDDEN,
            "a request that changes state is served only from this server's own origin",
        )
            .into_response(),
    }
}

/// The `Origin` check laid over a whole router, the server's and a test's
/// alike, so no route that changes state stands outside it.
pub fn guard(app: axum::Router, policy: Policy) -> axum::Router {
    app.layer(axum::middleware::from_fn_with_state(policy, origin_check))
}
