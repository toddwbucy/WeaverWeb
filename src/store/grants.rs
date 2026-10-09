//! **An admin's writes to roles and grants** (Spec 2.13; act 11, PR 5b):
//! granting a role, revoking a grant, and setting a per-agent role's verbs,
//! each through a session whose person holds a live admin grant.
//!
//! **Each write re-checks its authority inside its own transaction**, under
//! the identity exclusion, before anything else (`admin::authorized`), and
//! **the self-change rules are checked under the same exclusion**: no person
//! grants or revokes a grant whose grantee is themselves, and no person sets
//! the verbs of a role they hold a grant of on any agent, so a grant of the
//! role landing between the surface's read and the edit is seen. The
//! surface checks the same rules before any record; these are the store's,
//! beneath it. The last-admin rule binds a revocation, and a revocation and
//! a role's edit carry the version the page read (Spec 3.2).

use sqlx::Row;

use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;
use crate::store::identity::{self, Role, VOCABULARY};

/// **A live grant as the admin page lists it**: the person by identity and
/// name, the role, and the agent by identity and register name, or none
/// for a server-wide grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedGrant {
    pub grant_id: String,
    pub person_id: String,
    pub name: String,
    pub role: String,
    pub agent_id: Option<String>,
    /// The agent as `box/name`.
    pub agent: Option<String>,
    /// Whether the agent is retired, holding no live credential: its grant
    /// stands, the row never being live again, for an admin to revoke.
    pub agent_retired: bool,
    pub version: i64,
}

/// **A grant as a revocation reads it**: its grantee, its role, its
/// version, and whether it is live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub person_id: String,
    pub role: String,
    pub version: i64,
    pub live: bool,
}

/// **A role's verbs as asked**, deduplicated in order, every one within the
/// vocabulary of Spec 7.2 plus `turn`, or the first verb outside it.
pub fn verbs_within_vocabulary(verbs: &[String]) -> Result<Vec<String>, Refusal> {
    let mut set: Vec<String> = Vec::new();
    for verb in verbs {
        if !VOCABULARY.contains(&verb.as_str()) {
            return Err(Refusal::Vocabulary(verb.clone()));
        }
        if !set.contains(verb) {
            set.push(verb.clone());
        }
    }
    Ok(set)
}

/// **A role and an agent that agree**: `admin` server-wide and naming no
/// agent, every other role on one agent. The surface checks it before any
/// record and the store again in its transaction.
pub fn in_scope(role: &str, scope: &str, agent: Option<&str>) -> Result<(), Refusal> {
    match (scope, agent) {
        ("server", Some(_)) => Err(Refusal::Scope(format!(
            "{role} is server-wide and names no agent"
        ))),
        ("agent", None) => Err(Refusal::Scope(format!(
            "{role} is a role on an agent; name the agent"
        ))),
        _ => Ok(()),
    }
}

impl Store {
    /// **Every live grant**, by person name, role and agent.
    pub async fn listed_grants(&self) -> anyhow::Result<Vec<ListedGrant>> {
        let rows = sqlx::query(
            "SELECT g.grant_id, g.person_id, p.name, g.role, g.agent_id, \
             a.box || '/' || a.name AS agent, g.version, \
             (a.agent_id IS NOT NULL AND a.gate_state <> 'live' AND a.admin_state <> 'live') \
               AS agent_retired \
             FROM role_grant g JOIN person p ON p.person_id = g.person_id \
             LEFT JOIN agent a ON a.agent_id = g.agent_id \
             WHERE g.revoked_at IS NULL ORDER BY p.name_key, g.role, agent",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| ListedGrant {
                grant_id: r.get("grant_id"),
                person_id: r.get("person_id"),
                name: r.get("name"),
                role: r.get("role"),
                agent_id: r.get("agent_id"),
                agent: r.get("agent"),
                agent_retired: r.get("agent_retired"),
                version: r.get("version"),
            })
            .collect())
    }

    /// **Every role**, with its verbs and version.
    pub async fn roles(&self) -> anyhow::Result<Vec<Role>> {
        let rows = sqlx::query("SELECT name, scope, verbs, version FROM role ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .iter()
            .map(|r| Role {
                name: r.get("name"),
                scope: r.get("scope"),
                verbs: r.get("verbs"),
                version: r.get("version"),
            })
            .collect())
    }

    /// **The agents on which more than one enabled person holds a live
    /// grant of a role carrying `turn`** (Spec 2.13): their turns interleave
    /// in the agent's one conversation, which the page says beside each.
    pub async fn shared_turn(&self) -> anyhow::Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT g.agent_id FROM role_grant g \
             JOIN role r ON r.name = g.role JOIN person p ON p.person_id = g.person_id \
             WHERE g.revoked_at IS NULL AND g.agent_id IS NOT NULL AND p.enabled \
             AND 'turn' = ANY (r.verbs) \
             GROUP BY g.agent_id HAVING count(DISTINCT g.person_id) > 1",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Whether a person holds a live grant of a role on any agent.
    pub async fn holds_role(&self, person: &str, role: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(HOLDS_ROLE)
            .bind(person)
            .bind(role)
            .fetch_one(&self.pool)
            .await?)
    }

    /// A grant by identity, live or revoked.
    pub async fn grant_of(&self, grant: &str) -> anyhow::Result<Option<Grant>> {
        let mut conn = self.pool.acquire().await?;
        read_grant(&mut conn, grant).await
    }

    /// **A role granted by an admin**: a person, a role, and an agent of
    /// the register or none for `admin`, never the admin themselves, one
    /// live grant per person, role and agent.
    // The admin, the session and the first record that authorize it, and
    // the grant's four members: each one an argument the one insert takes.
    #[allow(clippy::too_many_arguments)]
    pub async fn admin_grant(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        grant_id: &str,
        person: &str,
        role: &str,
        agent: Option<&str>,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        if person == admin {
            return Ok(Err(Refusal::OwnGrant));
        }
        let scope: Option<String> = sqlx::query_scalar("SELECT scope FROM role WHERE name = $1")
            .bind(role)
            .fetch_optional(&mut *tx)
            .await?;
        let Some(scope) = scope else {
            return Ok(Err(Refusal::NoSuchRole));
        };
        if let Err(refusal) = in_scope(role, &scope, agent) {
            return Ok(Err(refusal));
        }
        let person_stands: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM person WHERE person_id = $1)")
                .bind(person)
                .fetch_one(&mut *tx)
                .await?;
        if !person_stands {
            return Ok(Err(Refusal::NoSuchPerson));
        }
        if let Some(agent) = agent {
            let agent_stands: bool =
                sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agent WHERE agent_id = $1)")
                    .bind(agent)
                    .fetch_one(&mut *tx)
                    .await?;
            if !agent_stands {
                return Ok(Err(Refusal::NoSuchAgent));
            }
        }
        let held: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM role_grant WHERE person_id = $1 AND role = $2 \
             AND agent_id IS NOT DISTINCT FROM $3 AND revoked_at IS NULL)",
        )
        .bind(person)
        .bind(role)
        .bind(agent)
        .fetch_one(&mut *tx)
        .await?;
        if held {
            return Ok(Err(Refusal::Held));
        }
        identity::insert_grant(&mut tx, grant_id, person, role, agent, Some(admin)).await?;
        commit_with_outcome(tx, "grant add", self, first).await?;
        Ok(Ok(()))
    }

    /// **A grant revoked by an admin**, at the version the page read: never
    /// a grant of their own, and never the last enabled admin's.
    pub async fn admin_revoke(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        grant: &str,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let Some(read) = read_grant(&mut tx, grant).await? else {
            return Ok(Err(Refusal::NoSuchGrant));
        };
        if !read.live {
            return Ok(Err(Refusal::Revoked));
        }
        // **The last-admin count comes before the own-grant rule**, so the
        // store's own answer to the last admin's grant is the count's, the
        // one rule that holds whoever asks: through the surface the revoker
        // is an enabled admin who is not the grantee, so it cannot be formed.
        if read.role == "admin" && identity::admins_remaining_without(&mut tx, grant).await? == 0 {
            return Ok(Err(Refusal::LastAdmin));
        }
        #[cfg(test)]
        crate::store::admin::hold_inside(admin).await;
        if read.person_id == admin {
            return Ok(Err(Refusal::OwnGrant));
        }
        if !identity::revoke_grant(&mut tx, grant, Some(admin), version).await? {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "grant remove", self, first).await?;
        Ok(Ok(()))
    }

    /// **A per-agent role's verbs set by an admin**, at the version the page
    /// read: never `admin`, within the vocabulary, and never a role the
    /// admin holds a grant of on any agent, checked under the exclusion.
    pub async fn admin_set_role(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        role: &str,
        verbs: &[String],
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        if role == "admin" {
            return Ok(Err(Refusal::Fixed));
        }
        let verbs = match verbs_within_vocabulary(verbs) {
            Ok(verbs) => verbs,
            Err(refusal) => return Ok(Err(refusal)),
        };
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let role_stands: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM role WHERE name = $1)")
                .bind(role)
                .fetch_one(&mut *tx)
                .await?;
        if !role_stands {
            return Ok(Err(Refusal::NoSuchRole));
        }
        let holds: bool = sqlx::query_scalar(HOLDS_ROLE)
            .bind(admin)
            .bind(role)
            .fetch_one(&mut *tx)
            .await?;
        if holds {
            return Ok(Err(Refusal::HoldsRole));
        }
        if !identity::set_role_verbs(&mut tx, role, &verbs, Some(admin), version).await? {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "role set", self, first).await?;
        Ok(Ok(()))
    }
}

/// Whether `$1` holds a live grant of the role `$2` on any agent.
const HOLDS_ROLE: &str = "SELECT EXISTS (SELECT 1 FROM role_grant \
     WHERE person_id = $1 AND role = $2 AND revoked_at IS NULL)";

async fn read_grant(conn: &mut sqlx::PgConnection, grant: &str) -> anyhow::Result<Option<Grant>> {
    Ok(sqlx::query(
        "SELECT person_id, role, version, revoked_at IS NULL AS live \
         FROM role_grant WHERE grant_id = $1",
    )
    .bind(grant)
    .fetch_optional(conn)
    .await?
    .map(|r| Grant {
        person_id: r.get("person_id"),
        role: r.get("role"),
        version: r.get("version"),
        live: r.get("live"),
    }))
}
