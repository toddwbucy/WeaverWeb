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
//!
//! **A person asks a lifecycle verb from the agent's page** (Spec 2.13; act
//! 11, PR 6b2): `POST /agents/{ag}/verb`, the identity and the verb parsed
//! first (400, no record), an agent the register does not hold answered
//! as unknown (404, no record), then the ask through the listener
//! (`Listener::ask_as_person`), which checks the session, the person's
//! grants on the agent and the connection's ceiling under the identity
//! exclusion held shared to the enqueue, each refusal one record: no grant
//! answered exactly as an unknown agent, a grant without the verb 403, the
//! ceiling 403, no connection 503. The page then waits for admin's answer
//! to the silence bound and shows it, or answers 202 naming the first
//! record, its outcome written when admin answers. The page's forms offer
//! the verbs the person's grants carry and the ceiling grants; hiding one
//! is presentation, and the ask checks everything again.

use std::convert::Infallible;
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use tokio_stream::StreamExt;

use crate::adapters::gate::request_line;
use crate::fault::Fault;
use crate::link::listener::{Act, Asker, Enqueued, Listener, TurnError, TurnText, VerbError};
use crate::link::register::{Agent, CredentialState};
use crate::store::identity::{VOCABULARY, is_agent_id};
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
/// policy, the listener's live windows, the re-check's interval, and the
/// listener a person's verb is asked through.
#[derive(Clone)]
pub struct Seams {
    pub policy: Policy,
    pub windows: TraceViews,
    pub recheck: Duration,
    pub asker: Listener,
}

/// The reading surface, taking its seams as its own argument.
pub fn routes(seams: Seams) -> Router<Store> {
    let (list_seams, page_seams, trace_seams, verb_seams, turn_seams, ack_seams) = (
        seams.clone(),
        seams.clone(),
        seams.clone(),
        seams.clone(),
        seams.clone(),
        seams.clone(),
    );
    let choose_seams = seams;
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
            "/agents/{agent}/verb",
            post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(agent): Path<String>,
                      Form(asked): Form<VerbAsk>| {
                    let seams = verb_seams.clone();
                    async move { act(store, &seams, headers, agent, Asking::Verb(asked.verb)).await }
                },
            ),
        )
        .route(
            "/agents/{agent}/turn",
            post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(agent): Path<String>,
                      Form(placed): Form<TurnAsk>| {
                    let seams = turn_seams.clone();
                    async move { act(store, &seams, headers, agent, Asking::Turn(placed.text)).await }
                },
            ),
        )
        .route(
            "/agents/{agent}/alerts/{alert}/ack",
            post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path((agent, alert)): Path<(String, String)>,
                      Form(form): Form<super::alerts::AckForm>| {
                    let seams = ack_seams.clone();
                    async move {
                        super::alerts::acknowledge(store, &seams, headers, agent, alert, form.version)
                            .await
                    }
                },
            ),
        )
        .route(
            "/agents/{agent}/alerts/{alert}/choose",
            post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path((agent, alert)): Path<(String, String)>,
                      Form(form): Form<super::alerts::ChoiceForm>| {
                    let seams = choose_seams.clone();
                    async move {
                        super::alerts::choose(store, &seams, headers, agent, alert, form.option).await
                    }
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

pub(super) fn fault(e: impl Into<anyhow::Error>) -> Response {
    Fault::from(e.into()).into_response()
}

pub(super) fn not_found() -> Response {
    (StatusCode::NOT_FOUND, NO_SUCH_AGENT).into_response()
}

pub(super) async fn session_of(
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
    /// Its open alerts.
    alerts: i64,
}

#[derive(Template)]
#[template(path = "agents_list.html")]
struct ListPage {
    here: &'static str,
    who: String,
    admin: bool,
    open_alerts: i64,
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
    let open = match store.open_alerts_by_agent(&session.person_id).await {
        Ok(open) => open,
        Err(e) => return fault(e),
    };
    let open_on = |agent: &str| open.iter().find(|(a, _)| a == agent).map_or(0, |(_, n)| *n);
    let rows = agents
        .into_iter()
        .map(|a| ListRow {
            agent_id: a.agent_id.as_str().to_owned(),
            label: format!("{}/{}", a.r#box, a.name),
            gate: a.gate.connected,
            admin: a.admin.connected,
            alerts: open_on(a.agent_id.as_str()),
        })
        .collect();
    let open_alerts = open.iter().map(|(_, n)| n).sum();
    match (ListPage {
        here: "agents",
        who: session.name,
        admin,
        open_alerts,
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
    forms: Vec<&'static str>,
    turn_form: bool,
    shared: bool,
    open_alerts: i64,
    alerts: Vec<AlertRow>,
    /// Whether the person holds `operator` here, and so acknowledges.
    can_ack: bool,
    /// The open decision prompt, with the options the person may choose.
    prompt: Option<PromptView>,
}

/// An alert as the agent page lists it.
struct AlertRow {
    alert_id: String,
    kind: String,
    subject: String,
    summary: String,
    detail: String,
    occurrences: i32,
    raised: String,
    last: String,
    restored: String,
    acknowledged: String,
    version: i64,
    open: bool,
}

/// An open decision prompt, as its `<dialog open>` shows it.
struct PromptView {
    alert_id: String,
    summary: String,
    options: Vec<(&'static str, &'static str)>,
}

fn when(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
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
    // **The verbs offered**: the person's grants carry them and the live
    // connection's declared ceiling grants them. Presentation alone: the
    // ask checks everything again.
    let granted = match store.verbs_on(&session.person_id, &id).await {
        Ok(granted) => granted,
        Err(e) => return fault(e),
    };
    let ceiling = row
        .ceiling
        .as_ref()
        .filter(|_| row.admin.connected)
        .cloned()
        .unwrap_or_default();
    // **The turn form** where the person's grants carry `turn` and gate-con
    // is connected, and **the shared-conversation note** where more than
    // one enabled person holds `turn` on the agent, by the grants page's
    // own query, one definition (Spec 2.13).
    let turn_form = granted.contains("turn") && row.gate.connected;
    let shared = match store.shared_turn().await {
        Ok(shared) => shared.iter().any(|agent| agent == id.as_str()),
        Err(e) => return fault(e),
    };
    let forms: Vec<&'static str> = VOCABULARY
        .into_iter()
        .filter(|verb| lifecycle(verb))
        .filter(|verb| granted.contains(*verb) && ceiling.iter().any(|c| c == verb))
        .collect();
    // **The agent's alerts** (Spec 2.14): seen by every grant holder,
    // acknowledged by an `operator` holder; the open prompt offers only the
    // options the person's grants and the connection's ceiling carry, and
    // each choice is checked again as the person's asks.
    let stored = match store.alerts_on(&id, 50).await {
        Ok(stored) => stored,
        Err(e) => return fault(e),
    };
    let open_alerts = match store.open_alerts_by_agent(&session.person_id).await {
        Ok(open) => open.iter().map(|(_, n)| n).sum(),
        Err(e) => return fault(e),
    };
    let can_ack = roles.iter().any(|r| r == "operator");
    let carries = |verb: &str| granted.contains(verb) && ceiling.iter().any(|c| c == verb);
    let prompt = stored
        .iter()
        .find(|a| a.acknowledged_at.is_none() && a.kind == "save_point_missed")
        .map(|a| PromptView {
            alert_id: a.alert_id.clone(),
            summary: a.summary.clone(),
            options: [
                (
                    crate::link::listener::SAVE_POINT_THEN_UNLOAD,
                    "Save point, then unload (the unload only once the save point is taken and read back to its manifest line)",
                    carries("save-point") && carries("unload"),
                ),
                (
                    crate::link::listener::FORCE_UNLOAD,
                    "Force-unload (loses the state since the last published save point)",
                    carries("force-unload"),
                ),
            ]
            .into_iter()
            .filter(|(option, _, offered)| {
                *offered && a.options.iter().any(|o| o == option)
            })
            .map(|(option, label, _)| (option, label))
            .collect(),
        });
    let alerts = stored
        .into_iter()
        .map(|a| AlertRow {
            open: a.acknowledged_at.is_none(),
            acknowledged: match (a.acknowledged_at, &a.acknowledged_by) {
                (Some(at), Some(by)) => format!("acknowledged {} by {by}", when(at)),
                _ => String::new(),
            },
            restored: a
                .restored_at
                .map(|at| format!("contact restored {}", when(at)))
                .unwrap_or_default(),
            raised: when(a.raised_at),
            last: when(a.last_at),
            alert_id: a.alert_id,
            kind: a.kind,
            subject: a.subject,
            summary: a.summary,
            detail: a.detail,
            occurrences: a.occurrences,
            version: a.version,
        })
        .collect();
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
        forms,
        turn_form,
        shared,
        open_alerts,
        alerts,
        can_ack,
        prompt,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// A verb asked from the agent's page.
#[derive(serde::Deserialize)]
struct VerbAsk {
    verb: String,
}

/// A turn placed from the agent's page: its text, which goes into its
/// frame and nowhere else.
#[derive(serde::Deserialize)]
struct TurnAsk {
    text: String,
}

/// What the page asks: a verb, or a turn's text.
enum Asking {
    Verb(String),
    Turn(String),
}

/// **A lifecycle verb**: the roles' vocabulary without `turn`, which is a
/// turn's and not a verb's.
fn lifecycle(verb: &str) -> bool {
    verb != "turn" && VOCABULARY.contains(&verb)
}

#[derive(Template)]
#[template(path = "verb_answer.html")]
struct VerbPage {
    here: &'static str,
    who: String,
    admin: bool,
    agent_id: String,
    label: String,
    verb: String,
    first: String,
    kind: String,
    detail: String,
    open_alerts: i64,
    /// **The leg an `unload` stopped at its save point missed** (Spec
    /// 2.14), whose prompt the page states, its options chosen on the
    /// agent's page, where the alert stands.
    missed: Option<String>,
}

/// **A person's verb, asked through the server** (Spec 2.13): parsed,
/// resolved, asked under the listener's hold, and its answer shown, or
/// 202 past the silence bound naming its first record.
async fn act(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    agent: String,
    asking: Asking,
) -> Response {
    if !is_agent_id(&agent) {
        return (
            StatusCode::BAD_REQUEST,
            "the agent asked for is not of its identity's shape",
        )
            .into_response();
    }
    // **The payload parsed first**: a turn's text is checked against the
    // gate's own line bound here, so a `line_too_long` is the ask's fault
    // and never reaches gate-con. Nothing of the text is answered back.
    match &asking {
        Asking::Verb(verb) if !lifecycle(verb) => {
            return (StatusCode::BAD_REQUEST, "not a lifecycle verb").into_response();
        }
        Asking::Turn(text) if text.trim().is_empty() => {
            return (StatusCode::BAD_REQUEST, "a turn needs its text").into_response();
        }
        Asking::Turn(text) if request_line(text).is_err() => {
            return (
                StatusCode::BAD_REQUEST,
                "the turn is longer than the gate's line bound",
            )
                .into_response();
        }
        _ => {}
    }
    let id: AgentId = match agent.parse() {
        Ok(id) => id,
        Err(e) => return fault(anyhow::Error::msg(e)),
    };
    let session = match session_of(&store, &seams.policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    // **Admitted before anything is resolved**: past the server's bound on
    // outstanding asks, or the person's share of it, the request is not
    // taken, a known agent and an unknown one alike, and nothing is
    // recorded, as for a malformed one; a record per shed request would
    // rebuild the growth the bound prevents.
    let Some(admission) = seams.asker.admit_ask(&session.person_id) else {
        return (StatusCode::SERVICE_UNAVAILABLE, VerbError::Shed.to_string()).into_response();
    };
    // **Resolved before any record**: an agent the register does not hold
    // is unknown, and nothing is recorded of it.
    let row = match store.agent(&id).await {
        Ok(Some(row)) => row,
        Ok(None) => return not_found(),
        Err(e) => return fault(e),
    };
    let asker = Asker {
        session_id: session.session_id,
        person_id: session.person_id.clone(),
    };
    let (shown, act) = match asking {
        Asking::Verb(verb) => (verb.clone(), Act::Verb(verb)),
        Asking::Turn(text) => ("turn".to_owned(), Act::Turn(TurnText::new(text))),
    };
    let enqueued = match seams.asker.ask_as_person(admission, &asker, &id, act).await {
        Ok(enqueued) => enqueued,
        // **No grant answers as an unknown agent**, so the page is no
        // oracle for agent identities.
        Err(VerbError::NoGrant) => return not_found(),
        Err(refused @ (VerbError::NotGranted { .. } | VerbError::OutsideCeiling { .. })) => {
            return (StatusCode::FORBIDDEN, refused.to_string()).into_response();
        }
        // Not connected, the persons' slots all taken, the connection lost
        // after the first, or the ask's bound passed on the server's own
        // waits (logged at error): nothing was asked.
        Err(
            refused @ (VerbError::NotConnected
            | VerbError::Busy
            | VerbError::Lost
            | VerbError::Stalled),
        ) => {
            return (StatusCode::SERVICE_UNAVAILABLE, refused.to_string()).into_response();
        }
        Err(e) => return fault(anyhow::Error::new(e)),
    };
    let first = enqueued.first.clone();
    // **The page's wait is its own**, bounding nothing of the ask: the
    // outcome is written at the answer whatever this does.
    let bound = seams.asker.silence();
    let (status, kind, detail, missed) = if shown == "turn" {
        let (status, kind, detail) = turn_shown(bound, enqueued).await;
        (status, kind, detail, None)
    } else {
        verb_shown(bound, &shown, enqueued).await
    };
    let (admin, open_alerts) = match (
        store.is_admin(&session.person_id).await,
        store.open_alerts_by_agent(&session.person_id).await,
    ) {
        (Ok(admin), Ok(open)) => (admin, open.iter().map(|(_, n)| n).sum()),
        (Err(e), _) | (_, Err(e)) => return fault(e),
    };
    let page = VerbPage {
        here: "agents",
        who: session.name,
        admin,
        agent_id: row.agent_id.as_str().to_owned(),
        label: format!("{}/{}", row.r#box, row.name),
        verb: shown,
        first,
        kind,
        detail,
        open_alerts,
        missed,
    };
    match page.render() {
        Ok(html) => (status, Html(html)).into_response(),
        Err(e) => fault(e),
    }
}

/// admin's answer to a verb, as the page shows it, with the leg an
/// `unload` stopped at its save point missed.
async fn verb_shown(
    bound: Duration,
    verb: &str,
    enqueued: Enqueued,
) -> (StatusCode, String, String, Option<String>) {
    match tokio::time::timeout(bound, enqueued.answer()).await {
        Ok(Ok(outcome)) => (
            StatusCode::OK,
            if outcome.exit_code == Some(0) {
                "admin's answer"
            } else {
                "admin's refusal"
            }
            .to_owned(),
            outcome
                .answer
                .as_ref()
                .map(|a| serde_json::to_string_pretty(a).unwrap_or_default())
                .unwrap_or_default(),
            crate::link::listener::stopped_at_save_point(verb, &outcome).map(str::to_owned),
        ),
        Ok(Err(VerbError::Fault(fault))) => (
            StatusCode::OK,
            "admin-con's fault".to_owned(),
            format!("{}: {}", fault.kind, fault.message),
            None,
        ),
        Ok(Err(VerbError::Unanswered)) => (
            StatusCode::OK,
            "the connection ended with the verb in flight: its outcome is unknown here, and the trace and the next show hold it".to_owned(),
            String::new(),
            None,
        ),
        Ok(Err(e)) => (StatusCode::OK, "no answer".to_owned(), e.to_string(), None),
        Err(_) => (
            StatusCode::ACCEPTED,
            "asked, and not answered within the bound: its outcome is written when admin answers".to_owned(),
            String::new(),
            None,
        ),
    }
}

/// **The gate's close of a turn, as the page shows it**, escaped: an
/// answered close's text, marked truncated where `finish` is `length`; a
/// stopped or refused close's reason; gate-con's fault; or the connection
/// ended with the turn in flight.
async fn turn_shown(bound: Duration, enqueued: Enqueued) -> (StatusCode, String, String) {
    match tokio::time::timeout(bound, enqueued.close()).await {
        Ok(Ok(close)) => match close.kind.as_str() {
            "answered" => (
                StatusCode::OK,
                if close.finish.as_deref() == Some("length") {
                    "the agent's answer, truncated at the turn's token limit"
                } else {
                    "the agent's answer"
                }
                .to_owned(),
                close.text.unwrap_or_default(),
            ),
            kind @ ("stopped" | "refused") => (
                StatusCode::OK,
                format!("the agent {kind} the turn"),
                close.reason.unwrap_or_default(),
            ),
            other => (
                StatusCode::OK,
                format!("a close of kind {other}"),
                close.text.or(close.reason).unwrap_or_default(),
            ),
        },
        Ok(Err(TurnError::Gate(fault))) => (
            StatusCode::OK,
            "gate-con's fault".to_owned(),
            format!("{}: {}", fault.kind, fault.message),
        ),
        Ok(Err(TurnError::Unanswered)) => (
            StatusCode::OK,
            "the connection ended with the turn in flight: its outcome is unknown here, and the agent's trace holds it".to_owned(),
            String::new(),
        ),
        Ok(Err(e)) => (StatusCode::OK, "no answer".to_owned(), e.to_string()),
        Err(_) => (
            StatusCode::ACCEPTED,
            "placed, and not answered within the bound: its outcome is written when the gate answers".to_owned(),
            String::new(),
        ),
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
    let recheck =
        tokio::time::interval_at(tokio::time::Instant::now() + seams.recheck, seams.recheck);
    let (cut, gate) = tokio::sync::watch::channel(None);
    let mut view = View {
        tx,
        recheck,
        cut,
        store,
        policy: seams.policy,
        headers,
        id,
    };
    tokio::spawn(async move {
        if !view
            .deliver(SseEvent::default().event("window").data(LIVE_WINDOW))
            .await
        {
            return;
        }
        let mut cursor = 0u64;
        for event in held {
            cursor = event.seq;
            if !view.deliver(trace_event(&event)).await {
                return;
            }
        }
        loop {
            let next = tokio::select! {
                // **A viewer gone ends the view at once**: a quiet agent
                // sends nothing, so a send's failure would not notice a
                // closed tab, and the task and its subscription would live
                // until the session's or the grant's end.
                _ = view.tx.closed() => return,
                received = live.recv() => match received {
                    Ok(event) if event.seq > cursor => {
                        cursor = event.seq;
                        Some(trace_event(&event))
                    }
                    Ok(_) => None,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => Some(
                        SseEvent::default()
                            .event("mark")
                            .data(format!("the view fell behind the window: {n} events not shown")),
                    ),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
                _ = view.recheck.tick() => {
                    if !view.still_open().await {
                        return;
                    }
                    None
                }
            };
            if let Some(event) = next
                && !view.deliver(event).await
            {
                return;
            }
        }
    });
    Sse::new(gated(rx, gate).map(Ok::<SseEvent, Infallible>))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// **The response's body, gated by the view's cut**: before every item it
/// reads the cut, and once the cut is set it yields the closing line and
/// ends, the events still queued in the channel dropped unread, so nothing
/// reaches the reader after access ended, whenever it reads.
fn gated(
    rx: tokio::sync::mpsc::Receiver<SseEvent>,
    gate: tokio::sync::watch::Receiver<Option<&'static str>>,
) -> impl futures::Stream<Item = SseEvent> {
    futures::stream::unfold(
        (rx, gate, true, false),
        |(mut rx, mut gate, gate_live, done)| async move {
            if done {
                return None;
            }
            let closed = |why: &str| SseEvent::default().event("closed").data(why);
            let mut gate_live = gate_live;
            loop {
                let cut = *gate.borrow_and_update();
                if let Some(why) = cut {
                    return Some((closed(why), (rx, gate, gate_live, true)));
                }
                tokio::select! {
                    biased;
                    changed = gate.changed(), if gate_live => {
                        // The view gone without a cut: its sender dropped.
                        if changed.is_err() {
                            gate_live = false;
                        }
                    }
                    item = rx.recv() => {
                        let item = item?;
                        let cut = *gate.borrow();
                        if let Some(why) = cut {
                            return Some((closed(why), (rx, gate, gate_live, true)));
                        }
                        return Some((item, (rx, gate, gate_live, false)));
                    }
                }
            }
        },
    )
}

/// **One open live view's sending half.** **No await in it outlasts the
/// re-check's deadline**: a reader that does not read, or a trace busier
/// than the reader, fills the channel, and a send awaited plainly would
/// hold the task past its re-check, so a revoked grant, a disabled person
/// or a closed session would keep receiving. Every send instead races a
/// slot's reservation against the re-check's tick and the viewer's going:
/// nothing is committed to the channel until a slot is held while access
/// was last found standing, and where the tick wins and access has ended,
/// the pending event is dropped and the view closes.
struct View {
    tx: tokio::sync::mpsc::Sender<SseEvent>,
    recheck: tokio::time::Interval,
    /// **The cut**: set, with its reason, when the re-check finds access
    /// ended; the response's body reads it before every item (`gated`).
    cut: tokio::sync::watch::Sender<Option<&'static str>>,
    store: Store,
    policy: Policy,
    headers: HeaderMap,
    id: AgentId,
}

impl View {
    /// **One event sent, or the view closed**: `false` where the viewer
    /// went or access ended while the event waited for a slot, the event
    /// then never sent.
    async fn deliver(&mut self, event: SseEvent) -> bool {
        let mut event = Some(event);
        loop {
            let ticked = tokio::select! {
                _ = self.tx.closed() => return false,
                permit = self.tx.reserve() => match permit {
                    Ok(permit) => {
                        if let Some(event) = event.take() {
                            permit.send(event);
                        }
                        return true;
                    }
                    Err(_) => return false,
                },
                _ = self.recheck.tick() => true,
            };
            if ticked && !self.still_open().await {
                return false;
            }
        }
    }

    /// **The re-check**: `true` where the session and the grant stand;
    /// otherwise the view cut, with its reason, and `false`.
    async fn still_open(&mut self) -> bool {
        let Some(why) = ended(&self.store, &self.policy, &self.headers, &self.id).await else {
            return true;
        };
        // The cut, which the body reads before every item: the events
        // already queued are dropped unread, and the closing line goes out
        // ahead of nothing else.
        self.cut.send_replace(Some(why));
        false
    }
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
