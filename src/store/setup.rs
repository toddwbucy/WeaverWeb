//! conforms: web-the-start-page-refuses-every-name-the-box-refuses
//!
//! **The agent's setup** (Spec 6.2, migration `0026`): one row per setup,
//! mutable until Confirm, written by an admin alone. Each write takes the
//! identity exclusion, re-checks the admin's session and grant inside its
//! transaction (`admin::authorized`), checks the version the page read, and
//! commits with its `ok` outcome, as every admin write does (Spec 2.13).
//! The setup pages write no declaration row. Confirm and the tuple it
//! writes are `tuple.rs`'s.

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::hugging_face::{RepoFile, RepoId};
use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;
use crate::store::model_pull::insert_pull;

/// The most characters an agent's name runs to, `create-agent.sh`'s bound
/// at WeaverAgent's `6b91747e`.
pub const NAME_MOST: usize = 16;

/// **The box's max-tokens-per-turn** (Spec 6.2, `toddwbucy/WeaverAgent#580`):
/// the agent-creation tool writes 4096 into every declaration it lays and
/// refuses a context capacity below it, since a capacity under the per-turn
/// ceiling makes an agent whose turn can exceed its whole context, which an
/// appliance fixes only by taking the agent down and creating it again. So
/// the placement page's context length is bounded below by it, and the
/// deployment tuple carries it beside the context length. This constant is
/// the one place it is written, so a later per-agent value replaces it here.
pub const MAX_TOKENS_PER_TURN: i64 = 4096;

/// **The names `weaver-admin-Spec` section 4 reserves whole**, at
/// WeaverAgent's `6b91747e`: the character rule below admits them, so they
/// are refused by name. Every reserved suffix of that section holds a `-`,
/// which the character rule refuses.
const RESERVED: [&str; 2] = ["gatecon", "admincon"];

/// **The start page's rule** (Spec 6.2): the name as given, or why it is
/// refused. A lowercase letter, then lowercase letters and digits, 2 to 16
/// characters in all, which is `create-agent.sh`'s rule and refuses every
/// character admin's name rule refuses, and neither name admin's rule
/// reserves whole. A name either rule refuses could not stand on a box. The
/// store's CHECK holds the same rule, so a page that took a refused name
/// would meet a fault, never a row.
pub fn agent_name(name: &str) -> Result<&str, &'static str> {
    let mut chars = name.chars();
    match chars.next() {
        None => return Err("a name is required"),
        Some(c) if !c.is_ascii_lowercase() => {
            return Err("a name begins with a lowercase letter, a to z");
        }
        Some(_) => {}
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) {
        return Err("a name holds lowercase letters and digits alone");
    }
    if name.len() < 2 {
        return Err("a name is 2 characters at least");
    }
    if name.len() > NAME_MOST {
        return Err("a name is 16 characters at most");
    }
    if RESERVED.contains(&name) {
        return Err("that name names one of a box's own accounts, so no agent takes it");
    }
    Ok(name)
}

/// A setup, as the pages read it.
#[derive(Debug, Clone)]
pub struct Setup {
    pub setup_id: String,
    pub name: String,
    pub virtual_machine: bool,
    /// The name of the person who began it.
    pub begun_by: String,
    pub begun_at: DateTime<Utc>,
    /// `open` or `confirmed`.
    pub state: String,
    /// The context length the placement page kept, and none until it does.
    pub context_length: Option<i64>,
    /// The turn context the placement page kept, and none until it does.
    pub turn_context: Option<String>,
    /// The devices the placement page kept, ascending, and none until it
    /// does.
    pub devices: Option<Vec<i32>>,
    pub version: i64,
}

fn setup_of(r: &sqlx::postgres::PgRow) -> Setup {
    Setup {
        setup_id: r.get("setup_id"),
        name: r.get("name"),
        virtual_machine: r.get("virtual_machine"),
        begun_by: r.get("begun_by"),
        begun_at: r.get("begun_at"),
        state: r.get("state"),
        context_length: r.get("context_length"),
        turn_context: r.get("turn_context"),
        devices: r.get("devices"),
        version: r.get("version"),
    }
}

const SELECT: &str = "SELECT s.setup_id, s.name, s.virtual_machine, p.name AS begun_by, \
     s.begun_at, s.state, s.context_length, s.turn_context, s.devices, s.version \
     FROM agent_setup s JOIN person p ON p.person_id = s.begun_by";

impl Store {
    /// **The setups left open**, the oldest first, for the start page.
    pub async fn open_setups(&self) -> anyhow::Result<Vec<Setup>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE s.state = 'open' ORDER BY s.begun_at, s.setup_id"
        )))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(setup_of).collect())
    }

    /// The setup `setup_id` names, or `None`.
    pub async fn setup(&self, setup_id: &str) -> anyhow::Result<Option<Setup>> {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE s.setup_id = $1"
        )))
        .bind(setup_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().map(setup_of))
    }

    /// **Whether an agent stands in the register**, so the start page draws
    /// New agent in the name's place. A retired row still stands there.
    pub async fn an_agent_stands(&self) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM agent)")
            .fetch_one(&self.pool)
            .await?)
    }

    /// **An admin's new setup of `name`**, already held to the start page's
    /// rule. Refused where an open setup holds the name, the one-open index
    /// deciding under concurrency.
    pub async fn begin_setup(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        setup_id: &str,
        name: &str,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query(
            "INSERT INTO agent_setup (setup_id, name, begun_by) VALUES ($1, $2, $3) \
             ON CONFLICT (name) WHERE state = 'open' DO NOTHING",
        )
        .bind(setup_id)
        .bind(name)
        .bind(admin)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::SetupNameHeld(name.to_owned())));
        }
        commit_with_outcome(tx, "setup begin", self, first).await?;
        Ok(Ok(()))
    }

    /// **The pairing page's checkbox**, over the open setup read at
    /// `version`. Stale where the row is not what the page read.
    pub async fn setup_machine(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        setup_id: &str,
        virtual_machine: bool,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query(
            "UPDATE agent_setup SET virtual_machine = $1, version = version + 1 \
             WHERE setup_id = $2 AND version = $3 AND state = 'open'",
        )
        .bind(virtual_machine)
        .bind(setup_id)
        .bind(version)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "setup pairing", self, first).await?;
        Ok(Ok(()))
    }
}

/// **The placement page's three fields** (Spec 6.2), each already held to
/// its source: the context length to the decoder's maximum, the devices to
/// the survey's cards, ascending. A field the page holds is none.
pub struct Placement<'a> {
    pub context_length: Option<i64>,
    pub turn_context: &'a str,
    pub devices: Option<&'a [i32]>,
}

impl Store {
    /// **The placement page's save** (Spec 6.2), over the open setup read
    /// at `version`: the three fields written as the page holds them, a
    /// held one none. Stale where the row is not what the page read.
    pub async fn setup_placement(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        setup_id: &str,
        version: i64,
        placement: &Placement<'_>,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query(
            "UPDATE agent_setup SET context_length = $1, turn_context = $2, devices = $3, \
             version = version + 1 WHERE setup_id = $4 AND version = $5 AND state = 'open'",
        )
        .bind(placement.context_length)
        .bind(placement.turn_context)
        .bind(placement.devices)
        .bind(setup_id)
        .bind(version)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "setup placement", self, first).await?;
        Ok(Ok(()))
    }
}

/// **A model role's row** (Spec 6.2), as the pages read it.
#[derive(Debug, Clone)]
pub struct SetupModel {
    /// `decoder` or `adapter`.
    pub role: String,
    pub repository: String,
    pub commit: String,
    /// The choice, named by its first part's path.
    pub choice: String,
    pub pull_id: String,
    /// The pull's state: `running`, `verified`, `failed` or `cancelled`.
    pub pull_state: String,
    /// The adapter's scale, and the decoder's none.
    pub scale: Option<f64>,
    /// The facts read from the choice's header.
    pub header: serde_json::Value,
    /// The name of the person who picked.
    pub picked_by: String,
    pub picked_at: DateTime<Utc>,
}

/// **A pick**, every member resolved: the role, the choice and its parts as
/// Hugging Face's listing gave them at the commit, the header's facts, the
/// adapter's scale, and the pull it asks.
pub struct Pick<'a> {
    pub role: &'a str,
    pub repository: &'a RepoId,
    pub commit: &'a str,
    pub choice: &'a str,
    pub scale: Option<f64>,
    pub header: &'a serde_json::Value,
    pub pull_id: &'a str,
    /// The account the pull runs through, where one is connected.
    pub account: Option<&'a str>,
    /// The choice's parts, in split order.
    pub parts: &'a [RepoFile],
}

impl Store {
    /// **A setup's model roles**, the decoder first.
    pub async fn setup_models(&self, setup_id: &str) -> anyhow::Result<Vec<SetupModel>> {
        let rows = sqlx::query(
            "SELECT m.role, m.repository, m.commit_id, m.choice, m.pull_id, \
             mp.state AS pull_state, m.scale, m.header, p.name AS picked_by, m.picked_at \
             FROM agent_setup_model m JOIN person p ON p.person_id = m.picked_by \
             JOIN model_pull mp ON mp.pull_id = m.pull_id \
             WHERE m.setup_id = $1 ORDER BY m.role = 'adapter', m.role",
        )
        .bind(setup_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| SetupModel {
                role: r.get("role"),
                repository: r.get("repository"),
                commit: r.get("commit_id"),
                choice: r.get("choice"),
                pull_id: r.get("pull_id"),
                pull_state: r.get("pull_state"),
                scale: r.get("scale"),
                header: r.get("header"),
                picked_by: r.get("picked_by"),
                picked_at: r.get("picked_at"),
            })
            .collect())
    }

    /// **An admin's pick** (Spec 6.2), in one transaction under the
    /// identity exclusion: the setup's version checked and moved, the pull's
    /// rows written (refused, naming it, where a pull runs), and the role's
    /// row written or, for a role the setup holds, replaced, the earlier
    /// pick's files staying in the cache. Refused where the setup was
    /// written since the page read it, or is confirmed.
    pub async fn setup_pick(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        setup_id: &str,
        version: i64,
        pick: &Pick<'_>,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query(
            "UPDATE agent_setup SET version = version + 1 \
             WHERE setup_id = $1 AND version = $2 AND state = 'open'",
        )
        .bind(setup_id)
        .bind(version)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        if let Err(refusal) = insert_pull(
            &mut tx,
            admin,
            pick.pull_id,
            pick.repository,
            pick.commit,
            pick.account,
            pick.parts,
        )
        .await?
        {
            return Ok(Err(refusal));
        }
        sqlx::query(
            "INSERT INTO agent_setup_model \
             (setup_id, role, repository, commit_id, choice, pull_id, scale, header, picked_by) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
             ON CONFLICT (setup_id, role) DO UPDATE SET repository = EXCLUDED.repository, \
             commit_id = EXCLUDED.commit_id, choice = EXCLUDED.choice, \
             pull_id = EXCLUDED.pull_id, scale = EXCLUDED.scale, header = EXCLUDED.header, \
             picked_by = EXCLUDED.picked_by, picked_at = now()",
        )
        .bind(setup_id)
        .bind(pick.role)
        .bind(pick.repository.to_string())
        .bind(pick.commit)
        .bind(pick.choice)
        .bind(pick.pull_id)
        .bind(pick.scale)
        .bind(pick.header)
        .bind(admin)
        .execute(&mut *tx)
        .await?;
        commit_with_outcome(tx, "setup pick", self, first).await?;
        Ok(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::agent_name;

    /// **One case per clause of the start page's rule**, each refused, and
    /// the bounds' own lengths kept. The store-backed walk is in
    /// `src/setup_tests.rs`.
    #[test]
    fn the_rule_refuses_each_clause_and_keeps_its_bounds() {
        for refused in [
            "",
            "Ada",
            "ad-a",
            "ad_a",
            "1ada",
            "a",
            "abcdefghijklmnopq",
            "gatecon",
            "admincon",
            "ad a",
            "ad\u{e9}",
        ] {
            assert!(agent_name(refused).is_err(), "{refused:?} is refused");
        }
        for kept in ["ab", "abcdefghijklmnop", "ada7", "gatecon2"] {
            assert_eq!(agent_name(kept), Ok(kept));
        }
    }
}
