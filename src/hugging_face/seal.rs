//! **The Hugging Face token's seal** (Spec 2.15): the stated exception to
//! the digest rule of Spec 2.8 and 2.13. Every token the console issues is
//! kept as its digest, because the console only checks it. The Hugging Face
//! token is the one credential the console presents outward, which a digest
//! cannot serve, so it is sealed: ChaCha20-Poly1305 from `ring`, under a
//! 32-byte key in `authority_dir`, with a fresh nonce for every seal. The
//! store holds the ciphertext and the nonce alone, so a dump of the store
//! holds no usable token.
//!
//! **The key is a file of the authority's set**, `seal.key`, 64 lowercase hex
//! digits. `authority init` writes it, `authority rotate` carries it into the
//! new set, and `authority seal-key` writes it beside an authority made before
//! it and refuses to overwrite one. **A lost key means the operator enters the
//! token again**: the row's seal no longer opens, and the page says so.
//!
//! The key is read where it is used, so a key `authority seal-key` writes
//! while the server runs serves the next connect without a restart.

use std::io::Read;
use std::path::Path;

use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};

use super::Token;

/// The key's file in `authority_dir`.
pub const SEAL_KEY: &str = "seal.key";

/// What the seal is bound to, so a ciphertext of another use never opens
/// as a token.
const AAD: &[u8] = b"weaver-web hugging-face token";

/// A sealed token: what the store holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// The key, read from its file. It renders as `SealKey(..)`.
pub struct SealKey(LessSafeKey);

impl std::fmt::Debug for SealKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SealKey(..)")
    }
}

/// Fresh key bytes from the operating system's random source.
pub(crate) fn mint() -> [u8; 32] {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).expect("the operating system's random source answers");
    key
}

/// **The key written to `path`**, as 64 hex digits and a newline: created
/// new, never over a file that stands, readable by its owner alone like the
/// authority's other keys, and synced before it is reported written.
pub(crate) fn write_key(path: &Path, key: &[u8; 32]) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
    let mut text = crate::store::identity::hex(key);
    text.push('\n');
    file.write_all(text.as_bytes())?;
    file.sync_all()
        .map_err(|e| anyhow::anyhow!("syncing {}: {e}", path.display()))?;
    Ok(())
}

/// The key's bytes as they stand in `path`, without following a symbolic
/// link and without waiting, so a FIFO at its name is refused at once, or
/// `None` where no file stands.
pub(crate) fn read_key(path: &Path) -> anyhow::Result<Option<[u8; 32]>> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => anyhow::bail!("reading {}: {e}", path.display()),
    };
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "{} is not a regular file",
        path.display()
    );
    let mut text = String::new();
    file.by_ref().take(256).read_to_string(&mut text)?;
    let text = text.trim_end_matches('\n');
    let mut key = [0u8; 32];
    anyhow::ensure!(
        text.len() == 64
            && text
                .as_bytes()
                .chunks(2)
                .zip(key.iter_mut())
                .all(|(pair, byte)| {
                    match std::str::from_utf8(pair)
                        .ok()
                        .and_then(|p| u8::from_str_radix(p, 16).ok())
                    {
                        Some(b) => {
                            *byte = b;
                            true
                        }
                        None => false,
                    }
                }),
        "{} is not a seal key: 64 hex digits",
        path.display()
    );
    Ok(Some(key))
}

impl SealKey {
    /// **The key in `authority_dir`**, or `None` where none stands.
    pub fn load(authority_dir: &Path) -> anyhow::Result<Option<SealKey>> {
        Ok(read_key(&authority_dir.join(SEAL_KEY))?.map(|key| SealKey::of(&key)))
    }

    fn of(key: &[u8; 32]) -> SealKey {
        SealKey(LessSafeKey::new(
            UnboundKey::new(&CHACHA20_POLY1305, key).expect("a 32-byte key is ChaCha20's length"),
        ))
    }

    /// **The token sealed** under a fresh nonce.
    pub fn seal(&self, token: &Token) -> Sealed {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).expect("the operating system's random source answers");
        let mut ciphertext = token.expose().as_bytes().to_vec();
        self.0
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce),
                Aad::from(AAD),
                &mut ciphertext,
            )
            .expect("a token of at most 256 bytes seals");
        Sealed {
            ciphertext,
            nonce: nonce.to_vec(),
        }
    }

    /// **The token a seal holds**, or `None` where it does not open under
    /// this key: another key's seal, or a row altered.
    pub fn open(&self, sealed: &Sealed) -> Option<Token> {
        let nonce = Nonce::try_assume_unique_for_key(&sealed.nonce).ok()?;
        let mut buffer = sealed.ciphertext.clone();
        let plain = self
            .0
            .open_in_place(nonce, Aad::from(AAD), &mut buffer)
            .ok()?;
        Token::parse(std::str::from_utf8(plain).ok()?)
    }
}

/// **`authority seal-key`'s act**: a key written beside an authority that
/// stands, refused where none stands or where a key stands already.
pub fn write_beside(authority_dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    anyhow::ensure!(
        crate::link::authority::stands(authority_dir),
        "no authority stands at {}; `authority init` creates one, and its seal key with it",
        authority_dir.display()
    );
    let path = authority_dir.join(SEAL_KEY);
    if read_key(&path)?.is_some() {
        anyhow::bail!(
            "a seal key stands at {}; it is never overwritten, since a token sealed under it would no longer open",
            path.display()
        );
    }
    write_key(&path, &mint())?;
    // Opened as a directory, so a FIFO at the name is refused at once.
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY)
        .open(authority_dir)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| anyhow::anyhow!("syncing {}: {e}", authority_dir.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    use std::time::Duration;

    use super::{SEAL_KEY, read_key};

    /// **A FIFO at the key's name is refused and never waited on**: the
    /// read opens without waiting and finds no regular file. An open that
    /// waited would hold the server's start, or `authority seal-key`, until
    /// a writer came.
    #[test]
    fn a_fifo_at_the_keys_name_is_refused_and_never_waited_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SEAL_KEY);
        let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: `name` is a NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let (sent, read) = std::sync::mpsc::channel();
        let reading = path.clone();
        std::thread::spawn(move || {
            let _ = sent.send(read_key(&reading).map(|key| key.is_some()));
        });
        let read = read.recv_timeout(Duration::from_secs(5));
        if read.is_err() {
            // A writer releases an open still waiting, so the test fails
            // rather than hangs.
            let _ = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path);
        }
        let read = read.expect("the key's read waited on the FIFO");
        let refused = read.expect_err("a FIFO was read as a key");
        assert!(
            refused.to_string().contains("not a regular file"),
            "{refused:#}"
        );
    }
}
