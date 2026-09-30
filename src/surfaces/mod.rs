//! The instrument's surfaces, one module per surface of the charter's
//! section 3, per `weaver-web-Spec` section 1.
//!
//! These stand apart from the legacy admin routes in `web/`. The
//! conversation modules named by `docs/project/inventory-weaver-web-code.md`
//! retired under W2; this tree retains its own layout and routes.
//!
//! **None of them writes the recorded half**, per Spec section 6: a position
//! and a run land by the ingest of section 3.1 alone.

pub mod gate;
pub mod record;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::get;

use crate::store::Store;

/// Every surface this tree serves, mounted on the store and nothing else.
///
/// **The state is the store because a surface that renders what is kept
/// reads the store and nothing else**, per Spec section 6. A surface that
/// holds a seam takes it as its own argument rather than widening this.
pub fn routes() -> Router<Store> {
    Router::new()
        .merge(record::routes())
        .route("/assets/surfaces/instrument.css", get(stylesheet))
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
