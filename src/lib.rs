//! weaver-web: the WeaverTools suite's frontend, two binaries in one
//! crate (Spec section 1). The server (`weaver-web`) presents HTTP
//! and holds everything that is not box-bound. The connector
//! (`weaver-web-connector`) runs on the agents' box, holds the
//! box-bound reaches, and dials the server (Spec section 8, which
//! names the dial where the charter names only the placement).

pub mod adapters;
pub mod config;
pub mod fault;
pub mod lifecycle;
pub mod registry;
pub mod store;
pub mod surfaces;
pub mod traceview;
pub mod web;
pub mod wire;
