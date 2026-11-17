//! conforms: web-the-pairing-page-reads-an-entry-in-crockfords-form-and-a-malformed-entry-is-no-miss
//! conforms: web-a-pairing-naming-a-live-box-and-name-is-refused-and-retires-nothing
//! conforms: web-a-lost-word-writes-no-row
//! conforms: web-on-unknown-the-pairing-page-shows-what-the-write-made
//! conforms: web-a-rows-box-changes-only-through-the-replace-box-write
//!
//! **An appliance's enrollment, the exchange** (Spec 8.2): a code's entry on
//! the pairing page, against the real listener, with the box's half
//! (`link::pair_client`) as the box and an admin's session as the operator.
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
            boxes: Arc::new(crate::box_report::NoBox),
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

/// The code's entry as the page posts it, the comparison confirmed.
async fn enter(
    rig: &Rig,
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
        &rig.app,
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
