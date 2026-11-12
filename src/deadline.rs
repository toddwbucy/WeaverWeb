//! conforms: web-a-bounded-write-is-never-sent-once-its-deadline-has-come
//!
//! **A store write the server bounds is never polled once its deadline has
//! come** (Spec 7.2). tokio's `timeout` and `timeout_at` poll the wrapped
//! future first and read their deadline after it, at every poll and at the
//! timer's own wake among them. A write still waiting when its deadline
//! comes, on the pool for a connection, on a lock or on an earlier
//! statement's answer, is then polled once more at the wake. If its next
//! step is the statement that commits, that statement goes out in that
//! poll, and it commits whether or not its answer is waited for, while the
//! caller takes its timeout branch and reports the write as not made.
//!
//! [`before`] reads the deadline, kept as an instant, at every poll and
//! before the write, as the admission's landing does since #81
//! (`link::listener::bounded_at`, which also reads the connection's close
//! signal). What it leaves open is a statement that goes out before the
//! deadline and is answered after it: it commits while the bound reads as
//! passed, a window of one statement's flight that section 10 holds open.

use std::future::Future;
use std::task::Poll;

use tokio::time::Instant;

/// **The deadline came before the write answered**: the write was dropped
/// where it stood, and no part of it was polled at or past the deadline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Passed;

/// **`write`, bounded by `deadline`, which is read before the write is
/// polled**: at the first poll, so a deadline already past never polls it,
/// and at every poll after, the timer's wake among them, so a write still
/// waiting when the deadline comes is dropped there and not polled again.
/// The timer is polled even where the clock answers first, so a poll that
/// finds the deadline still ahead leaves its wake registered.
pub(crate) async fn before<T>(
    write: impl Future<Output = T>,
    deadline: Instant,
) -> Result<T, Passed> {
    let mut write = std::pin::pin!(write);
    let mut timer = std::pin::pin!(tokio::time::sleep_until(deadline));
    std::future::poll_fn(|cx| {
        if Instant::now() >= deadline || timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(Passed));
        }
        write.as_mut().poll(cx).map(Ok)
    })
    .await
}

/// **A test's hold on a bounded write**, at the one place its failure
/// shows: just before the statement that commits it.
///
/// - The site records the deadline it bounds its write by ([`hold::bound`]).
/// - The store function, where the hold is armed for its point and a
///   deadline is recorded, waits until that very instant
///   ([`hold::statement`] for a write of one statement, [`hold::commit`]
///   for a transaction's commit), and marks the hold as let go.
///
/// The wait and the caller's timer share one instant, so they fire in one
/// tick of the timer and wake one poll. A bound that polls the write first,
/// as tokio's does, finds the wait over and sends the statement in that
/// poll; [`before`] reads the deadline first and drops the write unsent. No
/// sleep races another, so the verdict holds every run.
///
/// A statement of one write that has never run on its connection is
/// prepared first, a round trip of its own, so [`hold::statement`] takes a
/// connection and prepares the statement on it before the wait: what goes
/// out at the wake is then the statement itself, in one poll, as a write
/// whose connection and statement were already in hand would send it.
///
/// The hold is thread-local, as the audit's other test levers are: the
/// tests that arm it run on a current-thread runtime, where the listener's
/// tasks run on the test's own thread.
#[cfg(test)]
pub(crate) mod hold {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use tokio::time::Instant;

    /// The bounded writes a test can hold, one per store function.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum Point {
        /// `Store::audit_unsent`, the `failed` outcome of a first whose
        /// frame never left: its transaction's commit.
        Unsent,
        /// `Store::audit_first_as`, a first record: its transaction's
        /// commit.
        FirstRecord,
        /// `Store::audit_outcome`, a verb's outcome: its one statement.
        Outcome,
        /// `Store::audit_refusal`, a refusal's record: its one statement.
        Refusal,
    }

    #[derive(Debug, Default)]
    struct Held {
        deadline: Option<Instant>,
        let_go: bool,
    }

    thread_local! {
        static HELD: RefCell<HashMap<Point, Held>> = RefCell::new(HashMap::new());
    }

    /// **The hold armed on this thread for one point**, until it drops.
    /// Armed only by the site tests, which drive the listener as the
    /// person-facing tests do and share their log capture, so with them
    /// it needs the `passkeys` feature.
    #[cfg(feature = "passkeys")]
    #[derive(Debug)]
    pub(crate) struct Armed(Point);

    #[cfg(feature = "passkeys")]
    impl Armed {
        /// Whether the held write was let go at its deadline: its wait
        /// ended and the statement that commits it went out in that poll.
        pub(crate) fn let_go(&self) -> bool {
            HELD.with(|h| h.borrow().get(&self.0).is_some_and(|held| held.let_go))
        }
    }

    #[cfg(feature = "passkeys")]
    impl Drop for Armed {
        fn drop(&mut self) {
            HELD.with(|h| h.borrow_mut().remove(&self.0));
        }
    }

    /// Arm the hold for `point` on this thread.
    #[cfg(feature = "passkeys")]
    pub(crate) fn arm(point: Point) -> Armed {
        HELD.with(|h| h.borrow_mut().insert(point, Held::default()));
        Armed(point)
    }

    /// **The site's half**: the deadline it bounds its write by, recorded
    /// where the hold is armed for the point and none is recorded yet.
    pub(crate) fn bound(point: Point, deadline: Instant) {
        HELD.with(|h| {
            if let Some(held) = h.borrow_mut().get_mut(&point)
                && held.deadline.is_none()
            {
                held.deadline = Some(deadline);
            }
        });
    }

    /// The recorded deadline, taken once, so the hold holds one write.
    fn take(point: Point) -> Option<Instant> {
        HELD.with(|h| {
            h.borrow_mut()
                .get_mut(&point)
                .and_then(|held| held.deadline.take())
        })
    }

    async fn until(point: Point, deadline: Instant) {
        tokio::time::sleep_until(deadline).await;
        HELD.with(|h| {
            if let Some(held) = h.borrow_mut().get_mut(&point) {
                held.let_go = true;
            }
        });
    }

    /// **The store's half, before a transaction's commit**: where the hold
    /// is armed and bound, wait until the deadline.
    pub(crate) async fn commit(point: Point) {
        if let Some(deadline) = take(point) {
            until(point, deadline).await;
        }
    }

    /// **The store's half, for a write of one statement**: where the hold
    /// is armed and bound, a connection taken and `sql` prepared on it,
    /// then the wait; the connection the statement is to run on. `None`
    /// where the hold is not armed and bound, and the write runs as it
    /// always does.
    pub(crate) async fn statement(
        point: Point,
        pool: &sqlx::PgPool,
        sql: &'static str,
    ) -> anyhow::Result<Option<sqlx::pool::PoolConnection<sqlx::Postgres>>> {
        use sqlx::{Executor, SqlSafeStr};
        let Some(deadline) = take(point) else {
            return Ok(None);
        };
        let mut conn = pool.acquire().await?;
        (&mut *conn).prepare(sql.into_sql_str()).await?;
        until(point, deadline).await;
        Ok(Some(conn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    /// **A write that waits until its deadline and then sends**, as one
    /// waiting on the pool takes its connection at the timer's wake: the
    /// flag is the statement going out, set in the poll where its wait
    /// ends, and the write then waits on its answer forever.
    fn sent_at(deadline: Instant) -> (Arc<AtomicBool>, impl Future<Output = ()>) {
        let sent = Arc::new(AtomicBool::new(false));
        let flag = sent.clone();
        let write = async move {
            tokio::time::sleep_until(deadline).await;
            flag.store(true, Ordering::Relaxed);
            std::future::pending::<()>().await;
        };
        (sent, write)
    }

    /// **tokio's order, held as a fact**: `timeout_at` polls the write
    /// before it reads its deadline, so a write whose wait ends at the
    /// deadline sends at the timer's wake while the bound answers elapsed.
    /// This is the shape the nine sites had (the sweep of 2026-11-12); the
    /// test fails if a tokio upgrade reorders the poll, which would retire
    /// the helper's reason but not its rule.
    #[tokio::test]
    async fn tokios_timeout_sends_a_write_waiting_at_its_deadline() {
        let deadline = Instant::now() + Duration::from_millis(20);
        let (sent, write) = sent_at(deadline);
        let bounded = tokio::time::timeout_at(deadline, write).await;
        assert!(bounded.is_err(), "the bound elapsed");
        assert!(
            sent.load(Ordering::Relaxed),
            "tokio's timeout polled the write at its deadline, and it sent"
        );
    }

    /// **The same write under [`before`] is never sent**: the timer's wake
    /// is a poll at or past the deadline, the deadline is read first, and
    /// the write is dropped unpolled. No sleep races the deadline: the
    /// write's wait and the bound's timer share one instant.
    ///
    /// Perturbation: poll the write before reading the deadline in
    /// [`before`], and the write sends at the wake.
    #[tokio::test]
    async fn a_write_waiting_at_its_deadline_is_never_sent() {
        let deadline = Instant::now() + Duration::from_millis(20);
        let (sent, write) = sent_at(deadline);
        assert_eq!(before(write, deadline).await, Err(Passed));
        assert!(
            !sent.load(Ordering::Relaxed),
            "the write was polled at its deadline, where its statement went out"
        );
    }

    /// **A write handed a deadline already past is never polled**, at the
    /// deadline itself and at one long gone.
    ///
    /// Perturbation: drop the clock and keep the timer alone, and a
    /// deadline at this very instant, which the timer rounds up to its next
    /// tick, lets the write be polled.
    #[tokio::test]
    async fn a_write_handed_a_deadline_already_past_is_never_polled() {
        let now = Instant::now();
        let gone = now.checked_sub(Duration::from_secs(1)).unwrap_or(now);
        for deadline in [now, gone] {
            let polled = Arc::new(AtomicBool::new(false));
            let seen = polled.clone();
            let write = std::future::poll_fn(move |_| {
                seen.store(true, Ordering::Relaxed);
                Poll::Ready(())
            });
            assert_eq!(before(write, deadline).await, Err(Passed));
            assert!(!polled.load(Ordering::Relaxed));
        }
    }

    /// The bound takes nothing from a write with time left: it is polled
    /// and its answer is the bound's.
    #[tokio::test]
    async fn a_write_with_time_left_is_polled_and_its_answer_taken() {
        let deadline = Instant::now() + Duration::from_secs(60);
        assert_eq!(before(async { 7 }, deadline).await, Ok(7));
    }
}
