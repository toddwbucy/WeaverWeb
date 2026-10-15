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
pub mod agents;
pub mod alerts;
#[cfg(feature = "passkeys")]
pub mod enroll;
pub mod gate;
#[cfg(test)]
pub(crate) mod gate_tests;
pub mod grants;
#[cfg(feature = "passkeys")]
pub mod keys;
pub mod reading;
pub mod record;
#[cfg(feature = "passkeys")]
pub mod sign_in;

use axum::Router;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::store::Store;

/// **What every signed-in page carries above its body** (Spec 2.14): the
/// person's open alerts across the agents they hold a grant on, for the
/// nav, and a banner for each plane of those agents the server cannot
/// reach, both read from the store at the request, so a banner shows while
/// its plane is down and is gone at the next page after it reconnects. A
/// page without a session carries none.
#[derive(Debug, Default)]
pub(crate) struct Chrome {
    pub(crate) open_alerts: i64,
    pub(crate) banners: Vec<Banner>,
}

/// One unavailable plane, as its banner says it.
#[derive(Debug)]
pub(crate) struct Banner {
    pub(crate) agent_id: String,
    pub(crate) label: String,
    pub(crate) says: &'static str,
    pub(crate) since: String,
}

impl Chrome {
    /// The person's chrome, scoped by their grants as the alerts are.
    pub(crate) async fn of(store: &Store, person: &str) -> anyhow::Result<Self> {
        let open_alerts = store
            .open_alerts_by_agent(person)
            .await?
            .iter()
            .map(|(_, n)| n)
            .sum();
        let banners = store
            .unavailable_for(person)
            .await?
            .into_iter()
            .map(|u| Banner {
                says: if u.plane == "admin" {
                    "agent unavailable, admin-con down"
                } else {
                    "turns unavailable, gate-con down"
                },
                since: u
                    .since
                    .map(|at| format!("since {}", at.format("%Y-%m-%d %H:%M:%S UTC")))
                    .unwrap_or_else(|| "never connected".to_owned()),
                agent_id: u.agent_id,
                label: u.label,
            })
            .collect();
        Ok(Self {
            open_alerts,
            banners,
        })
    }
}

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
        .route("/assets/surfaces/instrument.css", get(stylesheet))
        .route("/assets/surfaces/htmx.min.js", get(htmx));
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

/// **The vendored htmx, served from the surfaces' own route** (Spec 2.14):
/// the availability fragments are re-fetched by its `hx-trigger`, no
/// inline script, under the script policy above.
async fn htmx() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_bytes!("../../assets/htmx.min.js").as_slice(),
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
