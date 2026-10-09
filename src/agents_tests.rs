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
use crate::link::frames::{FromClient, Plane, Position, Refusal as LinkRefusal, ToClient};
use crate::link::listener::Listener;
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys};
use crate::sign_in_tests::rows;
use crate::store::Store;
use crate::store::read::tests::{Fresh, fresh_store};
use crate::surfaces::admin::READ_HOLD;
use crate::surfaces::agents::{HANDOVER_LIFETIME, HANDOVERS_HELD, Handover, Seams, routes};
use crate::surfaces::gate;

/// A store of its own, an authority minted for the test, a listener over
/// both, and the app with the agents surface mounted as the server mounts
/// it.
struct Rig {
    fresh: Fresh,
    dir: tempfile::TempDir,
    authority: Arc<Authority>,
    listener: Listener,
    handover: Handover,
    app: axum::Router,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.listener.stop_now();
    }
}

fn server_config(dir: &std::path::Path, link: std::net::SocketAddr) -> crate::config::ServerConfig {
    crate::config::ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:0".into(),
        database: String::new(),
        authority_dir: dir.join("authority"),
        silence_bound_secs: 60,
        link_address: Some(link.to_string()),
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

async fn rig() -> Option<Rig> {
    rig_with(Handover::default()).await
}

async fn rig_with(handover: Handover) -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = tempfile::tempdir().unwrap();
    let authority =
        Arc::new(Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap());
    let listener = Listener::start(
        fresh.store.clone(),
        &authority,
        "127.0.0.1:0",
        Duration::from_secs(60),
    )
    .await
    .unwrap();
    let cfg = Arc::new(server_config(dir.path(), listener.address()));
    let policy = gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
    };
    let agents = gate::guard(
        crate::surfaces::script_policy(routes(Seams {
            policy: policy.clone(),
            cfg,
            authority: authority.clone(),
            handover: handover.clone(),
        }))
        .with_state(fresh.store.clone()),
        policy,
    );
    let app = app(&fresh.store, Some(passkeys())).merge(agents);
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
    fn store(&self) -> &Store {
        &self.fresh.store
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
    async fn register(&self, session: &str, r#box: &str, name: &str) -> (String, Vec<String>) {
        let (status, headers, page) = self
            .post(
                "/admin/agents/register",
                form(&[("box", r#box), ("name", name)]),
                session,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store");
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
    async fn take(&self, handle: &str, session: &str) -> (StatusCode, HeaderMap, String) {
        self.post("/admin/agents/config", form(&[("handle", handle)]), session)
            .await
    }
}

/// A connector built from a client config: its PEMs pinned and presented,
/// dialing the listener and saying hello on its plane.
struct Connector {
    reader: BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    writer: tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
}

impl Connector {
    async fn dial(config: &str) -> Self {
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
                ceiling: (plane == Plane::Admin).then(|| vec!["show".to_owned()]),
                door: (plane == Plane::Admin).then_some(true),
            })
            .await;
        connector
    }

    async fn send(&mut self, frame: FromClient) {
        let mut line = serde_json::to_string(&frame).unwrap();
        line.push('\n');
        let _ = self.writer.write_all(line.as_bytes()).await;
    }

    async fn recv(&mut self) -> Option<ToClient> {
        let mut line = String::new();
        match tokio::time::timeout(Duration::from_secs(5), self.reader.read_line(&mut line)).await {
            Ok(Ok(0)) | Err(_) | Ok(Err(_)) => None,
            Ok(Ok(_)) => serde_json::from_str(line.trim_end()).ok(),
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

/// The log lines at every level, captured on the test's own thread.
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
    let _logging = tracing::subscriber::set_default(
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
