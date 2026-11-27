//! conforms: web-an-alert-only-for-what-did-not-come-through-weaverweb
//! conforms: web-the-admin-log-comes-through-the-service
//!
//! **admin-con reads the box's `admin.log` through the weaver-admin
//! service** (Spec 2.14, 7.2; `weaver-admin-operator-contract` section
//! 8.6): it holds one `log` connection on the service's socket, and each
//! message the service sends becomes one entry, sent upstream through
//! [`Entries`]. admin-con reads no file.
//!
//! **One field is the service's judgment, carried and not made here**:
//! `own` is the message's `through_service`, true where the line names the
//! service's account as its caller or peer, which is an act that came
//! through WeaverWeb. The server raises an alert only for a line that is
//! not own, since its own acts raise theirs at the door. A line that is
//! not one JSON object is carried as text, `unparsed`; a line past the
//! service's bound is an `oversized` marker; a truncation or replacement of
//! the file is a discontinuity, each as before.
//!
//! **Every gap is marked**: any close of the connection, and an opening
//! admitted after a refusal or a failed opening, is a discontinuity entry,
//! since lines appended meanwhile were not read. A reopening backs off from
//! `BACKOFF.base` to `BACKOFF.cap`, except after `log_behind`, which is
//! reopened at once. A refusal of the opening by `log_unread` or by a word
//! that says admin-con or its config is at fault raises one connector
//! failure entry per change of word, not one at every retry.
//!
//! **It runs as a task of its own**: a slow or silent service never holds
//! the trace or the verbs, and it ends at the stop or where the stop's
//! sender is gone.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use tokio::sync::watch;

use crate::link::admin_service::{
    self, ANSWER_WAIT, Discontinuity, LogEvent, LogLine, Opened, Purpose, word,
};
use crate::link::client::Backoff;
use crate::link::entries::Entries;
use crate::link::service_invoker::{entry_kind, is_defect};

/// The entries' source for a line of `admin.log`.
pub const SOURCE: &str = "admin_log";

/// **The reopening's backoff**, 1 to 30 seconds (the brief's decision 6).
pub const BACKOFF: Backoff = Backoff {
    base: Duration::from_secs(1),
    cap: Duration::from_secs(30),
};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// **The entry for one message of the log connection**, the line as admin
/// wrote it beside admin-con's own fields.
pub fn entry_of(event: &LogEvent) -> serde_json::Value {
    match event {
        LogEvent::Line {
            line,
            through_service,
        } => json!({ "line": line, "own": through_service, "read_at": now_ms() }),
        LogEvent::Text {
            text,
            through_service,
        } => json!({
            "line": text,
            "unparsed": true,
            "own": through_service,
            "read_at": now_ms(),
        }),
        LogEvent::Oversized(len) => json!({
            "oversized": format!("a line of admin.log of {len} bytes, past the service's bound, was not sent"),
            "own": false,
            "read_at": now_ms(),
        }),
        LogEvent::Discontinuity(Discontinuity::Truncated) => {
            discontinuity("admin.log was truncated; read from its start")
        }
        LogEvent::Discontinuity(Discontinuity::Replaced) => {
            discontinuity("admin.log was replaced; read from its start")
        }
    }
}

/// A discontinuity, as an entry.
pub(crate) fn discontinuity(why: &str) -> serde_json::Value {
    json!({ "discontinuity": why, "read_at": now_ms() })
}

/// Whether a refusal of the log opening by `w` raises an entry: `log_unread`,
/// admin-con's defects, and a word the contract does not give the opening.
fn raises(w: &str) -> bool {
    w == word::LOG_UNREAD || is_defect(w) || !(word::OPENING.contains(&w))
}

/// **Read `admin.log` through the service at `socket` until `shutdown`**,
/// as a task of its own, each message pushed to `entries`.
pub fn follow(
    socket: PathBuf,
    agent: String,
    entries: Entries,
    backoff: Backoff,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // **A gap since the last connection**: a refusal or a failed opening,
        // so the next admission is marked.
        let mut gap = false;
        let mut last_word: Option<String> = None;
        let mut failures: u32 = 0;
        loop {
            if *shutdown.borrow() {
                return;
            }
            let opened = tokio::select! {
                opened = admin_service::open(&socket, Purpose::Log, &agent, ANSWER_WAIT) => opened,
                _ = shutdown.changed() => return,
            };
            let mut at_once = false;
            match opened {
                Ok(Opened::Open(mut conn)) => {
                    if std::mem::take(&mut gap) {
                        entries.push(
                            SOURCE,
                            discontinuity(
                                "admin.log was not read while its log connection was refused or could not open; lines appended meanwhile may be missing",
                            ),
                        );
                    }
                    last_word = None;
                    failures = 0;
                    loop {
                        let line = tokio::select! {
                            line = conn.next_log() => line,
                            _ = shutdown.changed() => return,
                        };
                        match line {
                            Ok(Some(LogLine::Log(event))) => entries.push(SOURCE, entry_of(&event)),
                            Ok(Some(LogLine::Refused(refusal)))
                                if refusal.word == word::LOG_BEHIND =>
                            {
                                entries.push(
                                    SOURCE,
                                    discontinuity(&format!(
                                        "the service fell behind admin-con's reading and closed the log connection ({}); lines are missing",
                                        refusal.detail
                                    )),
                                );
                                at_once = true;
                                break;
                            }
                            Ok(Some(LogLine::Refused(refusal))) => {
                                let why = format!(
                                    "the service closed the log connection with {}: {}",
                                    refusal.word, refusal.detail
                                );
                                tracing::error!("{why}");
                                entries.failure(&entry_kind(&refusal.word), why.clone());
                                entries.push(
                                    SOURCE,
                                    discontinuity(&format!(
                                        "{why}; lines appended meanwhile may be missing"
                                    )),
                                );
                                break;
                            }
                            Ok(None)
                            | Err(
                                admin_service::LineError::Io(_)
                                | admin_service::LineError::Truncated,
                            ) => {
                                entries.push(
                                    SOURCE,
                                    discontinuity("the log connection closed; lines appended until it reopens may be missing"),
                                );
                                break;
                            }
                            Err(e) => {
                                let why = format!("the log connection carried {e}");
                                entries.failure("service_malformed", why.clone());
                                entries.push(SOURCE, discontinuity(&format!("{why}; it was closed, and lines appended meanwhile may be missing")));
                                break;
                            }
                        }
                    }
                }
                Ok(Opened::Refused(refusal)) => {
                    gap = true;
                    if last_word.as_deref() != Some(refusal.word.as_str()) {
                        let why = format!(
                            "the service refused the log connection, {}: {}",
                            refusal.word, refusal.detail
                        );
                        tracing::warn!("{why}");
                        if raises(&refusal.word) {
                            entries.failure(&entry_kind(&refusal.word), why);
                        }
                        last_word = Some(refusal.word);
                    }
                }
                Err(e) => {
                    gap = true;
                    let why = format!("the log connection did not open ({e})");
                    if last_word.as_deref() != Some(why.as_str()) {
                        tracing::warn!("{why}");
                        last_word = Some(why);
                    }
                }
            }
            if at_once {
                continue;
            }
            failures = failures.saturating_add(1);
            tokio::select! {
                _ = tokio::time::sleep(backoff.delay(failures)) => {}
                _ = shutdown.changed() => return,
            }
        }
    })
}
