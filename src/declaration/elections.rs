//! conforms: web-the-screens-elections-are-the-agents-as-read
//!
//! **The three elections, as read at the pin** (Spec 6.1, the test of
//! membership): `turn-context`, a member of `AgentConfig` at the top of the
//! agent's file, and `residual-readout-election` and `surprisal-election`,
//! members of `DecoderInstruction`, `[spu-instruction.decoder]` in the
//! file, all in `crates/weaver-types/src/config.rs`. This crate links no
//! crate of the agent (Spec 1), so the passages are held beside this module
//! as read (`elections-5ec7ecf8.md`, byte for byte inside its fences), and a
//! test fails where the screen and the copy differ: a member's name, its
//! table in the file, whether a serving load requires it, what its absence
//! means, and the turn context's values. A reread at a new pin is a diff of
//! the copy.

/// **The WeaverAgent commit the three elections were read at**. A test
/// holds it to the build's pin, [`super::render::CORPUS_COMMIT`], so the pin
/// does not move until the elections are reread at the new commit.
pub const ELECTIONS_READ_AT: &str = "5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291";

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::ELECTIONS_READ_AT;
    use crate::declaration::render::{CORPUS_COMMIT, Meta, agent_toml};
    use crate::declaration::{Document, Kind};

    /// The passages as read at [`ELECTIONS_READ_AT`].
    const READ: &str = include_str!("elections-5ec7ecf8.md");

    /// **The fenced text under the heading naming `lines`**, one passage of
    /// `config.rs` the copy holds.
    fn passage(lines: &str) -> &'static str {
        let heading = format!("## `crates/weaver-types/src/config.rs`, lines {lines}\n");
        let at = READ
            .find(&heading)
            .unwrap_or_else(|| panic!("the copy holds no passage of lines {lines}"));
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

    /// A snake_case name as kebab-case writes it.
    fn kebab(name: &str) -> String {
        name.replace('_', "-")
    }

    /// A CamelCase variant as kebab-case writes it.
    fn kebab_variant(name: &str) -> String {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_ascii_uppercase() {
                if i > 0 {
                    out.push('-');
                }
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }

    /// **One member as the copy declares it**: its name in the file, the
    /// table its struct is in the file, and its absence's meaning, none
    /// where the struct's field carries no serde default and a serving load
    /// refuses its absence.
    #[derive(Debug, PartialEq)]
    struct Member {
        name: String,
        table: Vec<&'static str>,
        absent: Option<String>,
    }

    /// **Whether a struct's field carries a serde default, as the copy
    /// declares it**: the struct's kebab-case rename read off its head, the
    /// field found by its declaration, and the default read off the
    /// attribute on the line before it.
    fn field(lines: &[&str], strukt: &str, field: &str, ty: &str) -> bool {
        let head = lines
            .iter()
            .position(|l| *l == format!("pub struct {strukt} {{"))
            .unwrap_or_else(|| panic!("the copy declares no struct {strukt}"));
        assert!(
            lines[..head]
                .last()
                .is_some_and(|a| a.starts_with("#[serde(rename_all = \"kebab-case\"")),
            "{strukt}'s members are kebab-case in the file"
        );
        let at = lines
            .iter()
            .position(|l| *l == format!("pub {field}: {ty},"))
            .unwrap_or_else(|| panic!("{strukt} declares no {field}: {ty}"));
        lines[at - 1] == "#[serde(default)]"
    }

    /// **The three members as the copy declares them**, with the turn
    /// context's values in the enum's order.
    fn as_read() -> (Vec<Member>, Vec<String>) {
        let head = code(passage("43 to 45"));
        let turn = code(passage("97 to 120"));
        let decoder = code(passage("271 to 306"));

        // `AgentConfig`'s head and its `turn_context`, an option defaulted.
        let mut agent = head.clone();
        agent.extend_from_slice(&turn);
        let defaulted = field(&agent, "AgentConfig", "turn_context", "Option<TurnContext>");
        assert!(
            defaulted,
            "turn_context carries a serde default, so its absence is the enum's default"
        );

        // The enum: kebab-case, its variants in order, the default marked.
        let open = turn
            .iter()
            .position(|l| *l == "pub enum TurnContext {")
            .expect("the copy declares the enum TurnContext");
        assert_eq!(
            turn[open - 1],
            "#[serde(rename_all = \"kebab-case\")]",
            "TurnContext's values are kebab-case in the file"
        );
        let close = turn[open..].iter().position(|l| *l == "}").unwrap() + open;
        let mut values = Vec::new();
        let mut default = None;
        let mut marked = false;
        for line in &turn[open + 1..close] {
            if *line == "#[default]" {
                marked = true;
                continue;
            }
            let variant = line
                .strip_suffix(',')
                .unwrap_or_else(|| panic!("a variant's line: {line}"));
            let value = kebab_variant(variant);
            if marked {
                default = Some(value.clone());
                marked = false;
            }
            values.push(value);
        }

        // `DecoderInstruction`'s two booleans: the readout's with no
        // default, the surprisal's defaulted to false.
        let readout_defaulted = field(
            &decoder,
            "DecoderInstruction",
            "residual_readout_election",
            "bool",
        );
        let surprisal_defaulted =
            field(&decoder, "DecoderInstruction", "surprisal_election", "bool");

        let decoder_table = vec!["spu-instruction", "decoder"];
        let members = vec![
            Member {
                name: kebab("turn_context"),
                table: vec![],
                absent: default,
            },
            Member {
                name: kebab("residual_readout_election"),
                table: decoder_table.clone(),
                absent: readout_defaulted.then(|| "false".to_owned()),
            },
            Member {
                name: kebab("surprisal_election"),
                table: decoder_table,
                absent: surprisal_defaulted.then(|| "false".to_owned()),
            },
        ];
        (members, values)
    }

    /// A whole document, every member in range, the three elections given.
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
            "store": "none",
            "gate": {"tools": []},
            "prompt": null,
            "turn_context": "continuous",
            "residual_readout_election": false,
            "surprisal_election": false,
        })
    }

    /// The document's member for a file name.
    fn member_of(name: &str) -> String {
        name.replace('-', "_")
    }

    /// The file a document derives, read as TOML.
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

    /// The value at a member's place in the file, if any.
    fn at<'a>(table: &'a toml::Table, member: &Member) -> Option<&'a toml::Value> {
        let mut t = table;
        for key in &member.table {
            t = t.get(*key)?.as_table()?;
        }
        t.get(&member.name)
    }

    /// **The screen's three elections are the agent's at the pin, as
    /// read**: each member the copy declares is written under the copy's
    /// name in the copy's table, as the value the document gives; the turn
    /// context admits exactly the copy's values and refuses another; a
    /// member whose struct field carries no default is required at save,
    /// and one that carries a default is not, its absence written as a
    /// comment naming the copy's meaning of it and no member.
    #[test]
    fn the_three_elections_are_the_agents_as_read() {
        let (members, values) = as_read();
        assert_eq!(
            values,
            ["continuous", "independent"],
            "the turn context's values as read"
        );

        // Each value the copy admits is admitted and written; another is
        // refused by its kind, naming the member.
        let turn = &members[0];
        for value in &values {
            let mut v = whole();
            v[member_of(&turn.name)] = json!(value);
            let (_, table) = file_of(&v);
            assert_eq!(
                at(&table, turn).and_then(|x| x.as_str()),
                Some(value.as_str()),
                "{} = {value} is written at its place",
                turn.name
            );
        }
        let mut v = whole();
        v[member_of(&turn.name)] = json!("sometimes");
        let refused = Document::parse(&v).expect_err("a third turn context parses");
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (Kind::Election, "turn_context"),
            "{refused}"
        );

        // The two booleans, each written at its place as given.
        for member in &members[1..] {
            for given in [true, false] {
                let mut v = whole();
                v[member_of(&member.name)] = json!(given);
                let (_, table) = file_of(&v);
                assert_eq!(
                    at(&table, member).and_then(|x| x.as_bool()),
                    Some(given),
                    "{} = {given} is written at its place",
                    member.name
                );
            }
        }

        // Absent: required where the copy gives no default, its meaning in
        // a comment otherwise, and never a member.
        for member in &members {
            let mut v = whole();
            v.as_object_mut().unwrap().remove(&member_of(&member.name));
            let document = Document::parse(&v)
                .unwrap_or_else(|e| panic!("a row lacking {} reopens: {e}", member.name));
            match &member.absent {
                None => {
                    let refused = document
                        .serving()
                        .expect_err("a row lacking a required election saves");
                    assert_eq!(
                        (refused.kind, refused.field.as_str()),
                        (Kind::Missing, member_of(&member.name).as_str()),
                        "{refused}"
                    );
                }
                Some(meaning) => {
                    assert_eq!(
                        document.serving(),
                        Ok(()),
                        "{} is not required",
                        member.name
                    );
                    let (text, table) = file_of(&v);
                    assert!(at(&table, member).is_none(), "{} is written", member.name);
                    assert!(
                        text.contains(&format!(
                            "# {}: not declared on this row; the agent reads its absence as {meaning}.",
                            member.name
                        )),
                        "the absence of {} is said: {text}",
                        member.name
                    );
                }
            }
        }
        // The field's election, beside the two: an option the copy
        // declares with a serde default, so a file without it parses; the
        // screen declares it nowhere, and the file names it in its last
        // block and writes no member.
        let decoder = code(passage("271 to 306"));
        let pos = decoder
            .iter()
            .position(|l| *l == "pub field_election: Option<FieldElection>,")
            .expect("the copy declares field_election as an option");
        assert!(
            decoder[pos - 1].starts_with("#[serde(default"),
            "field_election carries a serde default"
        );
        let field = Member {
            name: kebab("field_election"),
            table: vec!["spu-instruction", "decoder"],
            absent: None,
        };
        let (text, table) = file_of(&whole());
        assert!(at(&table, &field).is_none(), "field-election is written");
        assert!(
            text.contains(&format!(
                "# [spu-instruction.decoder] {}: not declared on this screen; its absence is the ordinary posture, no probability field elected, and the agent reads no default into it.",
                field.name
            )),
            "the file names field-election: {text}"
        );
        assert!(
            READ.lines()
                .next()
                .is_some_and(|title| title.contains(ELECTIONS_READ_AT)),
            "the copy names the commit it was read at"
        );
    }

    /// **The pin moves only with a reread**: the elections were read at the
    /// commit the build pins.
    #[test]
    fn the_elections_were_read_at_the_builds_pin() {
        assert_eq!(
            ELECTIONS_READ_AT, CORPUS_COMMIT,
            "the build's pin moved: reread the three passages of config.rs at the new commit, \
             refresh elections-<commit>.md, then move ELECTIONS_READ_AT"
        );
    }
}
