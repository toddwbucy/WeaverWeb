//! **The browser's listener** (`docs/project/design-2026-10-07-iam.md`
//! section 3): TLS under a name where a certificate and key are configured,
//! plain HTTP otherwise, and the start refusals of the listed faults, each
//! naming its key, before anything listens.
//!
//! **The list is the promise.** This module refuses the faults the design
//! lists for the origin and the certificate, and nothing beyond them; any
//! other fault of the configuration shows at the first connection as the
//! browser's failure. The relying party's two refusals are the passkey pull
//! request's.
//!
//! **No crate is added.** axum is served over rustls through its own
//! `Listener` trait (`TlsListener`); the validity period is read with
//! `yasna`, already in the tree through rcgen, where `x509-parser` would add
//! thirteen crates; the name check is `rustls-webpki`'s, already in the tree
//! through rustls; the origin is parsed and serialized by `url`, the WHATWG
//! URL Standard, already in the tree through sqlx.

use crate::config::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

/// A certificate expiring within this long of the start is warned of.
pub const EXPIRY_WARNING: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// The most a TLS handshake may take before its connection is dropped.
pub const HANDSHAKE_BOUND: Duration = Duration::from_secs(10);

/// **The most TLS handshakes in flight at once.** A handful of people's
/// browsers open a few connections each; 256 is that with a wide margin.
/// The accepting task takes a permit before it accepts the next connection,
/// so a connection past the cap waits in the kernel's backlog rather than as
/// a task and TLS state in memory. This bounds memory before a byte is
/// authenticated; it is not a defence of availability, which the design
/// leaves out of scope: a client holding every permit delays the others by
/// at most `HANDSHAKE_BOUND`, then holds them again.
pub const HANDSHAKES_IN_FLIGHT: usize = 256;

/// **The shortest interval between two warnings of a refused certificate.**
/// Design section 3 promises that a certificate fault beyond its list shows
/// at the first connection as the browser's failure, which the server logs:
/// a handshake a client ends with a certificate alert is logged at warn.
/// Any unauthenticated client can send such an alert at will, so the warning
/// is given at most once in this interval, the next one counting those
/// suppressed, and the log is never the client's to fill.
pub const ALERT_WARNING_INTERVAL: Duration = Duration::from_secs(60);

/// **An origin as a browser serializes it** (design section 3): scheme,
/// host and a port where it is not the scheme's default, nothing else.
/// `host` is as the origin spells it, an IPv6 address in its brackets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl Origin {
    /// The origin as a browser writes it: scheme, `://`, host, and `:` and
    /// the port where one is held.
    pub fn serialized(&self) -> String {
        match self.port {
            Some(port) => format!("{}://{}:{port}", self.scheme, self.host),
            None => format!("{}://{}", self.scheme, self.host),
        }
    }

    /// The host as a TLS server name: a domain as spelled, an IPv6
    /// address without its brackets, so the name check reads it as an
    /// address against the certificate's IP names.
    pub fn server_name(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(&self.host)
    }
}

/// **The configured origin, refused where it is not a serialized origin.**
/// The value must be exactly what a browser sends in its `Origin` header,
/// which the server compares with it as a string, so it is parsed as a
/// browser parses a URL, by `url`, an implementation of the WHATWG URL
/// Standard, and **the configured value must equal the parsed URL's origin
/// in its ASCII serialization**, the browser's own algorithm. The port's
/// spelling, the case of scheme and host, an IPv6 address's canonical form,
/// a host a browser reads as an IPv4 address, and an internationalized
/// name's ASCII form are all the library's, as a browser applies them. A
/// refusal names the serialization a browser would send, and userinfo, a
/// path (a trailing `/` included), a query or a fragment by name.
pub fn parse_origin(origin: &str) -> Result<Origin, String> {
    let refuse = |why: &str| {
        Err(format!(
            "origin {origin:?} is not a serialized origin (scheme, host and a non-default port, nothing else): {why}"
        ))
    };
    let url = match url::Url::parse(origin) {
        Ok(url) => url,
        Err(e) => return refuse(&format!("it does not parse as a URL: {e}")),
    };
    let serialized = url.origin().ascii_serialization();
    if serialized != origin {
        // The equality is the rule; the reason names a part a bare origin
        // never carries, where there is one, for the operator.
        let why = if !url.username().is_empty() || url.password().is_some() {
            "it carries userinfo"
        } else if url.query().is_some() {
            "a query follows the host"
        } else if url.fragment().is_some() {
            "a fragment follows the host"
        } else if url.path() != "/" || origin.ends_with('/') {
            "a path follows the host"
        } else {
            "it is not as a browser serializes it"
        };
        return refuse(&format!("{why}; a browser sends it as {serialized:?}"));
    }
    let Some(host) = url.host_str() else {
        return refuse("no host");
    };
    Ok(Origin {
        scheme: url.scheme().to_owned(),
        host: host.to_owned(),
        port: url.port(),
    })
}

/// **The scheme rule**: `https`, or exactly `http` on the host `localhost`,
/// the one plain origin a browser treats as a secure context.
pub fn scheme_admits(origin: &Origin) -> Result<(), String> {
    match (origin.scheme.as_str(), origin.host.as_str()) {
        ("https", _) | ("http", "localhost") => Ok(()),
        _ => Err(format!(
            "origin's scheme is {}, and only https, or http on the host localhost, is a secure context",
            origin.scheme
        )),
    }
}

/// **A certificate's validity period**, read from its DER: the
/// TBSCertificate's `validity`, two times each a UTCTime or a
/// GeneralizedTime (RFC 5280, section 4.1.2.5).
pub fn validity(der: &[u8]) -> Result<(OffsetDateTime, OffsetDateTime), String> {
    use yasna::tags::{TAG_GENERALIZEDTIME, TAG_UTCTIME};
    let time = |r: yasna::BERReader| -> yasna::ASN1Result<OffsetDateTime> {
        if r.lookahead_tag()? == TAG_UTCTIME {
            Ok(*r.read_utctime()?.datetime())
        } else if r.lookahead_tag()? == TAG_GENERALIZEDTIME {
            Ok(*r.read_generalized_time()?.datetime())
        } else {
            Err(yasna::ASN1Error::new(yasna::ASN1ErrorKind::Invalid))
        }
    };
    yasna::parse_der(der, |r| {
        r.read_sequence(|r| {
            let period = r.next().read_sequence(|r| {
                // version [0] EXPLICIT, absent for a version 1 certificate.
                r.read_optional(|r| r.read_tagged(yasna::Tag::context(0), |r| r.read_der()))?;
                r.next().read_der()?; // serialNumber
                r.next().read_der()?; // signature
                r.next().read_der()?; // issuer
                let period = r
                    .next()
                    .read_sequence(|r| Ok((time(r.next())?, time(r.next())?)))?;
                // subject, subjectPublicKeyInfo, and the optional rest.
                while r.read_optional(|r| r.read_der())?.is_some() {}
                Ok(period)
            })?;
            r.next().read_der()?; // signatureAlgorithm
            r.next().read_der()?; // signatureValue
            Ok(period)
        })
    })
    .map_err(|e| format!("the certificate's validity period could not be read: {e}"))
}

/// **The browser's TLS, or why the server refuses to start**: `None` where
/// no certificate and key are configured and the origin, if any, admits a
/// plain listener; the TLS configuration and any warning otherwise. `now`
/// is the clock at start.
pub fn browser_tls(
    cfg: &ServerConfig,
    now: OffsetDateTime,
) -> anyhow::Result<Option<(Arc<rustls::ServerConfig>, Vec<String>)>> {
    let origin = match &cfg.origin {
        Some(origin) => {
            let parsed = parse_origin(origin).map_err(anyhow::Error::msg)?;
            scheme_admits(&parsed).map_err(anyhow::Error::msg)?;
            Some(parsed)
        }
        None => None,
    };
    let (certificate, key) = match (&cfg.tls_certificate, &cfg.tls_key) {
        (None, None) => {
            if origin.as_ref().is_some_and(|o| o.scheme == "https") {
                anyhow::bail!(
                    "origin is https but tls_certificate and tls_key are not configured, so the listener would serve it in the clear"
                );
            }
            return Ok(None);
        }
        (Some(certificate), Some(key)) => (certificate, key),
        (Some(_), None) => anyhow::bail!("tls_certificate is configured and tls_key is not"),
        (None, Some(_)) => anyhow::bail!("tls_key is configured and tls_certificate is not"),
    };
    let chain: Vec<CertificateDer<'static>> = {
        let pem = std::fs::read(certificate).map_err(|e| {
            anyhow::anyhow!(
                "tls_certificate {} could not be read: {e}",
                certificate.display()
            )
        })?;
        rustls_pemfile::certs(&mut pem.as_slice())
            .collect::<Result<_, _>>()
            .map_err(|e| {
                anyhow::anyhow!("tls_certificate {} is not PEM: {e}", certificate.display())
            })?
    };
    let Some(leaf) = chain.first().cloned() else {
        anyhow::bail!(
            "tls_certificate {} holds no certificate",
            certificate.display()
        );
    };
    let private: PrivateKeyDer<'static> = {
        let pem = std::fs::read(key)
            .map_err(|e| anyhow::anyhow!("tls_key {} could not be read: {e}", key.display()))?;
        rustls_pemfile::private_key(&mut pem.as_slice())
            .map_err(|e| anyhow::anyhow!("tls_key {} is not PEM: {e}", key.display()))?
            .ok_or_else(|| anyhow::anyhow!("tls_key {} holds no private key", key.display()))?
    };
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()?
    .with_no_client_auth()
    .with_single_cert(chain, private)
    .map_err(|e| anyhow::anyhow!("tls_certificate and tls_key do not load as a pair: {e}"))?;
    // **The name check waits for an origin**: without one there is no name
    // the listener answers under to check the certificate against.
    if let Some(origin) = &origin {
        let name = ServerName::try_from(origin.server_name().to_owned()).map_err(|e| {
            anyhow::anyhow!("origin's host {} is not a server name: {e}", origin.host)
        })?;
        let cert = webpki::EndEntityCert::try_from(&leaf).map_err(|e| {
            anyhow::anyhow!("tls_certificate is not a certificate webpki reads: {e:?}")
        })?;
        cert.verify_is_valid_for_subject_name(&name).map_err(|e| {
            anyhow::anyhow!(
                "tls_certificate is not valid for the origin's host {} ({e:?}), so every browser would refuse the listener",
                origin.host
            )
        })?;
    }
    let (not_before, not_after) = validity(leaf.as_ref()).map_err(anyhow::Error::msg)?;
    if now < not_before {
        anyhow::bail!("tls_certificate is not valid until {not_before}, after the clock at start");
    }
    if now > not_after {
        anyhow::bail!("tls_certificate expired at {not_after}, before the clock at start");
    }
    let mut warnings = Vec::new();
    if not_after - now < EXPIRY_WARNING {
        warnings.push(format!(
            "tls_certificate expires at {not_after}, within fourteen days; renew it"
        ));
    }
    Ok(Some((Arc::new(config), warnings)))
}

/// **A TLS listener for axum**: TCP accepted on one task, each handshake on
/// a task of its own bounded by `HANDSHAKE_BOUND`, at most
/// `HANDSHAKES_IN_FLIGHT` of them at once, and the finished streams handed
/// to axum in the order they complete, so a slow client never stalls the
/// others while a permit is free. A connection whose handshake fails, a
/// plain HTTP request among them, is dropped and reaches no surface.
pub struct TlsListener {
    streams: tokio::sync::mpsc::Receiver<(
        tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
        std::net::SocketAddr,
    )>,
    local: std::net::SocketAddr,
}

/// **The certificate alert a failed handshake received, if it was one**:
/// the client refusing this listener's certificate, which is the operator's
/// to fix. Any other failure, an end of stream, a timeout, a client that does
/// not speak TLS, is a scanner's noise and is not one.
fn certificate_alert(error: &std::io::Error) -> Option<rustls::AlertDescription> {
    use rustls::AlertDescription as A;
    match error.get_ref()?.downcast_ref::<rustls::Error>()? {
        rustls::Error::AlertReceived(
            alert @ (A::BadCertificate
            | A::UnsupportedCertificate
            | A::CertificateRevoked
            | A::CertificateExpired
            | A::CertificateUnknown
            | A::UnknownCA),
        ) => Some(*alert),
        _ => None,
    }
}

/// The warnings of refused certificates, at most one per `interval`.
struct AlertWarnings {
    interval: Duration,
    last: Option<std::time::Instant>,
    suppressed: u64,
}

impl AlertWarnings {
    /// One refusal noted at `now`: the count suppressed since the last
    /// warning where this one is to be warned, `None` where it is suppressed.
    fn note(&mut self, now: std::time::Instant) -> Option<u64> {
        match self.last {
            Some(last) if now.duration_since(last) < self.interval => {
                self.suppressed += 1;
                None
            }
            _ => {
                self.last = Some(now);
                Some(std::mem::take(&mut self.suppressed))
            }
        }
    }
}

impl TlsListener {
    pub fn new(
        tcp: tokio::net::TcpListener,
        tls: Arc<rustls::ServerConfig>,
    ) -> std::io::Result<Self> {
        Self::bounded(
            tcp,
            tls,
            HANDSHAKES_IN_FLIGHT,
            HANDSHAKE_BOUND,
            ALERT_WARNING_INTERVAL,
        )
    }

    /// The listener with its cap, bound and warning interval given, which a
    /// test sets small.
    pub(crate) fn bounded(
        tcp: tokio::net::TcpListener,
        tls: Arc<rustls::ServerConfig>,
        in_flight: usize,
        bound: Duration,
        alert_interval: Duration,
    ) -> std::io::Result<Self> {
        let local = tcp.local_addr()?;
        let acceptor = tokio_rustls::TlsAcceptor::from(tls);
        let (sender, streams) = tokio::sync::mpsc::channel(64);
        let handshakes = Arc::new(tokio::sync::Semaphore::new(in_flight));
        let warnings = Arc::new(std::sync::Mutex::new(AlertWarnings {
            interval: alert_interval,
            last: None,
            suppressed: 0,
        }));
        tokio::spawn(async move {
            loop {
                // The permit before the accept: past the cap, a connection
                // waits in the kernel's backlog, not in memory.
                let Ok(permit) = handshakes.clone().acquire_owned().await else {
                    break;
                };
                let (socket, peer) = match tcp.accept().await {
                    Ok(accepted) => accepted,
                    Err(e) => {
                        tracing::warn!("the browser's listener could not accept: {e}");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                };
                let (acceptor, handed, warnings) =
                    (acceptor.clone(), sender.clone(), warnings.clone());
                tokio::spawn(async move {
                    let _permit = permit;
                    match tokio::time::timeout(bound, acceptor.accept(socket)).await {
                        Ok(Ok(stream)) => {
                            let _ = handed.send((stream, peer)).await;
                        }
                        Ok(Err(e)) => match certificate_alert(&e) {
                            Some(alert) => {
                                let warn = warnings
                                    .lock()
                                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                                    .note(std::time::Instant::now());
                                if let Some(suppressed) = warn {
                                    tracing::warn!(
                                        "a client refused the browser's listener's certificate with the alert {alert:?} ({suppressed} more refusals since the last warning, not logged); the certificate is the operator's to fix"
                                    );
                                }
                            }
                            None => tracing::debug!("a TLS handshake from {peer} failed: {e}"),
                        },
                        Err(_) => tracing::debug!("a TLS handshake from {peer} passed its bound"),
                    }
                });
                if sender.is_closed() {
                    break;
                }
            }
        });
        Ok(Self { streams, local })
    }
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.streams.recv().await {
            Some(accepted) => accepted,
            // The accepting task ends only with the listener's receiver.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        Ok(self.local)
    }
}
