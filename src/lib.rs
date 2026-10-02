//! weaver-web: the WeaverTools suite's frontend (Spec section 1). The
//! server (`weaver-web`) presents HTTP, holds the store and the register
//! of agents, and listens for the two connectors over the link of Spec
//! section 8. The connectors, gate-con and admin-con, are this crate's
//! binaries beside the agent: gate-con (`link::gate_con`, relaying through
//! `adapters/gate.rs`) stands; admin-con is act 5's, the tailer half of
//! `traceview.rs` its seed. `lifecycle.rs` runs the verbs through sudo and is
//! not carried forward (Spec 7.2: no privileged invocation in this crate).

pub mod adapters;
pub mod config;
pub mod fault;
pub mod lifecycle;
pub mod link;
pub mod registry;
pub mod store;
pub mod surfaces;
pub mod traceview;
pub mod web;
