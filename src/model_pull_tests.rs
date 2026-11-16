//! conforms: web-authorization-goes-to-hugging-face-alone
//! conforms: web-every-download-names-the-commit
//! conforms: web-no-file-takes-its-blobs-name-before-its-digest-matches
//! conforms: web-a-repositorys-file-names-write-nothing-outside-the-cache
//! conforms: web-a-pull-asked-through-an-account-never-runs-without-its-token
//! conforms: web-the-free-space-is-checked-against-what-the-run-has-yet-to-write
//! conforms: web-a-server-that-answers-a-range-with-the-whole-body-restarts-the-file
//! conforms: web-an-admins-cancel-stops-the-running-pull-and-keeps-its-partial
//! conforms: web-the-hugging-face-token-is-held-sealed
//!
//! The model pulls (Spec 2.15, `src/hugging_face/pull.rs`,
//! `src/hugging_face/cache.rs`, `src/store/model_pull.rs`): a pull writes
//! each blob under its identity and its snapshot as a relative link, and
//! keeps the rulings' record. Every request names the commit. The token
//! reaches Hugging Face's own listener and never the CDN's. A body whose
//! digest differs takes no name. A cut download resumes by range. A blob in
//! the cache is found, and one that hashes wrong is fetched again. Too
//! little space, a path out of its shape and a link planted at a partial
//! each refuse before any write. A pull running at the stop resumes at the
//! next start. One pull runs at a time. Every test runs against a fake
//! Hugging Face and a fake CDN on two listeners of their own, and on a
//! store and a cache of its own. No token, account or digest is a real one.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::admin_tests::{admin, send_as, signed_in};
use crate::hugging_face::cache::Cache;
use crate::hugging_face::pull::{Puller, Settings};
use crate::hugging_face::{Client, RepoFile, RepoId};
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::read::tests::{Fresh, fresh_store};
use crate::surfaces::gate;

const COMMIT: &str = "27b350a791e81d9a4d1ddca1c49282e9ec533768";
const TOKEN: &str = "test-token-read-0001";
const CONFIG: &[u8] = b"{\"model_type\": \"test\"}\n";
const MODEL_LEN: usize = 3 * 1024 * 1024 + 17;

fn model_bytes() -> Vec<u8> {
    (0..MODEL_LEN)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect()
}

fn hex(d: ring::digest::Digest) -> String {
    crate::store::identity::hex(d.as_ref())
}

fn sha256(bytes: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, bytes))
}

fn git_blob_id(bytes: &[u8]) -> String {
    let mut c = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    c.update(format!("blob {}\0", bytes.len()).as_bytes());
    c.update(bytes);
    hex(c.finish())
}

/// What a fake was asked: its path, whether `Authorization` came, and the
/// range.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    authorized: bool,
    range: Option<String>,
}

/// How the CDN answers its next fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Whole,
    /// The body cut after half its bytes.
    Cut,
    /// The body stalls after half its bytes.
    Stall,
    /// Bytes of the right length that are not the file's.
    Wrong,
    /// The whole body, `200`, whatever range was asked, as a server that
    /// does not take ranges answers.
    IgnoreRange,
    /// The whole body, once the test releases it (`Fakes::release`).
    Held,
}

#[derive(Default)]
struct Fakes {
    hub: Mutex<Vec<Seen>>,
    cdn: Mutex<Vec<Seen>>,
    modes: Mutex<VecDeque<Mode>>,
    cdn_base: Mutex<String>,
    fetched: AtomicUsize,
    /// What a `Held` fetch waits for.
    release: tokio::sync::Notify,
}

fn seen(path: &str, headers: &HeaderMap) -> Seen {
    Seen {
        path: path.to_owned(),
        authorized: headers.contains_key(header::AUTHORIZATION),
        range: headers
            .get(header::RANGE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    }
}

/// **Hugging Face's resolve**: the commit alone, an LFS file redirected to
/// the CDN's host, a file kept in git redirected to a relative path on its
/// own host, as measured on 2026-11-16.
async fn resolve(
    State(fakes): State<Arc<Fakes>>,
    headers: HeaderMap,
    UrlPath((owner, name, revision, path)): UrlPath<(String, String, String, String)>,
) -> Response {
    fakes.hub.lock().unwrap().push(seen(
        &format!("/{owner}/{name}/resolve/{revision}/{path}"),
        &headers,
    ));
    if revision != COMMIT {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "RevisionNotFound")],
            "",
        )
            .into_response();
    }
    match path.as_str() {
        "model.gguf" => {
            let cdn = fakes.cdn_base.lock().unwrap().clone();
            (
                StatusCode::FOUND,
                [(
                    header::LOCATION,
                    format!("{cdn}blob/{}", sha256(&model_bytes())),
                )],
            )
                .into_response()
        }
        "config.json" => (
            StatusCode::TEMPORARY_REDIRECT,
            [(
                header::LOCATION,
                format!("/api/resolve-cache/{revision}/config.json"),
            )],
        )
            .into_response(),
        _ => (
            StatusCode::NOT_FOUND,
            [("x-error-code", "EntryNotFound")],
            "",
        )
            .into_response(),
    }
}

async fn resolve_cache(
    State(fakes): State<Arc<Fakes>>,
    headers: HeaderMap,
    UrlPath((revision, path)): UrlPath<(String, String)>,
) -> Response {
    fakes.hub.lock().unwrap().push(seen(
        &format!("/api/resolve-cache/{revision}/{path}"),
        &headers,
    ));
    serve(&headers, CONFIG.to_vec(), Mode::Whole)
}

async fn blob(
    State(fakes): State<Arc<Fakes>>,
    headers: HeaderMap,
    UrlPath(id): UrlPath<String>,
) -> Response {
    fakes
        .cdn
        .lock()
        .unwrap()
        .push(seen(&format!("/blob/{id}"), &headers));
    fakes.fetched.fetch_add(1, Ordering::SeqCst);
    let mode = fakes
        .modes
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or(Mode::Whole);
    let mut bytes = model_bytes();
    match mode {
        Mode::Wrong => bytes.iter_mut().for_each(|b| *b = b.wrapping_add(1)),
        Mode::IgnoreRange => return serve(&HeaderMap::new(), bytes, Mode::Whole),
        Mode::Held => fakes.release.notified().await,
        _ => {}
    }
    serve(&headers, bytes, mode)
}

/// A body from the range asked, whole or cut or stalled after half.
fn serve(headers: &HeaderMap, bytes: Vec<u8>, mode: Mode) -> Response {
    let len = bytes.len();
    let from = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("bytes="))
        .and_then(|v| v.strip_suffix('-'))
        .and_then(|v| v.parse::<usize>().ok());
    let start = from.unwrap_or(0);
    let body = bytes[start..].to_vec();
    let half = body.len() / 2;
    let stream: std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<Bytes, std::io::Error>> + Send>,
    > = match mode {
        Mode::Whole | Mode::Wrong | Mode::IgnoreRange | Mode::Held => Box::pin(tokio_stream::iter(
            body.chunks(64 * 1024)
                .map(|c| Ok(Bytes::copy_from_slice(c)))
                .collect::<Vec<_>>(),
        )),
        Mode::Cut => Box::pin(tokio_stream::iter(vec![
            Ok(Bytes::copy_from_slice(&body[..half])),
            Err(std::io::Error::other("cut")),
        ])),
        Mode::Stall => {
            use tokio_stream::StreamExt;
            Box::pin(
                tokio_stream::iter(vec![Ok(Bytes::copy_from_slice(&body[..half]))])
                    .chain(tokio_stream::pending()),
            )
        }
    };
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_LENGTH, body.len().into());
    headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
    if from.is_some() {
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{len}", len - 1).parse().unwrap(),
        );
    }
    response
}

async fn listen(router: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await });
    base
}

/// The two fakes, each on a listener of its own, and Hugging Face's base.
async fn fakes() -> (Arc<Fakes>, String) {
    let fakes = Arc::new(Fakes::default());
    let hub = axum::Router::new()
        .route("/{owner}/{name}/resolve/{revision}/{*path}", get(resolve))
        .route("/api/resolve-cache/{revision}/{*path}", get(resolve_cache))
        .with_state(fakes.clone());
    let cdn = axum::Router::new()
        .route("/blob/{id}", get(blob))
        .with_state(fakes.clone());
    let hub_base = listen(hub).await;
    *fakes.cdn_base.lock().unwrap() = listen(cdn).await;
    (fakes, hub_base)
}

struct Rig {
    fresh: Fresh,
    dir: tempfile::TempDir,
    fakes: Arc<Fakes>,
    base: String,
    admin: String,
    session_id: i64,
    /// The admin's session cookie, for the page's tests.
    cookie: String,
    /// The app the admin signed in through, for a second person's sign-in.
    web: axum::Router,
}

fn no_space(_: &Path) -> std::io::Result<u64> {
    Ok(10)
}

fn plenty(_: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX / 2)
}

/// **A disk that holds the model and the config and nothing more**: its
/// free space is that capacity less every regular file in the blobs
/// directory, so it shrinks as the pull writes.
fn room_for_both(dir: &Path) -> std::io::Result<u64> {
    let capacity = (MODEL_LEN + CONFIG.len()) as u64;
    let mut held = 0;
    for entry in std::fs::read_dir(dir)? {
        let meta = entry?.metadata()?;
        if meta.is_file() {
            held += meta.len();
        }
    }
    Ok(capacity.saturating_sub(held))
}

/// **A disk that holds the model and nothing more**, shrinking as the pull
/// writes, as `room_for_both` does.
fn room_for_the_model(dir: &Path) -> std::io::Result<u64> {
    let mut held = 0;
    for entry in std::fs::read_dir(dir)? {
        let meta = entry?.metadata()?;
        if meta.is_file() {
            held += meta.len();
        }
    }
    Ok((MODEL_LEN as u64).saturating_sub(held))
}

/// **The pulls a test holds at step 3**, by the blobs directory of the
/// test's own cache: whether the hold was reached, and whether the test has
/// released it.
#[allow(clippy::type_complexity)]
static HOLD_AT_STEP_THREE: Mutex<Vec<(std::path::PathBuf, Arc<(AtomicBool, AtomicBool)>)>> =
    Mutex::new(Vec::new());

/// **A free-space read that holds the pull at step 3** until the test
/// releases it, within ten seconds, as a long hash holds the one worker.
/// Plenty of room. It blocks its thread through `block_in_place`, which
/// hands the runtime's other work to another thread, so the test's own
/// timers run while it holds. Its tests run on a runtime of more than one
/// thread, which `block_in_place` needs.
fn hold_at_step_three(dir: &Path) -> std::io::Result<u64> {
    let held = HOLD_AT_STEP_THREE
        .lock()
        .unwrap()
        .iter()
        .find(|(blobs, _)| blobs.as_path() == dir)
        .map(|(_, held)| held.clone());
    if let Some(held) = held {
        held.0.store(true, Ordering::SeqCst);
        tokio::task::block_in_place(|| {
            for _ in 0..1000 {
                if held.1.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
    }
    Ok(u64::MAX / 2)
}

impl Rig {
    fn store(&self) -> &Store {
        &self.fresh.store
    }

    fn root(&self) -> std::path::PathBuf {
        self.dir.path().join("cache")
    }

    fn puller(&self, settings: Settings) -> Puller {
        Puller::start(
            self.fresh.store.clone(),
            Client::with_base(&self.base).unwrap(),
            Some(Cache::at(&self.root()).unwrap()),
            self.dir.path().join("authority"),
            settings,
        )
    }

    /// An ask of `files`, its first record written first, as an admin's
    /// write does.
    async fn ask(&self, puller: &Puller, files: &[RepoFile]) -> (String, Result<(), Refusal>) {
        let s = self.store();
        let pull_id = s.mint_key("mp").await.unwrap();
        let first = s
            .audit_first(
                Principal::Person {
                    person_id: &self.admin,
                    method: PersonMethod::Session,
                },
                Target::ModelPull(&pull_id),
                "model pull",
            )
            .await
            .unwrap();
        let asked = puller
            .ask(
                &self.admin,
                self.session_id,
                &first,
                &pull_id,
                &repo(),
                COMMIT,
                Some("ada-hf"),
                files,
            )
            .await
            .unwrap();
        (pull_id, asked)
    }

    /// The pull's state once it is no longer `running`, within ten seconds.
    async fn ended(&self, pull_id: &str) -> (String, Option<String>) {
        for _ in 0..200 {
            let pull = self.store().pull(pull_id).await.unwrap().unwrap();
            if pull.state != "running" {
                return (pull.state, pull.reason);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the pull {pull_id} still runs");
    }

    /// The Hugging Face page over `puller`, behind the session gate, with
    /// the app the admin signed in through.
    fn surface(&self, puller: &Puller) -> axum::Router {
        let policy = gate::Policy {
            origin: Some(ORIGIN.to_owned()),
            idle: Duration::from_secs(3600),
            absolute: Duration::from_secs(12 * 3600),
        };
        let surface = gate::guard(
            crate::surfaces::script_policy(crate::surfaces::hugging_face::routes(
                crate::surfaces::hugging_face::Seams {
                    policy: policy.clone(),
                    client: Client::with_base(&self.base).unwrap(),
                    authority_dir: self.dir.path().join("authority"),
                    puller: puller.clone(),
                },
            ))
            .with_state(self.fresh.store.clone()),
            policy,
        );
        self.web.clone().merge(surface)
    }

    fn hub(&self) -> Vec<Seen> {
        self.fakes.hub.lock().unwrap().clone()
    }

    fn cdn(&self) -> Vec<Seen> {
        self.fakes.cdn.lock().unwrap().clone()
    }

    fn blobs(&self) -> std::path::PathBuf {
        self.root().join("models--org--open").join("blobs")
    }

    fn snapshot(&self, path: &str) -> std::path::PathBuf {
        self.root()
            .join("models--org--open")
            .join("snapshots")
            .join(COMMIT)
            .join(path)
    }
}

fn repo() -> RepoId {
    RepoId::parse("org/open").unwrap()
}

fn model_file() -> RepoFile {
    let bytes = model_bytes();
    RepoFile {
        path: "model.gguf".into(),
        size: bytes.len() as u64,
        blob_id: git_blob_id(&bytes),
        sha256: Some(sha256(&bytes)),
    }
}

fn config_file() -> RepoFile {
    RepoFile {
        path: "config.json".into(),
        size: CONFIG.len() as u64,
        blob_id: git_blob_id(CONFIG),
        sha256: None,
    }
}

async fn rig() -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("cache")).unwrap();
    crate::link::authority::Authority::init(&dir.path().join("authority"), "weaver-web", &[])
        .unwrap();
    let (fakes, base) = fakes().await;
    let web = app(&fresh.store, Some(passkeys()));
    let (person, cookie) = admin(&web, &fresh.store, &mut authenticator(), "ada").await;
    let session_id: i64 = sqlx::query_scalar("SELECT session_id FROM session WHERE person_id = $1")
        .bind(&person)
        .fetch_one(&fresh.store.pool)
        .await
        .unwrap();
    // The account, so the puller holds a token to send.
    let key = crate::hugging_face::seal::SealKey::load(&dir.path().join("authority"))
        .unwrap()
        .unwrap();
    let sealed = key.seal(&crate::hugging_face::Token::parse(TOKEN).unwrap());
    sqlx::query(
        "INSERT INTO hugging_face_account (account, role, sealed, nonce, set_by) \
         VALUES ('ada-hf', 'read', $1, $2, $3)",
    )
    .bind(&sealed.ciphertext)
    .bind(&sealed.nonce)
    .bind(&person)
    .execute(&fresh.store.pool)
    .await
    .unwrap();
    Some(Rig {
        fresh,
        dir,
        fakes,
        base,
        admin: person,
        session_id,
        cookie,
        web,
    })
}

fn quick() -> Settings {
    Settings {
        stall: Duration::from_millis(500),
        retries: 3,
        backoff: Duration::from_millis(10),
        free_space: plenty,
    }
}

/// **A pull writes each blob under its identity and links its snapshot**,
/// and keeps the rulings' record: the commit, every file's SHA-256 and
/// size, the account and the time, each file `downloaded`. A file kept in
/// git follows the relative redirect, is verified by its git blob id, has
/// its SHA-256 recorded and is named by its blob id. **The token reaches
/// Hugging Face's own listener and never the CDN's**, and every resolve
/// names the commit. The ask is a first record and its `ok` outcome.
#[tokio::test]
async fn a_pull_writes_its_blobs_and_links_and_keeps_its_record() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let (pull_id, asked) = rig.ask(&puller, &[config_file(), model_file()]).await;
    assert_eq!(asked, Ok(()));
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));

    let model = model_file();
    let blob = rig.blobs().join(model.sha256.as_ref().unwrap());
    assert_eq!(std::fs::read(&blob).unwrap(), model_bytes());
    assert_eq!(
        std::fs::read_link(rig.snapshot("model.gguf")).unwrap(),
        Path::new(&format!("../../blobs/{}", model.sha256.as_ref().unwrap()))
    );
    let config = config_file();
    assert_eq!(
        std::fs::read(rig.blobs().join(&config.blob_id)).unwrap(),
        CONFIG
    );
    assert_eq!(std::fs::read(rig.snapshot("config.json")).unwrap(), CONFIG);
    assert!(
        !rig.blobs()
            .join(format!("{}.incomplete", model.sha256.as_ref().unwrap()))
            .exists()
    );

    let pull = rig.store().pull(&pull_id).await.unwrap().unwrap();
    assert_eq!(
        (
            pull.repository.as_str(),
            pull.commit.as_str(),
            pull.account.as_deref()
        ),
        ("org/open", COMMIT, Some("ada-hf"))
    );
    assert!(pull.finished_at.is_some());
    let files = rig.store().pull_files(&pull_id).await.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files[0].file.sha256.as_deref(),
        Some(sha256(CONFIG).as_str())
    );
    assert_eq!(files[1].file.sha256, model.sha256);
    for f in &files {
        assert_eq!(f.source.as_deref(), Some("downloaded"));
        assert_eq!(f.bytes_done as u64, f.file.size);
    }

    let (hub, cdn) = (rig.hub(), rig.cdn());
    assert!(!hub.is_empty() && !cdn.is_empty());
    assert!(hub.iter().all(|s| s.authorized), "{hub:?}");
    assert!(
        cdn.iter().all(|s| !s.authorized),
        "the CDN saw the token: {cdn:?}"
    );
    assert!(
        hub.iter()
            .filter(|s| s.path.contains("/resolve/"))
            .all(|s| s.path.contains(COMMIT)),
        "{hub:?}"
    );
    let audited: (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE answers IS NULL AND refusal IS NULL), \
         count(*) FILTER (WHERE outcome = 'ok') FROM audit \
         WHERE target_kind = 'model_pull' AND target_id = $1",
    )
    .bind(&pull_id)
    .fetch_one(&rig.store().pool)
    .await
    .unwrap();
    assert_eq!(audited, (1, 1));
    puller.stop();
}

/// **Every download names the commit and never a branch**: an ask naming
/// a branch is refused before any row or request, and the fake, which
/// answers the commit alone, is never asked another revision.
#[tokio::test]
async fn every_download_names_the_commit() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let pull_id = rig.store().mint_key("mp").await.unwrap();
    let asked = puller
        .ask(
            &rig.admin,
            rig.session_id,
            "au-0000000000000000",
            &pull_id,
            &repo(),
            "main",
            None,
            &[model_file()],
        )
        .await
        .unwrap();
    assert!(matches!(asked, Err(Refusal::Name(_))), "{asked:?}");
    assert!(rig.store().pull(&pull_id).await.unwrap().is_none());
    assert!(rig.hub().is_empty());

    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await.0, "verified");
    assert!(rig.hub().iter().all(|s| s.path.contains(COMMIT)));
    puller.stop();
}

/// **No file takes its blob's name before its digest matches**: a body of
/// the right length whose digest differs leaves nothing under the blob's
/// name, the snapshot's path or the partial's, and the pull fails saying
/// so.
#[tokio::test]
async fn a_body_whose_digest_differs_takes_no_name() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Wrong);
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    let (state, reason) = rig.ended(&pull_id).await;
    assert_eq!(state, "failed");
    assert!(reason.unwrap().contains("digest differs"));
    let id = model_file().sha256.unwrap();
    assert!(!rig.blobs().join(&id).exists());
    assert!(!rig.blobs().join(format!("{id}.incomplete")).exists());
    assert!(std::fs::symlink_metadata(rig.snapshot("model.gguf")).is_err());
    puller.stop();
}

/// **A cut download resumes by range** and verifies whole: the second
/// fetch asks from the bytes the first wrote.
#[tokio::test]
async fn a_cut_download_resumes_by_range() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Cut);
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));
    let cdn = rig.cdn();
    assert_eq!(cdn.len(), 2, "{cdn:?}");
    assert_eq!(cdn[0].range, None);
    assert_eq!(cdn[1].range, Some(format!("bytes={}-", MODEL_LEN / 2)));
    puller.stop();
}

/// **A blob in the cache is found and not fetched**, linked from the
/// snapshot and recorded `found`. **One that hashes wrong is fetched
/// again** and replaced.
#[tokio::test]
async fn a_blob_in_the_cache_is_found_and_one_that_hashes_wrong_is_fetched() {
    let Some(rig) = rig().await else { return };
    let id = model_file().sha256.unwrap();
    std::fs::create_dir_all(rig.blobs()).unwrap();
    std::fs::write(rig.blobs().join(&id), model_bytes()).unwrap();
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await.0, "verified");
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 0);
    let files = rig.store().pull_files(&pull_id).await.unwrap();
    assert_eq!(files[0].source.as_deref(), Some("found"));
    assert!(std::fs::read_link(rig.snapshot("model.gguf")).is_ok());

    let mut wrong = model_bytes();
    wrong[7] ^= 0xff;
    std::fs::write(rig.blobs().join(&id), wrong).unwrap();
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await.0, "verified");
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 1);
    assert_eq!(std::fs::read(rig.blobs().join(&id)).unwrap(), model_bytes());
    let files = rig.store().pull_files(&pull_id).await.unwrap();
    assert_eq!(files[0].source.as_deref(), Some("downloaded"));
    puller.stop();
}

/// **Too little space refuses before any byte**: the pull fails naming
/// the bytes needed, and the CDN is asked nothing.
#[tokio::test]
async fn too_little_space_refuses_before_any_byte() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(Settings {
        free_space: no_space,
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    let (state, reason) = rig.ended(&pull_id).await;
    assert_eq!(state, "failed");
    assert!(
        reason
            .unwrap()
            .contains(&format!("needs {MODEL_LEN} bytes free"))
    );
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 0);
    assert!(
        !rig.blobs()
            .join(format!("{}.incomplete", model_file().sha256.unwrap()))
            .exists()
    );
    puller.stop();
}

/// **A repository's file names write nothing outside the cache**: an ask
/// naming `..` is refused before any row, and a row that names one all the
/// same (written past the ask) fails at the first step, nothing written
/// outside the cache's root.
#[tokio::test]
async fn a_path_out_of_its_shape_writes_nothing_outside_the_cache() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let escaping = RepoFile {
        path: "../../../../escaped".into(),
        ..config_file()
    };
    let (pull_id, asked) = rig.ask(&puller, std::slice::from_ref(&escaping)).await;
    assert!(matches!(asked, Err(Refusal::Name(_))), "{asked:?}");
    assert!(rig.store().pull(&pull_id).await.unwrap().is_none());

    sqlx::query(
        "INSERT INTO model_pull (pull_id, repository, commit_id, asked_by, state) \
         VALUES ('mp-00000000000000aa', 'org/open', $1, $2, 'running')",
    )
    .bind(COMMIT)
    .bind(&rig.admin)
    .execute(&rig.store().pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_pull_file (pull_id, ordinal, path, size, blob_id, lfs) \
         VALUES ('mp-00000000000000aa', 0, $1, $2, $3, false)",
    )
    .bind(&escaping.path)
    .bind(escaping.size as i64)
    .bind(&escaping.blob_id)
    .execute(&rig.store().pool)
    .await
    .unwrap();
    puller.stop();
    let puller = rig.puller(quick());
    let (state, reason) = rig.ended("mp-00000000000000aa").await;
    assert_eq!(state, "failed");
    assert!(reason.unwrap().contains("is not a file's path"));
    let outside: Vec<_> = std::fs::read_dir(rig.dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert!(!outside.contains(&"escaped".to_owned()), "{outside:?}");
    assert!(rig.hub().is_empty());
    puller.stop();
}

/// **A symbolic link planted at a partial's name refuses the file**, and
/// the file it points to is untouched.
#[tokio::test]
async fn a_link_planted_at_the_partial_refuses() {
    let Some(rig) = rig().await else { return };
    let id = model_file().sha256.unwrap();
    std::fs::create_dir_all(rig.blobs()).unwrap();
    let elsewhere = rig.dir.path().join("elsewhere");
    std::fs::write(&elsewhere, b"untouched").unwrap();
    std::os::unix::fs::symlink(&elsewhere, rig.blobs().join(format!("{id}.incomplete"))).unwrap();
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    let (state, reason) = rig.ended(&pull_id).await;
    assert_eq!(state, "failed");
    assert!(reason.unwrap().contains("symbolic link"));
    assert_eq!(std::fs::read(&elsewhere).unwrap(), b"untouched");
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 0);
    puller.stop();
}

/// **A pull running at the server's stop resumes at the next start**: the
/// partial keeps what was written, a new puller reads the running pull
/// first, and its fetch asks from that byte.
#[tokio::test]
async fn a_pull_running_at_the_stop_resumes_at_the_next_start() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Stall);
    let puller = rig.puller(Settings {
        stall: Duration::from_secs(60),
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    let partial = rig
        .blobs()
        .join(format!("{}.incomplete", model_file().sha256.unwrap()));
    let mut held = 0;
    for _ in 0..200 {
        held = std::fs::metadata(&partial).map_or(0, |m| m.len());
        if held as usize >= MODEL_LEN / 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(held as usize, MODEL_LEN / 2);
    puller.stop();
    assert_eq!(
        rig.store().pull(&pull_id).await.unwrap().unwrap().state,
        "running"
    );

    let puller = rig.puller(quick());
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));
    let cdn = rig.cdn();
    assert_eq!(
        cdn.last().unwrap().range,
        Some(format!("bytes={}-", MODEL_LEN / 2))
    );
    puller.stop();
}

/// **One pull runs at a time**: an ask while one runs is refused, naming
/// the one that runs, and no row is written for it.
#[tokio::test]
async fn one_pull_runs_at_a_time() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Stall);
    let puller = rig.puller(Settings {
        stall: Duration::from_secs(60),
        ..quick()
    });
    let (running, _) = rig.ask(&puller, &[model_file()]).await;
    let (second, asked) = rig.ask(&puller, &[config_file()]).await;
    match asked {
        Err(Refusal::PullRunning(which)) => assert!(which.contains(&running), "{which}"),
        other => panic!("{other:?}"),
    }
    assert!(rig.store().pull(&second).await.unwrap().is_none());
    puller.stop();
}

/// **No cache, no pull**: an ask where `model_cache` is unset is refused
/// before any row.
#[tokio::test]
async fn without_a_cache_an_ask_is_refused() {
    let Some(rig) = rig().await else { return };
    let puller = Puller::start(
        rig.store().clone(),
        Client::with_base(&rig.base).unwrap(),
        None,
        rig.dir.path().join("authority"),
        quick(),
    );
    let (pull_id, asked) = rig.ask(&puller, &[model_file()]).await;
    assert_eq!(asked, Err(Refusal::NoModelCache));
    assert!(rig.store().pull(&pull_id).await.unwrap().is_none());
    puller.stop();
}

/// **The cache's section shows each pull with the rulings' record**: the
/// repository, the commit, every file's SHA-256 and size, its source, the
/// account and who asked, on the page and in the fragment the page
/// re-fetches, both kept by no cache.
#[tokio::test]
async fn the_cache_section_shows_each_pull_with_its_record() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[config_file(), model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await.0, "verified");
    let web = rig.surface(&puller);
    for uri in ["/admin/hugging-face", "/admin/hugging-face/pulls"] {
        let (status, headers, body) = send_as(&web, "GET", uri, None, &rig.cookie).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
        assert_eq!(headers[header::CACHE_CONTROL], "no-store", "{uri}");
        for shown in [
            pull_id.as_str(),
            "org/open",
            COMMIT,
            model_file().sha256.as_deref().unwrap(),
            sha256(CONFIG).as_str(),
            "verified",
            "downloaded",
            "ada-hf",
            "config.json",
            "model.gguf",
            "The model cache is set",
        ] {
            assert!(body.contains(shown), "{uri} lacks {shown}: {body}");
        }
    }
    puller.stop();
}

/// **Without a cache the section says why** no pull can run.
#[tokio::test]
async fn without_a_cache_the_section_says_why() {
    let Some(rig) = rig().await else { return };
    let puller = Puller::start(
        rig.store().clone(),
        Client::with_base(&rig.base).unwrap(),
        None,
        rig.dir.path().join("authority"),
        quick(),
    );
    let web = rig.surface(&puller);
    let (status, _, body) = send_as(&web, "GET", "/admin/hugging-face", None, &rig.cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("No model_cache is set"), "{body}");
    puller.stop();
}

/// **The section's re-fetch is an admin's**: no session answers 401, and a
/// person holding no admin grant 403, with no pull shown.
#[tokio::test]
async fn the_sections_refetch_is_an_admins() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[config_file()]).await;
    assert_eq!(rig.ended(&pull_id).await.0, "verified");
    let web = rig.surface(&puller);
    let (_, bea) = signed_in(&rig.web, rig.store(), &mut authenticator(), "bea").await;
    let (status, _, body) = send_as(&web, "GET", "/admin/hugging-face/pulls", None, &bea).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!body.contains(&pull_id));
    let (status, _, body) = send_as(
        &web,
        "GET",
        "/admin/hugging-face/pulls",
        None,
        "no-such-session",
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains(&pull_id));
    puller.stop();
}

/// **A pull asked through an account never runs without its token**: where
/// the account's seal no longer opens, the pull fails saying so, and
/// neither Hugging Face nor the CDN is asked anything.
#[tokio::test]
async fn a_pull_through_an_account_never_runs_without_its_token() {
    let Some(rig) = rig().await else { return };
    let key = rig
        .dir
        .path()
        .join("authority")
        .join(crate::hugging_face::seal::SEAL_KEY);
    std::fs::remove_file(&key).unwrap();
    crate::hugging_face::seal::write_key(&key, &crate::hugging_face::seal::mint()).unwrap();
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    let (state, reason) = rig.ended(&pull_id).await;
    assert_eq!(state, "failed");
    assert!(reason.unwrap().contains("does not open"));
    assert!(rig.hub().is_empty() && rig.cdn().is_empty());
    puller.stop();
}

/// **The free space counts only what this run has yet to verify**: on a
/// disk with room for the model and the config exactly, the config is
/// checked against its own size once the model is written, never against
/// the model's again.
#[tokio::test]
async fn free_space_counts_only_the_files_this_run_has_yet_to_verify() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(Settings {
        free_space: room_for_both,
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[model_file(), config_file()]).await;
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));
    puller.stop();
}

/// **A blob that already stands needs no free space**: a pull of a commit
/// whose model the cache holds checks the config against its own size,
/// and the model is found and not fetched.
#[tokio::test]
async fn a_blob_already_standing_needs_no_free_space() {
    let Some(rig) = rig().await else { return };
    std::fs::create_dir_all(rig.blobs()).unwrap();
    std::fs::write(
        rig.blobs().join(model_file().sha256.unwrap()),
        model_bytes(),
    )
    .unwrap();
    let puller = rig.puller(Settings {
        free_space: room_for_both,
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[config_file(), model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 0);
    let files = rig.store().pull_files(&pull_id).await.unwrap();
    assert_eq!(files[1].source.as_deref(), Some("found"));
    puller.stop();
}

/// **A blob two paths share counts once**: a pull of two paths holding the
/// same bytes, on a disk with room for one copy, fetches the blob once and
/// links both paths to it, the second found.
#[tokio::test]
async fn a_blob_two_paths_share_counts_once_in_the_free_space() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(Settings {
        free_space: room_for_the_model,
        ..quick()
    });
    let copy = RepoFile {
        path: "copy/model.gguf".into(),
        ..model_file()
    };
    let (pull_id, _) = rig.ask(&puller, &[model_file(), copy]).await;
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 1);
    let files = rig.store().pull_files(&pull_id).await.unwrap();
    assert_eq!(files[0].source.as_deref(), Some("downloaded"));
    assert_eq!(files[1].source.as_deref(), Some("found"));
    for path in ["model.gguf", "copy/model.gguf"] {
        assert_eq!(std::fs::read(rig.snapshot(path)).unwrap(), model_bytes());
    }
    puller.stop();
}

/// **A server that answers a range with the whole body restarts the
/// file**: after a cut, the resume asks from the bytes held, the server
/// answers `200` from byte zero, and the file is written again from its
/// start and verifies.
#[tokio::test]
async fn a_server_that_ignores_the_range_restarts_the_file() {
    let Some(rig) = rig().await else { return };
    rig.fakes
        .modes
        .lock()
        .unwrap()
        .extend([Mode::Cut, Mode::IgnoreRange]);
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    assert_eq!(rig.ended(&pull_id).await, ("verified".into(), None));
    let cdn = rig.cdn();
    assert_eq!(cdn.len(), 2, "{cdn:?}");
    assert_eq!(cdn[1].range, Some(format!("bytes={}-", MODEL_LEN / 2)));
    assert_eq!(
        std::fs::read(rig.blobs().join(model_file().sha256.unwrap())).unwrap(),
        model_bytes()
    );
    puller.stop();
}

/// Waits until the CDN has been asked `n` times, within five seconds.
async fn cdn_asked(rig: &Rig, n: usize) {
    for _ in 0..100 {
        if rig.fakes.fetched.load(Ordering::SeqCst) >= n {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "the CDN was asked {} times",
        rig.fakes.fetched.load(Ordering::SeqCst)
    );
}

/// **A disconnected account stops the pull before its next retry**: the
/// fetch stalls, the account is disconnected, and the retry is never
/// asked. The pull fails saying so.
#[tokio::test]
async fn a_disconnected_account_stops_the_pull_before_its_next_retry() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Stall);
    let puller = rig.puller(Settings {
        stall: Duration::from_secs(2),
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    cdn_asked(&rig, 1).await;
    sqlx::query("DELETE FROM hugging_face_account")
        .execute(&rig.store().pool)
        .await
        .unwrap();
    let (state, reason) = rig.ended(&pull_id).await;
    assert_eq!(state, "failed");
    assert!(reason.unwrap().contains("disconnected"));
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 1);
    puller.stop();
}

/// **A replaced account stops the pull before its next file**: the model's
/// fetch is held, the account is replaced by another, the model finishes,
/// and the config is never asked for.
#[tokio::test]
async fn a_replaced_account_stops_the_pull_before_its_next_file() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Held);
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[model_file(), config_file()]).await;
    cdn_asked(&rig, 1).await;
    sqlx::query("UPDATE hugging_face_account SET account = 'bea-hf'")
        .execute(&rig.store().pool)
        .await
        .unwrap();
    rig.fakes.release.notify_one();
    let (state, reason) = rig.ended(&pull_id).await;
    assert_eq!(state, "failed");
    assert!(reason.unwrap().contains("bea-hf is connected"));
    assert!(
        rig.hub()
            .iter()
            .all(|seen| !seen.path.contains("config.json")),
        "{:?}",
        rig.hub()
    );
    let files = rig.store().pull_files(&pull_id).await.unwrap();
    assert_eq!(files[0].source.as_deref(), Some("downloaded"));
    assert_eq!(files[1].source, None);
    puller.stop();
}

/// The audit's records of `action` on the pull: (first records, `ok`
/// outcomes, refusals).
async fn pull_records(rig: &Rig, pull_id: &str, action: &str) -> (i64, i64, i64) {
    sqlx::query_as(
        "WITH firsts AS (SELECT audit_id FROM audit \
             WHERE action = $1 AND target_id = $2 AND answers IS NULL AND refusal IS NULL) \
         SELECT (SELECT count(*) FROM firsts), \
                (SELECT count(*) FROM audit WHERE outcome = 'ok' \
                   AND answers IN (SELECT audit_id FROM firsts)), \
                (SELECT count(*) FROM audit \
                   WHERE action = $1 AND target_id = $2 AND refusal IS NOT NULL)",
    )
    .bind(action)
    .bind(pull_id)
    .fetch_one(&rig.store().pool)
    .await
    .unwrap()
}

/// **An admin's cancel stops the running pull at once and keeps its
/// partial**: the fetch stalls past any wait the test takes, the cancel is
/// a first record and its `ok` outcome, the pull ends `cancelled` naming
/// the admin, the partial stands, and the next ask of the file resumes by
/// range from it and verifies.
#[tokio::test]
async fn an_admins_cancel_stops_the_running_pull_and_keeps_its_partial() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Stall);
    let puller = rig.puller(Settings {
        stall: Duration::from_secs(60),
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    cdn_asked(&rig, 1).await;
    let partial = rig
        .blobs()
        .join(format!("{}.incomplete", model_file().sha256.unwrap()));
    let mut held = 0;
    for _ in 0..100 {
        held = std::fs::metadata(&partial).map_or(0, |m| m.len());
        if held > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(held > 0, "the partial holds nothing");
    let web = rig.surface(&puller);
    let uri = format!("/admin/hugging-face/pulls/{pull_id}/cancel");
    let (status, _, body) = send_as(&web, "POST", &uri, Some(String::new()), &rig.cookie).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let pull = rig.store().pull(&pull_id).await.unwrap().unwrap();
    assert_eq!(pull.state, "cancelled");
    assert_eq!(pull.cancelled_by.as_deref(), Some("ada"));
    assert_eq!(pull.reason, None);
    assert_eq!(
        pull_records(&rig, &pull_id, "model pull cancel").await,
        (1, 1, 0)
    );
    let kept = std::fs::metadata(&partial).unwrap().len();
    assert!(kept > 0);
    assert!(!rig.blobs().join(model_file().sha256.unwrap()).exists());

    let (again, asked) = rig.ask(&puller, &[model_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    assert_eq!(rig.ended(&again).await, ("verified".into(), None));
    let cdn = rig.cdn();
    assert_eq!(cdn.len(), 2, "{cdn:?}");
    assert_eq!(cdn[1].range, Some(format!("bytes={kept}-")));
    let (_, _, body) = send_as(&web, "GET", "/admin/hugging-face/pulls", None, &rig.cookie).await;
    assert!(body.contains("cancelled by ada"), "{body}");
    puller.stop();
}

/// **A cancel stops a pull waiting for its response's headers**: the CDN
/// holds its answer past any wait the test takes, the cancel ends the pull,
/// and the next ask runs and verifies at once, never waiting out the stall.
#[tokio::test]
async fn a_cancel_stops_a_pull_waiting_for_its_headers() {
    let Some(rig) = rig().await else { return };
    rig.fakes.modes.lock().unwrap().push_back(Mode::Held);
    let puller = rig.puller(Settings {
        stall: Duration::from_secs(60),
        ..quick()
    });
    let (pull_id, _) = rig.ask(&puller, &[model_file()]).await;
    cdn_asked(&rig, 1).await;
    let web = rig.surface(&puller);
    let uri = format!("/admin/hugging-face/pulls/{pull_id}/cancel");
    let (status, _, body) = send_as(&web, "POST", &uri, Some(String::new()), &rig.cookie).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(rig.ended(&pull_id).await, ("cancelled".into(), None));

    let (again, asked) = rig.ask(&puller, &[model_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    assert_eq!(rig.ended(&again).await, ("verified".into(), None));
    assert_eq!(rig.cdn().len(), 2, "{:?}", rig.cdn());
    rig.fakes.release.notify_one();
    puller.stop();
}

/// **A cancel that lands during a blob's check starts no partial's hash**:
/// step 2 hashes a blob that does not match, the pull is held at step 3 as
/// the check's end would hold it, an admin's cancel lands there, and once
/// released the download stops before its partial is opened or hashed, so
/// the hash under way is the only one that finishes after the cancel.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_during_a_blobs_check_starts_no_partials_hash() {
    let Some(rig) = rig().await else { return };
    std::fs::create_dir_all(rig.blobs()).unwrap();
    let wrong: Vec<u8> = model_bytes().iter().map(|b| b.wrapping_add(1)).collect();
    std::fs::write(rig.blobs().join(model_file().sha256.unwrap()), wrong).unwrap();
    let held = Arc::new((AtomicBool::new(false), AtomicBool::new(false)));
    HOLD_AT_STEP_THREE
        .lock()
        .unwrap()
        .push((rig.blobs(), held.clone()));
    let puller = rig.puller(Settings {
        free_space: hold_at_step_three,
        ..quick()
    });
    let (pull_id, asked) = rig.ask(&puller, &[model_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    for _ in 0..200 {
        if held.0.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        held.0.load(Ordering::SeqCst),
        "the pull never reached step 3"
    );
    // The admin's cancel, written and signalled as the surface does it.
    let web = rig.surface(&puller);
    let uri = format!("/admin/hugging-face/pulls/{pull_id}/cancel");
    let (status, _, body) = send_as(&web, "POST", &uri, Some(String::new()), &rig.cookie).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    held.1.store(true, Ordering::SeqCst);
    // The next ask verifies once the puller has left the cancelled pull.
    let (again, asked) = rig.ask(&puller, &[config_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    assert_eq!(rig.ended(&again).await, ("verified".into(), None));

    let partial = rig
        .blobs()
        .join(format!("{}.incomplete", model_file().sha256.unwrap()));
    let hashed = crate::hugging_face::pull::HASHED_PARTIALS
        .lock()
        .unwrap()
        .contains(&partial);
    assert!(!hashed, "a partial was hashed after the cancel");
    assert!(!partial.exists(), "a partial was opened after the cancel");
    assert_eq!(rig.fakes.fetched.load(Ordering::SeqCst), 0);
    puller.stop();
}

/// **A later cancel never undoes an earlier one**: pull A is held where a
/// hash would hold it, an admin cancels A, asks pull B and cancels B, and
/// once A is released it stops before its download. The worker is free for
/// the next ask, and A records no file it fetched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_later_cancel_never_undoes_an_earlier_one() {
    let Some(rig) = rig().await else { return };
    let held = Arc::new((AtomicBool::new(false), AtomicBool::new(false)));
    HOLD_AT_STEP_THREE
        .lock()
        .unwrap()
        .push((rig.blobs(), held.clone()));
    let puller = rig.puller(Settings {
        free_space: hold_at_step_three,
        ..quick()
    });
    let web = rig.surface(&puller);
    let cancel = |pull_id: String| {
        let (web, cookie) = (web.clone(), rig.cookie.clone());
        async move {
            let uri = format!("/admin/hugging-face/pulls/{pull_id}/cancel");
            let (status, _, body) = send_as(&web, "POST", &uri, Some(String::new()), &cookie).await;
            assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
        }
    };

    let (first, asked) = rig.ask(&puller, &[model_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    for _ in 0..200 {
        if held.0.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        held.0.load(Ordering::SeqCst),
        "the pull never reached step 3"
    );
    cancel(first.clone()).await;
    let (second, asked) = rig.ask(&puller, &[config_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    cancel(second).await;
    held.1.store(true, Ordering::SeqCst);

    let (third, asked) = rig.ask(&puller, &[config_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    assert_eq!(rig.ended(&third).await, ("verified".into(), None));
    assert_eq!(
        rig.fakes.fetched.load(Ordering::SeqCst),
        0,
        "the cancelled pull fetched its file"
    );
    let files = rig.store().pull_files(&first).await.unwrap();
    assert_eq!(files[0].source, None, "{files:?}");
    // B was cancelled before the puller took it, so leaving a pull never
    // removes its id: the puller drops every id once nothing runs.
    for _ in 0..200 {
        if puller.cancels_held() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert_eq!(
        puller.cancels_held(),
        0,
        "a cancelled id outlived the pulls"
    );
    puller.stop();
}

/// **A FIFO at a blob's name is refused and never waited on**: step 2
/// opens the blob without waiting, finds no regular file and fetches the
/// file, whose blob then takes the name. An open that waited would hold the
/// one worker in `open(2)` until a writer came, where no cancel reaches it.
/// A watchdog thread, outside the runtime the open would hold, opens the
/// FIFO's writing end after five seconds, so the test fails rather than
/// hangs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fifo_at_a_blobs_name_is_refused_and_never_waited_on() {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;
    let Some(rig) = rig().await else { return };
    std::fs::create_dir_all(rig.blobs()).unwrap();
    let blob = rig.blobs().join(model_file().sha256.unwrap());
    let name = std::ffi::CString::new(blob.as_os_str().as_bytes()).unwrap();
    // SAFETY: `name` is a NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    let watch = Arc::new((AtomicBool::new(false), AtomicBool::new(false)));
    let watchdog = {
        let (watch, fifo) = (watch.clone(), blob.clone());
        std::thread::spawn(move || {
            for _ in 0..50 {
                if watch.0.load(Ordering::SeqCst) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // An open that waits for a writer returns once one opens.
            let writer = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo);
            watch.1.store(writer.is_ok(), Ordering::SeqCst);
        })
    };
    let puller = rig.puller(quick());
    let (pull_id, asked) = rig.ask(&puller, &[model_file()]).await;
    assert!(asked.is_ok(), "{asked:?}");
    let ended = rig.ended(&pull_id).await;
    watch.0.store(true, Ordering::SeqCst);
    watchdog.join().unwrap();
    assert!(
        !watch.1.load(Ordering::SeqCst),
        "the puller waited on the FIFO until the watchdog opened it"
    );
    assert_eq!(ended, ("verified".into(), None));
    assert!(std::fs::symlink_metadata(&blob).unwrap().is_file());
    assert_eq!(std::fs::read(&blob).unwrap(), model_bytes());
    puller.stop();
}

/// **A cancel takes the admin writes' order**: an identity out of its
/// shape is the ask's fault, recorded nowhere; a person holding no admin
/// grant is refused with one record; an unknown pull is not found and a
/// pull that has ended is the ask's fault, each recorded nowhere.
#[tokio::test]
async fn a_cancel_takes_the_admin_writes_order() {
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let (pull_id, _) = rig.ask(&puller, &[config_file()]).await;
    assert_eq!(rig.ended(&pull_id).await.0, "verified");
    let web = rig.surface(&puller);
    let cancel = |id: &str| format!("/admin/hugging-face/pulls/{id}/cancel");
    let (status, _, _) = send_as(
        &web,
        "POST",
        &cancel("mp-nothex"),
        Some(String::new()),
        &rig.cookie,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (_, bea) = signed_in(&rig.web, rig.store(), &mut authenticator(), "bea").await;
    let (status, _, _) = send_as(&web, "POST", &cancel(&pull_id), Some(String::new()), &bea).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = send_as(
        &web,
        "POST",
        &cancel("mp-0123456789abcdef"),
        Some(String::new()),
        &rig.cookie,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, body) = send_as(
        &web,
        "POST",
        &cancel(&pull_id),
        Some(String::new()),
        &rig.cookie,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        pull_records(&rig, &pull_id, "model pull cancel").await,
        (0, 0, 1)
    );
    assert_eq!(
        pull_records(&rig, "mp-0123456789abcdef", "model pull cancel").await,
        (0, 0, 0)
    );
    assert_eq!(
        rig.store().pull(&pull_id).await.unwrap().unwrap().state,
        "verified"
    );
    puller.stop();
}

/// **The token enters no log line, page or error of a pull**: a pull
/// through the account that verifies and one whose body's digest differs,
/// with every log line at every level captured, the cache's section and
/// every pull's reason, and the audit, none of them carries the token.
#[tokio::test]
async fn a_pull_carries_its_token_into_no_log_line_page_or_error() {
    let captured = crate::agents_tests::Captured::default();
    let _logging = tracing::subscriber::set_default(
        tracing_subscriber::fmt()
            .with_writer(captured.clone())
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .finish(),
    );
    // **A second dispatcher, held for the test's life**: with one scoped
    // dispatcher alone, tracing-core 0.1.36 takes a callsite's interest from
    // the default of whichever thread meets it first
    // (`Dispatchers::rebuilder`), so the puller of a test running beside
    // this one, on a thread with no subscriber, would leave the puller's
    // lines uncaught here. With two registered, it asks each of them.
    let _second = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let Some(rig) = rig().await else { return };
    let puller = rig.puller(quick());
    let (verified, _) = rig.ask(&puller, &[config_file(), model_file()]).await;
    assert_eq!(rig.ended(&verified).await.0, "verified");
    std::fs::remove_file(rig.blobs().join(model_file().sha256.unwrap())).unwrap();
    rig.fakes.modes.lock().unwrap().push_back(Mode::Wrong);
    let (failed, _) = rig.ask(&puller, &[model_file()]).await;
    let (state, reason) = rig.ended(&failed).await;
    assert_eq!(state, "failed");
    let web = rig.surface(&puller);
    let (_, _, section) =
        send_as(&web, "GET", "/admin/hugging-face/pulls", None, &rig.cookie).await;
    assert!(section.contains(&failed), "{section}");
    let audit = sqlx::query_scalar::<_, Option<String>>(
        "SELECT string_agg(row_to_json(a)::text, E'\\n') FROM audit a",
    )
    .fetch_one(&rig.store().pool)
    .await
    .unwrap()
    .unwrap_or_default();
    let logged = String::from_utf8_lossy(&captured.0.lock().unwrap()).into_owned();
    assert!(
        logged.contains(&format!("the pull {failed} failed")),
        "the capture missed the puller's line: {logged}"
    );
    assert!(!logged.contains(TOKEN), "a log line holds the token");
    assert!(!section.contains(TOKEN), "the section holds the token");
    assert!(!reason.unwrap().contains(TOKEN), "a reason holds the token");
    assert!(!audit.contains(TOKEN), "the audit holds the token");
    puller.stop();
}
