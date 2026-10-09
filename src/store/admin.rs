//! **An admin's writes to persons** (Spec 2.13; act 11, PR 5a): enrolling a
//! person, issuing a token, disabling, enabling and renaming, each through a
//! session whose person holds a live admin grant.
//!
//! **Each write re-checks its authority inside its own transaction**, under
//! the identity exclusion, before anything else: the session standing (open,
//! its person enabled, the passkey it was opened with still theirs) and its
//! person still holding a live admin grant. A revocation, a disable or a
//! removal committed after the request read its session refuses the write
//! rather than landing beneath it. The checks the host's commands make stand
//! here too: a name unique in its canonical form and never an identity's
//! shape, a token only for a person holding no passkey, the last enabled
//! admin never disabled, and an authored edit against a stale version
//! refused (Spec 3.2).

use sqlx::{Postgres, Transaction};

use crate::store::Store;
use crate::store::commit::commit_or_read_back;
use crate::store::identity::{self, IssuedToken, Supersedes, session_stands};

/// **Why an admin's write was refused.** Nothing was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The session no longer stands, or its person no longer holds a live
    /// admin grant.
    AuthorityGone,
    /// No such person.
    NoSuchPerson,
    /// The row moved since the page read it.
    Stale,
    /// The name is refused: empty, too long, or an identity's shape.
    Name(String),
    /// The name's canonical form is another person's.
    Taken(String),
    /// A token is issued only for a person holding no passkey.
    HoldsPasskey,
    /// The write would leave no enabled person holding a live admin grant.
    LastAdmin,
    /// The person is already in the state asked for.
    Already,
    /// No such grant.
    NoSuchGrant,
    /// No such role.
    NoSuchRole,
    /// No such agent in the register.
    NoSuchAgent,
    /// The role and the agent do not agree: `admin` server-wide and with no
    /// agent, every other role on one agent.
    Scope(String),
    /// The person already holds the role there.
    Held,
    /// The grant was revoked already.
    Revoked,
    /// A grant whose grantee is the admin themselves, granting or revoking.
    OwnGrant,
    /// A role the admin holds a grant of on some agent.
    HoldsRole,
    /// The admin role is fixed by the store.
    Fixed,
    /// A verb outside the vocabulary.
    Vocabulary(String),
    /// The server's authority cannot mint now: its config's name moved, or
    /// it was rotated on disk since the server loaded it.
    Authority(String),
    /// The agent's row holds no live credential: it was retired, and the
    /// agent is registered again rather than rotated.
    Retired,
    /// The hand-over holds as many client configs as it may.
    HandoverFull,
    /// The plane's credential is revoked already.
    PlaneRevoked,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::AuthorityGone => write!(
                f,
                "the session or the admin grant that authorized this no longer stands; sign in and begin again"
            ),
            Refusal::NoSuchPerson => write!(f, "no such person"),
            Refusal::Stale => write!(
                f,
                "that row changed since the page was read; reload it and try again"
            ),
            Refusal::Name(why) => write!(f, "{why}"),
            Refusal::Taken(name) => write!(
                f,
                "a person whose name is {name} in its canonical form stands; names are one where they differ only by case, width or composition"
            ),
            Refusal::HoldsPasskey => write!(
                f,
                "a token is issued only for a person holding no passkey; a person with one recovers by another passkey or the host's reset"
            ),
            Refusal::LastAdmin => write!(
                f,
                "this would leave no enabled person holding the admin grant"
            ),
            Refusal::Already => write!(f, "the person is already so"),
            Refusal::NoSuchGrant => write!(f, "no such grant"),
            Refusal::NoSuchRole => write!(f, "no such role"),
            Refusal::NoSuchAgent => write!(f, "no such agent in the register"),
            Refusal::Scope(why) => write!(f, "{why}"),
            Refusal::Held => write!(f, "that person already holds that role there"),
            Refusal::Revoked => write!(f, "that grant is revoked already"),
            Refusal::OwnGrant => write!(
                f,
                "no person grants or revokes a grant of their own; another admin, or the host's grant commands, write it"
            ),
            Refusal::HoldsRole => write!(
                f,
                "no person edits a role they hold a grant of, since widening it widens their own grant; another admin, or the host's role set, writes it"
            ),
            Refusal::Fixed => write!(
                f,
                "the admin role is fixed by the store and carries no agent verb; nothing writes it"
            ),
            Refusal::Authority(why) => write!(f, "{why}"),
            Refusal::PlaneRevoked => write!(f, "that plane's credential is revoked already"),
            Refusal::Retired => write!(
                f,
                "that agent's row holds no live credential; register the agent again rather than rotating it"
            ),
            Refusal::HandoverFull => write!(
                f,
                "too many client configs wait to be taken; take or let expire those waiting, and begin again in a few minutes"
            ),
            Refusal::Vocabulary(verb) => write!(
                f,
                "{verb} is not in the vocabulary ({})",
                crate::store::identity::VOCABULARY.join(", ")
            ),
        }
    }
}

/// **A person as the admin page lists them.**
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Listed {
    pub person_id: String,
    pub name: String,
    pub enabled: bool,
    pub version: i64,
    pub holds_passkey: bool,
    pub token_outstanding: bool,
    pub admin: bool,
}

/// **The authority of an admin's write**, read in its transaction under the
/// identity exclusion: the session standing and its person holding a live
/// admin grant.
pub async fn admin_stands(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    admin: &str,
) -> anyhow::Result<bool> {
    if !session_stands(tx, session_id, admin).await? {
        return Ok(false);
    }
    holds_live_admin(tx, admin).await
}

async fn holds_live_admin(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
) -> anyhow::Result<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM role_grant \
         WHERE person_id = $1 AND role = 'admin' AND revoked_at IS NULL)",
    )
    .bind(person)
    .fetch_one(&mut **tx)
    .await?)
}

/// The authority checked, or the write refused as its authority gone.
pub(crate) async fn authorized(
    tx: &mut Transaction<'_, Postgres>,
    session_id: i64,
    admin: &str,
) -> anyhow::Result<Result<(), Refusal>> {
    let stands = admin_stands(tx, session_id, admin).await?;
    Ok(if stands {
        Ok(())
    } else {
        Err(Refusal::AuthorityGone)
    })
}

impl Store {
    /// **Whether a person holds a live admin grant and is enabled**, for the
    /// page's gate. A write re-checks it in its own transaction.
    pub async fn is_admin(&self, person: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM role_grant g JOIN person p ON p.person_id = g.person_id \
             WHERE g.person_id = $1 AND g.role = 'admin' AND g.revoked_at IS NULL AND p.enabled)",
        )
        .bind(person)
        .fetch_one(&self.pool)
        .await?)
    }

    /// **Every person, as the admin page lists them**: enabled or not,
    /// whether they hold a passkey, whether a token is outstanding, whether
    /// they hold the admin grant. Nothing of an agent.
    pub async fn listed_persons(&self) -> anyhow::Result<Vec<Listed>> {
        Ok(sqlx::query_as(
            "SELECT p.person_id, p.name, p.enabled, p.version, \
             EXISTS (SELECT 1 FROM passkey k WHERE k.person_id = p.person_id) AS holds_passkey, \
             EXISTS (SELECT 1 FROM enrollment_token t WHERE t.person_id = p.person_id \
               AND t.ended_at IS NULL AND t.expires_at > now()) AS token_outstanding, \
             EXISTS (SELECT 1 FROM role_grant g WHERE g.person_id = p.person_id \
               AND g.role = 'admin' AND g.revoked_at IS NULL) AS admin \
             FROM person p ORDER BY p.name_key",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// **A person enrolled by an admin**: the row, its canonical name unique
    /// and never an identity's shape, and a token issued for it, answered
    /// once.
    pub async fn admin_enroll(
        &self,
        admin: &str,
        session_id: i64,
        person_id: &str,
        name: &str,
        hours: u32,
    ) -> anyhow::Result<Result<IssuedToken, Refusal>> {
        let name = match identity::given_name(name) {
            Ok(name) => name,
            Err(why) => return Ok(Err(Refusal::Name(why))),
        };
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        if identity::name_taken(&mut tx, &identity::name_key(&name)).await? {
            return Ok(Err(Refusal::Taken(name)));
        }
        identity::insert_person(&mut tx, person_id, &name, Some(admin)).await?;
        let token = identity::issue_token(&mut tx, person_id, hours, Supersedes::Issue).await?;
        commit_or_read_back(tx, "person enroll", || {
            self.token_stands(person_id, &token.value)
        })
        .await?;
        Ok(Ok(token))
    }

    /// **A token issued by an admin** for a person holding no passkey, an
    /// earlier live token superseded: the token and the person's name.
    pub async fn admin_issue_token(
        &self,
        admin: &str,
        session_id: i64,
        person: &str,
        hours: u32,
    ) -> anyhow::Result<Result<(IssuedToken, String), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let name: Option<String> =
            sqlx::query_scalar("SELECT name FROM person WHERE person_id = $1")
                .bind(person)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(name) = name else {
            return Ok(Err(Refusal::NoSuchPerson));
        };
        if identity::holds_passkey(&mut tx, person).await? {
            return Ok(Err(Refusal::HoldsPasskey));
        }
        let token = identity::issue_token(&mut tx, person, hours, Supersedes::Issue).await?;
        commit_or_read_back(tx, "person token", || {
            self.token_stands(person, &token.value)
        })
        .await?;
        Ok(Ok((token, name)))
    }

    /// **A person disabled by an admin**, at the version the page read:
    /// their outstanding tokens revoked in the same write, refused where it
    /// would leave no enabled person holding a live admin grant. Their
    /// sessions end at their next use, which reads the person disabled.
    pub async fn admin_disable(
        &self,
        admin: &str,
        session_id: i64,
        person: &str,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let Some(enabled) = enabled(&mut tx, person).await? else {
            return Ok(Err(Refusal::NoSuchPerson));
        };
        if !enabled {
            return Ok(Err(Refusal::Already));
        }
        if holds_live_admin(&mut tx, person).await? && admins_besides(&mut tx, person).await? == 0 {
            return Ok(Err(Refusal::LastAdmin));
        }
        #[cfg(test)]
        hold_inside(admin).await;
        if !set_enabled(&mut tx, person, version, false, admin).await? {
            return Ok(Err(Refusal::Stale));
        }
        sqlx::query(
            "UPDATE enrollment_token SET ended_at = now(), ended = 'revoked' \
             WHERE person_id = $1 AND ended_at IS NULL",
        )
        .bind(person)
        .execute(&mut *tx)
        .await?;
        commit_or_read_back(tx, "person disable", || {
            self.state_set(person, version, false, admin)
        })
        .await?;
        Ok(Ok(()))
    }

    /// **A disabled person enabled again by an admin**, at the version the
    /// page read.
    pub async fn admin_enable(
        &self,
        admin: &str,
        session_id: i64,
        person: &str,
        version: i64,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let Some(enabled) = enabled(&mut tx, person).await? else {
            return Ok(Err(Refusal::NoSuchPerson));
        };
        if enabled {
            return Ok(Err(Refusal::Already));
        }
        if !set_enabled(&mut tx, person, version, true, admin).await? {
            return Ok(Err(Refusal::Stale));
        }
        commit_or_read_back(tx, "person enable", || {
            self.state_set(person, version, true, admin)
        })
        .await?;
        Ok(Ok(()))
    }

    /// **A person renamed by an admin**, at the version the page read: the
    /// name never an identity's shape, and unique in its canonical form
    /// among every other person.
    pub async fn admin_rename(
        &self,
        admin: &str,
        session_id: i64,
        person: &str,
        version: i64,
        name: &str,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let name = match identity::given_name(name) {
            Ok(name) => name,
            Err(why) => return Ok(Err(Refusal::Name(why))),
        };
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        if enabled(&mut tx, person).await?.is_none() {
            return Ok(Err(Refusal::NoSuchPerson));
        }
        let key = identity::name_key(&name);
        let held_by_another: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM person WHERE name_key = $1 AND person_id <> $2)",
        )
        .bind(&key)
        .bind(person)
        .fetch_one(&mut *tx)
        .await?;
        if held_by_another {
            return Ok(Err(Refusal::Taken(name)));
        }
        let moved = sqlx::query(
            "UPDATE person SET name = $3, name_key = $4, author = $5, version = version + 1 \
             WHERE person_id = $1 AND version = $2",
        )
        .bind(person)
        .bind(version)
        .bind(&name)
        .bind(&key)
        .bind(admin)
        .execute(&mut *tx)
        .await?;
        if moved.rows_affected() != 1 {
            return Ok(Err(Refusal::Stale));
        }
        commit_or_read_back(tx, "person rename", || {
            self.renamed(person, version, &name, admin)
        })
        .await?;
        Ok(Ok(()))
    }

    // **What each person write leaves, read back where its commit's
    // answer was lost** (`store::commit`), on a fresh connection.

    /// An enrollment's or a token's effect: the token, by its digest,
    /// stands for the person.
    async fn token_stands(&self, person: &str, token: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM enrollment_token \
             WHERE person_id = $1 AND token_digest = $2)",
        )
        .bind(person)
        .bind(identity::digest(token))
        .fetch_one(&self.pool)
        .await?)
    }

    /// A disable's or an enable's effect: the state set, at the version
    /// after the one read, by this admin.
    async fn state_set(
        &self,
        person: &str,
        version: i64,
        enabled: bool,
        admin: &str,
    ) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM person WHERE person_id = $1 \
             AND version = $2 + 1 AND enabled = $3 AND author = $4)",
        )
        .bind(person)
        .bind(version)
        .bind(enabled)
        .bind(admin)
        .fetch_one(&self.pool)
        .await?)
    }

    /// A rename's effect: the name set, at the version after the one read,
    /// by this admin.
    async fn renamed(
        &self,
        person: &str,
        version: i64,
        name: &str,
        admin: &str,
    ) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM person WHERE person_id = $1 \
             AND version = $2 + 1 AND name = $3 AND author = $4)",
        )
        .bind(person)
        .bind(version)
        .bind(name)
        .bind(admin)
        .fetch_one(&self.pool)
        .await?)
    }
}

async fn enabled(tx: &mut Transaction<'_, Postgres>, person: &str) -> anyhow::Result<Option<bool>> {
    Ok(
        sqlx::query_scalar("SELECT enabled FROM person WHERE person_id = $1")
            .bind(person)
            .fetch_optional(&mut **tx)
            .await?,
    )
}

/// The enabled persons other than `person` holding a live admin grant.
async fn admins_besides(tx: &mut Transaction<'_, Postgres>, person: &str) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(DISTINCT g.person_id) FROM role_grant g JOIN person p ON p.person_id = g.person_id \
         WHERE g.role = 'admin' AND g.revoked_at IS NULL AND p.enabled AND g.person_id <> $1",
    )
    .bind(person)
    .fetch_one(&mut **tx)
    .await?)
}

/// The person's state set at the version read, authored by the admin.
/// Whether the row moved.
async fn set_enabled(
    tx: &mut Transaction<'_, Postgres>,
    person: &str,
    version: i64,
    enabled: bool,
    admin: &str,
) -> anyhow::Result<bool> {
    let moved = sqlx::query(
        "UPDATE person SET enabled = $3, author = $4, version = version + 1 \
         WHERE person_id = $1 AND version = $2",
    )
    .bind(person)
    .bind(version)
    .bind(enabled)
    .bind(admin)
    .execute(&mut **tx)
    .await?;
    Ok(moved.rows_affected() == 1)
}

/// A hold inside an admin's disable or revocation, after its authority and
/// its last-admin count are read and before it writes, keyed by the admin,
/// one of a list so tests running at once each keep their own: where the
/// exclusion did not serialize two such writes, both counts would read the
/// other admin live.
#[cfg(test)]
pub(crate) type AdminHold = (
    String,
    std::sync::Arc<tokio::sync::Notify>,
    std::sync::Arc<tokio::sync::Notify>,
);

#[cfg(test)]
pub(crate) static INSIDE_HOLD: std::sync::Mutex<Vec<AdminHold>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(crate) async fn hold_inside(admin: &str) {
    let hold = {
        let mut holds = INSIDE_HOLD.lock().unwrap();
        holds
            .iter()
            .position(|(key, ..)| key == admin)
            .map(|at| holds.remove(at))
    };
    if let Some((_, read, release)) = hold {
        read.notify_one();
        release.notified().await;
    }
}
