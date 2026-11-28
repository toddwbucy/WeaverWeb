//! conforms: web-the-declaration-runs-in-the-order-of-the-work
//! conforms: web-the-models-roster-is-one-decoder-and-at-most-one-adapter
//! conforms: web-a-pick-asks-hugging-face-only-after-the-admin-gate
//! conforms: web-an-adapters-pick-carries-a-finite-scale
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//! conforms: web-an-open-setups-adapter-pick-can-be-dropped-and-a-confirmed-setups-cannot
//!
//! **The models page and the adapter's page** (Spec 6.2), a child of the
//! setup pages' module, **in the order of the work** (the operator's words
//! of 2026-11-27; the Planner's brief of 2026-11-28, part 4).
//! `GET /setup/{setup}/models` draws the decoder's role, the rest of the
//! roster drawn unbuilt, and, for a repository and a revision asked in its
//! form, the commit the revision resolves to and each choice by name, a
//! `<details>` that opens on a click to its parts and its header; a pick
//! of the decoder lands on `GET /setup/{setup}/adapter`, which draws the
//! adapter's role, picked from a repository the same way, or none, the
//! agent then loading its base alone. Each complete choice's header is its
//! own fragment (`GET /setup/{setup}/models/header`), read by range through
//! the commit when its choice is first opened and kept in memory
//! (`hugging_face::headers`), with the pick's form where the header's
//! `general.type` admits the role of the page that asked it. **A file's
//! download starts at its pick.** The setup's pulls, each file's bytes done
//! of its size with a percentage and a bar and the same in all, and the
//! base's identity, are a fragment both pages re-fetch every 3 s (`GET
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
use axum::routing::{get, post};
use serde::Deserialize;

use super::{
    Frame, MALFORMED_SETUP, Page, Seams, admin_read, declaration_state, frame, landed, when,
};
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
use crate::store::setup::{Pick, Setup, SetupModel};
use crate::surfaces::admin::{NOT_AN_ADMIN, fault, first_of, refused, settled, step_four, writer};
use crate::surfaces::gate::{self, Session};
use crate::surfaces::hugging_face::bytes;

/// The pick's action, as its audit records name it.
const ACTION: &str = "setup pick";

/// The drop's action, as its audit records name it.
const DROP: &str = "setup drop";

/// The fields a pick's form carries, each once.
const PICK_FIELDS: [&str; 6] = ["version", "role", "repository", "commit", "choice", "scale"];

pub(super) fn routes(seams: &Seams) -> Router<Store> {
    let page_seams = seams.clone();
    let pick_seams = seams.clone();
    let header_seams = seams.clone();
    let pull_seams = seams.clone();
    let drop_seams = seams.clone();
    let adapter_seams = seams.clone();
    Router::new()
        .route(
            "/setup/{setup}/adapter",
            get(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Query(look): Query<Look>| {
                    let seams = adapter_seams.clone();
                    async move { page(store, &seams, headers, &setup, look, Page::Adapter).await }
                },
            ),
        )
        .route(
            "/setup/{setup}/models",
            get(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Query(look): Query<Look>| {
                    let seams = page_seams.clone();
                    async move { page(store, &seams, headers, &setup, look, Page::Models).await }
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
        .route(
            "/setup/{setup}/models/drop",
            post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Form(fields): Form<Vec<(String, String)>>| {
                    let seams = drop_seams.clone();
                    async move { drop_adapter(store, &seams, headers, &setup, fields).await }
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

/// A role of the models page's roster.
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

    /// The role the page `here` picks: the decoder on Models, the adapter
    /// on Adapter.
    fn of_page(here: Page) -> Role {
        if here == Page::Adapter {
            Role::Adapter
        } else {
            Role::Decoder
        }
    }

    /// Why a header of this role is offered no pick on the other role's
    /// page: each role is picked on its own page.
    fn elsewhere(self) -> String {
        match self {
            Role::Decoder => {
                "this file is a model, which the Models step picks as the decoder".to_owned()
            }
            Role::Adapter => {
                "this file is an adapter, which the Adapter step picks after the model".to_owned()
            }
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
    /// Each part's path and size, shown once the choice is opened.
    files: Vec<(String, String)>,
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
    /// Whether a setup confirms without this role: the adapter's, which is
    /// optional (Spec 6.2), so its pane is `ok` with none picked.
    optional: bool,
    picked: Option<PickedView>,
    /// The setup's version for the Drop's form, where the role is optional,
    /// picked, and the setup open.
    drop: Option<i64>,
}

#[derive(Template)]
#[template(path = "setup_models.html")]
struct ModelsPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: crate::surfaces::Chrome,
    frame: Frame,
    /// The page's own address's last part, `models` or `adapter`, which its
    /// repository's form asks again.
    step: &'static str,
    /// Whether this is the adapter's page.
    adapter_step: bool,
    roles: Vec<RoleView>,
    listing: Option<ListingView>,
    /// Why no pick is offered on this page, where none is.
    no_pick: Option<String>,
    pulls: PullsView,
    /// **The models directory's need** (Spec 6.2), where a model is picked,
    /// and the models directory as the report shows it, judged against the
    /// need.
    need: Option<super::Need>,
    /// The models directory, or why none is judged: no report, or the
    /// report refused.
    models_directory: Result<super::report::DriveRow, String>,
}

/// The models directory as the report shows it, judged against `need`,
/// where the report reads, else why none is.
fn models_directory(
    reports: &crate::box_report::Reports,
    need: Option<&super::Need>,
) -> Result<super::report::DriveRow, String> {
    use crate::box_report::Report;
    let why = match &reports.report {
        Report::Waiting => "No report has arrived".to_owned(),
        Report::Refused { refused, .. } => format!(
            "The report is refused for cause {}: {}",
            refused.cause.number(),
            refused.cause.words()
        ),
        Report::Read { .. } => {
            if let super::report::Shown::Read { body, .. } = super::directories(reports, need)
                && let Some(row) = body.drives.into_iter().find(|d| d.role == "models")
            {
                return Ok(row);
            }
            // Unreachable: the survey's reader refuses a report whose
            // drives are not state then models (`survey::read`), so a read
            // report always names the models directory. The type of
            // `drives` cannot say so.
            "The report is read".to_owned()
        }
    };
    Err(format!(
        "{why}, so the need is not judged against the models directory."
    ))
}

fn role_view(role: Role, rows: &[SetupModel], setup: &Setup) -> RoleView {
    let row = rows.iter().find(|r| r.role == role.as_str());
    let optional = role == Role::Adapter;
    RoleView {
        title: match role {
            Role::Decoder => "The decoder",
            Role::Adapter => "The adapter",
        },
        name: role.as_str(),
        optional,
        drop: (optional && row.is_some() && setup.state == "open").then_some(setup.version),
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
    here: Page,
) -> Response {
    let wanted = wanted(&look);
    let (session, setup) = match super::declaration_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let (chrome, rows, through, pulls, need, reports) = match async {
        Ok::<_, anyhow::Error>((
            crate::surfaces::Chrome::of(&store, &session.person_id).await?,
            store.setup_models(setup_id).await?,
            through(&store, seams).await?,
            pulls_view(&store, setup_id).await?,
            super::need(&store, setup_id).await?,
            store.reports(seams.boxes.as_ref(), setup_id).await?,
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
            Some(listing(seams, setup_id, &through, &repo, &revision, here).await)
        }
    };
    let no_pick = no_pick(seams, &through, &setup.state);
    let rendered = ModelsPage {
        here: "declarations",
        who: session.name.clone(),
        admin: true,
        chrome,
        frame: frame(&setup, here),
        step: here.slug(),
        adapter_step: here == Page::Adapter,
        roles: vec![role_view(Role::of_page(here), &rows, &setup)],
        listing,
        no_pick,
        pulls,
        models_directory: models_directory(&reports, need.as_ref()),
        need,
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
    here: Page,
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
                        "/setup/{setup_id}/{}?repository={}&revision={}",
                        here.slug(),
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
                    "/setup/{setup_id}/models/header?repository={}&commit={}&choice={}&role={}",
                    query(&repo.to_string()),
                    model.commit,
                    query(&c.name),
                    Role::of_page(here).as_str()
                )
            }),
            name: c.name.clone(),
            parts: c.parts.len(),
            files: c
                .parts
                .iter()
                .map(|f| (f.path.clone(), bytes(f.size)))
                .collect(),
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
    /// The role of the page that asked, `decoder` or `adapter`: the pick's
    /// form is offered for that role alone. Absent, either.
    #[serde(default)]
    role: Option<String>,
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
    let wanted = match ask.role.as_deref() {
        None => Some(None),
        Some("decoder") => Some(Some(Role::Decoder)),
        Some("adapter") => Some(Some(Role::Adapter)),
        Some(_) => None,
    };
    let (Some(repo), Some(wanted)) = (
        repo.filter(|_| is_commit(&ask.commit) && is_choice(&ask.choice)),
        wanted,
    ) else {
        if !is_setup_id(setup_id) {
            return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
        }
        return (
            StatusCode::BAD_REQUEST,
            "a header names a repository, a 40-hex commit and a .gguf choice, and a role of decoder or adapter where it names one",
        )
            .into_response();
    };
    let (_, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    // **A fragment of an unpaired setup's declaration is refused as its
    // writes are**, before Hugging Face is asked (the Planner's ruling of
    // 2026-11-24 on the second review).
    if setup.paired.is_none() {
        return refused(&Refusal::SetupUnpaired);
    }
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
                .and_then(|role| match wanted {
                    Some(w) if w != role => Err(role.elsewhere()),
                    _ => Ok(role),
                })
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
    progress: Progress,
    files: Vec<FileProgress>,
}

/// **Bytes done of a size**, with the percentage and the bar's two values
/// (Spec 6.2): a `<progress>` element, so the bar needs no script.
struct Progress {
    done: String,
    size: String,
    /// Whole percent, rounded down, so 100 means every byte.
    percent: u64,
    /// The bar's value and maximum: the bytes, or 1 of 1 or 0 of 1 for a
    /// size of nothing, since a bar's maximum is above zero.
    value: u64,
    max: u64,
}

impl Progress {
    fn of(done: u64, size: u64) -> Progress {
        let (value, max, percent) = if size == 0 {
            (1, 1, 100)
        } else {
            let done = done.min(size);
            (
                done,
                size,
                u64::try_from(u128::from(done) * 100 / u128::from(size)).unwrap_or(100),
            )
        };
        Progress {
            done: bytes(done),
            size: bytes(size),
            percent,
            value,
            max,
        }
    }
}

/// One file of a pull, by name, with its progress.
struct FileProgress {
    name: String,
    progress: Progress,
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
    /// Every file of every pull, in all.
    total: Progress,
    identity: IdentityView,
    /// **Whether nothing on the fragment can move any more**: no pull
    /// runs, and no verified decoder's identity is still owed by its hash.
    /// The fragment then answers 286, which stops htmx's poll (the
    /// Planner's review of part 4, 2026-11-28).
    settled: bool,
}

/// **The status that stops an htmx poll**, answered with the fragment once
/// it is settled; htmx swaps it as any success and polls no more.
const POLL_STOPS: u16 = 286;

/// **A sum of byte counts that saturates**, so no size a listing or a row
/// carries panics a debug build or wraps a release one.
fn bytes_in_all(counts: impl IntoIterator<Item = u64>) -> u64 {
    counts.into_iter().fold(0, u64::saturating_add)
}

#[derive(Template)]
#[template(path = "setup_models_pull.html")]
struct PullsFragment {
    pulls: PullsView,
}

async fn pulls_view(store: &Store, setup_id: &str) -> anyhow::Result<PullsView> {
    let rows = store.setup_models(setup_id).await?;
    let mut pulls = Vec::new();
    let (mut all_done, mut all_size) = (0u64, 0u64);
    for row in &rows {
        let files = store.pull_files(&row.pull_id).await?;
        let pull = store.pull(&row.pull_id).await?;
        let done_of = |f: &crate::store::model_pull::PullFile| {
            u64::try_from(f.bytes_done).unwrap_or(0).min(f.file.size)
        };
        let size = bytes_in_all(files.iter().map(|f| f.file.size));
        let done = bytes_in_all(files.iter().map(done_of));
        all_done = all_done.saturating_add(done);
        all_size = all_size.saturating_add(size);
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
            progress: Progress::of(done, size),
            files: files
                .iter()
                .map(|f| FileProgress {
                    name: f.file.path.clone(),
                    progress: Progress::of(done_of(f), f.file.size),
                })
                .collect(),
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
    let owed = decoder.is_some_and(|d| d.pull_state == "verified") && identity.blake3.is_none();
    let settled = rows.iter().all(|r| r.pull_state != "running") && !owed;
    Ok(PullsView {
        pulls,
        total: Progress::of(all_done, all_size),
        identity,
        settled,
    })
}

/// **The setup's pulls, re-fetched by the page** every 3 s: the session
/// read in the background and the admin check, as the cache's section's;
/// answered 286 once settled, so the poll stops.
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
        Ok(Some(setup)) if setup.paired.is_none() => return refused(&Refusal::SetupUnpaired),
        Ok(Some(_)) => {}
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    }
    let pulls = match pulls_view(&store, setup_id).await {
        Ok(pulls) => pulls,
        Err(e) => return fault(e),
    };
    let status = if pulls.settled {
        StatusCode::from_u16(POLL_STOPS).expect("286 is a status")
    } else {
        StatusCode::OK
    };
    match (PullsFragment { pulls }).render() {
        Ok(html) => (status, Html(html)).into_response(),
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
/// each field of the six once and no other, the role one of the roster's
/// two, and the scale an adapter's alone, finite.
fn pick_ask(fields: &[(String, String)]) -> Result<PickAsk, String> {
    for (at, (name, _)) in fields.iter().enumerate() {
        if !PICK_FIELDS.contains(&name.as_str()) {
            return Err(format!(
                "a pick takes no {name:?}: the roster is one decoder and at most one adapter on it, placed on the GPU, and the rest of it is unbuilt"
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
                "the roster is one decoder and at most one adapter on it, and the role {other:?} is unbuilt"
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
    // **A pick on a setup whose box is not paired is refused**, as the
    // other declaration writes are; a confirmed setup's pick stays the
    // store's stale refusal, as before.
    match store.setup(setup_id).await {
        Ok(Some(setup)) if setup.paired.is_none() => return refused(&Refusal::SetupUnpaired),
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
        // **Each pick lands on the adapter's page**, the step after the
        // model's, where the downloads' progress shows too.
        landed(written, &setup_id, Page::Adapter)
    })
    .await
}

/// **The drop's form**: the setup's version alone, since the one role a
/// setup confirms without is the adapter's.
fn drop_ask(fields: &[(String, String)]) -> Result<i64, String> {
    match fields {
        [(name, version)] if name == "version" => version
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|v| *v >= 1)
            .ok_or_else(|| "a setup's version is 1 at least".to_owned()),
        _ => Err(
            "a drop takes the setup's version alone: the adapter's pick is the one dropped"
                .to_owned(),
        ),
    }
}

/// **The adapter's pick dropped** (Spec 6.2), an admin write in the order of
/// Spec 2.13: the setup's identity and the form parsed (400), the admin gate,
/// the setup resolved, open and holding an adapter's pick (404, 409, no
/// record), then the write under its first record. Its pull is left as it
/// stands, and its files stay in the cache.
async fn drop_adapter(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    fields: Vec<(String, String)>,
) -> Response {
    if !is_setup_id(setup_id) {
        return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
    }
    let version = match drop_ask(&fields) {
        Ok(version) => version,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let target = Target::Setup(setup_id);
    let session = match writer(&store, &seams.policy, &headers, target, DROP).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.setup(setup_id).await {
        Ok(Some(setup)) => {
            if let Err(refusal) = declaration_state(&setup) {
                return refused(&refusal);
            }
        }
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    }
    match store.setup_models(setup_id).await {
        Ok(rows) if rows.iter().any(|r| r.role == Role::Adapter.as_str()) => {}
        Ok(_) => return refused(&Refusal::NoAdapterPicked),
        Err(e) => return fault(e),
    }
    let setup_id = setup_id.to_owned();
    step_four(&seams.policy, async move {
        let first = match first_of(&store, &session, Target::Setup(&setup_id), DROP).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .setup_drop_adapter(
                &session.person_id,
                session.session_id,
                &first,
                &setup_id,
                version,
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, &setup_id, Page::Adapter)
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

    /// **A download's progress is bytes, a whole percentage and a bar**:
    /// rounded down, capped at the size, nothing to fetch done.
    #[test]
    fn a_downloads_progress_is_bytes_a_whole_percentage_and_a_bar() {
        let read = |done, size| {
            let p = Progress::of(done, size);
            (p.percent, p.value, p.max)
        };
        assert_eq!(read(0, 300), (0, 0, 300));
        assert_eq!(read(1, 3), (33, 1, 3), "rounded down");
        assert_eq!(read(299, 300), (99, 299, 300), "100 means every byte");
        assert_eq!(read(300, 300), (100, 300, 300));
        assert_eq!(read(400, 300), (100, 300, 300), "never past the size");
        assert_eq!(read(0, 0), (100, 1, 1), "nothing to fetch is done");
        assert_eq!(
            read(u64::MAX, u64::MAX),
            (100, u64::MAX, u64::MAX),
            "no overflow"
        );
    }

    /// **Byte counts sum saturating** (the Planner's review of part 4,
    /// 2026-11-28): sizes past `u64::MAX` in all read as `u64::MAX`, never
    /// a panic or a wrap.
    ///
    /// Perturbation: sum with `Iterator::sum`, and the test build panics on
    /// the overflow.
    #[test]
    fn byte_counts_sum_saturating() {
        assert_eq!(bytes_in_all([]), 0);
        assert_eq!(bytes_in_all([1, 2, 3]), 6);
        assert_eq!(bytes_in_all([u64::MAX, 1]), u64::MAX);
        assert_eq!(bytes_in_all([u64::MAX - 1, 1, u64::MAX]), u64::MAX);
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
