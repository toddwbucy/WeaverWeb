//! The link's vocabulary (Spec section 8, and the clauses of 7.2 the server
//! enacts): NDJSON over the TLS stream, one frame per line, `svc` the tag.
//! Acts 3 and 4 implement the client side of these shapes.

use crate::adapters::gate::GateClose;
use crate::traceview::TraceEvent;
use serde::{Deserialize, Serialize};

/// A frame line's bound, inclusive, excluding the delimiter. A line past it
/// with no delimiter found has left the framing and the connection closes
/// below any frame, the gate contract's own rule for its lines.
pub const LINE_BOUND: usize = 4 * 1024 * 1024;

/// **The longest send cadence the link carries, a day**, held by both ends
/// from this one constant: the server refuses a silence bound above four
/// times it (the cadence being the bound divided by four, Spec 8), and a
/// connector refuses a hello's answer naming more, a fault rather than a
/// configuration (a timer that far out would overflow).
pub const CADENCE_MAX_SECS: u64 = 86_400;

/// One read of a frame line.
#[derive(Debug)]
pub enum Line {
    Frame(String),
    /// The peer closed, or the stream failed.
    Closed,
    /// Over the bound with no delimiter found, or not UTF-8.
    Malformed(&'static str),
}

/// **The link's one line reader, for both halves**, so the framing rule
/// has one copy: the server's listener and the connectors' client read
/// through it.
///
/// **The bound is held per read, not after buffering**: what the
/// underlying buffer holds is appended and checked chunk by chunk, so a
/// peer streaming bytes with no delimiter costs at most the bound and one
/// buffer before the line is refused. **And it is cancel-safe**: the only
/// await is the buffer's fill, and a chunk is appended and consumed with
/// no await between, so a read dropped inside a `select!` leaves the
/// partial line in place for the next call rather than losing its bytes.
pub struct LineReader<R> {
    reader: tokio::io::BufReader<R>,
    buf: Vec<u8>,
}

impl<R: tokio::io::AsyncRead + Unpin> LineReader<R> {
    pub fn new(read: R) -> Self {
        Self {
            reader: tokio::io::BufReader::new(read),
            buf: Vec::new(),
        }
    }

    /// The next line, under the bound.
    pub async fn next(&mut self) -> Line {
        use tokio::io::AsyncBufReadExt;
        loop {
            let chunk = match self.reader.fill_buf().await {
                Ok(chunk) => chunk,
                Err(_) => return Line::Closed,
            };
            if chunk.is_empty() {
                return Line::Closed;
            }
            if let Some(i) = chunk.iter().position(|&b| b == b'\n') {
                self.buf.extend_from_slice(&chunk[..i]);
                self.reader.consume(i + 1);
                break;
            }
            let n = chunk.len();
            self.buf.extend_from_slice(chunk);
            self.reader.consume(n);
            if self.buf.len() > LINE_BOUND {
                self.buf.clear();
                return Line::Malformed("a line passed the bound with no delimiter");
            }
        }
        let line = std::mem::take(&mut self.buf);
        if line.len() > LINE_BOUND {
            return Line::Malformed("a line passed the bound");
        }
        match String::from_utf8(line) {
            Ok(s) => Line::Frame(s.trim_end().to_owned()),
            Err(_) => Line::Malformed("a line is not UTF-8"),
        }
    }
}

/// The plane a credential is bound to (Spec 8): a gate-con credential
/// cannot speak as admin-con.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plane {
    Gate,
    Admin,
}

impl Plane {
    pub fn as_str(&self) -> &'static str {
        match self {
            Plane::Gate => "gate",
            Plane::Admin => "admin",
        }
    }
}

impl std::fmt::Display for Plane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Plane {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gate" => Ok(Plane::Gate),
            "admin" => Ok(Plane::Admin),
            other => Err(format!("not a plane: {other} (gate or admin)")),
        }
    }
}

/// The acknowledged position of Spec 7.2, in the relay's terms: the file's
/// identity as the relay's header names it (device, inode and birth time),
/// the byte offset within the file at a record boundary, and the sha256 hex
/// of the whole record ending there, empty at offset zero. The server holds
/// it for the life of its process and persists nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub generation: String,
    pub offset: u64,
    pub digest: String,
}

/// The outcome of one verb as admin answered it (Spec 7.2): one JSON object,
/// a `lifecycle-answer` or a `lifecycle-refusal`, carried verbatim, with
/// whatever the invocation reported beside it so nothing is swallowed. The
/// invoker that produces it is admin-con's (`link::admin_con`); this crate
/// carries no privileged one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerbOutcome {
    pub verb: String,
    pub agent: String,
    pub exit_code: Option<i32>,
    /// The answer parsed as one JSON object, when it is one.
    pub answer: Option<serde_json::Value>,
    /// The raw answer, kept when it did not parse so nothing is swallowed.
    pub raw_stdout: Option<String>,
    pub stderr: Option<String>,
}

/// Who a verb is asked for, **a claim and never an authorization input on
/// the box** (Spec 8, 2.13): the requesting person, or the server for its
/// own asks, of which the admission's `show` is the one today.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Principal {
    Server,
    Person { name: String },
}

/// admin-con's typed error on a verb answer (Spec 8): why the verb was not
/// run, never a refusal of the connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerbFault {
    pub kind: String,
    pub message: String,
}

impl VerbFault {
    /// The ask names a verb outside the ceiling admin-con declared: the
    /// server's own defect, answered and not run, the connection kept.
    pub const OUTSIDE_CEILING: &'static str = "outside_ceiling";
    /// admin-con holds as many asks as it waits behind, its bound.
    pub const BUSY: &'static str = "busy";
    /// admin-con stopped before the verb was invoked: it did not run, and
    /// may be asked again. admin-con's own, like `busy`.
    pub const NOT_STARTED: &'static str = "not_started";
    /// The invocation passed its bound and was ended: whether admin acted
    /// is not known. A bound passed is this fault, never an outcome, so
    /// the fact has one shape.
    pub const UNKNOWN: &'static str = "unknown";
    /// The server's own: admin answered, and the store could not land the
    /// answer, so the ask answers the failure and is owed again.
    pub const NOT_LANDED: &'static str = "not_landed";
    /// The invocation ended with no answer to read: a status other than an
    /// answer's or a refusal's, no answer object, or one past its bound.
    /// The caller reads the next `show` for the real state.
    pub const FAULT: &'static str = "fault";
}

/// The gate adapter's typed error as a connector carries it (section 7.1's
/// variants verbatim in kind).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnFault {
    pub kind: String,
    pub message: String,
}

/// Frames a connector sends the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "svc", rename_all = "snake_case")]
pub enum FromClient {
    /// The roster: the agent's name and the plane, a check against the
    /// certificate's binding and never a source (Spec 8). On the admin
    /// plane the trace door's state, and where it is open the position
    /// admin-con took as the opening's boundary, which fixes the replay's
    /// end (Spec 7.2); a closed door carries none.
    Hello {
        agent: String,
        plane: Plane,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<Position>,
        /// **On the admin plane, the trace door's state** (Spec 7.2, 2.12):
        /// open where admin-con reached the relay, closed where it did not.
        /// Absent on the gate plane.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        door: Option<bool>,
        /// **On the admin plane, the ceiling** (Spec 8): exactly the verbs
        /// admin-con's invoker `grants`, the lines the box's sudo rules
        /// grant its user. Absent on the gate plane, which declares none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ceiling: Option<Vec<String>>,
    },
    /// At the cadence the hello's answer named.
    Heartbeat,
    /// **The replay has reached the boundary** (Spec 7.2): sent once by
    /// admin-con when what it relays from behind the file's tail is done,
    /// immediately after the hello's answer when there is nothing to
    /// replay. The server classifies every event before it as replayed and
    /// every event after it as live.
    CaughtUp,
    /// The gate plane's answer to a turn ask: the close, or the typed
    /// error.
    Turn {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        close: Option<GateClose>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<TurnFault>,
    },
    /// The admin plane's answer to a verb ask: admin's JSON object
    /// verbatim with its exit status, or the typed reason it was not run.
    Verb {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<VerbOutcome>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<VerbFault>,
    },
    /// **The trace door's state changed on the live connection** (Spec
    /// 7.2, 2.12), dated by admin-con. **An opening is an admission of the
    /// trace**: it carries the boundary admin-con took, events until the
    /// next `caught_up` are the opening's replay, and the server asks
    /// `show` where the ceiling grants it. A closing carries none.
    Door {
        open: bool,
        wall_ms: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<Position>,
    },
    /// One trace event from admin-con with its position and whether it
    /// was relayed from behind the file's tail (Spec 7.2).
    Event {
        position: Position,
        replayed: bool,
        event: TraceEvent,
    },
}

/// Frames the server sends a connector.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "svc", rename_all = "snake_case")]
pub enum ToClient {
    /// The send cadence, the silence bound divided by four (Spec 8), **the
    /// silence bound itself**, which the connector checks the cadence
    /// against (no opening's hold is timed by it: the hold runs on no clock
    /// of the connector's), and
    /// on the admin plane the acknowledged position this server process
    /// holds, or none after a restart (Spec 7.2).
    HelloAnswer {
        cadence_secs: u64,
        silence_secs: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        acknowledged: Option<Position>,
    },
    Turn {
        id: u64,
        text: String,
    },
    Verb {
        id: u64,
        verb: String,
        /// The principal, as a claim the box records (Spec 8).
        principal: Principal,
    },
    /// The position the server has landed through.
    Ack {
        position: Position,
    },
    /// Typed, before the connection closes.
    Refusal {
        reason: Refusal,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The credential is absent from the register or revoked (Spec 8).
    NotLive,
    /// The hello's name or plane disagrees with the certificate's binding.
    RosterMismatch,
    /// A connection is already installed for this credential.
    AlreadyConnected,
    /// Silent for the bound.
    Silence,
    /// A frame that belongs to the other plane.
    WrongPlane,
    /// A line that is not a frame, or a frame out of order.
    Malformed,
    /// The store could not land what this connection carried; the
    /// connection closes at the last acknowledged position and the
    /// connector reconnects and resends (Spec 7.2).
    StoreUnavailable,
    /// The admission's `show` did not answer with a usable observation
    /// within the silence bound, so the row would keep its tuple and load
    /// state from before the reconnect; the connection closes so the
    /// reconnect asks it again.
    AdmissionIncomplete,
}

impl Refusal {
    pub fn as_str(&self) -> &'static str {
        match self {
            Refusal::NotLive => "not_live",
            Refusal::RosterMismatch => "roster_mismatch",
            Refusal::AlreadyConnected => "already_connected",
            Refusal::Silence => "silence",
            Refusal::WrongPlane => "wrong_plane",
            Refusal::Malformed => "malformed",
            Refusal::StoreUnavailable => "store_unavailable",
            Refusal::AdmissionIncomplete => "admission_incomplete",
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod shape {
    use super::*;

    /// The tag is `svc` and the variants are snake case, the seed's shape,
    /// so acts 3 and 4 build against one convention.
    #[test]
    fn frames_round_trip_under_the_svc_tag() {
        let hello = FromClient::Hello {
            agent: "karl".into(),
            plane: Plane::Admin,
            tail: Some(Position {
                generation: "g1".into(),
                offset: 120,
                digest: "d".into(),
            }),
            ceiling: Some(vec!["show".to_owned()]),
            door: Some(true),
        };
        let line = serde_json::to_string(&hello).unwrap();
        assert!(line.starts_with("{\"svc\":\"hello\""), "{line}");
        assert!(line.contains("\"plane\":\"admin\""), "{line}");
        let back: FromClient = serde_json::from_str(&line).unwrap();
        assert!(matches!(
            back,
            FromClient::Hello {
                plane: Plane::Admin,
                ..
            }
        ));

        let answer = ToClient::HelloAnswer {
            cadence_secs: 15,
            silence_secs: 60,
            acknowledged: None,
        };
        let line = serde_json::to_string(&answer).unwrap();
        assert_eq!(
            line,
            "{\"svc\":\"hello_answer\",\"cadence_secs\":15,\"silence_secs\":60}"
        );
        // A cadence with no bound is not an answer.
        assert!(
            serde_json::from_str::<ToClient>("{\"svc\":\"hello_answer\",\"cadence_secs\":15}")
                .is_err()
        );

        let refusal = ToClient::Refusal {
            reason: Refusal::NotLive,
        };
        assert_eq!(
            serde_json::to_string(&refusal).unwrap(),
            "{\"svc\":\"refusal\",\"reason\":\"not_live\"}"
        );
        assert!(serde_json::from_str::<FromClient>("{\"svc\":\"heartbeat\"}").is_ok());
        assert!(serde_json::from_str::<FromClient>("{\"svc\":\"caught_up\"}").is_ok());
        assert!(serde_json::from_str::<FromClient>("{\"svc\":\"nonsense\"}").is_err());
    }
}

#[cfg(test)]
mod reader {
    use super::*;
    use std::time::Duration;
    use tokio::io::AsyncWriteExt;

    /// **A read dropped mid-line keeps its bytes.** gate-con's relay drops
    /// a pending read whenever a turn completes, so half a line read before
    /// the drop must still be there for the next call: exactly one intact
    /// frame comes out.
    #[tokio::test]
    async fn a_read_dropped_mid_line_keeps_its_bytes() {
        let (mut peer, ours) = tokio::io::duplex(1024);
        let mut reader = LineReader::new(ours);
        peer.write_all(br#"{"svc":"tu"#).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), reader.next())
                .await
                .is_err(),
            "no line yet"
        );
        peer.write_all(br#"rn","id":7,"text":"hi"}"#).await.unwrap();
        peer.write_all(b"\n").await.unwrap();
        match reader.next().await {
            Line::Frame(line) => {
                assert_eq!(line, r#"{"svc":"turn","id":7,"text":"hi"}"#);
                assert!(matches!(
                    serde_json::from_str::<ToClient>(&line).unwrap(),
                    ToClient::Turn { id: 7, .. }
                ));
            }
            other => panic!("expected the whole frame, got {other:?}"),
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(50), reader.next())
                .await
                .is_err(),
            "exactly one frame"
        );
    }
}
