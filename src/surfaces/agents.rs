//! conforms: web-a-client-config-is-handed-over-once-and-kept-nowhere
//!
//! **The register of agents through the server** (Spec 2.12, 2.13 and 8;
//! act 11, PR 5c): the page that lists the register's rows, and
//! registering an agent and rotating its credentials, the two verbs that
//! mint credentials, each by an admin under the operator's ruling of
//! 2026-10-09 that everything done with agents belongs in the web page. A
//! plain shell. The host's `register` and `rotate` stand beside them.
//!
//! **Each write takes the order `surfaces::admin` states**: the box and
//! name, or the `ag-`, parsed; the first gate with its one refusal record;
//! the agent resolved for the admin by the store; then the audited write,
//! which takes the authority lock as the host's verbs do and holds the
//! identity exclusion shared from the admin's re-check to the store's
//! commit (`Store::register_agent_by_admin_on`). **The store keeps
//! fingerprints only.**
//!
//! **The hand-over**: the two client configs a write mints are held in this
//! process's memory alone, each under a 32-byte handle from the operating
//! system's random source, taken once, within five minutes, by the session
//! that minted them, under `Cache-Control: no-store`, and dropped. They are
//! written to no store, file, log line or audit record, and a restart drops
//! any not taken, after which the admin rotates.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use askama::Template;
use axum::Router;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use tokio::time::Instant;

use crate::config::ServerConfig;
use crate::link::authority::{Authority, ClientCredential};
use crate::link::frames::Plane;
use crate::link::register::{AdminWrite, AuthorityGone, CredentialState, PlaneRevoked, Retired};
use crate::link::verbs::{
    authority_still_stands, client_config, mint_pair, name_agrees, well_formed,
};
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::identity::{bearer, hex, is_agent_id};
use crate::store::{AgentId, Store};
use crate::surfaces::admin::{NOT_AN_ADMIN, admin_session, audited, fault, refused, writer};
use crate::surfaces::gate::{Policy, Session};

/// How long a client config waits to be taken.
pub const HANDOVER_LIFETIME: Duration = Duration::from_secs(5 * 60);

/// **How many client configs may wait at once**, held or reserved: a
/// write reserves room for its two at step three, under the hand-over's
/// lock, so the bound counts the writes in flight too and is exact.
pub const HANDOVERS_HELD: usize = 64;

struct Held {
    session_id: i64,
    filename: String,
    content: String,
    until: Instant,
}

#[derive(Default)]
struct Table {
    held: HashMap<String, Held>,
    /// Slots reserved by writes in flight, not yet held.
    reserved: usize,
}

impl Table {
    fn prune(&mut self) {
        let now = Instant::now();
        self.held.retain(|_, h| h.until > now);
    }
}

/// **The client configs waiting to be taken**, in this process's memory
/// alone.
#[derive(Clone)]
pub struct Handover {
    table: Arc<Mutex<Table>>,
    cap: usize,
}

impl Default for Handover {
    fn default() -> Self {
        Self::with_cap(HANDOVERS_HELD)
    }
}

impl Handover {
    /// A hand-over bounded at `cap`, which tests set small.
    pub fn with_cap(cap: usize) -> Self {
        Self {
            table: Arc::default(),
            cap,
        }
    }

    /// **Room for `n` configs reserved**, atomically under the table's
    /// lock, or none where the held and the reserved would pass the bound.
    /// The reservation's slots become configs as it holds them, and any
    /// left are released when it drops, on every path a write takes.
    pub fn reserve(&self, n: usize) -> Option<Reservation> {
        let mut table = self.table.lock().unwrap();
        table.prune();
        if table.held.len() + table.reserved + n > self.cap {
            return None;
        }
        table.reserved += n;
        Some(Reservation {
            handover: self.clone(),
            slots: n,
        })
    }

    /// **The config under `handle`, taken once by the session that holds
    /// it**: removed as it is answered. Unknown, taken, expired or another
    /// session's answers nothing, and another session's ask leaves it held.
    pub(crate) fn take(&self, handle: &str, session_id: i64) -> Option<(String, String)> {
        let mut table = self.table.lock().unwrap();
        table.prune();
        match table.held.get(handle) {
            Some(h) if h.session_id == session_id => {
                table.held.remove(handle).map(|h| (h.filename, h.content))
            }
            _ => None,
        }
    }

    /// How many configs wait, expired ones dropped.
    pub fn held(&self) -> usize {
        let mut table = self.table.lock().unwrap();
        table.prune();
        table.held.len()
    }

    /// How many configs wait or are reserved, the count the bound holds.
    pub fn occupied(&self) -> usize {
        let mut table = self.table.lock().unwrap();
        table.prune();
        table.held.len() + table.reserved
    }
}

/// **Room reserved in the hand-over** for one write's configs.
pub struct Reservation {
    handover: Handover,
    slots: usize,
}

impl Reservation {
    /// A config held for the session in one of the reserved slots,
    /// answering its handle.
    pub(crate) fn hold(&mut self, session_id: i64, filename: String, content: String) -> String {
        assert!(
            self.slots > 0,
            "a reservation holds no more than it reserved"
        );
        let handle = hex(&bearer());
        let mut table = self.handover.table.lock().unwrap();
        table.prune();
        table.reserved -= 1;
        self.slots -= 1;
        table.held.insert(
            handle.clone(),
            Held {
                session_id,
                filename,
                content,
                until: Instant::now() + HANDOVER_LIFETIME,
            },
        );
        handle
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut table = self.handover.table.lock().unwrap();
        table.reserved -= self.slots;
    }
}

/// **What the agents surface holds beside the store**: the server's config
/// (its link address and name, for the client configs), the authority it
/// loaded at start, and the hand-over.
#[derive(Clone)]
pub struct Seams {
    pub policy: Policy,
    pub cfg: Arc<ServerConfig>,
    pub authority: Arc<Authority>,
    pub handover: Handover,
}

/// The agents surface, taking its seams as its own argument.
pub fn routes(seams: Seams) -> Router<Store> {
    macro_rules! seamed {
        ($handler:ident, $ask:ty) => {{
            let seams = seams.clone();
            move |State(store): State<Store>, headers: HeaderMap, Form(ask): Form<$ask>| {
                let seams = seams.clone();
                async move { $handler(store, &seams, headers, ask).await }
            }
        }};
    }
    let page_seams = seams.clone();
    Router::new()
        .route(
            "/admin/agents",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let seams = page_seams.clone();
                async move { page(store, &seams, headers).await }
            }),
        )
        .route(
            "/admin/agents/register",
            post(seamed!(register, RegisterAsk)),
        )
        .route("/admin/agents/rotate", post(seamed!(rotate, RotateAsk)))
        .route("/admin/agents/config", post(seamed!(take, TakeAsk)))
        .route("/admin/agents/revoke", post(seamed!(revoke, RevokeAsk)))
        .route("/admin/agents/retire", post(seamed!(retire, RotateAsk)))
}

struct AgentRow {
    agent_id: String,
    label: String,
    gate: &'static str,
    admin: &'static str,
    gate_live: bool,
    admin_live: bool,
    present: bool,
    registered: String,
    live: bool,
}

fn state(state: &CredentialState) -> &'static str {
    match state {
        CredentialState::Live => "live",
        CredentialState::Revoked => "revoked",
    }
}

#[derive(Template)]
#[template(path = "admin_agents.html")]
struct AgentsPage {
    here: &'static str,
    who: String,
    admin: bool,
    rows: Vec<AgentRow>,
}

/// **The register as the admin governs it** (Spec 2.13): each agent's box
/// and name, its identity, each plane's credential state, and presence;
/// not its door, load state or trace, which read access by grant shows.
async fn page(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, &seams.policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let agents = match store.agents().await {
        Ok(agents) => agents,
        Err(e) => return fault(e),
    };
    let rows = agents
        .iter()
        .map(|a| AgentRow {
            agent_id: a.agent_id.as_str().to_owned(),
            label: format!("{}/{}", a.r#box, a.name),
            gate: state(&a.gate.state),
            admin: state(&a.admin.state),
            gate_live: a.gate.state == CredentialState::Live,
            admin_live: a.admin.state == CredentialState::Live,
            present: a.present(),
            registered: a.registered_at.format("%Y-%m-%d %H:%M UTC").to_string(),
            live: matches!(a.gate.state, CredentialState::Live)
                || matches!(a.admin.state, CredentialState::Live),
        })
        .collect();
    match (AgentsPage {
        here: "agents",
        who: session.name,
        admin: true,
        rows,
    })
    .render()
    {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

/// One minted pair, its row's identity and name.
struct Minted {
    agent_id: String,
    r#box: String,
    name: String,
    gate: ClientCredential,
    admin: ClientCredential,
}

/// One config waiting, as the hand-over page offers it.
struct Waiting {
    plane: &'static str,
    filename: String,
    handle: String,
}

#[derive(Template)]
#[template(path = "admin_handover.html")]
struct HandoverPage {
    here: &'static str,
    who: String,
    admin: bool,
    verb: &'static str,
    agent: String,
    waiting: Vec<Waiting>,
    minutes: u64,
}

/// **The two client configs held for the session, and the page that
/// offers them**, under `no-store`. The page carries the handles, never the
/// configs.
fn hand_over(
    seams: &Seams,
    mut reservation: Reservation,
    session: Session,
    verb: &'static str,
    minted: Minted,
) -> Response {
    let mut waiting = Vec::new();
    for (plane, credential) in [(Plane::Gate, &minted.gate), (Plane::Admin, &minted.admin)] {
        let content = client_config(
            &seams.cfg,
            &seams.authority,
            &minted.agent_id,
            &minted.name,
            plane,
            credential,
        );
        let filename = format!("{}-{}-{}.toml", minted.r#box, minted.name, plane.as_str());
        let handle = reservation.hold(session.session_id, filename.clone(), content);
        waiting.push(Waiting {
            plane: plane.as_str(),
            filename,
            handle,
        });
    }
    match (HandoverPage {
        here: "agents",
        who: session.name,
        admin: true,
        verb,
        agent: format!("{}/{} ({})", minted.r#box, minted.name, minted.agent_id),
        waiting,
        minutes: HANDOVER_LIFETIME.as_secs() / 60,
    })
    .render()
    {
        Ok(html) => ([(header::CACHE_CONTROL, "no-store")], Html(html)).into_response(),
        Err(e) => fault(e),
    }
}

/// The authority as the server loaded it, checked against its config and
/// the disk inside the authority lock, as the host's verbs check it.
fn authority_stands(seams: &Seams) -> Result<(), Refusal> {
    name_agrees(&seams.cfg, &seams.authority)
        .and_then(|_| authority_still_stands(&seams.cfg, &seams.authority))
        .map_err(|e| {
            Refusal::Authority(format!(
                "{e:#}; the server mints under the authority it loaded at start, so restart it after a rotation"
            ))
        })
}

#[derive(Deserialize)]
pub struct RegisterAsk {
    r#box: String,
    name: String,
}

/// **An agent registered**: a row and two credentials, a live row for the
/// same box and name retired in the same transaction as the host's
/// `register` retires it, and the two client configs handed over.
async fn register(store: Store, seams: &Seams, headers: HeaderMap, ask: RegisterAsk) -> Response {
    const ACTION: &str = "register";
    if let Err(e) = well_formed(&ask.r#box, &ask.name) {
        return (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response();
    }
    let minted: AgentId = match store
        .mint_key("ag")
        .await
        .and_then(|id| id.parse().map_err(anyhow::Error::msg))
    {
        Ok(id) => id,
        Err(e) => return fault(e),
    };
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Agent(minted.as_str()),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let Some(reservation) = seams.handover.reserve(2) else {
        return refused(&Refusal::HandoverFull);
    };
    let write = async {
        let mut lock = store.authority_lock().await?;
        if let Err(refusal) = authority_stands(seams) {
            return Ok(Err(refusal));
        }
        // **Minted inside the register's transaction, once the admin's
        // re-check has passed.**
        let mut pair = None;
        let landed = Store::register_agent_by_admin_on(
            lock.connection(),
            AdminWrite {
                session_id: session.session_id,
                person: &session.person_id,
            },
            &minted,
            &ask.r#box,
            &ask.name,
            &seams.authority.fingerprint(),
            || {
                let (gate, admin) = mint_pair(&ask.name, &seams.authority)?;
                let fingerprints = (gate.fingerprint.clone(), admin.fingerprint.clone());
                pair = Some((gate, admin));
                Ok(fingerprints)
            },
        )
        .await;
        match (landed, pair) {
            (Ok((id, _)), Some((gate, admin))) => Ok(Ok((id, gate, admin))),
            (Ok(_), None) => anyhow::bail!("a register landed with no credentials minted"),
            (Err(e), _) if e.is::<AuthorityGone>() => Ok(Err(Refusal::AuthorityGone)),
            (Err(e), None) => Err(e),
            // **A commit's outcome is unknown until it is read back**, as in
            // the host's `register`: the row carrying the new fingerprint
            // means the write landed, and the configs are handed over.
            (Err(e), Some((gate, admin))) => {
                match store.agent_by_fingerprint(&gate.fingerprint).await? {
                    Some((row, _)) => Ok(Ok((row.agent_id, gate, admin))),
                    None => Err(e),
                }
            }
        }
    };
    let written = match audited(
        &store,
        &session,
        Target::Agent(minted.as_str()),
        ACTION,
        write,
    )
    .await
    {
        Ok(written) => written,
        Err(answer) => return answer,
    };
    match written {
        Ok(Ok((id, gate, admin))) => hand_over(
            seams,
            reservation,
            session,
            "registered",
            Minted {
                agent_id: id.as_str().to_owned(),
                r#box: ask.r#box,
                name: ask.name,
                gate,
                admin,
            },
        ),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct RotateAsk {
    agent: String,
}

/// **An agent's credentials rotated**: a fresh pair, the old pair revoked
/// and its live connections closed in this act, and the two client configs
/// handed over.
async fn rotate(store: Store, seams: &Seams, headers: HeaderMap, ask: RotateAsk) -> Response {
    const ACTION: &str = "rotate";
    if !is_agent_id(&ask.agent) {
        return (
            StatusCode::BAD_REQUEST,
            "the agent asked for is not of its identity's shape",
        )
            .into_response();
    }
    let session = match writer(
        &store,
        &seams.policy,
        &headers,
        Target::Agent(&ask.agent),
        ACTION,
    )
    .await
    {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let id: AgentId = match ask.agent.parse() {
        Ok(id) => id,
        Err(e) => return fault(anyhow::Error::msg(e)),
    };
    let agent = match store.agent(&id).await {
        Ok(Some(agent))
            if agent.credential(Plane::Gate).state == CredentialState::Live
                || agent.credential(Plane::Admin).state == CredentialState::Live =>
        {
            agent
        }
        Ok(Some(_)) => return refused(&Refusal::Retired),
        Ok(None) => return refused(&Refusal::NoSuchAgent),
        Err(e) => return fault(e),
    };
    let Some(reservation) = seams.handover.reserve(2) else {
        return refused(&Refusal::HandoverFull);
    };
    let write = async {
        let mut lock = store.authority_lock().await?;
        if let Err(refusal) = authority_stands(seams) {
            return Ok(Err(refusal));
        }
        // **The row step three resolved is re-checked inside the
        // rotation's transaction**, under its lock, and the credentials are
        // minted only after: a row retired meanwhile is refused, never made
        // live again.
        let mut pair = None;
        let landed = Store::rotate_credentials_by_admin_on(
            lock.connection(),
            AdminWrite {
                session_id: session.session_id,
                person: &session.person_id,
            },
            &agent,
            &seams.authority.fingerprint(),
            || {
                let (gate, admin) = mint_pair(&agent.name, &seams.authority)?;
                let fingerprints = (gate.fingerprint.clone(), admin.fingerprint.clone());
                pair = Some((gate, admin));
                Ok(fingerprints)
            },
        )
        .await;
        match (landed, pair) {
            (Ok(_), Some((gate, admin))) => Ok(Ok((gate, admin))),
            (Ok(_), None) => anyhow::bail!("a rotation landed with no credentials minted"),
            (Err(e), _) if e.is::<AuthorityGone>() => Ok(Err(Refusal::AuthorityGone)),
            (Err(e), _) if e.is::<Retired>() => Ok(Err(Refusal::Retired)),
            (Err(e), None) => Err(e),
            (Err(e), Some((gate, admin))) => {
                match store.agent_by_fingerprint(&gate.fingerprint).await? {
                    Some(_) => Ok(Ok((gate, admin))),
                    None => Err(e),
                }
            }
        }
    };
    let written = match audited(&store, &session, Target::Agent(&ask.agent), ACTION, write).await {
        Ok(written) => written,
        Err(answer) => return answer,
    };
    match written {
        Ok(Ok((gate, admin))) => hand_over(
            seams,
            reservation,
            session,
            "rotated",
            Minted {
                agent_id: agent.agent_id.as_str().to_owned(),
                r#box: agent.r#box,
                name: agent.name,
                gate,
                admin,
            },
        ),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct TakeAsk {
    handle: String,
}

/// **A client config taken**: once, by the session that minted it, within
/// its five minutes, as a download under `no-store`. Not a write of the
/// store, and so no audit record; nothing of it is logged.
async fn take(store: Store, seams: &Seams, headers: HeaderMap, ask: TakeAsk) -> Response {
    let session = match admin_session(&store, &seams.policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let Some((filename, content)) = seams.handover.take(&ask.handle, session.session_id) else {
        return (
            StatusCode::NOT_FOUND,
            "no such client config for this session: taken already, past its five minutes, or another session's; rotate the agent for a fresh pair",
        )
            .into_response();
    };
    let disposition = match HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        Ok(value) => value,
        Err(e) => return fault(e),
    };
    (
        [
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            ),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        content,
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct RevokeAsk {
    agent: String,
    plane: String,
}

/// **Steps one to three of a revocation or a retirement**: the `ag-`
/// parsed, the first gate, and the agent resolved for the admin by the
/// store, its plane live for a revocation and some plane live for a
/// retirement, each refusal before any record.
async fn resolve_revocation(
    store: &Store,
    seams: &Seams,
    headers: &HeaderMap,
    agent: &str,
    plane: Option<Plane>,
    action: &str,
) -> Result<(Session, AgentId), Response> {
    if !is_agent_id(agent) {
        return Err((
            StatusCode::BAD_REQUEST,
            "the agent asked for is not of its identity's shape",
        )
            .into_response());
    }
    let session = writer(store, &seams.policy, headers, Target::Agent(agent), action).await?;
    let id: AgentId = agent.parse().map_err(|e| fault(anyhow::Error::msg(e)))?;
    let row = match store.agent(&id).await {
        Ok(Some(row)) => row,
        Ok(None) => return Err(refused(&Refusal::NoSuchAgent)),
        Err(e) => return Err(fault(e)),
    };
    let live = |p: Plane| row.credential(p).state == CredentialState::Live;
    match plane {
        Some(p) if !live(p) => return Err(refused(&Refusal::PlaneRevoked)),
        None if !live(Plane::Gate) && !live(Plane::Admin) => {
            return Err(refused(&Refusal::Retired));
        }
        _ => {}
    }
    Ok((session, id))
}

/// **Step four of a revocation or a retirement**: the audited write, the
/// row re-read inside its transaction under the shared identity hold.
async fn revoked(
    store: &Store,
    session: &Session,
    id: &AgentId,
    plane: Option<Plane>,
    action: &str,
) -> Response {
    let write = async {
        match store
            .revoke_by_admin(
                AdminWrite {
                    session_id: session.session_id,
                    person: &session.person_id,
                },
                id,
                plane,
            )
            .await
        {
            Ok(revoked) => Ok(Ok(revoked)),
            Err(e) if e.is::<AuthorityGone>() => Ok(Err(Refusal::AuthorityGone)),
            Err(e) if e.is::<PlaneRevoked>() => Ok(Err(Refusal::PlaneRevoked)),
            Err(e) if e.is::<Retired>() => Ok(Err(Refusal::Retired)),
            Err(e) => Err(e),
        }
    };
    match audited(store, session, Target::Agent(id.as_str()), action, write).await {
        Ok(written) => crate::surfaces::admin::landed(written, "/admin/agents"),
        Err(answer) => answer,
    }
}

/// **One plane's credential revoked**: its live connection closed in this
/// act, the other plane untouched.
async fn revoke(store: Store, seams: &Seams, headers: HeaderMap, ask: RevokeAsk) -> Response {
    const ACTION: &str = "revoke";
    let Ok(plane) = ask.plane.parse::<Plane>() else {
        return (StatusCode::BAD_REQUEST, "the plane is gate or admin").into_response();
    };
    match resolve_revocation(&store, seams, &headers, &ask.agent, Some(plane), ACTION).await {
        Ok((session, id)) => revoked(&store, &session, &id, Some(plane), ACTION).await,
        Err(answer) => answer,
    }
}

/// **An agent retired**: every live plane's credential revoked in one
/// write, its connections closed in this act. A retired row is never live
/// again; the agent is registered afresh, as a new row.
async fn retire(store: Store, seams: &Seams, headers: HeaderMap, ask: RotateAsk) -> Response {
    const ACTION: &str = "retire";
    match resolve_revocation(&store, seams, &headers, &ask.agent, None, ACTION).await {
        Ok((session, id)) => revoked(&store, &session, &id, None, ACTION).await,
        Err(answer) => answer,
    }
}
