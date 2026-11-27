//! **The suite's temporary directories**, each named for its creation,
//! `wwt-<Unix seconds>-<16 hex>`, under the system's temporary directory.
//! A test's directory is removed when the test ends, a panic included
//! (`TempDir`'s drop). A run that ends without unwinding (a suite stopped
//! mid-run, a kill) leaves its directories, so a later run's first
//! [`tempdir`] sweeps every directory of that shape older than [`STALE`]
//! that this account owns, as `fresh_store` sweeps the test databases
//! (#139). A directory of the shape kept on purpose is swept after six
//! hours, so rename it out of the shape to keep it. A test makes a
//! temporary directory only through [`tempdir`], which the guard below
//! holds for `src/`. The integration tests under `tests/` are crates of
//! their own and keep `tempfile`'s names.

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// The age past which the sweep removes a directory of the suite's shape,
/// longer than any suite runs.
const STALE: u64 = 6 * 3600;

/// **A test's temporary directory**, `tempfile::tempdir`'s in all but its
/// name and its mode, 0700, after the run's one sweep.
pub(crate) fn tempdir() -> std::io::Result<tempfile::TempDir> {
    static SWEPT: std::sync::Once = std::sync::Once::new();
    let at = unix_now();
    SWEPT.call_once(|| {
        let swept = sweep(&std::env::temp_dir(), at, STALE);
        if !swept.is_empty() {
            eprintln!("swept {} stale test directories: {swept:?}", swept.len());
        }
    });
    // **0700, whatever the umask**: tempfile makes a directory by the umask
    // where it is given no mode, and a connector's trust rule refuses a
    // folder others may enter (the brief of 2026-11-27, child 6), so a key
    // laid directly under a test's directory reads as one laid in a folder
    // of root's.
    tempfile::Builder::new()
        .prefix(&named(at))
        .rand_bytes(0)
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after the epoch")
        .as_secs()
}

/// A directory's name for its creation at `at`.
fn named(at: u64) -> String {
    format!(
        "wwt-{at:010}-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..16]
    )
}

/// The Unix seconds a name carries, where it is `wwt-`, ten digits, `-`
/// and sixteen lowercase hex digits.
fn dated(name: &str) -> Option<u64> {
    let (secs, hex) = name.strip_prefix("wwt-")?.split_once('-')?;
    let shaped = secs.len() == 10
        && secs.bytes().all(|b| b.is_ascii_digit())
        && hex.len() == 16
        && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if shaped { secs.parse().ok() } else { None }
}

/// **The sweep**: every entry of `dir` whose name [`dated`] reads a time
/// from, more than `older_than` seconds before `at`, that is a directory
/// and not a link to one, and that this account owns, is removed with what
/// it holds, never following a link inside it. A failure is printed and
/// passed over. Answers the names removed.
fn sweep(dir: &Path, at: u64, older_than: u64) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    // SAFETY: geteuid takes nothing and cannot fail.
    let me = unsafe { libc::geteuid() };
    let mut swept = Vec::new();
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(made) = dated(&name) else { continue };
        if at.saturating_sub(made) <= older_than {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_dir() || meta.uid() != me {
            continue;
        }
        match std::fs::remove_dir_all(entry.path()) {
            Ok(()) => swept.push(name),
            Err(e) => eprintln!("the sweep left the test directory {name}: {e}"),
        }
    }
    swept
}

/// Every Rust source under `src/`, with its text, for the suite's guards,
/// each path relative to the crate's root.
pub(crate) fn rust_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, found: &mut Vec<(PathBuf, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .expect("read a source directory")
            .flatten()
            .collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if meta.is_dir() {
                walk(&path, found);
            } else if meta.is_file() && path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).expect("read a source");
                found.push((path, text));
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut found = Vec::new();
    walk(&root.join("src"), &mut found);
    found
        .into_iter()
        .map(|(path, text)| (path.strip_prefix(root).unwrap_or(&path).to_owned(), text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A test's directory is named for its creation** under the system's
    /// temporary directory, and is gone once the test lets it go.
    ///
    /// Perturbation: answer `tempfile::tempdir()`, and the name has no date.
    #[test]
    fn a_test_directory_is_named_for_its_creation() {
        let before = unix_now();
        let dir = tempdir().unwrap();
        let after = unix_now();
        let path = dir.path().to_owned();
        assert_eq!(path.parent(), Some(std::env::temp_dir().as_path()));
        let name = path.file_name().unwrap().to_str().unwrap();
        let made = dated(name).unwrap_or_else(|| panic!("an undated name: {name}"));
        assert!(
            (before..=after).contains(&made),
            "{made} not in {before}..={after}"
        );
        assert!(path.is_dir());
        drop(dir);
        assert!(!path.exists(), "the directory outlived its test");
    }

    /// **The sweep removes only a stale directory of the suite's shape**:
    /// not one at the age bound, not a name of another shape (`tempfile`'s
    /// own, a test database's, a near miss), not a regular file of the
    /// shape, and not a link of the shape, whose target is left whole. A
    /// directory another account owns is passed over too, which a test
    /// cannot make without privilege.
    ///
    /// Perturbations: drop the age check, and the directory at the bound
    /// goes. Read the entry following links, and the link goes.
    #[test]
    fn the_sweep_removes_only_a_stale_directory_of_the_suites_shape() {
        let parent = tempdir().unwrap();
        let at = unix_now();
        let hex = "0123456789abcdef";
        let stale = format!("wwt-{:010}-{hex}", at - STALE - 1);
        let at_bound = format!("wwt-{:010}-{hex}", at - STALE);
        let old = at - STALE - 60;
        let others = [
            ".tmpAbC123".to_owned(),
            format!("wwt_{old:010}_{hex}"),
            format!("wwt-{old:010}-0123456789ABCDEF"),
            format!("wwt-{old:010}-0123456789abcde"),
            format!("wwt-{old}0-{hex}"),
        ];
        for name in [&stale, &at_bound].into_iter().chain(&others) {
            std::fs::create_dir(parent.path().join(name)).unwrap();
            std::fs::write(parent.path().join(name).join("held"), b"x").unwrap();
        }
        let file = format!("wwt-{old:010}-{}", "fedcba9876543210");
        std::fs::write(parent.path().join(&file), b"x").unwrap();
        let target = parent.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("held"), b"x").unwrap();
        let link = format!("wwt-{old:010}-{}", "00112233aabbccdd");
        std::os::unix::fs::symlink(&target, parent.path().join(&link)).unwrap();

        let swept = sweep(parent.path(), at, STALE);

        assert_eq!(swept, vec![stale.clone()]);
        assert!(!parent.path().join(&stale).exists());
        for kept in [&at_bound, &file, &link].into_iter().chain(&others) {
            assert!(
                std::fs::symlink_metadata(parent.path().join(kept)).is_ok(),
                "{kept} was swept"
            );
        }
        assert!(
            target.join("held").exists(),
            "the link's target was emptied"
        );
    }

    /// **A test makes a temporary directory only through [`tempdir`]**, so
    /// no directory a killed run leaves goes unswept.
    ///
    /// Perturbation: make one with `tempfile` directly in any test file,
    /// and this names it.
    #[test]
    fn a_temporary_directory_is_made_only_through_this_module() {
        let calls = ["tempfile::tempdir", "TempDir::new", "tempfile::Builder"];
        let found: Vec<String> = rust_sources()
            .into_iter()
            .filter(|(path, _)| !path.ends_with("src/test_dir.rs"))
            .flat_map(|(path, text)| {
                text.lines()
                    .enumerate()
                    .filter(|(_, line)| calls.iter().any(|call| line.contains(call)))
                    .map(|(at, _)| format!("{}:{}", path.display(), at + 1))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            found.is_empty(),
            "made through `test_dir::tempdir`: {found:?}"
        );
    }
}
