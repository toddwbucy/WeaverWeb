//! The register of agents through the server (`src/surfaces/agents.rs`;
//! Spec 2.12, 2.13 and 8): registering and rotating end to end, each
//! client config taken once and valid for its plane against a real
//! listener, the hand-over's refusals, a rotation closing the old
//! credential's connection, the four-step order, the authority re-check,
//! and no private key anywhere it should not be. Each test runs on a
//! database of its own, with an authority minted at test time in a
//! temporary directory. No name is a real one.

use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::admin_tests::{admin, form, hold, send_as, signed_in};
use crate::host;
use crate::link::authority::{Authority, client_tls};
use crate::link::frames::{
    FromClient, Plane, Position, Refusal as LinkRefusal, ToClient, capability,
};
use crate::link::listener::Listener;
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys};
use crate::sign_in_tests::rows;
use crate::store::Store;
use crate::store::read::tests::{Fresh, fresh_store};
use crate::surfaces::admin::READ_HOLD;
use crate::surfaces::agents::{HANDOVER_LIFETIME, HANDOVERS_HELD, Handover, Seams, routes};
use crate::surfaces::gate;
use crate::test_log::Captured;

/// A store of its own, an authority minted for the test, a listener over
/// both, and the app with the agents surface mounted as the server mounts
/// it.
pub(crate) struct Rig {
    fresh: Fresh,
    dir: tempfile::TempDir,
    authority: Arc<Authority>,
    pub(crate) listener: Listener,
    handover: Handover,
    pub(crate) app: axum::Router,
}

/// The reading surface's re-check interval in these tests: short, so a
/// test sees a live view close within a second.
pub(crate) const TEST_RECHECK: Duration = Duration::from_millis(200);

impl Drop for Rig {
    fn drop(&mut self) {
        self.listener.stop_now();
    }
}

pub(crate) fn server_config(
    dir: &std::path::Path,
    link: std::net::SocketAddr,
) -> crate::config::ServerConfig {
    crate::config::ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:0".into(),
        database: String::new(),
        authority_dir: dir.join("authority"),
        silence_bound_secs: 60,
        link_address: Some(link.to_string()),
        gate_listen: None,
        gate_address: None,
        server_name: "weaver-web".into(),
        enrollment_token_hours: 24,
        origin: None,
        tls_certificate: None,
        tls_key: None,
        rp_id: None,
        session_idle_secs: 3600,
        session_absolute_secs: 43200,
        operations_log: None,
        model_cache: None,
        no_sign_in_as: None,
        sign_in: vec![crate::config::SignInMethod::Passkey],
    }
}

pub(crate) async fn rig() -> Option<Rig> {
    rig_with(Handover::default()).await
}

/// A rig whose listener holds the silence bound given.
pub(crate) async fn rig_silenced(silence: Duration) -> Option<Rig> {
    rig_of(Handover::default(), silence).await
}

async fn rig_with(handover: Handover) -> Option<Rig> {
    rig_of(handover, Duration::from_secs(60)).await
}

async fn rig_of(handover: Handover, silence: Duration) -> Option<Rig> {
    rig_on(handover, silence, false).await
}

/// A rig whose listener has two doors (Spec 8), its config naming the data
/// door's bound address as `gate_listen`.
pub(crate) async fn rig_doors() -> Option<Rig> {
    rig_on(Handover::default(), Duration::from_secs(60), true).await
}

async fn rig_on(handover: Handover, silence: Duration, two_doors: bool) -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = crate::test_dir::tempdir().unwrap();
    let authority =
        Arc::new(Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap());
    let listener = Listener::start_doors(
        fresh.store.clone(),
        &authority,
        "127.0.0.1:0",
        two_doors.then_some("127.0.0.1:0"),
        silence,
        Some(dir.path().join("operations.log")),
    )
    .await
    .unwrap();
    let mut cfg = server_config(dir.path(), listener.address());
    cfg.gate_listen = listener.data_address().map(|data| data.to_string());
    let cfg = Arc::new(cfg);
    let policy = gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
        write_bound: Duration::from_secs(60),
        standing: None,
    };
    let agents = gate::guard(
        crate::surfaces::script_policy(routes(Seams {
            policy: policy.clone(),
            cfg: cfg.clone(),
            authority: authority.clone(),
            handover: handover.clone(),
        }))
        .with_state(fresh.store.clone()),
        policy.clone(),
    );
    let reading = gate::guard(
        crate::surfaces::script_policy(crate::surfaces::reading::routes(
            crate::surfaces::reading::Seams {
                policy: policy.clone(),
                windows: listener.windows().clone(),
                recheck: TEST_RECHECK,
                asker: listener.clone(),
            },
        ))
        .with_state(fresh.store.clone()),
        policy.clone(),
    );
    // **The Hugging Face account's page**, for the bar's tests: its client
    // names a port nothing answers on, so no test of this rig reaches
    // Hugging Face.
    let puller = crate::hugging_face::pull::Puller::start(
        fresh.store.clone(),
        crate::hugging_face::Client::with_base("http://127.0.0.1:9/").unwrap(),
        None,
        dir.path().join("authority"),
        crate::hugging_face::pull::Settings::default(),
    );
    let hugging_face = gate::guard(
        crate::surfaces::script_policy(crate::surfaces::hugging_face::routes(
            crate::surfaces::hugging_face::Seams {
                policy: policy.clone(),
                client: crate::hugging_face::Client::with_base("http://127.0.0.1:9/").unwrap(),
                authority_dir: dir.path().join("authority"),
                puller: puller.clone(),
            },
        ))
        .with_state(fresh.store.clone()),
        policy.clone(),
    );
    // **The start page, the setup pages and the SSH keys' page**, for the
    // bar's tests and the start page's New agent.
    let setup = gate::guard(
        crate::surfaces::script_policy(crate::surfaces::setup::routes(
            crate::surfaces::setup::Seams {
                policy: policy.clone(),
                cfg: cfg.clone(),
                client: crate::hugging_face::Client::with_base("http://127.0.0.1:9/").unwrap(),
                puller,
                authority_dir: dir.path().join("authority"),
                headers: crate::hugging_face::headers::Headers::new(),
                boxes: std::sync::Arc::new(crate::box_report::Kept),
                enrollment: None,
            },
        ))
        .with_state(fresh.store.clone()),
        policy.clone(),
    );
    let app = app(&fresh.store, Some(passkeys()))
        .merge(agents)
        .merge(reading)
        .merge(hugging_face)
        .merge(setup);
    Some(Rig {
        fresh,
        dir,
        authority,
        listener,
        handover,
        app,
    })
}

impl Rig {
    /// **Another listener on the same store, authority and operations
    /// log**, started after the running one stops: the next server process.
    /// The surfaces keep the stopped one.
    pub(crate) async fn next_listener(&self) -> Listener {
        self.listener.stop().await;
        Listener::start_with(
            self.fresh.store.clone(),
            &self.authority,
            "127.0.0.1:0",
            Duration::from_secs(60),
            Some(self.dir.path().join("operations.log")),
        )
        .await
        .unwrap()
    }

    pub(crate) fn store(&self) -> &Store {
        &self.fresh.store
    }

    /// The operations log's lines, as the listener wrote them.
    pub(crate) fn operations_log_path(&self) -> std::path::PathBuf {
        self.dir.path().join("operations.log")
    }

    pub(crate) fn operations_log(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.dir.path().join("operations.log"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).expect("a JSON line"))
            .collect()
    }

    async fn post(
        &self,
        uri: &str,
        body: String,
        session: &str,
    ) -> (StatusCode, HeaderMap, String) {
        send_as(&self.app, "POST", uri, Some(body), session).await
    }

    /// The handles a hand-over page offers, the gate's first.
    fn handles(page: &str) -> Vec<String> {
        page.match_indices("name=\"handle\" value=\"")
            .map(|(at, m)| page[at + m.len()..at + m.len() + 64].to_owned())
            .collect()
    }

    /// Register through the page, answering the row's identity and the two
    /// handles.
    pub(crate) async fn register(
        &self,
        session: &str,
        r#box: &str,
        name: &str,
    ) -> (String, Vec<String>) {
        let (status, headers, page) = self
            .post(
                "/admin/agents/register",
                form(&[("box", r#box), ("name", name)]),
                session,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
        crate::surfaces::assert_framed(&page);
        assert!(
            !page.contains("PRIVATE KEY")
                && !headers
                    .values()
                    .any(|v| v.to_str().unwrap_or("").contains("PRIVATE KEY")),
            "the hand-over page carries handles, never a config"
        );
        let id: String = sqlx::query_scalar(
            "SELECT agent_id FROM agent WHERE box = $1 AND name = $2 AND gate_state = 'live'",
        )
        .bind(r#box)
        .bind(name)
        .fetch_one(&self.store().pool)
        .await
        .unwrap();
        (id, Self::handles(&page))
    }

    /// A config taken: its status, headers and body.
    pub(crate) async fn take(
        &self,
        handle: &str,
        session: &str,
    ) -> (StatusCode, HeaderMap, String) {
        self.post("/admin/agents/config", form(&[("handle", handle)]), session)
            .await
    }
}

/// A connector built from a client config: its PEMs pinned and presented,
/// dialing the listener and saying hello on its plane.
pub(crate) struct Connector {
    reader: BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    writer: tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
}

impl Connector {
    async fn dial(config: &str) -> Self {
        Self::dial_with(config, &["show"]).await
    }

    /// Dial on a client config, an admin plane declaring `ceiling`, as an
    /// admin-con of this build.
    pub(crate) async fn dial_with(config: &str, ceiling: &[&str]) -> Self {
        Self::dial_stating(config, ceiling, false).await
    }

    /// **Dial again until the listener admits**, past the teardown of a
    /// connection just dropped: the old one is live at the server until its
    /// reader sees the close, and a dial meeting it is refused
    /// `already_connected` (Spec 8), so a fixed wait before the dial races
    /// that teardown. Each such refusal is dialed again, for at most five
    /// seconds; the hello's answer is taken here.
    pub(crate) async fn redial(config: &str, ceiling: &[&str], older: bool) -> Self {
        let until = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let mut connector = Self::dial_stating(config, ceiling, older).await;
            match connector.recv().await {
                Some(ToClient::HelloAnswer { .. }) => return connector,
                Some(ToClient::Refusal {
                    reason: LinkRefusal::AlreadyConnected,
                }) if tokio::time::Instant::now() < until => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                other => panic!("expected the hello's answer, got {other:?}"),
            }
        }
    }

    async fn dial_stating(config: &str, ceiling: &[&str], older: bool) -> Self {
        let table: toml::Table = config.parse().expect("a client config is TOML");
        let field = |key: &str| table[key].as_str().unwrap().to_owned();
        let tls = client_tls(
            &field("server_certificate"),
            &field("certificate"),
            &field("key"),
        )
        .unwrap();
        let tcp = TcpStream::connect(field("server")).await.unwrap();
        let name = rustls::pki_types::ServerName::try_from(field("server_name")).unwrap();
        let stream = tokio_rustls::TlsConnector::from(tls)
            .connect(name, tcp)
            .await
            .expect("the handshake completes against the server the config pins");
        let (read, writer) = tokio::io::split(stream);
        let mut connector = Self {
            reader: BufReader::new(read),
            writer,
        };
        let plane = match field("plane").as_str() {
            "gate" => Plane::Gate,
            _ => Plane::Admin,
        };
        connector
            .send(FromClient::Hello {
                agent: field("agent"),
                plane,
                tail: Some(Position {
                    generation: "g".into(),
                    offset: 0,
                    digest: "d".into(),
                }),
                ceiling: (plane == Plane::Admin)
                    .then(|| ceiling.iter().map(|v| (*v).to_owned()).collect()),
                door: (plane == Plane::Admin).then_some(true),
                capabilities: (plane == Plane::Admin && !older)
                    .then(|| vec![capability::CLOSE_SHOW.to_owned()]),
            })
            .await;
        connector
    }

    pub(crate) async fn send(&mut self, frame: FromClient) {
        let mut line = serde_json::to_string(&frame).unwrap();
        line.push('\n');
        let _ = self.writer.write_all(line.as_bytes()).await;
        let _ = self.writer.flush().await;
    }

    pub(crate) async fn recv(&mut self) -> Option<ToClient> {
        let mut line = String::new();
        match tokio::time::timeout(Duration::from_secs(5), self.reader.read_line(&mut line)).await {
            Ok(Ok(0)) | Err(_) | Ok(Err(_)) => None,
            Ok(Ok(_)) => serde_json::from_str(line.trim_end()).ok(),
        }
    }

    /// **Every frame the server sends within `within`, and whether it closed
    /// the connection**, where [`recv`](Self::recv) reads a close and a
    /// quiet wait alike: a test that claims the connection stands reads it
    /// here.
    pub(crate) async fn frames_within(&mut self, within: Duration) -> (Vec<ToClient>, bool) {
        let until = tokio::time::Instant::now() + within;
        let mut frames = Vec::new();
        loop {
            let mut line = String::new();
            match tokio::time::timeout_at(until, self.reader.read_line(&mut line)).await {
                Err(_) => return (frames, false),
                Ok(Ok(0)) | Ok(Err(_)) => return (frames, true),
                Ok(Ok(_)) => {
                    if let Ok(frame) = serde_json::from_str(line.trim_end()) {
                        frames.push(frame);
                    }
                }
            }
        }
    }

    /// **The first line the server sends within `within`**, raw and
    /// whatever it parses as, or none where it sends nothing: a frame of no
    /// variant [`recv`](Self::recv) knows counts here too.
    pub(crate) async fn first_line_within(&mut self, within: Duration) -> Option<String> {
        let mut line = String::new();
        match tokio::time::timeout(within, self.reader.read_line(&mut line)).await {
            Ok(Ok(n)) if n > 0 => Some(line),
            _ => None,
        }
    }

    /// Whether the server closed the connection within the bound.
    async fn closed(&mut self) -> bool {
        let mut line = String::new();
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.reader.read_line(&mut line))
                .await
            {
                Ok(Ok(0)) | Ok(Err(_)) => return true,
                Ok(Ok(_)) => line.clear(),
                Err(_) => return false,
            }
        }
    }
}

/// The body of a PEM, the lines between its armour, which is the part a
/// leak would carry.
fn pem_body(pem: &str) -> String {
    pem.lines().filter(|l| !l.starts_with("-----")).collect()
}

/// **Registering lands a row of fingerprints and hands over two configs,
/// each taken once and valid for its plane**: the gate config's connector
/// is answered and its plane reads connected; the admin config's is
/// answered and asked `show`; each config is refused at a second take; the
/// audit holds the pair naming the agent; and no private key is in the
/// store, the audit, a log line, or any answer but the config's own.
#[tokio::test]
async fn registering_hands_over_two_configs_each_taken_once_and_valid() {
    let captured = Captured::default();
    let _logging = crate::test_log::scoped(
        tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish(),
    );
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    assert_eq!(handles.len(), 2);
    assert_eq!(rig.handover.held(), 2);

    let (status, headers, gate_config) = rig.take(&handles[0], &session).await;
    assert_eq!(status, StatusCode::OK, "{gate_config}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"box-a-karl-gate.toml\""
    );
    let (status, _, _) = rig.take(&handles[0], &session).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "taken once");
    let (status, headers, admin_config) = rig.take(&handles[1], &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers[header::CONTENT_DISPOSITION],
        "attachment; filename=\"box-a-karl-admin.toml\""
    );
    assert_eq!(rig.handover.held(), 0, "both taken, none held");

    let mut gate = Connector::dial(&gate_config).await;
    assert!(
        matches!(gate.recv().await, Some(ToClient::HelloAnswer { .. })),
        "the gate config is admitted"
    );
    let mut admin_plane = Connector::dial(&admin_config).await;
    assert!(matches!(
        admin_plane.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));
    assert!(
        matches!(admin_plane.recv().await, Some(ToClient::Verb { verb, .. }) if verb == "show"),
        "the admin config is admitted and asked show"
    );

    let first: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE principal = 'person' AND person_id = $1 \
         AND method = 'session' AND target_kind = 'agent' AND target_id = $2 \
         AND action = 'register' AND answers IS NULL AND refusal IS NULL",
    )
    .bind(&ada)
    .bind(&id)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    let ok: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE target_id = $1 AND action = 'register' AND outcome = 'ok'",
    )
    .bind(&id)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!((first, ok), (1, 1));

    // The log as the server wrote it, taken before this test's own checks,
    // whose statements name what they search for.
    let logged = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(
        !logged.contains("PRIVATE KEY"),
        "no private key in a log line"
    );
    for config in [&gate_config, &admin_config] {
        let table: toml::Table = config.parse().unwrap();
        let key = pem_body(table["key"].as_str().unwrap());
        assert!(key.len() > 40);
        for (what, table) in [("the store's agent", "agent"), ("the audit", "audit")] {
            let holding: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "SELECT count(*) FROM {table} t WHERE strpos(row_to_json(t)::text, $1) > 0 \
                 OR strpos(row_to_json(t)::text, 'PRIVATE KEY') > 0"
            )))
            .bind(&key)
            .fetch_one(&s.pool)
            .await
            .unwrap();
            assert_eq!(holding, 0, "no private key in {what}");
        }
        assert!(!logged.contains(&key), "no private key in a log line");
    }
}

/// **The hand-over's refusals**: another session of the same admin cannot
/// take a config, which stays held for the session that minted it; and a
/// config past its five minutes is gone, on a paused clock.
#[tokio::test]
async fn the_hand_over_refuses_another_session() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let mut key = authenticator();
    let (_, session) = admin(&rig.app, s, &mut key, "ada").await;
    let (_, headers, _) = crate::sign_in_tests::sign_in(&rig.app, &mut key, "ada").await;
    let other = crate::sign_in_tests::bearer(&headers);
    let (_, handles) = rig.register(&session, "box-a", "karl").await;

    let (status, _, _) = rig.take(&handles[0], &other).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "another session takes nothing"
    );
    let (status, _, _) = rig.take(&handles[0], &session).await;
    assert_eq!(status, StatusCode::OK, "and the minting session still can");
    let (status, _, _) = rig.take(&"0".repeat(64), &session).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "an unknown handle takes nothing"
    );
}

#[tokio::test(start_paused = true)]
async fn a_config_past_its_five_minutes_is_gone() {
    let handover = Handover::default();
    let mut reserved = handover.reserve(2).unwrap();
    let kept = reserved.hold(1, "a.toml".into(), "kept".into());
    let late = reserved.hold(1, "b.toml".into(), "late".into());
    drop(reserved);
    assert_eq!(
        handover.take(&kept, 1).map(|(_, c)| c).as_deref(),
        Some("kept")
    );
    tokio::time::advance(HANDOVER_LIFETIME + Duration::from_secs(1)).await;
    assert_eq!(handover.take(&late, 1), None, "past its lifetime");
    assert_eq!(handover.held(), 0);
}

/// **The hand-over's room is checked before any record**: with the table
/// all but full, a registration is refused as the server's limit, and
/// nothing is recorded or registered.
#[tokio::test]
async fn a_full_hand_over_refuses_before_any_record() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let mut filler = rig.handover.reserve(HANDOVERS_HELD - 1).unwrap();
    for n in 0..HANDOVERS_HELD - 1 {
        filler.hold(0, format!("{n}.toml"), String::new());
    }
    let before = rows(s, "SELECT count(*) FROM audit").await;
    let (status, _, answer) = rig
        .post(
            "/admin/agents/register",
            form(&[("box", "box-a"), ("name", "karl")]),
            &session,
        )
        .await;
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before,
        "no record"
    );
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{answer}");
    assert_eq!(rows(s, "SELECT count(*) FROM agent").await, 0);
}

/// **Rotating closes the old credentials' connections and hands over a
/// pair that admits**: the old gate config's connection closes in the act
/// and a new dial with it is refused as not live; the new gate config is
/// admitted; the audit pair names the agent.
#[tokio::test]
async fn rotating_closes_the_old_and_admits_the_new() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    let (_, _, old_config) = rig.take(&handles[0], &session).await;
    let mut old = Connector::dial(&old_config).await;
    assert!(matches!(
        old.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));

    let (status, headers, page) = rig
        .post("/admin/agents/rotate", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(
        old.closed().await,
        "the old credential's connection closed in the act"
    );
    let mut again = Connector::dial(&old_config).await;
    assert!(
        matches!(
            again.recv().await,
            Some(ToClient::Refusal {
                reason: LinkRefusal::NotLive
            })
        ),
        "the old credential is not live"
    );

    let handles = Rig::handles(&page);
    let (status, _, new_config) = rig.take(&handles[0], &session).await;
    assert_eq!(status, StatusCode::OK);
    let mut new = Connector::dial(&new_config).await;
    assert!(
        matches!(new.recv().await, Some(ToClient::HelloAnswer { .. })),
        "the new credential admits"
    );
    assert_eq!(
        rows(
            s,
            &format!(
                "SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = 'rotate' AND outcome = 'ok'"
            )
        )
        .await,
        1
    );
}

/// **A rotation whose COMMIT answers past its bound hands its pair over**
/// (Spec 2.13): karl's rotation is held just after its COMMIT answered until
/// a moment past the bound, the hand-over's reads of the chrome and the
/// admin's grants after it. The step runs on, so the page hands both
/// configs over under `no-store`, both handles redeem for the minting
/// session, one config for each plane, the new gate config admits, the old
/// one is not live, and the act's outcome is `ok`.
///
/// Perturbation: bound step four with `deadline::before`, and the request
/// is answered 503 "not made" for a rotation that landed, its two configs
/// dropped with it.
#[tokio::test]
async fn a_rotation_whose_commit_answers_past_its_bound_hands_its_pair_over() {
    use crate::deadline::hold::{self, Point};
    use crate::surfaces::admin::WRITE_BOUND_FOR_TEST;
    const BOUND: Duration = Duration::from_millis(1500);
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    let (_, _, old_config) = rig.take(&handles[0], &session).await;
    WRITE_BOUND_FOR_TEST.with(|b| b.set(Some(BOUND)));

    let held = hold::arm(Point::AdminWriteAnswered);
    let asked = tokio::time::Instant::now();
    let (status, headers, page) = rig
        .post("/admin/agents/rotate", form(&[("agent", &id)]), &session)
        .await;
    let took = asked.elapsed();
    assert_eq!(status, StatusCode::OK, "the pair is handed over: {page}");
    assert_eq!(headers[header::CACHE_CONTROL], "no-store");
    assert!(took >= BOUND, "answered past the bound: {took:?}");
    assert!(
        held.reached() && held.let_go(),
        "held after its commit until past the deadline"
    );
    drop(held);
    let handles = Rig::handles(&page);
    assert_eq!(handles.len(), 2, "both configs are held: {page}");
    let mut configs = Vec::new();
    for handle in &handles {
        let (status, _, config) = rig.take(handle, &session).await;
        assert_eq!(status, StatusCode::OK, "{config}");
        let table: toml::Table = config.parse().expect("a client config is TOML");
        configs.push((table["plane"].as_str().unwrap().to_owned(), config));
    }
    configs.sort();
    let planes: Vec<&str> = configs.iter().map(|(plane, _)| plane.as_str()).collect();
    assert_eq!(planes, ["admin", "gate"], "one config for each plane");
    let new_config = &configs[1].1;
    let mut new = Connector::dial(new_config).await;
    assert!(
        matches!(new.recv().await, Some(ToClient::HelloAnswer { .. })),
        "the new credential admits"
    );
    let mut old = Connector::dial(&old_config).await;
    assert!(
        matches!(
            old.recv().await,
            Some(ToClient::Refusal {
                reason: LinkRefusal::NotLive
            })
        ),
        "the old credential is not live"
    );
    assert_eq!(
        rows(
            s,
            &format!(
                "SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = 'rotate' AND outcome = 'ok'"
            )
        )
        .await,
        1
    );
}

/// **A rotation stalled before its COMMIT is not made, and says so**:
/// karl's rotation is held just before its COMMIT until the very deadline,
/// the converse of the pair handed over past its bound. The request is
/// answered 503 "not made" with no config handed over, the COMMIT was never
/// sent, the row keeps its gate credential, which still admits, and the
/// first record stands with no outcome.
///
/// Perturbation: await step four with no bound, and the held COMMIT goes
/// out, the row rotated while the admin is told nothing was made.
#[tokio::test]
async fn a_rotation_stalled_before_its_commit_is_not_made() {
    use crate::deadline::hold::{self, Point};
    use crate::surfaces::admin::WRITE_BOUND_FOR_TEST;
    const BOUND: Duration = Duration::from_millis(1500);
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    let (_, _, old_config) = rig.take(&handles[0], &session).await;
    let gate_fingerprint = || async {
        sqlx::query_scalar::<_, String>("SELECT gate_fingerprint FROM agent WHERE agent_id = $1")
            .bind(&id)
            .fetch_one(&s.pool)
            .await
            .unwrap()
    };
    let before = gate_fingerprint().await;
    WRITE_BOUND_FOR_TEST.with(|b| b.set(Some(BOUND)));

    let held = hold::arm(Point::AdminWrite);
    let (status, _, page) = rig
        .post("/admin/agents/rotate", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{page}");
    assert!(page.contains("the write was not made"), "{page}");
    assert!(Rig::handles(&page).is_empty(), "no config handed over");
    assert!(!held.let_go(), "the COMMIT was never sent");
    let after = held.settled(gate_fingerprint, |f| f != &before).await;
    assert_eq!(after, before, "the row keeps its gate credential");
    drop(held);
    let mut old = Connector::dial(&old_config).await;
    assert!(
        matches!(old.recv().await, Some(ToClient::HelloAnswer { .. })),
        "the old credential still admits"
    );
    let count = |what: &'static str| {
        let id = id.clone();
        async move {
            rows(
                s,
                &format!(
                    "SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = 'rotate' AND {what}"
                ),
            )
            .await
        }
    };
    assert_eq!(count("answers IS NULL AND refusal IS NULL").await, 1);
    assert_eq!(count("outcome IS NOT NULL").await, 0);
}

/// **The four-step order**: a malformed box, name or `ag-` answers 400
/// with no record from anyone; a session holding no admin grant is refused
/// with one record before any lookup, alike for a known agent and an
/// unknown one; for an admin, an unknown agent is not found and a retired
/// one is refused, each with no record.
#[tokio::test]
async fn each_write_takes_the_four_steps() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, other) = signed_in(&rig.app, s, &mut authenticator(), "dot").await;
    let (known, _) = rig.register(&session, "box-a", "karl").await;
    let (retired, _) = rig.register(&session, "box-b", "jane").await;
    for plane in [Plane::Gate, Plane::Admin] {
        let row = s.agent(&retired.parse().unwrap()).await.unwrap().unwrap();
        s.revoke_credential(&row, plane, None).await.unwrap();
    }

    let before = rows(s, "SELECT count(*) FROM audit").await;
    for (uri, body) in [
        (
            "/admin/agents/register",
            form(&[("box", "a/b"), ("name", "karl")]),
        ),
        (
            "/admin/agents/register",
            form(&[("box", ".."), ("name", "karl")]),
        ),
        (
            "/admin/agents/register",
            form(&[("box", "box-c"), ("name", "")]),
        ),
        (
            "/admin/agents/register",
            form(&[("box", "box-c"), ("name", "kar l")]),
        ),
        ("/admin/agents/rotate", form(&[("agent", "ag-0123")])),
        ("/admin/agents/rotate", form(&[("agent", "box-a/karl")])),
    ] {
        for (who, asking) in [("an admin", &session), ("a non-admin", &other)] {
            let (status, _, answer) = rig.post(uri, body.clone(), asking).await;
            assert_eq!(
                rows(s, "SELECT count(*) FROM audit").await,
                before,
                "{uri} {body} from {who}: no record"
            );
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{uri} {body} from {who}: {answer}"
            );
        }
    }

    let unknown = "ag-0123456789abcdef";
    for (agent, expected) in [
        (unknown, StatusCode::NOT_FOUND),
        (retired.as_str(), StatusCode::CONFLICT),
    ] {
        let before = rows(s, "SELECT count(*) FROM audit").await;
        let (status, _, answer) = rig
            .post("/admin/agents/rotate", form(&[("agent", agent)]), &session)
            .await;
        assert_eq!(
            rows(s, "SELECT count(*) FROM audit").await,
            before,
            "{agent}: no record"
        );
        assert_eq!(status, expected, "{agent}: {answer}");
    }

    let before = rows(s, "SELECT count(*) FROM audit").await;
    let (status, _, refused_unknown) = rig
        .post("/admin/agents/rotate", form(&[("agent", unknown)]), &other)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before + 1,
        "one record"
    );
    let (status, _, refused_known) = rig
        .post("/admin/agents/rotate", form(&[("agent", &known)]), &other)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(refused_unknown, refused_known, "no oracle");
    let (status, _, _) = rig
        .post(
            "/admin/agents/register",
            form(&[("box", "box-c"), ("name", "kim")]),
            &other,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(
        rows(s, "SELECT count(*) FROM agent WHERE box = 'box-c'").await,
        0
    );
    let (status, _, _) = send_as(&rig.app, "GET", "/admin/agents", None, &other).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "the page is an admin's");
}

/// **The authority is re-checked inside the register's transaction**: an
/// admin whose grant the host revokes between the surface's read and the
/// write is refused as its authority gone, audited as failed, and no row
/// is registered and no config held.
#[tokio::test]
async fn a_register_whose_admin_grant_was_revoked_is_refused() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    admin(&rig.app, s, &mut authenticator(), "cara").await;

    let (read, release) = hold(&READ_HOLD, &ada);
    let write = tokio::spawn({
        let (app, session) = (rig.app.clone(), session.clone());
        async move {
            send_as(
                &app,
                "POST",
                "/admin/agents/register",
                Some(form(&[("box", "box-a"), ("name", "karl")])),
                &session,
            )
            .await
        }
    });
    read.notified().await;
    let removed = host::grant_remove(s, &ada, "admin", None, None).await;
    assert!(removed.ok, "{}", removed.value);
    release.notify_one();
    let (status, _, answer) = write.await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(answer.contains("no longer stands"), "{answer}");
    assert_eq!(
        rows(s, "SELECT count(*) FROM agent").await,
        0,
        "nothing registered"
    );
    assert_eq!(rig.handover.held(), 0, "nothing handed over");
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action = 'register' AND outcome = 'failed'"
        )
        .await,
        1
    );
}

/// **An authority rotated on disk since the server loaded it refuses a
/// register**, which would mint under the retired one: the server is
/// restarted after an authority rotation.
#[tokio::test]
async fn a_register_under_a_rotated_authority_is_refused() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    std::fs::remove_dir_all(rig.dir.path().join("authority")).unwrap();
    Authority::init(&rig.dir.path().join("authority"), "weaver-web", &[]).unwrap();
    let (status, _, answer) = rig
        .post(
            "/admin/agents/register",
            form(&[("box", "box-a"), ("name", "karl")]),
            &session,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert!(answer.contains("restart"), "{answer}");
    assert_eq!(rows(s, "SELECT count(*) FROM agent").await, 0);
    let _ = &rig.authority;
}

/// **A register holds the identity exclusion shared from its re-check to
/// its commit**: held inside its transaction after the admin's re-check, a
/// revocation of that admin's grant by the host waits until the register
/// commits, and is not committed beneath it.
#[tokio::test]
async fn a_revocation_waits_for_the_register_it_would_have_refused() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    admin(&rig.app, s, &mut authenticator(), "cara").await;

    let (inside, release) = hold(&crate::store::admin::INSIDE_HOLD, &ada);
    let write = tokio::spawn({
        let (app, session) = (rig.app.clone(), session.clone());
        async move {
            send_as(
                &app,
                "POST",
                "/admin/agents/register",
                Some(form(&[("box", "box-a"), ("name", "karl")])),
                &session,
            )
            .await
        }
    });
    inside.notified().await;
    let revocation = tokio::spawn({
        let s = s.clone();
        let ada = ada.clone();
        async move { host::grant_remove(&s, &ada, "admin", None, None).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        s.is_admin(&ada).await.unwrap(),
        "the revocation waits for the register's commit"
    );
    release.notify_one();
    let (status, _, answer) = write.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{answer}");
    let removed = revocation.await.unwrap();
    assert!(removed.ok, "{}", removed.value);
    assert!(!s.is_admin(&ada).await.unwrap());
    assert_eq!(rows(s, "SELECT count(*) FROM agent").await, 1);
}

/// **The hand-over's bound counts the writes in flight**: with room for
/// four configs, five admins register at once, each held after reserving
/// at step three; two reserve their two slots and land, the other three are
/// refused before any record, and the configs held and reserved never pass
/// the bound.
#[tokio::test]
async fn concurrent_registers_never_pass_the_hand_overs_bound() {
    let Some(rig) = rig_with(Handover::with_cap(4)).await else {
        return;
    };
    let s = rig.store();
    let mut admins = Vec::new();
    for name in ["ada", "bea", "cara", "dot", "eve"] {
        admins.push(admin(&rig.app, s, &mut authenticator(), name).await);
    }
    let before = rows(s, "SELECT count(*) FROM audit").await;
    let mut releases = Vec::new();
    let mut writes = Vec::new();
    for (n, (person, session)) in admins.iter().enumerate() {
        let (_, release) = hold(&READ_HOLD, person);
        releases.push(release);
        writes.push(tokio::spawn({
            let (app, session) = (rig.app.clone(), session.clone());
            let r#box = format!("box-{n}");
            async move {
                send_as(
                    &app,
                    "POST",
                    "/admin/agents/register",
                    Some(form(&[("box", &r#box), ("name", "karl")])),
                    &session,
                )
                .await
            }
        }));
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(rig.handover.occupied() <= 4, "reserved within the bound");
    for release in &releases {
        release.notify_one();
    }
    let mut answered = Vec::new();
    for write in writes {
        answered.push(write.await.unwrap().0);
    }
    let landed = answered.iter().filter(|s| **s == StatusCode::OK).count();
    let refused = answered
        .iter()
        .filter(|s| **s == StatusCode::SERVICE_UNAVAILABLE)
        .count();
    assert_eq!((landed, refused), (2, 3), "{answered:?}");
    assert_eq!(rig.handover.held(), 4);
    assert_eq!(rig.handover.occupied(), 4, "no slot left reserved");
    assert_eq!(rows(s, "SELECT count(*) FROM agent").await, 2);
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before + 4,
        "two audit pairs, and none for the refused"
    );
    // Holds left unused by the refused writes are cleared for later tests.
    READ_HOLD
        .lock()
        .unwrap()
        .retain(|(key, ..)| !admins.iter().any(|(person, _)| person == key));
}

/// **A rotation re-checks, inside its transaction, the row step three
/// resolved**: the host revokes both of a live row's credentials while a
/// rotation is held after step three, and the rotation is refused as
/// retired, audited as failed, the row left retired and nothing minted or
/// handed over.
#[tokio::test]
async fn a_row_retired_mid_rotation_stays_retired() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    for handle in &handles {
        rig.take(handle, &session).await;
    }
    let before: (String, String) =
        sqlx::query_as("SELECT gate_fingerprint, admin_fingerprint FROM agent WHERE agent_id = $1")
            .bind(&id)
            .fetch_one(&s.pool)
            .await
            .unwrap();

    let (read, release) = hold(&READ_HOLD, &ada);
    let rotation = tokio::spawn({
        let (app, session, body) = (rig.app.clone(), session.clone(), form(&[("agent", &id)]));
        async move { send_as(&app, "POST", "/admin/agents/rotate", Some(body), &session).await }
    });
    read.notified().await;
    for plane in [Plane::Gate, Plane::Admin] {
        let revoked = crate::link::verbs::revoke(s, &id, plane, None).await;
        assert!(revoked.ok, "{}", revoked.value);
    }
    release.notify_one();
    let (status, _, answer) = rotation.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert!(answer.contains("no live credential"), "{answer}");
    let after: (String, String, String, String) = sqlx::query_as(
        "SELECT gate_state, admin_state, gate_fingerprint, admin_fingerprint FROM agent \
         WHERE agent_id = $1",
    )
    .bind(&id)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(
        after,
        ("revoked".into(), "revoked".into(), before.0, before.1),
        "the row stays retired, its fingerprints untouched"
    );
    assert_eq!(
        rig.handover.occupied(),
        0,
        "nothing handed over or reserved"
    );
    assert_eq!(
        rows(
            s,
            &format!(
                "SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = 'rotate' AND outcome = 'failed'"
            )
        )
        .await,
        1
    );
}

/// An agent registered through the page with both configs taken and both
/// connectors admitted: the row's identity and the two connectors.
async fn connected(
    rig: &Rig,
    session: &str,
    r#box: &str,
) -> (String, Connector, Connector, String) {
    let (id, handles) = rig.register(session, r#box, "karl").await;
    let (_, _, gate_config) = rig.take(&handles[0], session).await;
    let (_, _, admin_config) = rig.take(&handles[1], session).await;
    let mut gate = Connector::dial(&gate_config).await;
    assert!(matches!(
        gate.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));
    let mut admin = Connector::dial(&admin_config).await;
    assert!(matches!(
        admin.recv().await,
        Some(ToClient::HelloAnswer { .. })
    ));
    assert!(matches!(admin.recv().await, Some(ToClient::Verb { .. })));
    (id, gate, admin, gate_config)
}

async fn states(s: &Store, id: &str) -> (String, String) {
    sqlx::query_as("SELECT gate_state, admin_state FROM agent WHERE agent_id = $1")
        .bind(id)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **Revoking one plane closes its connection in the act and leaves the
/// other**: the gate's credential revoked and its connection closed, the
/// admin plane's live and connected, the audit pair naming the agent, and
/// the revoked config refused at its next dial.
#[tokio::test]
async fn revoking_a_plane_closes_its_connection_and_leaves_the_other() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, mut gate, mut admin_plane, gate_config) = connected(&rig, &session, "box-a").await;

    let (status, headers, answer) = rig
        .post(
            "/admin/agents/revoke",
            form(&[("agent", &id), ("plane", "gate")]),
            &session,
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(headers[header::LOCATION], "/admin/agents");
    assert!(
        gate.closed().await,
        "the gate's connection closed in the act"
    );
    assert!(!admin_plane.closed().await, "the admin plane's stays open");
    assert_eq!(states(s, &id).await, ("revoked".into(), "live".into()));
    let mut again = Connector::dial(&gate_config).await;
    assert!(matches!(
        again.recv().await,
        Some(ToClient::Refusal {
            reason: LinkRefusal::NotLive
        })
    ));
    let pair: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE answers IS NULL AND refusal IS NULL), \
         count(*) FILTER (WHERE outcome = 'ok') FROM audit \
         WHERE person_id = $1 AND method = 'session' AND target_kind = 'agent' \
         AND target_id = $2 AND action = 'revoke'",
    )
    .bind(&ada)
    .bind(&id)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(pair, (1, 1));
}

/// **Retiring revokes every live plane in one write**: both connections
/// close in the act, the page reads the row retired with no write but to
/// register again, and a rotation of it is refused.
#[tokio::test]
async fn retiring_closes_both_planes_and_the_row_is_never_live_again() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, mut gate, mut admin_plane, _) = connected(&rig, &session, "box-a").await;

    let (status, _, answer) = rig
        .post("/admin/agents/retire", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(
        gate.closed().await && admin_plane.closed().await,
        "both closed in the act"
    );
    assert_eq!(states(s, &id).await, ("revoked".into(), "revoked".into()));
    let (_, _, page) = send_as(&rig.app, "GET", "/admin/agents", None, &session).await;
    crate::surfaces::assert_framed(&page);
    assert!(page.contains("retired; register it again"), "{page}");
    assert!(
        !page.contains(&format!("name=\"agent\" value=\"{id}\"")),
        "no write on it"
    );
    let (status, _, _) = rig
        .post("/admin/agents/rotate", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a retired row is never rotated live"
    );
    assert_eq!(
        rows(
            s,
            &format!("SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = 'retire' AND outcome = 'ok'")
        )
        .await,
        1
    );
}

/// **The four steps of a revocation and a retirement**: malformed asks
/// refused before any record from anyone; a non-admin refused with one
/// record, alike for a known agent and an unknown one; for an admin, an
/// unknown agent not found, a plane already revoked and a retired row
/// refused, none recorded.
#[tokio::test]
async fn revoking_and_retiring_take_the_four_steps() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, other) = signed_in(&rig.app, s, &mut authenticator(), "dot").await;
    let (known, _) = rig.register(&session, "box-a", "karl").await;
    let (half, _) = rig.register(&session, "box-b", "jane").await;
    let (retired, _) = rig.register(&session, "box-c", "kim").await;
    for (agent, planes) in [
        (&half, &[Plane::Gate][..]),
        (&retired, &[Plane::Gate, Plane::Admin][..]),
    ] {
        for plane in planes {
            let revoked = crate::link::verbs::revoke(s, agent, *plane, None).await;
            assert!(revoked.ok, "{}", revoked.value);
        }
    }

    let before = rows(s, "SELECT count(*) FROM audit").await;
    for (uri, body) in [
        (
            "/admin/agents/revoke",
            form(&[("agent", "ag-0123"), ("plane", "gate")]),
        ),
        (
            "/admin/agents/revoke",
            form(&[("agent", &known), ("plane", "both")]),
        ),
        ("/admin/agents/retire", form(&[("agent", "box-a/karl")])),
    ] {
        for (who, asking) in [("an admin", &session), ("a non-admin", &other)] {
            let (status, _, answer) = rig.post(uri, body.clone(), asking).await;
            assert_eq!(
                rows(s, "SELECT count(*) FROM audit").await,
                before,
                "{uri} {body} from {who}: no record"
            );
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{uri} {body} from {who}: {answer}"
            );
        }
    }

    let unknown = "ag-0123456789abcdef";
    for (uri, body, expected) in [
        (
            "/admin/agents/revoke",
            form(&[("agent", unknown), ("plane", "gate")]),
            StatusCode::NOT_FOUND,
        ),
        (
            "/admin/agents/retire",
            form(&[("agent", unknown)]),
            StatusCode::NOT_FOUND,
        ),
        (
            "/admin/agents/revoke",
            form(&[("agent", &half), ("plane", "gate")]),
            StatusCode::CONFLICT,
        ),
        (
            "/admin/agents/retire",
            form(&[("agent", &retired)]),
            StatusCode::CONFLICT,
        ),
    ] {
        let before = rows(s, "SELECT count(*) FROM audit").await;
        let (status, _, answer) = rig.post(uri, body.clone(), &session).await;
        assert_eq!(
            rows(s, "SELECT count(*) FROM audit").await,
            before,
            "{uri} {body}: no record"
        );
        assert_eq!(status, expected, "{uri} {body}: {answer}");
    }

    for (uri, unknown_body, known_body) in [
        (
            "/admin/agents/revoke",
            form(&[("agent", unknown), ("plane", "gate")]),
            form(&[("agent", &known), ("plane", "gate")]),
        ),
        (
            "/admin/agents/retire",
            form(&[("agent", unknown)]),
            form(&[("agent", &known)]),
        ),
    ] {
        let before = rows(s, "SELECT count(*) FROM audit").await;
        let (status, _, refused_unknown) = rig.post(uri, unknown_body, &other).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(
            rows(s, "SELECT count(*) FROM audit").await,
            before + 1,
            "one record"
        );
        let (status, _, refused_known) = rig.post(uri, known_body, &other).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(refused_unknown, refused_known, "{uri}: no oracle");
    }
    assert_eq!(states(s, &known).await, ("live".into(), "live".into()));
}

/// **The row is re-read inside the write's transaction**: the host revokes
/// the plane, or retires the agent, while the web's revocation or
/// retirement is held after step three; the web's write is refused,
/// audited as failed, and the row is as the host left it.
#[tokio::test]
async fn a_plane_revoked_mid_write_refuses_the_write() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, _) = rig.register(&session, "box-a", "karl").await;

    for (uri, body, host_planes, action) in [
        (
            "/admin/agents/revoke",
            form(&[("agent", &id), ("plane", "gate")]),
            &[Plane::Gate][..],
            "revoke",
        ),
        (
            "/admin/agents/retire",
            form(&[("agent", &id)]),
            &[Plane::Admin][..],
            "retire",
        ),
    ] {
        let (read, release) = hold(&READ_HOLD, &ada);
        let write = tokio::spawn({
            let (app, session, body) = (rig.app.clone(), session.clone(), body.clone());
            async move { send_as(&app, "POST", uri, Some(body), &session).await }
        });
        read.notified().await;
        for plane in host_planes {
            let revoked = crate::link::verbs::revoke(s, &id, *plane, None).await;
            assert!(revoked.ok, "{}", revoked.value);
        }
        release.notify_one();
        let (status, _, answer) = write.await.unwrap();
        assert_eq!(status, StatusCode::CONFLICT, "{uri}: {answer}");
        assert_eq!(
            rows(
                s,
                &format!("SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = '{action}' AND outcome = 'failed'")
            )
            .await,
            1,
            "{uri}"
        );
    }
    assert_eq!(states(s, &id).await, ("revoked".into(), "revoked".into()));
}

/// **A revocation holds the identity exclusion shared from its re-check to
/// its commit**: held inside its transaction after the admin's re-check, a
/// removal of that admin's grant by the host waits until it commits.
#[tokio::test]
async fn a_grant_removal_waits_for_the_revocation_it_would_have_refused() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    admin(&rig.app, s, &mut authenticator(), "cara").await;
    let (id, _) = rig.register(&session, "box-a", "karl").await;

    let (inside, release) = hold(&crate::store::admin::INSIDE_HOLD, &ada);
    let write = tokio::spawn({
        let (app, session, body) = (
            rig.app.clone(),
            session.clone(),
            form(&[("agent", &id), ("plane", "gate")]),
        );
        async move { send_as(&app, "POST", "/admin/agents/revoke", Some(body), &session).await }
    });
    inside.notified().await;
    let removal = tokio::spawn({
        let (s, ada) = (s.clone(), ada.clone());
        async move { host::grant_remove(&s, &ada, "admin", None, None).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        s.is_admin(&ada).await.unwrap(),
        "the removal waits for the revocation's commit"
    );
    release.notify_one();
    let (status, _, answer) = write.await.unwrap();
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(removal.await.unwrap().ok);
    assert_eq!(states(s, &id).await, ("revoked".into(), "live".into()));
}

/// **A grant on a retired agent stands, and the grants page marks it**:
/// the row is never live again and a new registration is a new row, so the
/// grant names nothing an agent will answer, and an admin revokes it there.
#[tokio::test]
async fn the_grants_page_marks_a_grant_on_a_retired_agent() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (bea, _) = crate::passkeys_tests::person_with_token(s, "bea").await;
    let (id, _) = rig.register(&session, "box-a", "karl").await;
    let (other, _) = rig.register(&session, "box-b", "jane").await;
    for agent in [&id, &other] {
        let granted = host::grant_add(s, &bea, "observer", Some(agent), None).await;
        assert!(granted.ok, "{}", granted.value);
    }
    let marked = |page: &str, agent: &str| {
        let at = page.find(&format!("<td>{agent}")).unwrap();
        let end = at + page[at..].find("</td>").unwrap();
        page[at..end].contains("agent retired")
    };
    let (_, _, page) = send_as(&rig.app, "GET", "/admin/grants", None, &session).await;
    assert!(
        !marked(&page, "box-a/karl") && !marked(&page, "box-b/jane"),
        "{page}"
    );

    let (status, _, _) = rig
        .post("/admin/agents/retire", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (_, _, page) = send_as(&rig.app, "GET", "/admin/grants", None, &session).await;
    assert!(
        marked(&page, "box-a/karl"),
        "the retired agent's grant is marked: {page}"
    );
    assert!(!marked(&page, "box-b/jane"), "a live agent's is not");
    assert!(
        s.live_grant(&bea, "observer", Some(&id))
            .await
            .unwrap()
            .is_some(),
        "the grant stands"
    );
}

/// **A revocation or a retirement whose commit's answer is lost reads back
/// its outcome** (Spec 2.13): the planes revoked and notified, the answer
/// lost after PostgreSQL applied the commit, and the write answers its
/// success with its outcome `ok`, where a retry would meet step three's
/// refusal and the record could never be put right.
#[tokio::test]
async fn a_revocation_whose_commits_answer_is_lost_reads_back_its_outcome() {
    use crate::store::commit::{lose_the_commits_answer, the_answer_was_lost};
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, mut gate, mut admin_plane, _) = connected(&rig, &session, "box-a").await;

    lose_the_commits_answer("agent revoke");
    let (status, _, answer) = rig
        .post(
            "/admin/agents/revoke",
            form(&[("agent", &id), ("plane", "gate")]),
            &session,
        )
        .await;
    assert!(the_answer_was_lost());
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(gate.closed().await, "notified with the commit");

    lose_the_commits_answer("agent revoke");
    let (status, _, answer) = rig
        .post("/admin/agents/retire", form(&[("agent", &id)]), &session)
        .await;
    assert!(the_answer_was_lost());
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(admin_plane.closed().await);
    assert_eq!(states(s, &id).await, ("revoked".into(), "revoked".into()));
    for action in ["revoke", "retire"] {
        assert_eq!(
            rows(
                s,
                &format!("SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = '{action}' AND outcome = 'ok'")
            )
            .await,
            1,
            "{action}"
        );
    }
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit WHERE outcome = 'failed'").await,
        0
    );
}

/// **A registration or a rotation through the server whose commit's answer
/// is lost reads back its outcome**: the configs are handed over
/// and the outcome recorded `ok`.
#[tokio::test]
async fn a_register_whose_commits_answer_is_lost_reads_back_its_outcome() {
    use crate::store::commit::{lose_the_commits_answer, the_answer_was_lost};
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;

    lose_the_commits_answer("register");
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    assert!(the_answer_was_lost());
    assert_eq!(handles.len(), 2, "the configs handed over");

    lose_the_commits_answer("rotate");
    let (status, _, page) = rig
        .post("/admin/agents/rotate", form(&[("agent", &id)]), &session)
        .await;
    assert!(the_answer_was_lost());
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(Rig::handles(&page).len(), 2);
    for action in ["register", "rotate"] {
        assert_eq!(
            rows(
                s,
                &format!("SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = '{action}' AND outcome = 'ok'")
            )
            .await,
            1,
            "{action}"
        );
    }
}

/// A hold between a lost commit answer of `kind` and its read-back.
fn hold_the_read_back(kind: &'static str) -> (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>) {
    let (read, release) = (
        Arc::new(tokio::sync::Notify::new()),
        Arc::new(tokio::sync::Notify::new()),
    );
    crate::store::commit::READ_BACK_HOLD.lock().unwrap().push((
        kind,
        std::thread::current().id(),
        read.clone(),
        release.clone(),
    ));
    (read, release)
}

/// **A lost answer is read back by the act's own outcome, which no later
/// act can overwrite**: a revocation's commit answer is lost, and before
/// its read-back a rotation of the same agent commits, making the plane
/// live again under a new fingerprint; the revocation still answers its
/// success and its outcome reads `ok`, where reading back the plane's state
/// would have called it failed.
#[tokio::test]
async fn a_revocation_whose_answer_is_lost_lands_though_a_rotation_follows() {
    use crate::store::commit::{lose_the_commits_answer, the_answer_was_lost};
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, handles) = rig.register(&session, "box-a", "karl").await;
    for handle in &handles {
        rig.take(handle, &session).await;
    }

    lose_the_commits_answer("agent revoke");
    let (read, release) = hold_the_read_back("agent revoke");
    let revocation = tokio::spawn({
        let (app, session, body) = (
            rig.app.clone(),
            session.clone(),
            form(&[("agent", &id), ("plane", "gate")]),
        );
        async move { send_as(&app, "POST", "/admin/agents/revoke", Some(body), &session).await }
    });
    read.notified().await;
    assert!(the_answer_was_lost());
    let (status, _, page) = rig
        .post("/admin/agents/rotate", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::OK, "the rotation commits first: {page}");
    assert_eq!(states(s, &id).await, ("live".into(), "live".into()));
    release.notify_one();
    let (status, _, answer) = revocation.await.unwrap();
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(
        rows(
            s,
            &format!("SELECT count(*) FROM audit WHERE target_id = '{id}' AND action = 'revoke' AND outcome = 'ok'")
        )
        .await,
        1
    );
    assert_eq!(
        rows(s, "SELECT count(*) FROM audit WHERE outcome = 'failed'").await,
        0
    );
}

/// **A retired row is never live again, the host's `rotate` included**:
/// an agent retired through the web is refused by the host's rotation of
/// its identity, which names the refusal and tells the operator to register
/// again; the row stays retired, its grant stands and is still marked, and
/// no client config is left behind.
#[tokio::test]
async fn the_hosts_rotate_refuses_a_retired_row() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (bea, _) = crate::passkeys_tests::person_with_token(s, "bea").await;
    let (id, _) = rig.register(&session, "box-a", "karl").await;
    let granted = host::grant_add(s, &bea, "observer", Some(&id), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, _, _) = rig
        .post("/admin/agents/retire", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);

    let cfg = server_config(rig.dir.path(), rig.listener.address());
    let out = crate::test_dir::tempdir().unwrap();
    let rotated =
        crate::link::verbs::rotate(s, &cfg, &rig.authority, &id, out.path(), Some("lab")).await;
    assert!(!rotated.ok, "{}", rotated.value);
    let error = rotated.value["error"].as_str().unwrap_or_default();
    assert!(error.contains("register the agent again"), "{error}");
    assert_eq!(states(s, &id).await, ("revoked".into(), "revoked".into()));
    assert!(
        s.live_grant(&bea, "observer", Some(&id))
            .await
            .unwrap()
            .is_some(),
        "the grant stands"
    );
    let (_, _, page) = send_as(&rig.app, "GET", "/admin/grants", None, &session).await;
    assert!(page.contains("agent retired"), "and is still marked");
    let configs = walk(out.path());
    assert!(
        !configs.iter().any(|p| p.ends_with(".toml")),
        "no client config left behind: {configs:?}"
    );
}

/// Every file under `dir`, by path.
fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path.display().to_string());
            }
        }
    }
    found
}

/// **A retirement is one authored edit**: retiring a row with both planes
/// live advances its version by exactly one, and so does revoking one
/// plane.
#[tokio::test]
async fn a_retirement_advances_the_version_once() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (both, _) = rig.register(&session, "box-a", "karl").await;
    let before = agent_version(s, &both).await;
    let (status, _, _) = rig
        .post("/admin/agents/retire", form(&[("agent", &both)]), &session)
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(states(s, &both).await, ("revoked".into(), "revoked".into()));
    assert_eq!(
        agent_version(s, &both).await,
        before + 1,
        "one edit, one version"
    );

    let (one, _) = rig.register(&session, "box-b", "jane").await;
    let before = agent_version(s, &one).await;
    let (status, _, _) = rig
        .post(
            "/admin/agents/revoke",
            form(&[("agent", &one), ("plane", "admin")]),
            &session,
        )
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(agent_version(s, &one).await, before + 1);
}

async fn agent_version(s: &Store, id: &str) -> i64 {
    sqlx::query_scalar("SELECT version FROM agent WHERE agent_id = $1")
        .bind(id)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **Each write to the register runs from its first record to its outcome
/// in a task the request cannot cancel** (Spec 2.13, #150): each of the
/// four writes' requests is dropped while its first record is held, and
/// the write still lands, its first record answered by its `ok` outcome
/// and the register's page reading the state it left; a register's and a
/// rotation's two client configs are held for the session that minted
/// them, as the hand-over holds any.
///
/// Perturbation: run step four in the request's own future
/// (`admin::step_four` awaiting it in place), and the first write's
/// outcome never comes.
#[tokio::test]
async fn each_write_to_the_register_dropped_at_its_first_record_still_lands() {
    use crate::admin_tests::{dropped_at_the_hold, outcome_of};
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let handover = &rig.handover;
    let minted_by: i64 =
        sqlx::query_scalar("SELECT session_id FROM session WHERE bearer_digest = $1")
            .bind(gate::digest(&session))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    let held = |n: usize| async move {
        let until = tokio::time::Instant::now() + Duration::from_secs(10);
        while handover.held_for(minted_by) != n {
            assert!(
                tokio::time::Instant::now() < until,
                "{n} configs are not held for the minting session: {} are, of {}",
                handover.held_for(minted_by),
                handover.held()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(handover.held(), n, "and none for another session");
    };
    let row = |id: String| {
        let (app, session) = (rig.app.clone(), session.clone());
        async move {
            let (status, _, page) = send_as(&app, "GET", "/admin/agents", None, &session).await;
            assert_eq!(status, StatusCode::OK, "the register's page is served");
            let at = page
                .find(&format!("<td>{id}</td>"))
                .unwrap_or_else(|| panic!("the register's page lists {id}"));
            let start = page[..at].rfind("<tr>").unwrap();
            let end = at + page[at..].find("</tr>").unwrap();
            page[start..end].to_owned()
        }
    };

    dropped_at_the_hold(
        &rig.app,
        &ada,
        &session,
        "/admin/agents/register",
        form(&[("box", "box-a"), ("name", "karl")]),
    )
    .await;
    assert_eq!(outcome_of(s, &ada, "register").await, "ok");
    held(2).await;
    let id: String = sqlx::query_scalar(
        "SELECT agent_id FROM agent WHERE box = 'box-a' AND name = 'karl' AND gate_state = 'live'",
    )
    .fetch_one(&s.pool)
    .await
    .expect("the register landed");
    let page = row(id.clone()).await;
    assert!(page.contains("<td>box-a/karl</td>"), "{page}");
    assert!(
        page.contains("<td>live</td>\n      <td>live</td>"),
        "{page}"
    );

    dropped_at_the_hold(
        &rig.app,
        &ada,
        &session,
        "/admin/agents/rotate",
        form(&[("agent", &id)]),
    )
    .await;
    assert_eq!(outcome_of(s, &ada, "rotate").await, "ok");
    held(4).await;
    let page = row(id.clone()).await;
    assert!(
        page.contains("<td>live</td>\n      <td>live</td>"),
        "{page}"
    );
    assert!(page.contains("/admin/agents/rotate"), "{page}");

    dropped_at_the_hold(
        &rig.app,
        &ada,
        &session,
        "/admin/agents/revoke",
        form(&[("agent", &id), ("plane", "gate")]),
    )
    .await;
    assert_eq!(outcome_of(s, &ada, "revoke").await, "ok");
    assert_eq!(states(s, &id).await.0, "revoked", "the revocation landed");
    let page = row(id.clone()).await;
    assert!(
        page.contains("<td>revoked</td>\n      <td>live</td>"),
        "{page}"
    );
    assert!(!page.contains("Revoke gate"), "{page}");
    assert!(page.contains("Revoke admin"), "{page}");

    dropped_at_the_hold(
        &rig.app,
        &ada,
        &session,
        "/admin/agents/retire",
        form(&[("agent", &id)]),
    )
    .await;
    assert_eq!(outcome_of(s, &ada, "retire").await, "ok");
    assert_eq!(
        states(s, &id).await,
        ("revoked".to_owned(), "revoked".to_owned()),
        "the retirement landed"
    );
    let page = row(id.clone()).await;
    assert!(
        page.contains("<td>revoked</td>\n      <td>revoked</td>"),
        "{page}"
    );
    assert!(page.contains("retired; register it again"), "{page}");
}

/// **The admin's revoke of a gate not enrolled and rotation of its row are
/// refused `not_enrolled`, and a retire leaves it not enrolled** (Spec
/// 2.12, section 9): on a row whose gate is `none`, as a pairing writes it,
/// the register page offers no rotation and no gate revoke, a posted gate
/// revoke and a posted rotation are each refused 409 naming `not_enrolled`
/// with the row's version kept, and the retire revokes the admin plane
/// alone.
///
/// Perturbation: drop step three's state check and the store's, and the
/// revoke fails at the schema rather than as `not_enrolled`; drop the
/// rotation's, and a gate certificate is minted for a plane the pairing
/// never enrolled.
#[tokio::test]
async fn the_admins_revoke_and_rotation_of_a_gate_not_enrolled_are_refused_not_enrolled() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (id, _) = rig.register(&session, "box-a", "karl").await;
    sqlx::query(
        "UPDATE agent SET gate_fingerprint = NULL, gate_authority = NULL, gate_state = 'none' \
         WHERE agent_id = $1",
    )
    .bind(&id)
    .execute(&s.pool)
    .await
    .unwrap();
    let version = || async {
        sqlx::query_scalar::<_, i64>("SELECT version FROM agent WHERE agent_id = $1")
            .bind(&id)
            .fetch_one(&s.pool)
            .await
            .unwrap()
    };
    let before = version().await;
    let (_, _, page) = send_as(&rig.app, "GET", "/admin/agents", None, &session).await;
    assert!(
        page.contains(r#"<span class="status fail">admin alone, admin-con down</span>"#),
        "{page}"
    );
    assert!(page.contains("<td>not enrolled</td>"), "{page}");
    assert!(!page.contains("Revoke gate"), "{page}");
    assert!(!page.contains("/admin/agents/rotate"), "{page}");
    for (uri, body) in [
        (
            "/admin/agents/revoke",
            form(&[("agent", &id), ("plane", "gate")]),
        ),
        ("/admin/agents/rotate", form(&[("agent", &id)])),
    ] {
        let (status, _, answer) = rig.post(uri, body, &session).await;
        assert_eq!(status, StatusCode::CONFLICT, "{uri}: {answer}");
        assert!(answer.contains("not_enrolled"), "{uri}: {answer}");
    }
    assert_eq!(version().await, before, "nothing was written");
    let (status, _, answer) = rig
        .post("/admin/agents/retire", form(&[("agent", &id)]), &session)
        .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(states(s, &id).await, ("none".into(), "revoked".into()));
}

/// **The register page and the agent page say how many doors** (Spec 8), and
/// gate-con's config carries the data door's address: with one door, both
/// pages say "one door for both planes" and both configs name the one
/// address; with two, both say "a door for each plane", gate-con's config
/// names the data door and admin-con's the management door.
///
/// Perturbation: drop the line from the register page, and it fails; write
/// the management door's address into gate-con's config, and the two-door
/// case fails (each measured 2026-11-25).
#[tokio::test]
async fn the_register_page_and_the_agent_page_say_how_many_doors() {
    for two in [false, true] {
        let rig = if two { rig_doors().await } else { rig().await };
        let Some(rig) = rig else { return };
        let s = rig.store();
        let (ada, session) = admin(&rig.app, s, &mut authenticator(), "ada").await;
        let (karl, handles) = rig.register(&session, "box-a", "karl").await;
        let granted = host::grant_add(s, &ada, "observer", Some(&karl), None).await;
        assert!(granted.ok, "{}", granted.value);
        let (said, unsaid) = if two {
            ("a door for each plane", "one door for both planes")
        } else {
            ("one door for both planes", "a door for each plane")
        };
        let (_, _, register) = send_as(&rig.app, "GET", "/admin/agents", None, &session).await;
        let (_, _, page) =
            send_as(&rig.app, "GET", &format!("/agents/{karl}"), None, &session).await;
        for (shown, what) in [(&register, "the register page"), (&page, "the agent page")] {
            assert!(
                shown.to_lowercase().contains(said) && !shown.to_lowercase().contains(unsaid),
                "{what} with two doors {two}: {shown}"
            );
        }
        let server_of = |config: &str| -> String {
            config.parse::<toml::Table>().unwrap()["server"]
                .as_str()
                .unwrap()
                .to_owned()
        };
        let (_, _, gate) = rig.take(&handles[0], &session).await;
        let (_, _, admin_config) = rig.take(&handles[1], &session).await;
        let management = rig.listener.address().to_string();
        let data = rig.listener.data_address().map(|d| d.to_string());
        assert_eq!(server_of(&admin_config), management);
        assert_eq!(server_of(&gate), data.unwrap_or(management));
    }
}
