//! conforms: web-an-admins-write-to-a-role-or-grant-keeps-the-self-change-rules
//!
//! **An admin's writes to roles and grants** (Spec 2.13; act 11, PR 5b):
//! the page that lists every live grant, every role and the register's
//! agents, and granting a role, revoking a grant and setting a per-agent
//! role's verbs, each under a session whose person holds a live admin
//! grant. A plain shell, per the operator's ruling of 2026-10-09.
//!
//! **Each write is 5a's shape** (`surfaces::admin`): every name or
//! identity it refers to resolved before any record (identities parsed at
//! the boundary, a role named by the store with its scope checked), a
//! session without the grant refused with one record carrying it, the
//! admin as principal by `session`, its first record before and its outcome
//! after, and one identity transaction
//! that re-checks the authority first (`store::grants`). **The self-change
//! rules are refused before any record** here and checked again by the
//! store under the exclusion: no grant whose grantee is the admin, granting
//! or revoking, and no edit of a role the admin holds on any agent. **Where
//! more than one person holds `turn` on an agent, the page says so** beside
//! it, their turns interleaving in the agent's one conversation.

use askama::Template;
use axum::Router;
use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;

use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::grants::{ListedGrant, in_scope, verbs_within_vocabulary};
use crate::store::identity::{VOCABULARY, is_agent_id, is_grant_id, is_person_id};
use crate::surfaces::admin::{
    NOT_AN_ADMIN, admin_session, audited, fault, landed, refused, writer,
};
use crate::surfaces::gate::Policy;

/// Where a landed write answers.
const PAGE: &str = "/admin/grants";

/// The answer to a malformed identity, naming the field and its shape.
fn malformed(what: &'static str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        format!("the {what} asked for is not of its identity's shape"),
    )
        .into_response()
}

/// The roles-and-grants surface, taking the session's policy as its own
/// argument.
pub fn routes(policy: Policy) -> Router<Store> {
    macro_rules! seamed {
        ($handler:ident, $ask:ty) => {{
            let policy = policy.clone();
            move |State(store): State<Store>, headers: HeaderMap, Form(ask): Form<$ask>| {
                let policy = policy.clone();
                async move { $handler(store, &policy, headers, ask).await }
            }
        }};
    }
    let (page_policy, role_policy) = (policy.clone(), policy.clone());
    Router::new()
        .route(
            PAGE,
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let policy = page_policy.clone();
                async move { page(store, &policy, headers).await }
            }),
        )
        .route("/admin/grants/grant", post(seamed!(grant, GrantAsk)))
        .route("/admin/grants/revoke", post(seamed!(revoke, RevokeAsk)))
        .route(
            "/admin/roles/set",
            post(
                move |State(store): State<Store>, headers: HeaderMap, body: String| {
                    let policy = role_policy.clone();
                    async move { set_role(store, &policy, headers, body).await }
                },
            ),
        )
}

struct GrantRow {
    listed: ListedGrant,
    own: bool,
}

struct RoleRow {
    name: String,
    verbs: String,
    version: i64,
    /// Why the admin may not edit it, or none where they may.
    fixed: Option<&'static str>,
    checks: Vec<(&'static str, bool)>,
}

struct AgentRow {
    agent_id: String,
    label: String,
    shared_turn: bool,
}

#[derive(Template)]
#[template(path = "admin_grants.html")]
struct GrantsPage {
    here: &'static str,
    who: String,
    admin: bool,
    grants: Vec<GrantRow>,
    roles: Vec<RoleRow>,
    agents: Vec<AgentRow>,
    persons: Vec<(String, String)>,
}

/// **Every live grant, every role and the register's agents**, for an
/// admin; the shared conversation named beside each agent where more than
/// one person holds `turn` on it.
async fn page(store: Store, policy: &Policy, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let read = async {
        let grants = store.listed_grants().await?;
        let roles = store.roles().await?;
        let agents = store.agents().await?;
        let shared = store.shared_turn().await?;
        let persons = store.listed_persons().await?;
        let mut held = Vec::new();
        for role in &roles {
            if store.holds_role(&session.person_id, &role.name).await? {
                held.push(role.name.clone());
            }
        }
        anyhow::Ok((grants, roles, agents, shared, persons, held))
    };
    let (grants, roles, agents, shared, persons, held) = match read.await {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    let page = GrantsPage {
        here: "grants",
        who: session.name.clone(),
        admin: true,
        grants: grants
            .into_iter()
            .map(|listed| GrantRow {
                own: listed.person_id == session.person_id,
                listed,
            })
            .collect(),
        roles: roles
            .into_iter()
            .map(|role| RoleRow {
                fixed: if role.name == "admin" {
                    Some("fixed by the store; it carries no agent verb")
                } else if held.contains(&role.name) {
                    Some("you hold this role, so another admin or the host's role set edits it")
                } else {
                    None
                },
                checks: VOCABULARY
                    .iter()
                    .map(|verb| (*verb, role.verbs.iter().any(|v| v == verb)))
                    .collect(),
                verbs: role.verbs.join(" "),
                version: role.version,
                name: role.name,
            })
            .collect(),
        agents: agents
            .into_iter()
            .map(|agent| {
                let agent_id = agent.agent_id.as_str().to_owned();
                AgentRow {
                    shared_turn: shared.contains(&agent_id),
                    label: format!("{}/{}", agent.r#box, agent.name),
                    agent_id,
                }
            })
            .collect(),
        persons: persons
            .into_iter()
            .filter(|p| p.person_id != session.person_id)
            .map(|p| (p.person_id, p.name))
            .collect(),
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct GrantAsk {
    person: String,
    role: String,
    /// An agent's identity, or empty for a server-wide grant.
    #[serde(default)]
    agent: String,
}

/// **A role granted**: never to the admin themselves.
async fn grant(store: Store, policy: &Policy, headers: HeaderMap, ask: GrantAsk) -> Response {
    const ACTION: &str = "grant add";
    if !is_person_id(&ask.person) {
        return malformed("person");
    }
    let agent = match ask.agent.as_str() {
        "" => None,
        agent if is_agent_id(agent) => Some(agent),
        _ => return malformed("agent"),
    };
    // **The role is named by the store before any record**, and its scope
    // checked against the agent's presence, as every name or identity a
    // request refers to is resolved before the records begin.
    match store.role(&ask.role).await {
        Ok(Some(role)) => {
            if let Err(refusal) = in_scope(&role.name, &role.scope, agent) {
                return refused(&refusal);
            }
        }
        Ok(None) => return refused(&Refusal::NoSuchRole),
        Err(e) => return fault(e),
    }
    let grant_id = match store.mint_key("gr").await {
        Ok(id) => id,
        Err(e) => return fault(e),
    };
    let session = match writer(&store, policy, &headers, Target::Grant(&grant_id), ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    if ask.person == session.person_id {
        return refused(&Refusal::OwnGrant);
    }
    match audited(
        &store,
        &session,
        Target::Grant(&grant_id),
        ACTION,
        store.admin_grant(
            &session.person_id,
            session.session_id,
            &grant_id,
            &ask.person,
            &ask.role,
            agent,
        ),
    )
    .await
    {
        Ok(written) => landed(written, PAGE),
        Err(answer) => answer,
    }
}

#[derive(Deserialize)]
pub struct RevokeAsk {
    grant: String,
    version: i64,
}

/// **A grant revoked**: never one of the admin's own, never the last
/// enabled admin's.
async fn revoke(store: Store, policy: &Policy, headers: HeaderMap, ask: RevokeAsk) -> Response {
    const ACTION: &str = "grant remove";
    if !is_grant_id(&ask.grant) {
        return malformed("grant");
    }
    let session = match writer(&store, policy, &headers, Target::Grant(&ask.grant), ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.grant_of(&ask.grant).await {
        Ok(Some(read)) if read.person_id == session.person_id => {
            return refused(&Refusal::OwnGrant);
        }
        Ok(_) => {}
        Err(e) => return fault(e),
    }
    match audited(
        &store,
        &session,
        Target::Grant(&ask.grant),
        ACTION,
        store.admin_revoke(
            &session.person_id,
            session.session_id,
            &ask.grant,
            ask.version,
        ),
    )
    .await
    {
        Ok(written) => landed(written, PAGE),
        Err(answer) => answer,
    }
}

/// **A role's verbs set**, from a form whose `verbs` field repeats, one
/// per verb checked, which axum's form extractor does not gather: the body
/// is read as the URL Standard's form encoding by the `url` crate.
async fn set_role(store: Store, policy: &Policy, headers: HeaderMap, body: String) -> Response {
    const ACTION: &str = "role set";
    let (mut role, mut version, mut verbs) = (None, None, Vec::new());
    for (key, value) in url::form_urlencoded::parse(body.as_bytes()) {
        match key.as_ref() {
            "role" => role = Some(value.into_owned()),
            "version" => version = value.parse::<i64>().ok(),
            "verbs" => verbs.push(value.into_owned()),
            _ => {}
        }
    }
    let (Some(role), Some(version)) = (role, version) else {
        return (
            StatusCode::BAD_REQUEST,
            "a role's edit names the role and the version read",
        )
            .into_response();
    };
    // **The role is named by the store before any record**, so a request's
    // text never becomes an audit target: an unknown name, `admin`, or a
    // verb outside the vocabulary is refused here.
    match store.role(&role).await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(&Refusal::NoSuchRole),
        Err(e) => return fault(e),
    }
    if role == "admin" {
        return refused(&Refusal::Fixed);
    }
    if let Err(refusal) = verbs_within_vocabulary(&verbs) {
        return refused(&refusal);
    }
    let session = match writer(&store, policy, &headers, Target::Role(&role), ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.holds_role(&session.person_id, &role).await {
        Ok(true) => return refused(&Refusal::HoldsRole),
        Ok(false) => {}
        Err(e) => return fault(e),
    }
    match audited(
        &store,
        &session,
        Target::Role(&role),
        ACTION,
        store.admin_set_role(
            &session.person_id,
            session.session_id,
            &role,
            &verbs,
            version,
        ),
    )
    .await
    {
        Ok(written) => landed(written, PAGE),
        Err(answer) => answer,
    }
}
