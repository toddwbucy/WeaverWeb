//! **The model pulls' rows** (Spec 2.15, migration `0025`): one row per
//! pull and one per file, the rulings' record of what the console fetched:
//! the repository, the commit, every file's SHA-256 and size, the account
//! that pulled and the time.
//!
//! **An admin's ask is an admin write** (Spec 2.13): the identity exclusion,
//! the session and the admin grant re-checked inside its transaction, and
//! its `ok` outcome committed with it. **One pull runs at a time**: the ask
//! is refused, naming the pull that runs, where one runs, and a partial
//! unique index holds it at the store. The puller's own writes (progress, a
//! file done, the pull's end) are the server's, about a pull an audited ask
//! began, and carry no audit record of their own.

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::hugging_face::{RepoFile, RepoId};
use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;

/// A pull's row.
#[derive(Debug, Clone)]
pub struct Pull {
    pub pull_id: String,
    pub repository: String,
    pub commit: String,
    pub account: Option<String>,
    /// The name of the person who asked.
    pub asked_by: String,
    pub asked_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub state: String,
    pub reason: Option<String>,
    /// The name of the admin whose cancel ended the pull.
    pub cancelled_by: Option<String>,
}

/// A file's row.
#[derive(Debug, Clone)]
pub struct PullFile {
    pub file: RepoFile,
    pub bytes_done: i64,
    /// `downloaded` or `found`, once the file is verified.
    pub source: Option<String>,
}

/// Where a file's bytes came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Downloaded,
    Found,
}

impl Source {
    fn as_str(self) -> &'static str {
        match self {
            Source::Downloaded => "downloaded",
            Source::Found => "found",
        }
    }
}

const PULL: &str = "SELECT m.pull_id, m.repository, m.commit_id, m.account, p.name AS asked_by, \
     m.asked_at, m.finished_at, m.state, m.reason, c.name AS cancelled_by \
     FROM model_pull m JOIN person p ON p.person_id = m.asked_by \
     LEFT JOIN person c ON c.person_id = m.cancelled_by";

fn pull_of(r: &sqlx::postgres::PgRow) -> Pull {
    Pull {
        pull_id: r.get("pull_id"),
        repository: r.get("repository"),
        commit: r.get("commit_id"),
        account: r.get("account"),
        asked_by: r.get("asked_by"),
        asked_at: r.get("asked_at"),
        finished_at: r.get("finished_at"),
        state: r.get("state"),
        reason: r.get("reason"),
        cancelled_by: r.get("cancelled_by"),
    }
}

impl Store {
    /// **An admin's ask of a pull**: the pull's row, `running`, and a row
    /// for each file, in the order given. Refused where another pull runs.
    #[allow(clippy::too_many_arguments)]
    pub async fn ask_pull(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        pull_id: &str,
        repo: &RepoId,
        commit: &str,
        account: Option<&str>,
        files: &[RepoFile],
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let running: Option<(String, String, String)> = sqlx::query_as(
            "SELECT pull_id, repository, commit_id FROM model_pull WHERE state = 'running'",
        )
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((running, repository, commit)) = running {
            return Ok(Err(Refusal::PullRunning(format!(
                "{running}, of {repository} at {commit}"
            ))));
        }
        sqlx::query(
            "INSERT INTO model_pull (pull_id, repository, commit_id, account, asked_by, state) \
             VALUES ($1, $2, $3, $4, $5, 'running')",
        )
        .bind(pull_id)
        .bind(repo.to_string())
        .bind(commit)
        .bind(account)
        .bind(admin)
        .execute(&mut *tx)
        .await?;
        for (ordinal, file) in files.iter().enumerate() {
            sqlx::query(
                "INSERT INTO model_pull_file (pull_id, ordinal, path, size, blob_id, sha256, lfs) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(pull_id)
            .bind(ordinal as i32)
            .bind(&file.path)
            .bind(i64::try_from(file.size)?)
            .bind(&file.blob_id)
            .bind(&file.sha256)
            .bind(file.sha256.is_some())
            .execute(&mut *tx)
            .await?;
        }
        commit_with_outcome(tx, "model pull", self, first).await?;
        Ok(Ok(()))
    }

    /// **An admin's cancel of a running pull**: the pull ends `cancelled`,
    /// naming the admin, in the admin's transaction with the `ok` outcome.
    /// Its files' partials stay on disk, so a later pull of the same files
    /// resumes from them. Refused where the pull no longer runs.
    pub async fn cancel_pull(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        pull_id: &str,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query(
            "UPDATE model_pull SET state = 'cancelled', finished_at = now(), cancelled_by = $2 \
             WHERE pull_id = $1 AND state = 'running'",
        )
        .bind(pull_id)
        .bind(admin)
        .execute(&mut *tx)
        .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::PullNotRunning(pull_id.to_owned())));
        }
        commit_with_outcome(tx, "model pull cancel", self, first).await?;
        Ok(Ok(()))
    }

    /// The pull that runs, if one does.
    pub async fn running_pull(&self) -> anyhow::Result<Option<Pull>> {
        let query = format!("{PULL} WHERE m.state = 'running'");
        Ok(sqlx::query(sqlx::AssertSqlSafe(query))
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(pull_of))
    }

    /// A pull by its identity.
    pub async fn pull(&self, pull_id: &str) -> anyhow::Result<Option<Pull>> {
        let query = format!("{PULL} WHERE m.pull_id = $1");
        Ok(sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(pull_id)
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(pull_of))
    }

    /// The latest pulls, newest first.
    pub async fn pulls(&self, limit: i64) -> anyhow::Result<Vec<Pull>> {
        let query = format!("{PULL} ORDER BY m.asked_at DESC, m.pull_id LIMIT $1");
        Ok(sqlx::query(sqlx::AssertSqlSafe(query))
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(pull_of)
            .collect())
    }

    /// A pull's files, in the order asked.
    pub async fn pull_files(&self, pull_id: &str) -> anyhow::Result<Vec<PullFile>> {
        let rows = sqlx::query(
            "SELECT path, size, blob_id, sha256, bytes_done, source FROM model_pull_file \
             WHERE pull_id = $1 ORDER BY ordinal",
        )
        .bind(pull_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(PullFile {
                    file: RepoFile {
                        path: r.get("path"),
                        size: u64::try_from(r.get::<i64, _>("size"))?,
                        blob_id: r.get("blob_id"),
                        sha256: r.get("sha256"),
                    },
                    bytes_done: r.get("bytes_done"),
                    source: r.get("source"),
                })
            })
            .collect()
    }

    /// A file's bytes written so far.
    pub async fn pull_progress(&self, pull_id: &str, path: &str, bytes: u64) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE model_pull_file SET bytes_done = $3 \
             WHERE pull_id = $1 AND path = $2 AND source IS NULL",
        )
        .bind(pull_id)
        .bind(path)
        .bind(i64::try_from(bytes)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// **A file verified**: its SHA-256 recorded (Hugging Face's for an LFS
    /// file, the console's own for a file kept in git) and where its bytes
    /// came from.
    pub async fn pull_file_done(
        &self,
        pull_id: &str,
        path: &str,
        sha256: &str,
        source: Source,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE model_pull_file SET sha256 = $3, source = $4, bytes_done = size \
             WHERE pull_id = $1 AND path = $2",
        )
        .bind(pull_id)
        .bind(path)
        .bind(sha256)
        .bind(source.as_str())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// **A pull ended**: `verified`, or `failed` with its reason.
    pub async fn pull_finished(&self, pull_id: &str, failed: Option<&str>) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE model_pull SET finished_at = now(), \
             state = CASE WHEN $2::text IS NULL THEN 'verified' ELSE 'failed' END, reason = $2 \
             WHERE pull_id = $1 AND state = 'running'",
        )
        .bind(pull_id)
        .bind(failed)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
