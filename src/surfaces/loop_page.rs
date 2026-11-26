//! conforms: web-the-first-draft-is-todays-loop-as-wires
//! conforms: web-the-loop-page-says-a-saved-loop-is-not-on-the-appliance
//!
//! **The loop page** (Spec 6.3; the operator's words of 2026-11-25, "The
//! loop page for January is a text editor on the wire file, plus reload"):
//! one page per agent, for every person holding a grant on it, read as
//! every reading page is (`reading::readable`: the identity parsed first,
//! no grant answered exactly as an unknown agent). Reads are not audited.
//!
//! **The wire file** is the agent's stored head, or, where none is saved,
//! today's compiled loop written as wires, offered as the first draft and
//! not saved: the file the agent's side compares with its compiled loop,
//! carried byte for byte in `tests/fixtures/loop-wires-today.py` and
//! pinned by its sha256. **One sentence says what a save does not do**:
//! a saved loop is kept here and is not on the appliance, until loop
//! install lays it there. **Which loop ran** is the register row's member
//! the live load event wrote (Spec 2.12), never a guess at what the
//! appliance holds now.

use askama::Template;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use crate::store::loop_text::LoopVersion;
use crate::store::{AgentId, Store};
use crate::surfaces::reading::{Seams, fault, not_found, readable, when};

/// **Today's compiled loop written as wires**, the first draft: the agent's
/// side's file at its pushed head, carried byte for byte.
pub const FIRST_DRAFT: &str = include_str!("../../tests/fixtures/loop-wires-today.py");

/// The first draft's sha256 as the agent's side's file was measured, its
/// pin's figure.
pub const FIRST_DRAFT_SHA256: &str =
    "e083d6decb4abb1480947ed8d6c8eecb14bf2db877e5f894d0697cf143fa1af0";

/// **What a save does not do**, beside the stored digest (the Planner's
/// read of 2026-11-26): until the laying lines exist a reload runs the loop
/// the appliance holds.
pub const NOT_ON_THE_APPLIANCE: &str = "A saved loop is kept in Weaver Web and is not on the appliance. Until loop install lays it there, a reload runs the loop the appliance holds, the compiled loop where it holds none.";

/// What the pane says where no version is saved.
pub const NO_LOOP_SAVED: &str = "No loop is saved for this agent. The text below is today's compiled loop written as wires, offered as a first draft; it is not saved until you save it.";

/// The loop page's routes, merged into the reading surface's.
pub fn routes(seams: Seams) -> Router<Store> {
    Router::new().route(
        "/agents/{agent}/loop",
        get(
            move |State(store): State<Store>, headers: HeaderMap, Path(agent): Path<String>| {
                let seams = seams.clone();
                async move { page(store, &seams, headers, agent).await }
            },
        ),
    )
}

#[derive(Template)]
#[template(path = "agent_loop.html")]
struct LoopPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    agent_id: String,
    label: String,
    roles: String,
    head: Option<HeadView>,
    text: String,
    no_loop_saved: &'static str,
    not_on_the_appliance: &'static str,
    ran: String,
    ran_at: String,
}

/// The stored head as the pane says it.
struct HeadView {
    version: i32,
    bytes: i32,
    sha256: String,
    saved_by: String,
    saved_at: String,
}

/// **Which loop ran, said** (Spec 6.3): from the row's member and its
/// date's presence, and the saved version whose digest it names.
pub fn ran_said(member: Option<&str>, landed: bool, version: Option<i32>) -> String {
    match (member, landed) {
        (None, false) => "No load event is recorded for this agent.".to_owned(),
        (None, true) => "The last load named no loop this page reads.".to_owned(),
        (Some("compiled"), _) => "The compiled loop ran at the last load.".to_owned(),
        (Some(sha256), _) => match version {
            Some(n) => {
                format!("The wires ran at the last load, sha256 {sha256}, the saved version {n}.")
            }
            None => format!(
                "The wires ran at the last load, sha256 {sha256}, a digest no saved version here has."
            ),
        },
    }
}

async fn page(store: Store, seams: &Seams, headers: HeaderMap, agent: String) -> Response {
    let (session, id, roles) = match readable(&store, &seams.policy, &headers, &agent).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let row = match store.agent(&id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(e) => return fault(e),
    };
    let (head, ran, ran_at) = match read_loop(&store, &id).await {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    let (admin, chrome) = match (
        store.is_admin(&session.person_id).await,
        super::Chrome::of(&store, &session.person_id).await,
    ) {
        (Ok(admin), Ok(chrome)) => (admin, chrome),
        (Err(e), _) | (_, Err(e)) => return fault(e),
    };
    let text = head
        .as_ref()
        .map(|h| h.text.clone())
        .unwrap_or_else(|| FIRST_DRAFT.to_owned());
    let page = LoopPage {
        here: "agents",
        who: session.name,
        admin,
        chrome,
        agent_id: agent,
        label: format!("{}/{}", row.r#box, row.name),
        roles: roles.join(", "),
        head: head.map(|h| HeadView {
            version: h.version,
            bytes: h.bytes,
            sha256: h.sha256,
            saved_by: h.saved_by,
            saved_at: when(h.saved_at),
        }),
        text,
        no_loop_saved: NO_LOOP_SAVED,
        not_on_the_appliance: NOT_ON_THE_APPLIANCE,
        ran,
        ran_at,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// The head, and which loop ran said with its date.
async fn read_loop(
    store: &Store,
    id: &AgentId,
) -> anyhow::Result<(Option<LoopVersion>, String, String)> {
    let head = store.loop_head(id).await?;
    let (member, at) = store.loop_ran(id).await?;
    let version = match member.as_deref() {
        Some(sha256) if sha256 != "compiled" => store.loop_version_of(id, sha256).await?,
        _ => None,
    };
    let ran = ran_said(member.as_deref(), at.is_some(), version);
    Ok((head, ran, at.map(when).unwrap_or_default()))
}
