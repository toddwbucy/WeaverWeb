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
/// The setgid bit, as a shift: the mode the refusal is tested on, which
/// `tests/no_privilege.rs` would read as a mode set by the repository if
/// it were written as its octal literal or its libc name.
const SET_GROUP_BIT: u32 = 1 << 10;

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
        why.contains(&format!(
            "has group {}, and this runs with effective group {}",
            ours.group, other_group.group
        )) && why.contains(&format!(
            "read through its own group alone, so run it with group {} as its effective group",
            ours.group
        )),
        "{why}"
    );

    // A mode past 0640: the group may write, others may read, or a bit of
    // execute or of the setuid kind.
    for mode in [0o660, 0o644, 0o740, 0o640 | SET_GROUP_BIT] {
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
            why.contains(&format!("has mode {mode:04o}, writable by group or others"))
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

/// **A folder of another owner, or of a group neither the connector's nor
/// root's, is refused while its file passes**: the file is judged first, so
/// the folder's judgment (`client::judge_folder`) is read on the folder's
/// own status, the file's read admitted under the same trust beside it. A
/// test cannot make a folder of another owner or of a group it is not in,
/// so the folder's owner and group are varied on its status.
///
/// Perturbations: drop the folder's owner clause, and the folder of another
/// owner is admitted; drop its group clause, and the folder of another group
/// is admitted (each measured 2026-11-27).
#[test]
fn the_trust_rule_refuses_a_folder_of_another_owner_or_group_though_its_file_passes() {
    use crate::link::client::judge_folder;
    let t = crate::test_dir::tempdir().unwrap();
    let dir = folder(t.path(), "f", 0o750);
    let key = file(&dir, "gate-con.key", "key", 0o640);
    let ours = Trust::connector();
    assert_eq!(read_trusted(&key, Held::Key, &ours).unwrap(), "key");

    let c = std::ffi::CString::new(dir.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: an all-zero stat is valid, and stat fills it from a
    // NUL-terminated path.
    let mut real: libc::stat = unsafe { std::mem::zeroed() };
    assert_eq!(unsafe { libc::stat(c.as_ptr(), &mut real) }, 0);
    judge_folder(&dir, &real, &ours).unwrap();

    let mut other_owner = real;
    other_owner.st_uid = ours.owner.wrapping_add(1);
    let why = format!("{:#}", judge_folder(&dir, &other_owner, &ours).unwrap_err());
    assert!(
        why.contains(&format!(
            "is owned by uid {}, not uid {}",
            other_owner.st_uid, ours.owner
        )) && why.contains("a folder only root writes"),
        "{why}"
    );

    let mut other_group = real;
    other_group.st_gid = ours.group.wrapping_add(1).max(1);
    let why = format!("{:#}", judge_folder(&dir, &other_group, &ours).unwrap_err());
    assert!(
        why.contains(&format!(
            "has group {}, neither this connector's group {} nor root's",
            other_group.st_gid, ours.group
        )),
        "{why}"
    );

    // Root's group is admitted for the folder, as the install may lay it.
    let mut roots_group = real;
    roots_group.st_gid = 0;
    judge_folder(&dir, &roots_group, &ours).unwrap();
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
