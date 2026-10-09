//! The instrument's surfaces, one module per surface of the charter's
//! section 3, per `weaver-web-Spec` section 1.
//!
//! These stand apart from the legacy admin routes in `web/`. The
//! conversation modules named by `docs/project/inventory-weaver-web-code.md`
//! retired under W2; this tree retains its own layout and routes.
//!
//! **None of them writes the recorded half**, per Spec section 6: a position
//! and a run land by the ingest of section 3.1 alone.

pub mod admin;
#[cfg(feature = "passkeys")]
pub mod enroll;
pub mod gate;
#[cfg(test)]
pub(crate) mod gate_tests;
#[cfg(feature = "passkeys")]
pub mod keys;
pub mod record;
#[cfg(feature = "passkeys")]
pub mod sign_in;

use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::store::Store;

/// Every surface this tree serves, mounted on the store and nothing else.
///
/// **The state is the store because a surface that renders what is kept
/// reads the store and nothing else**, per Spec section 6. A surface that
/// holds a seam takes it as its own argument rather than widening this:
/// the session's policy is that argument here.
pub fn routes(policy: gate::Policy) -> Router<Store> {
    let router = Router::new()
        .merge(record::routes(policy))
        .route("/sign-out", post(gate::sign_out))
        .route("/assets/surfaces/instrument.css", get(stylesheet));
    #[cfg(feature = "passkeys")]
    let router = router.route("/assets/surfaces/passkeys.js", get(passkeys_module));
    script_policy(router)
}

/// **A script runs from this server's own files alone**: the surfaces answer
/// with `Content-Security-Policy: script-src 'self'`, which they can carry
/// since none has an inline script, an inline handler or an `eval`, and the
/// passkey module is a file of this binary's. **htmx holds under it too**,
/// measured on 2026-10-08 in Chromium 153 and Firefox 156: the vendored
/// htmx's `hx-get` and `hx-post` swaps ran with no violation reported, while
/// a control page's inline script was blocked and reported. What would not
/// hold is htmx's evaluating attributes (`hx-on`, `js:` values), which no
/// surface may use while this header stands.
pub fn script_policy<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router.layer(axum::middleware::map_response(
        |mut response: Response| async move {
            response.headers_mut().insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("script-src 'self'"),
            );
            response
        },
    ))
}

/// **The passkey ceremonies' browser module** (design section 4), one
/// hand-written file vendored into the binary.
#[cfg(feature = "passkeys")]
async fn passkeys_module() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../assets/surfaces/passkeys.js"),
    )
}

/// The instrument keeps its own stylesheet, independent of the legacy
/// assets retained by `web/` after the conversation routes retired.
async fn stylesheet() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../assets/surfaces/instrument.css"),
    )
}
