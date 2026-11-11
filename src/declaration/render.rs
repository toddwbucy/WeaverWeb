//! conforms: web-a-save-and-a-reopen-are-the-same-document
//!
//! **The agent's file a declaration becomes** (Spec 6.1): `agent.toml` in
//! the agent's grammar, `weaver-types-Spec` section 2 at [`CORPUS_COMMIT`],
//! derived from the saved document and never stored. What the document
//! places is written as the grammar's members. A member the grammar requires
//! and the screen does not declare is a comment naming what fills it. A
//! trailing block of comments lists what the grammar has no place for, each
//! naming its interface question.
//!
//! **The member names are the agent's**, read at the pin from
//! `crates/weaver-types/src/config.rs` (`AgentConfig`, kebab-case, closed
//! objects) and, for `tunable-values`, from the SPU's own parameter names in
//! `crates/weaver-spu/src/sampling.rs`. This crate links no crate of the
//! agent (Spec 1), so the file is checked as TOML 1.0 and nothing more.

use super::{Class, Document, Function, Part, Placement, Requirement, Value64};

/// **The WeaverAgent commit whose grammar the file follows**, a constant of
/// the build and Spec 2.4's staleness rule: a row saved under another pin is
/// named stale on its page.
pub const CORPUS_COMMIT: &str = "a3543b31f264b269f66ab55d8767f25290ec9d61";

/// The row a file is derived from, as its header names it.
pub struct Meta<'a> {
    pub de: &'a str,
    pub version: i64,
    pub corpus_commit: &'a str,
}

/// The SPU's name for each sampler member, in `tunable-values`.
fn tunable(member: &str) -> &'static str {
    match member {
        "temperature" => "temperature",
        "top_k" => "top-k",
        "top_p" => "top-p",
        "seed" => "seed",
        "repetition_penalty" => "repetition-penalty",
        "repetition_window" => "repetition-window",
        "max_tokens_per_turn" => "max-tokens-per-turn",
        _ => "unknown",
    }
}

/// A TOML basic string. The document's texts hold no control character
/// (Spec 6.1), so the backslash and the quote are all there is to escape.
fn string(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// A real as TOML writes a float, always with its point.
fn real(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        format!("{x:.1}")
    } else {
        format!("{x}")
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
        &format!(
            "# The grammar is weaver-types-Spec section 2 at WeaverAgent {}.",
            meta.corpus_commit
        ),
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
    line(&mut out, "");
    line(&mut out, "[state-store]");
    line(
        &mut out,
        &format!("engine = {}", string(document.store.word())),
    );
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
        line(
            &mut out,
            "# residual-readout-election: not declared on this screen.",
        );
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
        "# context-capacity and every other setting fixed at load: not declared on this screen (question 5).",
    );
    out
}

/// **The prompt pane's text**: the prompt the declaration act will write
/// into declared state, or none.
pub fn prompt(document: &Document) -> Option<&str> {
    document.prompt.as_deref()
}
