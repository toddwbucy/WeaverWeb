//! conforms: web-an-adopted-pair-connects
//!
//! **A connector's key and its config, made on the box** (Spec 8.2): `request`
//! creates the config holding the connector's new key alone, under the
//! connector's own account, and `adopt` completes that same file in place
//! with the members `register` writes, once the answer's certificate for its
//! plane is over its key and verifies under the answer's authority.
//!
//! **`adopt` appends and never rewrites the key** (the brief's decision 5, as
//! amended on 2026-11-17). It finds the key's prefix by parsing the exact
//! bytes `request` writes, never a stored length, refuses a file whose prefix
//! is not a well-formed key-only config, and replaces only what follows the
//! prefix, so a crash at any write leaves the key byte for byte and a re-run
//! completes the file. A config left with no certificate is refused by the
//! service at its start, its `certificate` member missing.

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime};
use serde::{Deserialize, Serialize};

use crate::link::client::{CONFIG_BOUND, open_private};
use crate::link::frames::Plane;

/// The first line of a key-only config, naming the command that wrote it
/// and the plane, so the other connector's `adopt` refuses it.
fn header(plane: Plane) -> String {
    format!(
        "# Written by `{}-con request`: this connector's key alone, which `adopt` completes.\n",
        plane.as_str()
    )
}

const KEY_OPEN: &str = "key = \"\"\"\n";
const PEM_BEGIN: &str = "-----BEGIN PRIVATE KEY-----\n";
const PEM_END: &str = "-----END PRIVATE KEY-----\n";
const KEY_CLOSE: &str = "\"\"\"\n";

/// **The exact bytes `request` writes**: the header, then the key's PKCS #8
/// PEM as a TOML multi-line string. The PEM's alphabet holds no quote and no
/// backslash, so the string's value is the PEM unchanged.
pub fn key_only(plane: Plane, key_pem: &str) -> String {
    format!("{}{KEY_OPEN}{key_pem}{KEY_CLOSE}", header(plane))
}

/// **The key's prefix of a config**, parsed from the bytes `request` writes:
/// the key's PEM and the prefix's length, or why the file is not a
/// well-formed key-only config for this plane. Whatever follows is a tail
/// `adopt` may replace.
pub fn prefix_of(plane: Plane, content: &str) -> Result<(String, usize), &'static str> {
    let opening = format!("{}{KEY_OPEN}", header(plane));
    let rest = content
        .strip_prefix(opening.as_str())
        .ok_or("the config does not open as this plane's request wrote it")?;
    let close = format!("{PEM_END}{KEY_CLOSE}");
    let at = rest
        .find(close.as_str())
        .ok_or("the config's key is not closed as request wrote it")?;
    let key_pem = &rest[..at + PEM_END.len()];
    let body = key_pem
        .strip_prefix(PEM_BEGIN)
        .and_then(|b| b.strip_suffix(PEM_END))
        .ok_or("the config's key is not one PKCS #8 PEM block")?;
    if body.is_empty()
        || !body.lines().all(|l| {
            !l.is_empty()
                && l.len() <= 64
                && l.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
        })
        || !body.ends_with('\n')
    {
        return Err("the config's key is not one PKCS #8 PEM block");
    }
    // The prefix parses as a key-only config, and nothing else.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct KeyOnly {
        key: String,
    }
    let length = opening.len() + at + close.len();
    toml::from_str::<KeyOnly>(&content[..length])
        .map_err(|_| "the config's prefix is not a key-only config")?;
    signing_key(key_pem).ok_or("the config's key is not an ECDSA P-256 key")?;
    Ok((key_pem.to_owned(), length))
}

/// The key as the TLS provider loads it, ECDSA P-256 alone.
fn signing_key(key_pem: &str) -> Option<Arc<dyn rustls::sign::SigningKey>> {
    let der = rustls_pemfile::private_key(&mut key_pem.as_bytes()).ok()??;
    let PrivateKeyDer::Pkcs8(_) = &der else {
        return None;
    };
    let key = crate::link::authority::provider()
        .key_provider
        .load_private_key(der)
        .ok()?;
    (key.algorithm() == rustls::SignatureAlgorithm::ECDSA).then_some(key)
}

/// **A new key and its certificate request**, ECDSA P-256 with SHA-256, the
/// kind the console signs (Spec 8.2): the key's PKCS #8 PEM and the request's
/// PEM.
pub fn new_key() -> anyhow::Result<(String, String)> {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)?;
    // No subject: the console signs its own members over the key alone.
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new())?;
    params.distinguished_name = rcgen::DistinguishedName::new();
    let request = params.serialize_request(&key)?.pem()?;
    Ok((key.serialize_pem(), request))
}

/// Why `request` made nothing.
#[derive(Debug)]
pub enum RequestRefused {
    /// The config's path holds a file or a link: `usage`.
    Exists(String),
    /// The command's own failure: `fault`.
    Fault(String),
}

/// **`request`'s config** (Spec 8.2): created new at `path`, never through a
/// link, owner-only, holding the key alone, synced with its directory. Answers
/// the certificate request's PEM.
pub fn request(plane: Plane, path: &Path) -> Result<String, RequestRefused> {
    let fault = |what: &str, e: &dyn std::fmt::Display| {
        RequestRefused::Fault(format!("{what} {}: {e}", path.display()))
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => RequestRefused::Exists(format!(
                "{} exists: request makes a new key into a new file",
                path.display()
            )),
            _ => fault("creating", &e),
        })?;
    // The umask narrows the mode at the create, and `adopt` writes the file,
    // so the owner's read and write are set whatever it is.
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|e| fault("setting the mode of", &e))?;
    let (key_pem, request) = new_key().map_err(|e| fault("making the key for", &e))?;
    file.write_all(key_only(plane, &key_pem).as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|e| fault("writing", &e))?;
    sync_parent(path).map_err(|e| fault("syncing the directory of", &e))?;
    Ok(request)
}

/// The directory that holds `path`, synced, so a new name survives a crash.
fn sync_parent(path: &Path) -> std::io::Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::fs::File::open(parent)?.sync_all()
}

/// **The answer file** (Spec 8.2): what `pair` writes once the console
/// answers, and `adopt` reads: the address `pair` dialed, which is the
/// console's link address, and the answer's members. It holds certificates
/// alone, so it is readable by the connectors' accounts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerFile {
    pub server: String,
    pub server_name: String,
    pub agent: String,
    pub agent_id: String,
    #[serde(rename = "box")]
    pub r#box: String,
    pub authority: String,
    pub gate: String,
    pub admin: String,
}

/// The longest answer file `adopt` reads.
const ANSWER_BOUND: u64 = 64 * 1024;

impl AnswerFile {
    /// **Written new at `path`**, never through a link, mode 0644 whatever
    /// the umask, synced with its directory.
    pub fn write_new(&self, path: &Path) -> anyhow::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|e| anyhow::anyhow!("creating {}: {e}", path.display()))?;
        file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        sync_parent(path)?;
        Ok(())
    }

    /// **Read from `path`**, never through a link, a regular file within its
    /// bound, every member in its shape.
    pub fn read(path: &Path) -> anyhow::Result<AnswerFile> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| anyhow::anyhow!("opening {}: {e}", path.display()))?;
        if !file.metadata()?.file_type().is_file() {
            anyhow::bail!("{} is not a regular file", path.display());
        }
        let mut text = String::new();
        file.take(ANSWER_BOUND + 1).read_to_string(&mut text)?;
        if text.len() as u64 > ANSWER_BOUND {
            anyhow::bail!("{} passes {ANSWER_BOUND} bytes", path.display());
        }
        let answer: AnswerFile = serde_json::from_str(&text)
            .map_err(|_| anyhow::anyhow!("{} is not an answer pair wrote", path.display()))?;
        if !crate::store::identity::is_agent_id(&answer.agent_id)
            || crate::link::verbs::well_formed(&answer.r#box, &answer.agent).is_err()
            || rustls::pki_types::ServerName::try_from(answer.server_name.as_str()).is_err()
            || answer.server.is_empty()
            || answer.server.bytes().any(|b| !(0x21..0x7f).contains(&b))
        {
            anyhow::bail!("{} names a member out of its shape", path.display());
        }
        Ok(answer)
    }

    /// The certificate the answer carries for `plane`.
    fn certificate(&self, plane: Plane) -> &str {
        match plane {
            Plane::Gate => &self.gate,
            Plane::Admin => &self.admin,
        }
    }
}

/// One certificate in PEM, alone, as DER.
fn one_certificate(pem: &str) -> Option<CertificateDer<'static>> {
    let mut bytes = pem.as_bytes();
    let mut certificates = rustls_pemfile::certs(&mut bytes);
    let first = certificates.next()?.ok()?;
    certificates.next().is_none().then_some(first)
}

/// **Where a write of `adopt` stands**, for the tests that end it at each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boundary {
    /// The config opened and checked, nothing written.
    Opened,
    /// The tail written into the middle of the certificate.
    MidCertificate,
    /// The tail written whole, its sync still to come.
    BeforeSync,
}

/// Why `adopt` wrote nothing more.
#[derive(Debug)]
pub enum AdoptRefused {
    /// The config or the answer refused before a byte is written: `usage`.
    Usage(String),
    /// The answer's certificate is not this connector's to adopt: `refused`,
    /// with the reason, and nothing written.
    Refused(&'static str),
    /// The command's own failure, or a write ended: `fault`.
    Fault(String),
}

/// `adopt`'s refusal: the answer's certificate is not over this key.
pub const KEY_MISMATCH: &str = "key_mismatch";
/// `adopt`'s refusal: the answer's certificate does not verify under the
/// answer's authority.
pub const NOT_SIGNED: &str = "not_signed";

/// **`adopt`** (Spec 8.2): the config opened by the rule the connectors apply
/// at their start, its key's prefix parsed, the answer's certificate for its
/// plane checked against the key and the answer's authority, and the members
/// `register` writes appended after the prefix.
pub fn adopt(plane: Plane, config: &Path, answer: &Path) -> Result<AnswerFile, AdoptRefused> {
    adopt_with(plane, config, answer, &mut |_| Ok(()))
}

/// `adopt`, answering the answer it adopted, with `at` called at each write boundary: an error from it ends
/// the write there, as a crash would.
pub fn adopt_with(
    plane: Plane,
    config: &Path,
    answer: &Path,
    at: &mut dyn FnMut(Boundary) -> std::io::Result<()>,
) -> Result<AnswerFile, AdoptRefused> {
    let usage = |e: anyhow::Error| AdoptRefused::Usage(format!("{e:#}"));
    let fault = |what: &str, e: &dyn std::fmt::Display| {
        AdoptRefused::Fault(format!("{what} {}: {e}", config.display()))
    };
    let mut file = open_private(config, true).map_err(usage)?;
    let mut content = String::new();
    (&mut file)
        .take(CONFIG_BOUND + 1)
        .read_to_string(&mut content)
        .map_err(|e| AdoptRefused::Usage(format!("reading {}: {e}", config.display())))?;
    if content.len() as u64 > CONFIG_BOUND {
        return Err(AdoptRefused::Usage(format!(
            "{} passes {CONFIG_BOUND} bytes and is not a connector's config",
            config.display()
        )));
    }
    let (key_pem, prefix) = prefix_of(plane, &content)
        .map_err(|why| AdoptRefused::Usage(format!("{}: {why}", config.display())))?;
    let answered = AnswerFile::read(answer).map_err(usage)?;

    // The answer's certificate for this plane is over this key, and verifies
    // under the answer's authority as a client's.
    let (Some(authority), Some(certificate)) = (
        one_certificate(&answered.authority),
        one_certificate(answered.certificate(plane)),
    ) else {
        return Err(AdoptRefused::Usage(format!(
            "{}: a certificate is not one PEM block",
            answer.display()
        )));
    };
    let key = signing_key(&key_pem)
        .ok_or_else(|| AdoptRefused::Usage("the config's key does not load".into()))?;
    let certified = rustls::sign::CertifiedKey::new(vec![certificate.clone()], key);
    if certified.keys_match().is_err() {
        return Err(AdoptRefused::Refused(KEY_MISMATCH));
    }
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(authority)
        .map_err(|_| AdoptRefused::Refused(NOT_SIGNED))?;
    let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
        Arc::new(roots),
        crate::link::authority::provider(),
    )
    .build()
    .map_err(|e| fault("building the verifier for", &e))?;
    if verifier
        .verify_client_cert(&certificate, &[], UnixTime::now())
        .is_err()
    {
        return Err(AdoptRefused::Refused(NOT_SIGNED));
    }

    let tail = tail(plane, &answered);
    at(Boundary::Opened).map_err(|e| fault("ended after opening", &e))?;
    // **Only what follows the prefix is replaced**: a re-run's truncation
    // stops at the prefix's end, and the write starts there.
    file.set_len(prefix as u64)
        .map_err(|e| fault("truncating the tail of", &e))?;
    file.seek(SeekFrom::Start(prefix as u64))
        .map_err(|e| fault("seeking in", &e))?;
    let middle = tail
        .find("-----BEGIN CERTIFICATE-----")
        .map_or(tail.len() / 2, |at| at + 40);
    file.write_all(&tail.as_bytes()[..middle])
        .map_err(|e| fault("writing", &e))?;
    at(Boundary::MidCertificate).map_err(|e| fault("ended mid-certificate in", &e))?;
    file.write_all(&tail.as_bytes()[middle..])
        .map_err(|e| fault("writing", &e))?;
    at(Boundary::BeforeSync).map_err(|e| fault("ended before syncing", &e))?;
    file.sync_all().map_err(|e| fault("syncing", &e))?;
    Ok(answered)
}

/// **What `adopt` appends**: the members `register` writes beside the key,
/// so the connector's unit reads the config unchanged (Spec 8.1).
fn tail(plane: Plane, answer: &AnswerFile) -> String {
    let mut table = toml::Table::new();
    table.insert("server".into(), answer.server.clone().into());
    table.insert("server_name".into(), answer.server_name.clone().into());
    table.insert("agent".into(), answer.agent.clone().into());
    table.insert("agent_id".into(), answer.agent_id.clone().into());
    table.insert("plane".into(), plane.as_str().into());
    table.insert("server_certificate".into(), answer.authority.clone().into());
    table.insert(
        "certificate".into(),
        answer.certificate(plane).to_owned().into(),
    );
    format!(
        "# Completed by `{}-con adopt` from the pairing's answer.\n{}",
        plane.as_str(),
        toml::to_string(&table).expect("a table of strings serializes")
    )
}
