//! Act 9a: the sudo invoker against a fake `sudo` generated at test time in
//! a temporary directory and placed first on the child's `PATH`, never
//! tracked, so the repository's privilege scan never meets it. A test build
//! of the invoker refuses to run without that `PATH`, and every test checks
//! the fake's own record that each line ran through it: no real program is
//! reached and no agent is either.

use super::admin_con::Invoker;
use super::admin_con_tests::{
    GRACE, POLL, Running, Trace, config, installed, restart_on_its_address, verb,
};
use super::frames::{Principal, VerbFault};
use super::listener::VerbError;
use super::sudo_invoker::{ANSWER_BOUND, SudoInvoker, VERBS};
use super::tests::{Lab, SOON};
use serde_json::json;
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const AGENT: &str = "karl";

/// The name a person's ask carries, planted where the argv instrument looks
/// for it: a string that crossed the link in a `ToClient::Verb` frame.
const PLANTED: &str = "planted-principal-name";

/// The fake's script: it records its argv (with `$0`, the file the kernel
/// ran), its session and its standard input, answers `-l` from the grants
/// file after the delay named `delay.list`, and otherwise runs a verb as scripted by files named after it: a
/// delay, standard error, standard output and an exit status. A second run
/// starting while one runs is recorded as an overlap, by an atomic `mkdir`,
/// and an answer whose write fails, a pipe closed before it was read, as
/// broken.
fn script(dir: &Path) -> String {
    format!(
        r#"#!/bin/sh
dir='{dir}'
line="argv"
for a in "$0" "$@"; do line="$line	$a"; done
printf '%s\n' "$line" >> "$dir/log"
read -r _pid _comm _state _ppid _pgrp session _rest < /proc/$$/stat
printf 'context\t%s\t%s\t%s\n' "$$" "$session" "$(readlink /proc/$$/fd/0)" >> "$dir/log"
if [ "$2" = "-l" ]; then
  if [ -f "$dir/delay.list" ]; then sleep "$(cat "$dir/delay.list")"; fi
  grep -qx -- "$4" "$dir/grants" 2>/dev/null
  exit $?
fi
verb="$3"
mkdir "$dir/running" 2>/dev/null || printf 'overlap\t%s\n' "$verb" >> "$dir/log"
printf 'start\t%s\n' "$verb" >> "$dir/log"
if [ -f "$dir/delay.$verb" ]; then sleep "$(cat "$dir/delay.$verb")"; fi
if [ -f "$dir/stderr.$verb" ]; then cat "$dir/stderr.$verb" >&2; fi
if [ -f "$dir/answer.$verb" ]; then
  cat "$dir/answer.$verb" || printf 'broken\t%s\n' "$verb" >> "$dir/log"
fi
printf 'end\t%s\n' "$verb" >> "$dir/log"
rmdir "$dir/running" 2>/dev/null
code=0
if [ -f "$dir/exit.$verb" ]; then code=$(cat "$dir/exit.$verb"); fi
exit "$code"
"#,
        dir = dir.display()
    )
}

/// The generated fake and its record.
struct FakeSudo {
    dir: tempfile::TempDir,
    weaver_admin: PathBuf,
}

impl FakeSudo {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let program = dir.path().join("sudo");
        std::fs::write(&program, script(dir.path())).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.path().join("log"), "").unwrap();
        // Absolute, as the config requires, and never run: the fake stands
        // where the rule's program would be reached.
        let weaver_admin = dir.path().join("weaver-admin");
        Self { dir, weaver_admin }
    }

    fn program(&self) -> PathBuf {
        self.dir.path().join("sudo")
    }

    /// The child's `PATH`: the fake's directory first, then the test's own.
    fn path(&self) -> OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(self.dir.path().to_owned()).chain(std::env::split_paths(&inherited)),
        )
        .unwrap();
        assert_eq!(
            std::env::split_paths(&path).next().as_deref(),
            Some(self.dir.path()),
            "the fake's directory is first on the child's PATH"
        );
        path
    }

    fn invoker(&self) -> SudoInvoker {
        let mut invoker = SudoInvoker::new(&self.weaver_admin, AGENT).unwrap();
        invoker.path = Some(self.path());
        invoker
    }

    fn file(&self, name: &str, content: impl AsRef<[u8]>) {
        std::fs::write(self.dir.path().join(name), content).unwrap();
    }

    fn grant(&self, verbs: &[&str]) {
        self.file(
            "grants",
            verbs.iter().map(|v| format!("{v}\n")).collect::<String>(),
        );
    }

    fn answer(&self, verb: &str, answer: impl AsRef<[u8]>) {
        self.file(&format!("answer.{verb}"), answer);
    }

    fn exit(&self, verb: &str, code: i32) {
        self.file(&format!("exit.{verb}"), code.to_string());
    }

    fn delay(&self, verb: &str, delay: Duration) {
        self.file(
            &format!("delay.{verb}"),
            format!("{:.3}", delay.as_secs_f64()),
        );
    }

    /// The record, one entry per line, its fields split.
    fn log(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.dir.path().join("log"))
            .unwrap()
            .lines()
            .map(|l| l.split('\t').map(str::to_owned).collect())
            .collect()
    }

    /// Every argv the fake was run with, `$0` first.
    fn argvs(&self) -> Vec<Vec<String>> {
        self.log()
            .into_iter()
            .filter(|l| l[0] == "argv")
            .map(|l| l[1..].to_vec())
            .collect()
    }

    /// How many listings (`-n -l` lines) the fake was run with.
    fn listings(&self) -> usize {
        self.argvs()
            .iter()
            .filter(|a| a.get(2).map(String::as_str) == Some("-l"))
            .count()
    }

    /// The verbs run (not listed), in the order their runs started and
    /// ended, with any overlap or broken answer: `start show`, `end show`,
    /// ...
    fn runs(&self) -> Vec<String> {
        self.log()
            .into_iter()
            .filter(|l| ["start", "end", "overlap", "broken"].contains(&l[0].as_str()))
            .map(|l| l.join(" "))
            .collect()
    }

    /// The run line the rule names, as the fake was reached with it.
    fn line(&self, verb: &str, list: bool) -> Vec<String> {
        let mut line = vec![self.program().display().to_string(), "-n".to_owned()];
        if list {
            line.push("-l".to_owned());
        }
        line.extend([
            self.weaver_admin.display().to_string(),
            verb.to_owned(),
            AGENT.to_owned(),
        ]);
        line
    }

    /// **Every line ran through the fake**: each argv's `$0` is the fake's
    /// file, so the real program was never reached; and each child ran in
    /// a session of its own with standard input closed to `/dev/null`.
    fn assert_every_line_ran_the_fake(&self) {
        let log = self.log();
        let argvs = self.argvs();
        assert!(!argvs.is_empty(), "the fake ran: {log:?}");
        for argv in &argvs {
            assert_eq!(Path::new(&argv[0]), self.program(), "{argv:?}");
        }
        let contexts: Vec<&Vec<String>> = log.iter().filter(|l| l[0] == "context").collect();
        assert_eq!(contexts.len(), argvs.len(), "{log:?}");
        for context in contexts {
            assert_eq!(
                context[1], context[2],
                "the child leads its own session: {context:?}"
            );
            assert_eq!(
                context[3], "/dev/null",
                "standard input is null: {context:?}"
            );
        }
    }

    /// Wait until the record holds `entry` among its runs.
    async fn wait_run(&self, entry: &str) {
        let until = tokio::time::Instant::now() + SOON;
        while !self.runs().iter().any(|r| r == entry) {
            assert!(
                tokio::time::Instant::now() < until,
                "the fake never recorded {entry}: {:?}",
                self.runs()
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

fn state(state: &str) -> String {
    json!({"kind": "state", "state": state}).to_string()
}

/// A `show` answer of exactly `bytes` bytes.
fn answer_of(bytes: usize) -> String {
    let empty = json!({"kind": "state", "state": "unloaded", "pad": ""}).to_string();
    let answer =
        json!({"kind": "state", "state": "unloaded", "pad": "x".repeat(bytes - empty.len())})
            .to_string();
    assert_eq!(answer.len(), bytes);
    answer
}

fn person() -> Principal {
    Principal::Person {
        name: PLANTED.to_owned(),
    }
}

/// **The ceiling is exactly the lines the rules grant**: each of the five
/// verbs listed with `-n -l` on its exact line, a granted one answering 0.
#[tokio::test]
async fn the_grants_are_exactly_the_lines_the_rules_grant() {
    let fake = FakeSudo::new();
    fake.grant(&["show", "load"]);
    let granted = fake.invoker().grants().await.unwrap();
    assert_eq!(granted, ["show", "load"]);
    let want: Vec<Vec<String>> = VERBS.iter().map(|v| fake.line(v, true)).collect();
    assert_eq!(fake.argvs(), want);
    assert!(
        fake.runs().is_empty(),
        "listing runs nothing: {:?}",
        fake.runs()
    );
    fake.assert_every_line_ran_the_fake();

    // A box that grants nothing declares the empty ceiling.
    let fake = FakeSudo::new();
    assert!(fake.invoker().grants().await.unwrap().is_empty());
}

/// **A listing that hangs is never doubled**: two `grants` asked at once
/// and both dropped at their bound start one listing, not two, and that
/// listing is owned to its end, so the next `grants` runs once it exits.
#[tokio::test]
async fn a_hung_listing_is_never_doubled() {
    let fake = FakeSudo::new();
    fake.grant(&["show"]);
    fake.delay("list", Duration::from_millis(1500));
    let invoker = fake.invoker();
    let bound = Duration::from_millis(400);
    let asked = tokio::time::Instant::now();
    let (first, second) = tokio::join!(
        tokio::time::timeout(bound, invoker.grants()),
        tokio::time::timeout(bound, invoker.grants()),
    );
    assert!(
        first.is_err() && second.is_err(),
        "both asks passed their bound"
    );
    assert_eq!(fake.listings(), 1, "{:?}", fake.argvs());

    std::fs::remove_file(fake.dir.path().join("delay.list")).unwrap();
    let granted = tokio::time::timeout(SOON, invoker.grants())
        .await
        .expect("the hung listing ended and released the next");
    assert_eq!(granted.unwrap(), ["show"]);
    assert!(
        asked.elapsed() >= Duration::from_millis(1400),
        "the next grants waited for the hung listing's child to exit: {:?}",
        asked.elapsed()
    );
    fake.assert_every_line_ran_the_fake();
}

/// **The command line is constants and config alone**, the argv
/// instrument: for every verb the line is exactly `sudo -n <weaver_admin>
/// <verb> <agent>`, and a run asked for a person carries nothing of the
/// frame that asked it. A verb outside the table builds no line and runs
/// nothing.
#[tokio::test]
async fn the_command_line_is_constants_and_config_alone() {
    let fake = FakeSudo::new();
    let invoker = fake.invoker();
    for verb in VERBS {
        fake.answer(verb, state("idle"));
        invoker.run(AGENT, verb, &person()).await.unwrap();
    }
    let want: Vec<Vec<String>> = VERBS.iter().map(|v| fake.line(v, false)).collect();
    let argvs = fake.argvs();
    assert_eq!(argvs, want);
    assert!(
        argvs.iter().flatten().all(|arg| !arg.contains(PLANTED)),
        "the principal reached the command: {argvs:?}"
    );
    fake.assert_every_line_ran_the_fake();

    for outside in ["show; true", "rm", ""] {
        assert!(invoker.argv(outside, false).is_none());
        match invoker.run(AGENT, outside, &person()).await {
            Err(fault) => assert_eq!(fault.kind, VerbFault::FAULT),
            Ok(outcome) => panic!("{outside:?} ran: {outcome:?}"),
        }
    }
    assert_eq!(
        fake.argvs().len(),
        VERBS.len(),
        "a verb outside the table ran"
    );
}

/// **A config naming `weaver-admin` by a relative path is refused**: the
/// rule names it absolutely, and a relative one would be found by `PATH`.
#[test]
fn a_relative_weaver_admin_is_refused() {
    for relative in ["weaver-admin", "bin/weaver-admin", ""] {
        assert!(
            SudoInvoker::new(Path::new(relative), AGENT).is_err(),
            "{relative:?}"
        );
    }
}

/// **Exit 0 with one object is an answer**, its standard error kept for
/// the log and never parsed.
#[tokio::test]
async fn a_run_that_answers_is_its_object() {
    let fake = FakeSudo::new();
    fake.answer("show", state("idle"));
    fake.file("stderr.show", "a diagnostic\n");
    let outcome = fake.invoker().run(AGENT, "show", &person()).await.unwrap();
    assert_eq!(outcome.exit_code, Some(0));
    assert_eq!(
        outcome.answer,
        Some(json!({"kind": "state", "state": "idle"}))
    );
    assert_eq!(outcome.stderr.as_deref(), Some("a diagnostic\n"));
    assert_eq!(fake.runs(), ["start show", "end show"]);
    fake.assert_every_line_ran_the_fake();
}

/// **Exit 1 with one object is a refusal**: an outcome, not a fault.
#[tokio::test]
async fn an_exit_of_one_with_an_object_is_a_refusal() {
    let fake = FakeSudo::new();
    let refusal = json!({"kind": "refused", "reason": "already loaded"});
    fake.answer("load", refusal.to_string());
    fake.exit("load", 1);
    let outcome = fake.invoker().run(AGENT, "load", &person()).await.unwrap();
    assert_eq!(outcome.exit_code, Some(1));
    assert_eq!(outcome.answer, Some(refusal));
    fake.assert_every_line_ran_the_fake();
}

/// **Any other status, or no object, is a fault naming the status**, of
/// the kind that says to read the next `show`.
#[tokio::test]
async fn any_other_status_or_no_object_is_a_fault() {
    let fake = FakeSudo::new();
    let invoker = fake.invoker();
    fake.answer("validate", state("idle"));
    fake.exit("validate", 2);
    fake.answer("show", "not an object\n");
    fake.answer("load", "[1, 2]\n");
    fake.answer("unload", format!("{}\n{}\n", state("idle"), state("idle")));
    fake.exit("stop", 1);
    for (verb, status) in [
        ("validate", "exit status: 2"),
        ("show", "exit status: 0"),
        ("load", "exit status: 0"),
        ("unload", "exit status: 0"),
        ("stop", "exit status: 1"),
    ] {
        match invoker.run(AGENT, verb, &person()).await {
            Err(fault) => {
                assert_eq!(fault.kind, VerbFault::FAULT, "{verb}");
                assert!(fault.message.contains(status), "{verb}: {}", fault.message);
                assert!(
                    fault.message.contains("read the next show"),
                    "{}",
                    fault.message
                );
            }
            Ok(outcome) => panic!("{verb} answered: {outcome:?}"),
        }
    }
    fake.assert_every_line_ran_the_fake();
}

/// **An answer past 64 KiB is a fault**, and the child is drained to its
/// end rather than left blocked on a full pipe; one at the bound is an
/// answer.
#[tokio::test]
async fn an_answer_past_the_bound_is_a_fault() {
    let fake = FakeSudo::new();
    let invoker = fake.invoker();
    fake.answer("show", answer_of(ANSWER_BOUND));
    assert!(invoker.run(AGENT, "show", &person()).await.is_ok());
    fake.answer("show", answer_of(ANSWER_BOUND + 1));
    match invoker.run(AGENT, "show", &person()).await {
        Err(fault) => {
            assert_eq!(fault.kind, VerbFault::FAULT);
            assert!(fault.message.contains("past the"), "{}", fault.message);
        }
        Ok(outcome) => panic!("an oversized answer was taken: {:?}", outcome.exit_code),
    }
    // Far past any pipe's buffer: still drained, still ended, where a
    // child left blocked on a full pipe would never be reaped.
    fake.answer("show", answer_of(16 * ANSWER_BOUND));
    let drained = tokio::time::timeout(SOON, invoker.run(AGENT, "show", &person()))
        .await
        .expect("the child was drained to its end and reaped");
    assert!(drained.is_err());
    assert_eq!(
        fake.runs(),
        [
            "start show",
            "end show",
            "start show",
            "end show",
            "start show",
            "end show"
        ],
        "every answer was read to its end, none cut by a closed pipe"
    );
    fake.assert_every_line_ran_the_fake();
}

/// **The child is never killed**: a run whose caller goes away mid-way
/// leaves its child running to its own end.
#[tokio::test]
async fn a_child_runs_on_when_its_caller_goes() {
    let fake = FakeSudo::new();
    fake.answer("load", state("idle"));
    fake.delay("load", Duration::from_millis(800));
    let invoker = fake.invoker();
    let gone = tokio::time::timeout(
        Duration::from_millis(200),
        invoker.run(AGENT, "load", &person()),
    )
    .await;
    assert!(gone.is_err(), "the caller left before the answer");
    fake.wait_run("end load").await;
    fake.assert_every_line_ran_the_fake();
}

/// admin-con run with the sudo invoker over the fake, for the slot and the
/// stop, its verb bound short so a run passes it.
struct Con {
    lab: Lab,
    fake: FakeSudo,
    id: crate::store::AgentId,
    path: PathBuf,
    _trace: Trace,
    _out: tempfile::TempDir,
}

const BOUND: Duration = Duration::from_millis(300);

impl Con {
    async fn open(grants: &[&str]) -> Option<Self> {
        let lab = Lab::open().await?;
        let fake = FakeSudo::new();
        fake.grant(grants);
        fake.answer("show", state("unloaded"));
        for verb in ["validate", "load", "unload"] {
            fake.answer(verb, json!({"kind": "done", "verb": verb}).to_string());
        }
        let trace = Trace::new();
        let out = tempfile::tempdir().unwrap();
        let (id, path) = installed(&lab, &trace.path, out.path(), None).await;
        Some(Self {
            lab,
            fake,
            id,
            path,
            _trace: trace,
            _out: out,
        })
    }

    async fn start(&self, grace: Duration) -> Running {
        let mut cfg = config(&self.path, POLL);
        cfg.verb_bound = BOUND;
        cfg.stop_grace = grace;
        let mut con = Running::start(cfg, std::sync::Arc::new(self.fake.invoker()));
        con.wait("admitted", |s| s.admitted).await;
        self.lab
            .wait_for(&self.id, "the admission's show", |a| {
                a.state_source.as_deref() == Some("show")
            })
            .await;
        con
    }

    /// Ask `verb`, past its bound: the answer is `unknown`, the child runs.
    async fn ask_past_the_bound(&self, verb: &str) {
        match super::admin_con_tests::verb(&self.lab.listener, &self.id, verb).await {
            Err(VerbError::Fault(fault)) => assert_eq!(fault.kind, VerbFault::UNKNOWN),
            other => panic!("expected the unknown fault, got {other:?}"),
        }
        assert!(
            !self.fake.runs().contains(&format!("end {verb}")),
            "{verb} ended inside its bound: {:?}",
            self.fake.runs()
        );
    }
}

/// **A timed-out run is reaped later, the slot held until then**: a verb
/// past its bound answers `unknown` and runs on; the next verb waits until
/// that child has exited and is reaped, and never runs beside it.
#[tokio::test]
async fn a_timed_out_run_is_reaped_later_with_the_slot_held() {
    let Some(con) = Con::open(&["show", "validate"]).await else {
        return;
    };
    con.fake.delay("validate", Duration::from_millis(1500));
    let running = con.start(GRACE).await;
    con.ask_past_the_bound("validate").await;
    let asked = tokio::time::Instant::now();
    let shown = verb(&con.lab.listener, &con.id, "show").await.unwrap();
    assert_eq!(shown.exit_code, Some(0));
    assert!(
        asked.elapsed() >= Duration::from_millis(900),
        "show waited for the detached child: {:?}",
        asked.elapsed()
    );
    assert_eq!(
        con.fake.runs(),
        [
            "start show",
            "end show",
            "start validate",
            "end validate",
            "start show",
            "end show"
        ]
    );
    con.fake.assert_every_line_ran_the_fake();
    running.stop().await;
}

/// **The slot is admin-con's own, held across a reconnection**: a verb
/// past its bound runs on while the server restarts, and the fresh
/// connection's admission `show` waits for that child and never runs
/// beside it.
#[tokio::test]
async fn the_slot_is_held_across_a_reconnection() {
    let Some(mut con) = Con::open(&["show", "validate"]).await else {
        return;
    };
    con.fake.delay("validate", Duration::from_millis(2000));
    let mut running = con.start(GRACE).await;
    con.ask_past_the_bound("validate").await;
    restart_on_its_address(&mut con.lab, || {}).await;
    running
        .wait("admitted again", |s| s.admitted && s.admissions >= 2)
        .await;
    con.fake.wait_run("end validate").await;
    let until = tokio::time::Instant::now() + SOON;
    while con.fake.runs().len() < 6 {
        assert!(
            tokio::time::Instant::now() < until,
            "the fresh admission's show never ran: {:?}",
            con.fake.runs()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        con.fake.runs(),
        [
            "start show",
            "end show",
            "start validate",
            "end validate",
            "start show",
            "end show"
        ]
    );
    con.fake.assert_every_line_ran_the_fake();
    running.stop().await;
}

/// **The orderly stop unloads the agent**, admin-con's one act on its own
/// initiative: on the stop, `unload` runs through the invoker on its exact
/// line.
#[tokio::test]
async fn the_stop_unloads_the_agent() {
    let Some(con) = Con::open(&["show", "unload"]).await else {
        return;
    };
    let running = con.start(GRACE).await;
    running.stop().await;
    assert_eq!(
        con.fake.runs(),
        ["start show", "end show", "start unload", "end unload"]
    );
    assert_eq!(
        con.fake.argvs().last(),
        Some(&con.fake.line("unload", false))
    );
    con.fake.assert_every_line_ran_the_fake();
}

/// **A ceiling without `unload` issues none at the stop.**
#[tokio::test]
async fn a_stop_without_unload_granted_issues_none() {
    let Some(con) = Con::open(&["show"]).await else {
        return;
    };
    let running = con.start(GRACE).await;
    running.stop().await;
    assert_eq!(con.fake.runs(), ["start show", "end show"]);
    con.fake.assert_every_line_ran_the_fake();
}

/// **The stop waits for the verb in flight until its child is reaped**,
/// then unloads: the `unload` never runs beside it.
#[tokio::test]
async fn the_stop_waits_for_the_child_in_flight_then_unloads() {
    let Some(con) = Con::open(&["show", "validate", "unload"]).await else {
        return;
    };
    con.fake.delay("validate", Duration::from_millis(1500));
    let running = con.start(GRACE).await;
    con.ask_past_the_bound("validate").await;
    running.stop().await;
    assert_eq!(
        con.fake.runs(),
        [
            "start show",
            "end show",
            "start validate",
            "end validate",
            "start unload",
            "end unload"
        ]
    );
    con.fake.assert_every_line_ran_the_fake();
}

/// **A child that outlasts the stop's grace means the `unload` is not
/// issued**, and the child is not killed: it runs to its own end after
/// admin-con has stopped.
#[tokio::test]
async fn a_child_that_outlasts_the_grace_leaves_the_unload_unissued() {
    let Some(con) = Con::open(&["show", "validate", "unload"]).await else {
        return;
    };
    con.fake.delay("validate", Duration::from_millis(3000));
    let grace = Duration::from_millis(1000);
    let running = con.start(grace).await;
    con.ask_past_the_bound("validate").await;
    let stopped = tokio::time::Instant::now();
    running.stop().await;
    assert!(
        stopped.elapsed() < Duration::from_millis(2500),
        "the stop kept to its grace: {:?}",
        stopped.elapsed()
    );
    con.fake.wait_run("end validate").await;
    // Long enough for an unload the stop wrongly left behind to start.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        con.fake.runs(),
        ["start show", "end show", "start validate", "end validate"]
    );
    con.fake.assert_every_line_ran_the_fake();
}

/// **The stop's `unload` runs on past the grace, owned to its end**: the
/// stop ends at its deadline, and the child, still read and reaped by its
/// task, writes its whole answer rather than into a closed pipe.
#[tokio::test]
async fn the_stops_unload_runs_on_past_the_grace_unbroken() {
    let Some(con) = Con::open(&["show", "unload"]).await else {
        return;
    };
    con.fake.delay("unload", Duration::from_millis(2000));
    let grace = Duration::from_millis(1000);
    let running = con.start(grace).await;
    let stopped = tokio::time::Instant::now();
    running.stop().await;
    assert!(
        stopped.elapsed() < Duration::from_millis(1800),
        "the stop kept to its grace: {:?}",
        stopped.elapsed()
    );
    con.fake.wait_run("end unload").await;
    assert_eq!(
        con.fake.runs(),
        ["start show", "end show", "start unload", "end unload"]
    );
    con.fake.assert_every_line_ran_the_fake();
}

/// **The stop's ceiling ask ends by the stop's deadline**: a slot freed a
/// second before the deadline, then a listing that hangs, and the stop
/// ends at the deadline with no `unload`, not a hello's bound after it.
#[tokio::test]
async fn the_stops_ceiling_ask_ends_by_the_deadline() {
    let Some(con) = Con::open(&["show", "validate", "unload"]).await else {
        return;
    };
    con.fake.delay("validate", Duration::from_millis(1300));
    let grace = Duration::from_millis(2000);
    let running = con.start(grace).await;
    con.ask_past_the_bound("validate").await;
    con.fake.delay("list", Duration::from_secs(5));
    let stopped = tokio::time::Instant::now();
    running.stop().await;
    assert!(
        stopped.elapsed() < Duration::from_millis(2600),
        "the stop ended by its deadline: {:?}",
        stopped.elapsed()
    );
    assert_eq!(
        con.fake.runs(),
        ["start show", "end show", "start validate", "end validate"]
    );
    con.fake.assert_every_line_ran_the_fake();
}

/// **A `show` lands its constituents on the row** with its date (Spec
/// 2.12), and one naming none writes none.
#[tokio::test]
async fn a_show_lands_its_constituents_on_the_row() {
    let Some(con) = Con::open(&["show"]).await else {
        return;
    };
    con.fake.answer(
        "show",
        json!({"kind": "state", "state": "idle", "load": {"declaration": "sha-1"},
               "constituents": [4101, 4102, 4103]})
        .to_string(),
    );
    let running = con.start(GRACE).await;
    let row = con
        .lab
        .wait_for(&con.id, "the constituents", |a| a.constituents.is_some())
        .await;
    assert_eq!(row.constituents, Some(vec![4101, 4102, 4103]));
    let first = row.constituents_at.expect("dated");

    con.fake.answer("show", state("unloaded"));
    verb(&con.lab.listener, &con.id, "show").await.unwrap();
    let row = con
        .lab
        .wait_for(&con.id, "no constituents", |a| a.constituents.is_none())
        .await;
    assert!(row.constituents_at.expect("dated") > first);
    assert_eq!(row.load_state.as_deref(), Some("unloaded"));
    running.stop().await;
}
