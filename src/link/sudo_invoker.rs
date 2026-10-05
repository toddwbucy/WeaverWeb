//! conforms: web-no-privilege-outside-the-sudo-invoker
//! conforms: web-ceiling-is-what-the-sudo-rules-grant
//!
//! **admin-con's sudo invoker, the one privileged invocation in this
//! crate** (Spec 7.2, 8). The box installs a strict rule per agent granting
//! admin-con's own service user exactly the fixed `weaver-admin <verb>
//! <agent>` command lines for that one agent, per
//! `weaver-admin-operator-contract` sections 1 and 2; this module is how
//! admin-con runs them, and `tests/no_privilege.rs` pins that nothing else
//! in the repository invokes privilege.
//!
//! **Nothing that crossed the link reaches the command.** The server sends
//! an abstract verb; the command's argv is built from constants (the
//! program, the non-interactive flag, the verb from a fixed table) and two
//! config values (the absolute path of `weaver-admin` the rule names, and
//! the agent's configured name). The principal a verb was asked for is
//! admin-con's to log and never passes to the box, whose record carries the
//! uid sudo reports. Standard input is closed, the environment is
//! admin-con's own, and the child runs in its own session, so admin-con's
//! own signals never reach it.
//!
//! **The child is never killed.** An invocation finishes even when its
//! caller disappears, per the contract's section 3: admin-con answers
//! `unknown` at its bound and reads the real state from the next `show`,
//! while the child runs on and is reaped when it exits. The bound lives in
//! admin-con's relay, around [`SudoInvoker::run`]'s future, which a
//! detached task owns to completion.

use crate::link::admin_con::Invoker;
use crate::link::frames::{Principal, VerbFault, VerbOutcome};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

/// The verbs a rule may grant, and so the only verbs this invoker builds a
/// line for: the fixed table argv's verb member comes from (Spec 7.2).
pub const VERBS: [&str; 5] = ["show", "validate", "load", "unload", "stop"];

/// The most an answer may be on standard output, per the contract's
/// section 3; one byte more is a fault.
pub const ANSWER_BOUND: usize = 64 * 1024;

/// The most of standard error kept for the log: diagnostics no caller
/// parses, bounded so a chatty child cannot grow admin-con.
const STDERR_KEPT: usize = 16 * 1024;

/// The program sudo is found as on admin-con's own `PATH`.
const SUDO: &str = "sudo";
/// Never prompt: a line the rule does not grant without a password fails
/// rather than waiting on a terminal admin-con does not have.
const NON_INTERACTIVE: &str = "-n";
/// List instead of run: whether the exact line is granted.
const LIST: &str = "-l";

/// The sudo invoker for one agent.
#[derive(Debug, Clone)]
pub struct SudoInvoker {
    weaver_admin: PathBuf,
    agent: String,
    /// **One listing at a time**, held by the task that owns the listing's
    /// child until it exits: a `grants` cancelled at its bound while a
    /// listing hangs leaves that one listing running, and the next `grants`
    /// waits on it rather than starting another, so reconnection attempts
    /// cannot pile up listings.
    listing: Arc<tokio::sync::Mutex<()>>,
    /// A test's `PATH` for the child, so the generated fake sudo is found
    /// first; a test build runs nothing without it. Absent from a service
    /// build, where the child inherits admin-con's.
    #[cfg(test)]
    pub(super) path: Option<OsString>,
}

impl SudoInvoker {
    /// An invoker for the agent named `agent`, running the `weaver-admin`
    /// at `weaver_admin`, which must be absolute, as the rule names it.
    pub fn new(weaver_admin: &Path, agent: &str) -> anyhow::Result<Self> {
        if !weaver_admin.is_absolute() {
            anyhow::bail!(
                "weaver_admin {} is not an absolute path; the box's sudo rule names it absolutely",
                weaver_admin.display()
            );
        }
        Ok(Self {
            weaver_admin: weaver_admin.to_owned(),
            agent: agent.to_owned(),
            listing: Arc::new(tokio::sync::Mutex::new(())),
            #[cfg(test)]
            path: None,
        })
    }

    /// **The command line, from constants and config alone**: the
    /// non-interactive flag, `weaver-admin`'s path, the verb from the fixed
    /// table, and the agent's name, with `-l` where the line is listed and
    /// not run. `None` for a verb outside the table, which no rule grants.
    pub fn argv(&self, verb: &str, list: bool) -> Option<Vec<OsString>> {
        let verb = VERBS.iter().find(|v| **v == verb)?;
        let mut argv: Vec<OsString> = vec![NON_INTERACTIVE.into()];
        if list {
            argv.push(LIST.into());
        }
        argv.push(self.weaver_admin.clone().into_os_string());
        argv.push((*verb).into());
        argv.push(self.agent.clone().into());
        Some(argv)
    }

    fn command(&self, argv: &[OsString]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(SUDO);
        command
            .args(argv)
            .stdin(Stdio::null())
            // The child is never killed: dropping its handle leaves it
            // running, and the task that owns it reaps it.
            .kill_on_drop(false);
        // **A test never runs the real program**: a test build refuses an
        // invoker without the generated fake's `PATH`.
        #[cfg(test)]
        command.env(
            "PATH",
            self.path
                .as_ref()
                .expect("a test's invoker runs the generated fake sudo"),
        );
        // SAFETY: `setsid` is async-signal-safe and touches nothing of the
        // parent's; it only moves the child into a session of its own, so
        // a signal sent to admin-con's process group never reaches it.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command
    }

    /// Whether the rule grants this exact line, asking without running it.
    /// **The listing runs in a task that owns its child to the end** and
    /// reaps it, holding the listing lock until then, so a caller dropped
    /// at its bound leaves a reaper behind and never a second listing.
    async fn granted(&self, verb: &str) -> anyhow::Result<bool> {
        let Some(argv) = self.argv(verb, true) else {
            return Ok(false);
        };
        let mut command = self.command(&argv);
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let held = self.listing.clone().lock_owned().await;
        let listing = tokio::spawn(async move {
            let status = command.status().await;
            drop(held);
            status
        });
        Ok(listing.await??.success())
    }
}

/// Read a stream to its end, keeping at most `keep` bytes and counting all,
/// so a child writing more than the bound is drained and can exit rather
/// than block on a full pipe while it holds the slot.
async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(
    mut stream: R,
    keep: usize,
) -> std::io::Result<(Vec<u8>, usize)> {
    let mut kept = Vec::new();
    let mut total = 0usize;
    let mut buf = vec![0u8; 8192];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok((kept, total));
        }
        total += n;
        let room = keep.saturating_sub(kept.len());
        kept.extend_from_slice(&buf[..n.min(room)]);
    }
}

/// One JSON object from standard output, or `None` where it is anything
/// else: not UTF-8, empty, more than one value, or not an object.
fn one_object(stdout: &[u8]) -> Option<serde_json::Value> {
    let text = std::str::from_utf8(stdout).ok()?;
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    value.is_object().then_some(value)
}

impl Invoker for SudoInvoker {
    /// **The ceiling is exactly the lines the box's rules grant**, each
    /// asked by `sudo -n -l` on the exact line, an exit of 0 granting it
    /// (Spec 7.2, 8). The listing needs no password only while every sudo
    /// entry admin-con's user holds is `NOPASSWD`, which the box's rule
    /// guarantees and the install must not widen; a listing that would
    /// prompt fails under `-n` and the line reads as not granted.
    async fn grants(&self) -> anyhow::Result<Vec<String>> {
        let mut granted = Vec::new();
        for verb in VERBS {
            if self.granted(verb).await? {
                granted.push(verb.to_owned());
            }
        }
        Ok(granted)
    }

    /// **Run one granted line to its end**: one JSON object of at most
    /// `ANSWER_BOUND` on standard output with exit 0 is an answer, with
    /// exit 1 a refusal, and anything else, or no object, is a fault whose
    /// message names the status, the caller reading the next `show`.
    /// Standard error is logged and never parsed. The principal is logged
    /// and never reaches the command.
    async fn run(
        &self,
        agent: &str,
        verb: &str,
        principal: &Principal,
    ) -> Result<VerbOutcome, VerbFault> {
        let fault = |message: String| VerbFault {
            kind: VerbFault::FAULT.into(),
            message,
        };
        tracing::info!("{agent}: invoking {verb}, asked for {principal:?}");
        let Some(argv) = self.argv(verb, false) else {
            return Err(fault(format!("{verb} is not a verb a rule grants")));
        };
        let mut child = self
            .command(&argv)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| fault(format!("the invocation of {verb} did not start: {e}")))?;
        let stdout = child.stdout.take().expect("piped");
        let stderr = child.stderr.take().expect("piped");
        let (out, err) = tokio::join!(
            read_bounded(stdout, ANSWER_BOUND + 1),
            read_bounded(stderr, STDERR_KEPT)
        );
        // Reaped here, whatever the streams did: a child is never left a
        // zombie and never killed.
        let status = child
            .wait()
            .await
            .map_err(|e| fault(format!("{verb}'s invocation could not be waited for: {e}")))?;
        let stderr = err
            .ok()
            .map(|(bytes, _)| String::from_utf8_lossy(&bytes).into_owned())
            .filter(|s| !s.is_empty());
        if let Some(stderr) = &stderr {
            tracing::info!("{agent}: {verb}'s diagnostics: {stderr}");
        }
        let (out, total) =
            out.map_err(|e| fault(format!("{verb}'s answer could not be read: {e}")))?;
        if total > ANSWER_BOUND {
            return Err(fault(format!(
                "{verb} wrote {total} bytes on standard output, past the {ANSWER_BOUND} byte bound, exit {status}; read the next show"
            )));
        }
        let code = status.code();
        match (code, one_object(&out)) {
            (Some(0 | 1), Some(object)) => Ok(VerbOutcome {
                verb: verb.to_owned(),
                agent: agent.to_owned(),
                exit_code: code,
                answer: Some(object),
                raw_stdout: None,
                stderr,
            }),
            (_, None) => Err(fault(format!(
                "{verb} exited {status} with no answer object; read the next show"
            ))),
            (_, Some(_)) => Err(fault(format!(
                "{verb} exited {status}, neither an answer nor a refusal; read the next show"
            ))),
        }
    }
}
