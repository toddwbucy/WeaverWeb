//! The crate's one answer to a fault, shared by every surface.
//!
//! **The chain goes to the log and the response stays generic.** A query's
//! text, a filesystem path, or an upstream's detail is a thing an operator
//! reads in a log and never a thing a browser is handed, which is the
//! posture `web/mod.rs` held for the conversation half and which this type
//! carries for every half.
//!
//! It lives at the crate root because it outlives both: the conversation
//! half retires per the register at
//! `docs/project/inventory-weaver-web-code.md` and the surfaces tree does
//! not, and two copies of one posture drift apart the first time either is
//! touched.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// A fault, carrying its chain for the log and nothing for the browser.
pub struct Fault(pub anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for Fault {
    fn from(error: E) -> Self {
        Fault(error.into())
    }
}

impl IntoResponse for Fault {
    fn into_response(self) -> Response {
        tracing::error!(error = ?self.0, "a surface met a fault");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "the request could not be served",
        )
            .into_response()
    }
}
