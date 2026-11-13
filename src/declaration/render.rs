//! conforms: web-a-save-and-a-reopen-are-the-same-document
//! conforms: web-a-stale-row-reopens-and-saves-once-filled
//! conforms: web-a-row-without-the-elections-reopens-and-saves-once-chosen
//! conforms: web-a-row-without-the-state-election-reopens-and-saves-the-default
//! conforms: web-an-election-beside-no-store-is-refused-and-none-is-written
//! conforms: web-an-agents-state-election-derives-as-its-file-holds-it
//!
//! **The agent's file a declaration becomes** (Spec 6.1): `agent.toml` in
//! the agent's grammar, `weaver-types-Spec` section 2 at [`CORPUS_COMMIT`],
//! derived from the saved document and never stored. What the document
//! places is written as the grammar's members. A member the grammar requires
//! and the screen does not declare is a comment naming what fills it. An
//! election a row saved before the screen held it does not carry is a
//! comment saying what its absence means, and is never written. A trailing
//! block of comments lists what the grammar has no place for, each naming
//! its interface question, and a last block names what the grammar holds
//! and the screen does not declare.
//!
//! **The member names are the agent's**, read at the pin from
//! `crates/weaver-types/src/config.rs` (`AgentConfig`, kebab-case, closed
//! objects) and, for `tunable-values`, from the eight names a serving load
//! requires, held to the agent's as read by `super::tunable_set`, and for
//! the three elections held to the agent's as read by `super::elections`,
//! and for `[state-election]` held to the agent's as read by
//! `super::state_election`. This crate links no crate of the agent (Spec
//! 1), so the file is checked as TOML 1.0 and nothing more.

use super::{Class, Document, Engine, Function, Part, Placement, Requirement, Value64};

/// **The WeaverAgent commit whose grammar the file follows**, a constant of
/// the build and Spec 2.4's staleness rule: a row saved under another pin is
/// named stale on its page.
pub const CORPUS_COMMIT: &str = "5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291";

/// The row a file is derived from, as its header names it. The grammar the
/// header names is this build's, [`CORPUS_COMMIT`], the one the file is
/// written in, whatever pin the row was saved under: a stale row's file is
/// this build's rendering of it, and its page names the row's own pin beside
/// it (Spec 6.1).
pub struct Meta<'a> {
    pub de: &'a str,
    pub version: i64,
}

/// **The agent's name for each tunable value**, in `tunable-values`.
pub(crate) fn tunable(member: &str) -> &'static str {
    match member {
        "temperature" => "temperature",
        "top_k" => "top-k",
        "top_p" => "top-p",
        "seed" => "seed",
        "repetition_penalty" => "repetition-penalty",
        "repetition_window" => "repetition-window",
        "max_tokens_per_turn" => "max-tokens-per-turn",
        "context_capacity" => "context-capacity",
        _ => "unknown",
    }
}

/// A TOML basic string. The document's texts hold no control character
/// (Spec 6.1), so the backslash and the quote are all there is to escape.
fn string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// **A real as TOML writes a float**, always with its point or its
/// exponent, so that TOML reads it as a float (Spec 6.1). Rust's `{}` writes
/// a whole value at or above 1e15 as its digits alone, which TOML reads as
/// an integer, or refuses past its range, so that value takes its exponent.
fn real(x: f64) -> String {
    if x.fract() != 0.0 {
        format!("{x}")
    } else if x.abs() < 1e15 {
        format!("{x:.1}")
    } else {
        format!("{x:e}")
    }
}

fn requirement_words(requirement: &Requirement) -> String {
    match requirement {
        Requirement::Card(card) => format!("the card {}", string(card)),
        Requirement::Class(class) => class_words(class),
        Requirement::Pair(parts) => format!(
            "a pair in shard order, {} then {}",
            part_words(&parts[0]),
            part_words(&parts[1])
        ),
    }
}

fn part_words(part: &Part) -> String {
    match part {
        Part::Card(card) => format!("the card {}", string(card)),
        Part::Class(class) => class_words(class),
    }
}

fn class_words(class: &Class) -> String {
    match class {
        Class::Family(family) => format!("any card of the family {}", string(family)),
        Class::Models(models) => format!(
            "any card of the models {}",
            models
                .iter()
                .map(|m| string(m))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// **The agent's file**, derived from the document.
pub fn agent_toml(document: &Document, meta: &Meta<'_>) -> String {
    let mut out = String::new();
    let line = |out: &mut String, text: &str| {
        out.push_str(text);
        out.push('\n');
    };
    line(
        &mut out,
        &format!(
            "# agent.toml, derived by weaver-web from the declaration {} version {} ({}).",
            document.name, meta.version, meta.de
        ),
    );
    line(
        &mut out,
        &format!("# The grammar is weaver-types-Spec section 2 at WeaverAgent {CORPUS_COMMIT}."),
    );
    line(
        &mut out,
        "# Derived, never edited: the declaration is the source.",
    );
    line(&mut out, "");
    line(
        &mut out,
        "# session: filled at Provision, the session the agent opens under.",
    );
    line(
        &mut out,
        "# trace-sink: filled at Provision, the sink admin opens at load.",
    );
    let tools = document
        .tools
        .iter()
        .map(|t| string(t))
        .collect::<Vec<_>>()
        .join(", ");
    line(&mut out, &format!("tool-set = [{tools}]"));
    match document.turn_context {
        Some(turn_context) => line(
            &mut out,
            &format!("turn-context = {}", string(turn_context.word())),
        ),
        None => line(
            &mut out,
            "# turn-context: not declared on this row; the agent reads its absence as continuous.",
        ),
    }
    line(&mut out, "");
    line(&mut out, "[state-store]");
    line(
        &mut out,
        &format!("engine = {}", string(document.store.word())),
    );
    line(&mut out, "");
    match (&document.state_election, document.store) {
        (Some(election), _) => {
            line(&mut out, "[state-election]");
            line(&mut out, &format!("all-kinds = {}", election.all_kinds));
            if election.keys.is_empty() {
                line(&mut out, "keys = []");
            } else {
                line(&mut out, "keys = [");
                for k in &election.keys {
                    let paths = k
                        .paths
                        .iter()
                        .map(|p| string(p))
                        .collect::<Vec<_>>()
                        .join(", ");
                    line(
                        &mut out,
                        &format!("  {{ kind = {}, paths = [{paths}] }},", string(&k.kind)),
                    );
                }
                line(&mut out, "]");
            }
        }
        (None, Engine::None) => line(
            &mut out,
            "# [state-election]: not written beside the store none, which admin refuses an election beside.",
        ),
        (None, Engine::Sqlite) => line(
            &mut out,
            "# [state-election]: not declared on this row; admin resolves its absence to the default election, the envelope of every kind.",
        ),
    }
    line(&mut out, "");
    line(
        &mut out,
        "# [gate-instruction] access-rule: filled at Provision, its gate-con's admission.",
    );

    // The decoder and the classifier, wherever their boxes hold them.
    let mut placed = Vec::new();
    for (i, b) in document.boxes.iter().enumerate() {
        for m in &b.models {
            placed.push((i, m));
        }
    }
    if let Some((i, decoder)) = placed.iter().find(|(_, m)| m.function == Function::Decoder) {
        line(&mut out, "");
        line(&mut out, "[spu-instruction.decoder]");
        match document.residual_readout_election {
            Some(readout) => line(&mut out, &format!("residual-readout-election = {readout}")),
            None => line(
                &mut out,
                "# residual-readout-election: not declared on this row: a serving load refuses this file, and the row's save is refused until it is chosen.",
            ),
        }
        match document.surprisal_election {
            Some(surprisal) => line(&mut out, &format!("surprisal-election = {surprisal}")),
            None => line(
                &mut out,
                "# surprisal-election: not declared on this row; the agent reads its absence as false.",
            ),
        }
        let values = document
            .sampler
            .present()
            .into_iter()
            .map(|(name, value)| {
                format!(
                    "{} = {}",
                    tunable(name),
                    match value {
                        Value64::Real(x) => real(x),
                        // At most `i64::MAX`, TOML's largest integer: the
                        // parse refuses a count above it.
                        Value64::Count(x) => x.to_string(),
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        if values.is_empty() {
            line(&mut out, "tunable-values = {}");
        } else {
            line(&mut out, &format!("tunable-values = {{ {values} }}"));
        }
        // A row saved before the eight were required (Spec 2.4): the file
        // the load would refuse, each name it lacks said.
        let missing = document.sampler.missing();
        if !missing.is_empty() {
            line(
                &mut out,
                &format!(
                    "# tunable-values lacks {}: a serving load refuses this file, and the row's save is refused until each is filled.",
                    missing
                        .iter()
                        .map(|m| tunable(m))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
        line(&mut out, "");
        line(&mut out, "[spu-instruction.decoder.model-binding]");
        line(
            &mut out,
            &format!("artifact = {}", string(&decoder.artifact)),
        );
        line(
            &mut out,
            &format!("# devices: filled at start from boxes[{i}]'s GPU requirement."),
        );
        if let Some(a) = &decoder.adapter {
            line(&mut out, "");
            line(&mut out, "[spu-instruction.decoder.model-binding.adapter]");
            line(&mut out, &format!("artifact = {}", string(&a.artifact)));
            line(&mut out, &format!("sha256 = {}", string(&a.sha256)));
            line(
                &mut out,
                &format!("base-blake3 = {}", string(&a.base_blake3)),
            );
            line(&mut out, &format!("scale = {}", real(a.scale)));
        }
    }
    if let Some((i, classifier)) = placed
        .iter()
        .find(|(_, m)| m.function == Function::Classifier)
    {
        line(&mut out, "");
        line(&mut out, "[spu-instruction.classify.model-binding]");
        line(
            &mut out,
            &format!("artifact = {}", string(&classifier.artifact)),
        );
        line(
            &mut out,
            &format!("# devices: filled at start from boxes[{i}]'s GPU requirement."),
        );
    }

    line(&mut out, "");
    line(
        &mut out,
        "# What the agent's grammar has no place for (interface question 1):",
    );
    match &document.placement {
        Placement::Host => line(&mut out, "# placement: a host agent."),
        Placement::VirtualMachine { template } => line(
            &mut out,
            &format!(
                "# placement: a virtual-machine agent, cloned from the template {}.",
                string(template)
            ),
        ),
    }
    for (i, b) in document.boxes.iter().enumerate() {
        let holds = b
            .models
            .iter()
            .map(|m| m.function.word())
            .collect::<Vec<_>>()
            .join(" and ");
        let box_words = if i == 0 {
            format!("# boxes[0], holding the {holds}")
        } else {
            format!("# boxes[{i}], a box of its own, holding the {holds}")
        };
        line(
            &mut out,
            &format!(
                "{box_words}: GPU requirement {}.",
                requirement_words(&b.gpu)
            ),
        );
    }
    line(
        &mut out,
        "# the sampler's mechanism and every other setting fixed at load: shown once an agent reports them (question 5).",
    );
    line(&mut out, "");
    line(
        &mut out,
        "# What the agent's grammar holds and this screen does not declare:",
    );
    line(
        &mut out,
        "# binding-kind: not declared on this screen; the agent reads its absence as serving, and every declaration this screen saves is serving.",
    );
    line(
        &mut out,
        "# [spu-instruction.decoder] field-election: not declared on this screen; its absence is the ordinary posture, no probability field elected, and the agent reads no default into it.",
    );
    out
}

/// **The prompt pane's text**: the prompt the declaration act will write
/// into declared state, or none.
pub fn prompt(document: &Document) -> Option<&str> {
    document.prompt.as_deref()
}
