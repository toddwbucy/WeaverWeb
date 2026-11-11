//! conforms: web-the-install-is-staged-unprivileged-from-named-parameters
//!
//! The staging tool (Spec 8.1) against invented parameters in a temporary
//! directory: every location, account and box fact here is made up, and
//! nothing is placed anywhere but under the test's own directory.

use crate::link::admin_con::{AdminConConfig, STOP_GRACE, VERB_BOUND};
use crate::link::gate_con::GateConConfig;
use crate::stage::{self, MANIFEST_HEADER};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn euid() -> u32 {
    // SAFETY: geteuid takes nothing and cannot fail.
    unsafe { libc::geteuid() }
}

/// A connector config shaped as `register` writes one (`verbs::client_config`):
/// the flat link members, then the box facts as commented placeholders. The
/// certificate and key members are invented text, not PEM, since loading a
/// config reads them as strings and never parses them.
fn handover(agent: &str, plane: &str) -> String {
    let mut t = toml::Table::new();
    t.insert("server".into(), "server.invalid:1".into());
    t.insert("server_name".into(), "weaver-web".into());
    t.insert("agent".into(), agent.into());
    t.insert("agent_id".into(), "ag-0123456789abcdef".into());
    t.insert("plane".into(), plane.into());
    t.insert(
        "server_certificate".into(),
        "invented, not a certificate".into(),
    );
    t.insert("certificate".into(), "invented, not a certificate".into());
    t.insert("key".into(), "invented, not a key".into());
    format!(
        "# Written by `weaver-web register` on 2026-11-11. Carries this connector's key:\n# keep it on the agent's box and never in a repository.\n{}\n# The box's facts, added on the box by the install:\n# gate_socket = \"<the agent's gate socket>\"\n",
        toml::to_string(&t).unwrap()
    )
}

fn private_file(path: &Path, text: &str) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

fn program(path: &Path) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o755)
        .open(path)
        .unwrap();
    f.write_all(b"#!/bin/sh\nexit 0\n").unwrap();
}

/// The temporary directory: three stand-in programs, `register`'s handover
/// for two agents on one box, and a parameter file naming them.
struct Rig {
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let rig = Rig { dir };
        std::fs::create_dir(rig.p("bin")).unwrap();
        for p in ["weaver-web", "gate-con", "admin-con"] {
            program(&rig.p(&format!("bin/{p}")));
        }
        for agent in ["a1", "a2"] {
            let d = rig.p(&format!("handover/box1/{agent}"));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .recursive(true)
                .create(&d)
                .unwrap();
            private_file(&d.join("gate-con.toml"), &handover(agent, "gate"));
            private_file(&d.join("admin-con.toml"), &handover(agent, "admin"));
        }
        rig
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    fn s(&self, rel: &str) -> String {
        self.p(rel).display().to_string()
    }

    fn out(&self) -> PathBuf {
        self.p("staged")
    }

    fn locations(&self) -> String {
        format!(
            "[locations]\nbinaries = \"{}\"\nunits = \"{}\"\nconfig_root = \"{}\"\n",
            self.s("bin"),
            self.s("units"),
            self.s("config")
        )
    }

    fn server(&self) -> String {
        format!(
            "\n[server]\naccount = \"web-acct\"\nbinary = \"{}\"\n",
            self.s("bin/weaver-web")
        )
    }

    fn connectors(&self) -> String {
        format!(
            "\n[connectors]\ngate_account = \"gate-acct\"\nadmin_account = \"admin-acct\"\nadmin_path = \"{}\"\ngate_con_binary = \"{}\"\nadmin_con_binary = \"{}\"\nhandover = \"{}\"\n",
            self.admin_path(),
            self.s("bin/gate-con"),
            self.s("bin/admin-con"),
            self.s("handover")
        )
    }

    fn admin_path(&self) -> String {
        format!("{}:{}", self.s("path1"), self.s("path2"))
    }

    fn agent(&self, name: &str, extra: &str) -> String {
        format!(
            "\n[[agent]]\nbox = \"box1\"\nname = \"{name}\"\ngate_socket = \"{}\"\ntrace_socket = \"{}\"\nweaver_admin = \"{}\"\nadmin_log = \"{}\"\n{extra}",
            self.s(&format!("box/{name}/gate.sock")),
            self.s(&format!("box/{name}/trace.sock")),
            self.s("box/admin-program"),
            self.s(&format!("box/{name}/admin.log")),
        )
    }

    /// The whole parameter file: the server, the connectors, agent a1 on
    /// the defaults and a2 with its own grace and verb bound.
    fn params(&self) -> String {
        format!(
            "{}{}{}{}{}",
            self.locations(),
            self.server(),
            self.connectors(),
            self.agent("a1", ""),
            self.agent("a2", "stop_grace_secs = 1500\nverb_bound_secs = 1200\n"),
        )
    }

    fn stage(&self, params: &str) -> anyhow::Result<stage::Staged> {
        stage::stage(params, &self.out(), euid())
    }

    /// The staged copy of a destination.
    fn staged(&self, destination: &str) -> PathBuf {
        self.out()
            .join("files")
            .join(destination.trim_start_matches('/'))
    }

    fn read_staged(&self, destination: &str) -> String {
        std::fs::read_to_string(self.staged(destination)).unwrap()
    }
}

fn has_line(text: &str, line: &str) -> bool {
    text.lines().any(|l| l == line)
}

#[test]
fn a_render_stages_every_unit_and_config_with_the_properties_spec_8_1_names() {
    let rig = Rig::new();
    let staged = rig.stage(&rig.params()).expect("stages");
    assert_eq!(staged.lines, 20, "8 directories and 12 files");
    let (bin, cfg, units) = (rig.s("bin"), rig.s("config"), rig.s("units"));

    let server = rig.read_staged(&format!("{units}/weaver-web.service"));
    let gate = rig.read_staged(&format!("{units}/weaver-web-gate-con@.service"));
    let admin = rig.read_staged(&format!("{units}/weaver-web-admin-con@.service"));
    for (unit, account) in [
        (&server, "web-acct"),
        (&gate, "gate-acct"),
        (&admin, "admin-acct"),
    ] {
        for line in [
            &format!("User={account}"),
            "UMask=0077",
            "Wants=network-online.target",
            "After=network-online.target",
            "Restart=on-failure",
            "RestartSec=5",
            "WantedBy=multi-user.target",
        ] {
            assert!(has_line(unit, line), "{line} missing from:\n{unit}");
        }
        assert!(!unit.lines().any(|l| l.starts_with("Group=")), "no Group=");
        assert!(
            unit.lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .all(|l| !l.contains('@')),
            "a placeholder left in:\n{unit}"
        );
    }
    assert!(has_line(
        &server,
        &format!("ExecStart={bin}/weaver-web --config {cfg}/config.toml serve")
    ));
    assert!(has_line(
        &gate,
        &format!("ExecStart={bin}/gate-con --config {cfg}/connectors/%i/gate-con.toml")
    ));
    assert!(has_line(
        &admin,
        &format!("ExecStart={bin}/admin-con --config {cfg}/connectors/%i/admin-con.toml")
    ));
    assert!(has_line(&admin, "KillMode=mixed"));
    assert!(has_line(&admin, "TimeoutStopSec=1140"));
    assert!(has_line(
        &admin,
        &format!("Environment=PATH={}", rig.admin_path())
    ));
    assert!(
        !has_line(&gate, "KillMode=mixed"),
        "gate-con keeps the default kill mode"
    );

    let a1 = rig.read_staged(&format!(
        "{units}/weaver-web-admin-con@a1.service.d/stop-timeout.conf"
    ));
    let a2 = rig.read_staged(&format!(
        "{units}/weaver-web-admin-con@a2.service.d/stop-timeout.conf"
    ));
    assert!(has_line(&a1, "[Service]") && has_line(&a1, "TimeoutStopSec=1140"));
    assert!(has_line(&a2, "[Service]") && has_line(&a2, "TimeoutStopSec=1560"));
}

#[test]
fn admin_cons_unit_sets_nothing_the_contract_forbids() {
    let rig = Rig::new();
    rig.stage(&rig.params()).expect("stages");
    let units = rig.s("units");
    for unit in [
        "weaver-web-admin-con@.service",
        "weaver-web-gate-con@.service",
        "weaver-web.service",
    ] {
        let text = rig.read_staged(&format!("{units}/{unit}"));
        assert_eq!(stage::forbidden_in(&text), vec![], "{unit}");
    }
    // The reader itself: one plant of each of Spec 8.1's five groups is
    // found, and a commented one is not.
    let admin = rig.read_staged(&format!("{units}/weaver-web-admin-con@.service"));
    for (plant, clause) in [
        ("NoNewPrivileges=yes", "no new privileges"),
        ("RestrictAddressFamilies=AF_INET", "no new privileges"),
        (
            "CapabilityBoundingSet=CAP_NET_RAW",
            "a capability bounding set narrower than root's",
        ),
        ("SystemCallLog=@privileged", "a system call filter"),
        ("ProtectSystem=strict", "a private or read-only view"),
        ("LimitNOFILE=1024", "a resource limit"),
    ] {
        let planted = admin.replace("[Service]\n", &format!("[Service]\n{plant}\n"));
        let found = stage::forbidden_in(&planted);
        assert_eq!(found.len(), 1, "{plant}: {found:?}");
        assert_eq!(found[0].1, clause, "{plant}");
        let commented = admin.replace("[Service]\n", &format!("[Service]\n# {plant}\n"));
        assert_eq!(stage::forbidden_in(&commented), vec![], "# {plant}");
    }
}

#[test]
fn the_units_pass_the_service_managers_verifier() {
    match std::process::Command::new("systemd-analyze")
        .arg("--version")
        .output()
    {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipped: systemd-analyze is not installed, so the units were not verified");
            return;
        }
        Err(e) => panic!("systemd-analyze did not run: {e}"),
        Ok(_) => {}
    }
    let rig = Rig::new();
    rig.stage(&rig.params()).expect("stages");
    let units = rig.s("units");
    let check = tempfile::tempdir().unwrap();
    let mut files = Vec::new();
    for (staged, as_named) in [
        ("weaver-web.service", "weaver-web.service"),
        (
            "weaver-web-gate-con@.service",
            "weaver-web-gate-con@a1.service",
        ),
        (
            "weaver-web-admin-con@.service",
            "weaver-web-admin-con@a1.service",
        ),
    ] {
        let to = check.path().join(as_named);
        std::fs::copy(rig.staged(&format!("{units}/{staged}")), &to).unwrap();
        files.push(to);
    }
    let out = std::process::Command::new("systemd-analyze")
        .arg("verify")
        .arg("--man=no")
        .args(&files)
        .output()
        .unwrap();
    // The verifier warns and still exits 0 on an unknown key or a value it
    // cannot parse (measured 2026-11-11), so its error stream must be empty.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && stderr.trim().is_empty(),
        "status {:?}, stderr:\n{stderr}",
        out.status
    );
}

#[test]
fn a_staged_config_is_the_handover_and_the_box_facts_and_loads() {
    let rig = Rig::new();
    rig.stage(&rig.params()).expect("stages");
    let cfg = rig.s("config");
    for agent in ["a1", "a2"] {
        let gate_path = rig.staged(&format!("{cfg}/connectors/{agent}/gate-con.toml"));
        let admin_path = rig.staged(&format!("{cfg}/connectors/{agent}/admin-con.toml"));
        let gate_text = std::fs::read_to_string(&gate_path).unwrap();
        let admin_text = std::fs::read_to_string(&admin_path).unwrap();
        assert!(
            gate_text.starts_with(&handover(agent, "gate")),
            "the handover, unchanged"
        );
        assert!(
            admin_text.starts_with(&handover(agent, "admin")),
            "the handover, unchanged"
        );

        let gate = GateConConfig::load(&gate_path).expect("gate-con loads its staged config");
        assert_eq!(gate.link.agent, agent);
        assert_eq!(gate.gate_socket, rig.p(&format!("box/{agent}/gate.sock")));
        let admin = AdminConConfig::load(&admin_path).expect("admin-con loads its staged config");
        assert_eq!(admin.link.agent, agent);
        assert_eq!(
            admin.trace_socket,
            rig.p(&format!("box/{agent}/trace.sock"))
        );
        assert_eq!(admin.weaver_admin, rig.p("box/admin-program"));
        assert_eq!(admin.admin_log, rig.p(&format!("box/{agent}/admin.log")));
        let (grace, bound) = if agent == "a2" {
            (Duration::from_secs(1500), Duration::from_secs(1200))
        } else {
            (STOP_GRACE, VERB_BOUND)
        };
        assert_eq!(admin.stop_grace, grace);
        assert_eq!(admin.verb_bound, bound);
    }
}

#[test]
fn a_handover_of_the_other_plane_or_another_agent_is_refused_and_writes_nothing() {
    // a1's two files swapped: its gate file is minted for the admin plane.
    let rig = Rig::new();
    let d = rig.p("handover/box1/a1");
    std::fs::rename(d.join("gate-con.toml"), d.join("swap")).unwrap();
    std::fs::rename(d.join("admin-con.toml"), d.join("gate-con.toml")).unwrap();
    std::fs::rename(d.join("swap"), d.join("admin-con.toml")).unwrap();
    let e = rig.stage(&rig.params()).expect_err("refused");
    assert!(format!("{e:#}").contains("does not load"), "{e:#}");
    assert!(!rig.out().exists(), "a refusal leaves no directory");

    // a1's gate file names another agent.
    let rig = Rig::new();
    let f = rig.p("handover/box1/a1/gate-con.toml");
    std::fs::remove_file(&f).unwrap();
    private_file(&f, &handover("zz", "gate"));
    let e = rig.stage(&rig.params()).expect_err("refused");
    assert!(format!("{e:#}").contains("names the agent \"zz\""), "{e:#}");
    assert!(!rig.out().exists(), "a refusal leaves no directory");
}

#[test]
fn every_parameter_out_of_shape_is_refused_and_writes_nothing() {
    let rig = Rig::new();
    let full = rig.params();
    let no_agents = format!("{}{}{}", rig.locations(), rig.server(), rig.connectors());
    let cases: Vec<(&str, String, &str)> = vec![
        (
            "a missing member",
            full.replace(&format!("units = \"{}\"\n", rig.s("units")), ""),
            "missing field `units`",
        ),
        (
            "an unknown member",
            full.replace("[locations]\n", "[locations]\nbin = \"/x\"\n"),
            "unknown field `bin`",
        ),
        (
            "a relative path",
            full.replace(&rig.s("config"), "relative/config"),
            "is not an absolute path",
        ),
        (
            "a .. component",
            full.replace(&rig.s("config"), &format!("{}/x/../config", rig.s(""))),
            "empty, . or .. component",
        ),
        (
            "a trailing separator",
            full.replace(
                &format!("\"{}\"", rig.s("config")),
                &format!("\"{}/\"", rig.s("config")),
            ),
            "ends with a separator",
        ),
        (
            "a character the unit syntax reads",
            full.replace(&rig.s("config"), &format!("{} x", rig.s("config"))),
            "which a unit or a PATH list would read",
        ),
        (
            "a relative element of admin-con's PATH",
            full.replace(&rig.admin_path(), &format!("{}:relative", rig.s("path1"))),
            "an element of connectors.admin_path",
        ),
        (
            "an account out of its grammar",
            full.replace("gate_account = \"gate-acct\"", "gate_account = \"Gate\""),
            "is not a portable user name",
        ),
        (
            "a name out of its grammar",
            full.replace("name = \"a2\"", "name = \"a/2\""),
            "is not a box or a name",
        ),
        (
            "a name twice",
            full.replace("name = \"a2\"", "name = \"a1\""),
            "is named twice",
        ),
        (
            "an agent without [connectors]",
            full.replace(&rig.connectors(), ""),
            "[connectors] is missing",
        ),
        (
            "[connectors] without an agent",
            no_agents.clone(),
            "no [[agent]] is",
        ),
        ("nothing to stage", rig.locations(), "stages nothing"),
        (
            "a program that is a directory",
            full.replace(&rig.s("bin/weaver-web"), &rig.s("bin")),
            "is not a regular file",
        ),
        (
            "a location that is a program's destination",
            full.replace(
                &format!("units = \"{}\"", rig.s("units")),
                &format!("units = \"{}\"", rig.s("bin/weaver-web")),
            ),
            "is a file's destination and a directory's",
        ),
        (
            "a location under a program's destination",
            full.replace(
                &format!("config_root = \"{}\"", rig.s("config")),
                &format!("config_root = \"{}\"", rig.s("bin/admin-con/config")),
            ),
            "lies under it",
        ),
        (
            "a missing handover",
            full.replacen("box = \"box1\"", "box = \"box9\"", 1),
            "is missing",
        ),
    ];
    for (case, params, expect) in cases {
        let e = rig.stage(&params).expect_err(case);
        assert!(format!("{e:#}").contains(expect), "{case}: {e:#}");
        assert!(!rig.out().exists(), "{case}: a refusal leaves no directory");
    }

    // A program that is a symlink.
    std::os::unix::fs::symlink(rig.p("bin/gate-con"), rig.p("gate-link")).unwrap();
    let e = rig
        .stage(&full.replace(&rig.s("bin/gate-con"), &rig.s("gate-link")))
        .expect_err("a symlink");
    assert!(
        format!("{e:#}").contains("connectors.gate_con_binary")
            && format!("{e:#}").contains("is a symlink"),
        "{e:#}"
    );
    assert!(!rig.out().exists());

    // A handover file that is a symlink.
    let f = rig.p("handover/box1/a2/gate-con.toml");
    std::fs::rename(&f, rig.p("a2-gate")).unwrap();
    std::os::unix::fs::symlink(rig.p("a2-gate"), &f).unwrap();
    let e = rig.stage(&full).expect_err("a symlinked handover");
    assert!(
        format!("{e:#}").contains("the handover file") && format!("{e:#}").contains("is a symlink"),
        "{e:#}"
    );
    assert!(!rig.out().exists());
    std::fs::remove_file(&f).unwrap();

    // A handover file that is a directory.
    std::fs::create_dir(&f).unwrap();
    let e = rig
        .stage(&full)
        .expect_err("a handover that is a directory");
    assert!(
        format!("{e:#}").contains("the handover file")
            && format!("{e:#}").contains("is not a regular file"),
        "{e:#}"
    );
    assert!(!rig.out().exists());
    std::fs::remove_dir(&f).unwrap();
    std::fs::rename(rig.p("a2-gate"), &f).unwrap();

    // A handover file open to others: refused, naming the mode found.
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o644)).unwrap();
    let e = rig.stage(&full).expect_err("a handover open to others");
    assert!(
        format!("{e:#}").contains("the handover file")
            && format!("{e:#}").contains("has mode 0644"),
        "{e:#}"
    );
    assert!(!rig.out().exists());
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600)).unwrap();

    // A handover file that is not the operator's own: the tool is handed a
    // uid other than the file's owner, with <out> absent.
    let e = stage::stage(&full, &rig.out(), euid() + 1).expect_err("another's handover");
    assert!(
        format!("{e:#}").contains("the handover file")
            && format!("{e:#}").contains("is owned by uid"),
        "{e:#}"
    );
    assert!(!rig.out().exists());

    // An <out> that is not empty, and one open to others: each left as found.
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(rig.out())
        .unwrap();
    std::fs::write(rig.out().join("left"), b"x").unwrap();
    let e = rig.stage(&full).expect_err("not empty");
    assert!(format!("{e:#}").contains("is not empty"), "{e:#}");
    let left: Vec<_> = std::fs::read_dir(rig.out()).unwrap().collect();
    assert_eq!(left.len(), 1, "nothing written beside what stood");
    std::fs::remove_file(rig.out().join("left")).unwrap();
    std::fs::set_permissions(rig.out(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let e = rig.stage(&full).expect_err("open to others");
    assert!(format!("{e:#}").contains("open to others"), "{e:#}");
    assert_eq!(std::fs::read_dir(rig.out()).unwrap().count(), 0);

    // An <out> that is not the operator's own: the tool is handed a uid that
    // is not the directory's owner.
    std::fs::set_permissions(rig.out(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let e = stage::stage(&full, &rig.out(), euid() + 1).expect_err("not the operator's own");
    assert!(
        format!("{e:#}").contains("is not the operator's own directory"),
        "{e:#}"
    );
    assert_eq!(std::fs::read_dir(rig.out()).unwrap().count(), 0);

    // An empty private <out> stands and is used, and a refusal empties it again.
    let f = rig.p("handover/box1/a2/admin-con.toml");
    std::fs::remove_file(&f).unwrap();
    private_file(&f, &handover("a2", "gate"));
    let e = rig
        .stage(&full)
        .expect_err("a2's admin file is the gate plane's");
    assert!(format!("{e:#}").contains("does not load"), "{e:#}");
    assert!(rig.out().exists(), "the directory that stood is kept");
    assert_eq!(
        std::fs::read_dir(rig.out()).unwrap().count(),
        0,
        "and emptied"
    );
}

#[test]
fn the_manifest_names_every_directory_and_file_parents_first() {
    use sha2::{Digest, Sha256};
    let rig = Rig::new();
    let staged = rig.stage(&rig.params()).expect("stages");
    let text = std::fs::read_to_string(&staged.manifest).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(MANIFEST_HEADER));
    let (bin, cfg, units) = (rig.s("bin"), rig.s("config"), rig.s("units"));
    let locations = [bin.clone(), units.clone(), cfg.clone()];
    let mut dirs: Vec<String> = Vec::new();
    let mut files = 0;
    for line in lines {
        let f: Vec<&str> = line.split('\t').collect();
        assert_eq!(f.len(), 7, "{line}");
        let (kind, source, dest, owner, group, mode, sum) =
            (f[0], f[1], f[2], f[3], f[4], f[5], f[6]);
        assert_eq!(group, "-", "{line}");
        let parent = Path::new(dest).parent().unwrap().display().to_string();
        match kind {
            "dir" => {
                assert_eq!(
                    (source, owner, mode, sum),
                    ("-", "root", "0755", "-"),
                    "{line}"
                );
                assert!(
                    locations.contains(&dest.to_owned()) || dirs.contains(&parent),
                    "{dest} before its parent"
                );
                dirs.push(dest.to_owned());
            }
            "file" => {
                files += 1;
                assert!(dirs.contains(&parent), "{dest} before its directory");
                let bytes = std::fs::read(source).unwrap();
                let hex: String = Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect();
                assert_eq!(sum, hex, "{line}");
                let expect = if dest.ends_with("/gate-con.toml") {
                    ("gate-acct", "0600")
                } else if dest.ends_with("/admin-con.toml") {
                    ("admin-acct", "0600")
                } else if parent == bin {
                    ("root", "0755")
                } else {
                    ("root", "0644")
                };
                assert_eq!((owner, mode), expect, "{line}");
                if parent == bin {
                    assert_eq!(
                        Path::new(source).parent().unwrap(),
                        rig.p("bin"),
                        "a program's source is the built binary"
                    );
                } else {
                    assert!(
                        source.starts_with(&rig.out().join("files").display().to_string()),
                        "{line}"
                    );
                }
            }
            other => panic!("kind {other}"),
        }
    }
    assert_eq!(dirs.len(), 8);
    assert_eq!(files, 12);
    for d in [
        format!("{cfg}/connectors"),
        format!("{cfg}/connectors/a1"),
        format!("{units}/weaver-web-admin-con@a2.service.d"),
    ] {
        assert!(dirs.contains(&d), "{d}");
    }
}

/// Every staged file under `<out>`, with its bytes, and the manifest's text.
fn snapshot(out: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut stack = vec![out.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_second_run_on_the_same_out_is_refused_and_removes_nothing_of_the_first() {
    // Two runs whose checks of <out> both passed before either wrote: the
    // first stages, then the second comes to write. Once with <out> absent at
    // both checks, once with <out> standing empty.
    for stood in [false, true] {
        let rig = Rig::new();
        if stood {
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(rig.out())
                .unwrap();
        }
        let params: stage::Params = toml::from_str(&rig.params()).unwrap();
        rig.stage(&rig.params()).expect("the first run stages");
        let before = snapshot(&rig.out());
        assert!(before.len() > 1, "stood {stood}: the first run's files");
        let e = stage::stage_checked(&params, &rig.out(), stood, euid())
            .expect_err("the second run is refused");
        assert!(
            format!("{e:#}").contains("cannot be made"),
            "stood {stood}: {e:#}"
        );
        assert_eq!(
            snapshot(&rig.out()),
            before,
            "stood {stood}: the first run's staged files and manifest stand, unchanged"
        );
    }
}

#[test]
fn a_relative_out_is_taken_against_the_working_directory_lexically() {
    let cwd = Path::new("/w");
    for (out, expect) in [
        ("./staged", "/w/staged"),
        ("staged/", "/w/staged"),
        ("a/./b//c/", "/w/a/b/c"),
        ("/x/./y/", "/x/y"),
    ] {
        assert_eq!(
            stage::out_path(cwd, Path::new(out)).display().to_string(),
            expect,
            "{out}"
        );
    }
    // A `..` is kept, so the path rule refuses it as in any path.
    let up = stage::out_path(cwd, Path::new("../x"));
    assert_eq!(up.display().to_string(), "/w/../x");
    assert!(stage::path_ok("--out", &up.display().to_string()).is_err());
    // A relative <out> taken so stages.
    let rig = Rig::new();
    let out = stage::out_path(rig.dir.path(), Path::new("./staged/"));
    assert_eq!(out.display().to_string(), rig.out().display().to_string());
    stage::stage(&rig.params(), &out, euid()).expect("stages");
}

#[test]
fn a_template_parameter_the_tool_does_not_fill_is_refused() {
    let e = stage::render(
        "[Service]\nUser=@ACCOUNT@\nExecStart=@BINARIES@/x\n",
        &[("ACCOUNT", "a")],
    )
    .expect_err("BINARIES is not filled");
    assert!(format!("{e:#}").contains("@BINARIES@"), "{e:#}");
    // A comment may name a placeholder, and an @ before a lowercase word is
    // not one.
    let rendered = stage::render(
        "# each @NAME@ is a parameter\nUser=@ACCOUNT@\nDescription=x@host\n",
        &[("ACCOUNT", "a")],
    )
    .expect("renders");
    assert!(has_line(&rendered, "User=a"), "{rendered}");
}

#[test]
fn the_tool_refuses_to_run_as_root() {
    let rig = Rig::new();
    let e = stage::stage(&rig.params(), &rig.out(), 0).expect_err("root");
    assert!(format!("{e:#}").contains("never as root"), "{e:#}");
    assert!(!rig.out().exists());
}
