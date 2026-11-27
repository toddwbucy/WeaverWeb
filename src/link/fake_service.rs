//! **A fake weaver-admin service for tests**, written from
//! `weaver-admin-operator-contract` section 8 alone (its section 8.10): a Unix
//! socket in a temporary directory, admitting the test's own uid in place of
//! the connector account's, judging each opening line, and serving the three
//! purposes as the contract states them. It reaches no agent and no
//! privileged program: a verb's answer is scripted, and a trace connection is
//! spliced to a `FakeRelay` serving a temporary file.
//!
//! What it can do, the contract's list:
//! - refuse an opening with any word, or a malformed one by its own judging,
//!   and delay its answer;
//! - answer the grants with a list, refuse them `grants_unread`, or stay
//!   silent;
//! - answer a verb with `started` and a scripted final line, answer `show`
//!   from a state it holds (`in_transition` for a set count), refuse it with
//!   any word of section 8.4, end it as a `fault`, or hold its final line;
//! - refuse `busy` by its machine A, and `not_granted` a verb that a grants
//!   answer listed;
//! - drop a control connection before `started` or while a verb runs, hold a
//!   run unstarted, and answer a later `run` with it standing or ended;
//! - forget its runs as a restart does, drawing a new `instance`, while a
//!   scripted run goes on;
//! - run a hook as a verb starts, which writes the trace file, and record
//!   each verb it ran with its principal;
//! - on a trace connection, refuse `relay_unreachable`, splice to the relay,
//!   close before the header as a relay that died (refusing the next opening
//!   `relay_unreachable`) or as one whose window passed (admitting the next);
//! - on a log connection, send scripted messages, `log_behind`, or close, the
//!   newest log connection replacing the older.

use crate::link::admin_service::{
    self, Final, Purpose, Ran, Refused, Standing, VERBS, read_line, to_line, word,
};
use crate::link::frames::Principal;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::OwnedWriteHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinHandle;

/// How the fake answers a grants ask.
#[derive(Clone)]
pub(crate) enum Grants {
    List(Vec<String>),
    Unread,
    Silent,
}

/// What a scripted verb comes to.
pub(crate) enum Act {
    /// Exit 0 or 1 with one object: `ran`.
    Answer {
        exit_code: i32,
        answer: serde_json::Value,
        stderr: String,
        stderr_cut: bool,
    },
    /// Ended any other way: `fault`, with its detail.
    Fault(String),
    /// Refused before machine A, with a word of section 8.4 such as
    /// `grant_unread`: nothing ran.
    Refuse(Refused),
    /// Granted and admitted, and the root program could not be started.
    StartFails,
}

/// Where a scripted verb drops its control connection.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cut {
    Never,
    /// Once the run stands, before `started` is sent.
    BeforeStarted,
    /// Once `started` is sent, while the run goes on.
    WhileRunning,
}

/// **One scripted invocation**, taken by the next verb ask it matches.
pub(crate) struct Script {
    /// The verb it answers; `None` answers any verb but `show`.
    pub(crate) verb: Option<String>,
    pub(crate) act: Act,
    /// Run as the root program starts: a test writes the trace file here.
    pub(crate) hook: Option<Box<dyn FnOnce() + Send>>,
    /// Held after the run stands and before its root program starts, until
    /// notified: `started_ms` is null meanwhile.
    pub(crate) hold_start: Option<Arc<Notify>>,
    /// Held before the final line, until notified.
    pub(crate) release: Option<Arc<Notify>>,
    pub(crate) cut: Cut,
}

impl Script {
    /// A verb that exits 0 with the given object.
    pub(crate) fn answer(verb: Option<&str>, answer: serde_json::Value) -> Self {
        Self::of(
            verb,
            Act::Answer {
                exit_code: 0,
                answer,
                stderr: String::new(),
                stderr_cut: false,
            },
        )
    }

    pub(crate) fn of(verb: Option<&str>, act: Act) -> Self {
        Self {
            verb: verb.map(str::to_owned),
            act,
            hook: None,
            hold_start: None,
            release: None,
            cut: Cut::Never,
        }
    }
}

/// A message the test sends down the log connection standing.
enum LogOut {
    Line(Vec<u8>),
    Behind,
    Close,
}

/// **What the fake holds**, which a test reads and sets through
/// `FakeService::with`.
pub(crate) struct State {
    pub(crate) instance: String,
    /// Refuse every opening of the purpose (`None`: of any purpose) with
    /// this refusal, after `answer_delay`; `full` is sent before the
    /// opening line is read, as the contract places it.
    pub(crate) refuse: Option<(Option<Purpose>, Refused)>,
    pub(crate) answer_delay: Duration,
    /// Sent in place of the answer, as it stands, and the connection closed.
    pub(crate) raw_answer: Option<Vec<u8>>,
    /// How long the fake waits for the opening line: the service's 10 s.
    pub(crate) opening_wait: Duration,
    pub(crate) grants: Grants,
    /// The verbs whose grant reads granted at the verb; `None` reads the
    /// grants answer's list.
    pub(crate) granted: Option<Vec<String>>,
    scripts: VecDeque<Script>,
    /// The state `show` answers.
    pub(crate) show_state: String,
    /// How many `show` runs answer `in_transition` before one answers the
    /// state.
    pub(crate) in_transition: usize,
    /// The runs machine A holds, a `show` never among them.
    held: Vec<Standing>,
    ended: Option<Final>,
    /// Every verb whose root program started, with its principal.
    pub(crate) ran: Vec<(String, Principal)>,
    /// The relay's socket a trace connection is spliced to.
    pub(crate) relay: Option<PathBuf>,
    relay_died: bool,
    die_next: bool,
    window_next: bool,
    log: Option<(u64, mpsc::UnboundedSender<LogOut>)>,
    log_seq: u64,
    /// Every opening the fake answered `open`, by purpose.
    pub(crate) opened: Vec<Purpose>,
}

/// A fresh `instance`: 16 lowercase hex digits from the OS's random source.
fn draw() -> String {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the OS draws random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// The fake service.
pub(crate) struct FakeService {
    _dir: tempfile::TempDir,
    pub(crate) socket: PathBuf,
    state: Arc<Mutex<State>>,
    run: JoinHandle<()>,
}

impl FakeService {
    /// A fake service standing at a temporary socket, granting `grants` and
    /// answering `show` with `state`.
    pub(crate) fn start(grants: &[&str], state: &str) -> Self {
        let dir = crate::test_dir::tempdir().unwrap();
        let socket = dir.path().join("service.sock");
        let state = Arc::new(Mutex::new(State {
            instance: draw(),
            refuse: None,
            answer_delay: Duration::ZERO,
            raw_answer: None,
            opening_wait: Duration::from_secs(10),
            grants: Grants::List(grants.iter().map(|g| (*g).to_owned()).collect()),
            granted: None,
            scripts: VecDeque::new(),
            show_state: state.to_owned(),
            in_transition: 0,
            held: Vec::new(),
            ended: None,
            ran: Vec::new(),
            relay: None,
            relay_died: false,
            die_next: false,
            window_next: false,
            log: None,
            log_seq: 0,
            opened: Vec::new(),
        }));
        let listener = UnixListener::bind(&socket).unwrap();
        let shared = state.clone();
        let run = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(serve(stream, shared.clone()));
            }
        });
        Self {
            _dir: dir,
            socket,
            state,
            run,
        }
    }

    /// Read or set what the fake holds.
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.state.lock().unwrap())
    }

    /// Refuse every opening of `purpose` (`None`: any) with `word`.
    pub(crate) fn refuse(&self, purpose: Option<Purpose>, word: &str) {
        self.with(|s| {
            s.refuse = Some((
                purpose,
                Refused::new(word, format!("the fake refuses {word}")),
            ))
        });
    }

    pub(crate) fn script(&self, script: Script) {
        self.with(|s| s.scripts.push_back(script));
    }

    /// **The service restarts**: a new `instance`, no run kept, and a run it
    /// started goes on with its end recorded nowhere.
    pub(crate) fn restart(&self) {
        self.with(|s| {
            s.instance = draw();
            s.held.clear();
            s.ended = None;
        });
    }

    /// The next trace opening is answered `open` and closed before any
    /// header byte, as a relay that died after the dial; every opening
    /// after it is refused `relay_unreachable`.
    pub(crate) fn relay_dies_next(&self) {
        self.with(|s| s.die_next = true);
    }

    /// The next trace opening is answered `open` and closed before any
    /// header byte, as a relay whose window passed; the next is admitted.
    pub(crate) fn window_passes_next(&self) {
        self.with(|s| s.window_next = true);
    }

    /// Send one message down the log connection standing, if one stands.
    pub(crate) fn log(&self, event: &admin_service::LogEvent) {
        self.send_log(LogOut::Line(event.to_line()));
    }

    /// Send raw bytes down the log connection standing, as they stand.
    pub(crate) fn log_raw(&self, bytes: &[u8]) {
        self.send_log(LogOut::Line(bytes.to_vec()));
    }

    /// `refused` `log_behind`, then the close.
    pub(crate) fn log_behind(&self) {
        self.send_log(LogOut::Behind);
    }

    /// Close the log connection standing, with nothing sent.
    pub(crate) fn log_close(&self) {
        self.send_log(LogOut::Close);
    }

    fn send_log(&self, out: LogOut) {
        if let Some((_, tx)) = &self.state.lock().unwrap().log {
            let _ = tx.send(out);
        }
    }
}

impl Drop for FakeService {
    fn drop(&mut self) {
        self.run.abort();
    }
}

/// **The root program's name rule, as the fake judges it**: non-empty ASCII
/// letters, digits, `-` and `_`, and never `admin`.
fn name_judges(name: &str) -> bool {
    !name.is_empty()
        && name != "admin"
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

async fn write(stream: &mut UnixStream, bytes: &[u8]) {
    let _ = stream.write_all(bytes).await;
}

fn refused(refusal: Refused) -> Vec<u8> {
    to_line(&serde_json::json!({ "refused": refusal }))
}

/// **One connection**: admission, the opening line, and its purpose.
async fn serve(mut stream: UnixStream, state: Arc<Mutex<State>>) {
    // **The peer is judged at accept**: the test's own uid alone.
    // SAFETY: geteuid takes nothing and cannot fail.
    let me = unsafe { libc::geteuid() };
    match stream.peer_cred() {
        Ok(cred) if cred.uid() == me => {}
        _ => return,
    }
    let (refuse, delay, raw, wait) = {
        let s = state.lock().unwrap();
        (
            s.refuse.clone(),
            s.answer_delay,
            s.raw_answer.clone(),
            s.opening_wait,
        )
    };
    if let Some((_, refusal)) = refuse.as_ref().filter(|(_, r)| r.word == word::FULL) {
        write(&mut stream, &refused(refusal.clone())).await;
        return;
    }
    let mut reader = BufReader::new(stream);
    let line = match tokio::time::timeout(wait, read_line(&mut reader, 4096)).await {
        Err(_) => {
            write(
                reader.get_mut(),
                &refused(Refused::new(word::OPENING_LATE, "no opening line in time")),
            )
            .await;
            return;
        }
        Ok(Ok(Some(line))) => line,
        Ok(Ok(None)) => return,
        Ok(Err(_)) => {
            write(
                reader.get_mut(),
                &refused(Refused::new(
                    word::OPENING_MALFORMED,
                    "past its bound or cut",
                )),
            )
            .await;
            return;
        }
    };
    let judged = judge(&line);
    let purpose = match judged {
        Ok(purpose) => purpose,
        Err(refusal) => {
            write(reader.get_mut(), &refused(refusal)).await;
            return;
        }
    };
    tokio::time::sleep(delay).await;
    if let Some(raw) = raw {
        write(reader.get_mut(), &raw).await;
        return;
    }
    if let Some((only, refusal)) = refuse
        && only.is_none_or(|only| only == purpose)
    {
        write(reader.get_mut(), &refused(refusal)).await;
        return;
    }
    match purpose {
        Purpose::Control => control(reader, state).await,
        Purpose::Trace => trace(reader, state).await,
        Purpose::Log => log(reader, state).await,
    }
}

/// **Judge an opening line** as the contract's table does.
fn judge(line: &[u8]) -> Result<Purpose, Refused> {
    let malformed = |why: &str| Refused::new(word::OPENING_MALFORMED, why);
    let value: serde_json::Value =
        serde_json::from_slice(line).map_err(|_| malformed("does not parse"))?;
    let body = value
        .as_object()
        .filter(|o| o.len() == 1)
        .and_then(|o| o.get("weaver_admin"))
        .and_then(|b| b.as_object())
        .ok_or_else(|| malformed("lacks a member"))?;
    let version = body
        .get("version")
        .and_then(|v| v.as_u64())
        .filter(|v| *v >= 1)
        .ok_or_else(|| malformed("the version is not a positive integer"))?;
    let (Some(purpose), Some(agent)) = (
        body.get("purpose").and_then(|p| p.as_str()),
        body.get("agent").and_then(|a| a.as_str()),
    ) else {
        return Err(malformed("lacks a member"));
    };
    if version > admin_service::VERSION {
        let mut refusal = Refused::new(word::VERSION_UNKNOWN, "a version above the service's");
        refusal.versions = Some(vec![admin_service::VERSION]);
        return Err(refusal);
    }
    let purpose = match purpose {
        "control" => Purpose::Control,
        "trace" => Purpose::Trace,
        "log" => Purpose::Log,
        _ => {
            let mut refusal = Refused::new(word::PURPOSE_UNKNOWN, "not a purpose of version 1");
            refusal.purposes = Some(vec!["control".into(), "trace".into(), "log".into()]);
            return Err(refusal);
        }
    };
    if !name_judges(agent) {
        return Err(Refused::new(word::NAME_REFUSED, "the name fails the rule"));
    }
    Ok(purpose)
}

/// The writing half of a control connection, shared by its asks; `None`
/// once the fake has closed it.
type Writer = Arc<tokio::sync::Mutex<Option<OwnedWriteHalf>>>;

async fn say(writer: &Writer, bytes: &[u8]) {
    if let Some(w) = writer.lock().await.as_mut() {
        let _ = w.write_all(bytes).await;
    }
}

/// Close the connection's writing half: the connector reads its end.
async fn close(writer: &Writer) {
    if let Some(mut w) = writer.lock().await.take() {
        let _ = w.shutdown().await;
    }
}

fn open_line(state: &Arc<Mutex<State>>, purpose: Purpose) -> Vec<u8> {
    let mut s = state.lock().unwrap();
    s.opened.push(purpose);
    to_line(&serde_json::json!({"open": {"version": 1, "instance": s.instance}}))
}

/// **A control connection**: one ask at a time, each answered by a task of
/// its own, so a run goes on when the connection closes.
async fn control(reader: BufReader<UnixStream>, state: Arc<Mutex<State>>) {
    let open = open_line(&state, Purpose::Control);
    let (read, mut write_half) = reader.into_inner().into_split();
    let _ = write_half.write_all(&open).await;
    let writer: Writer = Arc::new(tokio::sync::Mutex::new(Some(write_half)));
    let outstanding = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut read = BufReader::new(read);
    loop {
        let line = match read_line(&mut read, 4096).await {
            Ok(Some(line)) => line,
            Ok(None) => return,
            Err(_) => {
                say(
                    &writer,
                    &refused(Refused::new(word::ASK_MALFORMED, "past its bound")),
                )
                .await;
                close(&writer).await;
                return;
            }
        };
        if outstanding.load(std::sync::atomic::Ordering::SeqCst) {
            say(
                &writer,
                &refused(Refused::new(word::ASK_OUTSTANDING, "one ask at a time")),
            )
            .await;
            close(&writer).await;
            return;
        }
        let malformed = || refused(Refused::new(word::ASK_MALFORMED, "not an ask of its shape"));
        let Some((name, body)) = serde_json::from_slice::<serde_json::Value>(&line)
            .ok()
            .and_then(|v| v.as_object().filter(|o| o.len() == 1).cloned())
            .and_then(|o| o.into_iter().next())
            .filter(|(_, body)| body.is_object())
        else {
            say(&writer, &malformed()).await;
            close(&writer).await;
            return;
        };
        if body.get("length").is_some() {
            say(&writer, &malformed()).await;
            close(&writer).await;
            return;
        }
        match name.as_str() {
            "grants" => {
                let grants = state.lock().unwrap().grants.clone();
                match grants {
                    Grants::List(verbs) => {
                        let line = to_line(&serde_json::json!({"grants": {"verbs": verbs}}));
                        say(&writer, &line).await;
                    }
                    Grants::Unread => {
                        say(
                            &writer,
                            &refused(Refused::new(word::GRANTS_UNREAD, "a read failed")),
                        )
                        .await;
                    }
                    Grants::Silent => {
                        outstanding.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                }
            }
            "run" => {
                let line = {
                    let s = state.lock().unwrap();
                    to_line(&serde_json::json!({"run": {
                        "instance": s.instance,
                        "standing": s.held,
                        "ended": s.ended,
                    }}))
                };
                say(&writer, &line).await;
            }
            "verb" => {
                let verb = body.get("verb").and_then(|v| v.as_str()).map(str::to_owned);
                let principal = body
                    .get("principal")
                    .cloned()
                    .and_then(|p| serde_json::from_value::<Principal>(p).ok())
                    .filter(|p| match p {
                        Principal::Server => true,
                        Principal::Person { person_id } => person_id
                            .strip_prefix("pe-")
                            .is_some_and(admin_service::is_id),
                    });
                let (Some(verb), Some(principal)) = (verb, principal) else {
                    say(&writer, &malformed()).await;
                    close(&writer).await;
                    return;
                };
                if !VERBS.contains(&verb.as_str()) {
                    say(
                        &writer,
                        &refused(Refused::new(word::VERB_UNKNOWN, "not one of the eight")),
                    )
                    .await;
                    continue;
                }
                outstanding.store(true, std::sync::atomic::Ordering::SeqCst);
                let (state, writer, outstanding) =
                    (state.clone(), writer.clone(), outstanding.clone());
                tokio::spawn(async move {
                    run_verb(&state, &writer, verb, principal).await;
                    outstanding.store(false, std::sync::atomic::Ordering::SeqCst);
                });
            }
            _ => {
                say(
                    &writer,
                    &refused(Refused::new(word::ASK_UNKNOWN, "not an ask of version 1")),
                )
                .await;
            }
        }
    }
}

/// The answer `show` gives from the state the fake holds.
fn show_answer(s: &mut State) -> serde_json::Value {
    if s.in_transition > 0 {
        s.in_transition -= 1;
        return serde_json::json!({"kind": "in_transition", "constituents": [4100]});
    }
    let load = if s.show_state == "idle" {
        serde_json::json!({"declaration": "sha-show"})
    } else {
        serde_json::Value::Null
    };
    serde_json::json!({"kind": "state", "state": s.show_state, "load": load})
}

/// **One verb ask, to its final line**: the grant read in the request, then
/// machine A, then the run.
async fn run_verb(state: &Arc<Mutex<State>>, writer: &Writer, verb: String, principal: Principal) {
    let show = verb == "show";
    // The script it takes, the grant's read and machine A, under one lock.
    let admitted = {
        let mut s = state.lock().unwrap();
        let at = s.scripts.iter().position(|script| match &script.verb {
            Some(v) => *v == verb,
            None => !show,
        });
        let script = at.and_then(|at| s.scripts.remove(at));
        let script = script.unwrap_or_else(|| {
            if show {
                Script::of(
                    Some("show"),
                    Act::Answer {
                        exit_code: 0,
                        answer: serde_json::Value::Null,
                        stderr: String::new(),
                        stderr_cut: false,
                    },
                )
            } else {
                Script::answer(None, serde_json::json!({"kind": "answered", "verb": verb}))
            }
        });
        if let Act::Refuse(refusal) = &script.act {
            Err(refusal.clone())
        } else if !s
            .granted
            .clone()
            .unwrap_or_else(|| match &s.grants {
                Grants::List(verbs) => verbs.clone(),
                _ => Vec::new(),
            })
            .contains(&verb)
        {
            Err(Refused::new(word::NOT_GRANTED, "the grant's read exited 1"))
        } else {
            let holder = s.held.first().cloned();
            let beside = holder
                .as_ref()
                .is_some_and(|h| verb == "force-unload" && h.verb == "unload" && s.held.len() == 1);
            match holder {
                Some(holder) if !show && !beside => {
                    let mut refusal = Refused::new(word::BUSY, "another run stands");
                    refusal.run = Some(holder.run);
                    Err(refusal)
                }
                _ => {
                    let run = draw();
                    if !show {
                        s.held.push(Standing {
                            run: run.clone(),
                            verb: verb.clone(),
                            started_ms: None,
                        });
                    }
                    Ok((run, s.instance.clone(), script))
                }
            }
        }
    };
    let (run, instance, script) = match admitted {
        Ok(admitted) => admitted,
        Err(refusal) => {
            say(writer, &refused(refusal)).await;
            return;
        }
    };
    let Script {
        act,
        hook,
        hold_start,
        release,
        cut,
        ..
    } = script;
    if let Some(gate) = hold_start {
        gate.notified().await;
    }
    if matches!(act, Act::StartFails) {
        state.lock().unwrap().held.retain(|h| h.run != run);
        say(
            writer,
            &refused(Refused::new(
                word::NOT_STARTED,
                "the root program could not start",
            )),
        )
        .await;
        return;
    }
    // **The root program starts.**
    {
        let mut s = state.lock().unwrap();
        if let Some(held) = s.held.iter_mut().find(|h| h.run == run) {
            held.started_ms = Some(now_ms());
        }
        s.ran.push((verb.clone(), principal));
    }
    if let Some(hook) = hook {
        hook();
    }
    if cut == Cut::BeforeStarted {
        close(writer).await;
    }
    say(
        writer,
        &to_line(&serde_json::json!({"started": {"run": run}})),
    )
    .await;
    if cut == Cut::WhileRunning {
        close(writer).await;
    }
    if let Some(gate) = release {
        gate.notified().await;
    }
    let last = {
        let mut s = state.lock().unwrap();
        let last = match act {
            Act::Answer {
                exit_code,
                answer,
                stderr,
                stderr_cut,
            } => Final::Ran(Ran {
                run: run.clone(),
                verb: verb.clone(),
                exit_code,
                answer: if show { show_answer(&mut s) } else { answer },
                stderr,
                stderr_cut,
            }),
            Act::Fault(detail) => Final::Fault(admin_service::Fault {
                run: run.clone(),
                verb: verb.clone(),
                detail,
            }),
            Act::Refuse(_) | Act::StartFails => unreachable!("answered before the start"),
        };
        // **Kept as the last ended**, unless a `show`, or a run of a start
        // the service has since forgotten.
        let mine = s.held.iter().position(|h| h.run == run);
        if let Some(at) = mine {
            s.held.remove(at);
        }
        if !show && s.instance == instance {
            s.ended = Some(last.clone());
        }
        last
    };
    say(writer, &to_line(&last)).await;
}

/// **A trace connection**: the relay's dial before `open`, then the splice.
async fn trace(reader: BufReader<UnixStream>, state: Arc<Mutex<State>>) {
    let mut reader = reader;
    let (relay, die, window) = {
        let mut s = state.lock().unwrap();
        let relay = s.relay.clone().filter(|_| !s.relay_died);
        let die = std::mem::take(&mut s.die_next);
        let window = std::mem::take(&mut s.window_next);
        if die {
            s.relay_died = true;
        }
        (relay, die, window)
    };
    let unreachable = || refused(Refused::new(word::RELAY_UNREACHABLE, "no relay stands"));
    let Some(relay) = relay else {
        write(reader.get_mut(), &unreachable()).await;
        return;
    };
    if die || window {
        // `open`, then a close before any header byte.
        let open = open_line(&state, Purpose::Trace);
        write(reader.get_mut(), &open).await;
        let mut request = Vec::new();
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            tokio::io::AsyncBufReadExt::read_until(&mut reader, b'\n', &mut request),
        )
        .await;
        return;
    }
    let Ok(mut dialed) = UnixStream::connect(&relay).await else {
        write(reader.get_mut(), &unreachable()).await;
        return;
    };
    let open = open_line(&state, Purpose::Trace);
    write(reader.get_mut(), &open).await;
    let early = reader.buffer().to_vec();
    if !early.is_empty() {
        let _ = dialed.write_all(&early).await;
    }
    let mut client = reader.into_inner();
    let _ = tokio::io::copy_bidirectional(&mut client, &mut dialed).await;
}

/// **A log connection**: the newest replaces the older when it opens.
async fn log(reader: BufReader<UnixStream>, state: Arc<Mutex<State>>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let open = {
        let mut s = state.lock().unwrap();
        s.log_seq += 1;
        let seq = s.log_seq;
        // The older connection's channel closes as its sender is dropped.
        s.log = Some((seq, tx));
        s.opened.push(Purpose::Log);
        to_line(&serde_json::json!({"open": {"version": 1, "instance": s.instance}}))
    };
    let mut stream = reader.into_inner();
    write(&mut stream, &open).await;
    let mut byte = [0u8; 1];
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(LogOut::Line(bytes)) => write(&mut stream, &bytes).await,
                Some(LogOut::Behind) => {
                    write(&mut stream, &refused(Refused::new(word::LOG_BEHIND, "1 MiB unsent"))).await;
                    return;
                }
                Some(LogOut::Close) | None => return,
            },
            read = stream.read(&mut byte) => match read {
                Ok(0) | Err(_) => return,
                Ok(_) => {
                    write(&mut stream, &refused(Refused::new(word::ASK_MALFORMED, "a byte on a log connection"))).await;
                    return;
                }
            },
        }
    }
}

/// A connection that sends `bytes` as its opening line and returns the
/// service's answer as it came, for tests of the fake's own judging.
pub(crate) async fn raw_opening(socket: &Path, bytes: &[u8]) -> Vec<u8> {
    let mut stream = UnixStream::connect(socket).await.unwrap();
    let _ = stream.write_all(bytes).await;
    let mut answer = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut answer)).await;
    answer
}
