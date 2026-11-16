//! conforms: web-authorization-goes-to-hugging-face-alone
//! conforms: web-every-download-names-the-commit
//! conforms: web-no-file-takes-its-blobs-name-before-its-digest-matches
//! conforms: web-a-repositorys-file-names-write-nothing-outside-the-cache
//! conforms: web-a-pull-asked-through-an-account-never-runs-without-its-token
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
use std::sync::atomic::{AtomicUsize, Ordering};
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
}

#[derive(Default)]
struct Fakes {
    hub: Mutex<Vec<Seen>>,
    cdn: Mutex<Vec<Seen>>,
    modes: Mutex<VecDeque<Mode>>,
    cdn_base: Mutex<String>,
    fetched: AtomicUsize,
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
    if mode == Mode::Wrong {
        bytes.iter_mut().for_each(|b| *b = b.wrapping_add(1));
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
        Mode::Whole | Mode::Wrong => Box::pin(tokio_stream::iter(
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
