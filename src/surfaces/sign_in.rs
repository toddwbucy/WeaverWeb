//! conforms: web-a-person-authenticates-by-passkey-alone
//!
//! **Sign-in** (`docs/project/design-2026-10-07-iam.md` sections 6 and 10;
//! Spec 2.8 and 2.13): name-first. The person gives their name; the options
//! endpoint finds the person by the name's canonical form and starts an
//! authentication ceremony for their passkeys; the finish has the library
//! verify the assertion, applies the counter rule in a transaction of its own
//! committed first, and only then opens the session, in a second
//! transaction, audited with method `passkey assertion`.
//!
//! **What is audited and what is not**: a failed signature proves no
//! principal and is refused unaudited; a counter that did not rise is
//! audited as a possible cloned credential, the signature having proved the
//! person, and opens nothing, the passkey staying enrolled so a clone cannot
//! lock its owner out; a session's opening is audited on the passkey it
//! began with, its first record before the write. Every audit of an attempt
//! names the passkey the ceremony challenged, by its own identity. No
//! bearer, digest or name reaches a log line.

use std::collections::HashMap;

use askama::Template;
use axum::Router;
use axum::extract::{Json, State};
use axum::http::{StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use serde_json::json;
use webauthn_rs::prelude::{Passkey, PublicKeyCredential, WebauthnError};

use crate::fault::Fault;
use crate::passkeys::{self, Ceremony, Counted, Full, Passkeys};
use crate::store::Store;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity;
use crate::surfaces::gate;

/// The one answer to a name that cannot sign in, whether no person has it,
/// the person is disabled, or they hold no passkey. Name-first sign-in
/// answers whether a name exists at all, which the design accepts and
/// states (section 6); it says nothing more than that.
const NO_SIGN_IN: &str = "no passkey sign-in is open for that name";

/// The answer to an assertion by a passkey removed since the challenge.
const PASSKEY_REMOVED: &str = "the passkey you signed in with has been removed; sign in with another passkey, or ask your admin";

/// The answer to an assertion whose counter did not rise.
const POSSIBLE_CLONE: &str = "this passkey's signature counter did not rise, so it may be a copy; sign in with another passkey, or ask your admin";

/// The sign-in surface, taking the passkeys' seam as its own argument;
/// without one, the page says passkeys are not configured.
pub fn routes(passkeys: Option<Passkeys>) -> Router<Store> {
    let configured = passkeys.is_some();
    let router = Router::new().route("/sign-in", get(move || page(configured)));
    let Some(passkeys) = passkeys else {
        return router;
    };
    let finishing = passkeys.clone();
    router
        .route(
            "/sign-in/options",
            post(
                move |State(store): State<Store>, Json(ask): Json<OptionsAsk>| {
                    let passkeys = passkeys.clone();
                    async move { options(store, &passkeys, ask).await }
                },
            ),
        )
        .route(
            "/sign-in/finish",
            post(
                move |State(store): State<Store>, Json(ask): Json<FinishAsk>| {
                    let passkeys = finishing.clone();
                    async move { finish(store, &passkeys, ask).await }
                },
            ),
        )
}

#[derive(Template)]
#[template(path = "sign_in.html")]
struct SignInPage {
    here: &'static str,
    who: String,
    configured: bool,
}

async fn page(configured: bool) -> Response {
    match (SignInPage {
        here: "sign-in",
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

#[derive(Deserialize)]
pub struct OptionsAsk {
    name: String,
}

/// **The ceremony's start**: the person found by the name's canonical form,
/// enabled and holding a passkey, and an authentication ceremony started
/// for their passkeys.
async fn options(store: Store, passkeys: &Passkeys, ask: OptionsAsk) -> Response {
    let found: Option<String> =
        match sqlx::query_scalar("SELECT person_id FROM person WHERE name_key = $1 AND enabled")
            .bind(identity::name_key(ask.name.trim()))
            .fetch_optional(&store.pool)
            .await
        {
            Ok(found) => found,
            Err(e) => return Fault::from(anyhow::Error::from(e)).into_response(),
        };
    let Some(person_id) = found else {
        return refused(StatusCode::FORBIDDEN, NO_SIGN_IN);
    };
    let stored: Vec<(String, String, serde_json::Value)> = match sqlx::query_as(
        "SELECT passkey_id, credential_id, credential FROM passkey WHERE person_id = $1",
    )
    .bind(&person_id)
    .fetch_all(&store.pool)
    .await
    {
        Ok(stored) => stored,
        Err(e) => return Fault::from(anyhow::Error::from(e)).into_response(),
    };
    if stored.is_empty() {
        return refused(StatusCode::FORBIDDEN, NO_SIGN_IN);
    }
    // **The challenged passkeys, by credential ID to their own identity**,
    // which the finish uses alone.
    let mut challenged = HashMap::new();
    let mut held: Vec<Passkey> = Vec::new();
    for (passkey_id, credential_id, credential) in stored {
        match serde_json::from_value(credential) {
            Ok(passkey) => held.push(passkey),
            Err(e) => return Fault::from(anyhow::Error::from(e)).into_response(),
        }
        challenged.insert(credential_id, passkey_id);
    }
    let (options, state) = match passkeys.webauthn.start_passkey_authentication(&held) {
        Ok(started) => started,
        Err(e) => return Fault::from(anyhow::Error::from(e)).into_response(),
    };
    match passkeys.ceremonies.start(Ceremony::SignIn {
        state,
        person_id,
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
pub struct FinishAsk {
    ceremony: String,
    credential: PublicKeyCredential,
}

/// A credential ID as the passkey table holds it: the library's own
/// base64url form.
fn credential_id(id: &impl serde::Serialize) -> anyhow::Result<String> {
    match serde_json::to_value(id)? {
        serde_json::Value::String(id) => Ok(id),
        other => anyhow::bail!("a credential ID serialized as {other}"),
    }
}

/// **A possible cloned credential, audited** (design section 6): a refusal
/// record, its principal the passkey's person by `passkey assertion`, on
/// the passkey. Nothing is opened and the passkey is not disabled.
async fn clone_refused(store: &Store, person_id: &str, passkey_id: &str) -> Response {
    let principal = Principal::Person {
        person_id,
        method: PersonMethod::PasskeyAssertion,
    };
    if let Err(e) = store
        .audit_refusal(
            principal,
            Target::Passkey(passkey_id),
            "sign in",
            "possible cloned credential: the signature counter did not rise",
        )
        .await
    {
        // **The refusal's record lost is the server's failure**, never the
        // ordinary refusal, which would read as a refusal the audit holds.
        tracing::error!("a possible cloned credential's audit record was not written: {e:#}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "the refusal could not be recorded; nothing was done",
        )
            .into_response();
    }
    refused(StatusCode::FORBIDDEN, POSSIBLE_CLONE)
}

/// **The ceremony's finish**: the ceremony taken once; the assertion
/// verified by the library; the counter rule, committed in its own
/// transaction; then the session's opening, audited, in its own.
async fn finish(store: Store, passkeys: &Passkeys, ask: FinishAsk) -> Response {
    let Some(Ceremony::SignIn {
        state,
        person_id,
        challenged,
    }) = passkeys.ceremonies.take(&ask.ceremony)
    else {
        return refused(
            StatusCode::BAD_REQUEST,
            "this ceremony is unknown, already finished or expired; begin again",
        );
    };
    let verified = passkeys
        .webauthn
        .finish_passkey_authentication(&ask.credential, &state);
    let result = match verified {
        Ok(result) => result,
        Err(WebauthnError::CredentialPossibleCompromise) => {
            // The library's own check against the copy loaded at the start:
            // the signature verified, so the person is proved, and the
            // attempt is audited against the passkey the ceremony
            // challenged, whether or not it still stands.
            let submitted = match credential_id(&ask.credential.raw_id) {
                Ok(id) => id,
                Err(e) => return Fault::from(e).into_response(),
            };
            return match challenged.get(&submitted) {
                Some(passkey_id) => clone_refused(&store, &person_id, passkey_id).await,
                None => refused(StatusCode::FORBIDDEN, NO_SIGN_IN),
            };
        }
        Err(e) => {
            tracing::info!("a passkey assertion did not verify: {e}");
            return refused(
                StatusCode::BAD_REQUEST,
                "the passkey's assertion did not verify; begin again",
            );
        }
    };
    let asserted = match credential_id(result.cred_id()) {
        Ok(id) => id,
        Err(e) => return Fault::from(e).into_response(),
    };
    // **The passkey the ceremony challenged, by its own identity**: the
    // mapping recorded at the start, never a lookup by credential ID now.
    let Some(challenged_id) = challenged.get(&asserted) else {
        return refused(StatusCode::FORBIDDEN, NO_SIGN_IN);
    };
    let passkey_id = match passkeys::count(&store, challenged_id, &person_id, &result).await {
        Ok(Counted::Admitted { passkey_id }) => passkey_id,
        Ok(Counted::PossibleClone { passkey_id }) => {
            return clone_refused(&store, &person_id, &passkey_id).await;
        }
        Ok(Counted::Gone) => return refused(StatusCode::FORBIDDEN, PASSKEY_REMOVED),
        Err(e) => return Fault::from(e).into_response(),
    };
    let principal = Principal::Person {
        person_id: &person_id,
        method: PersonMethod::PasskeyAssertion,
    };
    let first = match store
        .audit_first(principal, Target::Passkey(&passkey_id), "session open")
        .await
    {
        Ok(first) => first,
        Err(e) => return Fault::from(e).into_response(),
    };
    let opened = gate::open(&store, &person_id, &passkey_id).await;
    if let Err(e) = store
        .audit_outcome(&first, matches!(opened, Ok(Some(_))))
        .await
    {
        tracing::error!("the session's opening outcome record {first} was not written: {e:#}");
    }
    match opened {
        Ok(Some(bearer)) => (
            [(header::SET_COOKIE, gate::set_cookie(&bearer))],
            Json(json!({ "signed_in": true, "next": "/record" })),
        )
            .into_response(),
        Ok(None) => refused(StatusCode::FORBIDDEN, NO_SIGN_IN),
        Err(e) => Fault::from(e).into_response(),
    }
}
