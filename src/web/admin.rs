//! The admin surface: everything that crosses the operator boundary -
//! lifecycle verbs (sudo weaver-admin) and trace views. Every route
//! here is unavailable until the session/IAM act, per the operator
//! ruling of 2026-09-21. The handlers remain compiled behind that gate.

use super::{AppResult, AppState, nav_agents, sse_cursor};
use crate::lifecycle;
use crate::registry::Participant;
use crate::traceview::TraceEvent;
use askama::Template;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(unavailable))
        .route("/lifecycle", get(lifecycle_page))
        .route("/lifecycle/{agent}/{verb}", post(run_verb))
        .route("/agents/{agent}/config", get(agent_config))
        .route("/trace/{agent}", get(trace_page))
        .route("/trace/{agent}/stream", get(trace_stream))
}

/// The temporary legacy-admin refusal, per the W2 operator ruling.
async fn unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "the legacy admin surface is unavailable until the session/IAM act.",
    )
        .into_response()
}

/// Keep the legacy handlers compiled without consulting retired sessions.
async fn require_admin(
    _state: &AppState,
    _headers: &HeaderMap,
) -> anyhow::Result<Result<Participant, Response>> {
    Ok(Err(unavailable().await))
}

// ---------- lifecycle ----------

struct AgentRow {
    name: String,
    state: String,
    socket: String,
}

#[derive(Template)]
#[template(path = "lifecycle.html")]
struct LifecyclePage {
    nav_agents: Vec<String>,
    who: String,
    is_admin: bool,
    agents: Vec<AgentRow>,
    outcome: String,
}

/// The load-state rows: the connector's `status` answer over the
/// link, still the socket-existence inference the UI labels
/// (PRD section 3.6). A down or unresponsive link renders as unreachable
/// rather than unloaded - the absence of the observable is not the
/// observable's absence.
async fn agent_rows(state: &AppState) -> Vec<AgentRow> {
    let roster = state.link.roster().await;
    let status = state.link.status().await;
    roster
        .into_iter()
        .map(|name| match status.as_ref().and_then(|m| m.get(&name)) {
            Some(true) => AgentRow {
                name,
                state: "loaded".into(),
                socket: "present".into(),
            },
            Some(false) => AgentRow {
                name,
                state: "unloaded".into(),
                socket: "absent".into(),
            },
            // No box answered for this agent: its connection is down
            // or unresponsive, which is not the same fact as unloaded.
            None => AgentRow {
                name,
                state: "unreachable".into(),
                socket: "link down".into(),
            },
        })
        .collect()
}

async fn lifecycle_page(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    let me = match require_admin(&state, &headers).await? {
        Ok(p) => p,
        Err(refusal) => return Ok(refusal),
    };
    let page = LifecyclePage {
        nav_agents: nav_agents(&state).await,
        who: me.name,
        is_admin: true,
        agents: agent_rows(&state).await,
        outcome: String::new(),
    };
    Ok(Html(page.render()?).into_response())
}

async fn run_verb(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((agent, verb)): Path<(String, String)>,
) -> AppResult<Response> {
    let me = match require_admin(&state, &headers).await? {
        Ok(p) => p,
        Err(refusal) => return Ok(refusal),
    };
    if !state.link.has_agent(&agent).await {
        return Ok((StatusCode::NOT_FOUND, "no such agent").into_response());
    }
    if !lifecycle::VERBS.contains(&verb.as_str()) {
        return Ok((StatusCode::NOT_FOUND, "no such verb").into_response());
    }
    // The verb crosses the link; its outcome renders verbatim either
    // way, and a link failure is reported as itself, never swallowed.
    let outcome = match state.link.verb(&agent, &verb).await {
        Ok(o) => serde_json::to_string_pretty(&o)?,
        Err(e) => format!("verb not run: {e}"),
    };
    let page = LifecyclePage {
        nav_agents: nav_agents(&state).await,
        who: me.name,
        is_admin: true,
        agents: agent_rows(&state).await,
        outcome,
    };
    Ok(Html(page.render()?).into_response())
}

// ---------- agent declaration, read-only ----------

#[derive(Template)]
#[template(path = "agent_config.html")]
struct AgentConfigPage {
    nav_agents: Vec<String>,
    who: String,
    is_admin: bool,
    agent: String,
    path: String,
    content: String,
}

async fn agent_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent): Path<String>,
) -> AppResult<Response> {
    let me = match require_admin(&state, &headers).await? {
        Ok(p) => p,
        Err(refusal) => return Ok(refusal),
    };
    if !state.link.has_agent(&agent).await {
        return Ok((StatusCode::NOT_FOUND, "no such agent").into_response());
    }
    // The declaration lives on the agents' box; the connector reads
    // it (Spec section 8) and a read failure arrives as its own text.
    let (path, content) = state.link.declaration(&agent).await.unwrap_or_else(|| {
        (
            String::new(),
            "the link to the agents' box is down or unresponsive".into(),
        )
    });
    let page = AgentConfigPage {
        nav_agents: nav_agents(&state).await,
        who: me.name,
        is_admin: true,
        agent,
        path,
        content,
    };
    Ok(Html(page.render()?).into_response())
}

// ---------- trace ----------

/// The view filters: fields the operator elected to hide, and a search
/// needle. Both are view concerns, applied server-side per the
/// display-engine constraint, which the retired charter stated at its
/// section 3 and no standing document states, per the register.
/// Discontinuity marks bypass both:
/// a gap in the record is never filterable out of sight.
fn trace_filters(
    params: &HashMap<String, String>,
) -> (std::collections::BTreeSet<String>, Option<String>) {
    let hidden = params
        .keys()
        .filter_map(|k| k.strip_prefix("hide.").map(str::to_owned))
        .collect();
    let q = params
        .get("q")
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());
    (hidden, q)
}

/// The search runs over the full raw event, not the filtered view, so
/// hiding a field never hides a search hit.
fn event_matches(ev: &TraceEvent, q: &Option<String>) -> bool {
    if ev.mark.is_some() {
        return true;
    }
    match q {
        None => true,
        Some(q) => serde_json::to_string(&ev.raw)
            .map(|s| s.to_lowercase().contains(q.as_str()))
            .unwrap_or(true),
    }
}

/// Remove hidden fields from the rendered raw JSON. Top-level keys
/// match by name; payload subkeys match as `payload.<key>`.
fn filtered_raw(
    raw: &serde_json::Value,
    hidden: &std::collections::BTreeSet<String>,
) -> serde_json::Value {
    let mut v = raw.clone();
    if let Some(obj) = v.as_object_mut() {
        obj.retain(|k, _| !hidden.contains(k));
        if let Some(p) = obj.get_mut("payload").and_then(|p| p.as_object_mut()) {
            p.retain(|k, _| !hidden.contains(&format!("payload.{k}")));
        }
    }
    v
}

/// Percent-encode a query value for the SSE URL the template carries.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Template)]
#[template(path = "trace_event.html")]
struct TraceFragment {
    seq: u64,
    is_mark: bool,
    mark: String,
    run: String,
    turn: String,
    kind: String,
    raw: String,
}

fn render_trace_event(ev: &TraceEvent, hidden: &std::collections::BTreeSet<String>) -> String {
    TraceFragment {
        seq: ev.seq,
        is_mark: ev.mark.is_some(),
        mark: ev.mark.clone().unwrap_or_default(),
        run: ev.run.clone().unwrap_or_default(),
        turn: ev.turn.clone().unwrap_or_default(),
        kind: ev.kind.clone().unwrap_or_else(|| "?".into()),
        raw: serde_json::to_string_pretty(&filtered_raw(&ev.raw, hidden)).unwrap_or_default(),
    }
    .render()
    .unwrap_or_else(|e| format!("<div class=\"tev\">render error: {e}</div>"))
}

struct FieldControl {
    param: String,
    label: String,
    hidden: bool,
}

#[derive(Template)]
#[template(path = "trace.html")]
struct TracePage {
    nav_agents: Vec<String>,
    who: String,
    is_admin: bool,
    agent: String,
    events: Vec<String>,
    q: String,
    controls: Vec<FieldControl>,
    stream_url: String,
}

async fn trace_page(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Response> {
    let me = match require_admin(&state, &headers).await? {
        Ok(p) => p,
        Err(refusal) => return Ok(refusal),
    };
    let Some(snapshot) = state.traces.snapshot(&agent) else {
        return Ok((StatusCode::NOT_FOUND, "no such agent").into_response());
    };
    let (hidden, q) = trace_filters(&params);
    let cursor = snapshot.last().map(|e| e.seq).unwrap_or(0);

    // The field list is the union of what the record itself carries:
    // top-level keys plus payload subkeys, from the current window.
    let mut keys = std::collections::BTreeSet::new();
    for ev in &snapshot {
        if let Some(obj) = ev.raw.as_object() {
            for (k, v) in obj {
                keys.insert(k.clone());
                if k == "payload"
                    && let Some(p) = v.as_object()
                {
                    for pk in p.keys() {
                        keys.insert(format!("payload.{pk}"));
                    }
                }
            }
        }
    }
    let controls: Vec<FieldControl> = keys
        .into_iter()
        .map(|k| FieldControl {
            param: format!("hide.{k}"),
            hidden: hidden.contains(&k),
            label: k,
        })
        .collect();

    let mut stream_url = format!("/admin/trace/{agent}/stream?after={cursor}");
    for h in &hidden {
        stream_url.push_str(&format!("&hide.{}=on", urlencode(h)));
    }
    if let Some(q) = &q {
        stream_url.push_str(&format!("&q={}", urlencode(q)));
    }

    let page = TracePage {
        nav_agents: nav_agents(&state).await,
        who: me.name,
        is_admin: true,
        agent,
        events: snapshot
            .iter()
            .filter(|e| event_matches(e, &q))
            .map(|e| render_trace_event(e, &hidden))
            .collect(),
        q: q.unwrap_or_default(),
        controls,
        stream_url,
    };
    Ok(Html(page.render()?).into_response())
}

async fn trace_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(agent): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> AppResult<Response> {
    if let Err(refusal) = require_admin(&state, &headers).await? {
        return Ok(refusal);
    }
    let Some(mut live) = state.traces.subscribe(&agent) else {
        return Ok((StatusCode::NOT_FOUND, "no such agent").into_response());
    };
    // Clamp: a negative cursor would wrap into a huge u64 and filter
    // everything out forever.
    let mut cursor = sse_cursor(&headers, &params).max(0) as u64;
    let (hidden, q) = trace_filters(&params);
    let snapshot = state.traces.snapshot(&agent).unwrap_or_default();

    let (tx, rx) = tokio::sync::mpsc::channel::<SseEvent>(64);
    tokio::spawn(async move {
        let start = cursor;
        for ev in snapshot.iter().filter(|e| e.seq > start) {
            cursor = ev.seq;
            if !event_matches(ev, &q) {
                continue;
            }
            let sse = SseEvent::default()
                .event("trace")
                .id(ev.seq.to_string())
                .data(render_trace_event(ev, &hidden));
            if tx.send(sse).await.is_err() {
                return;
            }
        }
        loop {
            match live.recv().await {
                Ok(ev) => {
                    if ev.seq <= cursor {
                        continue;
                    }
                    cursor = ev.seq;
                    if !event_matches(&ev, &q) {
                        continue;
                    }
                    let sse = SseEvent::default()
                        .event("trace")
                        .id(ev.seq.to_string())
                        .data(render_trace_event(&ev, &hidden));
                    if tx.send(sse).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // The viewer fell behind the broadcast: a gap in
                    // this view, marked rather than silently dropped.
                    let sse = SseEvent::default().event("trace").data(format!(
                        "<div class=\"tev tev-mark\"><span class=\"mark\">DISCONTINUITY: \
                         viewer lagged, {n} events not shown</span></div>"
                    ));
                    if tx.send(sse).await.is_err() {
                        return;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            }
        }
    });

    let stream = ReceiverStream::new(rx).map(Ok::<SseEvent, Infallible>);
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response())
}
