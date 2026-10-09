//! **The identity rows and the identity exclusion** (Spec 2.13,
//! `docs/project/design-2026-10-07-iam.md` sections 6 to 10): persons,
//! their enrollment tokens and passkeys, the roles and the grants, as the
//! host's identity commands write them (`src/host.rs`).
//!
//! **Every identity write runs under the identity exclusion**: one
//! transaction-level advisory lock on a key of its own, taken exclusively by
//! each write before it reads what it checks, so two writes never interleave
//! and a rule counted under it, the last enabled admin above all, cannot be
//! raced. The shared form, which every authorized act takes, is the
//! authorization pull request's.
//!
//! **No secret is stored**: a token is printed once and kept as its digest.

use crate::store::Store;
use crate::store::key::shaped;
use caseless::Caseless;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use unicode_normalization::UnicodeNormalization;

/// The identity exclusion's advisory key, beside the listener's and the
/// authority's: one key for every identity write in the store.
pub const IDENTITY_LOCK_KEY: i64 = i64::from_be_bytes(*b"weaverid");

/// **A role's verbs are drawn from this vocabulary alone**: the verbs a box
/// rule may grant (Spec 7.2) and `turn`.
pub const VOCABULARY: [&str; 9] = [
    "show",
    "validate",
    "load",
    "unload",
    "stop",
    "save-point",
    "restore",
    "force-unload",
    "turn",
];

/// **An enrollment token's lifetime never exceeds seven days** (design
/// section 7), refused at the config's load and at every issue.
pub const TOKEN_LIFETIME_MAX_HOURS: u32 = 7 * 24;

/// The most bytes a name may run to, the store's key bound.
pub const NAME_BOUND: usize = 1024;

/// **Whether `spec` is a person's identity**: exactly `pe-` and sixteen
/// lowercase hex, as an agent's is read. Anything else is a name.
///
/// **An identity a request submits is parsed into its exact shape at the
/// surface's boundary, before any record**, by these four helpers, one per
/// kind: a malformed one is refused there as the ask's fault and written
/// nowhere, so a request's text never becomes an audit target, and the
/// targets the audit checks by shape (a person's, a grant's) never turn an
/// ordinary malformed request into the server's failure. A well-shaped
/// identity naming nothing goes on to the store, which answers that no such
/// row stands.
pub fn is_person_id(spec: &str) -> bool {
    shaped("pe-", spec)
}

/// Whether `spec` is a passkey's identity: exactly `pk-` and sixteen
/// lowercase hex.
pub fn is_passkey_id(spec: &str) -> bool {
    shaped("pk-", spec)
}

/// Whether `spec` is a grant's identity: exactly `gr-` and sixteen
/// lowercase hex.
pub fn is_grant_id(spec: &str) -> bool {
    shaped("gr-", spec)
}

/// Whether `spec` is an agent's identity: exactly `ag-` and sixteen
/// lowercase hex, as `AgentId` parses one.
pub fn is_agent_id(spec: &str) -> bool {
    shaped("ag-", spec)
}

/// **A name as given, trimmed of surrounding white space**, or why it is
/// refused: empty, past the bound, holding a NUL, which the store refuses
/// in text, or **of an identity's own shape**, which would shadow the
/// person whose identity it spells wherever a person is named.
pub fn given_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if is_person_id(name) {
        return Err(format!(
            "{name} is a person's identity in shape, and a name never takes one, so no name shadows an identity"
        ));
    }
    if name.is_empty() {
        return Err("a person's name is empty".into());
    }
    if name.len() > NAME_BOUND {
        return Err(format!(
            "a person's name runs past {NAME_BOUND} bytes ({})",
            name.len()
        ));
    }
    if name.contains('\0') {
        return Err("a person's name holds a NUL byte, which the store refuses".into());
    }
    Ok(name.to_owned())
}

/// **A name's canonical form** (design section 6): Unicode's compatibility
/// caseless form of the Unicode Standard section 3.13, D146,
/// `NFKD(casefold(NFKD(casefold(NFD(name)))))`, with the full case folding,
/// on the name trimmed of surrounding white space. Two names are one where
/// their forms are equal, so a case, width or composition variant of a name
/// is the same name.
pub fn name_key(name: &str) -> String {
    name.trim()
        .chars()
        .nfd()
        .default_case_fold()
        .nfkd()
        .default_case_fold()
        .nfkd()
        .collect()
}

/// **A bearer the server issues**: 32 bytes from the operating system's
/// cryptographic random source (design section 6), never derived from a
/// counter, a time or a row's identity.
pub fn bearer() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system's random source answers");
    bytes
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A bearer's digest, the one form the store keeps.
pub fn digest(bearer: &str) -> String {
    hex(&Sha256::digest(bearer.as_bytes()))
}

/// **An enrollment token as issued**: its value, printed once and never
/// stored, and when it expires.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    pub value: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// A person as the commands read them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub person_id: String,
    pub name: String,
    pub enabled: bool,
}

/// **An author member, resolved** for a surface to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Author {
    /// A person, by their identity, with their current name.
    Person { person_id: String, name: String },
    /// A name that resolves to no person: a claim written before persons
    /// stood, which reads as a claim.
    Claim(String),
    /// No author could be named when the row was written.
    Absent,
}

/// A role as the commands read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub scope: String,
    pub verbs: Vec<String>,
    /// The version read, which a write of the role carries (Spec 3.2).
    pub version: i64,
}

/// The ending an issued token gives the live token it replaces.
#[derive(Debug, Clone, Copy)]
pub enum Supersedes {
    /// A newer token issued for the person.
    Issue,
    /// The host reset.
    Reset,
}

/// **What makes a token redeemable**, over `enrollment_token t` joined to
/// `person p`: unexpired and not ended, its person enabled and holding no
/// passkey. One definition, read at a ceremony's start and again under the
/// exclusion at its finish.
const REDEEMABLE: &str = "t.ended_at IS NULL AND t.expires_at > now() AND p.enabled \
     AND NOT EXISTS (SELECT 1 FROM passkey k WHERE k.person_id = t.person_id)";

/// **One of a person's own passkeys**, as their page shows it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct OwnPasskey {
    pub passkey_id: String,
    pub label: Option<String>,
    pub added_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// **What an addition came to.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// The passkey is inserted.
    Inserted,
    /// The session that asked no longer stands, or the passkey whose
    /// assertion earned the grant was removed. Nothing was written.
    AuthorityGone,
    /// Another passkey holds the credential ID. Nothing was written.
    CredentialHeld,
}

/// **What a removal came to.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    /// The passkey is removed.
    Removed,
    /// The passkey is not the person's, or no longer stands.
    NotTheirs,
    /// It is the person's last, which is never removed.
    Last,
    /// The session that asked no longer stands. Nothing was written.
    AuthorityGone,
}

/// **The rule for every write a session authorizes** (Spec 2.13): inside
/// the write's own transaction, under the identity exclusion, the session is
/// re-checked as standing (open, its person enabled, the passkey it was
/// opened with still the person's), so a disable, a removal or a host reset
/// committed after the request read its session refuses the write rather
/// than landing beneath it.
pub async fn session_stands(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    person_id: &str,
) -> anyhow::Result<bool> {
    let standing: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM session s \
         JOIN person p ON p.person_id = s.person_id \
         JOIN passkey k ON k.passkey_id = s.passkey_id AND k.person_id = s.person_id \
         WHERE s.session_id = $1 AND s.person_id = $2 AND s.closed_at IS NULL AND p.enabled",
    )
    .bind(session_id)
    .bind(person_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(standing.is_some())
}

/// **A passkey that earned a write still stands**, its person's, read in
/// the write's transaction under the identity exclusion.
pub async fn passkey_stands(
    tx: &mut Transaction<'_, Postgres>,
    passkey_id: &str,
    person_id: &str,
) -> anyhow::Result<bool> {
    let standing: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM passkey WHERE passkey_id = $1 AND person_id = $2")
            .bind(passkey_id)
            .bind(person_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(standing.is_some())
}

/// A hold after a removal's count, keyed by the passkey's identity, one of
/// a list so tests running at once each keep their own: the removal signals
/// `read` and waits on `release`, which lets a test start a second removal.
#[cfg(test)]
pub(crate) type RemoveHold = (
    String,
    std::sync::Arc<tokio::sync::Notify>,
    std::sync::Arc<tokio::sync::Notify>,
);

#[cfg(test)]
pub(crate) static REMOVE_HOLD: std::sync::Mutex<Vec<RemoveHold>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
async fn hold_after_the_count(passkey_id: &str) {
    let hold = {
        let mut holds = REMOVE_HOLD.lock().unwrap();
        holds
            .iter()
            .position(|(key, ..)| key == passkey_id)
            .map(|at| holds.remove(at))
    };
    if let Some((_, read, release)) = hold {
        read.notify_one();
        release.notified().await;
    }
}

/// **What a redemption came to.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Redeemed {
    /// The passkey is enrolled and the token ended `redeemed`.
    Enrolled,
    /// The token no longer redeems: expired, ended, its person disabled or
    /// now holding a passkey. Nothing was written.
    TokenRefused,
    /// Another passkey holds the credential ID. Nothing was written.
    CredentialHeld,
}

impl Store {
    /// **A transaction holding the identity exclusion**, exclusively, until
    /// it commits or rolls back.
    pub async fn identity_transaction(&self) -> anyhow::Result<Transaction<'static, Postgres>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(IDENTITY_LOCK_KEY)
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    /// **The person a token can enroll a passkey for**, by the token's
    /// digest: their identity and name, where the token is unexpired and
    /// has not ended, and its person is enabled and holds no passkey (Spec
    /// 2.13: a token registers a person's first passkey and nothing else).
    pub async fn redeemable(&self, token_digest: &str) -> anyhow::Result<Option<(String, String)>> {
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT p.person_id, p.name FROM enrollment_token t \
             JOIN person p ON p.person_id = t.person_id \
             WHERE t.token_digest = $1 AND {REDEEMABLE}"
        )))
        .bind(token_digest)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// **A token redeemed** (design section 7): in one transaction under the
    /// identity exclusion, the token checked again as [`Self::redeemable`]
    /// checks it and bound to the same person, the credential ID found held
    /// by no passkey, the passkey inserted under the identity `passkey_id`,
    /// and the token ended `redeemed`. A check that fails writes nothing.
    pub async fn redeem(
        &self,
        token_digest: &str,
        person_id: &str,
        passkey_id: &str,
        credential_id: &str,
        credential: &serde_json::Value,
    ) -> anyhow::Result<Redeemed> {
        let mut tx = self.identity_transaction().await?;
        let still: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT p.person_id FROM enrollment_token t \
             JOIN person p ON p.person_id = t.person_id \
             WHERE t.token_digest = $1 AND t.person_id = $2 AND {REDEEMABLE}"
        )))
        .bind(token_digest)
        .bind(person_id)
        .fetch_optional(&mut *tx)
        .await?;
        if still.is_none() {
            return Ok(Redeemed::TokenRefused);
        }
        let held: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM passkey WHERE credential_id = $1")
                .bind(credential_id)
                .fetch_optional(&mut *tx)
                .await?;
        if held.is_some() {
            return Ok(Redeemed::CredentialHeld);
        }
        sqlx::query(
            "INSERT INTO passkey (passkey_id, credential_id, person_id, credential) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(passkey_id)
        .bind(credential_id)
        .bind(person_id)
        .bind(credential)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE enrollment_token SET ended_at = now(), ended = 'redeemed' \
             WHERE token_digest = $1 AND ended_at IS NULL",
        )
        .bind(token_digest)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Redeemed::Enrolled)
    }

    /// **A person's own passkeys**, oldest first, for their page: the
    /// identity, the label, when each was added and last used.
    pub async fn own_passkeys(&self, person_id: &str) -> anyhow::Result<Vec<OwnPasskey>> {
        Ok(sqlx::query_as(
            "SELECT passkey_id, label, added_at, last_used_at FROM passkey \
             WHERE person_id = $1 ORDER BY added_at, passkey_id",
        )
        .bind(person_id)
        .fetch_all(&self.pool)
        .await?)
    }

    /// **A passkey added by its person** (design section 7), after the
    /// fresh assertion's grant was consumed: in one transaction under the
    /// identity exclusion, the person found enabled, the credential ID found
    /// held by no passkey, and the passkey inserted with its label. A check
    /// that fails writes nothing.
    #[allow(clippy::too_many_arguments)]
    pub async fn add_passkey(
        &self,
        person_id: &str,
        session_id: i64,
        earned_by: &str,
        passkey_id: &str,
        credential_id: &str,
        credential: &serde_json::Value,
        label: Option<&str>,
    ) -> anyhow::Result<Added> {
        let mut tx = self.identity_transaction().await?;
        if !session_stands(&mut tx, session_id, person_id).await?
            || !passkey_stands(&mut tx, earned_by, person_id).await?
        {
            return Ok(Added::AuthorityGone);
        }
        let held: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM passkey WHERE credential_id = $1")
                .bind(credential_id)
                .fetch_optional(&mut *tx)
                .await?;
        if held.is_some() {
            return Ok(Added::CredentialHeld);
        }
        sqlx::query(
            "INSERT INTO passkey (passkey_id, credential_id, person_id, credential, label) \
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(passkey_id)
        .bind(credential_id)
        .bind(person_id)
        .bind(credential)
        .bind(label)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Added::Inserted)
    }

    /// **A passkey removed by its person, never their last** (design
    /// section 7): in one transaction under the identity exclusion, the
    /// passkey found theirs and their passkeys counted, so two concurrent
    /// removals of a person's last two cannot both land. Every session
    /// opened with it ends at its next use, the session's own check seeing
    /// the passkey gone; nothing here writes a session.
    pub async fn remove_passkey(
        &self,
        person_id: &str,
        session_id: i64,
        passkey_id: &str,
    ) -> anyhow::Result<Removed> {
        let mut tx = self.identity_transaction().await?;
        if !session_stands(&mut tx, session_id, person_id).await? {
            return Ok(Removed::AuthorityGone);
        }
        let held: i64 = sqlx::query_scalar("SELECT count(*) FROM passkey WHERE person_id = $1")
            .bind(person_id)
            .fetch_one(&mut *tx)
            .await?;
        let theirs: Option<i32> =
            sqlx::query_scalar("SELECT 1 FROM passkey WHERE passkey_id = $1 AND person_id = $2")
                .bind(passkey_id)
                .bind(person_id)
                .fetch_optional(&mut *tx)
                .await?;
        #[cfg(test)]
        hold_after_the_count(passkey_id).await;
        if theirs.is_none() {
            return Ok(Removed::NotTheirs);
        }
        if held <= 1 {
            return Ok(Removed::Last);
        }
        sqlx::query("DELETE FROM passkey WHERE passkey_id = $1 AND person_id = $2")
            .bind(passkey_id)
            .bind(person_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Removed::Removed)
    }

    /// A new key of `kind`, minted by the store and written nowhere yet.
    pub async fn mint_key(&self, kind: &str) -> anyhow::Result<String> {
        Ok(sqlx::query_scalar("SELECT weaver_key($1)")
            .bind(kind)
            .fetch_one(&self.pool)
            .await?)
    }

    /// The person `spec` names: their identity (`pe-`), or their name in
    /// its canonical form.
    pub async fn person(&self, spec: &str) -> anyhow::Result<Option<Person>> {
        let row = if is_person_id(spec) {
            sqlx::query("SELECT person_id, name, enabled FROM person WHERE person_id = $1")
                .bind(spec)
                .fetch_optional(&self.pool)
                .await?
        } else {
            sqlx::query("SELECT person_id, name, enabled FROM person WHERE name_key = $1")
                .bind(name_key(spec))
                .fetch_optional(&self.pool)
                .await?
        };
        Ok(row.map(|r| Person {
            person_id: r.get("person_id"),
            name: r.get("name"),
            enabled: r.get("enabled"),
        }))
    }

    /// The role named `name`.
    /// **What an authored row's author member names**, as a surface renders
    /// it (Spec 3.2, design section 6): a person's identity resolves to the
    /// person's current name, since a person can be renamed and the member
    /// holds the identity; a member that resolves to no person is a claim
    /// written before persons stood, rendered as a claim; and a null names
    /// no author.
    pub async fn author(&self, member: Option<&str>) -> anyhow::Result<Author> {
        let Some(member) = member else {
            return Ok(Author::Absent);
        };
        if is_person_id(member)
            && let Some(name) =
                sqlx::query_scalar::<_, String>("SELECT name FROM person WHERE person_id = $1")
                    .bind(member)
                    .fetch_optional(&self.pool)
                    .await?
        {
            return Ok(Author::Person {
                person_id: member.to_owned(),
                name,
            });
        }
        Ok(Author::Claim(member.to_owned()))
    }

    pub async fn role(&self, name: &str) -> anyhow::Result<Option<Role>> {
        Ok(
            sqlx::query("SELECT name, scope, verbs, version FROM role WHERE name = $1")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?
                .map(|r| Role {
                    name: r.get("name"),
                    scope: r.get("scope"),
                    verbs: r.get("verbs"),
                    version: r.get("version"),
                }),
        )
    }

    /// The live grant binding a person to a role on an agent, or
    /// server-wide where `agent` is `None`, with its version.
    pub async fn live_grant(
        &self,
        person: &str,
        role: &str,
        agent: Option<&str>,
    ) -> anyhow::Result<Option<(String, i64)>> {
        Ok(sqlx::query_as(
            "SELECT grant_id, version FROM role_grant WHERE person_id = $1 AND role = $2 \
             AND agent_id IS NOT DISTINCT FROM $3 AND revoked_at IS NULL",
        )
        .bind(person)
        .bind(role)
        .bind(agent)
        .fetch_optional(&self.pool)
        .await?)
    }
}

/// Whether a person row with this canonical name stands.
pub async fn name_taken(tx: &mut Transaction<'_, Postgres>, key: &str) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM person WHERE name_key = $1)")
            .bind(key)
            .fetch_one(&mut **tx)
            .await?,
    )
}

/// Write the person row.
pub async fn insert_person(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
    name: &str,
    author: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO person (person_id, name, name_key, author) VALUES ($1, $2, $3, $4)")
        .bind(person)
        .bind(name)
        .bind(name_key(name))
        .bind(author)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Write a live grant.
pub async fn insert_grant(
    tx: &mut Transaction<'_, Postgres>,
    grant: &str,
    person: &str,
    role: &str,
    agent: Option<&str>,
    author: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO role_grant (grant_id, person_id, role, agent_id, author) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(grant)
    .bind(person)
    .bind(role)
    .bind(agent)
    .bind(author)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Whether the person holds a passkey, read under the exclusion.
pub async fn holds_passkey(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM passkey WHERE person_id = $1)")
            .bind(person)
            .fetch_one(&mut **tx)
            .await?,
    )
}

/// Clear the person's passkeys, answering how many there were.
pub async fn clear_passkeys(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
) -> anyhow::Result<u64> {
    Ok(sqlx::query("DELETE FROM passkey WHERE person_id = $1")
        .bind(person)
        .execute(&mut **tx)
        .await?
        .rows_affected())
}

/// **Issue an enrollment token** for the person: a fresh bearer, its digest
/// written beside its expiry, the person's earlier live token ended first,
/// and the value answered once, to be printed and never stored.
pub async fn issue_token(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
    hours: u32,
    supersedes: Supersedes,
) -> anyhow::Result<IssuedToken> {
    if hours == 0 || hours > TOKEN_LIFETIME_MAX_HOURS {
        anyhow::bail!(
            "an enrollment token lives between 1 and {TOKEN_LIFETIME_MAX_HOURS} hours, not {hours}"
        );
    }
    let ended = match supersedes {
        Supersedes::Issue => "superseded",
        Supersedes::Reset => "reset",
    };
    sqlx::query(
        "UPDATE enrollment_token SET ended_at = now(), ended = $2 \
         WHERE person_id = $1 AND ended_at IS NULL",
    )
    .bind(person)
    .bind(ended)
    .execute(&mut **tx)
    .await?;
    let value = hex(&bearer());
    let expires_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "INSERT INTO enrollment_token (token_digest, person_id, expires_at) \
         VALUES ($1, $2, now() + make_interval(hours => $3)) RETURNING expires_at",
    )
    .bind(digest(&value))
    .bind(person)
    .bind(hours as i32)
    .fetch_one(&mut **tx)
    .await?;
    Ok(IssuedToken { value, expires_at })
}

/// **The enabled persons who would still hold a live admin grant** were the
/// grant `without` revoked, counted under the exclusion: the last-admin
/// rule refuses where this is zero.
pub async fn admins_remaining_without(
    tx: &mut Transaction<'_, Postgres>,
    without: &str,
) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(DISTINCT g.person_id) FROM role_grant g JOIN person p USING (person_id) \
         WHERE g.role = 'admin' AND g.revoked_at IS NULL AND p.enabled AND g.grant_id <> $1",
    )
    .bind(without)
    .fetch_one(&mut **tx)
    .await?)
}

/// Whether the grant is live, read under the exclusion.
pub async fn grant_is_live(
    tx: &mut Transaction<'_, Postgres>,
    grant: &str,
) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM role_grant WHERE grant_id = $1 AND revoked_at IS NULL)",
    )
    .bind(grant)
    .fetch_one(&mut **tx)
    .await?)
}

/// **Revoke a live grant, at the version read** (Spec 3.2): its row stands,
/// with when it was revoked and the version moved. `false` where no row
/// moved, a stale edit the caller refuses.
pub async fn revoke_grant(
    tx: &mut Transaction<'_, Postgres>,
    grant: &str,
    author: Option<&str>,
    version: i64,
) -> anyhow::Result<bool> {
    Ok(sqlx::query(
        "UPDATE role_grant SET revoked_at = now(), author = $2, version = version + 1 \
         WHERE grant_id = $1 AND revoked_at IS NULL AND version = $3",
    )
    .bind(grant)
    .bind(author)
    .bind(version)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1)
}

/// **Write a role's verbs, at the version read** (Spec 3.2), the version
/// moved. `false` where no row moved, a stale edit the caller refuses.
pub async fn set_role_verbs(
    tx: &mut Transaction<'_, Postgres>,
    role: &str,
    verbs: &[String],
    author: Option<&str>,
    version: i64,
) -> anyhow::Result<bool> {
    Ok(sqlx::query(
        "UPDATE role SET verbs = $2, author = $3, version = version + 1 \
         WHERE name = $1 AND version = $4",
    )
    .bind(role)
    .bind(verbs)
    .bind(author)
    .bind(version)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1)
}
