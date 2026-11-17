//! **The base's identity rows** (Spec 6.2, migration `0028`): one immutable
//! row per repository, commit and choice, the BLAKE3 digest the console
//! computed from its cached parts, naming the verified pull it hashed. The
//! hasher (`src/hugging_face/base_identity.rs`) is the one writer, about a
//! pull an audited pick began, and its write carries no audit record.

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::store::Store;

/// A base's identity, as the pages read it.
#[derive(Debug, Clone, PartialEq)]
pub struct BaseIdentity {
    pub blake3: String,
    pub bytes: i64,
    pub pull_id: String,
    pub computed_at: DateTime<Utc>,
}

/// **A decoder's choice owed its identity**: its pull verified, and no row
/// holding its digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owed {
    pub repository: String,
    pub commit: String,
    pub choice: String,
    pub pull_id: String,
}

impl Store {
    /// The identity of the choice at the commit, where it is computed.
    pub async fn base_identity(
        &self,
        repository: &str,
        commit: &str,
        choice: &str,
    ) -> anyhow::Result<Option<BaseIdentity>> {
        let row = sqlx::query(
            "SELECT blake3, bytes, pull_id, computed_at FROM base_identity \
             WHERE repository = $1 AND commit_id = $2 AND choice = $3",
        )
        .bind(repository)
        .bind(commit)
        .bind(choice)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| BaseIdentity {
            blake3: r.get("blake3"),
            bytes: r.get("bytes"),
            pull_id: r.get("pull_id"),
            computed_at: r.get("computed_at"),
        }))
    }

    /// **Every decoder's choice owed its identity**, once each, through the
    /// oldest pick's verified pull: what a start finds and what each
    /// verified pull adds.
    pub async fn base_identities_owed(&self) -> anyhow::Result<Vec<Owed>> {
        let rows = sqlx::query(
            "SELECT DISTINCT ON (m.repository, m.commit_id, m.choice) \
             m.repository, m.commit_id, m.choice, m.pull_id \
             FROM agent_setup_model m JOIN model_pull p ON p.pull_id = m.pull_id \
             WHERE m.role = 'decoder' AND p.state = 'verified' AND NOT EXISTS ( \
               SELECT 1 FROM base_identity b WHERE b.repository = m.repository \
               AND b.commit_id = m.commit_id AND b.choice = m.choice) \
             ORDER BY m.repository, m.commit_id, m.choice, m.picked_at",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| Owed {
                repository: r.get("repository"),
                commit: r.get("commit_id"),
                choice: r.get("choice"),
                pull_id: r.get("pull_id"),
            })
            .collect())
    }

    /// **The identity kept**, once: a row that stands already is left as
    /// it is, since the same parts hash the same.
    pub async fn keep_base_identity(
        &self,
        owed: &Owed,
        blake3: &str,
        bytes: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO base_identity (repository, commit_id, choice, blake3, bytes, pull_id) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT (repository, commit_id, choice) DO NOTHING",
        )
        .bind(&owed.repository)
        .bind(&owed.commit)
        .bind(&owed.choice)
        .bind(blake3)
        .bind(i64::try_from(bytes)?)
        .bind(&owed.pull_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
