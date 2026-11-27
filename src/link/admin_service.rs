//! conforms: web-the-service-socket-bounds-every-line
//!
//! **admin-con's client of the weaver-admin service's socket** (Spec 7.2), per
//! `weaver-admin-operator-contract` section 8 at WeaverAgent `265a332d`: the
//! connector asks the service which verbs are granted, to run a verb, for the
//! trace from a position, and for the admin log's lines, and invokes nothing
//! itself.
//!
//! **The framing is the contract's** (its section 8.2): every message is one
//! JSON object on one line ending in a newline, externally tagged, its one
//! member naming the message. Each connection opens with one line from the
//! connector naming the version, the purpose and the agent, and the service
//! answers it with `open`, naming its `instance`, or with one `refused` line
//! and the close. On a trace connection, after `open`, the bytes are the
//! relay's (`link::relay`), so this module hands the stream over and reads no
//! more of it.
//!
//! **Every line is read within its bound and never past it** (section 8.7):
//! the opening's answer at 4096 bytes with its newline, within
//! `ANSWER_WAIT`; a control line at `CONTROL_LINE_BOUND`; a log line at
//! `LOG_LINE_BOUND`. A line past its bound, one that ends without a newline,
//! or one that does not parse is the service's malformed answer, never read
//! further. The connector's own lines are checked against their bound before
//! a byte is sent.
//!
//! **The shapes are re-declared here, never linked**: this crate links none
//! of the agent's crates, so each message is this module's type, built to the
//! contract's text.

use crate::link::frames::Principal;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The contract version this connector speaks (section 8.2).
pub const VERSION: u64 = 1;

/// The opening line and the service's answer to it, each with its newline
/// (section 8.7).
pub const OPENING_BOUND: usize = 4096;

/// A connector's line on a control connection, with its newline.
pub const ASK_BOUND: usize = 4096;

/// **How long the connector waits for the opening's answer.** The service
/// answers within 10 seconds of the opening line, its log's judging and its
/// relay's dial included (section 8.7); the connector reads one second past
/// that, so a refusal sent at the service's edge still arrives, and reads
/// past it as a fault.
pub const ANSWER_WAIT: Duration = Duration::from_secs(11);

/// A line the service sends on a control connection, with its newline: a
/// 64 KiB object, 16 KiB of standard error at most 96 KiB escaped, and the
/// members (section 8.7).
pub const CONTROL_LINE_BOUND: usize = 256 * 1024;

/// **A line the service sends on a log connection, with its newline.** The
/// contract carries a log line of 8 KiB at most and names no bound for the
/// message around it; an 8 KiB line escaped as JSON is at most six times
/// that, so 64 KiB holds any message the service may send, and nothing a
/// well-formed service sends is past it.
pub const LOG_LINE_BOUND: usize = 64 * 1024;

/// What a connection is opened for (section 8.2), a closed list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Control,
    Trace,
    Log,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Control => "control",
            Purpose::Trace => "trace",
            Purpose::Log => "log",
        }
    }
}

/// **The service's refusal words** (section 8.2's table), every one.
pub mod word {
    pub const OPENING_MALFORMED: &str = "opening_malformed";
    pub const OPENING_LATE: &str = "opening_late";
    pub const VERSION_UNKNOWN: &str = "version_unknown";
    pub const PURPOSE_UNKNOWN: &str = "purpose_unknown";
    pub const NAME_REFUSED: &str = "name_refused";
    pub const LOG_UNREAD: &str = "log_unread";
    pub const RELAY_UNREACHABLE: &str = "relay_unreachable";
    pub const FULL: &str = "full";
    pub const ASK_UNKNOWN: &str = "ask_unknown";
    pub const ASK_MALFORMED: &str = "ask_malformed";
    pub const ASK_OUTSTANDING: &str = "ask_outstanding";
    pub const GRANTS_UNREAD: &str = "grants_unread";
    pub const VERB_UNKNOWN: &str = "verb_unknown";
    pub const NOT_GRANTED: &str = "not_granted";
    pub const GRANT_UNREAD: &str = "grant_unread";
    pub const BUSY: &str = "busy";
    pub const NOT_STARTED: &str = "not_started";
    pub const LOG_BEHIND: &str = "log_behind";

    /// The words that can answer an opening, on any purpose but the two
    /// that name their own.
    pub const OPENING: [&str; 6] = [
        OPENING_MALFORMED,
        OPENING_LATE,
        VERSION_UNKNOWN,
        PURPOSE_UNKNOWN,
        NAME_REFUSED,
        FULL,
    ];
}

/// The eight verbs, in the order a grants answer lists them (section 8.3).
pub const VERBS: [&str; 8] = [
    "show",
    "validate",
    "load",
    "unload",
    "stop",
    "save-point",
    "restore",
    "force-unload",
];

/// **One `refused` line** (section 8.2): its word, its detail, and the
/// members a word names beside them, `run` on `busy`, `versions` on
/// `version_unknown` and `purposes` on `purpose_unknown`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refused {
    pub word: String,
    #[serde(default)]
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub versions: Option<Vec<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purposes: Option<Vec<String>>,
}

impl Refused {
    /// A refusal with its word and detail alone.
    pub fn new(word: &str, detail: impl Into<String>) -> Self {
        Self {
            word: word.to_owned(),
            detail: detail.into(),
            run: None,
            versions: None,
            purposes: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct OpeningLine {
    pub(crate) weaver_admin: OpeningBody,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct OpeningBody {
    pub(crate) version: u64,
    pub(crate) purpose: Purpose,
    pub(crate) agent: String,
}

/// The service's answer to an opening line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpeningAnswer {
    Open { version: u64, instance: String },
    Refused(Refused),
}

/// **An ask on a control connection** (sections 8.3 and 8.4), one at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ask {
    Grants {},
    Verb { verb: String, principal: Principal },
    Run {},
}

/// **A run that ended with exit 0 or 1 and one object** (section 8.4): the
/// object carried untouched, a `lifecycle-answer` at 0 or a
/// `lifecycle-refusal` at 1, and standard error at most 16 KiB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ran {
    pub run: String,
    pub verb: String,
    pub exit_code: i32,
    pub answer: serde_json::Value,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub stderr_cut: bool,
}

/// **A run that ended any other way** (section 8.4): whether it acted is read
/// with the next `show`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fault {
    pub run: String,
    pub verb: String,
    #[serde(default)]
    pub detail: String,
}

/// A run's final line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Final {
    Ran(Ran),
    Fault(Fault),
}

impl Final {
    pub fn run(&self) -> &str {
        match self {
            Final::Ran(ran) => &ran.run,
            Final::Fault(fault) => &fault.run,
        }
    }

    pub fn verb(&self) -> &str {
        match self {
            Final::Ran(ran) => &ran.verb,
            Final::Fault(fault) => &fault.verb,
        }
    }
}

/// A run standing, as a `run` answer lists it: `started_ms` null while its
/// root program has not started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Standing {
    pub run: String,
    pub verb: String,
    pub started_ms: Option<u64>,
}

/// **The answer to a `run` ask** (section 8.4): the service's `instance`, the
/// agent's runs standing, a `show` excepted, and the last that ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunAnswer {
    pub instance: String,
    pub standing: Vec<Standing>,
    pub ended: Option<Final>,
}

/// **A line the service sends on a control connection.**
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlLine {
    Grants { verbs: Vec<String> },
    Started { run: String },
    Ran(Ran),
    Fault(Fault),
    Run(RunAnswer),
    Refused(Refused),
}

/// What a truncation or a replacement of `admin.log` is sent as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Discontinuity {
    Truncated,
    Replaced,
}

/// **One message on a log connection** (section 8.6).
#[derive(Debug, Clone, PartialEq)]
pub enum LogEvent {
    /// An appended line that is one JSON object, and whether it names the
    /// service's account as its caller or peer.
    Line {
        line: serde_json::Value,
        through_service: bool,
    },
    /// An appended line that is not one JSON object.
    Text {
        text: String,
        through_service: bool,
    },
    /// A line past 8 KiB, not carried: its length.
    Oversized(u64),
    Discontinuity(Discontinuity),
}

/// A line the service sends on a log connection.
#[derive(Debug, Clone, PartialEq)]
pub enum LogLine {
    Log(LogEvent),
    Refused(Refused),
}

/// The log message's body as read, before its one shape is judged.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogBody {
    #[serde(default)]
    line: Option<serde_json::Value>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    through_service: Option<bool>,
    #[serde(default)]
    oversized: Option<u64>,
    #[serde(default)]
    discontinuity: Option<Discontinuity>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum LogLineRaw {
    Log(LogBody),
    Refused(Refused),
}

impl LogEvent {
    /// The message as the service writes it, its newline included.
    pub fn to_line(&self) -> Vec<u8> {
        let body = match self {
            LogEvent::Line {
                line,
                through_service,
            } => serde_json::json!({"line": line, "through_service": through_service}),
            LogEvent::Text {
                text,
                through_service,
            } => serde_json::json!({"text": text, "through_service": through_service}),
            LogEvent::Oversized(length) => serde_json::json!({"oversized": length}),
            LogEvent::Discontinuity(what) => serde_json::json!({"discontinuity": what}),
        };
        to_line(&serde_json::json!({ "log": body }))
    }
}

/// **A log line's one shape**: exactly one of the four bodies, `line` an
/// object and `text` a string each with its flag, `oversized` and
/// `discontinuity` alone.
fn log_line(bytes: &[u8]) -> Result<LogLine, String> {
    let raw: LogLineRaw =
        serde_json::from_slice(bytes).map_err(|e| format!("a log line does not parse: {e}"))?;
    let body = match raw {
        LogLineRaw::Refused(refused) => return Ok(LogLine::Refused(refused)),
        LogLineRaw::Log(body) => body,
    };
    let event = match body {
        LogBody {
            line: Some(line),
            text: None,
            through_service: Some(through_service),
            oversized: None,
            discontinuity: None,
        } if line.is_object() => LogEvent::Line {
            line,
            through_service,
        },
        LogBody {
            line: None,
            text: Some(text),
            through_service: Some(through_service),
            oversized: None,
            discontinuity: None,
        } => LogEvent::Text {
            text,
            through_service,
        },
        LogBody {
            line: None,
            text: None,
            through_service: None,
            oversized: Some(length),
            discontinuity: None,
        } => LogEvent::Oversized(length),
        LogBody {
            line: None,
            text: None,
            through_service: None,
            oversized: None,
            discontinuity: Some(what),
        } => LogEvent::Discontinuity(what),
        _ => return Err("a log line has no one shape of the four".to_owned()),
    };
    Ok(LogLine::Log(event))
}

/// One JSON value as a line, its newline included.
pub(crate) fn to_line<T: Serialize>(value: &T) -> Vec<u8> {
    let mut line = serde_json::to_vec(value).expect("a message serializes");
    line.push(b'\n');
    line
}

/// **An `instance` or a run's id**: 16 hex digits, of either case, as the
/// contract writes "16 hex" for both and names a case only for a person's
/// id. An id is compared as it is written.
pub fn is_id(text: &str) -> bool {
    text.len() == 16 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Why a line could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineError {
    /// The line passed its bound before its newline: read no further.
    TooLong(usize),
    /// The connection ended inside a line.
    Truncated,
    /// The line does not parse as a message of its connection.
    Malformed(String),
    Io(String),
}

impl std::fmt::Display for LineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LineError::TooLong(bound) => {
                write!(f, "the service sent a line past its {bound} byte bound")
            }
            LineError::Truncated => write!(f, "the service's connection ended inside a line"),
            LineError::Malformed(why) => write!(f, "the service sent a malformed line: {why}"),
            LineError::Io(e) => write!(f, "reading the service's socket: {e}"),
        }
    }
}

/// **Read one line within `bound` bytes, its newline included**: `None` where
/// the connection ended before any byte of a line. Bytes past the bound are
/// never kept: the read stops there.
pub(crate) async fn read_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    bound: usize,
) -> Result<Option<Vec<u8>>, LineError> {
    read_line_into(reader, &mut Vec::new(), bound).await
}

/// **One line, taken up where an earlier call stood**: `partial` holds what
/// a call dropped mid-line had already taken from the reader, so a caller
/// that bounds the read with a timeout loses no byte and the next call
/// never starts mid-line. Cancel-safe at its one await, `fill_buf`, since
/// every byte consumed is in `partial` before the next. Emptied on every
/// answer, a line or an error.
pub(crate) async fn read_line_into<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    partial: &mut Vec<u8>,
    bound: usize,
) -> Result<Option<Vec<u8>>, LineError> {
    let result = loop {
        let buf = match reader.fill_buf().await {
            Ok(buf) => buf,
            Err(e) => break Err(LineError::Io(e.to_string())),
        };
        if buf.is_empty() {
            break if partial.is_empty() {
                Ok(None)
            } else {
                Err(LineError::Truncated)
            };
        }
        let room = bound - partial.len();
        match buf.iter().take(room).position(|&b| b == b'\n') {
            Some(i) => {
                partial.extend_from_slice(&buf[..=i]);
                reader.consume(i + 1);
                break Ok(Some(std::mem::take(partial)));
            }
            None if buf.len() >= room => break Err(LineError::TooLong(bound)),
            None => {
                let n = buf.len();
                partial.extend_from_slice(buf);
                reader.consume(n);
            }
        }
    };
    if result.is_err() {
        partial.clear();
    }
    result
}

/// Why an opening came to nothing the service said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// No service answers at the socket: it is absent, or refuses connects.
    Unreachable(String),
    /// The service closed the connection before any byte of an answer, as it
    /// closes a peer it does not admit.
    Closed,
    /// No complete answer arrived within the wait.
    Late,
    /// The answer passed its bound, ended without a newline, did not parse,
    /// or named a version or an `instance` of another shape.
    Malformed(String),
    /// The connector's own opening line would pass its bound: never sent.
    TooLong,
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Unreachable(e) => write!(f, "no service answers at its socket: {e}"),
            OpenError::Closed => write!(
                f,
                "the service closed the connection before answering; its own log names why"
            ),
            OpenError::Late => write!(
                f,
                "the service gave no answer to the opening within {ANSWER_WAIT:?}"
            ),
            OpenError::Malformed(why) => write!(f, "the service's answer is malformed: {why}"),
            OpenError::TooLong => write!(
                f,
                "the opening line would pass its {OPENING_BOUND} byte bound: the agent's name is too long"
            ),
        }
    }
}

/// What an opening came to.
#[derive(Debug)]
pub enum Opened {
    /// The service answered `open`: the purpose's exchange follows.
    Open(Connection),
    /// The service refused the opening with one of its words, and closed.
    Refused(Refused),
}

/// **A connection the service opened.**
#[derive(Debug)]
pub struct Connection {
    reader: BufReader<UnixStream>,
    /// What a read dropped mid-line had taken, the next read's start.
    partial: Vec<u8>,
    /// The service's start, as its `open` named it (section 8.4).
    pub instance: String,
    pub purpose: Purpose,
}

/// **Open a connection for `purpose` on `agent`**: one opening line, then one
/// answer within `wait`, read at `OPENING_BOUND`. The connect itself counts
/// against the same wait.
pub async fn open(
    socket: &Path,
    purpose: Purpose,
    agent: &str,
    wait: Duration,
) -> Result<Opened, OpenError> {
    let opening = to_line(&OpeningLine {
        weaver_admin: OpeningBody {
            version: VERSION,
            purpose,
            agent: agent.to_owned(),
        },
    });
    if opening.len() > OPENING_BOUND {
        return Err(OpenError::TooLong);
    }
    let until = tokio::time::Instant::now() + wait;
    let stream = match tokio::time::timeout_at(until, UnixStream::connect(socket)).await {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => return Err(OpenError::Unreachable(format!("{}: {e}", socket.display()))),
        Err(_) => return Err(OpenError::Late),
    };
    let mut reader = BufReader::new(stream);
    // A service that refuses `full` sends before it reads the opening, and
    // closes: a write that fails then is no reason to miss its line.
    let _ = tokio::time::timeout_at(until, reader.get_mut().write_all(&opening)).await;
    let line = match tokio::time::timeout_at(until, read_line(&mut reader, OPENING_BOUND)).await {
        Err(_) => return Err(OpenError::Late),
        Ok(Ok(Some(line))) => line,
        Ok(Ok(None)) => return Err(OpenError::Closed),
        Ok(Err(LineError::TooLong(bound))) => {
            return Err(OpenError::Malformed(format!(
                "the answer passed its {bound} byte bound"
            )));
        }
        Ok(Err(LineError::Truncated)) => {
            return Err(OpenError::Malformed(
                "the answer ended without a newline".to_owned(),
            ));
        }
        Ok(Err(e)) => return Err(OpenError::Malformed(e.to_string())),
    };
    match serde_json::from_slice::<OpeningAnswer>(&line) {
        Ok(OpeningAnswer::Open { version, instance }) => {
            if version != VERSION {
                return Err(OpenError::Malformed(format!(
                    "the service opened version {version}, not the {VERSION} asked"
                )));
            }
            if !is_id(&instance) {
                return Err(OpenError::Malformed(
                    "the instance is not 16 lowercase hex digits".to_owned(),
                ));
            }
            Ok(Opened::Open(Connection {
                partial: Vec::new(),
                reader,
                instance,
                purpose,
            }))
        }
        Ok(OpeningAnswer::Refused(refused)) => Ok(Opened::Refused(refused)),
        Err(e) => Err(OpenError::Malformed(format!(
            "the answer does not parse: {e}"
        ))),
    }
}

impl Connection {
    /// **Send one ask**, checked against `ASK_BOUND` before a byte is sent.
    pub async fn ask(&mut self, ask: &Ask) -> std::io::Result<()> {
        let line = to_line(ask);
        if line.len() > ASK_BOUND {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("an ask passed its {ASK_BOUND} byte bound and was not sent"),
            ));
        }
        let stream = self.reader.get_mut();
        stream.write_all(&line).await?;
        stream.flush().await
    }

    /// **The next line of a control connection**, read at
    /// `CONTROL_LINE_BOUND`: `None` where the service closed. It waits as
    /// long as the service takes, since a run's length is unbounded at the
    /// service (section 8.7); a caller bounds it.
    pub async fn next_control(&mut self) -> Result<Option<ControlLine>, LineError> {
        let Some(line) =
            read_line_into(&mut self.reader, &mut self.partial, CONTROL_LINE_BOUND).await?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&line)
            .map(Some)
            .map_err(|e| LineError::Malformed(format!("a control line does not parse: {e}")))
    }

    /// **The next line of a log connection**, read at `LOG_LINE_BOUND`:
    /// `None` where the service closed.
    pub async fn next_log(&mut self) -> Result<Option<LogLine>, LineError> {
        let Some(line) =
            read_line_into(&mut self.reader, &mut self.partial, LOG_LINE_BOUND).await?
        else {
            return Ok(None);
        };
        log_line(&line).map(Some).map_err(LineError::Malformed)
    }

    /// **The stream, for the relay's wire after a trace's `open`**, with any
    /// byte already read past the answer kept in its buffer. Taken at a
    /// line's boundary: a part line a dropped read held is discarded.
    pub fn into_inner(self) -> BufReader<UnixStream> {
        self.reader
    }
}
