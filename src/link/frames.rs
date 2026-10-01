//! The link's vocabulary (Spec section 8, and the clauses of 7.2 the server
//! enacts): NDJSON over the TLS stream, one frame per line, `svc` the tag.
//! Acts 3 and 4 implement the client side of these shapes.

use crate::adapters::gate::GateClose;
use crate::lifecycle::VerbOutcome;
use crate::traceview::TraceEvent;
use serde::{Deserialize, Serialize};

/// A frame line's bound, inclusive, excluding the delimiter. A line past it
/// with no delimiter found has left the framing and the connection closes
/// below any frame, the gate contract's own rule for its lines.
pub const LINE_BOUND: usize = 4 * 1024 * 1024;

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

/// The acknowledged position of Spec 7.2: a generation admin-con derives
/// from the trace file's durable identity, the byte offset within the file
/// at a record boundary, and the digest of the last acknowledged line. The
/// server holds it for the life of its process and persists nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub generation: String,
    pub offset: u64,
    pub digest: String,
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
    /// plane the file position admin-con reports as its tail, which fixes
    /// the replay boundary (Spec 7.2).
    Hello {
        agent: String,
        plane: Plane,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tail: Option<Position>,
    },
    /// At the cadence the hello's answer named.
    Heartbeat,
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
    /// verbatim with its exit status, or why it was not run.
    Verb {
        id: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<VerbOutcome>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
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
    /// The send cadence, the silence bound divided by four (Spec 8), and
    /// on the admin plane the acknowledged position this server process
    /// holds, or none after a restart (Spec 7.2).
    HelloAnswer {
        cadence_secs: u64,
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
            acknowledged: None,
        };
        let line = serde_json::to_string(&answer).unwrap();
        assert_eq!(line, "{\"svc\":\"hello_answer\",\"cadence_secs\":15}");

        let refusal = ToClient::Refusal {
            reason: Refusal::NotLive,
        };
        assert_eq!(
            serde_json::to_string(&refusal).unwrap(),
            "{\"svc\":\"refusal\",\"reason\":\"not_live\"}"
        );
        assert!(serde_json::from_str::<FromClient>("{\"svc\":\"heartbeat\"}").is_ok());
        assert!(serde_json::from_str::<FromClient>("{\"svc\":\"nonsense\"}").is_err());
    }
}
