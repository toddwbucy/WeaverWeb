//! conforms: web-the-pairing-page-reads-an-entry-in-crockfords-form-and-a-malformed-entry-is-no-miss
//! conforms: web-a-pairing-naming-a-live-box-and-name-is-refused-and-retires-nothing
//! conforms: web-a-lost-word-writes-no-row
//! conforms: web-on-unknown-the-pairing-page-shows-what-the-write-made
//! conforms: web-a-rows-box-changes-only-through-the-replace-box-write
//! conforms: web-a-replacements-drive-setup-mounts-the-state-drive-it-carries-and-never-formats-it
//! conforms: web-a-setup-ask-is-refused-while-its-credentials-connection-stands-and-counts-for-nothing-in-presence
//! conforms: web-the-setup-keeps-the-latest-report-of-each-stage-verbatim
//! conforms: web-an-adopted-pair-connects
//!
//! **An appliance's enrollment, the exchange** (Spec 8.2): a code's entry on
//! the pairing page, against the real listener, with the box's half
//! (`link::pair_client`) as the box and an admin's session as the operator,
//! and the commands on the box (`link::box_commands`) end to end, with the
//! setup connection's two asks (Spec 8.3) under the config `adopt` completed.
//! Each test runs on a store of its own with an authority in a temporary
//! directory. No name, address or key is a real one.

use std::sync::Arc;
use std::time::Duration;

use axum::http::{StatusCode, header};
use rcgen::{CertificateParams, KeyPair};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::admin_tests::{admin, form, send_as};
use crate::link::authority::{Authority, client_tls, fingerprint};
use crate::link::frames::{FromClient, Plane, ToClient};
use crate::link::listener::Listener;
use crate::link::pair_client::{self, Answer, Claim, Dialed, Heard, LastWord};
use crate::link::pair_code::grouped;
use crate::link::register::CredentialState;
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys};
use crate::setup_tests::{begin, page};
use crate::store::read::tests::{Fresh, fresh_store};
use crate::store::{AgentId, Store};
use crate::surfaces::gate;
use crate::surfaces::setup::{Enrollment, Seams, routes};

/// The least silence bound the link takes: a cadence of one second.
const SILENCE: Duration = Duration::from_secs(4);
const SOON: Duration = Duration::from_secs(15);

struct Rig {
    fresh: Fresh,
    _dir: tempfile::TempDir,
    listener: Listener,
    authority: Arc<Authority>,
    app: axum::Router,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.listener.stop_now();
    }
}

impl Rig {
    fn store(&self) -> &Store {
        &self.fresh.store
    }
}

async fn rig() -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = tempfile::tempdir().unwrap();
    let authority =
        Arc::new(Authority::init(&dir.path().join("authority"), "weaver-web", &[]).unwrap());
    let listener = Listener::start_with(
        fresh.store.clone(),
        &authority,
        "127.0.0.1:0",
        SILENCE,
        Some(dir.path().join("operations.log")),
    )
    .await
    .unwrap();
    let cfg = crate::agents_tests::server_config(dir.path(), listener.address());
    let policy = gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
        write_bound: Duration::from_secs(60),
    };
    let client = crate::hugging_face::Client::with_base("http://127.0.0.1:9/").unwrap();
    let puller = crate::hugging_face::pull::Puller::start(
        fresh.store.clone(),
        client.clone(),
        None,
        dir.path().join("authority"),
        crate::hugging_face::pull::Settings::default(),
    );
    let surface = gate::guard(
        crate::surfaces::script_policy(routes(Seams {
            policy: policy.clone(),
            cfg: Arc::new(cfg),
            client,
            puller,
            authority_dir: dir.path().join("authority"),
            headers: crate::hugging_face::headers::Headers::new(),
            boxes: Arc::new(crate::box_report::Kept),
            enrollment: Some(Enrollment {
                listener: listener.clone(),
                authority: authority.clone(),
            }),
        }))
        // The register page, for its label.
        .merge(crate::surfaces::script_policy(
            crate::surfaces::agents::routes(crate::surfaces::agents::Seams {
                policy: policy.clone(),
                cfg: Arc::new(crate::agents_tests::server_config(
                    dir.path(),
                    listener.address(),
                )),
                authority: authority.clone(),
                handover: crate::surfaces::agents::Handover::default(),
            }),
        ))
        .with_state(fresh.store.clone()),
        policy,
    );
    let app = app(&fresh.store, Some(passkeys())).merge(surface);
    Some(Rig {
        fresh,
        _dir: dir,
        listener,
        authority,
        app,
    })
}

/// A box's two keys, made inside it, and its claim with their requests.
struct BoxKeys {
    gate: KeyPair,
    admin: KeyPair,
    claim: Claim,
}

fn request_of(key: &KeyPair) -> String {
    CertificateParams::new(Vec::<String>::new())
        .unwrap()
        .serialize_request(key)
        .unwrap()
        .pem()
        .unwrap()
}

fn box_keys(r#box: &str) -> BoxKeys {
    let gate = KeyPair::generate().unwrap();
    let admin = KeyPair::generate().unwrap();
    let claim = Claim {
        r#box: r#box.into(),
        gate_request: request_of(&gate),
        admin_request: request_of(&admin),
    };
    BoxKeys { gate, admin, claim }
}

async fn dial(rig: &Rig, claim: &Claim) -> Dialed {
    pair_client::dial(&rig.listener.address().to_string(), claim)
        .await
        .unwrap()
}

/// **A rig a box enrolls against**: the app serving the pairing page, and
/// the real listener with its authority.
trait Enrolling {
    fn app(&self) -> &axum::Router;
    fn listener(&self) -> &Listener;
    fn authority(&self) -> &Authority;
}

impl Enrolling for Rig {
    fn app(&self) -> &axum::Router {
        &self.app
    }

    fn listener(&self) -> &Listener {
        &self.listener
    }

    fn authority(&self) -> &Authority {
        &self.authority
    }
}

impl Enrolling for crate::setup_tests::Rig {
    fn app(&self) -> &axum::Router {
        &self.app
    }

    fn listener(&self) -> &Listener {
        &self
            .enrollment
            .as_ref()
            .expect("the rig serves the enrollment")
            .0
    }

    fn authority(&self) -> &Authority {
        &self
            .enrollment
            .as_ref()
            .expect("the rig serves the enrollment")
            .1
    }
}

/// The code's entry as the page posts it, the comparison confirmed.
async fn enter(
    rig: &impl Enrolling,
    session: &str,
    setup: &str,
    code: &str,
    replace: Option<&AgentId>,
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut fields = vec![("compared", "on"), ("code", code)];
    if let Some(id) = replace {
        fields.push(("replace", id.as_str()));
    }
    send_as(
        rig.app(),
        "POST",
        &format!("/setup/{setup}/pair"),
        Some(form(&fields)),
        session,
    )
    .await
}

/// **What the box does with the answer**: the box's word, then the last
/// word, or, for the tests of a lost word and an unknown, less.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Box {
    /// Send the word and read the last word.
    Whole,
    /// Close before the word.
    NoWord,
    /// Send the word and close, reading no last word: the box's unknown.
    WordThenGone,
}

/// The box's side of one exchange, run beside the entry: what it heard,
/// and the last word where it read one.
async fn the_box(mut dialed: Dialed, does: Box) -> (Heard, Option<LastWord>) {
    let heard = tokio::time::timeout(SOON, dialed.next())
        .await
        .expect("the box heard the console");
    if !matches!(heard, Heard::Answer(_)) {
        return (heard, None);
    }
    match does {
        Box::NoWord => (heard, None),
        Box::WordThenGone => {
            dialed.written().await.unwrap();
            (heard, None)
        }
        Box::Whole => {
            dialed.written().await.unwrap();
            let last = dialed.last_word().await;
            (heard, Some(last))
        }
    }
}

fn answer_of(heard: &Heard) -> &Answer {
    match heard {
        Heard::Answer(answer) => answer,
        other => panic!("the box heard no answer: {other:?}"),
    }
}

fn fingerprint_of(pem: &str) -> String {
    let mut bytes = pem.as_bytes();
    let der = rustls_pemfile::certs(&mut bytes).next().unwrap().unwrap();
    fingerprint(der.as_ref())
}

/// A live row for `box`/`name`, registered as the host's `register` writes
/// one, its fingerprints made up.
async fn registered(s: &Store, r#box: &str, name: &str, authority: &Authority) -> AgentId {
    let id: AgentId = s.mint_key("ag").await.unwrap().parse().unwrap();
    let mut conn = s.pool.acquire().await.unwrap();
    Store::register_agent_on(
        &mut conn,
        &id,
        r#box,
        name,
        None,
        &"1".repeat(64),
        &"2".repeat(64),
        &authority.fingerprint(),
    )
    .await
    .unwrap();
    id
}

async fn pair_audit(s: &Store, action: &str) -> (i64, i64, i64) {
    let count = |clause: &'static str| {
        let query = format!("SELECT count(*) FROM audit WHERE action = $1 AND {clause}");
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
                .bind(action)
                .fetch_one(&s.pool)
                .await
                .unwrap()
        }
    };
    (
        count("answers IS NULL AND refusal IS NULL").await,
        count("outcome = 'ok'").await,
        count("refusal IS NOT NULL").await,
    )
}

async fn rows_named(s: &Store, name: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM agent WHERE name = $1")
        .bind(name)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **A pairing writes the row with both certificates live**, signed over
/// the box's own keys, its author the admin who entered the code, the
/// setup linked, and the label "paired, setup running" until a connector
/// is first admitted; the box hears its answer, the console's authority
/// among it, and then `paired`.
#[tokio::test]
async fn a_pairing_writes_the_row_and_the_box_hears_paired() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let (status, headers, body) = enter(&rig, &ada, &setup, &code, None).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(headers[header::LOCATION], format!("/setup/{setup}"));
    let (heard, last) = the_box.await.unwrap();
    assert_eq!(last, Some(LastWord::Paired));
    let answer = answer_of(&heard);
    assert_eq!(answer.agent, "karl");
    assert_eq!(answer.r#box, "box-a");
    assert_eq!(answer.server_name, "weaver-web");
    assert_eq!(answer.authority, rig.authority.certificate_pem());

    let id: AgentId = answer.agent_id.parse().unwrap();
    let row = s.agent(&id).await.unwrap().expect("the row stands");
    assert_eq!((row.name.as_str(), row.r#box.as_str()), ("karl", "box-a"));
    assert_eq!(row.gate.fingerprint, fingerprint_of(&answer.gate));
    assert_eq!(row.admin.fingerprint, fingerprint_of(&answer.admin));
    assert_eq!(row.gate.state, CredentialState::Live);
    assert_eq!(row.admin.state, CredentialState::Live);
    assert_eq!(row.author.as_deref(), Some(ada_id.as_str()));
    assert!(row.setup_running);
    // Each certificate is over the box's own key for its plane.
    for (pem, key) in [(&answer.gate, &keys.gate), (&answer.admin, &keys.admin)] {
        let mut bytes = pem.as_bytes();
        let der = rustls_pemfile::certs(&mut bytes).next().unwrap().unwrap();
        let point = key.public_key_raw();
        assert!(
            der.windows(point.len()).any(|w| w == point),
            "a certificate is not over the box's key"
        );
    }
    let linked = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!(
        linked.paired,
        Some((id.as_str().to_owned(), "pair".to_owned()))
    );
    assert_eq!(pair_audit(s, "pair").await, (1, 1, 0));
    let pairing = page(&rig.app, &format!("/setup/{setup}"), &ada).await;
    assert!(pairing.contains("paired, setup running"), "{pairing}");
    assert!(
        pairing.contains(&grouped(&row.gate.fingerprint)),
        "{pairing}"
    );
    let register = page(&rig.app, "/admin/agents", &ada).await;
    assert!(register.contains("paired, setup running"), "{register}");

    // **The first admission clears the label**: the gate plane's hello
    // with the answer's certificate and the box's own key.
    let tls = client_tls(
        rig.authority.certificate_pem(),
        &answer.gate,
        &keys.gate.serialize_pem(),
    )
    .unwrap();
    let tcp = tokio::net::TcpStream::connect(rig.listener.address())
        .await
        .unwrap();
    let name = rustls::pki_types::ServerName::try_from("weaver-web".to_string()).unwrap();
    let stream = tokio_rustls::TlsConnector::from(tls)
        .connect(name, tcp)
        .await
        .unwrap();
    let (read, mut write) = tokio::io::split(stream);
    let mut hello = serde_json::to_string(&FromClient::Hello {
        agent: "karl".into(),
        plane: Plane::Gate,
        tail: None,
        door: None,
        ceiling: None,
        capabilities: None,
    })
    .unwrap();
    hello.push('\n');
    write.write_all(hello.as_bytes()).await.unwrap();
    write.flush().await.unwrap();
    let mut line = String::new();
    tokio::time::timeout(SOON, BufReader::new(read).read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(
            serde_json::from_str(line.trim_end()),
            Ok(ToClient::HelloAnswer { .. })
        ),
        "{line}"
    );
    let row = s.agent(&id).await.unwrap().unwrap();
    assert!(!row.setup_running, "the first admission clears the label");
    assert!(row.gate.connected);
}

/// **The page reads an entry in Crockford's form, and a malformed entry is
/// no miss** (Spec 8.2): answered as malformed, no record and no count. A
/// well-formed code that matches no waiting box is one refusal record
/// naming the setup and never the code, and a miss; five close the field,
/// kept on the setup row, so the right code is refused while it is closed.
///
/// Perturbation: count a malformed entry as a miss, and the count moves.
#[tokio::test]
async fn a_malformed_entry_is_no_miss_and_five_misses_close_the_field() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();

    let (status, _, body) = enter(&rig, &ada, &setup, "HWX5U0F74A", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(
        pair_audit(s, "pair").await,
        (0, 0, 0),
        "a malformed entry wrote a record"
    );
    assert_eq!(s.setup(&setup).await.unwrap().unwrap().code_misses, 0);

    // No confirmation of the fingerprints: refused before anything.
    let (status, _, _) = send_as(
        &rig.app,
        "POST",
        &format!("/setup/{setup}/pair"),
        Some(form(&[("code", "HWX5R 0F74A")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(pair_audit(s, "pair").await, (0, 0, 0));

    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let right = dialed.code().to_string();
    let wrong = if right.starts_with('0') {
        "1111111111"
    } else {
        "0000000000"
    };
    for miss in 1..=5 {
        let (status, _, body) = enter(&rig, &ada, &setup, wrong, None).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(
            pair_audit(s, "pair").await,
            (0, 0, miss),
            "one refusal record per miss"
        );
    }
    let refusals: Vec<String> = sqlx::query_scalar(
        "SELECT refusal FROM audit WHERE action = 'pair' AND refusal IS NOT NULL",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    assert!(refusals.iter().all(|r| !r.contains(wrong)), "{refusals:?}");
    let closed = s.setup(&setup).await.unwrap().unwrap();
    assert!(
        closed.code_closed(chrono::Utc::now()),
        "five misses close the field"
    );
    assert_eq!(closed.code_misses, 0, "the count starts again");
    let (status, _, body) = enter(&rig, &ada, &setup, &right, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("five misses"), "{body}");
    assert_eq!(rows_named(s, "karl").await, 0);
    let pairing = page(&rig.app, &format!("/setup/{setup}"), &ada).await;
    assert!(
        pairing.contains("Five misses closed the code field"),
        "{pairing}"
    );
    assert!(!pairing.contains(r#"name="code""#), "{pairing}");
    drop(dialed);
}

/// Whether the authority lock is free, taken and given back at once on
/// one connection.
async fn authority_lock_free(s: &Store) -> bool {
    let mut conn = s.pool.acquire().await.unwrap();
    let free: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(crate::link::listener::AUTHORITY_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    if free {
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(crate::link::listener::AUTHORITY_LOCK_KEY)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
    free
}

/// **The authority lock is held across the box's word** (Spec 8.2): from
/// step 1 through step 6, so no rotation or registration runs between the
/// answer the box holds and the row that names it, and it is given back
/// at the end.
///
/// Perturbation: give the lock back before step 5, and it is free while
/// the box holds its answer.
#[tokio::test]
async fn the_authority_lock_is_held_across_the_boxs_word() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let mut dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = async {
        let heard = tokio::time::timeout(SOON, dialed.next()).await.unwrap();
        let free = authority_lock_free(s).await;
        dialed.written().await.unwrap();
        (heard, free, dialed.last_word().await)
    };
    let ((status, _, body), (heard, free, last)) =
        tokio::join!(enter(&rig, &ada, &setup, &code, None), the_box);
    assert!(matches!(heard, Heard::Answer(_)), "{heard:?}");
    assert!(!free, "the lock was free while the box held its answer");
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(last, LastWord::Paired);
    assert!(
        authority_lock_free(s).await,
        "the lock outlived the exchange"
    );
}

/// **Entries at once for one setup compare one at a time, and none past
/// the fifth miss** (Spec 8.2): twelve wrong entries sent together count
/// exactly five misses, each one record, and close the field; the seven
/// after the fifth find it closed and compare nothing; and the box's own
/// code is then refused while the field is closed.
///
/// Perturbation: read the field without the setup row's lock, and every
/// entry compares, so the count runs past five.
#[tokio::test]
async fn entries_at_once_count_five_misses_and_compare_none_past_them() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let right = dialed.code().to_string();
    let wrong = if right.starts_with('0') {
        "1111111111"
    } else {
        "0000000000"
    };
    let mut entries = tokio::task::JoinSet::new();
    for _ in 0..12 {
        let (app, ada, setup) = (rig.app.clone(), ada.clone(), setup.clone());
        entries.spawn(async move {
            send_as(
                &app,
                "POST",
                &format!("/setup/{setup}/pair"),
                Some(form(&[("compared", "on"), ("code", wrong)])),
                &ada,
            )
            .await
        });
    }
    while let Some(answer) = entries.join_next().await {
        let (status, _, body) = answer.unwrap();
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
    }
    let refusals = |why: &'static str| async move {
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit WHERE action = 'pair' AND refusal = $1",
        )
        .bind(why)
        .fetch_one(&s.pool)
        .await
        .unwrap()
    };
    assert_eq!(
        refusals("the code matches no waiting box").await,
        5,
        "exactly five misses compare"
    );
    assert_eq!(refusals("the code field is closed").await, 7);
    let closed = s.setup(&setup).await.unwrap().unwrap();
    assert!(closed.code_closed(chrono::Utc::now()));
    assert_eq!(closed.code_misses, 0, "the count runs past five");
    let (status, _, body) = enter(&rig, &ada, &setup, &right, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("five misses"), "{body}");
    assert_eq!(rows_named(s, "karl").await, 0);
    drop(dialed);
}

/// **The exchange's first step refuses a field five misses closed**
/// (Spec 8.2), a second guard beside the entry's lock: the box would read
/// `setup_closed`.
///
/// Perturbation: drop the field from step 1's check, and the pairing
/// stands.
#[tokio::test]
async fn the_first_step_refuses_a_closed_field() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let id: AgentId = s.mint_key("ag").await.unwrap().parse().unwrap();
    let act = crate::link::register::PairingAct::Pair(id);
    let pairing = crate::link::register::Pairing {
        setup_id: &setup,
        act: &act,
        r#box: "box-a",
        name: "karl",
    };
    let mut conn = s.pool.acquire().await.unwrap();
    let open = Store::pairing_check_on(&mut conn, &pairing).await.unwrap();
    assert_eq!(open, Ok(()));
    sqlx::query("UPDATE agent_setup SET code_closed_until = now() + interval '10 minutes' WHERE setup_id = $1")
        .bind(&setup)
        .execute(&s.pool)
        .await
        .unwrap();
    let closed = Store::pairing_check_on(&mut conn, &pairing).await.unwrap();
    assert_eq!(
        closed,
        Err(crate::link::register::PairingRefusal::FieldClosed)
    );
    assert_eq!(closed.unwrap_err().reason(), "setup_closed");
}

/// **A matched box hears heartbeats while its exchange waits on the
/// authority lock, and pairs once the lock is free** (Spec 8.2): another
/// write holds the lock past four of the box's cadences, the box's silence.
///
/// Perturbation: send no heartbeat while the connection is kept, and the box
/// hears silence and spends its code with no answer.
#[tokio::test]
async fn a_box_whose_exchange_waits_on_the_authority_lock_hears_heartbeats() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let held = s.authority_lock().await.unwrap();
    let ((status, _, body), ()) = tokio::join!(enter(&rig, &ada, &setup, &code, None), async {
        // Past four cadences of the box's, the silence bound itself.
        tokio::time::sleep(SILENCE + Duration::from_secs(2)).await;
        drop(held);
    });
    let (heard, last) = the_box.await.unwrap();
    assert!(matches!(heard, Heard::Answer(_)), "the box heard {heard:?}");
    assert_eq!(last, Some(LastWord::Paired));
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(rows_named(s, "karl").await, 1);
}

/// **An exchange that cannot take the authority lock within the box's life
/// refuses the box `busy`** (Spec 8.2), rather than leaving it waiting, and
/// writes nothing.
///
/// Perturbation: wait for the lock past the box's life, and the entry is
/// still waiting when the test gives up.
#[tokio::test]
async fn a_box_whose_exchange_waits_past_its_life_is_refused_busy() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    rig.listener
        .set_pairing_bounds(crate::link::pairing::Bounds {
            life: Duration::from_secs(8),
            ..Default::default()
        });
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let held = s.authority_lock().await.unwrap();
    let entered = tokio::time::timeout(SOON, enter(&rig, &ada, &setup, &code, None)).await;
    drop(held);
    let (status, _, body) = entered.expect("the entry is answered within the box's life");
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert!(body.contains("refused busy"), "{body}");
    let (heard, _) = the_box.await.unwrap();
    assert!(
        matches!(&heard, Heard::Refused(reason) if reason == "busy"),
        "the box heard {heard:?}"
    );
    assert_eq!(rows_named(s, "karl").await, 0);
    assert!(s.setup(&setup).await.unwrap().unwrap().in_pairing_step());
}

/// **The box hears heartbeats while the pairing's write runs** (Spec 8.2,
/// step 6), so a write that waits on the store past four of its cadences
/// still reaches the box as `paired`.
///
/// Perturbation: send no heartbeat while the write runs, and the box hears
/// silence after its word and prints unknown.
#[tokio::test]
async fn a_box_whose_pairings_write_waits_hears_heartbeats_and_paired() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let mut dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = async {
        let heard = tokio::time::timeout(SOON, dialed.next()).await.unwrap();
        assert!(matches!(heard, Heard::Answer(_)), "the box heard {heard:?}");
        // Step 6 reads the setup's row under its lock, so it waits on this
        // one.
        let mut held = s.pool.begin().await.unwrap();
        sqlx::query("SELECT 1 FROM agent_setup WHERE setup_id = $1 FOR UPDATE")
            .bind(&setup)
            .execute(&mut *held)
            .await
            .unwrap();
        dialed.written().await.unwrap();
        let (last, ()) = tokio::join!(dialed.last_word(), async {
            tokio::time::sleep(SILENCE + Duration::from_secs(2)).await;
            held.rollback().await.unwrap();
        });
        last
    };
    let ((status, _, body), last) = tokio::join!(enter(&rig, &ada, &setup, &code, None), the_box);
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(last, LastWord::Paired);
}

/// **A pairing naming a live box and name is refused and retires
/// nothing** (Spec 8.2, step 1): the box hears `box_and_name_live`, the
/// live row keeps its credentials and its version, and no row is added.
///
/// Perturbation: write the pairing's row through registration's insert,
/// which retires the live row of the same box and name.
#[tokio::test]
async fn a_pairing_naming_a_live_box_and_name_is_refused_and_retires_nothing() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let live = registered(s, "box-a", "karl", &rig.authority).await;
    let before = s.agent(&live).await.unwrap().unwrap();
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let (status, _, body) = enter(&rig, &ada, &setup, &code, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains(r#"retire it on <a href="/admin/agents">the register</a>"#),
        "the register's retire is offered: {body}"
    );
    let (heard, _) = the_box.await.unwrap();
    assert!(
        matches!(&heard, Heard::Refused(reason) if reason == "box_and_name_live"),
        "{heard:?}"
    );
    let after = s.agent(&live).await.unwrap().unwrap();
    assert_eq!(
        after.gate.state,
        CredentialState::Live,
        "the live row was retired"
    );
    assert_eq!(after.admin.state, CredentialState::Live);
    assert_eq!(after.version, before.version);
    assert_eq!(rows_named(s, "karl").await, 1);
    assert!(s.setup(&setup).await.unwrap().unwrap().paired.is_none());
}

/// **A certificate is signed only over a request whose own signature
/// verifies** (Spec 8.2, step 2): a request naming the box's key and
/// carrying another key's signature is refused, the box told
/// `request_refused`, nothing signed and nothing written.
///
/// Perturbation: take the request's key without its signature's
/// verification, and the box is answered with a certificate over it.
#[tokio::test]
async fn a_request_whose_signature_is_another_keys_is_refused_at_the_exchange() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let mut keys = box_keys("box-a");
    keys.claim.admin_request = crate::link::request::tests::signed_by_another_key(&keys.admin);
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let (status, _, body) = enter(&rig, &ada, &setup, &code, None).await;
    let (heard, _) = the_box.await.unwrap();
    assert!(
        matches!(&heard, Heard::Refused(reason) if reason == "request_refused"),
        "a request whose signature is another key's was signed: {heard:?}"
    );
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(rows_named(s, "karl").await, 0);
    let (first, ok, _) = pair_audit(s, "pair").await;
    assert_eq!((first, ok), (1, 0), "the act is recorded and did not land");
}

/// **A request of another kind is refused at the exchange**: a P-384
/// request verifies under its own key and is still not the kind the
/// console signs.
#[tokio::test]
async fn a_request_of_another_kind_is_refused_at_the_exchange() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let mut keys = box_keys("box-a");
    keys.claim.gate_request =
        request_of(&KeyPair::generate_for(&rcgen::PKCS_ECDSA_P384_SHA384).unwrap());
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let (status, _, body) = enter(&rig, &ada, &setup, &code, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let (heard, _) = the_box.await.unwrap();
    assert!(
        matches!(&heard, Heard::Refused(reason) if reason == "request_refused"),
        "{heard:?}"
    );
    assert_eq!(rows_named(s, "karl").await, 0);
}

/// **A lost word writes no row** (Spec 8.2, step 5): a box that closes
/// before its word leaves no row, no link and a failed outcome.
///
/// Perturbation: write the row without the box's word, and a row stands
/// for an answer no box adopted.
#[tokio::test]
async fn a_lost_word_writes_no_row() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::NoWord));
    let (status, _, body) = enter(&rig, &ada, &setup, &code, None).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("nothing was written"), "{body}");
    assert!(
        !body.contains("The register's live rows holding this name"),
        "the page draws a row for a pair that wrote none: {body}"
    );
    let (heard, _) = the_box.await.unwrap();
    assert!(matches!(heard, Heard::Answer(_)), "{heard:?}");
    assert_eq!(
        rows_named(s, "karl").await,
        0,
        "a row stands for an answer no box adopted"
    );
    assert!(s.setup(&setup).await.unwrap().unwrap().paired.is_none());
    let failed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE action = 'pair' AND outcome = 'failed'",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(failed, 1);
}

/// **On unknown the pairing page shows what the write made** (Spec 8.2):
/// after a committed `replace box` whose last word the box never read, the
/// page shows the row's new box and the new certificates' fingerprints;
/// after a write that did not commit, the old box and its credentials, and
/// never the answer's fingerprints, which the box's unknown line carries
/// whether the write committed or not.
///
/// Perturbation: draw the answer's fingerprints in the rows' place.
#[tokio::test]
async fn on_unknown_the_pairing_page_shows_what_the_write_made() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let mut key = authenticator();
    let (ada_id, ada) = admin(&rig.app, s, &mut key, "ada").await;

    // A write that does not commit: the admin's session ends between the
    // answer and the box's word, so step 6's re-check refuses.
    let karl = registered(s, "box-old", "karl", &rig.authority).await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-new");
    let mut dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let store = s.clone();
    let person = ada_id.clone();
    let lost = tokio::spawn(async move {
        let heard = tokio::time::timeout(SOON, dialed.next()).await.unwrap();
        sqlx::query(
            "UPDATE session SET closed_at = now() WHERE person_id = $1 AND closed_at IS NULL",
        )
        .bind(&person)
        .execute(&store.pool)
        .await
        .unwrap();
        dialed.written().await.unwrap();
        (heard, dialed.last_word().await)
    });
    let (status, _, body) = enter(&rig, &ada, &setup, &code, Some(&karl)).await;
    let (heard, last) = lost.await.unwrap();
    assert_eq!(last, LastWord::Unknown, "the box heard a last word");
    let answer = answer_of(&heard);
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(body.contains("nothing was written"), "{body}");
    assert!(
        body.contains(&grouped(&"1".repeat(64))),
        "the old box's credential: {body}"
    );
    assert!(body.contains("box-old"), "{body}");
    assert!(
        !body.contains(&grouped(&fingerprint_of(&answer.gate))),
        "the page shows the answer's fingerprints: {body}"
    );
    let row = s.agent(&karl).await.unwrap().unwrap();
    assert_eq!(row.r#box, "box-old");

    // A committed replacement whose last word the box never read.
    let (status, headers, _) = crate::sign_in_tests::sign_in(&rig.app, &mut key, "ada").await;
    assert_eq!(status, StatusCode::OK);
    let ada = crate::sign_in_tests::bearer(&headers);
    let keys = box_keys("box-new");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::WordThenGone));
    let (status, _, body) = enter(&rig, &ada, &setup, &code, Some(&karl)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let (heard, _) = the_box.await.unwrap();
    let answer = answer_of(&heard);
    let pairing = page(&rig.app, &format!("/setup/{setup}"), &ada).await;
    assert!(
        pairing.contains(&grouped(&fingerprint_of(&answer.gate))),
        "{pairing}"
    );
    assert!(
        pairing.contains(&grouped(&fingerprint_of(&answer.admin))),
        "{pairing}"
    );
    assert!(pairing.contains("box-new"), "{pairing}");
    assert!(!pairing.contains(&grouped(&"1".repeat(64))), "{pairing}");
}

/// **A row's box changes only through the replace-box write, and the row
/// keeps the box it held** (Spec 2.12, 8.2): the replacement keeps the
/// row's identity, sets its box, revokes the old box's credentials and
/// keeps the old box in the history; any other write moving a box is
/// refused by the schema.
///
/// Perturbation: write the new box without the history's row, and the old
/// box leaves the record.
#[tokio::test]
async fn a_replacement_keeps_the_box_the_row_held() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let karl = registered(s, "box-old", "karl", &rig.authority).await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let pairing = page(&rig.app, &format!("/setup/{setup}"), &ada).await;
    assert!(
        pairing.contains(&format!(r#"name="replace" value="{}""#, karl.as_str())),
        "the page asks whether to replace the live row's box: {pairing}"
    );
    assert!(pairing.contains("that box's credentials end"), "{pairing}");
    let keys = box_keys("box-new");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let (status, _, body) = enter(&rig, &ada, &setup, &code, Some(&karl)).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let (heard, last) = the_box.await.unwrap();
    assert_eq!(last, Some(LastWord::Paired));
    let answer = answer_of(&heard);
    assert_eq!(answer.agent_id, karl.as_str(), "the row keeps its identity");
    let row = s.agent(&karl).await.unwrap().unwrap();
    assert_eq!(row.r#box, "box-new");
    assert_eq!(row.gate.fingerprint, fingerprint_of(&answer.gate));
    assert!(row.setup_running);
    let held = s.boxes_held(&karl).await.unwrap();
    assert_eq!(
        held.iter().map(|(b, _)| b.as_str()).collect::<Vec<_>>(),
        ["box-old"],
        "the old box left the record"
    );
    assert_eq!(rows_named(s, "karl").await, 1);
    assert_eq!(
        s.setup(&setup).await.unwrap().unwrap().paired,
        Some((karl.as_str().to_owned(), "replace box".to_owned()))
    );
    assert_eq!(pair_audit(s, "replace box").await, (1, 1, 0));
    let moved = sqlx::query("UPDATE agent SET box = 'box-other' WHERE agent_id = $1")
        .bind(karl.as_str())
        .execute(&s.pool)
        .await;
    assert!(moved.is_err(), "a box moved outside the replace-box write");
}

// ---------------------------------------------------------------------------
// The commands on the box (Spec 8.2, 8.3): `request`, `pair` and `adopt`
// against the real listener with the code entered through the page, and the
// setup connection's two asks under the config `adopt` completed.
// ---------------------------------------------------------------------------

use crate::link::box_commands::{self, PairArgs};
use crate::link::box_line::{Printer, Status};
use crate::link::client::{Asked, Connect, Link, LinkConfig};
use crate::link::frames::{REPORT_BOUND, Refusal, SetupAsk, Stage};
use crate::link::key_config::AnswerFile;

/// A printer's standard output, sent line by line to the test while the
/// command runs.
struct Piped {
    lines: tokio::sync::mpsc::UnboundedSender<String>,
    partial: Vec<u8>,
}

impl std::io::Write for Piped {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for b in buf {
            if *b == b'\n' {
                let line = String::from_utf8(std::mem::take(&mut self.partial)).unwrap();
                let _ = self.lines.send(line);
            } else {
                self.partial.push(*b);
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn captured() -> Printer<Vec<u8>, Vec<u8>> {
    Printer::new("test", Vec::new(), Vec::new())
}

/// The command's one line on standard output, parsed, and its standard
/// error.
fn outcome_of(p: Printer<Vec<u8>, Vec<u8>>) -> (serde_json::Value, String) {
    let (out, err) = p.into_inner();
    let out = String::from_utf8(out).unwrap();
    let mut lines = out.lines();
    let line = serde_json::from_str(lines.next().expect("a line")).unwrap();
    assert_eq!(lines.next(), None, "one line: {out}");
    (line, String::from_utf8(err).unwrap())
}

/// **The box's three steps**, in `dir`: `request` for each connector, `pair`
/// against the rig's listener with its code entered through the page, and
/// `adopt` for each. Answers the answer `pair` wrote.
async fn enrolled_on_box(
    rig: &impl Enrolling,
    session: &str,
    setup: &str,
    dir: &std::path::Path,
    r#box: &str,
    replace: Option<&AgentId>,
) -> AnswerFile {
    for plane in [Plane::Gate, Plane::Admin] {
        let mut p = captured();
        let config = dir.join(format!("{}-con.toml", plane.as_str()));
        assert_eq!(box_commands::request(&mut p, plane, &config), Status::Done);
        let (out, _) = p.into_inner();
        std::fs::write(dir.join(format!("{}.request", plane.as_str())), out).unwrap();
    }
    let args = PairArgs {
        console: rig.listener().address().to_string(),
        gate_request: dir.join("gate.request"),
        admin_request: dir.join("admin.request"),
        answer: dir.join("answer.json"),
        r#box: Some(r#box.into()),
    };
    let (lines, mut heard) = tokio::sync::mpsc::unbounded_channel();
    let pair = tokio::spawn(async move {
        let mut p = Printer::new(
            "admin-con pair",
            Piped {
                lines,
                partial: Vec::new(),
            },
            Vec::new(),
        );
        box_commands::pair(&mut p, &args).await
    });
    let waiting: serde_json::Value = serde_json::from_str(
        &tokio::time::timeout(SOON, heard.recv())
            .await
            .expect("the box waits")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(waiting["event"], "waiting", "{waiting}");
    assert_eq!(
        waiting["fingerprint"],
        grouped(&rig.authority().fingerprint()),
        "the box shows the console's authority"
    );
    assert!(waiting["expires_in_secs"].as_u64().unwrap() > 0);
    let code = waiting["code"].as_str().unwrap().to_owned();
    let (status, _, body) = enter(rig, session, setup, &code, replace).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let ended = tokio::time::timeout(SOON, pair).await.unwrap().unwrap();
    assert_eq!(ended, Status::Done);
    let paired: serde_json::Value = serde_json::from_str(&heard.recv().await.unwrap()).unwrap();
    assert_eq!(paired["event"], "paired", "{paired}");
    assert_eq!(
        (paired["agent"].as_str(), paired["box"].as_str()),
        (Some("karl"), Some(r#box))
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir.join("answer.json"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o7777, 0o644, "the answer holds certificates alone");
    let answer = AnswerFile::read(&dir.join("answer.json")).unwrap();
    assert_eq!(paired["agent_id"].as_str(), Some(answer.agent_id.as_str()));
    for plane in [Plane::Gate, Plane::Admin] {
        let mut p = captured();
        let config = dir.join(format!("{}-con.toml", plane.as_str()));
        let status = box_commands::adopt(&mut p, plane, &config, &dir.join("answer.json"));
        let (line, err) = outcome_of(p);
        assert_eq!(status, Status::Done, "{line} {err}");
        assert_eq!(line["event"], "adopted");
        assert_eq!(line["plane"], plane.as_str());
    }
    answer
}

fn link_of(config: &std::path::Path) -> Link {
    let cfg: LinkConfig = toml::from_str(&std::fs::read_to_string(config).unwrap()).unwrap();
    Link::new(cfg).unwrap()
}

/// A connector's hello: admin-con's with its trace door closed and a
/// ceiling naming no verb, so the listener asks it nothing.
fn hello(plane: Plane) -> FromClient {
    let admin = plane == Plane::Admin;
    FromClient::Hello {
        agent: "karl".into(),
        plane,
        tail: None,
        door: admin.then_some(false),
        ceiling: admin.then(Vec::new),
        capabilities: None,
    }
}

async fn describe(config: &std::path::Path) -> (Status, serde_json::Value) {
    let mut p = captured();
    let status = box_commands::setup_describe(&mut p, config).await;
    let (line, err) = outcome_of(p);
    if status != Status::Done {
        eprintln!("setup describe: {line} {err}");
    }
    (status, line)
}

async fn report(
    config: &std::path::Path,
    stage: Stage,
    text: &[u8],
) -> (Status, serde_json::Value) {
    let mut p = captured();
    let status = box_commands::setup_report(&mut p, config, stage, text).await;
    let (line, err) = outcome_of(p);
    if status != Status::Done {
        eprintln!("setup report: {line} {err}");
    }
    (status, line)
}

/// A setup set confirmed at the schema, as Confirm leaves it. Confirm's
/// own path is `setup_place_tests`'.
async fn confirmed_at_the_schema(s: &Store, setup: &str) {
    sqlx::query("UPDATE agent_setup SET state = 'confirmed' WHERE setup_id = $1")
        .bind(setup)
        .execute(&s.pool)
        .await
        .unwrap();
}

/// **An adopted pair connects** (Spec 8.2): `request` for each connector,
/// `pair` against the real listener with the code the box's `waiting` line
/// shows entered through the page, `adopt` for each, and both connectors
/// are admitted on the one row the pairing wrote, which reads present.
#[tokio::test]
async fn an_adopted_pair_connects() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    let answer = enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let id: AgentId = answer.agent_id.parse().unwrap();
    let mut connections = Vec::new();
    for plane in [Plane::Gate, Plane::Admin] {
        let link = link_of(&on_box.path().join(format!("{}-con.toml", plane.as_str())));
        match link.connect(hello(plane)).await {
            Connect::Admitted(connection) => connections.push(connection),
            Connect::Refused(r) => panic!("{plane:?} refused: {}", r.as_str()),
            Connect::Untrusted(why) | Connect::Failed(why) => panic!("{plane:?}: {why}"),
        }
    }
    let row = s.agent(&id).await.unwrap().unwrap();
    assert!(row.present(), "both connectors on the one row");
    assert_eq!(rows_named(s, "karl").await, 1);
    for connection in connections {
        connection.close().await;
    }
}

/// **A replacement's drive setup mounts the state drive it carries** (Spec
/// 8.3): `setup describe` answers `replacement` false after a `pair` and
/// true after a `replace box`, under the replacement's own credential,
/// and the replaced box's credential is refused.
///
/// Perturbation: answer false after a replacement, and the carried drive
/// would go to the format.
#[tokio::test]
async fn setup_describe_answers_replacement_after_replace_box_alone() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, first) = begin(&rig.app, &ada, "karl").await;
    let first = first.unwrap();
    let old_box = tempfile::tempdir().unwrap();
    let answer = enrolled_on_box(&rig, &ada, &first, old_box.path(), "box-a", None).await;
    let old_admin = old_box.path().join("admin-con.toml");
    let (status, described) = describe(&old_admin).await;
    assert_eq!(status, Status::Done, "{described}");
    assert_eq!(described["event"], "described");
    assert_eq!(described["replacement"], false);
    assert_eq!(described["agent"], "karl");
    assert_eq!(described["agent_id"], answer.agent_id.as_str());
    assert_eq!(described["bytes"], 0);
    assert!(described.get("models").is_none(), "{described}");
    assert!(described.get("laying").is_none(), "{described}");

    confirmed_at_the_schema(s, &first).await;
    let (_, _, second) = begin(&rig.app, &ada, "karl").await;
    let second = second.unwrap();
    let karl: AgentId = answer.agent_id.parse().unwrap();
    let new_box = tempfile::tempdir().unwrap();
    let replaced = enrolled_on_box(&rig, &ada, &second, new_box.path(), "box-b", Some(&karl)).await;
    assert_eq!(
        replaced.agent_id, answer.agent_id,
        "the row keeps its identity"
    );
    let (status, described) = describe(&new_box.path().join("admin-con.toml")).await;
    assert_eq!(status, Status::Done, "{described}");
    assert_eq!(described["replacement"], true);
    let (status, refused) = describe(&old_admin).await;
    assert_eq!(status, Status::Refused, "{refused}");
}

/// **A setup ask is refused while its credential's connection stands, and
/// counts for nothing in presence** (Spec 8.3): two asks in turn are each
/// answered, the row never reading the admin plane connected, and an ask
/// beside the standing admin connection is refused `already_connected`.
///
/// Perturbation: drop the live connection from the setup's admission, and
/// the ask beside it is answered.
#[tokio::test]
async fn a_setup_ask_beside_a_standing_connection_is_refused_and_counts_for_nothing() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    let answer = enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let id: AgentId = answer.agent_id.parse().unwrap();
    let admin_config = on_box.path().join("admin-con.toml");
    for _ in 0..2 {
        let (status, line) = describe(&admin_config).await;
        assert_eq!(status, Status::Done, "{line}");
        let row = s.agent(&id).await.unwrap().unwrap();
        assert!(
            !row.admin.connected,
            "a setup connection is no admin connection"
        );
        assert_eq!(row.admin.link_at, None);
        assert!(!row.present());
        assert!(row.setup_running, "a setup ask is no first admission");
    }
    let Connect::Admitted(standing) = link_of(&admin_config).connect(hello(Plane::Admin)).await
    else {
        panic!("the admin connection is admitted");
    };
    let (status, line) = describe(&admin_config).await;
    assert_eq!(status, Status::Refused, "{line}");
    assert_eq!(line["reason"], "already_connected");
    let (status, line) = report(&admin_config, Stage::Pairing, b"{}").await;
    assert_eq!(status, Status::Refused, "{line}");
    assert_eq!(line["reason"], "already_connected");
    assert!(s.setup_reports(&setup).await.unwrap().is_empty());
    standing.close().await;
}

/// **The setup keeps the latest report of each stage, verbatim** (Spec
/// 8.3): a pairing report, a drives report, and the pairing report run
/// again, which replaces the pairing's and leaves the drives'. A report past
/// the bound is refused `too_large` on the box before any dial and at the
/// listener too, a confirmed setup's `setup_closed`, and a row no pairing
/// linked `no_setup`.
///
/// Perturbation: keep one report per setup, and the pairing report run
/// again replaces the drives page's.
#[tokio::test]
async fn the_setup_keeps_the_latest_report_of_each_stage_verbatim() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    let answer = enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let admin_config = on_box.path().join("admin-con.toml");
    let first = "{ \"cards\" : [1,  2] }\n";
    let drives = "{\"disks\":[{\"model\":\"\\u00e9\"}]}";
    let again = "{\"cards\":[]}";
    for (stage, text) in [
        (Stage::Pairing, first),
        (Stage::Drives, drives),
        (Stage::Pairing, again),
    ] {
        let (status, line) = report(&admin_config, stage, text.as_bytes()).await;
        assert_eq!(status, Status::Done, "{line}");
        assert_eq!(line["event"], "accepted");
    }
    let kept: Vec<(String, String)> = s
        .setup_reports(&setup)
        .await
        .unwrap()
        .into_iter()
        .map(|(stage, text, _)| (stage, text))
        .collect();
    assert_eq!(
        kept,
        [
            ("pairing".into(), again.into()),
            ("drives".into(), drives.into())
        ]
    );

    // Past the bound: refused on the box, and at the listener for a client
    // that skips the box's check, the kept reports unchanged.
    let large = format!("{{\"x\":\"{}\"}}", "a".repeat(REPORT_BOUND));
    let (status, line) = report(&admin_config, Stage::Drives, large.as_bytes()).await;
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("too_large"))
    );
    let asked = link_of(&admin_config)
        .ask(FromClient::Setup {
            agent: "karl".into(),
            plane: Plane::Admin,
            ask: SetupAsk::Report {
                stage: Stage::Drives,
                report: large,
            },
        })
        .await;
    assert!(
        matches!(
            asked,
            Asked::Answer(ToClient::Refusal {
                reason: Refusal::TooLarge
            })
        ),
        "the listener refuses a report past the bound"
    );
    let (status, line) = report(&admin_config, Stage::Drives, b"[1]").await;
    assert_eq!(
        (status, line["event"].as_str()),
        (Status::Usage, Some("usage"))
    );
    assert_eq!(s.setup_reports(&setup).await.unwrap().len(), 2);

    // A confirmed setup never changes, and its describe still answers.
    confirmed_at_the_schema(s, &setup).await;
    let (status, line) = report(&admin_config, Stage::Drives, b"{}").await;
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("setup_closed"))
    );
    let (status, line) = describe(&admin_config).await;
    assert_eq!(status, Status::Done, "{line}");
    assert_eq!(line["agent_id"], answer.agent_id.as_str());

    // A row the host's `register` wrote has no setup.
    let out = tempfile::tempdir().unwrap();
    let cfg = crate::agents_tests::server_config(rig._dir.path(), rig.listener.address());
    let registered =
        crate::link::verbs::register(s, &cfg, &rig.authority, "box-z", "zed", out.path(), None)
            .await;
    assert!(registered.ok, "{}", registered.value);
    let zed = out.path().join("box-z").join("zed").join("admin-con.toml");
    let (status, line) = describe(&zed).await;
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("no_setup"))
    );
    let (status, line) = report(&zed, Stage::Pairing, b"{}").await;
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("no_setup"))
    );
}

/// **A `pair` whose `waiting` line is not delivered ends at once**: its code
/// cannot be read, so nobody can enter it, and `pair` answers `fault`, writes
/// no answer and closes, the console holding no box for it, rather than
/// waiting out the box's ten minutes.
///
/// Perturbation: ignore the `waiting` line's failure, and `pair` is still
/// waiting when the test gives up.
#[tokio::test]
async fn a_pair_whose_waiting_line_is_not_delivered_ends_at_once() {
    let Some(rig) = rig().await else { return };
    let dir = tempfile::tempdir().unwrap();
    for plane in [Plane::Gate, Plane::Admin] {
        let mut p = captured();
        let config = dir.path().join(format!("{}-con.toml", plane.as_str()));
        assert_eq!(box_commands::request(&mut p, plane, &config), Status::Done);
        let (out, _) = p.into_inner();
        std::fs::write(dir.path().join(format!("{}.request", plane.as_str())), out).unwrap();
    }
    let args = PairArgs {
        console: rig.listener.address().to_string(),
        gate_request: dir.path().join("gate.request"),
        admin_request: dir.path().join("admin.request"),
        answer: dir.path().join("answer.json"),
        r#box: Some("box-a".into()),
    };
    let refusing = crate::link::box_line::tests::Refusing { at_flush: false };
    let mut p = Printer::new("admin-con pair", refusing, Vec::new());
    let status = tokio::time::timeout(SOON, box_commands::pair(&mut p, &args))
        .await
        .expect("pair ends at once, not at the box's life");
    assert_eq!(status, Status::Fault);
    let (_, err) = p.into_inner();
    let err = String::from_utf8(err).unwrap();
    assert!(err.contains("so its code cannot be read"), "{err}");
    assert!(
        !dir.path().join("answer.json").exists(),
        "no answer written"
    );
    let by = tokio::time::Instant::now() + SOON;
    while !rig.listener.waiting().is_empty() {
        assert!(
            tokio::time::Instant::now() < by,
            "the console still holds the box"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// **The fifth miss closes the field for ten minutes from the miss** (Spec
/// 8.2), not from the moment its entry began waiting: an entry that holds
/// the field two seconds before its miss still leaves the field closed ten
/// minutes past the store's clock at the miss.
///
/// Perturbation: take the transaction's start, `now()`, as the miss's
/// time, and the closure ends two seconds short.
#[tokio::test]
async fn the_fifth_miss_closes_the_field_for_ten_minutes_from_the_miss() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    sqlx::query("UPDATE agent_setup SET code_misses = 4 WHERE setup_id = $1")
        .bind(&setup)
        .execute(&s.pool)
        .await
        .unwrap();
    let (field, state) = s.code_field(&setup).await.unwrap();
    assert!(matches!(state, crate::store::setup::Field::Open));
    tokio::time::sleep(Duration::from_secs(2)).await;
    let at_the_miss: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert!(
        field.missed().await.unwrap(),
        "the fifth miss closes the field"
    );
    let until: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT code_closed_until FROM agent_setup WHERE setup_id = $1")
            .bind(&setup)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert!(
        until >= at_the_miss + chrono::Duration::minutes(10),
        "closed until {until}, the miss at {at_the_miss}"
    );
}

/// **The field is read as it stands once its lock is held**: a closure
/// that ends while another transaction holds the setup's row reads as open
/// to the entry that waited for it. The test waits until the entry's
/// transaction is seen waiting on the row's lock, reads that it began
/// before the closure ended, and lets the row go once the closure has
/// passed, so the transaction's start falls inside the closure.
///
/// Perturbation: read the closure against the transaction's start, `now()`,
/// and the waiting entry reads the field closed.
#[tokio::test]
async fn a_closure_that_ends_while_the_entry_waits_reads_open() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let mut holder = s.pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE agent_setup SET code_closed_until = clock_timestamp() + interval '2 seconds' \
         WHERE setup_id = $1",
    )
    .bind(&setup)
    .execute(&mut *holder)
    .await
    .unwrap();
    holder.commit().await.unwrap();
    let mut holder = s.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM agent_setup WHERE setup_id = $1 FOR UPDATE")
        .bind(&setup)
        .execute(&mut *holder)
        .await
        .unwrap();
    let waiting = {
        let (s, setup) = (s.clone(), setup.clone());
        tokio::spawn(async move { s.code_field(&setup).await.unwrap() })
    };
    let began_inside = tokio::time::timeout(SOON, async {
        loop {
            let began: Option<bool> = sqlx::query_scalar(
                "SELECT a.xact_start < s.code_closed_until \
                 FROM pg_stat_activity a, agent_setup s \
                 WHERE a.datname = current_database() AND a.wait_event_type = 'Lock' \
                 AND s.setup_id = $1",
            )
            .bind(&setup)
            .fetch_optional(&s.pool)
            .await
            .unwrap();
            if let Some(began) = began {
                return began;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the entry waits on the setup's row");
    assert!(
        began_inside,
        "the entry's transaction began after the closure ended"
    );
    loop {
        let passed: bool = sqlx::query_scalar(
            "SELECT clock_timestamp() > code_closed_until FROM agent_setup WHERE setup_id = $1",
        )
        .bind(&setup)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        if passed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    holder.rollback().await.unwrap();
    let (field, state) = tokio::time::timeout(SOON, waiting).await.unwrap().unwrap();
    assert!(
        matches!(state, crate::store::setup::Field::Open),
        "the closure ended while the entry waited"
    );
    field.release().await.unwrap();
}

/// **The authority lock is waited for by the box's end alone** (Spec 8.2):
/// one deadline spans the pool's checkout and the store's wait, so an end
/// already past answers no lock though the lock is free, and the box is
/// refused `busy` rather than taken past its life. An end ahead takes the
/// free lock.
///
/// Perturbation: drop the end's check after the checkout, and a free lock
/// is taken past the end.
#[tokio::test]
async fn the_authority_lock_is_not_taken_past_the_boxs_end() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let past = tokio::time::Instant::now() - Duration::from_millis(1);
    assert!(
        s.authority_lock_by(past).await.unwrap().is_none(),
        "a free lock was taken past the box's end"
    );
    // A checkout ready at its first poll, an idle connection taken with no
    // ping, is handed over by tokio's bound although the end has passed:
    // the end read once the connection is held answers it.
    let ready = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .test_before_acquire(false)
        .connect_with((*s.pool.connect_options()).clone())
        .await
        .unwrap();
    drop(ready.acquire().await.unwrap());
    let prompt = Store {
        pool: ready.clone(),
    };
    let past = tokio::time::Instant::now() - Duration::from_millis(1);
    assert!(
        prompt.authority_lock_by(past).await.unwrap().is_none(),
        "a connection ready at once took a free lock past the box's end"
    );
    ready.close().await;
    let ahead = tokio::time::Instant::now() + SOON;
    let lock = s.authority_lock_by(ahead).await.unwrap();
    assert!(lock.is_some(), "a free lock within the end");
    drop(lock);

    // A pool with no connection to give answers by the end, not at the
    // pool's own 30 s.
    let one = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with((*s.pool.connect_options()).clone())
        .await
        .unwrap();
    let held = one.acquire().await.unwrap();
    let narrow = Store { pool: one.clone() };
    let end = tokio::time::Instant::now() + Duration::from_millis(300);
    let answered = tokio::time::timeout(SOON, narrow.authority_lock_by(end))
        .await
        .expect("the checkout is bounded by the box's end");
    assert!(answered.unwrap().is_none(), "no connection, no lock");
    drop(held);
    one.close().await;
}

/// `setup report` bounded at `bound`, its status and line.
async fn report_within(
    config: &std::path::Path,
    text: &[u8],
    bound: Duration,
) -> (Status, serde_json::Value) {
    let mut p = captured();
    let status =
        box_commands::setup_report_within(&mut p, config, Stage::Pairing, text, bound).await;
    let (line, err) = outcome_of(p);
    if status != Status::Done {
        eprintln!("setup report: {line} {err}");
    }
    (status, line)
}

/// **A setup ask the store does not answer within its bound is refused
/// `store_unavailable` and lets its claim go** (Spec 8.3), as every store
/// wait on a connection's path is bounded: a report held behind the
/// setup's row, locked by another transaction, and a describe whose store
/// call stalls are each answered at the bound, well within the box's own,
/// and the next ask, the hold gone, is answered, not refused
/// `already_connected`. The report held past the bound lands nothing, read
/// once the hold is gone and before the next report replaces it.
///
/// Perturbations: await the store with no bound, and the box gives up
/// first, the console still holding the claim. Land the report outside its
/// transaction before its checks, and the held report lands once the hold
/// goes, though its ask was refused.
#[tokio::test]
async fn a_setup_ask_the_store_holds_past_the_bound_is_refused_and_lets_its_claim_go() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");
    let the_box = SILENCE * 3;

    let mut holder = s.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM agent_setup WHERE setup_id = $1 FOR UPDATE")
        .bind(&setup)
        .execute(&mut *holder)
        .await
        .unwrap();
    let (status, line) = report_within(&config, br#"{"held": true}"#, the_box).await;
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("store_unavailable")),
        "{line}"
    );
    holder.rollback().await.unwrap();
    // The held report never sent its commit: a settle after the hold goes,
    // the setup holds no report, read before the next ask replaces it.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let held: i64 =
        sqlx::query_scalar("SELECT count(*) FROM agent_setup_report WHERE setup_id = $1")
            .bind(&setup)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(held, 0, "the report held past the bound landed");
    let (status, line) = report_within(&config, br#"{"after": true}"#, the_box).await;
    assert_eq!(status, Status::Done, "{line}");
    let kept: Vec<String> =
        sqlx::query_scalar("SELECT report FROM agent_setup_report WHERE setup_id = $1")
            .bind(&setup)
            .fetch_all(&s.pool)
            .await
            .unwrap();
    assert_eq!(
        kept,
        [r#"{"after": true}"#],
        "the held report landed nothing"
    );

    rig.listener.stall_next_setup_ask();
    let mut p = captured();
    let status = box_commands::setup_describe_within(&mut p, &config, the_box).await;
    let (line, _) = outcome_of(p);
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("store_unavailable")),
        "{line}"
    );
    let (status, line) = describe(&config).await;
    assert_eq!(
        (status, line["event"].as_str()),
        (Status::Done, Some("described"))
    );
}

/// **An entry whose request is dropped once its code matched still pairs
/// the box** (Spec 8.2): the match, its first record, the exchange and the
/// outcome run in one task a dropped request cannot cut. The request is
/// dropped while its first record is held, and the box still hears its
/// answer and `paired`, one row standing for the name.
///
/// Perturbation: run the continuation from the match in the request's own
/// future, and the dropped request takes the box's connection with it.
#[tokio::test]
async fn an_entry_dropped_once_its_code_matched_still_pairs_the_box() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (person, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let the_box = tokio::spawn(the_box(dialed, Box::Whole));
    let (read, release) = crate::admin_tests::hold(&crate::surfaces::admin::READ_HOLD, &person);
    let entry = {
        let (app, session, setup) = (rig.app.clone(), ada.clone(), setup.clone());
        tokio::spawn(async move {
            send_as(
                &app,
                "POST",
                &format!("/setup/{setup}/pair"),
                Some(form(&[("compared", "on"), ("code", &code)])),
                &session,
            )
            .await
        })
    };
    tokio::time::timeout(SOON, read.notified())
        .await
        .expect("the entry matched and reached its first record");
    entry.abort();
    assert!(entry.await.unwrap_err().is_cancelled());
    release.notify_one();
    let (heard, last) = tokio::time::timeout(SOON, the_box).await.unwrap().unwrap();
    assert!(matches!(heard, Heard::Answer(_)), "{heard:?}");
    assert!(matches!(last, Some(LastWord::Paired)), "{last:?}");
    assert_eq!(rows_named(s, "karl").await, 1);
}

/// **One deadline from the setup frame spans the admission and the store's
/// answer, at the silence bound or `SETUP_ANSWER_SECS`, whichever is
/// shorter** (Spec 8.3), so the box hears the refusal before it gives the
/// ask up: a describe whose admission waits on the agent's row, held by
/// another transaction, is refused `store_unavailable` within a box's
/// bound shorter than the silence bound, the defaults' shape at a test's
/// scale, and the next ask, the hold gone, is answered.
///
/// Perturbations: run the admission outside the deadline, and the box gives
/// up first. Bound the ask by the silence bound alone, and the box gives up
/// first.
#[tokio::test]
async fn a_setup_ask_whose_admission_waits_is_refused_within_the_boxs_bound() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    let answer = enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");
    rig.listener.set_setup_answer_bound(Duration::from_secs(1));
    let the_box = SILENCE / 2;

    let mut holder = s.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM agent WHERE agent_id = $1 FOR UPDATE")
        .bind(answer.agent_id.as_str())
        .execute(&mut *holder)
        .await
        .unwrap();
    let mut p = captured();
    let status = box_commands::setup_describe_within(&mut p, &config, the_box).await;
    let (line, _) = outcome_of(p);
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("store_unavailable")),
        "{line}"
    );
    holder.rollback().await.unwrap();
    let (status, line) = describe(&config).await;
    assert_eq!(
        (status, line["event"].as_str()),
        (Status::Done, Some("described")),
        "{line}"
    );
}

/// Every connection of the store's pool that comes free within 300 ms,
/// held, so the listener's next checkout waits until they are given back.
async fn hold_the_pool(s: &Store) -> Vec<sqlx::pool::PoolConnection<sqlx::Postgres>> {
    let mut held = Vec::new();
    while let Ok(c) = tokio::time::timeout(Duration::from_millis(300), s.pool.acquire()).await {
        held.push(c.unwrap());
    }
    assert!(!held.is_empty(), "the pool had a free connection to hold");
    held
}

/// **The setup ask's deadline is made at the accept and bounds the
/// register's lookup** (Spec 8 and 8.3): the box writes its frame once its
/// handshake completes, so a lookup that waits spends the box's own bound.
/// A describe whose lookup waits for a connection, every connection of the
/// store's pool held past the box's bound, is refused `store_unavailable`
/// within that bound, and the next ask, the pool given back, is answered.
///
/// Perturbations: make the deadline after the lookup, the lookup unbounded,
/// and the box gives up first. Leave the deadline at the accept and run the
/// lookup outside it, and the box gives up first.
#[tokio::test]
async fn a_setup_ask_whose_lookup_waits_is_refused_within_the_boxs_bound() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");
    rig.listener.set_setup_answer_bound(Duration::from_secs(1));
    let the_box = SILENCE / 2;

    let held = hold_the_pool(s).await;
    let gives_back = tokio::spawn(async move {
        tokio::time::sleep(the_box + Duration::from_millis(500)).await;
        drop(held);
    });
    let mut p = captured();
    let status = box_commands::setup_describe_within(&mut p, &config, the_box).await;
    let (line, _) = outcome_of(p);
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("store_unavailable")),
        "{line}"
    );
    gives_back.await.unwrap();
    let (status, line) = describe(&config).await;
    assert_eq!(
        (status, line["event"].as_str()),
        (Status::Done, Some("described")),
        "{line}"
    );
}

/// **The lookup's wait and the admission's share the one deadline** (Spec
/// 8.3): a describe whose lookup waits for a connection, the store's pool
/// given back at 0.8 s, and whose admission then waits on the agent's row,
/// held by another transaction, is refused `store_unavailable` at the
/// deadline from the accept, within the box's bound of 1.4 s, and the next
/// ask, the holds gone, is answered.
///
/// Perturbation: make the admission's deadline after the lookup, and the
/// refusal comes at about 1.8 s, after the box has given up.
#[tokio::test]
async fn a_setup_ask_whose_lookup_and_admission_wait_is_refused_within_the_boxs_bound() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    let answer = enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");
    rig.listener.set_setup_answer_bound(Duration::from_secs(1));
    let the_box = Duration::from_millis(1400);

    let mut holder = s.pool.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM agent WHERE agent_id = $1 FOR UPDATE")
        .bind(answer.agent_id.as_str())
        .execute(&mut *holder)
        .await
        .unwrap();
    let held = hold_the_pool(s).await;
    let gives_back = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(800)).await;
        drop(held);
    });
    let mut p = captured();
    let status = box_commands::setup_describe_within(&mut p, &config, the_box).await;
    let (line, _) = outcome_of(p);
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("store_unavailable")),
        "{line}"
    );
    gives_back.await.unwrap();
    holder.rollback().await.unwrap();
    let (status, line) = describe(&config).await;
    assert_eq!(
        (status, line["event"].as_str()),
        (Status::Done, Some("described")),
        "{line}"
    );
}

/// **A lookup that fails is refused `store_unavailable`** (Spec 8 and
/// 8.3): the box hears a definite refusal, not a connection that ends, and
/// the next ask is answered.
///
/// Perturbation: end the connection with no refusal where the register
/// cannot be read, and the box reads `fault`.
#[tokio::test]
async fn a_setup_ask_whose_lookup_fails_is_refused_store_unavailable() {
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");

    rig.listener.fail_next_lookup();
    let (status, line) = describe(&config).await;
    assert_eq!(
        (status, line["reason"].as_str()),
        (Status::Refused, Some("store_unavailable")),
        "{line}"
    );
    let (status, line) = describe(&config).await;
    assert_eq!(
        (status, line["event"].as_str()),
        (Status::Done, Some("described")),
        "{line}"
    );
}

/// **An entry whose request is dropped still logs what the box was
/// answered** (Spec 8.2): the exchange's diagnostics are written in the
/// task that runs it, not in the request's answer. The request is dropped
/// while the matched entry's first record is held, the box closes before
/// its word, and the console's log names the two fingerprints the box was
/// answered with and no row confirmed.
///
/// Perturbation: write the diagnostics in the request's answer, and a
/// dropped request logs nothing of them.
#[tokio::test]
async fn an_entry_dropped_once_matched_still_logs_what_the_box_was_answered() {
    let logs = crate::agents_tests::Captured::default();
    let _logging = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish(),
    );
    let Some(rig) = rig().await else { return };
    let s = rig.store();
    let (person, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let keys = box_keys("box-a");
    let dialed = dial(&rig, &keys.claim).await;
    let code = dialed.code().to_string();
    let mark = logs.mark();
    let the_box = tokio::spawn(the_box(dialed, Box::NoWord));
    let (read, release) = crate::admin_tests::hold(&crate::surfaces::admin::READ_HOLD, &person);
    let entry = {
        let (app, session, setup) = (rig.app.clone(), ada.clone(), setup.clone());
        tokio::spawn(async move {
            send_as(
                &app,
                "POST",
                &format!("/setup/{setup}/pair"),
                Some(form(&[("compared", "on"), ("code", &code)])),
                &session,
            )
            .await
        })
    };
    tokio::time::timeout(SOON, read.notified())
        .await
        .expect("the entry matched and reached its first record");
    entry.abort();
    assert!(entry.await.unwrap_err().is_cancelled());
    release.notify_one();
    let (heard, _) = tokio::time::timeout(SOON, the_box).await.unwrap().unwrap();
    assert!(matches!(heard, Heard::Answer(_)), "{heard:?}");
    assert!(
        logs.logged(
            mark,
            &[
                setup.as_str(),
                "ended with no row confirmed",
                "the box was answered gate"
            ],
            SOON
        )
        .await,
        "the answered fingerprints were not logged"
    );
    assert_eq!(rows_named(s, "karl").await, 0);
}

/// A report sent by the box's `setup report` through the real listener,
/// accepted.
async fn sent(config: &std::path::Path, stage: Stage, text: &str) {
    let (status, line) = report(config, stage, text.as_bytes()).await;
    assert_eq!(status, Status::Done, "{line}");
    assert_eq!(line["event"], "accepted", "{line}");
}

/// A report's text with one edit made.
fn edited_report(text: String, edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    edit(&mut value);
    value.to_string()
}

async fn version_of(s: &Store, setup: &str) -> String {
    s.setup(setup).await.unwrap().unwrap().version.to_string()
}

/// **The reports a box sends feed the setup's pages, end to end** (Spec
/// 6.2, 8.3): a box enrolled against the real listener sends `setup report`
/// at each stage through it, and the pages read the kept reports through
/// the survey's reader. The placement page offers the pairing report's one
/// card, then the drives report's two. Confirm holds for a drive not
/// mounted, for a state drive one byte under 9 GiB of filesystem, and for a
/// kept device a later report lacks, naming each, and its post writes
/// nothing; the drives page draws the first two on the drive, and the
/// placement page names the lacking card. Once the drives report stands,
/// its models drive short of the picks' need a warning on the drives page
/// and no hold, Confirm writes the tuple with the box's members typed as
/// ruled.
#[tokio::test]
async fn the_reports_a_box_sends_feed_placement_and_confirm() {
    let cache = tempfile::tempdir().unwrap();
    let (_fake, base) = crate::setup_models_tests::fake().await;
    let Some(rig) = crate::setup_tests::rig_full_with(
        None,
        Some(cache.path().to_path_buf()),
        &base,
        Arc::new(crate::box_report::Kept),
    )
    .await
    else {
        return;
    };
    let s = rig.store();
    let _identities = crate::hugging_face::base_identity::BaseIdentities::start(
        s.clone(),
        rig.puller.cache().cloned(),
        rig.puller.verified(),
    );
    let (ada, setup) = crate::setup_models_tests::setup_of(&rig, "karl").await;
    let on_box = tempfile::tempdir().unwrap();
    enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let admin_config = on_box.path().join("admin-con.toml");
    let placement = format!("/setup/{setup}/placement");
    let confirm = format!("/setup/{setup}/confirm");
    let action = format!(r#"<form method="post" action="{confirm}">"#);
    let read = |uri: String| {
        let app = rig.app.clone();
        let ada = ada.clone();
        async move { crate::setup_place_tests::read(&app, &uri, &ada).await }
    };
    let card =
        |n: u32, model: &str| format!(r#"name="device" value="{n}"> Card {n}: {model}</label>"#);

    // The pairing stage's report, one card: the placement page offers it.
    let pairing = edited_report(crate::setup_place_tests::pairing_report(), |v| {
        v["cards"] = serde_json::json!([v["cards"][0].clone()]);
    });
    sent(&admin_config, Stage::Pairing, &pairing).await;
    let offered = read(placement.clone()).await;
    assert!(offered.contains(&card(0, "card-a")), "{offered}");
    assert!(!offered.contains("Card 1:"), "{offered}");

    // The drives stage's report, the models drive not mounted: its two cards
    // are offered, and Confirm holds naming the drive.
    let unmounted = edited_report(crate::setup_place_tests::drives_report(), |v| {
        v["drives"][1] = serde_json::json!(
            {"role": "models", "mount_point": "/mnt/weaver/models", "mounted": false}
        );
    });
    sent(&admin_config, Stage::Drives, &unmounted).await;
    let offered = read(placement.clone()).await;
    assert!(offered.contains(&card(0, "card-a")), "{offered}");
    assert!(offered.contains(&card(1, "card-b")), "{offered}");

    crate::setup_place_tests::decoder_picked(&rig, &ada, &setup, &version_of(s, &setup).await)
        .await;
    let pull = crate::setup_models_tests::pull_of(s, &setup, "decoder").await;
    assert_eq!(crate::setup_models_tests::ended(s, &pull).await, "verified");
    let (status, body) = crate::setup_models_tests::post_pick(
        &rig,
        &ada,
        &setup,
        crate::setup_models_tests::pick(
            &version_of(s, &setup).await,
            "adapter",
            "org/adapter",
            "adapter.gguf",
            Some("0.5"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let pull = crate::setup_models_tests::pull_of(s, &setup, "adapter").await;
    assert_eq!(crate::setup_models_tests::ended(s, &pull).await, "verified");
    let (status, body, _) = crate::setup_place_tests::post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", &version_of(s, &setup).await),
            ("context_length", "4096"),
            ("turn_context", "independent"),
            ("device", "1"),
            ("device", "0"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    crate::setup_place_tests::identity_kept(s).await;

    let held_for = |reason: &'static str| {
        let confirm = confirm.clone();
        let action = action.clone();
        let rig = &rig;
        let ada = ada.clone();
        let setup = setup.clone();
        async move {
            let held = read(confirm.clone()).await;
            assert!(held.contains(reason), "{reason}: {held}");
            assert!(!held.contains(&action), "{held}");
            let version = version_of(rig.store(), &setup).await;
            let (status, body, _) = crate::setup_place_tests::post(
                rig,
                &ada,
                &setup,
                "confirm",
                &[("version", &version)],
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{body}");
            assert!(body.contains(reason), "{reason}: {body}");
            assert_eq!(rig.store().deployment_tuple(&setup).await.unwrap(), None);
        }
    };
    held_for("the models drive at /mnt/weaver/models is not mounted").await;
    let drives = read(format!("/setup/{setup}/drives")).await;
    assert!(
        drives.contains(
            r#"<span class="status fail">not mounted</span> Confirm holds until the box mounts it."#
        ),
        "{drives}"
    );

    // The state drive one byte under the figure holds Confirm.
    let small_state = edited_report(crate::setup_place_tests::drives_report(), |v| {
        v["drives"][0]["bytes"] = serde_json::json!(crate::box_report::STATE_DRIVE_FLOOR - 1);
    });
    sent(&admin_config, Stage::Drives, &small_state).await;
    held_for("under 9 GiB").await;
    let drives = read(format!("/setup/{setup}/drives")).await;
    assert!(
        drives.contains(&format!(
            r#"<span class="status fail">under 9 GiB</span> Its filesystem is {} bytes"#,
            crate::box_report::STATE_DRIVE_FLOOR - 1
        )),
        "{drives}"
    );

    // A later report lacking a kept device holds Confirm, naming its ordinal.
    let one_card = edited_report(crate::setup_place_tests::drives_report(), |v| {
        v["cards"] = serde_json::json!([v["cards"][0].clone()]);
    });
    sent(&admin_config, Stage::Drives, &one_card).await;
    held_for("card 1 is not among the survey's cards").await;
    let offered = read(placement.clone()).await;
    assert!(
        offered.contains(r#"<span class="status fail">lacking</span> Card 1 is kept, and the drives stage's report lacks it"#),
        "{offered}"
    );

    // The drives report as it stands but for the models drive's free space,
    // 1 byte, under the picks' need: a warning, and Confirm stands and
    // writes the tuple.
    let short = edited_report(crate::setup_place_tests::drives_report(), |v| {
        v["drives"][1]["free_bytes"] = serde_json::json!(1);
    });
    sent(&admin_config, Stage::Drives, &short).await;
    let drives = read(format!("/setup/{setup}/drives")).await;
    assert!(
        drives.contains(
            r#"<span class="status hold">warning</span> Its free space is under the picks' need"#
        ),
        "{drives}"
    );
    let ready = read(confirm.clone()).await;
    assert!(ready.contains(&action), "{ready}");
    assert!(ready.contains("card-b, 0000:07:00.0, 23.99 GiB"), "{ready}");
    let (status, body, _) = crate::setup_place_tests::post(
        &rig,
        &ada,
        &setup,
        "confirm",
        &[("version", &version_of(s, &setup).await)],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let confirmed = s.deployment_tuple(&setup).await.unwrap().unwrap();
    let members = &confirmed.tuple.box_members;
    assert_eq!(members.report.stage, "drives");
    assert_eq!(members.image.version, "2.0");
    assert_eq!(members.driver_version, "620.70.06");
    assert_eq!(members.cuda_runtime_version, "13.4.92");
    let cards: Vec<_> = members
        .cards
        .iter()
        .map(|c| (c.ordinal, c.model.as_str(), c.pci_address.as_str()))
        .collect();
    assert_eq!(
        cards,
        vec![(0, "card-a", "0000:06:00.0"), (1, "card-b", "0000:07:00.0")]
    );
    assert_eq!(members.cpu.model_name, "cpu-1");
    assert_eq!(members.machine.product, "machine-1");
    assert_eq!(members.machine.kernel_release, "7.3.4-arch1-2");
    assert_eq!(
        members
            .drives
            .iter()
            .map(|d| (d.role.as_str(), d.bytes))
            .collect::<Vec<_>>(),
        vec![("state", 25_191_575_552), ("models", 135_089_524_736)]
    );
    assert_eq!(confirmed.tuple.devices, vec![0, 1]);
    let stored: serde_json::Value =
        sqlx::query_scalar("SELECT box_members FROM deployment_tuple WHERE setup_id = $1")
            .bind(&setup)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert!(!stored.to_string().contains("free_bytes"), "{stored}");
}

/// The lines of `logs` since `mark` that log a refused setup report.
fn refusals_logged(logs: &crate::agents_tests::Captured, mark: usize) -> Vec<String> {
    let bytes = logs.0.lock().unwrap();
    String::from_utf8_lossy(&bytes[mark.min(bytes.len())..])
        .lines()
        .filter(|line| line.contains("the survey's reader refused a setup report"))
        .map(str::to_owned)
        .collect()
}

/// **A refused report is logged once, at its arrival** (Spec 8.3): a box
/// enrolled against the real listener sends a report whose `format` holds
/// an ESC byte, and the console's log holds exactly one line naming the
/// setup, the stage, cause 4 and the detail with the byte written `\x1b`,
/// and no ESC byte at all. Two page views log nothing more, and a report
/// the reader reads logs nothing.
///
/// Perturbation: log at the page's read of the reports, and each view adds
/// a line.
#[tokio::test]
async fn a_refused_reports_arrival_is_logged_once_with_its_detail_escaped() {
    let logs = crate::agents_tests::Captured::default();
    let _logging = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(logs.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish(),
    );
    let Some(rig) = rig().await else { return };
    let (_, ada) = admin(&rig.app, rig.store(), &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");
    let mark = logs.mark();

    let escaped = serde_json::json!({
        "format": format!("weaver-survey/1{}[2J", char::from(0x1b_u8)),
    })
    .to_string();
    sent(&config, Stage::Pairing, &escaped).await;
    let lines = refusals_logged(&logs, mark);
    assert_eq!(lines.len(), 1, "{lines:?}");
    for needle in [
        setup.as_str(),
        "pairing",
        "cause=4",
        "weaver-survey/1\\x1b[2J",
    ] {
        assert!(lines[0].contains(needle), "{needle}: {}", lines[0]);
    }
    assert!(
        !logs.0.lock().unwrap()[mark..].contains(&0x1b),
        "no ESC byte reaches the log"
    );

    for _ in 0..2 {
        let page =
            crate::setup_place_tests::read(&rig.app, &format!("/setup/{setup}/pairing"), &ada)
                .await;
        assert!(page.contains("Refused for cause 4: "), "{page}");
    }
    assert_eq!(
        refusals_logged(&logs, mark).len(),
        1,
        "a page view logs nothing"
    );

    sent(
        &config,
        Stage::Drives,
        &crate::setup_place_tests::drives_report(),
    )
    .await;
    assert_eq!(
        refusals_logged(&logs, mark).len(),
        1,
        "a report the reader reads logs nothing"
    );
}

/// **The panes a box's reports feed are served in each status, end to end**
/// (Spec 6.2): a box enrolled against the real listener sends `setup report`
/// through it, and the pages read the kept reports. Before any report the
/// pairing page's report, the drives page's drives and the placement page's
/// devices each wait. A refused pairing report is drawn refused by its
/// cause, and the devices are refused naming it. A pairing report whose
/// machine's product holds a script element is drawn read, the value
/// escaped, and the devices are read from it. A refused drives report is
/// drawn refused, and Confirm holds naming it. A drives report that reads
/// is drawn with each drive and its judgments.
#[tokio::test]
async fn the_panes_are_served_in_each_status_as_the_box_reports() {
    use crate::setup_panes_tests::{pane, status};
    let Some(rig) = rig().await else { return };
    let (_, ada) = admin(&rig.app, rig.store(), &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let on_box = tempfile::tempdir().unwrap();
    enrolled_on_box(&rig, &ada, &setup, on_box.path(), "box-a", None).await;
    let config = on_box.path().join("admin-con.toml");
    let page_of = |name: &str| {
        let (app, ada, uri) = (
            rig.app.clone(),
            ada.clone(),
            format!("/setup/{setup}/{name}"),
        );
        async move { crate::setup_place_tests::read(&app, &uri, &ada).await }
    };
    let mark = |class: &str, word: &str| format!(r#"<span class="status {class}">{word}</span>"#);
    let pairing = "The box's report at the pairing stage";
    let drives = "The drives as mounted";
    let devices = "The devices";

    for (page, heading) in [
        ("pairing", pairing),
        ("drives", drives),
        ("placement", devices),
    ] {
        let shown = page_of(page).await;
        assert_eq!(
            status(pane(&shown, heading)),
            mark("hold", "waiting"),
            "{page}"
        );
        assert!(crate::setup_tests::walk(&shown).is_empty(), "{page}");
    }

    let refused = r#"{"format":"weaver-survey/2"}"#;
    sent(&config, Stage::Pairing, refused).await;
    let shown = page_of("pairing").await;
    let report = pane(&shown, pairing);
    assert_eq!(status(report), mark("fail", "refused"));
    assert!(report.contains("Refused for cause 4: "), "{report}");
    assert!(!shown.contains("weaver-survey/2"), "{shown}");
    let shown = page_of("placement").await;
    let held = pane(&shown, devices);
    assert_eq!(status(held), mark("fail", "refused"));
    assert!(
        held.contains("The pairing stage's report is refused for cause 4: "),
        "{held}"
    );

    let script = edited_report(crate::setup_place_tests::pairing_report(), |v| {
        v["machine"]["product"] = serde_json::json!("<script>alert(1)</script>");
    });
    sent(&config, Stage::Pairing, &script).await;
    let shown = page_of("pairing").await;
    let report = pane(&shown, pairing);
    assert_eq!(status(report), mark("ok", "read"));
    assert!(
        report.contains(
            "<tr><th>Product</th><td>&#60;script&#62;alert(1)&#60;/script&#62;</td></tr>"
        ),
        "{report}"
    );
    assert!(!shown.contains("<script>alert"), "{shown}");
    assert!(crate::setup_tests::walk(&shown).is_empty());
    let shown = page_of("placement").await;
    let offered = pane(&shown, devices);
    assert_eq!(status(offered), mark("ok", "read"));
    assert!(
        offered.contains(r#"<span class="source">the pairing stage's report</span>"#),
        "{offered}"
    );

    sent(
        &config,
        Stage::Drives,
        &refused.replace("}", r#","stage":"drives"}"#),
    )
    .await;
    let shown = page_of("drives").await;
    let report = pane(&shown, drives);
    assert_eq!(status(report), mark("fail", "refused"));
    assert!(report.contains("Refused for cause 4: "), "{report}");
    let confirm = page_of("confirm").await;
    assert!(
        confirm.contains("the drives stage's report is refused for cause 4"),
        "{confirm}"
    );

    sent(
        &config,
        Stage::Drives,
        &crate::setup_place_tests::drives_report(),
    )
    .await;
    let shown = page_of("drives").await;
    let report = pane(&shown, drives);
    assert_eq!(status(report), mark("ok", "read"));
    assert!(
        report.contains("<td class=\"mono\">weaver-state</td>"),
        "{report}"
    );
    assert!(report.contains(&mark("ok", "at least 9 GiB")), "{report}");
    assert!(
        report.contains("No model is picked, so its free space is not judged."),
        "{report}"
    );
    assert!(crate::setup_tests::walk(&shown).is_empty());
}
