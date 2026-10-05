//! The trace relay's client (Spec 7.2): admin-con's one reader of the
//! agent's record, through the door the agent's start step opens, per
//! `weaver-types-Spec` section 3.1. admin-con reads no file: it dials the
//! relay's socket, sends one `TraceRequest` line naming a position, and
//! reads a `TraceHeader` line naming the file's identity, then the trace's
//! own lines byte for byte, a heartbeat while the relay is idle, and
//! `truncated` before the stream ends where the file shrank below it.
//!
//! **The shapes are re-declared here, never linked**: this crate links none
//! of the agent's crates, so the request and the stream's own lines are
//! this module's types, built to the wire the section names.
//!
//! **A position is the relay's**: the file's identity as the header names
//! it, a byte offset on a record boundary, and the sha256 hex of the whole
//! record ending there, from the start of its line through its newline,
//! empty at offset zero. A record of any length is hashed whole, in pieces,
//! so a position after a record past `RECORD_BOUND` is still one the relay
//! verifies.

use crate::link::frames::{LINE_BOUND, Position};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// A record longer than this is not relayed: its event, re-encoded inside a
/// frame, could pass the link's line bound. It is read through, hashed whole
/// for the position after it, and marked instead.
pub const RECORD_BOUND: usize = LINE_BOUND / 2;

/// The request's bound, newline included, per the wire.
pub const REQUEST_BOUND: usize = 4096;

/// How long a dial waits for the header. The relay verifies a position in
/// bounded steps before it sends a byte, so a resume after a long record
/// takes several of its ticks; a relay that says nothing for this long is
/// taken as a closed door and redialed on the backoff.
pub const HEADER_BOUND: Duration = Duration::from_secs(30);

/// A stream line's own prefix: one JSON object whose one member is
/// `trace_stream`, which no trace event carries.
const CONTROL_PREFIX: &[u8] = b"{\"trace_stream\":";

/// A line longer than this is never one of the stream's own lines.
const CONTROL_BOUND: usize = 4096;

#[derive(Serialize)]
struct TraceRequest<'a> {
    offset: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    prior_digest: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceLine {
    trace_stream: TraceControl,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TraceControl {
    Header(TraceHeader),
    Heartbeat { wall_ms: u64 },
    Truncated { size: u64 },
}

#[derive(Deserialize)]
struct TraceHeader {
    device: u64,
    inode: u64,
    #[serde(default)]
    birth_ns: Option<i128>,
}

/// **The file's identity, as the header names it**: device, inode and birth
/// time, the birth time `-` where the filesystem reports none, never a zero
/// that would let a reused device and inode read as the same file.
fn identity(header: &TraceHeader) -> String {
    let birth = header
        .birth_ns
        .map_or_else(|| "-".to_owned(), |b| b.to_string());
    format!("{}:{}:{birth}", header.device, header.inode)
}

/// The sha256 hex of a record, its newline included.
pub fn digest(record: &[u8]) -> String {
    format!("{:x}", Sha256::digest(record))
}

/// Offset zero in a file of the given identity, which carries no digest.
pub fn zero(identity: &str) -> Position {
    Position {
        generation: identity.to_owned(),
        offset: 0,
        digest: String::new(),
    }
}

/// What a dial came to.
pub enum Dial {
    /// The relay admitted the request and named its file.
    Open(Box<Stream>),
    /// The relay closed before a header: it refused the position, which no
    /// longer names the record the reader read there.
    Refused,
    /// No relay stands at the socket, or it said nothing usable: the door
    /// is closed.
    Closed(String),
}

/// **Dial the relay and request the stream from `from`**: one request line,
/// then the header within `HEADER_BOUND`. The offset is sent with the
/// position's digest, and offset zero with none, as the relay requires.
pub async fn dial(socket: &Path, from: &Position) -> Dial {
    let mut stream = match UnixStream::connect(socket).await {
        Ok(stream) => stream,
        Err(e) => return Dial::Closed(format!("dialing the trace relay: {e}")),
    };
    let request = TraceRequest {
        offset: from.offset,
        prior_digest: (from.offset > 0).then_some(from.digest.as_str()),
    };
    let mut line = serde_json::to_vec(&request).expect("a request serializes");
    line.push(b'\n');
    if line.len() > REQUEST_BOUND {
        return Dial::Closed(format!(
            "a request of {} bytes, past the relay's {REQUEST_BOUND} byte bound",
            line.len()
        ));
    }
    if let Err(e) = stream.write_all(&line).await {
        return Dial::Closed(format!("sending the trace request: {e}"));
    }
    let mut stream = Stream {
        reader: BufReader::new(stream),
        identity: String::new(),
        line: Vec::new(),
        over: None,
    };
    match tokio::time::timeout(HEADER_BOUND, stream.line()).await {
        Err(_) => Dial::Closed(format!(
            "the trace relay sent no header within {HEADER_BOUND:?}"
        )),
        Ok(Raw::Ended(_)) => Dial::Refused,
        Ok(Raw::Control(TraceControl::Header(header))) => {
            stream.identity = identity(&header);
            Dial::Open(Box::new(stream))
        }
        Ok(_) => Dial::Closed("the trace relay's first line was not a header".to_owned()),
    }
}

/// One thing the stream carried after its header.
#[derive(Debug)]
pub enum Read {
    /// A whole record, its newline included, with its digest.
    Record { line: Vec<u8>, digest: String },
    /// A record past `RECORD_BOUND`, read through and hashed whole.
    Oversized { len: u64, digest: String },
    /// The relay is idle: everything the file held at `wall_ms` was sent.
    Heartbeat { wall_ms: u64 },
    /// The file shrank below the stream's position; the stream ends.
    Truncated { size: u64 },
    /// The stream ended, or broke the wire's shape.
    Ended(String),
}

enum Raw {
    Record(Vec<u8>),
    Oversized { len: u64, digest: String },
    Control(TraceControl),
    Ended(String),
}

/// The relay's stream, after its header.
pub struct Stream {
    reader: BufReader<UnixStream>,
    /// The file's identity, from the header.
    pub identity: String,
    /// The record read so far, while it is within `RECORD_BOUND`.
    line: Vec<u8>,
    /// A record past the bound, being hashed through: its hash and length.
    over: Option<(Sha256, u64)>,
}

impl Stream {
    /// The next thing the stream carries. **Cancel-safe**, as the link's
    /// reader is: the only await is the buffer's fill, and a chunk is taken
    /// and consumed with no await between, so a read dropped inside a
    /// `select!` leaves a partial record in place for the next call.
    pub async fn next(&mut self) -> Read {
        match self.line().await {
            Raw::Record(line) => Read::Record {
                digest: digest(&line),
                line,
            },
            Raw::Oversized { len, digest } => Read::Oversized { len, digest },
            Raw::Control(TraceControl::Heartbeat { wall_ms }) => Read::Heartbeat { wall_ms },
            Raw::Control(TraceControl::Truncated { size }) => Read::Truncated { size },
            Raw::Control(TraceControl::Header(_)) => {
                Read::Ended("the trace relay sent a second header".to_owned())
            }
            Raw::Ended(why) => Read::Ended(why),
        }
    }

    async fn line(&mut self) -> Raw {
        loop {
            let chunk = match self.reader.fill_buf().await {
                Ok(chunk) => chunk,
                Err(e) => return Raw::Ended(format!("reading the trace relay: {e}")),
            };
            if chunk.is_empty() {
                let partial = self.line.len() as u64 + self.over.as_ref().map_or(0, |o| o.1);
                return Raw::Ended(if partial > 0 {
                    format!("the trace relay's stream ended inside a record, {partial} bytes in")
                } else {
                    "the trace relay's stream ended".to_owned()
                });
            }
            let newline = chunk.iter().position(|&b| b == b'\n');
            let take = newline.map_or(chunk.len(), |i| i + 1);
            if let Some((hasher, len)) = &mut self.over {
                hasher.update(&chunk[..take]);
                *len += take as u64;
            } else {
                self.line.extend_from_slice(&chunk[..take]);
                if self.line.len() > RECORD_BOUND {
                    let mut hasher = Sha256::new();
                    hasher.update(&self.line);
                    self.over = Some((hasher, self.line.len() as u64));
                    self.line = Vec::new();
                }
            }
            self.reader.consume(take);
            if newline.is_none() {
                continue;
            }
            if let Some((hasher, len)) = self.over.take() {
                return Raw::Oversized {
                    len,
                    digest: format!("{:x}", hasher.finalize()),
                };
            }
            let line = std::mem::take(&mut self.line);
            if line.len() <= CONTROL_BOUND
                && line.starts_with(CONTROL_PREFIX)
                && let Ok(TraceLine { trace_stream }) = serde_json::from_slice(&line)
            {
                return Raw::Control(trace_stream);
            }
            return Raw::Record(line);
        }
    }
}
