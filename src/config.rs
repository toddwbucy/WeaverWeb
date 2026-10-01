//! TOML configuration, one file per process (Spec section 8). Box
//! facts live in the box's config, and a connector's config is written
//! by the server's register verb at registration and carried to the box
//! by the operator's install script; nothing of it enters a repository.
//! The server's config names where its authority stands and the one
//! tunable of the link, the silence bound.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The server's config, default `/etc/weaver-web/config.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub listen: String,
    /// Where the server listens for the connector's dial. Loopback by
    /// default, so any exposure is the operator's explicit widening
    /// (Spec section 8).
    #[serde(default = "default_link_listen")]
    pub link_listen: String,
    pub database: String,
    /// The directory holding the server's authority and its own
    /// certificate (Spec section 8): created once by `authority init`,
    /// loaded before the listener starts, and never minted at start. The
    /// path is the operator's and never enters a repository.
    pub authority_dir: PathBuf,
    /// **The silence bound is the link's one tunable** (Spec section 8):
    /// a connection silent this long is closed, and each connector is
    /// told its send cadence, the bound divided by four, in the answer to
    /// its hello. Sixty seconds is the Spec's election.
    #[serde(default = "default_silence_bound_secs")]
    pub silence_bound_secs: u64,
    /// The address the client configs carry as the server's, where it
    /// differs from `link_listen` (a bind on every interface, say, or a
    /// forwarded port). Defaults to `link_listen`.
    #[serde(default)]
    pub link_address: Option<String>,
    /// The name the server's certificate carries and a connector verifies
    /// it under, independent of the address it dials, so a server that
    /// moves keeps its identity. Pinned in each client config at
    /// registration.
    #[serde(default = "default_server_name")]
    pub server_name: String,
    /// Participant names holding the admin role. v1 role assignment is
    /// the operator's declaration; IAM later changes how a session
    /// proves it is a participant, not where roles live.
    #[serde(default)]
    pub admins: Vec<String>,
    /// The retired conversation router's agent-hop budget. Retained for
    /// config compatibility; W2 removed its consumer.
    #[serde(default = "default_agent_hop_budget")]
    pub agent_hop_budget: u32,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

/// The connector's config, default `/etc/weaver-web/connector.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectorConfig {
    /// The server's link address, the one line that changes when the
    /// presentation stack moves to another box (Spec section 8:
    /// separated by changing one address).
    #[serde(default = "default_server")]
    pub server: String,
    /// Directory of agent declarations, served read-only to the admin
    /// surface over the link. The files are the operator's own
    /// deployment config.
    #[serde(default = "default_agent_declarations")]
    pub agent_declarations: PathBuf,
    /// Where this box installed the admin binary and its config -
    /// deployment facts that differ per box, defaulted to the
    /// original deployment's paths.
    #[serde(default = "default_admin_bin")]
    pub admin_bin: PathBuf,
    #[serde(default = "default_admin_config")]
    pub admin_config: PathBuf,
    /// Whether the invocation passes WEAVER_ADMIN_CONFIG through sudo.
    /// True for a direct binary under a SETENV sudoers rule. False
    /// when admin_bin names the root-owned wrapper from deploy/, which
    /// fixes the config itself - the shape that makes the sudoers
    /// widening exact.
    #[serde(default = "default_admin_env")]
    pub admin_env: bool,
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub name: String,
    pub gate: PathBuf,
    pub trace: PathBuf,
}

// Read by the upstream adapter once it is implemented. **No standing
// section charters one**: the retired Spec's section 10 did and the
// rewrite carried nothing forward, which is why `adapters/upstream.rs`
// still cites a number that now means open elections. That module
// retires, so its citation is left where it stands and this one does
// not repeat it. Providers are the server's business alone: nothing
// upstream-facing touches the agents' box.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub api: String,
    pub model: String,
    pub key_env: String,
}

fn default_link_listen() -> String {
    "127.0.0.1:8081".into()
}

fn default_server() -> String {
    "127.0.0.1:8081".into()
}

fn default_silence_bound_secs() -> u64 {
    60
}

fn default_server_name() -> String {
    "weaver-web".into()
}

impl ServerConfig {
    /// The address a client config carries for the server.
    pub fn link_address(&self) -> &str {
        self.link_address.as_deref().unwrap_or(&self.link_listen)
    }
}

fn default_agent_declarations() -> PathBuf {
    PathBuf::from("/etc/weaver/agents")
}

fn default_agent_hop_budget() -> u32 {
    8
}

fn load_toml<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("reading config {}: {e}", path.display()))?;
    Ok(toml::from_str(&raw)?)
}

impl ServerConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        load_toml(path)
    }
}

impl ConnectorConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        load_toml(path)
    }
}

fn default_admin_bin() -> PathBuf {
    PathBuf::from("/usr/local/libexec/weaver/weaver-admin")
}

fn default_admin_config() -> PathBuf {
    PathBuf::from("/etc/weaver/config")
}

fn default_admin_env() -> bool {
    true
}
