//! conforms: web-a-connectors-entry-is-taken-once
//!
//! **A connector's log entries, sent upstream** (Spec 2.14, 8; act 12, PR
//! 2; the operator's rulings of 2026-10-14 and 2026-10-15): what a
//! connector meets, a line of the box's `admin.log` or its own failure, is
//! written to its own log first and then queued here, and the link's
//! [`crate::link::client::Connection`] sends it to the server as an
//! `entry` frame on a task of its own.
//!
//! **Nothing here waits.** [`Entries::push`] takes a lock for a moment and
//! returns, so reading `admin.log`, relaying the trace and running a verb
//! never wait on an entry's send or its acknowledgement.
//!
//! **At least once, and bounded.** An entry stays until the server
//! acknowledges its sequence, so a link lost before the acknowledgement
//! resends it on the next connection, and the server, holding the highest
//! sequence it took of each run, takes it once. The backlog holds at most
//! [`BACKLOG_ENTRIES`] entries and [`BACKLOG_BYTES`] bytes, the oldest
//! dropped first; the drops are counted and reported to the server as one
//! entry once there is room again, a reconnection or an acknowledgement.
//! A connector's restart draws a new run, so its sequence starts afresh
//! and nothing of an earlier run is mistaken for a resend.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde_json::json;

use crate::link::frames::{ENTRY_BOUND, FromClient};

/// **How many entries wait for the server's acknowledgement** before the
/// oldest is dropped.
pub const BACKLOG_ENTRIES: usize = 1024;
/// **How many bytes of entries wait** before the oldest is dropped.
pub const BACKLOG_BYTES: usize = 1024 * 1024;
/// **How many entries one connection has sent and not had acknowledged**
/// before its pump waits for an acknowledgement, so entries never take
/// more than a small share of the write path the trace and the answers
/// ride.
pub const WINDOW: usize = 16;

struct Waiting {
    seq: u64,
    bytes: usize,
    frame: FromClient,
}

struct Inner {
    run: String,
    next: u64,
    backlog: VecDeque<Waiting>,
    bytes: usize,
    /// Dropped since the last report.
    dropped: u64,
    /// The connector's own source, for its reports.
    source: &'static str,
    /// Acknowledgements taken, the pump's measure of progress.
    acks: u64,
}

/// **The connector's backlog of entries**, cloned wherever an entry is met.
#[derive(Clone)]
pub struct Entries {
    inner: Arc<Mutex<Inner>>,
    wake: Arc<tokio::sync::Notify>,
}

impl Entries {
    /// A backlog for a connector whose own entries carry `source`
    /// (`admin-con` or `gate-con`), with a run drawn now.
    pub fn new(source: &'static str) -> Self {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).expect("the OS draws random bytes");
        let run = bytes.iter().map(|b| format!("{b:02x}")).collect();
        Self {
            inner: Arc::new(Mutex::new(Inner {
                run,
                next: 1,
                backlog: VecDeque::new(),
                bytes: 0,
                dropped: 0,
                source,
                acks: 0,
            })),
            wake: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// This connector's run, drawn at its start.
    pub fn run(&self) -> String {
        self.inner.lock().unwrap().run.clone()
    }

    /// **Queue an entry, never waiting**: past [`ENTRY_BOUND`] the entry
    /// is replaced by one saying so; past the backlog's bounds the oldest
    /// is dropped and counted.
    pub fn push(&self, source: &str, entry: serde_json::Value) {
        let mut inner = self.inner.lock().unwrap();
        push_in(&mut inner, source, entry);
        drop(inner);
        self.wake.notify_one();
    }

    /// The connector's own failure, from its own source: `kind` names it
    /// and is the alert's subject at the server.
    pub fn failure(&self, kind: &str, message: impl Into<String>) {
        let source = self.inner.lock().unwrap().source;
        self.push(source, json!({ "kind": kind, "message": message.into() }));
    }

    /// **The server took the run's entries through `seq`**: they leave the
    /// backlog, and drops counted meanwhile are reported now that there is
    /// room.
    pub fn ack(&self, run: &str, seq: u64) {
        let mut inner = self.inner.lock().unwrap();
        if inner.run != run {
            return;
        }
        inner.acks += 1;
        while inner.backlog.front().is_some_and(|w| w.seq <= seq) {
            let gone = inner.backlog.pop_front().expect("a front");
            inner.bytes -= gone.bytes;
        }
        report_drops(&mut inner);
        drop(inner);
        self.wake.notify_one();
    }

    /// **At a connection's start**: drops counted while the link was down
    /// are reported as one entry.
    pub(crate) fn reconnected(&self) {
        let mut inner = self.inner.lock().unwrap();
        report_drops(&mut inner);
        drop(inner);
        self.wake.notify_one();
    }

    /// The next entry past `sent` to send on a connection, where fewer
    /// than [`WINDOW`] sent on it wait for their acknowledgement.
    pub(crate) fn next_after(&self, sent: u64) -> Option<(u64, FromClient)> {
        let inner = self.inner.lock().unwrap();
        let in_flight = inner.backlog.iter().filter(|w| w.seq <= sent).count();
        if in_flight >= WINDOW {
            return None;
        }
        inner
            .backlog
            .iter()
            .find(|w| w.seq > sent)
            .map(|w| (w.seq, w.frame.clone()))
    }

    /// Wait for an entry pushed or acknowledged since.
    pub(crate) async fn changed(&self) {
        self.wake.notified().await;
    }

    /// How many acknowledgements were taken: the pump's measure of
    /// progress.
    pub(crate) fn acks(&self) -> u64 {
        self.inner.lock().unwrap().acks
    }

    /// Whether any entry sent on a connection, through `sent`, still
    /// waits for its acknowledgement.
    pub(crate) fn unacknowledged(&self, sent: u64) -> bool {
        self.inner
            .lock()
            .unwrap()
            .backlog
            .front()
            .is_some_and(|w| w.seq <= sent)
    }

    /// The entries waiting, by source and object, oldest first.
    #[cfg(test)]
    pub fn pending(&self) -> Vec<(String, serde_json::Value)> {
        self.inner
            .lock()
            .unwrap()
            .backlog
            .iter()
            .filter_map(|w| match &w.frame {
                FromClient::Entry { source, entry, .. } => Some((source.clone(), entry.clone())),
                _ => None,
            })
            .collect()
    }

    /// How many entries wait, and how many were dropped since the last
    /// report.
    #[cfg(test)]
    pub fn held(&self) -> (usize, u64) {
        let inner = self.inner.lock().unwrap();
        (inner.backlog.len(), inner.dropped)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn push_in(inner: &mut Inner, source: &str, entry: serde_json::Value) {
    let rendered = serde_json::to_string(&entry)
        .map(|s| s.len())
        .unwrap_or(usize::MAX);
    // **Every shape sent is one the door takes for its source**, the
    // fallback's included: `admin_log` has its own `oversized` marker, and
    // a connector's own source the failure shape.
    let entry = if rendered > ENTRY_BOUND {
        let said =
            format!("an entry of {rendered} bytes, past the bound of {ENTRY_BOUND}, was not sent");
        if source == crate::link::admin_log::SOURCE {
            json!({ "oversized": said, "own": false, "read_at": now_ms() })
        } else {
            json!({ "kind": "entry_oversized", "message": said })
        }
    } else {
        entry
    };
    let bytes = serde_json::to_string(&entry).map(|s| s.len()).unwrap_or(0) + source.len();
    let seq = inner.next;
    inner.next += 1;
    let frame = FromClient::Entry {
        run: inner.run.clone(),
        seq,
        source: source.to_owned(),
        entry,
    };
    inner.backlog.push_back(Waiting { seq, bytes, frame });
    inner.bytes += bytes;
    while inner.backlog.len() > BACKLOG_ENTRIES || inner.bytes > BACKLOG_BYTES {
        let gone = inner
            .backlog
            .pop_front()
            .expect("over a bound, so not empty");
        inner.bytes -= gone.bytes;
        inner.dropped += 1;
    }
}

/// **The drops, as one entry**, where there were any and there is room for
/// it without dropping another.
fn report_drops(inner: &mut Inner) {
    if inner.dropped == 0 || inner.backlog.len() >= BACKLOG_ENTRIES {
        return;
    }
    let count = std::mem::take(&mut inner.dropped);
    tracing::error!("{count} log entries were dropped from the backlog, oldest first");
    let source = inner.source;
    push_in(
        inner,
        source,
        json!({
            "kind": "entries_dropped",
            "message": format!("{count} entries were dropped from the backlog, oldest first, while the server could not take them"),
            "count": count,
        }),
    );
}
