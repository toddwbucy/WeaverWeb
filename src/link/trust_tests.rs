//! conforms: web-a-connectors-key-is-a-file-of-its-own
//!
//! **The connectors' trust rule** (the brief of 2026-11-27, child 6;
//! `client::read_trusted`): each file root's, of the connector's group, no
//! bit outside 0640; its folder root's, of that group or root's, writable by
//! no group or other and nothing for others; a key carrying no access-list
//! entry beyond the three base ones, a config's named entries granting read
//! alone. The files stand in a temporary folder of the test's own uid, which
//! `Trust::connector` takes as the owner in a test build; a test of root's
//! rule names owner 0 itself. The access lists are written by `fsetxattr`.
//! No database.

use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::link::client::{Held, Trust, read_trusted};

const ACL_USER_OBJ: u16 = 0x01;
const ACL_USER: u16 = 0x02;
const ACL_GROUP_OBJ: u16 = 0x04;
const ACL_MASK: u16 = 0x10;
const ACL_OTHER: u16 = 0x20;
const ACL_UNDEFINED_ID: u32 = u32::MAX;
/// A named party's uid in an entry; it need not exist.
const NAMED: u32 = 65534;

/// A folder of the test's own at `mode`, under the test's temporary folder.
fn folder(parent: &Path, name: &str, mode: u32) -> PathBuf {
    let at = parent.join(name);
    std::fs::create_dir(&at).unwrap();
    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(mode)).unwrap();
    at
}

fn file(dir: &Path, name: &str, text: &str, mode: u32) -> PathBuf {
    let at = dir.join(name);
    std::fs::write(&at, text).unwrap();
    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(mode)).unwrap();
    at
}

/// **Write the access list** `(tag, perm, id)` onto `path`, sorted as the
/// kernel requires. Answers false where the filesystem keeps none.
fn set_acl(path: &Path, entries: &[(u16, u16, u32)]) -> bool {
    let mut value = 2u32.to_le_bytes().to_vec();
    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|(tag, _, id)| (*tag, *id));
    for (tag, perm, id) in sorted {
        value.extend_from_slice(&tag.to_le_bytes());
        value.extend_from_slice(&perm.to_le_bytes());
        value.extend_from_slice(&id.to_le_bytes());
    }
    let f = std::fs::File::open(path).unwrap();
    // SAFETY: a valid descriptor, a NUL-terminated name and a buffer of
    // its stated length.
    let set = unsafe {
        libc::fsetxattr(
            f.as_raw_fd(),
            c"system.posix_acl_access".as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
        )
    };
    if set < 0 {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::ENOTSUP) {
            return false;
        }
        panic!("setting the access list of {}: {e}", path.display());
    }
    true
}

/// The base entries of a 0640 file, with `named` entries and a mask.
fn with_named(named: &[(u16, u16, u32)], mask: u16) -> Vec<(u16, u16, u32)> {
    let mut entries = vec![
        (ACL_USER_OBJ, 6, ACL_UNDEFINED_ID),
        (ACL_GROUP_OBJ, 4, ACL_UNDEFINED_ID),
        (ACL_MASK, mask, ACL_UNDEFINED_ID),
        (ACL_OTHER, 0, ACL_UNDEFINED_ID),
    ];
    entries.extend_from_slice(named);
    entries
}

fn refused(path: &Path, held: Held, trust: &Trust) -> String {
    format!("{:#}", read_trusted(path, held, trust).unwrap_err())
}

/// **The laid shape is admitted**: a config and a key at 0640 and at 0600,
/// in a folder at 0750 and at 0700, the test's uid standing for root's and
/// its group for the connector's.
#[test]
fn the_trust_rule_admits_root_files_of_the_connectors_group_at_0640() {
    let t = crate::test_dir::tempdir().unwrap();
    let trust = Trust::connector();
    for (folder_mode, file_mode) in [(0o750, 0o640), (0o700, 0o600)] {
        let dir = folder(t.path(), &format!("f{folder_mode:o}"), folder_mode);
        let config = file(&dir, "gate-con.toml", "config", file_mode);
        let key = file(&dir, "gate-con.key", "key", file_mode);
        assert_eq!(
            read_trusted(&config, Held::Config, &trust).unwrap(),
            "config"
        );
        assert_eq!(read_trusted(&key, Held::Key, &trust).unwrap(), "key");
    }
}

/// **A file of another owner, of another group, or with a bit outside 0640
/// is refused**, each naming what was found: a config of the connector's own
/// uid (root's rule, owner 0, read by a connector whose uid owns the file),
/// a key of a group not the connector's, and a key its group may write.
///
/// Perturbations: admit a group-writable key (0660 inside the mode's
/// bound), and the key at 0660 is read; drop the owner check, and the config
/// of the connector's own uid is read, or refused for its folder instead.
#[test]
fn the_trust_rule_refuses_another_owner_another_group_and_a_wider_mode() {
    let t = crate::test_dir::tempdir().unwrap();
    let dir = folder(t.path(), "f", 0o750);
    let ours = Trust::connector();

    // Root's rule, read of a config the connector's own uid owns.
    let config = file(&dir, "admin-con.toml", "config", 0o600);
    let root = Trust {
        owner: 0,
        group: ours.group,
    };
    let why = refused(&config, Held::Config, &root);
    assert!(
        why.contains(&format!("is owned by uid {}", ours.owner))
            && why.contains("a connector's config and key are root's"),
        "{why}"
    );

    // A group not the connector's.
    let key = file(&dir, "admin-con.key", "key", 0o640);
    let other_group = Trust {
        owner: ours.owner,
        group: ours.group.wrapping_add(1),
    };
    let why = refused(&key, Held::Key, &other_group);
    assert!(
        why.contains(&format!("has group {}", ours.group))
            && why.contains("read through its own group alone"),
        "{why}"
    );

    // A mode past 0640: the group may write, others may read, or a bit of
    // execute or of the setuid kind.
    for mode in [0o660, 0o644, 0o740, 0o2640] {
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(mode)).unwrap();
        let why = refused(&key, Held::Key, &ours);
        assert!(
            why.contains(&format!("has mode {mode:04o}"))
                && why.contains("0640 or tighter, with nothing for others"),
            "{mode:o}: {why}"
        );
    }

    // Not a regular file, and a symlink.
    let fifo = dir.join("fifo.key");
    let c = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: a NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o640) }, 0);
    assert!(refused(&fifo, Held::Key, &ours).contains("is not a regular file"));
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.join("link.key");
    std::os::unix::fs::symlink(&key, &link).unwrap();
    assert!(refused(&link, Held::Key, &ours).contains("is a symlink"));
}

/// **A folder another party may write, or any party but its owner and group
/// may enter, is refused**, and a folder that is a symlink.
#[test]
fn the_trust_rule_refuses_a_folder_others_may_write_or_enter() {
    let t = crate::test_dir::tempdir().unwrap();
    let ours = Trust::connector();
    for mode in [0o770, 0o751, 0o755, 0o757] {
        let dir = folder(t.path(), &format!("f{mode:o}"), mode);
        let key = file(&dir, "gate-con.key", "key", 0o640);
        let why = refused(&key, Held::Key, &ours);
        assert!(
            why.contains("is writable by group or others, or not root's")
                && why.contains("a folder only root writes"),
            "{mode:o}: {why}"
        );
    }
    let real = folder(t.path(), "real", 0o750);
    file(&real, "gate-con.key", "key", 0o640);
    let linked = t.path().join("linked");
    std::os::unix::fs::symlink(&real, &linked).unwrap();
    let why = refused(&linked.join("gate-con.key"), Held::Key, &ours);
    assert!(why.contains("is a symlink"), "{why}");
}

/// **A key carrying any access-list entry beyond its three base ones is
/// refused; a config's named entry granting read is admitted, and one
/// granting write is refused**, though the mode, which shows the mask, reads
/// 0640 in each case.
///
/// Perturbations: drop the key's check, and the key a named party reads is
/// read; drop the config's check, and the config a named entry grants write
/// is read.
#[test]
fn the_trust_rule_reads_the_access_list_as_well_as_the_mode() {
    let t = crate::test_dir::tempdir().unwrap();
    let dir = folder(t.path(), "f", 0o750);
    let ours = Trust::connector();

    let key = file(&dir, "admin-con.key", "key", 0o640);
    if !set_acl(&key, &with_named(&[(ACL_USER, 4, NAMED)], 4)) {
        eprintln!("skipped: the temporary folder's filesystem keeps no access lists");
        return;
    }
    let meta = std::fs::metadata(&key).unwrap();
    assert_eq!(
        meta.permissions().mode() & 0o7777,
        0o640,
        "the mask reads as 0640"
    );
    let why = refused(&key, Held::Key, &ours);
    assert!(
        why.contains("carries an access-list entry beyond its owner, group and others")
            && why.contains("read by its own group alone"),
        "{why}"
    );

    // A config the service's account reads through an entry of its own.
    let read = file(&dir, "read.toml", "config", 0o640);
    assert!(set_acl(&read, &with_named(&[(ACL_USER, 4, NAMED)], 4)));
    assert_eq!(read_trusted(&read, Held::Config, &ours).unwrap(), "config");

    // A config a named entry grants write, the mask holding the mode at 0640.
    let write = file(&dir, "write.toml", "config", 0o640);
    assert!(set_acl(&write, &with_named(&[(ACL_USER, 6, NAMED)], 4)));
    let meta = std::fs::metadata(&write).unwrap();
    assert_eq!(
        meta.permissions().mode() & 0o7777,
        0o640,
        "the mask reads as 0640"
    );
    let why = refused(&write, Held::Config, &ours);
    assert!(
        why.contains("carries an access-list entry that grants more than read")
            && why.contains("written by root alone"),
        "{why}"
    );
}

/// **Each file is read under its bound**: a key past 64 KiB and a config
/// past 1 MiB are refused.
#[test]
fn the_trust_rule_reads_each_file_under_its_bound() {
    let t = crate::test_dir::tempdir().unwrap();
    let dir = folder(t.path(), "f", 0o750);
    let ours = Trust::connector();
    let key = file(&dir, "big.key", &"k".repeat(64 * 1024 + 1), 0o640);
    assert!(refused(&key, Held::Key, &ours).contains("is not a connector's key"));
    let config = file(&dir, "big.toml", &"#".repeat(1024 * 1024 + 1), 0o640);
    assert!(refused(&config, Held::Config, &ours).contains("is not a connector's config"));
}
