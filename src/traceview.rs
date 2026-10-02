//! The trace view (Spec section 6), split across the link (Spec
//! section 8): admin-con tails the agent's NDJSON file and streams every
//! event and mark over the link (`link::admin_con`), and the server holds
//! the bounded rings and the per-agent broadcast the views render from.
//! Rotation, truncation, and link loss all surface as discontinuity marks,
//! never smoothed.
//!
//! **Section 8 charters the link and names none of its frames**, so the
//! roster below and the discontinuity mark cite it for the link and not
//! for the thing named. Discontinuity appears nowhere in that Spec and
//! roster appears once, in another sense.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

const RING_CAP: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub seq: u64,
    /// None for real events; Some(reason) for discontinuity marks the
    /// tailer or the server inserted (rotation, truncation, parse
    /// failure, link loss).
    pub mark: Option<String>,
    pub run: Option<String>,
    pub turn: Option<String>,
    pub kind: Option<String>,
    pub raw: serde_json::Value,
}

// ---------- server half: rings and broadcast ----------

struct View {
    ring: Mutex<VecDeque<TraceEvent>>,
    tx: broadcast::Sender<TraceEvent>,
}

/// The server's per-agent views, fed by the link. Agents register
/// dynamically as hellos announce them, roster-by-hello (Spec
/// section 8, and the frame rule in this module's header). Views are
/// never removed: a view over a departed agent stays readable,
/// honestly stale.
#[derive(Clone)]
pub struct TraceViews {
    views: Arc<Mutex<HashMap<String, Arc<View>>>>,
}

impl TraceViews {
    pub fn new() -> Self {
        Self {
            views: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn ensure(&self, agent: &str) {
        self.views
            .lock()
            .unwrap()
            .entry(agent.to_owned())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(1024);
                Arc::new(View {
                    ring: Mutex::new(VecDeque::with_capacity(RING_CAP)),
                    tx,
                })
            });
    }

    fn view(&self, agent: &str) -> Option<Arc<View>> {
        self.views.lock().unwrap().get(agent).cloned()
    }

    /// Ingest one event from the link. Sequence numbers are clamped
    /// monotonic per view: the connector's epoch-based numbering can
    /// fold back on a same-second reconnect, and a view's own order
    /// must not.
    pub fn ingest(&self, agent: &str, mut ev: TraceEvent) {
        self.ensure(agent);
        let Some(view) = self.view(agent) else { return };
        {
            let mut ring = view.ring.lock().unwrap();
            if let Some(last) = ring.back()
                && ev.seq <= last.seq
            {
                ev.seq = last.seq + 1;
            }
            if ring.len() == RING_CAP {
                ring.pop_front();
            }
            ring.push_back(ev.clone());
        }
        let _ = view.tx.send(ev);
    }

    /// Insert a server-authored discontinuity mark into one agent's
    /// view - the link's own honesty (Spec section 8).
    pub fn mark(&self, agent: &str, reason: &str) {
        self.ensure(agent);
        let Some(view) = self.view(agent) else { return };
        let seq = view
            .ring
            .lock()
            .unwrap()
            .back()
            .map(|e| e.seq + 1)
            .unwrap_or(1);
        let ev = TraceEvent {
            seq,
            mark: Some(reason.to_owned()),
            run: None,
            turn: None,
            kind: None,
            raw: serde_json::Value::Null,
        };
        self.ingest(agent, ev);
    }

    /// Whether a view exists and holds anything - the pump's test for
    /// bracketing a reconnect's fresh backfill.
    pub fn has_events(&self, agent: &str) -> bool {
        self.view(agent)
            .map(|v| !v.ring.lock().unwrap().is_empty())
            .unwrap_or(false)
    }

    pub fn snapshot(&self, agent: &str) -> Option<Vec<TraceEvent>> {
        self.view(agent)
            .map(|v| v.ring.lock().unwrap().iter().cloned().collect())
    }

    pub fn subscribe(&self, agent: &str) -> Option<broadcast::Receiver<TraceEvent>> {
        self.view(agent).map(|v| v.tx.subscribe())
    }
}

impl Default for TraceViews {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- one record as an event ----------

pub(crate) fn parse_line(seq: u64, line: &str) -> TraceEvent {
    match serde_json::from_str::<serde_json::Value>(line) {
        Ok(raw) => {
            let get = |k: &str| raw.get(k).and_then(|v| v.as_str()).map(str::to_owned);
            TraceEvent {
                seq,
                mark: None,
                run: get("run"),
                turn: get("turn"),
                kind: get("kind"),
                raw,
            }
        }
        Err(e) => TraceEvent {
            seq,
            mark: Some(format!("line did not parse as JSON: {e}")),
            run: None,
            turn: None,
            kind: None,
            raw: serde_json::Value::String(line.to_owned()),
        },
    }
}
