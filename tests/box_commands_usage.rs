//! **Version 1's two forms of the box's report are usage** (Spec 8.3):
//! `admin-con setup report --stage drives` and `admin-con pair
//! --drives-report` each answer `usage` (2) as clap reads the arguments,
//! before any config is read or any dial, since `weaver-survey/2` has one
//! stage, `pairing`. The paths name nothing that exists, and the console's
//! address is a documentation address, so a run that went past the
//! arguments would fail otherwise.

use std::io::Write;
use std::process::{Command, Stdio};

/// admin-con's status, its standard output and its standard error, run on
/// `args` with `input` on standard input.
fn admin_con(args: &[&str], input: &[u8]) -> (Option<i32>, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_admin-con"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("admin-con runs");
    // The binary may exit on usage before it reads, closing the pipe, so
    // a failed write is no fault of the test.
    let _ = child.stdin.take().unwrap().write_all(input);
    let out = child.wait_with_output().unwrap();
    (
        out.status.code(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn setup_report_at_the_stage_drives_is_usage() {
    let (status, out, err) = admin_con(
        &[
            "setup",
            "report",
            "--config",
            "/nonexistent/admin-con.toml",
            "--stage",
            "drives",
        ],
        b"{}\n",
    );
    assert_eq!(status, Some(2), "{out}{err}");
    assert_eq!(out, "{\"event\":\"usage\"}\n", "{err}");
    assert!(err.contains("invalid value 'drives' for '--stage"), "{err}");
    assert!(err.contains("[possible values: pairing]"), "{err}");
}

#[test]
fn pair_with_a_drives_report_is_usage() {
    let (status, out, err) = admin_con(
        &[
            "pair",
            "--console",
            "192.0.2.1:9",
            "--admin-request",
            "/nonexistent/admin.request",
            "--report",
            "/nonexistent/pairing.json",
            "--answer",
            "/nonexistent/answer.json",
            "--drives-report",
            "/nonexistent/drives.json",
        ],
        b"",
    );
    assert_eq!(status, Some(2), "{out}{err}");
    assert_eq!(out, "{\"event\":\"usage\"}\n", "{err}");
    assert!(
        err.contains("unexpected argument '--drives-report' found"),
        "{err}"
    );
}
