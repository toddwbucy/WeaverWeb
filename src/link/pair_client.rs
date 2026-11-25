//! **The box's half of the pairing** (Spec 8.2), as a library: the dial
//! under the pairing protocol, the check of the chain the console presents,
//! the first frame, the code, and the wait. `admin-con pair` is a command
//! over it, and the server's tests drive a real pairing client through it.
//! It builds without the default features, since the connectors carry it.
//!
//! **The check at the pairing is the chain and no name.** Before the
//! exchange's step 4 the box knows no name the server's certificate carries
//! and holds no trust anchor, so it checks that the server's certificate
//! chains to the authority certificate presented after it, and prints that
//! authority's fingerprint for the operator to compare with the console's.
//! Every plane connection after the exchange keeps `client_tls` as built,
//! with its name check. **Trust at the pairing comes from the operator's
//! comparison and the code**, which is the session's exporter and differs
//! at the two ends where a party terminates TLS between them.

use crate::link::authority::{fingerprint, provider};
use crate::link::frames::{
    Line, LineReader, PAIR_FIRST_FRAME_BOUND, PAIR_PROTOCOL, PairFromBox, PairToBox,
};
use crate::link::pair_code::{self, Code};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// The bound on the TCP connection, on the handshake, and on the console's
/// answer to the first frame: the listener's own handshake bound.
pub const DIAL_SECS: u64 = 10;

/// The longest reason from the console the box prints. Longer, or not
/// printable ASCII, is the console's fault and never printed (Spec 8.2).
pub const REASON_BOUND: usize = 64;

/// What the box claims in its first frame: its name, in section 2.12's
/// grammar, and admin-con's certificate request (Spec 8.2).
#[derive(Debug, Clone)]
pub struct Claim {
    pub r#box: String,
    pub admin_request: String,
}

/// Why a dial ended before the box waits.
#[derive(Debug)]
pub enum DialError {
    /// No TCP connection, no TLS handshake, or no pairing protocol at the
    /// address: `pair`'s `unreachable`.
    Unreachable(String),
    /// The console refused, with its reason, checked printable.
    Refused(String),
    /// The console answered `expired`.
    Expired,
    /// The console spoke out of turn, closed, or went silent.
    Fault(String),
}

impl std::fmt::Display for DialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DialError::Unreachable(why) => write!(f, "unreachable: {why}"),
            DialError::Refused(reason) => write!(f, "refused: {reason}"),
            DialError::Expired => f.write_str("expired"),
            DialError::Fault(why) => write!(f, "fault: {why}"),
        }
    }
}

/// What a waiting box hears next, heartbeats aside.
#[derive(Debug)]
pub enum Heard {
    /// Ten minutes from the dial with no code entered.
    Expired,
    /// The console refused, with its reason, checked printable.
    Refused(String),
    /// The console closed with no word.
    Closed,
    /// Nothing for four heartbeats' cadence.
    Silent,
    /// The console spoke out of turn.
    Fault(String),
    /// **The exchange's answer** (Spec 8.2, step 4), its shapes checked and
    /// its authority the one the handshake presented. The box writes it,
    /// then sends its word (`Dialed::written`).
    Answer(Answer),
}

/// **The exchange's answer as the box holds it** (Spec 8.2): the members a
/// config needs that the box did not know, each checked in its shape, and
/// the certificates in PEM. `adopt` checks each certificate against its
/// own key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub agent: String,
    pub agent_id: String,
    pub r#box: String,
    pub server_name: String,
    pub authority: String,
    pub admin: String,
}

/// **The console's last word**, after the box's (Spec 8.2, step 6).
#[derive(Debug, PartialEq, Eq)]
pub enum LastWord {
    /// The row stands with the answer's certificate live.
    Paired,
    /// The console refused, with its reason, checked printable.
    Refused(String),
    /// The connection ended, or went silent, after the box's word and
    /// before the console's: the console may have written the row, and the
    /// Pair page says what the write made.
    Unknown,
}

/// A box waiting on the console: its connection, its code and the
/// authority it met.
pub struct Dialed {
    reader: LineReader<TlsStream<TcpStream>>,
    code: Code,
    authority: CertificateDer<'static>,
    expires_in: Duration,
    cadence: Duration,
    /// Every line the box sent and heard, in order, for the test that no
    /// frame carries the code.
    #[cfg(test)]
    pub(crate) transcript: Vec<String>,
}

impl Dialed {
    /// The code the box shows, derived from this connection's exporter.
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// The authority's certificate the console presented.
    pub fn authority(&self) -> &CertificateDer<'static> {
        &self.authority
    }

    /// The authority's fingerprint as the box prints it: sixty-four hex
    /// digits in sixteen groups of four.
    pub fn fingerprint(&self) -> String {
        pair_code::grouped(&fingerprint(self.authority.as_ref()))
    }

    /// What was left of the box's ten minutes when the console answered.
    pub fn expires_in(&self) -> Duration {
        self.expires_in
    }

    /// **An answer the box takes, or why not**, in fixed words: every
    /// member in its shape, each certificate one PEM block, and the
    /// authority the one the handshake presented, whose fingerprint the
    /// box printed (Spec 8.2), so `adopt` verifies under the authority the
    /// operator compared.
    fn checked(&self, answer: Answer) -> Result<Answer, &'static str> {
        if !crate::store::identity::is_agent_id(&answer.agent_id)
            || crate::link::verbs::well_formed(&answer.r#box, &answer.agent).is_err()
            || ServerName::try_from(answer.server_name.as_str()).is_err()
        {
            return Err("the console's answer names a member out of its shape");
        }
        let authority = one_certificate(&answer.authority)
            .ok_or("the answer's authority is not one certificate")?;
        if authority != self.authority {
            return Err("the answer's authority is not the one the handshake presented");
        }
        if one_certificate(&answer.admin).is_none() {
            return Err("the answer's certificate is not one certificate");
        }
        Ok(answer)
    }

    /// **The box's word** (Spec 8.2, step 5): it wrote the answer.
    pub async fn written(&mut self) -> Result<(), DialError> {
        let mut line = serde_json::to_string(&PairFromBox::Written).expect("a frame serializes");
        line.push('\n');
        #[cfg(test)]
        self.transcript.push(line.trim_end().to_owned());
        let stream = self.reader.get_mut();
        match tokio::time::timeout(Duration::from_secs(DIAL_SECS), async {
            stream.write_all(line.as_bytes()).await?;
            stream.flush().await
        })
        .await
        {
            Ok(Ok(())) => Ok(()),
            _ => Err(DialError::Fault("the box's word was not sent".into())),
        }
    }

    /// **The console's last word** after the box's (Spec 8.2, step 6),
    /// heartbeats aside, which the console sends while its write runs:
    /// `paired`, a refusal, or, where the connection ends or stays silent
    /// for four heartbeats' cadence first, unknown.
    pub async fn last_word(&mut self) -> LastWord {
        let line = loop {
            let line = match tokio::time::timeout(self.cadence * 4, self.reader.next()).await {
                Ok(Line::Frame(line)) => line,
                _ => return LastWord::Unknown,
            };
            #[cfg(test)]
            self.transcript.push(line.clone());
            if !matches!(decode(&line), Some(PairToBox::Heartbeat)) {
                break line;
            }
        };
        match decode(&line) {
            Some(PairToBox::Paired) => LastWord::Paired,
            Some(PairToBox::Refused { reason }) => match printable(&reason) {
                Some(reason) => LastWord::Refused(reason),
                None => LastWord::Unknown,
            },
            _ => LastWord::Unknown,
        }
    }

    /// The next thing the console says, heartbeats aside. Silent past four
    /// heartbeats' cadence.
    pub async fn next(&mut self) -> Heard {
        let silence = self.cadence * 4;
        loop {
            let line = match tokio::time::timeout(silence, self.reader.next()).await {
                Err(_) => return Heard::Silent,
                Ok(Line::Closed) => return Heard::Closed,
                Ok(Line::Malformed(why)) => return Heard::Fault(why.to_owned()),
                Ok(Line::Frame(line)) => line,
            };
            #[cfg(test)]
            self.transcript.push(line.clone());
            match decode(&line) {
                Some(PairToBox::Heartbeat) => continue,
                Some(PairToBox::Answer {
                    agent,
                    agent_id,
                    r#box,
                    server_name,
                    authority,
                    admin,
                }) => {
                    let answer = Answer {
                        agent,
                        agent_id,
                        r#box,
                        server_name,
                        authority,
                        admin,
                    };
                    return match self.checked(answer) {
                        Ok(answer) => Heard::Answer(answer),
                        Err(why) => Heard::Fault(why.into()),
                    };
                }
                Some(PairToBox::Expired) => return Heard::Expired,
                Some(PairToBox::Refused { reason }) => {
                    return match printable(&reason) {
                        Some(reason) => Heard::Refused(reason),
                        None => Heard::Fault("the console's reason is not printable".into()),
                    };
                }
                Some(other) => {
                    let named = named(&other);
                    return Heard::Fault(format!("the console sent {named} out of turn"));
                }
                None => return Heard::Fault(NO_FRAME.into()),
            }
        }
    }
}

/// What a line that is no pairing frame is called. **Never serde's error**,
/// which quotes the line, and the line is the console's: a tag it made up
/// would reach a printed fault with whatever escapes it holds.
const NO_FRAME: &str = "the console sent no pairing frame";

/// The console's frame, or none.
fn decode(line: &str) -> Option<PairToBox> {
    serde_json::from_str(line).ok()
}

/// A frame as the box names it in a fault: its tag alone, never a member
/// the console wrote.
fn named(frame: &PairToBox) -> &'static str {
    match frame {
        PairToBox::Waiting { .. } => "waiting",
        PairToBox::Heartbeat => "heartbeat",
        PairToBox::Expired => "expired",
        PairToBox::Refused { .. } => "refused",
        PairToBox::Answer { .. } => "answer",
        PairToBox::Paired => "paired",
    }
}

/// One certificate in PEM, alone, as DER.
fn one_certificate(pem: &str) -> Option<CertificateDer<'static>> {
    let mut bytes = pem.as_bytes();
    let mut certificates = rustls_pemfile::certs(&mut bytes);
    let first = certificates.next()?.ok()?;
    certificates.next().is_none().then_some(first)
}

/// A reason as the box prints it: printable ASCII within `REASON_BOUND`, or
/// nothing.
fn printable(reason: &str) -> Option<String> {
    (!reason.is_empty()
        && reason.len() <= REASON_BOUND
        && reason.bytes().all(|b| (0x20..0x7f).contains(&b)))
    .then(|| reason.to_owned())
}

/// The host of `host:port`, an IPv6 literal's brackets dropped.
fn host_of(address: &str) -> Option<&str> {
    let (host, port) = address.rsplit_once(':')?;
    port.parse::<u16>().ok()?;
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    (!host.is_empty()).then_some(host)
}

/// **Dial the console's link port under the pairing protocol**, send the
/// first frame and read the console's answer: waiting, or why not.
pub async fn dial(address: &str, claim: &Claim) -> Result<Dialed, DialError> {
    let host = host_of(address)
        .ok_or_else(|| DialError::Fault(format!("{address:?} is not a host and a port")))?;
    let tcp =
        match tokio::time::timeout(Duration::from_secs(DIAL_SECS), TcpStream::connect(address))
            .await
        {
            Ok(Ok(tcp)) => tcp,
            Ok(Err(e)) => return Err(DialError::Unreachable(format!("{address}: {e}"))),
            Err(_) => {
                return Err(DialError::Unreachable(format!(
                    "{address}: no connection within {DIAL_SECS} s"
                )));
            }
        };
    over(tcp, host, claim).await
}

/// The dial over a connection already made, which the tests make from an
/// address of their choosing.
pub(crate) async fn over(tcp: TcpStream, host: &str, claim: &Claim) -> Result<Dialed, DialError> {
    let name = ServerName::try_from(host.to_owned())
        .map_err(|e| DialError::Fault(format!("{host:?} is not a host: {e}")))?;
    let tls = match tokio::time::timeout(
        Duration::from_secs(DIAL_SECS),
        TlsConnector::from(pairing_client_tls()).connect(name, tcp),
    )
    .await
    {
        Ok(Ok(tls)) => tls,
        Ok(Err(e)) => return Err(DialError::Unreachable(format!("no TLS handshake: {e}"))),
        Err(_) => {
            return Err(DialError::Unreachable(format!(
                "no TLS handshake within {DIAL_SECS} s"
            )));
        }
    };
    let (_, connection) = tls.get_ref();
    if connection.alpn_protocol() != Some(PAIR_PROTOCOL) {
        return Err(DialError::Unreachable(
            "the console did not answer the pairing protocol".into(),
        ));
    }
    let authority = connection
        .peer_certificates()
        .and_then(|chain| chain.get(1))
        .map(|c| c.clone().into_owned())
        .ok_or_else(|| DialError::Fault("the console presented no authority".into()))?;
    let code = Code::of(&**connection)
        .map_err(|e| DialError::Fault(format!("the connection's exporter: {e}")))?;

    let mut reader = LineReader::with_bound(tls, PAIR_FIRST_FRAME_BOUND);
    #[cfg(test)]
    let mut transcript = Vec::new();
    let hello = PairFromBox::Hello {
        r#box: claim.r#box.clone(),
        admin_request: claim.admin_request.clone(),
    };
    let mut line = serde_json::to_string(&hello).expect("a frame serializes");
    #[cfg(test)]
    transcript.push(line.clone());
    line.push('\n');
    let sent = tokio::time::timeout(Duration::from_secs(DIAL_SECS), async {
        let stream = reader.get_mut();
        stream.write_all(line.as_bytes()).await?;
        stream.flush().await
    })
    .await;
    if !matches!(sent, Ok(Ok(()))) {
        return Err(DialError::Fault("the first frame was not sent".into()));
    }

    let answer = match tokio::time::timeout(Duration::from_secs(DIAL_SECS), reader.next()).await {
        Ok(Line::Frame(line)) => line,
        Ok(Line::Closed) => return Err(DialError::Fault("the console closed".into())),
        Ok(Line::Malformed(why)) => return Err(DialError::Fault(why.into())),
        Err(_) => {
            return Err(DialError::Fault(format!(
                "the console did not answer within {DIAL_SECS} s"
            )));
        }
    };
    #[cfg(test)]
    transcript.push(answer.clone());
    match decode(&answer) {
        Some(PairToBox::Waiting {
            expires_in_secs,
            cadence_secs,
        }) if cadence_secs > 0 && cadence_secs <= crate::link::frames::CADENCE_MAX_SECS => {
            Ok(Dialed {
                reader,
                code,
                authority,
                expires_in: Duration::from_secs(expires_in_secs),
                cadence: Duration::from_secs(cadence_secs),
                #[cfg(test)]
                transcript,
            })
        }
        Some(PairToBox::Refused { reason }) => Err(match printable(&reason) {
            Some(reason) => DialError::Refused(reason),
            None => DialError::Fault("the console's reason is not printable".into()),
        }),
        Some(PairToBox::Expired) => Err(DialError::Expired),
        Some(other) => Err(DialError::Fault(format!(
            "the console answered {named} to the first frame",
            named = named(&other)
        ))),
        None => Err(DialError::Fault(NO_FRAME.into())),
    }
}

/// The box's TLS configuration at the pairing: TLS 1.3 alone, ALPN
/// `weaver-pair/1`, no client certificate, and the presented authority's
/// check in place of a trust anchor.
pub(crate) fn pairing_client_tls() -> Arc<rustls::ClientConfig> {
    let provider = provider();
    let mut config = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("the ring provider speaks TLS 1.3")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(PresentedAuthority { provider }))
        .with_no_client_auth();
    config.alpn_protocols = vec![PAIR_PROTOCOL.to_vec()];
    Arc::new(config)
}

/// **The pairing's check of the console's chain**: exactly the server's
/// certificate and one authority certificate, the first chaining to the
/// second as its trust anchor, and no name.
#[derive(Debug)]
struct PresentedAuthority {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PresentedAuthority {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let [authority] = intermediates else {
            return Err(rustls::Error::General(
                "the console must present its certificate and the authority's alone".into(),
            ));
        };
        let mut roots = rustls::RootCertStore::empty();
        roots.add(authority.clone().into_owned()).map_err(|e| {
            rustls::Error::General(format!("the presented authority is not one: {e}"))
        })?;
        let certificate = rustls::server::ParsedCertificate::try_from(end_entity)?;
        rustls::client::verify_server_cert_signed_by_trust_anchor(
            &certificate,
            &roots,
            &[],
            now,
            self.provider.signature_verification_algorithms.all,
        )?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General(
            "the pairing is TLS 1.3 alone".into(),
        ))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_is_read_from_its_address() {
        assert_eq!(host_of("127.0.0.1:7443"), Some("127.0.0.1"));
        assert_eq!(host_of("[::1]:7443"), Some("::1"));
        assert_eq!(host_of("console.example:7443"), Some("console.example"));
        assert_eq!(host_of("console.example"), None);
        assert_eq!(host_of(":7443"), None);
        assert_eq!(host_of("console.example:port"), None);
    }

    #[test]
    fn a_reason_is_printed_only_when_printable_and_short() {
        assert_eq!(printable("busy").as_deref(), Some("busy"));
        assert_eq!(printable("bu\nsy"), None);
        assert_eq!(printable("\u{1b}[31mbusy"), None);
        assert_eq!(printable(""), None);
        assert_eq!(printable(&"x".repeat(REASON_BOUND + 1)), None);
    }
}
