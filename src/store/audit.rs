//! conforms: web-every-verb-asked-is-audited
//!
//! **The audit's one writer** (Spec 2.13, `docs/project/design-2026-10-07-iam.md`
//! section 10). Nothing else in the crate inserts into `audit`, which a test
//! holds by reading the tree, so "every act is audited" is checkable by
//! reading one call graph: each audited act sits between the first record
//! this module writes and the outcome record naming it.
//!
//! **Two records per act and never a rewrite**: a first record before the
//! act, which answers its identity; the outcome naming it once the act has
//! answered; a refusal at the first gate as one record carrying its refusal.
//! The table refuses every update, delete and truncate (migration `0014`).
//!
//! **A record holds no secret.** The outcome is `ok` or `failed` and never
//! the act's error text, which can carry a path; a refusal's text is the
//! caller's, and names no credential.

use crate::store::Store;
use crate::store::commit::OUTCOME_LOCK_CLASS;

/// **Who acted, and how it was authenticated.** The host, a person and the
/// server are the principals this crate writes as.
#[derive(Debug, Clone, Copy)]
pub enum Principal<'a> {
    /// A command on the server's host, carrying the name it was given with
    /// `--author` as an unverified claim.
    Host { author: Option<&'a str> },
    /// **The server, for its own asks** (Spec 2.13, design section 10): the
    /// `show` it asks at an admission and at each opening of the trace
    /// door, its method `server`, as the schema pairs them.
    Server,
    /// A person, by their identity, and how they were authenticated.
    Person {
        person_id: &'a str,
        method: PersonMethod,
    },
}

/// **How a person was authenticated** for the act (Spec 2.13): the methods
/// the audit's schema admits, as this crate comes to write them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonMethod {
    /// An enrollment token, which authenticates its person for the one
    /// write it is redeemed for and nothing else.
    EnrollmentToken,
    /// A passkey assertion the library verified: a sign-in, the fresh
    /// assertion before a passkey's addition and the addition it
    /// authorizes, and an assertion whose counter the rule refused, the
    /// signature having proved its person.
    PasskeyAssertion,
    /// A live session: what a signed-in person does with it, removing one
    /// of their passkeys among them.
    Session,
}

impl Principal<'_> {
    fn kind(&self) -> &'static str {
        match self {
            Principal::Host { .. } => "host",
            Principal::Server => "server",
            Principal::Person { .. } => "person",
        }
    }

    fn method(&self) -> &'static str {
        match self {
            Principal::Host { .. } => "host",
            Principal::Server => "server",
            Principal::Person {
                method: PersonMethod::EnrollmentToken,
                ..
            } => "enrollment token",
            Principal::Person {
                method: PersonMethod::PasskeyAssertion,
                ..
            } => "passkey assertion",
            Principal::Person {
                method: PersonMethod::Session,
                ..
            } => "session",
        }
    }

    fn claimed_author(&self) -> Option<&str> {
        match self {
            Principal::Host { author } => *author,
            Principal::Server | Principal::Person { .. } => None,
        }
    }

    fn person_id(&self) -> Option<&str> {
        match self {
            Principal::Host { .. } | Principal::Server => None,
            Principal::Person { person_id, .. } => Some(person_id),
        }
    }
}

/// **What was acted on**, by kind and identity (Spec 2.13): the server's
/// authority, which has no identity of its own, an agent's row, a person, a
/// grant, or a role by its name.
#[derive(Debug, Clone, Copy)]
pub enum Target<'a> {
    Authority,
    Agent(&'a str),
    Person(&'a str),
    Grant(&'a str),
    Role(&'a str),
    /// A passkey, by its own identity (`pk-`).
    Passkey(&'a str),
    /// An alert, by its own identity (`al-`), for its acknowledgement.
    Alert(&'a str),
}

impl Target<'_> {
    fn kind(&self) -> &'static str {
        match self {
            Target::Authority => "authority",
            Target::Agent(_) => "agent",
            Target::Person(_) => "person",
            Target::Grant(_) => "grant",
            Target::Role(_) => "role",
            Target::Passkey(_) => "passkey",
            Target::Alert(_) => "alert",
        }
    }

    fn id(&self) -> Option<&str> {
        match self {
            Target::Authority => None,
            Target::Agent(id)
            | Target::Person(id)
            | Target::Grant(id)
            | Target::Role(id)
            | Target::Passkey(id)
            | Target::Alert(id) => Some(id),
        }
    }
}

// A test's lever on the first record: the next first record on this thread
// fails as a store refusing the insert would, so a test can show an act
// never lands without its record.
#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_FIRST_RECORD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The same lever on a refusal's record.
    pub(crate) static FAIL_REFUSAL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// A test's lever on the next first record written under its own
    /// identity (`audit_first_as`): what its insert does around its commit.
    pub(crate) static FIRST_FAULT: std::cell::Cell<Option<FirstFault>> = const { std::cell::Cell::new(None) };
}

/// What a test makes the next `audit_first_as` do around its commit.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum FirstFault {
    /// Commit, then hold the answer this long.
    DelayAnswer(std::time::Duration),
    /// Commit, then lose the answer.
    LoseAnswer,
    /// Lose the answer before the commit, which lands this much later: a
    /// connection that failed while its backend still completed the COMMIT.
    CommitLate(std::time::Duration),
    /// Never commit.
    Abort,
}

/// **A first record's identity, drawn on the server** (Spec 2.13): the
/// audit's own shape, `au-` and sixteen hex digits from the operating
/// system's random source, so the step that writes it owns its answer from
/// before its insert.
pub fn draw_audit_id() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the operating system's random source answers");
    format!("au-{}", crate::store::identity::hex(&bytes))
}

/// **A first record's members, owned**, so its write can run in a task of
/// its own that its asker cannot cancel.
#[derive(Debug, Clone)]
pub struct FirstRecord {
    principal: &'static str,
    person_id: Option<String>,
    method: &'static str,
    claimed_author: Option<String>,
    target_kind: &'static str,
    target_id: Option<String>,
    action: String,
}

impl FirstRecord {
    pub fn new(principal: Principal<'_>, target: Target<'_>, action: &str) -> Self {
        Self {
            principal: principal.kind(),
            person_id: principal.person_id().map(str::to_owned),
            method: principal.method(),
            claimed_author: principal.claimed_author().map(str::to_owned),
            target_kind: target.kind(),
            target_id: target.id().map(str::to_owned),
            action: action.to_owned(),
        }
    }
}

/// The outcome record's one statement: its principal, target and action
/// copied from the first it answers.
const OUTCOME: &str = "INSERT INTO audit (principal, person_id, method, claimed_author, \
     target_kind, target_id, action, answers, outcome) \
     SELECT principal, person_id, method, claimed_author, target_kind, target_id, \
     action, audit_id, $2 FROM audit WHERE audit_id = $1 AND answers IS NULL \
     AND refusal IS NULL RETURNING audit_id";

impl Store {
    /// **The first record, written before the act**, answering its
    /// identity. An act whose first record cannot be written does not act:
    /// the caller answers this error instead.
    pub async fn audit_first(
        &self,
        principal: Principal<'_>,
        target: Target<'_>,
        action: &str,
    ) -> anyhow::Result<String> {
        #[cfg(test)]
        if FAIL_FIRST_RECORD.with(|f| f.replace(false)) {
            anyhow::bail!("a test fault refused the first record");
        }
        let id: String = sqlx::query_scalar(
            "INSERT INTO audit (principal, person_id, method, claimed_author, target_kind, \
             target_id, action) VALUES ($1, $7, $2, $3, $4, $5, $6) RETURNING audit_id",
        )
        .bind(principal.kind())
        .bind(principal.method())
        .bind(principal.claimed_author())
        .bind(target.kind())
        .bind(target.id())
        .bind(action)
        .bind(principal.person_id())
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    /// **A first record under the identity its writer drew, in a transaction
    /// that first takes the record's outcome lock** (`store::commit`'s
    /// class, the identity's `hashtext`), held until the transaction ends.
    /// `audit_unsent` takes the same lock, so a `failed` outcome written for
    /// a first whose answer was lost waits for this transaction to commit or
    /// abort, and then lands exactly where the first did.
    pub async fn audit_first_as(&self, id: &str, record: &FirstRecord) -> anyhow::Result<()> {
        #[cfg(test)]
        if FAIL_FIRST_RECORD.with(|f| f.replace(false)) {
            anyhow::bail!("a test fault refused the first record");
        }
        #[cfg(test)]
        let fault = FIRST_FAULT.with(|f| f.take());
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
            .bind(OUTCOME_LOCK_CLASS)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO audit (audit_id, principal, person_id, method, claimed_author, \
             target_kind, target_id, action) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(record.principal)
        .bind(&record.person_id)
        .bind(record.method)
        .bind(&record.claimed_author)
        .bind(record.target_kind)
        .bind(&record.target_id)
        .bind(&record.action)
        .execute(&mut *tx)
        .await?;
        #[cfg(test)]
        match fault {
            Some(FirstFault::Abort) => anyhow::bail!("a test fault aborted the first record"),
            Some(FirstFault::CommitLate(after)) => {
                tokio::spawn(async move {
                    tokio::time::sleep(after).await;
                    let _ = tx.commit().await;
                });
                anyhow::bail!("a test fault lost the first record's answer before its commit");
            }
            _ => {}
        }
        tx.commit().await?;
        #[cfg(test)]
        match fault {
            Some(FirstFault::DelayAnswer(after)) => tokio::time::sleep(after).await,
            Some(FirstFault::LoseAnswer) => {
                anyhow::bail!("a test fault lost the first record's answer after its commit")
            }
            _ => {}
        }
        Ok(())
    }

    /// **The `failed` outcome of a first whose frame never left**, once the
    /// first's own transaction has ended: the record's outcome lock taken
    /// first, as `audit_first_as` took it, then the outcome's one statement,
    /// which lands only where the first did. Answers whether it landed; a
    /// first that never committed gets nothing.
    pub async fn audit_unsent(&self, first: &str) -> anyhow::Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
            .bind(OUTCOME_LOCK_CLASS)
            .bind(first)
            .execute(&mut *tx)
            .await?;
        let landed: Option<String> = sqlx::query_scalar(OUTCOME)
            .bind(first)
            .bind("failed")
            .fetch_optional(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(landed.is_some())
    }

    /// **The outcome record, naming the first**: its principal, target and
    /// action copied from the first in the one statement, so the two never
    /// disagree, and the outcome `ok` or `failed`.
    pub async fn audit_outcome(&self, first: &str, ok: bool) -> anyhow::Result<String> {
        let id: String = sqlx::query_scalar(OUTCOME)
            .bind(first)
            .bind(if ok { "ok" } else { "failed" })
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| anyhow::anyhow!("{first} is not a first record"))?;
        Ok(id)
    }

    /// **The `ok` outcome, inside the act's own transaction** (Spec 2.13):
    /// written on the act's connection before its commit, so the effect and
    /// the outcome answering it commit together or not at all. A write
    /// that rolls back takes this record with it, and its `failed` outcome
    /// is written after by `audit_outcome`, the one-outcome index holding.
    pub async fn audit_outcome_in(
        conn: &mut sqlx::PgConnection,
        first: &str,
    ) -> anyhow::Result<String> {
        let id: String = sqlx::query_scalar(OUTCOME)
            .bind(first)
            .bind("ok")
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| anyhow::anyhow!("{first} is not a first record"))?;
        Ok(id)
    }

    /// **Whether an `ok` outcome answers `first`**: the one read-back for
    /// every act whose `ok` outcome commits with it, where the commit's
    /// answer was lost. The audit is append-only, so no later act can erase
    /// what this reads.
    pub async fn outcome_landed(&self, first: &str) -> anyhow::Result<bool> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM audit WHERE answers = $1 AND outcome = 'ok')",
        )
        .bind(first)
        .fetch_one(&self.pool)
        .await?)
    }

    /// **A refusal at the first gate, as one record**, since nothing acted.
    /// The refusal names why, and no credential. **A caller whose refusal
    /// cannot be recorded answers with the server's failure**, logged at
    /// error, and never with the ordinary refusal: a refusal the audit lost
    /// would read as one it holds, as a first record that cannot be written
    /// already refuses its act.
    pub async fn audit_refusal(
        &self,
        principal: Principal<'_>,
        target: Target<'_>,
        action: &str,
        refusal: &str,
    ) -> anyhow::Result<String> {
        #[cfg(test)]
        if FAIL_REFUSAL.with(|f| f.replace(false)) {
            anyhow::bail!("a test fault refused the refusal's record");
        }
        let id: String = sqlx::query_scalar(
            "INSERT INTO audit (principal, person_id, method, claimed_author, target_kind, \
             target_id, action, refusal) VALUES ($1, $8, $2, $3, $4, $5, $6, $7) RETURNING audit_id",
        )
        .bind(principal.kind())
        .bind(principal.method())
        .bind(principal.claimed_author())
        .bind(target.kind())
        .bind(target.id())
        .bind(action)
        .bind(refusal)
        .bind(principal.person_id())
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }
}
