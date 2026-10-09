//! conforms: web-one-live-row-per-box-and-name
//! conforms: web-client-credential-stored-as-fingerprint-never-key
//! conforms: web-link-state-is-reset-when-the-listener-starts
//! conforms: web-agent-present-only-when-both-planes-match-one-row
//! conforms: web-one-live-connection-per-credential
//! conforms: web-tuple-is-admins-word-and-never-gate-cons
//!
//! The register of agents (Spec section 2.12) at the store: the reads, the
//! register verbs' writes under section 3.2's version, and the link's
//! writes of what it observed under section 2.12's arrival sequence. The
//! two writers meet at disjoint members of one row, per section 3, and
//! every path that installs or uninstalls a live connection runs under the
//! row's lock, per the module header.

use crate::link::frames::{Plane, Refusal};
use crate::store::{AgentId, Store};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::postgres::PgRow;
use sqlx::{Connection, Row};

/// The channel a revoking transaction notifies on, with the fingerprint as
/// its payload, so a running listener closes the live connection in the
/// revoking act (Spec 8) rather than at the next hello.
pub const REVOCATION_CHANNEL: &str = "weaver_web_credential";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Live,
    Revoked,
}

impl CredentialState {
    fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "live" => Ok(Self::Live),
            "revoked" => Ok(Self::Revoked),
            other => anyhow::bail!("not a credential state: {other}"),
        }
    }
}

/// One plane's credential and link state as the row carries them.
#[derive(Debug, Clone, Serialize)]
pub struct Credential {
    pub fingerprint: String,
    /// The fingerprint of the authority that signed it (Spec 8).
    pub authority: String,
    pub state: CredentialState,
    pub state_at: DateTime<Utc>,
    pub connected: bool,
    pub link_at: Option<DateTime<Utc>>,
    pub incarnation: Option<i64>,
    pub address: Option<String>,
    pub address_at: Option<DateTime<Utc>>,
}

/// The registered agent of Spec 2.12.
#[derive(Debug, Clone, Serialize)]
pub struct Agent {
    pub agent_id: AgentId,
    pub name: String,
    #[serde(rename = "box")]
    pub r#box: String,
    pub author: Option<String>,
    pub version: i64,
    pub registered_at: DateTime<Utc>,
    pub gate: Credential,
    pub admin: Credential,
    pub tuple: Option<serde_json::Value>,
    pub tuple_at: Option<DateTime<Utc>>,
    pub load_state: Option<String>,
    pub load_state_at: Option<DateTime<Utc>>,
    /// Which source the load state stands on: `show` or `event` (Spec
    /// 2.12). Its date is the load state's.
    pub state_source: Option<String>,
    /// Which source the tuple stands on: `show` or `event` (Spec 2.12),
    /// and so which shape the opaque tuple has. Its date is the tuple's: a
    /// turn's start or close moves the load state and leaves both.
    pub tuple_source: Option<String>,
    /// The ceiling admin-con last declared, with its date (Spec 2.12, 8):
    /// the copy surfaces read, never the authorization input.
    pub ceiling: Option<Vec<String>>,
    pub ceiling_at: Option<DateTime<Utc>>,
    /// The run's constituents as the last `show` named them, by process
    /// id, with that answer's date (Spec 2.12): a fact for the operator and
    /// for the install's one-time containment check, which drives nothing
    /// in admin-con. `None` where the answer named none.
    pub constituents: Option<Vec<i32>>,
    pub constituents_at: Option<DateTime<Utc>>,
    /// The trace door's state, open or closed, with its date (Spec 2.12):
    /// admin-con's word at admission and at every change after. Written by
    /// the act that builds the relay client.
    pub trace_door: Option<bool>,
    pub trace_door_at: Option<DateTime<Utc>>,
}

impl Agent {
    /// **Presence is derived and never stored** (Spec 8): both planes
    /// connected, from credentials on this row.
    pub fn present(&self) -> bool {
        self.gate.connected && self.admin.connected
    }

    pub fn credential(&self, plane: Plane) -> &Credential {
        match plane {
            Plane::Gate => &self.gate,
            Plane::Admin => &self.admin,
        }
    }

    /// Which plane a fingerprint is bound to on this row, if either.
    pub fn plane_of(&self, fingerprint: &str) -> Option<Plane> {
        if self.gate.fingerprint == fingerprint {
            Some(Plane::Gate)
        } else if self.admin.fingerprint == fingerprint {
            Some(Plane::Admin)
        } else {
            None
        }
    }
}

// A test's lever on the one outcome a verb cannot see: the next commit of
// the named kind is applied and then reported as an error, as a connection
// lost between the commit and its answer would.
#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_AFTER_COMMIT: std::cell::Cell<Option<&'static str>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn fail_after_commit(kind: &'static str) -> anyhow::Result<()> {
    let armed = FAIL_AFTER_COMMIT.with(|f| {
        if f.get() == Some(kind) {
            f.set(None);
            true
        } else {
            false
        }
    });
    if armed {
        anyhow::bail!("a test fault lost the commit's answer ({kind})");
    }
    Ok(())
}

/// The authority lock held: dropping it ends the session that holds the
/// advisory lock, which releases it. **A verb that holds it runs its store
/// work on this connection**, so a session PostgreSQL dropped kills the
/// verb's transaction rather than letting the verb continue on the pool
/// while another process holds the lock, and pings it before its file
/// switch.
/// **An admin's session asking a register verb through the server**: the
/// session and its person, re-checked inside the verb's transaction.
#[derive(Debug, Clone, Copy)]
pub struct AdminWrite<'a> {
    pub session_id: i64,
    pub person: &'a str,
}

/// **The authority of an admin's register verb no longer stands**: the
/// session, its person or their admin grant went between the request's read
/// and the verb's transaction. Nothing was written.
#[derive(Debug)]
pub struct AuthorityGone;

impl std::fmt::Display for AuthorityGone {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the session or the admin grant that authorized this no longer stands"
        )
    }
}

impl std::error::Error for AuthorityGone {}

/// **The agent's row holds no live credential**, read inside an admin's
/// rotation under the row's lock: it was retired since the request
/// resolved it, and a rotation would make it live again. Nothing was
/// written.
#[derive(Debug)]
pub struct Retired;

impl std::fmt::Display for Retired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the agent's row holds no live credential")
    }
}

impl std::error::Error for Retired {}

impl AdminWrite<'_> {
    /// The identity exclusion taken shared for the rest of the
    /// transaction, then the admin's authority re-checked under it.
    async fn stands(&self, tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> anyhow::Result<()> {
        sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
            .bind(crate::store::identity::IDENTITY_LOCK_KEY)
            .execute(&mut **tx)
            .await?;
        if !crate::store::admin::admin_stands(tx, self.session_id, self.person).await? {
            return Err(AuthorityGone.into());
        }
        #[cfg(test)]
        crate::store::admin::hold_inside(self.person).await;
        Ok(())
    }
}

pub struct AuthorityLock {
    connection: sqlx::PgConnection,
}

impl AuthorityLock {
    /// The lock's own connection, for the verb's store work.
    pub fn connection(&mut self) -> &mut sqlx::PgConnection {
        &mut self.connection
    }

    /// The session still holds the lock, or the verb aborts naming it.
    /// **The window between this ping and the caller's next step is one
    /// round trip and is accepted**: a session lost inside it cannot be
    /// told from one lost a moment later, and the next verb's lock finds
    /// whatever state the switch left.
    pub async fn ping(&mut self) -> anyhow::Result<()> {
        sqlx::query("SELECT 1")
            .execute(&mut self.connection)
            .await
            .map(|_| ())
            .map_err(|e| {
                anyhow::anyhow!(
                    "the session holding the authority lock was lost ({e}); another verb may hold it now, so this one stops"
                )
            })
    }
}

/// What the link observed and lands on the row (Spec 2.12): the load state
/// and the tuple from the agent's side, a `show` answer (admin's word) or a
/// trace event (the agent's own record), with that source's date.
#[derive(Debug, Clone)]
pub struct Observation {
    pub load_state: Option<String>,
    pub tuple: TupleWrite,
    pub at: DateTime<Utc>,
    /// `show` or `event` (Spec 2.12): which source this observation is.
    pub source: &'static str,
    /// What the observation does to the run's constituents.
    pub constituents: ConstituentsWrite,
}

/// What an observation does to the row's constituents (Spec 2.12). **Only
/// a `show` answer names them**, so a trace event keeps the ones the row
/// holds, with their date; a `show` answer writes them, `None` where it
/// named none, which is where no run holds the agent's run lock.
#[derive(Debug, Clone)]
pub enum ConstituentsWrite {
    Write(Option<Vec<i32>>),
    Keep,
}

/// What an observation does to the row's tuple. **A turn's start or close
/// says nothing of the tuple**, so it keeps the one the row holds, with
/// that one's date and source; a `load`, an `unload` or a `show` answer
/// writes it, `None` where the source names none, and its source with it.
#[derive(Debug, Clone)]
pub enum TupleWrite {
    Write(Option<serde_json::Value>),
    Keep,
}

const COLUMNS: &str = "agent_id, name, box, author, version, registered_at, \
    gate_fingerprint, gate_authority, gate_state, gate_state_at, gate_connected, gate_link_at, \
    gate_incarnation, gate_address, gate_address_at, \
    admin_fingerprint, admin_authority, admin_state, admin_state_at, admin_connected, admin_link_at, \
    admin_incarnation, admin_address, admin_address_at, \
    tuple, tuple_at, tuple_source, load_state, load_state_at, state_source, admin_ceiling, \
    admin_ceiling_at, constituents, constituents_at, trace_door, trace_door_at";

fn credential_from_row(row: &PgRow, plane: &str) -> anyhow::Result<Credential> {
    let col = |s: &str| format!("{plane}_{s}");
    Ok(Credential {
        fingerprint: row.try_get(col("fingerprint").as_str())?,
        authority: row.try_get(col("authority").as_str())?,
        state: CredentialState::parse(row.try_get::<String, _>(col("state").as_str())?.as_str())?,
        state_at: row.try_get(col("state_at").as_str())?,
        connected: row.try_get(col("connected").as_str())?,
        link_at: row.try_get(col("link_at").as_str())?,
        incarnation: row.try_get(col("incarnation").as_str())?,
        address: row.try_get(col("address").as_str())?,
        address_at: row.try_get(col("address_at").as_str())?,
    })
}

fn agent_from_row(row: &PgRow) -> anyhow::Result<Agent> {
    let id: String = row.try_get("agent_id")?;
    Ok(Agent {
        agent_id: id.parse().map_err(|e: String| anyhow::anyhow!(e))?,
        name: row.try_get("name")?,
        r#box: row.try_get("box")?,
        author: row.try_get("author")?,
        version: row.try_get("version")?,
        registered_at: row.try_get("registered_at")?,
        gate: credential_from_row(row, "gate")?,
        admin: credential_from_row(row, "admin")?,
        tuple: row.try_get("tuple")?,
        tuple_at: row.try_get("tuple_at")?,
        load_state: row.try_get("load_state")?,
        load_state_at: row.try_get("load_state_at")?,
        state_source: row.try_get("state_source")?,
        tuple_source: row.try_get("tuple_source")?,
        ceiling: row.try_get("admin_ceiling")?,
        ceiling_at: row.try_get("admin_ceiling_at")?,
        constituents: row.try_get("constituents")?,
        constituents_at: row.try_get("constituents_at")?,
        trace_door: row.try_get("trace_door")?,
        trace_door_at: row.try_get("trace_door_at")?,
    })
}

fn plane_columns(plane: Plane) -> &'static str {
    plane.as_str()
}

/// **The one audit this file owes sqlx.** Every dynamic SQL string below
/// interpolates exactly two things: a plane's name, which is one of the two
/// static strings `Plane::as_str` answers and never operator input, and the
/// constant column list above. Every value crosses as a bind parameter.
fn audited(sql: String) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(sql)
}

impl Store {
    /// One registered agent by identity.
    pub async fn agent(&self, id: &AgentId) -> anyhow::Result<Option<Agent>> {
        let row = sqlx::query(audited(format!(
            "SELECT {COLUMNS} FROM agent WHERE agent_id = $1"
        )))
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(agent_from_row).transpose()
    }

    /// Every registered agent, live or retired, by box and name then by
    /// registration. The Agents surface renders the whole row; presence is
    /// derived on each.
    pub async fn agents(&self) -> anyhow::Result<Vec<Agent>> {
        let rows = sqlx::query(audited(format!(
            "SELECT {COLUMNS} FROM agent ORDER BY box, name, registered_at"
        )))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(agent_from_row).collect()
    }

    /// The row a fingerprint is bound to, with its plane, live or not. The
    /// hello's lookup (Spec 8), made before any byte of the roster is read.
    pub async fn agent_by_fingerprint(
        &self,
        fingerprint: &str,
    ) -> anyhow::Result<Option<(Agent, Plane)>> {
        let row = sqlx::query(audited(format!(
            "SELECT {COLUMNS} FROM agent WHERE gate_fingerprint = $1 OR admin_fingerprint = $1"
        )))
        .bind(fingerprint)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let agent = agent_from_row(&row)?;
        let plane = agent
            .plane_of(fingerprint)
            .ok_or_else(|| anyhow::anyhow!("a row matched a fingerprint it does not carry"))?;
        Ok(Some((agent, plane)))
    }

    /// Resolve an operator's spelling of an agent: its identity, or
    /// `box/name` for the row holding live credentials under that pair.
    pub async fn resolve_agent(&self, spec: &str) -> anyhow::Result<Agent> {
        if let Ok(id) = spec.parse::<AgentId>() {
            return self
                .agent(&id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("no registered agent {id}"));
        }
        let Some((r#box, name)) = spec.split_once('/') else {
            anyhow::bail!("name the agent as ag-<sixteen hex> or as box/name: {spec}");
        };
        let row = sqlx::query(audited(format!(
            "SELECT {COLUMNS} FROM agent WHERE box = $1 AND name = $2 \
             AND (gate_state = 'live' OR admin_state = 'live')"
        )))
        .bind(r#box)
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref()
            .map(agent_from_row)
            .transpose()?
            .ok_or_else(|| anyhow::anyhow!("no registered agent with live credentials at {spec}"))
    }

    /// **Register an agent** (Spec 8, 2.12): write the row with both
    /// fingerprints. A live row for the same box and name is retired in the
    /// same transaction, its credentials revoked and the running listener
    /// told, which is what the partial unique index forces. Answers the new
    /// identity and the fingerprints retired.
    pub async fn register_agent(
        &self,
        r#box: &str,
        name: &str,
        author: Option<&str>,
        gate_fingerprint: &str,
        admin_fingerprint: &str,
        authority: &str,
    ) -> anyhow::Result<(AgentId, Vec<String>)> {
        let mut conn = self.pool.acquire().await?;
        let id = Self::mint_agent_id_on(&mut conn).await?;
        Self::register_agent_on(
            &mut conn,
            &id,
            r#box,
            name,
            author,
            gate_fingerprint,
            admin_fingerprint,
            authority,
        )
        .await
    }

    /// **A new row's identity, minted by the store** through the function
    /// every kind's identity comes from, before the row is written: the
    /// register verbs stage each client config with the identity it will
    /// name before the store commits (Spec 8).
    pub async fn mint_agent_id_on(conn: &mut sqlx::PgConnection) -> anyhow::Result<AgentId> {
        let id: String = sqlx::query_scalar("SELECT weaver_key('ag')")
            .fetch_one(&mut *conn)
            .await?;
        id.parse().map_err(|e: String| anyhow::anyhow!(e))
    }

    /// The same on a given connection, under an identity minted first: a
    /// verb that holds the authority lock runs its store work on the lock's
    /// own session, so a lost session kills the transaction with it.
    // The row's members arrive as the verb holds them: eight arguments,
    // each one a column of the one insert the lock orders.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_agent_on(
        conn: &mut sqlx::PgConnection,
        id: &AgentId,
        r#box: &str,
        name: &str,
        author: Option<&str>,
        gate_fingerprint: &str,
        admin_fingerprint: &str,
        authority: &str,
    ) -> anyhow::Result<(AgentId, Vec<String>)> {
        Self::register_agent_in(conn, None, id, r#box, name, author, authority, || {
            Ok((gate_fingerprint.to_owned(), admin_fingerprint.to_owned()))
        })
        .await
    }

    /// **The same, asked by an admin through the server** (Spec 2.13):
    /// inside the register's own transaction the identity exclusion is
    /// held shared, from the admin's re-check to the commit, so a
    /// revocation or a disable either commits before the check and is seen,
    /// or waits for the commit. A grant or session gone answers
    /// `AuthorityGone` and nothing is written. **The credentials are minted
    /// only once the re-check passes**: `fingerprints` mints them and
    /// answers the two fingerprints, called inside the transaction after it.
    pub async fn register_agent_by_admin_on(
        conn: &mut sqlx::PgConnection,
        admin: AdminWrite<'_>,
        id: &AgentId,
        r#box: &str,
        name: &str,
        authority: &str,
        fingerprints: impl FnOnce() -> anyhow::Result<(String, String)>,
    ) -> anyhow::Result<(AgentId, Vec<String>)> {
        Self::register_agent_in(
            conn,
            Some(admin),
            id,
            r#box,
            name,
            Some(admin.person),
            authority,
            fingerprints,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn register_agent_in(
        conn: &mut sqlx::PgConnection,
        admin: Option<AdminWrite<'_>>,
        id: &AgentId,
        r#box: &str,
        name: &str,
        author: Option<&str>,
        authority: &str,
        fingerprints: impl FnOnce() -> anyhow::Result<(String, String)>,
    ) -> anyhow::Result<(AgentId, Vec<String>)> {
        let mut tx = conn.begin().await?;
        if let Some(admin) = admin {
            admin.stands(&mut tx).await?;
        }
        let (gate_fingerprint, admin_fingerprint) = fingerprints()?;
        // **The retire is ordered on the previous row's version** like every
        // register verb (Spec 8): the live row is read under the lock and
        // the update names the version it read.
        let previous = sqlx::query(
            "SELECT agent_id, version, gate_fingerprint, admin_fingerprint FROM agent \
             WHERE box = $1 AND name = $2 AND (gate_state = 'live' OR admin_state = 'live') \
             FOR UPDATE",
        )
        .bind(r#box)
        .bind(name)
        .fetch_optional(&mut *tx)
        .await?;
        let mut retired_fingerprints = Vec::new();
        if let Some(row) = previous {
            let id: String = row.try_get("agent_id")?;
            let version: i64 = row.try_get("version")?;
            let affected = sqlx::query(
                "UPDATE agent SET \
                   gate_state = 'revoked', gate_state_at = now(), \
                   admin_state = 'revoked', admin_state_at = now(), \
                   gate_link_at = CASE WHEN gate_connected THEN now() ELSE gate_link_at END, \
                   admin_link_at = CASE WHEN admin_connected THEN now() ELSE admin_link_at END, \
                   gate_connected = false, admin_connected = false, \
                   gate_incarnation = NULL, admin_incarnation = NULL, \
                   author = $3, version = version + 1 \
                 WHERE agent_id = $1 AND version = $2",
            )
            .bind(&id)
            .bind(version)
            .bind(author)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if affected != 1 {
                anyhow::bail!("the row {id} moved since it was read at version {version}");
            }
            for col in ["gate_fingerprint", "admin_fingerprint"] {
                let fp: String = row.try_get(col)?;
                notify(&mut tx, &fp).await?;
                retired_fingerprints.push(fp);
            }
        }
        let id: String = sqlx::query_scalar(
            "INSERT INTO agent (agent_id, name, box, author, gate_fingerprint, admin_fingerprint, \
             gate_authority, admin_authority) \
             VALUES ($7, $1, $2, $3, $4, $5, $6, $6) RETURNING agent_id",
        )
        .bind(name)
        .bind(r#box)
        .bind(author)
        .bind(gate_fingerprint)
        .bind(admin_fingerprint)
        .bind(authority)
        .bind(id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        #[cfg(test)]
        fail_after_commit("register")?;
        Ok((
            id.parse().map_err(|e: String| anyhow::anyhow!(e))?,
            retired_fingerprints,
        ))
    }

    /// **Revoke one credential** (Spec 8): an authored edit under section
    /// 3.2, refusing on a stale version, and the running listener told so
    /// the live connection closes in this act. The plane's link state reads
    /// disconnected with this act's date.
    pub async fn revoke_credential(
        &self,
        agent: &Agent,
        plane: Plane,
        author: Option<&str>,
    ) -> anyhow::Result<String> {
        let p = plane_columns(plane);
        let mut tx = self.pool.begin().await?;
        lock_row(&mut tx, &agent.agent_id).await?;
        let affected = sqlx::query(audited(format!(
            "UPDATE agent SET \
               {p}_state = 'revoked', {p}_state_at = now(), \
               {p}_link_at = CASE WHEN {p}_connected THEN now() ELSE {p}_link_at END, \
               {p}_connected = false, {p}_incarnation = NULL, \
               author = $3, version = version + 1 \
             WHERE agent_id = $1 AND version = $2"
        )))
        .bind(agent.agent_id.as_str())
        .bind(agent.version)
        .bind(author)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            anyhow::bail!(
                "the row moved since it was read at version {}: read it again",
                agent.version
            );
        }
        let fingerprint = agent.credential(plane).fingerprint.clone();
        notify(&mut tx, &fingerprint).await?;
        tx.commit().await?;
        Ok(fingerprint)
    }

    /// **Rotate both credentials** (Spec 8): new fingerprints, both live,
    /// both planes dropped until the install script carries the new config,
    /// the old two revoked and the listener told. Answers the fingerprints
    /// retired.
    pub async fn rotate_credentials(
        &self,
        agent: &Agent,
        author: Option<&str>,
        gate_fingerprint: &str,
        admin_fingerprint: &str,
        authority: &str,
    ) -> anyhow::Result<Vec<String>> {
        let mut conn = self.pool.acquire().await?;
        Self::rotate_credentials_on(
            &mut conn,
            agent,
            author,
            gate_fingerprint,
            admin_fingerprint,
            authority,
        )
        .await
    }

    /// The same on a given connection: a verb that holds the authority
    /// lock runs its store work on the lock's own session, so a lost session
    /// kills the transaction with it.
    pub async fn rotate_credentials_on(
        conn: &mut sqlx::PgConnection,
        agent: &Agent,
        author: Option<&str>,
        gate_fingerprint: &str,
        admin_fingerprint: &str,
        authority: &str,
    ) -> anyhow::Result<Vec<String>> {
        Self::rotate_credentials_in(conn, None, agent, author, authority, || {
            Ok((gate_fingerprint.to_owned(), admin_fingerprint.to_owned()))
        })
        .await
    }

    /// **The same, asked by an admin through the server**, the identity
    /// exclusion held shared from the admin's re-check to the commit, as
    /// `register_agent_by_admin_on` holds it. **Every fact the request
    /// resolved before the lock is re-checked inside the transaction**: the
    /// row is read again under its own lock, and a row holding no live
    /// credential, retired since the request read it, answers `Retired`
    /// rather than being made live again. The credentials are minted only
    /// once both re-checks pass, by `fingerprints`, and the retired
    /// fingerprints and the version are the row's as read here.
    pub async fn rotate_credentials_by_admin_on(
        conn: &mut sqlx::PgConnection,
        admin: AdminWrite<'_>,
        agent: &Agent,
        authority: &str,
        fingerprints: impl FnOnce() -> anyhow::Result<(String, String)>,
    ) -> anyhow::Result<Vec<String>> {
        Self::rotate_credentials_in(
            conn,
            Some(admin),
            agent,
            Some(admin.person),
            authority,
            fingerprints,
        )
        .await
    }

    async fn rotate_credentials_in(
        conn: &mut sqlx::PgConnection,
        admin: Option<AdminWrite<'_>>,
        agent: &Agent,
        author: Option<&str>,
        authority: &str,
        fingerprints: impl FnOnce() -> anyhow::Result<(String, String)>,
    ) -> anyhow::Result<Vec<String>> {
        let mut tx = conn.begin().await?;
        if let Some(admin) = admin {
            admin.stands(&mut tx).await?;
        }
        lock_row(&mut tx, &agent.agent_id).await?;
        let (version, retired) = if admin.is_some() {
            let row = sqlx::query(
                "SELECT version, gate_state, admin_state, gate_fingerprint, admin_fingerprint \
                 FROM agent WHERE agent_id = $1",
            )
            .bind(agent.agent_id.as_str())
            .fetch_one(&mut *tx)
            .await?;
            let live = |col: &str| -> anyhow::Result<bool> {
                Ok(row.try_get::<String, _>(col)? == "live")
            };
            if !live("gate_state")? && !live("admin_state")? {
                return Err(Retired.into());
            }
            (
                row.try_get::<i64, _>("version")?,
                vec![
                    row.try_get::<String, _>("gate_fingerprint")?,
                    row.try_get::<String, _>("admin_fingerprint")?,
                ],
            )
        } else {
            (
                agent.version,
                vec![
                    agent.gate.fingerprint.clone(),
                    agent.admin.fingerprint.clone(),
                ],
            )
        };
        let (gate_fingerprint, admin_fingerprint) = fingerprints()?;
        let affected = sqlx::query(
            "UPDATE agent SET \
               gate_fingerprint = $3, gate_state = 'live', gate_state_at = now(), \
               admin_fingerprint = $4, admin_state = 'live', admin_state_at = now(), \
               gate_authority = $6, admin_authority = $6, \
               gate_link_at = CASE WHEN gate_connected THEN now() ELSE gate_link_at END, \
               admin_link_at = CASE WHEN admin_connected THEN now() ELSE admin_link_at END, \
               gate_connected = false, admin_connected = false, \
               gate_incarnation = NULL, admin_incarnation = NULL, \
               author = $5, version = version + 1 \
             WHERE agent_id = $1 AND version = $2",
        )
        .bind(agent.agent_id.as_str())
        .bind(version)
        .bind(&gate_fingerprint)
        .bind(&admin_fingerprint)
        .bind(author)
        .bind(authority)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if affected != 1 {
            anyhow::bail!("the row moved since it was read at version {version}: read it again");
        }
        for fp in &retired {
            notify(&mut tx, fp).await?;
        }
        tx.commit().await?;
        #[cfg(test)]
        fail_after_commit("rotate")?;
        Ok(retired)
    }

    /// **Every live credential revoked**, which is what rotating the
    /// authority means (Spec 8): each client config pinned the old
    /// certificate, so each agent is re-registered after. Answers how many
    /// rows were retired.
    pub async fn revoke_every_credential(&self, author: Option<&str>) -> anyhow::Result<u64> {
        let mut conn = self.pool.acquire().await?;
        Self::revoke_every_credential_on(&mut conn, author).await
    }

    /// The same on a given connection: a verb that holds the authority
    /// lock runs its store work on the lock's own session, so a lost session
    /// kills the transaction with it.
    pub async fn revoke_every_credential_on(
        conn: &mut sqlx::PgConnection,
        author: Option<&str>,
    ) -> anyhow::Result<u64> {
        let mut tx = conn.begin().await?;
        let rows = sqlx::query(
            "UPDATE agent SET \
               gate_state = 'revoked', gate_state_at = now(), \
               admin_state = 'revoked', admin_state_at = now(), \
               gate_link_at = CASE WHEN gate_connected THEN now() ELSE gate_link_at END, \
               admin_link_at = CASE WHEN admin_connected THEN now() ELSE admin_link_at END, \
               gate_connected = false, admin_connected = false, \
               gate_incarnation = NULL, admin_incarnation = NULL, \
               author = $1, version = version + 1 \
             WHERE gate_state = 'live' OR admin_state = 'live' \
             RETURNING gate_fingerprint, admin_fingerprint",
        )
        .bind(author)
        .fetch_all(&mut *tx)
        .await?;
        for row in &rows {
            for col in ["gate_fingerprint", "admin_fingerprint"] {
                let fp: String = row.try_get(col)?;
                notify(&mut tx, &fp).await?;
            }
        }
        tx.commit().await?;
        #[cfg(test)]
        fail_after_commit("revoke_every")?;
        Ok(rows.len() as u64)
    }

    /// **The reconciliation that makes a lost lock session harmless rather
    /// than merely unlikely** (Spec 8): after the authority is switched, every
    /// live credential whose recorded authority is not the new one is
    /// revoked, with the listener told, and the count answered, which is
    /// zero unless a registration raced the rotation.
    pub async fn revoke_credentials_not_signed_by_on(
        conn: &mut sqlx::PgConnection,
        authority: &str,
        author: Option<&str>,
    ) -> anyhow::Result<u64> {
        let mut tx = conn.begin().await?;
        let mut count = 0u64;
        for plane in ["gate", "admin"] {
            let rows = sqlx::query(audited(format!(
                "UPDATE agent SET \
                   {plane}_state = 'revoked', {plane}_state_at = now(), \
                   {plane}_link_at = CASE WHEN {plane}_connected THEN now() ELSE {plane}_link_at END, \
                   {plane}_connected = false, {plane}_incarnation = NULL, \
                   author = $2, version = version + 1 \
                 WHERE {plane}_state = 'live' AND {plane}_authority <> $1 \
                 RETURNING {plane}_fingerprint AS fingerprint"
            )))
            .bind(authority)
            .bind(author)
            .fetch_all(&mut *tx)
            .await?;
            for row in &rows {
                let fp: String = row.try_get("fingerprint")?;
                notify(&mut tx, &fp).await?;
                count += 1;
            }
        }
        tx.commit().await?;
        Ok(count)
    }

    /// The live fingerprints as they stand, on the given connection: what
    /// a rotation's revocation is about to select, captured so a lost
    /// answer can be read back against that set and not against whatever
    /// is live later.
    pub async fn live_fingerprints_on(
        conn: &mut sqlx::PgConnection,
    ) -> anyhow::Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT gate_fingerprint, gate_state, admin_fingerprint, admin_state FROM agent \
             WHERE gate_state = 'live' OR admin_state = 'live'",
        )
        .fetch_all(conn)
        .await?;
        let mut live = Vec::new();
        for row in &rows {
            if row.try_get::<String, _>("gate_state")? == "live" {
                live.push(row.try_get("gate_fingerprint")?);
            }
            if row.try_get::<String, _>("admin_state")? == "live" {
                live.push(row.try_get("admin_fingerprint")?);
            }
        }
        Ok(live)
    }

    /// Whether any of the given fingerprints is still live, which a
    /// rotation reads back where its revocation's answer was lost: one left
    /// live means the commit did not land, and a credential registered in
    /// between is another fingerprint and does not confuse the answer.
    pub async fn any_live_among(&self, fingerprints: &[String]) -> anyhow::Result<bool> {
        let live: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM agent \
             WHERE (gate_fingerprint = ANY($1) AND gate_state = 'live') \
                OR (admin_fingerprint = ANY($1) AND admin_state = 'live'))",
        )
        .bind(fingerprints)
        .fetch_one(&self.pool)
        .await?;
        Ok(live)
    }

    /// **The listener's start** (Spec 8, 2.12), in one transaction:
    /// increment the epoch, and set every plane recorded as connected to
    /// disconnected with the start's date, leaving an already-disconnected
    /// plane's date alone. Unconditional and not an observation. Answers
    /// the epoch this process lands under.
    pub async fn listener_start(&self) -> anyhow::Result<i64> {
        let mut tx = self.pool.begin().await?;
        let epoch: i64 = sqlx::query_scalar(
            "UPDATE listener SET epoch = epoch + 1, started_at = now() RETURNING epoch",
        )
        .fetch_one(&mut *tx)
        .await?;
        for plane in ["gate", "admin"] {
            sqlx::query(audited(format!(
                "UPDATE agent SET {plane}_connected = false, {plane}_link_at = now(), \
                 {plane}_incarnation = NULL WHERE {plane}_connected"
            )))
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(epoch)
    }

    /// **Admission under the row's lock** (Spec 8): recheck that the
    /// credential is live, let the listener install the connection as the
    /// credential's live one (which refuses when one is already installed),
    /// and write the plane's link state connected with the incarnation and
    /// the observed address. The install runs while the lock is held, so a
    /// revocation either waits for this to commit and then closes what it
    /// finds installed, or committed first and the recheck refuses.
    // The row's identity, the connection's four facts, and the two hooks
    // that run under the row's lock: nine arguments, each one the lock
    // orders against the others.
    #[allow(clippy::too_many_arguments)]
    pub async fn admit(
        &self,
        agent_id: &AgentId,
        plane: Plane,
        fingerprint: &str,
        incarnation: i64,
        address: &str,
        ceiling: Option<&[String]>,
        door: Option<bool>,
        prove: impl AsyncFnOnce() -> Result<(), Refusal>,
        install: impl FnOnce() -> Result<(), Refusal>,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let p = plane_columns(plane);
        let mut tx = self.pool.begin().await?;
        // **The recheck is of the credential this connection presented**,
        // its fingerprint as well as the plane's state: after a rotation the
        // plane is live again for the new fingerprint, and a connection on
        // the old one that passed the handshake's lookup before the
        // rotation committed must still be refused here.
        let row = sqlx::query(audited(format!(
            "SELECT {p}_state AS state, {p}_fingerprint AS fingerprint FROM agent \
             WHERE agent_id = $1 FOR UPDATE"
        )))
        .bind(agent_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        let state: String = row.try_get("state")?;
        let bound: String = row.try_get("fingerprint")?;
        if state != "live" || bound != fingerprint {
            tx.rollback().await?;
            return Ok(Err(Refusal::NotLive));
        }
        // **Admission is coupled to ownership of the listener's lock**:
        // the caller proves it here, under the row's lock and before the
        // install, so nothing is admitted by a listener whose lock session
        // is gone. A refusal drops the transaction, which rolls back.
        if let Err(refusal) = prove().await {
            return Ok(Err(refusal));
        }
        if let Err(refusal) = install() {
            tx.rollback().await?;
            return Ok(Err(refusal));
        }
        sqlx::query(audited(format!(
            "UPDATE agent SET {p}_connected = true, {p}_link_at = now(), {p}_incarnation = $2, \
             {p}_address = $3, {p}_address_at = now() WHERE agent_id = $1"
        )))
        .bind(agent_id.as_str())
        .bind(incarnation)
        .bind(address)
        .execute(&mut *tx)
        .await?;
        // **The ceiling's copy lands with the admission** (Spec 2.12, 8),
        // under the same lock, so the row's copy is the ceiling of the
        // connection the row records connected. Surfaces read it; the
        // listener authorizes against the live connection's own.
        if let Some(ceiling) = ceiling {
            sqlx::query(
                "UPDATE agent SET admin_ceiling = $2, admin_ceiling_at = now() WHERE agent_id = $1",
            )
            .bind(agent_id.as_str())
            .bind(ceiling)
            .execute(&mut *tx)
            .await?;
        }
        // **The trace door's state lands with the admission too** (Spec
        // 2.12, 7.2): admin-con's word in its hello, dated at admission.
        if let Some(door) = door {
            sqlx::query(
                "UPDATE agent SET trace_door = $2, trace_door_at = now() WHERE agent_id = $1",
            )
            .bind(agent_id.as_str())
            .bind(door)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        #[cfg(test)]
        fail_after_commit("admit")?;
        Ok(Ok(()))
    }

    /// **A change of the trace door's state, bound to the incarnation**
    /// (Spec 2.12, 7.2): admin-con's word on its live connection, with its
    /// date, landing only while this incarnation is the row's live admin
    /// connection, as a link-state write does. Answers whether it landed.
    pub async fn land_door(
        &self,
        agent_id: &AgentId,
        incarnation: i64,
        open: bool,
        at: DateTime<Utc>,
    ) -> anyhow::Result<bool> {
        let landed = sqlx::query(audited(
            "UPDATE agent SET trace_door = $3, trace_door_at = $4 \
             WHERE agent_id = $1 AND admin_incarnation = $2"
                .to_owned(),
        ))
        .bind(agent_id.as_str())
        .bind(incarnation)
        .bind(open)
        .bind(at)
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(landed == 1)
    }

    /// **Teardown under the row's lock, bound to the incarnation** (Spec
    /// 8): the disconnected write lands only while this incarnation is
    /// still the credential's live connection, and the listener's
    /// uninstall runs under the same lock. Answers whether the write
    /// landed; a stale teardown answers false and changes nothing.
    pub async fn teardown(
        &self,
        agent_id: &AgentId,
        plane: Plane,
        incarnation: i64,
        uninstall: impl FnOnce(),
    ) -> anyhow::Result<bool> {
        let p = plane_columns(plane);
        let mut tx = self.pool.begin().await?;
        let live: Option<i64> = sqlx::query_scalar(audited(format!(
            "SELECT {p}_incarnation FROM agent WHERE agent_id = $1 FOR UPDATE"
        )))
        .bind(agent_id.as_str())
        .fetch_one(&mut *tx)
        .await?;
        // The uninstall runs under the lock whatever the row says, so the
        // live map never holds an entry the row no longer names.
        uninstall();
        if live != Some(incarnation) {
            tx.rollback().await?;
            return Ok(false);
        }
        sqlx::query(audited(format!(
            "UPDATE agent SET {p}_connected = false, {p}_link_at = now(), {p}_incarnation = NULL \
             WHERE agent_id = $1"
        )))
        .bind(agent_id.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// **The authority lock** (Spec 8): a session-level advisory lock on
    /// `AUTHORITY_LOCK_KEY`, taken on a connection of its own and released
    /// when the guard drops, held by every verb that mints a credential or
    /// replaces the authority across its store transaction and its file
    /// switch. Blocks until the lock is free, so such verbs run one at a
    /// time.
    pub async fn authority_lock(&self) -> anyhow::Result<AuthorityLock> {
        let mut connection = self.pool.acquire().await?.detach();
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(crate::link::listener::AUTHORITY_LOCK_KEY)
            .execute(&mut connection)
            .await?;
        Ok(AuthorityLock { connection })
    }

    /// **Run a closure under the row's lock and nothing else**: the cleanup
    /// the listener owes after a store error on a path that already
    /// installed, so the live map is mutated under the same exclusion as
    /// every other path.
    pub async fn with_row_lock(&self, agent_id: &AgentId, f: impl FnOnce()) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        lock_row(&mut tx, agent_id).await?;
        f();
        tx.commit().await?;
        Ok(())
    }

    /// **Land an observation** (Spec 2.12): the tuple and the load state as
    /// admin reported them, each taking this observation only where its
    /// arrival sequence (epoch, arrival) is higher than the stored one. The
    /// source date is kept for display and decides nothing. Answers whether
    /// the row took it.
    pub async fn land_observation(
        &self,
        agent_id: &AgentId,
        observation: &Observation,
        epoch: i64,
        arrival: i64,
    ) -> anyhow::Result<bool> {
        let affected = sqlx::query(
            "UPDATE agent SET \
               load_state = $2, load_state_at = $3, load_epoch = $4, load_arrival = $5, \
               tuple = CASE WHEN $8 THEN tuple ELSE $6 END, \
               tuple_at = CASE WHEN $8 THEN tuple_at ELSE $3 END, \
               tuple_epoch = CASE WHEN $8 THEN tuple_epoch ELSE $4 END, \
               tuple_arrival = CASE WHEN $8 THEN tuple_arrival ELSE $5 END, \
               tuple_source = CASE WHEN $8 THEN tuple_source ELSE $7 END, \
               constituents = CASE WHEN $9 THEN constituents ELSE $10 END, \
               constituents_at = CASE WHEN $9 THEN constituents_at ELSE $3 END, \
               state_source = $7 \
             WHERE agent_id = $1 \
               AND (load_epoch IS NULL OR (load_epoch, load_arrival) < ($4, $5)) \
               AND (tuple_epoch IS NULL OR (tuple_epoch, tuple_arrival) < ($4, $5))",
        )
        .bind(agent_id.as_str())
        .bind(&observation.load_state)
        .bind(observation.at)
        .bind(epoch)
        .bind(arrival)
        .bind(match &observation.tuple {
            TupleWrite::Write(tuple) => tuple.clone(),
            TupleWrite::Keep => None,
        })
        .bind(observation.source)
        .bind(matches!(observation.tuple, TupleWrite::Keep))
        .bind(matches!(observation.constituents, ConstituentsWrite::Keep))
        .bind(match &observation.constituents {
            ConstituentsWrite::Write(pids) => pids.clone(),
            ConstituentsWrite::Keep => None,
        })
        .execute(&self.pool)
        .await?
        .rows_affected();
        Ok(affected == 1)
    }
}

async fn lock_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    agent_id: &AgentId,
) -> anyhow::Result<()> {
    sqlx::query("SELECT 1 FROM agent WHERE agent_id = $1 FOR UPDATE")
        .bind(agent_id.as_str())
        .fetch_one(&mut **tx)
        .await?;
    Ok(())
}

async fn notify(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    fingerprint: &str,
) -> anyhow::Result<()> {
    sqlx::query("SELECT pg_notify($1, $2)")
        .bind(REVOCATION_CHANNEL)
        .bind(fingerprint)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
