//! **An act's `ok` outcome commits with the act, and a lost commit answer
//! is read back by that one record** (Spec 2.13). The error of a commit may
//! come after PostgreSQL applied it and before its answer arrived, so an
//! act that took it as failed would answer the server's failure and record
//! its outcome `failed` for an act that landed, and a retry would meet step
//! three's refusal with the record never put right. Reading back the act's
//! effect would read state a later act may already have overwritten; the
//! `ok` outcome, written in the act's own transaction, commits with it or
//! not at all, and the audit is append-only, so whether it stands answers
//! whether the act landed.

//!
//! **The read-back waits for the original transaction to end.** A
//! connection may fail after its COMMIT was sent while the original backend
//! is still completing it, and an immediate read would find no outcome for
//! an act about to land. So the act takes a transaction-level advisory lock
//! keyed by its own first record before it writes its outcome and commits,
//! held until its COMMIT ends; the read-back takes the same lock on a fresh
//! connection before it reads, and so returns only once the original has
//! committed or aborted, when the outcome's absence is definitive. Its wait
//! is bounded; past the bound the act's fate is unknown, answered as the
//! server's failure with no outcome written, so a later read can settle it.

use std::time::Duration;

use sqlx::{Postgres, Transaction};

use crate::store::Store;

/// **The class of the outcome locks**, one lock per act keyed by its first
/// record in the two-key form (this class, the record's `hashtext`), a
/// keyspace PostgreSQL keeps apart from the one-key form the identity
/// exclusion, the listener's and the authority's locks take
/// (`IDENTITY_LOCK_KEY`, `LISTENER_LOCK_KEY`, `AUTHORITY_LOCK_KEY`), so no
/// key collides with theirs. Two first records whose hashes meet only make
/// one read-back wait for the other act's commit.
pub const OUTCOME_LOCK_CLASS: i32 = i32::from_be_bytes(*b"wout");

/// **How long a read-back waits for the original transaction to end.** All
/// the original backend has left is its COMMIT, which this store completes
/// in milliseconds; thirty seconds is far past any commit and short of a
/// browser's patience, and past it the act's fate is answered as unknown.
pub const READ_BACK_BOUND: Duration = Duration::from_secs(30);

/// **A commit's answer was lost and the read-back could not settle it**
/// within its bound: the original transaction had not ended. No outcome is
/// written, so a later read of the audit can still settle the act.
#[derive(Debug)]
pub struct OutcomeUnknown;

impl std::fmt::Display for OutcomeUnknown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the commit's answer was lost and whether it landed is not yet known; its outcome is left unwritten"
        )
    }
}

impl std::error::Error for OutcomeUnknown {}

/// Whether an act's error is an unknown outcome, which its caller answers
/// as the server's failure without writing a `failed` outcome.
pub(crate) fn outcome_unknown<T>(result: &anyhow::Result<T>) -> bool {
    matches!(result, Err(e) if e.is::<OutcomeUnknown>())
}

/// **The act's `ok` outcome written in its transaction, then the commit**;
/// where the commit's answer is lost, whether that outcome stands decides,
/// read once the original transaction has ended: present answers success,
/// absent the commit's own error, and a wait past its bound an unknown
/// outcome. `kind` names the act to the log and to the test hooks.
pub(crate) async fn commit_with_outcome(
    mut tx: Transaction<'_, Postgres>,
    kind: &'static str,
    store: &Store,
    first: &str,
) -> anyhow::Result<()> {
    // **The act's own lock, held until its COMMIT ends**: taken before the
    // COMMIT is sent, so any read-back, which runs only after it was sent,
    // finds it held while the original backend is still completing it.
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(OUTCOME_LOCK_CLASS)
        .bind(first)
        .execute(&mut *tx)
        .await?;
    Store::audit_outcome_in(&mut tx, first).await?;
    #[cfg(test)]
    let committed = match give_up_on_the_commit(kind) {
        Some(after) => match tokio::time::timeout(after, tx.commit()).await {
            Ok(committed) => committed.map_err(anyhow::Error::from),
            Err(_) => Err(anyhow::anyhow!("a test gave up on the commit's answer")),
        },
        None => tx.commit().await.map_err(anyhow::Error::from),
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(anyhow::Error::from);
    #[cfg(test)]
    let committed = committed.and_then(|()| crate::link::register::fail_after_commit(kind));
    let Err(lost) = committed else {
        return Ok(());
    };
    #[cfg(test)]
    hold_before_the_read_back(kind).await;
    match outcome_after_its_transaction(store, first).await {
        Ok(Some(true)) => {
            tracing::warn!(
                "the commit's answer of {kind} was lost ({lost:#}); its outcome stands, so it landed"
            );
            Ok(())
        }
        Ok(Some(false)) => Err(lost),
        Ok(None) => {
            tracing::error!(
                "the commit's answer of {kind} was lost ({lost:#}) and its transaction had not ended within the read-back's bound; its outcome is unknown"
            );
            Err(anyhow::Error::new(OutcomeUnknown).context(format!("{kind}: {lost:#}")))
        }
        Err(read) => Err(lost.context(format!(
            "and whether {kind} landed could not be read back: {read:#}"
        ))),
    }
}

/// **Whether the act's `ok` outcome stands, read once its transaction has
/// ended**: the act's own lock taken on a fresh connection, within the
/// bound, then the outcome read. `None` where the bound passed first.
async fn outcome_after_its_transaction(store: &Store, first: &str) -> anyhow::Result<Option<bool>> {
    let mut tx = store.pool.begin().await?;
    sqlx::query("SELECT set_config('lock_timeout', $1, true)")
        .bind(format!("{}ms", read_back_bound().as_millis()))
        .execute(&mut *tx)
        .await?;
    let locked = sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(OUTCOME_LOCK_CLASS)
        .bind(first)
        .execute(&mut *tx)
        .await;
    if let Err(e) = locked {
        if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("55P03") {
            return Ok(None);
        }
        return Err(e.into());
    }
    let landed: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM audit WHERE answers = $1 AND outcome = 'ok')",
    )
    .bind(first)
    .fetch_one(&mut *tx)
    .await?;
    tx.rollback().await?;
    Ok(Some(landed))
}

fn read_back_bound() -> Duration {
    #[cfg(test)]
    if let Some(bound) = READ_BACK_BOUND_FOR_TEST.with(|b| b.get()) {
        return bound;
    }
    READ_BACK_BOUND
}

#[cfg(test)]
thread_local! {
    /// A test's shorter read-back bound.
    pub(crate) static READ_BACK_BOUND_FOR_TEST: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
    /// A test's giving up on the answer of the next commit of a kind after
    /// a wait, the COMMIT already sent and left to complete on its own.
    pub(crate) static GIVE_UP_ON_COMMIT: std::cell::Cell<Option<(&'static str, Duration)>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn give_up_on_the_commit(kind: &'static str) -> Option<Duration> {
    GIVE_UP_ON_COMMIT.with(|g| match g.get() {
        Some((armed, after)) if armed == kind => {
            g.set(None);
            Some(after)
        }
        _ => None,
    })
}

/// A hold between a lost commit answer and its read-back, keyed by the
/// act's kind **and the thread of the test that set it**, one of a list so
/// tests running at once each keep their own: two tests losing the same
/// kind's answer would otherwise take each other's hold, one waking before
/// its own commit and the other waiting on a release that never comes. A
/// test's runtime is current-thread, so its act's read-back runs on the
/// thread that set the hold, as the thread-local fault that loses the
/// answer already assumes.
#[cfg(test)]
pub(crate) type ReadBackHold = (
    &'static str,
    std::thread::ThreadId,
    std::sync::Arc<tokio::sync::Notify>,
    std::sync::Arc<tokio::sync::Notify>,
);

#[cfg(test)]
pub(crate) static READ_BACK_HOLD: std::sync::Mutex<Vec<ReadBackHold>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
async fn hold_before_the_read_back(kind: &'static str) {
    let hold = {
        let mut holds = READ_BACK_HOLD.lock().unwrap();
        holds
            .iter()
            .position(|(key, thread, ..)| *key == kind && *thread == std::thread::current().id())
            .map(|at| holds.remove(at))
    };
    if let Some((_, _, read, release)) = hold {
        read.notify_one();
        release.notified().await;
    }
}

/// Arm the test fault that loses the answer of the next commit of `kind`.
#[cfg(test)]
pub(crate) fn lose_the_commits_answer(kind: &'static str) {
    crate::link::register::FAIL_AFTER_COMMIT.with(|f| f.set(Some(kind)));
}

/// Whether the armed fault fired, and so the answer was lost.
#[cfg(test)]
pub(crate) fn the_answer_was_lost() -> bool {
    crate::link::register::FAIL_AFTER_COMMIT.with(|f| f.get().is_none())
}
