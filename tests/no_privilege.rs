//! conforms: web-no-privileged-invocation
//!
//! **No privileged invocation in the crate** (Spec 7.2, 8): the source tree
//! runs no `sudo` and no other privilege-escalating program, calls no
//! setuid family function, and carries no root wrapper. The scan reads every
//! Rust file under `src/` and looks for the code that would do any of it,
//! skipping comment lines, which name the rule rather than break it. Its
//! patterns are assembled at run time so this file does not match itself.

use std::path::{Path, PathBuf};

/// The patterns: a privilege-escalating program named as a command or a
/// string to run, and the setuid family called.
fn patterns() -> Vec<String> {
    let programs = [
        ["su", "do"].concat(),
        ["do", "as"].concat(),
        ["pk", "exec"].concat(),
    ];
    let mut out = Vec::new();
    for program in &programs {
        out.push(format!("Command::new(\"{program}\""));
        out.push(format!("\"{program}\""));
        out.push(format!("/{program}\""));
    }
    for call in [
        ["set", "uid("].concat(),
        ["set", "euid("].concat(),
        ["set", "reuid("].concat(),
        ["set", "resuid("].concat(),
        ["set", "gid("].concat(),
        ["set", "egid("].concat(),
    ] {
        out.push(call);
    }
    out
}

fn rust_files(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Every line under `root` that carries a privileged invocation, as
/// `path:line: text`.
fn privileged_invocations(root: &Path) -> Vec<String> {
    let patterns = patterns();
    let mut files = Vec::new();
    rust_files(root, &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for (i, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if patterns.iter().any(|p| code.contains(p.as_str())) {
                found.push(format!("{}:{}: {}", file.display(), i + 1, code));
            }
        }
    }
    found
}

#[test]
fn the_source_tree_holds_no_privileged_invocation() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = privileged_invocations(&src);
    assert!(found.is_empty(), "privileged invocations: {found:#?}");
}

/// **The scan finds one when one is planted**: a watch that cannot fail is
/// not a test.
#[test]
fn a_planted_privileged_invocation_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let program = ["su", "do"].concat();
    std::fs::write(
        dir.path().join("planted.rs"),
        format!("// a comment naming {program} is not an invocation\nfn f() {{ let _ = std::process::Command::new(\"{program}\"); }}\n"),
    )
    .unwrap();
    let call = ["set", "uid("].concat();
    std::fs::write(
        dir.path().join("also.rs"),
        format!("fn g() {{ unsafe {{ libc::{call}0) }}; }}\n"),
    )
    .unwrap();
    let found = privileged_invocations(dir.path());
    assert_eq!(found.len(), 2, "{found:#?}");
}
