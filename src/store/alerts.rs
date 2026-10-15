//! conforms: web-one-open-alert-per-agent-kind-and-subject
//! conforms: web-an-acknowledgement-names-the-version-it-read
//!
//! **Alerts, and the operator's decisions** (Spec 2.14; act 12, PR 1;
//! migration `0019`): what the server raises on an agent when an act fails,
//! its outcome is unknown, it is refused for a system reason, contact with a
//! connector is lost, or a failure could lose data, which last may carry the
//! options a person chooses among. Everyone holding a grant on the agent
//! sees its alerts; one stays until a person holding `operator` on the agent
//! acknowledges it, naming the version they read.
//!
//! **One open alert per agent, kind and subject**: a repeat updates the open
//! one through an insert that resolves its conflict on the partial unique
//! index, so concurrent raisers leave one alert with its count right.
//! **An alert never holds a turn's text** or a secret; its summary and
//! detail are bounded here as the table bounds them.

use sqlx::{Postgres, Transaction};

use crate::store::{AgentId, Store};

/// The bounds the table checks, applied before the write so a long reason
/// is cut rather than refused.
const SUMMARY_BOUND: usize = 1024;
const DETAIL_BOUND: usize = 8192;

/// A string cut to at most `bound` bytes, at a character boundary, **a NUL
/// replaced** by U+FFFD: PostgreSQL's text holds none, and a reason that
/// carries one, an entry's message among them, would otherwise fail its
/// write at every try.
pub fn bounded(text: &str, bound: usize) -> String {
    let text = &text.replace('\0', "\u{fffd}");
    if text.len() <= bound {
        return text.to_owned();
    }
    let mut end = bound;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// What an alert is about: `failed`, `unknown`, `refused`, `lost_contact`,
/// `save_point_missed` (the decision prompt of ruling 2), `data_at_risk`, or
/// `data_lost` (a loss that already happened, migration `0020`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Failed,
    Unknown,
    Refused,
    LostContact,
    SavePointMissed,
    DataAtRisk,
    DataLost,
}

impl AlertKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertKind::Failed => "failed",
            AlertKind::Unknown => "unknown",
            AlertKind::Refused => "refused",
            AlertKind::LostContact => "lost_contact",
            AlertKind::SavePointMissed => "save_point_missed",
            AlertKind::DataAtRisk => "data_at_risk",
            AlertKind::DataLost => "data_lost",
        }
    }
}

/// An alert to raise, or to repeat where one stands open.
#[derive(Debug, Clone)]
pub struct Raise {
    pub agent: AgentId,
    pub kind: AlertKind,
    /// The plane for lost contact, the act (a verb, or `turn`) otherwise.
    pub subject: String,
    pub summary: String,
    pub detail: String,
    /// The first record of the act it concerns, where there is one.
    pub first: Option<String>,
    /// A decision prompt's options.
    pub options: Vec<String>,
    /// **Where it came from** (migration `0020`): `server`, `admin` (the
    /// box's `admin.log`), `admin-con`, `gate-con` or `trace`.
    pub source: &'static str,
    /// **Raised once ever, never repeated or reopened**: a loss that
    /// already happened, named by its subject, read again from a replay
    /// or a backfill of the trace raises nothing.
    pub once: bool,
}

/// **Raise an alert, or repeat the open one**, on the connection given: a
/// pool's for a raise of its own, a transaction's for an inbox row's.
async fn raise_in(conn: &mut sqlx::PgConnection, raise: &Raise) -> anyhow::Result<Option<String>> {
    if raise.once {
        return raise_once_in(conn, raise).await;
    }
    let id: String = sqlx::query_scalar(
        "INSERT INTO alert (agent_id, kind, subject, source, summary, detail, \
         first_record, options) VALUES ($1, $2, $3, $8, $4, $5, $6, $7) \
         ON CONFLICT (agent_id, kind, subject) WHERE acknowledged_at IS NULL \
         DO UPDATE SET last_at = now(), occurrences = alert.occurrences + 1, \
         version = alert.version + 1, summary = EXCLUDED.summary, \
         detail = EXCLUDED.detail, first_record = EXCLUDED.first_record, \
         options = EXCLUDED.options, source = EXCLUDED.source, restored_at = NULL \
         RETURNING alert_id",
    )
    .bind(raise.agent.as_str())
    .bind(raise.kind.as_str())
    .bind(bounded(&raise.subject, 64))
    .bind(bounded(&raise.summary, SUMMARY_BOUND))
    .bind(bounded(&raise.detail, DETAIL_BOUND))
    .bind(&raise.first)
    .bind(&raise.options)
    .bind(raise.source)
    .fetch_one(&mut *conn)
    .await?;
    Ok(Some(id))
}

/// **An alert raised once ever** for its agent, kind and subject: none where
/// one stands, open or acknowledged, and one of two raisers at once resolved
/// by the open alert's index. `None` where nothing was raised.
async fn raise_once_in(
    conn: &mut sqlx::PgConnection,
    raise: &Raise,
) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "INSERT INTO alert (agent_id, kind, subject, source, summary, detail, \
         first_record, options) \
         SELECT $1, $2, $3, $8, $4, $5, $6, $7 WHERE NOT EXISTS ( \
           SELECT 1 FROM alert WHERE agent_id = $1 AND kind = $2 AND subject = $3) \
         ON CONFLICT (agent_id, kind, subject) WHERE acknowledged_at IS NULL DO NOTHING \
         RETURNING alert_id",
    )
    .bind(raise.agent.as_str())
    .bind(raise.kind.as_str())
    .bind(bounded(&raise.subject, 64))
    .bind(bounded(&raise.summary, SUMMARY_BOUND))
    .bind(bounded(&raise.detail, DETAIL_BOUND))
    .bind(&raise.first)
    .bind(&raise.options)
    .bind(raise.source)
    .fetch_optional(&mut *conn)
    .await?)
}

/// **An entry as the door took it**, for the inbox.
pub struct TakenEntry<'a> {
    pub run: &'a str,
    pub seq: u64,
    pub agent: &'a AgentId,
    pub plane: &'a str,
    pub source: &'a str,
    pub entry: &'a serde_json::Value,
}

/// **An inbox row claimed**, its transaction holding the claim.
pub struct Claimed {
    tx: Transaction<'static, Postgres>,
    pub row: InboxRow,
}

impl Claimed {
    /// **The row's alert, raised and kept on its row in the claim's
    /// transaction**: never committed without its record, so a retry raises
    /// it no second time. `fail_record` is a test's lever: the record fails
    /// after the raise.
    pub async fn raise(
        &mut self,
        raise: &Raise,
        fail_record: bool,
    ) -> anyhow::Result<Option<String>> {
        let id = raise_in(&mut self.tx, raise).await?;
        if fail_record {
            anyhow::bail!("a test fault failed the row's record of its alert");
        }
        if let Some(id) = &id {
            sqlx::query("UPDATE entry_inbox SET alert_id = $2 WHERE inbox_id = $1")
                .bind(self.row.inbox_id)
                .bind(id)
                .execute(&mut *self.tx)
                .await?;
        }
        Ok(id)
    }

    /// The row applied, its line written: gone, and the claim committed.
    pub async fn done(mut self) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM entry_inbox WHERE inbox_id = $1")
            .bind(self.row.inbox_id)
            .execute(&mut *self.tx)
            .await?;
        self.tx.commit().await?;
        Ok(())
    }

    /// The row's line not written: backed off, with any alert it raised
    /// kept, and the claim committed.
    pub async fn failed(mut self) -> anyhow::Result<()> {
        sqlx::query(BACK_OFF)
            .bind(self.row.inbox_id)
            .execute(&mut *self.tx)
            .await?;
        self.tx.commit().await?;
        Ok(())
    }
}

/// A row's next try, its wait doubling from a second to five minutes.
const BACK_OFF: &str = "UPDATE entry_inbox SET attempts = attempts + 1, \
     next_try = now() + make_interval(secs => least(power(2, attempts), 300)) \
     WHERE inbox_id = $1";

/// **An inbox row**: an entry taken and not yet applied.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboxRow {
    pub inbox_id: i64,
    pub run_id: String,
    pub seq: i64,
    pub agent_id: String,
    pub plane: String,
    pub source: String,
    /// The entry's JSON, as the door's parser rendered it.
    pub entry: String,
    pub alert_id: Option<String>,
}

/// An alert as the pages read it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Alert {
    pub alert_id: String,
    pub agent_id: String,
    pub kind: String,
    pub subject: String,
    pub source: String,
    pub summary: String,
    pub detail: String,
    pub first_record: Option<String>,
    pub options: Vec<String>,
    pub raised_at: chrono::DateTime<chrono::Utc>,
    pub last_at: chrono::DateTime<chrono::Utc>,
    pub occurrences: i32,
    pub restored_at: Option<chrono::DateTime<chrono::Utc>>,
    pub version: i64,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub acknowledged_by: Option<String>,
}

impl Store {
    /// **Raise an alert, or repeat the open one**: one statement, an insert
    /// resolving its conflict on the open alert's index, so two raisers at
    /// once leave one alert whose count holds both. A repeat takes the
    /// latest reason, moves the version, and clears a restoration, contact
    /// being lost again. Answers the alert's identity.
    pub async fn raise_alert(&self, raise: &Raise) -> anyhow::Result<Option<String>> {
        let mut conn = self.pool.acquire().await?;
        raise_in(&mut conn, raise).await
    }

    /// **Take a connector's entry once, durably** (Spec 2.14, migration
    /// `0020`): in one transaction, the run's mark moves to the sequence
    /// where it is past the highest taken, and the entry's row enters the
    /// inbox, so an entry acknowledged after this is durable here. True
    /// where it was taken; false for a resend. A run is the agent's and
    /// the plane's it was first met on, and an entry of it from another is
    /// taken as a resend, never as new.
    pub async fn take_entry(&self, taken: &TakenEntry<'_>) -> anyhow::Result<bool> {
        let seq = i64::try_from(taken.seq).map_err(|_| anyhow::anyhow!("a sequence past i64"))?;
        let mut tx = self.pool.begin().await?;
        let moved: Option<i32> = sqlx::query_scalar(
            "INSERT INTO entry_run (run_id, agent_id, plane, last_seq) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (run_id) DO UPDATE SET last_seq = EXCLUDED.last_seq, last_at = now() \
             WHERE entry_run.last_seq < EXCLUDED.last_seq \
             AND entry_run.agent_id = EXCLUDED.agent_id AND entry_run.plane = EXCLUDED.plane \
             RETURNING 1",
        )
        .bind(taken.run)
        .bind(taken.agent.as_str())
        .bind(taken.plane)
        .bind(seq)
        .fetch_optional(&mut *tx)
        .await?;
        if moved.is_none() {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO entry_inbox (run_id, seq, agent_id, plane, source, entry) \
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(taken.run)
        .bind(seq)
        .bind(taken.agent.as_str())
        .bind(taken.plane)
        .bind(taken.source)
        .bind(taken.entry.to_string())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// **Claim the inbox's next row due a try**, fairly (by when each is
    /// due, a fresh row's take or a failed row's backed-off try, so a row
    /// that failed is never chosen ahead of one not tried since), **in a
    /// transaction of its own that holds the claim**: `FOR UPDATE SKIP
    /// LOCKED`, so no two drainers, of one process or two, ever apply one
    /// row. The claim ends with the transaction: committed with the row's
    /// effects, or rolled back where the drainer fails or dies, the row then
    /// due again.
    pub async fn claim_entry(&self) -> anyhow::Result<Option<Claimed>> {
        let mut tx = self.pool.begin().await?;
        let row: Option<InboxRow> = sqlx::query_as(
            "SELECT inbox_id, run_id, seq, agent_id, plane, source, entry, alert_id \
             FROM entry_inbox WHERE next_try <= now() ORDER BY next_try, inbox_id \
             LIMIT 1 FOR UPDATE SKIP LOCKED",
        )
        .fetch_optional(&mut *tx)
        .await?;
        Ok(row.map(|row| Claimed { tx, row }))
    }

    /// **A row whose effect failed** is tried again later, its wait
    /// doubling from a second to five minutes, never deleted.
    pub async fn inbox_failed(&self, inbox_id: i64) -> anyhow::Result<()> {
        sqlx::query(BACK_OFF)
            .bind(inbox_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// **Contact restored**: the open lost-contact alert on the plane, if
    /// any, notes when, and stays until acknowledged.
    pub async fn alert_contact_restored(&self, agent: &AgentId, plane: &str) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE alert SET restored_at = now(), version = version + 1 \
             WHERE agent_id = $1 AND kind = 'lost_contact' AND subject = $2 \
             AND acknowledged_at IS NULL AND restored_at IS NULL",
        )
        .bind(agent.as_str())
        .bind(plane)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The agent's alerts, open ones first, then the most recent
    /// acknowledged, newest first within each, at most `limit`.
    pub async fn alerts_on(&self, agent: &AgentId, limit: i64) -> anyhow::Result<Vec<Alert>> {
        Ok(sqlx::query_as(
            "SELECT alert_id, agent_id, kind, subject, source, summary, detail, first_record, \
             options, raised_at, last_at, occurrences, restored_at, version, acknowledged_at, \
             acknowledged_by FROM alert WHERE agent_id = $1 \
             ORDER BY (acknowledged_at IS NULL) DESC, last_at DESC LIMIT $2",
        )
        .bind(agent.as_str())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// One alert by identity.
    pub async fn alert(&self, alert: &str) -> anyhow::Result<Option<Alert>> {
        Ok(sqlx::query_as(
            "SELECT alert_id, agent_id, kind, subject, source, summary, detail, first_record, \
                 options, raised_at, last_at, occurrences, restored_at, version, acknowledged_at, \
                 acknowledged_by FROM alert WHERE alert_id = $1",
        )
        .bind(alert)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// **The open alerts on each agent the person holds a live grant on**,
    /// an enabled person's, by agent; nothing of any other agent.
    pub async fn open_alerts_by_agent(&self, person: &str) -> anyhow::Result<Vec<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT a.agent_id, count(*) FROM alert a \
             WHERE a.acknowledged_at IS NULL AND a.agent_id IN ( \
               SELECT g.agent_id FROM role_grant g JOIN person p ON p.person_id = g.person_id \
               WHERE g.person_id = $1 AND g.agent_id IS NOT NULL AND g.revoked_at IS NULL \
               AND p.enabled) \
             GROUP BY a.agent_id",
        )
        .bind(person)
        .fetch_all(&self.pool)
        .await?)
    }
}

/// **A plane of an agent the server cannot reach** (Spec 2.14, the
/// unavailable banner): state derived from the register at the read, never
/// a record.
#[derive(Debug, Clone)]
pub struct Unavailable {
    pub agent_id: String,
    /// The agent's box and name.
    pub label: String,
    /// `admin` or `gate`.
    pub plane: String,
    /// When the plane's link state last changed, where the row knows it.
    pub since: Option<chrono::DateTime<chrono::Utc>>,
}

impl Store {
    /// **The planes the person cannot reach on the agents they hold a live
    /// grant on**, an enabled person's, any role: a plane whose credential
    /// is live and whose link is not connected, a connector that never
    /// connected since registration included. A retired agent's planes hold
    /// no live credential and show none; nothing of any other agent.
    pub async fn unavailable_for(&self, person: &str) -> anyhow::Result<Vec<Unavailable>> {
        let rows: Vec<(
            String,
            String,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
        )> = sqlx::query_as(
            "SELECT a.agent_id, a.box || '/' || a.name, p.plane, p.since \
                 FROM agent a CROSS JOIN LATERAL (VALUES \
                   ('admin', a.admin_state = 'live' AND NOT a.admin_connected, a.admin_link_at), \
                   ('gate', a.gate_state = 'live' AND NOT a.gate_connected, a.gate_link_at) \
                 ) AS p (plane, down, since) \
                 WHERE p.down AND a.agent_id IN ( \
                   SELECT g.agent_id FROM role_grant g JOIN person e ON e.person_id = g.person_id \
                   WHERE g.person_id = $1 AND g.agent_id IS NOT NULL AND g.revoked_at IS NULL \
                   AND e.enabled) \
                 ORDER BY a.box, a.name, p.plane",
        )
        .bind(person)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(agent_id, label, plane, since)| Unavailable {
                agent_id,
                label,
                plane,
                since,
            })
            .collect())
    }
}

/// **The acknowledgement's write**, in the caller's transaction: lands only
/// on the open alert at the version the person read, and answers whether it
/// did. A repeat or a restoration since that read moved the version, and the
/// acknowledgement is refused rather than landing on what was not seen.
pub async fn acknowledge_in(
    tx: &mut Transaction<'_, Postgres>,
    alert: &str,
    version: i64,
    person: &str,
) -> anyhow::Result<bool> {
    let landed = sqlx::query(
        "UPDATE alert SET acknowledged_at = now(), acknowledged_by = $3, version = version + 1 \
         WHERE alert_id = $1 AND version = $2 AND acknowledged_at IS NULL",
    )
    .bind(alert)
    .bind(version)
    .bind(person)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(landed == 1)
}
