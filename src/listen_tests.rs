//! The browser's listener (`src/listen.rs`): each start refusal of design
//! section 3 that this act owns, and a TLS client against the listener.
//! Every certificate is minted at test time into a temporary directory;
//! none is in the repository.

use crate::config::ServerConfig;
use crate::listen::{browser_tls, parse_origin, scheme_admits};
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

/// **An origin is a serialized origin or is refused**, form by form:
/// userinfo, a path, a trailing slash, a query, a fragment, a scheme or host
/// in upper case, a default port written, and no scheme.
#[test]
fn every_malformed_origin_form_is_refused() {
    for (origin, why) in [
        ("https://ada@example.test", "userinfo"),
        ("https://example.test/", "path"),
        ("https://example.test/app", "path"),
        ("https://example.test?a=1", "query"),
        ("https://example.test#top", "fragment"),
        ("HTTPS://example.test", "scheme is not in lower case"),
        ("https://Example.test", "host is not in lower case"),
        ("https://example.test:443", "default port"),
        ("http://localhost:80", "default port"),
        ("example.test", "no scheme"),
        ("https://[2001:db8::1", "a bracketed host is not closed"),
        ("https://[example.test]", "not an IPv6 address"),
        ("https://[192.0.2.1]", "not an IPv6 address"),
        ("https://[2001:db8::1]8443", "something other than a port"),
        ("https://[2001:DB8::1]", "host is not in lower case"),
        ("https://[2001:db8::1]:443", "default port"),
        ("https://example.test:1:2", "port is not a number"),
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
    ] {
        parse_origin(origin).unwrap_or_else(|e| panic!("{origin}: {e}"));
    }
    let ported = parse_origin("https://[2001:db8::1]:8443").unwrap();
    assert_eq!(
        (ported.host.as_str(), ported.port, ported.server_name()),
        ("[2001:db8::1]", Some(8443), "2001:db8::1")
    );
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
    )
    .await
}

/// `serving` with the handshake cap and bound given.
async fn serving_bounded(
    in_flight: usize,
    bound: Duration,
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
    let listener = crate::listen::TlsListener::bounded(tcp, tls, in_flight, bound).unwrap();
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
    let (_dir, address, _listener, connector) = serving_bounded(cap, bound).await;
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
