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
    // The set the revocation is about to select, captured first, so a lost
    // answer is read back against it: a credential registered in between
    // is another fingerprint and does not confuse the answer, and the
    // reconciliation after the switch catches it anyway.
    let selected = match Store::live_fingerprints_on(lock.connection()).await {
        Ok(selected) => selected,
        Err(e) => {
            return refused(
                "authority rotate",
                format!("the register could not be read: {e:#}"),
            );
        }
    };
    let retired = match Store::revoke_every_credential_on(lock.connection(), author).await {
        Ok(retired) => Some(retired),
        Err(e) => {
            // A commit's outcome is unknown until it is read back: any of
            // the selected credentials left live means the revocation did
            // not land.
            match store.any_live_among(&selected).await {
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

/// **An agent's config directory, `<out>/<box>/<name>/`, held by
/// descriptor.** `--out` is canonicalized and opened, then each of the box
/// and the name directory is opened relative to its parent with
/// `O_NOFOLLOW` (created with `mkdirat` at 0700 where absent), and every
/// later create, rename and unlink is relative to the final directory's
/// descriptor, so a symlink at any component below `--out` refuses at the
/// open and nothing swapped in after the open is followed: there is no
/// check-then-open gap. An ancestor of `--out` above its canonical path is
/// the operator's filesystem and is out of scope. **The box and the name
/// directory must be the invoking user's and writable by nobody else**:
/// a shared `--out` lets another party pre-create the predictable
/// directory writable, and then swap an entry under it between the staging
/// and the publish, so each is checked by its descriptor once opened and
/// refused otherwise, naming what was found. The directories are nested
/// rather than joined into one name, since a joined name is not injective
/// and a collision would overwrite another agent's keys.
pub(super) struct ConfigDir {
    fd: std::os::fd::OwnedFd,
    display: PathBuf,
}

const GATE_CONFIG: &str = "gate-con.toml";
const ADMIN_CONFIG: &str = "admin-con.toml";
const STAGING: &str = ".staging";

/// The `*at` calls the config directory is used through.
mod at {
    use std::ffi::{CString, OsStr};
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    fn c(name: impl AsRef<OsStr>) -> io::Result<CString> {
        CString::new(name.as_ref().as_bytes()).map_err(|_| io::Error::other("a path with a NUL"))
    }

    fn parent_fd(parent: Option<&OwnedFd>) -> libc::c_int {
        parent.map_or(libc::AT_FDCWD, |p| p.as_raw_fd())
    }

    /// Open a directory, following no symlink.
    pub fn open_dir(parent: Option<&OwnedFd>, name: impl AsRef<OsStr>) -> io::Result<OwnedFd> {
        let name = c(name)?;
        let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: a valid descriptor or AT_FDCWD, a NUL-terminated path, and
        // the result owned below.
        let fd = unsafe { libc::openat(parent_fd(parent), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a descriptor this call just opened and nothing else owns.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }

    /// Open a directory under its parent, creating it at 0700 where absent.
    pub fn open_or_create_dir(parent: &OwnedFd, name: &str) -> io::Result<OwnedFd> {
        match open_dir(Some(parent), name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let c_name = c(name)?;
                // SAFETY: a valid descriptor and a NUL-terminated name.
                if unsafe { libc::mkdirat(parent.as_raw_fd(), c_name.as_ptr(), 0o700) } < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() != io::ErrorKind::AlreadyExists {
                        return Err(e);
                    }
                }
                open_dir(Some(parent), name)
            }
            other => other,
        }
    }

    /// Create a file new, following no symlink, at 0600.
    pub fn create_new(dir: &OwnedFd, name: &str) -> io::Result<std::fs::File> {
        let name = c(name)?;
        let flags =
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        // SAFETY: a valid descriptor, a NUL-terminated name, and the mode
        // O_CREAT takes.
        let fd =
            unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags, 0o600 as libc::c_uint) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a descriptor this call just opened and nothing else owns.
        Ok(std::fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
    }

    /// Rename within the directory, replacing an existing entry only where
    /// asked.
    pub fn rename(dir: &OwnedFd, from: &str, to: &str, replace: bool) -> io::Result<()> {
        let (from, to) = (c(from)?, c(to)?);
        // SAFETY: a valid descriptor and two NUL-terminated names.
        let r = if replace {
            unsafe { libc::renameat(dir.as_raw_fd(), from.as_ptr(), dir.as_raw_fd(), to.as_ptr()) }
        } else {
            unsafe {
                libc::renameat2(
                    dir.as_raw_fd(),
                    from.as_ptr(),
                    dir.as_raw_fd(),
                    to.as_ptr(),
                    libc::RENAME_NOREPLACE,
                )
            }
        };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn fstat(fd: libc::c_int) -> io::Result<libc::stat> {
        // SAFETY: a valid descriptor and a zeroed stat buffer the call fills.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut st) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(st)
    }

    fn fstatat_nofollow(dir: &OwnedFd, name: &str) -> io::Result<libc::stat> {
        let name = c(name)?;
        // SAFETY: a valid descriptor, a NUL-terminated name, and a zeroed
        // stat buffer the call fills.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let r = unsafe {
            libc::fstatat(
                dir.as_raw_fd(),
                name.as_ptr(),
                &mut st,
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if r < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(st)
    }

    /// The owner and the permission bits of an open descriptor.
    pub fn owner_and_mode(fd: &OwnedFd) -> io::Result<(libc::uid_t, libc::mode_t)> {
        let st = fstat(fd.as_raw_fd())?;
        Ok((st.st_uid, st.st_mode & 0o7777))
    }

    /// The device and inode behind an open descriptor.
    pub fn identity(file: &impl AsRawFd) -> io::Result<(u64, u64)> {
        let st = fstat(file.as_raw_fd())?;
        Ok((st.st_dev, st.st_ino))
    }

    /// The device and inode of an entry in the directory, following no
    /// symlink.
    pub fn identity_at(dir: &OwnedFd, name: &str) -> io::Result<(u64, u64)> {
        let st = fstatat_nofollow(dir, name)?;
        Ok((st.st_dev, st.st_ino))
    }

    pub fn unlink(dir: &OwnedFd, name: &str) -> io::Result<()> {
        let name = c(name)?;
        // SAFETY: a valid descriptor and a NUL-terminated name.
        if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Whether an entry of any kind stands in the directory.
    pub fn exists(dir: &OwnedFd, name: &str) -> io::Result<bool> {
        match fstatat_nofollow(dir, name) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// A symlink opened with O_NOFOLLOW answers ELOOP, and one opened with O_DIRECTORY
// besides answers ENOTDIR on Linux; both are refused the same way, and the entry is
// asked (without following) which it was so the answer names the symlink.
fn describe(path: &Path, e: std::io::Error) -> anyhow::Error {
    let is_symlink = || std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    match e.raw_os_error() {
        Some(libc::ELOOP) => anyhow::anyhow!(
            "{} is a symlink, and a config is written under no symlink",
            path.display()
        ),
        Some(libc::ENOTDIR) if is_symlink() => anyhow::anyhow!(
            "{} is a symlink, and a config is written under no symlink",
            path.display()
        ),
        Some(libc::ENOTDIR) => anyhow::anyhow!("{} is not a directory", path.display()),
        _ => anyhow::anyhow!("opening {}: {e}", path.display()),
    }
}

/// The directory behind the descriptor is owned by the invoking user and
/// carries no group or other write bit; otherwise the refusal says what
/// was found.
fn private(fd: &std::os::fd::OwnedFd, path: &Path) -> anyhow::Result<()> {
    let (uid, mode) =
        at::owner_and_mode(fd).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    // SAFETY: geteuid takes nothing and cannot fail.
    let me = unsafe { libc::geteuid() };
    if uid != me {
        anyhow::bail!(
            "{} is owned by uid {uid}, not the invoking uid {me}; a config is written under no directory another party owns",
            path.display()
        );
    }
    if mode & 0o022 != 0 {
        anyhow::bail!(
            "{} has mode {mode:04o}, writable by group or others; a config is written under no directory another party could write",
            path.display()
        );
    }
    Ok(())
}

impl ConfigDir {
    pub(super) fn open(out: &Path, r#box: &str, name: &str) -> anyhow::Result<Self> {
        let out = std::fs::canonicalize(out)
            .map_err(|e| anyhow::anyhow!("resolving {}: {e}", out.display()))?;
        let out_fd = at::open_dir(None, out.as_os_str()).map_err(|e| describe(&out, e))?;
        let box_display = out.join(r#box);
        let box_fd =
            at::open_or_create_dir(&out_fd, r#box).map_err(|e| describe(&box_display, e))?;
        private(&box_fd, &box_display)?;
        let display = box_display.join(name);
        let fd = at::open_or_create_dir(&box_fd, name).map_err(|e| describe(&display, e))?;
        private(&fd, &display)?;
        Ok(Self { fd, display })
    }

    fn has(&self, entry: &str) -> anyhow::Result<bool> {
        at::exists(&self.fd, entry)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", self.display.join(entry).display()))
    }

    fn path(&self, entry: &str) -> PathBuf {
        self.display.join(entry)
    }
}

// A test's lever on the staging write: the write after the create fails
// once, on the thread that set it.
#[cfg(test)]
thread_local! {
    pub(super) static FAIL_STAGE_WRITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Write one staged config under the directory's descriptor: **created new,
/// following no symlink**, mode 0600, fully written and synced, answering
/// the device and inode of the file written so the publish can check it
/// renames that file. An existing entry of any kind at the staging name
/// refuses, since a symlink there would let a privileged register write a
/// minted key where another owner can read it; a staging file a crashed
/// run left behind is moved aside by the operator. A write or sync that
/// fails after the create unlinks the entry, so the partial file does not
/// refuse every retry as an entry that already exists.
fn stage_file(dir: &ConfigDir, entry: &str, content: &str) -> anyhow::Result<(u64, u64)> {
    use std::io::Write;
    let mut file = at::create_new(&dir.fd, entry).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            anyhow::anyhow!(
                "{} already exists and a staging file is never written over an entry; move it aside",
                dir.path(entry).display()
            )
        } else {
            describe(&dir.path(entry), e)
        }
    })?;
    let written = (|| -> std::io::Result<(u64, u64)> {
        #[cfg(test)]
        if FAIL_STAGE_WRITE.with(|f| f.replace(false)) {
            return Err(std::io::Error::other("a test fault made the write fail"));
        }
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        at::identity(&file)
    })();
    written.map_err(|e| {
        let _ = at::unlink(&dir.fd, entry);
        anyhow::anyhow!(
            "writing {}: {e}; the partial file is removed",
            dir.path(entry).display()
        )
    })
}

/// The two configs staged under the agent's directory and not yet in
/// place: the authority's staging-then-publish shape reused. The store
/// commits between the staging and the publish, so a store failure
/// discards the staged files and a publish failure says exactly which file
/// stands where. Every step is relative to the directory's descriptor, and
/// **the publish renames the inode it staged**: the entry is read without
/// following immediately before each rename and refused where its device
/// and inode are not the staged file's, a tripwire behind the directory
/// check for an entry swapped in between the staging and the publish.
pub(super) struct Staged {
    dir: ConfigDir,
    /// The gate config's staging path and final path, for the answers.
    gate: (PathBuf, PathBuf),
    admin: (PathBuf, PathBuf),
    /// The device and inode of each staged file.
    gate_identity: (u64, u64),
    admin_identity: (u64, u64),
}

impl Staged {
    pub(super) fn discard(&self) {
        let _ = at::unlink(&self.dir.fd, &format!("{GATE_CONFIG}{STAGING}"));
        let _ = at::unlink(&self.dir.fd, &format!("{ADMIN_CONFIG}{STAGING}"));
    }

    /// The entry at the staging name is still the file staged.
    fn still_staged(&self, staging: &str, staged: (u64, u64)) -> anyhow::Result<()> {
        let found = at::identity_at(&self.dir.fd, staging)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", self.dir.path(staging).display()))?;
        if found != staged {
            anyhow::bail!(
                "{} is not the file this run staged (device {} inode {} found, device {} inode {} staged); it is not published",
                self.dir.path(staging).display(),
                found.0,
                found.1,
                staged.0,
                staged.1
            );
        }
        Ok(())
    }

    /// Rename both into place, replacing an existing config only for a
    /// rotation of the agent's own.
    pub(super) fn publish(self, replace: bool) -> anyhow::Result<(String, String)> {
        let gate_staging = format!("{GATE_CONFIG}{STAGING}");
        let admin_staging = format!("{ADMIN_CONFIG}{STAGING}");
        self.still_staged(&gate_staging, self.gate_identity)
            .map_err(|e| {
                anyhow::anyhow!(
                    "{e}; the admin config stands staged at {}",
                    self.admin.0.display()
                )
            })?;
        at::rename(&self.dir.fd, &gate_staging, GATE_CONFIG, replace).map_err(|e| {
            anyhow::anyhow!(
                "the gate config stands staged at {} and could not be renamed to {}: {e}; the admin config stands staged at {}",
                self.gate.0.display(),
                self.gate.1.display(),
                self.admin.0.display()
            )
        })?;
        self.still_staged(&admin_staging, self.admin_identity)
            .map_err(|e| {
                anyhow::anyhow!("the gate config stands at {}; {e}", self.gate.1.display())
            })?;
        at::rename(&self.dir.fd, &admin_staging, ADMIN_CONFIG, replace).map_err(|e| {
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

pub(super) fn mint_pair(
    name: &str,
    authority: &Authority,
) -> anyhow::Result<(ClientCredential, ClientCredential)> {
    Ok((
        authority.mint_client(name, Plane::Gate)?,
        authority.mint_client(name, Plane::Admin)?,
    ))
}

pub(super) fn stage_pair(
    cfg: &ServerConfig,
    authority: &Authority,
    dir: ConfigDir,
    name: &str,
    gate: &ClientCredential,
    admin: &ClientCredential,
) -> anyhow::Result<Staged> {
    let gate_staging = format!("{GATE_CONFIG}{STAGING}");
    let admin_staging = format!("{ADMIN_CONFIG}{STAGING}");
    let gate_identity = stage_file(
        &dir,
        &gate_staging,
        &client_config(cfg, authority, name, Plane::Gate, gate),
    )?;
    // A failure staging the second leaves no minted key behind in the
    // first.
    let admin_identity = match stage_file(
        &dir,
        &admin_staging,
        &client_config(cfg, authority, name, Plane::Admin, admin),
    ) {
        Ok(identity) => identity,
        Err(e) => {
            let _ = at::unlink(&dir.fd, &gate_staging);
            return Err(e);
        }
    };
    Ok(Staged {
        gate: (dir.path(&gate_staging), dir.path(GATE_CONFIG)),
        admin: (dir.path(&admin_staging), dir.path(ADMIN_CONFIG)),
        gate_identity,
        admin_identity,
        dir,
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
    let dir = match ConfigDir::open(out, r#box, name) {
        Ok(dir) => dir,
        Err(e) => return refused("register", format!("{e:#}")),
    };
    for entry in [GATE_CONFIG, ADMIN_CONFIG] {
        match dir.has(entry) {
            Ok(false) => {}
            Ok(true) => {
                return refused(
                    "register",
                    format!(
                        "{} already exists and register does not overwrite a config; move it aside, or rotate the agent",
                        dir.path(entry).display()
                    ),
                );
            }
            Err(e) => return refused("register", format!("{e:#}")),
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
    let staged = match stage_pair(cfg, authority, dir, name, &gate, &admin) {
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
    match staged.publish(false) {
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
    let dir = match ConfigDir::open(out, &agent.r#box, &agent.name) {
        Ok(dir) => dir,
        Err(e) => return refused("rotate", format!("{e:#}")),
    };
    let staged = match stage_pair(cfg, authority, dir, &agent.name, &gate, &admin) {
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
    match staged.publish(true) {
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
