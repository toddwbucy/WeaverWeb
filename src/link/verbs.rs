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
//! **Every verb that mints or replaces the authority holds the authority
//! lock across its store transaction and its file switch**, a session-level
//! advisory lock on the store (`AUTHORITY_LOCK_KEY`), so a registration
//! cannot mint under an authority a concurrent rotation is retiring. A verb
//! that mints checks, inside the lock, that the authority it loaded is the
//! one on disk before it commits a fingerprint.
//!
//! **No verb writes into a repository's tree.** The client configs go to
//! the path the operator names, and the authority to the directory the
//! server's config names.

use crate::config::ServerConfig;
use crate::link::authority::{Authority, ClientCredential};
use crate::link::frames::Plane;
use crate::store::Store;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

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

/// `authority rotate`: every credential revoked, then a new authority,
/// since every client config pinned the old certificate. **The store first
/// and the files second, under the authority lock**: a failure at either
/// step leaves a state the operator can read and re-run from, every
/// credential revoked under the old authority or under the new, where the
/// other order could leave the files replaced with every old credential
/// still live, the running server serving them and a restart stranding
/// every connector. The lock keeps a concurrent registration from minting
/// under the authority being retired.
pub async fn authority_rotate(
    store: &Store,
    cfg: &ServerConfig,
    sans: &[String],
    author: Option<&str>,
) -> Answer {
    let _lock = match store.authority_lock().await {
        Ok(lock) => lock,
        Err(e) => {
            return refused(
                "authority rotate",
                format!("the authority lock could not be taken: {e:#}"),
            );
        }
    };
    let retired = match store.revoke_every_credential(author).await {
        Ok(retired) => retired,
        Err(e) => {
            return refused(
                "authority rotate",
                format!(
                    "the credentials could not be revoked, so the authority was not replaced; nothing changed, re-run when the store answers: {e:#}"
                ),
            );
        }
    };
    match Authority::rotate(&cfg.authority_dir, &cfg.server_name, sans) {
        Ok(authority) => Answer {
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
            format!(
                "every credential is revoked ({retired} agents retired) but the authority could not be replaced and the old one stands; re-run to replace it: {e:#}"
            ),
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
    credential: &ClientCredential,
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

/// Where an agent's two client configs go: `<out>/<box>/<name>/`, nested
/// rather than joined into one file name, since a joined name is not
/// injective (`foo-bar/baz` and `foo/bar-baz` would collide) and a
/// collision would overwrite another agent's keys.
fn config_dir(out: &Path, r#box: &str, name: &str) -> PathBuf {
    out.join(r#box).join(name)
}

fn config_paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join("gate-con.toml"), dir.join("admin-con.toml"))
}

fn write_client_config(path: &Path, content: &str, may_overwrite: bool) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut options = std::fs::OpenOptions::new();
    options.write(true).mode(0o600);
    if may_overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!(
                "{} already exists and this verb does not overwrite a config; move it aside, or rotate the agent",
                path.display()
            )
        } else {
            anyhow::anyhow!("writing {}: {e}", path.display())
        }
    })?;
    // The mode on open applies only where the file is created; a config
    // written over an older one keeps the older mode unless it is set.
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

fn mint_pair(
    name: &str,
    authority: &Authority,
) -> anyhow::Result<(ClientCredential, ClientCredential)> {
    Ok((
        authority.mint_client(name, Plane::Gate)?,
        authority.mint_client(name, Plane::Admin)?,
    ))
}

/// Create the agent's config directory, `0700` at every level created.
fn create_config_dir(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|e| anyhow::anyhow!("creating {}: {e}", dir.display()))
}

fn write_pair(
    cfg: &ServerConfig,
    authority: &Authority,
    dir: &Path,
    name: &str,
    gate: &ClientCredential,
    admin: &ClientCredential,
    may_overwrite: bool,
) -> anyhow::Result<(String, String)> {
    create_config_dir(dir)?;
    let (gate_path, admin_path) = config_paths(dir);
    write_client_config(
        &gate_path,
        &client_config(cfg, authority, name, Plane::Gate, gate),
        may_overwrite,
    )?;
    write_client_config(
        &admin_path,
        &client_config(cfg, authority, name, Plane::Admin, admin),
        may_overwrite,
    )?;
    Ok((
        gate_path.display().to_string(),
        admin_path.display().to_string(),
    ))
}

/// The authority this verb loaded must be the one on disk, checked inside
/// the authority lock before any fingerprint is committed: a rotation that
/// committed between the load and the lock retired what this verb would
/// mint under.
fn authority_still_stands(cfg: &ServerConfig, authority: &Authority) -> anyhow::Result<()> {
    let on_disk = Authority::load(&cfg.authority_dir)?;
    if on_disk.fingerprint() != authority.fingerprint() {
        anyhow::bail!(
            "the authority was rotated since this verb loaded it ({} on disk, {} loaded); re-run",
            &on_disk.fingerprint()[..12],
            &authority.fingerprint()[..12]
        );
    }
    Ok(())
}

/// `register <box> <name> --out <path>`: the row, the two credentials, and
/// the two client configs. A live row for the same box and name is retired
/// in the same transaction. The configs are checked for before the store
/// is touched, since this verb does not overwrite a config.
pub async fn register(
    store: &Store,
    cfg: &ServerConfig,
    authority: &Authority,
    r#box: &str,
    name: &str,
    out: &Path,
    author: Option<&str>,
) -> Answer {
    let dir = config_dir(out, r#box, name);
    let (gate_path, admin_path) = config_paths(&dir);
    for path in [&gate_path, &admin_path] {
        if path.exists() {
            return refused(
                "register",
                format!(
                    "{} already exists and register does not overwrite a config; move it aside, or rotate the agent",
                    path.display()
                ),
            );
        }
    }
    let _lock = match store.authority_lock().await {
        Ok(lock) => lock,
        Err(e) => {
            return refused(
                "register",
                format!("the authority lock could not be taken: {e:#}"),
            );
        }
    };
    if let Err(e) = authority_still_stands(cfg, authority) {
        return refused("register", format!("{e:#}"));
    }
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
    match write_pair(cfg, authority, &dir, name, &gate, &admin, false) {
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
/// client configs written over the agent's own.
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
    let _lock = match store.authority_lock().await {
        Ok(lock) => lock,
        Err(e) => {
            return refused(
                "rotate",
                format!("the authority lock could not be taken: {e:#}"),
            );
        }
    };
    if let Err(e) = authority_still_stands(cfg, authority) {
        return refused("rotate", format!("{e:#}"));
    }
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
    let dir = config_dir(out, &agent.r#box, &agent.name);
    match write_pair(cfg, authority, &dir, &agent.name, &gate, &admin, true) {
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
