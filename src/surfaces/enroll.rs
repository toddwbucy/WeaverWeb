//! conforms: web-a-token-registers-one-passkey-and-opens-no-session
//!
//! **Enrollment by token** (`docs/project/design-2026-10-07-iam.md` sections
//! 4, 6 and 7; Spec 2.13): a person redeems the enrollment token the host
//! issued them to register their first passkey. **A token authenticates its
//! person for that one write and nothing else**, so a redemption opens no
//! session: the person signs in with the passkey afterwards.
//!
//! The page asks for the token pasted, never in a URL, so it lands in no log
//! and no history. Two endpoints serve the browser's module: the options,
//! where the token is checked and a registration ceremony starts, and the
//! finish, where the library verifies the registration and the token is
//! redeemed in one transaction under the identity exclusion, audited with
//! method `enrollment token`, its first record before the write. No token
//! is ever written to a log line.

use askama::Template;
use axum::Router;
use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use webauthn_rs::prelude::{RegisterPublicKeyCredential, Uuid};

use crate::fault::Fault;
use crate::passkeys::{Ceremony, Full, Passkeys};
use crate::store::Store;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity::{self, Redeemed};

/// The one answer to a token that cannot enroll, whatever the reason, so a
/// guess at a token learns nothing of one that exists.
const NOT_REDEEMABLE: &str = "this token cannot enroll a passkey: it is unknown, expired, used or replaced, its person is disabled, or they already hold a passkey";

/// The enrollment surface, taking the passkeys' seam as its own argument;
/// without one, the page says passkeys are not configured and no ceremony
/// is served.
pub fn routes(passkeys: Option<Passkeys>) -> Router<Store> {
    let configured = passkeys.is_some();
    let router = Router::new().route("/enroll", get(move || page(configured)));
    let Some(passkeys) = passkeys else {
        return router;
    };
    let finishing = passkeys.clone();
    router
        .route(
            "/enroll/options",
            post(
                move |State(store): State<Store>, Json(ask): Json<OptionsAsk>| {
                    let passkeys = passkeys.clone();
                    async move { options(store, &passkeys, ask).await }
                },
            ),
        )
        .route(
            "/enroll/finish",
            post(
                move |State(store): State<Store>, Json(ask): Json<FinishAsk>| {
                    let passkeys = finishing.clone();
                    async move { finish(store, &passkeys, ask).await }
                },
            ),
        )
}

#[derive(Template)]
#[template(path = "enroll.html")]
struct EnrollPage {
    here: &'static str,
    who: String,
    configured: bool,
}

async fn page(configured: bool) -> Response {
    match (EnrollPage {
        here: "enroll",
        who: String::new(),
        configured,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => Fault::from(e).into_response(),
    }
}

fn refused(status: StatusCode, why: &'static str) -> Response {
    (status, why).into_response()
}

/// **The user handle a person's passkeys carry**, stable for the person and
/// derived from their identity's sixteen hex alone, so it names no one
/// beyond what the identity does.
pub(crate) fn user_handle(person_id: &str) -> Uuid {
    let bits = person_id
        .strip_prefix("pe-")
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .unwrap_or_default();
    Uuid::from_u64_pair(bits, 0)
}

#[derive(Deserialize)]
pub struct OptionsAsk {
    token: String,
}

/// **The ceremony's start**: the token checked (unexpired, not ended, its
/// person enabled and holding no passkey), a registration ceremony started,
/// and its options answered with the ceremony's identity.
async fn options(store: Store, passkeys: &Passkeys, ask: OptionsAsk) -> Response {
    let token_digest = identity::digest(ask.token.trim());
    let (person_id, name) = match store.redeemable(&token_digest).await {
        Ok(Some(found)) => found,
        Ok(None) => return refused(StatusCode::FORBIDDEN, NOT_REDEEMABLE),
        Err(e) => return Fault::from(e).into_response(),
    };
    let (options, state) = match passkeys.webauthn.start_passkey_registration(
        user_handle(&person_id),
        &name,
        &name,
        None,
    ) {
        Ok(started) => started,
        Err(e) => return Fault::from(anyhow::Error::from(e)).into_response(),
    };
    match passkeys.ceremonies.start(Ceremony::Enrollment {
        state,
        person_id,
        token_digest,
    }) {
        Ok(ceremony) => Json(json!({ "ceremony": ceremony, "options": options })).into_response(),
        Err(Full) => refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "too many ceremonies are in flight; begin again in a few minutes",
        ),
    }
}

#[derive(Deserialize)]
pub struct FinishAsk {
    ceremony: String,
    credential: RegisterPublicKeyCredential,
}

/// **The ceremony's finish**: the ceremony taken once, the registration
/// verified by the library, then the first audit record, then the
/// redemption in one transaction under the identity exclusion (the token
/// checked again, the passkey inserted, the token ended `redeemed`), then
/// the outcome record. **No session is opened.**
async fn finish(store: Store, passkeys: &Passkeys, ask: FinishAsk) -> Response {
    let Some(Ceremony::Enrollment {
        state,
        person_id,
        token_digest,
    }) = passkeys.ceremonies.take(&ask.ceremony)
    else {
        return refused(
            StatusCode::BAD_REQUEST,
            "this ceremony is unknown, already finished or expired; begin again",
        );
    };
    let passkey = match passkeys
        .webauthn
        .finish_passkey_registration(&ask.credential, &state)
    {
        Ok(passkey) => passkey,
        Err(e) => {
            tracing::info!("a passkey registration did not verify: {e}");
            return refused(
                StatusCode::BAD_REQUEST,
                "the authenticator's registration did not verify; begin again",
            );
        }
    };
    let (credential_id, credential) = match (
        serde_json::to_value(passkey.cred_id()),
        serde_json::to_value(&passkey),
    ) {
        (Ok(serde_json::Value::String(id)), Ok(credential)) => (id, credential),
        _ => {
            return Fault::from(anyhow::anyhow!("a passkey did not serialize")).into_response();
        }
    };
    let passkey_id = match store.mint_key("pk").await {
        Ok(id) => id,
        Err(e) => return Fault::from(e).into_response(),
    };
    let principal = Principal::Person {
        person_id: &person_id,
        method: PersonMethod::EnrollmentToken,
    };
    let first = match store
        .audit_first(principal, Target::Passkey(&passkey_id), "passkey enroll")
        .await
    {
        Ok(first) => first,
        Err(e) => return Fault::from(e).into_response(),
    };
    let redeemed = store
        .redeem(
            &token_digest,
            &person_id,
            &passkey_id,
            &credential_id,
            &credential,
        )
        .await;
    if let Err(e) = store
        .audit_outcome(&first, matches!(redeemed, Ok(Redeemed::Enrolled)))
        .await
    {
        tracing::error!("the enrollment's outcome record {first} was not written: {e:#}");
    }
    match redeemed {
        Ok(Redeemed::Enrolled) => Json(json!({
            "enrolled": true,
            "next": "sign in with your passkey",
        }))
        .into_response(),
        Ok(Redeemed::TokenRefused) => refused(StatusCode::FORBIDDEN, NOT_REDEEMABLE),
        Ok(Redeemed::CredentialHeld) => refused(
            StatusCode::CONFLICT,
            "this authenticator's credential is already enrolled",
        ),
        Err(e) => Fault::from(e).into_response(),
    }
}
