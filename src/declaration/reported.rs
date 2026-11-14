//! conforms: web-the-agent-page-sets-what-the-agent-reports-beside-the-declaration
//! conforms: web-the-reported-members-are-the-agents-as-read
//! conforms: web-each-reported-member-reads-equal-differs-or-not-in-this-report
//!
//! **What an agent reports of its load, set beside the declaration saved
//! under its name** (Spec 2.12, 2.13). The register row keeps one tuple,
//! written from the latest `show` or the latest live load event, and names
//! its source; this module reads that tuple by its source and sets each
//! member the agent reports beside the same member of a document, member by
//! member: equal, differs, or not in this report.
//!
//! **Nothing here says which file the agent was loaded from.** The console
//! derives a file it cannot load as it stands and records no digest of a
//! file it wrote, so a reported member that differs may only mean an older
//! version was the source; the pane says so.
//!
//! **The shapes are the agent's at the pin** ([`REPORTED_READ_AT`]), held by
//! a dated copy of the passages (`reported-5ec7ecf8.md`):
//! - `show`'s `LoadFacts`, its members by their field names, the state
//!   election kebab-case, and no turn context;
//! - the harness's load event, `Elections`, snake_case, the tee's election
//!   as `tee` and the turn context kebab-case, and no artifact;
//! - the serving flow's load event, `LoadRecord`, which carries neither the
//!   tee nor the turn context, nor the artifact.
//!
//! A member is read by its presence: one the shape does not carry, or that
//! a record carries as null, is not in this report.

use serde_json::Value;

use super::state_election::{ElectedKind, StateElection};
use super::{Document, Engine, Function, TurnContext};

/// **The commit the reported shapes were read at**: moves only with a
/// reread of the copy, and equals the build's pin.
pub const REPORTED_READ_AT: &str = "5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291";

/// **Which source the register row's tuple stands on** (Spec 2.12), and so
/// which shape it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `show`'s answer, its member `load`.
    Show,
    /// A live load event's payload, either load event's shape.
    Event,
}

impl Source {
    /// The source as the register row names it, none for any other word.
    pub fn of(word: &str) -> Option<Source> {
        match word {
            "show" => Some(Source::Show),
            "event" => Some(Source::Event),
            _ => None,
        }
    }
}

/// **The members the agent reports that the declaration also states**, in
/// the pane's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Member {
    Artifact,
    ResidualReadout,
    Surprisal,
    TurnContext,
    StateElection,
    Store,
}

impl Member {
    pub fn label(self) -> &'static str {
        match self {
            Member::Artifact => "the decoder's artifact",
            Member::ResidualReadout => "the residual readout election",
            Member::Surprisal => "the surprisal election",
            Member::TurnContext => "the turn context",
            Member::StateElection => "the state election",
            Member::Store => "the store",
        }
    }
}

/// **Where each shape carries each member**: its name in the tuple, none
/// where the shape carries no such member. `show`'s facts name no turn
/// context; a load event names no artifact, and carries the state election
/// as the tee's.
pub const SHOW: [(Member, Option<&str>); 6] = [
    (Member::Artifact, Some("artifact")),
    (Member::ResidualReadout, Some("residual_readout")),
    (Member::Surprisal, Some("surprisal")),
    (Member::TurnContext, None),
    (Member::StateElection, Some("state_election")),
    (Member::Store, Some("state_store")),
];

/// The same, for a load event.
pub const EVENT: [(Member, Option<&str>); 6] = [
    (Member::Artifact, None),
    (Member::ResidualReadout, Some("residual_readout")),
    (Member::Surprisal, Some("surprisal")),
    (Member::TurnContext, Some("turn_context")),
    (Member::StateElection, Some("tee")),
    (Member::Store, Some("state_store")),
];

/// The state election's flag, in `show`'s spelling and in the tee's.
const ALL_KINDS_SHOW: &str = "all-kinds";
const ALL_KINDS_TEE: &str = "all_kinds";

/// **A member's verdict** against the agent's report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Equal,
    Differs {
        declared: String,
        reported: String,
    },
    /// The report's shape carries no such member, or carries it null.
    NotInReport,
    /// The report carries the member in a shape this build does not read.
    Unreadable(String),
}

impl Verdict {
    pub fn words(&self) -> String {
        match self {
            Verdict::Equal => "equal".to_owned(),
            Verdict::Differs { declared, reported } => {
                format!("differs (declared {declared}, reported {reported})")
            }
            Verdict::NotInReport => "not in this report".to_owned(),
            Verdict::Unreadable(raw) => {
                format!("in this report in a shape this build does not read: {raw}")
            }
        }
    }
}

/// **One member as the document declares it**: its words, and whether they
/// stand by the agent's reading of an absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub words: String,
    /// How the absence is read, where the row states no value.
    pub by_absence: Option<&'static str>,
}

impl Declared {
    fn stated(words: String) -> Declared {
        Declared {
            words,
            by_absence: None,
        }
    }

    /// The words the pane shows: the value, and how an absence is read.
    pub fn shown(&self) -> String {
        match self.by_absence {
            Some(how) => format!("{} ({how})", self.words),
            None => self.words.clone(),
        }
    }
}

/// The words of an absence the agent has no reading for.
const NOT_DECLARED: &str = "nothing";

/// **A member as the document declares it** (Spec 6.1): an absence read as
/// the agent reads it, the residual readout's having no reading.
pub fn declared(document: &Document, member: Member) -> Declared {
    match member {
        Member::Artifact => document
            .boxes
            .iter()
            .flat_map(|b| &b.models)
            .find(|m| m.function == Function::Decoder)
            .map(|m| Declared::stated(m.artifact.clone()))
            .unwrap_or(Declared {
                words: NOT_DECLARED.to_owned(),
                by_absence: Some("no decoder on this row"),
            }),
        Member::ResidualReadout => match document.residual_readout_election {
            Some(b) => Declared::stated(b.to_string()),
            None => Declared {
                words: NOT_DECLARED.to_owned(),
                by_absence: Some("not declared on this row; a serving load refuses its absence"),
            },
        },
        Member::Surprisal => match document.surprisal_election {
            Some(b) => Declared::stated(b.to_string()),
            None => Declared {
                words: "false".to_owned(),
                by_absence: Some("by absence, as the agent reads it"),
            },
        },
        Member::TurnContext => match document.turn_context {
            Some(t) => Declared::stated(t.word().to_owned()),
            None => Declared {
                words: TurnContext::Continuous.word().to_owned(),
                by_absence: Some("by absence, as the agent reads it"),
            },
        },
        Member::StateElection => match &document.state_election {
            Some(e) => Declared::stated(election_words(e.all_kinds, &e.keys)),
            None => {
                let d = StateElection::agents_default();
                Declared {
                    words: election_words(d.all_kinds, &d.keys),
                    by_absence: Some("by absence, the default admin resolves"),
                }
            }
        },
        Member::Store => Declared::stated(document.store.word().to_owned()),
    }
}

/// **A state election in words**, its kinds in order of name and each
/// kind's paths sorted, once each, so that two spellings of one election
/// read alike: the agent takes the kinds as a set and a path as electing
/// itself however often it is named.
fn election_words(all_kinds: bool, keys: &[ElectedKind]) -> String {
    let mut keys: Vec<(String, Vec<String>)> = keys
        .iter()
        .map(|k| {
            let mut paths = k.paths.clone();
            paths.sort();
            paths.dedup();
            (k.kind.clone(), paths)
        })
        .collect();
    keys.sort();
    let head = if all_kinds {
        "every kind with its envelope"
    } else {
        "only the kinds named"
    };
    if keys.is_empty() {
        return format!("{head}, no key paths");
    }
    let named: Vec<String> = keys
        .iter()
        .map(|(kind, paths)| {
            if paths.is_empty() {
                format!("{kind} (no paths)")
            } else {
                format!("{kind} ({})", paths.join(", "))
            }
        })
        .collect();
    format!("{head}; {}", named.join("; "))
}

/// **A member as the report carries it**: none where it is not in the
/// report, its words where it reads, and the raw value where it does not.
fn read(member: Member, value: &Value, source: Source) -> Result<String, String> {
    let raw = || cut(&value.to_string());
    match member {
        Member::Artifact => value.as_str().map(str::to_owned).ok_or_else(raw),
        Member::ResidualReadout | Member::Surprisal => {
            value.as_bool().map(|b| b.to_string()).ok_or_else(raw)
        }
        Member::TurnContext => match value.as_str() {
            Some(w @ ("continuous" | "independent")) => Ok(w.to_owned()),
            _ => Err(raw()),
        },
        Member::StateElection => {
            let flag = match source {
                Source::Show => ALL_KINDS_SHOW,
                Source::Event => ALL_KINDS_TEE,
            };
            let all_kinds = value.get(flag).and_then(Value::as_bool).ok_or_else(raw)?;
            let keys = value
                .get("keys")
                .and_then(Value::as_array)
                .ok_or_else(raw)?
                .iter()
                .map(|k| {
                    let kind = k.get("kind").and_then(Value::as_str)?;
                    let paths = k
                        .get("paths")
                        .and_then(Value::as_array)?
                        .iter()
                        .map(|p| p.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()?;
                    Some(ElectedKind {
                        kind: kind.to_owned(),
                        paths,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(raw)?;
            Ok(election_words(all_kinds, &keys))
        }
        Member::Store => match value.get("engine").and_then(Value::as_str) {
            Some(w @ ("none" | "sqlite")) => Ok(w.to_owned()),
            _ => Err(raw()),
        },
    }
}

/// A raw value cut for the page.
fn cut(text: &str) -> String {
    if text.chars().count() > 80 {
        format!("{}...", text.chars().take(80).collect::<String>())
    } else {
        text.to_owned()
    }
}

/// **One member's verdict** against a tuple of the named source.
pub fn verdict(document: &Document, member: Member, tuple: &Value, source: Source) -> Verdict {
    let shape = match source {
        Source::Show => &SHOW,
        Source::Event => &EVENT,
    };
    let name = shape
        .iter()
        .find(|(m, _)| *m == member)
        .and_then(|(_, n)| *n);
    let Some(value) = name.and_then(|n| tuple.get(n)).filter(|v| !v.is_null()) else {
        return Verdict::NotInReport;
    };
    match read(member, value, source) {
        Ok(reported) => {
            let declared = declared(document, member);
            if declared.by_absence.is_some() && declared.words == NOT_DECLARED {
                Verdict::Differs {
                    declared: declared.shown(),
                    reported,
                }
            } else if declared.words == reported {
                Verdict::Equal
            } else {
                Verdict::Differs {
                    declared: declared.words,
                    reported,
                }
            }
        }
        Err(raw) => Verdict::Unreadable(raw),
    }
}

/// **The declaration file's digest the report names**, which both shapes
/// carry: admin's reading of the file it loaded.
pub fn digest(tuple: &Value) -> Option<&str> {
    tuple.get("declaration").and_then(Value::as_str)
}

/// The store engines a declaration names, for the reader's own check.
pub fn engine_word(engine: Engine) -> &'static str {
    engine.word()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{EVENT, Member, REPORTED_READ_AT, SHOW, Source, Verdict, digest, verdict};
    use crate::declaration::Document;
    use crate::declaration::render::CORPUS_COMMIT;

    /// The passages as read at [`REPORTED_READ_AT`].
    const READ: &str = include_str!("reported-5ec7ecf8.md");

    /// **The fenced text under the heading naming `file` and `lines`**.
    fn passage(file: &str, lines: &str) -> &'static str {
        let heading = format!("## `{file}`, lines {lines}\n");
        let at = READ
            .find(&heading)
            .unwrap_or_else(|| panic!("the copy holds no passage of {file}, lines {lines}"));
        let rest = &READ[at..];
        let open = rest.find("```rust\n").expect("a passage opens its fence") + "```rust\n".len();
        let close = rest[open..]
            .find("\n```")
            .expect("a passage closes its fence");
        &rest[open..open + close]
    }

    /// The passage's lines, trimmed, its comments left out.
    fn code(text: &'static str) -> Vec<&'static str> {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .collect()
    }

    const WIRE: &str = "crates/weaver-types/src/wire.rs";
    const CONFIG: &str = "crates/weaver-types/src/config.rs";
    const EVENT_RS: &str = "crates/weaver-trace/src/event.rs";
    const TEE: &str = "crates/weaver-trace/src/tee.rs";
    const RECORDS: &str = "crates/weaver-trace/src/serving/payloads/records.rs";
    const SERVING: &str = "crates/weaver-trace/src/serving/event.rs";

    /// **A struct as the copy declares it**: the attribute lines above its
    /// head (back to the derive) and each field's name and type.
    fn strukt(lines: &[&str], name: &str) -> (Vec<String>, Vec<(String, String)>) {
        let head = lines
            .iter()
            .position(|l| *l == format!("pub struct {name} {{"))
            .unwrap_or_else(|| panic!("the copy declares no struct {name}"));
        let mut attrs = Vec::new();
        let mut i = head;
        while i > 0 && lines[i - 1].starts_with("#[") {
            attrs.push(lines[i - 1].to_owned());
            i -= 1;
        }
        let close = lines[head..].iter().position(|l| *l == "}").unwrap() + head;
        let fields = lines[head + 1..close]
            .iter()
            .filter(|l| l.starts_with("pub "))
            .map(|l| {
                let (n, t) = l
                    .trim_start_matches("pub ")
                    .trim_end_matches(',')
                    .split_once(": ")
                    .unwrap();
                (n.to_owned(), t.to_owned())
            })
            .collect();
        (attrs, fields)
    }

    fn field_type<'a>(fields: &'a [(String, String)], name: &str) -> Option<&'a str> {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t.as_str())
    }

    /// Whether a struct's attributes rename its members.
    fn renames(attrs: &[String]) -> Option<String> {
        attrs
            .iter()
            .find(|a| a.contains("rename_all"))
            .map(|a| a.split('"').nth(1).unwrap().to_owned())
    }

    /// **The members the pane reads are the agent's as read at the pin**:
    /// each name the reader takes from a shape is a field of that shape's
    /// struct in the copy, of the type the reader reads, under no rename;
    /// each member the reader takes as absent from a shape is not a field
    /// of it; the state election's and the store's inner spellings are the
    /// copy's; and the load event's two shapes are both read.
    #[test]
    fn the_reported_members_are_the_agents_as_read() {
        // `show`'s facts.
        let wire = code(passage(WIRE, "486 to 519"));
        let (attrs, facts) = strukt(&wire, "LoadFacts");
        assert_eq!(
            renames(&attrs),
            None,
            "LoadFacts's members keep their field names"
        );
        let show_types = [
            ("artifact", "crate::ArtifactRef"),
            ("residual_readout", "bool"),
            ("surprisal", "bool"),
            ("state_election", "crate::config::StateElection"),
            ("state_store", "crate::config::StateStore"),
        ];
        for (member, name) in SHOW {
            match name {
                Some(name) => {
                    let want = show_types.iter().find(|(n, _)| *n == name).unwrap().1;
                    assert_eq!(field_type(&facts, name), Some(want), "{member:?} in show");
                }
                None => assert!(
                    !facts.iter().any(|(n, _)| n.contains("turn_context")),
                    "{member:?}: show's facts carry it, and the reader takes it as absent"
                ),
            }
        }
        assert_eq!(field_type(&facts, "declaration"), Some("String"));
        let artifact = code(passage(CONFIG, "535 to 540"));
        assert!(
            artifact.contains(&"pub struct ArtifactRef(pub String);"),
            "the artifact crosses as a string"
        );
        // The state election and the store as show spells them.
        let election = code(passage(CONFIG, "215 to 238"));
        let (attrs, members) = strukt(&election, "StateElection");
        assert_eq!(renames(&attrs).as_deref(), Some("kebab-case"));
        assert_eq!(
            members,
            [
                ("all_kinds".to_owned(), "bool".to_owned()),
                ("keys".to_owned(), "Vec<ElectedKindConfig>".to_owned())
            ]
        );
        assert_eq!(super::ALL_KINDS_SHOW, "all_kinds".replace('_', "-"));
        let (attrs, key) = strukt(&election, "ElectedKindConfig");
        assert_eq!(renames(&attrs).as_deref(), Some("kebab-case"));
        assert_eq!(
            key,
            [
                ("kind".to_owned(), "String".to_owned()),
                ("paths".to_owned(), "Vec<String>".to_owned())
            ]
        );
        let store = code(passage(CONFIG, "150 to 175"));
        let (attrs, engine) = strukt(&store, "StateStore");
        assert_eq!(renames(&attrs).as_deref(), Some("kebab-case"));
        assert_eq!(engine, [("engine".to_owned(), "StoreEngine".to_owned())]);
        let at = store
            .iter()
            .position(|l| *l == "pub enum StoreEngine {")
            .expect("the engines");
        assert_eq!(store[at - 1], "#[serde(rename_all = \"kebab-case\")]");
        let close = store[at..].iter().position(|l| *l == "}").unwrap() + at;
        let engines: Vec<String> = store[at + 1..close]
            .iter()
            .map(|v| v.trim_end_matches(',').to_lowercase())
            .collect();
        assert_eq!(
            engines,
            [
                super::engine_word(crate::declaration::Engine::None),
                super::engine_word(crate::declaration::Engine::Sqlite)
            ],
            "the engines the reader reads are the agent's"
        );

        // The harness's load event.
        let head = code(passage(EVENT_RS, "224 to 242"));
        assert!(head.contains(&"#[serde(untagged)]") && head.contains(&"pub enum Payload {"));
        assert!(
            passage(EVENT_RS, "224 to 242").contains("`load`\n/// carries `Elections`"),
            "a load event carries Elections"
        );
        assert!(code(passage(EVENT_RS, "269 to 274")).contains(&"Elections(Box<Elections>),"));
        let elections = code(passage(EVENT_RS, "707 to 801"));
        let (attrs, loads) = strukt(&elections, "Elections");
        assert_eq!(
            renames(&attrs),
            None,
            "Elections's members keep their field names"
        );
        let event_types = [
            ("residual_readout", "bool"),
            ("surprisal", "bool"),
            ("turn_context", "TurnContext"),
            ("tee", "Option<crate::tee::Election>"),
            ("state_store", "StoreIdentity"),
        ];
        for (member, name) in EVENT {
            match name {
                Some(name) => {
                    let want = event_types.iter().find(|(n, _)| *n == name).unwrap().1;
                    assert_eq!(
                        field_type(&loads, name),
                        Some(want),
                        "{member:?} in a load event"
                    );
                }
                None => assert!(
                    !loads.iter().any(|(n, _)| n.contains("artifact")),
                    "{member:?}: a load event carries it, and the reader takes it as absent"
                ),
            }
        }
        assert_eq!(field_type(&loads, "declaration"), Some("String"));
        let at = elections
            .iter()
            .position(|l| *l == "pub enum TurnContext {")
            .expect("the turn context");
        assert_eq!(elections[at - 1], "#[serde(rename_all = \"kebab-case\")]");
        assert_eq!(
            &elections[at + 1..at + 4],
            ["Continuous,", "Independent,", "}"],
            "the turn context's two words"
        );
        let identity = code(passage(EVENT_RS, "653 to 660"));
        let (attrs, engine) = strukt(&identity, "StoreIdentity");
        assert_eq!(renames(&attrs), None);
        assert_eq!(engine, [("engine".to_owned(), "String".to_owned())]);
        let tee = code(passage(TEE, "22 to 44"));
        let (attrs, members) = strukt(&tee, "Election");
        assert_eq!(
            renames(&attrs),
            None,
            "the tee's members keep their field names"
        );
        assert_eq!(
            members,
            [
                (super::ALL_KINDS_TEE.to_owned(), "bool".to_owned()),
                ("keys".to_owned(), "Vec<ElectedKind>".to_owned())
            ]
        );
        let (_, key) = strukt(&tee, "ElectedKind");
        assert_eq!(
            key,
            [
                ("kind".to_owned(), "String".to_owned()),
                ("paths".to_owned(), "Vec<String>".to_owned())
            ]
        );

        // The serving flow's load event: neither the tee nor the turn
        // context, nor the artifact, so each reads as not in this report.
        let serving = code(passage(SERVING, "37 to 49"));
        assert!(serving.contains(&"Elections(Box<LoadRecord>),"));
        let records = code(passage(RECORDS, "170 to 200"));
        let (attrs, record) = strukt(&records, "LoadRecord");
        assert_eq!(renames(&attrs), None);
        for (_, name) in EVENT {
            let present = name.is_some_and(|n| record.iter().any(|(f, _)| f == n));
            let carried = matches!(name, Some("residual_readout" | "surprisal" | "state_store"));
            assert_eq!(
                present, carried,
                "{name:?} in the serving flow's load event"
            );
        }
        assert_eq!(field_type(&record, "declaration"), Some("String"));

        assert!(
            READ.lines()
                .next()
                .is_some_and(|title| title.contains(REPORTED_READ_AT)),
            "the copy names the commit it was read at"
        );
    }

    /// **The pin moves only with a reread**.
    #[test]
    fn the_reported_members_were_read_at_the_builds_pin() {
        assert_eq!(
            REPORTED_READ_AT, CORPUS_COMMIT,
            "the build's pin moved: reread the passages at the new commit, \
             refresh reported-<commit>.md, then move REPORTED_READ_AT"
        );
    }

    /// A document as the screen saves it, with the members the pane sets
    /// beside a report.
    fn document() -> Document {
        Document::parse(&json!({
            "name": "karl",
            "boxes": [{
                "models": [{"function": "decoder", "artifact": "decoder-a", "format": "gguf"}],
                "gpu": {"card": "card-0"}
            }],
            "sampler": {
                "temperature": 0.7, "top_k": 40, "top_p": 0.95, "seed": 1,
                "repetition_penalty": 1.1, "repetition_window": 64,
                "max_tokens_per_turn": 512, "context_capacity": 4096
            },
            "store": "sqlite",
            "gate": {"tools": []},
            "prompt": null,
            "turn_context": "independent",
            "residual_readout_election": false,
            "surprisal_election": true,
            "state_election": {"all_kinds": true, "keys": [
                {"kind": "message.user", "paths": ["content"]},
                {"kind": "message.assistant", "paths": ["content"]}
            ]}
        }))
        .expect("the document reads")
    }

    fn show() -> Value {
        json!({
            "session": "s-1", "run": "r-1", "declaration": "d0",
            "artifact": "decoder-a",
            "residual_readout": false, "surprisal": true,
            "state_election": {"all-kinds": true, "keys": [
                {"kind": "message.assistant", "paths": ["content"]},
                {"kind": "message.user", "paths": ["content"]}
            ]},
            "state_store": {"engine": "sqlite"},
            "state_member": true,
            "composer": {"binary": "w"}
        })
    }

    fn elections() -> Value {
        json!({
            "residual_readout": false, "surprisal": true, "state_member": true,
            "state_store": {"engine": "sqlite"}, "declaration": "d0",
            "composer": {"binary": "w"},
            "tee": {"all_kinds": true, "keys": [
                {"kind": "message.user", "paths": ["content"]},
                {"kind": "message.assistant", "paths": ["content"]}
            ]},
            "stack": {}, "boundary": "b", "cause": {"uid": 0},
            "turn_context": "independent"
        })
    }

    fn all(document: &Document, tuple: &Value, source: Source) -> Vec<(Member, Verdict)> {
        SHOW.iter()
            .map(|(m, _)| (*m, verdict(document, *m, tuple, source)))
            .collect()
    }

    /// **Each member reads equal, differs or not in this report**, by the
    /// shape the source names: `show` names no turn context, a load event
    /// no artifact, and the serving flow's load event neither the tee nor
    /// the turn context; a state election reads alike however its kinds
    /// are ordered; a member in a shape this build does not read says so.
    #[test]
    fn each_member_reads_equal_differs_or_not_in_this_report() {
        let d = document();
        let verdicts = all(&d, &show(), Source::Show);
        for (m, v) in &verdicts {
            let want = if *m == Member::TurnContext {
                Verdict::NotInReport
            } else {
                Verdict::Equal
            };
            assert_eq!(v, &want, "{m:?} by show");
        }
        let verdicts = all(&d, &elections(), Source::Event);
        for (m, v) in &verdicts {
            let want = if *m == Member::Artifact {
                Verdict::NotInReport
            } else {
                Verdict::Equal
            };
            assert_eq!(v, &want, "{m:?} by the harness's load event");
        }
        let mut serving = elections();
        let o = serving.as_object_mut().unwrap();
        o.remove("tee");
        o.remove("turn_context");
        o.insert("state_flow".into(), json!({}));
        for (m, v) in all(&d, &serving, Source::Event) {
            let want = if matches!(
                m,
                Member::Artifact | Member::TurnContext | Member::StateElection
            ) {
                Verdict::NotInReport
            } else {
                Verdict::Equal
            };
            assert_eq!(v, want, "{m:?} by the serving flow's load event");
        }
        let mut null = elections();
        null["tee"] = Value::Null;
        assert_eq!(
            verdict(&d, Member::StateElection, &null, Source::Event),
            Verdict::NotInReport,
            "a record before the tee carries it null"
        );

        let mut moved = show();
        moved["artifact"] = json!("decoder-b");
        moved["surprisal"] = json!(false);
        moved["state_store"] = json!({"engine": "none"});
        moved["state_election"] = json!({"all-kinds": false, "keys": []});
        assert_eq!(
            verdict(&d, Member::Artifact, &moved, Source::Show).words(),
            "differs (declared decoder-a, reported decoder-b)"
        );
        assert_eq!(
            verdict(&d, Member::Surprisal, &moved, Source::Show).words(),
            "differs (declared true, reported false)"
        );
        assert_eq!(
            verdict(&d, Member::Store, &moved, Source::Show).words(),
            "differs (declared sqlite, reported none)"
        );
        assert_eq!(
            verdict(&d, Member::StateElection, &moved, Source::Show).words(),
            "differs (declared every kind with its envelope; message.assistant (content); \
             message.user (content), reported only the kinds named, no key paths)"
        );
        let mut turned = elections();
        turned["turn_context"] = json!("continuous");
        assert_eq!(
            verdict(&d, Member::TurnContext, &turned, Source::Event).words(),
            "differs (declared independent, reported continuous)"
        );
        // A turn context the agent never names is not read.
        let mut fresh = elections();
        fresh["turn_context"] = json!("fresh");
        assert!(
            matches!(
                verdict(&d, Member::TurnContext, &fresh, Source::Event),
                Verdict::Unreadable(_)
            ),
            "a turn context the agent never names"
        );
        // A path named twice elects itself once.
        let mut twice = show();
        twice["state_election"]["keys"][0]["paths"] = json!(["content", "content"]);
        assert_eq!(
            verdict(&d, Member::StateElection, &twice, Source::Show),
            Verdict::Equal,
            "a path named twice"
        );
        // The tee's spelling read by show's name is not read.
        let mut misspelled = show();
        misspelled["state_election"] = json!({"all_kinds": true, "keys": []});
        assert!(matches!(
            verdict(&d, Member::StateElection, &misspelled, Source::Show),
            Verdict::Unreadable(_)
        ));
        let mut odd = show();
        odd["state_store"] = json!({"engine": "postgres"});
        assert_eq!(
            verdict(&d, Member::Store, &odd, Source::Show).words(),
            "in this report in a shape this build does not read: {\"engine\":\"postgres\"}"
        );
        assert_eq!(digest(&show()), Some("d0"));
        assert_eq!(digest(&elections()), Some("d0"));
    }

    /// **An absence is set beside the report as the agent reads it**: the
    /// turn context's as continuous, the surprisal's as false, the state
    /// election's as the default; the residual readout's, which the agent
    /// has no reading for, differs from whatever is reported.
    #[test]
    fn an_absence_is_set_beside_the_report_as_the_agent_reads_it() {
        let mut d = document();
        d.turn_context = None;
        d.surprisal_election = None;
        d.state_election = None;
        d.residual_readout_election = None;
        let mut tuple = elections();
        tuple["turn_context"] = json!("continuous");
        tuple["surprisal"] = json!(false);
        tuple["tee"] = json!({"all_kinds": true, "keys": []});
        for m in [
            Member::TurnContext,
            Member::Surprisal,
            Member::StateElection,
        ] {
            assert_eq!(
                verdict(&d, m, &tuple, Source::Event),
                Verdict::Equal,
                "{m:?}"
            );
        }
        assert_eq!(
            verdict(&d, Member::ResidualReadout, &tuple, Source::Event).words(),
            "differs (declared nothing (not declared on this row; a serving load refuses its absence), reported false)"
        );
    }
}
