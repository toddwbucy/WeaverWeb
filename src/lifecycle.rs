//! The verb invocation (Spec section 7.2): sudo weaver-admin, one JSON
//! object on stdout, rendered verbatim; failure never swallowed.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

pub const VERBS: [&str; 3] = ["validate", "load", "unload"];

/// Generous ceiling on a verb invocation: load blocks until the
/// interior is idle (tens of seconds for the 35B admit), so this only
/// catches a hang, never a slow success.
const VERB_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerbOutcome {
    pub verb: String,
    pub agent: String,
    pub exit_code: Option<i32>,
    /// stdout parsed as one JSON object, when it is one.
    pub answer: Option<serde_json::Value>,
    /// raw stdout, kept when parsing failed so nothing is swallowed.
    pub raw_stdout: Option<String>,
    pub stderr: Option<String>,
    /// True when the invocation hit the timeout and was killed.
    pub timed_out: bool,
}

/// Where the box installed the admin binary and its config - box
/// facts, from the connector's config (Spec section 8). `admin_env`
/// false means `admin_bin` is the root-owned wrapper that fixes the
/// config itself, so no environment crosses sudo at all.
pub async fn run_verb(
    verb: &str,
    agent: &str,
    admin_bin: &Path,
    admin_config: &Path,
    admin_env: bool,
) -> anyhow::Result<VerbOutcome> {
    if !VERBS.contains(&verb) {
        anyhow::bail!("'{verb}' is not a lifecycle verb");
    }
    if agent.is_empty()
        || !agent
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!("'{agent}' is not a well-formed agent name");
    }

    let mut cmd = Command::new("sudo");
    cmd.arg("-n");
    if admin_env {
        cmd.arg(format!("WEAVER_ADMIN_CONFIG={}", admin_config.display()));
    }
    let fut = cmd
        .arg(admin_bin)
        .arg(verb)
        .arg(agent)
        .kill_on_drop(true)
        .output();
    let output =
        match tokio::time::timeout(std::time::Duration::from_secs(VERB_TIMEOUT_SECS), fut).await {
            Ok(res) => res?,
            Err(_) => {
                return Ok(VerbOutcome {
                    verb: verb.to_string(),
                    agent: agent.to_string(),
                    exit_code: None,
                    answer: None,
                    raw_stdout: None,
                    stderr: Some(format!(
                        "invocation exceeded {VERB_TIMEOUT_SECS}s and was killed"
                    )),
                    timed_out: true,
                });
            }
        };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let answer: Option<serde_json::Value> = serde_json::from_str(stdout.trim()).ok();

    Ok(VerbOutcome {
        verb: verb.to_string(),
        agent: agent.to_string(),
        exit_code: output.status.code(),
        raw_stdout: if answer.is_none() && !stdout.is_empty() {
            Some(stdout)
        } else {
            None
        },
        answer,
        stderr: if stderr.is_empty() {
            None
        } else {
            Some(stderr)
        },
        timed_out: false,
    })
}
