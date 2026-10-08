//! TOML configuration, one file per process (Spec section 8). Box
//! facts live in the box's config, and a connector's config is written
//! by the server's register verb at registration and carried to the box
//! by the operator's install script; nothing of it enters a repository.
//! The connectors' configs are read by `link::client` and
//! `link::gate_con`, which hold their own trust rule.
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
    /// its hello. Sixty seconds is the Spec's election; at least four
    /// seconds and at most four days (four times `CADENCE_MAX_SECS`), so
    /// the cadence is one a connector accepts.
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
    /// **How long an enrollment token lives**, in hours (design section 7):
    /// 24 by default, long enough to hand a printed token to its person and
    /// short enough that a lost one dies; refused at load at 0 or past
    /// seven days, which bounds a configuration mistake.
    #[serde(default = "default_enrollment_token_hours")]
    pub enrollment_token_hours: u32,
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

fn default_agent_hop_budget() -> u32 {
    8
}

fn default_enrollment_token_hours() -> u32 {
    24
}

fn load_toml<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("reading config {}: {e}", path.display()))?;
    Ok(toml::from_str(&raw)?)
}

impl ServerConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let cfg: Self = load_toml(path)?;
        // The cadence is the bound divided by four (Spec 8), so a bound under
        // four seconds has no cadence and is refused here rather than at the
        // first hello.
        if cfg.silence_bound_secs < 4 {
            anyhow::bail!(
                "silence_bound_secs is {}, under the 4 the cadence rule admits",
                cfg.silence_bound_secs
            );
        }
        // And a bound whose cadence a connector would refuse is refused
        // here, from the one constant both ends hold.
        let max = 4 * crate::link::frames::CADENCE_MAX_SECS;
        if cfg.silence_bound_secs > max {
            anyhow::bail!(
                "silence_bound_secs is {}, over the {max} (four days) whose cadence a connector accepts",
                cfg.silence_bound_secs
            );
        }
        let most = crate::store::identity::TOKEN_LIFETIME_MAX_HOURS;
        if cfg.enrollment_token_hours == 0 || cfg.enrollment_token_hours > most {
            anyhow::bail!(
                "enrollment_token_hours is {}, outside 1 to {most} (seven days)",
                cfg.enrollment_token_hours
            );
        }
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The server refuses a silence bound whose cadence a connector would
    /// refuse**, so the two ends agree from one constant.
    #[test]
    fn a_silence_bound_past_four_days_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.toml");
        let write = |bound: u64| {
            std::fs::write(
                &path,
                format!(
                    "listen = \"127.0.0.1:0\"\ndatabase = \"postgres:///x\"\nauthority_dir = \"/a\"\nsilence_bound_secs = {bound}\n"
                ),
            )
            .unwrap();
        };
        let max = 4 * crate::link::frames::CADENCE_MAX_SECS;
        write(max);
        assert_eq!(ServerConfig::load(&path).unwrap().silence_bound_secs, max);
        write(max + 1);
        let why = ServerConfig::load(&path).unwrap_err().to_string();
        assert!(why.contains("four days"), "{why}");
        write(3);
        assert!(ServerConfig::load(&path).is_err());
    }

    /// **An enrollment token's configured lifetime is at most seven days**,
    /// refused at load past it or at zero; 24 hours where unset.
    #[test]
    fn a_token_lifetime_past_seven_days_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("server.toml");
        let base =
            "listen = \"127.0.0.1:0\"\ndatabase = \"postgres:///x\"\nauthority_dir = \"/a\"\n";
        std::fs::write(&path, base).unwrap();
        assert_eq!(
            ServerConfig::load(&path).unwrap().enrollment_token_hours,
            24
        );
        for (hours, ok) in [(168, true), (169, false), (0, false)] {
            std::fs::write(&path, format!("{base}enrollment_token_hours = {hours}\n")).unwrap();
            let loaded = ServerConfig::load(&path);
            assert_eq!(loaded.is_ok(), ok, "{hours}: {:?}", loaded.err());
        }
    }
}
