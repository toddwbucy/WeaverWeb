//! conforms: web-the-hugging-face-client-reads-no-proxy-from-the-environment
//!
//! **The Hugging Face client reads no proxy from the environment** (Spec
//! 2.15): it speaks to its one fixed endpoint directly, so `HTTP_PROXY`,
//! `HTTPS_PROXY` and `ALL_PROXY` set in the server's environment carry none
//! of its requests, and the token never reaches a host the operator did not
//! name. reqwest reads those variables through hyper-util whatever its
//! features, so the client must refuse them itself.
//!
//! A test binary of its own: it sets the process's environment before any
//! client is built, which no other test may share.

use axum::http::StatusCode;
use axum::routing::get;
use weaver_web::hugging_face::{Client, Token, Whoami};

#[tokio::test]
async fn the_client_reads_no_proxy_from_the_environment() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let router =
        axum::Router::new().route("/api/whoami-v2", get(|| async { StatusCode::UNAUTHORIZED }));
    tokio::spawn(async move { axum::serve(listener, router).await });
    // A proxy no request can reach: port 9 on the loopback, where nothing
    // listens. A client that took it would fail to connect.
    for name in [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        // SAFETY: set before any client or other thread reads the
        // environment, in a test binary holding this one test.
        unsafe { std::env::set_var(name, "http://127.0.0.1:9") };
    }
    for name in ["NO_PROXY", "no_proxy"] {
        // SAFETY: as above.
        unsafe { std::env::remove_var(name) };
    }
    let client = Client::with_base(&base).unwrap();
    let token = Token::parse("test-token-proxy-0001").unwrap();
    let answer = client.whoami(&token).await;
    assert!(
        matches!(answer, Ok(Whoami::Refused)),
        "the request went through the environment's proxy: {answer:?}"
    );
}
