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
use super::frames::{Principal, VerbFault, VerbOutcome};
use super::listener::{Listener, VerbError};
use super::register::{Agent, CredentialState};
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

pub(super) const SILENCE: Duration = Duration::from_secs(60);
pub(super) const SOON: Duration = Duration::from_secs(5);

/// The identity a directly staged config names where the test is about the
/// staging and not the row the config belongs to.
const PLACEHOLDER_ID: &str = "ag-0000000000000000";

pub(super) fn serial() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

pub(super) async fn store() -> Option<Store> {
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
pub(super) struct Registered {
    pub(super) id: AgentId,
    pub(super) name: String,
    pub(super) r#box: String,
    pub(super) gate: ClientCredential,
    pub(super) admin: ClientCredential,
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
pub(super) struct Fake {
    reader: BufReader<ReadHalf<TlsStream<TcpStream>>>,
    writer: WriteHalf<TlsStream<TcpStream>>,
}

impl Fake {
    pub(super) async fn try_connect(
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

    pub(super) async fn send(&mut self, frame: FromClient) {
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
    pub(super) async fn recv(&mut self) -> Option<ToClient> {
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

pub(super) struct Lab {
    pub(super) _dir: tempfile::TempDir,
    pub(super) authority: Authority,
    pub(super) store: Store,
    pub(super) listener: Listener,
    pub(super) silence: Duration,
    pub(super) _serial: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Lab {
    /// The store's lock is released before the next test's listener
    /// starts, which the serial guard alone would not order.
    fn drop(&mut self) {
        self.listener.stop_now();
    }
}

impl Lab {
    pub(super) async fn open() -> Option<Self> {
        Self::open_with(SILENCE).await
    }

    pub(super) async fn open_with(silence: Duration) -> Option<Self> {
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

    pub(super) async fn agent(&self, id: &AgentId) -> Agent {
        self.store.agent(id).await.unwrap().expect("the row stands")
    }

    /// Poll the row until a condition holds, or fail with the row.
    pub(super) async fn wait_for(
        &self,
        id: &AgentId,
        what: &str,
        cond: impl Fn(&Agent) -> bool,
    ) -> Agent {
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
            ceiling: (plane == Plane::Admin).then(|| vec!["show".to_owned()]),
            door: (plane == Plane::Admin).then_some(true),
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
            ceiling: None,
            door: None,
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
            None,
            None,
            async || Ok(()),
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
            ceiling: None,
            door: None,
        })
        .await;
    other_name.expect_refusal(Refusal::RosterMismatch).await;

    let mut other_plane = lab.connect(&karl.gate).await;
    other_plane
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: None,
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(true),
        })
        .await;
    other_plane.expect_refusal(Refusal::RosterMismatch).await;

    for id in [&karl.id, &m1.id] {
        let row = lab.agent(id).await;
        assert!(!row.gate.connected && !row.admin.connected, "{row:?}");
    }
}

/// **The trace door and its boundary agree, on the hello and on the door's
/// frame** (Spec 7.2): an open door's hello carries its boundary and a
/// closed door's carries none. An admin hello naming no door, an open door
/// with no boundary, or a closed one with a boundary is refused as
/// malformed, as are a gate hello naming a door and a door frame whose
/// state and boundary disagree. **A closed door's hello is admitted at
/// once**, its `show` asked, and the row reads the door closed.
#[tokio::test]
async fn the_trace_door_and_its_boundary_agree() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    for (door, tail) in [
        (None, Some(position(100))),
        (Some(true), None),
        (Some(false), Some(position(100))),
    ] {
        let mut fake = lab.connect(&karl.admin).await;
        fake.send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail,
            ceiling: Some(vec!["show".to_owned()]),
            door,
        })
        .await;
        fake.expect_refusal(Refusal::Malformed).await;
    }
    let mut gate = lab.connect(&karl.gate).await;
    gate.send(FromClient::Hello {
        agent: karl.name.clone(),
        plane: Plane::Gate,
        tail: None,
        ceiling: None,
        door: Some(true),
    })
    .await;
    gate.expect_refusal(Refusal::Malformed).await;

    let mut closed = lab.connect(&karl.admin).await;
    closed
        .send(FromClient::Hello {
            agent: karl.name.clone(),
            plane: Plane::Admin,
            tail: None,
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(false),
        })
        .await;
    match closed.recv().await {
        Some(ToClient::HelloAnswer { .. }) => {}
        other => panic!("expected the hello answer, got {other:?}"),
    }
    match closed.recv().await {
        Some(ToClient::Verb { verb, .. }) if verb == "show" => {}
        other => panic!("expected the show ask, got {other:?}"),
    }
    lab.wait_for(&karl.id, "the closed door admitted", |a| {
        a.admin.connected && a.trace_door == Some(false) && a.trace_door_at.is_some()
    })
    .await;
    closed
        .send(FromClient::Door {
            open: true,
            wall_ms: 1_790_000_000_000,
            tail: None,
        })
        .await;
    closed.expect_refusal(Refusal::Malformed).await;
}

/// **A door frame the row no longer takes closes the connection**: the
/// row names another admin connection (a rotation or revocation committed
/// whose notification has not yet closed this socket, staged here by moving
/// the row's incarnation), so the door's landing finds nothing to write.
/// The connection is refused `not_live` at once, and nothing it carries
/// after writes the row.
#[tokio::test]
async fn a_door_frame_the_row_no_longer_takes_closes_the_connection() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    sqlx::query("UPDATE agent SET admin_incarnation = admin_incarnation + 1 WHERE agent_id = $1")
        .bind(karl.id.as_str())
        .execute(&lab.store.pool)
        .await
        .unwrap();
    admin
        .send(FromClient::Door {
            open: false,
            wall_ms: 1_790_000_000_000,
            tail: None,
        })
        .await;
    // A live load right behind it: a connection the row retired must not
    // land it.
    admin
        .try_send(FromClient::Event {
            position: position(200),
            replayed: false,
            event: trace_event(1, "load", json!({"declaration": "sha-1"})),
        })
        .await;
    admin.expect_refusal(Refusal::NotLive).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let row = lab.agent(&karl.id).await;
    assert_eq!(row.trace_door, Some(true), "{row:?}");
    assert_eq!(row.load_state, None, "{row:?}");
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
                ceiling: None,
                door: None,
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
/// event writes; and after a restart a backfilled load event still writes
/// nothing.
///
/// Perturbations, each a clause of the Spec's row: (1) in
/// `serve_connection`, accept `Verb` and `Event` frames on the gate plane,
/// and the gate's answer sets the load state; (2) in `land_event`, write
/// the observation whatever `behind` says, and the replayed load event
/// after an unload reads loaded, including after the restart; (3) classify
/// by the client's flag instead of the boundary, and the event at 50
/// flagged live writes though it is behind the hello's tail. The
/// out-of-order answer, the skipped drain and the overlapping verbs are
/// admin-con's ordering (Spec 7.2), tested against the real admin-con in
/// `link::admin_con_tests`.
///
/// conforms: web-tuple-is-admins-word-and-never-gate-cons
#[tokio::test]
async fn the_tuple_is_admins_word_and_never_gate_cons() {
    let Some(mut lab) = Lab::open().await else {
        return;
    };
    let karl = lab.register("karl").await;

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
    // The server reports the admission's `show` landed.
    assert!(matches!(
        admin.recv().await,
        Some(ToClient::Landed { id: 1 })
    ));

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

    // After a restart, a backfilled load event behind the new boundary
    // still writes nothing, so the row keeps the live load's tuple.
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
        lab.agent(&karl.id).await.tuple,
        Some(json!({"declaration": "sha-new"}))
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
        ceiling: None,
        door: None,
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
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(true),
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
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(true),
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
                    finish: None,
                    reason: None,
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
            ceiling: None,
            door: None,
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

/// An open door's admin hello with no tail fixes no boundary and is refused; a line
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
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(true),
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
            ceiling: None,
            door: None,
        })
        .await;
    twice.expect_refusal(Refusal::Malformed).await;
}

/// **Events of another generation classify by the stream's order and not by
/// the client's flag** (Spec 7.2): before the `caught_up` frame they are
/// the old file's tail, replayed; after it they are a rotation after the
/// hello, live.
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
    assert!(matches!(
        admin.recv().await,
        Some(ToClient::Landed { id: 1 })
    ));

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

    // An event beyond the boundary in its generation before caught_up is a
    // protocol fault. A position is the byte after its record, so the event
    // ending at the boundary (100) is the last one behind it, and the first
    // past it ends beyond.
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    admin
        .send(FromClient::Event {
            position: position(100),
            replayed: true,
            event: trace_event(5, "turn", json!({})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    admin
        .send(FromClient::Event {
            position: position(101),
            replayed: false,
            event: trace_event(6, "turn", json!({})),
        })
        .await;
    admin.expect_refusal(Refusal::Malformed).await;

    // A mark is held to the same rule: it carries the position relaying
    // resumes at, never past the boundary before caught_up.
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    let mut mark = trace_event(7, "turn", json!({}));
    mark.mark = Some("a mark past the boundary".into());
    mark.kind = None;
    admin
        .send(FromClient::Event {
            position: position(101),
            replayed: true,
            event: mark,
        })
        .await;
    admin.expect_refusal(Refusal::Malformed).await;
}

/// **An acknowledgement never blocks the server's read** (Spec 7.2): an
/// admin-con that sends many events and reads nothing back fills the
/// server's write path with acks, and the server drops the acks that do not
/// fit rather than stop reading, so every event lands and the last one is
/// acknowledged. An ack that waited for room would stop the read, and both
/// ends would wait on each other to the silence bound.
#[tokio::test]
async fn an_ack_never_blocks_the_servers_read() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    let events = 20_000u64;
    let sent = tokio::time::timeout(Duration::from_secs(30), async {
        for n in 1..=events {
            admin
                .send(FromClient::Event {
                    position: position_in("g1", 100 + n),
                    replayed: false,
                    event: trace_event(n, "turn", json!({})),
                })
                .await;
        }
    })
    .await;
    assert!(sent.is_ok(), "the server stopped reading");
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    while lab.listener.acknowledged(&karl.id).map(|p| p.offset) != Some(100 + events) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the last: {:?}",
            lab.listener.acknowledged(&karl.id)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **The `landed` frame is delivered or the connection closes** (Spec 8):
/// an admin-con that stops reading fills the server's write path with acks
/// while the admission's `show` is outstanding, then answers it and keeps
/// its heartbeats coming. The `landed` cannot be delivered, and the server
/// closes the connection at the bound rather than dropping the frame and
/// leaving admin-con holding every ordinary verb on a live connection.
#[tokio::test]
async fn an_undeliverable_landed_closes_the_connection() {
    let Some(lab) = Lab::open_with(Duration::from_secs(8)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit(&karl, Plane::Admin).await;
    // Each ack names a long generation, so a few thousand outrun every
    // buffer between the server's queue and this reader.
    let generation = "g".repeat(4096);
    let events = 5_000u64;
    for n in 1..=events {
        admin
            .send(FromClient::Event {
                position: position_in(&generation, 100 + n),
                replayed: false,
                event: trace_event(n, "turn", json!({})),
            })
            .await;
    }
    // The landing is the store's and this test asserts the later close, so
    // the wait has the ingest sweep's headroom: a scratch database shared
    // with the ingest's heavy tests lands these more slowly.
    let until = tokio::time::Instant::now() + Duration::from_secs(60);
    while lab.listener.acknowledged(&karl.id).map(|p| p.offset) != Some(100 + events) {
        assert!(
            tokio::time::Instant::now() < until,
            "never acknowledged through the last: {:?}",
            lab.listener.acknowledged(&karl.id)
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    admin
        .send(FromClient::Verb {
            id: 1,
            outcome: Some(show_answer("karl", "idle", None)),
            error: None,
        })
        .await;
    lab.wait_for(&karl.id, "the show landed", |a| {
        a.state_source.as_deref() == Some("show")
    })
    .await;
    // Heartbeats keep the read side alive for three bounds; only the
    // undeliverable frame can close it.
    let until = tokio::time::Instant::now() + Duration::from_secs(24);
    while lab.listener.connected(&karl.id, Plane::Admin) {
        assert!(
            tokio::time::Instant::now() < until,
            "the connection stayed up with the landing undelivered"
        );
        admin.try_send(FromClient::Heartbeat).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
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

/// **A turn's start and close refresh the load state** (Spec 2.12): an
/// unclean stop writes no `unload`, so these keep a state from events no
/// older than the agent's last turn. Under the rules a load lands by: a
/// replayed `turn.started` writes nothing; a live one lands `active` with
/// the event's own date and source `event`, and `turn.closed` lands `idle`;
/// neither touches the tuple, which keeps the load's and its date.
///
/// Perturbation: drop the `turn.started` arm in `land_event`. The row never
/// reads `active` between the two events.
///
/// conforms: web-tuple-is-admins-word-and-never-gate-cons
#[tokio::test]
async fn a_turns_start_and_close_refresh_the_load_state() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let mut admin = lab.admit_replaying(&karl, Plane::Admin).await;
    let event = |position: Position, replayed: bool, event: TraceEvent| FromClient::Event {
        position,
        replayed,
        event,
    };
    // Behind the boundary: history, never the row.
    admin
        .send(event(
            position(50),
            true,
            trace_event(1, "turn.started", json!({})),
        ))
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(lab.agent(&karl.id).await.load_state, None);
    admin.send(FromClient::CaughtUp).await;

    admin
        .send(event(
            position(101),
            false,
            trace_event(2, "load", json!({"declaration": "sha-1"})),
        ))
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    let loaded = lab
        .wait_for(&karl.id, "idle from the load", |a| {
            a.load_state.as_deref() == Some("idle")
        })
        .await;

    admin
        .send(event(
            position(102),
            false,
            trace_event(3, "turn.started", json!({})),
        ))
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    let active = lab
        .wait_for(&karl.id, "active from the turn's start", |a| {
            a.load_state.as_deref() == Some("active")
        })
        .await;
    assert_eq!(
        active.load_state_at.unwrap().timestamp_millis(),
        WALL_MS + 3
    );
    assert_eq!(active.state_source.as_deref(), Some("event"));
    assert_eq!(active.tuple, Some(json!({"declaration": "sha-1"})));
    assert_eq!(active.tuple_source.as_deref(), Some("event"));
    assert_eq!(
        active.tuple_at, loaded.tuple_at,
        "the turn keeps the tuple's date"
    );

    // A `show` answered between the two lands by arrival, as any answer
    // does, and the turn's close, arriving after it, is the row's last
    // word. (The admission's own `show` stays unanswered; its deadline is
    // past this test's end.)
    let listener = lab.listener.clone();
    let asked = karl.id.clone();
    let shown = tokio::spawn(async move { listener.verb(&asked, "show", Principal::Server).await });
    let id = loop {
        match admin.recv().await {
            Some(ToClient::Verb { id, verb, .. }) if verb == "show" => break id,
            Some(_) => {}
            None => panic!("the show was never asked"),
        }
    };
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer(
                "karl",
                "active",
                Some(json!({"artifact": "a-1"})),
            )),
            error: None,
        })
        .await;
    shown.await.unwrap().unwrap();
    let mid = lab
        .wait_for(&karl.id, "the show's word mid-turn", |a| {
            a.state_source.as_deref() == Some("show")
        })
        .await;
    assert_eq!(mid.load_state.as_deref(), Some("active"));
    assert_eq!(mid.tuple_source.as_deref(), Some("show"));

    admin
        .send(event(
            position(103),
            false,
            trace_event(4, "turn.closed", json!({})),
        ))
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    let closed = lab
        .wait_for(&karl.id, "idle from the turn's close", |a| {
            a.load_state.as_deref() == Some("idle")
                && a.load_state_at
                    .is_some_and(|t| t.timestamp_millis() == WALL_MS + 4)
        })
        .await;
    assert_eq!(closed.state_source.as_deref(), Some("event"));
    assert_eq!(
        closed.tuple,
        Some(json!({"artifact": "a-1"})),
        "the close keeps the tuple the show wrote"
    );
    assert_eq!(
        closed.tuple_source.as_deref(),
        Some("show"),
        "the close keeps the tuple's source, so its shape still reads as a show's"
    );
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
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(true),
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
    let asked = tokio::spawn(async move { listener.verb(&id, "show", Principal::Server).await });
    let ask = match admin.recv().await {
        Some(ToClient::Verb { id, verb, .. }) if verb == "show" => id,
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
    match asked.await.unwrap() {
        Err(VerbError::Fault(fault)) => {
            assert_eq!(fault.kind, VerbFault::NOT_LANDED);
            assert!(
                fault.message.contains("the store could not land"),
                "{}",
                fault.message
            );
        }
        other => panic!("expected the not-landed fault, got {other:?}"),
    }
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

pub(super) fn lab_config(lab: &Lab) -> crate::config::ServerConfig {
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
    // **Each config names its row by identity**, which a re-install is held
    // to, and rotation keeps.
    let agent_id_of = |path: &std::path::Path| {
        let table: toml::Table = std::fs::read_to_string(path).unwrap().parse().unwrap();
        table["agent_id"].as_str().unwrap().to_owned()
    };
    let admin_path = out.path().join(&r#box).join("karl").join("admin-con.toml");
    assert_eq!(agent_id_of(&gate_path), first_id);
    assert_eq!(agent_id_of(&admin_path), first_id);

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
    assert_eq!(agent_id_of(&gate_path), first_id, "rotation keeps the row");
    assert_eq!(agent_id_of(&admin_path), first_id, "rotation keeps the row");
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
    // A long bound, so the monitor's next tick (a quarter of it) is well
    // after the admission attempted below: the admission's own proof of
    // the lock is what refuses it and halts the listener.
    let Some(lab) = Lab::open_with(Duration::from_secs(40)).await else {
        return;
    };
    let karl = lab.register("karl").await;
    let lena = lab.register("lena").await;
    let mut gate = lab.admit(&karl, Plane::Gate).await;
    // Handshaken but yet to say hello: a connection task halt must end
    // too, before the next listener can be admitted.
    let mut mid_hello = lab.connect(&karl.admin).await;
    sqlx::query("SELECT pg_terminate_backend($1)")
        .bind(lab.listener.lock_pid())
        .execute(&lab.store.pool)
        .await
        .unwrap();
    // An admission before the monitor's next tick: refused, since every
    // admission proves the lock, and the failed proof halts the listener.
    let started = tokio::time::Instant::now();
    let mut lena_gate = lab.connect(&lena.gate).await;
    lena_gate
        .send(FromClient::Hello {
            agent: lena.name.clone(),
            plane: Plane::Gate,
            tail: None,
            ceiling: None,
            door: None,
        })
        .await;
    lena_gate.expect_refusal(Refusal::StoreUnavailable).await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "refused by the admission's own proof, not by the monitor's tick"
    );
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
    // A connection arriving after the halt meets no accept, or a closed
    // set: refused or dropped, never admitted.
    match Fake::try_connect(
        lab.listener.address(),
        lab.authority.certificate_pem(),
        &karl.gate,
    )
    .await
    {
        Err(_) => {}
        Ok(mut late) => assert!(
            late.closed().await,
            "a connection after the halt is dropped"
        ),
    }
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
        ceiling: None,
        door: None,
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

/// **The revocation's read-back asks about the set it selected**, so a
/// credential registered between a lost answer and the read-back does not
/// make the rotation conclude the revocation did not land.
#[tokio::test]
async fn the_revocation_read_back_asks_about_the_set_it_selected() {
    let Some(lab) = Lab::open().await else { return };
    let karl = lab.register("karl").await;
    let selected = Store::live_fingerprints_on(&mut lab.store.pool.acquire().await.unwrap())
        .await
        .unwrap();
    assert!(selected.contains(&karl.gate.fingerprint));
    let revoked = Store::revoke_every_credential_on(
        &mut lab.store.pool.acquire().await.unwrap(),
        Some("lab"),
    )
    .await
    .unwrap();
    assert!(revoked >= 1);
    // A registration in between: live, and another fingerprint.
    let m1 = lab.register("m1").await;
    assert!(
        !lab.store.any_live_among(&selected).await.unwrap(),
        "the selected set is revoked"
    );
    assert!(
        lab.store
            .any_live_among(std::slice::from_ref(&m1.gate.fingerprint))
            .await
            .unwrap(),
        "the newcomer is live and is not in the set"
    );
}

/// **A symlink anywhere under the output path refuses the registration**:
/// at the staging entry, where the file is created new and follows no
/// symlink, and at the name directory, with nothing written at the
/// target either way.
#[tokio::test]
async fn a_symlink_under_the_output_path_refuses_the_registration() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());

    // A symlink at the staging path.
    let dir = out.path().join(&r#box).join("karl");
    std::fs::create_dir_all(&dir).unwrap();
    let target = elsewhere.path().join("victim");
    std::fs::write(&target, "precious").unwrap();
    std::os::unix::fs::symlink(&target, dir.join("gate-con.toml.staging")).unwrap();
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
    assert!(!answer.ok, "{}", answer.value);
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "precious",
        "the target is untouched"
    );
    assert!(!dir.join("gate-con.toml").exists());
    assert!(
        lab.store
            .resolve_agent(&format!("{box}/karl", box = r#box))
            .await
            .is_err()
    );

    // A symlinked name directory.
    let linked_dir = elsewhere.path().join("linked");
    std::fs::create_dir(&linked_dir).unwrap();
    std::os::unix::fs::symlink(&linked_dir, out.path().join(&r#box).join("m1")).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "m1",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    assert!(
        answer.value["error"].as_str().unwrap().contains("symlink"),
        "{}",
        answer.value
    );
    assert!(
        std::fs::read_dir(&linked_dir).unwrap().next().is_none(),
        "nothing written at the target"
    );

    // A symlinked box directory.
    let other_out = tempfile::tempdir().unwrap();
    let linked_box = elsewhere.path().join("linked-box");
    std::fs::create_dir(&linked_box).unwrap();
    std::os::unix::fs::symlink(&linked_box, other_out.path().join(&r#box)).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "m2",
        other_out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    assert!(
        answer.value["error"].as_str().unwrap().contains("symlink"),
        "{}",
        answer.value
    );
    assert!(
        std::fs::read_dir(&linked_box).unwrap().next().is_none(),
        "nothing written at the target"
    );

    // A real directory a registration created, swapped for a symlink before
    // the next registration: refused at the open, following nothing.
    let third_out = tempfile::tempdir().unwrap();
    let first = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "m3",
        third_out.path(),
        Some("lab"),
    )
    .await;
    assert!(first.ok, "{}", first.value);
    let box_dir = third_out.path().join(&r#box);
    std::fs::remove_dir_all(&box_dir).unwrap();
    let swapped = elsewhere.path().join("swapped");
    std::fs::create_dir(&swapped).unwrap();
    std::os::unix::fs::symlink(&swapped, &box_dir).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "m4",
        third_out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    assert!(
        answer.value["error"].as_str().unwrap().contains("symlink"),
        "{}",
        answer.value
    );
    assert!(
        std::fs::read_dir(&swapped).unwrap().next().is_none(),
        "nothing written at the target"
    );
}

/// **A staging write that fails leaves no entry behind.** The file created
/// for the staging is unlinked when its write fails, so the retry is not
/// refused by the partial file as an entry that already exists.
#[tokio::test]
async fn a_staging_write_that_fails_leaves_no_entry_behind() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let dir = out.path().join(&r#box).join("retry");

    super::verbs::FAIL_STAGE_WRITE.with(|f| f.set(true));
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "retry",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    let error = answer.value["error"].as_str().unwrap();
    assert!(error.contains("the partial file is removed"), "{error}");
    assert!(!dir.join("gate-con.toml.staging").exists());
    assert!(!dir.join("admin-con.toml.staging").exists());
    assert!(
        lab.store
            .resolve_agent(&format!("{box}/retry"))
            .await
            .is_err(),
        "no row landed"
    );

    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "retry",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "the retry lands: {}", answer.value);
    assert!(dir.join("gate-con.toml").exists());
    assert!(dir.join("admin-con.toml").exists());
}

/// **A config directory another party could write is refused.** A box or a
/// name directory that is group- or other-writable is refused once opened,
/// naming the mode found, and nothing is staged under it; made private, the
/// registration lands. (A directory owned by another user is refused by
/// the same check, which a test without root cannot stage.)
#[tokio::test]
async fn a_config_directory_another_party_could_write_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let box_dir = out.path().join(&r#box);
    let dir = box_dir.join("shared");

    // A group-writable box directory.
    std::fs::create_dir(&box_dir).unwrap();
    std::fs::set_permissions(&box_dir, std::fs::Permissions::from_mode(0o770)).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "shared",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    let error = answer.value["error"].as_str().unwrap();
    assert!(error.contains("writable by group or others"), "{error}");
    assert!(error.contains("0770"), "{error}");
    assert!(!dir.exists(), "nothing is made under it");

    // A private box directory and an other-writable name directory.
    std::fs::set_permissions(&box_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o707)).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "shared",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(!answer.ok, "{}", answer.value);
    let error = answer.value["error"].as_str().unwrap();
    assert!(error.contains("writable by group or others"), "{error}");
    assert!(error.contains("0707"), "{error}");
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "nothing is staged under it"
    );
    assert!(
        lab.store
            .resolve_agent(&format!("{box}/shared"))
            .await
            .is_err(),
        "no row landed"
    );

    // Both private: the registration lands.
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let answer = super::verbs::register(
        &lab.store,
        &cfg,
        &lab.authority,
        &r#box,
        "shared",
        out.path(),
        Some("lab"),
    )
    .await;
    assert!(answer.ok, "{}", answer.value);
    assert!(dir.join("gate-con.toml").exists());
}

/// **The publish renames the inode it staged.** An entry swapped in at a
/// staging name between the staging and the publish is not the file this
/// run wrote, and the publish refuses it rather than renaming it into
/// place, for the gate config before anything is published and for the
/// admin config after the gate's is.
#[test]
fn a_staging_entry_swapped_before_the_publish_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let authority = Authority::init(&tmp.path().join("authority"), "weaver-web", &[]).unwrap();
    let cfg = crate::config::ServerConfig {
        listen: "127.0.0.1:0".into(),
        link_listen: "127.0.0.1:1".into(),
        database: String::new(),
        authority_dir: authority.dir().to_owned(),
        silence_bound_secs: 60,
        link_address: None,
        server_name: "weaver-web".into(),
        admins: Vec::new(),
        agent_hop_budget: 8,
        providers: Vec::new(),
    };
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let (gate, admin) = super::verbs::mint_pair("swap", &authority).unwrap();

    // The gate staging entry swapped: nothing is published.
    let dir = super::verbs::ConfigDir::open(&out, "box", "swap").unwrap();
    let staged =
        super::verbs::stage_pair(&cfg, &authority, dir, PLACEHOLDER_ID, "swap", &gate, &admin)
            .unwrap();
    let agent_dir = out.join("box").join("swap");
    std::fs::remove_file(agent_dir.join("gate-con.toml.staging")).unwrap();
    std::fs::write(agent_dir.join("gate-con.toml.staging"), "swapped in").unwrap();
    let refused = staged.publish(false).unwrap_err().to_string();
    assert!(
        refused.contains("not the file this run staged"),
        "{refused}"
    );
    assert!(refused.contains("admin config stands staged"), "{refused}");
    assert!(!agent_dir.join("gate-con.toml").exists());
    assert!(!agent_dir.join("admin-con.toml").exists());

    // The admin staging entry swapped: the gate config stands, the admin's
    // is refused.
    let dir = super::verbs::ConfigDir::open(&out, "box", "swap2").unwrap();
    let staged = super::verbs::stage_pair(
        &cfg,
        &authority,
        dir,
        PLACEHOLDER_ID,
        "swap2",
        &gate,
        &admin,
    )
    .unwrap();
    let agent_dir = out.join("box").join("swap2");
    std::fs::remove_file(agent_dir.join("admin-con.toml.staging")).unwrap();
    std::fs::write(agent_dir.join("admin-con.toml.staging"), "swapped in").unwrap();
    let refused = staged.publish(false).unwrap_err().to_string();
    assert!(refused.contains("the gate config stands at"), "{refused}");
    assert!(
        refused.contains("not the file this run staged"),
        "{refused}"
    );
    assert!(agent_dir.join("gate-con.toml").exists());
    assert!(!agent_dir.join("admin-con.toml").exists());
    assert_eq!(
        std::fs::read_to_string(agent_dir.join("admin-con.toml.staging")).unwrap(),
        "swapped in",
        "the swapped entry is left where it was found"
    );
}

/// **The admission's show is the connection's initialisation.** An
/// admin-con that answers it with an error is closed at once, and one that
/// only heartbeats is closed at the silence bound, each with the typed
/// refusal and the row disconnected, so the reconnect asks again; one that
/// answers with a state stays admitted past the bound. A show answered
/// late whose landing stalls is closed at the one bound from the hello,
/// not a fresh bound from the answer.
#[tokio::test]
async fn an_admission_whose_show_is_not_answered_is_closed() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;

    async fn hello(lab: &Lab, karl: &Registered) -> (Fake, u64) {
        let mut admin = lab.connect(&karl.admin).await;
        admin
            .send(FromClient::Hello {
                agent: karl.name.clone(),
                plane: Plane::Admin,
                tail: Some(position(100)),
                ceiling: Some(vec!["show".to_owned()]),
                door: Some(true),
            })
            .await;
        assert!(matches!(
            admin.recv().await,
            Some(ToClient::HelloAnswer { .. })
        ));
        match admin.recv().await {
            Some(ToClient::Verb { id, verb, .. }) if verb == "show" => (admin, id),
            other => panic!("expected the show ask, got {other:?}"),
        }
    }

    // Answered with an error: closed at once.
    let (mut admin, id) = hello(&lab, &karl).await;
    lab.wait_for(&karl.id, "connected", |a| a.admin.connected)
        .await;
    admin
        .send(FromClient::Verb {
            id,
            outcome: None,
            error: Some(VerbFault {
                kind: VerbFault::UNKNOWN.into(),
                message: "show failed".into(),
            }),
        })
        .await;
    admin.expect_refusal(Refusal::AdmissionIncomplete).await;
    lab.wait_for(&karl.id, "disconnected", |a| !a.admin.connected)
        .await;

    // Heartbeats only: closed at the bound, not before.
    let (admin, _) = hello(&lab, &karl).await;
    let started = tokio::time::Instant::now();
    let Fake {
        mut reader,
        mut writer,
    } = admin;
    let heartbeats = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let mut line = serde_json::to_string(&FromClient::Heartbeat).unwrap();
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                return;
            }
        }
    });
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
        .await
        .expect("a frame within the bound")
        .unwrap();
    let frame: ToClient = serde_json::from_str(line.trim_end()).unwrap();
    assert!(
        matches!(
            frame,
            ToClient::Refusal {
                reason: Refusal::AdmissionIncomplete
            }
        ),
        "{frame:?}"
    );
    assert!(
        started.elapsed() >= Duration::from_secs(3),
        "closed at the bound, not at once: {:?}",
        started.elapsed()
    );
    line.clear();
    assert_eq!(reader.read_line(&mut line).await.unwrap(), 0, "then closed");
    heartbeats.abort();
    lab.wait_for(&karl.id, "disconnected", |a| !a.admin.connected)
        .await;

    // Answered with a state: admitted past the bound.
    let (mut admin, id) = hello(&lab, &karl).await;
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer(&karl.name, "idle", None)),
            error: None,
        })
        .await;
    lab.wait_for(&karl.id, "idle", |a| {
        a.load_state.as_deref() == Some("idle")
    })
    .await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    admin.send(FromClient::Heartbeat).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert!(
        lab.agent(&karl.id).await.admin.connected,
        "a connection whose show answered stays admitted"
    );
    assert!(!admin.closed().await);
    drop(admin);
    lab.wait_for(&karl.id, "disconnected", |a| !a.admin.connected)
        .await;

    // Answered late, the landing stalled: closed at the one bound from the
    // hello, not a fresh bound from the answer.
    let (mut admin, id) = hello(&lab, &karl).await;
    let since_hello = tokio::time::Instant::now();
    tokio::time::sleep(Duration::from_millis(2500)).await;
    lab.listener.stall_next_land();
    admin
        .send(FromClient::Verb {
            id,
            outcome: Some(show_answer(&karl.name, "idle", None)),
            error: None,
        })
        .await;
    admin.expect_refusal(Refusal::AdmissionIncomplete).await;
    let elapsed = since_hello.elapsed();
    assert!(
        elapsed >= Duration::from_secs(3) && elapsed < Duration::from_millis(5500),
        "closed at the admission's one bound: {elapsed:?}"
    );
    lab.wait_for(&karl.id, "disconnected", |a| !a.admin.connected)
        .await;
}

/// **A store landing is bounded like a read or an enqueue.** A landing
/// that does not complete within the bound closes the connection as
/// `store_unavailable` with no ack, the acknowledged position standing, so
/// the replay resends; and a revocation closes a connection whose landing
/// is stalled without waiting on the store.
#[tokio::test]
async fn a_landing_that_stalls_closes_the_connection_without_an_ack() {
    let Some(lab) = Lab::open_with(Duration::from_secs(4)).await else {
        return;
    };
    let karl = lab.register("karl").await;

    // An admission whose show is answered, so only the landing is in play.
    async fn admitted(lab: &Lab, karl: &Registered) -> Fake {
        let mut admin = lab.connect(&karl.admin).await;
        admin
            .send(FromClient::Hello {
                agent: karl.name.clone(),
                plane: Plane::Admin,
                tail: Some(position(100)),
                ceiling: Some(vec!["show".to_owned()]),
                door: Some(true),
            })
            .await;
        assert!(matches!(
            admin.recv().await,
            Some(ToClient::HelloAnswer { .. })
        ));
        let id = match admin.recv().await {
            Some(ToClient::Verb { id, verb, .. }) if verb == "show" => id,
            other => panic!("expected the show ask, got {other:?}"),
        };
        admin.send(FromClient::CaughtUp).await;
        admin
            .send(FromClient::Verb {
                id,
                outcome: Some(show_answer(&karl.name, "idle", None)),
                error: None,
            })
            .await;
        lab.wait_for(&karl.id, "idle", |a| {
            a.admin.connected && a.load_state.as_deref() == Some("idle")
        })
        .await;
        match admin.recv().await {
            Some(ToClient::Landed { id: landed }) => assert_eq!(landed, id),
            other => panic!("expected the show's landing, got {other:?}"),
        }
        admin
    }

    // A landing that stalls: refused at the bound, no ack, the position
    // standing at the last success.
    let mut admin = admitted(&lab, &karl).await;
    admin
        .send(FromClient::Event {
            position: position(110),
            replayed: false,
            event: trace_event(1, "turn", json!({})),
        })
        .await;
    assert!(matches!(admin.recv().await, Some(ToClient::Ack { .. })));
    lab.listener.stall_next_land();
    let started = tokio::time::Instant::now();
    admin
        .send(FromClient::Event {
            position: position(120),
            replayed: false,
            event: trace_event(2, "load", json!({"declaration": "sha-1"})),
        })
        .await;
    admin.expect_refusal(Refusal::StoreUnavailable).await;
    assert!(
        started.elapsed() >= Duration::from_secs(3),
        "cut off at the bound, not before: {:?}",
        started.elapsed()
    );
    assert_eq!(lab.listener.acknowledged(&karl.id), Some(position(110)));
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;

    // A revocation closes a connection whose landing is stalled without
    // waiting on the store.
    let mut admin = admitted(&lab, &karl).await;
    lab.listener.stall_next_land();
    admin
        .send(FromClient::Event {
            position: position(140),
            replayed: false,
            event: trace_event(3, "load", json!({"declaration": "sha-2"})),
        })
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let started = tokio::time::Instant::now();
    let row = lab.agent(&karl.id).await;
    lab.store
        .revoke_credential(&row, Plane::Admin, Some("lab"))
        .await
        .unwrap();
    let eof = tokio::time::timeout(Duration::from_secs(3), async {
        let mut line = String::new();
        loop {
            line.clear();
            if admin.reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                break;
            }
        }
    })
    .await;
    assert!(
        eof.is_ok(),
        "closed by the revocation well within the bound, not after the landing: {:?}",
        started.elapsed()
    );
    lab.wait_for(&karl.id, "admin down", |a| !a.admin.connected)
        .await;
}

/// **A retained staged pair is consumed by the retry.** A pair staged by a
/// run whose commit landed and whose answer and read-back were both lost
/// is published by the re-run, with a note; a pair the register does not
/// carry is discarded and the re-run proceeds with a fresh one; half a
/// pair is discarded; and a rotation's retained pair is published the same
/// way, replacing the standing configs.
#[tokio::test]
async fn a_retained_staged_pair_is_published_where_the_register_carries_it() {
    let Some(lab) = Lab::open().await else { return };
    let cfg = lab_config(&lab);
    let out = tempfile::tempdir().unwrap();
    let r#box = format!("box-{}", uuid::Uuid::new_v4().simple());
    let authority_fp = lab.authority.fingerprint();
    let stage = |name: &str| {
        let (gate, admin) = super::verbs::mint_pair(name, &lab.authority).unwrap();
        let dir = super::verbs::ConfigDir::open(out.path(), &r#box, name).unwrap();
        // The `Staged` is dropped unpublished: the files stand staged.
        super::verbs::stage_pair(
            &cfg,
            &lab.authority,
            dir,
            PLACEHOLDER_ID,
            name,
            &gate,
            &admin,
        )
        .unwrap();
        (gate, admin)
    };
    let register = |name: &'static str| {
        super::verbs::register(
            &lab.store,
            &cfg,
            &lab.authority,
            &r#box,
            name,
            out.path(),
            Some("lab"),
        )
    };

    // Carried by the register: published, with the note.
    let (gate, admin) = stage("karl");
    let (id, _) = lab
        .store
        .register_agent(
            &r#box,
            "karl",
            Some("lab"),
            &gate.fingerprint,
            &admin.fingerprint,
            &authority_fp,
        )
        .await
        .unwrap();
    let answer = register("karl").await;
    assert!(answer.ok, "{}", answer.value);
    assert_eq!(answer.value["agent"].as_str().unwrap(), id.as_str());
    assert_eq!(
        answer.value["gate_fingerprint"].as_str().unwrap(),
        gate.fingerprint
    );
    assert!(
        answer.value["note"]
            .as_str()
            .unwrap()
            .contains("staged by an earlier run"),
        "{}",
        answer.value
    );
    let karl_dir = out.path().join(&r#box).join("karl");
    let published = std::fs::read_to_string(karl_dir.join("gate-con.toml")).unwrap();
    assert!(
        published.contains(&gate.certificate_pem),
        "the staged pair itself"
    );
    assert!(!karl_dir.join("gate-con.toml.staging").exists());
    assert!(!karl_dir.join("admin-con.toml.staging").exists());

    // Not carried: discarded, and the registration proceeds with a fresh
    // pair the row then carries.
    let (stale_gate, _) = stage("lena");
    let answer = register("lena").await;
    assert!(answer.ok, "{}", answer.value);
    assert!(answer.value["note"].is_null(), "{}", answer.value);
    let fresh_fp = answer.value["gate_fingerprint"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(fresh_fp, stale_gate.fingerprint);
    let lena_dir = out.path().join(&r#box).join("lena");
    let published = std::fs::read_to_string(lena_dir.join("gate-con.toml")).unwrap();
    assert!(!published.contains(&stale_gate.certificate_pem));
    let row = lab
        .store
        .resolve_agent(&format!("{box}/lena", box = r#box))
        .await
        .unwrap();
    assert_eq!(row.gate.fingerprint, fresh_fp);
    assert!(!lena_dir.join("gate-con.toml.staging").exists());

    // Half a pair: discarded, and the registration proceeds.
    let mira_dir = out.path().join(&r#box).join("mira");
    std::fs::create_dir(&mira_dir).unwrap();
    std::fs::write(mira_dir.join("admin-con.toml.staging"), "half").unwrap();
    let answer = register("mira").await;
    assert!(answer.ok, "{}", answer.value);
    assert!(!mira_dir.join("admin-con.toml.staging").exists());
    assert!(mira_dir.join("admin-con.toml").exists());

    // A rotation's retained pair: published, replacing the standing configs.
    let karl_row = lab
        .store
        .resolve_agent(&format!("{box}/karl", box = r#box))
        .await
        .unwrap();
    let (rotated_gate, rotated_admin) = stage("karl");
    lab.store
        .rotate_credentials(
            &karl_row,
            Some("lab"),
            &rotated_gate.fingerprint,
            &rotated_admin.fingerprint,
            &authority_fp,
        )
        .await
        .unwrap();
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
    assert_eq!(
        answer.value["gate_fingerprint"].as_str().unwrap(),
        rotated_gate.fingerprint
    );
    assert!(
        answer.value["note"]
            .as_str()
            .unwrap()
            .contains("staged by an earlier run"),
        "{}",
        answer.value
    );
    let published = std::fs::read_to_string(karl_dir.join("gate-con.toml")).unwrap();
    assert!(published.contains(&rotated_gate.certificate_pem));
    assert!(!karl_dir.join("gate-con.toml.staging").exists());
}
