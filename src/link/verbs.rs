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
    let mut lock = match store.authority_lock().await {
        Ok(lock) => lock,
        Err(e) => {
            return refused(
                "authority rotate",
                format!("the authority lock could not be taken: {e:#}"),
            );
        }
    };
    let retired = match Store::revoke_every_credential_on(lock.connection(), author).await {
        Ok(retired) => Some(retired),
        Err(e) => {
            // A commit's outcome is unknown until it is read back: any
            // credential left live means the revocation did not land.
            match store.any_live_credential().await {
                Ok(false) => None,
                Ok(true) => {
                    return refused(
                        "authority rotate",
                        format!(
                            "the credentials could not be revoked, so the authority was not replaced; nothing changed, re-run when the store answers: {e:#}"
                        ),
                    );
                }
                Err(read) => {
                    return refused(
                        "authority rotate",
                        format!(
                            "{e:#}; and whether the revocation landed could not be read back ({read:#}); nothing of the authority changed, re-run when the store answers"
                        ),
                    );
                }
            }
        }
    };
    // The lock's session is checked right before the switch; the window
    // between this ping and the rename is one round trip and is accepted.
    if let Err(e) = lock.ping().await {
        return refused(
            "authority rotate",
            format!(
                "every credential is revoked but the authority was not replaced: {e:#}; re-run"
            ),
        );
    }
    let authority = match Authority::rotate(&cfg.authority_dir, &cfg.server_name, sans) {
        Ok(authority) => authority,
        Err(e) => {
            return refused(
                "authority rotate",
                format!(
                    "every credential is revoked but the authority could not be replaced and the old one stands; re-run to replace it: {e:#}"
                ),
            );
        }
    };
    // **The reconciliation that makes the lock's loss harmless rather than
    // merely unlikely**: the switch ran after the last ping, so a session
    // lost during it could have let a racing registration verify the old
    // authority and commit a credential it signed. Every credential records
    // its signer, so after the switch, still under the lock, whatever was
    // not signed by the new authority is revoked; the count is zero unless
    // the race happened.
    if let Err(e) = lock.ping().await {
        return refused(
            "authority rotate",
            format!(
                "the authority is replaced ({}) but the lock's session was lost before the reconciliation: {e:#}; re-run, which revokes anything signed by another authority",
                authority.fingerprint()
            ),
        );
    }
    let stranded = match Store::revoke_credentials_not_signed_by_on(
        lock.connection(),
        &authority.fingerprint(),
        author,
    )
    .await
    {
        Ok(n) => n,
        Err(e) => {
            return refused(
                "authority rotate",
                format!(
                    "the authority is replaced ({}) but the reconciliation did not land: {e:#}; re-run, which revokes anything signed by another authority",
                    authority.fingerprint()
                ),
            );
        }
    };
    Answer {
        value: json!({
            "verb": "authority rotate",
            "ok": true,
            "fingerprint": authority.fingerprint(),
            "agents_retired": retired,
            "stranded_credentials_revoked": stranded,
            "note": "every credential is revoked: re-register each agent, and restart the server so it loads the new authority",
        }),
        ok: true,
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
    // The name the server's certificate was minted for, which is what the
    // connector verifies, and not the config's, which could have moved.
    table.insert(
        "server_name".into(),
        authority.server_name().to_owned().into(),
    );
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

/// Write one staged config: created or truncated under a staging name,
/// mode 0600 set explicitly, fully written and synced to disk.
fn stage_file(path: &Path, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| anyhow::anyhow!("staging {}: {e}", path.display()))?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

/// The two configs staged under the agent's directory and not yet in
/// place: the authority's staging-then-publish shape reused. The store
/// commits between the staging and the publish, so a store failure
/// discards the staged files and a publish failure says exactly which file
/// stands where.
struct Staged {
    gate: (PathBuf, PathBuf),
    admin: (PathBuf, PathBuf),
}

impl Staged {
    fn discard(&self) {
        let _ = std::fs::remove_file(&self.gate.0);
        let _ = std::fs::remove_file(&self.admin.0);
    }

    fn publish(self) -> anyhow::Result<(String, String)> {
        std::fs::rename(&self.gate.0, &self.gate.1).map_err(|e| {
            anyhow::anyhow!(
                "the gate config stands staged at {} and could not be renamed to {}: {e}; the admin config stands staged at {}",
                self.gate.0.display(),
                self.gate.1.display(),
                self.admin.0.display()
            )
        })?;
        std::fs::rename(&self.admin.0, &self.admin.1).map_err(|e| {
            anyhow::anyhow!(
                "the gate config stands at {}; the admin config stands staged at {} and could not be renamed to {}: {e}",
                self.gate.1.display(),
                self.admin.0.display(),
                self.admin.1.display()
            )
        })?;
        Ok((
            self.gate.1.display().to_string(),
            self.admin.1.display().to_string(),
        ))
    }
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

fn stage_pair(
    cfg: &ServerConfig,
    authority: &Authority,
    dir: &Path,
    name: &str,
    gate: &ClientCredential,
    admin: &ClientCredential,
) -> anyhow::Result<Staged> {
    create_config_dir(dir)?;
    let (gate_final, admin_final) = config_paths(dir);
    let gate_new = dir.join("gate-con.toml.staging");
    let admin_new = dir.join("admin-con.toml.staging");
    stage_file(
        &gate_new,
        &client_config(cfg, authority, name, Plane::Gate, gate),
    )?;
    // A failure staging the second leaves no minted key behind in the
    // first.
    if let Err(e) = stage_file(
        &admin_new,
        &client_config(cfg, authority, name, Plane::Admin, admin),
    ) {
        let _ = std::fs::remove_file(&gate_new);
        return Err(e);
    }
    Ok(Staged {
        gate: (gate_new, gate_final),
        admin: (admin_new, admin_final),
    })
}

/// **The config's server name must be the authority's.** The server
/// presents the certificate minted for the authority's persisted name and
/// the connectors verify it under that name, so a config that moved would
/// strand every registration made after it; serving, registering and
/// rotating refuse until the config is changed back or the authority is
/// rotated for the new name.
pub fn name_agrees(cfg: &ServerConfig, authority: &Authority) -> anyhow::Result<()> {
    if cfg.server_name != authority.server_name() {
        anyhow::bail!(
            "the config's server_name is {} but the authority was minted for {}; change the config back, or rotate the authority for the new name",
            cfg.server_name,
            authority.server_name()
        );
    }
    Ok(())
}

/// **A box and a name are well formed by the schema's rule, checked here
/// before any filesystem access.** The patterns are migration 0010's,
/// stated once beside them as the same rule: a name is `^[A-Za-z0-9_-]+$`,
/// a box is `^[A-Za-z0-9_.-]+$` and not `.` or `..`. Both name a directory
/// under the operator's output path, so anything with a separator, a
/// traversal or an absolute form must never reach a staging write.
pub fn well_formed(r#box: &str, name: &str) -> anyhow::Result<()> {
    let name_ok = !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !name_ok {
        anyhow::bail!("{name:?} is not a box or a name: a name is letters, digits, _ and -");
    }
    let box_ok = !r#box.is_empty()
        && r#box != "."
        && r#box != ".."
        && r#box
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-');
    if !box_ok {
        anyhow::bail!(
            "{box:?} is not a box or a name: a box is letters, digits, _, . and -, and not . or ..",
            box = r#box
        );
    }
    Ok(())
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
    if let Err(e) = well_formed(r#box, name) {
        return refused("register", e);
    }
    let mut lock = match store.authority_lock().await {
        Ok(lock) => lock,
        Err(e) => {
            return refused(
                "register",
                format!("the authority lock could not be taken: {e:#}"),
            );
        }
    };
    if let Err(e) = name_agrees(cfg, authority).and_then(|_| authority_still_stands(cfg, authority))
    {
        return refused("register", format!("{e:#}"));
    }
    // Checked inside the lock: two registrations of one box and name run
    // one at a time here, so the second sees the first's published configs
    // rather than racing it to the same directory.
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
    // The certificates name the agent by its registered name; the identity
    // the row takes is minted by the store on insert.
    let (gate, admin) = match mint_pair(name, authority) {
        Ok(pair) => pair,
        Err(e) => return refused("register", format!("{e:#}")),
    };
    // Staged before the store commits, published after: a store failure
    // leaves no config, and the credentials are never live without one.
    let staged = match stage_pair(cfg, authority, &dir, name, &gate, &admin) {
        Ok(s) => s,
        Err(e) => return refused("register", format!("{e:#}")),
    };
    let (id, retired, note) = match Store::register_agent_on(
        lock.connection(),
        r#box,
        name,
        author,
        &gate.fingerprint,
        &admin.fingerprint,
        &authority.fingerprint(),
    )
    .await
    {
        Ok((id, retired)) => (id, retired, None),
        Err(e) => {
            // **A commit's outcome is unknown until it is read back.** The
            // error may have come after PostgreSQL applied the commit and
            // before its answer arrived, in which case the register holds
            // live fingerprints and the staged pair must be published, not
            // discarded. The row is read on a pool connection.
            match store.agent_by_fingerprint(&gate.fingerprint).await {
                Ok(Some((row, _))) => (
                    row.agent_id,
                    Vec::new(),
                    Some(format!(
                        "the store's answer was lost after the commit ({e:#}); the row was read back and stands"
                    )),
                ),
                Ok(None) => {
                    staged.discard();
                    return refused("register", format!("{e:#}"));
                }
                Err(read) => {
                    return refused(
                        "register",
                        format!(
                            "{e:#}; and whether the commit landed could not be read back ({read:#}), so the configs stand staged at {} and {} until the store answers; re-run then",
                            staged.gate.0.display(),
                            staged.admin.0.display()
                        ),
                    );
                }
            }
        }
    };
    if let Err(e) = lock.ping().await {
        return refused(
            "register",
            format!(
                "{id} is registered but its configs stand staged at {} and {}: {e:#}; rotate the agent to publish a pair",
                staged.gate.0.display(),
                staged.admin.0.display()
            ),
        );
    }
    match staged.publish() {
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
                "note": note,
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
    if let Err(e) = well_formed(&agent.r#box, &agent.name) {
        return refused("rotate", e);
    }
    let mut lock = match store.authority_lock().await {
        Ok(lock) => lock,
        Err(e) => {
            return refused(
                "rotate",
                format!("the authority lock could not be taken: {e:#}"),
            );
        }
    };
    if let Err(e) = name_agrees(cfg, authority).and_then(|_| authority_still_stands(cfg, authority))
    {
        return refused("rotate", format!("{e:#}"));
    }
    let (gate, admin) = match mint_pair(&agent.name, authority) {
        Ok(pair) => pair,
        Err(e) => return refused("rotate", format!("{e:#}")),
    };
    let dir = config_dir(out, &agent.r#box, &agent.name);
    let staged = match stage_pair(cfg, authority, &dir, &agent.name, &gate, &admin) {
        Ok(s) => s,
        Err(e) => return refused("rotate", format!("{e:#}")),
    };
    let (retired, note) = match Store::rotate_credentials_on(
        lock.connection(),
        &agent,
        author,
        &gate.fingerprint,
        &admin.fingerprint,
        &authority.fingerprint(),
    )
    .await
    {
        Ok(r) => (r, None),
        Err(e) => {
            // A commit's outcome is unknown until it is read back, as in
            // `register`: the row carrying the new fingerprint means the
            // rotation landed.
            match store.agent_by_fingerprint(&gate.fingerprint).await {
                Ok(Some(_)) => (
                    vec![
                        agent.gate.fingerprint.clone(),
                        agent.admin.fingerprint.clone(),
                    ],
                    Some(format!(
                        "the store's answer was lost after the commit ({e:#}); the row was read back and stands"
                    )),
                ),
                Ok(None) => {
                    staged.discard();
                    return refused("rotate", format!("{e:#}"));
                }
                Err(read) => {
                    return refused(
                        "rotate",
                        format!(
                            "{e:#}; and whether the commit landed could not be read back ({read:#}), so the configs stand staged at {} and {} until the store answers; re-run then",
                            staged.gate.0.display(),
                            staged.admin.0.display()
                        ),
                    );
                }
            }
        }
    };
    if let Err(e) = lock.ping().await {
        return refused(
            "rotate",
            format!(
                "{} is rotated but its configs stand staged at {} and {}: {e:#}; re-run rotate to publish a pair",
                agent.agent_id,
                staged.gate.0.display(),
                staged.admin.0.display()
            ),
        );
    }
    match staged.publish() {
        Ok((gate_path, admin_path)) => Answer {
            value: json!({
                "verb": "rotate",
                "ok": true,
                "agent": agent.agent_id.as_str(),
                "gate_fingerprint": gate.fingerprint,
                "admin_fingerprint": admin.fingerprint,
                "configs": [gate_path, admin_path],
                "retired": retired,
                "note": note,
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
