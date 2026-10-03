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
use crate::link::frames::{
    CADENCE_MAX_SECS, FromClient, Line, LineReader, Plane, Position, Refusal, ToClient,
};
use serde::Deserialize;
use std::future::Future;
use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// The bound on a connector's config file: it holds a key and two
/// certificates, a few KiB, so anything near this is not a config.
const CONFIG_BOUND: u64 = 1024 * 1024;

/// The bound on dialing, the handshake, the hello and its answer: the
/// server's own handshake bound.
pub const HELLO_SECS: u64 = 10;

/// The frames a connection may hold queued for its writer.
const WRITE_QUEUE: usize = 16;

/// Read a config that carries a private key, **refusing one another party
/// could read or swap**: opened without following a symlink (and without
/// blocking, so a FIFO planted at the path cannot hang the start), then
/// checked by its descriptor, not its path, to be a regular file owned by
/// the invoking uid with no permission bit outside 0600, and read under a
/// bound. The directories above it are the operator's, as `--out`'s are
/// for the register verb.
pub fn read_private(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ELOOP) => anyhow::anyhow!(
                "{} is a symlink, and a config carrying a key is read through no symlink",
                path.display()
            ),
            _ => anyhow::anyhow!("opening {}: {e}", path.display()),
        })?;
    let meta = file
        .metadata()
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    if !meta.file_type().is_file() {
        anyhow::bail!(
            "{} is not a regular file, and a config carrying a key is one",
            path.display()
        );
    }
    // SAFETY: geteuid takes nothing and cannot fail.
    let me = unsafe { libc::geteuid() };
    if meta.uid() != me {
        anyhow::bail!(
            "{} is owned by uid {}, not the invoking uid {me}; a config carrying a key is read only from its owner",
            path.display(),
            meta.uid()
        );
    }
    let mode = meta.mode() & 0o7777;
    if mode & !0o600 != 0 {
        anyhow::bail!(
            "{} has mode {mode:04o}; a config carrying a key is 0600 or tighter",
            path.display()
        );
    }
    let mut content = String::new();
    file.take(CONFIG_BOUND + 1)
        .read_to_string(&mut content)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    if content.len() as u64 > CONFIG_BOUND {
        anyhow::bail!(
            "{} passes {CONFIG_BOUND} bytes and is not a connector's config",
            path.display()
        );
    }
    Ok(content)
}

/// Parse a connector's config, **naming only its own members in an error**
/// (see `unparsed`).
pub fn parse_config<T: serde::de::DeserializeOwned>(
    path: &Path,
    content: &str,
    members: &[&str],
) -> anyhow::Result<T> {
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
    pub key: String,
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
    Plain(String),
}

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
        let attempt = async {
            let tcp = TcpStream::connect(&self.config.server)
                .await
                .map_err(|e| Failure::Plain(format!("dialing {}: {e}", self.config.server)))?;
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
                        Failure::Plain(why)
                    }
                })?;
            let (read, mut write) = tokio::io::split(stream);
            let mut reader = LineReader::new(read);
            let mut line = serde_json::to_vec(&hello).map_err(|e| Failure::Plain(e.to_string()))?;
            line.push(b'\n');
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
        let (fd, reader, write, answer) = match tokio::time::timeout(bound, attempt).await {
            Ok(Ok(parts)) => parts,
            Ok(Err(Failure::Plain(why))) => return Connect::Failed(why),
            Ok(Err(Failure::Untrusted(why))) => return Connect::Untrusted(why),
            Err(_) => {
                return Connect::Failed(format!("no answer to the hello within {HELLO_SECS} s"));
            }
        };
        match answer {
            ToClient::HelloAnswer {
                cadence_secs,
                acknowledged,
            } => {
                if cadence_secs == 0 || cadence_secs > CADENCE_MAX_SECS {
                    return Connect::Failed(format!(
                        "protocol fault: the hello's answer named a cadence of {cadence_secs} s, outside 1 to {CADENCE_MAX_SECS}"
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
        ToClient::Refusal { .. } => "refusal",
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
        }
    }

    fn lost(&self) -> String {
        self.failed
            .borrow()
            .clone()
            .unwrap_or_else(|| "the write path ended".to_owned())
    }

    /// The next frame from the server, or why the connection is over.
    /// Cancel-safe: the line reader is, and so is the watch.
    pub async fn recv(&mut self) -> Incoming {
        if self.failed.borrow().is_some() {
            return Incoming::Lost(self.lost());
        }
        tokio::select! {
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
pub async fn run<S, F, H, HF>(
    mut link: Link,
    mut hello: H,
    mut reload: impl FnMut() -> Option<LinkConfig>,
    backoff: Backoff,
    mut shutdown: watch::Receiver<bool>,
    status: &watch::Sender<LinkStatus>,
    mut serve: S,
) where
    S: FnMut(Connection) -> F,
    F: Future<Output = Ended>,
    H: FnMut(&LinkConfig) -> HF,
    HF: Future<Output = Result<FromClient, String>>,
{
    const REINSTALL: &str = "Re-install this agent's config at the path this connector was started with, from `weaver-web register` or `weaver-web rotate`: it is re-read before each retry at the cap, so no restart is needed";
    let mut failures: u32 = 0;
    let mut reload_due = false;
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
            Connect::Admitted(conn) => {
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
        let delay = match &ended {
            Ended::Shutdown => return,
            Ended::Refused(reason @ (Refusal::NotLive | Refusal::RosterMismatch)) => {
                let what = if *reason == Refusal::NotLive {
                    "its credential is not live in the server's register (revoked, rotated, or registered on another server)"
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
            s.last_end = Some(match &ended {
                Ended::Refused(r) => format!("refused {r}"),
                Ended::Untrusted(why) => {
                    format!("{why}: the server's authority is not the one this config pins")
                }
                Ended::Lost(why) => why.clone(),
                Ended::Shutdown => "shutdown".to_owned(),
            });
            s.next_delay = Some(delay);
        });
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = shutdown.changed() => return,
        }
    }
}
