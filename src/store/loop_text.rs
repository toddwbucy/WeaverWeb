//! conforms: web-a-loops-text-enters-no-audit-record
//! conforms: web-a-loop-save-against-a-stale-version-refuses
//! conforms: web-a-loop-version-is-never-changed-or-removed
//!
//! **An agent's loop, versioned** (Spec 6.3, migration `0035`): the wire
//! file a person holding `operator` on the agent saves through the loop
//! page, one row per version, never changed or removed. A save re-checks the
//! person's session and their `operator` grant inside its transaction,
//! under the identity exclusion held shared, and lands only where the
//! version the person read is still the head, its `ok` outcome committed
//! with it, as every person's write does (Spec 2.13).
//!
//! **The text enters no audit record.** The record names the version's
//! `lp-` identity, and its action names the digest and the byte count
//! ([`action`]); the text is the row's alone. The grammar is the agent's
//! side's and is not judged here.

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::commit::commit_with_outcome;
use crate::store::identity::session_stands;
use crate::store::{AgentId, Store};

/// **The most bytes a wire file holds**, the contract's 256 KiB
/// (`weaver-harness-loop-contract` section 7), counted after each CRLF is
/// stored as LF.
pub const WIRE_FILE_MOST: usize = 262_144;

/// A stored version, as the page reads it.
#[derive(Debug, Clone)]
pub struct LoopVersion {
    pub loop_id: String,
    pub version: i32,
    pub text: String,
    pub bytes: i32,
    pub sha256: String,
    /// The name of the person who saved it.
    pub saved_by: String,
    pub saved_at: DateTime<Utc>,
}

/// The text's SHA-256, lowercase hex: the digest the row holds and the
/// schema checks.
pub fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// **A save's audit action**: the version's digest and byte count, and
/// never its text (Spec 2.13). The first record, a refusal and the outcome
/// all carry it.
pub fn action(sha256: &str, bytes: usize) -> String {
    format!("loop save sha256 {sha256} bytes {bytes}")
}

/// How a save ended, where the store answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// The version, at the head's number plus one.
    Done { version: i32 },
    /// The session or the `operator` grant ended before the write.
    AuthorityGone,
    /// The version read is no longer the head.
    Stale,
}

impl Store {
    /// **A save's first record**, written before the save (Spec 2.13): the
    /// person by their session, the target the version's `lp-` identity
    /// minted for it, the action [`action`]'s, so the record holds the
    /// digest and the byte count of `text` and never `text`.
    pub async fn loop_save_first(
        &self,
        person: &str,
        loop_id: &str,
        text: &str,
    ) -> anyhow::Result<String> {
        let principal = Principal::Person {
            person_id: person,
            method: PersonMethod::Session,
        };
        self.audit_first(
            principal,
            Target::Loop(loop_id),
            &action(&digest(text), text.len()),
        )
        .await
    }

    /// **The agent's head**, its latest version, or none where none is
    /// saved.
    pub async fn loop_head(&self, agent: &AgentId) -> anyhow::Result<Option<LoopVersion>> {
        let row = sqlx::query(
            "SELECT l.loop_id, l.version, l.text, l.bytes, l.sha256, p.name AS saved_by, \
             l.saved_at FROM agent_loop l JOIN person p ON p.person_id = l.saved_by \
             WHERE l.agent_id = $1 ORDER BY l.version DESC LIMIT 1",
        )
        .bind(agent.as_str())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| LoopVersion {
            loop_id: r.get("loop_id"),
            version: r.get("version"),
            text: r.get("text"),
            bytes: r.get("bytes"),
            sha256: r.get("sha256"),
            saved_by: r.get("saved_by"),
            saved_at: r.get("saved_at"),
        }))
    }

    /// **The latest version of the agent's that has the digest**, or none:
    /// which saved version a load's digest names.
    pub async fn loop_version_of(
        &self,
        agent: &AgentId,
        sha256: &str,
    ) -> anyhow::Result<Option<i32>> {
        Ok(sqlx::query_scalar(
            "SELECT max(version) FROM agent_loop WHERE agent_id = $1 AND sha256 = $2",
        )
        .bind(agent.as_str())
        .bind(sha256)
        .fetch_one(&self.pool)
        .await?)
    }

    /// **A person's save** of a text the surface checked, at the version
    /// they read (0 where none was saved): in one transaction under the
    /// identity exclusion held shared, the session and the `operator` grant
    /// re-checked, then the version inserted at the head's number plus one
    /// only where the head is still the one read. Two saves of one head
    /// both reach the insert, and the second meets the first's row at the
    /// unique version and inserts nothing, so it is stale too.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_loop(
        &self,
        session_id: i64,
        person: &str,
        agent: &AgentId,
        read: i32,
        loop_id: &str,
        text: &str,
        first: &str,
    ) -> anyhow::Result<Saved> {
        let mut tx = crate::store::access::shared_hold(self).await?;
        let operator: bool = if session_stands(&mut tx, session_id, person).await? {
            sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM role_grant WHERE person_id = $1 AND agent_id = $2 \
                 AND role = 'operator' AND revoked_at IS NULL)",
            )
            .bind(person)
            .bind(agent.as_str())
            .fetch_one(&mut *tx)
            .await?
        } else {
            false
        };
        if !operator {
            tx.rollback().await?;
            return Ok(Saved::AuthorityGone);
        }
        let bytes = i32::try_from(text.len())?;
        let version: Option<i32> = sqlx::query_scalar(
            "INSERT INTO agent_loop (loop_id, agent_id, version, text, bytes, sha256, saved_by) \
             SELECT $1, $2, $3 + 1, $4, $5, $6, $7 \
             WHERE coalesce((SELECT max(version) FROM agent_loop WHERE agent_id = $2), 0) = $3 \
             ON CONFLICT (agent_id, version) DO NOTHING RETURNING version",
        )
        .bind(loop_id)
        .bind(agent.as_str())
        .bind(read)
        .bind(text)
        .bind(bytes)
        .bind(digest(text))
        .bind(person)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(version) = version else {
            tx.rollback().await?;
            return Ok(Saved::Stale);
        };
        commit_with_outcome(tx, "loop save", self, first).await?;
        Ok(Saved::Done { version })
    }
}
