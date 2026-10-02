//! The HTTP surface. **Spec section 6 is the surfaces tree's and this
//! module retains the legacy admin routes and assets.
//! `docs/project/inventory-weaver-web-code.md` holds its redesign unruled
//! rather than retiring it, waiting on the charter's section 6 trigger
//! being met. So that number says where a surface is chartered and not
//! where this one is.
//!
//! The conversation routes retired under W2. The legacy admin handlers
//! remain compiled, returning 503 until the session/IAM act by the
//! operator's ruling of 2026-09-21.

pub mod admin;

use crate::config::ServerConfig;
use crate::traceview::TraceViews;
use axum::Router;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<ServerConfig>,
    pub traces: TraceViews,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .nest("/admin", admin::routes())
        .route("/assets/{file}", get(asset))
        .with_state(state)
}

// ---------- errors ----------

pub struct AppError(pub anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // The full chain goes to the log; the response stays generic so
        // SQL, filesystem, and upstream detail never reach a browser.
        tracing::error!("request failed: {:#}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal error; see the server log",
        )
            .into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError(e.into())
    }
}

pub type AppResult<T> = Result<T, AppError>;

// ---------- shared view helpers ----------

/// The agents the legacy surfaces name. **Empty since 2026-10-01**: the
/// seed's dialed link that announced a roster left with the act that built
/// the register of agents (Spec 2.12), these routes answer 503 until act 5
/// removes them, and the Agents surface that renders the register is a
/// later act's. Leaving the roster empty was smaller than wiring a surface
/// that is leaving to a register it will never read.
pub async fn nav_agents(_state: &AppState) -> Vec<String> {
    Vec::new()
}

pub fn sse_cursor(headers: &HeaderMap, params: &HashMap<String, String>) -> i64 {
    // Last-Event-ID (reconnect) takes precedence over ?after= (initial).
    headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .or_else(|| params.get("after").and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

// ---------- assets ----------

async fn asset(Path(file): Path<String>) -> Response {
    let (bytes, ctype): (&'static [u8], &'static str) = match file.as_str() {
        "htmx.min.js" => (
            include_bytes!("../../assets/htmx.min.js"),
            "text/javascript",
        ),
        "sse.js" => (include_bytes!("../../assets/sse.js"), "text/javascript"),
        "style.css" => (include_bytes!("../../assets/style.css"), "text/css"),
        _ => return (StatusCode::NOT_FOUND, "no such asset").into_response(),
    };
    ([(header::CONTENT_TYPE, ctype)], bytes).into_response()
}
