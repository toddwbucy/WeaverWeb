//! conforms: web-reading-an-agent-takes-a-grant-on-it
//!
//! **Reading an agent** (design sections 6 and 9; Spec 2.12 and 2.13; act
//! 11, PR 6a): the agents a person holds a grant on, one agent's register
//! row with the person's roles on it, and the live trace view. A plain
//! shell, per the operator's backend-first ruling of 2026-10-09.
//!
//! **A person without a grant on an agent sees nothing of it**: it is
//! absent from their list, and its page and its live view answer exactly as
//! an agent that does not exist, so neither is an oracle for agent
//! identities. The identity is parsed to its shape first. The check is the
//! store's (`Store::roles_on`), at every read, under the identity exclusion
//! held shared for the check alone. No read is audited.
//!
//! **The live view is one request that re-checks**: every fifteen seconds
//! it reads its session again, as every use does (closed, disabled, passkey
//! removed, idle, the twelve-hour limit, the last use refreshed at most once
//! a minute), and the person's grant, and closes at the first that fails.
//! It is the listener's live window, what the server holds in memory now,
//! and says so: not a record.

use std::convert::Infallible;
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use crate::fault::Fault;
use crate::link::register::{Agent, CredentialState};
use crate::store::identity::is_agent_id;
use crate::store::{AgentId, Store};
use crate::surfaces::gate::{self, Policy, Session};
use crate::surfaces::record::NoSession;
use crate::traceview::TraceViews;

/// **How often an open live view re-checks its session and grant**.
pub const RECHECK: Duration = Duration::from_secs(15);

/// The answer to an agent the person may not read, identical to the one
/// for an agent that does not exist.
const NO_SUCH_AGENT: &str = "no such agent";

/// What the live view says it is.
const LIVE_WINDOW: &str = "the live window: what this server holds in memory of the agent's trace now, not a record; a restart or a full ring loses it";

/// **What the reading surface holds beside the store**: the session's
/// policy, the listener's live windows, and the re-check's interval.
#[derive(Clone)]
pub struct Seams {
    pub policy: Policy,
    pub windows: TraceViews,
    pub recheck: Duration,
}

/// The reading surface, taking its seams as its own argument.
pub fn routes(seams: Seams) -> Router<Store> {
    let (list_seams, page_seams, trace_seams) = (seams.clone(), seams.clone(), seams);
    Router::new()
        .route(
            "/agents",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let seams = list_seams.clone();
                async move { list(store, &seams, headers).await }
            }),
        )
        .route(
            "/agents/{agent}",
            get(
                move |State(store): State<Store>, headers: HeaderMap, Path(agent): Path<String>| {
                    let seams = page_seams.clone();
                    async move { page(store, &seams, headers, agent).await }
                },
            ),
        )
        .route(
            "/agents/{agent}/trace",
            get(
                move |State(store): State<Store>, headers: HeaderMap, Path(agent): Path<String>| {
                    let seams = trace_seams.clone();
                    async move { trace(store, seams, headers, agent).await }
                },
            ),
        )
}

fn fault(e: impl Into<anyhow::Error>) -> Response {
    Fault::from(e.into()).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, NO_SUCH_AGENT).into_response()
}

async fn session_of(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
) -> Result<Session, Response> {
    match gate::session(store, policy, headers).await {
        Ok(Some(session)) => Ok(session),
        Ok(None) => Err(NoSession.into_response()),
        Err(e) => Err(fault(e)),
    }
}

/// **An agent the session's person may read**: the identity parsed, the
/// session standing, and a grant held on it; otherwise the same answer as
/// for an agent that does not exist.
async fn readable(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
    agent: &str,
) -> Result<(Session, AgentId, Vec<String>), Response> {
    if !is_agent_id(agent) {
        return Err((
            StatusCode::BAD_REQUEST,
            "the agent asked for is not of its identity's shape",
        )
            .into_response());
    }
    let id: AgentId = agent.parse().map_err(|e| fault(anyhow::Error::msg(e)))?;
    let session = session_of(store, policy, headers).await?;
    let roles = store
        .roles_on(&session.person_id, &id)
        .await
        .map_err(fault)?;
    if roles.is_empty() {
        return Err(not_found());
    }
    Ok((session, id, roles))
}

struct ListRow {
    agent_id: String,
    label: String,
    gate: bool,
    admin: bool,
}

#[derive(Template)]
#[template(path = "agents_list.html")]
struct ListPage {
    here: &'static str,
    who: String,
    admin: bool,
    rows: Vec<ListRow>,
}

/// **The agents the person holds a grant on**, and no other: a person with
/// none sees an empty list, never an error that reveals there are agents.
async fn list(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match session_of(&store, &seams.policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let (agents, admin) = match (
        store.readable_agents(&session.person_id).await,
        store.is_admin(&session.person_id).await,
    ) {
        (Ok(agents), Ok(admin)) => (agents, admin),
        (Err(e), _) | (_, Err(e)) => return fault(e),
    };
    let rows = agents
        .into_iter()
        .map(|a| ListRow {
            agent_id: a.agent_id.as_str().to_owned(),
            label: format!("{}/{}", a.r#box, a.name),
            gate: a.gate.connected,
            admin: a.admin.connected,
        })
        .collect();
    match (ListPage {
        here: "agents",
        who: session.name,
        admin,
        rows,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Template)]
#[template(path = "agent_page.html")]
struct AgentPage {
    here: &'static str,
    who: String,
    admin: bool,
    agent_id: String,
    label: String,
    roles: String,
    present: bool,
    gate: String,
    admin_plane: String,
    ceiling: String,
    door: String,
    load_state: String,
    tuple: String,
    constituents: String,
    window: &'static str,
}

fn stamp(at: Option<chrono::DateTime<chrono::Utc>>) -> String {
    at.map(|at| format!(" (since {})", at.format("%Y-%m-%d %H:%M:%S UTC")))
        .unwrap_or_default()
}

fn plane(credential: &crate::link::register::Credential) -> String {
    let state = match credential.state {
        CredentialState::Live => "live",
        CredentialState::Revoked => "revoked",
    };
    let link = if credential.connected {
        "connected"
    } else {
        "not connected"
    };
    format!("credential {state}, {link}{}", stamp(credential.link_at))
}

/// **One agent's register row and the person's roles on it**, for a
/// person holding a grant: presence per plane, the declared ceiling, the
/// trace door, the load state with its source and date, the run's tuple
/// and constituents where held.
async fn page(store: Store, seams: &Seams, headers: HeaderMap, agent: String) -> Response {
    let (session, id, roles) = match readable(&store, &seams.policy, &headers, &agent).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let row: Agent = match store.agent(&id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(e) => return fault(e),
    };
    let admin = match store.is_admin(&session.person_id).await {
        Ok(admin) => admin,
        Err(e) => return fault(e),
    };
    let none = || "none held".to_owned();
    let page = AgentPage {
        here: "agents",
        who: session.name,
        admin,
        agent_id: row.agent_id.as_str().to_owned(),
        label: format!("{}/{}", row.r#box, row.name),
        roles: roles.join(", "),
        present: row.present(),
        gate: plane(&row.gate),
        admin_plane: plane(&row.admin),
        ceiling: row
            .ceiling
            .as_ref()
            .map(|c| format!("{}{}", c.join(" "), stamp(row.ceiling_at)))
            .unwrap_or_else(|| "none declared".to_owned()),
        door: row
            .trace_door
            .map(|open| {
                format!(
                    "{}{}",
                    if open { "open" } else { "closed" },
                    stamp(row.trace_door_at)
                )
            })
            .unwrap_or_else(|| "not reported".to_owned()),
        load_state: row
            .load_state
            .as_ref()
            .map(|state| {
                format!(
                    "{state}, by {}{}",
                    row.state_source.as_deref().unwrap_or("an unnamed source"),
                    stamp(row.load_state_at)
                )
            })
            .unwrap_or_else(none),
        tuple: row
            .tuple
            .as_ref()
            .map(|tuple| {
                format!(
                    "{tuple}, by {}{}",
                    row.tuple_source.as_deref().unwrap_or("an unnamed source"),
                    stamp(row.tuple_at)
                )
            })
            .unwrap_or_else(none),
        constituents: row
            .constituents
            .as_ref()
            .map(|pids| {
                let pids: Vec<String> = pids.iter().map(i32::to_string).collect();
                format!("{}{}", pids.join(" "), stamp(row.constituents_at))
            })
            .unwrap_or_else(none),
        window: LIVE_WINDOW,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// **The live trace view**: the window's events as they stand, then each
/// as it arrives, the session and the grant re-checked every interval and
/// the view closed, with its reason, at the first check that fails.
async fn trace(store: Store, seams: Seams, headers: HeaderMap, agent: String) -> Response {
    let (_, id, _) = match readable(&store, &seams.policy, &headers, &agent).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    seams.windows.ensure(id.as_str());
    let (Some(mut live), Some(held)) = (
        seams.windows.subscribe(id.as_str()),
        seams.windows.snapshot(id.as_str()),
    ) else {
        return fault(anyhow::anyhow!("a live window ensured is not there"));
    };
    let (tx, rx) = tokio::sync::mpsc::channel::<SseEvent>(64);
    tokio::spawn(async move {
        if tx
            .send(SseEvent::default().event("window").data(LIVE_WINDOW))
            .await
            .is_err()
        {
            return;
        }
        let mut cursor = 0u64;
        for event in held {
            cursor = event.seq;
            if tx.send(trace_event(&event)).await.is_err() {
                return;
            }
        }
        let mut recheck =
            tokio::time::interval_at(tokio::time::Instant::now() + seams.recheck, seams.recheck);
        loop {
            tokio::select! {
                // **A viewer gone ends the view at once**: a quiet agent
                // sends nothing, so a send's failure would not notice a
                // closed tab, and the task and its subscription would live
                // until the session's or the grant's end.
                _ = tx.closed() => return,
                received = live.recv() => match received {
                    Ok(event) if event.seq > cursor => {
                        cursor = event.seq;
                        if tx.send(trace_event(&event)).await.is_err() {
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        let mark = SseEvent::default()
                            .event("mark")
                            .data(format!("the view fell behind the window: {n} events not shown"));
                        if tx.send(mark).await.is_err() {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
                _ = recheck.tick() => {
                    if let Some(why) = ended(&store, &seams.policy, &headers, &id).await {
                        let _ = tx.send(SseEvent::default().event("closed").data(why)).await;
                        return;
                    }
                }
            }
        }
    });
    Sse::new(ReceiverStream::new(rx).map(Ok::<SseEvent, Infallible>))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// **What ended the view**, where something did: the session (at its use,
/// which refreshes its last use at most once a minute) or the grant.
async fn ended(
    store: &Store,
    policy: &Policy,
    headers: &HeaderMap,
    id: &AgentId,
) -> Option<&'static str> {
    let session = match gate::session(store, policy, headers).await {
        Ok(Some(session)) => session,
        Ok(None) => return Some("the session ended"),
        Err(_) => return Some("the session could not be read"),
    };
    match store.roles_on(&session.person_id, id).await {
        Ok(roles) if !roles.is_empty() => None,
        Ok(_) => Some("the grant on this agent ended"),
        Err(_) => Some("the grant could not be read"),
    }
}

fn trace_event(event: &crate::traceview::TraceEvent) -> SseEvent {
    let event_name = if event.mark.is_some() {
        "mark"
    } else {
        "trace"
    };
    SseEvent::default()
        .event(event_name)
        .id(event.seq.to_string())
        .data(serde_json::to_string(event).unwrap_or_default())
}
