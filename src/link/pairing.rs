//! conforms: web-three-misses-close-the-chosen-connection-alone-the-box-told-code-missed
//! conforms: web-a-chosen-appliance-whose-wait-ended-is-said-so-apart-from-one-gone-with-no-record
//! conforms: web-every-compared-miss-new-agent-learns-of-leaves-its-record
//! conforms: web-a-chosen-appliance-that-does-not-answer-is-said-so-and-never-called-gone
//! conforms: web-misses-on-one-chosen-connection-leave-anothers-tries-whole
//!
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
//! **New agent asks one connection** (Spec 8.2, the brief of 2026-11-25):
//! each listed connection carries a handle this process mints from the
//! operating system's random source, the page names the appliance chosen by
//! it, and `Waiting::take_chosen` asks that connection alone. Its code is
//! never compared with another's, and `TRIES` misses on it close that
//! connection alone, the box told `code_missed`. **The misses are counted
//! by the connection's own task**, where the code is compared, and the
//! count rides the answer (the fix round of 2026-11-25): a miss the entry
//! learns of is never lost to the connection ending before the entry reads
//! a table, and a miss compared after its entry gave up still counts.
//!
//! **A matched box keeps hearing the console** (`Kept`): from the match until
//! the exchange writes to it, and again while the exchange's write runs, a
//! task of its own sends the box a heartbeat at the same cadence, so a box
//! whose exchange waits on the authority lock or the store never goes silent.

use crate::link::frames::{
    Line, LineReader, PAIR_FIRST_FRAME_BOUND, PairFromBox, PairReport, PairToBox, pair_reason,
};
use crate::link::pair_code::Code;
use crate::link::verbs::is_box;
use std::collections::{HashMap, VecDeque};
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

/// **The tries on one chosen connection** (Spec 8.2, plan A of
/// 2026-11-25): the third miss closes that connection, and the box dials
/// anew.
pub const TRIES: u8 = 3;

/// **The handles whose wait ended kept, newest last**, so New agent says
/// the wait ended rather than gone (Spec 8.2): a handle is 16 hex, so the
/// list holds at most 64 such in memory alone.
const ENDED_KEPT: usize = 64;

/// A handle as the page posts it: sixteen lowercase hex.
pub fn is_handle(text: &str) -> bool {
    text.len() == 16
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A fresh handle, from the operating system's random source.
fn handle() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the operating system's random source answers");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

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
/// peer address and how long it has waited, and the handle New agent
/// chooses it by. Never its code.
#[derive(Debug, Clone)]
pub struct WaitingBox {
    pub handle: String,
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
    pub admin_request: String,
    /// The first frame's reports, checked at the frame, written by the
    /// exchange with the row (Spec 8.2).
    pub reports: Vec<PairReport>,
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
    answer: oneshot::Sender<Answer>,
    /// Asked by New agent, which chose this connection by its handle: a
    /// miss counts on it, the `TRIES`th closing it, the box told
    /// `code_missed`. The old code entry, `Waiting::take`, counts none.
    chosen: bool,
}

/// **A waiting connection's answer to a probe**, its misses counted by the
/// connection's own task.
enum Answer {
    /// The code is this connection's: taken for the exchange.
    Taken(Box<Taken>),
    /// A miss, with the tries left on this connection.
    Missed { left: u8 },
    /// A chosen probe's last try missed, and the connection closes.
    Closed,
}

/// **What a chosen connection answered** (Spec 8.2).
pub enum Chosen {
    /// The code is that connection's: taken for the exchange.
    Matched(Box<Taken>),
    /// A miss, with the tries left on that connection.
    Missed { left: u8 },
    /// The last try missed, and the connection is closed.
    Closed,
    /// The connection took no probe or gave no answer within `PROBE_SECS`.
    /// It is still listed, and whether the code was compared is not known.
    Silent,
    /// No listed connection carries the handle, or the connection ended
    /// without answering: gone, its wait ended, or never listed. New agent
    /// tells an ended wait apart by `Waiting::wait_ended`.
    Gone,
}

/// **A test's lever on every waiting connection's task.**
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lever {
    /// The connection ends as it answers a miss, its place given back
    /// before the answer is sent.
    EndOnMiss,
    /// The connection reads each probe and answers none, holding it.
    Silent,
}

struct Slot {
    peer: IpAddr,
    dialed: Instant,
    handle: String,
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
    /// The latest handles whose wait ended, at most `ENDED_KEPT`.
    ended: Mutex<VecDeque<String>>,
    #[cfg(test)]
    lever: Mutex<Option<Lever>>,
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
            ended: Mutex::new(VecDeque::new()),
            #[cfg(test)]
            lever: Mutex::new(None),
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

    // Pulled by `enrollment_tests` alone.
    #[cfg(all(test, feature = "passkeys", feature = "enrollment"))]
    pub(crate) fn set_lever(&self, lever: Option<Lever>) {
        *self.lever.lock().unwrap() = lever;
    }

    #[cfg(test)]
    fn lever(&self) -> Option<Lever> {
        *self.lever.lock().unwrap()
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
                handle: handle(),
                listed: None,
            },
        );
        Some(Reservation { waiting: self, id })
    }

    /// The connection `id` holds, its wait ended: its handle kept among the
    /// latest.
    fn wait_ended_at(&self, id: u64) {
        let Some(handle) = self
            .slots
            .lock()
            .unwrap()
            .get(&id)
            .map(|s| s.handle.clone())
        else {
            return;
        };
        let mut ended = self.ended.lock().unwrap();
        if ended.len() >= ENDED_KEPT {
            ended.pop_front();
        }
        ended.push_back(handle);
    }

    /// **The wait of the connection `handle` named ended lately** (Spec
    /// 8.2): its ten minutes passed, so New agent says so.
    pub fn wait_ended(&self, handle: &str) -> bool {
        self.ended.lock().unwrap().iter().any(|h| h == handle)
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
                        handle: slot.handle.clone(),
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
                chosen: false,
            });
            if asked.is_err() {
                continue;
            }
            if let Ok(Ok(Answer::Taken(taken))) =
                tokio::time::timeout(Duration::from_secs(PROBE_SECS), answered).await
            {
                return Some(*taken);
            }
        }
        None
    }

    /// **The connection `handle` names, asked alone** (Spec 8.2): taken
    /// where `code` is its own, else a miss counted on it by its own task,
    /// the `TRIES`th closing it. No other connection's code is compared.
    pub async fn take_chosen(&self, handle: &str, code: &Code) -> Chosen {
        let _one = self.entry.lock().await;
        let probe = self.slots.lock().unwrap().values().find_map(|slot| {
            let (_, probe) = slot.listed.as_ref()?;
            (slot.handle == handle).then(|| probe.clone())
        });
        let Some(probe) = probe else {
            return Chosen::Gone;
        };
        let (answer, answered) = oneshot::channel();
        // **Room for the probe is waited for within the bound**: a probe an
        // earlier entry gave up on may still hold the one place, and the
        // connection's task reads it and passes it over.
        let asked = async {
            probe
                .send(Probe {
                    code: code.clone(),
                    answer,
                    chosen: true,
                })
                .await
                .ok()?;
            answered.await.ok()
        };
        match tokio::time::timeout(Duration::from_secs(PROBE_SECS), asked).await {
            Ok(Some(Answer::Taken(taken))) => Chosen::Matched(taken),
            Ok(Some(Answer::Missed { left })) => Chosen::Missed { left },
            Ok(Some(Answer::Closed)) => Chosen::Closed,
            Ok(None) => Chosen::Gone,
            Err(_) => Chosen::Silent,
        }
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
                    chosen: false,
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
    let line = match first {
        Ok(Line::Frame(line)) => line,
        _ => return,
    };
    // **A first frame that is not this hello is refused `frame_refused`**
    // (Spec 8.2), so a box built to the earlier hello, which carried a gate
    // request too, prints why and is never listed.
    let Ok(PairFromBox::Hello {
        mut r#box,
        mut admin_request,
        mut reports,
    }) = serde_json::from_str::<PairFromBox>(&line)
    else {
        let refused = PairToBox::Refused {
            reason: crate::link::frames::pair_reason::FRAME_REFUSED.to_owned(),
        };
        close_with(&mut reader, &refused, cx.write_bound).await;
        return;
    };
    if !is_box(&r#box) || admin_request.is_empty() {
        return;
    }
    // **The reports are checked before the box is listed** (Spec 8.2), so
    // a box without its receipt hears why at once.
    if let Some(reason) = crate::link::frames::reports_refused(&reports) {
        let refused = PairToBox::Refused {
            reason: reason.to_owned(),
        };
        close_with(&mut reader, &refused, cx.write_bound).await;
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
    // **The misses on this connection**, counted here where the code is
    // compared, chosen probes alone.
    let mut misses: u8 = 0;
    #[cfg(test)]
    let mut held = Vec::new();
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
                cx.waiting.wait_ended_at(reservation.id);
                close_with(&mut reader, &PairToBox::Expired, cx.write_bound).await;
                return;
            }
            Event::Probe(Some(Probe {
                code,
                answer,
                chosen,
            })) => {
                // **A probe its entry gave up on spends nothing**: the entry
                // has gone on without an answer, so the box keeps waiting
                // for the next.
                if answer.is_closed() {
                    continue;
                }
                #[cfg(test)]
                if cx.waiting.lever() == Some(Lever::Silent) {
                    held.push(answer);
                    continue;
                }
                let mine = Code::of(&**reader.get_ref().get_ref().1);
                if !mine.is_ok_and(|mine| mine.matches(&code)) {
                    if chosen {
                        misses += 1;
                    }
                    // **The last try's miss closes the connection** (Spec
                    // 8.2), the box told why, so it dials anew.
                    if chosen && misses >= TRIES {
                        let _ = answer.send(Answer::Closed);
                        let missed = PairToBox::Refused {
                            reason: pair_reason::CODE_MISSED.to_owned(),
                        };
                        close_with(&mut reader, &missed, cx.write_bound).await;
                        return;
                    }
                    let left = TRIES - misses;
                    #[cfg(test)]
                    if cx.waiting.lever() == Some(Lever::EndOnMiss) {
                        drop(reservation);
                        let _ = answer.send(Answer::Missed { left });
                        return;
                    }
                    let _ = answer.send(Answer::Missed { left });
                    continue;
                }
                let lifted = reservation.lift();
                let handed = answer.send(Answer::Taken(Box::new(Taken {
                    r#box,
                    admin_request,
                    reports,
                    peer: cx.peer,
                    #[cfg(feature = "enrollment")]
                    connection: Kept::new(reader, cx.cadence, cx.write_bound),
                    #[cfg(feature = "enrollment")]
                    ends,
                })));
                // **A hand-over its entry gave up on as it was made is taken
                // back**: the place, the claim and the connection, its
                // heartbeats stopped, and the box waits on.
                let Err(Answer::Taken(back)) = handed else {
                    return;
                };
                let back = *back;
                if let Some(slot) = lifted {
                    reservation.restore(slot);
                }
                r#box = back.r#box;
                admin_request = back.admin_request;
                reports = back.reports;
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
