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
//!
//! **A verb takes a grant that carries it** (act 11, PR 6b2): the union of
//! the verbs of the person's live grants on that agent, the per-agent
//! grants alone; the server-wide admin grant names no agent and authorizes
//! nothing here. Read inside the ask's own hold, which `link::listener`
//! keeps from the check to the enqueue.

use std::collections::BTreeSet;

use sqlx::{Postgres, Transaction};

use crate::link::register::Agent;
use crate::store::identity::IDENTITY_LOCK_KEY;
use crate::store::{AgentId, Store};

/// A transaction holding the identity exclusion shared: a check's alone,
/// or an ask's from its check to its enqueue.
pub(crate) async fn shared_hold(store: &Store) -> anyhow::Result<Transaction<'static, Postgres>> {
    let mut tx = store.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
        .bind(IDENTITY_LOCK_KEY)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

/// **The verbs the person's live grants on the agent carry**, read in the
/// caller's transaction under its hold; `None` where they hold no grant on
/// it. Only per-agent grants count: `agent_id = $2` is never true of a
/// server-wide grant, whose agent is NULL. The person's standing is the
/// caller's, by `identity::session_stands` in the same transaction.
pub(crate) async fn verbs_granted_in(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
    agent: &AgentId,
) -> anyhow::Result<Option<BTreeSet<String>>> {
    let sets: Vec<Vec<String>> = sqlx::query_scalar(
        "SELECT r.verbs FROM role_grant g JOIN role r ON r.name = g.role \
         WHERE g.person_id = $1 AND g.agent_id = $2 AND g.revoked_at IS NULL",
    )
    .bind(person)
    .bind(agent.as_str())
    .fetch_all(&mut **tx)
    .await?;
    if sets.is_empty() {
        return Ok(None);
    }
    Ok(Some(sets.into_iter().flatten().collect()))
}

impl Store {
    /// **The verbs an enabled person's grants on the agent carry**, for the
    /// page's forms, under the shared hold for the read alone; empty where
    /// they hold none. Hiding a form is presentation: the ask checks again.
    pub async fn verbs_on(
        &self,
        person: &str,
        agent: &AgentId,
    ) -> anyhow::Result<BTreeSet<String>> {
        let mut tx = shared_hold(self).await?;
        let enabled: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM person WHERE person_id = $1 AND enabled)",
        )
        .bind(person)
        .fetch_one(&mut *tx)
        .await?;
        let verbs = if enabled {
            verbs_granted_in(&mut tx, person, agent).await?
        } else {
            None
        };
        tx.commit().await?;
        Ok(verbs.unwrap_or_default())
    }

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
