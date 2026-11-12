//! **No bound of tokio's is added unread** (Spec 7.2; `crate::deadline`):
//! the class guard of the act of 2026-11-12. tokio's `timeout` and
//! `timeout_at` poll the bounded future before they read the deadline, so a
//! store write they bound can commit at the deadline's wake while its
//! caller reports it as not made. The sweep of that date read every call
//! compiled outside test, moved the nine that bound a committing write to
//! `deadline::before`, and found one reason for each of the rest. This test
//! holds the calls to [`CALLS`], one entry per call in file and line order,
//! each naming the function it is in and its reason ([`Why`]): a new call
//! fails it, and so does a call removed or moved to another function.
//!
//! **Adding a call.** Where the bounded future holds a store write that
//! commits and the caller reports at the bound, bound it with
//! `deadline::before`, not with tokio, and add nothing here. Otherwise add
//! the call's entry at its place in [`CALLS`], with its function and the
//! one reason that holds. Where no reason holds, the call is the class this
//! guard is for. A call removed takes its entry with it.
//!
//! **What the scan reads.** The files the crate compiles, found by their
//! module declarations from `src/lib.rs` and each binary under `src/bin/`;
//! a `.rs` file under `src/` that no declaration reaches fails the test. In
//! each, every comment, string and character literal is blanked, and every
//! item or statement under `#[cfg(test)]`, or under a `cfg(all(..))` that
//! names `test`, is dropped. What is left is searched for `timeout(` and
//! `timeout_at(` after a character that cannot end a name, so a method of
//! that name counts too (the passkey library's builder has one). It reads
//! text, not resolved paths: tokio's functions imported under another name,
//! or a call a macro writes, are not seen.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// **Why a call bounds no write that could commit unreported**, the
/// sweep's seven reasons and the one call that is not tokio's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// The bounded future holds no write that commits: a read, a lock in a
    /// transaction that rolls back when dropped, a semaphore or a permit.
    Reads,
    /// The bounded commit ends a transaction that wrote nothing.
    CommitsNothing,
    /// The write runs in a task of its own, outside the bounded future,
    /// which waits for its hand-over; the task's own guard answers it.
    WriteOutside,
    /// The write may be sent again, and the report at the bound asks for
    /// that: a resend meets the run's mark and is taken as one.
    Resendable,
    /// A channel's send or receive, whose effect and readiness come in one
    /// poll, so the bound answers what happened.
    Channel,
    /// A join, or a wait on a task the bound does not cancel, whose report
    /// at the bound says the task runs on.
    Join,
    /// A socket read or write, or a handshake: a bound that passes ends the
    /// connection, and the peer reads the outcome as lost or unknown.
    Socket,
    /// A method of the same name that bounds no future.
    NotTokio,
}

use Why::*;

/// **Every call of `timeout(` and `timeout_at(` compiled outside test**,
/// in file and line order: the file, the function it is in, and its reason.
const CALLS: &[(&str, &str, Why)] = &[
    ("src/alerting.rs", "stop", Channel), // the worker's stop note
    ("src/alerting.rs", "stop", Join),    // the worker, at its stop
    ("src/alerting.rs", "drain", Reads),  // `claim_entry`, rolled back when dropped
    ("src/link/admin_con.rs", "measure", Socket),
    ("src/link/admin_con.rs", "measure", Socket),
    ("src/link/admin_con.rs", "verify", Socket),
    ("src/link/admin_con.rs", "run", Reads), // the ceiling's listing
    ("src/link/admin_con.rs", "orderly_stop", Reads), // the slot's permit
    ("src/link/admin_con.rs", "serve", Join), // the relay within the stop's grace
    ("src/link/admin_con.rs", "relay", Join), // a verb's answer, `unknown` at the bound
    ("src/link/admin_con.rs", "relay", Join), // the stop's grace
    ("src/link/admin_con.rs", "drain", Socket),
    ("src/link/client.rs", "connect", Socket),
    ("src/link/client.rs", "start", Socket),
    ("src/link/client.rs", "start", Socket),
    ("src/link/client.rs", "start", Channel), // the heartbeat
    ("src/link/client.rs", "carry", Channel),
    ("src/link/client.rs", "send", Channel),
    ("src/link/client.rs", "close_within", Join), // the writer
    ("src/link/gate_con.rs", "serve", Channel),   // the turns declined at the grace
    ("src/link/gate_con.rs", "serve", Join),      // the turns in flight at the grace
    ("src/link/listener.rs", "start_with", Reads), // the lock's ping
    ("src/link/listener.rs", "record_ask", WriteOutside), // the first record's hand-over
    ("src/link/listener.rs", "ask", Channel),
    ("src/link/listener.rs", "ask_held", Reads), // the held asks' semaphore
    ("src/link/listener.rs", "ask_held", Reads), // `shared_hold`, rolled back when dropped
    ("src/link/listener.rs", "ask_held", Reads), // a read in the hold
    ("src/link/listener.rs", "ask_held", Reads), // a read in the hold
    ("src/link/listener.rs", "ask_held", WriteOutside), // the first record's hand-over
    ("src/link/listener.rs", "ask_held", CommitsNothing), // the hold's commit
    ("src/link/listener.rs", "reserve", Reads),  // a channel permit
    ("src/link/listener.rs", "prove_lock", Channel),
    ("src/link/listener.rs", "quiesce", Join), // the connections, aborted after the grace
    ("src/link/listener.rs", "send", Channel),
    ("src/link/listener.rs", "enqueue", Channel),
    ("src/link/listener.rs", "drain_writer", Join),
    ("src/link/listener.rs", "serve_connection", Socket),
    ("src/link/listener.rs", "serve_connection", Socket),
    ("src/link/listener.rs", "serve_connection", Socket),
    ("src/link/listener.rs", "serve_connection", Socket),
    ("src/link/listener.rs", "serve_connection", Resendable), // `take_entry`
    ("src/link/relay.rs", "dial", Socket),
    ("src/listen.rs", "bounded", Socket),
    ("src/passkeys.rs", "from_config", NotTokio), // the ceremony's lifetime
    ("src/surfaces/alerts.rs", "choose", Join),   // the decision, 202 pending
    ("src/surfaces/reading.rs", "verb_shown", Join), // a verb's answer, 202
    ("src/surfaces/reading.rs", "turn_shown", Join), // a turn's answer, 202
];

/// The calls as the list holds them and as the scan finds them agree, file
/// by file; a file that differs is shown both ways, the scan's with lines.
#[test]
fn every_tokio_bound_compiled_outside_test_is_listed_with_its_reason() {
    let found = calls(Path::new(env!("CARGO_MANIFEST_DIR")));
    let files: BTreeSet<&str> = CALLS
        .iter()
        .map(|(file, _, _)| *file)
        .chain(found.iter().map(|c| c.file.as_str()))
        .collect();
    let mut differ = Vec::new();
    for file in files {
        let listed: Vec<&str> = CALLS
            .iter()
            .filter(|(f, _, _)| *f == file)
            .map(|(_, function, _)| *function)
            .collect();
        let scanned: Vec<&Call> = found.iter().filter(|c| c.file == file).collect();
        if listed
            != scanned
                .iter()
                .map(|c| c.function.as_str())
                .collect::<Vec<_>>()
        {
            let lines: Vec<String> = scanned
                .iter()
                .map(|c| {
                    format!(
                        "    (\"{}\", \"{}\", ?), // line {}",
                        c.file, c.function, c.line
                    )
                })
                .collect();
            differ.push(format!(
                "{file}: listed {listed:?}, found:\n{}",
                lines.join("\n")
            ));
        }
    }
    assert!(
        differ.is_empty(),
        "tokio's timeout or timeout_at, compiled outside test, differs from CALLS. \
         A bound on a committing store write takes deadline::before; any other call \
         takes its entry and one reason (see this module's doc).\n{}",
        differ.join("\n")
    );
}

/// The scan's own reading, on sources written here: what it must count and
/// what it must not.
#[test]
fn the_scan_counts_production_calls_and_nothing_else() {
    let functions = |src: &str| -> Vec<String> {
        calls_in("t.rs", &production(&code(src)).expect("not a test file"))
            .into_iter()
            .map(|c| c.function)
            .collect()
    };
    // Comments, strings and character literals hold no call, and a quote
    // in a character literal or a lifetime opens no string.
    assert_eq!(
        functions(
            "fn a<'x>(s: &'x str) {\n// timeout(d, f)\n/* timeout( /* nested */ */\n\
             let s = \"timeout(\"; let r = r#\"timeout(\"#; let q = '\"';\n\
             tokio::time::timeout(d, f); }"
        ),
        ["a"]
    );
    // An item or a statement under test is dropped, and a `let` under test
    // ends at its `;`, not at the block inside it.
    assert_eq!(
        functions(
            "#[cfg(test)]\nmod t { fn b() { timeout(d, f); } }\n\
             #[cfg(all(feature = \"x\", test))]\nfn c() { timeout(d, f); }\n\
             fn e() {\n#[cfg(test)]\nlet x = match y { _ => timeout(d, f) };\ntimeout_at(d, f);\n}"
        ),
        ["e"]
    );
    // What compiles outside test counts: `not(test)`, `any(test, ..)`.
    assert_eq!(
        functions(
            "#[cfg(not(test))]\nfn g() { timeout(d, f); }\n\
             #[cfg(any(test, feature = \"x\"))]\nfn h() { timeout(d, f); }"
        ),
        ["g", "h"]
    );
    // A closure's call is its function's; a nested function's is its own;
    // a method of the name counts, a longer name does not.
    assert_eq!(
        functions(
            "fn i() { let c = || timeout(d, f); fn j() { s.timeout(d); } }\n\
             fn k() { verb_timeout(d); s.timeout_secs(d); }"
        ),
        ["i", "j"]
    );
    // A file compiled only under test is none of the scan's.
    assert!(production(&code("#![cfg(test)]\nfn l() { timeout(d, f); }")).is_none());
}

/// One call compiled outside test: its file under the crate, the function
/// it is in and its line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    file: String,
    function: String,
    line: usize,
}

fn ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn blank(out: &mut [char]) {
    for c in out {
        if *c != '\n' {
            *c = ' ';
        }
    }
}

/// **The source with every comment, string and character literal
/// blanked**, newlines kept, so what is left is code and a line is still a
/// line.
fn code(src: &str) -> Vec<char> {
    let s: Vec<char> = src.chars().collect();
    let mut out = s.clone();
    let at = |k: usize| s.get(k).copied().unwrap_or('\0');
    let mut i = 0;
    while i < s.len() {
        let prev = if i > 0 { s[i - 1] } else { ' ' };
        let end = if s[i] == '/' && at(i + 1) == '/' {
            s[i..]
                .iter()
                .position(|&c| c == '\n')
                .map_or(s.len(), |n| i + n)
        } else if s[i] == '/' && at(i + 1) == '*' {
            let (mut j, mut depth) = (i + 2, 1);
            while j < s.len() && depth > 0 {
                if s[j] == '/' && at(j + 1) == '*' {
                    depth += 1;
                    j += 2;
                } else if s[j] == '*' && at(j + 1) == '/' {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            j
        } else if s[i] == 'r'
            && (!ident(prev) || (prev == 'b' && (i < 2 || !ident(s[i - 2]))))
            && matches!(at(i + 1), '"' | '#')
        {
            // A raw string, `r"..."` or `r#"..."#`; `r#name` is a raw name.
            let mut j = i + 1;
            while at(j) == '#' {
                j += 1;
            }
            if at(j) != '"' {
                i += 1;
                continue;
            }
            let hashes = j - i - 1;
            let mut k = j + 1;
            while k < s.len() {
                if s[k] == '"' && (0..hashes).all(|h| at(k + 1 + h) == '#') {
                    k += 1 + hashes;
                    break;
                }
                k += 1;
            }
            k
        } else if s[i] == '"' {
            let mut j = i + 1;
            while j < s.len() && s[j] != '"' {
                j += if s[j] == '\\' { 2 } else { 1 };
            }
            j + 1
        } else if s[i] == '\'' && at(i + 1) == '\\' {
            let mut j = i + 3;
            while j < s.len() && s[j] != '\'' {
                j += 1;
            }
            j + 1
        } else if s[i] == '\'' && at(i + 2) == '\'' && at(i + 1) != '\'' {
            i + 3
        } else {
            i += 1;
            continue;
        };
        let end = end.min(s.len());
        blank(&mut out[i..end]);
        i = end;
    }
    out
}

fn starts(s: &[char], i: usize, word: &str) -> bool {
    let mut k = i;
    for w in word.chars() {
        if s.get(k) != Some(&w) {
            return false;
        }
        k += 1;
    }
    true
}

/// The index past the bracket closing the one opened at `open`.
fn closing(s: &[char], open: usize) -> usize {
    let mut depth = 0i32;
    for (j, &c) in s.iter().enumerate().skip(open) {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
    }
    s.len()
}

/// Whether an attribute compiles its item only under test: `cfg(test)`,
/// or `cfg(all(..))` with `test` among its terms.
fn test_only(attr: &str) -> bool {
    let a: String = attr.chars().filter(|c| !c.is_whitespace()).collect();
    if a == "cfg(test)" {
        return true;
    }
    let Some(inner) = a
        .strip_prefix("cfg(all(")
        .and_then(|r| r.strip_suffix("))"))
    else {
        return false;
    };
    let (mut depth, mut term, mut terms) = (0, String::new(), Vec::new());
    for c in inner.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                terms.push(std::mem::take(&mut term));
                continue;
            }
            _ => {}
        }
        term.push(c);
    }
    terms.push(term);
    terms.iter().any(|t| t == "test")
}

/// **The code with every item or statement compiled only under test
/// blanked**, from its attribute to its end: a `let` statement's at its
/// `;`; any other's at a `;` or `,` at its own depth, at the `}` closing a
/// block it opened, or before the bracket closing the one it sits in.
/// `None` where the file is compiled only under test (`#![cfg(test)]`).
fn production(code: &[char]) -> Option<Vec<char>> {
    let mut out = code.to_vec();
    let mut i = 0;
    while i < code.len() {
        if !(starts(code, i, "#[") || starts(code, i, "#![")) {
            i += 1;
            continue;
        }
        let inner = code[i + 1] == '!';
        let open = if inner { i + 2 } else { i + 1 };
        let end = closing(code, open);
        let attr: String = code[open + 1..end - 1].iter().collect();
        if !test_only(&attr) {
            i = end;
            continue;
        }
        if inner {
            return None;
        }
        // The item's further attributes.
        let mut j = end;
        loop {
            while j < code.len() && code[j].is_whitespace() {
                j += 1;
            }
            if starts(code, j, "#[") {
                j = closing(code, j + 1);
            } else {
                break;
            }
        }
        let statement = starts(code, j, "let") && code.get(j + 3).is_some_and(|c| !ident(*c));
        let mut depth = 0i32;
        let mut k = j;
        while k < code.len() {
            match code[k] {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth < 0 {
                        break;
                    }
                }
                '}' => {
                    depth -= 1;
                    if depth < 0 {
                        break;
                    }
                    if depth == 0 && !statement {
                        k += 1;
                        break;
                    }
                }
                ';' if depth == 0 => {
                    k += 1;
                    break;
                }
                ',' if depth == 0 && !statement => {
                    k += 1;
                    break;
                }
                _ => {}
            }
            k += 1;
        }
        blank(&mut out[i..k]);
        i = k;
    }
    Some(out)
}

/// The name after a keyword at `i`, and the index past it; `None` where
/// the keyword is part of a longer name or no name follows.
fn named(code: &[char], i: usize, keyword: &str) -> Option<(String, usize)> {
    let after = i + keyword.chars().count();
    if !starts(code, i, keyword)
        || (i > 0 && ident(code[i - 1]))
        || !code.get(after).is_some_and(|c| c.is_whitespace())
    {
        return None;
    }
    let mut j = after;
    while j < code.len() && code[j].is_whitespace() {
        j += 1;
    }
    let from = j;
    while j < code.len() && ident(code[j]) {
        j += 1;
    }
    (j > from).then(|| (code[from..j].iter().collect(), j))
}

/// The modules a file declares out of line (`mod name;`).
fn declared(code: &[char]) -> Vec<String> {
    let mut found = Vec::new();
    for i in 0..code.len() {
        if let Some((name, mut j)) = named(code, i, "mod") {
            while j < code.len() && code[j].is_whitespace() {
                j += 1;
            }
            if code.get(j) == Some(&';') {
                found.push(name);
            }
        }
    }
    found
}

/// The file a module declared in `file` is read from.
fn module_file(file: &Path, name: &str, root: bool) -> PathBuf {
    let dir = if root || file.file_name().is_some_and(|n| n == "mod.rs") {
        file.parent().unwrap().to_path_buf()
    } else {
        file.with_extension("")
    };
    let flat = dir.join(format!("{name}.rs"));
    if flat.exists() {
        return flat;
    }
    let nested = dir.join(name).join("mod.rs");
    assert!(
        nested.exists(),
        "no file for module `{name}`, declared in {}",
        file.display()
    );
    nested
}

/// The named functions' bodies: the name and the body's span.
fn bodies(code: &[char]) -> Vec<(String, usize, usize)> {
    let mut found = Vec::new();
    for i in 0..code.len() {
        let Some((name, mut j)) = named(code, i, "fn") else {
            continue;
        };
        let mut depth = 0i32;
        while j < code.len() {
            match code[j] {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                '{' | ';' if depth == 0 => break,
                _ => {}
            }
            j += 1;
        }
        if code.get(j) == Some(&'{') {
            found.push((name, j, closing(code, j)));
        }
    }
    found
}

/// The calls in one file's production code, in order, each in the
/// innermost named function holding it.
fn calls_in(file: &str, code: &[char]) -> Vec<Call> {
    let functions = bodies(code);
    let mut found = Vec::new();
    for i in 0..code.len() {
        if (i > 0 && ident(code[i - 1]))
            || !(starts(code, i, "timeout(") || starts(code, i, "timeout_at("))
        {
            continue;
        }
        let function = functions
            .iter()
            .filter(|(_, from, to)| *from < i && i < *to)
            .max_by_key(|(_, from, _)| *from)
            .map_or_else(|| "(no function)".to_string(), |(n, _, _)| n.clone());
        let line = code[..i].iter().filter(|&&c| c == '\n').count() + 1;
        found.push(Call {
            file: file.to_string(),
            function,
            line,
        });
    }
    found
}

/// **Every call compiled outside test**, in file then line order, the
/// files found from the crate's roots by their module declarations.
fn calls(crate_dir: &Path) -> Vec<Call> {
    let src = crate_dir.join("src");
    let mut work: Vec<(PathBuf, bool, bool)> = vec![(src.join("lib.rs"), true, false)];
    for entry in std::fs::read_dir(src.join("bin")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "rs") {
            work.push((path, true, false));
        }
    }
    let (mut production_files, mut test_files) = (BTreeSet::new(), BTreeSet::new());
    while let Some((file, root, under_test)) = work.pop() {
        let all = code(&std::fs::read_to_string(&file).unwrap());
        let kept = if under_test { None } else { production(&all) };
        let live = kept.as_deref().map(declared).unwrap_or_default();
        for name in declared(&all) {
            let test = kept.is_none() || !live.contains(&name);
            work.push((module_file(&file, &name, root), false, test));
        }
        if kept.is_some() {
            production_files.insert(file);
        } else {
            test_files.insert(file);
        }
    }
    let mut unplaced = Vec::new();
    let mut dirs = vec![src];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && !production_files.contains(&path)
                && !test_files.contains(&path)
            {
                unplaced.push(
                    path.strip_prefix(crate_dir)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    assert!(
        unplaced.is_empty(),
        "files under src/ no module declaration reaches: {unplaced:?}"
    );
    let mut found = Vec::new();
    for file in &production_files {
        let rel = file
            .strip_prefix(crate_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let kept = production(&code(&std::fs::read_to_string(file).unwrap())).unwrap();
        found.extend(calls_in(&rel, &kept));
    }
    found
}
