//! **The console's half of the pairing** (Spec 8.2): a pairing connection
//! from its handshake to the moment a code matches it, and the table of
//! boxes waiting.
//!
//! **The waiting box is held in this process's memory alone.** Nothing
//! reaches the store before a code matches: no row, no audit record and no
//! log line per connection. At most `Bounds::total` connections wait and
//! `Bounds::per_peer` from one peer address, and one past either is refused
//! `busy`. The first frame arrives within `Bounds::first_frame` and holds at
//! most `PAIR_FIRST_FRAME_BOUND`. A heartbeat goes at the link's cadence
//! while the box waits, and `Bounds::life` from the dial the console closes
//! the connection `expired`.
//!
//! **The console holds no code.** At an entry, `Waiting::take` asks each
//! waiting connection's own task to derive that connection's code from its
//! exporter and compare; the task that matches answers with the box's claim
//! and leaves the table, so a code is spent with its connection.
//!
//! **A matched box keeps hearing the console** (`Kept`): from the match until
//! the exchange writes to it, and again while the exchange's write runs, a
//! task of its own sends the box a heartbeat at the same cadence, so a box
//! whose exchange waits on the authority lock or the store never goes silent.

use crate::link::frames::{
    Line, LineReader, PAIR_FIRST_FRAME_BOUND, PairFromBox, PairToBox, pair_reason,
};
use crate::link::pair_code::Code;
use crate::link::verbs::is_box;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, watch};
use tokio_rustls::StartHandshake;
use tokio_rustls::server::TlsStream;

/// The bound on a waiting connection's answer to an entry's probe: a task
/// held longer on a write is passed over for that entry.
const PROBE_SECS: u64 = 5;

/// **The waiting boxes' bounds** (Spec 8.2), each a constant of the Spec's
/// and set shorter only by a test.
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub total: usize,
    pub per_peer: usize,
    pub first_frame: Duration,
    pub life: Duration,
}

impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            total: 16,
            per_peer: 4,
            first_frame: Duration::from_secs(10),
            life: Duration::from_secs(600),
        }
    }
}

/// A waiting box as the Pair page lists it: what the box claims, its
/// peer address and how long it has waited. Never its code.
#[derive(Debug, Clone)]
pub struct WaitingBox {
    pub r#box: String,
    pub peer: IpAddr,
    pub waited: Duration,
}

/// A pairing connection, framed, as the console holds it.
pub(crate) type PairConnection = LineReader<TlsStream<TcpStream>>;

/// **A pairing connection a code matched**, taken out of the table: what
/// the box claimed, and the connection itself, which the exchange carries
/// on (Spec 8.2). The match spends the connection, and its code matches
/// nothing after.
pub struct Taken {
    pub r#box: String,
    pub gate_request: String,
    pub admin_request: String,
    pub peer: IpAddr,
    /// The connection, kept alive by its heartbeats until the exchange
    /// writes to it, which only the exchange carries on, so a build without
    /// the enrollment closes it at the match.
    #[cfg(feature = "enrollment")]
    pub(crate) connection: Kept,
    /// The end of the box's life, `Bounds::life` from its dial: the longest
    /// its exchange waits for the authority lock.
    #[cfg(feature = "enrollment")]
    pub(crate) ends: tokio::time::Instant,
}

impl std::fmt::Debug for Taken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Taken")
            .field("box", &self.r#box)
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}

/// An entry's question to one waiting connection.
struct Probe {
    code: Code,
    answer: oneshot::Sender<Option<Taken>>,
}

struct Slot {
    peer: IpAddr,
    dialed: Instant,
    /// Set once the first frame is read: the box's claim and the way to
    /// probe its connection. A slot without it holds a place and is not
    /// listed.
    listed: Option<(String, mpsc::Sender<Probe>)>,
}

/// **The table of waiting connections**, in memory alone.
pub struct Waiting {
    bounds: Mutex<Bounds>,
    slots: Mutex<HashMap<u64, Slot>>,
    next: AtomicU64,
    /// One entry at a time, so a connection is never handed to two.
    entry: tokio::sync::Mutex<()>,
}

/// A held place in the table, given back when dropped.
struct Reservation<'a> {
    waiting: &'a Waiting,
    id: u64,
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        self.waiting.slots.lock().unwrap().remove(&self.id);
    }
}

impl Reservation<'_> {
    /// The place out of the table for a hand-over, so the box is listed
    /// nowhere once its entry holds it.
    fn lift(&self) -> Option<Slot> {
        self.waiting.slots.lock().unwrap().remove(&self.id)
    }

    /// The place back, its hand-over not taken.
    fn restore(&self, slot: Slot) {
        self.waiting.slots.lock().unwrap().insert(self.id, slot);
    }
}

impl Default for Waiting {
    fn default() -> Self {
        Waiting {
            bounds: Mutex::new(Bounds::default()),
            slots: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            entry: tokio::sync::Mutex::new(()),
        }
    }
}

impl Waiting {
    pub fn bounds(&self) -> Bounds {
        *self.bounds.lock().unwrap()
    }

    #[cfg(test)]
    pub(crate) fn set_bounds(&self, bounds: Bounds) {
        *self.bounds.lock().unwrap() = bounds;
    }

    /// A place for one connection, or none past either bound.
    fn reserve(&self, peer: IpAddr, dialed: Instant) -> Option<Reservation<'_>> {
        let bounds = self.bounds();
        let mut slots = self.slots.lock().unwrap();
        let from_peer = slots.values().filter(|s| s.peer == peer).count();
        if slots.len() >= bounds.total || from_peer >= bounds.per_peer {
            return None;
        }
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        slots.insert(
            id,
            Slot {
                peer,
                dialed,
                listed: None,
            },
        );
        Some(Reservation { waiting: self, id })
    }

    fn list_slot(&self, id: u64, r#box: String, probe: mpsc::Sender<Probe>) {
        if let Some(slot) = self.slots.lock().unwrap().get_mut(&id) {
            slot.listed = Some((r#box, probe));
        }
    }

    /// **The boxes waiting**, each by the name it claims, its peer address
    /// and how long it has waited, oldest first.
    pub fn list(&self) -> Vec<WaitingBox> {
        let slots = self.slots.lock().unwrap();
        let mut boxes: Vec<(Instant, WaitingBox)> = slots
            .values()
            .filter_map(|slot| {
                let (r#box, _) = slot.listed.as_ref()?;
                Some((
                    slot.dialed,
                    WaitingBox {
                        r#box: r#box.clone(),
                        peer: slot.peer,
                        waited: slot.dialed.elapsed(),
                    },
                ))
            })
            .collect();
        boxes.sort_by_key(|(dialed, _)| *dialed);
        boxes.into_iter().map(|(_, b)| b).collect()
    }

    /// **The waiting connection whose code is `code`**, taken out of the
    /// table, or none. Each connection derives its own code at this entry
    /// and compares, so the table holds no code.
    pub async fn take(&self, code: &Code) -> Option<Taken> {
        let _one = self.entry.lock().await;
        let probes: Vec<mpsc::Sender<Probe>> = self
            .slots
            .lock()
            .unwrap()
            .values()
            .filter_map(|slot| slot.listed.as_ref().map(|(_, probe)| probe.clone()))
            .collect();
        for probe in probes {
            let (answer, answered) = oneshot::channel();
            let asked = probe.try_send(Probe {
                code: code.clone(),
                answer,
            });
            if asked.is_err() {
                continue;
            }
            if let Ok(Ok(Some(taken))) =
                tokio::time::timeout(Duration::from_secs(PROBE_SECS), answered).await
            {
                return Some(taken);
            }
        }
        None
    }

    /// A test's lever: an entry's probe for `code` that its entry gave up
    /// on, its answer's receiver gone before it is read, sent to every
    /// listed connection and answered once each has read it.
    #[cfg(test)]
    pub(crate) async fn probe_abandoned(&self, code: &Code) {
        let probes: Vec<mpsc::Sender<Probe>> = self
            .slots
            .lock()
            .unwrap()
            .values()
            .filter_map(|slot| slot.listed.as_ref().map(|(_, probe)| probe.clone()))
            .collect();
        for probe in probes {
            let (answer, answered) = oneshot::channel();
            drop(answered);
            let _ = probe
                .send(Probe {
                    code: code.clone(),
                    answer,
                })
                .await;
            let _ = probe.reserve().await;
        }
    }
}

/// What a waiting connection's loop met.
enum Event {
    Beat,
    Expired,
    Probe(Option<Probe>),
    Spoke,
    Halt,
}

/// One frame to the box, bounded: a box that stopped reading ends the
/// connection.
pub(crate) async fn send(reader: &mut PairConnection, frame: &PairToBox, bound: Duration) -> bool {
    let Ok(mut line) = serde_json::to_string(frame) else {
        return false;
    };
    line.push('\n');
    let stream = reader.get_mut();
    matches!(
        tokio::time::timeout(bound, async {
            stream.write_all(line.as_bytes()).await?;
            stream.flush().await
        })
        .await,
        Ok(Ok(()))
    )
}

/// **A matched connection kept alive** (Spec 8.2): a task of its own sends
/// the box a heartbeat at the link's cadence until the exchange takes the
/// connection back, so the box, which hears nothing from its match to its
/// answer otherwise, never reads four cadences of silence while the exchange
/// waits on the authority lock or the store. A heartbeat that cannot be sent
/// within the write bound ends it, the box gone. Dropped, it closes the
/// connection.
#[cfg(feature = "enrollment")]
pub(crate) struct Kept {
    stop: Option<oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<Option<PairConnection>>>,
    cadence: Duration,
    write_bound: Duration,
}

#[cfg(feature = "enrollment")]
impl Kept {
    /// Keep `connection` alive, its first heartbeat one cadence from now.
    pub(crate) fn new(
        connection: PairConnection,
        cadence: Duration,
        write_bound: Duration,
    ) -> Kept {
        let (stop, mut stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let mut connection = connection;
            let mut beat = tokio::time::interval_at(tokio::time::Instant::now() + cadence, cadence);
            beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    biased;
                    // Taken back between two frames, never inside one.
                    _ = &mut stopped => return Some(connection),
                    _ = beat.tick() => {
                        if !send(&mut connection, &PairToBox::Heartbeat, write_bound).await {
                            return None;
                        }
                    }
                }
            }
        });
        Kept {
            stop: Some(stop),
            task: Some(task),
            cadence,
            write_bound,
        }
    }

    /// The bound on one write to the box, the listener's own.
    pub(crate) fn write_bound(&self) -> Duration {
        self.write_bound
    }

    /// Resolves once the box is gone, a heartbeat not sent within the
    /// bound, and never while it hears the console.
    pub(crate) async fn lost(&mut self) {
        if let Some(task) = self.task.as_mut() {
            let _ = task.await;
            self.task = None;
        }
    }

    /// **The connection back**, its heartbeats stopped, or none where the
    /// box went.
    pub(crate) async fn resume(mut self) -> Option<PairConnection> {
        drop(self.stop.take());
        self.task.take()?.await.ok().flatten()
    }

    /// The cadence its heartbeats keep, the link's.
    pub(crate) fn cadence(&self) -> Duration {
        self.cadence
    }
}

/// A last frame, then the close.
pub(crate) async fn close_with(reader: &mut PairConnection, frame: &PairToBox, bound: Duration) {
    if send(reader, frame, bound).await {
        let _ = tokio::time::timeout(bound, reader.get_mut().shutdown()).await;
    }
}

/// What a pairing connection's task needs of the listener.
pub(crate) struct Context<'a> {
    pub waiting: &'a Waiting,
    pub config: Arc<rustls::ServerConfig>,
    pub peer: IpAddr,
    pub dialed: Instant,
    pub handshake_by: tokio::time::Instant,
    pub cadence: Duration,
    pub write_bound: Duration,
    pub halting: watch::Receiver<bool>,
}

/// **Serve one pairing connection** from its ClientHello: the handshake
/// under the pairing configuration, a place in the table, the first frame,
/// and the wait, until a code takes it, its life ends, the box leaves, or
/// the listener halts. It writes nothing anywhere but to the box.
pub(crate) async fn serve(start: StartHandshake<TcpStream>, mut cx: Context<'_>) {
    let handshake = tokio::select! {
        h = tokio::time::timeout_at(cx.handshake_by, start.into_stream(cx.config.clone())) => h,
        _ = cx.halting.changed() => return,
    };
    let Ok(Ok(tls)) = handshake else {
        return;
    };
    let mut reader = LineReader::with_bound(tls, PAIR_FIRST_FRAME_BOUND);
    let Some(reservation) = cx.waiting.reserve(cx.peer, cx.dialed) else {
        let busy = PairToBox::Refused {
            reason: pair_reason::BUSY.into(),
        };
        close_with(&mut reader, &busy, cx.write_bound).await;
        return;
    };

    let first = tokio::select! {
        f = tokio::time::timeout(cx.waiting.bounds().first_frame, reader.next()) => f,
        _ = cx.halting.changed() => return,
    };
    let claim = match first {
        Ok(Line::Frame(line)) => serde_json::from_str::<PairFromBox>(&line).ok(),
        _ => None,
    };
    let Some(PairFromBox::Hello {
        mut r#box,
        mut gate_request,
        mut admin_request,
    }) = claim
    else {
        return;
    };
    if !is_box(&r#box) || gate_request.is_empty() || admin_request.is_empty() {
        return;
    }

    let (probe, mut probes) = mpsc::channel::<Probe>(1);
    cx.waiting.list_slot(reservation.id, r#box.clone(), probe);
    let life = cx.waiting.bounds().life;
    let ends = tokio::time::Instant::from_std(cx.dialed + life);
    let waiting = PairToBox::Waiting {
        expires_in_secs: life.saturating_sub(cx.dialed.elapsed()).as_secs(),
        cadence_secs: cx.cadence.as_secs().max(1),
    };
    if !send(&mut reader, &waiting, cx.write_bound).await {
        return;
    }
    let mut beat = tokio::time::interval_at(tokio::time::Instant::now() + cx.cadence, cx.cadence);
    loop {
        let event = tokio::select! {
            _ = beat.tick() => Event::Beat,
            _ = tokio::time::sleep_until(ends) => Event::Expired,
            p = probes.recv() => Event::Probe(p),
            _ = reader.next() => Event::Spoke,
            _ = cx.halting.changed() => Event::Halt,
        };
        match event {
            Event::Beat => {
                if !send(&mut reader, &PairToBox::Heartbeat, cx.write_bound).await {
                    return;
                }
            }
            Event::Expired => {
                close_with(&mut reader, &PairToBox::Expired, cx.write_bound).await;
                return;
            }
            Event::Probe(Some(Probe { code, answer })) => {
                // **A probe its entry gave up on spends nothing**: the entry
                // has gone on without an answer, so the box keeps waiting
                // for the next.
                if answer.is_closed() {
                    continue;
                }
                let mine = Code::of(&**reader.get_ref().get_ref().1);
                if !mine.is_ok_and(|mine| mine.matches(&code)) {
                    let _ = answer.send(None);
                    continue;
                }
                let lifted = reservation.lift();
                let handed = answer.send(Some(Taken {
                    r#box,
                    gate_request,
                    admin_request,
                    peer: cx.peer,
                    #[cfg(feature = "enrollment")]
                    connection: Kept::new(reader, cx.cadence, cx.write_bound),
                    #[cfg(feature = "enrollment")]
                    ends,
                }));
                // **A hand-over its entry gave up on as it was made is taken
                // back**: the place, the claim and the connection, its
                // heartbeats stopped, and the box waits on.
                let Err(Some(back)) = handed else {
                    return;
                };
                if let Some(slot) = lifted {
                    reservation.restore(slot);
                }
                r#box = back.r#box;
                gate_request = back.gate_request;
                admin_request = back.admin_request;
                #[cfg(feature = "enrollment")]
                match back.connection.resume().await {
                    Some(connection) => reader = connection,
                    None => return,
                }
            }
            // The box speaks only its first frame before the exchange, so
            // anything else, or its close, ends the wait.
            Event::Probe(None) | Event::Spoke | Event::Halt => return,
        }
    }
}
