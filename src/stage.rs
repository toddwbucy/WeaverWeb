//! conforms: web-the-install-is-staged-unprivileged-from-named-parameters
//!
//! **The install's staging tool** (Spec 8.1): it reads a parameter file the
//! operator keeps outside the repository, renders the three units from the
//! templates under `install/units/`, composes each agent's two connector
//! configs from `register`'s handover files and the box facts, and writes
//! them with a manifest into a directory the operator names. It runs as the
//! operator's own user, refuses to run as root, and applies nothing: placing
//! the staged files is the operator's (Spec 8.1, section 10).
//!
//! **No location, account, group or socket has a default here**: each is a
//! parameter, so none is written into the repository. Every check runs, and
//! every file is rendered in memory, before anything is written; a refusal
//! after the writes begin (a staged connector config its own loader
//! refuses) removes everything the tool wrote, so `<out>` is left as it was
//! found.

use crate::link::admin_con::{AdminConConfig, STOP_GRACE};
use crate::link::gate_con::GateConConfig;
use anyhow::{Context, bail};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The server's unit, a plain unit.
pub const SERVER_UNIT: &str = "weaver-web.service";
/// gate-con's unit, a template instanced by the agent's name.
pub const GATE_UNIT: &str = "weaver-web-gate-con@.service";
/// admin-con's unit, a template instanced by the agent's name.
pub const ADMIN_UNIT: &str = "weaver-web-admin-con@.service";

const SERVER_TEMPLATE: &str = include_str!("../install/units/weaver-web.service.in");
const GATE_TEMPLATE: &str = include_str!("../install/units/weaver-web-gate-con@.service.in");
const ADMIN_TEMPLATE: &str = include_str!("../install/units/weaver-web-admin-con@.service.in");

/// **The margin admin-con's stop timeout stands above its grace** (Spec
/// 8.1), so the service manager's timeout never cuts the grace short.
pub const STOP_MARGIN_SECS: u64 = 60;

/// The manifest's file name under `<out>`, and its header line.
pub const MANIFEST: &str = "manifest.tsv";
pub const MANIFEST_HEADER: &str = "kind\tsource\tdestination\towner\tgroup\tmode\tsha256";

/// **The parameter file** (Spec 8.1). Unknown members are refused, so a
/// misspelled member is a refusal and never a default.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    pub locations: Locations,
    pub server: Option<Server>,
    pub connectors: Option<Connectors>,
    #[serde(default, rename = "agent")]
    pub agents: Vec<Agent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locations {
    /// Where the programs are placed.
    pub binaries: String,
    /// Where the unit files and their drop-ins are placed.
    pub units: String,
    /// The directory holding the server's `config.toml` and
    /// `connectors/<agent>/` beneath it.
    pub config_root: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    pub account: String,
    /// The built `weaver-web` to place.
    pub binary: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Connectors {
    pub gate_account: String,
    pub admin_account: String,
    /// admin-con's `PATH`: absolute directories joined by `:`.
    pub admin_path: String,
    pub gate_con_binary: String,
    pub admin_con_binary: String,
    /// The directory `register` wrote into (its `--out`).
    pub handover: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    #[serde(rename = "box")]
    pub r#box: String,
    pub name: String,
    pub gate_socket: String,
    pub trace_socket: String,
    pub weaver_admin: String,
    pub admin_log: String,
    pub stop_grace_secs: Option<u64>,
    pub verb_bound_secs: Option<u64>,
}

/// What a successful staging wrote.
#[derive(Debug)]
pub struct Staged {
    pub manifest: PathBuf,
    /// The manifest's lines past its header.
    pub lines: usize,
}

/// **Root never stages** (Spec 8.1): the staged tree is the operator's own,
/// and nothing in this repository runs privileged but admin-con's invoker.
/// A function of the effective uid it is handed, so a test reaches it.
pub fn refuse_root(euid: u32) -> anyhow::Result<()> {
    if euid == 0 {
        bail!(
            "weaver-web-stage runs as the operator's own user and never as root: it applies nothing, and the staged directory is the operator's"
        );
    }
    Ok(())
}

/// **A path the units and the configs can carry as written** (Spec 8.1):
/// absolute, with no empty, `.` or `..` component and no trailing
/// separator, and only printable ASCII outside the characters the unit
/// syntax or a `PATH` list would read (whitespace, quotes, the backslash,
/// `%`, `$`, `;` and `:`).
pub fn path_ok(member: &str, p: &str) -> anyhow::Result<PathBuf> {
    if !p.starts_with('/') {
        bail!("{member} {p:?} is not an absolute path");
    }
    if p.len() > 1 && p.ends_with('/') {
        bail!("{member} {p:?} ends with a separator");
    }
    if p == "/" {
        bail!("{member} is the root directory, which no location may be");
    }
    if let Some(c) = p
        .chars()
        .find(|c| !c.is_ascii_graphic() || "\"'\\%$;:".contains(*c))
    {
        bail!("{member} {p:?} carries {c:?}, which a unit or a PATH list would read");
    }
    if p[1..]
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("{member} {p:?} has an empty, . or .. component");
    }
    Ok(PathBuf::from(p))
}

/// **admin-con's `PATH`**: one or more absolute directories joined by `:`,
/// each held to `path_ok`.
pub fn admin_path_ok(p: &str) -> anyhow::Result<()> {
    if p.is_empty() {
        bail!("connectors.admin_path is empty");
    }
    for part in p.split(':') {
        path_ok("an element of connectors.admin_path", part)?;
    }
    Ok(())
}

/// **`--out` as the tool takes it** (Spec 8.1): a relative path against the
/// working directory, then its `.` components and repeated or trailing
/// separators dropped, the lexical normalization `Path::components` gives.
/// A `..` is kept, so `path_ok` refuses it as in any path, and no symlink is
/// resolved.
pub fn out_path(cwd: &Path, out: &Path) -> PathBuf {
    let joined = if out.is_absolute() {
        out.to_path_buf()
    } else {
        cwd.join(out)
    };
    // The working directory is absolute, and `Path::components` drops every
    // `.` past an absolute path's root.
    joined.components().collect()
}

/// **A portable user name** (Spec 8.1): a lowercase letter or `_`, then
/// lowercase letters, digits, `_` or `-`, at most 32 in all.
pub fn account_ok(member: &str, a: &str) -> anyhow::Result<()> {
    let mut bytes = a.bytes();
    let first_ok = matches!(bytes.next(), Some(b'a'..=b'z' | b'_'));
    let rest_ok = bytes.all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'));
    if !first_ok || !rest_ok || a.len() > 32 {
        bail!(
            "{member} {a:?} is not a portable user name: a lowercase letter or _, then lowercase letters, digits, _ or -, at most 32"
        );
    }
    Ok(())
}

/// One line of the manifest: a directory or a file, with its owner, group
/// (`-`, the owner's primary group) and mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub kind: Kind,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub owner: String,
    pub mode: &'static str,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
}

impl Entry {
    /// The entry as one tab-separated manifest line, in `MANIFEST_HEADER`'s
    /// order; a directory's source and sum are `-`.
    fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}\t-\t{}\t{}",
            match self.kind {
                Kind::Dir => "dir",
                Kind::File => "file",
            },
            if self.kind == Kind::Dir {
                "-".to_owned()
            } else {
                self.source.display().to_string()
            },
            self.destination.display(),
            self.owner,
            self.mode,
            self.sha256.as_deref().unwrap_or("-"),
        )
    }
}

/// A file the plan places: rendered or composed here, or a built program.
enum Content {
    Staged(Vec<u8>),
    /// A built program and its sum, read once when it was checked.
    Program(PathBuf, String),
}

struct PlannedFile {
    destination: PathBuf,
    owner: String,
    mode: &'static str,
    content: Content,
    /// The connector config this file is, checked by its loader once staged.
    config: Option<(Plane, String)>,
}

#[derive(Clone, Copy)]
enum Plane {
    Gate,
    Admin,
}

/// **Render a template**: every `@NAME@` on a line that is not a comment
/// must be one the caller fills, so a template that grew a parameter the
/// tool does not know never stages half-rendered; then each is replaced.
pub fn render(template: &str, values: &[(&str, &str)]) -> anyhow::Result<String> {
    for line in template.lines() {
        if line.trim_start().starts_with('#') {
            continue;
        }
        let mut parts = line.split('@');
        parts.next();
        while let Some(name) = parts.next() {
            let is_name =
                !name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_');
            if is_name {
                if !values.iter().any(|(n, _)| *n == name) {
                    bail!("the template carries @{name}@, which the tool does not fill");
                }
                parts.next();
            }
        }
    }
    let mut out = template.to_owned();
    for (name, value) in values {
        out = out.replace(&format!("@{name}@"), value);
    }
    Ok(out)
}

/// The rendered server unit.
pub fn server_unit(account: &str, binaries: &str, config_root: &str) -> anyhow::Result<String> {
    render(
        SERVER_TEMPLATE,
        &[
            ("ACCOUNT", account),
            ("BINARIES", binaries),
            ("CONFIG_ROOT", config_root),
        ],
    )
}

/// The rendered gate-con template.
pub fn gate_unit(account: &str, binaries: &str, config_root: &str) -> anyhow::Result<String> {
    render(
        GATE_TEMPLATE,
        &[
            ("ACCOUNT", account),
            ("BINARIES", binaries),
            ("CONFIG_ROOT", config_root),
        ],
    )
}

/// The rendered admin-con template.
pub fn admin_unit(
    account: &str,
    binaries: &str,
    config_root: &str,
    admin_path: &str,
) -> anyhow::Result<String> {
    render(
        ADMIN_TEMPLATE,
        &[
            ("ACCOUNT", account),
            ("BINARIES", binaries),
            ("CONFIG_ROOT", config_root),
            ("ADMIN_PATH", admin_path),
        ],
    )
}

/// **The stop timeout's drop-in for one admin-con instance** (Spec 8.1):
/// the agent's grace plus `STOP_MARGIN_SECS`.
pub fn stop_timeout_drop_in(grace_secs: u64) -> anyhow::Result<String> {
    let timeout = grace_secs
        .checked_add(STOP_MARGIN_SECS)
        .context("stop_grace_secs is too large to carry a stop timeout above it")?;
    Ok(format!(
        "# Written by weaver-web-stage: this agent's stop_grace_secs plus {STOP_MARGIN_SECS} (Spec 8.1).\n[Service]\nTimeoutStopSec={timeout}\n"
    ))
}

/// **The directives admin-con's unit never sets** (Spec 8.1), by the clause
/// of `weaver-admin-operator-contract` section 2 each would break.
pub const FORBIDDEN: &[(&str, &[&str])] = &[
    (
        "no new privileges",
        &[
            "NoNewPrivileges",
            "SystemCallFilter",
            "SystemCallArchitectures",
            "RestrictAddressFamilies",
            "RestrictNamespaces",
            "PrivateDevices",
            "ProtectKernelTunables",
            "ProtectKernelModules",
            "ProtectKernelLogs",
            "ProtectClock",
            "MemoryDenyWriteExecute",
            "RestrictRealtime",
            "RestrictSUIDSGID",
            "DynamicUser",
            "LockPersonality",
        ],
    ),
    (
        "a capability bounding set narrower than root's",
        &["CapabilityBoundingSet", "SecureBits", "PrivateUsers"],
    ),
    ("a system call filter", &["SystemCallLog"]),
    (
        "a private or read-only view",
        &[
            "ProtectSystem",
            "ProtectHome",
            "ProtectControlGroups",
            "ProtectProc",
            "ProcSubset",
            "ReadOnlyPaths",
            "InaccessiblePaths",
            "TemporaryFileSystem",
            "BindReadOnlyPaths",
            "RootDirectory",
            "RootImage",
        ],
    ),
];

/// **Every directive in a unit's text that admin-con's unit must not set**,
/// with the clause it breaks: the list above, and any `Limit` directive (a
/// resource limit passes through to the agent). Comment lines are skipped.
pub fn forbidden_in(unit: &str) -> Vec<(String, &'static str)> {
    let mut found = Vec::new();
    for line in unit.lines() {
        let line = line.trim_start();
        if line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.starts_with("Limit") {
            found.push((key.to_owned(), "a resource limit"));
            continue;
        }
        for (clause, keys) in FORBIDDEN {
            if keys.contains(&key) {
                found.push((key.to_owned(), *clause));
            }
        }
    }
    found
}

/// **A connector config as staged** (Spec 8.1): the handover file
/// unchanged, then the box facts from the parameter file, written as TOML
/// so every value is quoted as the format requires.
fn compose(handover: &str, facts: toml::Table) -> String {
    let mut out = handover.to_owned();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("\n# The box's facts, from the parameter file (weaver-web-stage):\n");
    out.push_str(&toml::to_string(&facts).expect("a table of strings and integers serializes"));
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// **A file the tool reads, opened without following a symlink** (Spec
/// 8.1), as the connectors' trust rule reads a config
/// (`client::read_private`): open with `O_NOFOLLOW` and `O_NONBLOCK`, then
/// judge that descriptor and read it, so the check and the read are one look
/// at the name and a FIFO never blocks the open.
fn read_regular(what: &str, path: &Path) -> anyhow::Result<Vec<u8>> {
    let (file, _) = open_regular(what, path)?;
    read_all(what, path, file)
}

/// The no-follow open and the regular-file judgement of `read_regular`,
/// answering the descriptor and its metadata, so a caller judges more of the
/// same descriptor before it reads.
fn open_regular(what: &str, path: &Path) -> anyhow::Result<(std::fs::File, std::fs::Metadata)> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ELOOP) => anyhow::anyhow!(
                "{what} {} is a symlink, and the tool reads through none",
                path.display()
            ),
            Some(libc::ENOENT) => anyhow::anyhow!("{what} {} is missing", path.display()),
            _ => anyhow::anyhow!("{what} {} cannot be opened: {e}", path.display()),
        })?;
    let meta = file
        .metadata()
        .with_context(|| format!("{what} {} cannot be read", path.display()))?;
    if !meta.file_type().is_file() {
        bail!("{what} {} is not a regular file", path.display());
    }
    Ok((file, meta))
}

/// Read the whole of a descriptor `open_regular` gave.
fn read_all(what: &str, path: &Path, mut file: std::fs::File) -> anyhow::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("{what} {} cannot be read", path.display()))?;
    Ok(bytes)
}

/// A built program the parameter file names: a regular file, never a
/// symlink, read for its sum.
fn program(member: &str, p: &str) -> anyhow::Result<(PathBuf, String)> {
    let path = path_ok(member, p)?;
    let bytes = read_regular(member, &path)?;
    Ok((path, sha256_hex(&bytes)))
}

/// **A handover file `register` wrote, held to the connectors' own trust
/// rule** (Spec 8.1, `client::read_private`): a regular file, never a
/// symlink, the invoking user's, at `0600` or tighter, judged on the
/// descriptor it is read from. It names the server the connector will trust
/// and carries the connector's key, so the staged copy's `0600` must not
/// stand for a source anyone else could have written or read. A handover
/// downloaded through a browser arrives under the browser's umask, so the
/// refusal names the mode found and the operator tightens it.
fn handover_file(path: &Path, euid: u32) -> anyhow::Result<String> {
    let what = "the handover file";
    let (file, meta) = open_regular(what, path)?;
    if meta.uid() != euid {
        bail!(
            "{what} {} is owned by uid {}, not the invoking uid {euid}; a file carrying a key is read only from its owner",
            path.display(),
            meta.uid()
        );
    }
    let mode = meta.mode() & 0o7777;
    if mode & !0o600 != 0 {
        bail!(
            "{what} {} has mode {mode:04o}; a file carrying a key is 0600 or tighter, so tighten it (chmod 600) before staging",
            path.display()
        );
    }
    let bytes = read_all(what, path, file)?;
    String::from_utf8(bytes).map_err(|_| anyhow::anyhow!("{what} {} is not UTF-8", path.display()))
}

/// **`<out>` holds keys** (Spec 8.1): absent, or an empty directory of the
/// operator's own at `0700` or tighter. Answers whether it stands already.
fn out_ok(out: &Path, euid: u32) -> anyhow::Result<bool> {
    match std::fs::symlink_metadata(out) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(anyhow::anyhow!("{} cannot be read: {e}", out.display())),
        Ok(meta) => {
            if !meta.file_type().is_dir() {
                bail!("{} stands and is not a directory", out.display());
            }
            if meta.uid() != euid {
                bail!("{} is not the operator's own directory", out.display());
            }
            if meta.permissions().mode() & 0o077 != 0 {
                bail!(
                    "{} is open to others ({:o}); the staged directory holds keys and must be 0700 or tighter",
                    out.display(),
                    meta.permissions().mode() & 0o777
                );
            }
            if std::fs::read_dir(out)?.next().is_some() {
                bail!("{} is not empty; stage into a new directory", out.display());
            }
            Ok(true)
        }
    }
}

/// **Stage the install** (Spec 8.1): check, render and compose in memory,
/// then write under `out`, load each staged connector config through its
/// own loader, and write the manifest. A refusal leaves `out` as found.
pub fn stage(params_text: &str, out: &Path, euid: u32) -> anyhow::Result<Staged> {
    refuse_root(euid)?;
    let params: Params = toml::from_str(params_text)
        .map_err(|e| anyhow::anyhow!("the parameter file does not parse: {e}"))?;
    let out = path_ok("--out", &out.display().to_string())?;
    let existed = out_ok(&out, euid)?;
    stage_checked(&params, &out, existed, euid)
}

/// **What this run holds of `<out>`** (Spec 8.1): only what it made by an
/// exclusive make, so a refusal removes only that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    /// Nothing: `<out>`, or its `files/`, was another run's when this run
    /// came to make it.
    Nothing,
    /// `<out>`, made by this run, and not yet its `files/`.
    Out,
    /// `<out>`'s `files/`, made by this run in an `<out>` that stood.
    Tree,
    /// `<out>` and its `files/`, both made by this run.
    OutAndTree,
}

/// **Everything after `<out>` is checked**: plan, write holding only what
/// this run makes, and on a refusal remove only that. A step of its own so
/// a test can stand in for a second run whose check of `<out>` passed before
/// the first run wrote.
pub(crate) fn stage_checked(
    params: &Params,
    out: &Path,
    existed: bool,
    euid: u32,
) -> anyhow::Result<Staged> {
    let (dirs, files) = plan(params, euid)?;
    let mut held = Held::Nothing;
    match write(out, existed, &dirs, &files, &mut held) {
        Ok(staged) => Ok(staged),
        Err(e) => {
            clean(out, held);
            Err(e)
        }
    }
}

/// Every check, and every file rendered or composed, before a write.
fn plan(params: &Params, euid: u32) -> anyhow::Result<(Vec<Entry>, Vec<PlannedFile>)> {
    let loc = &params.locations;
    let binaries = path_ok("locations.binaries", &loc.binaries)?;
    let units = path_ok("locations.units", &loc.units)?;
    let config_root = path_ok("locations.config_root", &loc.config_root)?;
    if params.server.is_none() && params.agents.is_empty() {
        bail!("the parameter file stages nothing: give [server], or [connectors] and an [[agent]]");
    }
    let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
    let mut files: Vec<PlannedFile> = Vec::new();
    let root_file = |destination: PathBuf, mode: &'static str, content: Content| PlannedFile {
        destination,
        owner: "root".to_owned(),
        mode,
        content,
        config: None,
    };

    if let Some(server) = &params.server {
        account_ok("server.account", &server.account)?;
        let (bin, sum) = program("server.binary", &server.binary)?;
        dirs.insert(binaries.clone());
        dirs.insert(units.clone());
        files.push(root_file(
            binaries.join("weaver-web"),
            "0755",
            Content::Program(bin, sum),
        ));
        let unit = server_unit(&server.account, &loc.binaries, &loc.config_root)?;
        files.push(root_file(
            units.join(SERVER_UNIT),
            "0644",
            Content::Staged(unit.into_bytes()),
        ));
    }

    match (&params.connectors, params.agents.is_empty()) {
        (None, false) => bail!("an [[agent]] is given and [connectors] is missing"),
        (Some(_), true) => bail!("[connectors] is given and no [[agent]] is"),
        _ => {}
    }
    if let Some(c) = &params.connectors {
        account_ok("connectors.gate_account", &c.gate_account)?;
        account_ok("connectors.admin_account", &c.admin_account)?;
        admin_path_ok(&c.admin_path)?;
        let handover = path_ok("connectors.handover", &c.handover)?;
        let (gate_bin, gate_sum) = program("connectors.gate_con_binary", &c.gate_con_binary)?;
        let (admin_bin, admin_sum) = program("connectors.admin_con_binary", &c.admin_con_binary)?;
        dirs.insert(binaries.clone());
        dirs.insert(units.clone());
        files.push(root_file(
            binaries.join("gate-con"),
            "0755",
            Content::Program(gate_bin, gate_sum),
        ));
        files.push(root_file(
            binaries.join("admin-con"),
            "0755",
            Content::Program(admin_bin, admin_sum),
        ));
        let gate = gate_unit(&c.gate_account, &loc.binaries, &loc.config_root)?;
        let admin = admin_unit(
            &c.admin_account,
            &loc.binaries,
            &loc.config_root,
            &c.admin_path,
        )?;
        files.push(root_file(
            units.join(GATE_UNIT),
            "0644",
            Content::Staged(gate.into_bytes()),
        ));
        files.push(root_file(
            units.join(ADMIN_UNIT),
            "0644",
            Content::Staged(admin.into_bytes()),
        ));
        let connectors = config_root.join("connectors");
        dirs.insert(config_root.clone());
        dirs.insert(connectors.clone());

        let mut names = BTreeSet::new();
        for agent in &params.agents {
            crate::link::verbs::well_formed(&agent.r#box, &agent.name)?;
            if !names.insert(agent.name.clone()) {
                bail!("the agent {} is named twice", agent.name);
            }
            let n = &agent.name;
            let member = |m: &str| format!("agent {n}'s {m}");
            for (m, v) in [
                ("gate_socket", &agent.gate_socket),
                ("trace_socket", &agent.trace_socket),
                ("weaver_admin", &agent.weaver_admin),
                ("admin_log", &agent.admin_log),
            ] {
                path_ok(&member(m), v)?;
            }
            let from = handover.join(&agent.r#box).join(n);
            let gate_text = handover_file(&from.join("gate-con.toml"), euid)?;
            let admin_text = handover_file(&from.join("admin-con.toml"), euid)?;

            let mut gate_facts = toml::Table::new();
            gate_facts.insert("gate_socket".into(), agent.gate_socket.clone().into());
            let mut admin_facts = toml::Table::new();
            admin_facts.insert("trace_socket".into(), agent.trace_socket.clone().into());
            admin_facts.insert("weaver_admin".into(), agent.weaver_admin.clone().into());
            admin_facts.insert("admin_log".into(), agent.admin_log.clone().into());
            let grace = agent.stop_grace_secs.unwrap_or(STOP_GRACE.as_secs());
            for (m, v) in [
                ("verb_bound_secs", agent.verb_bound_secs),
                ("stop_grace_secs", agent.stop_grace_secs),
            ] {
                if let Some(v) = v {
                    let v =
                        i64::try_from(v).with_context(|| format!("{} is too large", member(m)))?;
                    admin_facts.insert(m.into(), v.into());
                }
            }

            let dir = connectors.join(n);
            dirs.insert(dir.clone());
            files.push(PlannedFile {
                destination: dir.join("gate-con.toml"),
                owner: c.gate_account.clone(),
                mode: "0600",
                content: Content::Staged(compose(&gate_text, gate_facts).into_bytes()),
                config: Some((Plane::Gate, n.clone())),
            });
            files.push(PlannedFile {
                destination: dir.join("admin-con.toml"),
                owner: c.admin_account.clone(),
                mode: "0600",
                content: Content::Staged(compose(&admin_text, admin_facts).into_bytes()),
                config: Some((Plane::Admin, n.clone())),
            });
            let drop_in = units.join(format!("weaver-web-admin-con@{n}.service.d"));
            dirs.insert(drop_in.clone());
            files.push(root_file(
                drop_in.join("stop-timeout.conf"),
                "0644",
                Content::Staged(stop_timeout_drop_in(grace)?.into_bytes()),
            ));
        }
    }

    // No file's destination is a directory's, or lies above another
    // destination (Spec 8.1): a program is not copied into the staged tree,
    // so nothing there would collide, and the manifest would list a path as
    // both, which no order applies.
    for f in &files {
        let d = &f.destination;
        if dirs.contains(d) {
            bail!(
                "{} is a file's destination and a directory's; the locations overlap",
                d.display()
            );
        }
        if let Some(under) = dirs
            .iter()
            .chain(files.iter().map(|g| &g.destination))
            .find(|other| *other != d && other.starts_with(d))
        {
            bail!(
                "{} is a file's destination and {} lies under it; the locations overlap",
                d.display(),
                under.display()
            );
        }
    }

    // Parents first: shallower directories before deeper ones, every file
    // after every directory.
    let mut dirs: Vec<PathBuf> = dirs.into_iter().collect();
    dirs.sort_by_key(|d| (d.components().count(), d.clone()));
    let dirs = dirs
        .into_iter()
        .map(|d| Entry {
            kind: Kind::Dir,
            source: PathBuf::new(),
            destination: d,
            owner: "root".to_owned(),
            mode: "0755",
            sha256: None,
        })
        .collect();
    Ok((dirs, files))
}

/// Write the staged tree and the manifest, loading each staged connector
/// config through its own loader before the manifest is written.
fn write(
    out: &Path,
    existed: bool,
    dirs: &[Entry],
    files: &[PlannedFile],
    held: &mut Held,
) -> anyhow::Result<Staged> {
    // Each make is exclusive, never recursive: where another run made it
    // first, this run holds nothing there and is refused.
    if !existed {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(out)
            .with_context(|| format!("{} cannot be made", out.display()))?;
        *held = Held::Out;
    }
    let tree = out.join("files");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&tree)
        .with_context(|| format!("{} cannot be made", tree.display()))?;
    *held = if existed {
        Held::Tree
    } else {
        Held::OutAndTree
    };
    let mut entries: Vec<Entry> = dirs.to_vec();
    for file in files {
        let (source, sha256) = match &file.content {
            Content::Program(path, sum) => (path.clone(), sum.clone()),
            Content::Staged(bytes) => {
                let rel = file
                    .destination
                    .strip_prefix("/")
                    .expect("every destination is absolute");
                let staged = tree.join(rel);
                let parent = staged.parent().expect("a staged file has a parent");
                std::fs::DirBuilder::new()
                    .mode(0o700)
                    .recursive(true)
                    .create(parent)
                    .with_context(|| format!("{} cannot be made", parent.display()))?;
                let mut f = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&staged)
                    .with_context(|| format!("{} cannot be written", staged.display()))?;
                f.write_all(bytes)?;
                f.sync_all()?;
                if let Some((plane, name)) = &file.config {
                    let agent = match plane {
                        Plane::Gate => GateConConfig::load(&staged).map(|c| c.link.agent),
                        Plane::Admin => AdminConConfig::load(&staged).map(|c| c.link.agent),
                    }
                    .with_context(|| {
                        format!(
                            "the staged config for {} does not load as its connector's",
                            file.destination.display()
                        )
                    })?;
                    if &agent != name {
                        bail!(
                            "the handover config staged for {} names the agent {agent:?}, not {name:?}",
                            file.destination.display()
                        );
                    }
                }
                (staged, sha256_hex(bytes))
            }
        };
        entries.push(Entry {
            kind: Kind::File,
            source,
            destination: file.destination.clone(),
            owner: file.owner.clone(),
            mode: file.mode,
            sha256: Some(sha256),
        });
    }
    let mut text = String::from(MANIFEST_HEADER);
    text.push('\n');
    for e in &entries {
        text.push_str(&e.line());
        text.push('\n');
    }
    let manifest = out.join(MANIFEST);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&manifest)
        .with_context(|| format!("{} cannot be written", manifest.display()))?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(Staged {
        manifest,
        lines: entries.len(),
    })
}

/// **Remove only what this run made** (Spec 8.1), so `<out>` is left as
/// this run found it and nothing of another run's goes with it: `<out>`
/// whole where this run made it and its `files/`; the `files/` and the
/// manifest where `<out>` stood; `<out>` alone, and only while empty, where
/// another run made its `files/` first; nothing where this run made nothing.
fn clean(out: &Path, held: Held) {
    match held {
        Held::Nothing => {}
        Held::Out => {
            let _ = std::fs::remove_dir(out);
        }
        Held::Tree => {
            let _ = std::fs::remove_dir_all(out.join("files"));
            let _ = std::fs::remove_file(out.join(MANIFEST));
        }
        Held::OutAndTree => {
            let _ = std::fs::remove_dir_all(out);
        }
    }
}
