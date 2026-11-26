//! conforms: web-the-first-draft-is-todays-loop-as-wires
//! conforms: web-the-loop-page-says-a-saved-loop-is-not-on-the-appliance
//! conforms: web-a-wire-file-is-refused-past-its-bound-empty-with-a-nul-or-not-utf8
//! conforms: web-only-an-operator-holder-saves-a-loop
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
//!
//! **A save** (`POST /agents/{agent}/loop`) takes the admin writes' one
//! order (Spec 2.13, 6.3): the form parsed from its raw bytes with no
//! record, the session, the version's `lp-` identity minted, the grant
//! with one refusal record, the agent resolved, the first record, then the
//! store's transaction (`Store::save_loop`). The text enters no record,
//! log line or error.

use askama::Template;
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity::is_agent_id;
use crate::store::loop_text::{LoopVersion, Saved, WIRE_FILE_MOST, action, digest};
use crate::store::{AgentId, Store};
use crate::surfaces::reading::{Seams, fault, not_found, readable, session_of, when};

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

/// **The bound the save's form is read under** (Spec 6.3): 2 MiB. The
/// largest file the contract admits, in its longest form, is 262,144 line
/// breaks, each one stored byte sent as `%0D%0A`: 1,572,864 bytes of value,
/// with the field names and the largest version beside it. A body past
/// the bound is the framework's 413.
pub const LOOP_FORM_MOST: usize = 2 * 1024 * 1024;

/// The most a version read may be: a Postgres `integer`.
const VERSION_MOST: i64 = i32::MAX as i64;

/// The loop page's routes, merged into the reading surface's.
pub fn routes(seams: Seams) -> Router<Store> {
    let save_seams = seams.clone();
    Router::new().route(
        "/agents/{agent}/loop",
        get(
            move |State(store): State<Store>, headers: HeaderMap, Path(agent): Path<String>| {
                let seams = seams.clone();
                async move { page(store, &seams, headers, agent).await }
            },
        )
        .post(
            move |State(store): State<Store>,
                  headers: HeaderMap,
                  Path(agent): Path<String>,
                  body: Bytes| {
                let seams = save_seams.clone();
                async move { save(store, &seams, headers, agent, body).await }
            },
        )
        .layer(DefaultBodyLimit::max(LOOP_FORM_MOST)),
    )
}

/// A number with its thousands grouped by commas, as the refusals say it.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One form value percent-decoded to its bytes, `+` a space; `None` where
/// a `%` is not followed by two hex digits.
fn percent_decoded(raw: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = raw.get(i + 1..i + 3)?;
                let hex = std::str::from_utf8(hex).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    Some(out)
}

/// What the form must hold, sent by any client.
pub const NOT_THE_FORM: &str =
    "The form is not the loop page's: it carries the text and the version once each.";
/// The version's refusal.
pub const NOT_A_VERSION: &str = "The version is not a whole number of at most 2,147,483,647.";

/// **The save's form, parsed with no record** (Spec 6.3): from the raw
/// bytes, each value percent-decoded and then read as UTF-8 strictly, never
/// lossily; each CRLF stored as LF; the text refused empty, past 262,144
/// bytes, holding a NUL or not UTF-8; the version a whole number of at
/// most 2^31 - 1. Each refusal is one sentence.
pub fn parse_save(body: &[u8]) -> Result<(String, i32), String> {
    let (mut text, mut version) = (None, None);
    for pair in body.split(|b| *b == b'&').filter(|p| !p.is_empty()) {
        let (name, value) = match pair.iter().position(|b| *b == b'=') {
            Some(at) => (&pair[..at], &pair[at + 1..]),
            None => (pair, &pair[pair.len()..]),
        };
        let slot = match name {
            b"text" => &mut text,
            b"version" => &mut version,
            _ => return Err(NOT_THE_FORM.to_owned()),
        };
        if slot.is_some() {
            return Err(NOT_THE_FORM.to_owned());
        }
        *slot = Some(percent_decoded(value).ok_or_else(|| NOT_THE_FORM.to_owned())?);
    }
    let (Some(text), Some(version)) = (text, version) else {
        return Err(NOT_THE_FORM.to_owned());
    };
    let version = std::str::from_utf8(&version)
        .ok()
        .filter(|v| !v.is_empty() && v.len() <= 10 && v.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v <= VERSION_MOST)
        .ok_or_else(|| NOT_A_VERSION.to_owned())?;
    let text = String::from_utf8(text).map_err(|_| "The file is not UTF-8.".to_owned())?;
    let text = text.replace("\r\n", "\n");
    if text.is_empty() {
        return Err("The file is empty.".to_owned());
    }
    if text.len() > WIRE_FILE_MOST {
        return Err(format!(
            "The file is {} bytes; a wire file is at most {}.",
            grouped(text.len()),
            grouped(WIRE_FILE_MOST)
        ));
    }
    if text.contains('\0') {
        return Err("The file holds a NUL byte.".to_owned());
    }
    Ok((
        text,
        i32::try_from(version).map_err(|_| NOT_A_VERSION.to_owned())?,
    ))
}

/// **A save**, in the admin writes' one order (Spec 2.13, 6.3).
async fn save(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    agent: String,
    body: Bytes,
) -> Response {
    if !is_agent_id(&agent) {
        return (
            StatusCode::BAD_REQUEST,
            "the agent asked for is not of its identity's shape",
        )
            .into_response();
    }
    let (text, read) = match parse_save(&body) {
        Ok(parsed) => parsed,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let id: AgentId = match agent.parse() {
        Ok(id) => id,
        Err(e) => return fault(anyhow::Error::msg(e)),
    };
    let session = match session_of(&store, &seams.policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let loop_id = match store.mint_key("lp").await {
        Ok(loop_id) => loop_id,
        Err(e) => return fault(e),
    };
    let said = action(&digest(&text), text.len());
    let principal = Principal::Person {
        person_id: &session.person_id,
        method: PersonMethod::Session,
    };
    // **The grant, before the agent is resolved**: no grant answers as an
    // unknown agent, a grant without `operator` is 403, each one refusal
    // record, so the page is no oracle.
    let roles = match store.roles_on(&session.person_id, &id).await {
        Ok(roles) => roles,
        Err(e) => return fault(e),
    };
    let refused = if roles.is_empty() {
        Some((StatusCode::NOT_FOUND, "no grant on the agent"))
    } else if !roles.iter().any(|r| r == "operator") {
        Some((
            StatusCode::FORBIDDEN,
            "only a person holding operator on the agent saves its loop",
        ))
    } else {
        None
    };
    if let Some((status, why)) = refused {
        return match store
            .audit_refusal(principal, Target::Loop(&loop_id), &said, why)
            .await
        {
            Ok(_) if status == StatusCode::NOT_FOUND => not_found(),
            Ok(_) => (status, why).into_response(),
            Err(e) => fault(e.context("the refusal could not be recorded")),
        };
    }
    match store.agent(&id).await {
        Ok(Some(_)) => {}
        Ok(None) => return not_found(),
        Err(e) => return fault(e),
    }
    let first = match store
        .loop_save_first(&session.person_id, &loop_id, &text)
        .await
    {
        Ok(first) => first,
        Err(e) => return fault(e.context("the first record could not be written")),
    };
    let landed = store
        .save_loop(
            session.session_id,
            &session.person_id,
            &id,
            read,
            &loop_id,
            &text,
            &first,
        )
        .await;
    // **A commit whose fate is unknown writes no outcome** (Spec 2.13).
    let unknown = crate::store::commit::outcome_unknown(&landed);
    match landed {
        Ok(Saved::Done { .. }) => (
            StatusCode::SEE_OTHER,
            [(header::LOCATION, format!("/agents/{agent}/loop"))],
        )
            .into_response(),
        Ok(Saved::AuthorityGone) => {
            let _ = store.audit_outcome(&first, false).await;
            (
                StatusCode::FORBIDDEN,
                "your session or your operator grant ended before the save",
            )
                .into_response()
        }
        Ok(Saved::Stale) => {
            let _ = store.audit_outcome(&first, false).await;
            (
                StatusCode::CONFLICT,
                "A newer version was saved since you read the loop; read it again. Nothing was saved.",
            )
                .into_response()
        }
        Err(e) => {
            if !unknown {
                let _ = store.audit_outcome(&first, false).await;
            }
            fault(e)
        }
    }
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
    /// The person holds `operator` on the agent: the wire file is a form.
    operator: bool,
    /// The version read, 0 where none is saved: the form's.
    version: i32,
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
    let version = head.as_ref().map(|h| h.version).unwrap_or(0);
    let page = LoopPage {
        here: "agents",
        who: session.name,
        admin,
        chrome,
        agent_id: agent,
        label: format!("{}/{}", row.r#box, row.name),
        operator: roles.iter().any(|r| r == "operator"),
        version,
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
