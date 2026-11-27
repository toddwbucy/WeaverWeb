//! The link of `weaver-web-Spec` section 8, the server's half: the durable
//! authority, the register verbs, the mutual-TLS listener with its
//! admission, heartbeat, startup reset and epoch, presence, the landing of
//! observations on the register of agents (section 2.12), and the live
//! window. The client half is `client`, which gate-con runs (`gate_con`)
//! and admin-con will, both building against `frames`.
//!
//! **The exclusion the Spec names is the store's own row lock.** Section 8
//! serializes admission, teardown and revocation per credential. The
//! register verbs run as the operator's own process and the listener runs
//! in the server's, and the one exclusion two processes share without a
//! second channel is the row they both write: every path takes
//! `SELECT ... FOR UPDATE` on the agent's row, installs or uninstalls the
//! live connection while it holds that lock, and commits. A revocation
//! then either finds the connection installed and closes it, through the
//! notification the revoking transaction raises and the listener hears, or
//! commits first and the admission's locked recheck sees the credential
//! revoked and refuses. The listener's in-process map is mutated only under
//! that lock, which is what makes it agree with the row.

pub mod admin_con;
pub mod admin_log;
#[cfg(test)]
mod admin_log_tests;
pub mod admin_service;
pub mod authority;
pub mod box_commands;
pub mod box_line;
pub mod client;
pub mod entries;
#[cfg(feature = "enrollment")]
pub mod exchange;
pub mod frames;
pub mod gate_con;
pub mod key_config;
pub mod listener;
pub mod pair_client;
pub mod pair_code;
pub mod pairing;
pub mod register;
pub mod relay;
#[cfg(feature = "enrollment")]
pub mod request;
pub mod service_invoker;
pub mod verbs;

#[cfg(test)]
mod admin_con_tests;
#[cfg(test)]
mod admin_service_tests;
#[cfg(all(test, feature = "passkeys"))]
mod admission_tests;
#[cfg(test)]
mod client_tests;
#[cfg(all(test, feature = "passkeys"))]
mod deadline_tests;
#[cfg(test)]
pub(crate) mod fake_relay;
#[cfg(test)]
pub(crate) mod fake_service;
#[cfg(test)]
mod key_config_tests;
#[cfg(test)]
mod pairing_tests;
#[cfg(test)]
mod run_end_tests;
#[cfg(test)]
mod server_asks_tests;
#[cfg(test)]
mod service_invoker_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod trust_tests;
#[cfg(test)]
mod verbs_audit_tests;

pub use authority::{Authority, ClientCredential, fingerprint};
pub use frames::{FromClient, Plane, Position, Refusal, ToClient};
pub use listener::{AUTHORITY_LOCK_KEY, LISTENER_LOCK_KEY, Listener};
pub use register::{Agent, Credential, CredentialState, Observation};
