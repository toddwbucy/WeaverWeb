//! conforms: web-a-clienthello-without-the-pairing-protocol-meets-mutual-tls-as-built
//! conforms: web-a-pairing-connection-writes-nothing-before-its-code-matches
//! conforms: web-the-pairing-code-crosses-the-link-in-no-frame
//! conforms: web-the-pairing-code-is-the-first-50-bits-of-a-32-byte-export
//! conforms: web-a-party-between-the-box-and-the-console-meets-a-code-that-matches-nothing
//!
//! The pairing connection, both ends (Spec 8.2): the link port's choice of
//! configuration by ALPN, the waiting boxes and their bounds, the code at
//! both ends, and a party between them. Each test runs against a real
//! listener on a store of its own (`fresh_store`), with the box's half
//! (`pair_client`) as the box. No test reaches an agent or a box.
//!
//! Every perturbation below names its guard, and the sub-act that landed it
//! showed the test failing with the guard removed, per Spec section 9.

use super::authority::{Authority, provider};
use super::frames::{Line, LineReader, PAIR_FIRST_FRAME_BOUND, PairFromBox, PairToBox};
use super::listener::Listener;
use super::pair_client::{self, Claim, DialError, Dialed, Heard};
use super::pair_code::{self, Code, EXPORT_BYTES, LABEL};
use super::pairing::Bounds;
use crate::store::read::tests::{Fresh, fresh_store};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpSocket, TcpStream};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// The least silence bound the link takes: a cadence of one second, so a
/// test hears heartbeats.
const SILENCE: Duration = Duration::from_secs(4);
const SOON: Duration = Duration::from_secs(5);

struct Rig {
    listener: Listener,
    authority: Authority,
    dir: tempfile::TempDir,
    fresh: Fresh,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.listener.stop_now();
    }
}

async fn rig() -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = crate::test_dir::tempdir().unwrap();
    let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
    let listener = Listener::start_with(
        fresh.store.clone(),
        &authority,
        "127.0.0.1:0",
        SILENCE,
        Some(dir.path().join("operations.log")),
    )
    .await
    .unwrap();
    Some(Rig {
        listener,
        authority,
        dir,
        fresh,
    })
}

/// A box's claim. The requests are text alone here, since no test of this
/// module reaches the exchange that verifies them (`enrollment_tests`).
fn claim(r#box: &str) -> Claim {
    Claim {
        r#box: r#box.into(),
        admin_request:
            "-----BEGIN CERTIFICATE REQUEST-----\nadmin\n-----END CERTIFICATE REQUEST-----\n".into(),
    }
}

/// A TCP connection to the listener from a loopback address of the test's
/// choosing, so the bounds per peer can be met.
async fn tcp_from(to: SocketAddr, from: Ipv4Addr) -> TcpStream {
    let socket = TcpSocket::new_v4().unwrap();
    socket.bind(SocketAddr::new(IpAddr::V4(from), 0)).unwrap();
    socket.connect(to).await.unwrap()
}

async fn pair_from(rig: &Rig, from: Ipv4Addr, r#box: &str) -> Result<Dialed, DialError> {
    let tcp = tcp_from(rig.listener.address(), from).await;
    pair_client::over(tcp, "127.0.0.1", &claim(r#box)).await
}

fn local(n: u8) -> Ipv4Addr {
    Ipv4Addr::new(127, 0, 0, n)
}

/// The console's fingerprint in the pairing's grouping.
fn console_fingerprint(authority: &Authority) -> String {
    pair_code::grouped(&authority.fingerprint())
}

/// Every table of the store, each as a digest of its rows: what a write of
/// any kind would change.
async fn every_table(rig: &Rig) -> Vec<(String, String)> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables
          WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
          ORDER BY table_name",
    )
    .fetch_all(&rig.fresh.store.pool)
    .await
    .unwrap();
    let mut digests = Vec::new();
    for table in tables {
        let digest: String = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT coalesce(md5(string_agg(t::text, '|' ORDER BY t::text)), '') FROM \"{table}\" t"
        )))
        .fetch_one(&rig.fresh.store.pool)
        .await
        .unwrap();
        digests.push((table, digest));
    }
    digests
}

/// Wait, briefly, until the listener lists `n` boxes.
async fn until_waiting(rig: &Rig, n: usize) {
    let by = tokio::time::Instant::now() + SOON;
    while rig.listener.waiting().len() != n {
        assert!(
            tokio::time::Instant::now() < by,
            "{} boxes wait, not {n}",
            rig.listener.waiting().len()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn authority_der(authority: &Authority) -> rustls::pki_types::CertificateDer<'static> {
    rustls_pemfile::certs(&mut authority.certificate_pem().as_bytes())
        .next()
        .unwrap()
        .unwrap()
}

/// The alert a TLS error carries, where it is one received.
fn alert_of(e: &std::io::Error) -> Option<rustls::AlertDescription> {
    match e.get_ref()?.downcast_ref::<rustls::Error>()? {
        rustls::Error::AlertReceived(alert) => Some(*alert),
        _ => None,
    }
}

/// **A ClientHello without the pairing protocol meets the mutual-TLS
/// configuration as built** (Spec 8.2, section 9). A client that trusts the
/// authority, names the server, offers no ALPN and holds no certificate
/// says the pairing's first frame and is refused by the handshake itself,
/// with the alert `certificate_required`, and no box waits. The same box
/// under the pairing protocol waits.
///
/// Perturbation: choose the pairing configuration for every ClientHello
/// (`pairs` in `serve_connection`), and the certificate-less client is
/// answered `waiting`. Perturbation: let the mutual-TLS configuration
/// admit a client with no certificate, as every standing link test still
/// passes, and the handshake completes with no alert.
#[tokio::test]
async fn a_clienthello_without_the_pairing_protocol_meets_mutual_tls_as_built() {
    let Some(rig) = rig().await else { return };
    let mut roots = rustls::RootCertStore::empty();
    roots.add(authority_der(&rig.authority)).unwrap();
    let config = rustls::ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tcp = TcpStream::connect(rig.listener.address()).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("weaver-web").unwrap();
    let refused_by = match TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
    {
        Err(e) => alert_of(&e),
        Ok(mut tls) => {
            let hello = PairFromBox::Hello {
                r#box: "box-a".into(),
                admin_request: "admin".into(),
            };
            let line = serde_json::to_string(&hello).unwrap() + "\n";
            let _ = tls.write_all(line.as_bytes()).await;
            let _ = tls.flush().await;
            let mut answer = String::new();
            match tokio::time::timeout(SOON, BufReader::new(tls).read_line(&mut answer)).await {
                Ok(Ok(n)) if n > 0 => panic!("a certificate-less client was answered {answer}"),
                Ok(Err(e)) => alert_of(&e),
                other => panic!("a certificate-less client met no refusal: {other:?}"),
            }
        }
    };
    assert_eq!(
        refused_by,
        Some(rustls::AlertDescription::CertificateRequired)
    );
    assert!(
        rig.listener.waiting().is_empty(),
        "a certificate-less client waits"
    );

    let dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    assert!(dialed.expires_in() > Duration::ZERO);
    until_waiting(&rig, 1).await;
}

/// **A pairing connection writes nothing before its code matches** (Spec
/// 8.2, section 9): a box that dials, says its first frame, waits through
/// heartbeats and is met by a code that is not its own leaves every table
/// of the store as it was and the operations log empty.
///
/// Perturbation: write to the store as a pairing connection is served (the
/// epoch's increment, at the dispatch), and a table's rows change.
#[tokio::test]
async fn a_pairing_connection_writes_nothing_before_its_code_matches() {
    let Some(rig) = rig().await else { return };
    let before = every_table(&rig).await;
    let mut dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    until_waiting(&rig, 1).await;
    let wrong = Code::read("0000000000").unwrap();
    assert!(
        !wrong.matches(dialed.code()),
        "the made-up code is the box's"
    );
    assert!(rig.listener.take_pairing(&wrong).await.is_none());
    // Two heartbeats' time, the box still waiting.
    assert!(
        tokio::time::timeout(Duration::from_millis(2500), dialed.next())
            .await
            .is_err()
    );
    assert_eq!(rig.listener.waiting().len(), 1);
    let after = every_table(&rig).await;
    for ((table, was), (_, is)) in before.iter().zip(after.iter()) {
        assert_eq!(was, is, "{table} changed while a box waited");
    }
    let log = std::fs::read_to_string(rig.dir.path().join("operations.log")).unwrap_or_default();
    assert!(log.is_empty(), "the operations log holds {log:?}");
}

/// **The code crosses the link in no frame** (Spec 8.2, section 9): every
/// line the box sent and heard, from its first frame through its
/// heartbeats to `expired`, holds the code in no form.
///
/// Perturbation: send the code in the first frame, and the recorded stream
/// holds it.
#[tokio::test]
async fn the_pairing_code_crosses_the_link_in_no_frame() {
    let Some(rig) = rig().await else { return };
    rig.listener.set_pairing_bounds(Bounds {
        life: Duration::from_secs(3),
        ..Bounds::default()
    });
    let mut dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    let shown = dialed.code().to_string();
    let heard = tokio::time::timeout(SOON, dialed.next()).await;
    assert!(matches!(heard, Ok(Heard::Expired)), "{heard:?}");
    let transcript = dialed.transcript.join("\n");
    assert!(transcript.contains("\"svc\":\"hello\""), "{transcript}");
    assert!(transcript.contains("\"svc\":\"heartbeat\""), "{transcript}");
    assert!(transcript.contains("\"svc\":\"expired\""), "{transcript}");
    let packed = shown.replace(' ', "");
    for form in [
        shown.clone(),
        packed.clone(),
        packed.to_lowercase(),
        shown.to_lowercase(),
    ] {
        assert!(
            !transcript.contains(&form),
            "the stream holds the code as {form:?}"
        );
    }
}

/// **Both ends derive one code, from a 32-byte export** (Spec 8.2, section
/// 9): over one session of the pairing configuration the box's half and
/// the console's derive the same code, and an export of 16 bytes under the
/// same label begins with other bits, so its code is another. The encoding
/// itself is `pair_code`'s test of the Spec's example.
#[tokio::test]
async fn both_ends_derive_one_code_from_a_32_byte_export() {
    let dir = crate::test_dir::tempdir().unwrap();
    let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
    let (box_io, console_io) = tokio::io::duplex(64 * 1024);
    let name = rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();
    let (console, r#box) = tokio::join!(
        TlsAcceptor::from(authority.pairing_tls().unwrap()).accept(console_io),
        TlsConnector::from(pair_client::pairing_client_tls()).connect(name, box_io),
    );
    let (console, r#box) = (console.unwrap(), r#box.unwrap());
    let at_console = Code::of(&**console.get_ref().1).unwrap();
    let at_box = Code::of(&**r#box.get_ref().1).unwrap();
    assert!(
        at_console.matches(&at_box),
        "the two ends derived two codes"
    );

    let short = r#box
        .get_ref()
        .1
        .export_keying_material([0u8; 16], LABEL, Some(&[]))
        .unwrap();
    let mut padded = [0u8; EXPORT_BYTES];
    padded[..16].copy_from_slice(&short);
    assert!(
        !Code::from_export(&padded).matches(&at_box),
        "a 16-byte export gave the 32-byte export's code"
    );
}

/// **A party between the box and the console meets a code that matches
/// nothing** (Spec 8.2, section 9). An in-process relay terminates the
/// box's TLS under an authority of its own and dials the console as a box
/// itself, passing the first frame on. The box waits, and the code it
/// shows matches no waiting connection, while the relay's own session code
/// would. The fingerprint the box shows is the relay's authority, not the
/// console's, which is why the comparison is never skipped.
#[tokio::test]
async fn a_party_between_the_box_and_the_console_meets_a_code_that_matches_nothing() {
    let Some(rig) = rig().await else { return };
    let fake = Authority::init(&rig.dir.path().join("between"), "weaver-web", &[]).unwrap();
    let relay = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_at = relay.local_addr().unwrap();
    let console = rig.listener.address().to_string();
    let acceptor = TlsAcceptor::from(fake.pairing_tls().unwrap());
    let between = tokio::spawn(async move {
        let (tcp, _) = relay.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let mut from_box = LineReader::with_bound(tls, PAIR_FIRST_FRAME_BOUND);
        let Line::Frame(line) = from_box.next().await else {
            panic!("the box sent no first frame");
        };
        let Ok(PairFromBox::Hello {
            r#box,
            admin_request,
        }) = serde_json::from_str(&line)
        else {
            panic!("the box's first frame is not its hello");
        };
        let onward = pair_client::dial(
            &console,
            &Claim {
                r#box,
                admin_request,
            },
        )
        .await
        .unwrap();
        let waiting = PairToBox::Waiting {
            expires_in_secs: onward.expires_in().as_secs(),
            cadence_secs: 1,
        };
        let line = serde_json::to_string(&waiting).unwrap() + "\n";
        from_box.get_mut().write_all(line.as_bytes()).await.unwrap();
        from_box.get_mut().flush().await.unwrap();
        (onward, from_box)
    });
    let at_box = pair_client::dial(&relay_at.to_string(), &claim("box-a"))
        .await
        .unwrap();
    let (onward, _held) = between.await.unwrap();
    until_waiting(&rig, 1).await;

    assert_ne!(at_box.fingerprint(), console_fingerprint(&rig.authority));
    assert_eq!(
        at_box.fingerprint(),
        pair_code::grouped(&fake.fingerprint())
    );
    assert!(
        !at_box.code().matches(onward.code()),
        "the two sessions share a code"
    );
    assert!(
        rig.listener.take_pairing(at_box.code()).await.is_none(),
        "the box's code matched through the relay"
    );
    let taken = rig.listener.take_pairing(onward.code()).await;
    assert_eq!(taken.map(|t| t.r#box).as_deref(), Some("box-a"));
}

/// The box's half prints the authority the console presented, in the
/// pairing's grouping, and that is the console's own.
#[tokio::test]
async fn the_box_shows_the_consoles_authority_fingerprint() {
    let Some(rig) = rig().await else { return };
    let dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    assert_eq!(dialed.fingerprint(), console_fingerprint(&rig.authority));
}

/// **A match spends the connection** (Spec 8.2): the box's code takes its
/// waiting connection once, with the box's claim, and then matches
/// nothing. The taker holds the connection for the exchange, and the box
/// hears the close once the taker lets it go.
#[tokio::test]
async fn a_match_spends_the_connection() {
    let Some(rig) = rig().await else { return };
    let mut dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    until_waiting(&rig, 1).await;
    let listed = rig.listener.waiting();
    assert_eq!(listed[0].r#box, "box-a");
    assert_eq!(listed[0].peer, IpAddr::V4(local(1)));
    let code = dialed.code().clone();
    let taken = rig
        .listener
        .take_pairing(&code)
        .await
        .expect("its own code");
    assert_eq!(taken.r#box, "box-a");
    assert!(taken.admin_request.contains("admin"));
    assert!(rig.listener.waiting().is_empty());
    assert!(
        rig.listener.take_pairing(&code).await.is_none(),
        "a code matched twice"
    );
    drop(taken);
    let heard = tokio::time::timeout(SOON, dialed.next()).await;
    assert!(matches!(heard, Ok(Heard::Closed)), "{heard:?}");
}

/// **A probe its entry gave up on spends nothing** (Spec 8.2): a probe
/// whose entry stopped waiting, its receiver gone before the connection
/// reads it, leaves the box waiting, and the box's own code entered after
/// takes it.
///
/// Perturbation: drop both the closed probe's check and the hand-over's
/// return, and the abandoned probe spends the connection, the right code
/// then matching nothing.
#[tokio::test]
async fn a_probe_its_entry_gave_up_on_spends_nothing() {
    let Some(rig) = rig().await else { return };
    let dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    until_waiting(&rig, 1).await;
    let code = dialed.code().clone();
    rig.listener.probe_abandoned(&code).await;
    until_waiting(&rig, 1).await;
    let taken = rig
        .listener
        .take_pairing(&code)
        .await
        .expect("its own code, after a probe its entry gave up on");
    assert_eq!(taken.r#box, "box-a");
    assert!(rig.listener.waiting().is_empty());
}

fn refused_busy(dialed: Result<Dialed, DialError>) {
    match dialed {
        Err(DialError::Refused(reason)) => assert_eq!(reason, "busy"),
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("a box past the bound waits"),
    }
}

/// **Four boxes wait from one peer address, and a fifth is refused
/// `busy`** (Spec 8.2), while a box from another address waits.
///
/// Perturbation: count the bound over all peers alone, and the fifth from
/// one address waits.
#[tokio::test]
async fn past_four_from_one_peer_a_box_is_refused_busy() {
    let Some(rig) = rig().await else { return };
    let mut held = Vec::new();
    for n in 0..4 {
        held.push(
            pair_from(&rig, local(1), &format!("box-{n}"))
                .await
                .unwrap(),
        );
    }
    refused_busy(pair_from(&rig, local(1), "box-4").await);
    held.push(pair_from(&rig, local(2), "box-5").await.unwrap());
    until_waiting(&rig, 5).await;
}

/// **Sixteen boxes wait in all, and a seventeenth is refused `busy`**
/// (Spec 8.2); a box that leaves gives its place back.
///
/// Perturbation: drop the total's bound, and the seventeenth waits.
#[tokio::test]
async fn past_sixteen_in_all_a_box_is_refused_busy() {
    let Some(rig) = rig().await else { return };
    let mut held = Vec::new();
    for peer in 2..6 {
        for n in 0..4 {
            held.push(
                pair_from(&rig, local(peer), &format!("box-{peer}-{n}"))
                    .await
                    .unwrap(),
            );
        }
    }
    until_waiting(&rig, 16).await;
    refused_busy(pair_from(&rig, local(6), "box-6").await);
    drop(held.pop());
    until_waiting(&rig, 15).await;
    held.push(pair_from(&rig, local(6), "box-6").await.unwrap());
}

/// A pairing handshake with no first frame, or a first frame past its
/// bound, from the box `raw` connects.
async fn raw(rig: &Rig) -> tokio_rustls::client::TlsStream<TcpStream> {
    let tcp = TcpStream::connect(rig.listener.address()).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();
    TlsConnector::from(pair_client::pairing_client_tls())
        .connect(name, tcp)
        .await
        .unwrap()
}

/// Whether the console closed the connection within `within`, answering
/// nothing.
async fn closed_unanswered(
    tls: tokio_rustls::client::TlsStream<TcpStream>,
    within: Duration,
) -> bool {
    let mut line = String::new();
    matches!(
        tokio::time::timeout(within, BufReader::new(tls).read_line(&mut line)).await,
        Ok(Ok(0)) | Ok(Err(_))
    )
}

/// **The first frame arrives within its bound and holds at most 16 KiB**
/// (Spec 8.2): a box silent past the bound, and one whose first line passes
/// 16 KiB, are closed unanswered, listed never, and leave their places.
///
/// Perturbation: read the first frame under the link's 4 MiB bound, and the
/// long line is read to its end and the box held.
#[tokio::test]
async fn a_first_frame_late_or_long_is_closed() {
    let Some(rig) = rig().await else { return };
    rig.listener.set_pairing_bounds(Bounds {
        first_frame: Duration::from_secs(1),
        per_peer: 2,
        ..Bounds::default()
    });
    let silent = raw(&rig).await;
    assert!(
        closed_unanswered(silent, Duration::from_secs(3)).await,
        "a silent box was held"
    );

    let mut long = raw(&rig).await;
    let bytes = vec![b'x'; PAIR_FIRST_FRAME_BOUND + 1];
    let _ = long.write_all(&bytes).await;
    let _ = long.flush().await;
    assert!(
        closed_unanswered(long, Duration::from_millis(900)).await,
        "a first line past 16 KiB was held"
    );
    assert!(rig.listener.waiting().is_empty());
    // Both places given back: two more from the same peer wait.
    let _a = pair_from(&rig, local(1), "box-a").await.unwrap();
    let _b = pair_from(&rig, local(1), "box-b").await.unwrap();
}

/// **Ten minutes from the dial the console closes the connection
/// `expired`** (Spec 8.2), here a life of two seconds: the box is told
/// what is left of it, hears heartbeats, then `expired`, and leaves the
/// list.
///
/// Perturbation: drop the life's close, and the box hears heartbeats past
/// it and no `expired`.
#[tokio::test]
async fn the_console_closes_a_waiting_box_expired_at_its_life() {
    let Some(rig) = rig().await else { return };
    rig.listener.set_pairing_bounds(Bounds {
        life: Duration::from_secs(2),
        ..Bounds::default()
    });
    let started = tokio::time::Instant::now();
    let mut dialed = pair_from(&rig, local(1), "box-a").await.unwrap();
    assert!(dialed.expires_in() <= Duration::from_secs(2));
    let heard = tokio::time::timeout(SOON, dialed.next()).await;
    assert!(matches!(heard, Ok(Heard::Expired)), "{heard:?}");
    let took = started.elapsed();
    assert!(
        took >= Duration::from_millis(1900) && took < Duration::from_secs(4),
        "expired after {took:?}"
    );
    until_waiting(&rig, 0).await;
}

/// **The box checks that the server's certificate chains to the authority
/// presented with it** (Spec 8.2): a console that presents its certificate
/// with another authority's completes no handshake with the box, which
/// derives no code.
///
/// Perturbation: let the box's verifier pass any chain, and the box
/// completes the handshake.
#[tokio::test]
async fn a_chain_that_does_not_hold_together_is_refused_at_the_box() {
    let dir = crate::test_dir::tempdir().unwrap();
    let console = Authority::init(&dir.path().join("console"), "weaver-web", &[]).unwrap();
    let other = Authority::init(&dir.path().join("other"), "weaver-web", &[]).unwrap();
    let (box_io, console_io) = tokio::io::duplex(64 * 1024);
    let name = rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap();
    let presented = console.pairing_tls_with_authority_of(&other).unwrap();
    let (_, at_box) = tokio::join!(
        TlsAcceptor::from(presented).accept(console_io),
        TlsConnector::from(pair_client::pairing_client_tls()).connect(name, box_io),
    );
    assert!(
        at_box.is_err(),
        "the box took a chain that does not hold together"
    );
}

/// **The pairing is TLS 1.3 alone** (Spec 8.2), since its code is the
/// session's exporter: a client that trusts the authority and offers the
/// pairing protocol under TLS 1.2 alone completes no handshake.
///
/// Perturbation: build the pairing's configuration on the safe default
/// versions, and the TLS 1.2 client completes it.
#[tokio::test]
async fn the_pairing_is_tls_1_3_alone() {
    let Some(rig) = rig().await else { return };
    let mut roots = rustls::RootCertStore::empty();
    roots.add(authority_der(&rig.authority)).unwrap();
    let mut config = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS12])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![super::frames::PAIR_PROTOCOL.to_vec()];
    let tcp = TcpStream::connect(rig.listener.address()).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("weaver-web").unwrap();
    let tls = TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await;
    assert!(
        tls.is_err(),
        "a TLS 1.2 client completed the pairing's handshake"
    );
    assert!(rig.listener.waiting().is_empty());
}

/// A console of the test's making: it completes the pairing's handshake
/// under its own authority, reads the first frame, and answers `lines`.
async fn console_saying(lines: &'static [&'static str]) -> SocketAddr {
    let dir = crate::test_dir::tempdir().unwrap();
    let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
    let acceptor = TlsAcceptor::from(authority.pairing_tls().unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _dir = dir;
        let (tcp, _) = listener.accept().await.unwrap();
        let tls = acceptor.accept(tcp).await.unwrap();
        let mut reader = LineReader::with_bound(tls, PAIR_FIRST_FRAME_BOUND);
        let _ = reader.next().await;
        for line in lines {
            let _ = reader
                .get_mut()
                .write_all(format!("{line}\n").as_bytes())
                .await;
        }
        let _ = reader.get_mut().flush().await;
        tokio::time::sleep(SOON).await;
    });
    at
}

/// **Nothing the console wrote reaches a printed fault but a reason checked
/// printable** (Spec 8.2): a line whose tag is an escape sequence, as the
/// first answer and as a later frame, is named by a fixed text.
///
/// Perturbation: print serde's error for a line that is no frame, which
/// quotes the console's tag, and the fault carries the escape.
#[tokio::test]
async fn a_consoles_line_reaches_no_fault_unchecked() {
    const HOSTILE: &str = r#"{"svc":"\u001b[2J\u001b]0;x\u0007"}"#;
    let printable = |text: &str| text.bytes().all(|b| (0x20..0x7f).contains(&b));
    let at = console_saying(&[HOSTILE]).await;
    match pair_client::dial(&at.to_string(), &claim("box-a")).await {
        Err(DialError::Fault(why)) => assert!(printable(&why), "{why:?}"),
        Err(other) => panic!("{other}"),
        Ok(_) => panic!("a hostile answer was taken as waiting"),
    }
    let at = console_saying(&[
        r#"{"svc":"waiting","expires_in_secs":600,"cadence_secs":1}"#,
        HOSTILE,
    ])
    .await;
    let mut dialed = pair_client::dial(&at.to_string(), &claim("box-a"))
        .await
        .unwrap();
    match tokio::time::timeout(SOON, dialed.next()).await {
        Ok(Heard::Fault(why)) => assert!(printable(&why), "{why:?}"),
        other => panic!("{other:?}"),
    }
}

/// **A first frame that is not this console's hello is refused
/// `frame_refused`** (Spec 8.2, section 9): the earlier hello, which
/// carried a gate request beside admin-con's, hears `refused` with that
/// reason, so a box built to it prints why, and it is never listed.
///
/// Perturbation: close such a frame unanswered, as the console did before,
/// and the box reads no line.
#[tokio::test]
async fn a_first_frame_naming_a_gate_request_is_refused_frame_refused() {
    let Some(rig) = rig().await else { return };
    let tcp = TcpStream::connect(rig.listener.address()).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from("127.0.0.1")
        .unwrap()
        .to_owned();
    let mut tls = TlsConnector::from(crate::link::pair_client::pairing_client_tls())
        .connect(name, tcp)
        .await
        .unwrap();
    let earlier = r#"{"svc":"hello","box":"box-a","gate_request":"gate","admin_request":"admin"}"#;
    tls.write_all(format!("{earlier}\n").as_bytes())
        .await
        .unwrap();
    tls.flush().await.unwrap();
    let mut answer = String::new();
    let read = tokio::time::timeout(SOON, BufReader::new(tls).read_line(&mut answer))
        .await
        .expect("the console answers within its bound");
    assert!(read.is_ok_and(|n| n > 0), "the box reads no line");
    assert_eq!(
        serde_json::from_str::<PairToBox>(answer.trim_end()).unwrap(),
        PairToBox::Refused {
            reason: "frame_refused".into()
        }
    );
    assert!(rig.listener.waiting().is_empty(), "the box is never listed");
}
