//! weaver-web: the WeaverTools suite's frontend (Spec section 1). The
//! server (`weaver-web`) presents HTTP, holds the store and the register
//! of agents, and listens for the two connectors over the link of Spec
//! section 8. The connectors, gate-con and admin-con, are this crate's
//! binaries beside the agent: gate-con (`link::gate_con`, relaying through
//! `adapters/gate.rs`) and admin-con (`link::admin_con`, reading the trace
//! through its relay, `link::relay`, and answering verbs through the sudo
//! invoker, `link::sudo_invoker`, Spec 7.2).

pub mod adapters;
#[cfg(all(test, feature = "passkeys"))]
mod admin_tests;
pub mod config;
pub mod fault;
#[cfg(all(test, feature = "passkeys"))]
mod grants_tests;
pub mod host;
#[cfg(test)]
mod host_tests;
#[cfg(all(test, feature = "passkeys"))]
mod keys_tests;
pub mod link;
pub mod listen;
#[cfg(test)]
mod listen_tests;
#[cfg(feature = "passkeys")]
pub mod passkeys;
#[cfg(all(test, feature = "passkeys"))]
mod passkeys_tests;
pub mod registry;
#[cfg(all(test, feature = "passkeys"))]
mod sign_in_tests;
pub mod store;
pub mod surfaces;
pub mod traceview;
pub mod web;
