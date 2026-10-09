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

/// **Who acted, and how it was authenticated.** The host and a person are
/// the principals this crate writes as; the server's own asks are audited
/// with every ask.
#[derive(Debug, Clone, Copy)]
pub enum Principal<'a> {
    /// A command on the server's host, carrying the name it was given with
    /// `--author` as an unverified claim.
    Host { author: Option<&'a str> },
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
}

impl Principal<'_> {
    fn kind(&self) -> &'static str {
        match self {
            Principal::Host { .. } => "host",
            Principal::Person { .. } => "person",
        }
    }

    fn method(&self) -> &'static str {
        match self {
            Principal::Host { .. } => "host",
            Principal::Person {
                method: PersonMethod::EnrollmentToken,
                ..
            } => "enrollment token",
        }
    }

    fn claimed_author(&self) -> Option<&str> {
        match self {
            Principal::Host { author } => *author,
            Principal::Person { .. } => None,
        }
    }

    fn person_id(&self) -> Option<&str> {
        match self {
            Principal::Host { .. } => None,
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
        }
    }

    fn id(&self) -> Option<&str> {
        match self {
            Target::Authority => None,
            Target::Agent(id)
            | Target::Person(id)
            | Target::Grant(id)
            | Target::Role(id)
            | Target::Passkey(id) => Some(id),
        }
    }
}

// A test's lever on the first record: the next first record on this thread
// fails as a store refusing the insert would, so a test can show an act
// never lands without its record.
#[cfg(test)]
thread_local! {
    pub(crate) static FAIL_FIRST_RECORD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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

    /// **The outcome record, naming the first**: its principal, target and
    /// action copied from the first in the one statement, so the two never
    /// disagree, and the outcome `ok` or `failed`.
    pub async fn audit_outcome(&self, first: &str, ok: bool) -> anyhow::Result<String> {
        let id: String = sqlx::query_scalar(
            "INSERT INTO audit (principal, person_id, method, claimed_author, target_kind, \
             target_id, action, answers, outcome) \
             SELECT principal, person_id, method, claimed_author, target_kind, target_id, \
             action, audit_id, $2 FROM audit WHERE audit_id = $1 AND answers IS NULL \
             AND refusal IS NULL RETURNING audit_id",
        )
        .bind(first)
        .bind(if ok { "ok" } else { "failed" })
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("{first} is not a first record"))?;
        Ok(id)
    }

    /// **A refusal at the first gate, as one record**, since nothing acted.
    /// The refusal names why, and no credential.
    pub async fn audit_refusal(
        &self,
        principal: Principal<'_>,
        target: Target<'_>,
        action: &str,
        refusal: &str,
    ) -> anyhow::Result<String> {
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
