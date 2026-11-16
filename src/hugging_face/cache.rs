//! conforms: web-a-repositorys-file-names-write-nothing-outside-the-cache
//!
//! **The model cache** (Spec 2.15): the directory `model_cache` names, in
//! Hugging Face's own layout, written by the console itself so no Hugging
//! Face tool is needed on the machine.
//!
//! - `models--<owner>--<name>/blobs/<id>`, where `<id>` is the SHA-256 for a
//!   file kept in LFS and the git blob id for a file kept in git, and
//!   `<id>.incomplete` beside it while the file downloads
//! - `snapshots/<commit>/<path>`, a relative symbolic link to its blob
//! - no `refs/`, no lock file and no file mode: every file is created under
//!   the process's umask, and a host's group read comes from the directory's
//!   group and the service's umask, set outside this repository
//!
//! **Nothing is written outside the root.** Every path is the root joined
//! with names this module checked (`RepoId`, `is_commit`, `is_repo_path`, an
//! identity of hex digits), every directory on the way is created or found
//! as a directory and never followed as a symbolic link, and a partial is
//! opened without following one.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

use super::{RepoFile, RepoId, is_commit, is_repo_path};

/// The suffix of a file in flight.
pub const INCOMPLETE: &str = ".incomplete";

/// A blob's name: its SHA-256 (64 hex) or its git blob id (40 hex).
fn is_identity(id: &str) -> bool {
    matches!(id.len(), 40 | 64) && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl RepoFile {
    /// **The blob's name**: the SHA-256 for a file kept in LFS, the git blob
    /// id for a file kept in git, Hugging Face's own rule.
    pub fn identity(&self) -> &str {
        self.sha256.as_deref().unwrap_or(&self.blob_id)
    }

    /// Whether the file is kept in git, so verified by its git blob id.
    pub fn in_git(&self) -> bool {
        self.sha256.is_none()
    }
}

/// The cache at its root.
#[derive(Clone, Debug)]
pub struct Cache {
    root: PathBuf,
}

impl Cache {
    /// **The cache at `root`**: an absolute path to a directory that stands,
    /// not a symbolic link.
    pub fn at(root: &Path) -> anyhow::Result<Cache> {
        anyhow::ensure!(
            root.is_absolute(),
            "model_cache is an absolute path, and {} is not",
            root.display()
        );
        let meta = std::fs::symlink_metadata(root)
            .map_err(|e| anyhow::anyhow!("model_cache {}: {e}", root.display()))?;
        anyhow::ensure!(
            meta.is_dir(),
            "model_cache {} is not a directory",
            root.display()
        );
        Ok(Cache {
            root: root.to_owned(),
        })
    }

    /// **The cache the config names**, the start's refusal (Spec 2.15):
    /// none where `model_cache` is unset, and where it is set, a directory
    /// the server can create a file in, or an error that stops the start.
    pub fn configured(root: Option<&Path>) -> anyhow::Result<Option<Cache>> {
        let Some(root) = root else {
            return Ok(None);
        };
        let cache = Cache::at(root)?;
        cache.probe()?;
        Ok(Some(cache))
    }

    /// **Whether the server can create a file in the cache**, the start's
    /// check: a probe file created new and removed.
    pub fn probe(&self) -> anyhow::Result<()> {
        let mut name = [0u8; 8];
        getrandom::fill(&mut name).expect("the operating system's random source answers");
        let probe = self.root.join(format!(
            ".weaver-web-probe-{}",
            crate::store::identity::hex(&name)
        ));
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&probe)
            .map_err(|e| {
                anyhow::anyhow!(
                    "the server cannot create a file in model_cache {}: {e}",
                    self.root.display()
                )
            })?;
        std::fs::remove_file(&probe)?;
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn repo_dir(&self, repo: &RepoId) -> PathBuf {
        self.root
            .join(format!("models--{}--{}", repo.owner(), repo.name()))
    }

    /// The repository's `blobs` directory.
    pub fn blobs(&self, repo: &RepoId) -> PathBuf {
        self.repo_dir(repo).join("blobs")
    }

    /// A blob's path.
    pub fn blob(&self, repo: &RepoId, id: &str) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(is_identity(id), "a blob's name is not of its shape");
        Ok(self.blobs(repo).join(id))
    }

    /// A blob's partial while it downloads.
    pub fn incomplete(&self, repo: &RepoId, id: &str) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(is_identity(id), "a blob's name is not of its shape");
        Ok(self.blobs(repo).join(format!("{id}{INCOMPLETE}")))
    }

    /// A file's place in its snapshot.
    pub fn snapshot(&self, repo: &RepoId, commit: &str, path: &str) -> anyhow::Result<PathBuf> {
        anyhow::ensure!(is_commit(commit), "a snapshot names a commit");
        anyhow::ensure!(is_repo_path(path), "a file's path is not of its shape");
        Ok(self
            .repo_dir(repo)
            .join("snapshots")
            .join(commit)
            .join(path))
    }

    /// **A directory under the root, made where it is missing**: every
    /// component found as a directory, never a symbolic link, or created.
    pub fn ensure_dir(&self, dir: &Path) -> anyhow::Result<()> {
        let rest = dir
            .strip_prefix(&self.root)
            .map_err(|_| anyhow::anyhow!("a directory outside the cache"))?;
        let mut at = self.root.clone();
        for component in rest.components() {
            let Component::Normal(name) = component else {
                anyhow::bail!("a directory's path is not of its shape");
            };
            at.push(name);
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.is_dir() => {}
                Ok(_) => anyhow::bail!(
                    "{} stands in the cache and is not a directory",
                    at.strip_prefix(&self.root).unwrap_or(&at).display()
                ),
                Err(e) if e.kind() == io::ErrorKind::NotFound => match std::fs::create_dir(&at) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                        anyhow::ensure!(
                            std::fs::symlink_metadata(&at)?.is_dir(),
                            "a directory of the cache was replaced as it was made"
                        );
                    }
                    Err(e) => return Err(e.into()),
                },
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }

    /// **The snapshot's link to its blob**, relative, as Hugging Face's
    /// tools write it: `../../blobs/<id>` for a file at the snapshot's top,
    /// one `../` more for each directory of its path. Written beside its
    /// place and renamed over it, so a reader meets the old link or the new.
    pub fn link(&self, repo: &RepoId, commit: &str, path: &str, id: &str) -> anyhow::Result<()> {
        let place = self.snapshot(repo, commit, path)?;
        anyhow::ensure!(is_identity(id), "a blob's name is not of its shape");
        let parent = place.parent().expect("a snapshot's file has a parent");
        self.ensure_dir(parent)?;
        let target = relative_target(path, id);
        let staged = parent.join(format!(
            ".{}.weaver-web-link",
            place.file_name().unwrap().to_string_lossy()
        ));
        let _ = std::fs::remove_file(&staged);
        std::os::unix::fs::symlink(&target, &staged)?;
        std::fs::rename(&staged, &place)?;
        Ok(())
    }

    /// **The blob a snapshot's file links to**, where its link stands in the
    /// relative form `link` writes. `None` for anything else.
    pub fn linked(&self, repo: &RepoId, commit: &str, path: &str) -> Option<String> {
        let place = self.snapshot(repo, commit, path).ok()?;
        let target = std::fs::read_link(&place).ok()?;
        let target = target.to_str()?;
        let id = target.rsplit('/').next()?;
        (is_identity(id) && target == relative_target(path, id)).then(|| id.to_owned())
    }

    /// **A blob opened to read**, without following a symbolic link: what
    /// the box's pull from the cache reads (Spec 8.3, a later act).
    pub fn open(&self, repo: &RepoId, id: &str) -> anyhow::Result<File> {
        let path = self.blob(repo, id)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        anyhow::ensure!(file.metadata()?.is_file(), "a blob is not a regular file");
        Ok(file)
    }

    /// **The bytes free** where the repository's blobs are written, from
    /// `statvfs`: what an unprivileged writer may use.
    pub fn free_space(dir: &Path) -> io::Result<u64> {
        use std::os::unix::ffi::OsStrExt;
        let path = std::ffi::CString::new(dir.as_os_str().as_bytes())
            .map_err(|_| io::Error::other("a path with a NUL"))?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `path` is a NUL-terminated string and `stat` a struct of
        // the type `statvfs` writes.
        if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(stat.f_bavail as u64 * stat.f_frsize as u64)
    }
}

fn relative_target(path: &str, id: &str) -> String {
    let depth = path.split('/').count() - 1;
    format!("{}blobs/{id}", "../".repeat(2 + depth))
}

/// **A file's two digests as it is written**: the SHA-256, always, and for
/// a file kept in git the git blob id, the SHA-1 of `blob <size>\0` and
/// the content.
pub struct Hasher {
    sha256: ring::digest::Context,
    git: Option<ring::digest::Context>,
}

impl Hasher {
    pub fn new(size: u64, in_git: bool) -> Hasher {
        Hasher {
            sha256: ring::digest::Context::new(&ring::digest::SHA256),
            git: in_git.then(|| {
                let mut git = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
                git.update(format!("blob {size}\0").as_bytes());
                git
            }),
        }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.sha256.update(bytes);
        if let Some(git) = &mut self.git {
            git.update(bytes);
        }
    }

    /// The SHA-256 and, for a file kept in git, the git blob id, in hex.
    pub fn finish(self) -> (String, Option<String>) {
        let hex = |d: ring::digest::Digest| crate::store::identity::hex(d.as_ref());
        (hex(self.sha256.finish()), self.git.map(|g| hex(g.finish())))
    }

    /// **Whether the digests are the file's**: the SHA-256 Hugging Face
    /// published for a file kept in LFS, or the git blob id for a file kept
    /// in git, whose SHA-256 the console computed. Answers the SHA-256.
    pub fn verify(self, file: &RepoFile) -> Result<String, String> {
        let (sha256, git) = self.finish();
        let matches = match (&file.sha256, git) {
            (Some(expected), _) => *expected == sha256,
            (None, Some(blob_id)) => blob_id == file.blob_id,
            (None, None) => false,
        };
        if matches {
            Ok(sha256)
        } else {
            Err(format!(
                "{}: its digest differs from the one Hugging Face published",
                file.path
            ))
        }
    }
}

/// **A file's digests from its start**, read whole: a found blob's check,
/// and a partial's bytes hashed again before its resume. Answers the hasher
/// and the bytes read.
pub fn hash_from_start(file: &mut File, size: u64, in_git: bool) -> io::Result<(Hasher, u64)> {
    use std::io::Seek;
    file.seek(io::SeekFrom::Start(0))?;
    let mut hasher = Hasher::new(size, in_git);
    let mut buffer = vec![0u8; 1 << 20];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            return Ok((hasher, read));
        }
        hasher.update(&buffer[..n]);
        read += n as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The start's refusal** (Spec 2.15): unset is no cache, and a set
    /// `model_cache` that is relative, missing, a file, a symbolic link or
    /// a directory the server cannot create a file in stops the start. A
    /// directory that serves is taken, and its probe leaves nothing behind.
    #[test]
    fn the_start_refuses_a_cache_it_cannot_write() {
        assert!(Cache::configured(None).unwrap().is_none());
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        assert!(Cache::configured(Some(Path::new("relative/cache"))).is_err());
        assert!(Cache::configured(Some(&root)).is_err());
        std::fs::write(&root, b"").unwrap();
        assert!(Cache::configured(Some(&root)).is_err());
        std::fs::remove_file(&root).unwrap();
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &root).unwrap();
        assert!(Cache::configured(Some(&root)).is_err());
        std::fs::remove_file(&root).unwrap();
        std::fs::create_dir(&root).unwrap();
        let cache = Cache::configured(Some(&root)).unwrap().unwrap();
        assert_eq!(cache.root(), root);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        // A directory the server cannot write, where the test does not run
        // as root, whom no mode refuses.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o500)).unwrap();
        if unsafe { libc::geteuid() } != 0 {
            assert!(Cache::configured(Some(&root)).is_err());
        }
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    /// **A snapshot's link is relative**, one `../` more for each directory
    /// of the file's path, as Hugging Face's own tools write it.
    #[test]
    fn a_snapshots_link_is_relative() {
        let id = "a".repeat(64);
        assert_eq!(
            relative_target("model.gguf", &id),
            format!("../../blobs/{id}")
        );
        assert_eq!(
            relative_target("onnx/sub/model.onnx", &id),
            format!("../../../../blobs/{id}")
        );
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path()).unwrap();
        let repo = RepoId::parse("org/open").unwrap();
        let commit = "27b350a791e81d9a4d1ddca1c49282e9ec533768";
        cache.ensure_dir(&cache.blobs(&repo)).unwrap();
        std::fs::write(cache.blob(&repo, &id).unwrap(), b"x").unwrap();
        cache
            .link(&repo, commit, "onnx/sub/model.onnx", &id)
            .unwrap();
        let place = cache
            .snapshot(&repo, commit, "onnx/sub/model.onnx")
            .unwrap();
        assert_eq!(std::fs::read(&place).unwrap(), b"x");
        assert_eq!(
            cache.linked(&repo, commit, "onnx/sub/model.onnx"),
            Some(id.clone())
        );
    }

    /// **No name leaves the root**: a blob's name out of its shape, a
    /// snapshot's path with `..` and a directory outside the root are each
    /// refused before any write.
    #[test]
    fn no_name_leaves_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        std::fs::create_dir(&root).unwrap();
        let cache = Cache::at(&root).unwrap();
        let repo = RepoId::parse("org/open").unwrap();
        let commit = "27b350a791e81d9a4d1ddca1c49282e9ec533768";
        assert!(cache.blob(&repo, "../../escaped").is_err());
        assert!(cache.incomplete(&repo, "not-hex").is_err());
        assert!(
            cache
                .snapshot(&repo, commit, "../../../../escaped")
                .is_err()
        );
        assert!(cache.snapshot(&repo, "main", "model.gguf").is_err());
        assert!(cache.ensure_dir(&dir.path().join("outside")).is_err());
        assert!(cache.ensure_dir(&root.join("a/../../outside")).is_err());
        assert!(!dir.path().join("outside").exists());
        assert!(!dir.path().join("escaped").exists());
    }
}
