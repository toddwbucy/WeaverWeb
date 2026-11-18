//! **The suite's log captures** (#164). tracing-core 0.1.36 caches a
//! callsite's interest for the whole process. While at most one dispatcher
//! is registered (`has_just_one`), a callsite's first reach takes its
//! interest from the default of the thread that reaches it
//! (`Rebuilder::JustOne`), so a line first met on a thread with no subscriber
//! is cached `never`, and a test counting it through a subscriber scoped to
//! its own thread counts nothing. Which test meets a line first is the
//! scheduler's choice, so such a test passed alone and failed beside others.
//!
//! [`scoped`] installs the process's capture ([`global_capture`]) before
//! the test's own subscriber. The capture is a dispatcher that lives as long
//! as the process and is interested in every callsite at every level, so no
//! interest is `never` once it stands, whichever test registered first:
//! - Its registration recomputes the interest of every callsite already
//!   met, over every live dispatcher.
//! - Every later registration counts at least two dispatchers, so a
//!   callsite met later asks each of them.
//! - While it is the only one registered, a thread with no subscriber of
//!   its own has it as its default.
//!
//! A counted line then reaches the subscriber of the thread that writes it.
//! A test scopes a subscriber only through [`scoped`], which the guard below
//! holds.

use std::sync::Arc;

/// The log lines a capture's subscriber writes.
#[derive(Clone, Default)]
pub(crate) struct Captured(pub(crate) Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// Read by the tests of a build with the `passkeys` feature alone.
#[cfg(feature = "passkeys")]
impl Captured {
    /// Where the capture stands now: the lines a test reads come after it.
    pub(crate) fn mark(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    /// **A line of the capture** written since `from` and holding every
    /// needle, looked for until `within` has passed.
    pub(crate) async fn logged(
        &self,
        from: usize,
        needles: &[&str],
        within: std::time::Duration,
    ) -> bool {
        let until = tokio::time::Instant::now() + within;
        loop {
            let found = {
                let bytes = self.0.lock().unwrap();
                String::from_utf8_lossy(&bytes[from.min(bytes.len())..])
                    .lines()
                    .any(|line| needles.iter().all(|needle| line.contains(needle)))
            };
            if found {
                return true;
            }
            if tokio::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

/// **One capture for every thread of the test binary**, installed once as
/// the process's default: a capture scoped to a test's thread misses lines
/// that other threads write (measured 2026-10-14: the stall's own line was
/// absent in every parallel run and present alone), while the process's
/// default receives every line no scoped capture takes. The text a test
/// checks is its own, so every other test's lines in it only widen the
/// check. It is also the dispatcher that keeps every callsite's interest
/// from being `never` (the module's comment).
pub(crate) fn global_capture() -> &'static Captured {
    static CAPTURE: std::sync::OnceLock<Captured> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let captured = Captured::default();
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_writer(captured.clone())
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .finish(),
        )
        .expect("the test binary's one process-wide subscriber is the capture");
        captured
    })
}

/// **A subscriber scoped to the test's own thread**, set after the
/// process's capture stands, so the lines the test counts are counted
/// whichever thread first met them (the module's comment). Held for the
/// test's life, as `tracing::subscriber::set_default`'s guard is.
pub(crate) fn scoped<S>(subscriber: S) -> tracing::subscriber::DefaultGuard
where
    S: tracing::Subscriber + Send + Sync + 'static,
{
    global_capture();
    tracing::subscriber::set_default(subscriber)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST_MET_ELSEWHERE: &str = "a line first met on a thread with no subscriber of its own";

    /// The one callsite of the test below, met first on another thread.
    fn met_first_elsewhere() {
        tracing::warn!("{FIRST_MET_ELSEWHERE}");
    }

    /// **A line first met on a thread with no subscriber is still counted**
    /// by the subscriber of the thread that writes it next (#164).
    ///
    /// Perturbation: set the test's subscriber without the process's
    /// capture in `scoped`, and run this test alone (`cargo test --lib
    /// test_log::`). The other thread's first reach caches the callsite
    /// `never`, and the line is counted no times.
    #[test]
    fn a_line_first_met_on_a_thread_with_no_subscriber_is_counted() {
        let counted = Captured::default();
        let _logging = scoped(
            tracing_subscriber::fmt()
                .with_writer(counted.clone())
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .finish(),
        );
        std::thread::spawn(met_first_elsewhere).join().unwrap();
        met_first_elsewhere();
        let text = String::from_utf8_lossy(&counted.0.lock().unwrap()).into_owned();
        assert_eq!(text.matches(FIRST_MET_ELSEWHERE).count(), 1, "{text}");
    }

    /// **A test scopes a subscriber or sets the process's only through this
    /// module**, so no test's capture misses a line for another's order.
    ///
    /// Perturbation: set a subscriber with `tracing::subscriber` directly
    /// in any test file, and this names it.
    #[test]
    fn a_subscriber_is_set_only_through_this_module() {
        let calls = ["set_default(", "set_global_default(", "with_default("];
        let found: Vec<String> = crate::test_dir::rust_sources()
            .into_iter()
            .filter(|(path, _)| !path.ends_with("src/test_log.rs"))
            .flat_map(|(path, text)| {
                text.lines()
                    .enumerate()
                    .filter(|(_, line)| calls.iter().any(|call| line.contains(call)))
                    .map(|(at, _)| format!("{}:{}", path.display(), at + 1))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            found.is_empty(),
            "set through `test_log::scoped`: {found:?}"
        );
    }
}
