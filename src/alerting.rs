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

use crate::store::alerts::Raise;
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
    Entry(Box<Entry>),
    Restored { agent: AgentId, plane: &'static str },
}

/// **The raising half**, cloned wherever a failure is met.
#[derive(Clone)]
pub struct Alerts {
    tx: mpsc::Sender<Note>,
    dropped: Arc<AtomicU64>,
    #[cfg(test)]
    stall: Arc<tokio::sync::Mutex<()>>,
}

impl Alerts {
    /// Start the worker, which writes alerts to the store and lines to the
    /// operations log where one is configured.
    pub fn start(store: Store, log: Option<PathBuf>) -> Self {
        let (tx, rx) = mpsc::channel(QUEUE);
        let alerts = Self {
            tx,
            dropped: Arc::new(AtomicU64::new(0)),
            #[cfg(test)]
            stall: Arc::new(tokio::sync::Mutex::new(())),
        };
        tokio::spawn(work(
            store,
            log,
            rx,
            #[cfg(test)]
            alerts.stall.clone(),
        ));
        alerts
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

    /// **A connector's entry, never waiting**: dropped and counted like a
    /// failure where the queue is full.
    pub fn entry(&self, entry: Entry) {
        if self.tx.try_send(Note::Entry(Box::new(entry))).is_err() {
            let dropped = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::error!(
                "the alerts' queue is full or gone: an entry dropped, {dropped} dropped in all"
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
            Note::Entry(entry) => {
                let alert = match &entry.raise {
                    Some(raise) => raised(&store, raise).await,
                    None => None,
                };
                if let Some(path) = &log {
                    let line = serde_json::json!({
                        "time": chrono::Utc::now().to_rfc3339(),
                        "agent": entry.agent.as_str(),
                        "plane": entry.plane,
                        "source": entry.source,
                        "own": entry.own,
                        "entry": entry.entry,
                        "alert": alert,
                    });
                    append(path, line).await;
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

/// One line appended to the operations log.
async fn append(path: &PathBuf, line: serde_json::Value) {
    let mut bytes = line.to_string().into_bytes();
    bytes.push(b'\n');
    let written = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .await?;
        file.write_all(&bytes).await?;
        file.flush().await
    };
    if let Err(e) = written.await {
        tracing::error!("the operations log could not be written: {e}");
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
