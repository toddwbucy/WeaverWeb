//! The trace view (Spec section 6), split across the link (Spec
//! section 8): the connector tails each agent's NDJSON file and streams
//! every event and mark over the link, the server holds the bounded
//! rings and the per-agent broadcast the views render from. Rotation,
//! truncation, and link loss all surface as discontinuity marks, never
//! smoothed.
//!
//! **Section 8 charters the link and names none of its frames**, so the
//! roster below and the discontinuity mark cite it for the link and not
//! for the thing named. Discontinuity appears nowhere in that Spec and
//! roster appears once, in another sense.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::{broadcast, mpsc};

const BACKFILL_BYTES: u64 = 1024 * 1024;
const RING_CAP: usize = 10_000;
const POLL_MS: u64 = 500;

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

// ---------- connector half: the tailer ----------

fn parse_line(seq: u64, line: &str) -> TraceEvent {
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

/// Tail one agent's NDJSON sink: backfill a bounded window, then
/// follow appends by polling. Every event and mark goes to `out`,
/// whose closed end is the tailer's stop signal (the link connection
/// this tailer serves is gone).
pub async fn tail_task(path: PathBuf, out: mpsc::Sender<TraceEvent>) {
    use std::os::unix::fs::MetadataExt;
    // Sequence ids stay monotonic across connector restarts and
    // redials (a reconnecting browser replays Last-Event-ID minted
    // before either), by basing them on the epoch second at start.
    // The server clamps per-view monotonicity for the residue.
    let mut seq: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        * 1_000_000;
    let mut pos: u64 = 0;
    let mut opened = false;
    let mut skip_partial_first = false;
    let mut identity: Option<(u64, u64)> = None;

    loop {
        // The receiver's end is the stop signal even when a quiet or
        // absent sink gives this poll nothing to send - without this
        // check an idle tailer outlives every connection it served.
        if out.is_closed() {
            return;
        }
        match File::open(&path).await {
            Ok(mut file) => {
                // A failed stat is a fact about this poll, not a
                // zero-length file: substituting zero fired a false
                // truncation mark. Skip the poll and ask again.
                let meta = match file.metadata().await {
                    Ok(m) => m,
                    Err(_) => {
                        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
                        continue;
                    }
                };
                let len = meta.len();
                let ident = Some((meta.dev(), meta.ino()));
                if !opened {
                    // Backfill window on first open.
                    pos = len.saturating_sub(BACKFILL_BYTES);
                    opened = true;
                    identity = ident;
                    if pos > 0 {
                        skip_partial_first = true;
                        seq += 1;
                        let ev = TraceEvent {
                            seq,
                            mark: Some(format!("backfill starts {pos} bytes into the file")),
                            run: None,
                            turn: None,
                            kind: None,
                            raw: serde_json::Value::Null,
                        };
                        if out.send(ev).await.is_err() {
                            return;
                        }
                    }
                } else if (ident.is_some() && identity.is_some() && ident != identity) || len < pos
                {
                    // A different inode, or a shrink in place: either
                    // way a discontinuity, marked, never smoothed.
                    seq += 1;
                    let ev = TraceEvent {
                        seq,
                        mark: Some(if ident != identity {
                            "file replaced: rotation".into()
                        } else {
                            "file shrank: truncation".into()
                        }),
                        run: None,
                        turn: None,
                        kind: None,
                        raw: serde_json::Value::Null,
                    };
                    if out.send(ev).await.is_err() {
                        return;
                    }
                    pos = 0;
                    identity = ident;
                    skip_partial_first = false;
                }
                if len > pos && file.seek(SeekFrom::Start(pos)).await.is_ok() {
                    let mut reader = BufReader::new(file);
                    // Bytes to the delimiter, decoded lossily: a
                    // non-UTF-8 record surfaces as a mark (the JSON
                    // parse refuses the replacement characters) where
                    // read_line would stall on it forever, re-reading
                    // the same offset every poll.
                    let mut buf: Vec<u8> = Vec::new();
                    loop {
                        buf.clear();
                        match reader.read_until(b'\n', &mut buf).await {
                            Ok(0) => break,
                            Ok(n) => {
                                if buf.last() != Some(&b'\n') {
                                    // Incomplete tail: leave it for
                                    // the next poll, do not advance.
                                    break;
                                }
                                pos += n as u64;
                                if skip_partial_first {
                                    // The backfill seek landed
                                    // mid-line; this fragment is
                                    // not a whole record.
                                    skip_partial_first = false;
                                    continue;
                                }
                                // Validated, not lossily decoded, before
                                // any parse: replacement characters
                                // inside a JSON string still parse as
                                // valid JSON, which would smooth a
                                // corrupt record into a clean-looking
                                // event instead of a mark (review of
                                // #350, round three). Lossy decoding is
                                // for the mark's display only.
                                let ev = match std::str::from_utf8(&buf) {
                                    Ok(s) => {
                                        let trimmed = s.trim_end();
                                        if trimmed.is_empty() {
                                            continue;
                                        }
                                        seq += 1;
                                        parse_line(seq, trimmed)
                                    }
                                    Err(e) => {
                                        seq += 1;
                                        TraceEvent {
                                            seq,
                                            mark: Some(format!("record is not UTF-8: {e}")),
                                            run: None,
                                            turn: None,
                                            kind: None,
                                            raw: serde_json::Value::String(
                                                String::from_utf8_lossy(&buf).trim_end().to_owned(),
                                            ),
                                        }
                                    }
                                };
                                if out.send(ev).await.is_err() {
                                    return;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            Err(_) => {
                // File absent: the agent has never run or the sink
                // moved. Keep polling; absence is not an error here.
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(POLL_MS)).await;
    }
}
