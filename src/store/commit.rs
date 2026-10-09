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

use sqlx::{Postgres, Transaction};

use crate::store::Store;

/// **The act's `ok` outcome written in its transaction, then the commit**;
/// where the commit's answer is lost, whether that outcome stands decides:
/// present answers success, absent the commit's own error. `kind` names the
/// act to the log and to the test hooks.
pub(crate) async fn commit_with_outcome(
    mut tx: Transaction<'_, Postgres>,
    kind: &'static str,
    store: &Store,
    first: &str,
) -> anyhow::Result<()> {
    Store::audit_outcome_in(&mut tx, first).await?;
    let committed = tx.commit().await.map_err(anyhow::Error::from);
    #[cfg(test)]
    let committed = committed.and_then(|()| crate::link::register::fail_after_commit(kind));
    let Err(lost) = committed else {
        return Ok(());
    };
    #[cfg(test)]
    hold_before_the_read_back(kind).await;
    match store.outcome_landed(first).await {
        Ok(true) => {
            tracing::warn!(
                "the commit's answer of {kind} was lost ({lost:#}); its outcome stands, so it landed"
            );
            Ok(())
        }
        Ok(false) => Err(lost),
        Err(read) => Err(lost.context(format!(
            "and whether {kind} landed could not be read back: {read:#}"
        ))),
    }
}

/// A hold between a lost commit answer and its read-back, keyed by the
/// act's kind, one of a list so tests running at once each keep their own.
#[cfg(test)]
pub(crate) type ReadBackHold = (
    &'static str,
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
            .position(|(key, ..)| *key == kind)
            .map(|at| holds.remove(at))
    };
    if let Some((_, read, release)) = hold {
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
