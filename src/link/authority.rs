//! conforms: web-servers-authority-is-loaded-and-never-minted-at-start
//! conforms: web-nothing-crosses-the-link-in-the-clear
//! conforms: web-client-credential-stored-as-fingerprint-never-key
//!
//! The server's authority (Spec section 8): its key and certificate,
//! created once by the first register verb, loaded before the listener
//! starts, and never minted at start. It signs the server's own certificate
//! and one client certificate per connector, two per agent. The server
//! stores a client certificate's fingerprint, SHA-256 over its DER, and
//! never the key.
//!
//! **The authority is one atomic state.** Its five files, the authority's
//! key and certificate, the server's key and certificate, and the name the
//! server's certificate is verified under, are written as a set into a
//! staging directory beside the live one, verified as a set (the server's
//! certificate verifies under the authority's, and a certificate the key
//! signs verifies under it), and switched into place by renaming the
//! directory; a set written in place could be left half old and half new
//! by a failure between two writes, and `load` would parse the halves
//! without noticing. `load` verifies the set the same way and refuses one
//! that does not hold together. Replacing a standing set renames it aside
//! first and the new set into place second, so the old set stays whole
//! through the switch. **Each invocation stages into its own directory**,
//! named with the process id and a nonce beside the live path, and `init`
//! switches with a rename that refuses to replace: two inits racing
//! (`init` takes no lock, since there is no store to lock on) cannot rename
//! each other's partial sets, the second fails cleanly with the first's
//! set intact, and the loser's staging is removed. (An empty directory at
//! the path, as an operator may make, is removed before the switch; a
//! non-empty one is what the refusal names.) `rotate` runs under the
//! authority lock and keeps its retire-then-switch shape.
//!
//! **The only accept path is mutual TLS with this authority as the sole
//! trust root for client certificates**, which `server_tls` builds and
//! nothing else in the crate can bypass: there is no plaintext listener.
//! The client config carries its own key and certificate, the server's
//! certificate, and the name the server's certificate is verified under,
//! so the address a connector dials and the identity it checks are two
//! facts rather than one.

use crate::link::frames::Plane;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    Issuer, KeyPair, KeyUsagePurpose,
};
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const AUTHORITY_KEY: &str = "authority.key";
pub const AUTHORITY_CERT: &str = "authority.crt";
pub const SERVER_KEY: &str = "server.key";
pub const SERVER_CERT: &str = "server.crt";
/// The name the server's certificate carries and is verified under, kept
/// with the set so `load` can verify the set without being told it.
pub const SERVER_NAME: &str = "server_name";

/// The authority's own name. **Fixed, because the issuer is rebuilt from
/// it**: rcgen signs a leaf from the issuer's distinguished name and key,
/// so the name the authority's certificate carries and the name every leaf
/// names as its issuer must be one constant rather than a value read back
/// from the certificate, which would want a parser this crate does not
/// otherwise carry.
const AUTHORITY_NAME: &str = "weaver-web authority";

/// The loaded authority: what signs, what the server presents, and what a
/// client pins.
pub struct Authority {
    dir: PathBuf,
    key: KeyPair,
    certificate_pem: String,
    certificate: CertificateDer<'static>,
    server_certificate: CertificateDer<'static>,
    server_key_pem: String,
    server_name: String,
}

/// A client credential as minted: the certificate and its key for the
/// client's config, and the fingerprint the register stores. **The key
/// leaves this process in the client's config and in nothing else.**
pub struct ClientCredential {
    pub fingerprint: String,
    pub certificate_pem: String,
    pub key_pem: String,
}

/// SHA-256 over a certificate's DER, as sixty-four hex digits.
pub fn fingerprint(der: &[u8]) -> String {
    format!("{:x}", Sha256::digest(der))
}

fn authority_params() -> CertificateParams {
    let mut params = CertificateParams::new(Vec::<String>::new()).expect("no names to validate");
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, AUTHORITY_NAME);
    params.distinguished_name = dn;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
}

// **Every file of the set is synced, then the staging directory, then the
// parent after the switch**: the switch is reported only once the set and
// its names would survive a power loss, since an authority that is in
// place by name but empty on disk is a server that cannot start and
// configs that pin a certificate nothing holds.
fn write_private(path: &Path, pem: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
    file.write_all(pem.as_bytes())?;
    file.sync_all()
        .map_err(|e| anyhow::anyhow!("syncing {}: {e}", path.display()))?;
    Ok(())
}

fn write_synced(path: &Path, content: &str) -> anyhow::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
    file.write_all(content.as_bytes())?;
    file.sync_all()
        .map_err(|e| anyhow::anyhow!("syncing {}: {e}", path.display()))?;
    Ok(())
}

/// Flush a directory's entries to disk.
fn sync_dir(dir: &Path) -> anyhow::Result<()> {
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| anyhow::anyhow!("syncing {}: {e}", dir.display()))
}

fn read(dir: &Path, name: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(dir.join(name))
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", dir.join(name).display()))
}

fn first_certificate(pem: &str) -> anyhow::Result<CertificateDer<'static>> {
    rustls_pemfile::certs(&mut pem.as_bytes())
        .next()
        .ok_or_else(|| anyhow::anyhow!("no certificate in the PEM"))?
        .map_err(|e| anyhow::anyhow!("certificate PEM did not parse: {e}"))
}

fn private_key(pem: &str) -> anyhow::Result<PrivateKeyDer<'static>> {
    rustls_pemfile::private_key(&mut pem.as_bytes())
        .map_err(|e| anyhow::anyhow!("key PEM did not parse: {e}"))?
        .ok_or_else(|| anyhow::anyhow!("no private key in the PEM"))
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// A sibling of the authority's directory, named for its role.
fn sibling(dir: &Path, role: &str) -> PathBuf {
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "authority".into());
    dir.with_file_name(format!("{name}.{role}"))
}

/// A staging directory of this invocation's own, beside the live path.
fn staging_sibling(dir: &Path) -> PathBuf {
    static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    sibling(
        dir,
        &format!("staging-{}-{n}-{nanos:x}", std::process::id()),
    )
}

/// Whether a staging directory of any invocation stands beside the live
/// path.
#[cfg(test)]
pub(crate) fn staging_stands(dir: &Path) -> bool {
    let prefix = format!(
        "{}.staging",
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "authority".into())
    );
    dir.parent()
        .and_then(|parent| std::fs::read_dir(parent).ok())
        .into_iter()
        .flatten()
        .flatten()
        .any(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
}

/// Rename a directory into place, refusing to replace anything there.
fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::other("a path with a NUL"))?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::other("a path with a NUL"))?;
    // SAFETY: two NUL-terminated paths relative to the working directory.
    let r = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if r < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn stands(dir: &Path) -> bool {
    dir.join(AUTHORITY_CERT).exists() || dir.join(AUTHORITY_KEY).exists()
}

/// A switch that failed between its two renames: the retired set stands
/// beside an absent live directory. `rotate` restores it and rotates;
/// `load` and `init` name the state rather than acting on half of it.
fn switch_failed(dir: &Path) -> bool {
    !dir.exists() && stands(&sibling(dir, "retired"))
}

fn switch_failed_message(dir: &Path) -> String {
    format!(
        "a previous switch of the authority failed: the retired set stands at {} beside an absent {}; run `weaver-web authority rotate`, which restores it and rotates",
        sibling(dir, "retired").display(),
        dir.display()
    )
}

// Two test levers, each firing once on the thread that set it, so parallel
// tests do not trip each other's: the switch's second rename fails, and a
// whole second `init` runs between this one's verification and its switch.
#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_SWITCH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static CONCURRENT_INIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static FAIL_MINT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

impl Authority {
    /// Create the authority, once. **Refuses to overwrite one that stands**:
    /// every client config pins the certificate, so replacing it is
    /// `rotate`'s and is by definition a re-registration of every agent.
    pub fn init(dir: &Path, server_name: &str, sans: &[String]) -> anyhow::Result<Self> {
        if switch_failed(dir) {
            anyhow::bail!("{}", switch_failed_message(dir));
        }
        if stands(dir) {
            anyhow::bail!(
                "an authority already stands at {}; `authority rotate` replaces it, and that re-registers every agent",
                dir.display()
            );
        }
        Self::mint_into(dir, server_name, sans, false)
    }

    /// Replace the authority. The caller revokes every credential in the
    /// same act and says so in its answer.
    pub fn rotate(dir: &Path, server_name: &str, sans: &[String]) -> anyhow::Result<Self> {
        if switch_failed(dir) {
            let retired = sibling(dir, "retired");
            std::fs::rename(&retired, dir).map_err(|e| {
                anyhow::anyhow!("restoring {} to {}: {e}", retired.display(), dir.display())
            })?;
            tracing::warn!(
                "a previous switch had failed: the retired set at {} is restored before this rotation",
                dir.display()
            );
        }
        if !stands(dir) {
            anyhow::bail!(
                "no authority stands at {} to rotate; `authority init` creates one",
                dir.display()
            );
        }
        Self::mint_into(dir, server_name, sans, true)
    }

    /// Mint a whole set into this invocation's staging directory, verify
    /// it by loading it, and switch it into place by renaming the
    /// directory: replacing the set that stands for a rotation, refusing to
    /// replace anything for an init.
    fn mint_into(
        dir: &Path,
        server_name: &str,
        sans: &[String],
        replace: bool,
    ) -> anyhow::Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        if let Some(parent) = dir.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("creating {}: {e}", parent.display()))?;
        }
        let staging = staging_sibling(dir);
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&staging)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", staging.display()))?;

        // **Every failure before the switch removes this invocation's
        // staging directory**: a key generation, a certificate construction
        // or a write that fails would otherwise leave key material behind
        // and a directory per retry.
        let minted = (|| -> anyhow::Result<()> {
            let key = KeyPair::generate()?;
            let params = authority_params();
            let certificate = params.self_signed(&key)?;
            write_private(&staging.join(AUTHORITY_KEY), &key.serialize_pem())?;
            #[cfg(test)]
            if FAIL_MINT.with(|f| f.replace(false)) {
                anyhow::bail!("a test fault made the mint fail");
            }
            write_synced(&staging.join(AUTHORITY_CERT), &certificate.pem())?;

            let issuer = Issuer::from_params(&params, &key);
            let server_key = KeyPair::generate()?;
            let mut names = vec![server_name.to_owned()];
            names.extend(sans.iter().cloned());
            let mut server_params = CertificateParams::new(names)?;
            let mut dn = DistinguishedName::new();
            dn.push(DnType::CommonName, server_name);
            server_params.distinguished_name = dn;
            server_params.is_ca = IsCa::ExplicitNoCa;
            server_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
            server_params.key_usages = vec![
                KeyUsagePurpose::DigitalSignature,
                KeyUsagePurpose::KeyEncipherment,
            ];
            let server_certificate = server_params.signed_by(&server_key, &issuer)?;
            write_private(&staging.join(SERVER_KEY), &server_key.serialize_pem())?;
            write_synced(&staging.join(SERVER_CERT), &server_certificate.pem())?;
            write_synced(&staging.join(SERVER_NAME), server_name)?;
            Ok(())
        })();
        // The set is verified where it was written, before anything live
        // moves, and its names are synced before they are renamed.
        if let Err(e) = minted
            .and_then(|_| Self::load(&staging).map(|_| ()))
            .and_then(|_| sync_dir(&staging))
        {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(e);
        }

        #[cfg(test)]
        if !replace && CONCURRENT_INIT.with(|f| f.replace(false)) {
            Self::init(dir, server_name, sans)?;
        }

        let retired = sibling(dir, "retired");
        let mut retired_now = false;
        if replace && dir.exists() {
            if retired.exists() {
                std::fs::remove_dir_all(&retired)?;
            }
            std::fs::rename(dir, &retired).map_err(|e| {
                anyhow::anyhow!("retiring {} to {}: {e}", dir.display(), retired.display())
            })?;
            retired_now = true;
        }
        #[cfg(test)]
        let switched = if FAIL_SWITCH.with(|f| f.replace(false)) {
            Err(std::io::Error::other("a test fault made the switch fail"))
        } else if replace {
            std::fs::rename(&staging, dir)
        } else {
            rename_noreplace(&staging, dir)
        };
        #[cfg(not(test))]
        let switched = if replace {
            std::fs::rename(&staging, dir)
        } else {
            rename_noreplace(&staging, dir)
        };
        // An empty directory the operator made at the path is not an
        // authority: it is removed (which fails on anything else) and the
        // switch tried once more, still refusing to replace.
        let switched = match switched {
            Err(e) if !replace && e.kind() == std::io::ErrorKind::AlreadyExists => {
                match std::fs::remove_dir(dir) {
                    Ok(()) => rename_noreplace(&staging, dir),
                    Err(rm) if rm.kind() == std::io::ErrorKind::DirectoryNotEmpty => Err(e),
                    Err(rm) => Err(rm),
                }
            }
            other => other,
        };
        if let Err(e) = switched {
            // The window between the two renames has no authority at the
            // path: the set retired a moment ago goes back before the
            // error is answered, so the server can start and `rotate` can
            // run again.
            let _ = std::fs::remove_dir_all(&staging);
            if retired_now {
                std::fs::rename(&retired, dir).map_err(|restore| {
                    anyhow::anyhow!(
                        "switching the new set into place at {} failed ({e}) and restoring the retired set failed too ({restore}): {}",
                        dir.display(),
                        switch_failed_message(dir)
                    )
                })?;
                anyhow::bail!(
                    "switching the new set into place at {} failed: {e}; the previous authority is restored and nothing changed",
                    dir.display()
                );
            }
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                anyhow::bail!(
                    "an authority was created at {} by a concurrent init while this one was minting; this one is discarded and the one that stands is kept",
                    dir.display()
                );
            }
            anyhow::bail!("switching the new set into place at {}: {e}", dir.display());
        }
        // The staging, the live and the retired paths are siblings, so the
        // one parent holds both sides of each rename; synced once the
        // switch is complete, before it is reported.
        if let Some(parent) = dir.parent()
            && !parent.as_os_str().is_empty()
        {
            sync_dir(parent).map_err(|e| {
                anyhow::anyhow!(
                    "the new set stands at {} but its name may not survive a power loss: {e}",
                    dir.display()
                )
            })?;
        }
        Self::load(dir)
    }

    /// Load the authority that stands, or refuse. **The server calls this
    /// before its listener starts and mints nothing when it is absent.**
    /// The set is verified as a set: a half-replaced one is refused.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        if switch_failed(dir) {
            anyhow::bail!("{}", switch_failed_message(dir));
        }
        if !stands(dir) {
            anyhow::bail!(
                "no authority stands at {}: run `weaver-web authority init` first; the server never mints one at start, since every installed connector pins it",
                dir.display()
            );
        }
        let key = KeyPair::from_pem(&read(dir, AUTHORITY_KEY)?)?;
        let certificate_pem = read(dir, AUTHORITY_CERT)?;
        let certificate = first_certificate(&certificate_pem)?;
        let server_certificate = first_certificate(&read(dir, SERVER_CERT)?)?;
        let server_key_pem = read(dir, SERVER_KEY)?;
        private_key(&server_key_pem)?;
        let server_name = read(dir, SERVER_NAME)?.trim().to_owned();
        let authority = Self {
            dir: dir.to_owned(),
            key,
            certificate_pem,
            certificate,
            server_certificate,
            server_key_pem,
            server_name,
        };
        authority.verify().map_err(|e| {
            anyhow::anyhow!(
                "the authority at {} does not hold together: {e:#}",
                dir.display()
            )
        })?;
        Ok(authority)
    }

    /// The set holds together: the server's certificate verifies under the
    /// authority's as the stored name, and a certificate the authority's
    /// key signs verifies under the authority's certificate, which is what
    /// ties the key to the certificate without parsing either.
    fn verify(&self) -> anyhow::Result<()> {
        let provider = provider();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.certificate.clone())?;
        let roots = Arc::new(roots);
        let name = ServerName::try_from(self.server_name.clone())
            .map_err(|e| anyhow::anyhow!("the stored server name is not a server name: {e}"))?;
        let servers = rustls::client::WebPkiServerVerifier::builder_with_provider(
            roots.clone(),
            provider.clone(),
        )
        .build()?;
        servers
            .verify_server_cert(&self.server_certificate, &[], &name, &[], UnixTime::now())
            .map_err(|e| {
                anyhow::anyhow!(
                    "the server certificate does not verify under the authority as {}: {e}",
                    self.server_name
                )
            })?;
        let clients =
            rustls::server::WebPkiClientVerifier::builder_with_provider(roots, provider).build()?;
        let probe = self.mint_client("verification", Plane::Gate)?;
        let probe_der = first_certificate(&probe.certificate_pem)?;
        clients
            .verify_client_cert(&probe_der, &[], UnixTime::now())
            .map_err(|e| {
                anyhow::anyhow!("the authority's key does not sign under its certificate: {e}")
            })?;
        Ok(())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The name the server's certificate is verified under.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// The authority's certificate, which every client config pins.
    pub fn certificate_pem(&self) -> &str {
        &self.certificate_pem
    }

    /// The authority certificate's fingerprint, for the verbs' answers and
    /// for a verb's check that the set on disk is the one it loaded.
    pub fn fingerprint(&self) -> String {
        fingerprint(self.certificate.as_ref())
    }

    /// Mint one client certificate, bound to one agent and one plane by
    /// its subject. The binding the server acts on is the fingerprint's row
    /// in the register, not the subject; the subject is for a human reading
    /// the file.
    pub fn mint_client(&self, agent: &str, plane: Plane) -> anyhow::Result<ClientCredential> {
        let params = authority_params();
        let issuer = Issuer::from_params(&params, &self.key);
        let key = KeyPair::generate()?;
        let mut client = CertificateParams::new(Vec::<String>::new())?;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, agent);
        dn.push(DnType::OrganizationalUnitName, plane.as_str());
        client.distinguished_name = dn;
        client.is_ca = IsCa::ExplicitNoCa;
        client.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        client.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        let certificate = client.signed_by(&key, &issuer)?;
        Ok(ClientCredential {
            fingerprint: fingerprint(certificate.der().as_ref()),
            certificate_pem: certificate.pem(),
            key_pem: key.serialize_pem(),
        })
    }

    /// The server's TLS configuration: mutual TLS, a client certificate
    /// required and verified against this authority alone. **This is the
    /// listener's only accept path.**
    pub fn server_tls(&self) -> anyhow::Result<Arc<rustls::ServerConfig>> {
        let provider = provider();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(self.certificate.clone())?;
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            provider.clone(),
        )
        .build()?;
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()?
            .with_client_cert_verifier(verifier)
            .with_single_cert(
                vec![self.server_certificate.clone()],
                private_key(&self.server_key_pem)?,
            )?;
        Ok(Arc::new(config))
    }
}

/// A connector's TLS configuration from the three PEMs its config carries:
/// the authority it pins, its own certificate and its own key. Used by this
/// crate's tests as the fake connectors, and by acts 3 and 4 as the
/// connectors' own.
pub fn client_tls(
    authority_pem: &str,
    certificate_pem: &str,
    key_pem: &str,
) -> anyhow::Result<Arc<rustls::ClientConfig>> {
    let provider = provider();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(first_certificate(authority_pem)?)?;
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_client_auth_cert(
            vec![first_certificate(certificate_pem)?],
            private_key(key_pem)?,
        )?;
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The authority is loaded and never minted at start.** An absent
    /// directory refuses, `init` creates one, a second `init` refuses to
    /// overwrite it, and a load answers the same certificate `init` wrote.
    ///
    /// Perturbation: have the server mint an authority when none stands.
    /// A restart then mints another, and every connector's hello is
    /// refused against a certificate it does not pin, which the TLS pair
    /// test in `tests.rs` stages across two authorities.
    ///
    /// conforms: web-servers-authority-is-loaded-and-never-minted-at-start
    #[test]
    fn the_authority_is_loaded_and_never_minted_at_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority");
        let refused = Authority::load(&path).map(|_| ()).unwrap_err().to_string();
        assert!(refused.contains("never mints one at start"), "{refused}");

        let made = Authority::init(&path, "weaver-web", &[]).unwrap();
        let again = Authority::init(&path, "weaver-web", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(again.contains("already stands"), "{again}");

        let loaded = Authority::load(&path).unwrap();
        assert_eq!(loaded.fingerprint(), made.fingerprint());
        assert_eq!(loaded.certificate_pem(), made.certificate_pem());
        assert_eq!(loaded.server_name(), "weaver-web");

        let rotated = Authority::rotate(&path, "weaver-web", &[]).unwrap();
        assert_ne!(rotated.fingerprint(), made.fingerprint());
        assert_eq!(
            Authority::load(&path).unwrap().fingerprint(),
            rotated.fingerprint()
        );
        // The set replaced stands whole beside the new one, and nothing of
        // the staging remains.
        let retired = Authority::load(&sibling(&path, "retired")).unwrap();
        assert_eq!(retired.fingerprint(), made.fingerprint());
        assert!(!staging_stands(&path));
    }

    /// **A failed mint leaves no staging behind**: a failure between the
    /// staging directory's creation and the switch removes the directory
    /// with whatever key material it held, and the next init starts clean.
    #[test]
    fn a_failed_mint_leaves_no_staging_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority");
        FAIL_MINT.with(|f| f.set(true));
        let refused = Authority::init(&path, "weaver-web", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(refused.contains("test fault"), "{refused}");
        assert!(!staging_stands(&path), "the staging directory is removed");
        assert!(!path.exists());
        Authority::init(&path, "weaver-web", &[]).unwrap();
    }

    /// **Two inits racing leave one whole authority.** A second init that
    /// lands between this one's verification and its switch wins: the
    /// switch refuses to replace it, this one is discarded with its
    /// staging removed, and the set that stands loads whole.
    #[test]
    fn two_inits_racing_leave_one_whole_authority() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority");
        CONCURRENT_INIT.with(|f| f.set(true));
        let refused = Authority::init(&path, "weaver-web", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(refused.contains("concurrent init"), "{refused}");
        let standing = Authority::load(&path).unwrap();
        assert_eq!(standing.server_name(), "weaver-web");
        assert!(!staging_stands(&path), "no staging of either init remains");
        assert!(!sibling(&path, "retired").exists());
        // A plain init afterwards still refuses to overwrite it.
        let refused = Authority::init(&path, "weaver-web", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(refused.contains("already stands"), "{refused}");
    }

    /// **The authority is one atomic state**: a set whose halves do not
    /// hold together is refused by `load`, which is what the staging
    /// directory and the renamed switch protect.
    #[test]
    fn a_half_replaced_authority_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        Authority::init(&a, "weaver-web", &[]).unwrap();
        Authority::init(&b, "weaver-web", &[]).unwrap();
        // b's server certificate under a's authority: the server half of
        // one set and the authority half of another.
        std::fs::copy(b.join(SERVER_CERT), a.join(SERVER_CERT)).unwrap();
        std::fs::copy(b.join(SERVER_KEY), a.join(SERVER_KEY)).unwrap();
        let refused = Authority::load(&a).map(|_| ()).unwrap_err().to_string();
        assert!(refused.contains("does not hold together"), "{refused}");
        assert!(refused.contains("server certificate"), "{refused}");

        // a's key with b's certificate: the key does not sign under it.
        let c = dir.path().join("c");
        Authority::init(&c, "weaver-web", &[]).unwrap();
        std::fs::copy(b.join(AUTHORITY_CERT), c.join(AUTHORITY_CERT)).unwrap();
        let refused = Authority::load(&c).map(|_| ()).unwrap_err().to_string();
        assert!(refused.contains("does not hold together"), "{refused}");
    }

    /// **A failed switch restores the old authority**: where the second
    /// rename fails the retired set goes back before the error is
    /// answered, and a switch that was left half done (the retired set
    /// beside an absent live directory) is named by `load` and `init` and
    /// repaired by `rotate`.
    #[test]
    fn a_failed_switch_restores_the_old_authority() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authority");
        let made = Authority::init(&path, "weaver-web", &[]).unwrap();

        FAIL_SWITCH.with(|f| f.set(true));
        let refused = Authority::rotate(&path, "weaver-web", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("the previous authority is restored"),
            "{refused}"
        );
        assert_eq!(
            Authority::load(&path).unwrap().fingerprint(),
            made.fingerprint()
        );
        assert!(!staging_stands(&path));

        // A switch left half done by a crash between the renames.
        std::fs::rename(&path, sibling(&path, "retired")).unwrap();
        let named = Authority::load(&path).map(|_| ()).unwrap_err().to_string();
        assert!(
            named.contains("previous switch of the authority failed"),
            "{named}"
        );
        let named = Authority::init(&path, "weaver-web", &[])
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(
            named.contains("previous switch of the authority failed"),
            "{named}"
        );
        let rotated = Authority::rotate(&path, "weaver-web", &[]).unwrap();
        assert_ne!(rotated.fingerprint(), made.fingerprint());
        assert_eq!(
            Authority::load(&sibling(&path, "retired"))
                .unwrap()
                .fingerprint(),
            made.fingerprint(),
            "the restored set is what this rotation retired"
        );
    }

    /// A minted client credential's fingerprint is SHA-256 over its DER and
    /// nothing of the key is in it.
    #[test]
    fn a_client_credential_is_fingerprinted_over_its_der() {
        let dir = tempfile::tempdir().unwrap();
        let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
        let minted = authority
            .mint_client("ag-0123456789abcdef", Plane::Gate)
            .unwrap();
        let der = first_certificate(&minted.certificate_pem).unwrap();
        assert_eq!(minted.fingerprint, fingerprint(der.as_ref()));
        assert_eq!(minted.fingerprint.len(), 64);
        assert!(minted.key_pem.contains("PRIVATE KEY"));
        assert!(!minted.certificate_pem.contains("PRIVATE KEY"));
    }
}
