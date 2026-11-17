//! conforms: web-the-accounts-version-is-never-repeated-across-a-disconnect
//!
//! **The Hugging Face account's row** (Spec 2.15, migration `0024`): one
//! row, a console setting, written by an admin alone. Each write takes the
//! identity exclusion, re-checks the admin's session and grant inside its
//! transaction (`admin::authorized`), checks the version the page read, and
//! commits with its `ok` outcome, as every admin write does (Spec 2.13).
//!
//! **The row holds the token sealed** (`hugging_face::seal`), and nothing in
//! this module sees it in clear: a connect is handed the seal, and a read
//! answers it for the surface to open.

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::hugging_face::Account;
use crate::hugging_face::seal::Sealed;
use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;

/// The row, as the page reads it.
#[derive(Debug, Clone)]
pub struct AccountRow {
    pub account: String,
    pub role: String,
    pub sealed: Sealed,
    pub set_at: DateTime<Utc>,
    /// The name of the person who set it.
    pub set_by: String,
    pub checked_at: DateTime<Utc>,
    pub refused_at: Option<DateTime<Utc>>,
    pub version: i64,
}

/// What a check met at Hugging Face.
#[derive(Debug, Clone)]
pub enum Checked {
    Accepted(Account),
    Refused,
}

impl Store {
    /// **The account's row**, or `None` where none is connected.
    pub async fn hugging_face_account(&self) -> anyhow::Result<Option<AccountRow>> {
        let row = sqlx::query(
            "SELECT h.account, h.role, h.sealed, h.nonce, h.set_at, p.name AS set_by, \
             h.checked_at, h.refused_at, h.version \
             FROM hugging_face_account h JOIN person p ON p.person_id = h.set_by",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| AccountRow {
            account: r.get("account"),
            role: r.get("role"),
            sealed: Sealed {
                ciphertext: r.get("sealed"),
                nonce: r.get("nonce"),
            },
            set_at: r.get("set_at"),
            set_by: r.get("set_by"),
            checked_at: r.get("checked_at"),
            refused_at: r.get("refused_at"),
            version: r.get("version"),
        }))
    }

    /// **An admin's connect**: the account Hugging Face named and the token
    /// sealed, written where none stands (`version` 0) or over the row read
    /// at `version`. Stale where the row is not what the page read. Every
    /// write draws its version from migration `0024`'s sequence, so a version
    /// read before a disconnect never matches the row a connect again makes.
    pub async fn hugging_face_connect(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        account: &Account,
        sealed: &Sealed,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = if version == 0 {
            sqlx::query(
                "INSERT INTO hugging_face_account (account, role, sealed, nonce, set_by) \
                 VALUES ($1, $2, $3, $4, $5) ON CONFLICT (one) DO NOTHING",
            )
            .bind(&account.name)
            .bind(account.role.as_str())
            .bind(&sealed.ciphertext)
            .bind(&sealed.nonce)
            .bind(admin)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "UPDATE hugging_face_account SET account = $1, role = $2, sealed = $3, \
                 nonce = $4, set_by = $5, set_at = now(), checked_at = now(), \
                 refused_at = NULL, version = nextval('hugging_face_account_version') \
                 WHERE version = $6",
            )
            .bind(&account.name)
            .bind(account.role.as_str())
            .bind(&sealed.ciphertext)
            .bind(&sealed.nonce)
            .bind(admin)
            .bind(version)
            .execute(&mut *tx)
            .await?
        };
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "hugging-face connect", self, first).await?;
        Ok(Ok(()))
    }

    /// **An admin's check**: what Hugging Face answered for the stored
    /// token, over the row read at `version`. An accepted token records the
    /// account and role it answered and clears a refusal. A refused one
    /// records when, and the setting stands until a disconnect.
    pub async fn hugging_face_checked(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        checked: &Checked,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = match checked {
            Checked::Accepted(account) => {
                sqlx::query(
                    "UPDATE hugging_face_account SET account = $1, role = $2, \
                     checked_at = now(), refused_at = NULL, \
                     version = nextval('hugging_face_account_version') WHERE version = $3",
                )
                .bind(&account.name)
                .bind(account.role.as_str())
                .bind(version)
                .execute(&mut *tx)
                .await?
            }
            Checked::Refused => {
                sqlx::query(
                    "UPDATE hugging_face_account SET checked_at = now(), refused_at = now(), \
                     version = nextval('hugging_face_account_version') WHERE version = $1",
                )
                .bind(version)
                .execute(&mut *tx)
                .await?
            }
        };
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "hugging-face check", self, first).await?;
        Ok(Ok(()))
    }

    /// **An admin's disconnect**: the row read at `version` removed, its
    /// sealed token with it.
    pub async fn hugging_face_disconnect(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let written = sqlx::query("DELETE FROM hugging_face_account WHERE version = $1")
            .bind(version)
            .execute(&mut *tx)
            .await?;
        if written.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        commit_with_outcome(tx, "hugging-face disconnect", self, first).await?;
        Ok(Ok(()))
    }
}
