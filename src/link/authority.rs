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
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const AUTHORITY_KEY: &str = "authority.key";
pub const AUTHORITY_CERT: &str = "authority.crt";
pub const SERVER_KEY: &str = "server.key";
pub const SERVER_CERT: &str = "server.crt";

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

fn write_private(path: &Path, pem: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))?;
    file.write_all(pem.as_bytes())?;
    Ok(())
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

impl Authority {
    /// Create the authority, once. **Refuses to overwrite one that stands**:
    /// every client config pins the certificate, so replacing it is
    /// `rotate`'s and is by definition a re-registration of every agent.
    pub fn init(dir: &Path, server_name: &str, sans: &[String]) -> anyhow::Result<Self> {
        if dir.join(AUTHORITY_CERT).exists() || dir.join(AUTHORITY_KEY).exists() {
            anyhow::bail!(
                "an authority already stands at {}; `authority rotate` replaces it, and that re-registers every agent",
                dir.display()
            );
        }
        std::fs::create_dir_all(dir)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", dir.display()))?;
        Self::mint_into(dir, server_name, sans)
    }

    /// Replace the authority. The caller revokes every credential in the
    /// same act and says so in its answer.
    pub fn rotate(dir: &Path, server_name: &str, sans: &[String]) -> anyhow::Result<Self> {
        if !dir.join(AUTHORITY_CERT).exists() {
            anyhow::bail!(
                "no authority stands at {} to rotate; `authority init` creates one",
                dir.display()
            );
        }
        Self::mint_into(dir, server_name, sans)
    }

    fn mint_into(dir: &Path, server_name: &str, sans: &[String]) -> anyhow::Result<Self> {
        let key = KeyPair::generate()?;
        let params = authority_params();
        let certificate = params.self_signed(&key)?;
        write_private(&dir.join(AUTHORITY_KEY), &key.serialize_pem())?;
        std::fs::write(dir.join(AUTHORITY_CERT), certificate.pem())?;

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
        write_private(&dir.join(SERVER_KEY), &server_key.serialize_pem())?;
        std::fs::write(dir.join(SERVER_CERT), server_certificate.pem())?;
        Self::load(dir)
    }

    /// Load the authority that stands, or refuse. **The server calls this
    /// before its listener starts and mints nothing when it is absent.**
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        if !dir.join(AUTHORITY_CERT).exists() || !dir.join(AUTHORITY_KEY).exists() {
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
        Ok(Self {
            dir: dir.to_owned(),
            key,
            certificate_pem,
            certificate,
            server_certificate,
            server_key_pem,
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The authority's certificate, which every client config pins.
    pub fn certificate_pem(&self) -> &str {
        &self.certificate_pem
    }

    /// The authority certificate's fingerprint, for the verbs' answers.
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
    /// test below stages across two authorities.
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

        let rotated = Authority::rotate(&path, "weaver-web", &[]).unwrap();
        assert_ne!(rotated.fingerprint(), made.fingerprint());
        assert_eq!(
            Authority::load(&path).unwrap().fingerprint(),
            rotated.fingerprint()
        );
    }

    /// A minted client credential's fingerprint is SHA-256 over its DER and
    /// nothing of the key is in it.
    #[test]
    fn a_client_credential_is_fingerprinted_over_its_der() {
        let dir = tempfile::tempdir().unwrap();
        let authority = Authority::init(dir.path(), "weaver-web", &[]).unwrap();
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
