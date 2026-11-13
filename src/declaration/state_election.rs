//! conforms: web-the-screens-state-election-is-the-agents-as-read
//! conforms: web-a-state-election-names-only-the-kinds-that-cross
//!
//! **The state election** (Spec 6.1): the tee's election, `[state-election]`
//! in the agent's file, a member of `AgentConfig` in
//! `crates/weaver-types/src/config.rs`. Whether every kind crosses to the
//! store with its envelope, and for each kind named the payload key paths
//! that cross beside it, each dotted from the payload root.
//!
//! **The kind is closed** to the kinds that can cross: the trace's canonical
//! kinds (`crates/weaver-trace/src/event.rs`, twenty-six by ruling) less
//! `save_point`, which the tee never sends whatever the election
//! (`crates/weaver-trace/src/tee.rs`). The agent's parse admits any text
//! there, and an entry naming another kind elects nothing; the screen
//! refuses it instead, since nothing it could mean is lost. **The paths are
//! open**: the screen judges their spelling and their bounds, and never
//! whether the kind's payload holds them, which the agent does not judge
//! either.
//!
//! This crate links no crate of the agent (Spec 1), so the passages are held
//! beside this module as read (`state-election-5ec7ecf8.md`, byte for byte
//! inside its fences), and a test fails where the screen and the copy
//! differ: the kinds a row may name, each member's name and place in the
//! file, whether a present table requires it, the default's spelling, and
//! admin's refusal beside the store `none`. A reread at a new pin is a diff
//! of the copy.

use serde_json::{Map, Value, json};

use super::{Kind, Refused, join, object, refused};

/// **The WeaverAgent commit the state election was read at**. A test holds
/// it to the build's pin, [`super::render::CORPUS_COMMIT`], so the pin does
/// not move until the election is reread at the new commit.
pub const STATE_ELECTION_READ_AT: &str = "5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291";

/// **The kinds a row may name**: the canonical kinds in the enum's order,
/// less `save_point`. A test holds them to the copy.
pub const KINDS: [&str; 25] = [
    "load",
    "unload",
    "session.closed",
    "turn.started",
    "turn.closed",
    "message.system",
    "message.user",
    "message.assistant",
    "message.tool_result",
    "tool.call.started",
    "tool.call.completed",
    "fault",
    "elision",
    "refusal",
    "flush",
    "model.request",
    "model.output",
    "model.measurement",
    "model.field",
    "classify.request",
    "classify.output",
    "recall",
    "message.restored",
    "score",
    "model.unrecovered_call",
];

/// The most paths one kind's row admits (the Planner, 2026-11-13).
pub const MAX_PATHS: usize = 32;

/// The longest path admitted, in characters (the Planner, 2026-11-13).
pub const MAX_PATH_CHARS: usize = 256;

/// **The election, judged.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateElection {
    /// Whether every kind crosses with its envelope; when false, only the
    /// kinds named in `keys` cross at all.
    pub all_kinds: bool,
    /// Each kind named, in the order given, once.
    pub keys: Vec<ElectedKind>,
}

/// **One kind's election**: the kind as the canonical form spells it, and
/// its payload key paths, none being a meaningful election (presence itself
/// is state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectedKind {
    pub kind: String,
    pub paths: Vec<String>,
}

impl StateElection {
    /// **The default election, as the agent spells it**: every kind with its
    /// envelope and no key paths, what admin resolves an absent table to.
    pub fn agents_default() -> StateElection {
        StateElection {
            all_kinds: true,
            keys: Vec::new(),
        }
    }

    /// Whether this is the default's spelling.
    pub fn is_default(&self) -> bool {
        *self == StateElection::agents_default()
    }

    /// **The canonical form a save stores**: both members, every kind's
    /// entry in its order with its paths, none an empty list.
    pub fn to_value(&self) -> Value {
        let keys: Vec<Value> = self
            .keys
            .iter()
            .map(|k| json!({"kind": k.kind, "paths": k.paths}))
            .collect();
        json!({"all_kinds": self.all_kinds, "keys": keys})
    }
}

const FIELD: &str = "state_election";

/// **The election a JSON value is, or the refusal naming its field**, judged
/// alone; the store's rule beside it is [`super::Document::parse`]'s.
/// Inside a present election every member is required, as the agent's
/// grammar requires them.
pub(super) fn parse(value: &Value) -> Result<StateElection, Refused> {
    let top = object(value, FIELD, &["all_kinds", "keys"])?;
    let all_kinds = match top.get("all_kinds") {
        None | Some(Value::Null) => {
            return Err(refused(
                join(FIELD, "all_kinds"),
                Kind::Missing,
                "is required: a state election says whether every kind crosses",
            ));
        }
        Some(Value::Bool(b)) => *b,
        Some(_) => {
            return Err(refused(
                join(FIELD, "all_kinds"),
                Kind::Election,
                "is true or false",
            ));
        }
    };
    let entries = match top.get("keys") {
        None | Some(Value::Null) => {
            return Err(refused(
                join(FIELD, "keys"),
                Kind::Missing,
                "is required: a state election lists the kinds it names, none an empty list",
            ));
        }
        Some(Value::Array(entries)) => entries,
        Some(_) => {
            return Err(refused(join(FIELD, "keys"), Kind::Shape, "is not a list"));
        }
    };
    let mut keys: Vec<ElectedKind> = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let path = format!("{FIELD}.keys[{i}]");
        let map = object(entry, &path, &["kind", "paths"])?;
        let kind = entry_kind(map, &path)?;
        if keys.iter().any(|k| k.kind == kind) {
            return Err(refused(
                join(&path, "kind"),
                Kind::Twice,
                "names a kind an earlier entry names: the agent would take the first entry for a kind and drop the rest",
            ));
        }
        let paths = entry_paths(map, &path)?;
        keys.push(ElectedKind { kind, paths });
    }
    Ok(StateElection { all_kinds, keys })
}

/// An entry's kind: present, a text, one of [`KINDS`].
fn entry_kind(map: &Map<String, Value>, path: &str) -> Result<String, Refused> {
    let field = join(path, "kind");
    match map.get("kind") {
        None | Some(Value::Null) => Err(refused(field, Kind::Missing, "is required")),
        Some(Value::String(s)) if s.is_empty() => Err(refused(field, Kind::Missing, "is required")),
        Some(Value::String(s)) if KINDS.contains(&s.as_str()) => Ok(s.clone()),
        Some(Value::String(_)) => Err(refused(
            field,
            Kind::Election,
            "is none of the kinds that can cross: the trace's canonical kinds, save_point aside, which no election matches",
        )),
        Some(_) => Err(refused(field, Kind::Shape, "is not a text")),
    }
}

/// An entry's paths: present, a list of at most [`MAX_PATHS`], each a text
/// free of control characters, at most [`MAX_PATH_CHARS`] long, with no
/// empty segment.
fn entry_paths(map: &Map<String, Value>, path: &str) -> Result<Vec<String>, Refused> {
    let field = join(path, "paths");
    let list = match map.get("paths") {
        None | Some(Value::Null) => {
            return Err(refused(
                field,
                Kind::Missing,
                "is required: a kind's paths, none an empty list",
            ));
        }
        Some(Value::Array(list)) => list,
        Some(_) => return Err(refused(field, Kind::Shape, "is not a list")),
    };
    if list.len() > MAX_PATHS {
        return Err(refused(
            field,
            Kind::Bound,
            format!("holds more than {MAX_PATHS} paths, the most the screen admits for one kind"),
        ));
    }
    let mut paths = Vec::new();
    for (j, p) in list.iter().enumerate() {
        let at = format!("{field}[{j}]");
        let Value::String(text) = p else {
            return Err(refused(at, Kind::Shape, "is not a text"));
        };
        if text.chars().any(char::is_control) {
            return Err(refused(at, Kind::Control, "holds a control character"));
        }
        if text.chars().count() > MAX_PATH_CHARS {
            return Err(refused(
                at,
                Kind::Bound,
                format!(
                    "is longer than {MAX_PATH_CHARS} characters, the longest path the screen admits"
                ),
            ));
        }
        if text.split('.').any(str::is_empty) {
            return Err(refused(
                at,
                Kind::Path,
                "has an empty segment: a path is the payload's member names joined by dots, from its root",
            ));
        }
        paths.push(text.clone());
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{KINDS, STATE_ELECTION_READ_AT, StateElection};
    use crate::declaration::render::{CORPUS_COMMIT, Meta, agent_toml};
    use crate::declaration::{Document, Kind};

    /// The passages as read at [`STATE_ELECTION_READ_AT`].
    const READ: &str = include_str!("state-election-5ec7ecf8.md");

    /// **The fenced text under the heading naming `file` and `lines`**, one
    /// passage the copy holds.
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

    /// The passage's lines, trimmed, its doc comments and comments left out.
    fn code(text: &str) -> Vec<&str> {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("//"))
            .collect()
    }

    const CONFIG: &str = "crates/weaver-types/src/config.rs";

    /// A snake_case name as kebab-case writes it.
    fn kebab(name: &str) -> String {
        name.replace('_', "-")
    }

    /// **A struct's fields as the copy declares them**, each its name, its
    /// type and whether a serde default sits on the line before it; the
    /// struct's head checked kebab-case and closed to unknown members.
    fn fields(lines: &[&str], strukt: &str) -> Vec<(String, String, bool)> {
        let head = lines
            .iter()
            .position(|l| *l == format!("pub struct {strukt} {{"))
            .unwrap_or_else(|| panic!("the copy declares no struct {strukt}"));
        assert_eq!(
            lines[head - 1],
            "#[serde(rename_all = \"kebab-case\", deny_unknown_fields)]",
            "{strukt}'s members are kebab-case in the file and closed to unknown ones"
        );
        let close = lines[head..].iter().position(|l| *l == "}").unwrap() + head;
        lines[head + 1..close]
            .iter()
            .enumerate()
            .filter_map(|(k, l)| {
                let (name, ty) = l
                    .strip_prefix("pub ")?
                    .strip_suffix(',')?
                    .split_once(": ")?;
                let defaulted = lines[head + k].starts_with("#[serde(default");
                Some((name.to_owned(), ty.to_owned(), defaulted))
            })
            .collect()
    }

    /// **The canonical kinds, as the copy spells them**, in the enum's order.
    fn canonical_kinds() -> Vec<String> {
        let lines = code(passage("crates/weaver-trace/src/event.rs", "117 to 212"));
        assert!(
            lines.contains(&"pub enum Kind {"),
            "the copy declares the enum Kind"
        );
        lines
            .iter()
            .filter_map(|l| {
                l.strip_prefix("#[serde(rename = \"")
                    .and_then(|r| r.strip_suffix("\")]"))
                    .map(str::to_owned)
            })
            .collect()
    }

    /// **The kind the tee never sends**, as the copy holds it: the kind its
    /// distillation answers with nothing before the election is read.
    fn never_crosses() -> String {
        let lines = code(passage("crates/weaver-trace/src/tee.rs", "124 to 131"));
        let at = lines
            .iter()
            .position(|l| l.starts_with("if event.kind == \""))
            .expect("the tee tests one kind before the election");
        assert_eq!(
            lines[at + 1],
            "return None;",
            "the kind the tee tests is never sent"
        );
        lines[at]
            .strip_prefix("if event.kind == \"")
            .and_then(|r| r.strip_suffix("\" {"))
            .expect("the tee names the kind")
            .to_owned()
    }

    /// A whole document the stores and elections admit, `sqlite` its store.
    fn whole() -> Value {
        json!({
            "name": "karl",
            "placement": {"kind": "host"},
            "boxes": [{
                "models": [{"function": "decoder", "artifact": "decoder-a", "format": "gguf"}],
                "gpu": {"card": "card-0"},
            }],
            "sampler": {
                "temperature": 0.7, "top_k": 40, "top_p": 0.95, "seed": 1,
                "repetition_penalty": 1.1, "repetition_window": 64,
                "max_tokens_per_turn": 512, "context_capacity": 4096,
            },
            "store": "sqlite",
            "gate": {"tools": []},
            "prompt": null,
            "turn_context": "continuous",
            "residual_readout_election": false,
            "surprisal_election": false,
        })
    }

    /// The file a document derives, as text and read as TOML.
    fn file_of(value: &Value) -> (String, toml::Table) {
        let document = Document::parse(value).unwrap_or_else(|e| panic!("{e}"));
        let text = agent_toml(
            &document,
            &Meta {
                de: "de-0000000000000001",
                version: 1,
            },
        );
        let table = toml::from_str::<toml::Table>(&text)
            .unwrap_or_else(|e| panic!("the file is TOML 1.0: {e}\n{text}"));
        (text, table)
    }

    /// **The kinds a row may name are the canonical kinds less the one the
    /// tee never sends, as read**: twenty-six read, `save_point` the one
    /// never sent, and the screen's list the other twenty-five in the
    /// enum's order. Each is admitted in a row; the one never sent and a
    /// kind outside the set are refused by their kind, naming the row's
    /// kind.
    #[test]
    fn the_kinds_a_row_may_name_are_the_canonical_kinds_less_save_point() {
        let canonical = canonical_kinds();
        assert_eq!(canonical.len(), 26, "the canonical kinds as read");
        let never = never_crosses();
        assert_eq!(never, "save_point", "the kind the tee never sends");
        let crossing: Vec<&str> = canonical
            .iter()
            .map(String::as_str)
            .filter(|k| *k != never)
            .collect();
        assert_eq!(KINDS.to_vec(), crossing, "the kinds a row may name");

        for kind in KINDS {
            let mut v = whole();
            v["state_election"] =
                json!({"all_kinds": false, "keys": [{"kind": kind, "paths": []}]});
            let document = Document::parse(&v).unwrap_or_else(|e| panic!("{kind} is refused: {e}"));
            let election = document.state_election.expect("the election is held");
            assert_eq!(election.keys[0].kind, kind);
        }
        for kind in [never.as_str(), "message.users", "Message.user", ""] {
            let mut v = whole();
            v["state_election"] = json!({"all_kinds": true, "keys": [{"kind": kind, "paths": []}]});
            let refused = Document::parse(&v)
                .err()
                .unwrap_or_else(|| panic!("the kind {kind:?} was admitted"));
            let expected = if kind.is_empty() {
                Kind::Missing
            } else {
                Kind::Election
            };
            assert_eq!(
                (refused.kind, refused.field.as_str()),
                (expected, "state_election.keys[0].kind"),
                "{kind:?}: {refused}"
            );
        }
    }

    /// **The screen's state election is the agent's at the pin, as read**:
    /// `state_election` an option of `AgentConfig` with a serde default, so
    /// a file without the table parses and its absence is the default; the
    /// two structs closed and kebab-case, every member of a present table
    /// required; each member written under its kebab-case name at its
    /// place, and each refused naming its path where a document lacks it;
    /// the default's spelling the agent's; and admin's refusal of a present
    /// table beside the store `none` held at the screen.
    #[test]
    fn the_state_election_is_the_agents_as_read() {
        // `AgentConfig`: kebab-case at the file's top level, the member an
        // option defaulted.
        let mut agent = code(passage(CONFIG, "43 to 45"));
        agent.extend(code(passage(CONFIG, "68 to 74")));
        agent.push("}");
        let top = fields(&agent, "AgentConfig");
        assert_eq!(
            top,
            [(
                "state_election".to_owned(),
                "Option<StateElection>".to_owned(),
                true
            )],
            "AgentConfig's state_election as read"
        );
        let table_name = kebab(&top[0].0);

        // The two structs, every member required inside a present table.
        let structs = code(passage(CONFIG, "207 to 238"));
        let election = fields(&structs, "StateElection");
        assert_eq!(
            election,
            [
                ("all_kinds".to_owned(), "bool".to_owned(), false),
                (
                    "keys".to_owned(),
                    "Vec<ElectedKindConfig>".to_owned(),
                    false
                ),
            ],
            "StateElection as read"
        );
        let entry = fields(&structs, "ElectedKindConfig");
        assert_eq!(
            entry,
            [
                ("kind".to_owned(), "String".to_owned(), false),
                ("paths".to_owned(), "Vec<String>".to_owned(), false),
            ],
            "ElectedKindConfig as read"
        );

        // The default's spelling: every kind, no key paths.
        let open = structs
            .iter()
            .position(|l| *l == "impl Default for StateElection {")
            .expect("the copy spells the default");
        let spelled = &structs[open..];
        assert!(
            spelled.contains(&"all_kinds: true,") && spelled.contains(&"keys: Vec::new(),"),
            "the default is every kind and no keys: {spelled:?}"
        );
        assert_eq!(
            StateElection::agents_default(),
            StateElection {
                all_kinds: true,
                keys: Vec::new(),
            }
        );
        assert!(StateElection::agents_default().is_default());

        // Each member written under its name at its place, as given.
        let mut v = whole();
        v["state_election"] = json!({
            "all_kinds": false,
            "keys": [
                {"kind": "message.user", "paths": ["content", "content.0.text"]},
                {"kind": "turn.closed", "paths": []},
            ],
        });
        let (text, table) = file_of(&v);
        let written = table
            .get(&table_name)
            .and_then(toml::Value::as_table)
            .unwrap_or_else(|| panic!("[{table_name}] is written: {text}"));
        assert_eq!(written[&kebab(&election[0].0)].as_bool(), Some(false));
        let keys = written[&kebab(&election[1].0)].as_array().unwrap();
        assert_eq!(keys.len(), 2, "{text}");
        let pairs: Vec<(String, Vec<String>)> = keys
            .iter()
            .map(|k| {
                let k = k.as_table().unwrap();
                let mut names: Vec<&str> = k.keys().map(String::as_str).collect();
                names.sort_unstable();
                let mut want = [kebab(&entry[0].0), kebab(&entry[1].0)];
                want.sort_unstable();
                assert_eq!(names, want, "an entry's members: {text}");
                (
                    k[&kebab(&entry[0].0)].as_str().unwrap().to_owned(),
                    k[&kebab(&entry[1].0)]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| p.as_str().unwrap().to_owned())
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            pairs,
            [
                (
                    "message.user".to_owned(),
                    vec!["content".to_owned(), "content.0.text".to_owned()]
                ),
                ("turn.closed".to_owned(), Vec::new()),
            ],
            "{text}"
        );

        // Each member of a present table is required, refused naming it.
        for (member, at) in [
            (&election[0].0, "state_election.all_kinds"),
            (&election[1].0, "state_election.keys"),
        ] {
            let mut v = whole();
            v["state_election"] = json!({"all_kinds": true, "keys": []});
            v["state_election"].as_object_mut().unwrap().remove(member);
            let refused = Document::parse(&v)
                .err()
                .unwrap_or_else(|| panic!("an election lacking {member} was admitted"));
            assert_eq!((refused.kind, refused.field.as_str()), (Kind::Missing, at));
        }
        for (member, at) in [
            (&entry[0].0, "state_election.keys[0].kind"),
            (&entry[1].0, "state_election.keys[0].paths"),
        ] {
            let mut v = whole();
            v["state_election"] =
                json!({"all_kinds": true, "keys": [{"kind": "message.user", "paths": []}]});
            v["state_election"]["keys"][0]
                .as_object_mut()
                .unwrap()
                .remove(member);
            let refused = Document::parse(&v)
                .err()
                .unwrap_or_else(|| panic!("an entry lacking {member} was admitted"));
            assert_eq!((refused.kind, refused.field.as_str()), (Kind::Missing, at));
        }

        // Absent, the table is not written: its meaning is said.
        let (text, table) = file_of(&whole());
        assert!(!table.contains_key(&table_name), "{text}");
        assert!(
            text.contains(&format!(
                "# [{table_name}]: not declared on this row; admin resolves its absence to the default election, the envelope of every kind."
            )),
            "the absence is said: {text}"
        );

        // Admin refuses a present table beside the store `none`, the
        // default's spelling among them: so does the screen.
        let inventory = code(passage(
            "crates/weaver-admin/src/inventory.rs",
            "292 to 301",
        ));
        let at = inventory
            .iter()
            .position(|l| *l == "StoreEngine::None if config.state_election.is_some() => {")
            .expect("admin judges a present election beside none");
        assert_eq!(
            inventory[at + 1],
            format!("return Err(store_invalid(\"{table_name}\"));"),
            "admin refuses it, naming the table"
        );
        let mut v = whole();
        v["store"] = json!("none");
        v["state_election"] = json!({"all_kinds": true, "keys": []});
        let refused =
            Document::parse(&v).expect_err("an election beside the store none was admitted");
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (Kind::Declined, "state_election")
        );
        assert!(
            READ.lines()
                .next()
                .is_some_and(|title| title.contains(STATE_ELECTION_READ_AT)),
            "the copy names the commit it was read at"
        );
    }

    /// **The pin moves only with a reread**: the election was read at the
    /// commit the build pins.
    #[test]
    fn the_state_election_was_read_at_the_builds_pin() {
        assert_eq!(
            STATE_ELECTION_READ_AT, CORPUS_COMMIT,
            "the build's pin moved: reread the six passages at the new commit, \
             refresh state-election-<commit>.md, then move STATE_ELECTION_READ_AT"
        );
    }
}
