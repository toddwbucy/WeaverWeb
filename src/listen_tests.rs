//! The browser's listener (`src/listen.rs`): each start refusal of design
//! section 3 that this act owns, and a TLS client against the listener.
//! Every certificate is minted at test time into a temporary directory;
//! none is in the repository.

use crate::config::ServerConfig;
use crate::listen::{browser_tls, parse_origin, relying_party, scheme_admits};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;

fn cfg() -> ServerConfig {
    ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:0".into(),
        database: String::new(),
        authority_dir: PathBuf::new(),
        silence_bound_secs: 60,
        link_address: None,
        server_name: "weaver-web".into(),
        admins: Vec::new(),
        agent_hop_budget: 8,
        providers: Vec::new(),
        enrollment_token_hours: 24,
        origin: None,
        tls_certificate: None,
        tls_key: None,
        rp_id: None,
        session_idle_secs: 3600,
        session_absolute_secs: 43200,
    }
}

/// A self-signed certificate for `names`, valid from `from` to `to`, and
/// its key, written as PEM under `dir`; answers the two paths and the DER.
fn mint(
    dir: &tempfile::TempDir,
    stem: &str,
    names: &[&str],
    from: OffsetDateTime,
    to: OffsetDateTime,
) -> (PathBuf, PathBuf, Vec<u8>) {
    let key = rcgen::KeyPair::generate().unwrap();
    let mut params =
        rcgen::CertificateParams::new(names.iter().map(|n| n.to_string()).collect::<Vec<_>>())
            .unwrap();
    params.not_before = from;
    params.not_after = to;
    let cert = params.self_signed(&key).unwrap();
    let (cert_path, key_path) = (
        dir.path().join(format!("{stem}.crt")),
        dir.path().join(format!("{stem}.key")),
    );
    std::fs::write(&cert_path, cert.pem()).unwrap();
    std::fs::write(&key_path, key.serialize_pem()).unwrap();
    (cert_path, key_path, cert.der().to_vec())
}

fn days(n: i64) -> time::Duration {
    time::Duration::days(n)
}

fn with(origin: Option<&str>, pair: Option<(PathBuf, PathBuf)>) -> ServerConfig {
    let mut c = cfg();
    c.origin = origin.map(str::to_owned);
    if let Some((cert, key)) = pair {
        c.tls_certificate = Some(cert);
        c.tls_key = Some(key);
    }
    c
}

fn refused(c: &ServerConfig, now: OffsetDateTime, why: &str) {
    let error = match browser_tls(c, now) {
        Ok(_) => panic!("started where {why} should refuse"),
        Err(e) => e.to_string(),
    };
    assert!(error.contains(why), "{error}");
}

/// **An origin is a serialized origin or is refused**, against the WHATWG
/// URL Standard as `url` implements it. Each refusal names either the part
/// a bare origin never carries, the parse's own failure, or the
/// serialization a browser would send instead.
#[test]
fn every_malformed_origin_form_is_refused() {
    for (origin, why) in [
        ("https://ada@example.test", "userinfo"),
        ("https://example.test/", "path"),
        ("https://example.test/app", "path"),
        ("https://example.test?a=1", "query"),
        ("https://example.test#top", "fragment"),
        (
            "HTTPS://example.test",
            "sends it as \"https://example.test\"",
        ),
        (
            "https://Example.test",
            "sends it as \"https://example.test\"",
        ),
        (
            "https://example.test:443",
            "sends it as \"https://example.test\"",
        ),
        ("http://localhost:80", "sends it as \"http://localhost\""),
        ("example.test", "does not parse as a URL"),
        ("https://[2001:db8::1", "invalid IPv6 address"),
        ("https://[example.test]", "invalid IPv6 address"),
        ("https://[192.0.2.1]", "invalid IPv6 address"),
        ("https://[2001:db8::1]8443", "invalid IPv6 address"),
        (
            "https://[2001:DB8::1]",
            "sends it as \"https://[2001:db8::1]\"",
        ),
        (
            "https://[2001:db8::1]:443",
            "sends it as \"https://[2001:db8::1]\"",
        ),
        ("https://example.test:1:2", "invalid port number"),
        (
            "https://example.test:08443",
            "sends it as \"https://example.test:8443\"",
        ),
        ("https://example.test:+8443", "invalid port number"),
        (
            "https://[2001:db8:0:0::1]",
            "sends it as \"https://[2001:db8::1]\"",
        ),
        (
            "https://[::ffff:192.0.2.1]",
            "sends it as \"https://[::ffff:c000:201]\"",
        ),
        ("https://192.000.2.1", "sends it as \"https://192.0.2.1\""),
        ("https://0x7f.1", "sends it as \"https://127.0.0.1\""),
        ("https://example.0x", "invalid IPv4 address"),
        ("https://example.0x1f", "invalid IPv4 address"),
        (
            "https://ex%41mple.test",
            "sends it as \"https://example.test\"",
        ),
        (
            "https://b\u{fc}cher.test",
            "sends it as \"https://xn--bcher-kva.test\"",
        ),
        (
            "https://B\u{dc}CHER.test",
            "sends it as \"https://xn--bcher-kva.test\"",
        ),
    ] {
        let error = parse_origin(origin).expect_err(origin);
        assert!(error.contains(why), "{origin}: {error}");
    }
    for origin in [
        "https://example.test",
        "https://example.test:8443",
        "http://localhost",
        "http://localhost:8080",
        "https://[2001:db8::1]",
        "https://[2001:db8::1]:8443",
        "https://[::ffff:c000:201]",
        "https://[::1]",
        "https://192.0.2.1:8443",
        "https://xn--bcher-kva.test",
        "https://example.0xyz",
    ] {
        let parsed = parse_origin(origin).unwrap_or_else(|e| panic!("{origin}: {e}"));
        assert_eq!(parsed.serialized(), origin);
    }
    let ported = parse_origin("https://[2001:db8::1]:8443").unwrap();
    assert_eq!(
        (ported.host.as_str(), ported.port, ported.server_name()),
        ("[2001:db8::1]", Some(8443), "2001:db8::1")
    );
}

/// **An IPv6 address is accepted only as a browser serializes it**: the
/// first longest run of two or more zero groups compressed, a single zero
/// group never, lower-case hex, and no dotted IPv4 tail.
#[test]
fn an_ipv6_address_is_accepted_only_as_a_browser_serializes_it() {
    for (address, serialized) in [
        ("2001:db8:0:0:0:0:0:1", "2001:db8::1"),
        ("0:0:0:0:0:0:0:1", "::1"),
        ("0:0:0:0:0:0:0:0", "::"),
        ("1:0:0:0:0:0:0:0", "1::"),
        ("2001:db8:0:1:1:1:1:1", "2001:db8:0:1:1:1:1:1"),
        ("1:0:0:2:0:0:0:3", "1:0:0:2::3"),
        ("1:0:0:2:0:0:3:4", "1::2:0:0:3:4"),
        ("2001:DB8:00AB::1", "2001:db8:ab::1"),
        ("::ffff:192.0.2.1", "::ffff:c000:201"),
        ("::192.0.2.1", "::c000:201"),
    ] {
        let origin = format!("https://[{address}]");
        let canonical = format!("https://[{serialized}]");
        if address == serialized {
            parse_origin(&origin).unwrap_or_else(|e| panic!("{origin}: {e}"));
        } else {
            let error = parse_origin(&origin).expect_err(&origin);
            assert!(
                error.contains(&format!("sends it as {canonical:?}")),
                "{origin}: {error}"
            );
        }
        parse_origin(&canonical).unwrap_or_else(|e| panic!("{canonical}: {e}"));
    }
}

/// **The relying party's refusals** (design section 3), each its own case:
/// an `rp_id` without an `origin` and the reverse, an empty `rp_id`, an
/// `rp_id` that is an IP address, and an origin whose host is neither the
/// `rp_id` nor under it, a bare suffix and the reverse nesting among them;
/// neither configured starts with no passkey on.
#[test]
fn the_relying_partys_faults_are_refused() {
    let with_rp = |origin: Option<&str>, rp_id: Option<&str>| {
        let mut c = cfg();
        c.origin = origin.map(str::to_owned);
        c.rp_id = rp_id.map(str::to_owned);
        c
    };
    relying_party(&with_rp(None, None)).expect("no passkey on, nothing refused");
    for (origin, rp_id, why) in [
        (
            None,
            Some("weaver.test"),
            "rp_id is configured and origin is not",
        ),
        (
            Some("https://weaver.test"),
            None,
            "origin is configured and rp_id is not",
        ),
        (Some("https://weaver.test"), Some(""), "rp_id is empty"),
        (
            Some("https://192.0.2.1"),
            Some("192.0.2.1"),
            "is an IP address",
        ),
        (
            Some("https://[2001:db8::1]"),
            Some("2001:db8::1"),
            "is an IP address",
        ),
        (
            Some("https://[2001:db8::1]"),
            Some("[2001:db8::1]"),
            "is an IP address",
        ),
        (Some("https://weaver.test"), Some("other.test"), "neither"),
        (
            Some("https://notweaver.test"),
            Some("weaver.test"),
            "neither",
        ),
        (
            Some("https://weaver.test"),
            Some("app.weaver.test"),
            "neither",
        ),
    ] {
        let error = relying_party(&with_rp(origin, rp_id))
            .expect_err(&format!("{origin:?} {rp_id:?}"))
            .to_string();
        assert!(error.contains(why), "{origin:?} {rp_id:?}: {error}");
    }
    for (origin, rp_id) in [
        ("https://weaver.test", "weaver.test"),
        ("https://app.weaver.test:8443", "weaver.test"),
        ("http://localhost:8080", "localhost"),
    ] {
        relying_party(&with_rp(Some(origin), Some(rp_id)))
            .unwrap_or_else(|e| panic!("{origin} {rp_id}: {e}"));
    }
}

/// **The scheme rule**: https, or exactly http on the host localhost; http
/// on any other host is refused.
#[test]
fn only_https_or_http_on_localhost_is_admitted() {
    for origin in [
        "https://example.test",
        "http://localhost",
        "http://localhost:8080",
    ] {
        scheme_admits(&parse_origin(origin).unwrap()).unwrap_or_else(|e| panic!("{origin}: {e}"));
    }
    for origin in ["http://example.test", "http://127.0.0.1", "ws://localhost"] {
        let error = scheme_admits(&parse_origin(origin).unwrap()).expect_err(origin);
        assert!(error.contains("secure context"), "{origin}: {error}");
    }
    let now = OffsetDateTime::now_utc();
    refused(
        &with(Some("http://example.test"), None),
        now,
        "secure context",
    );
    assert!(
        browser_tls(&with(Some("http://localhost"), None), now)
            .unwrap()
            .is_none()
    );
}

/// **An https origin with no certificate and key is refused**, as is a
/// certificate without its key.
#[test]
fn an_https_origin_needs_a_certificate_and_key() {
    let now = OffsetDateTime::now_utc();
    refused(
        &with(Some("https://example.test"), None),
        now,
        "tls_certificate and tls_key are not configured",
    );
    let dir = tempfile::tempdir().unwrap();
    let (cert, _, _) = mint(&dir, "a", &["example.test"], now - days(1), now + days(90));
    let mut c = cfg();
    c.tls_certificate = Some(cert);
    refused(&c, now, "tls_key is not");
}

/// **A key that does not pair with its certificate is refused.**
#[test]
fn a_key_that_does_not_pair_is_refused() {
    let now = OffsetDateTime::now_utc();
    let dir = tempfile::tempdir().unwrap();
    let (cert, _, _) = mint(&dir, "a", &["example.test"], now - days(1), now + days(90));
    let (_, other_key, _) = mint(&dir, "b", &["example.test"], now - days(1), now + days(90));
    refused(
        &with(Some("https://example.test"), Some((cert, other_key))),
        now,
        "do not load as a pair",
    );
}

/// **A certificate for another name is refused against the origin**, and
/// accepted for its own; without an origin the name check waits.
#[test]
fn a_certificate_for_another_name_is_refused() {
    let now = OffsetDateTime::now_utc();
    let dir = tempfile::tempdir().unwrap();
    let (cert, key, _) = mint(&dir, "a", &["other.test"], now - days(1), now + days(90));
    let pair = Some((cert, key));
    refused(
        &with(Some("https://example.test"), pair.clone()),
        now,
        "not valid for the origin's host",
    );
    assert!(
        browser_tls(&with(Some("https://other.test"), pair.clone()), now)
            .unwrap()
            .is_some()
    );
    assert!(
        browser_tls(&with(None, pair.clone()), now)
            .unwrap()
            .is_some(),
        "no origin, no name check"
    );

    // An IPv6 origin is checked as an address against the IP names.
    let (cert, key, _) = mint(&dir, "ip", &["2001:db8::1"], now - days(1), now + days(90));
    let ip = Some((cert, key));
    for origin in ["https://[2001:db8::1]", "https://[2001:db8::1]:8443"] {
        assert!(
            browser_tls(&with(Some(origin), ip.clone()), now)
                .unwrap()
                .is_some(),
            "{origin}"
        );
    }
    refused(
        &with(Some("https://[2001:db8::2]"), ip),
        now,
        "not valid for the origin's host",
    );
    refused(
        &with(Some("https://[2001:db8::1]"), pair),
        now,
        "not valid for the origin's host",
    );
}

/// **A certificate outside its validity period at start is refused**, not
/// yet valid or expired; one expiring within fourteen days starts, warned.
#[test]
fn a_certificate_outside_its_period_is_refused_and_a_near_one_warned() {
    let now = OffsetDateTime::now_utc();
    let dir = tempfile::tempdir().unwrap();
    let origin = Some("https://example.test");
    let (cert, key, _) = mint(
        &dir,
        "old",
        &["example.test"],
        now - days(90),
        now - days(1),
    );
    refused(&with(origin, Some((cert, key))), now, "expired");
    let (cert, key, _) = mint(
        &dir,
        "new",
        &["example.test"],
        now + days(1),
        now + days(90),
    );
    refused(&with(origin, Some((cert, key))), now, "not valid until");
    let (cert, key, _) = mint(
        &dir,
        "near",
        &["example.test"],
        now - days(80),
        now + days(10),
    );
    let (_, warnings) = browser_tls(&with(origin, Some((cert, key))), now)
        .unwrap()
        .unwrap();
    assert!(
        warnings.iter().any(|w| w.contains("within fourteen days")),
        "{warnings:?}"
    );
    let (cert, key, _) = mint(
        &dir,
        "far",
        &["example.test"],
        now - days(1),
        now + days(90),
    );
    let (_, warnings) = browser_tls(&with(origin, Some((cert, key))), now)
        .unwrap()
        .unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// A TLS listener on a loopback port for a certificate minted for
/// `localhost`, and a client that trusts that certificate alone.
async fn serving() -> (
    tempfile::TempDir,
    std::net::SocketAddr,
    crate::listen::TlsListener,
    tokio_rustls::TlsConnector,
) {
    serving_bounded(
        crate::listen::HANDSHAKES_IN_FLIGHT,
        crate::listen::HANDSHAKE_BOUND,
        crate::listen::ALERT_WARNING_INTERVAL,
    )
    .await
}

/// `serving` with the handshake cap, bound and warning interval given.
async fn serving_bounded(
    in_flight: usize,
    bound: Duration,
    alert_interval: Duration,
) -> (
    tempfile::TempDir,
    std::net::SocketAddr,
    crate::listen::TlsListener,
    tokio_rustls::TlsConnector,
) {
    let now = OffsetDateTime::now_utc();
    let dir = tempfile::tempdir().unwrap();
    let (cert, key, der) = mint(&dir, "a", &["localhost"], now - days(1), now + days(90));
    let (tls, _) = browser_tls(&with(Some("https://localhost"), Some((cert, key))), now)
        .unwrap()
        .unwrap();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = tcp.local_addr().unwrap();
    let listener =
        crate::listen::TlsListener::bounded(tcp, tls, in_flight, bound, alert_interval).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(rustls_pki_types::CertificateDer::from(der))
        .unwrap();
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    (
        dir,
        address,
        listener,
        tokio_rustls::TlsConnector::from(Arc::new(client)),
    )
}

/// **A TLS client completes a handshake and gets a surface's answer, and a
/// plain HTTP request to the TLS listener gets none.**
#[tokio::test]
async fn the_tls_listener_answers_a_tls_client_and_no_plain_request() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (_dir, address, listener, connector) = serving().await;
    let app = axum::Router::new().route("/", axum::routing::get(|| async { "a surface's answer" }));
    tokio::spawn(async move { axum::serve(listener, app).await });

    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
    let mut stream = connector
        .connect(name, socket)
        .await
        .expect("the handshake completes");
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut answer = Vec::new();
    tokio::time::timeout(Duration::from_secs(10), stream.read_to_end(&mut answer))
        .await
        .unwrap()
        .ok();
    let answer = String::from_utf8_lossy(&answer);
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("a surface's answer"), "{answer}");

    let mut plain = tokio::net::TcpStream::connect(address).await.unwrap();
    plain
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut answer = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), plain.read_to_end(&mut answer)).await;
    let answer = String::from_utf8_lossy(&answer);
    assert!(
        !answer.contains("a surface's answer"),
        "a plain request reached a surface: {answer}"
    );
    assert!(
        !answer.starts_with("HTTP/"),
        "a plain request got an HTTP answer: {answer}"
    );
}

/// **A handshake that never finishes stalls no other**: a client that
/// connects and never speaks holds only its own task, and the next client's
/// handshake completes well inside `HANDSHAKE_BOUND`.
#[tokio::test]
async fn a_silent_client_stalls_no_other_handshake() {
    let (_dir, address, _listener, connector) = serving().await;
    let _silent = tokio::net::TcpStream::connect(address).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
    assert!(crate::listen::HANDSHAKE_BOUND > Duration::from_secs(3));
    tokio::time::timeout(Duration::from_secs(3), connector.connect(name, socket))
        .await
        .expect("a silent client stalled the next handshake")
        .expect("the handshake completes");
}

/// **The handshakes in flight are capped**: with the cap held by silent
/// clients, the next connection waits unaccepted, in the kernel's backlog,
/// until one silent client's bound passes and frees its permit, and then
/// completes. A bound on memory, not on availability.
#[tokio::test]
async fn handshakes_past_the_cap_wait_for_a_permit() {
    let (cap, bound) = (2, Duration::from_secs(2));
    let (_dir, address, _listener, connector) =
        serving_bounded(cap, bound, crate::listen::ALERT_WARNING_INTERVAL).await;
    let mut silent = Vec::new();
    for _ in 0..cap {
        silent.push(tokio::net::TcpStream::connect(address).await.unwrap());
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
    let mut handshake = Box::pin(connector.connect(name, socket));
    assert!(
        tokio::time::timeout(Duration::from_secs(1), &mut handshake)
            .await
            .is_err(),
        "a connection past the cap was accepted while every permit was held"
    );
    tokio::time::timeout(bound * 3, handshake)
        .await
        .expect("a permit freed at a silent client's bound")
        .expect("the handshake completes");
}

/// The log lines at warn, captured on the test's own thread; the listener's
/// tasks run there too, the test's runtime being single-threaded.
#[derive(Clone, Default)]
struct Captured(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Captured {
    fn warnings(&self) -> Vec<String> {
        String::from_utf8_lossy(&self.0.lock().unwrap())
            .lines()
            .filter(|line| line.contains("WARN"))
            .map(str::to_owned)
            .collect()
    }

    /// Wait up to five seconds for the warnings to number `n`.
    async fn until(&self, n: usize) -> Vec<String> {
        for _ in 0..100 {
            if self.warnings().len() >= n {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.warnings()
    }
}

/// A client that distrusts the listener's certificate, and so ends the
/// handshake with a certificate alert, `unknown_ca`.
async fn refuse_the_certificate(address: std::net::SocketAddr) {
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(rustls::RootCertStore::empty())
    .with_no_client_auth();
    let socket = tokio::net::TcpStream::connect(address).await.unwrap();
    let name = rustls_pki_types::ServerName::try_from("localhost").unwrap();
    let refused = tokio_rustls::TlsConnector::from(Arc::new(client))
        .connect(name, socket)
        .await;
    assert!(
        refused.is_err(),
        "a client with no roots accepted the certificate"
    );
}

/// **A client's certificate alert is logged at warn, at most once in the
/// interval**: the first names the alert, a second within the interval is
/// suppressed and counted in the next warning, and a client that does not
/// speak TLS logs nothing at warn.
#[tokio::test]
async fn a_certificate_alert_is_warned_at_most_once_an_interval() {
    use tokio::io::AsyncWriteExt;
    let captured = Captured::default();
    let _logging = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::WARN)
            .with_ansi(false)
            .finish(),
    );
    let interval = Duration::from_secs(1);
    let (_dir, address, _listener, _) =
        serving_bounded(4, crate::listen::HANDSHAKE_BOUND, interval).await;

    refuse_the_certificate(address).await;
    let warned = captured.until(1).await;
    assert_eq!(warned.len(), 1, "{warned:?}");
    assert!(warned[0].contains("UnknownCA"), "{warned:?}");
    assert!(warned[0].contains("0 more refusals"), "{warned:?}");

    refuse_the_certificate(address).await;
    let mut plain = tokio::net::TcpStream::connect(address).await.unwrap();
    plain
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    drop(plain);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let warned = captured.warnings();
    assert_eq!(
        warned.len(),
        1,
        "a refusal within the interval, or a plain client, warned: {warned:?}"
    );

    tokio::time::sleep(interval).await;
    refuse_the_certificate(address).await;
    let warned = captured.until(2).await;
    assert_eq!(warned.len(), 2, "{warned:?}");
    assert!(warned[1].contains("1 more refusals"), "{warned:?}");
}
