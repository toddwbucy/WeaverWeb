//! conforms: web-the-start-page-refuses-every-name-the-box-refuses
//!
//! **The agent's setup** (Spec 6.2, migration `0026`): one row per setup,
//! mutable until Confirm, written by an admin alone. Each write takes the
//! identity exclusion, re-checks the admin's session and grant inside its
//! transaction (`admin::authorized`), checks the version the page read, and
//! commits with its `ok` outcome, as every admin write does (Spec 2.13).
//! The setup pages write no declaration row.

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;

/// The most characters an agent's name runs to, `create-agent.sh`'s bound
/// at WeaverAgent's `6b91747e`.
pub const NAME_MOST: usize = 16;

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
        version: r.get("version"),
    }
}

const SELECT: &str = "SELECT s.setup_id, s.name, s.virtual_machine, p.name AS begun_by, \
     s.begun_at, s.state, s.version \
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
