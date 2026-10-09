//! conforms: web-an-admins-write-to-a-person-is-checked-in-its-transaction
//!
//! **An admin's writes to persons** (Spec 2.13; act 11, PR 5a): the page
//! that lists every person, and enrolling, issuing a token, disabling,
//! enabling and renaming, each under a session whose person holds a live
//! admin grant. A plain shell, per the operator's ruling of 2026-10-09 that
//! the backend is finished before any presentation.
//!
//! **Each write is audited with the admin as principal by `session`**, its
//! first record before the write and its outcome after, and lands in one
//! identity transaction that re-checks the authority first
//! (`store::admin`). **A session without the grant is refused at the page
//! and at every write**, a write's refusal being one record carrying it, as
//! a verb refused at the first gate is. **An admin never writes their own
//! row** (Spec 2.13: never their own name or state), refused before any
//! record. **A token is shown once**, in the answer to the write that
//! issued it, under `Cache-Control: no-store`, and never in a URL or a log.

use askama::Template;
use axum::Router;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::fault::Fault;
use crate::store::Store;
use crate::store::admin::{Listed, Refusal};
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity::{IssuedToken, is_person_id};
use crate::surfaces::gate::{self, Policy, Session};
use crate::surfaces::record::NoSession;

/// The answer to a session whose person holds no live admin grant.
pub(crate) const NOT_AN_ADMIN: &str =
    "this is an admin's page, and this session's person holds no admin grant";

/// The answer to a submitted person that is not a person's identity.
const MALFORMED_PERSON: &str =
    "the person asked for is not a person's identity: `pe-` and sixteen lowercase hex";

/// The answer to an admin's write to their own row.
const OWN_ROW: &str = "an admin never writes their own row; another admin, or the host, does";

/// The admin's surface, taking the session's policy and the configured
/// lifetime of an enrollment token as its own arguments.
pub fn routes(policy: Policy, token_hours: u32) -> Router<Store> {
    let seams = Seams {
        policy,
        token_hours,
    };
    macro_rules! seamed {
        ($handler:ident, $ask:ty) => {{
            let seams = seams.clone();
            move |State(store): State<Store>, headers: HeaderMap, Form(ask): Form<$ask>| {
                let seams = seams.clone();
                async move { $handler(store, &seams, headers, ask).await }
            }
        }};
    }
    let page_seams = seams.clone();
    Router::new()
        .route(
            "/admin/persons",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let seams = page_seams.clone();
                async move { page(store, &seams, headers).await }
            }),
        )
        .route("/admin/persons/enroll", post(seamed!(enroll, EnrollAsk)))
        .route("/admin/persons/token", post(seamed!(token, PersonAsk)))
        .route("/admin/persons/disable", post(seamed!(disable, VersionAsk)))
        .route("/admin/persons/enable", post(seamed!(enable, VersionAsk)))
        .route("/admin/persons/rename", post(seamed!(rename, RenameAsk)))
        .merge(crate::surfaces::grants::routes(seams.policy))
}

#[derive(Clone)]
struct Seams {
    policy: Policy,
    token_hours: u32,
}

pub(crate) fn fault(e: impl Into<anyhow::Error>) -> Response {
    Fault::from(e.into()).into_response()
}

/// A refused write's answer, in words the admin can act on.
pub(crate) fn refused(refusal: &Refusal) -> Response {
    let status = match refusal {
        Refusal::AuthorityGone | Refusal::OwnGrant | Refusal::HoldsRole | Refusal::Fixed => {
            StatusCode::FORBIDDEN
        }
        Refusal::NoSuchPerson
        | Refusal::NoSuchGrant
        | Refusal::NoSuchRole
        | Refusal::NoSuchAgent => StatusCode::NOT_FOUND,
        Refusal::Name(_) | Refusal::Scope(_) | Refusal::Vocabulary(_) => StatusCode::BAD_REQUEST,
        Refusal::Stale
        | Refusal::Taken(_)
        | Refusal::HoldsPasskey
        | Refusal::LastAdmin
        | Refusal::Already
        | Refusal::Held
        | Refusal::Revoked => StatusCode::CONFLICT,
    };
    (status, refusal.to_string()).into_response()
}

/// The session a request carries, its person holding a live admin grant,
/// or the answer that it does not. A refusal for want of the grant is not
/// recorded here; a write records its own.
pub(crate) async fn admin_session(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
) -> Result<Result<Session, Session>, Response> {
    let session = match gate::session(store, policy, headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return Err(NoSession.into_response()),
        Err(e) => return Err(fault(e)),
    };
    match store.is_admin(&session.person_id).await {
        Ok(true) => Ok(Ok(session)),
        Ok(false) => Ok(Err(session)),
        Err(e) => Err(fault(e)),
    }
}

/// **A write's admin, or the write's answer**: no session answers 401; a
/// session whose person holds no live admin grant is refused, one audit
/// record carrying the refusal, a refusal whose record cannot be written
/// answering as the server's failure.
pub(crate) async fn writer(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
    target: Target<'_>,
    action: &str,
) -> Result<Session, Response> {
    match admin_session(store, policy, headers).await? {
        Ok(session) => Ok(session),
        Err(session) => {
            let principal = Principal::Person {
                person_id: &session.person_id,
                method: PersonMethod::Session,
            };
            match store
                .audit_refusal(principal, target, action, "not an admin")
                .await
            {
                Ok(_) => Err((StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response()),
                Err(e) => {
                    tracing::error!("a refusal of {action} was not recorded: {e:#}");
                    Err(fault(e))
                }
            }
        }
    }
}

/// **The write, between its two audit records**: the first before it, the
/// outcome after, an act whose first record cannot be written not acting.
pub(crate) async fn audited<T, F>(
    store: &Store,
    session: &Session,
    target: Target<'_>,
    action: &str,
    write: F,
) -> Result<anyhow::Result<Result<T, Refusal>>, Response>
where
    F: std::future::Future<Output = anyhow::Result<Result<T, Refusal>>>,
{
    #[cfg(test)]
    hold_after_the_admin_read(&session.person_id).await;
    let principal = Principal::Person {
        person_id: &session.person_id,
        method: PersonMethod::Session,
    };
    let first = match store.audit_first(principal, target, action).await {
        Ok(first) => first,
        Err(e) => return Err(fault(e)),
    };
    let written = write.await;
    if let Err(e) = store
        .audit_outcome(&first, matches!(written, Ok(Ok(_))))
        .await
    {
        tracing::error!("the outcome record of {first} was not written: {e:#}");
    }
    Ok(written)
}

/// Back to the page, the write landed.
pub(crate) fn landed<T>(
    written: anyhow::Result<Result<T, Refusal>>,
    page: &'static str,
) -> Response {
    match written {
        Ok(Ok(_)) => (StatusCode::SEE_OTHER, [(header::LOCATION, page)]).into_response(),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

/// One row of the page.
struct Row {
    listed: Listed,
    own: bool,
}

#[derive(Template)]
#[template(path = "admin_persons.html")]
struct PersonsPage {
    here: &'static str,
    who: String,
    admin: bool,
    rows: Vec<Row>,
}

/// **Every person**, enabled or not, whether they hold a passkey, whether a
/// token is outstanding, whether they hold the admin grant. Nothing of an
/// agent.
async fn page(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, &seams.policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let listed = match store.listed_persons().await {
        Ok(listed) => listed,
        Err(e) => return fault(e),
    };
    let rows = listed
        .into_iter()
        .map(|listed| Row {
            own: listed.person_id == session.person_id,
            listed,
        })
        .collect();
    match (PersonsPage {
        here: "admin",
        who: session.name,
        admin: true,
        rows,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Template)]
#[template(path = "admin_token.html")]
struct TokenPage {
    here: &'static str,
    who: String,
    admin: bool,
    name: String,
    token: String,
    expires: String,
}

/// **The token, shown once**: the one answer that carries it, kept from
/// every cache.
fn shown_once(session: Session, name: String, issued: IssuedToken) -> Response {
    match (TokenPage {
        here: "admin",
        who: session.name,
        admin: true,
        name,
        token: issued.value,
        expires: issued.expires_at.format("%Y-%m-%d %H:%M UTC").to_string(),
    })
    .render()
    {
        Ok(html) => ([(header::CACHE_CONTROL, "no-store")], Html(html)).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct EnrollAsk {
    name: String,
}

/// **A person enrolled**: the row and a token for it, the token shown once.
async fn enroll(store: Store, seams: &Seams, headers: HeaderMap, ask: EnrollAsk) -> Response {
    const ACTION: &str = "person enroll";
    let person = match store.mint_key("pe").await {
        Ok(person) => person,
        Err(e) => return fault(e),
    };
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Person(&person),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let written = match audited(
        &store,
        &session,
        Target::Person(&person),
        ACTION,
        store.admin_enroll(
            &session.person_id,
            session.session_id,
            &person,
            &ask.name,
            seams.token_hours,
        ),
    )
    .await
    {
        Ok(written) => written,
        Err(answer) => return answer,
    };
    match written {
        Ok(Ok(issued)) => shown_once(session, ask.name.trim().to_owned(), issued),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct PersonAsk {
    person: String,
}

/// **A token issued** for a person holding no passkey, shown once.
async fn token(store: Store, seams: &Seams, headers: HeaderMap, ask: PersonAsk) -> Response {
    const ACTION: &str = "person token";
    if !is_person_id(&ask.person) {
        return (StatusCode::BAD_REQUEST, MALFORMED_PERSON).into_response();
    }
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Person(&ask.person),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    if ask.person == session.person_id {
        return (StatusCode::FORBIDDEN, OWN_ROW).into_response();
    }
    let written = match audited(
        &store,
        &session,
        Target::Person(&ask.person),
        ACTION,
        store.admin_issue_token(
            &session.person_id,
            session.session_id,
            &ask.person,
            seams.token_hours,
        ),
    )
    .await
    {
        Ok(written) => written,
        Err(answer) => return answer,
    };
    match written {
        Ok(Ok((issued, name))) => shown_once(session, name, issued),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct VersionAsk {
    person: String,
    version: i64,
}

/// **A person disabled**, their outstanding tokens revoked in the same
/// write, never the last enabled admin.
async fn disable(store: Store, seams: &Seams, headers: HeaderMap, ask: VersionAsk) -> Response {
    const ACTION: &str = "person disable";
    if !is_person_id(&ask.person) {
        return (StatusCode::BAD_REQUEST, MALFORMED_PERSON).into_response();
    }
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Person(&ask.person),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    if ask.person == session.person_id {
        return (StatusCode::FORBIDDEN, OWN_ROW).into_response();
    }
    match audited(
        &store,
        &session,
        Target::Person(&ask.person),
        ACTION,
        store.admin_disable(
            &session.person_id,
            session.session_id,
            &ask.person,
            ask.version,
        ),
    )
    .await
    {
        Ok(written) => landed(written, "/admin/persons"),
        Err(answer) => answer,
    }
}

/// **A disabled person enabled again.**
async fn enable(store: Store, seams: &Seams, headers: HeaderMap, ask: VersionAsk) -> Response {
    const ACTION: &str = "person enable";
    if !is_person_id(&ask.person) {
        return (StatusCode::BAD_REQUEST, MALFORMED_PERSON).into_response();
    }
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Person(&ask.person),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    if ask.person == session.person_id {
        return (StatusCode::FORBIDDEN, OWN_ROW).into_response();
    }
    match audited(
        &store,
        &session,
        Target::Person(&ask.person),
        ACTION,
        store.admin_enable(
            &session.person_id,
            session.session_id,
            &ask.person,
            ask.version,
        ),
    )
    .await
    {
        Ok(written) => landed(written, "/admin/persons"),
        Err(answer) => answer,
    }
}

#[derive(Deserialize)]
pub struct RenameAsk {
    person: String,
    version: i64,
    name: String,
}

/// **A person renamed**, the canonical name unique and never an identity's
/// shape.
async fn rename(store: Store, seams: &Seams, headers: HeaderMap, ask: RenameAsk) -> Response {
    const ACTION: &str = "person rename";
    if !is_person_id(&ask.person) {
        return (StatusCode::BAD_REQUEST, MALFORMED_PERSON).into_response();
    }
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Person(&ask.person),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    if ask.person == session.person_id {
        return (StatusCode::FORBIDDEN, OWN_ROW).into_response();
    }
    match audited(
        &store,
        &session,
        Target::Person(&ask.person),
        ACTION,
        store.admin_rename(
            &session.person_id,
            session.session_id,
            &ask.person,
            ask.version,
            &ask.name,
        ),
    )
    .await
    {
        Ok(written) => landed(written, "/admin/persons"),
        Err(answer) => answer,
    }
}

/// A hold in a write after the surface read its admin, before the write's
/// transaction, keyed by the admin, one of a list so tests running at once
/// each keep their own.
#[cfg(test)]
pub(crate) static READ_HOLD: std::sync::Mutex<Vec<crate::store::admin::AdminHold>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
async fn hold_after_the_admin_read(admin: &str) {
    let hold = {
        let mut holds = READ_HOLD.lock().unwrap();
        holds
            .iter()
            .position(|(key, ..)| key == admin)
            .map(|at| holds.remove(at))
    };
    if let Some((_, read, release)) = hold {
        read.notify_one();
        release.notified().await;
    }
}
