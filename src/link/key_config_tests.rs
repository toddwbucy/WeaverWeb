//! **`request` and `adopt` on the box** (Spec 8.2; the brief of 2026-11-27,
//! child 6): the key laid new in the connector's group at 0640 before its
//! first byte, the request line, the config `adopt` writes naming the key file
//! it never opens, and the checks before a byte is written.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use rustls::pki_types::UnixTime;

use crate::link::authority::Authority;
use crate::link::client::{self, Held, Trust};
use crate::link::frames::Plane;
use crate::link::key_config::{
    self, AdoptRefused, Adopting, AnswerFile, KEY_MISMATCH, NOT_SIGNED, RequestRefused,
};

struct Bench {
    dir: tempfile::TempDir,
    authority: Authority,
}

fn bench() -> Bench {
    let dir = crate::test_dir::tempdir().unwrap();
    let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
    Bench { dir, authority }
}

fn egid() -> libc::gid_t {
    // SAFETY: getegid takes nothing and cannot fail.
    unsafe { libc::getegid() }
}

impl Bench {
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// `request` for `plane`: the key file it laid, the key in it, and the
    /// file holding the request line it printed.
    fn requested(&self, plane: Plane) -> (PathBuf, String, PathBuf) {
        let key_file = self.path(&format!("{}-con.key", plane.as_str()));
        let request = key_config::request(&key_file, egid()).unwrap();
        let line = self.path(&format!("{}-request.json", plane.as_str()));
        std::fs::write(
            &line,
            format!(
                "{}\n",
                serde_json::json!({"event": "request", "request": request})
            ),
        )
        .unwrap();
        let key_pem = std::fs::read_to_string(&key_file).unwrap();
        (key_file, key_pem, line)
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
            gate: Some(sign(Plane::Gate, gate_key)),
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

/// **`request` lays the key new, in the group it is given, at 0640, holding
/// the key alone**, which the key's trust rule then admits; it refuses a path
/// that holds a file or a link, writing nothing; and a group its account may
/// not set leaves no file behind.
#[test]
fn request_lays_the_key_new_in_its_group_at_0640_and_refuses_an_existing_path() {
    let b = bench();
    let key_file = b.path("gate-con.key");
    let request = key_config::request(&key_file, egid()).unwrap();
    assert!(request.starts_with("-----BEGIN CERTIFICATE REQUEST-----"));
    let meta = std::fs::symlink_metadata(&key_file).unwrap();
    assert_eq!(meta.mode() & 0o7777, 0o640);
    assert_eq!(meta.gid(), egid());
    let key_pem = std::fs::read_to_string(&key_file).unwrap();
    assert!(key_config::one_key(&key_pem), "the key alone");
    assert_eq!(
        client::read_trusted(&key_file, Held::Key, &Trust::connector()).unwrap(),
        key_pem,
        "the key's trust rule admits the file request laid"
    );

    let before = std::fs::read(&key_file).unwrap();
    assert!(matches!(
        key_config::request(&key_file, egid()),
        Err(RequestRefused::Exists(_))
    ));
    assert_eq!(std::fs::read(&key_file).unwrap(), before, "unchanged");
    let link = b.path("link.key");
    std::os::unix::fs::symlink(b.path("absent.key"), &link).unwrap();
    assert!(matches!(
        key_config::request(&link, egid()),
        Err(RequestRefused::Exists(_))
    ));
    assert!(
        !b.path("absent.key").exists(),
        "nothing made through a link"
    );

    // Root's group, which an unprivileged account may not give a file: the
    // command's fault, and no file left behind.
    // SAFETY: geteuid takes nothing and cannot fail.
    if unsafe { libc::geteuid() } != 0 && egid() != 0 {
        let refused = b.path("refused.key");
        assert!(matches!(
            key_config::request(&refused, 0),
            Err(RequestRefused::Fault(_))
        ));
        assert!(!refused.exists(), "a failed request leaves no file");
    }
}

/// **`adopt` writes the members `register` writes and names the key file it
/// never opens**: new at `--out`, 0644, holding no key; the key file named
/// need not exist. Laid as the install lays it, with its box fact appended,
/// the config loads through its connector, the certificate over the key.
#[test]
fn adopt_writes_the_members_register_writes_and_names_the_key_file() {
    let b = bench();
    let (gate_key_file, gate_key, gate_line) = b.requested(Plane::Gate);
    let (admin_key_file, admin_key, admin_line) = b.requested(Plane::Admin);
    let answer = b.answer(&b.authority, &gate_key, &admin_key);
    let answered = AnswerFile::read(&answer).unwrap();

    // The key file named and never opened: absent, adopt still writes.
    let unopened = b.path("out-unopened.toml");
    key_config::adopt(&Adopting {
        plane: Plane::Gate,
        request: &gate_line,
        answer: &answer,
        key_file: Path::new("/nowhere/gate-con.key"),
        out: &unopened,
    })
    .unwrap();

    for (plane, key_file, line, socket) in [
        (Plane::Gate, &gate_key_file, &gate_line, "gate_socket"),
        (Plane::Admin, &admin_key_file, &admin_line, "admin_socket"),
    ] {
        let out = b.path(&format!("{}-adopted.toml", plane.as_str()));
        key_config::adopt(&Adopting {
            plane,
            request: line,
            answer: &answer,
            key_file,
            out: &out,
        })
        .unwrap();
        let meta = std::fs::symlink_metadata(&out).unwrap();
        assert_eq!(meta.mode() & 0o7777, 0o644);
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(!text.contains("PRIVATE KEY"), "{text}");
        let table: toml::Table = text.parse().unwrap();
        for (member, value) in [
            ("server", answered.server.as_str()),
            ("server_name", answered.server_name.as_str()),
            ("agent", answered.agent.as_str()),
            ("agent_id", answered.agent_id.as_str()),
            ("plane", plane.as_str()),
            ("server_certificate", answered.authority.as_str()),
            ("key_file", key_file.to_str().unwrap()),
        ] {
            assert_eq!(table[member].as_str(), Some(value), "{member}");
        }
        assert!(!table.contains_key("key"));

        // Laid as the install lays it: the connector's group, 0640, the box
        // fact appended.
        let laid = b.path(&format!("{}-con.toml", plane.as_str()));
        std::fs::write(
            &laid,
            format!(
                "{text}{socket} = {}\n",
                toml::Value::String(b.path("socket").display().to_string())
            ),
        )
        .unwrap();
        std::fs::set_permissions(&laid, std::fs::Permissions::from_mode(0o640)).unwrap();
        let link = match plane {
            Plane::Gate => {
                crate::link::gate_con::GateConConfig::load(&laid)
                    .unwrap()
                    .link
            }
            Plane::Admin => {
                crate::link::admin_con::AdminConConfig::load(&laid)
                    .unwrap()
                    .link
            }
        };
        assert_eq!(&link.key_file, key_file);
        let key = if plane == Plane::Gate {
            &gate_key
        } else {
            &admin_key
        };
        assert_eq!(&link.key, key);
    }
}

/// **`adopt` checks before it writes a byte**: a certificate over another
/// key or signed by another authority is refused with its reason; a relative
/// key file, a request line that is not one, an answer naming no certificate
/// for the plane, and an `--out` that exists are each refused as usage.
/// `--out` is absent after each refusal, or unchanged where it stood.
#[test]
fn adopt_refuses_an_answer_or_a_path_not_its_own_and_writes_nothing() {
    let b = bench();
    let (gate_key_file, gate_key, gate_line) = b.requested(Plane::Gate);
    let (_, admin_key, admin_line) = b.requested(Plane::Admin);
    let out = b.path("out.toml");
    let adopt = |request: &Path, answer: &Path, key_file: &Path, out: &Path| {
        key_config::adopt(&Adopting {
            plane: Plane::Gate,
            request,
            answer,
            key_file,
            out,
        })
    };

    // The answer's gate certificate is over the admin key.
    let crossed = b.answer(&b.authority, &admin_key, &admin_key);
    assert!(matches!(
        adopt(&gate_line, &crossed, &gate_key_file, &out),
        Err(AdoptRefused::Refused(KEY_MISMATCH))
    ));
    // The request line is the admin key's, and the certificate the gate's.
    let answer = b.answer(&b.authority, &gate_key, &admin_key);
    assert!(matches!(
        adopt(&admin_line, &answer, &gate_key_file, &out),
        Err(AdoptRefused::Refused(KEY_MISMATCH))
    ));
    // The certificate is over the key and signed by another authority.
    let other = Authority::init(&b.path("other"), "weaver-web", &[]).unwrap();
    let mut forged = AnswerFile::read(&b.answer(&other, &gate_key, &admin_key)).unwrap();
    forged.authority = b.authority.certificate_pem().to_owned();
    let forged_path = b.path("forged.json");
    forged.write_new(&forged_path).unwrap();
    assert!(matches!(
        adopt(&gate_line, &forged_path, &gate_key_file, &out),
        Err(AdoptRefused::Refused(NOT_SIGNED))
    ));
    // A relative key file.
    assert!(matches!(
        adopt(&gate_line, &answer, Path::new("gate-con.key"), &out),
        Err(AdoptRefused::Usage(why)) if why.contains("key_file is not an absolute path")
    ));
    // A request line that is not one.
    let not_a_line = b.path("not-a-line.json");
    std::fs::write(&not_a_line, "{\"event\":\"other\"}\n").unwrap();
    assert!(matches!(
        adopt(&not_a_line, &answer, &gate_key_file, &out),
        Err(AdoptRefused::Usage(_))
    ));
    // An answer enrolling admin-con alone.
    let mut admin_only = AnswerFile::read(&answer).unwrap();
    admin_only.gate = None;
    let admin_only_path = b.path("admin-only.json");
    admin_only.write_new(&admin_only_path).unwrap();
    assert!(matches!(
        adopt(&gate_line, &admin_only_path, &gate_key_file, &out),
        Err(AdoptRefused::Usage(_))
    ));
    assert!(!out.exists(), "no refusal writes --out");

    // An --out that stands is never written over.
    std::fs::write(&out, "standing\n").unwrap();
    assert!(matches!(
        adopt(&gate_line, &answer, &gate_key_file, &out),
        Err(AdoptRefused::Usage(why)) if why.contains("exists")
    ));
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "standing\n");
}

/// **A box whose clock is outside the certificate's window is told so**:
/// `adopt` under a clock before the certificate's start, or past its end,
/// answers the command's own fault naming the window's edge and the box's
/// clock, never `not_signed`, while a certificate another authority signed
/// stays `not_signed` under the same clock. Nothing is written.
///
/// Perturbation: answer every verification failure `not_signed`, and the
/// box with a clock at the epoch is sent after the signature. Take a time
/// refusal as the clock's without verifying again at the window's edge, and
/// the forged certificate is told it is the clock's.
#[test]
fn adopt_names_the_boxs_clock_for_a_certificate_signed_and_out_of_its_window() {
    let b = bench();
    let (gate_key_file, gate_key, gate_line) = b.requested(Plane::Gate);
    let (_, admin_key, _) = b.requested(Plane::Admin);
    let out = b.path("out.toml");
    let answer = b.answer(&b.authority, &gate_key, &admin_key);
    let epoch = UnixTime::since_unix_epoch(std::time::Duration::ZERO);
    let far = UnixTime::since_unix_epoch(std::time::Duration::from_secs(70_000_000_000));
    let at = |answer: &Path, now: UnixTime| {
        key_config::adopt_at(
            &Adopting {
                plane: Plane::Gate,
                request: &gate_line,
                answer,
                key_file: &gate_key_file,
                out: &out,
            },
            now,
        )
    };
    for (clock, edge) in [(epoch, "valid from"), (far, "expired at")] {
        match at(&answer, clock) {
            Err(AdoptRefused::Fault(why)) => {
                assert!(why.contains(edge), "{why}");
                assert!(why.contains("the box's clock reads"), "{why}");
            }
            other => panic!("{edge}: {other:?}"),
        }
        assert!(!out.exists(), "{edge}: nothing written");
    }

    let other = Authority::init(&b.path("other"), "weaver-web", &[]).unwrap();
    let mut forged = AnswerFile::read(&b.answer(&other, &gate_key, &admin_key)).unwrap();
    forged.authority = b.authority.certificate_pem().to_owned();
    let forged_path = b.path("forged-clock.json");
    forged.write_new(&forged_path).unwrap();
    assert!(matches!(
        at(&forged_path, epoch),
        Err(AdoptRefused::Refused(NOT_SIGNED))
    ));
    assert!(!out.exists());
}
