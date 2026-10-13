//! **Reading an agent takes a grant on it** (Spec 2.13; design section 9;
//! act 11, PR 6a): any grant, `observer` or `operator`, permits reading
//! everything the server holds of that agent, and a person holding no grant
//! on an agent sees nothing of it, not even its name.
//!
//! **The check is the server's, at every read**, under the identity
//! exclusion held shared for the check alone, as an ask holds it: a
//! revocation or a disable either commits before the check and is seen, or
//! waits until the check's own short transaction ends. The read of the row
//! that follows holds nothing, since it asks no peer and writes nothing.
//! **No read is audited**: a read asks nothing of an agent.

use sqlx::{Postgres, Transaction};

use crate::link::register::Agent;
use crate::store::identity::IDENTITY_LOCK_KEY;
use crate::store::{AgentId, Store};

/// A short transaction holding the identity exclusion shared.
async fn shared_hold(store: &Store) -> anyhow::Result<Transaction<'static, Postgres>> {
    let mut tx = store.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
        .bind(IDENTITY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

impl Store {
    /// **The person's roles on the agent**, live grants of an enabled
    /// person, read under the shared hold; empty where they hold none, and
    /// so may read nothing of it.
    pub async fn roles_on(&self, person: &str, agent: &AgentId) -> anyhow::Result<Vec<String>> {
        let mut tx = shared_hold(self).await?;
        let roles: Vec<String> = sqlx::query_scalar(
            "SELECT g.role FROM role_grant g JOIN person p ON p.person_id = g.person_id \
             WHERE g.person_id = $1 AND g.agent_id = $2 AND g.revoked_at IS NULL \
             AND p.enabled ORDER BY g.role",
        )
        .bind(person)
        .bind(agent.as_str())
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(roles)
    }

    /// **The agents the person may read**: those they hold a live grant
    /// on, any role, an enabled person's, by box and name; nothing of any
    /// other agent.
    pub async fn readable_agents(&self, person: &str) -> anyhow::Result<Vec<Agent>> {
        let mut tx = shared_hold(self).await?;
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT g.agent_id FROM role_grant g \
             JOIN person p ON p.person_id = g.person_id \
             WHERE g.person_id = $1 AND g.agent_id IS NOT NULL AND g.revoked_at IS NULL \
             AND p.enabled",
        )
        .bind(person)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        let mut agents = Vec::new();
        for id in ids {
            let id: AgentId = id.parse().map_err(anyhow::Error::msg)?;
            if let Some(agent) = self.agent(&id).await? {
                agents.push(agent);
            }
        }
        agents.sort_by(|a, b| (&a.r#box, &a.name).cmp(&(&b.r#box, &b.name)));
        Ok(agents)
    }
}
