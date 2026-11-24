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
//! its next use, so no end needs a write to every session. **A session is
//! opened by a passkey or, where the config admits it, by a sign-in token**
//! (Spec 2.8, the operator's ruling of 2026-11-10); one a token opened names
//! no passkey and has none to lose.

use std::time::Duration;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::config::ServerConfig;
use crate::store::Store;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity;

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
/// it may stand at all, the origin a request that changes state must come
/// from, and the standing person where sign-in is off. The surfaces take it
/// as their own argument.
#[derive(Debug, Clone)]
pub struct Policy {
    /// The configured origin; with none, nothing that changes state is
    /// served, since nothing can sign in without one.
    pub origin: Option<String>,
    pub idle: Duration,
    pub absolute: Duration,
    /// **An admin write's bound** from its place until its COMMIT is sent
    /// (Spec 2.13): the silence bound.
    pub write_bound: Duration,
    /// **The standing person, where sign-in is off** (the config's
    /// `no_sign_in_as`): every use is served as them and no cookie is read.
    /// `None`, the default, and a session is a signed-in person's as before.
    /// Carried here rather than in a process-wide cell, so a surface's test
    /// drives the path with no cookie in process and no other test is
    /// served under it.
    pub standing: Option<std::sync::Arc<Standing>>,
}

impl Policy {
    pub fn from_config(cfg: &ServerConfig) -> Self {
        Self {
            origin: cfg.origin.clone(),
            idle: Duration::from_secs(cfg.session_idle_secs),
            absolute: Duration::from_secs(cfg.session_absolute_secs),
            write_bound: Duration::from_secs(cfg.silence_bound_secs),
            standing: Standing::from_config(cfg).map(std::sync::Arc::new),
        }
    }
}

/// **A live session**: the person it is and, where a passkey opened it,
/// that passkey.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: i64,
    pub person_id: String,
    /// The person's current name, for a page to show.
    pub name: String,
    /// The identity of the passkey it was opened with, never reused, so a
    /// credential re-enrolled after a reset is not this passkey; `None` for
    /// a session a sign-in token opened (Spec 2.8).
    pub passkey_id: Option<String>,
}

impl Session {
    /// **The author member a row written in this session takes** (Spec
    /// 3.2): the person's identity and never their name, since a person can
    /// be renamed and an author must not move with them.
    pub fn author(&self) -> &str {
        &self.person_id
    }
}

/// **No sign-in, for development** (the operator's ruling of 2026-11-21, the
/// config's `no_sign_in_as`): every browser request is served as one named
/// person, through a standing session this server opens for them by the
/// sign-in token's own path, the host's issue and then its redemption, so
/// each is audited as it always is and everything past the gate (the grant
/// checks, the authors, the session a write is held to) is unchanged. No
/// cookie is read and none is set. **Anyone who reaches the listener is that
/// person**, so the listener's exposure is the box's to hold, and release
/// removes this.
///
/// **An opening ends the person's other tokens**, by the sign-in token's
/// own rules (Spec 2.13): the host's issue ends the person's live
/// enrollment token `revoked` and any sign-in token the host printed for
/// them `superseded`, and from the first opening on an admin's enrollment
/// token is refused for them. A passkey for the standing person therefore
/// takes the host's `person token`, redeemed before the next opening, which
/// comes when the held session ends: at the idle or absolute limit, a
/// disable, or a restart.
pub struct Standing {
    cfg: ServerConfig,
    name: String,
    /// The standing session's bearer, held in memory alone. Its lock is
    /// taken only to copy the bearer out or put a new one in, never across
    /// an await, so uses served by the held session run side by side.
    bearer: std::sync::Mutex<Option<String>>,
    /// **Held across an opening, and nothing else**: a use that finds no
    /// session serving takes it, looks again under it, and opens only where
    /// no other use opened one meanwhile, so two first uses open one
    /// session between them and a served use waits on no opening.
    opening: tokio::sync::Mutex<()>,
    /// Why the last opening served no one, held so the log carries one line
    /// per change of reason and not one per request.
    refused: std::sync::Mutex<Option<String>>,
}

/// The author the host's issue is recorded under where the server issues
/// the standing session's token to itself.
const STANDING_AUTHOR: &str = "no_sign_in_as";

/// The person's name alone: the held bearer is a credential and never
/// enters a log line.
impl std::fmt::Debug for Standing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Standing")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl Standing {
    /// The standing person the config names, or `None` where it names none
    /// and sign-in is required as before.
    pub fn from_config(cfg: &ServerConfig) -> Option<Self> {
        let name = cfg.no_sign_in_as.clone()?;
        Some(Self {
            cfg: cfg.clone(),
            name,
            bearer: std::sync::Mutex::new(None),
            opening: tokio::sync::Mutex::new(()),
            refused: std::sync::Mutex::new(None),
        })
    }

    /// The person's name, as the config gives it.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// **The start's check**: the person the config names stands and is
    /// enabled, or the server does not start. A name no person holds, or a
    /// disabled person, would serve no one at every request, so it is
    /// refused here, where the store is first open, with the remedy named.
    /// A person holding no grant passes: they are served and see nothing.
    pub async fn ready(&self, store: &Store) -> anyhow::Result<()> {
        match store.person(&self.name).await? {
            None => anyhow::bail!(
                "no_sign_in_as names {}, and no person holds that name: make them with `person bootstrap`, or leave no_sign_in_as out",
                self.name
            ),
            Some(person) if !person.enabled => anyhow::bail!(
                "no_sign_in_as names {}, who is disabled: enable them, or name another person",
                self.name
            ),
            Some(_) => Ok(()),
        }
    }

    /// **The standing session, at this use**: the one held where it still
    /// serves, and a fresh one opened where none is held or the held one has
    /// ended (closed, idle, expired, its person disabled). `None` where no
    /// enabled person holds the name, which the pages answer as any request
    /// with no session.
    pub async fn session(
        &self,
        store: &Store,
        policy: &Policy,
        used: bool,
    ) -> anyhow::Result<Option<Session>> {
        let seen = self.held();
        if let Some(bearer) = seen.as_deref()
            && let Ok(session) = checked(store, policy, bearer, used).await?
        {
            return Ok(Some(session));
        }
        let _opening = self.opening.lock().await;
        // **A second look under the lock**: another use that found none
        // serving may have opened one while this one waited.
        let now = self.held();
        if now != seen
            && let Some(bearer) = now.as_deref()
            && let Ok(session) = checked(store, policy, bearer, used).await?
        {
            return Ok(Some(session));
        }
        let opened = self.open(store).await?;
        *self.bearer.lock().unwrap_or_else(|e| e.into_inner()) = opened.clone();
        match opened.as_deref() {
            Some(bearer) => Ok(checked(store, policy, bearer, used).await?.ok()),
            None => Ok(None),
        }
    }

    /// The held bearer, copied out under its lock for a moment.
    fn held(&self) -> Option<String> {
        self.bearer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The opening lock, held by a test to stand for an opening in flight.
    #[cfg(all(test, feature = "passkeys"))]
    pub(crate) async fn hold_opening(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.opening.lock().await
    }

    /// A session opened for the standing person: the host's sign-in token,
    /// one minute long, issued and audited as the host's command is, then
    /// redeemed and audited as the sign-in page redeems one. The bearer, or
    /// `None` where no enabled person holds the name, the host's issue is
    /// refused, or the token no longer redeems.
    ///
    /// **A read holds a refusal, not a write.** The person is read first,
    /// with no record: where the name names no one, or a disabled person,
    /// no host issue runs, so a person disabled while the server runs adds
    /// nothing to the audit at each use, and a re-enable is served at the
    /// next use with no timer to wait out. A disable landing between this
    /// read and the issue's transaction writes the issue's one refused pair,
    /// once for that race.
    async fn open(&self, store: &Store) -> anyhow::Result<Option<String>> {
        let person = match store.person(&self.name).await? {
            None => {
                self.refused_for(Some(format!("no person holds the name {}", self.name)));
                return Ok(None);
            }
            Some(person) if !person.enabled => {
                self.refused_for(Some(format!("{} is disabled", person.person_id)));
                return Ok(None);
            }
            Some(person) => person,
        };
        let issued = crate::host::sign_in_token(
            store,
            &self.cfg,
            &person.person_id,
            Some(1),
            Some(STANDING_AUTHOR),
        )
        .await;
        if !issued.ok {
            self.refused_for(Some(issued.value.to_string()));
            return Ok(None);
        }
        let Some(token) = issued.value["token"].as_str() else {
            anyhow::bail!("the host's sign-in token answer carries no token");
        };
        let token_digest = identity::digest(token);
        // The redemption matches the person's canonical name, so the read's
        // name serves a config naming the person by `pe-` identity too.
        let name_key = identity::name_key(&person.name);
        let Some(person_id) = store.sign_in_token_person(&token_digest, &name_key).await? else {
            return Ok(None);
        };
        let principal = Principal::Person {
            person_id: &person_id,
            method: PersonMethod::SignInToken,
        };
        let first = store
            .audit_first(principal, Target::Person(&person_id), "session open")
            .await?;
        let opened =
            open_by_sign_in_token(store, &token_digest, &name_key, &person_id, &first).await;
        if !matches!(opened, Ok(Some(_)))
            && !crate::store::commit::outcome_unknown(&opened)
            && let Err(e) = store.audit_outcome(&first, false).await
        {
            tracing::error!("the session's opening outcome record {first} was not written: {e:#}");
        }
        if matches!(opened, Ok(Some(_))) {
            self.refused_for(None);
        }
        opened
    }

    /// Note why an opening served no one, or that one served: a line at
    /// error where the reason changes, one at info where the person is
    /// served again, and nothing where nothing changed.
    fn refused_for(&self, reason: Option<String>) {
        let mut last = self.refused.lock().unwrap_or_else(|e| e.into_inner());
        if *last == reason {
            return;
        }
        match &reason {
            Some(why) => tracing::error!(
                "no_sign_in_as names {}, and no session was opened for them: {why}",
                self.name
            ),
            None => tracing::info!("no_sign_in_as: {} is served again", self.name),
        }
        *last = reason;
    }
}

/// **Why a session no longer serves**, in the order it is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// Signed out, or ended at an earlier use.
    Closed,
    /// Its person is disabled.
    PersonDisabled,
    /// The passkey it was opened with is gone, where a passkey opened it.
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
    if let Some(standing) = &policy.standing {
        return standing.session(store, policy, true).await;
    }
    let Some(bearer) = bearer(headers) else {
        return Ok(None);
    };
    Ok(at_use(store, policy, &bearer).await?.ok())
}

/// **The session a background read names, its last use left as it was**
/// (Spec 2.14, the availability fragments the page re-fetches by itself):
/// every end is checked and closed as at any use, but a poll is not the
/// person's use, so an open tab never holds an idle session open.
pub async fn session_in_background(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
) -> anyhow::Result<Option<Session>> {
    if let Some(standing) = &policy.standing {
        return standing.session(store, policy, false).await;
    }
    let Some(bearer) = bearer(headers) else {
        return Ok(None);
    };
    Ok(checked(store, policy, &bearer, false).await?.ok())
}

/// **A session checked at this use** (design section 6): the session where
/// it serves, why it ended where it has, or `Err(None)` where the bearer
/// names no session. **Each end is checked at every use**: closed, its
/// person disabled, its passkey removed where a passkey opened it (a session
/// a sign-in token opened has none to lose), open past the absolute limit,
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
    checked(store, policy, bearer, true).await
}

/// [`at_use`], refreshing the last use only where `used`.
async fn checked(
    store: &Store,
    policy: &Policy,
    bearer: &str,
    used: bool,
) -> anyhow::Result<Result<Session, Option<Ended>>> {
    let digest = digest(bearer);
    for again in [false, true] {
        let Some((session, ended)) = read(store, policy, &digest).await? else {
            return Ok(Err(None));
        };
        let Some(ended) = ended else {
            if used {
                refresh(store, session.session_id).await?;
            }
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
        "SELECT s.session_id, s.person_id, p.name, s.passkey_id, \
         s.closed_at IS NOT NULL AS closed, \
         NOT p.enabled AS disabled, \
         s.passkey_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM passkey k \
           WHERE k.passkey_id = s.passkey_id AND k.person_id = s.person_id) AS removed, \
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
        passkey_id: row.try_get("passkey_id")?,
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
            "session.passkey_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM passkey k \
             WHERE k.passkey_id = session.passkey_id AND k.person_id = session.person_id)",
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

/// **A session opened** for a person by the passkey they just asserted with
/// (design section 6), after the counter rule has committed: in a
/// transaction of its own, the person checked again as enabled and the
/// passkey as standing, both read under their rows' share locks so neither
/// a disable nor a removal lands between the check and the insert; then a
/// bearer of 32 bytes from the operating system's random source, stored as
/// its digest alone, and the session naming the person and the passkey's
/// own identity. The bearer, for the cookie, or `None` where the person or
/// the passkey no longer stands.
pub async fn open(
    store: &Store,
    person_id: &str,
    passkey_id: &str,
    first: &str,
) -> anyhow::Result<Option<String>> {
    let mut tx = store.pool.begin().await?;
    let standing: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM person p JOIN passkey k ON k.person_id = p.person_id \
         WHERE p.person_id = $1 AND p.enabled AND k.passkey_id = $2 \
         FOR SHARE OF p, k",
    )
    .bind(person_id)
    .bind(passkey_id)
    .fetch_optional(&mut *tx)
    .await?;
    if standing.is_none() {
        return Ok(None);
    }
    let bearer = crate::store::identity::hex(&crate::store::identity::bearer());
    sqlx::query(
        "INSERT INTO session (bearer_digest, person_id, passkey_id, method) \
         VALUES ($1, $2, $3, 'passkey assertion')",
    )
    .bind(digest(&bearer))
    .bind(person_id)
    .bind(passkey_id)
    .execute(&mut *tx)
    .await?;
    crate::store::commit::commit_with_outcome(tx, "session open", store, first).await?;
    Ok(Some(bearer))
}

/// **A session opened by a sign-in token** (Spec 2.8, the operator's ruling
/// of 2026-11-10), after its first record: in a transaction of its own, the
/// token ended `redeemed` by one conditional update that matches it only
/// while it is still redeemable (live, unexpired, its person enabled and the
/// name given theirs), its person's row read under a share lock so no
/// disable lands between the check and the insert, so of two redemptions at
/// once one opens a session and the other finds nothing to end; then a
/// bearer under the rule every bearer is drawn by, stored as its digest, and
/// the session naming the person, the method `sign-in token` and no
/// passkey. The bearer, for the cookie, or `None` where the token no longer
/// redeems for this person.
pub async fn open_by_sign_in_token(
    store: &Store,
    token_digest: &str,
    name_key: &str,
    person_id: &str,
    first: &str,
) -> anyhow::Result<Option<String>> {
    let mut tx = store.pool.begin().await?;
    let enabled: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM person WHERE person_id = $1 AND enabled FOR SHARE")
            .bind(person_id)
            .fetch_optional(&mut *tx)
            .await?;
    if enabled.is_none() {
        return Ok(None);
    }
    let redeemed: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "UPDATE sign_in_token t SET ended_at = now(), ended = 'redeemed' \
         FROM person p WHERE p.person_id = t.person_id \
         AND t.token_digest = $1 AND t.person_id = $3 AND {} \
         RETURNING t.person_id",
        crate::store::identity::SIGN_IN_REDEEMABLE
    )))
    .bind(token_digest)
    .bind(name_key)
    .bind(person_id)
    .fetch_optional(&mut *tx)
    .await?;
    if redeemed.is_none() {
        return Ok(None);
    }
    let bearer = crate::store::identity::hex(&crate::store::identity::bearer());
    sqlx::query(
        "INSERT INTO session (bearer_digest, person_id, passkey_id, method) \
         VALUES ($1, $2, NULL, 'sign-in token')",
    )
    .bind(digest(&bearer))
    .bind(person_id)
    .execute(&mut *tx)
    .await?;
    crate::store::commit::commit_with_outcome(tx, "session open", store, first).await?;
    Ok(Some(bearer))
}

/// **Sign-out**: the session's row closed and the cookie cleared, and the
/// browser sent to the sign-in page, which a plain form posting here
/// follows. A request naming no session is answered the same, since there
/// is nothing to end.
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
        StatusCode::SEE_OTHER,
        [
            (header::SET_COOKIE, cleared_cookie()),
            (header::LOCATION, HeaderValue::from_static("/sign-in")),
        ],
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
