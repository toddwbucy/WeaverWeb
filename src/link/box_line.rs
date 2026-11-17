//! **The box commands' lines** (Spec 8.2): every command writes JSON lines
//! on standard output, one object per line, each with an `event` member, the
//! last its outcome, and its exit status agrees with that line. Diagnostics
//! go to standard error.
//!
//! **One printer reaches the terminal**, and it escapes every control and
//! non-ASCII byte in a fault or a reason before it is written: a line on
//! standard output is JSON whose strings escape both, and a reason on
//! standard error writes each such byte as `\xNN`. A console's words, a
//! peer's error text and a path the operator typed all pass through it, so
//! none reaches the terminal with an escape sequence in it.

use std::io::Write;

use serde::Serialize;

/// **One set of exit statuses serves every command** (Spec 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Paired, adopted, described or accepted.
    Done,
    /// The command's own failure, its reason on standard error.
    Fault,
    /// The command's own input refused before any dial: an argument, a
    /// file, a config.
    Usage,
    /// No TCP connection or no TLS handshake at the address and port.
    Unreachable,
    /// The console refused, with its reason.
    Refused,
    /// The pairing alone: no code entered within the window.
    Expired,
    /// The connection ended after the console may have acted.
    Unknown,
}

impl Status {
    /// The exit status.
    pub fn code(self) -> i32 {
        match self {
            Status::Done => 0,
            Status::Fault => 1,
            Status::Usage => 2,
            Status::Unreachable => 3,
            Status::Refused => 4,
            Status::Expired => 5,
            Status::Unknown => 6,
        }
    }

    /// The status's name, the `event` of a line that carries no other.
    pub fn name(self) -> &'static str {
        match self {
            Status::Done => "done",
            Status::Fault => "fault",
            Status::Usage => "usage",
            Status::Unreachable => "unreachable",
            Status::Refused => "refused",
            Status::Expired => "expired",
            Status::Unknown => "unknown",
        }
    }
}

/// No members beyond the event.
#[derive(Serialize)]
pub struct Nothing {}

/// A line: its event first, then its members.
#[derive(Serialize)]
struct Line<'a, T: Serialize> {
    event: &'a str,
    #[serde(flatten)]
    members: &'a T,
}

/// **A JSON line in printable ASCII**: serde escapes the control bytes
/// below 0x20, and this escapes DEL and every non-ASCII character as
/// `\uXXXX`, a pair of them past the basic plane, which JSON reads back
/// unchanged. Outside a string a JSON line holds neither.
pub fn ascii_json(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        if c.is_ascii() && c != '\u{7f}' {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}

/// **A reason for standard error in printable ASCII**: each byte outside
/// space through tilde, and the backslash itself, written as `\xNN`, so the
/// reason reads back exactly and moves no cursor.
pub fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if (0x20..0x7f).contains(&b) && b != b'\\' {
            out.push(b as char);
        } else {
            out.push_str(&format!("\\x{b:02x}"));
        }
    }
    out
}

/// **The printer**: standard output's lines and standard error's reasons,
/// for the command it names.
pub struct Printer<O: Write, E: Write> {
    command: &'static str,
    out: O,
    err: E,
}

impl<O: Write, E: Write> Printer<O, E> {
    pub fn new(command: &'static str, out: O, err: E) -> Self {
        Printer { command, out, err }
    }

    /// One line on standard output. A line that cannot be written is the
    /// terminal's gone, and nothing more can be said to it.
    pub fn line<T: Serialize>(&mut self, event: &str, members: &T) {
        let Ok(line) = serde_json::to_string(&Line { event, members }) else {
            self.reason("a line could not be rendered");
            return;
        };
        let _ = writeln!(self.out, "{}", ascii_json(&line));
        let _ = self.out.flush();
    }

    /// The outcome: its line, and the status the command exits with.
    pub fn outcome<T: Serialize>(&mut self, status: Status, event: &str, members: &T) -> Status {
        self.line(event, members);
        status
    }

    /// One reason on standard error, prefixed with the command, escaped.
    pub fn reason(&mut self, why: &str) {
        let _ = writeln!(self.err, "{}: {}", self.command, escaped(why));
        let _ = self.err.flush();
    }

    /// **An end the status names alone**: the reason on standard error,
    /// and a line whose event is the status's name.
    pub fn ended(&mut self, status: Status, why: &str) -> Status {
        self.reason(why);
        self.outcome(status, status.name(), &Nothing {})
    }

    /// The streams, for a test to read.
    #[cfg(test)]
    pub fn into_inner(self) -> (O, E) {
        (self.out, self.err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Every byte the printer writes is printable ASCII or a newline**,
    /// and what it escaped reads back: the JSON line's string unchanged,
    /// and the reason byte for byte.
    #[test]
    fn the_printer_writes_printable_ascii_alone() {
        let hostile = "a\u{1b}[2Jb\u{7f}c\u{e9}d\u{1f600}e\\f\u{9b}";
        let mut p = Printer::new("test", Vec::new(), Vec::new());
        #[derive(Serialize)]
        struct Reason<'a> {
            reason: &'a str,
        }
        p.line("refused", &Reason { reason: hostile });
        p.reason(hostile);
        let (out, err) = p.into_inner();
        for b in out.iter().chain(err.iter()) {
            assert!(
                (0x20..0x7f).contains(b) || *b == b'\n',
                "byte {b:#04x} reached the terminal"
            );
        }
        let line: serde_json::Value =
            serde_json::from_slice(out.strip_suffix(b"\n").unwrap()).unwrap();
        assert_eq!(line["event"], "refused");
        assert_eq!(line["reason"], hostile);
        let err = String::from_utf8(err).unwrap();
        assert!(
            err.starts_with("test: a\\x1b[2Jb\\x7fc\\xc3\\xa9d"),
            "{err}"
        );
        assert!(err.contains("e\\x5cf"), "{err}");
    }

    #[test]
    fn each_status_has_its_code() {
        let codes: Vec<i32> = [
            Status::Done,
            Status::Fault,
            Status::Usage,
            Status::Unreachable,
            Status::Refused,
            Status::Expired,
            Status::Unknown,
        ]
        .iter()
        .map(|s| s.code())
        .collect();
        assert_eq!(codes, [0, 1, 2, 3, 4, 5, 6]);
    }
}
