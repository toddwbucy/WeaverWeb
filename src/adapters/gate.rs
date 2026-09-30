//! The weaver agent adapter: dial the gate socket, send one request
//! line, read one close line. Dial-per-turn, per Spec section 7.1.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// The line bound, inclusive, excluding the delimiter (gate contract
/// section 5; the number is the framework Spec's, mirrored here).
pub const LINE_BOUND: usize = 32 * 1024;

/// Defensive cap on the close line we will buffer. The contract bounds
/// the request line, not the close; this is weaver-web's own guard
/// against unbounded buffer growth from a misbehaving peer.
pub const CLOSE_BOUND: usize = 1024 * 1024;

#[derive(Debug)]
pub enum GateError {
    /// Socket absent or connection refused: the agent is not loaded.
    Unloaded,
    /// The serialized request exceeded the line bound - weaver-web's
    /// own defect, never sent (Spec section 7.1).
    LineTooLong(usize),
    /// Socket-level failure mid-turn: delivery lost, not the turn
    /// (the record holds the close).
    DeliveryLost(std::io::Error),
    /// The close line did not parse as one JSON object.
    BadClose(String),
    /// The close line exceeded weaver-web's own buffer cap with no
    /// delimiter found.
    CloseTooLong(usize),
}

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GateError::Unloaded => write!(f, "agent is not loaded (no listener)"),
            GateError::LineTooLong(n) => write!(
                f,
                "request line of {n} bytes exceeds the {LINE_BOUND} byte bound"
            ),
            GateError::DeliveryLost(e) => write!(f, "delivery lost mid-turn: {e}"),
            GateError::BadClose(s) => write!(f, "close line did not parse: {s}"),
            GateError::CloseTooLong(n) => {
                write!(
                    f,
                    "close line exceeded the {CLOSE_BOUND} byte buffer cap ({n} bytes read, no delimiter)"
                )
            }
        }
    }
}

impl std::error::Error for GateError {}

/// A parsed close. `kind` is the contract's; the labels are opaque and
/// stored verbatim. `text` is the response body when the close carries
/// one under that member; `raw` always holds the whole close for
/// faithful rendering until the close's field list is verified against
/// a live agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateClose {
    pub kind: String,
    pub run: Option<String>,
    pub turn: Option<String>,
    pub text: Option<String>,
    pub raw: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct GateAdapter {
    socket: PathBuf,
}

impl GateAdapter {
    pub fn new(socket: &Path) -> Self {
        Self {
            socket: socket.to_owned(),
        }
    }

    /// The load-state observable: the socket path's existence, read for
    /// the Agents surface of PRD section 3.6.
    ///
    /// **This read is the inference the charter retired.** The charter is
    /// `weaver-web-PRD`, whose own first line names it so, and its section
    /// 8 records the observation exchange landed 2026-09-04 at issue #440:
    /// load state is answered by the harness's own word rather than
    /// inferred from a socket's existence, and the archived charter's
    /// sentence naming this an inference is retired by that act. The
    /// replacement is `observe`. This code has not followed and moves when
    /// the lifecycle surface is rewritten, per the register.
    pub fn socket_exists(&self) -> bool {
        self.socket.exists()
    }

    /// One turn: dial, write one line, read one line, drop.
    pub async fn turn(&self, text: &str) -> Result<GateClose, GateError> {
        #[derive(Serialize)]
        struct Request<'a> {
            text: &'a str,
        }
        let line =
            serde_json::to_string(&Request { text }).expect("string serialization cannot fail");
        if line.len() > LINE_BOUND {
            return Err(GateError::LineTooLong(line.len()));
        }

        let stream = UnixStream::connect(&self.socket)
            .await
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused => {
                    GateError::Unloaded
                }
                _ => GateError::DeliveryLost(e),
            })?;
        let (read_half, mut write_half) = stream.into_split();

        write_half
            .write_all(line.as_bytes())
            .await
            .map_err(GateError::DeliveryLost)?;
        write_half
            .write_all(b"\n")
            .await
            .map_err(GateError::DeliveryLost)?;
        write_half.flush().await.map_err(GateError::DeliveryLost)?;

        // Take-limit the read so a peer that never sends the delimiter
        // cannot grow the buffer without bound.
        use tokio::io::AsyncReadExt as _;
        let mut reader = BufReader::new(read_half.take(CLOSE_BOUND as u64 + 1));
        let mut close_line = String::new();
        let n = reader
            .read_line(&mut close_line)
            .await
            .map_err(GateError::DeliveryLost)?;
        if n > CLOSE_BOUND && !close_line.ends_with('\n') {
            return Err(GateError::CloseTooLong(n));
        }
        if n == 0 {
            return Err(GateError::DeliveryLost(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "connection closed before the close line",
            )));
        }

        let raw: serde_json::Value = serde_json::from_str(close_line.trim_end())
            .map_err(|e| GateError::BadClose(format!("{e}")))?;
        let get_str = |key: &str| raw.get(key).and_then(|v| v.as_str()).map(str::to_owned);
        let kind = get_str("kind")
            .ok_or_else(|| GateError::BadClose("close carries no string member 'kind'".into()))?;
        Ok(GateClose {
            kind,
            run: get_str("run"),
            turn: get_str("turn"),
            text: get_str("text"),
            raw,
        })
    }
}
