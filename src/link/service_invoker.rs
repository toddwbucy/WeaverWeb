//! conforms: web-a-verb-asks-the-service-and-reads-its-run-after-a-drop
//! conforms: web-ceiling-is-what-the-services-grants-name
//!
//! **admin-con's invoker: it asks the weaver-admin service** (Spec 7.2, 8;
//! `weaver-admin-operator-contract` section 8, the operator's rulings of
//! 2026-11-26). The service, on the agent's box, reads the grant at every
//! verb and runs the fixed lines itself; admin-con opens a control
//! connection on its local socket for each ask (`link::admin_service`) and
//! carries what comes back. **Nothing here runs a program**: no command is
//! built, and nothing that crossed the link reaches one, since the service
//! runs fixed lines alone.
//!
//! **One control connection per ask**: the hello's `grants`, each verb,
//! and each `run` poll after a drop open their own, so at most three stand
//! at once, under the service's eight. A verb holds its connection until
//! its final line.
//!
//! **Every refusal before a start is `not_started`** (nothing ran), the
//! message naming the service's word, and a word that names admin-con's
//! own defect or its config's adds a connector failure entry, which the
//! server raises. `not_granted` and `busy` keep kinds of their own.
//!
//! **A baseline before every verb but `show`**: the verb's own connection
//! first asks `run`, and that answer's `ended` and `standing` are what the
//! service already held, so nothing admin-con remembers between asks (a
//! `show`, its own restart, an exit of the drop table) can make the reading
//! after a drop stale.
//!
//! **After a drop** (the connection closes with the verb outstanding, its
//! run possibly standing), the invocation keeps admin-con's slot and asks
//! the service `run` every `POLL` on a fresh connection, reading its own
//! run by the id `started` named, or with none held, as a run of its verb
//! that the baseline did not hold. It answers the run's final line when the
//! run has ended, `not_started` when the service holds nothing of it, and
//! `unknown` when the service has restarted under another `instance`, no
//! socket has answered for the verb's bound, or a poll is refused for good.
//! While the run stands it polls on, so admin-con's bound answers `unknown`
//! and the slot is held until the run ends. A `show` is never listed as
//! standing or kept as ended, so a dropped `show` is `unknown` at once.

use crate::link::admin_con::Invoker;
use crate::link::admin_service::{
    self, ANSWER_WAIT, Ask, ControlLine, Final, LineError, Opened, Purpose, Refused, RunAnswer,
    VERBS, word,
};
use crate::link::entries::Entries;
use crate::link::frames::{Principal, VerbFault, VerbOutcome};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// **How often a `run` is asked after a drop**, the brief's cadence.
pub const POLL: Duration = Duration::from_secs(2);

/// The opening's words that may pass, so a poll meeting one waits for the
/// next.
const PASSING: [&str; 2] = [word::FULL, word::OPENING_LATE];

/// **The words that say admin-con or its config is at fault**, each raised
/// as an entry beside the `not_started`: the service judged what admin-con
/// sent and found it wrong, which no retry mends.
const DEFECTS: [&str; 7] = [
    word::OPENING_MALFORMED,
    word::VERSION_UNKNOWN,
    word::PURPOSE_UNKNOWN,
    word::NAME_REFUSED,
    word::ASK_UNKNOWN,
    word::ASK_MALFORMED,
    word::VERB_UNKNOWN,
];

/// The words a verb's control connection can be refused with before its
/// start and that say only that nothing ran, raising no entry.
const QUIET: [&str; 4] = [
    word::OPENING_LATE,
    word::FULL,
    word::GRANT_UNREAD,
    word::NOT_STARTED,
];

/// **The entry kind for a word of the service's**, the alert's subject at
/// the server: one per word, so an operator reads which.
fn entry_kind(word: &str) -> String {
    format!("service_{word}")
}

/// The service's invoker for one agent.
#[derive(Clone)]
pub struct ServiceInvoker {
    socket: PathBuf,
    agent: String,
    /// The verb's bound: a drop with no socket answering this long is
    /// `unknown`, the slot freed.
    verb_bound: Duration,
    entries: Arc<OnceLock<Entries>>,
    /// The poll's cadence: `POLL`, settable in code so a test can reach it.
    pub poll: Duration,
}

impl std::fmt::Debug for ServiceInvoker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceInvoker")
            .field("socket", &self.socket)
            .field("agent", &self.agent)
            .finish_non_exhaustive()
    }
}

/// How a verb's own connection ended short of its final line: the run it
/// may have left standing, read after.
struct Dropped {
    /// The run `started` named, if it came, or the standing run of the verb
    /// the baseline did not hold, once a poll finds it.
    run: Option<String>,
    /// The service's start the verb's connection opened under.
    instance: String,
    /// What the service held just before the verb's ask.
    baseline: Baseline,
}

/// **The service's runs just before a verb's ask**, from a `run` on the
/// verb's own connection: the run kept as ended, and every run standing.
#[derive(Default)]
struct Baseline {
    ended: Option<String>,
    standing: Vec<String>,
}

/// **Every word a verb's ask can be refused with before `started`**
/// (`weaver-admin-operator-contract` section 8.2): another word, from a
/// later service, may follow a run's admission, so it goes to the drop
/// table.
const VERB_WORDS: [&str; 13] = [
    word::OPENING_MALFORMED,
    word::OPENING_LATE,
    word::VERSION_UNKNOWN,
    word::PURPOSE_UNKNOWN,
    word::NAME_REFUSED,
    word::FULL,
    word::ASK_UNKNOWN,
    word::ASK_MALFORMED,
    word::VERB_UNKNOWN,
    word::NOT_GRANTED,
    word::GRANT_UNREAD,
    word::BUSY,
    word::NOT_STARTED,
];

impl ServiceInvoker {
    /// An invoker asking the service at `socket` for the agent `agent`.
    pub fn new(socket: &Path, agent: &str, verb_bound: Duration) -> Self {
        Self {
            socket: socket.to_owned(),
            agent: agent.to_owned(),
            verb_bound,
            entries: Arc::new(OnceLock::new()),
            poll: POLL,
        }
    }

    /// A connector failure entry, where the entries are attached.
    fn entry(&self, kind: &str, message: String) {
        tracing::warn!("{}: {message}", self.agent);
        if let Some(entries) = self.entries.get() {
            entries.failure(kind, message);
        }
    }

    /// Open a control connection.
    async fn control(&self) -> Result<Opened, admin_service::OpenError> {
        admin_service::open(&self.socket, Purpose::Control, &self.agent, ANSWER_WAIT).await
    }

    /// **A refusal of the verb before its start**: `not_granted` and `busy`
    /// keep their kinds, every other word is `not_started`, and a word of
    /// admin-con's defect, or one that cannot answer a verb at all, adds an
    /// entry.
    fn refused_before_start(&self, verb: &str, refusal: &Refused) -> VerbFault {
        let fault = |kind: &str, message: String| VerbFault {
            kind: kind.into(),
            message,
        };
        match refusal.word.as_str() {
            word::NOT_GRANTED => fault(
                VerbFault::NOT_GRANTED,
                format!(
                    "the service read the grant of {verb} and it is not granted ({}); it did not run",
                    refusal.detail
                ),
            ),
            word::BUSY => fault(
                VerbFault::BUSY,
                format!(
                    "the service holds run {} of this agent; {verb} did not run",
                    refusal.run.as_deref().unwrap_or("(unnamed)")
                ),
            ),
            w => {
                let message = format!(
                    "the service refused {verb} before its start, {w}: {}; it did not run",
                    refusal.detail
                );
                if !QUIET.contains(&w) {
                    let why = if DEFECTS.contains(&w) {
                        "admin-con or its config is at fault"
                    } else {
                        "a word that cannot answer a verb"
                    };
                    self.entry(&entry_kind(w), format!("{message} ({why})"));
                }
                fault(VerbFault::NOT_STARTED, message)
            }
        }
    }

    /// The outcome of a final line.
    fn ended(&self, agent: &str, last: Final) -> Result<VerbOutcome, VerbFault> {
        match last {
            Final::Ran(ran) => {
                if ran.stderr_cut {
                    tracing::info!(
                        "{agent}: the service cut {}'s standard error at 16 KiB",
                        ran.verb
                    );
                }
                Ok(VerbOutcome {
                    verb: ran.verb,
                    agent: agent.to_owned(),
                    exit_code: Some(ran.exit_code),
                    answer: Some(ran.answer),
                    raw_stdout: None,
                    stderr: Some(ran.stderr).filter(|s| !s.is_empty()),
                })
            }
            Final::Fault(f) => Err(VerbFault {
                kind: VerbFault::FAULT.into(),
                message: format!(
                    "{} ran and ended with no answer to read: {}; read the next show",
                    f.verb, f.detail
                ),
            }),
        }
    }

    /// **The verb on its own connection**: its final line, its refusal, or
    /// how it dropped.
    async fn ask_verb(
        &self,
        agent: &str,
        verb: &str,
        principal: &Principal,
    ) -> Result<Result<VerbOutcome, VerbFault>, Dropped> {
        let not_started = |message: String| VerbFault {
            kind: VerbFault::NOT_STARTED.into(),
            message,
        };
        let mut conn = match self.control().await {
            Ok(Opened::Open(conn)) => conn,
            Ok(Opened::Refused(refusal)) => {
                return Ok(Err(self.refused_before_start(verb, &refusal)));
            }
            Err(admin_service::OpenError::Late) => {
                return Ok(Err(not_started(format!(
                    "the service's socket answered no opening within {ANSWER_WAIT:?}; {verb} did not run"
                ))));
            }
            Err(e) => {
                let message =
                    format!("the service's socket did not open ({e}); {verb} did not run");
                self.entry("service_unreachable", message.clone());
                return Ok(Err(not_started(message)));
            }
        };
        // **The baseline**, on the verb's own connection, before its ask:
        // nothing is asked yet, so any failure here is `not_started`.
        let baseline = if verb == "show" {
            Baseline::default()
        } else {
            let read = match conn.ask(&Ask::Run {}).await {
                Err(e) => Err(format!("the baseline's run ask was not sent ({e})")),
                Ok(()) => match tokio::time::timeout(ANSWER_WAIT, conn.next_control()).await {
                    Ok(Ok(Some(ControlLine::Run(answer)))) if answer.instance == conn.instance => {
                        Ok(Baseline {
                            ended: answer.ended.as_ref().map(|e| e.run().to_owned()),
                            standing: answer.standing.into_iter().map(|s| s.run).collect(),
                        })
                    }
                    Ok(Ok(Some(ControlLine::Refused(refusal)))) => {
                        let message = format!(
                            "the service refused the baseline's run ask, {}: {}; {verb} was not asked and did not run",
                            refusal.word, refusal.detail
                        );
                        self.entry(&entry_kind(&refusal.word), message.clone());
                        return Ok(Err(not_started(message)));
                    }
                    Ok(Ok(other)) => Err(format!("the baseline's run ask was answered {other:?}")),
                    Ok(Err(e)) => Err(format!("the baseline's run ask carried {e}")),
                    Err(_) => Err(format!(
                        "the baseline's run ask was not answered within {ANSWER_WAIT:?}"
                    )),
                },
            };
            match read {
                Ok(baseline) => baseline,
                Err(why) => {
                    let message = format!("{why}; {verb} was not asked and did not run");
                    self.entry("service_malformed", message.clone());
                    return Ok(Err(not_started(message)));
                }
            }
        };
        let mut dropped = Dropped {
            run: None,
            instance: conn.instance.clone(),
            baseline,
        };
        let ask = Ask::Verb {
            verb: verb.to_owned(),
            principal: principal.clone(),
        };
        if conn.ask(&ask).await.is_err() {
            // Whether the ask reached the service is not known here; its
            // `run` answer says.
            return Err(dropped);
        }
        loop {
            let line = conn.next_control().await;
            match line {
                Ok(Some(ControlLine::Started { run })) if dropped.run.is_none() => {
                    dropped.run = Some(run);
                }
                Ok(Some(ControlLine::Ran(ran))) => return Ok(self.ended(agent, Final::Ran(ran))),
                Ok(Some(ControlLine::Fault(fault))) => {
                    return Ok(self.ended(agent, Final::Fault(fault)));
                }
                Ok(Some(ControlLine::Refused(refusal)))
                    if dropped.run.is_none() && VERB_WORDS.contains(&refusal.word.as_str()) =>
                {
                    return Ok(Err(self.refused_before_start(verb, &refusal)));
                }
                Ok(Some(ControlLine::Refused(refusal))) => {
                    // `ask_outstanding`, a refusal after `started`, or a
                    // word no verb's refusal holds: the run may stand, so
                    // its `run` answer says.
                    self.entry(
                        &entry_kind(&refusal.word),
                        format!(
                            "the service refused {verb}'s connection with {} while the verb was outstanding: {}; its run is read after",
                            refusal.word, refusal.detail
                        ),
                    );
                    return Err(dropped);
                }
                Ok(Some(other)) => {
                    self.entry(
                        "service_malformed",
                        format!(
                            "the service sent {other:?} on {verb}'s connection, not a line of a verb; its run is read after"
                        ),
                    );
                    return Err(dropped);
                }
                Ok(None) | Err(LineError::Io(_) | LineError::Truncated) => return Err(dropped),
                Err(e) => {
                    self.entry(
                        "service_malformed",
                        format!("{verb}'s connection carried {e}; its run is read after"),
                    );
                    return Err(dropped);
                }
            }
        }
    }

    /// One `run` poll on a fresh connection: its answer, `None` where the
    /// socket did not answer or a passing word met it, or the word that
    /// ends the poll.
    async fn poll_once(&self) -> Result<Option<RunAnswer>, String> {
        let mut conn = match self.control().await {
            Ok(Opened::Open(conn)) => conn,
            Ok(Opened::Refused(refusal)) if PASSING.contains(&refusal.word.as_str()) => {
                return Ok(None);
            }
            Ok(Opened::Refused(refusal)) => return Err(refusal.word),
            Err(_) => return Ok(None),
        };
        if conn.ask(&Ask::Run {}).await.is_err() {
            return Ok(None);
        }
        match tokio::time::timeout(ANSWER_WAIT, conn.next_control()).await {
            Ok(Ok(Some(ControlLine::Run(answer)))) => Ok(Some(answer)),
            Ok(Ok(Some(ControlLine::Refused(refusal)))) => Err(refusal.word),
            _ => Ok(None),
        }
    }

    /// **The drop table**: the run read by `run` polls until it says
    /// what came of the verb.
    async fn after_drop(
        &self,
        agent: &str,
        verb: &str,
        mut dropped: Dropped,
    ) -> Result<VerbOutcome, VerbFault> {
        let fault = |kind: &str, message: String| VerbFault {
            kind: kind.into(),
            message,
        };
        tracing::info!(
            "{agent}: {verb}'s connection dropped with the verb outstanding; asking the service run"
        );
        let mut answered = tokio::time::Instant::now();
        let mut first = true;
        loop {
            if !first {
                tokio::time::sleep(self.poll).await;
            }
            first = false;
            let answer = match self.poll_once().await {
                Ok(Some(answer)) => answer,
                Ok(None) => {
                    if answered.elapsed() >= self.verb_bound {
                        return Err(fault(
                            VerbFault::UNKNOWN,
                            format!(
                                "no socket answered within {:?} after {verb}'s connection dropped; whether it took effect is unknown until the next show",
                                self.verb_bound
                            ),
                        ));
                    }
                    continue;
                }
                Err(w) => {
                    let message = format!(
                        "the service refused a run poll after {verb}'s connection dropped, {w}; whether it took effect is unknown until the next show"
                    );
                    self.entry(&entry_kind(&w), message.clone());
                    return Err(fault(VerbFault::UNKNOWN, message));
                }
            };
            answered = tokio::time::Instant::now();
            if answer.instance != dropped.instance {
                return Err(fault(
                    VerbFault::UNKNOWN,
                    format!(
                        "the service restarted after {verb}'s connection dropped; whether it took effect is unknown until the next show"
                    ),
                ));
            }
            let baseline = &dropped.baseline;
            let mine = |run: &str, of: &str| match &dropped.run {
                Some(id) => run == id,
                None => {
                    of == verb
                        && baseline.ended.as_deref() != Some(run)
                        && !baseline.standing.iter().any(|b| b == run)
                }
            };
            if let Some(standing) = answer.standing.iter().find(|s| mine(&s.run, &s.verb)) {
                dropped.run = Some(standing.run.clone());
                continue;
            }
            if let Some(last) = answer.ended
                && mine(last.run(), last.verb())
            {
                return self.ended(agent, last);
            }
            return Err(fault(
                VerbFault::NOT_STARTED,
                format!(
                    "the service holds no run of {verb} after its connection dropped; it did not run"
                ),
            ));
        }
    }
}

impl Invoker for ServiceInvoker {
    fn attach(&self, entries: &Entries) {
        let _ = self.entries.set(entries.clone());
    }

    /// **The ceiling is exactly what the service's grants answer names**,
    /// among the eight verbs (Spec 7.2, 8): a refusal or a line of another
    /// shape is an error naming why, so the hello declares the empty
    /// ceiling with its entry. The caller bounds the whole ask.
    async fn grants(&self) -> anyhow::Result<Vec<String>> {
        let mut conn = match self.control().await? {
            Opened::Open(conn) => conn,
            Opened::Refused(refusal) => anyhow::bail!(
                "the service refused the grants' connection, {}: {}",
                refusal.word,
                refusal.detail
            ),
        };
        conn.ask(&Ask::Grants {}).await?;
        match conn.next_control().await? {
            Some(ControlLine::Grants { verbs }) => {
                let (known, other): (Vec<String>, Vec<String>) =
                    verbs.into_iter().partition(|v| VERBS.contains(&v.as_str()));
                if !other.is_empty() {
                    tracing::warn!(
                        "{}: the grants name verbs admin-con does not ask, left out: {}",
                        self.agent,
                        other.join(", ")
                    );
                }
                Ok(known)
            }
            Some(ControlLine::Refused(refusal)) => anyhow::bail!(
                "the service refused the grants, {}: {}",
                refusal.word,
                refusal.detail
            ),
            Some(other) => anyhow::bail!("the service answered the grants with {other:?}"),
            None => anyhow::bail!("the service closed the grants' connection unanswered"),
        }
    }

    /// **One verb, to what came of it**: its final line, its refusal before
    /// its start, or, after a drop, what the service's `run` answer says.
    /// The principal goes to the service for its record and nowhere else.
    async fn run(
        &self,
        agent: &str,
        verb: &str,
        principal: &Principal,
    ) -> Result<VerbOutcome, VerbFault> {
        tracing::info!("{agent}: asking the service {verb}, for {principal:?}");
        match self.ask_verb(agent, verb, principal).await {
            Ok(done) => done,
            // **A `show` is never standing nor kept as ended** (section
            // 8.4), so no `run` answer can say what came of one.
            Err(_) if verb == "show" => Err(VerbFault {
                kind: VerbFault::UNKNOWN.into(),
                message: "show's connection dropped with the verb outstanding; a show is never listed as standing or ended, so whether it ran is unknown".into(),
            }),
            Err(dropped) => self.after_drop(agent, verb, dropped).await,
        }
    }
}
