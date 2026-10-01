//! The link's instruments: fake connectors holding minted credentials drive
//! a listener over mutual TLS against a live PostgreSQL named by
//! `DATABASE_URL`. Without it the DB-backed tests print `skipped:` and pass
//! asserting nothing, as the store's own do. Each test registers its own
//! agent under a box named by a fresh identifier, and the tests run one at
//! a time because a listener's start resets every row's link state, which
//! is the claim and not an accident.
//!
//! Every perturbation below names its guard, and the act that landed it
//! showed the test failing with the guard removed, per Spec section 9.

use super::authority::{Authority, ClientCredential, client_tls, fingerprint};
use super::frames::{FromClient, Plane, Position, Refusal, ToClient};
use super::listener::Listener;
use super::register::{Agent, CredentialState};
use crate::lifecycle::VerbOutcome;
use crate::store::{AgentId, Store};
use crate::traceview::TraceEvent;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

const SILENCE: Duration = Duration::from_secs(60);
const SOON: Duration = Duration::from_secs(5);

fn serial() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn store() -> Option<Store> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipped: DATABASE_URL is not set, and the register is tested against a schema");
        return None;
    };
    Some(Store::connect(&url).await.expect("connect and migrate"))
}

fn position(offset: u64) -> Position {
    Position {
        generation: "g1".into(),
        offset,
        digest: format!("d{offset}"),
    }
}

fn show_answer(agent: &str, state: &str, load: Option<serde_json::Value>) -> VerbOutcome {
    let mut answer = json!({ "kind": "state", "state": state });
    if let Some(load) = load {
        answer["load"] = load;
    }
    VerbOutcome {
        verb: "show".into(),
        agent: agent.into(),
        exit_code: Some(0),
        answer: Some(answer),
        raw_stdout: None,
        stderr: None,
        timed_out: false,
    }
}

fn list_answer(rows: &[(&str, &str)]) -> VerbOutcome {
    let agents: Vec<serde_json::Value> = rows
        .iter()
        .map(|(name, state)| json!({ "name": name, "state": state }))
        .collect();
    VerbOutcome {
        verb: "list".into(),
        agent: String::new(),
        exit_code: Some(0),
        answer: Some(json!({ "kind": "agents", "agents": agents })),
        raw_stdout: None,
        stderr: None,
        timed_out: false,
    }
}

/// A trace event with its own time, `wall_ms`, as the envelope carries it.
const WALL_MS: i64 = 1_790_000_000_000;

fn trace_event(seq: u64, kind: &str, payload: serde_json::Value) -> TraceEvent {
    TraceEvent {
        seq,
        mark: None,
        run: Some("run-1".into()),
        turn: None,
        kind: Some(kind.into()),
        raw: json!({ "kind": kind, "run": "run-1", "wall_ms": WALL_MS + seq as i64, "payload": payload }),
    }
}

fn position_in(generation: &str, offset: u64) -> Position {
    Position {
        generation: generation.into(),
        offset,
        digest: format!("{generation}-{offset}"),
    }
}

/// A registered agent with its two minted credentials.
struct Registered {
    id: AgentId,
    name: String,
    r#box: String,
    gate: ClientCredential,
    admin: ClientCredential,
}

impl Registered {
    fn credential(&self, plane: Plane) -> &ClientCredential {
        match plane {
            Plane::Gate => &self.gate,
            Plane::Admin => &self.admin,
        }
    }
}

/// A fake connector: one TLS connection speaking the link's lines.
struct Fake {
    reader: BufReader<ReadHalf<TlsStream<TcpStream>>>,
    writer: WriteHalf<TlsStream<TcpStream>>,
}

impl Fake {
    async fn try_connect(
        address: SocketAddr,
        authority_pem: &str,
        credential: &ClientCredential,
    ) -> anyhow::Result<Self> {
        let tls = client_tls(
            authority_pem,
            &credential.certificate_pem,
            &credential.key_pem,
        )?;
        let tcp = TcpStream::connect(address).await?;
        let name = rustls::pki_types::ServerName::try_from("weaver-web".to_string())?;
        let stream = TlsConnector::from(tls).connect(name, tcp).await?;
        let (read, writer) = tokio::io::split(stream);
        Ok(Self {
            reader: BufReader::new(read),
            writer,
        })
    }

    async fn connect(
        address: SocketAddr,
        authority_pem: &str,
        credential: &ClientCredential,
    ) -> Self {
        Self::try_connect(address, authority_pem, credential)
            .await
            .expect("the handshake completes against the authority that minted the credential")
    }

    async fn send(&mut self, frame: FromClient) {
        let mut line = serde_json::to_string(&frame).unwrap();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.unwrap();
    }

    /// A send that may meet a connection the server already closed.
    async fn try_send(&mut self, frame: FromClient) {
        let mut line = serde_json::to_string(&frame).unwrap();
        line.push('\n');
        let _ = self.writer.write_all(line.as_bytes()).await;
    }

    /// The next frame, or `None` where the server closed the connection or
    /// sent nothing within the bound.
    async fn recv(&mut self) -> Option<ToClient> {
        let mut line = String::new();
        match tokio::time::timeout(SOON, self.reader.read_line(&mut line)).await {
            Ok(Ok(0)) | Err(_) => None,
            Ok(Ok(_)) => Some(serde_json::from_str(line.trim_end()).expect("a frame")),
            Ok(Err(_)) => None,
        }
    }

    /// Whether the server closed the connection: end of stream or an
    /// error within the bound, as against a connection merely silent.
    async fn closed(&mut self) -> bool {
        let mut line = String::new();
        match tokio::time::timeout(SOON, self.reader.read_line(&mut line)).await {
            Ok(Ok(0)) | Ok(Err(_)) => true,
            Ok(Ok(_)) => false,
            Err(_) => false,
        }
    }

    async fn expect_refusal(&mut self, reason: Refusal) {
        match self.recv().await {
            Some(ToClient::Refusal { reason: got }) => assert_eq!(got, reason),
            other => panic!("expected the refusal {reason}, got {other:?}"),
        }
        assert!(
            self.recv().await.is_none(),
            "the connection closes after a refusal"
        );
    }
}

struct Lab {
    _dir: tempfile::TempDir,
    authority: Authority,
    store: Store,
    listener: Listener,
    silence: Duration,
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Lab {
    /// The store's lock is released before the next test's listener
    /// starts, which the serial guard alone would not order.
    fn drop(&mut self) {
        self.listener.stop_now();
    }
}

impl Lab {
    async fn open() -> Option<Self> {
        Self::open_with(SILENCE).await
    }

    async fn open_with(silence: Duration) -> Option<Self> {
        let serial = serial().lock().await;
        let store = store().await?;
        let dir = tempfile::tempdir().unwrap();
        let authority = Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap();
        let listener = Listener::start(store.clone(), &authority, "127.0.0.1:0", silence)
            .await
            .unwrap();
        Some(Self {
            _dir: dir,
            authority,
            store,
            listener,
            silence,
            _serial: serial,
        })
    }

    /// A new server process over the same store and authority.
    async fn restart(&mut self) {
        // A dead process: nothing torn down, the lock released with the
        // session, which is what the startup reset is for.
        self.listener.stop_now();
        self.listener =
            Listener::start(self.store.clone(), &self.authority, "127.0.0.1:0", SILENCE)
                .await
                .unwrap();
    }

    async fn register(&self, name: &str) -> Registered {
        let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
        self.register_at(&r#box, name).await
    }

    async fn register_at(&self, r#box: &str, name: &str) -> Registered {
        let gate = self.authority.mint_client(name, Plane::Gate).unwrap();
        let admin = self.authority.mint_client(name, Plane::Admin).unwrap();
        let (id, _) = self
            .store
            .register_agent(
                r#box,
                name,
                Some("lab"),
                &gate.fingerprint,
                &admin.fingerprint,
                &self.authority.fingerprint(),
            )
            .await
            .unwrap();
        Registered {
            id,
            name: name.into(),
            r#box: r#box.into(),
            gate,
            admin,
        }
    }

    async fn agent(&self, id: &AgentId) -> Agent {
        self.store.agent(id).await.unwrap().expect("the row stands")
    }

    /// Poll the row until a condition holds, or fail with the row.
    async fn wait_for(&self, id: &AgentId, what: &str, cond: impl Fn(&Agent) -> bool) -> Agent {
        let until = tokio::time::Instant::now() + SOON;
        loop {
            let agent = self.agent(id).await;
            if cond(&agent) {
                return agent;
            }
            assert!(
                tokio::time::Instant::now() < until,
                "the row never read {what}: {agent:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn connect(&self, credential: &ClientCredential) -> Fake {
        Fake::connect(
            self.listener.address(),
            self.authority.certificate_pem(),
            credential,
        )
        .await
    }

    /// Connect, say hello with a tail at 100, take the hello answer and,
    /// on the admin plane, the `show` ask that follows it, then say the
    /// replay is caught up (nothing to replay).
    async fn admit(&self, agent: &Registered, plane: Plane) -> Fake {
        self.admit_with(agent, plane, true).await
    }

    /// As `admit`, but the replay is left open for the test to send
    /// replayed events before it says caught up.
    async fn admit_replaying(&self, agent: &Registered, plane: Plane) -> Fake {
        self.admit_with(agent, plane, false).await
    }

    async fn admit_with(&self, agent: &Registered, plane: Plane, caught_up: bool) -> Fake {
        let mut fake = self.connect(agent.credential(plane)).await;
        fake.send(FromClient::Hello {
            agent: agent.name.clone(),
            plane,
            tail: Some(position(100)),
        })
        .await;
        match fake.recv().await {
            Some(ToClient::HelloAnswer { cadence_secs, .. }) => {
                assert_eq!(cadence_secs, self.silence.as_secs() / 4)
            }
            other => panic!("expected the hello answer, got {other:?}"),
        }
        if plane == Plane::Admin {
            match fake.recv().await {
                Some(ToClient::Verb { verb, .. }) if verb == "show" => {}
                other => panic!("expected the show ask after the hello answer, got {other:?}"),
            }
            if caught_up {
                fake.send(FromClient::CaughtUp).await;
            }
        }
        self.wait_for(&agent.id, "connected", |a| a.credential(plane).connected)
            .await;
        fake
    }
}

/// **A connection whose credential is not live is refused before its
/// roster is read.** The fake sends no hello at all and is still refused,
/// which is what "before" means; a credential the authority minted but the
/// register never held is refused the same way.
///
/// Perturbation: in `serve_connection`, read the hello before the lookup
/// (move the `agent_by_fingerprint` match below the hello's parse). The
/// fake that sends no hello then hangs to the silence bound instead of
/// being refused, and the revoked fake's roster is read first.
///
/// conforms: web-link-refuses-a-credential-not-live-before-the-roster
#[tokio::test]
async fn a_credential_not_live_is_refused_before_its_roster_is_read() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let row = lab.agent(&karl.id).await;
    lab.store
        .revoke_credential(&row, Plane::Gate, Some("lab"))
        .await
        .unwrap();

    let mut revoked = lab.connect(&karl.gate).await;
    revoked.expect_refusal(Refusal::NotLive).await;

    let stranger = lab.authority.mint_client("nobody", Plane::Admin).unwrap();
    let mut unknown = lab.connect(&stranger).await;
    unknown.expect_refusal(Refusal::NotLive).await;

    let row = lab.agent(&karl.id).await;
    assert_eq!(row.gate.state, CredentialState::Revoked);
    assert!(!row.gate.connected && !row.admin.connected);
}

/// **One live connection per credential, and the revoking act closes the
/// live one.** A second connection on a connected credential is refused
/// rather than replacing the first, which stays installed under its
/// incarnation; a revocation while connected closes the connection at
/// once and the row reads disconnected; an admission against a revoked
/// credential refuses under the lock without installing; and a stale
/// teardown writes nothing.
///
/// Perturbations, one per clause of the Spec's row: (1) in the install
/// closure, replace an existing entry instead of refusing, and the second
/// fake is admitted while the row reads connected throughout; (2) in
/// `Store::admit`, drop the state recheck under the lock, and an admission
/// on a revoked credential installs; (3) in `Store::teardown`, drop the
/// incarnation comparison, and the stale teardown marks the plane missing
/// while its replacement relays. The uninstall closure itself runs under
/// the lock on every outcome and decides by incarnation in the listener.
///
/// conforms: web-one-live-connection-per-credential
#[tokio::test]
async fn one_live_connection_per_credential() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;

    let mut first = lab.admit(&karl, Plane::Gate).await;
    let before = lab.agent(&karl.id).await;
    let incarnation = before.gate.incarnation.expect("the install named itself");

    let mut second = lab.connect(&karl.gate).await;
    second
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Gate,
            tail: None,
        })
        .await;
    second.expect_refusal(Refusal::AlreadyConnected).await;
    let after = lab.agent(&karl.id).await;
    assert!(after.gate.connected);
    assert_eq!(
        after.gate.incarnation,
        Some(incarnation),
        "the first stays installed"
    );
    assert!(lab.listener.connected(&karl.id, Plane::Gate));

    // The revoking act closes the live connection at once.
    lab.store
        .revoke_credential(&after, Plane::Gate, Some("lab"))
        .await
        .unwrap();
    assert!(
        first.recv().await.is_none(),
        "the revoked connection is closed"
    );
    let row = lab
        .wait_for(&karl.id, "gate disconnected after revocation", |a| {
            !a.gate.connected && !lab.listener.connected(&karl.id, Plane::Gate)
        })
        .await;
    assert_eq!(row.gate.state, CredentialState::Revoked);

    // An admission racing the revocation rechecks under the lock and
    // refuses without installing.
    let installed = std::cell::Cell::new(false);
    let admitted = lab
        .store
        .admit(
            &karl.id,
            Plane::Gate,
            &karl.gate.fingerprint,
            99,
            "127.0.0.1:1",
            || {
                installed.set(true);
                Ok(())
            },
        )
        .await
        .unwrap();
    assert_eq!(admitted, Err(Refusal::NotLive));
    assert!(
        !installed.get(),
        "nothing is installed on a revoked credential"
    );

    // A stale teardown is bound to its incarnation and writes nothing.
    let _admin = lab.admit(&karl, Plane::Admin).await;
    let live = lab.agent(&karl.id).await.admin.incarnation.unwrap();
    let uninstalled = std::cell::Cell::new(false);
    let landed = lab
        .store
        .teardown(&karl.id, Plane::Admin, live - 1, || uninstalled.set(true))
        .await
        .unwrap();
    assert!(!landed, "a stale teardown writes nothing");
    assert!(
        uninstalled.get(),
        "the uninstall runs under the lock on every outcome; the listener's own check keeps the live one"
    );
    let row = lab.agent(&karl.id).await;
    assert!(row.admin.connected, "the live plane stays connected");
    assert_eq!(row.admin.incarnation, Some(live));
}

/// **Identity is the certificate's binding and never the roster.** A hello
/// naming another agent, or the other plane, on a bound credential is
/// refused as a mismatch, and nothing is installed.
///
/// Perturbation: in `serve_connection`, act on the hello's name and plane
/// instead of comparing them to the binding (take `name` and `said_plane`
/// as the agent and plane). The hello on karl's gate credential naming m1
/// is believed and m1's row reads connected from a credential that is not
/// its own.
///
/// conforms: web-link-identity-is-the-certificates-binding-never-the-roster
#[tokio::test]
async fn identity_is_the_certificates_binding_never_the_roster() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let m1 = lab.register("m1").await;

    let mut other_name = lab.connect(&karl.gate).await;
    other_name
        .send(FromClient::Hello {
            agent: m1.name.clone(),
            plane: Plane::Gate,
            tail: None,
        })
        .await;
    other_name.expect_refusal(Refusal::RosterMismatch).await;

    let mut other_plane = lab.connect(&karl.gate).await;
    other_plane
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: None,
        })
        .await;
    other_plane.expect_refusal(Refusal::RosterMismatch).await;

    for id in [&karl.id, &m1.id] {
        let row = lab.agent(id).await;
        assert!(!row.gate.connected && !row.admin.connected, "{row:?}");
    }
}

/// **At most one row per box and name holds live credentials**, at the
/// schema, and re-registering a live pair retires the previous row. The
/// name is immutable at the schema too.
///
/// Perturbation: drop the partial unique index from migration 0010. The
/// direct insert of a second live row for one box and name lands, and two
/// rows hold live credentials for one agent.
///
/// conforms: web-one-live-row-per-box-and-name
#[tokio::test]
async fn at_most_one_live_row_per_box_and_name() {
    let Some(lab) = Lab::open().await else { return };
    let first = lab.register("karl").await;
    let second = lab.register_at(&first.r#box, "karl").await;
    assert_ne!(first.id, second.id);
    let retired = lab.agent(&first.id).await;
    assert_eq!(retired.gate.state, CredentialState::Revoked);
    assert_eq!(retired.admin.state, CredentialState::Revoked);
    let live = lab.agent(&second.id).await;
    assert_eq!(live.gate.state, CredentialState::Live);

    let smuggled = sqlx::query(
        "INSERT INTO agent (name, box, author, gate_fingerprint, admin_fingerprint, gate_authority, admin_authority) VALUES ($1, $2, NULL, $3, $4, $4, $4)",
    )
    .bind("karl")
    .bind(&first.r#box)
    .bind(fingerprint(b"one"))
    .bind(fingerprint(b"two"))
    .execute(&lab.store.pool)
    .await;
    let refusal = smuggled.unwrap_err().to_string();
    assert!(
        refusal.contains("agent_one_live_row_per_box_and_name"),
        "the index refuses a second live row: {refusal}"
    );

    let renamed = sqlx::query("UPDATE agent SET name = 'karl2' WHERE agent_id = $1")
        .bind(second.id.as_str())
        .execute(&lab.store.pool)
        .await;
    assert!(
        renamed.unwrap_err().to_string().contains("immutable"),
        "the name does not move"
    );
    sqlx::query("UPDATE agent SET box = $2 WHERE agent_id = $1")
        .bind(second.id.as_str())
        .bind(format!("{}-moved", first.r#box))
        .execute(&lab.store.pool)
        .await
        .expect("the box is the operator's to edit");
}

/// **The link state is reset when the listener starts**, and only for
/// planes recorded as connected: a plane already disconnected keeps its
/// date. The epoch advances by one in the same act.
///
/// Perturbation: in `Store::listener_start`, skip the two resets (keep the
/// epoch update). After the restart the row still reads the gate connected
/// though the socket belongs to a process that no longer listens, and a
/// surface would render the agent's gate present.
///
/// conforms: web-link-state-is-reset-when-the-listener-starts
#[tokio::test]
async fn the_link_state_is_reset_when_the_listener_starts() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let karl = lab.register("karl").await;

    // The admin plane connects and drops, so its disconnected date is a
    // real one the reset must leave alone.
    let admin = lab.admit(&karl, Plane::Admin).await;
    drop(admin);
    let dropped = lab
        .wait_for(&karl.id, "admin disconnected", |a| !a.admin.connected)
        .await;
    let admin_dropped_at = dropped.admin.link_at.expect("the drop is dated");

    let _gate = lab.admit(&karl, Plane::Gate).await;
    let before = lab.agent(&karl.id).await;
    assert!(before.gate.connected);
    let epoch_before = lab.listener.epoch();

    tokio::time::sleep(Duration::from_millis(30)).await;
    lab.restart().await;
    assert_eq!(lab.listener.epoch(), epoch_before + 1);

    let after = lab.agent(&karl.id).await;
    assert!(!after.gate.connected, "reset: {after:?}");
    assert!(after.gate.incarnation.is_none());
    assert!(after.gate.link_at.unwrap() > before.gate.link_at.unwrap());
    assert_eq!(
        after.admin.link_at,
        Some(admin_dropped_at),
        "an already-disconnected plane keeps its date"
    );
    assert!(!after.present());
}

/// **An agent is present only when both planes connect from its row.**
///
/// Perturbation: in `Agent::present`, answer on either plane alone
/// (`||`). An agent whose admin-con is down reads present, with a tuple
/// and a load state nobody has confirmed.
///
/// conforms: web-agent-present-only-when-both-planes-match-one-row
#[tokio::test]
async fn an_agent_is_present_only_when_both_planes_connect_from_its_row() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    assert!(!lab.agent(&karl.id).await.present());

    let _gate = lab.admit(&karl, Plane::Gate).await;
    assert!(
        !lab.agent(&karl.id).await.present(),
        "one plane is not presence"
    );

    let admin = lab.admit(&karl, Plane::Admin).await;
    assert!(lab.agent(&karl.id).await.present());

    drop(admin);
    let row = lab
        .wait_for(&karl.id, "admin gone", |a| !a.admin.connected)
        .await;
    assert!(!row.present());
    assert!(row.gate.connected, "the row says which plane is missing");
}

/// **The authority is loaded before the listener starts and never minted
/// at start**: a credential signed by another authority, which is what a
/// re-minted one would be to every installed connector, fails the
/// handshake, so nothing of its hello is read.
///
/// Perturbation: have `serve` mint an authority when none stands. Every
/// restart then mints another and every installed connector meets this
/// refusal.
///
/// conforms: web-servers-authority-is-loaded-and-never-minted-at-start
#[tokio::test]
async fn a_credential_of_another_authority_fails_the_handshake() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let other_dir = tempfile::tempdir().unwrap();
    let other = Authority::init(other_dir.path(), "weaver-web", &[]).unwrap();
    let foreign = other.mint_client(&karl.name, Plane::Gate).unwrap();

    // Under TLS 1.3 the client's handshake completes before the server
    // has verified its certificate, so the refusal arrives as the alert
    // that closes the connection on its first exchange: no frame answers.
    match Fake::try_connect(
        lab.listener.address(),
        lab.authority.certificate_pem(),
        &foreign,
    )
    .await
    {
        Err(_) => {}
        Ok(mut fake) => {
            fake.try_send(FromClient::Hello {
                agent: karl.name.clone(),
                plane: Plane::Gate,
                tail: None,
            })
            .await;
            assert!(
                fake.recv().await.is_none(),
                "a certificate the authority did not sign is refused below any frame"
            );
        }
    }
    assert!(
        Fake::try_connect(lab.listener.address(), other.certificate_pem(), &karl.gate)
            .await
            .is_err(),
        "a connector pinning another authority refuses this server at the handshake"
    );
    assert!(!lab.agent(&karl.id).await.gate.connected);
}

/// **The client credential is stored as a fingerprint and never the key**,
/// at the schema: the register holds SHA-256 over the certificate's DER,
/// and a key cannot be written in its place.
///
/// Perturbation: drop the two fingerprint checks from migration 0010. The
/// insert of a key lands, and a read of the register is a set of
/// credentials anyone can present.
///
/// conforms: web-client-credential-stored-as-fingerprint-never-key
#[tokio::test]
async fn the_client_credential_is_stored_as_a_fingerprint_and_never_the_key() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let row = lab.agent(&karl.id).await;
    let der = rustls_pemfile::certs(&mut karl.gate.certificate_pem.as_bytes())
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(row.gate.fingerprint, fingerprint(der.as_ref()));

    let smuggled = sqlx::query(
        "INSERT INTO agent (name, box, author, gate_fingerprint, admin_fingerprint, gate_authority, admin_authority) VALUES ('karl', $1, NULL, $2, $3, $3, $3)",
    )
    .bind(format!("{}-keyed", karl.r#box))
    .bind(&karl.gate.key_pem)
    .bind(fingerprint(b"two"))
    .execute(&lab.store.pool)
    .await;
    let refusal = smuggled.unwrap_err().to_string();
    assert!(
        refusal.contains("agent_gate_fingerprint_is_sha256_hex"),
        "the schema refuses a key: {refusal}"
    );

    let text: String =
        sqlx::query_scalar("SELECT to_jsonb(agent)::text FROM agent WHERE agent_id = $1")
            .bind(karl.id.as_str())
            .fetch_one(&lab.store.pool)
            .await
            .unwrap();
    assert!(
        !text.contains("PRIVATE KEY"),
        "no member of the row is a key"
    );
}

/// **The tuple is admin's word and never gate-con's.** The data plane's
/// attempt to write it is refused on the wrong plane; a `show` answer
/// lands; a replayed event never writes the row, whether the client flags
/// it or the server's boundary, the hello's tail, says so; a live load
/// event writes; of a `list` answer only the connection's own row lands;
/// and after a restart a backfilled load event still writes nothing.
///
/// Perturbations, each a clause of the Spec's row: (1) in
/// `serve_connection`, accept `Verb` and `Event` frames on the gate plane,
/// and the gate's answer sets the load state; (2) in `land_event`, write
/// the observation whatever `behind` says, and the replayed load event
/// after an unload reads loaded, including after the restart; (3) classify
/// by the client's flag instead of the boundary, and the event at 50
/// flagged live writes though it is behind the hello's tail; (4) in
/// `land_verb`, land the first summary of a `list` instead of the own
/// row's, and karl reads the other agent's state. The out-of-order answer,
/// the skipped drain and the overlapping verbs are admin-con's ordering
/// (Spec 7.2) and wait for act 4's real admin-con.
///
/// conforms: web-tuple-is-admins-word-and-never-gate-cons
#[tokio::test]
async fn the_tuple_is_admins_word_and_never_gate_cons() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let karl = lab.register("karl").await;
    let other = lab.register("other").await;

    // (1) The data plane cannot carry it.
    let mut gate = lab.admit(&karl, Plane::Gate).await;
    gate.send(FromClient::Verb {
        id: 7,
        outcome: Some(show_answer("karl", "idle", None)),
        error: None,
    })
    .await;
    gate.expect_refusal(Refusal::WrongPlane).await;
    let mut gate = lab.admit(&karl, Plane::Gate).await;
    gate.send(FromClient::Event {
        position: position(100),
        replayed: false,
        event: trace_event(1, "load", json!({"declaration": "sha-1"})),
    })
    .await;
    gate.expect_refusal(Refusal::WrongPlane).await;
    assert!(lab.agent(&karl.id).await.load_state.is_none());

    // The admin plane's show answer is admin's word.
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Verb {
            id: 1,
            outcome: Some(show_answer("karl", "unloaded", None)),
            error: None,
        })
        .await;
    let row = lab
        .wait_for(&karl.id, "unloaded", |a| {
            a.load_state.as_deref() == Some("unloaded")
        })
        .await;
    assert!(row.tuple.is_none());

    // (2) A replayed event feeds the window and writes nothing.
    admin
        .send(FromClient::Event {
            position: position(50),
            replayed: true,
            event: trace_event(2, "load", json!({"declaration": "sha-old"})),
        })
        .await;
    match admin.recv().await {
        Some(ToClient::Ack { position: p }) => assert_eq!(p, position(50)),
        other => panic!("expected an ack, got {other:?}"),
    }
    assert_eq!(
        lab.listener
            .windows()
            .snapshot(karl.id.as_str())
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        lab.agent(&karl.id).await.load_state.as_deref(),
        Some("unloaded")
    );

    // (3) Before caught_up an event is the replay's whatever the client
    // flags, and the boundary's offset rule is a check.
    admin
        .send(FromClient::Event {
            position: position(60),
            replayed: false,
            event: trace_event(3, "load", json!({"declaration": "sha-lied"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    assert_eq!(
        lab.agent(&karl.id).await.load_state.as_deref(),
        Some("unloaded")
    );

    // The replay reaches the boundary; a live load event at the boundary
    // writes, and its payload is the tuple the trace carries.
    admin.send(FromClient::CaughtUp).await;
    admin
        .send(FromClient::Event {
            position: position(100),
            replayed: false,
            event: trace_event(4, "load", json!({"declaration": "sha-new"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    let row = lab
        .wait_for(&karl.id, "idle", |a| {
            a.load_state.as_deref() == Some("idle")
        })
        .await;
    assert_eq!(row.tuple, Some(json!({"declaration": "sha-new"})));

    // (4) Of a list answer only the own row lands.
    admin
        .send(FromClient::Verb {
            id: 2,
            outcome: Some(list_answer(&[("other", "active"), ("karl", "unloaded")])),
            error: None,
        })
        .await;
    lab.wait_for(&karl.id, "unloaded by list", |a| {
        a.load_state.as_deref() == Some("unloaded")
    })
    .await;
    assert!(
        lab.agent(&other.id).await.load_state.is_none(),
        "other's row is untouched"
    );

    // After a restart, a backfilled load event behind the new boundary
    // still writes nothing, so the row keeps the newer unload.
    drop(admin);
    lab.restart().await;
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position(90),
            replayed: true,
            event: trace_event(5, "load", json!({"declaration": "sha-backfill"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        lab.agent(&karl.id).await.load_state.as_deref(),
        Some("unloaded")
    );
}

/// **Nothing crosses the link in the clear.** A plaintext hello meets no
/// frame reader: the handshake fails, the connection closes, and the row
/// is untouched. The review half of the row is the listener's one accept
/// path, `TlsAcceptor::accept` in `serve_connection`.
///
/// Perturbation: accept the TCP stream as a frame stream when the
/// handshake fails. The plaintext hello is then read as a roster.
///
/// conforms: web-nothing-crosses-the-link-in-the-clear
#[tokio::test]
async fn nothing_crosses_the_link_in_the_clear() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut plain = TcpStream::connect(lab.listener.address()).await.unwrap();
    let hello = serde_json::to_string(&FromClient::Hello {
        agent: karl.name.clone(),
        plane: Plane::Gate,
        tail: None,
    })
    .unwrap();
    plain
        .write_all(format!("{hello}\n").as_bytes())
        .await
        .unwrap();
    let mut answer = Vec::new();
    let read = tokio::time::timeout(
        SOON,
        tokio::io::AsyncReadExt::read_to_end(&mut plain, &mut answer),
    )
    .await;
    assert!(
        read.is_ok(),
        "the plaintext connection is closed rather than held"
    );
    // What comes back, if anything, is the handshake's alert and never a
    // frame: a TLS record (content type 21, an alert) that no line parser
    // reads as JSON.
    let as_frame = std::str::from_utf8(&answer)
        .ok()
        .and_then(|s| serde_json::from_str::<ToClient>(s.trim()).ok());
    assert!(
        as_frame.is_none(),
        "no frame answers a plaintext hello: {answer:?}"
    );
    assert!(
        answer.is_empty() || answer[0] == 21,
        "only a TLS alert may answer: {answer:?}"
    );
    let row = lab.agent(&karl.id).await;
    assert!(!row.gate.connected && !row.admin.connected);
}

/// The acknowledged position lives for the life of the server process:
/// a reconnection within it is answered with the last landed position,
/// and a restarted server answers none (Spec 7.2). Owed to act 4 as the
/// replay row's instrument; what stands here is the server's half.
#[tokio::test]
async fn the_acknowledged_position_is_per_process() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position(140),
            replayed: false,
            event: trace_event(1, "turn", json!({})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    assert_eq!(lab.listener.acknowledged(&karl.id), Some(position(140)));
    drop(admin);
    lab.wait_for(&karl.id, "admin gone", |a| !a.admin.connected)
        .await;

    let mut again = lab.connect(&karl.admin).await;
    again
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: Some(position(200)),
        })
        .await;
    match again.recv().await {
        Some(ToClient::HelloAnswer { acknowledged, .. }) => {
            assert_eq!(acknowledged, Some(position(140)))
        }
        other => panic!("expected the hello answer, got {other:?}"),
    }
    drop(again);
    lab.wait_for(&karl.id, "admin gone", |a| !a.admin.connected)
        .await;

    lab.restart().await;
    let mut fresh = lab.connect(&karl.admin).await;
    fresh
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: Some(position(200)),
        })
        .await;
    match fresh.recv().await {
        Some(ToClient::HelloAnswer { acknowledged, .. }) => assert_eq!(acknowledged, None),
        other => panic!("expected the hello answer, got {other:?}"),
    }
}

/// Observations order on the listener's arrival sequence and a later
/// epoch orders above an earlier one (Spec 2.12): a `show` answer after a
/// live event supersedes it, and a restarted server's first answer
/// supersedes whatever the last process left.
#[tokio::test]
async fn observations_order_on_the_arrival_sequence_and_the_epoch() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position(100),
            replayed: false,
            event: trace_event(1, "load", json!({"declaration": "sha-1"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    lab.wait_for(&karl.id, "idle", |a| {
        a.load_state.as_deref() == Some("idle")
    })
    .await;
    admin
        .send(FromClient::Verb {
            id: 1,
            outcome: Some(show_answer("karl", "unloaded", None)),
            error: None,
        })
        .await;
    lab.wait_for(&karl.id, "unloaded", |a| {
        a.load_state.as_deref() == Some("unloaded")
    })
    .await;

    drop(admin);
    lab.restart().await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Verb {
            id: 1,
            outcome: Some(show_answer(
                "karl",
                "active",
                Some(json!({"declaration": "sha-2"})),
            )),
            error: None,
        })
        .await;
    let row = lab
        .wait_for(&karl.id, "active", |a| {
            a.load_state.as_deref() == Some("active")
        })
        .await;
    assert_eq!(row.tuple, Some(json!({"declaration": "sha-2"})));
}

/// The server's asks reach the connector that holds the plane: a turn to
/// gate-con, a verb to admin-con, and an agent with neither connected is
/// answered as not connected.
#[tokio::test]
async fn asks_are_routed_to_the_plane_that_holds_them() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    assert!(lab.listener.turn(&karl.id, "hello").await.is_err());

    let mut gate = lab.admit(&karl, Plane::Gate).await;
    let listener = lab.listener.clone();
    let id = karl.id.clone();
    let asked = tokio::spawn(async move { listener.turn(&id, "what is the time").await });
    match gate.recv().await {
        Some(ToClient::Turn { id, text }) => {
            assert_eq!(text, "what is the time");
            gate.send(FromClient::Turn {
                id,
                close: Some(crate::adapters::gate::GateClose {
                    kind: "answered".into(),
                    run: Some("run-1".into()),
                    turn: Some("t-1".into()),
                    text: Some("noon".into()),
                    raw: json!({"kind": "answered"}),
                }),
                error: None,
            })
            .await;
        }
        other => panic!("expected the turn ask, got {other:?}"),
    }
    let close = asked.await.unwrap().unwrap();
    assert_eq!(close.text.as_deref(), Some("noon"));
}

/// **Rotation closes both live connections in the rotating act** (Spec 8:
/// both planes drop until the install script carries the new config, and
/// no interleaving leaves a revoked credential relaying), and the new pair
/// is admitted while the old one is refused.
///
/// Perturbation: in `close_fingerprint`, resolve the notified fingerprint
/// through the register instead of the live map. The rotation has already
/// replaced the row's fingerprints, nothing is found to close, both old
/// connections stay installed and relaying, and the new credential is
/// refused as already connected.
///
/// conforms: web-one-live-connection-per-credential
#[tokio::test]
async fn rotation_closes_both_live_connections_and_admits_the_new_pair() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut gate = lab.admit(&karl, Plane::Gate).await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    assert!(lab.agent(&karl.id).await.present());

    // A connection on the old credential that passed the handshake's
    // lookup before the rotation and says hello after it.
    let mut raced = lab.connect(&karl.gate).await;

    let row = lab.agent(&karl.id).await;
    let new_gate = lab.authority.mint_client("karl", Plane::Gate).unwrap();
    let new_admin = lab.authority.mint_client("karl", Plane::Admin).unwrap();
    lab.store
        .rotate_credentials(
            &row,
            Some("lab"),
            &new_gate.fingerprint,
            &new_admin.fingerprint,
            &lab.authority.fingerprint(),
        )
        .await
        .unwrap();

    assert!(
        gate.recv().await.is_none(),
        "the old gate connection is closed"
    );
    assert!(
        admin.recv().await.is_none(),
        "the old admin connection is closed"
    );
    let row = lab
        .wait_for(&karl.id, "both planes down and uninstalled", |a| {
            !a.gate.connected
                && !a.admin.connected
                && !lab.listener.connected(&karl.id, Plane::Gate)
                && !lab.listener.connected(&karl.id, Plane::Admin)
        })
        .await;
    assert_eq!(row.gate.fingerprint, new_gate.fingerprint);
    assert!(row.gate.incarnation.is_none() && row.admin.incarnation.is_none());
    // The window is marked for the admin link lost to the rotation, though
    // the row's write answered nothing, the incarnation having been cleared
    // by the rotating act before the close.
    let marks: Vec<String> = lab
        .listener
        .windows()
        .snapshot(karl.id.as_str())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| e.mark)
        .collect();
    assert!(
        marks
            .iter()
            .any(|m| m.starts_with("link to admin-con lost")),
        "the window names the lost link: {marks:?}"
    );

    // The locked recheck compares the presented fingerprint, not only the
    // plane's state, which is live again for the new credential.
    raced
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Gate,
            tail: None,
        })
        .await;
    raced.expect_refusal(Refusal::NotLive).await;

    let mut old = lab.connect(&karl.gate).await;
    old.expect_refusal(Refusal::NotLive).await;

    let rotated = Registered {
        id: karl.id.clone(),
        name: karl.name.clone(),
        r#box: karl.r#box.clone(),
        gate: new_gate,
        admin: new_admin,
    };
    let _gate = lab.admit(&rotated, Plane::Gate).await;
    let _admin = lab.admit(&rotated, Plane::Admin).await;
    assert!(lab.agent(&karl.id).await.present());
}

/// An admin hello with no tail fixes no boundary and is refused; a line
/// past the bound is refused as malformed rather than read whole; a second
/// hello mid-stream is refused.
#[tokio::test]
async fn a_tailless_admin_hello_and_a_line_past_the_bound_are_malformed() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;

    let mut tailless = lab.connect(&karl.admin).await;
    tailless
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: None,
        })
        .await;
    tailless.expect_refusal(Refusal::Malformed).await;
    assert!(!lab.agent(&karl.id).await.admin.connected);

    let mut long = lab.admit(&karl, Plane::Gate).await;
    let line = vec![b'a'; super::frames::LINE_BOUND + 64];
    let _ = long.writer.write_all(&line).await;
    long.expect_refusal(Refusal::Malformed).await;
    lab.wait_for(&karl.id, "gate down", |a| !a.gate.connected)
        .await;

    let mut twice = lab.admit(&karl, Plane::Gate).await;
    twice
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Gate,
            tail: None,
        })
        .await;
    twice.expect_refusal(Refusal::Malformed).await;
}

/// **Events of another generation classify by the stream's order and not by
/// the client's flag** (Spec 7.2): before the first event at or beyond the
/// boundary they are the old file's tail, replayed; after it they are a
/// rotation after the hello, live.
///
/// Perturbation: in `land_event`, take the client's flag for another
/// generation. The old generation's load, flagged live, writes the row.
///
/// conforms: web-tuple-is-admins-word-and-never-gate-cons
#[tokio::test]
async fn events_of_another_generation_classify_by_the_streams_order() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Verb {
            id: 1,
            outcome: Some(show_answer("karl", "unloaded", None)),
            error: None,
        })
        .await;
    lab.wait_for(&karl.id, "unloaded", |a| {
        a.load_state.as_deref() == Some("unloaded")
    })
    .await;

    // The old generation's tail, before caught_up: replayed whatever the
    // client says.
    admin
        .send(FromClient::Event {
            position: position_in("g0", 900),
            replayed: false,
            event: trace_event(1, "load", json!({"declaration": "sha-old"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        lab.agent(&karl.id).await.load_state.as_deref(),
        Some("unloaded")
    );

    // The replay reaches the boundary; a rotation after the hello is live,
    // whatever the client says.
    admin.send(FromClient::CaughtUp).await;
    admin
        .send(FromClient::Event {
            position: position_in("g2", 0),
            replayed: true,
            event: trace_event(3, "load", json!({"declaration": "sha-rotated"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    let row = lab
        .wait_for(&karl.id, "idle from the rotated file", |a| {
            a.load_state.as_deref() == Some("idle")
        })
        .await;
    assert_eq!(row.tuple, Some(json!({"declaration": "sha-rotated"})));
    drop(admin);
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;

    // A file rotated after the hello before any event of the boundary's
    // generation reached the boundary: caught_up at once, and the new
    // generation's first event is live.
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position_in("g3", 0),
            replayed: false,
            event: trace_event(4, "unload", json!({})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    lab.wait_for(&karl.id, "unloaded from the rotated file", |a| {
        a.load_state.as_deref() == Some("unloaded")
    })
    .await;
    drop(admin);
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;

    // An event at or beyond the boundary in its generation before
    // caught_up is a protocol fault.
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position(100),
            replayed: false,
            event: trace_event(5, "turn", json!({})),
        })
        .await;
    admin.expect_refusal(Refusal::Malformed).await;
}

/// **A load or unload event carries admin's date**, the trace's own
/// `wall_ms`, and not the receipt's (Spec 2.12).
#[tokio::test]
async fn a_load_events_date_is_the_traces_own() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position(100),
            replayed: false,
            event: trace_event(7, "load", json!({"declaration": "sha-1"})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    let row = lab
        .wait_for(&karl.id, "idle", |a| {
            a.load_state.as_deref() == Some("idle")
        })
        .await;
    assert_eq!(
        row.load_state_at.unwrap().timestamp_millis(),
        WALL_MS + 7,
        "the date is the event's and not the receipt's"
    );
    assert_eq!(row.tuple_at, row.load_state_at);
}

/// **A revocation whose notification was lost is found by the sweep**: the
/// register is the word, the channel only the prompt.
#[tokio::test]
async fn a_lost_revocation_is_found_by_the_sweep() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut gate = lab.admit(&karl, Plane::Gate).await;
    // Revoked behind the listener's back, with no notification raised.
    sqlx::query(
        "UPDATE agent SET gate_state = 'revoked', gate_state_at = now(), \
         gate_connected = false, gate_incarnation = NULL WHERE agent_id = $1",
    )
    .bind(karl.id.as_str())
    .execute(&lab.store.pool)
    .await
    .unwrap();
    assert!(
        lab.listener.connected(&karl.id, Plane::Gate),
        "still installed"
    );
    lab.listener.sweep_revoked().await;
    assert!(gate.recv().await.is_none(), "the sweep closed it");
    lab.wait_for(&karl.id, "uninstalled", |_| {
        !lab.listener.connected(&karl.id, Plane::Gate)
    })
    .await;
}

/// **An event is acknowledged only once the register took what it owed**
/// (Spec 7.2): a store write that fails leaves the position unadvanced and
/// no ack sent, and the connector's resend from its last acknowledged
/// position lands it.
///
/// Perturbation: send the ack and advance the position whatever
/// `land_event` answered. The failed observation is acknowledged, the row
/// never reads it, and a reconnection in this process resumes past it.
///
/// conforms: web-tuple-is-admins-word-and-never-gate-cons
#[tokio::test]
async fn an_observation_the_register_never_took_is_not_acknowledged() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;

    // A success first, so the last acknowledged position is a real one.
    admin
        .send(FromClient::Event {
            position: position(100),
            replayed: false,
            event: trace_event(1, "turn", json!({})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));

    lab.listener.fail_next_land();
    admin
        .send(FromClient::Event {
            position: position(120),
            replayed: false,
            event: trace_event(2, "load", json!({"declaration": "sha-1"})),
        })
        .await;
    admin.expect_refusal(Refusal::StoreUnavailable).await;
    assert_eq!(lab.listener.acknowledged(&karl.id), Some(position(100)));
    assert!(lab.agent(&karl.id).await.load_state.is_none());
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;
    // The failed lifecycle event never entered the window.
    let loads = |events: &[TraceEvent]| {
        events
            .iter()
            .filter(|e| e.kind.as_deref() == Some("load"))
            .count()
    };
    assert_eq!(
        loads(&lab.listener.windows().snapshot(karl.id.as_str()).unwrap()),
        0
    );

    // The reconnection is answered with the pre-failure position and the
    // admission's show is asked again; the resend is acked and lands.
    let mut again = lab.connect(&karl.admin).await;
    again
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: Some(position(200)),
        })
        .await;
    match again.recv().await {
        Some(ToClient::HelloAnswer { acknowledged, .. }) => {
            assert_eq!(acknowledged, Some(position(100)))
        }
        other => panic!("expected the hello answer, got {other:?}"),
    }
    assert!(matches!(again.recv().await, Some(ToClient::Verb { .. })));
    again
        .send(FromClient::Event {
            position: position(120),
            replayed: true,
            event: trace_event(2, "load", json!({"declaration": "sha-1"})),
        })
        .await;
    match again.recv().await {
        Some(ToClient::Ack { position: p }) => assert_eq!(p, position(120)),
        other => panic!("expected the ack on the resend, got {other:?}"),
    }
    assert_eq!(lab.listener.acknowledged(&karl.id), Some(position(120)));
    // The resend is the one load in the window, not a second one.
    assert_eq!(
        loads(&lab.listener.windows().snapshot(karl.id.as_str()).unwrap()),
        1
    );
    // Replayed by the new boundary, it feeds the window and writes nothing;
    // the lifecycle fact reaches the row by the show the reconnection asked.
    again
        .send(FromClient::Verb {
            id: 0,
            outcome: Some(show_answer(
                "karl",
                "idle",
                Some(json!({"declaration": "sha-1"})),
            )),
            error: None,
        })
        .await;
    lab.wait_for(&karl.id, "idle", |a| {
        a.load_state.as_deref() == Some("idle")
    })
    .await;
}

/// **A verb answer the register could not land closes the connection and
/// fails the ask**: the pending ask answers the store failure and never
/// the outcome, and the admission's `show` is asked again on reconnection.
///
/// Perturbation: resolve the ask with the outcome whatever `land_verb`
/// answered. The caller reads a state the row never took.
///
/// conforms: web-tuple-is-admins-word-and-never-gate-cons
#[tokio::test]
async fn a_show_answer_the_register_never_took_closes_the_connection_and_fails_the_ask() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;

    let listener = lab.listener.clone();
    let id = karl.id.clone();
    let asked = tokio::spawn(async move { listener.verb(&id, "show").await });
    let ask = match admin.recv().await {
        Some(ToClient::Verb { id, verb }) if verb == "show" => id,
        other => panic!("expected the show ask, got {other:?}"),
    };
    lab.listener.fail_next_land();
    admin
        .send(FromClient::Verb {
            id: ask,
            outcome: Some(show_answer("karl", "active", None)),
            error: None,
        })
        .await;
    let refused = asked.await.unwrap().unwrap_err().to_string();
    assert!(refused.contains("the store could not land"), "{refused}");
    admin.expect_refusal(Refusal::StoreUnavailable).await;
    assert!(lab.agent(&karl.id).await.load_state.is_none());
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;

    let _again = lab.admit(&karl, Plane::Admin).await;
}

/// **One listener per store, held at the store** (Spec 8): a second
/// listener against the same database is refused while the first holds
/// the lock, and admitted once it has stopped.
///
/// Perturbation: skip the advisory lock in `Listener::start`. The second
/// listener starts, and its reset marks the first's connections
/// disconnected while they relay.
///
/// conforms: web-one-live-connection-per-credential
#[tokio::test]
async fn a_second_listener_against_one_store_is_refused() {
    let Some(lab) = Lab::open().await else { return };
    let refused = Listener::start(lab.store.clone(), &lab.authority, "127.0.0.1:0", SILENCE)
        .await
        .map(|_| ())
        .unwrap_err()
        .to_string();
    assert!(refused.contains("another weaver-web listener"), "{refused}");

    lab.listener.stop().await;
    let second = Listener::start(lab.store.clone(), &lab.authority, "127.0.0.1:0", SILENCE)
        .await
        .expect("admitted once the first released the store");
    second.stop().await;
}

fn lab_config(lab: &Lab) -> crate::config::ServerConfig {
    crate::config::ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: lab.listener.address().to_string(),
        database: String::new(),
        authority_dir: lab.authority.dir().to_owned(),
        silence_bound_secs: 60,
        link_address: None,
        server_name: "weaver-web".into(),
        admins: Vec::new(),
        agent_hop_budget: 8,
        providers: Vec::new(),
    }
}

/// **A minting verb runs under the authority lock and under the authority
/// on disk**: a registration waits while the lock is held, and refuses
/// where the authority was rotated since it was loaded, so no fingerprint
/// is committed under an authority a rotation retired.
///
/// Perturbation: drop the on-disk check from `register`. The registration
/// under the stale authority commits fingerprints that die at the next
/// restart.
///
/// conforms: web-servers-authority-is-loaded-and-never-minted-at-start
#[tokio::test]
async fn a_minting_verb_waits_for_the_authority_lock_and_refuses_a_rotated_authority() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());

    // Held by this test, as a concurrent rotation would hold it.
    let held = lab.store.authority_lock().await.unwrap();
    let store = lab.store.clone();
    let cfg2 = cfg.clone();
    let out2 = out.path().to_owned();
    let box2 = r#box.clone();
    let authority = Authority::load(lab.authority.dir()).unwrap();
    let registering = tokio::spawn(async move {
        super::verbs::register(&store, &cfg2, &authority, &box2, "karl", &out2, Some("lab")).await
    });
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        !registering.is_finished(),
        "register waits for the authority lock"
    );
    drop(held);
    let answer = registering.await.unwrap();
    assert!(answer.ok, "{}", answer.value);

    // Rotated on disk after the verb loaded its authority: refused.
    let stale = Authority::load(lab.authority.dir()).unwrap();
    Authority::rotate(lab.authority.dir(), "weaver-web", &[]).unwrap();
    let out3 = tempfile::tempdir().unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &stale,
        &r#box,
        "m1",
        out3.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok);
    let why = answer.value["error"].as_str().unwrap().to_owned();
    assert!(why.contains("rotated since this verb loaded it"), "{why}");
    assert!(
        lab.store
            .resolve_agent(&format!("{box}/m1", box = r#box))
            .await
            .is_err(),
        "no row was committed under the retired authority"
    );
}

/// **A second registration does not overwrite the first's configs**: the
/// configs live at `<out>/<box>/<name>/` and `register` refuses to write
/// over a file that stands, before it touches the store; `rotate` writes
/// the agent's own over.
#[tokio::test]
async fn a_second_registration_does_not_overwrite_the_firsts_configs() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let first = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(first.ok, "{}", first.value);
    let gate_path = out.path().join(&r#box).join("karl").join("gate-con.toml");
    assert!(gate_path.exists());
    let written = std::fs::read_to_string(&gate_path).unwrap();
    let first_id = first.value["agent"].as_str().unwrap().to_owned();

    let second = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!second.ok, "{}", second.value);
    assert!(
        second.value["error"]
            .as_str()
            .unwrap()
            .contains("already exists")
    );
    assert_eq!(
        std::fs::read_to_string(&gate_path).unwrap(),
        written,
        "untouched"
    );
    let live = lab
        .store
        .resolve_agent(&format!("{box}/karl", box = r#box))
        .await
        .unwrap();
    assert_eq!(
        live.agent_id.as_str(),
        first_id,
        "the first row still holds the live credentials"
    );

    // Two agents of colliding joined names do not share a directory.
    let other = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &format!("{box}-karl", box = r#box),
        "x",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(other.ok, "{}", other.value);
    assert_eq!(std::fs::read_to_string(&gate_path).unwrap(), written);

    let rotated = super::verbs::rotate(
        &lab.store,
        &cfg,
        &lab.authority,
        &format!("{box}/karl", box = r#box),
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(rotated.ok, "{}", rotated.value);
    assert_ne!(
        std::fs::read_to_string(&gate_path).unwrap(),
        written,
        "rotate writes the agent's own over"
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&gate_path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        std::fs::metadata(out.path().join(&r#box))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

/// **A teardown the store refused is reconciled**: the row still records
/// the incarnation as connected, so the disconnected write is retried
/// until it lands, and presence is not asserted for a socket that is gone.
#[tokio::test]
async fn a_teardown_the_store_refused_is_reconciled() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let gate = lab.admit(&karl, Plane::Gate).await;
    lab.listener.fail_next_teardown();
    drop(gate);
    let until = tokio::time::Instant::now() + SOON;
    while lab.listener.failed_teardowns() == 0 {
        assert!(
            tokio::time::Instant::now() < until,
            "the teardown was never deferred"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        !lab.listener.connected(&karl.id, Plane::Gate),
        "out of the live map"
    );
    lab.wait_for(&karl.id, "gate disconnected by the reconciliation", |a| {
        !a.gate.connected
    })
    .await;
    assert_eq!(lab.listener.failed_teardowns(), 0);
}

/// **The lock's session is monitored, and its loss halts the listener**:
/// the backend holding the advisory lock is ended from outside, the
/// listener stops accepting, closes every connection and reports why, and
/// another listener can then take the store.
#[tokio::test]
async fn the_listener_halts_when_its_lock_session_is_lost() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let mut gate = lab.admit(&karl, Plane::Gate).await;
    // Handshaken but yet to say hello: a connection task halt must end
    // too, before the next listener can be admitted.
    let mut mid_hello = lab.connect(&karl.admin).await;
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(lab.listener.lock_pid())
        .execute(&lab.store.pool)
        .await
        .unwrap();
    let why = tokio::time::timeout(Duration::from_secs(10), lab.listener.halted())
        .await
        .expect("the listener halts within a few cadences");
    assert!(why.contains("lock was lost"), "{why}");
    assert!(gate.closed().await, "every connection is closed");
    assert!(
        mid_hello.closed().await,
        "a connection still handshaking or waiting for its hello is dropped too"
    );
    lab.wait_for(&karl.id, "gate down", |a| !a.gate.connected)
        .await;
    let next = Listener::start(lab.store.clone(), &lab.authority, "127.0.0.1:0", SILENCE)
        .await
        .expect("the store is free for the next listener");
    next.stop().await;
}

/// **The config's server name must be the authority's**: a client config
/// carries the authority's persisted name, and a verb refuses a config
/// whose name differs, naming both.
#[tokio::test]
async fn register_refuses_a_config_whose_server_name_differs_from_the_authoritys() {
    let Some(lab) = Lab::open().await else { return };
    let mut cfg = lab_config(&lab);
    cfg.server_name = "moved".into();
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok);
    let why = answer.value["error"].as_str().unwrap();
    assert!(why.contains("moved") && why.contains("weaver-web"), "{why}");
    assert!(
        lab.store
            .resolve_agent(&format!("{box}/karl", box = r#box))
            .await
            .is_err()
    );

    let cfg = lab_config(&lab);
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    let written =
        std::fs::read_to_string(out.path().join(&r#box).join("karl").join("gate-con.toml"))
            .unwrap();
    assert!(
        written.contains("server_name = \"weaver-web\""),
        "the authority's name: {written}"
    );
    assert!(
        !out.path()
            .join(&r#box)
            .join("karl")
            .join("gate-con.toml.staging")
            .exists(),
        "nothing staged remains"
    );
}

/// **A box of `.` or `..` is refused by the verb and by the schema**, since
/// a box names a directory under the operator's output path.
#[tokio::test]
async fn a_box_that_names_a_parent_directory_is_refused() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    for bad in [".", "..", "foo/../../victim", "/absolute", "with space"] {
        let answer = super::verbs::register(
            &lab.store,
            &cfg,
            &lab.authority,
            bad,
            "karl",
            out.path(),
            Some("lab"),
        )
        .await;
        assert!(!answer.ok, "{bad}: {}", answer.value);
        assert!(
            answer.value["error"]
                .as_str()
                .unwrap()
                .contains("not a box")
        );
        let smuggled = sqlx::query(
            "INSERT INTO agent (name, box, author, gate_fingerprint, admin_fingerprint, gate_authority, admin_authority) VALUES ('karl', $1, NULL, $2, $3, $3, $3)",
        )
        .bind(bad)
        .bind(fingerprint(bad.as_bytes()))
        .bind(fingerprint(b"other"))
        .execute(&lab.store.pool)
        .await;
        assert!(
            smuggled
                .unwrap_err()
                .to_string()
                .contains("agent_box_is_well_formed"),
            "{bad}"
        );
    }
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        "box",
        "../name",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(
        !answer.ok
            && answer.value["error"]
                .as_str()
                .unwrap()
                .contains("not a box")
    );
    assert!(
        std::fs::read_dir(out.path()).unwrap().next().is_none(),
        "nothing written under the output path"
    );
    assert!(
        !out.path().parent().unwrap().join("victim").exists(),
        "nothing written beside it"
    );
}

/// **A store failure after staging leaves no config behind**: the files
/// are staged before the store commits and discarded where it refuses.
#[tokio::test]
async fn a_store_refusal_discards_the_staged_configs() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    // The schema refuses a box with a slash; the verb lets it through to
    // the store, which is the failure staged here.
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        "box/with/slash",
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    let leftovers: Vec<_> = walkdir(out.path())
        .into_iter()
        .filter(|p| p.extension().is_some())
        .collect();
    assert!(leftovers.is_empty(), "staged configs remain: {leftovers:?}");
}

fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walkdir(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}

/// **A commit's outcome is unknown until it is read back**: a store error
/// after the commit was applied does not discard the staged pair; the row
/// is read back, the pair published and the answer notes the lost answer.
#[tokio::test]
async fn a_commit_whose_answer_was_lost_is_read_back_and_published() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());

    super::register::FAIL_AFTER_COMMIT.with(|f| f.set(Some("register")));
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "karl",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    assert!(
        answer.value["note"].as_str().unwrap().contains("read back"),
        "{}",
        answer.value
    );
    let dir = out.path().join(&r#box).join("karl");
    assert!(dir.join("gate-con.toml").exists() && dir.join("admin-con.toml").exists());
    assert!(!dir.join("gate-con.toml.staging").exists());
    let live = lab
        .store
        .resolve_agent(&format!("{box}/karl", box = r#box))
        .await
        .unwrap();
    assert_eq!(
        live.agent_id.as_str(),
        answer.value["agent"].as_str().unwrap()
    );

    super::register::FAIL_AFTER_COMMIT.with(|f| f.set(Some("rotate")));
    let before = std::fs::read_to_string(dir.join("gate-con.toml")).unwrap();
    let answer = super::verbs::rotate(
        &lab.store,
        &cfg,
        &lab.authority,
        &format!("{box}/karl", box = r#box),
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    assert!(answer.value["note"].as_str().unwrap().contains("read back"));
    assert_ne!(
        std::fs::read_to_string(dir.join("gate-con.toml")).unwrap(),
        before
    );
    let rotated = lab.agent(&live.agent_id).await;
    assert_eq!(
        rotated.gate.fingerprint,
        answer.value["gate_fingerprint"].as_str().unwrap()
    );
}

/// **An admission whose commit's answer was lost is reconciled**: the row
/// was left connected under an incarnation with no socket, and the deferred
/// teardown writes it disconnected once the store answers.
#[tokio::test]
async fn an_admission_whose_answer_was_lost_is_reconciled() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    super::register::FAIL_AFTER_COMMIT.with(|f| f.set(Some("admit")));
    let mut fake = lab.connect(&karl.gate).await;
    fake.send(FromClient::Hello {
        agent: karl.name.clone(),
        plane: Plane::Gate,
        tail: None,
    })
    .await;
    assert!(
        fake.closed().await,
        "the admission that lost its answer closes"
    );
    assert!(!lab.listener.connected(&karl.id, Plane::Gate));
    lab.wait_for(&karl.id, "gate reconciled to disconnected", |a| {
        !a.gate.connected
    })
    .await;
    let _again = lab.admit(&karl, Plane::Gate).await;
}

/// **A credential signed by another authority is revoked by the rotation's
/// reconciliation**: a registration that raced the switch (staged here by
/// recording a stale signer) is found and revoked after the switch, and
/// the answer counts it.
#[tokio::test]
async fn a_credential_signed_by_another_authority_is_revoked_by_the_rotation() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let karl = lab.register("karl").await;
    let stale = lab.authority.mint_client("m1", Plane::Gate).unwrap();
    let stale_admin = lab.authority.mint_client("m1", Plane::Admin).unwrap();
    let (raced, _) = lab
        .store
        .register_agent(
            &karl.r#box,
            "m1",
            Some("lab"),
            &stale.fingerprint,
            &stale_admin.fingerprint,
            &fingerprint(b"an authority the switch retired"),
        )
        .await
        .unwrap();

    let answer = super::verbs::authority_rotate(&lab.store, &cfg, &[], Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    // The scratch database is shared with the other tests' rows, so the
    // count is at least this test's two; the rows themselves are checked.
    assert!(
        answer.value["agents_retired"].as_u64().unwrap() >= 2,
        "{}",
        answer.value
    );
    assert_eq!(
        answer.value["stranded_credentials_revoked"], 0,
        "{}",
        answer.value
    );
    assert_eq!(
        lab.agent(&karl.id).await.gate.state,
        CredentialState::Revoked
    );
    assert_eq!(
        lab.agent(&raced).await.admin.state,
        CredentialState::Revoked
    );

    // The race proper: a credential committed under the old signer after
    // the rotation's revocation but before its reconciliation.
    let new_authority = Authority::load(lab.authority.dir()).unwrap();
    let late = lab.authority.mint_client("m2", Plane::Gate).unwrap();
    let late_admin = lab.authority.mint_client("m2", Plane::Admin).unwrap();
    let (late_id, _) = lab
        .store
        .register_agent(
            &karl.r#box,
            "m2",
            Some("lab"),
            &late.fingerprint,
            &late_admin.fingerprint,
            &lab.authority.fingerprint(),
        )
        .await
        .unwrap();
    let stranded = Store::revoke_credentials_not_signed_by_on(
        &mut lab.store.pool.acquire().await.unwrap(),
        &new_authority.fingerprint(),
        Some("lab"),
    )
    .await
    .unwrap();
    assert_eq!(stranded, 2, "both planes of the late registration");
    let row = lab.agent(&late_id).await;
    assert_eq!(row.gate.state, CredentialState::Revoked);
    assert_eq!(row.admin.state, CredentialState::Revoked);
    assert_eq!(lab.agent(&raced).await.gate.state, CredentialState::Revoked);
}

/// **A peer that stops reading is torn down as silent**: a send that cannot
/// complete within the bound is the peer gone silent, refused and torn
/// down, rather than a connection task suspended forever.
#[tokio::test]
async fn a_peer_that_stops_reading_is_torn_down_as_silent() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let gate = lab.admit(&karl, Plane::Gate).await;
    // The fake keeps heartbeating, so the read-side silence bound never
    // fires, and never reads again, so the server's writes back up.
    let Fake { reader, mut writer } = gate;
    let heartbeats = tokio::spawn(async move {
        let _reader = reader;
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut line = serde_json::to_string(&FromClient::Heartbeat).unwrap();
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
    });
    // Enough asks to fill the socket and the write channel behind it.
    let text = "x".repeat(1 << 20);
    let asks: Vec<_> = (0..80)
        .map(|_| {
            let listener = lab.listener.clone();
            let id = karl.id.clone();
            let text = text.clone();
            tokio::spawn(async move { listener.turn(&id, &text).await })
        })
        .collect();
    let until = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if !lab.agent(&karl.id).await.gate.connected
            && !lab.listener.connected(&karl.id, Plane::Gate)
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the unread peer was never torn down"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    for ask in asks {
        assert!(
            ask.await.unwrap().is_err(),
            "every ask on the torn-down link fails"
        );
    }
    heartbeats.abort();
}

/// **The authority rotation reads its revocation's commit back**: an
/// error after the revocation was applied leaves no credential live, so
/// the rotation continues rather than stopping with the files untouched.
#[tokio::test]
async fn an_authority_rotation_reads_a_lost_revocation_answer_back() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let karl = lab.register("karl").await;
    let before = lab.authority.fingerprint();
    super::register::FAIL_AFTER_COMMIT.with(|f| f.set(Some("revoke_every")));
    let answer = super::verbs::authority_rotate(&lab.store, &cfg, &[], Some("lab")).await;
    assert!(answer.ok, "{}", answer.value);
    assert!(
        answer.value["agents_retired"].is_null(),
        "the count is unknown: {}",
        answer.value
    );
    assert_ne!(
        Authority::load(lab.authority.dir()).unwrap().fingerprint(),
        before
    );
    assert_eq!(
        lab.agent(&karl.id).await.gate.state,
        CredentialState::Revoked
    );
}
