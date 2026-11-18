//! conforms: web-the-models-roster-is-the-first-agents
//! conforms: web-a-pick-asks-hugging-face-only-after-the-admin-gate
//! conforms: web-an-adapters-pick-carries-a-finite-scale
//! conforms: web-the-base-identity-is-blake3-over-the-parts-in-split-order
//!
//! The models page (Spec 6.2, `src/surfaces/setup/models.rs`), against a
//! fake Hugging Face of its own that lists repositories, answers a
//! file's `HEAD`, serves a file's bytes by range, gates one repository
//! until the test grants it, and counts what it was asked. Two parts show
//! as one choice. A header is read by range, kept, and "not read" past its
//! bound. A gated repository offers no pick until it is granted. A pick
//! asks one pull with its record, the sizes the listing's. A crafted pick
//! and an adapter's pick without a finite scale are refused, saving
//! nothing, and no pick reaches Hugging Face before the admin gate. A
//! role picked again replaces its row. The base's identity is "not read"
//! while it is owed and computed at a start. The models drive's need
//! counts a shared blob once. At most four header reads run at once. Each
//! DB-backed test runs on a store of its own. No byte, name or address
//! here is a real model's or a real host's.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::box_report::{BoxReports, Kept};
use crate::gguf::build::{F32, Gguf, STRING};
use crate::hugging_face::RepoId;
use crate::passkeys_tests::authenticator;
use crate::setup_tests::{Rig, audited, begin, page, rig_full, walk};
use crate::store::Store;
use crate::surfaces::hugging_face::bytes;

pub(crate) const COMMIT: &str = "27b350a791e81d9a4d1ddca1c49282e9ec533768";

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

/// `n` bytes of a pattern seeded by `seed`, standing for a model's tensors.
fn filler(seed: u32, n: usize) -> Vec<u8> {
    (0..n as u32)
        .map(|i| (i.wrapping_mul(2_654_435_761).wrapping_add(seed) >> 13) as u8)
        .collect()
}

fn with_filler(header: Vec<u8>, seed: u32, n: usize) -> Vec<u8> {
    let mut bytes = header;
    bytes.extend(filler(seed, n));
    bytes
}

struct Repo {
    name: &'static str,
    gated: bool,
    files: Vec<(&'static str, Vec<u8>)>,
}

/// The split model's two parts: part 1 a model's header with its split,
/// then its tensors' stand-in, and part 2 other bytes.
pub(crate) fn split_parts() -> (Vec<u8>, Vec<u8>) {
    let header = Gguf::model("granite")
        .u16("split.no", 0)
        .u16("split.count", 2)
        .bytes();
    (with_filler(header, 1, 200_000), filler(2, 150_000))
}

/// The one part of a model kept in git: a model's header, then its
/// tensors' stand-in.
fn in_git_part() -> Vec<u8> {
    with_filler(Gguf::model("granite").bytes(), 12, 5_000)
}

fn twin_part() -> Vec<u8> {
    let header = Gguf::model("granite")
        .u16("split.no", 0)
        .u16("split.count", 2)
        .bytes();
    with_filler(header, 3, 40_000)
}

fn repos() -> Vec<Repo> {
    let (one, two) = split_parts();
    let model = |seed| with_filler(Gguf::model("granite").bytes(), seed, 1_000);
    let adapter = with_filler(
        Gguf::new()
            .string("general.architecture", "granite")
            .string("general.type", "adapter")
            .string("adapter.type", "lora")
            .bytes(),
        4,
        2_000,
    );
    // A string whose length claims 17 MiB, so the header runs past the
    // read's bound.
    let long = Gguf::new()
        .raw("general.name", STRING, &(17u64 << 20).to_le_bytes())
        .bytes();
    let mismatch = with_filler(Gguf::model("granite").u16("split.count", 3).bytes(), 5, 100);
    // A model's `general.type` followed by a float one, the last standing.
    let float_type = with_filler(
        Gguf::model("granite")
            .raw("general.type", F32, &1.5f32.to_le_bytes())
            .bytes(),
        9,
        100,
    );
    let mut many = Vec::new();
    for (at, name) in [
        "a0.gguf", "a1.gguf", "a2.gguf", "a3.gguf", "a4.gguf", "a5.gguf", "a6.gguf", "a7.gguf",
    ]
    .into_iter()
    .enumerate()
    {
        many.push((name, model(10 + at as u32)));
    }
    vec![
        Repo {
            name: "org/split",
            gated: false,
            files: vec![
                ("m-00002-of-00002.gguf", two),
                ("config.json", b"{}\n".to_vec()),
                ("m-00001-of-00002.gguf", one),
            ],
        },
        Repo {
            name: "org/lone",
            gated: false,
            files: vec![("lone.gguf", model(6))],
        },
        Repo {
            name: "org/adapter",
            gated: false,
            files: vec![("adapter.gguf", adapter)],
        },
        Repo {
            name: "org/gated",
            gated: true,
            files: vec![("g.gguf", model(7))],
        },
        Repo {
            name: "org/long",
            gated: false,
            files: vec![("long.gguf", long)],
        },
        Repo {
            name: "org/mismatch",
            gated: false,
            files: vec![
                ("x-00001-of-00002.gguf", mismatch),
                ("x-00002-of-00002.gguf", filler(8, 100)),
            ],
        },
        Repo {
            name: "org/twin",
            gated: false,
            files: vec![
                ("t-00001-of-00002.gguf", twin_part()),
                ("t-00002-of-00002.gguf", twin_part()),
            ],
        },
        Repo {
            name: "org/many",
            gated: false,
            files: many,
        },
        // A private model, which only the token `seen-token` sees.
        Repo {
            name: "org/private",
            gated: false,
            files: vec![("p.gguf", in_git_part())],
        },
        // A model whose one part is kept in git, not LFS.
        Repo {
            name: "org/in-git",
            gated: false,
            files: vec![("in-git.gguf", in_git_part())],
        },
        // A model whose maximum context, 2048, is below the box's
        // max-tokens-per-turn, the header's last value standing.
        Repo {
            name: "org/short",
            gated: false,
            files: vec![(
                "s.gguf",
                with_filler(
                    Gguf::model("granite")
                        .u32("granite.context_length", 2048)
                        .bytes(),
                    11,
                    100,
                ),
            )],
        },
        Repo {
            name: "org/float-type",
            gated: false,
            files: vec![("f.gguf", float_type)],
        },
    ]
}

#[derive(Default)]
pub(crate) struct Fake {
    repos: Vec<Repo>,
    /// Whether the gated repository is granted to the account.
    granted: AtomicBool,
    /// Every request, of any kind.
    requests: AtomicUsize,
    /// The ranges of the header reads, each `bytes=0-<end>`.
    header_reads: Mutex<Vec<(String, String)>>,
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
    /// How long a header read is held before its answer.
    header_delay: Mutex<Duration>,
}

impl Fake {
    fn repo(&self, owner: &str, name: &str) -> Option<&Repo> {
        let full = format!("{owner}/{name}");
        self.repos.iter().find(|r| r.name == full)
    }

    fn header_reads(&self) -> usize {
        self.header_reads.lock().unwrap().len()
    }
}

async fn listing(
    State(fake): State<Arc<Fake>>,
    UrlPath((owner, name, revision)): UrlPath<(String, String, String)>,
) -> Response {
    fake.requests.fetch_add(1, Ordering::SeqCst);
    let Some(repo) = fake.repo(&owner, &name) else {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "RepoNotFound")],
            "",
        )
            .into_response();
    };
    if revision != "main" && revision != COMMIT {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "RevisionNotFound")],
            "",
        )
            .into_response();
    }
    let siblings: Vec<serde_json::Value> = repo
        .files
        .iter()
        .map(|(path, bytes)| {
            let mut sibling = serde_json::json!({
                "rfilename": path,
                "size": bytes.len(),
                "blobId": git_blob_id(bytes),
            });
            // A `.gguf` file is kept in LFS, but for `in-git.gguf`, which
            // stands for a small file Hugging Face keeps in git.
            if path.ends_with(".gguf") && *path != "in-git.gguf" {
                sibling["lfs"] = serde_json::json!({ "sha256": sha256(bytes) });
            }
            sibling
        })
        .collect();
    axum::Json(serde_json::json!({
        "sha": COMMIT,
        "gated": if repo.gated { serde_json::json!("manual") } else { serde_json::json!(false) },
        "private": false,
        "siblings": siblings,
    }))
    .into_response()
}

/// **A file's resolve**: gated where the repository is gated and not
/// granted, the commit alone, and the bytes from the range asked.
async fn resolve(
    State(fake): State<Arc<Fake>>,
    headers: HeaderMap,
    UrlPath((owner, name, revision, path)): UrlPath<(String, String, String, String)>,
) -> Response {
    fake.requests.fetch_add(1, Ordering::SeqCst);
    let Some(repo) = fake.repo(&owner, &name) else {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "RepoNotFound")],
            "",
        )
            .into_response();
    };
    if repo.gated && !fake.granted.load(Ordering::SeqCst) {
        return (StatusCode::FORBIDDEN, [("x-error-code", "GatedRepo")], "").into_response();
    }
    // A private repository answers 404 to a token that cannot see it, as
    // Hugging Face answers one.
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if repo.name == "org/private" && bearer != Some("Bearer seen-token") {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "RepoNotFound")],
            "",
        )
            .into_response();
    }
    if revision != COMMIT {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "RevisionNotFound")],
            "",
        )
            .into_response();
    }
    let Some((_, bytes)) = repo.files.iter().find(|(p, _)| *p == path) else {
        return (
            StatusCode::NOT_FOUND,
            [("x-error-code", "EntryNotFound")],
            "",
        )
            .into_response();
    };
    let range = headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let len = bytes.len();
    let Some(range) = range else {
        return (
            [(header::CONTENT_LENGTH, len.to_string())],
            Body::from(bytes.clone()),
        )
            .into_response();
    };
    let (from, to) = range
        .strip_prefix("bytes=")
        .and_then(|r| r.split_once('-'))
        .expect("a range of bytes");
    let start: usize = from.parse().unwrap();
    let end = if to.is_empty() {
        len - 1
    } else {
        to.parse::<usize>().unwrap().min(len - 1)
    };
    let header_read = start == 0 && !to.is_empty();
    if header_read {
        fake.header_reads
            .lock()
            .unwrap()
            .push((format!("{owner}/{name}/{path}"), range.clone()));
        let now = fake.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        fake.most_in_flight.fetch_max(now, Ordering::SeqCst);
        let delay = *fake.header_delay.lock().unwrap();
        tokio::time::sleep(delay).await;
        fake.in_flight.fetch_sub(1, Ordering::SeqCst);
    }
    (
        StatusCode::PARTIAL_CONTENT,
        [
            (header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}")),
            (header::CONTENT_LENGTH, (end + 1 - start).to_string()),
        ],
        Body::from(bytes[start..=end].to_vec()),
    )
        .into_response()
}

/// The fake on a listener of its own, and its base.
pub(crate) async fn fake() -> (Arc<Fake>, String) {
    let fake = Arc::new(Fake {
        repos: repos(),
        ..Default::default()
    });
    let router = axum::Router::new()
        .route(
            "/api/models/{owner}/{name}/revision/{revision}",
            get(listing),
        )
        .route("/{owner}/{name}/resolve/{revision}/{*path}", get(resolve))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, router).await });
    (fake, base)
}

/// A rig over the fake, with a model cache where `cached`, and no box
/// reporting.
async fn rig(cached: bool) -> Option<(Rig, Arc<Fake>, String, tempfile::TempDir)> {
    rig_boxed(cached, Arc::new(Kept)).await
}

/// A rig over the fake, with a model cache where `cached`, and what the
/// box reports answered by `boxes`: the rig, the fake, its base and the
/// cache's directory.
pub(crate) async fn rig_boxed(
    cached: bool,
    boxes: Arc<dyn BoxReports>,
) -> Option<(Rig, Arc<Fake>, String, tempfile::TempDir)> {
    let cache = tempfile::tempdir().unwrap();
    let (fake, base) = fake().await;
    let rig = rig_full(
        "127.0.0.1:8081",
        Some("console.test:7443"),
        cached.then(|| cache.path().to_path_buf()),
        &base,
        boxes,
    )
    .await?;
    Some((rig, fake, base, cache))
}

/// An admin and a setup begun by them.
pub(crate) async fn setup_of(rig: &Rig, name: &str) -> (String, String) {
    let (_, ada) = admin(&rig.app, rig.store(), &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, name).await;
    (ada, setup.unwrap())
}

fn models(setup: &str, repository: &str) -> String {
    format!("/setup/{setup}/models?repository={repository}&revision=main")
}

fn header_of(setup: &str, repository: &str, choice: &str) -> String {
    format!("/setup/{setup}/models/header?repository={repository}&commit={COMMIT}&choice={choice}")
}

/// A pick's form.
pub(crate) fn pick(
    version: &str,
    role: &str,
    repository: &str,
    choice: &str,
    scale: Option<&str>,
) -> String {
    let mut fields = vec![
        ("version", version),
        ("role", role),
        ("repository", repository),
        ("commit", COMMIT),
        ("choice", choice),
    ];
    if let Some(scale) = scale {
        fields.push(("scale", scale));
    }
    form(&fields)
}

pub(crate) async fn post_pick(
    rig: &Rig,
    session: &str,
    setup: &str,
    body: String,
) -> (StatusCode, String) {
    let (status, _, answer) = send_as(
        &rig.app,
        "POST",
        &format!("/setup/{setup}/models"),
        Some(body),
        session,
    )
    .await;
    (status, answer)
}

pub(crate) async fn count(s: &Store, table: &'static str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// A fragment answered 200 under a session: a part of a page, unframed.
async fn part(rig: &Rig, uri: &str, session: &str) -> String {
    let (status, _, body) = send_as(&rig.app, "GET", uri, None, session).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    assert!(
        !body.contains("<nav class=\"masthead\">"),
        "a fragment is unframed: {body}"
    );
    // The apostrophe as askama escapes it, read back for the assertions.
    body.replace("&#39;", "'")
}

/// The pull a setup's role names.
pub(crate) async fn pull_of(s: &Store, setup: &str, role: &str) -> String {
    sqlx::query_scalar("SELECT pull_id FROM agent_setup_model WHERE setup_id = $1 AND role = $2")
        .bind(setup)
        .bind(role)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The pull's state once it leaves `running`, within twenty seconds.
pub(crate) async fn ended(s: &Store, pull_id: &str) -> String {
    for _ in 0..400 {
        let state = s.pull(pull_id).await.unwrap().unwrap().state;
        if state != "running" {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the pull {pull_id} still runs");
}

/// **Two parts of one stem and count show as one choice with their total
/// size**, named by its first part, its header's fragment offered, the
/// revision resolved to its commit, and a file not GGUF no choice.
#[tokio::test]
async fn two_parts_show_as_one_choice_with_their_total_size() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let listed = page(&rig.app, &models(&setup, "org/split"), &ada).await;
    let (one, two) = split_parts();
    assert!(
        listed.contains(&format!(r#"<span class="mono">{COMMIT}</span>"#)),
        "{listed}"
    );
    assert!(
        listed.contains(r#"<h3 class="mono">m-00001-of-00002.gguf</h3>"#),
        "{listed}"
    );
    assert!(
        !listed.contains(r#"<h3 class="mono">m-00002-of-00002.gguf</h3>"#),
        "{listed}"
    );
    assert!(!listed.contains("config.json"), "{listed}");
    assert!(
        listed.contains(&format!(
            "2 parts, {}",
            bytes((one.len() + two.len()) as u64)
        )),
        "{listed}"
    );
    assert!(
        listed.contains(
            &header_of(&setup, "org%2Fsplit", "m-00001-of-00002.gguf").replace('&', "&#38;")
        ),
        "{listed}"
    );
    assert!(walk(&listed).is_empty(), "{:?}", walk(&listed));
    // Nothing asked: the form and the roles, every pane walked.
    let bare = page(&rig.app, &format!("/setup/{setup}/models"), &ada).await;
    assert!(bare.contains("No decoder is picked."), "{bare}");
    assert!(bare.contains("Beyond the first agent"), "{bare}");
    assert!(bare.contains(r#"<span class="backing unbuilt">unbuilt</span><span class="status hold">hold</span></header>"#));
    assert!(walk(&bare).is_empty(), "{:?}", walk(&bare));
}

/// **A choice's header is read by range through the commit before any
/// download**, its facts shown with llama.cpp's word for the precision,
/// kept so a second fragment reads nothing, and a decoder's form offered
/// for a model's file. A header past the bound is "not read" and a split
/// count the names contradict is shown with its reason, neither offering
/// a pick.
#[tokio::test]
async fn the_header_is_read_by_range_and_one_past_the_bound_is_not_read() {
    let Some((rig, fake, _, _cache)) = rig(true).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let fragment = part(
        &rig,
        &header_of(&setup, "org/split", "m-00001-of-00002.gguf"),
        &ada,
    )
    .await;
    assert!(fragment.contains("<td>Q4_K_M</td>"), "{fragment}");
    assert!(fragment.contains("<td>131072</td>"), "{fragment}");
    assert!(
        fragment.contains("<td>part 0 of 2, counted from 0</td>"),
        "{fragment}"
    );
    assert!(
        fragment.contains(r#"name="role" value="decoder""#),
        "{fragment}"
    );
    assert!(!fragment.contains(r#"name="scale""#), "{fragment}");
    assert_eq!(
        *fake.header_reads.lock().unwrap(),
        [(
            "org/split/m-00001-of-00002.gguf".to_owned(),
            "bytes=0-16777215".to_owned()
        )]
    );
    part(
        &rig,
        &header_of(&setup, "org/split", "m-00001-of-00002.gguf"),
        &ada,
    )
    .await;
    assert_eq!(fake.header_reads(), 1, "the header is kept");

    let long = part(&rig, &header_of(&setup, "org/long", "long.gguf"), &ada).await;
    assert!(
        long.contains("not read: the header runs past the read's bound of 16 MiB"),
        "{long}"
    );
    assert!(!long.contains("<form"), "{long}");
    assert!(!long.contains("Q4_K_M"), "{long}");

    let mismatch = part(
        &rig,
        &header_of(&setup, "org/mismatch", "x-00001-of-00002.gguf"),
        &ada,
    )
    .await;
    assert!(
        mismatch.contains("the header's split count is 3, and the file names say 2 parts"),
        "{mismatch}"
    );
    assert!(!mismatch.contains("<form"), "{mismatch}");

    let adapter = part(
        &rig,
        &header_of(&setup, "org/adapter", "adapter.gguf"),
        &ada,
    )
    .await;
    assert!(
        adapter.contains(r#"name="role" value="adapter""#),
        "{adapter}"
    );
    assert!(
        adapter.contains(r#"<input name="scale" inputmode="decimal" required>"#),
        "{adapter}"
    );
    assert!(
        adapter.contains("<td>not read</td>"),
        "the adapter's context: {adapter}"
    );
}

/// **A gated repository the account lacks shows its accept link and a
/// check again, and offers no pick**: no header is read and a pick is
/// refused, saving nothing. Once the fake grants it, the header is
/// offered and the pick lands.
#[tokio::test]
async fn a_gated_repository_offers_no_pick_until_it_is_granted() {
    let Some((rig, fake, base, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let gated = page(&rig.app, &models(&setup, "org/gated"), &ada).await;
    assert!(
        gated.contains(&format!(r#"<a href="{base}org/gated""#)),
        "{gated}"
    );
    assert!(
        gated.contains(&format!(
            r#"<a href="/setup/{setup}/models?repository=org%2Fgated&#38;revision=main">check again</a>"#
        )),
        "{gated}"
    );
    assert!(!gated.contains("models/header?"), "{gated}");
    assert!(walk(&gated).is_empty(), "{:?}", walk(&gated));
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/gated", "g.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("gated"), "{body}");
    assert_eq!(fake.header_reads(), 0);
    assert_eq!(count(s, "model_pull").await, 0);
    assert_eq!(audited(s, "setup pick").await, (0, 0, 0, 0));

    fake.granted.store(true, Ordering::SeqCst);
    let granted = page(&rig.app, &models(&setup, "org/gated"), &ada).await;
    assert!(granted.contains("models/header?"), "{granted}");
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/gated", "g.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(count(s, "model_pull").await, 1);
}

/// **A pick asks one pull** of the choice's parts in split order, the sizes
/// and digests Hugging Face's listing gave, its role row naming it and
/// keeping the header's facts, a first record and its `ok` outcome, and
/// the page shows the pull. A stale version answers 409 and writes no pull.
#[tokio::test]
async fn a_pick_asks_one_pull_and_the_page_shows_its_record() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/split", "m-00001-of-00002.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(count(s, "model_pull").await, 1);
    let pull_id = pull_of(s, &setup, "decoder").await;
    let files = s.pull_files(&pull_id).await.unwrap();
    let (one, two) = split_parts();
    let listed: Vec<(&str, u64, Option<String>)> = files
        .iter()
        .map(|f| (f.file.path.as_str(), f.file.size, f.file.sha256.clone()))
        .collect();
    assert_eq!(
        listed,
        [
            (
                "m-00001-of-00002.gguf",
                one.len() as u64,
                Some(sha256(&one))
            ),
            (
                "m-00002-of-00002.gguf",
                two.len() as u64,
                Some(sha256(&two))
            ),
        ]
    );
    let row = &s.setup_models(&setup).await.unwrap()[0];
    assert_eq!(
        (row.role.as_str(), row.choice.as_str()),
        ("decoder", "m-00001-of-00002.gguf")
    );
    assert_eq!(row.scale, None);
    assert_eq!(row.header["context_length"], 131_072);
    assert_eq!(audited(s, "setup pick").await, (1, 1, 0, 0));
    assert_eq!(s.setup(&setup).await.unwrap().unwrap().version, 2);

    let shown = page(&rig.app, &format!("/setup/{setup}/models"), &ada).await;
    assert!(shown.contains(&pull_id), "{shown}");
    assert!(
        shown.contains("<td>Q4_K_M</td>"),
        "the role keeps its facts: {shown}"
    );
    assert!(walk(&shown).is_empty(), "{:?}", walk(&shown));
    let (status, _, fragment) = send_as(
        &rig.app,
        "GET",
        &format!("/setup/{setup}/models/pull"),
        None,
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fragment}");
    assert!(fragment.contains(&pull_id), "{fragment}");

    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/lone", "lone.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        count(s, "model_pull").await,
        1,
        "a stale pick writes no pull"
    );
    assert_eq!(audited(s, "setup pick").await, (2, 1, 1, 0));
}

/// **A crafted pick naming a second model, a second adapter role or the
/// CPU is refused, saving nothing**: answered 400 before anything is
/// looked up, so no record, no row, no pull and no request to Hugging
/// Face.
#[tokio::test]
async fn a_crafted_pick_is_refused_saving_nothing() {
    let Some((rig, fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let base = |extra: &[(&str, &str)]| {
        let mut fields = vec![
            ("version", "1"),
            ("role", "decoder"),
            ("repository", "org/lone"),
            ("commit", COMMIT),
            ("choice", "lone.gguf"),
        ];
        fields.extend_from_slice(extra);
        form(&fields)
    };
    for (why, crafted) in [
        (
            "a second model",
            base(&[("choice", "m-00001-of-00002.gguf")]),
        ),
        (
            "a second model's repository",
            base(&[("repository", "org/split")]),
        ),
        ("a second role", base(&[("role", "adapter")])),
        (
            "a second adapter role",
            pick("1", "adapter2", "org/adapter", "adapter.gguf", Some("1")),
        ),
        (
            "a classifier-only agent",
            pick("1", "classifier", "org/lone", "lone.gguf", None),
        ),
        ("the CPU", base(&[("device", "cpu")])),
        ("placement on the CPU", base(&[("placement", "cpu")])),
    ] {
        let (status, body) = post_pick(&rig, &ada, &setup, crafted).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{why}: {body}");
    }
    assert_eq!(
        fake.requests.load(Ordering::SeqCst),
        0,
        "no request reached Hugging Face"
    );
    assert_eq!(count(s, "agent_setup_model").await, 0);
    assert_eq!(count(s, "model_pull").await, 0);
    assert_eq!(audited(s, "setup pick").await, (0, 0, 0, 0));
    assert_eq!(s.setup(&setup).await.unwrap().unwrap().version, 1);
}

/// **With `model_cache` unset the page says why and offers no pick**: the
/// header's facts are shown without a form, and a pick is refused at 503,
/// saving nothing.
#[tokio::test]
async fn without_a_cache_the_page_says_why_and_offers_no_pick() {
    let Some((rig, _fake, _, _cache)) = rig(false).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let shown = page(&rig.app, &models(&setup, "org/lone"), &ada).await;
    let why = crate::store::admin::Refusal::NoModelCache.to_string();
    assert!(shown.contains(&why), "{shown}");
    let fragment = part(&rig, &header_of(&setup, "org/lone", "lone.gguf"), &ada).await;
    assert!(fragment.contains("<td>Q4_K_M</td>"), "{fragment}");
    assert!(fragment.contains(&why), "{fragment}");
    assert!(!fragment.contains("<form"), "{fragment}");
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/lone", "lone.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(count(s, "model_pull").await, 0);
    assert_eq!(count(s, "agent_setup_model").await, 0);
    assert_eq!(audited(s, "setup pick").await, (0, 0, 0, 0));
}

/// **An adapter's pick carries a finite scale**: one without a scale, or
/// with one that is not finite, is refused, saving nothing. A decoder's
/// pick of an adapter's file and an adapter's pick of a model's are
/// refused by the header's type, saving nothing. An adapter's pick with a
/// finite scale keeps it.
#[tokio::test]
async fn an_adapters_pick_carries_a_finite_scale() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    for scale in [None, Some(""), Some("inf"), Some("NaN"), Some("1e400")] {
        let (status, body) = post_pick(
            &rig,
            &ada,
            &setup,
            pick("1", "adapter", "org/adapter", "adapter.gguf", scale),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{scale:?}: {body}");
    }
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/adapter", "adapter.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("the file is an adapter's, and the pick's role is the decoder"),
        "{body}"
    );
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "adapter", "org/lone", "lone.gguf", Some("1")),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("the file is a decoder's, and the pick's role is the adapter"),
        "{body}"
    );
    assert_eq!(count(s, "agent_setup_model").await, 0);
    assert_eq!(count(s, "model_pull").await, 0);
    assert_eq!(audited(s, "setup pick").await, (0, 0, 0, 0));

    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "adapter", "org/adapter", "adapter.gguf", Some("0.5")),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let row = &s.setup_models(&setup).await.unwrap()[0];
    assert_eq!((row.role.as_str(), row.scale), ("adapter", Some(0.5)));
    let shown = page(&rig.app, &format!("/setup/{setup}/models"), &ada).await;
    assert!(
        shown.contains(r#"<tr><th>Scale</th><td class="mono">0.5</td></tr>"#),
        "{shown}"
    );
}

/// **A named fact of the wrong kind refuses the pick** (Spec 6.2): a
/// model whose `general.type` is followed by a float one is shown "not
/// read" with the reason and no pick's form, and a crafted pick of it is
/// refused with nothing written, never taken as a decoder's.
#[tokio::test]
async fn a_named_fact_of_the_wrong_kind_refuses_the_pick() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let fragment = part(&rig, &header_of(&setup, "org/float-type", "f.gguf"), &ada).await;
    assert!(
        fragment.contains("not read: general.type is not a string"),
        "{fragment}"
    );
    assert!(!fragment.contains("<form"), "{fragment}");
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/float-type", "f.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("its header was not read: general.type is not a string"),
        "{body}"
    );
    assert_eq!(count(s, "agent_setup_model").await, 0);
    assert_eq!(count(s, "model_pull").await, 0);
}

/// **A pick asks Hugging Face only after the admin gate**: a person
/// without an admin grant is refused the page, the header, the pulls and
/// the pick, the pick's refusal one record, and the fake was asked
/// nothing.
#[tokio::test]
async fn a_pick_asks_hugging_face_only_after_the_admin_gate() {
    let Some((rig, fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (_, setup) = setup_of(&rig, "karl").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    for uri in [
        models(&setup, "org/lone"),
        header_of(&setup, "org/lone", "lone.gguf"),
        format!("/setup/{setup}/models/pull"),
    ] {
        let (status, _, body) = send_as(&rig.app, "GET", &uri, None, &bea).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
    }
    let (status, body) = post_pick(
        &rig,
        &bea,
        &setup,
        pick("1", "decoder", "org/lone", "lone.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body, crate::surfaces::admin::NOT_AN_ADMIN);
    assert_eq!(fake.requests.load(Ordering::SeqCst), 0);
    assert_eq!(audited(s, "setup pick").await, (0, 0, 0, 1));
    assert_eq!(count(s, "model_pull").await, 0);
}

/// **A pick for a role the setup holds replaces that role's row**, and
/// the files the earlier pick pulled stay in the cache.
#[tokio::test]
async fn a_role_picked_again_replaces_its_row() {
    let Some((rig, _fake, _, cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/lone", "lone.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let first = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &first).await, "verified");
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("2", "decoder", "org/split", "m-00001-of-00002.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let rows = s.setup_models(&setup).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].repository, "org/split");
    assert_ne!(rows[0].pull_id, first);
    let lone = crate::hugging_face::cache::Cache::at(cache.path()).unwrap();
    let kept = s.pull_files(&first).await.unwrap();
    let blob = lone
        .blob(&RepoId::parse("org/lone").unwrap(), kept[0].file.blob())
        .unwrap();
    assert!(
        blob.is_file(),
        "the earlier pick's blob stays: {}",
        blob.display()
    );
    let later = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &later).await, "verified");
}

/// The pulls fragment, read until it shows `wanted`, for ten seconds at
/// most, under the hasher's idle recheck of fifteen.
async fn shows(rig: &Rig, uri: &str, session: &str, wanted: &str) -> String {
    let mut shown = String::new();
    for _ in 0..200 {
        shown = send_as(&rig.app, "GET", uri, None, session).await.2;
        if shown.contains(wanted) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    shown
}

fn split_identity() -> String {
    let (one, two) = split_parts();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&one);
    hasher.update(&two);
    hasher.finalize().to_hex().to_string()
}

/// **The base's identity is computed from the cache**: once the decoder's
/// pull is verified and while no row holds its digest, the page shows it
/// "not read"; a start of the hasher that finds it, with no word of the
/// puller's pending, as after a restart, hashes the cached parts in split
/// order, BLAKE3 over part 1's bytes then part 2's, and keeps one row
/// naming the pull.
#[tokio::test]
async fn the_base_identity_is_owed_until_a_start_hashes_it() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let pulls = format!("/setup/{setup}/models/pull");
    let fragment = send_as(&rig.app, "GET", &pulls, None, &ada).await.2;
    assert!(
        fragment.contains("not read: no decoder is picked"),
        "{fragment}"
    );
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/split", "m-00001-of-00002.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let pull_id = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &pull_id).await, "verified");
    let fragment = send_as(&rig.app, "GET", &pulls, None, &ada).await.2;
    assert!(fragment.contains("not read: its hash runs"), "{fragment}");

    let (one, two) = split_parts();
    let expected = split_identity();
    let hashing = crate::hugging_face::base_identity::BaseIdentities::start(
        s.clone(),
        rig.puller.cache().cloned(),
        Arc::new(tokio::sync::Notify::new()),
    );
    let shown = shows(&rig, &pulls, &ada, &expected).await;
    assert!(shown.contains(&expected), "{shown}");
    let kept = s
        .base_identity("org/split", COMMIT, "m-00001-of-00002.gguf")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.pull_id, pull_id);
    assert_eq!(kept.bytes, (one.len() + two.len()) as i64);
    assert_eq!(count(s, "base_identity").await, 1);
    drop(hashing);
}

/// **A verified pull wakes the hasher**: a hasher running before the pick
/// hashes the decoder's parts once the puller verifies them, without
/// waiting for its idle recheck.
#[tokio::test]
async fn a_verified_pull_wakes_the_hasher() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let hashing = crate::hugging_face::base_identity::BaseIdentities::start(
        s.clone(),
        rig.puller.cache().cloned(),
        rig.puller.verified(),
    );
    // The hasher's first read of the owed choices, at its start, finds
    // none.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/split", "m-00001-of-00002.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let pull_id = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &pull_id).await, "verified");
    let expected = split_identity();
    let shown = shows(
        &rig,
        &format!("/setup/{setup}/models/pull"),
        &ada,
        &expected,
    )
    .await;
    assert!(shown.contains(&expected), "{shown}");
    drop(hashing);
}

/// **A 404 is never kept, so another account reads through** (Spec 6.2,
/// CodeRabbit on #129): Hugging Face answers 404 to an account that cannot
/// see a private repository, so that answer holds for the read and not for
/// the file. The header memory keeps no such answer, and the same file read
/// under a token that sees it is read. An answer the file's own bytes give,
/// a fact of the wrong kind here, is kept, and its second read asks Hugging
/// Face nothing.
#[tokio::test]
async fn a_404_is_never_kept_so_another_account_reads_through() {
    let (fake, base) = fake().await;
    let client = crate::hugging_face::Client::with_base(&base).unwrap();
    let headers = crate::hugging_face::headers::Headers::new();
    let unseen = crate::hugging_face::Token::parse("unseen-token").unwrap();
    let seen = crate::hugging_face::Token::parse("seen-token").unwrap();
    let private = RepoId::parse("org/private").unwrap();

    let first = headers
        .read(&client, Some(&unseen), &private, COMMIT, "p.gguf")
        .await
        .unwrap();
    assert!(
        !matches!(first, crate::hugging_face::HeaderRead::Read(_)),
        "{first:?}"
    );
    assert_eq!(headers.kept(&private, COMMIT, "p.gguf"), None);
    let second = headers
        .read(&client, Some(&seen), &private, COMMIT, "p.gguf")
        .await
        .unwrap();
    assert!(
        matches!(second, crate::hugging_face::HeaderRead::Read(_)),
        "{second:?}"
    );

    let float = RepoId::parse("org/float-type").unwrap();
    let malformed = headers
        .read(&client, Some(&seen), &float, COMMIT, "f.gguf")
        .await
        .unwrap();
    assert!(
        matches!(malformed, crate::hugging_face::HeaderRead::NotRead(_)),
        "{malformed:?}"
    );
    let asked = fake.requests.load(Ordering::SeqCst);
    let again = headers
        .read(&client, Some(&seen), &float, COMMIT, "f.gguf")
        .await
        .unwrap();
    assert_eq!(again, malformed);
    assert_eq!(fake.requests.load(Ordering::SeqCst), asked, "kept");
}

/// **A decoder part kept in git is hashed from its blob** (Spec 6.2, Codex
/// on #129): the cache names a file kept in git by its git blob id, and the
/// verified pull row holds the console's computed SHA-256 beside it. The
/// hasher takes the blob's name from the row's `lfs` flag, as the tuple's
/// parts do, so it opens the part's blob and keeps its identity, BLAKE3
/// over the part's bytes.
#[tokio::test]
async fn a_git_kept_decoder_part_is_hashed_from_its_blob() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/in-git", "in-git.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let pull_id = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &pull_id).await, "verified");

    // The row: kept in git, its git blob id and the computed SHA-256.
    let part = in_git_part();
    let (lfs, blob_id, digest): (bool, String, Option<String>) =
        sqlx::query_as("SELECT lfs, blob_id, sha256 FROM model_pull_file WHERE pull_id = $1")
            .bind(&pull_id)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert!(!lfs, "the part is kept in git");
    assert_eq!(blob_id, git_blob_id(&part));
    assert_eq!(digest, Some(sha256(&part)));
    // The cache: the blob under its git id, and none under the SHA-256.
    let cache = rig.puller.cache().cloned().expect("a cache");
    let repo = RepoId::parse("org/in-git").unwrap();
    assert!(cache.blob(&repo, &blob_id).unwrap().is_file());
    assert!(!cache.blob(&repo, &sha256(&part)).unwrap().exists());

    let expected = blake3::hash(&part).to_hex().to_string();
    let hashing = crate::hugging_face::base_identity::BaseIdentities::start(
        s.clone(),
        Some(cache),
        Arc::new(tokio::sync::Notify::new()),
    );
    let pulls = format!("/setup/{setup}/models/pull");
    let shown = shows(&rig, &pulls, &ada, &expected).await;
    assert!(shown.contains(&expected), "{shown}");
    let kept = s
        .base_identity("org/in-git", COMMIT, "in-git.gguf")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.pull_id, pull_id);
    assert_eq!(kept.bytes, part.len() as i64);
    drop(hashing);
}

/// **The models drive's need counts a blob two paths share once**: two
/// parts with the same bytes are one blob.
#[tokio::test]
async fn the_models_drives_need_counts_a_shared_blob_once() {
    let Some((rig, _fake, _, _cache)) = rig(true).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let drives = format!("/setup/{setup}/drives");
    let before = page(&rig.app, &drives, &ada).await;
    assert!(before.contains("No model is picked."), "{before}");
    assert!(walk(&before).is_empty(), "{:?}", walk(&before));
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("1", "decoder", "org/twin", "t-00001-of-00002.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let after = page(&rig.app, &drives, &ada).await;
    let one = twin_part().len() as u64;
    assert!(
        after.contains(&format!(
            "{}: the 1 distinct blob of the setup's 1 pick",
            bytes(one)
        )),
        "{after}"
    );
    assert!(walk(&after).is_empty(), "{:?}", walk(&after));
}

/// **At most four header reads run at once across the console**: eight
/// fragments asked together, each read held a while, never put more than
/// four in flight.
#[tokio::test]
async fn at_most_four_header_reads_run_at_once() {
    let Some((rig, fake, _, _cache)) = rig(true).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    *fake.header_delay.lock().unwrap() = Duration::from_millis(300);
    let asks: Vec<_> = (0..8)
        .map(|n| {
            let app = rig.app.clone();
            let uri = header_of(&setup, "org/many", &format!("a{n}.gguf"));
            let ada = ada.clone();
            tokio::spawn(async move { send_as(&app, "GET", &uri, None, &ada).await })
        })
        .collect();
    for ask in asks {
        let (status, _, body) = ask.await.unwrap();
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("<td>Q4_K_M</td>"), "{body}");
    }
    assert_eq!(fake.header_reads(), 8);
    let most = fake.most_in_flight.load(Ordering::SeqCst);
    assert!((2..=4).contains(&most), "{most} in flight at once");
}
