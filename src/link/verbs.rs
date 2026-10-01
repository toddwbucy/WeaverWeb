//! The register verbs (Spec section 8), subcommands of the `weaver-web`
//! binary the operator runs on the server: `authority init` and `rotate`,
//! `register`, `revoke` and `rotate`. Each answers one JSON object on
//! stdout with the exit status agreeing, the shape `weaver-admin` uses, so
//! the install script and the operator read one convention.
//!
//! **A verb reaches a running server's live connection through the store.**
//! The verb is the operator's own process and the listener is the server's;
//! the revoking transaction takes the row's lock, which admission and
//! teardown also take, and raises a notification the listener hears and
//! closes on. A second channel between the two processes would be a second
//! thing to configure and secure for one message the store already carries.
//! See the module header of `link`.
//!
//! **No verb writes into a repository's tree.** The client configs go to
//! the path the operator names, and the authority to the directory the
//! server's config names.

use crate::config::ServerConfig;
use crate::link::authority::Authority;
use crate::link::frames::Plane;
use crate::store::Store;
use serde_json::{Value, json};
use std::path::Path;

/// A verb's answer: the object, and whether the exit status is success.
pub struct Answer {
    pub value: Value,
    pub ok: bool,
}

fn refused(verb: &str, error: impl std::fmt::Display) -> Answer {
    Answer {
        value: json!({ "verb": verb, "ok": false, "error": error.to_string() }),
        ok: false,
    }
}

/// `authority init`: create the server's authority once.
pub fn authority_init(cfg: &ServerConfig, sans: &[String]) -> Answer {
    match Authority::init(&cfg.authority_dir, &cfg.server_name, sans) {
        Ok(authority) => Answer {
            value: json!({
                "verb": "authority init",
                "ok": true,
                "fingerprint": authority.fingerprint(),
                "server_name": cfg.server_name,
            }),
            ok: true,
        },
        Err(e) => refused("authority init", format!("{e:#}")),
    }
}

/// `authority rotate`: a new authority, and every credential revoked, since
/// every client config pinned the old certificate. The answer says so.
pub async fn authority_rotate(
    store: &Store,
    cfg: &ServerConfig,
    sans: &[String],
    author: Option<&str>,
) -> Answer {
    let authority = match Authority::rotate(&cfg.authority_dir, &cfg.server_name, sans) {
        Ok(a) => a,
        Err(e) => return refused("authority rotate", format!("{e:#}")),
    };
    match store.revoke_every_credential(author).await {
        Ok(retired) => Answer {
            value: json!({
                "verb": "authority rotate",
                "ok": true,
                "fingerprint": authority.fingerprint(),
                "agents_retired": retired,
                "note": "every credential is revoked: re-register each agent, and restart the server so it loads the new authority",
            }),
            ok: true,
        },
        Err(e) => refused(
            "authority rotate",
            format!("the authority was replaced but the credentials could not be revoked: {e:#}"),
        ),
    }
}

/// The client config a connector reads, one per plane: its own key and
/// certificate, the server's certificate, the server's link address and
/// the name it is verified under, the agent's name and the plane.
fn client_config(
    cfg: &ServerConfig,
    authority: &Authority,
    agent_name: &str,
    plane: Plane,
    credential: &crate::link::authority::ClientCredential,
) -> String {
    let mut table = toml::Table::new();
    table.insert("server".into(), cfg.link_address().into());
    table.insert("server_name".into(), cfg.server_name.clone().into());
    table.insert("agent".into(), agent_name.into());
    table.insert("plane".into(), plane.as_str().into());
    table.insert(
        "server_certificate".into(),
        authority.certificate_pem().to_owned().into(),
    );
    table.insert(
        "certificate".into(),
        credential.certificate_pem.clone().into(),
    );
    table.insert("key".into(), credential.key_pem.clone().into());
    format!(
        "# Written by `weaver-web register` on {}. Carries this connector's key:\n\
         # keep it on the agent's box and never in a repository.\n{}",
        chrono::Utc::now().format("%Y-%m-%d"),
        toml::to_string(&table).expect("a table of strings serializes")
    )
}

fn write_client_config(path: &Path, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn mint_pair(
    store_agent: &str,
    authority: &Authority,
) -> anyhow::Result<(
    crate::link::authority::ClientCredential,
    crate::link::authority::ClientCredential,
)> {
    Ok((
        authority.mint_client(store_agent, Plane::Gate)?,
        authority.mint_client(store_agent, Plane::Admin)?,
    ))
}

fn write_pair(
    cfg: &ServerConfig,
    authority: &Authority,
    out: &Path,
    name: &str,
    gate: &crate::link::authority::ClientCredential,
    admin: &crate::link::authority::ClientCredential,
) -> anyhow::Result<(String, String)> {
    std::fs::create_dir_all(out).map_err(|e| anyhow::anyhow!("creating {}: {e}", out.display()))?;
    let gate_path = out.join(format!("{name}-gate-con.toml"));
    let admin_path = out.join(format!("{name}-admin-con.toml"));
    write_client_config(
        &gate_path,
        &client_config(cfg, authority, name, Plane::Gate, gate),
    )?;
    write_client_config(
        &admin_path,
        &client_config(cfg, authority, name, Plane::Admin, admin),
    )?;
    Ok((
        gate_path.display().to_string(),
        admin_path.display().to_string(),
    ))
}

/// `register <box> <name> --out <path>`: the row, the two credentials, and
/// the two client configs. A live row for the same box and name is retired
/// in the same transaction.
pub async fn register(
    store: &Store,
    cfg: &ServerConfig,
    authority: &Authority,
    r#box: &str,
    name: &str,
    out: &Path,
    author: Option<&str>,
) -> Answer {
    // The certificates name the agent by its registered name; the identity
    // the row takes is minted by the store on insert.
    let (gate, admin) = match mint_pair(name, authority) {
        Ok(pair) => pair,
        Err(e) => return refused("register", format!("{e:#}")),
    };
    let (id, retired) = match store
        .register_agent(r#box, name, author, &gate.fingerprint, &admin.fingerprint)
        .await
    {
        Ok(x) => x,
        Err(e) => return refused("register", format!("{e:#}")),
    };
    match write_pair(cfg, authority, out, name, &gate, &admin) {
        Ok((gate_path, admin_path)) => Answer {
            value: json!({
                "verb": "register",
                "ok": true,
                "agent": id.as_str(),
                "box": r#box,
                "name": name,
                "gate_fingerprint": gate.fingerprint,
                "admin_fingerprint": admin.fingerprint,
                "configs": [gate_path, admin_path],
                "retired": retired,
            }),
            ok: true,
        },
        Err(e) => refused(
            "register",
            format!("{id} is registered but its client configs could not be written: {e:#}"),
        ),
    }
}

/// `revoke <agent> <plane>`: one credential's state, and its live
/// connection closed in this act.
pub async fn revoke(store: &Store, spec: &str, plane: Plane, author: Option<&str>) -> Answer {
    let agent = match store.resolve_agent(spec).await {
        Ok(a) => a,
        Err(e) => return refused("revoke", e),
    };
    match store.revoke_credential(&agent, plane, author).await {
        Ok(fingerprint) => Answer {
            value: json!({
                "verb": "revoke",
                "ok": true,
                "agent": agent.agent_id.as_str(),
                "plane": plane.as_str(),
                "fingerprint": fingerprint,
            }),
            ok: true,
        },
        Err(e) => refused("revoke", format!("{e:#}")),
    }
}

/// `rotate <agent> --out <path>`: a fresh pair, the old pair revoked, new
/// client configs written.
pub async fn rotate(
    store: &Store,
    cfg: &ServerConfig,
    authority: &Authority,
    spec: &str,
    out: &Path,
    author: Option<&str>,
) -> Answer {
    let agent = match store.resolve_agent(spec).await {
        Ok(a) => a,
        Err(e) => return refused("rotate", e),
    };
    let (gate, admin) = match mint_pair(&agent.name, authority) {
        Ok(pair) => pair,
        Err(e) => return refused("rotate", format!("{e:#}")),
    };
    let retired = match store
        .rotate_credentials(&agent, author, &gate.fingerprint, &admin.fingerprint)
        .await
    {
        Ok(r) => r,
        Err(e) => return refused("rotate", format!("{e:#}")),
    };
    match write_pair(cfg, authority, out, &agent.name, &gate, &admin) {
        Ok((gate_path, admin_path)) => Answer {
            value: json!({
                "verb": "rotate",
                "ok": true,
                "agent": agent.agent_id.as_str(),
                "gate_fingerprint": gate.fingerprint,
                "admin_fingerprint": admin.fingerprint,
                "configs": [gate_path, admin_path],
                "retired": retired,
            }),
            ok: true,
        },
        Err(e) => refused(
            "rotate",
            format!(
                "{} is rotated but its client configs could not be written: {e:#}",
                agent.agent_id
            ),
        ),
    }
}

/// `agents`: the register as it stands, presence derived on each row.
pub async fn agents(store: &Store) -> Answer {
    match store.agents().await {
        Ok(rows) => {
            let rows: Vec<Value> = rows
                .iter()
                .map(|a| {
                    let mut v = serde_json::to_value(a).expect("the row serializes");
                    v["present"] = Value::Bool(a.present());
                    v
                })
                .collect();
            Answer {
                value: json!({ "verb": "agents", "ok": true, "agents": rows }),
                ok: true,
            }
        }
        Err(e) => refused("agents", format!("{e:#}")),
    }
}
