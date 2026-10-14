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

/// A string cut to at most `bound` bytes, at a character boundary.
pub fn bounded(text: &str, bound: usize) -> String {
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
/// `save_point_missed` (the decision prompt of ruling 2) or `data_at_risk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Failed,
    Unknown,
    Refused,
    LostContact,
    SavePointMissed,
    DataAtRisk,
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
    pub async fn raise_alert(&self, raise: &Raise) -> anyhow::Result<String> {
        let id: String = sqlx::query_scalar(
            "INSERT INTO alert (agent_id, kind, subject, source, summary, detail, \
             first_record, options) VALUES ($1, $2, $3, 'server', $4, $5, $6, $7) \
             ON CONFLICT (agent_id, kind, subject) WHERE acknowledged_at IS NULL \
             DO UPDATE SET last_at = now(), occurrences = alert.occurrences + 1, \
             version = alert.version + 1, summary = EXCLUDED.summary, \
             detail = EXCLUDED.detail, first_record = EXCLUDED.first_record, \
             options = EXCLUDED.options, restored_at = NULL \
             RETURNING alert_id",
        )
        .bind(raise.agent.as_str())
        .bind(raise.kind.as_str())
        .bind(bounded(&raise.subject, 64))
        .bind(bounded(&raise.summary, SUMMARY_BOUND))
        .bind(bounded(&raise.detail, DETAIL_BOUND))
        .bind(&raise.first)
        .bind(&raise.options)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
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
