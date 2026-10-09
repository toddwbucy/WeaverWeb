//! **A commit whose answer is lost is read back before it is classified**
//! (Spec 2.13). The error may come after PostgreSQL applied the commit and
//! before its answer arrived, so an admin's write that took it as failed
//! would answer the server's failure and record its outcome failed for an
//! act that landed, and a retry would then meet step three's refusal with
//! the record never put right. Each write names, beside itself, the effect
//! it intended, and where the answer is lost that effect is read on a fresh
//! connection: present, the write landed; absent, it failed.

use std::future::Future;

use sqlx::{Postgres, Transaction};

/// The transaction committed, or, where the commit's answer is lost, the
/// write's intended effect read back by `read_back`: present answers
/// success, absent the commit's own error. `kind` names the write to the
/// log and to the test hook that loses an answer.
pub(crate) async fn commit_or_read_back<F, Fut>(
    tx: Transaction<'_, Postgres>,
    kind: &'static str,
    read_back: F,
) -> anyhow::Result<()>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<bool>>,
{
    let committed = tx.commit().await.map_err(anyhow::Error::from);
    #[cfg(test)]
    let committed = committed.and_then(|()| crate::link::register::fail_after_commit(kind));
    let Err(lost) = committed else {
        return Ok(());
    };
    match read_back().await {
        Ok(true) => {
            tracing::warn!(
                "the commit's answer of {kind} was lost ({lost:#}); read back, it landed"
            );
            Ok(())
        }
        Ok(false) => Err(lost),
        Err(read) => Err(lost.context(format!(
            "and whether {kind} landed could not be read back: {read:#}"
        ))),
    }
}
