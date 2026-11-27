//! The link's client half (Spec section 8): what gate-con runs now and
//! admin-con will run in act 4, each adding only its plane. It reads a
//! connector's config under the trust rule a file carrying a key is owed,
//! dials the server, verifies it under the authority's persisted name and
//! never the dialed address, sends the hello, heartbeats at the cadence the
//! answer names, and reconnects under the policy Spec section 8 states for
//! the client side.
//!
//! **Every read is bounded per read, every write against the cadence, and
//! every task is joined.** Frames are read through `frames::LineReader`, the
//! one reader both halves share. A connection's frames go out through one
//! writer task over a bounded channel, each write held to the cadence, so a
//! server that stops taking bytes ends the connection rather than
//! suspending a task; the heartbeat rides the same channel. `close` stops
//! the heartbeat, drains the writer within the cadence and joins both, and
//! a connection dropped without `close` aborts both, so no task of a
//! connection outlives it.

use crate::link::authority::client_tls;
use crate::link::entries::Entries;
use crate::link::frames::{
    CADENCE_MAX_SECS, FromClient, Line, LineReader, Plane, Position, Refusal, ToClient,
};
use serde::Deserialize;
use std::future::Future;
use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// The bound on a connector's config file: it holds two certificates and
/// a few members, a few KiB, so anything near this is not a config.
pub(crate) const CONFIG_BOUND: u64 = 1024 * 1024;

/// The bound on a connector's key file: one PEM block of a P-256 key, a few
/// hundred bytes.
pub(crate) const KEY_BOUND: u64 = 64 * 1024;

/// The bound on dialing, the handshake, the hello and its answer: the
/// server's own handshake bound.
pub const HELLO_SECS: u64 = 10;

/// The frames a connection may hold queued for its writer.
const WRITE_QUEUE: usize = 16;

/// **Whose files a connector reads its config and key from** (Spec 8.1,
/// the brief of 2026-11-27, child 6): the owner each file and its folder
/// must have, and the group each file must have, the connector's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trust {
    pub owner: libc::uid_t,
    pub group: libc::gid_t,
}

impl Trust {
    /// **The rule a connector reads under**: root's files, read through its
    /// own effective group. A test build takes its own uid as the owner,
    /// since a test makes no file of root's; a test of the root rule names
    /// owner 0 itself.
    pub fn connector() -> Self {
        // SAFETY: getegid and geteuid take nothing and cannot fail.
        let group = unsafe { libc::getegid() };
        #[cfg(not(test))]
        let owner = 0;
        #[cfg(test)]
        let owner = unsafe { libc::geteuid() };
        Self { owner, group }
    }

    /// The invoking user's own files and group: the staging tool, which
    /// runs unprivileged and checks what it composes before it is laid.
    pub fn invoking() -> Self {
        // SAFETY: as above.
        unsafe {
            Self {
                owner: libc::geteuid(),
                group: libc::getegid(),
            }
        }
    }
}

/// **Which file a read is of**: a config, whose access list may grant read
/// to named parties, or a key, which no party beyond the three base
/// entries reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Held {
    Config,
    Key,
}

/// Read a connector's config under the rule a connector reads it by
/// (`read_trusted`, `Trust::connector`).
pub fn read_private(path: &Path) -> anyhow::Result<String> {
    read_trusted(path, Held::Config, &Trust::connector())
}

// The access list's extended attribute, its version and its tags
// (acl_ea.h).
const ACL_XATTR: &[u8] = b"system.posix_acl_access\0";
const ACL_VERSION: u32 = 2;
const ACL_USER: u16 = 0x02;
const ACL_GROUP_OBJ: u16 = 0x04;
const ACL_GROUP: u16 = 0x08;
const ACL_MASK: u16 = 0x10;
const ACL_READ: u16 = 0x04;

/// **Read a connector's config or key, refusing one another party could
/// write, swap or, for a key, read** (the brief of 2026-11-27, child 6).
/// Each check is made by descriptor: the folder is opened first without
/// following a symlink, root's (`trust.owner`), of the connector's group or
/// root's, writable by no group or other and nothing for others; then the
/// file under it with `openat`, without following a symlink and without
/// blocking, so a FIFO planted at the path cannot hang the start: a regular
/// file, `trust.owner`'s, of the connector's group, no bit outside 0640,
/// and its access list read as well as its mode, since on a file that
/// carries one the mode's group bits show the mask. A key carries no entry
/// beyond the three base ones; a config's named entries grant read alone.
/// Both descriptors are judged, the file's first, before a byte is read.
/// Read under its bound.
pub fn read_trusted(path: &Path, held: Held, trust: &Trust) -> anyhow::Result<String> {
    use std::io::Read;
    let file = open_trusted(path, held, trust)?;
    let bound = match held {
        Held::Config => CONFIG_BOUND,
        Held::Key => KEY_BOUND,
    };
    let mut content = String::new();
    file.take(bound + 1)
        .read_to_string(&mut content)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    if content.len() as u64 > bound {
        let what = match held {
            Held::Config => "config",
            Held::Key => "key",
        };
        anyhow::bail!(
            "{} passes {bound} bytes and is not a connector's {what}",
            path.display()
        );
    }
    Ok(content)
}

/// Open `path` by `read_trusted`'s rule, for reading.
fn open_trusted(path: &Path, held: Held, trust: &Trust) -> anyhow::Result<std::fs::File> {
    use std::ffi::CString;
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;
    let symlink = |at: &Path| {
        anyhow::anyhow!(
            "{} is a symlink, and a file carrying or naming a key is read through no symlink",
            at.display()
        )
    };
    let folder = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("{} names no file", path.display()))?;
    let c = |p: &[u8]| {
        CString::new(p).map_err(|_| anyhow::anyhow!("{} holds a NUL byte", path.display()))
    };
    let folder_c = c(folder.as_os_str().as_bytes())?;
    // SAFETY: a NUL-terminated path; the descriptor is owned at once.
    let dir = unsafe {
        libc::open(
            folder_c.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if dir < 0 {
        let e = std::io::Error::last_os_error();
        return Err(match e.raw_os_error() {
            Some(libc::ELOOP) => symlink(folder),
            Some(libc::ENOTDIR) => anyhow::anyhow!(
                "{} is not a directory, or is a symlink, and a connector's config and key stand in a folder only root writes",
                folder.display()
            ),
            _ => anyhow::anyhow!("opening {}: {e}", folder.display()),
        });
    }
    // SAFETY: `dir` is a descriptor this call opened.
    let dir = unsafe { OwnedFd::from_raw_fd(dir) };
    let name_c = c(name.as_bytes())?;
    // SAFETY: as above, relative to the folder's descriptor.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name_c.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        let e = std::io::Error::last_os_error();
        return Err(match e.raw_os_error() {
            Some(libc::ELOOP) => symlink(path),
            _ => anyhow::anyhow!("opening {}: {e}", path.display()),
        });
    }
    // SAFETY: `fd` is a descriptor this call opened.
    let file = unsafe { std::fs::File::from_raw_fd(fd) };
    let st = fstat(fd).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    if st.st_mode & libc::S_IFMT != libc::S_IFREG {
        anyhow::bail!(
            "{} is not a regular file, and a connector's config and key are each one",
            path.display()
        );
    }
    if st.st_uid != trust.owner {
        anyhow::bail!(
            "{} is owned by uid {}; a connector's config and key are root's, and this connector only reads them",
            path.display(),
            st.st_uid
        );
    }
    if st.st_gid != trust.group {
        anyhow::bail!(
            "{} has group {}, and this runs with effective group {}; a connector's config and key are read through its own group alone, so run it with group {} as its effective group",
            path.display(),
            st.st_gid,
            trust.group,
            st.st_gid
        );
    }
    let mode = st.st_mode & 0o7777;
    if mode & !0o640 != 0 {
        anyhow::bail!(
            "{} has mode {mode:04o}; a connector's config and key are 0640 or tighter, with nothing for others",
            path.display()
        );
    }
    access_list(fd, path, held)?;
    // The folder's descriptor, opened before the file's, is judged after
    // it: both are judged before a byte is read, and the file's own
    // refusal names the file.
    let st =
        fstat(dir.as_raw_fd()).map_err(|e| anyhow::anyhow!("reading {}: {e}", folder.display()))?;
    judge_folder(folder, &st, trust)?;
    Ok(file)
}

/// **The folder's judgment**: root's (the trust's owner), of the connector's
/// group or root's, writable by no group or other and nothing for others.
/// Each refusal names what it found.
pub(crate) fn judge_folder(folder: &Path, st: &libc::stat, trust: &Trust) -> anyhow::Result<()> {
    const WHY: &str = "a connector's config and key stand in a folder only root writes";
    if st.st_uid != trust.owner {
        anyhow::bail!(
            "{} is owned by uid {}, not uid {}; {WHY}",
            folder.display(),
            st.st_uid,
            trust.owner
        );
    }
    if st.st_gid != trust.group && st.st_gid != 0 {
        anyhow::bail!(
            "{} has group {}, neither this connector's group {} nor root's; {WHY}",
            folder.display(),
            st.st_gid,
            trust.group
        );
    }
    if st.st_mode & 0o027 != 0 {
        anyhow::bail!(
            "{} has mode {:04o}, writable by group or others or open to others; {WHY}",
            folder.display(),
            st.st_mode & 0o7777
        );
    }
    Ok(())
}

fn fstat(fd: RawFd) -> std::io::Result<libc::stat> {
    // SAFETY: an all-zero stat is valid, and fstat fills it.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: a valid descriptor and a stat to fill.
    if unsafe { libc::fstat(fd, &mut st) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(st)
}

/// **The file's access list, judged** (the brief of 2026-11-27, child 6):
/// read by descriptor; none where the file carries none or the filesystem
/// keeps none; a key refused for any entry beyond its owner, group and
/// others; a config refused for a named entry, the owning group's entry or
/// the mask granting write or execute. Any other error reading it refuses
/// the file: nothing is admitted on a list that was not read.
fn access_list(fd: RawFd, path: &Path, held: Held) -> anyhow::Result<()> {
    let mut buf = [0u8; 4 + 8 * 64];
    // SAFETY: a valid descriptor, a NUL-terminated name and a buffer of
    // its stated length.
    let n = unsafe {
        libc::fgetxattr(
            fd,
            ACL_XATTR.as_ptr().cast(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if n < 0 {
        let e = std::io::Error::last_os_error();
        return match e.raw_os_error() {
            Some(libc::ENODATA) | Some(libc::ENOTSUP) => Ok(()),
            _ => Err(anyhow::anyhow!(
                "reading the access list of {}: {e}; a file whose list was not read is refused",
                path.display()
            )),
        };
    }
    let list = &buf[..n as usize];
    if list.len() < 4
        || (list.len() - 4) % 8 != 0
        || u32::from_le_bytes([list[0], list[1], list[2], list[3]]) != ACL_VERSION
    {
        anyhow::bail!(
            "{} carries an access list that does not parse; a file whose list was not read is refused",
            path.display()
        );
    }
    for entry in list[4..].chunks_exact(8) {
        let tag = u16::from_le_bytes([entry[0], entry[1]]);
        let perm = u16::from_le_bytes([entry[2], entry[3]]);
        match held {
            Held::Key if matches!(tag, ACL_USER | ACL_GROUP | ACL_MASK) => anyhow::bail!(
                "{} carries an access-list entry beyond its owner, group and others; a connector's key is read by its own group alone",
                path.display()
            ),
            Held::Config
                if matches!(tag, ACL_USER | ACL_GROUP | ACL_GROUP_OBJ | ACL_MASK)
                    && perm & !ACL_READ != 0 =>
            {
                anyhow::bail!(
                    "{} carries an access-list entry that grants more than read; a connector's config is written by root alone",
                    path.display()
                )
            }
            _ => {}
        }
    }
    Ok(())
}

/// Parse a connector's config, **naming only its own members in an error**
/// (see `unparsed`).
pub fn parse_config<T: serde::de::DeserializeOwned>(
    path: &Path,
    content: &str,
    members: &[&str],
) -> anyhow::Result<T> {
    // **`local_address` is checked by name first** (Spec 8): it rides the
    // flattened `LinkConfig`, and serde's flatten drops a member's span, so
    // a value that is not an IP literal would be blamed on the file's first
    // line. The value is never quoted.
    if members.contains(&"local_address")
        && let Ok(table) = content.parse::<toml::Table>()
        && let Some(value) = table.get("local_address")
        && value
            .as_str()
            .is_none_or(|text| text.parse::<std::net::IpAddr>().is_err())
    {
        anyhow::bail!(
            "{}: the config does not parse: the member local_address is not an IP literal",
            path.display()
        );
    }
    // **A config carrying its key inline is an old one** (the brief of
    // 2026-11-27, child 6): refused by name, so it never starts keyless.
    if members.contains(&"key_file")
        && let Ok(table) = content.parse::<toml::Table>()
        && table.contains_key("key")
    {
        anyhow::bail!(
            "{}: the config carries its key inline (key); a connector reads its key from key_file, a file of its own",
            path.display()
        );
    }
    toml::from_str(content).map_err(|e| unparsed(path, content, &e, members))
}

/// **A parse error names the line and the member, never the text**: toml's
/// message can quote the rejected value (a key pasted into an integer
/// member, a PEM line that lost its quoting), and the file carries a key.
/// The member is named only where it is one of the connector's members, since a corrupted
/// line's own "key" may be base64 of the key itself.
fn unparsed(path: &Path, content: &str, e: &toml::de::Error, members: &[&str]) -> anyhow::Error {
    let member = |name: &str| members.iter().find(|m| **m == name).copied();
    // A member that is absent carries no value to echo; serde names it.
    if let Some(rest) = e.message().strip_prefix("missing field `")
        && let Some(name) = rest.split('`').next().and_then(member)
    {
        return anyhow::anyhow!(
            "{}: the config does not parse: the member {name} is missing",
            path.display()
        );
    }
    let Some(span) = e.span() else {
        return anyhow::anyhow!("{}: the config does not parse", path.display());
    };
    let start = span.start.min(content.len());
    let line = content[..start].matches('\n').count() + 1;
    let text = content.lines().nth(line - 1).unwrap_or("");
    match text
        .split_once('=')
        .and_then(|(name, _)| member(name.trim()))
    {
        Some(name) => anyhow::anyhow!(
            "{}: the config does not parse at line {line}, the member {name}",
            path.display()
        ),
        None => anyhow::anyhow!(
            "{}: the config does not parse at line {line}",
            path.display()
        ),
    }
}

/// **Dial `server` from `local`, where one is named** (Spec 8): the socket
/// binds that address, port zero, before it connects, so the connection
/// leaves by that card's address; which card carries it is still the
/// machine's routes'. Each of the server's addresses of the local address's
/// family is tried in turn. With no `local`, a plain connect by the routes.
pub(crate) async fn dial_from(
    server: &str,
    local: Option<std::net::IpAddr>,
) -> std::io::Result<TcpStream> {
    let Some(local) = local else {
        return TcpStream::connect(server).await;
    };
    let mut last = None;
    for at in tokio::net::lookup_host(server).await? {
        if at.is_ipv4() != local.is_ipv4() {
            continue;
        }
        let socket = if at.is_ipv4() {
            tokio::net::TcpSocket::new_v4()?
        } else {
            tokio::net::TcpSocket::new_v6()?
        };
        socket.bind(std::net::SocketAddr::new(local, 0))?;
        match socket.connect(at).await {
            Ok(tcp) => return Ok(tcp),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            format!("{server} has no address of the family of the local address {local}"),
        )
    }))
}

/// The members `weaver-web register` writes into every connector's config.
/// A connector's own config flattens this and adds its box facts.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct LinkConfig {
    /// The server's link address, dialed.
    pub server: String,
    /// The name the server's certificate is verified under, which is the
    /// authority's and never the dialed address.
    pub server_name: String,
    pub agent: String,
    /// The row's identity (`ag-...`), which a re-install is held to: the
    /// name is not unique across boxes. Rotation keeps the row and so this.
    pub agent_id: String,
    pub plane: Plane,
    pub server_certificate: String,
    pub certificate: String,
    /// **The connector's key, a file of its own** (the brief of 2026-11-27,
    /// child 6): absolute, a box fact, read by `load_key` under the key's
    /// rule once the config has passed.
    pub key_file: std::path::PathBuf,
    /// The key's PEM as `load_key` read it; never a config member.
    #[serde(skip)]
    pub key: String,
    /// **The card's address this connector dials from** (Spec 8): an IP
    /// literal bound, port zero, before the dial, a box fact appended after
    /// each adopt.
    /// Absent, the connector dials by the machine's routes.
    #[serde(default)]
    pub local_address: Option<std::net::IpAddr>,
}

// The key is never printed.
impl std::fmt::Debug for LinkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkConfig")
            .field("server", &self.server)
            .field("server_name", &self.server_name)
            .field("agent", &self.agent)
            .field("agent_id", &self.agent_id)
            .field("plane", &self.plane)
            .finish_non_exhaustive()
    }
}

impl LinkConfig {
    /// **Read the key from `key_file` under `trust`**, after the config at
    /// `config` passed: an absolute path, one PKCS #8 ECDSA P-256 PEM
    /// block, and the config's certificate over it, checked at every load
    /// as `adopt`'s `key_mismatch` is.
    pub fn load_key(&mut self, config: &Path, trust: &Trust) -> anyhow::Result<()> {
        if !self.key_file.is_absolute() {
            anyhow::bail!("{}: key_file is not an absolute path", config.display());
        }
        let pem = read_trusted(&self.key_file, Held::Key, trust)?;
        if !crate::link::key_config::one_key(&pem) {
            anyhow::bail!(
                "{}: the key is not one PKCS #8 ECDSA P-256 PEM block",
                self.key_file.display()
            );
        }
        if !crate::link::key_config::over_key(&self.certificate, &pem) {
            anyhow::bail!(
                "{}: the certificate is not over the key in {}",
                config.display(),
                self.key_file.display()
            );
        }
        self.key = pem;
        Ok(())
    }

    /// A config minted for the other plane is refused: its credential is
    /// bound to that plane (Spec 8), and the server would refuse its hello.
    pub fn expect_plane(&self, plane: Plane) -> anyhow::Result<()> {
        if self.plane != plane {
            anyhow::bail!(
                "the config is for the {} plane and this connector runs the {plane} plane's credential only",
                self.plane
            );
        }
        Ok(())
    }
}

/// A dialable link: the TLS client built once from the config.
pub struct Link {
    config: LinkConfig,
    tls: TlsConnector,
    server_name: rustls::pki_types::ServerName<'static>,
}

/// How one connection attempt ended.
pub enum Connect {
    Admitted(Connection),
    Refused(Refusal),
    /// **The server's certificate is not signed by the authority this config
    /// pins**: the server's authority was rotated (or the config names
    /// another server). Treated as a credential refusal, since only a
    /// re-installed config can cure it.
    Untrusted(String),
    Failed(String),
}

/// Why an attempt failed before the hello's answer.
enum Failure {
    Untrusted(String),
    /// No TCP connection or no TLS handshake.
    Unreachable(String),
    Plain(String),
}

/// **How a setup ask ended** (Spec 8.3).
pub enum Asked {
    /// The server's one frame.
    Answer(ToClient),
    /// No TCP connection or no TLS handshake, the server's certificate
    /// failing under the pinned authority among them, or either stalling
    /// past the bound: the ask was never sent.
    Unreachable(String),
    /// The command's own failure before the ask was sent.
    Fault(String),
    /// The connection ended, or went silent, once the ask's frame had begun
    /// to be written and before an answer: the server may have acted.
    Ended(String),
}

/// The bound on a setup ask from the dial to its answer.
pub const ASK_SECS: u64 = 30;

/// Whether a handshake error is the server's certificate failing under the
/// pinned authority: unknown issuer or a bad signature.
fn untrusted(e: &std::io::Error) -> bool {
    e.get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        .is_some_and(|r| {
            matches!(
                r,
                rustls::Error::InvalidCertificate(
                    rustls::CertificateError::UnknownIssuer
                        | rustls::CertificateError::BadSignature
                )
            )
        })
}

impl Link {
    pub fn new(config: LinkConfig) -> anyhow::Result<Self> {
        let tls = client_tls(&config.server_certificate, &config.certificate, &config.key)?;
        let server_name = rustls::pki_types::ServerName::try_from(config.server_name.clone())
            .map_err(|e| anyhow::anyhow!("server_name {}: {e}", config.server_name))?;
        Ok(Self {
            config,
            tls: TlsConnector::from(tls),
            server_name,
        })
    }

    pub fn config(&self) -> &LinkConfig {
        &self.config
    }

    /// Dial, handshake, hello, and read the answer, all within
    /// `HELLO_SECS`. **Any frame before the answer other than a refusal is
    /// a protocol fault.**
    pub async fn connect(&self, hello: FromClient) -> Connect {
        let bound = Duration::from_secs(HELLO_SECS);
        let sent = AtomicBool::new(false);
        let (fd, reader, write, answer) =
            match tokio::time::timeout(bound, self.first_frame(&hello, &sent)).await {
                Ok(Ok(parts)) => parts,
                Ok(Err(Failure::Plain(why) | Failure::Unreachable(why))) => {
                    return Connect::Failed(why);
                }
                Ok(Err(Failure::Untrusted(why))) => return Connect::Untrusted(why),
                Err(_) => {
                    return Connect::Failed(format!(
                        "no answer to the hello within {HELLO_SECS} s"
                    ));
                }
            };
        self.answered(fd, reader, write, answer)
    }

    /// **A setup ask** (Spec 8.3): dial, handshake, the ask as the first
    /// frame, and the server's one frame, all within `ASK_SECS`.
    pub async fn ask(&self, ask: FromClient) -> Asked {
        self.ask_within(ask, Duration::from_secs(ASK_SECS)).await
    }

    /// `ask` within `bound`. **What ended it is told by whether the ask's
    /// frame had begun to be written**: before it, nothing reached the
    /// server, and a stall of the dial or the handshake is unreachable
    /// like their refusal. After it, the server may have acted.
    pub async fn ask_within(&self, ask: FromClient, bound: Duration) -> Asked {
        let sent = AtomicBool::new(false);
        let ended = tokio::time::timeout(bound, self.first_frame(&ask, &sent)).await;
        let sent = sent.load(Ordering::SeqCst);
        match ended {
            Ok(Ok((_, _, _, answer))) => Asked::Answer(answer),
            Ok(Err(Failure::Unreachable(why) | Failure::Untrusted(why))) => Asked::Unreachable(why),
            Ok(Err(Failure::Plain(why))) if sent => Asked::Ended(why),
            Ok(Err(Failure::Plain(why))) => Asked::Fault(why),
            Err(_) if sent => Asked::Ended(format!(
                "no answer within {} s of the dial",
                bound.as_secs_f64()
            )),
            Err(_) => Asked::Unreachable(format!(
                "no connection and handshake with {} within {} s",
                self.config.server,
                bound.as_secs_f64()
            )),
        }
    }

    /// Dial, handshake, send the first frame, and read the server's first.
    /// `sent` is set as the frame's first byte may leave.
    async fn first_frame(
        &self,
        first: &FromClient,
        sent: &AtomicBool,
    ) -> Result<
        (
            std::os::fd::RawFd,
            LineReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
            tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
            ToClient,
        ),
        Failure,
    > {
        let hello = first;
        let attempt = async {
            let tcp = dial_from(&self.config.server, self.config.local_address)
                .await
                .map_err(|e| {
                    Failure::Unreachable(format!("dialing {}: {e}", self.config.server))
                })?;
            let _ = tcp.set_nodelay(true);
            let fd = tcp.as_raw_fd();
            let stream = self
                .tls
                .connect(self.server_name.clone(), tcp)
                .await
                .map_err(|e| {
                    let why = format!("the handshake with {}: {e}", self.config.server);
                    if untrusted(&e) {
                        Failure::Untrusted(why)
                    } else {
                        Failure::Unreachable(why)
                    }
                })?;
            let (read, mut write) = tokio::io::split(stream);
            let mut reader = LineReader::new(read);
            let mut line = serde_json::to_vec(&hello).map_err(|e| Failure::Plain(e.to_string()))?;
            line.push(b'\n');
            sent.store(true, Ordering::SeqCst);
            write
                .write_all(&line)
                .await
                .map_err(|e| Failure::Plain(format!("sending the hello: {e}")))?;
            write
                .flush()
                .await
                .map_err(|e| Failure::Plain(format!("sending the hello: {e}")))?;
            let answer = match reader.next().await {
                Line::Frame(line) => serde_json::from_str::<ToClient>(&line).map_err(|e| {
                    Failure::Plain(format!("the hello's answer did not parse: {e}"))
                })?,
                Line::Closed => {
                    return Err(Failure::Plain(
                        "the server closed the connection before answering the hello".to_owned(),
                    ));
                }
                Line::Malformed(why) => {
                    return Err(Failure::Plain(format!("the server sent {why}")));
                }
            };
            Ok::<_, Failure>((fd, reader, write, answer))
        };
        attempt.await
    }

    /// The hello's answer read: admitted, refused, or a protocol fault.
    fn answered(
        &self,
        fd: std::os::fd::RawFd,
        reader: LineReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
        write: tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
        answer: ToClient,
    ) -> Connect {
        match answer {
            ToClient::HelloAnswer {
                cadence_secs,
                silence_secs,
                acknowledged,
            } => {
                if cadence_secs == 0 || cadence_secs > CADENCE_MAX_SECS {
                    return Connect::Failed(format!(
                        "protocol fault: the hello's answer named a cadence of {cadence_secs} s, outside 1 to {CADENCE_MAX_SECS}"
                    ));
                }
                // The bound is at least four cadences, the cadence being its
                // quotient, and no more than the longest cadence's four.
                if silence_secs < cadence_secs * 4 || silence_secs > CADENCE_MAX_SECS * 4 {
                    return Connect::Failed(format!(
                        "protocol fault: the hello's answer named a silence bound of {silence_secs} s against a cadence of {cadence_secs} s"
                    ));
                }
                let cadence = Duration::from_secs(cadence_secs);
                if let Err(e) = keepalive(fd, cadence) {
                    tracing::warn!("TCP keepalive could not be set on the link: {e}");
                }
                if let Err(e) = bound_unsent(fd) {
                    tracing::warn!("the link's unsent bound could not be set: {e}");
                }
                Connect::Admitted(Connection::start(reader, write, cadence, acknowledged))
            }
            ToClient::Refusal { reason } => Connect::Refused(reason),
            other => Connect::Failed(format!(
                "protocol fault: a {} frame before the hello's answer",
                to_client_name(&other)
            )),
        }
    }
}

/// A server frame's tag, for a log line.
pub fn to_client_name(frame: &ToClient) -> &'static str {
    match frame {
        ToClient::HelloAnswer { .. } => "hello_answer",
        ToClient::Turn { .. } => "turn",
        ToClient::Verb { .. } => "verb",
        ToClient::Ack { .. } => "ack",
        ToClient::Landed { .. } => "landed",
        ToClient::EntryAck { .. } => "entry_ack",
        ToClient::Refusal { .. } => "refusal",
        ToClient::Described { .. } => "described",
        ToClient::Accepted => "accepted",
    }
}

/// **A dead server is noticed within a bound**: keepalive probes start
/// after one cadence of idleness, and data the server's kernel has not
/// acknowledged for two cadences (heartbeats included, since the
/// connector sends one every cadence) errors the socket, so a server whose
/// box vanished without a reset ends the connection rather than leaving it
/// half open. A connection that is merely quiet is not dead: the server
/// sends nothing unasked on the gate plane, and its kernel still answers.
fn keepalive(fd: RawFd, cadence: Duration) -> std::io::Result<()> {
    let secs = cadence.as_secs().clamp(1, i32::MAX as u64 / 2000) as libc::c_int;
    let set = |level: libc::c_int, name: libc::c_int, value: libc::c_int| {
        // SAFETY: a descriptor the caller's stream holds open, and an int
        // option of the size the call is told.
        let r = unsafe {
            libc::setsockopt(
                fd,
                level,
                name,
                &value as *const libc::c_int as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        if r < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    };
    set(libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1)?;
    set(libc::IPPROTO_TCP, libc::TCP_KEEPIDLE, secs)?;
    set(libc::IPPROTO_TCP, libc::TCP_KEEPINTVL, secs)?;
    set(libc::IPPROTO_TCP, libc::TCP_KEEPCNT, 2)?;
    set(libc::IPPROTO_TCP, libc::TCP_USER_TIMEOUT, secs * 2000)?;
    Ok(())
}

/// The most bytes the socket holds unsent: see `bound_unsent`.
pub const UNSENT_BOUND: usize = 64 * 1024;

/// **What the kernel holds unsent ahead of the next frame is bounded**, so
/// a frame the connector sends next, a verb's answer among them, waits
/// behind at most `UNSENT_BOUND` bytes plus what the link carries in
/// flight, and not behind a send buffer autotuned to megabytes on a slow
/// link, which would hold an admission `show` past its deadline whatever
/// order the connector chose. A write blocks while the unsent bytes stand
/// above the bound, and the link's throughput is not otherwise affected.
fn bound_unsent(fd: RawFd) -> std::io::Result<()> {
    let value = UNSENT_BOUND as libc::c_int;
    // SAFETY: a descriptor the caller's stream holds open, and an int option
    // of the size the call is told.
    let r = unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_TCP,
            libc::TCP_NOTSENT_LOWAT,
            &value as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    if r < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

type Reader = LineReader<ReadHalf<TlsStream<TcpStream>>>;
type Writer = WriteHalf<TlsStream<TcpStream>>;

/// An admitted connection.
pub struct Connection {
    /// The cadence the hello's answer named: the heartbeat's period and
    /// every write's bound.
    pub cadence: Duration,
    /// On the admin plane, the position this server process acknowledged
    /// (Spec 7.2); none on the gate plane.
    pub acknowledged: Option<Position>,
    reader: Reader,
    tx: Option<mpsc::Sender<FromClient>>,
    /// Why the write path ended, once it has.
    failed: watch::Receiver<Option<String>>,
    writer: Option<JoinHandle<()>>,
    heartbeat: Option<JoinHandle<()>>,
    /// The connector's entries, where it sends them (Spec 2.14), and the
    /// pump that sends them on this connection.
    entries: Option<Entries>,
    pump: Option<JoinHandle<()>>,
}

/// What the server sent, or why the connection is over.
#[derive(Debug)]
pub enum Incoming {
    Frame(ToClient),
    Refused(Refusal),
    Lost(String),
}

impl Connection {
    fn start(
        reader: Reader,
        mut write: Writer,
        cadence: Duration,
        acknowledged: Option<Position>,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<FromClient>(WRITE_QUEUE);
        let (failed_tx, failed) = watch::channel(None);
        let fail = failed_tx.clone();
        // **Every write is held to the cadence.** One that does not
        // complete within it is the server gone or no longer taking bytes,
        // and the connection ends for that reason.
        let writer = tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                let Ok(mut line) = serde_json::to_vec(&frame) else {
                    continue;
                };
                line.push(b'\n');
                let wrote = tokio::time::timeout(cadence, async {
                    write.write_all(&line).await?;
                    write.flush().await
                })
                .await;
                match wrote {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        fail.send_replace(Some(format!("writing to the server: {e}")));
                        return;
                    }
                    Err(_) => {
                        fail.send_replace(Some(format!(
                            "a write did not complete within the cadence ({cadence:?}): the server is gone or no longer taking bytes"
                        )));
                        return;
                    }
                }
            }
            let _ = tokio::time::timeout(cadence, write.shutdown()).await;
        });
        let beat = tx.clone();
        let heartbeat = tokio::spawn(async move {
            let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + cadence, cadence);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                match tokio::time::timeout(cadence, beat.send(FromClient::Heartbeat)).await {
                    Ok(Ok(())) => {}
                    // The writer ended and said why.
                    Ok(Err(_)) => return,
                    Err(_) => {
                        failed_tx.send_replace(Some(format!(
                            "a heartbeat could not be queued within the cadence ({cadence:?})"
                        )));
                        return;
                    }
                }
            }
        });
        Self {
            cadence,
            acknowledged,
            reader,
            tx: Some(tx),
            failed,
            writer: Some(writer),
            heartbeat: Some(heartbeat),
            entries: None,
            pump: None,
        }
    }

    fn lost(&self) -> String {
        self.failed
            .borrow()
            .clone()
            .unwrap_or_else(|| "the write path ended".to_owned())
    }

    /// **Send the connector's entries on this connection** (Spec 2.14):
    /// a pump of its own resends what the server has not acknowledged, then
    /// sends each entry as it is pushed, at most [`crate::link::entries::WINDOW`]
    /// unacknowledged at once, each send held to the cadence like every
    /// write. The connector's own loop never waits on it, and
    /// [`Connection::recv`] takes the acknowledgements.
    pub fn carry(&mut self, entries: Entries) {
        let Some(tx) = self.tx.clone() else {
            return;
        };
        let cadence = self.cadence;
        entries.reconnected();
        let pumped = entries.clone();
        // **What was sent and not acknowledged is sent again** a silence
        // bound (four cadences) after the last progress, an acknowledgement
        // taken or a resend: the server sends an acknowledgement without
        // waiting and drops it where its write queue is full, and a full
        // window sends nothing more to draw a later one. The server's mark
        // takes a resend once and acknowledges it again. **The deadline is an
        // instant, read at each wake**: a push wakes the pump and moves it
        // not, so a steady stream of entries never postpones the resend.
        let resend_after = cadence * 4;
        self.pump = Some(tokio::spawn(async move {
            let mut sent = 0u64;
            let mut acks = pumped.acks();
            let mut since = tokio::time::Instant::now();
            loop {
                let now_acks = pumped.acks();
                if now_acks != acks || !pumped.unacknowledged(sent) {
                    acks = now_acks;
                    since = tokio::time::Instant::now();
                }
                if tokio::time::Instant::now() >= since + resend_after {
                    sent = 0;
                    since = tokio::time::Instant::now();
                }
                match pumped.next_after(sent) {
                    Some((seq, frame)) => {
                        match tokio::time::timeout(cadence, tx.send(frame)).await {
                            Ok(Ok(())) => sent = seq,
                            // The writer ended or stopped taking frames:
                            // the connection ends on its own account, and
                            // what was not acknowledged is resent on the
                            // next.
                            _ => return,
                        }
                    }
                    None => {
                        tokio::select! {
                            () = pumped.changed() => {}
                            () = tokio::time::sleep_until(since + resend_after) => {}
                        }
                    }
                }
            }
        }));
        self.entries = Some(entries);
    }

    /// The next frame from the server, or why the connection is over.
    /// Cancel-safe: the line reader is, and so is the watch. **An entry's
    /// acknowledgement is taken here** and never surfaces.
    pub async fn recv(&mut self) -> Incoming {
        loop {
            if self.failed.borrow().is_some() {
                return Incoming::Lost(self.lost());
            }
            let incoming = tokio::select! {
                line = self.reader.next() => match line {
                    Line::Frame(line) => match serde_json::from_str::<ToClient>(&line) {
                        Ok(ToClient::Refusal { reason }) => Incoming::Refused(reason),
                        Ok(frame) => Incoming::Frame(frame),
                        Err(e) => Incoming::Lost(format!("a frame from the server did not parse: {e}")),
                    },
                    Line::Closed => Incoming::Lost("the server closed the connection".to_owned()),
                    Line::Malformed(why) => Incoming::Lost(format!("the server sent {why}")),
                },
                _ = self.failed.changed() => Incoming::Lost(self.lost()),
            };
            match incoming {
                Incoming::Frame(ToClient::EntryAck { run, seq }) => {
                    if let Some(entries) = &self.entries {
                        entries.ack(&run, seq);
                    }
                }
                other => return other,
            }
        }
    }

    /// Queue a frame for the server, bounded by the cadence.
    pub async fn send(&self, frame: FromClient) -> Result<(), String> {
        let Some(tx) = &self.tx else {
            return Err("the connection is closing".to_owned());
        };
        match tokio::time::timeout(self.cadence, tx.send(frame)).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(self.lost()),
            Err(_) => Err(format!(
                "a frame could not be queued within the cadence ({:?})",
                self.cadence
            )),
        }
    }

    /// Close: the heartbeat stopped and joined, the writer drained within
    /// the cadence (it shuts the TLS stream down) or aborted, and joined.
    pub async fn close(self) {
        let cadence = self.cadence;
        self.close_within(cadence).await;
    }

    /// Close with the writer's drain bounded by `bound` rather than the
    /// cadence: shutdown passes what remains of its grace, so a server that
    /// stopped taking bytes cannot hold a stop a cadence past the grace.
    pub async fn close_within(mut self, bound: Duration) {
        if let Some(pump) = self.pump.take() {
            pump.abort();
            let _ = pump.await;
        }
        if let Some(heartbeat) = self.heartbeat.take() {
            heartbeat.abort();
            let _ = heartbeat.await;
        }
        drop(self.tx.take());
        if let Some(mut writer) = self.writer.take()
            && tokio::time::timeout(bound, &mut writer).await.is_err()
        {
            writer.abort();
            let _ = writer.await;
        }
    }
}

impl Drop for Connection {
    /// A connection dropped without `close` (a cancelled future) leaves no
    /// task behind: both are aborted, which drops the socket's halves.
    fn drop(&mut self) {
        if let Some(pump) = self.pump.take() {
            pump.abort();
        }
        if let Some(heartbeat) = self.heartbeat.take() {
            heartbeat.abort();
        }
        if let Some(writer) = self.writer.take() {
            writer.abort();
        }
    }
}

/// How a served connection ended.
#[derive(Debug, Clone)]
pub enum Ended {
    Refused(Refusal),
    /// The server's certificate failed under the pinned authority.
    Untrusted(String),
    Lost(String),
    Shutdown,
}

/// Why an attempt or a connection ended, as the status and an outage's
/// entry say it.
fn why_ended(ended: &Ended) -> String {
    match ended {
        Ended::Refused(r) => format!("refused {r}"),
        Ended::Untrusted(why) => {
            format!("{why}: the server's authority is not the one this config pins")
        }
        Ended::Lost(why) => why.clone(),
        Ended::Shutdown => "shutdown".to_owned(),
    }
}

/// **The reconnect policy of Spec section 8's client paragraph**:
/// exponential from `base`, doubling per failure, capped at `cap`, with
/// jitter. A connection that stayed admitted for at least one cadence
/// resets the count, so a server restart is met at the base and a server
/// that drops every connection at once is backed off.
#[derive(Debug, Clone, Copy)]
pub struct Backoff {
    pub base: Duration,
    pub cap: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            base: Duration::from_secs(1),
            cap: Duration::from_secs(60),
        }
    }
}

/// A fraction in [0, 1) for jitter, drawn from the standard library's
/// randomly keyed hasher, so the policy needs no generator of its own.
fn jitter() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    (h.finish() >> 11) as f64 / (1u64 << 53) as f64
}

impl Backoff {
    /// The delay after `failures` consecutive failures: the exponential
    /// step, capped, with half of it jittered.
    pub fn delay(&self, failures: u32) -> Duration {
        let step = self
            .base
            .saturating_mul(1u32 << failures.saturating_sub(1).min(20));
        let step = step.min(self.cap);
        step / 2 + step.mul_f64(jitter() / 2.0)
    }

    /// The delay for a refusal that means the credential is wrong: **at
    /// the cap and never faster**, jittered above it only.
    pub fn at_cap(&self) -> Duration {
        self.cap + self.cap.mul_f64(jitter() / 4.0)
    }
}

/// What the client loop reports, for a test or a supervisor's log.
#[derive(Debug, Clone, Default)]
pub struct LinkStatus {
    /// Whether a connection is admitted now.
    pub admitted: bool,
    /// Connections admitted since the loop started.
    pub admissions: u64,
    /// Connection attempts since the loop started.
    pub attempts: u64,
    /// The refusal that ended the last attempt, if one did.
    pub last_refusal: Option<Refusal>,
    /// Why the last connection or attempt ended.
    pub last_end: Option<String>,
    /// The delay before the next attempt.
    pub next_delay: Option<Duration>,
}

/// **The client loop: connect, serve, and on any end reconnect under the
/// policy, never exiting on a refusal** (Spec 8). `not_live` and
/// `roster_mismatch` mean the credential is revoked or wrong, and a server
/// certificate the pinned authority did not sign means the server's
/// authority was rotated: all three retry only at the cap, logged loudly
/// with what the operator must do, and **before each retry at the cap the
/// config is re-read through `reload`**, so a config re-installed at its
/// path is dialed with at the next attempt and no restart is needed, which
/// is what never exiting requires. `malformed` and `wrong_plane` are this
/// connector's own defect and say so; every other end retries on the normal
/// backoff. It returns only on shutdown.
#[allow(clippy::too_many_arguments)]
pub async fn run<S, F, H, HF>(
    mut link: Link,
    mut hello: H,
    mut reload: impl FnMut() -> Option<LinkConfig>,
    backoff: Backoff,
    mut shutdown: watch::Receiver<bool>,
    status: &watch::Sender<LinkStatus>,
    entries: Option<Entries>,
    mut serve: S,
) where
    S: FnMut(Connection) -> F,
    F: Future<Output = Ended>,
    H: FnMut(&LinkConfig) -> HF,
    HF: Future<Output = Result<FromClient, String>>,
{
    const REINSTALL: &str = "Re-install this agent's config and key at the paths this connector was started with, from `weaver-web register` or `weaver-web rotate`: both are re-read before each retry at the cap, so no restart is needed";
    let mut failures: u32 = 0;
    let mut reload_due = false;
    // **Since when the link has been down, and why** (Spec 2.14): an
    // outage's length is the connector's own entry once it is back.
    let mut down: Option<(tokio::time::Instant, String)> = None;
    loop {
        if *shutdown.borrow_and_update() {
            return;
        }
        if std::mem::take(&mut reload_due)
            && let Some(fresh) = reload()
            && fresh != *link.config()
        {
            // **Only the link's members change at a re-install**: the
            // agent and the plane are what the rest of the connector is
            // bound to (gate-con's socket, admin-con's tailer and invoker),
            // so a config for another agent or plane at the path would
            // file one agent's traffic under another's row. It is refused
            // and the credential in hand kept. The agent is compared by its
            // row's identity, never its name, which another box may share.
            let held = link.config();
            if fresh.agent_id != held.agent_id || fresh.plane != held.plane {
                tracing::error!(
                    "{} {} ({}): the config at its path is for {} {} ({}), not the agent and plane this connector runs for; refused, keeping the credential in hand. Restart the connector to serve another agent",
                    held.agent_id,
                    held.agent,
                    held.plane,
                    fresh.agent_id,
                    fresh.agent,
                    fresh.plane
                );
            } else {
                match Link::new(fresh) {
                    Ok(fresh) => {
                        tracing::info!(
                            "{} ({}): the config at its path changed; dialing with its credential",
                            fresh.config().agent,
                            fresh.config().plane
                        );
                        link = fresh;
                    }
                    Err(e) => tracing::error!(
                        "the config at its path changed but its credential does not build ({e:#}); keeping the one in hand"
                    ),
                }
            }
        }
        let agent = link.config().agent.clone();
        let plane = link.config().plane;
        status.send_modify(|s| s.attempts += 1);
        // The hello is the plane's to make: admin-con's reads its trace
        // file's tail and asks its invoker for the ceiling, either of which
        // can fail and is then an attempt that ended.
        let attempt = tokio::select! {
            made = hello(link.config()) => match made {
                Ok(frame) => tokio::select! {
                    c = link.connect(frame) => c,
                    _ = shutdown.changed() => return,
                },
                Err(why) => Connect::Failed(why),
            },
            _ = shutdown.changed() => return,
        };
        let ended = match attempt {
            Connect::Admitted(mut conn) => {
                if let Some(entries) = &entries {
                    if let Some((since, why)) = down.take() {
                        entries.failure(
                            "link_outage",
                            format!(
                                "the link to the server was down for {} s ({why})",
                                since.elapsed().as_secs()
                            ),
                        );
                    }
                    conn.carry(entries.clone());
                }
                tracing::info!(
                    "{agent} ({plane}): admitted by {}, cadence {:?}",
                    link.config().server,
                    conn.cadence
                );
                let cadence = conn.cadence;
                let since = tokio::time::Instant::now();
                status.send_modify(|s| {
                    s.admitted = true;
                    s.admissions += 1;
                    s.last_refusal = None;
                    s.next_delay = None;
                });
                let ended = serve(conn).await;
                status.send_modify(|s| s.admitted = false);
                if since.elapsed() >= cadence {
                    failures = 0;
                }
                ended
            }
            Connect::Refused(reason) => Ended::Refused(reason),
            Connect::Untrusted(why) => Ended::Untrusted(why),
            Connect::Failed(why) => Ended::Lost(why),
        };
        if down.is_none() && !matches!(ended, Ended::Shutdown) {
            down = Some((tokio::time::Instant::now(), why_ended(&ended)));
        }
        let delay = match &ended {
            Ended::Shutdown => return,
            Ended::Refused(
                reason @ (Refusal::NotLive | Refusal::RosterMismatch | Refusal::WrongDoor),
            ) => {
                let what = if *reason == Refusal::NotLive {
                    "its credential is not live in the server's register (revoked, rotated, or registered on another server)"
                } else if *reason == Refusal::WrongDoor {
                    "its config's server address is the other plane's door (Spec 8)"
                } else {
                    "its hello named an agent or plane other than the credential's (the config was edited)"
                };
                let delay = backoff.at_cap();
                reload_due = true;
                tracing::error!(
                    "{agent} ({plane}): refused {reason}: {what}. {REINSTALL}. Retrying at the cap, in {delay:?}"
                );
                delay
            }
            Ended::Untrusted(why) => {
                let delay = backoff.at_cap();
                reload_due = true;
                tracing::error!(
                    "{agent} ({plane}): {why}: the server's certificate is not signed by the authority this config pins, so the server's authority was rotated. {REINSTALL}. Retrying at the cap, in {delay:?}"
                );
                delay
            }
            Ended::Refused(reason) => {
                failures = failures.saturating_add(1);
                let delay = backoff.delay(failures);
                if matches!(reason, Refusal::Malformed | Refusal::WrongPlane) {
                    tracing::error!(
                        "{agent} ({plane}): refused {reason}, this connector's own defect; reconnecting in {delay:?}"
                    );
                } else {
                    tracing::warn!(
                        "{agent} ({plane}): refused {reason}; reconnecting in {delay:?}"
                    );
                }
                delay
            }
            Ended::Lost(why) => {
                failures = failures.saturating_add(1);
                let delay = backoff.delay(failures);
                tracing::warn!("{agent} ({plane}): {why}; reconnecting in {delay:?}");
                delay
            }
        };
        status.send_modify(|s| {
            s.last_refusal = match &ended {
                Ended::Refused(r) => Some(*r),
                _ => None,
            };
            s.last_end = Some(why_ended(&ended));
            s.next_delay = Some(delay);
        });
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown.changed() => return,
        }
    }
}
