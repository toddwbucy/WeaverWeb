//! conforms: web-a-passkey-is-added-after-a-fresh-assertion-and-the-last-never-removed
//!
//! **A person's own passkeys** (`docs/project/design-2026-10-07-iam.md`
//! sections 6, 7 and 10; Spec 2.13): the page that lists them, adding one,
//! and removing one, every request under the person's live session.
//!
//! **Adding is two ceremonies bound by a one-time add grant**: a fresh
//! assertion with a passkey the person holds, under the counter rule by the
//! challenged passkey's own identity, yields a grant in the ceremony table
//! bound to that session and that person; a registration requires and
//! consumes the grant at its start, so it serves one registration from the
//! session that earned it, and a stolen cookie alone adds nothing. A
//! registration that fails has consumed its grant, and the person begins
//! again with a fresh assertion. **Removing never takes the last**, the
//! count under the identity exclusion; every session opened with the
//! removed passkey ends at its next use, this one included.

use std::collections::HashMap;

use askama::Template;
use axum::Router;
use axum::extract::{Form, Json, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use webauthn_rs::prelude::{
    CredentialID, Passkey, PublicKeyCredential, RegisterPublicKeyCredential, WebauthnError,
};

use crate::fault::Fault;
use crate::passkeys::{self, Ceremony, Counted, Full, Passkeys};
use crate::store::Store;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity::{Added, OwnPasskey, Removed};
use crate::surfaces::gate::{self, Policy, Session};
use crate::surfaces::record::NoSession;

/// The answer to an assertion whose counter did not rise.
const POSSIBLE_CLONE: &str = "this passkey's signature counter did not rise, so it may be a copy; no passkey can be added on its word";

/// The answer to a ceremony or grant this session cannot use.
const NOT_THIS_SESSIONS: &str =
    "this ceremony or grant is unknown, already used, expired, or not this session's; begin again";

/// The longest label a person may give a passkey.
const LABEL_BOUND: usize = 100;

/// The passkeys surface, taking the session's policy and the passkeys' seam
/// as its own arguments.
pub fn routes(policy: Policy, passkeys: Passkeys) -> Router<Store> {
    let seams = (policy, passkeys);
    macro_rules! seamed {
        ($handler:ident, $ask:ty) => {{
            let (policy, passkeys) = seams.clone();
            move |State(store): State<Store>, headers: HeaderMap, Json(ask): Json<$ask>| {
                let (policy, passkeys) = (policy.clone(), passkeys.clone());
                async move { $handler(store, &policy, &passkeys, headers, ask).await }
            }
        }};
    }
    let (page_policy, remove_policy) = (seams.0.clone(), seams.0.clone());
    Router::new()
        .route(
            "/passkeys",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let policy = page_policy.clone();
                async move { page(store, &policy, headers).await }
            }),
        )
        .route(
            "/passkeys/add/assert/options",
            post(seamed!(assert_options, NoAsk)),
        )
        .route(
            "/passkeys/add/assert/finish",
            post(seamed!(assert_finish, AssertFinishAsk)),
        )
        .route(
            "/passkeys/add/options",
            post(seamed!(add_options, AddOptionsAsk)),
        )
        .route(
            "/passkeys/add/finish",
            post(seamed!(add_finish, AddFinishAsk)),
        )
        .route(
            "/passkeys/remove",
            post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Form(ask): Form<RemoveAsk>| {
                    let policy = remove_policy.clone();
                    async move { remove(store, &policy, headers, ask).await }
                },
            ),
        )
}

/// The session a request carries, or the answer that it carries none.
async fn session_of(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
) -> Result<Session, Response> {
    match gate::session(store, policy, headers).await {
        Ok(Some(session)) => Ok(session),
        Ok(None) => Err(NoSession.into_response()),
        Err(e) => Err(Fault::from(e).into_response()),
    }
}

fn refused(status: StatusCode, why: &'static str) -> Response {
    (status, why).into_response()
}

fn fault(e: impl Into<anyhow::Error>) -> Response {
    Fault::from(e.into()).into_response()
}

/// A credential ID as the passkey table holds it: the library's own
/// base64url form.
fn credential_id(id: &impl serde::Serialize) -> anyhow::Result<String> {
    match serde_json::to_value(id)? {
        serde_json::Value::String(id) => Ok(id),
        other => anyhow::bail!("a credential ID serialized as {other}"),
    }
}

/// One row of the page.
struct Row {
    passkey_id: String,
    label: String,
    added: String,
    used: String,
    this_session: bool,
}

#[derive(Template)]
#[template(path = "passkeys.html")]
struct PasskeysPage {
    here: &'static str,
    who: String,
    rows: Vec<Row>,
    only_one: bool,
}

/// **The page of the session's person's own passkeys**: nothing of
/// another person's.
async fn page(store: Store, policy: &Policy, headers: HeaderMap) -> Response {
    let session = match session_of(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let held: Vec<OwnPasskey> = match store.own_passkeys(&session.person_id).await {
        Ok(held) => held,
        Err(e) => return fault(e),
    };
    let stamp = |at: chrono::DateTime<chrono::Utc>| at.format("%Y-%m-%d %H:%M UTC").to_string();
    let rows: Vec<Row> = held
        .into_iter()
        .map(|own| Row {
            this_session: own.passkey_id == session.passkey_id,
            label: own.label.unwrap_or_else(|| "unlabelled".to_owned()),
            added: stamp(own.added_at),
            used: own
                .last_used_at
                .map(stamp)
                .unwrap_or_else(|| "never".to_owned()),
            passkey_id: own.passkey_id,
        })
        .collect();
    let only_one = rows.len() == 1;
    match (PasskeysPage {
        here: "passkeys",
        who: session.name,
        rows,
        only_one,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct NoAsk {}

/// **The fresh assertion's start**: an authentication ceremony for the
/// session's person, recording the challenged passkeys by credential ID to
/// their own identities, bound to this session.
async fn assert_options(
    store: Store,
    policy: &Policy,
    passkeys: &Passkeys,
    headers: HeaderMap,
    _: NoAsk,
) -> Response {
    let session = match session_of(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let stored: Vec<(String, String, serde_json::Value)> = match sqlx::query_as(
        "SELECT passkey_id, credential_id, credential FROM passkey WHERE person_id = $1",
    )
    .bind(&session.person_id)
    .fetch_all(&store.pool)
    .await
    {
        Ok(stored) => stored,
        Err(e) => return fault(e),
    };
    let mut challenged = HashMap::new();
    let mut held: Vec<Passkey> = Vec::new();
    for (passkey_id, credential_id, credential) in stored {
        match serde_json::from_value(credential) {
            Ok(passkey) => held.push(passkey),
            Err(e) => return fault(e),
        }
        challenged.insert(credential_id, passkey_id);
    }
    let (options, state) = match passkeys.webauthn.start_passkey_authentication(&held) {
        Ok(started) => started,
        Err(e) => return fault(e),
    };
    match passkeys.ceremonies.start(Ceremony::AddAssertion {
        state,
        person_id: session.person_id,
        session_id: session.session_id,
        challenged,
    }) {
        Ok(ceremony) => Json(json!({ "ceremony": ceremony, "options": options })).into_response(),
        Err(Full) => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "too many ceremonies are in flight; begin again in a few minutes",
        ),
    }
}

#[derive(Deserialize)]
pub struct AssertFinishAsk {
    ceremony: String,
    credential: PublicKeyCredential,
}

/// A possible cloned credential at the fresh assertion, audited as at
/// sign-in, granting nothing; a refusal whose record cannot be written is
/// the server's failure.
async fn clone_refused(store: &Store, person_id: &str, passkey_id: &str) -> Response {
    let principal = Principal::Person {
        person_id,
        method: PersonMethod::PasskeyAssertion,
    };
    if let Err(e) = store
        .audit_refusal(
            principal,
            Target::Passkey(passkey_id),
            "add grant",
            "possible cloned credential: the signature counter did not rise",
        )
        .await
    {
        tracing::error!("a possible cloned credential's audit record was not written: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "the refusal could not be recorded; nothing was done",
        )
            .into_response();
    }
    refused(StatusCode::FORBIDDEN, POSSIBLE_CLONE)
}

/// **The fresh assertion's finish**: the ceremony taken once and this
/// session's; the library's verification; the counter rule by the
/// challenged passkey's identity, committed first; then the grant, audited
/// as the assertion's record by `passkey assertion` on that passkey.
async fn assert_finish(
    store: Store,
    policy: &Policy,
    passkeys: &Passkeys,
    headers: HeaderMap,
    ask: AssertFinishAsk,
) -> Response {
    let session = match session_of(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let Some(Ceremony::AddAssertion {
        state,
        person_id,
        session_id,
        challenged,
    }) = passkeys.ceremonies.take(&ask.ceremony)
    else {
        return refused(StatusCode::BAD_REQUEST, NOT_THIS_SESSIONS);
    };
    if session_id != session.session_id || person_id != session.person_id {
        return refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS);
    }
    let result = match passkeys
        .webauthn
        .finish_passkey_authentication(&ask.credential, &state)
    {
        Ok(result) => result,
        Err(WebauthnError::CredentialPossibleCompromise) => {
            let submitted = match credential_id(&ask.credential.raw_id) {
                Ok(id) => id,
                Err(e) => return fault(e),
            };
            return match challenged.get(&submitted) {
                Some(passkey_id) => clone_refused(&store, &person_id, passkey_id).await,
                None => refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS),
            };
        }
        Err(e) => {
            tracing::info!("a fresh assertion did not verify: {e}");
            return refused(
                StatusCode::BAD_REQUEST,
                "the passkey's assertion did not verify; begin again",
            );
        }
    };
    let asserted = match credential_id(result.cred_id()) {
        Ok(id) => id,
        Err(e) => return fault(e),
    };
    let Some(challenged_id) = challenged.get(&asserted) else {
        return refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS);
    };
    let passkey_id = match passkeys::count(&store, challenged_id, &person_id, &result).await {
        Ok(Counted::Admitted { passkey_id }) => passkey_id,
        Ok(Counted::PossibleClone { passkey_id }) => {
            return clone_refused(&store, &person_id, &passkey_id).await;
        }
        Ok(Counted::Gone) => {
            return refused(
                StatusCode::FORBIDDEN,
                "the passkey you answered with has been removed; begin again",
            );
        }
        Err(e) => return fault(e),
    };
    let principal = Principal::Person {
        person_id: &person_id,
        method: PersonMethod::PasskeyAssertion,
    };
    let first = match store
        .audit_first(principal, Target::Passkey(&passkey_id), "add grant")
        .await
    {
        Ok(first) => first,
        Err(e) => return fault(e),
    };
    let granted = passkeys.ceremonies.start(Ceremony::AddGrant {
        person_id,
        session_id,
    });
    if let Err(e) = store.audit_outcome(&first, granted.is_ok()).await {
        tracing::error!("the add grant's outcome record {first} was not written: {e:#}");
    }
    match granted {
        Ok(grant) => Json(json!({ "grant": grant })).into_response(),
        Err(Full) => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "too many ceremonies are in flight; begin again in a few minutes",
        ),
    }
}

#[derive(Deserialize)]
pub struct AddOptionsAsk {
    grant: String,
}

/// **The registration's start, consuming the grant**: the grant taken once,
/// whatever it answers, and refused unless it is this session's and this
/// person's; a registration ceremony started excluding the person's
/// existing credentials.
async fn add_options(
    store: Store,
    policy: &Policy,
    passkeys: &Passkeys,
    headers: HeaderMap,
    ask: AddOptionsAsk,
) -> Response {
    let session = match session_of(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let Some(Ceremony::AddGrant {
        person_id,
        session_id,
    }) = passkeys.ceremonies.take(&ask.grant)
    else {
        return refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS);
    };
    if session_id != session.session_id || person_id != session.person_id {
        return refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS);
    }
    let stored: Vec<serde_json::Value> =
        match sqlx::query_scalar("SELECT credential FROM passkey WHERE person_id = $1")
            .bind(&person_id)
            .fetch_all(&store.pool)
            .await
        {
            Ok(stored) => stored,
            Err(e) => return fault(e),
        };
    let mut exclude: Vec<CredentialID> = Vec::new();
    for credential in stored {
        match serde_json::from_value::<Passkey>(credential) {
            Ok(passkey) => exclude.push(passkey.cred_id().clone()),
            Err(e) => return fault(e),
        }
    }
    let (options, state) = match passkeys.webauthn.start_passkey_registration(
        crate::surfaces::enroll::user_handle(&person_id),
        &session.name,
        &session.name,
        Some(exclude),
    ) {
        Ok(started) => started,
        Err(e) => return fault(e),
    };
    match passkeys.ceremonies.start(Ceremony::AddRegistration {
        state,
        person_id,
        session_id,
    }) {
        Ok(ceremony) => Json(json!({ "ceremony": ceremony, "options": options })).into_response(),
        Err(Full) => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "too many ceremonies are in flight; begin again in a few minutes",
        ),
    }
}

#[derive(Deserialize)]
pub struct AddFinishAsk {
    ceremony: String,
    credential: RegisterPublicKeyCredential,
    #[serde(default)]
    label: Option<String>,
}

/// **The registration's finish**: the ceremony taken once and this
/// session's; the library's verification; the addition's first record, by
/// `passkey assertion` on the new passkey; the insert under the identity
/// exclusion, a credential ID held by anyone refused; the outcome record.
async fn add_finish(
    store: Store,
    policy: &Policy,
    passkeys: &Passkeys,
    headers: HeaderMap,
    ask: AddFinishAsk,
) -> Response {
    let session = match session_of(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let Some(Ceremony::AddRegistration {
        state,
        person_id,
        session_id,
    }) = passkeys.ceremonies.take(&ask.ceremony)
    else {
        return refused(StatusCode::BAD_REQUEST, NOT_THIS_SESSIONS);
    };
    if session_id != session.session_id || person_id != session.person_id {
        return refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS);
    }
    let label = ask
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty());
    if label.is_some_and(|l| l.len() > LABEL_BOUND || !l.chars().all(|c| !c.is_control())) {
        return refused(
            StatusCode::BAD_REQUEST,
            "a label is at most 100 bytes, with no control characters",
        );
    }
    let passkey = match passkeys
        .webauthn
        .finish_passkey_registration(&ask.credential, &state)
    {
        Ok(passkey) => passkey,
        Err(e) => {
            tracing::info!("a passkey registration did not verify: {e}");
            return refused(
                StatusCode::BAD_REQUEST,
                "the authenticator's registration did not verify; begin again with a fresh assertion",
            );
        }
    };
    let (credential_id, credential) = match (
        credential_id(passkey.cred_id()),
        serde_json::to_value(&passkey),
    ) {
        (Ok(id), Ok(credential)) => (id, credential),
        _ => return fault(anyhow::anyhow!("a passkey did not serialize")),
    };
    let passkey_id = match store.mint_key("pk").await {
        Ok(id) => id,
        Err(e) => return fault(e),
    };
    let principal = Principal::Person {
        person_id: &person_id,
        method: PersonMethod::PasskeyAssertion,
    };
    let first = match store
        .audit_first(principal, Target::Passkey(&passkey_id), "passkey add")
        .await
    {
        Ok(first) => first,
        Err(e) => return fault(e),
    };
    let added = store
        .add_passkey(&person_id, &passkey_id, &credential_id, &credential, label)
        .await;
    if let Err(e) = store
        .audit_outcome(&first, matches!(added, Ok(Added::Inserted)))
        .await
    {
        tracing::error!("the addition's outcome record {first} was not written: {e:#}");
    }
    match added {
        Ok(Added::Inserted) => Json(json!({ "added": true, "next": "/passkeys" })).into_response(),
        Ok(Added::PersonRefused) => refused(StatusCode::FORBIDDEN, NOT_THIS_SESSIONS),
        Ok(Added::CredentialHeld) => refused(
            StatusCode::CONFLICT,
            "this authenticator's credential is already enrolled; begin again with a fresh assertion",
        ),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct RemoveAsk {
    passkey: String,
}

/// **A passkey removed by its person, never their last**, audited by
/// `session`, its first record before the removal. The page is the answer,
/// where this session still stands.
async fn remove(store: Store, policy: &Policy, headers: HeaderMap, ask: RemoveAsk) -> Response {
    let session = match session_of(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let principal = Principal::Person {
        person_id: &session.person_id,
        method: PersonMethod::Session,
    };
    let first = match store
        .audit_first(principal, Target::Passkey(&ask.passkey), "passkey remove")
        .await
    {
        Ok(first) => first,
        Err(e) => return fault(e),
    };
    let removed = store.remove_passkey(&session.person_id, &ask.passkey).await;
    if let Err(e) = store
        .audit_outcome(&first, matches!(removed, Ok(Removed::Removed)))
        .await
    {
        tracing::error!("the removal's outcome record {first} was not written: {e:#}");
    }
    match removed {
        Ok(Removed::Removed) => {
            (StatusCode::SEE_OTHER, [(header::LOCATION, "/passkeys")]).into_response()
        }
        Ok(Removed::NotTheirs) => refused(StatusCode::NOT_FOUND, "no such passkey of yours"),
        Ok(Removed::Last) => refused(
            StatusCode::CONFLICT,
            "this is your last passkey, and your last is never removed; add another first",
        ),
        Err(e) => fault(e),
    }
}
