//! **The operator's SSH public keys** (Spec 6.2, migration `0027`): a
//! console setting, written by an admin alone, at most `KEYS_MOST` held.
//! Each write takes the identity exclusion, re-checks the admin's session
//! and grant inside its transaction (`admin::authorized`) and commits with
//! its `ok` outcome, as every admin write does (Spec 2.13). **A key is a
//! public half and no secret**, so the row holds its line in clear, and
//! nothing of the line enters the audit, which names the key's identity.

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;

/// The most keys the console holds.
pub const KEYS_MOST: i64 = 16;

/// A key, as the pages read it.
#[derive(Debug, Clone)]
pub struct HeldKey {
    pub key_id: String,
    pub line: String,
    pub fingerprint: String,
    /// The name of the person who added it.
    pub added_by: String,
    pub added_at: DateTime<Utc>,
}

impl Store {
    /// **Every key held**, the oldest first.
    pub async fn ssh_keys(&self) -> anyhow::Result<Vec<HeldKey>> {
        let mut conn = self.pool.acquire().await?;
        Self::ssh_keys_on(&mut conn).await
    }

    /// The same read on a connection the caller holds, so it can share a
    /// transaction's snapshot.
    pub(crate) async fn ssh_keys_on(conn: &mut sqlx::PgConnection) -> anyhow::Result<Vec<HeldKey>> {
        let rows = sqlx::query(
            "SELECT k.key_id, k.line, k.fingerprint, p.name AS added_by, k.added_at \
             FROM operator_ssh_key k JOIN person p ON p.person_id = k.added_by \
             ORDER BY k.added_at, k.key_id",
        )
        .fetch_all(&mut *conn)
        .await?;
        Ok(rows
            .iter()
            .map(|r| HeldKey {
                key_id: r.get("key_id"),
                line: r.get("line"),
                fingerprint: r.get("fingerprint"),
                added_by: r.get("added_by"),
                added_at: r.get("added_at"),
            })
            .collect())
    }

    /// Whether the key `key_id` names is held.
    pub async fn ssh_key_held(&self, key_id: &str) -> anyhow::Result<bool> {
        Ok(
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM operator_ssh_key WHERE key_id = $1)")
                .bind(key_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    /// **An admin's add** of a line whose form the surface checked, under
    /// the identity exclusion, so the bound is counted against every other
    /// write. Refused where the key is held already or `KEYS_MOST` are.
    pub async fn add_ssh_key(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        key_id: &str,
        line: &str,
        fingerprint: &str,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let held: i64 = sqlx::query_scalar("SELECT count(*) FROM operator_ssh_key")
            .fetch_one(&mut *tx)
            .await?;
        if held >= KEYS_MOST {
            return Ok(Err(Refusal::KeysFull));
        }
        let written = sqlx::query(
            "INSERT INTO operator_ssh_key (key_id, line, fingerprint, added_by) \
             VALUES ($1, $2, $3, $4) ON CONFLICT (fingerprint) DO NOTHING",
        )
        .bind(key_id)
        .bind(line)
        .bind(fingerprint)
        .bind(admin)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::KeyHeld));
        }
        commit_with_outcome(tx, "ssh-key add", self, first).await?;
        Ok(Ok(()))
    }

    /// **An admin's removal** of the key `key_id` names.
    pub async fn remove_ssh_key(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        key_id: &str,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query("DELETE FROM operator_ssh_key WHERE key_id = $1")
            .bind(key_id)
            .execute(&mut *tx)
            .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::NoSuchKey));
        }
        commit_with_outcome(tx, "ssh-key remove", self, first).await?;
        Ok(Ok(()))
    }
}
