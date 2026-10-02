//! conforms: web-no-privileged-invocation
//!
//! **No privileged invocation in the repository** (Spec 7.2, 8): nothing
//! tracked runs a privilege-escalating program, calls the setuid family,
//! sets a child's user or group, or sets a setuid or setgid mode bit. The
//! scan reads every tracked file outside `docs/` (by `git ls-files` where
//! the tree is a checkout, else by walking it past `.git/` and `target/`),
//! never following a symlink, so a symlinked directory cannot pull a tree
//! outside the repository into the scan or hide one from it.
//!
//! **Comments name the rule rather than break it**, so comment lines are
//! skipped by the file's own syntax: `//` and block-comment lines in Rust,
//! JavaScript and CSS, `#` in TOML and shell, `--` in SQL, `{#` and `<!--`
//! in templates. **Markdown is prose**, and only its fenced code blocks are
//! scanned, since a fence is what someone copies to run. `docs/` is prose
//! end to end and is left out. This file is left out too: its plants and
//! pattern lists name what it hunts.

use std::path::{Path, PathBuf};

/// The families a finding belongs to, so the self-test can plant one of each.
const PROGRAM: &str = "a privilege-escalating program";
const CALL: &str = "a setuid family call";
const CHILD_ID: &str = "a child's user or group set";
const MODE: &str = "a setuid or setgid mode bit";

/// The programs, each matched as a word. Assembled so the list does not
/// spell what it hunts.
fn programs() -> Vec<String> {
    [
        ["su", "do"].concat(),
        "su".to_owned(),
        ["run", "user"].concat(),
        ["run", "0"].concat(),
        ["systemd", "-run"].concat(),
        ["do", "as"].concat(),
        ["pk", "exec"].concat(),
        ["set", "cap"].concat(),
    ]
    .into()
}

/// The calls, each matched as a word followed by its parenthesis.
fn calls() -> Vec<String> {
    [
        "uid", "euid", "reuid", "resuid", "fsuid", "gid", "egid", "regid", "resgid", "fsgid",
        "groups",
    ]
    .iter()
    .map(|tail| ["set", tail].concat())
    .chain(std::iter::once(["cap", "set"].concat()))
    .collect()
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Every start of `needle` in `line` that stands as a whole word.
fn words<'a>(line: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = line.as_bytes();
    line.match_indices(needle)
        .map(|(i, _)| i)
        .filter(move |&i| {
            let before = i == 0 || !is_word_byte(bytes[i - 1]);
            let end = i + needle.len();
            let after = end == bytes.len() || !is_word_byte(bytes[end]);
            before && after
        })
}

/// A word followed, past any spaces, by an opening parenthesis.
fn called(line: &str, name: &str) -> bool {
    words(line, name).any(|i| line[i + name.len()..].trim_start().starts_with('('))
}

/// `.uid(x)`, `.gid(x)` or `.groups(x)` with an argument: the child's
/// identity set on a command. A metadata read (`meta.uid()`) has none.
fn sets_child_id(line: &str) -> bool {
    ["uid", "gid", "groups"].iter().any(|m| {
        let needle = format!(".{m}(");
        line.match_indices(needle.as_str())
            .any(|(i, _)| !line[i + needle.len()..].trim_start().starts_with(')'))
    })
}

/// A four-digit octal mode with a setuid or setgid bit, unless it is a
/// mask read with `&`; the named constants; `chmod` granting either bit.
fn sets_mode_bit(line: &str) -> bool {
    let bytes = line.as_bytes();
    let literal = line.match_indices("0o").any(|(i, _)| {
        let digits: Vec<u8> = bytes[i + 2..]
            .iter()
            .copied()
            .take_while(|b| b.is_ascii_digit() || *b == b'_')
            .filter(|b| *b != b'_')
            .collect();
        let masked = line[..i].trim_end().ends_with('&') || line[..i].trim_end().ends_with("&=");
        digits.len() == 4 && (b'2'..=b'7').contains(&digits[0]) && !masked
    });
    let constant = ["S_IS", "UID"].concat();
    let gconstant = ["S_IS", "GID"].concat();
    let named = words(line, &constant).next().is_some() || words(line, &gconstant).next().is_some();
    let chmod = words(line, "chmod").any(|i| {
        let rest = &line[i + 5..];
        rest.contains("+s")
            || rest.split_whitespace().any(|arg| {
                arg.len() == 4
                    && arg.bytes().all(|b| (b'0'..=b'7').contains(&b))
                    && (b'2'..=b'7').contains(&arg.as_bytes()[0])
            })
    });
    literal || named || chmod
}

/// The family a code line's privileged invocation belongs to, if any.
fn family(line: &str) -> Option<&'static str> {
    if programs().iter().any(|p| words(line, p).next().is_some()) {
        return Some(PROGRAM);
    }
    if calls().iter().any(|c| called(line, c)) {
        return Some(CALL);
    }
    if sets_child_id(line) {
        return Some(CHILD_ID);
    }
    if sets_mode_bit(line) {
        return Some(MODE);
    }
    None
}

/// Whether a line is a comment in its file's syntax.
fn is_comment(path: &Path, code: &str) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    match ext {
        "rs" | "js" | "css" | "c" | "h" => {
            code.starts_with("//")
                || code.starts_with("/*")
                || code == "*"
                || code.starts_with("* ")
                || code.starts_with("*/")
        }
        "sql" => code.starts_with("--"),
        "toml" | "sh" | "bash" | "yml" | "yaml" | "cfg" | "conf" => code.starts_with('#'),
        "html" | "htm" | "xml" | "svg" => code.starts_with("{#") || code.starts_with("<!--"),
        _ if name.starts_with('.') => code.starts_with('#'),
        _ => false,
    }
}

/// The tracked files under `root`, or every file where `root` is no
/// checkout. Never a symlink, never `docs/`, never this file.
fn files(root: &Path) -> Vec<PathBuf> {
    let tracked = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .ok()
        .filter(|o| o.status.success() && root.join(".git").exists());
    let mut out: Vec<PathBuf> = match tracked {
        Some(o) => o
            .stdout
            .split(|&b| b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| PathBuf::from(String::from_utf8_lossy(p).into_owned()))
            .collect(),
        None => {
            let mut out = Vec::new();
            walk(root, Path::new(""), &mut out);
            out
        }
    };
    out.retain(|rel| {
        !rel.starts_with("docs")
            && rel != Path::new("tests/no_privilege.rs")
            && std::fs::symlink_metadata(root.join(rel)).is_ok_and(|m| m.file_type().is_file())
    });
    out
}

fn walk(root: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root.join(rel)).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let child = rel.join(&name);
        if rel.as_os_str().is_empty() && (name == ".git" || name == "target") {
            continue;
        }
        // `file_type` does not follow a symlink: a link is neither a
        // directory to descend nor a file to read.
        let kind = entry.file_type().unwrap();
        if kind.is_dir() {
            walk(root, &child, out);
        } else if kind.is_file() {
            out.push(child);
        }
    }
}

/// Every privileged invocation under `root`, as `family: path:line: text`.
fn privileged_invocations(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for rel in files(root) {
        let bytes = std::fs::read(root.join(&rel)).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let markdown = rel.extension().is_some_and(|e| e == "md");
        let mut fenced = false;
        for (i, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if markdown {
                if code.starts_with("```") || code.starts_with("~~~") {
                    fenced = !fenced;
                    continue;
                }
                if !fenced || code.starts_with('#') || code.starts_with("//") {
                    continue;
                }
            } else if is_comment(&rel, code) {
                continue;
            }
            if let Some(family) = family(code) {
                found.push(format!("{family}: {}:{}: {code}", rel.display(), i + 1));
            }
        }
    }
    found
}

#[test]
fn the_repository_holds_no_privileged_invocation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let found = privileged_invocations(root);
    assert!(found.is_empty(), "privileged invocations: {found:#?}");
    assert!(
        files(root).iter().any(|f| f.starts_with("src")),
        "the scan reached the source tree"
    );
}

/// **The scan finds one of each family when it is planted**, in each file
/// syntax it reads, and nothing in a comment, in Markdown prose, under
/// `docs/`, or behind a symlinked directory: a watch that cannot fail is
/// not a test.
#[test]
fn a_planted_privileged_invocation_of_each_family_is_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let write = |rel: &str, content: String| {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    };
    let escalate = ["su", "do"].concat();
    let as_user = ["run", "user"].concat();
    let run0 = ["run", "0"].concat();
    let setresuid = ["set", "resuid"].concat();
    let setgroups = ["set", "groups"].concat();
    write(
        "src/program.rs",
        format!(
            "// a comment naming {escalate} is not an invocation\nfn f() {{ let _ = std::process::Command::new(\"{escalate}\"); }}\n"
        ),
    );
    write(
        "src/call.rs",
        format!(
            "/// {setgroups}( in a doc comment\nfn g() {{ unsafe {{ libc::{setresuid}(0, 0, 0) }}; }}\n"
        ),
    );
    write(
        "src/child.rs",
        "fn h(c: &mut std::process::Command, m: std::fs::Metadata) { let _ = m.uid(); c.uid(0); }\n"
            .to_owned(),
    );
    write(
        "src/mode.rs",
        "fn k(p: std::fs::Permissions, m: u32) { let _ = m & 0o7777; p.set_mode(0o4755); }\n"
            .to_owned(),
    );
    write(
        "install.sh",
        format!("# {escalate} in a shell comment\n{as_user} -u agent true\n"),
    );
    write(
        "migrations/0001.sql",
        format!("-- {setresuid}( in a SQL comment\nSELECT 1;\n"),
    );
    write(
        "README.md",
        format!(
            "Prose may name {escalate}.\n\n```sh\n# {escalate} in a fenced comment\n{run0} true\n```\n"
        ),
    );
    write(
        "docs/note.rs",
        format!("fn d() {{ {setresuid}(0, 0, 0); }}\n"),
    );
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(
        outside.path().join("hidden.rs"),
        format!("fn e() {{ {setresuid}(0, 0, 0); }}\n"),
    )
    .unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("linked")).unwrap();

    let found = privileged_invocations(root);
    let mut got: Vec<(String, String)> = found
        .iter()
        .map(|f| {
            let (family, rest) = f.split_once(": ").unwrap();
            let path = rest.split(':').next().unwrap();
            (family.to_owned(), path.to_owned())
        })
        .collect();
    got.sort();
    let mut want: Vec<(String, String)> = [
        (PROGRAM, "src/program.rs"),
        (CALL, "src/call.rs"),
        (CHILD_ID, "src/child.rs"),
        (MODE, "src/mode.rs"),
        (PROGRAM, "install.sh"),
        (PROGRAM, "README.md"),
    ]
    .iter()
    .map(|(f, p)| ((*f).to_owned(), (*p).to_owned()))
    .collect();
    want.sort();
    assert_eq!(got, want, "{found:#?}");
}
