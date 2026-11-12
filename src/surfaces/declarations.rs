//! conforms: web-only-the-admin-grant-reads-or-writes-a-declaration
//! conforms: web-the-declaration-screen-reaches-no-agent-and-no-host
//!
//! **The declaration screen** (Spec 6.1): the first of the two screens
//! Declare becomes. A signed-in admin declares an agent's components, saves
//! the declaration as a versioned document (Spec 2.4), and reads it back as
//! the file it will become. The five routes:
//!
//! - `GET /declarations`: every lineage, by name, with its head
//! - `GET /declarations/new`: the screen, empty (`?boxes=` for more slots)
//! - `POST /declarations`: save the first row of a new lineage
//! - `GET /declarations/{de}`: reopen that row, with its version, its last
//!   save, its author, whether it is its lineage's head, the class
//!   readings and the two panes (`?boxes=` for more empty boxes)
//! - `POST /declarations/{de}`: save from that row, a new row whose parent
//!   it is
//!
//! **Who may declare: the server-wide admin grant**, for reading and writing
//! alike. Reads are not audited. **A save takes Spec 2.13's one order**:
//! (1) the path's identity and the form parsed, a refusal recorded nowhere;
//! (2) the admin gate with its one refusal record, before any row is looked
//! up; (3) the row saved from resolved, its head and the name checked, each
//! refusal recorded nowhere; (4) the audited write (`store::declarations`).
//!
//! **The screen reaches no agent and no host**: a save writes the store and
//! nothing else. This module takes no listener, no asker and no seam to
//! the link, so it holds no way to send a frame.

use askama::Template;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use crate::declaration::capability;
use crate::declaration::render::{self, CORPUS_COMMIT, Meta};
use crate::declaration::{Document, Form, MAX_BOXES, Reading};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::declarations::{is_declaration_id, saveable};
use crate::store::identity::Author;
use crate::surfaces::admin::{
    NOT_AN_ADMIN, admin_session, fault, first_of, refused, settled, writer,
};
use crate::surfaces::gate::{Policy, Session};

/// The audit's action for a save.
pub const ACTION: &str = "declaration save";

/// The answer to a path that is not a declaration's identity.
const MALFORMED: &str =
    "the declaration asked for is not a declaration's identity: `de-` and sixteen lowercase hex";

/// The declaration screen's routes, taking the session's policy as their
/// own argument and nothing else.
pub fn routes(policy: Policy) -> Router<Store> {
    let (list_policy, new_policy, create_policy, open_policy, save_policy) = (
        policy.clone(),
        policy.clone(),
        policy.clone(),
        policy.clone(),
        policy,
    );
    Router::new()
        .route(
            "/declarations",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let policy = list_policy.clone();
                async move { list(store, &policy, headers).await }
            })
            .post(
                move |State(store): State<Store>, headers: HeaderMap, body: Bytes| {
                    let policy = create_policy.clone();
                    async move { save(store, &policy, headers, None, body).await }
                },
            ),
        )
        .route(
            "/declarations/new",
            get(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Query(slots): Query<Slots>| {
                    let policy = new_policy.clone();
                    async move { new(store, &policy, headers, slots).await }
                },
            ),
        )
        .route(
            "/declarations/{de}",
            get(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(de): Path<String>,
                      Query(slots): Query<Slots>| {
                    let policy = open_policy.clone();
                    async move { open(store, &policy, headers, de, slots).await }
                },
            )
            .post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(de): Path<String>,
                      body: Bytes| {
                    let policy = save_policy.clone();
                    async move { save(store, &policy, headers, Some(de), body).await }
                },
            ),
        )
}

/// The slots a screen offers: how many SPU boxes, at most [`MAX_BOXES`].
#[derive(Debug, Default, Deserialize)]
pub struct Slots {
    #[serde(default)]
    boxes: Option<usize>,
}

/// The session's person, an admin, or the page's answer: 401 without a
/// session, 403 without the grant. A read is not audited.
async fn reader(store: &Store, policy: &Policy, headers: &HeaderMap) -> Result<Session, Response> {
    match admin_session(store, policy, headers).await? {
        Ok(session) => Ok(session),
        Err(_) => Err((StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response()),
    }
}

fn author_words(author: Author) -> String {
    match author {
        Author::Person { name, .. } => name,
        Author::Claim(claim) => format!("{claim} (a claim)"),
        Author::Absent => "nobody named".to_owned(),
    }
}

fn when(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

struct ListRow {
    name: String,
    head: String,
    version: i64,
    saved_at: String,
    author: String,
}

#[derive(Template)]
#[template(path = "declarations.html")]
struct ListPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    rows: Vec<ListRow>,
}

/// **Every lineage, by name, with its head.**
async fn list(store: Store, policy: &Policy, headers: HeaderMap) -> Response {
    let session = match reader(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let read = async {
        let mut rows = Vec::new();
        for lineage in store.lineages().await? {
            rows.push(ListRow {
                author: author_words(store.author(lineage.author.as_deref()).await?),
                name: lineage.name,
                head: lineage.head,
                version: lineage.version,
                saved_at: when(lineage.saved_at),
            });
        }
        anyhow::Ok(rows)
    };
    let rows = match read.await {
        Ok(rows) => rows,
        Err(e) => return fault(e),
    };
    let chrome = match super::Chrome::of(&store, &session.person_id).await {
        Ok(chrome) => chrome,
        Err(e) => return fault(e),
    };
    let page = ListPage {
        here: "declarations",
        who: session.name,
        admin: true,
        chrome,
        rows,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// One version in a row's lineage, as its page lists it.
struct Version {
    de: String,
    version: i64,
    saved_at: String,
    author: String,
    moved: String,
    this: bool,
}

/// **What a saved row shows beside the form**: its version, last save,
/// author and standing, the class readings and the two panes.
struct Opened {
    de: String,
    version: i64,
    saved_at: String,
    author: String,
    head: bool,
    head_de: String,
    head_version: i64,
    stale: Option<String>,
    readings: Vec<Reading>,
    file: String,
    prompt: Option<String>,
    versions: Vec<Version>,
}

#[derive(Template)]
#[template(path = "declaration.html")]
struct ScreenPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    /// Where the form posts.
    action: String,
    /// Where "more boxes" asks.
    slots_href: String,
    more_boxes: usize,
    refusal: Option<String>,
    form: Form,
    opened: Option<Opened>,
    /// The table's models and families, offered as the requirement's
    /// suggestions.
    suggestions: Vec<&'static str>,
    sampler_hints: Vec<(&'static str, &'static str)>,
    /// The corpus commit this build's file follows.
    corpus: &'static str,
}

fn suggestions() -> Vec<&'static str> {
    let mut out = capability::families();
    out.extend(capability::TABLE.iter().map(|card| card.model));
    out
}

fn sampler_hints() -> Vec<(&'static str, &'static str)> {
    crate::declaration::SAMPLER
        .iter()
        .map(|(name, real)| {
            (
                *name,
                if *real {
                    "a finite number"
                } else {
                    "a whole number, 0 or more"
                },
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn screen(
    store: &Store,
    session: &Session,
    status: StatusCode,
    action: String,
    slots_href: String,
    form: Form,
    refusal: Option<String>,
    opened: Option<Opened>,
) -> Response {
    let chrome = match super::Chrome::of(store, &session.person_id).await {
        Ok(chrome) => chrome,
        Err(e) => return fault(e),
    };
    let more_boxes = (form.boxes.len() + 1).min(MAX_BOXES);
    let page = ScreenPage {
        here: "declarations",
        who: session.name.clone(),
        admin: true,
        chrome,
        action,
        slots_href,
        more_boxes,
        refusal,
        form,
        opened,
        suggestions: suggestions(),
        sampler_hints: sampler_hints(),
        corpus: CORPUS_COMMIT,
    };
    match page.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(e) => fault(e),
    }
}

/// **The screen, empty.**
async fn new(store: Store, policy: &Policy, headers: HeaderMap, slots: Slots) -> Response {
    let session = match reader(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let boxes = slots.boxes.unwrap_or(1).clamp(1, MAX_BOXES);
    screen(
        &store,
        &session,
        StatusCode::OK,
        "/declarations".to_owned(),
        "/declarations/new".to_owned(),
        Form::empty(boxes),
        None,
        None,
    )
    .await
}

/// **A saved row reopened**: its document filled into the screen, and what
/// it shows beside the form.
async fn open(
    store: Store,
    policy: &Policy,
    headers: HeaderMap,
    de: String,
    slots: Slots,
) -> Response {
    if !is_declaration_id(&de) {
        return (StatusCode::BAD_REQUEST, MALFORMED).into_response();
    }
    let session = match reader(&store, policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let row = match store.declaration(&de).await {
        Ok(Some(row)) => row,
        Ok(None) => return refused(&Refusal::NoSuchDeclaration),
        Err(e) => return fault(e),
    };
    let document = match Document::parse(&row.body) {
        Ok(document) => document,
        Err(e) => return fault(anyhow::anyhow!("a saved declaration does not read: {e}")),
    };
    let read = async {
        let lineage = store.lineage(row.root_id).await?;
        let mut versions = Vec::new();
        for r in &lineage {
            versions.push(Version {
                de: r.de.clone(),
                version: r.version,
                saved_at: when(r.saved_at),
                author: author_words(store.author(r.author.as_deref()).await?),
                moved: if r.parent.is_none() {
                    "the first save".to_owned()
                } else if r.what_moved.is_empty() {
                    "nothing".to_owned()
                } else {
                    r.what_moved.join(", ")
                },
                this: r.de == row.de,
            });
        }
        let author = author_words(store.author(row.author.as_deref()).await?);
        anyhow::Ok((lineage, versions, author))
    };
    let (lineage, versions, author) = match read.await {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    let head = lineage.last().map(|h| (h.de.clone(), h.version));
    let (head_de, head_version) = head.unwrap_or_else(|| (row.de.clone(), row.version));
    // `?boxes=` asks for that many box slots in all, as on the empty
    // screen: the row's own boxes and empty ones after them.
    let own = document.boxes.len();
    let extra = slots.boxes.unwrap_or(0).min(MAX_BOXES).saturating_sub(own);
    let opened = Opened {
        file: render::agent_toml(
            &document,
            &Meta {
                de: &row.de,
                version: row.version,
                corpus_commit: &row.corpus_commit,
            },
        ),
        prompt: render::prompt(&document).map(str::to_owned),
        readings: document.readings(),
        stale: (row.corpus_commit != CORPUS_COMMIT).then(|| row.corpus_commit.clone()),
        de: row.de.clone(),
        version: row.version,
        saved_at: when(row.saved_at),
        author,
        head: row.head,
        head_de,
        head_version,
        versions,
    };
    screen(
        &store,
        &session,
        StatusCode::OK,
        format!("/declarations/{}", row.de),
        format!("/declarations/{}", row.de),
        Form::of(&document, extra),
        None,
        Some(opened),
    )
    .await
}

/// **A save**, of a new lineage's root (`from` none) or from a row.
async fn save(
    store: Store,
    policy: &Policy,
    headers: HeaderMap,
    from: Option<String>,
    body: Bytes,
) -> Response {
    // (1) The path's identity and the form, a refusal recorded nowhere.
    if let Some(from) = &from
        && !is_declaration_id(from)
    {
        return (StatusCode::BAD_REQUEST, MALFORMED).into_response();
    }
    let form = match Form::read(&body) {
        Ok(form) => form,
        Err(refusal) => return (StatusCode::BAD_REQUEST, refusal.to_string()).into_response(),
    };
    let action = match &from {
        Some(from) => format!("/declarations/{from}"),
        None => "/declarations".to_owned(),
    };
    let document = match form.document() {
        Ok(document) => document,
        Err(refusal) => {
            return refused_form(&store, policy, &headers, action, form, refusal.to_string()).await;
        }
    };
    // (2) The admin gate, its refusal one record naming the row the save
    // would mint, before any row is looked up.
    let de = match store.mint_key("de").await {
        Ok(de) => de,
        Err(e) => return fault(e),
    };
    let session = match writer(&store, policy, &headers, Target::Declaration(&de), ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    // (3) The row saved from, its head and the name, recorded nowhere.
    let resolved = async {
        match &from {
            Some(from) => match store.declaration(from).await? {
                None => anyhow::Ok(Err(Refusal::NoSuchDeclaration)),
                Some(parent) => Ok(saveable(&parent, &document)),
            },
            None if store.declaration_name_held(&document.name).await? => {
                Ok(Err(Refusal::NameHeld(document.name.clone())))
            }
            None => Ok(Ok(())),
        }
    };
    match resolved.await {
        Ok(Ok(())) => {}
        Ok(Err(refusal)) => return refused(&refusal),
        Err(e) => return fault(e),
    }
    // (4) The audited write.
    let first = match first_of(&store, &session, Target::Declaration(&de), ACTION).await {
        Ok(first) => first,
        Err(answer) => return answer,
    };
    let written = store
        .save_declaration(
            &session.person_id,
            session.session_id,
            &first,
            &de,
            from.as_deref(),
            &document,
        )
        .await;
    settled(&store, &first, &written).await;
    match written {
        Ok(Ok(())) => (
            StatusCode::SEE_OTHER,
            [(header::LOCATION, format!("/declarations/{de}"))],
        )
            .into_response(),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

/// **A document the form refused**, recorded nowhere: the screen again with
/// what was typed and the refusal naming its field, where the session is
/// an admin's; the refusal's words alone otherwise, so no page is shown to
/// a session the gate would refuse.
async fn refused_form(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
    action: String,
    mut form: Form,
    refusal: String,
) -> Response {
    match admin_session(store, policy, headers).await {
        Ok(Ok(session)) => {
            form.slotted();
            let slots_href = if action == "/declarations" {
                "/declarations/new".to_owned()
            } else {
                action.clone()
            };
            screen(
                store,
                &session,
                StatusCode::BAD_REQUEST,
                action,
                slots_href,
                form,
                Some(refusal),
                None,
            )
            .await
        }
        _ => (StatusCode::BAD_REQUEST, refusal).into_response(),
    }
}
