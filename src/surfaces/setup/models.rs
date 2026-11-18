//! conforms: web-the-models-roster-is-the-first-agents
//! conforms: web-a-pick-asks-hugging-face-only-after-the-admin-gate
//! conforms: web-an-adapters-pick-carries-a-finite-scale
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//!
//! **The models page** (Spec 6.2), a child of the setup pages' module.
//! `GET /setup/{setup}/models` draws the setup's two roles, the first
//! agent's roster of one decoder and one adapter, the rest of the roster
//! drawn unbuilt, and, for a repository and a revision asked in its form,
//! the commit the revision resolves to and each choice with its size. Each
//! complete choice's header is its own fragment (`GET
//! /setup/{setup}/models/header`), read by range through the commit and
//! kept in memory (`hugging_face::headers`), with the pick's form where the
//! header's `general.type` admits the role. The setup's pulls and the
//! base's identity are a fragment the page re-fetches every 15 s (`GET
//! /setup/{setup}/models/pull`) through the session's background read.
//!
//! **The pick** (`POST /setup/{setup}/models`) takes the admin writes'
//! order (Spec 2.13): the form parsed to its shape, where a field the pick
//! does not take, a field given twice, a role beyond the roster, a scale on
//! a decoder's pick and an adapter's pick without a finite scale are each
//! refused, saving nothing; then the admin gate with its one refusal
//! record; then the setup, the cache, the account and **Hugging Face's
//! listing and header, asked only after the gate**, so a person without an
//! admin grant causes no request to it; then the audited write, the role's
//! row and the pull's rows in one transaction, and the puller woken. The
//! sizes and digests are the listing's at the commit, never the form's. A
//! token Hugging Face refuses is one refusal record, as on the account's
//! page; every other answer of Hugging Face's is resolved before the first
//! record and records nothing.

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use super::{Frame, MALFORMED_SETUP, Page, Seams, admin_read, frame, landed, when};
use crate::gguf::{self, Header, KvHeads};
use crate::hugging_face::choice::{self, Choice, part_of};
use crate::hugging_face::seal::SealKey;
use crate::hugging_face::{
    Access, Gated, HeaderRead, ModelAt, RepoId, Token, is_commit, is_repo_path, is_revision,
};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity::is_setup_id;
use crate::store::setup::{Pick, SetupModel};
use crate::surfaces::admin::{NOT_AN_ADMIN, fault, first_of, refused, settled, step_four, writer};
use crate::surfaces::gate::{self, Session};
use crate::surfaces::hugging_face::bytes;

/// The pick's action, as its audit records name it.
const ACTION: &str = "setup pick";

/// The fields a pick's form carries, each once.
const PICK_FIELDS: [&str; 6] = ["version", "role", "repository", "commit", "choice", "scale"];

pub(super) fn routes(seams: &Seams) -> Router<Store> {
    let page_seams = seams.clone();
    let pick_seams = seams.clone();
    let header_seams = seams.clone();
    let pull_seams = seams.clone();
    Router::new()
        .route(
            "/setup/{setup}/models",
            get(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Query(look): Query<Look>| {
                    let seams = page_seams.clone();
                    async move { page(store, &seams, headers, &setup, look).await }
                },
            )
            .post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Form(fields): Form<Vec<(String, String)>>| {
                    let seams = pick_seams.clone();
                    async move { pick(store, &seams, headers, &setup, fields).await }
                },
            ),
        )
        .route(
            "/setup/{setup}/models/header",
            get(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Query(ask): Query<HeaderAsk>| {
                    let seams = header_seams.clone();
                    async move { header(store, &seams, headers, &setup, ask).await }
                },
            ),
        )
        .route(
            "/setup/{setup}/models/pull",
            get(
                move |State(store): State<Store>, headers: HeaderMap, Path(setup): Path<String>| {
                    let seams = pull_seams.clone();
                    async move { pulls(store, &seams, headers, &setup).await }
                },
            ),
        )
}

/// **The account Hugging Face is asked through**: none where none is
/// connected, and where one is, its token, or why it cannot be opened.
enum Through {
    Anonymous,
    Account { name: String, token: Token },
    Shut(Refusal),
}

impl Through {
    fn token(&self) -> Option<&Token> {
        match self {
            Through::Account { token, .. } => Some(token),
            _ => None,
        }
    }

    fn account(&self) -> Option<&str> {
        match self {
            Through::Account { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Why a pick cannot run through the account, where it cannot.
    fn shut(&self) -> Option<&Refusal> {
        match self {
            Through::Shut(refusal) => Some(refusal),
            _ => None,
        }
    }
}

async fn through(store: &Store, seams: &Seams) -> anyhow::Result<Through> {
    let Some(row) = store.hugging_face_account().await? else {
        return Ok(Through::Anonymous);
    };
    let Some(key) = SealKey::load(&seams.authority_dir)? else {
        return Ok(Through::Shut(Refusal::NoSealKey));
    };
    Ok(match key.open(&row.sealed) {
        Some(token) => Through::Account {
            name: row.account,
            token,
        },
        None => Through::Shut(Refusal::SealDoesNotOpen),
    })
}

/// A role of the first agent's roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Decoder,
    Adapter,
}

impl Role {
    fn as_str(self) -> &'static str {
        match self {
            Role::Decoder => "decoder",
            Role::Adapter => "adapter",
        }
    }

    /// The role a header's `general.type` admits: a model's file, or one
    /// that names no type, a decoder's, and an adapter's file an
    /// adapter's. Any other type admits neither, with the reason.
    fn of(header: &Header) -> Result<Role, String> {
        match header.general_type.as_deref() {
            None | Some("model") => Ok(Role::Decoder),
            Some("adapter") => Ok(Role::Adapter),
            Some(other) => Err(format!(
                "general.type is {other}, which is neither a model nor an adapter"
            )),
        }
    }
}

/// **Why a choice's header and its file names disagree**, where they do:
/// the count the names claim, or one for a file that names no part,
/// against the header's `split.count`, or one where it names none.
fn split_mismatch(choice: &str, header: &Header) -> Option<String> {
    let named = part_of(choice).map_or(1, |(_, _, count)| u64::from(count));
    let carried = header.split_count.unwrap_or(1);
    (named != carried).then(|| {
        format!(
            "the header's split count is {carried}, and the file names say {named} part{}",
            if named == 1 { "" } else { "s" }
        )
    })
}

/// A header's facts as the page shows them, "not read" for each it does
/// not carry.
struct FactsView {
    architecture: String,
    precision: String,
    context: String,
    blocks: String,
    kv_heads: String,
    split: String,
    general_type: String,
}

const NOT_READ: &str = "not read";

fn facts(header: &Header) -> FactsView {
    let or = |v: Option<String>| v.unwrap_or_else(|| NOT_READ.to_owned());
    FactsView {
        architecture: or(header.architecture.clone()),
        precision: or(header.file_type.map(gguf::precision)),
        context: or(header.context_length.map(|n| n.to_string())),
        blocks: or(header.block_count.map(|n| n.to_string())),
        kv_heads: or(header.kv_heads.as_ref().map(|heads| match heads {
            KvHeads::One(n) => n.to_string(),
            KvHeads::PerBlock(list) => format!("per block: {list:?}"),
        })),
        split: match (header.split_no, header.split_count) {
            (Some(no), Some(count)) => format!("part {no} of {count}, counted from 0"),
            (None, Some(count)) => format!("{count} parts"),
            (Some(no), None) => format!("part {no}, of a count not read"),
            (None, None) => NOT_READ.to_owned(),
        },
        general_type: or(header.general_type.clone()),
    }
}

// ---- the page ----

#[derive(Deserialize)]
pub(super) struct Look {
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    revision: Option<String>,
}

/// What the page's form asked, parsed before the gate.
enum Wanted {
    Nothing,
    Bad(String),
    Repository(RepoId, String),
}

fn wanted(look: &Look) -> Wanted {
    let repository = look.repository.as_deref().map(str::trim).unwrap_or("");
    let revision = look.revision.as_deref().map(str::trim).unwrap_or("");
    if repository.is_empty() && revision.is_empty() {
        return Wanted::Nothing;
    }
    let Some(repo) = RepoId::parse(repository) else {
        return Wanted::Bad("a repository is named owner/name, as Hugging Face names it".into());
    };
    if !is_revision(revision) {
        return Wanted::Bad("a revision is a branch, a tag or a 40-hex commit".into());
    }
    Wanted::Repository(repo, revision.to_owned())
}

struct ChoiceView {
    name: String,
    parts: usize,
    size: String,
    /// Why no pick is offered, where a part does not stand.
    missing: Option<String>,
    /// The header fragment's address, for a choice whose header is read.
    header_href: Option<String>,
}

struct GatedView {
    accept: String,
    again: String,
}

/// The repository's pane: what was asked and what Hugging Face answered.
struct ListingView {
    repository: String,
    revision: String,
    commit: Option<String>,
    /// Why nothing is listed, where nothing is.
    trouble: Option<String>,
    gated: Option<GatedView>,
    choices: Vec<ChoiceView>,
}

struct PickedView {
    repository: String,
    commit: String,
    choice: String,
    scale: Option<String>,
    facts: Option<FactsView>,
    picked_by: String,
    picked_at: String,
}

struct RoleView {
    title: &'static str,
    name: &'static str,
    picked: Option<PickedView>,
}

#[derive(Template)]
#[template(path = "setup_models.html")]
struct ModelsPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: crate::surfaces::Chrome,
    frame: Frame,
    roles: Vec<RoleView>,
    listing: Option<ListingView>,
    /// Why no pick is offered on this page, where none is.
    no_pick: Option<String>,
    pulls: PullsView,
}

fn role_view(role: Role, rows: &[SetupModel]) -> RoleView {
    let row = rows.iter().find(|r| r.role == role.as_str());
    RoleView {
        title: match role {
            Role::Decoder => "The decoder",
            Role::Adapter => "The adapter",
        },
        name: role.as_str(),
        picked: row.map(|r| PickedView {
            repository: r.repository.clone(),
            commit: r.commit.clone(),
            choice: r.choice.clone(),
            scale: r.scale.map(|s| s.to_string()),
            facts: serde_json::from_value::<Header>(r.header.clone())
                .ok()
                .map(|h| facts(&h)),
            picked_by: r.picked_by.clone(),
            picked_at: when(r.picked_at),
        }),
    }
}

/// Why the page offers no pick, where it offers none: no cache, an account
/// whose token cannot be opened, or a confirmed setup.
fn no_pick(seams: &Seams, through: &Through, state: &str) -> Option<String> {
    if seams.puller.cache().is_none() {
        return Some(Refusal::NoModelCache.to_string());
    }
    if let Some(refusal) = through.shut() {
        return Some(refusal.to_string());
    }
    (state != "open").then(|| format!("the setup is {state}, so its picks are kept"))
}

async fn page(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    look: Look,
) -> Response {
    let wanted = wanted(&look);
    let (session, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let (chrome, rows, through, pulls) = match async {
        Ok::<_, anyhow::Error>((
            crate::surfaces::Chrome::of(&store, &session.person_id).await?,
            store.setup_models(setup_id).await?,
            through(&store, seams).await?,
            pulls_view(&store, setup_id).await?,
        ))
    }
    .await
    {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    let listing = match wanted {
        Wanted::Nothing => None,
        Wanted::Bad(why) => Some(ListingView {
            repository: look.repository.clone().unwrap_or_default(),
            revision: look.revision.clone().unwrap_or_default(),
            commit: None,
            trouble: Some(why),
            gated: None,
            choices: Vec::new(),
        }),
        Wanted::Repository(repo, revision) => {
            Some(listing(seams, setup_id, &through, &repo, &revision).await)
        }
    };
    let no_pick = no_pick(seams, &through, &setup.state);
    let rendered = ModelsPage {
        here: "declarations",
        who: session.name.clone(),
        admin: true,
        chrome,
        frame: frame(&setup, Page::Models),
        roles: vec![
            role_view(Role::Decoder, &rows),
            role_view(Role::Adapter, &rows),
        ],
        listing,
        no_pick,
        pulls,
    }
    .render();
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// **The repository at the revision, as Hugging Face lists it**: the
/// commit, whether the account may fetch it where it is gated, and its
/// choices, each complete one with its header's fragment.
async fn listing(
    seams: &Seams,
    setup_id: &str,
    through: &Through,
    repo: &RepoId,
    revision: &str,
) -> ListingView {
    let mut view = ListingView {
        repository: repo.to_string(),
        revision: revision.to_owned(),
        commit: None,
        trouble: None,
        gated: None,
        choices: Vec::new(),
    };
    let token = through.token();
    let model = match seams.client.model_at(token, repo, revision).await {
        Ok(ModelAt::Model(model)) => model,
        Ok(ModelAt::Missing) => {
            view.trouble = Some(Refusal::NoSuchModel.to_string());
            return view;
        }
        Ok(ModelAt::Refused) => {
            view.trouble = Some(Refusal::TokenRefused.to_string());
            return view;
        }
        Err(e) => {
            tracing::warn!("a repository's listing could not be read: {e:#}");
            view.trouble = Some(format!("Hugging Face did not answer: {e:#}"));
            return view;
        }
    };
    let choices = choice::choices(&model.files);
    let probe = choices.first().and_then(|c| c.parts.first());
    if let (true, Some(probe)) = (model.gated != Gated::No, probe) {
        match seams
            .client
            .access(token, repo, &model.commit, &probe.path)
            .await
        {
            Ok(Access::Granted) => {}
            Ok(Access::Gated { accept }) => {
                view.gated = Some(GatedView {
                    accept,
                    again: format!(
                        "/setup/{setup_id}/models?repository={}&revision={}",
                        query(&repo.to_string()),
                        query(revision)
                    ),
                });
            }
            Ok(Access::Missing) => {
                view.trouble = Some(Refusal::NoSuchModel.to_string());
                return view;
            }
            Ok(Access::Refused) => {
                view.trouble = Some(Refusal::TokenRefused.to_string());
                return view;
            }
            Err(e) => {
                view.trouble = Some(format!("Hugging Face did not answer: {e:#}"));
                return view;
            }
        }
    }
    let readable = view.gated.is_none();
    view.choices = choices
        .iter()
        .map(|c| ChoiceView {
            header_href: (readable && c.missing.is_none()).then(|| {
                format!(
                    "/setup/{setup_id}/models/header?repository={}&commit={}&choice={}",
                    query(&repo.to_string()),
                    model.commit,
                    query(&c.name)
                )
            }),
            name: c.name.clone(),
            parts: c.parts.len(),
            size: bytes(c.bytes()),
            missing: c.missing.clone(),
        })
        .collect();
    view.commit = Some(model.commit);
    view
}

/// A query value, its reserved characters escaped.
fn query(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

// ---- the header fragment ----

#[derive(Deserialize)]
pub(super) struct HeaderAsk {
    repository: String,
    commit: String,
    choice: String,
}

#[derive(Template)]
#[template(path = "setup_models_header.html")]
struct HeaderFragment {
    setup_id: String,
    version: i64,
    repository: String,
    commit: String,
    choice: String,
    facts: Option<FactsView>,
    /// Why the header's facts are not shown, or no pick is offered.
    reason: Option<String>,
    /// The role the pick's form offers, where one is offered.
    role: Option<&'static str>,
    /// Whether that role is the adapter's, whose form asks the scale.
    adapter: bool,
}

/// The choice a header's fragment or a pick names: a path of a repository
/// that ends in `.gguf`.
fn is_choice(text: &str) -> bool {
    is_repo_path(text) && text.ends_with(".gguf")
}

async fn header(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    ask: HeaderAsk,
) -> Response {
    let repo = RepoId::parse(&ask.repository);
    let Some(repo) = repo.filter(|_| is_commit(&ask.commit) && is_choice(&ask.choice)) else {
        if !is_setup_id(setup_id) {
            return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
        }
        return (
            StatusCode::BAD_REQUEST,
            "a header names a repository, a 40-hex commit and a .gguf choice",
        )
            .into_response();
    };
    let (_, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let through = match through(&store, seams).await {
        Ok(through) => through,
        Err(e) => return fault(e),
    };
    let mut fragment = HeaderFragment {
        setup_id: setup.setup_id.clone(),
        version: setup.version,
        repository: repo.to_string(),
        commit: ask.commit.clone(),
        choice: ask.choice.clone(),
        facts: None,
        reason: None,
        role: None,
        adapter: false,
    };
    match seams
        .headers
        .read(
            &seams.client,
            through.token(),
            &repo,
            &ask.commit,
            &ask.choice,
        )
        .await
    {
        Ok(HeaderRead::Read(h)) => {
            fragment.facts = Some(facts(&h));
            let offered = split_mismatch(&ask.choice, &h)
                .map_or_else(|| Role::of(&h), Err)
                .and_then(|role| match no_pick(seams, &through, &setup.state) {
                    Some(why) => Err(why),
                    None => Ok(role),
                });
            match offered {
                Ok(role) => {
                    fragment.role = Some(role.as_str());
                    fragment.adapter = role == Role::Adapter;
                }
                Err(why) => fragment.reason = Some(why),
            }
        }
        Ok(HeaderRead::NotRead(why) | HeaderRead::NotFound(why)) => {
            fragment.reason = Some(format!("{NOT_READ}: {why}"));
        }
        Err(e) => {
            tracing::warn!("a header could not be read: {e:#}");
            fragment.reason = Some(format!("{NOT_READ}: Hugging Face did not answer: {e:#}"));
        }
    }
    match fragment.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

// ---- the pulls fragment ----

struct RolePullView {
    role: &'static str,
    pull_id: String,
    state: String,
    hue: &'static str,
    reason: Option<String>,
    done: String,
    size: String,
}

/// The base's identity as the page shows it.
struct IdentityView {
    /// The digest, where it is computed.
    blake3: Option<String>,
    /// Why it is not read, where it is not.
    not_read: Option<String>,
    computed_at: String,
}

struct PullsView {
    pulls: Vec<RolePullView>,
    identity: IdentityView,
}

#[derive(Template)]
#[template(path = "setup_models_pull.html")]
struct PullsFragment {
    pulls: PullsView,
}

async fn pulls_view(store: &Store, setup_id: &str) -> anyhow::Result<PullsView> {
    let rows = store.setup_models(setup_id).await?;
    let mut pulls = Vec::new();
    for row in &rows {
        let files = store.pull_files(&row.pull_id).await?;
        let pull = store.pull(&row.pull_id).await?;
        let size: u64 = files.iter().map(|f| f.file.size).sum();
        let done: u64 = files
            .iter()
            .map(|f| u64::try_from(f.bytes_done).unwrap_or(0))
            .sum();
        pulls.push(RolePullView {
            role: if row.role == "decoder" {
                "decoder"
            } else {
                "adapter"
            },
            pull_id: row.pull_id.clone(),
            hue: match row.pull_state.as_str() {
                "verified" => "ok",
                "running" | "cancelled" => "warn",
                _ => "fail",
            },
            state: row.pull_state.clone(),
            reason: pull.and_then(|p| p.reason),
            done: bytes(done),
            size: bytes(size),
        });
    }
    let decoder = rows.iter().find(|r| r.role == "decoder");
    let identity = match decoder {
        None => IdentityView {
            blake3: None,
            not_read: Some(format!("{NOT_READ}: no decoder is picked")),
            computed_at: String::new(),
        },
        Some(d) => match store
            .base_identity(&d.repository, &d.commit, &d.choice)
            .await?
        {
            Some(identity) => IdentityView {
                blake3: Some(identity.blake3),
                not_read: None,
                computed_at: when(identity.computed_at),
            },
            None => IdentityView {
                blake3: None,
                not_read: Some(if d.pull_state == "verified" {
                    format!("{NOT_READ}: its hash runs")
                } else {
                    format!(
                        "{NOT_READ}: it waits on the decoder's pull, which is {}",
                        d.pull_state
                    )
                }),
                computed_at: String::new(),
            },
        },
    };
    Ok(PullsView { pulls, identity })
}

/// **The setup's pulls, re-fetched by the page** every 15 s: the session
/// read in the background and the admin check, as the cache's section's.
async fn pulls(store: Store, seams: &Seams, headers: HeaderMap, setup_id: &str) -> Response {
    if !is_setup_id(setup_id) {
        return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
    }
    let session = match gate::session_in_background(&store, &seams.policy, &headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return crate::surfaces::record::NoSession.into_response(),
        Err(e) => return fault(e),
    };
    match store.is_admin(&session.person_id).await {
        Ok(true) => {}
        Ok(false) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(e) => return fault(e),
    }
    match store.setup(setup_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    }
    let pulls = match pulls_view(&store, setup_id).await {
        Ok(pulls) => pulls,
        Err(e) => return fault(e),
    };
    match (PullsFragment { pulls }).render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

// ---- the pick ----

/// A pick's form, parsed to its shape.
#[derive(Debug, PartialEq)]
struct PickAsk {
    version: i64,
    role: Role,
    repository: RepoId,
    commit: String,
    choice: String,
    scale: Option<f64>,
}

/// **A pick's form parsed to its shape**, before anything is looked up:
/// each field of the six once and no other, the role one of the first
/// agent's two, and the scale an adapter's alone, finite.
fn pick_ask(fields: &[(String, String)]) -> Result<PickAsk, String> {
    for (at, (name, _)) in fields.iter().enumerate() {
        if !PICK_FIELDS.contains(&name.as_str()) {
            return Err(format!(
                "a pick takes no {name:?}: the first agent's roster is one decoder and one adapter, placed on the GPU, and the rest of it is unbuilt"
            ));
        }
        if fields[..at].iter().any(|(earlier, _)| earlier == name) {
            return Err(format!(
                "a pick names {name} once: one model and one role at a time"
            ));
        }
    }
    let field = |name: &str| {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.trim())
    };
    let need = |name: &str| field(name).ok_or_else(|| format!("a pick names its {name}"));
    let version: i64 = need("version")?
        .parse()
        .ok()
        .filter(|v| *v >= 1)
        .ok_or("a setup's version is 1 at least")?;
    let role = match need("role")? {
        "decoder" => Role::Decoder,
        "adapter" => Role::Adapter,
        other => {
            return Err(format!(
                "the first agent's roster is one decoder and one adapter, and the role {other:?} is unbuilt"
            ));
        }
    };
    let repository = RepoId::parse(need("repository")?)
        .ok_or("a repository is named owner/name, as Hugging Face names it")?;
    let commit = need("commit")?;
    if !is_commit(commit) {
        return Err("a pick names a 40-hex commit and never a branch".into());
    }
    let choice = need("choice")?;
    if !is_choice(choice) {
        return Err("a choice is a .gguf file's path in the repository".into());
    }
    let scale = match (role, field("scale")) {
        (Role::Decoder, None) => None,
        (Role::Decoder, Some(_)) => return Err("a decoder's pick carries no scale".into()),
        (Role::Adapter, None) | (Role::Adapter, Some("")) => {
            return Err("an adapter's pick carries its scale, and there is no default".into());
        }
        (Role::Adapter, Some(text)) => match text.parse::<f64>() {
            Ok(scale) if scale.is_finite() => Some(scale),
            _ => {
                return Err(format!(
                    "an adapter's scale is a finite number, and {text:?} is not"
                ));
            }
        },
    };
    Ok(PickAsk {
        version,
        role,
        repository,
        commit: commit.to_owned(),
        choice: choice.to_owned(),
        scale,
    })
}

/// The answer where Hugging Face did not answer: nothing was saved.
fn unanswered(e: anyhow::Error) -> Response {
    tracing::warn!("a pick's read of Hugging Face failed: {e:#}");
    (
        StatusCode::BAD_GATEWAY,
        format!("{e:#}; nothing was saved, and the pick may be posted again"),
    )
        .into_response()
}

/// A token Hugging Face refused: one refusal record, as on the account's
/// page.
async fn token_refused(store: &Store, session: &Session, setup_id: &str) -> Response {
    let principal = Principal::Person {
        person_id: &session.person_id,
        method: PersonMethod::Session,
    };
    match store
        .audit_refusal(
            principal,
            Target::Setup(setup_id),
            ACTION,
            "Hugging Face refused the token",
        )
        .await
    {
        Ok(_) => refused(&Refusal::TokenRefused),
        Err(e) => {
            tracing::error!("a refusal of {ACTION} was not recorded: {e:#}");
            fault(e)
        }
    }
}

/// **What a pick's checks resolve**: the choice's parts from the listing
/// at the commit, and the header's facts.
struct Resolved {
    choice: Choice,
    header: serde_json::Value,
}

/// **The choice resolved through Hugging Face**, after the gate: the
/// listing at the commit, the choice complete in it, the account granted
/// it where it is gated, and its header read, its split count agreeing
/// with its names and its type admitting the role.
async fn resolve(
    store: &Store,
    seams: &Seams,
    session: &Session,
    setup_id: &str,
    through: &Through,
    ask: &PickAsk,
) -> Result<Resolved, Response> {
    let token = through.token();
    let model = match seams
        .client
        .model_at(token, &ask.repository, &ask.commit)
        .await
    {
        Ok(ModelAt::Model(model)) => model,
        Ok(ModelAt::Missing) => return Err(refused(&Refusal::NoSuchModel)),
        Ok(ModelAt::Refused) => return Err(token_refused(store, session, setup_id).await),
        Err(e) => return Err(unanswered(e)),
    };
    if model.commit != ask.commit {
        return Err(unanswered(anyhow::anyhow!(
            "Hugging Face's listing at {} answered the commit {}",
            ask.commit,
            model.commit
        )));
    }
    let Some(choice) = choice::choices(&model.files)
        .into_iter()
        .find(|c| c.name == ask.choice)
    else {
        return Err(refused(&Refusal::NoSuchChoice));
    };
    if let Some(missing) = &choice.missing {
        return Err(refused(&Refusal::Unpickable(missing.clone())));
    }
    if model.gated != Gated::No {
        match seams
            .client
            .access(token, &ask.repository, &ask.commit, &choice.parts[0].path)
            .await
        {
            Ok(Access::Granted) => {}
            Ok(Access::Gated { accept }) => {
                return Err(refused(&Refusal::Unpickable(format!(
                    "the repository is gated, and the account has not been granted it; accept its terms at {accept} and check again"
                ))));
            }
            Ok(Access::Missing) => return Err(refused(&Refusal::NoSuchChoice)),
            Ok(Access::Refused) => return Err(token_refused(store, session, setup_id).await),
            Err(e) => return Err(unanswered(e)),
        }
    }
    let header = match seams
        .headers
        .read(
            &seams.client,
            token,
            &ask.repository,
            &ask.commit,
            &ask.choice,
        )
        .await
    {
        Ok(HeaderRead::Read(header)) => header,
        Ok(HeaderRead::NotRead(why) | HeaderRead::NotFound(why)) => {
            return Err(refused(&Refusal::Unpickable(format!(
                "its header was not read: {why}"
            ))));
        }
        Err(e) => return Err(unanswered(e)),
    };
    if let Some(why) = split_mismatch(&ask.choice, &header) {
        return Err(refused(&Refusal::Unpickable(why)));
    }
    match Role::of(&header) {
        Ok(role) if role == ask.role => {}
        Ok(role) => {
            return Err(refused(&Refusal::Unpickable(format!(
                "the file is {} {}'s, and the pick's role is the {}",
                if role == Role::Adapter { "an" } else { "a" },
                role.as_str(),
                ask.role.as_str()
            ))));
        }
        Err(why) => return Err(refused(&Refusal::Unpickable(why))),
    }
    let header = match serde_json::to_value(&header) {
        Ok(header) => header,
        Err(e) => return Err(fault(e)),
    };
    Ok(Resolved { choice, header })
}

async fn pick(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    fields: Vec<(String, String)>,
) -> Response {
    if !is_setup_id(setup_id) {
        return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
    }
    let ask = match pick_ask(&fields) {
        Ok(ask) => ask,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let pull_id = match store.mint_key("mp").await {
        Ok(pull_id) => pull_id,
        Err(e) => return fault(e),
    };
    let target = Target::Setup(setup_id);
    let session = match writer(&store, &seams.policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.setup(setup_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    }
    if seams.puller.cache().is_none() {
        return refused(&Refusal::NoModelCache);
    }
    let through = match through(&store, seams).await {
        Ok(through) => through,
        Err(e) => return fault(e),
    };
    if let Some(refusal) = through.shut() {
        return refused(refusal);
    }
    let resolved = match resolve(&store, seams, &session, setup_id, &through, &ask).await {
        Ok(resolved) => resolved,
        Err(answer) => return answer,
    };
    let (setup_id, puller) = (setup_id.to_owned(), seams.puller.clone());
    // **The account's name alone goes into step four**, as the pick's
    // attribution: the token stays here and is dropped with `through`,
    // since nothing past the resolve reads it.
    let account = through.account().map(str::to_owned);
    drop(through);
    step_four(&seams.policy, async move {
        let first = match first_of(&store, &session, Target::Setup(&setup_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let pick = Pick {
            role: ask.role.as_str(),
            repository: &ask.repository,
            commit: &ask.commit,
            choice: &ask.choice,
            scale: ask.scale,
            header: &resolved.header,
            pull_id: &pull_id,
            account: account.as_deref(),
            parts: &resolved.choice.parts,
        };
        let written = puller
            .ask_with(&ask.commit, &resolved.choice.parts, || {
                store.setup_pick(
                    &session.person_id,
                    session.session_id,
                    &first,
                    &setup_id,
                    ask.version,
                    &pick,
                )
            })
            .await;
        settled(&store, &first, &written).await;
        landed(written, &setup_id, Page::Models)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "27b350a791e81d9a4d1ddca1c49282e9ec533768";

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn decoder() -> Vec<(&'static str, &'static str)> {
        vec![
            ("version", "1"),
            ("role", "decoder"),
            ("repository", "org/split"),
            ("commit", COMMIT),
            ("choice", "m-00001-of-00002.gguf"),
        ]
    }

    /// **A pick's form is parsed to its shape**: the decoder's without a
    /// scale and the adapter's with a finite one are taken.
    #[test]
    fn a_picks_form_is_taken_in_its_shape() {
        let ask = pick_ask(&fields(&decoder())).unwrap();
        assert_eq!(ask.role, Role::Decoder);
        assert_eq!(ask.scale, None);
        let mut adapter = decoder();
        adapter[1] = ("role", "adapter");
        adapter.push(("scale", "0.75"));
        assert_eq!(pick_ask(&fields(&adapter)).unwrap().scale, Some(0.75));
        adapter[5] = ("scale", "-2");
        assert_eq!(pick_ask(&fields(&adapter)).unwrap().scale, Some(-2.0));
    }

    /// **A crafted pick naming a second model, a second adapter role or the
    /// CPU is refused** before anything is looked up, and so is an
    /// adapter's pick without a finite scale and a decoder's carrying one.
    #[test]
    fn a_crafted_pick_is_refused_by_its_shape() {
        let with = |extra: (&'static str, &'static str)| {
            let mut f = decoder();
            f.push(extra);
            fields(&f)
        };
        let swapped = |at: usize, to: (&'static str, &'static str)| {
            let mut f = decoder();
            f[at] = to;
            fields(&f)
        };
        for (why, crafted) in [
            ("a second model", with(("choice", "other.gguf"))),
            ("a second repository", with(("repository", "org/other"))),
            ("a second role", with(("role", "adapter"))),
            ("a second adapter role", swapped(1, ("role", "adapter2"))),
            ("a classifier", swapped(1, ("role", "classifier"))),
            ("the CPU", with(("device", "cpu"))),
            ("placement", with(("placement", "cpu"))),
            ("a decoder's scale", with(("scale", "1"))),
            ("a branch", swapped(3, ("commit", "main"))),
            ("a file not GGUF", swapped(4, ("choice", "config.json"))),
            (
                "a path out of the repository",
                swapped(4, ("choice", "../m.gguf")),
            ),
        ] {
            assert!(pick_ask(&crafted).is_err(), "{why}");
        }
        for scale in ["", "inf", "-inf", "NaN", "1e400", "one", " "] {
            let mut f = decoder();
            f[1] = ("role", "adapter");
            f.push(("scale", scale));
            assert!(pick_ask(&fields(&f)).is_err(), "scale {scale:?}");
        }
        let mut unscaled = decoder();
        unscaled[1] = ("role", "adapter");
        assert!(
            pick_ask(&fields(&unscaled)).is_err(),
            "an adapter without a scale"
        );
    }

    /// **The split count of the header agrees with the names**, a lone
    /// file's being one or none.
    #[test]
    fn the_header_and_the_names_agree_on_the_split() {
        let with = |count: Option<u64>| Header {
            version: 3,
            architecture: None,
            file_type: None,
            context_length: None,
            block_count: None,
            kv_heads: None,
            split_count: count,
            split_no: None,
            general_type: None,
            metadata_bytes: 0,
        };
        assert_eq!(
            split_mismatch("m-00001-of-00002.gguf", &with(Some(2))),
            None
        );
        assert!(split_mismatch("m-00001-of-00002.gguf", &with(Some(3))).is_some());
        assert!(split_mismatch("m-00001-of-00002.gguf", &with(None)).is_some());
        assert_eq!(split_mismatch("lone.gguf", &with(None)), None);
        assert_eq!(split_mismatch("lone.gguf", &with(Some(1))), None);
        assert!(split_mismatch("lone.gguf", &with(Some(2))).is_some());
    }
}
