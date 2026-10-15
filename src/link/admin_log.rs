//! conforms: web-an-alert-only-for-what-did-not-come-through-weaverweb
//!
//! **admin-con reads the box's `admin.log`** (Spec 2.14, 8; act 12, PR 2):
//! each line admin appends after admin-con starts becomes one entry, passed
//! along as admin wrote it, sent upstream through [`Entries`].
//!
//! **One field is judged, and nothing else**: `own`, true where the line's
//! caller uid is admin-con's own. admin's lines name the caller as `uid`
//! (the uid sudo reported, `weaver-admin-Spec` section 8 and its `log.rs`),
//! and the relay's lines name their peer as `peer_uid`; whichever the line
//! carries is compared with admin-con's real uid, which is the uid its
//! `sudo -n` reports. A line carrying neither is not own. The server raises
//! an alert only for a line that is not own, an act on the box that did not
//! come through WeaverWeb, since its own acts raise theirs at the door.
//!
//! **The file is a box fact, read with care**: opened read-only without
//! following a link and without blocking, so a link or a FIFO planted at
//! the path is refused rather than read, and judged a regular file by its
//! descriptor. **No history at start**: reading begins at the end of the
//! file as admin-con first finds it; the backlog of [`Entries`] covers an
//! outage of the link, and the box's file keeps the rest. **A truncation or
//! a replacement** (the size below what was read, or another file at the
//! path) is read from its start, after an entry marking the discontinuity.
//! A line past [`LINE_BOUND`] is replaced by an entry saying so.
//!
//! **It runs on a thread of its own**, polling every [`POLL`]: a read of a
//! box file never runs on the connector's runtime, so reading the trace and
//! answering verbs never wait on it, and it never waits on an entry's send.

use std::fs::File;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;
use tokio::sync::watch;

use crate::link::entries::Entries;

/// The entries' source for a line of `admin.log`.
pub const SOURCE: &str = "admin_log";
/// **The bound on one line**: a longer one is replaced by an entry saying
/// so, and reading resumes at the next line.
pub const LINE_BOUND: usize = 8 * 1024;
/// How often the file is looked at.
pub const POLL: Duration = Duration::from_millis(500);
/// **The span of the last bytes read whose digest each look checks.**
pub const CHECK_SPAN: u64 = 4096;

/// The digest of the up to [`CHECK_SPAN`] bytes ending at `offset`; none at
/// offset zero, and a mark no digest matches where they cannot all be read.
fn span_digest(file: &File, offset: u64) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    if offset == 0 {
        return None;
    }
    let span = offset.min(CHECK_SPAN);
    let mut buf = vec![0u8; span as usize];
    match file.read_at(&mut buf, offset - span) {
        Ok(read) if read as u64 == span => Some(Sha256::digest(&buf).into()),
        _ => Some([0xff; 32]),
    }
}

/// The most read in one look, so a large append is taken over several.
const READ_BOUND: u64 = 1024 * 1024;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// **Whether a line is admin-con's own**: its caller uid, `uid` for an
/// invocation's line and `peer_uid` for the relay's, against `me`. The one
/// thing admin-con judges.
pub fn own(line: &serde_json::Value, me: u32) -> bool {
    line.get("uid")
        .or_else(|| line.get("peer_uid"))
        .and_then(|u| u.as_u64())
        .is_some_and(|u| u == u64::from(me))
}

/// The entry for one line, as admin wrote it, beside admin-con's own
/// fields.
pub fn entry_of(raw: &[u8], me: u32) -> serde_json::Value {
    match serde_json::from_slice::<serde_json::Value>(raw) {
        Ok(line) if line.is_object() => {
            let own = own(&line, me);
            json!({ "line": line, "own": own, "read_at": now_ms() })
        }
        _ => json!({
            "line": String::from_utf8_lossy(raw),
            "unparsed": true,
            "own": false,
            "read_at": now_ms(),
        }),
    }
}

/// A discontinuity, as an entry.
fn discontinuity(why: &str) -> serde_json::Value {
    json!({ "discontinuity": why, "read_at": now_ms() })
}

struct Tail {
    path: PathBuf,
    entries: Entries,
    me: u32,
    file: Option<(File, (u64, u64))>,
    offset: u64,
    partial: Vec<u8>,
    /// Inside a line past the bound, its entry already sent.
    skipping: bool,
    /// **The start's baseline is still to be taken**: the open made before
    /// the thread's first look starts at the file's end; any later first
    /// open (the file absent or unreadable at the start) reads it from its
    /// start, nothing of it having been read.
    at_start: bool,
    /// Whether a file was ever opened: another opened after it is a
    /// replacement, marked.
    had_file: bool,
    /// **The digest of the last bytes read**, up to [`CHECK_SPAN`] of them
    /// ending at `offset`, checked at each look: a copy-truncate that
    /// regrew past the offset between two looks changes them.
    seen: Option<[u8; 32]>,
    /// The failure last reported, so a failure is an entry once, at its
    /// start, and not at every look.
    failing: Option<String>,
}

impl Tail {
    fn fail(&mut self, why: String) {
        if self.failing.as_deref() != Some(why.as_str()) {
            tracing::warn!("admin_log {}: {why}", self.path.display());
            self.entries.failure(
                "admin_log_unreadable",
                format!("the box's admin.log cannot be read: {why}"),
            );
            self.failing = Some(why);
        }
    }

    fn open(&mut self) {
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(&self.path)
        {
            Ok(file) => file,
            Err(e) => {
                let why = match e.raw_os_error() {
                    Some(libc::ELOOP) => {
                        "it is a symlink, and admin.log is read through none".to_owned()
                    }
                    _ => format!("opening it: {e}"),
                };
                return self.fail(why);
            }
        };
        let meta = match file.metadata() {
            Ok(meta) => meta,
            Err(e) => return self.fail(format!("reading its metadata: {e}")),
        };
        if !meta.file_type().is_file() {
            return self.fail("it is not a regular file".to_owned());
        }
        self.failing = None;
        if self.had_file {
            self.entries.push(
                SOURCE,
                discontinuity("admin.log was replaced; read from its start"),
            );
        }
        self.offset = if self.at_start { meta.len() } else { 0 };
        self.had_file = true;
        self.partial.clear();
        self.skipping = false;
        self.seen = span_digest(&file, self.offset);
        self.file = Some((file, (meta.dev(), meta.ino())));
    }

    /// One look: the file opened where it is not, replaced or truncated
    /// where it was, and what was appended read.
    fn look(&mut self) {
        if let Some((_, identity)) = &self.file {
            let replaced = match std::fs::symlink_metadata(&self.path) {
                Ok(meta) => (meta.dev(), meta.ino()) != *identity,
                Err(_) => true,
            };
            if replaced {
                self.file = None;
            }
        }
        if self.file.is_none() {
            self.open();
        }
        let Some((file, _)) = &self.file else {
            return;
        };
        let len = match file.metadata() {
            Ok(meta) => meta.len(),
            Err(e) => {
                self.file = None;
                return self.fail(format!("reading its metadata: {e}"));
            }
        };
        // **What was read is still there**, by its size and by the digest
        // of its last bytes: a file shrunk, or truncated and regrown past
        // the offset between two looks, is read from its start after a
        // discontinuity, never resumed mid-line.
        if len < self.offset || span_digest(file, self.offset) != self.seen {
            self.entries.push(
                SOURCE,
                discontinuity("admin.log was truncated; read from its start"),
            );
            self.offset = 0;
            self.seen = None;
            self.partial.clear();
            self.skipping = false;
        }
        let want = (len - self.offset).min(READ_BOUND) as usize;
        if want == 0 {
            return;
        }
        let mut buf = vec![0u8; want];
        let read = match file.read_at(&mut buf, self.offset) {
            Ok(read) => read,
            Err(e) => {
                self.file = None;
                return self.fail(format!("reading it: {e}"));
            }
        };
        buf.truncate(read);
        self.offset += read as u64;
        self.seen = span_digest(file, self.offset);
        for byte in buf {
            if byte == b'\n' {
                if !self.skipping && !self.partial.is_empty() {
                    let line = std::mem::take(&mut self.partial);
                    self.entries.push(SOURCE, entry_of(&line, self.me));
                }
                self.partial.clear();
                self.skipping = false;
            } else if !self.skipping {
                self.partial.push(byte);
                if self.partial.len() > LINE_BOUND {
                    self.partial.clear();
                    self.skipping = true;
                    self.entries.push(
                        SOURCE,
                        json!({
                            "oversized": format!("a line of admin.log past the bound of {LINE_BOUND} bytes was not sent"),
                            "own": false,
                            "read_at": now_ms(),
                        }),
                    );
                }
            }
        }
    }
}

/// **Read `admin.log` until `shutdown`**, on a thread of its own, each line
/// pushed to `entries`. `me` is the uid a line's caller is compared with:
/// admin-con's real uid in service, a test's own in tests.
pub fn tail(
    path: PathBuf,
    entries: Entries,
    me: u32,
    shutdown: watch::Receiver<bool>,
) -> std::thread::JoinHandle<()> {
    // **The baseline, taken before the thread's first look**: the file as
    // it stands at admin-con's start is history and is not read; one that
    // cannot be opened now is read from its start once it can.
    let mut tail = Tail {
        path,
        entries,
        me,
        file: None,
        offset: 0,
        partial: Vec::new(),
        skipping: false,
        at_start: true,
        had_file: false,
        seen: None,
        failing: None,
    };
    tail.open();
    tail.at_start = false;
    std::thread::spawn(move || {
        let mut tail = tail;
        // Ends at the stop, and where the stop's sender is gone with the
        // connector that held it, so a dropped connector leaves no thread.
        while shutdown.has_changed().is_ok() && !*shutdown.borrow() {
            tail.look();
            std::thread::sleep(POLL);
        }
    })
}

/// admin-con's real uid, the uid its `sudo -n` reports to admin.
pub fn my_uid() -> u32 {
    // SAFETY: getuid takes nothing and cannot fail.
    unsafe { libc::getuid() }
}
