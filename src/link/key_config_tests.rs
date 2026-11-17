//! **`request` and `adopt` on the box** (Spec 8.2, and the brief's decision 5 as
//! amended on 2026-11-17): the key-only config, its prefix found by parsing,
//! the append, a crash at each write boundary, and the checks before a byte is
//! written.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::link::authority::Authority;
use crate::link::frames::Plane;
use crate::link::key_config::{
    self, AdoptRefused, AnswerFile, Boundary, KEY_MISMATCH, NOT_SIGNED, RequestRefused, prefix_of,
};

struct Bench {
    dir: tempfile::TempDir,
    authority: Authority,
}

fn bench() -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
    Bench { dir, authority }
}

impl Bench {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// `request`'s config for `plane`, and the key in it.
    fn requested(&self, plane: Plane) -> (PathBuf, String) {
        let config = self.path(&format!("{}.toml", plane.as_str()));
        key_config::request(plane, &config).unwrap();
        let content = std::fs::read_to_string(&config).unwrap();
        let (key_pem, _) = prefix_of(plane, &content).unwrap();
        (config, key_pem)
    }

    /// An answer file whose certificate for each plane is over the key the
    /// caller names, signed by `by`.
    fn answer(&self, by: &Authority, gate_key: &str, admin_key: &str) -> PathBuf {
        let sign = |plane, key_pem: &str| {
            let key = rcgen::KeyPair::from_pem(key_pem).unwrap();
            by.sign_client("karl", plane, &key).unwrap().certificate_pem
        };
        let path = self.path(&format!("answer-{}.json", rand_suffix()));
        AnswerFile {
            server: "console.test:7443".into(),
            server_name: "weaver-web".into(),
            agent: "karl".into(),
            agent_id: "ag-0123456789abcdef".into(),
            r#box: "box-a".into(),
            authority: by.certificate_pem().to_owned(),
            gate: sign(Plane::Gate, gate_key),
            admin: sign(Plane::Admin, admin_key),
        }
        .write_new(&path)
        .unwrap();
        path
    }
}

fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed).to_string()
}

fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

/// **`request` creates the config new, owner-only, holding the key alone**,
/// and refuses a path that holds a file or a link, writing nothing.
#[test]
fn request_creates_a_key_only_config_and_refuses_an_existing_path() {
    let b = bench();
    let config = b.path("gate.toml");
    let request = key_config::request(Plane::Gate, &config).unwrap();
    assert!(request.starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
    let mode = std::fs::metadata(&config).unwrap().permissions().mode() & 0o7777;
    assert_eq!(mode, 0o600);
    let content = std::fs::read_to_string(&config).unwrap();
    let (key_pem, length) = prefix_of(Plane::Gate, &content).unwrap();
    assert_eq!(length, content.len(), "the config holds the key alone");
    assert_eq!(content, key_config::key_only(Plane::Gate, &key_pem));
    // The request is over the key in the config.
    let key = rcgen::KeyPair::from_pem(&key_pem).unwrap();
    let der = rustls_pemfile::csr(&mut request.as_bytes())
        .unwrap()
        .unwrap();
    let point = key.public_key_raw();
    assert!(der.as_ref().windows(point.len()).any(|w| w == point));

    let before = bytes(&config);
    assert!(matches!(
        key_config::request(Plane::Gate, &config),
        Err(RequestRefused::Exists(_))
    ));
    assert_eq!(bytes(&config), before);
    let link = b.path("link.toml");
    std::os::unix::fs::symlink(b.path("elsewhere.toml"), &link).unwrap();
    assert!(matches!(
        key_config::request(Plane::Gate, &link),
        Err(RequestRefused::Exists(_))
    ));
    assert!(
        !b.path("elsewhere.toml").exists(),
        "a link is never followed"
    );
}

/// **`adopt` completes the config in place**: the key's bytes stand where
/// `request` wrote them, the members `register` writes follow, and the
/// connector's config parses with its certificate over its key.
#[test]
fn adopt_appends_the_members_register_writes_after_the_key() {
    let b = bench();
    let (gate, gate_key) = b.requested(Plane::Gate);
    let (admin, admin_key) = b.requested(Plane::Admin);
    let answer = b.answer(&b.authority, &gate_key, &admin_key);
    for (config, plane) in [(&gate, Plane::Gate), (&admin, Plane::Admin)] {
        let before = bytes(config);
        let adopted = key_config::adopt(plane, config, &answer).unwrap();
        assert_eq!(adopted.agent_id, "ag-0123456789abcdef");
        let after = bytes(config);
        assert!(after.starts_with(&before), "the key's bytes moved");
        let content = String::from_utf8(after).unwrap();
        let link: crate::link::client::LinkConfig = toml::from_str(&content).unwrap();
        assert_eq!(link.plane, plane);
        assert_eq!(
            (
                link.server.as_str(),
                link.agent.as_str(),
                link.agent_id.as_str()
            ),
            ("console.test:7443", "karl", "ag-0123456789abcdef")
        );
        assert_eq!(link.server_name, "weaver-web");
        assert_eq!(link.server_certificate, b.authority.certificate_pem());
        crate::link::authority::client_tls(&link.server_certificate, &link.certificate, &link.key)
            .unwrap();
        let mode = std::fs::metadata(config).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode, 0o600);
    }
}

/// **A crash at each write boundary leaves the key byte for byte, and a
/// re-run completes the config** (the brief's decision 5): after the open,
/// mid-certificate and before the sync. The service refuses each config the
/// crash left, its certificate missing or cut, at its start.
///
/// Perturbation: truncate into the prefix, and the key's bytes move.
#[test]
fn a_crash_at_each_write_boundary_keeps_the_key_and_a_re_run_completes_it() {
    for boundary in [
        Boundary::Opened,
        Boundary::MidCertificate,
        Boundary::BeforeSync,
    ] {
        let b = bench();
        let (gate, gate_key) = b.requested(Plane::Gate);
        let (_, admin_key) = b.requested(Plane::Admin);
        let answer = b.answer(&b.authority, &gate_key, &admin_key);
        let prefix = bytes(&gate);
        let crashed = key_config::adopt_with(Plane::Gate, &gate, &answer, &mut |at| {
            if at == boundary {
                Err(std::io::Error::other("a crash"))
            } else {
                Ok(())
            }
        });
        assert!(
            matches!(crashed, Err(AdoptRefused::Fault(_))),
            "{boundary:?}"
        );
        let left = bytes(&gate);
        assert_eq!(
            &left[..prefix.len()],
            &prefix[..],
            "{boundary:?}: the key's bytes moved"
        );
        if boundary != Boundary::BeforeSync {
            // The service's start refuses what the crash left, before any
            // dial: the box facts added, the certificate is still missing
            // or cut.
            let mut started = left.clone();
            started.extend_from_slice(b"\ngate_socket = \"/a/socket\"\n");
            std::fs::write(b.path("started.toml"), &started).unwrap();
            std::fs::set_permissions(
                b.path("started.toml"),
                std::fs::Permissions::from_mode(0o600),
            )
            .unwrap();
            let refused = crate::link::gate_con::GateConConfig::load(&b.path("started.toml"))
                .map(|_| ())
                .unwrap_err()
                .to_string();
            assert!(
                refused.contains("does not parse"),
                "{boundary:?}: {refused}"
            );
        }
        key_config::adopt(Plane::Gate, &gate, &answer).unwrap();
        let done = bytes(&gate);
        assert_eq!(&done[..prefix.len()], &prefix[..], "{boundary:?}");
        let link: crate::link::client::LinkConfig =
            toml::from_str(std::str::from_utf8(&done).unwrap()).unwrap();
        assert_eq!(link.agent, "karl", "{boundary:?}");
    }
}

/// **A key-only config is refused by the service at its start**, its
/// certificate missing (Spec 8.2).
#[test]
fn the_service_refuses_a_config_with_no_certificate() {
    let b = bench();
    let (gate, _) = b.requested(Plane::Gate);
    let mut started = bytes(&gate);
    started.extend_from_slice(
        b"server = \"console.test:7443\"\nserver_name = \"weaver-web\"\nagent = \"karl\"\n\
          agent_id = \"ag-0123456789abcdef\"\nplane = \"gate\"\nserver_certificate = \"x\"\n\
          gate_socket = \"/a/socket\"\n",
    );
    std::fs::write(&gate, &started).unwrap();
    let refused = crate::link::gate_con::GateConConfig::load(&gate)
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("the member certificate is missing"),
        "{refused}"
    );
}

/// **`adopt` checks before it writes a byte**: a certificate over another
/// key or signed by another authority is refused with its reason, and a
/// config the move left in any other state, or the other plane's, is refused
/// as usage. The config is unchanged each time.
#[test]
fn adopt_refuses_an_answer_or_a_config_not_its_own_and_writes_nothing() {
    let b = bench();
    let (gate, gate_key) = b.requested(Plane::Gate);
    let (admin, admin_key) = b.requested(Plane::Admin);
    let before = bytes(&gate);

    // The answer's gate certificate is over the admin key.
    let crossed = b.answer(&b.authority, &admin_key, &admin_key);
    assert!(matches!(
        key_config::adopt(Plane::Gate, &gate, &crossed),
        Err(AdoptRefused::Refused(KEY_MISMATCH))
    ));
    // The certificate is over the key and signed by another authority.
    let other = Authority::init(&b.path("other"), "weaver-web", &[]).unwrap();
    let mut forged = AnswerFile::read(&b.answer(&other, &gate_key, &admin_key)).unwrap();
    forged.authority = b.authority.certificate_pem().to_owned();
    let forged_path = b.path("forged.json");
    forged.write_new(&forged_path).unwrap();
    assert!(matches!(
        key_config::adopt(Plane::Gate, &gate, &forged_path),
        Err(AdoptRefused::Refused(NOT_SIGNED))
    ));
    assert_eq!(bytes(&gate), before);

    let answer = b.answer(&b.authority, &gate_key, &admin_key);
    // The other plane's config.
    assert!(matches!(
        key_config::adopt(Plane::Gate, &admin, &answer),
        Err(AdoptRefused::Usage(_))
    ));
    // A mode another party could read.
    std::fs::set_permissions(&gate, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(matches!(
        key_config::adopt(Plane::Gate, &gate, &answer),
        Err(AdoptRefused::Usage(_))
    ));
    std::fs::set_permissions(&gate, std::fs::Permissions::from_mode(0o600)).unwrap();
    // A link to the config.
    let link = b.path("link.toml");
    std::os::unix::fs::symlink(&gate, &link).unwrap();
    assert!(matches!(
        key_config::adopt(Plane::Gate, &link, &answer),
        Err(AdoptRefused::Usage(_))
    ));
    // A prefix that is not request's: one byte of the key changed.
    let mut bent = before.clone();
    let at = bent.len() - 40;
    bent[at] = if bent[at] == b'A' { b'B' } else { b'A' };
    std::fs::write(&gate, &bent).unwrap();
    assert!(matches!(
        key_config::adopt(Plane::Gate, &gate, &answer),
        Err(AdoptRefused::Usage(_))
    ));
    assert_eq!(bytes(&gate), bent);
}
