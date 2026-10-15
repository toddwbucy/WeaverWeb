//! conforms: web-raising-an-alert-never-waits
//!
//! **The server's alerts and its operations log** (Spec 2.14; act 12, PR
//! 1; the operator's rulings of 2026-10-14): a failure the listener meets,
//! an act failed or unknown, a refusal for a system reason, lost contact, a
//! failure that could lose data, becomes an alert on the agent (the store's
//! `alerts`) and one line in the operations log, a file the server appends
//! to, whose path is the config's (no default, no path in this tree).
//!
//! **Raising never waits.** The connection task and an ask hand a failure
//! to a bounded queue with `try_send` and go on; a worker of its own writes
//! the alert, then the log line naming it. A full queue drops the failure
//! and counts the drops, the count logged at error, so a stalled store or a
//! full disk never holds a connection, a teardown or an ask.
//!
//! **Never a turn's text**: a failure carries the act's name (a verb, or
//! `turn`) and the reason's fields, which are admin's refusal object, a
//! connector's fault, the gate's close reason or a teardown's reason.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

use crate::store::alerts::{InboxRow, Raise};
use crate::store::{AgentId, Store};

/// **How many failures wait for the worker** before a raise drops one.
const QUEUE: usize = 256;

/// How long the worker gives the store for one alert before logging the
/// failure without its identity.
const STORE_BOUND: Duration = Duration::from_secs(10);

/// A failure, as raised.
#[derive(Debug, Clone)]
pub struct Failure {
    pub raise: Raise,
    /// `admin` or `gate`.
    pub plane: &'static str,
    /// The principal of the act: `server`, or the person's `pe-` identity;
    /// `server` too for lost contact.
    pub principal: String,
    /// The outcome or the refusal: `failed`, `unknown`, `refused`,
    /// `lost_contact`.
    pub outcome: &'static str,
    /// The reason's fields, as the log line carries them.
    pub reason: serde_json::Value,
}

/// **A connector's entry, as the server took it** (Spec 2.14; act 12, PR
/// 2): one line of the operations log, and the alert it raises where it
/// raises one.
#[derive(Debug, Clone)]
pub struct Entry {
    pub agent: AgentId,
    pub plane: &'static str,
    pub source: String,
    /// For a line of `admin.log`, whether admin-con found it its own.
    pub own: Option<bool>,
    pub entry: serde_json::Value,
    pub raise: Option<Raise>,
}

enum Note {
    Failure(Box<Failure>),
    Restored { agent: AgentId, plane: &'static str },
}

/// **The raising half**, cloned wherever a failure is met.
#[derive(Clone)]
pub struct Alerts {
    tx: mpsc::Sender<Note>,
    dropped: Arc<AtomicU64>,
    /// Wakes the inbox's drainer: an entry was taken.
    inbox: Arc<tokio::sync::Notify>,
    #[cfg(test)]
    stall: Arc<tokio::sync::Mutex<()>>,
    /// A test's lever: the drainer's next line fails once.
    #[cfg(test)]
    fail_line: Arc<std::sync::atomic::AtomicBool>,
}

impl Alerts {
    /// Start the worker, which writes alerts to the store and lines to the
    /// operations log where one is configured.
    pub fn start(store: Store, log: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE);
        let alerts = Self {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
            inbox: Arc::new(tokio::sync::Notify::new()),
            #[cfg(test)]
            stall: Arc::new(tokio::sync::Mutex::new(())),
            #[cfg(test)]
            fail_line: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        tokio::spawn(work(
            store.clone(),
            log.clone(),
            rx,
            #[cfg(test)]
            alerts.stall.clone(),
        ));
        tokio::spawn(drain(store, log, alerts.clone()));
        alerts
    }

    /// **An entry was taken into the inbox**: the drainer is woken, never
    /// waited on.
    pub fn wake_inbox(&self) {
        self.inbox.notify_one();
    }

    /// The drainer's next line fails once.
    #[cfg(test)]
    pub fn fail_next_line(&self) {
        self.fail_line.store(true, Ordering::Relaxed);
    }

    /// **Raise a failure, never waiting**: a full queue drops it, counts
    /// the drop, and logs the count at error.
    pub fn raise(&self, failure: Failure) {
        if self.tx.try_send(Note::Failure(Box::new(failure))).is_err() {
            let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::error!(
                "the alerts' queue is full or gone: a failure dropped, {dropped} dropped in all"
            );
        }
    }

    /// Contact restored on the plane: the open lost-contact alert notes
    /// when. Dropped like a failure where the queue is full.
    pub fn restored(&self, agent: &AgentId, plane: &'static str) {
        if self
            .tx
            .try_send(Note::Restored {
                agent: agent.clone(),
                plane,
            })
            .is_err()
        {
            let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::error!(
                "the alerts' queue is full or gone: a restoration dropped, {dropped} dropped in all"
            );
        }
    }

    /// How many raises the full queue dropped.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Hold the worker until the guard drops, so a test can fill the queue.
    #[cfg(test)]
    pub async fn stall_worker(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.stall.clone().lock_owned().await
    }
}

async fn work(
    store: Store,
    log: Option<PathBuf>,
    mut rx: mpsc::Receiver<Note>,
    #[cfg(test)] stall: Arc<tokio::sync::Mutex<()>>,
) {
    while let Some(note) = rx.recv().await {
        #[cfg(test)]
        drop(stall.lock().await);
        match note {
            Note::Restored { agent, plane } => {
                match tokio::time::timeout(STORE_BOUND, store.alert_contact_restored(&agent, plane))
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => tracing::error!(
                        "{agent} ({plane}): contact's restoration could not be noted on its alert: {e:#}"
                    ),
                    Err(_) => tracing::error!(
                        "{agent} ({plane}): contact's restoration was not noted within {STORE_BOUND:?}"
                    ),
                }
            }
            Note::Failure(failure) => {
                let alert = raised(&store, &failure.raise).await;
                if let Some(path) = &log {
                    write_line(path, &failure, alert.as_deref()).await;
                }
            }
        }
    }
}

/// The alert raised, bounded; its identity, none where nothing was raised.
async fn raised(store: &Store, raise: &Raise) -> Option<String> {
    match tokio::time::timeout(STORE_BOUND, store.raise_alert(raise)).await {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => {
            tracing::error!("{}: an alert could not be raised: {e:#}", raise.agent);
            None
        }
        Err(_) => {
            tracing::error!(
                "{}: an alert was not raised within {STORE_BOUND:?}",
                raise.agent
            );
            None
        }
    }
}

/// **One JSON line per failure**, the file opened per write with append, so
/// a rotation needs no signal. A line that cannot be written is logged at
/// error and the worker goes on.
async fn write_line(path: &PathBuf, failure: &Failure, alert: Option<&str>) {
    let line = serde_json::json!({
        "time": chrono::Utc::now().to_rfc3339(),
        "agent": failure.raise.agent.as_str(),
        "plane": failure.plane,
        "act": failure.raise.subject,
        "first": failure.raise.first,
        "principal": failure.principal,
        "outcome": failure.outcome,
        "kind": failure.raise.kind.as_str(),
        "summary": failure.raise.summary,
        "reason": failure.reason,
        "alert": alert,
    });
    append(path, line).await;
}

/// One line appended to the operations log, a failure logged at error.
async fn append(path: &PathBuf, line: serde_json::Value) {
    if let Err(e) = appended(path, line).await {
        tracing::error!("the operations log could not be written: {e}");
    }
}

/// One line appended to the operations log, or why not.
async fn appended(path: &PathBuf, line: serde_json::Value) -> std::io::Result<()> {
    let mut bytes = line.to_string().into_bytes();
    bytes.push(b'\n');
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(&bytes).await?;
    file.flush().await
}

/// How often the drainer looks at the inbox with nothing woken it: a
/// restart's rows, and a row whose line or alert failed, are taken again.
const DRAIN_PERIOD: Duration = Duration::from_secs(1);

/// **The inbox's drainer** (Spec 2.14): each entry taken, oldest first,
/// applied from its durable copy, its alert raised and kept on its row,
/// then its operations-log line written, then the row deleted. A step that
/// fails leaves the row for the next look, its alert never raised twice;
/// the line is written at least once.
async fn drain(store: Store, log: Option<PathBuf>, alerts: Alerts) {
    loop {
        #[cfg(test)]
        drop(alerts.stall.lock().await);
        let rows = match tokio::time::timeout(STORE_BOUND, store.inbox(64)).await {
            Ok(Ok(rows)) => rows,
            Ok(Err(e)) => {
                tracing::error!("the entries' inbox could not be read: {e:#}");
                Vec::new()
            }
            Err(_) => {
                tracing::error!("the entries' inbox was not read within {STORE_BOUND:?}");
                Vec::new()
            }
        };
        let mut stuck = rows.is_empty();
        for row in rows {
            if !apply(&store, log.as_ref(), &alerts, &row).await {
                stuck = true;
                break;
            }
        }
        if stuck {
            tokio::select! {
                () = alerts.inbox.notified() => {}
                () = tokio::time::sleep(DRAIN_PERIOD) => {}
            }
        }
    }
}

/// One row applied, or false where a step failed and it stays.
async fn apply(store: &Store, log: Option<&PathBuf>, alerts: &Alerts, row: &InboxRow) -> bool {
    let Some(effect) = crate::link::listener::entry_effect(row) else {
        tracing::error!(
            "an inbox row ({} {}) no longer passes the door's check; removed",
            row.run_id,
            row.seq
        );
        return bounded(store.inbox_done(row.inbox_id)).await;
    };
    let mut alert = row.alert_id.clone();
    if alert.is_none()
        && let Some(raise) = &effect.raise
    {
        match tokio::time::timeout(STORE_BOUND, store.raise_alert(raise)).await {
            Ok(Ok(Some(id))) => {
                if !bounded(store.inbox_raised(row.inbox_id, &id)).await {
                    return false;
                }
                alert = Some(id);
            }
            Ok(Ok(None)) => {}
            Ok(Err(e)) => {
                tracing::error!(
                    "{}: an entry's alert could not be raised: {e:#}",
                    raise.agent
                );
                return false;
            }
            Err(_) => {
                tracing::error!(
                    "{}: an entry's alert was not raised within {STORE_BOUND:?}",
                    raise.agent
                );
                return false;
            }
        }
    }
    if let Some(path) = log {
        let line = serde_json::json!({
            "time": chrono::Utc::now().to_rfc3339(),
            "agent": effect.agent.as_str(),
            "plane": effect.plane,
            "source": effect.source,
            "own": effect.own,
            "entry": effect.entry,
            "alert": alert,
        });
        #[cfg(test)]
        let written = if alerts.fail_line.swap(false, Ordering::Relaxed) {
            Err(std::io::Error::other("a test fault failed an entry's line"))
        } else {
            appended(path, line).await
        };
        #[cfg(not(test))]
        let written = appended(path, line).await;
        // **A line not written leaves its row**, to be written at the next
        // look, its alert already kept on the row.
        if let Err(e) = written {
            tracing::error!("an entry's operations-log line could not be written: {e}");
            return false;
        }
    }
    let _ = alerts;
    bounded(store.inbox_done(row.inbox_id)).await
}

/// A store step bounded, true where it completed.
async fn bounded(step: impl std::future::Future<Output = anyhow::Result<()>>) -> bool {
    match tokio::time::timeout(STORE_BOUND, step).await {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::error!("the entries' inbox could not be written: {e:#}");
            false
        }
        Err(_) => {
            tracing::error!("the entries' inbox was not written within {STORE_BOUND:?}");
            false
        }
    }
}

/// **The operations log can be appended to**, checked at the server's
/// start, which refuses where it cannot.
pub fn appendable(path: &PathBuf) -> anyhow::Result<()> {
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(|_| ())
        .map_err(|e| {
            anyhow::anyhow!(
                "operations_log {} cannot be opened for appending: {e}",
                path.display()
            )
        })
}
