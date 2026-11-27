//! conforms: web-an-adopted-pair-connects
//! conforms: web-a-connectors-key-is-a-file-of-its-own
//!
//! **A connector's key and its config, made on the box** (Spec 8.2; the
//! brief of 2026-11-27, child 6): `request`, run as root, creates the
//! connector's key file new, its group the connector's and its mode 0640
//! set by descriptor before the first key byte is written, and prints the
//! certificate request; `adopt`, run unprivileged, checks the pairing's
//! answer against the request's public half and writes the connector's
//! config to a new file, naming the key file it never opens. A fixed root
//! program of the appliance's lays that config (the brief's install list).

use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer, UnixTime};
use serde::{Deserialize, Serialize};

use crate::link::frames::Plane;

/// The key as the TLS provider loads it, ECDSA alone.
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

/// **One PKCS #8 ECDSA P-256 PEM block, and nothing else**: what a key file
/// holds.
pub fn one_key(pem: &str) -> bool {
    let mut bytes = pem.as_bytes();
    let items: Vec<_> = rustls_pemfile::read_all(&mut bytes).collect();
    let [Ok(rustls_pemfile::Item::Pkcs8Key(_))] = items.as_slice() else {
        return false;
    };
    signing_key(pem).is_some()
        && rcgen::KeyPair::from_pem(pem)
            .is_ok_and(|k| k.algorithm() == &rcgen::PKCS_ECDSA_P256_SHA256)
}

/// **The certificate is over the key**, `adopt`'s `key_mismatch` check as
/// a connector runs it again at every load.
pub fn over_key(certificate_pem: &str, key_pem: &str) -> bool {
    let (Some(certificate), Some(key)) = (one_certificate(certificate_pem), signing_key(key_pem))
    else {
        return false;
    };
    rustls::sign::CertifiedKey::new(vec![certificate], key)
        .keys_match()
        .is_ok()
}

/// The DER prefix of a P-256 key's SubjectPublicKeyInfo, up to its point:
/// `SEQUENCE { SEQUENCE { id-ecPublicKey, prime256v1 }, BIT STRING 04 ... }`.
const P256_SPKI: [u8; 27] = [
    0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
    0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00, 0x04,
];

/// **The one P-256 public key a certificate or a request carries**, as its
/// SubjectPublicKeyInfo's bytes: `None` where there is none or more than
/// one, so a key compared is never a guess.
fn p256_key(der: &[u8]) -> Option<&[u8]> {
    let len = P256_SPKI.len() + 64;
    let mut found = der
        .windows(P256_SPKI.len())
        .enumerate()
        .filter(|(_, w)| *w == P256_SPKI)
        .map(|(at, _)| at);
    let at = found.next()?;
    if found.next().is_some() || at + len > der.len() {
        return None;
    }
    Some(&der[at..at + len])
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
    /// The key file's path holds a file or a link: `usage`.
    Exists(String),
    /// The command's own failure: `fault`.
    Fault(String),
}

/// **`request`'s key file** (Spec 8.2; the brief of 2026-11-27, child 6):
/// created new at `key_file`, never through a link; its group set to
/// `group` and its mode to 0640 by descriptor before the first key byte is
/// written, its owner the account that runs it, which is root; holding the
/// key's PEM alone, synced with its directory. Answers the certificate
/// request's PEM. A failure after the create removes the file, so a re-run
/// is not refused by it.
pub fn request(key_file: &Path, group: libc::gid_t) -> Result<String, RequestRefused> {
    use std::os::fd::AsRawFd;
    let fault = |what: &str, e: &dyn std::fmt::Display| {
        RequestRefused::Fault(format!("{what} {}: {e}", key_file.display()))
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(key_file)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => RequestRefused::Exists(format!(
                "{} exists: request makes a new key into a new file",
                key_file.display()
            )),
            _ => fault("creating", &e),
        })?;
    let made = (|| -> Result<String, RequestRefused> {
        let fd = file.as_raw_fd();
        // SAFETY: a descriptor this call opened; the owner left unchanged.
        if unsafe { libc::fchown(fd, libc::uid_t::MAX, group) } < 0 {
            return Err(fault(
                "setting the group of",
                &std::io::Error::last_os_error(),
            ));
        }
        // SAFETY: as above.
        if unsafe { libc::fchmod(fd, 0o640) } < 0 {
            return Err(fault(
                "setting the mode of",
                &std::io::Error::last_os_error(),
            ));
        }
        let (key_pem, request) = new_key().map_err(|e| fault("making the key for", &e))?;
        file.write_all(key_pem.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| fault("writing", &e))?;
        sync_parent(key_file).map_err(|e| fault("syncing the directory of", &e))?;
        Ok(request)
    })();
    if made.is_err() {
        let _ = std::fs::remove_file(key_file);
    }
    made
}

/// The directory that holds `path`, synced, so a new name survives a crash.
fn sync_parent(path: &Path) -> std::io::Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    std::fs::File::open(parent)?.sync_all()
}

/// The longest request line `pair` and `adopt` read: a connector's `request`
/// line is far inside it.
pub const REQUEST_LINE_BOUND: usize = 16 * 1024;

/// **The request line a connector's `request` printed**, read from `path`
/// never through a link, a regular file within its bound: the request's PEM.
pub fn read_request_line(path: &Path) -> Result<String, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| format!("opening {}: {e}", path.display()))?;
    let regular = file.metadata().map(|m| m.file_type().is_file());
    if !matches!(regular, Ok(true)) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let mut text = String::new();
    file.take(REQUEST_LINE_BOUND as u64 + 1)
        .read_to_string(&mut text)
        .map_err(|e| format!("reading {}: {e}", path.display()))?;
    #[derive(serde::Deserialize)]
    struct Line {
        event: String,
        request: String,
    }
    match serde_json::from_str::<Line>(text.trim_end()) {
        Ok(line)
            if line.event == "request"
                && line
                    .request
                    .starts_with("-----BEGIN CERTIFICATE REQUEST-----")
                && text.len() <= REQUEST_LINE_BOUND =>
        {
            Ok(line.request)
        }
        _ => Err(format!(
            "{} is not the request line a connector's request printed",
            path.display()
        )),
    }
}

/// The P-256 public key of a request's PEM, as its SubjectPublicKeyInfo.
fn request_key(request_pem: &str) -> Option<Vec<u8>> {
    let mut bytes = request_pem.as_bytes();
    match rustls_pemfile::read_one(&mut bytes).ok()?? {
        rustls_pemfile::Item::Csr(der) => p256_key(der.as_ref()).map(<[u8]>::to_vec),
        _ => None,
    }
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
    /// The gate's certificate, absent where the pairing enrolled admin-con
    /// alone, as every pairing does (Spec 8.2); `gate-con adopt` given such
    /// an answer refuses it as `usage`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
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

    /// The certificate the answer carries for `plane`, if it carries one.
    fn certificate(&self, plane: Plane) -> Option<&str> {
        match plane {
            Plane::Gate => self.gate.as_deref(),
            Plane::Admin => Some(&self.admin),
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

/// Why `adopt` wrote nothing.
#[derive(Debug)]
pub enum AdoptRefused {
    /// The request, the answer or a path refused before a byte is written:
    /// `usage`.
    Usage(String),
    /// The answer's certificate is not this connector's to adopt: `refused`,
    /// with the reason, and nothing written.
    Refused(&'static str),
    /// The command's own failure, or a write ended: `fault`.
    Fault(String),
}

/// `adopt`'s refusal: the answer's certificate is not over the request's
/// key.
pub const KEY_MISMATCH: &str = "key_mismatch";
/// `adopt`'s refusal: the answer's certificate does not verify under the
/// answer's authority.
pub const NOT_SIGNED: &str = "not_signed";

/// What `adopt` is given (Spec 8.2): the request line `request` printed,
/// the answer `pair` wrote, the key file the config names, and the config
/// to write.
pub struct Adopting<'a> {
    pub plane: Plane,
    pub request: &'a Path,
    pub answer: &'a Path,
    pub key_file: &'a Path,
    pub out: &'a Path,
}

/// **`adopt`** (Spec 8.2; the brief of 2026-11-27, child 6): the answer's
/// certificate for its plane checked against the request's public half and
/// the answer's authority, then the config written new at `out`, naming
/// `key_file`, which it never opens.
pub fn adopt(a: &Adopting<'_>) -> Result<AnswerFile, AdoptRefused> {
    adopt_at(a, UnixTime::now())
}

/// `adopt`, its certificate verified at `now`, the box's clock.
pub fn adopt_at(a: &Adopting<'_>, now: UnixTime) -> Result<AnswerFile, AdoptRefused> {
    let usage = |e: anyhow::Error| AdoptRefused::Usage(format!("{e:#}"));
    let plane = a.plane;
    if !a.key_file.is_absolute() {
        return Err(AdoptRefused::Usage(format!(
            "{}: key_file is not an absolute path",
            a.key_file.display()
        )));
    }
    let request_pem = read_request_line(a.request).map_err(AdoptRefused::Usage)?;
    let requested = request_key(&request_pem).ok_or_else(|| {
        AdoptRefused::Usage(format!(
            "{}: the request is not one P-256 certificate request",
            a.request.display()
        ))
    })?;
    let answered = AnswerFile::read(a.answer).map_err(usage)?;
    if answered.certificate(plane).is_none() {
        return Err(AdoptRefused::Usage(format!(
            "{}: the answer carries no certificate for the {} plane, which the pairing does not enroll",
            a.answer.display(),
            plane.as_str()
        )));
    }
    let (Some(authority), Some(certificate)) = (
        one_certificate(&answered.authority),
        answered.certificate(plane).and_then(one_certificate),
    ) else {
        return Err(AdoptRefused::Usage(format!(
            "{}: a certificate is not one PEM block",
            a.answer.display()
        )));
    };
    // The answer's certificate for this plane is over the request's key,
    // and verifies under the answer's authority as a client's.
    if p256_key(certificate.as_ref()) != Some(requested.as_slice()) {
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
    .map_err(|e| AdoptRefused::Fault(format!("building the verifier: {e}")))?;
    // **A box whose clock is outside the certificate's window is told so**,
    // rather than sent after a signature that holds: a certificate refused for
    // its time alone is verified again at the edge of its own window, and only
    // one that verifies there is the clock's fault.
    use rustls::CertificateError::{ExpiredContext, NotValidYetContext};
    match verifier.verify_client_cert(&certificate, &[], now) {
        Ok(_) => {}
        Err(rustls::Error::InvalidCertificate(NotValidYetContext { time, not_before })) => {
            return Err(clock(
                &*verifier,
                &certificate,
                plane,
                time,
                not_before,
                "valid from",
            ));
        }
        Err(rustls::Error::InvalidCertificate(ExpiredContext { time, not_after })) => {
            return Err(clock(
                &*verifier,
                &certificate,
                plane,
                time,
                not_after,
                "expired at",
            ));
        }
        Err(_) => return Err(AdoptRefused::Refused(NOT_SIGNED)),
    }
    write_config(a.out, &config_of(plane, &answered, a.key_file))?;
    Ok(answered)
}

/// **The config written new at `out`**, never through a link nor over an
/// entry, synced with its directory; removed where a write after the create
/// fails, so a re-run is not refused by a partial file. It holds
/// certificates and paths alone, no secret.
fn write_config(out: &Path, content: &str) -> Result<(), AdoptRefused> {
    use std::os::unix::fs::PermissionsExt;
    let fault = |what: &str, e: &dyn std::fmt::Display| {
        AdoptRefused::Fault(format!("{what} {}: {e}", out.display()))
    };
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .custom_flags(libc::O_NOFOLLOW)
        .open(out)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => AdoptRefused::Usage(format!(
                "{} exists: adopt writes the config into a new file",
                out.display()
            )),
            _ => fault("creating", &e),
        })?;
    let written = file
        .set_permissions(std::fs::Permissions::from_mode(0o644))
        .and_then(|()| file.write_all(content.as_bytes()))
        .and_then(|()| file.sync_all())
        .and_then(|()| sync_parent(out));
    written.map_err(|e| {
        let _ = std::fs::remove_file(out);
        fault("writing", &e)
    })
}

/// **A certificate refused for its time**: the box's clock's fault where the
/// certificate verifies at `edge`, the end of its window the clock passed,
/// naming both times, and `not_signed` otherwise.
fn clock(
    verifier: &dyn rustls::server::danger::ClientCertVerifier,
    certificate: &CertificateDer<'_>,
    plane: Plane,
    clock: UnixTime,
    edge: UnixTime,
    what: &str,
) -> AdoptRefused {
    if verifier.verify_client_cert(certificate, &[], edge).is_err() {
        return AdoptRefused::Refused(NOT_SIGNED);
    }
    let at = |t: UnixTime| {
        i64::try_from(t.as_secs())
            .ok()
            .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
            .map_or_else(|| format!("{} s", t.as_secs()), |d| d.to_rfc3339())
    };
    AdoptRefused::Fault(format!(
        "the answer's {} certificate is {what} {}, and the box's clock reads {}: set the box's clock and run adopt again",
        plane.as_str(),
        at(edge),
        at(clock)
    ))
}

/// **The config `adopt` writes**: the members `register` writes, and
/// `key_file`, so the connector's unit reads it unchanged (Spec 8.1).
fn config_of(plane: Plane, answer: &AnswerFile, key_file: &Path) -> String {
    let mut table = toml::Table::new();
    table.insert("server".into(), answer.server.clone().into());
    table.insert("server_name".into(), answer.server_name.clone().into());
    table.insert("agent".into(), answer.agent.clone().into());
    table.insert("agent_id".into(), answer.agent_id.clone().into());
    table.insert("plane".into(), plane.as_str().into());
    table.insert("server_certificate".into(), answer.authority.clone().into());
    table.insert(
        "certificate".into(),
        answer
            .certificate(plane)
            .expect("adopt checked the plane's certificate")
            .to_owned()
            .into(),
    );
    table.insert(
        "key_file".into(),
        key_file.to_string_lossy().into_owned().into(),
    );
    format!(
        "# Written by `{}-con adopt` from the pairing's answer. Its key is the file key_file\n\
         # names, which adopt never opened.\n{}",
        plane.as_str(),
        toml::to_string(&table).expect("a table of strings serializes")
    )
}
