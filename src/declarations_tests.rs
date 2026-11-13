//! **The declaration screen** (`src/surfaces/declarations.rs`,
//! `src/store/declarations.rs`, `src/declaration/`; Spec 2.4 and 6.1,
//! section 9's rows for them): every refusal by its field and reason, and
//! nothing saved after one; the one warning, and the word for an unknown;
//! the capability table against the vendor's page as read; every tunable
//! value required and judged on the agent's range, the five starting values,
//! and a stale row lacking one reopening and saving once filled; a save and a
//! reopen the same document, the agent's file TOML; the head, the name and
//! the authored half held by the screen and the store both; the admin grant
//! alone reading or writing; and no frame to either connector. Each
//! DB-backed test runs on a database of its own. No name, card, artifact or
//! digest here is a real deployment's.

use std::time::{Duration, Instant};

use axum::http::{HeaderMap, StatusCode, header};
use serde_json::{Value, json};

use crate::admin_tests::{admin, form, send_as};
use crate::declaration::capability::{self, READ, Rule, TABLE};
use crate::declaration::render::{self, CORPUS_COMMIT, Meta};
use crate::declaration::{Document, Form, Kind};
use crate::passkeys_tests::{app, authenticator, passkeys};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::declarations::{Under, insert_row};
use crate::store::read::tests::fresh_store;
use crate::verbs_tests::{dial_gate, operated};

/// **The eight tunable values of a whole document**, each in range (Spec
/// 6.1). The seed is 1, so a save setting it to 7 moves it.
fn tunables() -> Value {
    json!({
        "temperature": 0.7,
        "top_k": 40,
        "top_p": 0.95,
        "seed": 1,
        "repetition_penalty": 1.1,
        "repetition_window": 64,
        "max_tokens_per_turn": 512,
        "context_capacity": 4096,
    })
}

/// A document every check admits, a save's among them: one box, one
/// decoder on a named card, the eight tunable values and the three
/// elections, the surprisal's unchecked.
fn base() -> Value {
    json!({
        "name": "karl",
        "placement": {"kind": "host"},
        "boxes": [{
            "models": [{"function": "decoder", "artifact": "decoder-a", "format": "gguf"}],
            "gpu": {"card": "card-0"},
        }],
        "sampler": tunables(),
        "store": "none",
        "gate": {"tools": []},
        "prompt": null,
        "turn_context": "continuous",
        "residual_readout_election": false,
        "surprisal_election": false,
    })
}

/// The base document edited.
fn edit(f: impl FnOnce(&mut Value)) -> Value {
    let mut v = base();
    f(&mut v);
    v
}

/// The base document's form, as the screen posts it.
const BASE_FORM: [(&str, &str); 18] = [
    ("name", "karl"),
    ("placement", "host"),
    ("b0.m0.function", "decoder"),
    ("b0.m0.artifact", "decoder-a"),
    ("b0.m0.format", "gguf"),
    ("b0.gpu1.kind", "card"),
    ("b0.gpu1.value", "card-0"),
    ("sampler.temperature", "0.7"),
    ("sampler.top_k", "40"),
    ("sampler.top_p", "0.95"),
    ("sampler.seed", "1"),
    ("sampler.repetition_penalty", "1.1"),
    ("sampler.repetition_window", "64"),
    ("sampler.max_tokens_per_turn", "512"),
    ("sampler.context_capacity", "4096"),
    ("store", "none"),
    ("turn_context", "continuous"),
    ("residual_readout_election", "false"),
];

/// The base form with `set` replacing or adding fields and `drop` leaving
/// fields out.
fn posted(set: &[(&str, &str)], drop: &[&str]) -> String {
    let mut fields: Vec<(&str, &str)> = BASE_FORM
        .iter()
        .filter(|(k, _)| !drop.contains(k) && !set.iter().any(|(s, _)| s == k))
        .copied()
        .collect();
    fields.extend_from_slice(set);
    form(&fields)
}

async fn count(s: &Store, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The row a save's answer sends the browser to.
fn saved_at(status: StatusCode, headers: &HeaderMap, body: &str) -> String {
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    headers[header::LOCATION]
        .to_str()
        .unwrap()
        .strip_prefix("/declarations/")
        .unwrap()
        .to_owned()
}

/// A save posted and its new row's identity.
async fn save(app: &axum::Router, session: &str, uri: &str, body: String) -> String {
    let (status, headers, answer) = send_as(app, "POST", uri, Some(body), session).await;
    saved_at(status, &headers, &answer)
}

/// The latest session of a person, for a direct call to the store.
async fn session_id(s: &Store, person: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT session_id FROM session WHERE person_id = $1 ORDER BY session_id DESC LIMIT 1",
    )
    .bind(person)
    .fetch_one(&s.pool)
    .await
    .unwrap()
}

/// HTML's character references undone, numeric and named, as the
/// templates write them.
fn unescape(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else { break };
        let name = &rest[1..end];
        let decoded = match name {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => name
                .strip_prefix("#x")
                .map(|hex| u32::from_str_radix(hex, 16))
                .or_else(|| name.strip_prefix('#').map(str::parse::<u32>))
                .and_then(Result::ok)
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// An attribute's value in a tag's text, unescaped.
fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let end = tag[at..].find('"')? + at;
    Some(unescape(&tag[at..end]))
}

/// A text whose tag HTML drops one leading newline after, as a browser
/// reads it.
fn after_tag(text: &str) -> String {
    unescape(text.strip_prefix('\n').unwrap_or(text))
}

/// **What the page's declaration form posts**, as a browser would: every
/// named input, a checkbox only where checked; each select's selected
/// option, its first where none is; and each textarea's text.
fn posted_by(page: &str) -> Vec<(String, String)> {
    let start = page
        .find("<form method=\"post\" action=\"/declarations")
        .expect("the page holds the declaration form");
    let end = page[start..].find("</form>").unwrap() + start;
    let mut rest = &page[start..end];
    let mut out = Vec::new();
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        let close = rest.find('>').unwrap();
        let tag = &rest[..=close];
        if tag.starts_with("<input") {
            if let Some(name) = attr(tag, "name") {
                let checkbox = attr(tag, "type").as_deref() == Some("checkbox");
                if !checkbox || tag.contains(" checked") {
                    out.push((name, attr(tag, "value").unwrap_or_default()));
                }
            }
        } else if tag.starts_with("<select") {
            let name = attr(tag, "name").unwrap();
            let end = rest.find("</select>").unwrap();
            let options: Vec<&str> = rest[..end].split("<option").skip(1).collect();
            let opening = |o: &&str| o[..o.find('>').unwrap()].to_owned();
            let chosen = options
                .iter()
                .find(|o| opening(o).contains(" selected"))
                .or(options.first())
                .unwrap();
            out.push((name, attr(&opening(chosen), "value").unwrap_or_default()));
            rest = &rest[end..];
            continue;
        } else if tag.starts_with("<textarea") {
            let name = attr(tag, "name").unwrap();
            let end = rest.find("</textarea>").unwrap();
            out.push((name, after_tag(&rest[close + 1..end])));
            rest = &rest[end..];
            continue;
        }
        rest = &rest[close + 1..];
    }
    out
}

/// The text of the page's `n`th `pre`, as a browser reads it.
fn pre(page: &str, n: usize) -> String {
    let mut rest = page;
    for _ in 0..n {
        rest = &rest[rest.find("<pre>").unwrap() + 5..];
    }
    let start = rest.find("<pre>").unwrap() + 5;
    let end = rest[start..].find("</pre>").unwrap() + start;
    after_tag(&rest[start..end])
}

/// **Every refusal of Spec 6.1, by its field and its reason**: one case
/// each, judged on the document, so a check dropped lets its case through
/// or refuses it for another reason, which the case's kind catches. A class
/// spanning capabilities is no case: it parses.
#[test]
fn every_refusal_names_its_field_and_its_reason() {
    let decoder = || json!({"function": "decoder", "artifact": "decoder-a", "format": "gguf"});
    let adapter = || {
        json!({
            "artifact": "adapter-a",
            "sha256": "0123456789abcdef".repeat(4),
            "base_blake3": "f".repeat(64),
            "scale": 0.5,
        })
    };
    let cases: Vec<(&str, Value, Kind, &str)> = vec![
        // A missing required field.
        (
            "no name",
            edit(|v| {
                v.as_object_mut().unwrap().remove("name");
            }),
            Kind::Missing,
            "name",
        ),
        (
            "an empty name",
            edit(|v| v["name"] = json!("")),
            Kind::Missing,
            "name",
        ),
        (
            "no placement",
            edit(|v| {
                v.as_object_mut().unwrap().remove("placement");
            }),
            Kind::Missing,
            "placement",
        ),
        (
            "a machine with no template",
            edit(|v| v["placement"] = json!({"kind": "virtual-machine"})),
            Kind::Missing,
            "placement.template",
        ),
        (
            "no box",
            edit(|v| v["boxes"] = json!([])),
            Kind::Missing,
            "boxes",
        ),
        (
            "a box with no model",
            edit(|v| v["boxes"][0]["models"] = json!([])),
            Kind::Missing,
            "boxes[0].models",
        ),
        (
            "a model with no function",
            edit(|v| {
                v["boxes"][0]["models"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("function");
            }),
            Kind::Missing,
            "boxes[0].models[0].function",
        ),
        (
            "a model with no artifact",
            edit(|v| v["boxes"][0]["models"][0]["artifact"] = json!("")),
            Kind::Missing,
            "boxes[0].models[0].artifact",
        ),
        (
            "a model with no format",
            edit(|v| {
                v["boxes"][0]["models"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("format");
            }),
            Kind::Missing,
            "boxes[0].models[0].format",
        ),
        (
            "a box with no requirement",
            edit(|v| {
                v["boxes"][0].as_object_mut().unwrap().remove("gpu");
            }),
            Kind::Missing,
            "boxes[0].gpu",
        ),
        (
            "no store",
            edit(|v| v["store"] = json!("")),
            Kind::Missing,
            "store",
        ),
        // A requirement that names nothing.
        (
            "a requirement naming nothing",
            edit(|v| v["boxes"][0]["gpu"] = json!({})),
            Kind::NamesNothing,
            "boxes[0].gpu",
        ),
        (
            "a card with no identifier",
            edit(|v| v["boxes"][0]["gpu"] = json!({"card": " "})),
            Kind::NamesNothing,
            "boxes[0].gpu.card",
        ),
        (
            "a class giving neither form",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {}})),
            Kind::NamesNothing,
            "boxes[0].gpu.class",
        ),
        (
            "a family with no name",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {"family": ""}})),
            Kind::NamesNothing,
            "boxes[0].gpu.class.family",
        ),
        (
            "a list with no model",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {"models": []}})),
            Kind::NamesNothing,
            "boxes[0].gpu.class.models",
        ),
        (
            "a model with no name",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {"models": ["NVIDIA A10", ""]}})),
            Kind::NamesNothing,
            "boxes[0].gpu.class.models[1]",
        ),
        (
            "a pair that is not two",
            edit(|v| v["boxes"][0]["gpu"] = json!({"pair": [{"card": "card-0"}]})),
            Kind::NamesNothing,
            "boxes[0].gpu.pair",
        ),
        (
            "a pair inside a pair",
            edit(|v| {
                v["boxes"][0]["gpu"] =
                    json!({"pair": [{"pair": [{"card": "a"}, {"card": "b"}]}, {"card": "c"}]})
            }),
            Kind::NamesNothing,
            "boxes[0].gpu.pair[0].pair",
        ),
        (
            "a list naming a model twice, case and spacing folded",
            edit(|v| {
                v["boxes"][0]["gpu"] = json!({"class": {"models": ["NVIDIA A10", "nvidia  a10"]}})
            }),
            Kind::NamesNothing,
            "boxes[0].gpu.class.models[1]",
        ),
        // A class giving both forms, and a requirement of more than one kind.
        (
            "a class giving both a family and a list",
            edit(|v| {
                v["boxes"][0]["gpu"] = json!({"class": {"family": "Ada", "models": ["NVIDIA L40"]}})
            }),
            Kind::BothForms,
            "boxes[0].gpu.class",
        ),
        (
            "a requirement naming a card and a class",
            edit(|v| v["boxes"][0]["gpu"] = json!({"card": "card-0", "class": {"family": "Ada"}})),
            Kind::Shape,
            "boxes[0].gpu",
        ),
        // A roster the framework would not load.
        (
            "no decoder",
            edit(|v| v["boxes"][0]["models"][0]["function"] = json!("classifier")),
            Kind::Roster,
            "boxes",
        ),
        (
            "a second decoder",
            edit(|v| v["boxes"][0]["models"] = json!([decoder(), decoder()])),
            Kind::Roster,
            "boxes[0].models[1].function",
        ),
        (
            "a second classifier",
            edit(|v| {
                let mut c = decoder();
                c["function"] = json!("classifier");
                v["boxes"][0]["models"] = json!([decoder(), c.clone(), c]);
            }),
            Kind::Roster,
            "boxes[0].models[2].function",
        ),
        (
            "a function the roster does not hold",
            edit(|v| v["boxes"][0]["models"][0]["function"] = json!("embedder")),
            Kind::Roster,
            "boxes[0].models[0].function",
        ),
        (
            "an adapter on a classifier",
            edit(|v| {
                let mut c = decoder();
                c["function"] = json!("classifier");
                c["adapter"] = adapter();
                v["boxes"][0]["models"] = json!([decoder(), c]);
            }),
            Kind::Roster,
            "boxes[0].models[1].adapter",
        ),
        // A format other than GGUF.
        (
            "a format other than GGUF",
            edit(|v| v["boxes"][0]["models"][0]["format"] = json!("safetensors")),
            Kind::Format,
            "boxes[0].models[0].format",
        ),
        // An adapter out of its shape.
        (
            // Its artifact, which no later check of the adapter's would
            // refuse by the adapter's kind.
            "an adapter missing a member",
            edit(|v| {
                let mut a = adapter();
                a.as_object_mut().unwrap().remove("artifact");
                v["boxes"][0]["models"][0]["adapter"] = a;
            }),
            Kind::Adapter,
            "boxes[0].models[0].adapter.artifact",
        ),
        (
            "an adapter digest of another shape",
            edit(|v| {
                let mut a = adapter();
                a["sha256"] = json!("0123456789ABCDEF".repeat(4));
                v["boxes"][0]["models"][0]["adapter"] = a;
            }),
            Kind::Adapter,
            "boxes[0].models[0].adapter.sha256",
        ),
        (
            "an adapter scale that is not a number",
            edit(|v| {
                let mut a = adapter();
                a["scale"] = json!("half");
                v["boxes"][0]["models"][0]["adapter"] = a;
            }),
            Kind::Adapter,
            "boxes[0].models[0].adapter.scale",
        ),
        (
            "a null adapter",
            edit(|v| v["boxes"][0]["models"][0]["adapter"] = Value::Null),
            Kind::Adapter,
            "boxes[0].models[0].adapter",
        ),
        // A name outside its grammar.
        (
            "a name outside the grammar",
            edit(|v| v["name"] = json!("karl agent")),
            Kind::Name,
            "name",
        ),
        // A control character in any text.
        (
            "a control character in an artifact",
            edit(|v| v["boxes"][0]["models"][0]["artifact"] = json!("decoder\u{7}a")),
            Kind::Control,
            "boxes[0].models[0].artifact",
        ),
        (
            "a control character in a card",
            edit(|v| v["boxes"][0]["gpu"] = json!({"card": "card\u{1b}0"})),
            Kind::Control,
            "boxes[0].gpu.card",
        ),
        (
            "a line feed in a template",
            edit(|v| v["placement"] = json!({"kind": "virtual-machine", "template": "t\na"})),
            Kind::Control,
            "placement.template",
        ),
        (
            "a carriage return in the prompt",
            edit(|v| v["prompt"] = json!("one\rtwo")),
            Kind::Control,
            "prompt",
        ),
        // A sampler value outside its kind.
        (
            "a real that is text",
            edit(|v| v["sampler"] = json!({"temperature": "hot"})),
            Kind::Sampler,
            "sampler.temperature",
        ),
        (
            "a count below zero",
            edit(|v| v["sampler"] = json!({"top_k": -1})),
            Kind::Sampler,
            "sampler.top_k",
        ),
        (
            "a count that is not whole",
            edit(|v| v["sampler"] = json!({"max_tokens_per_turn": 1.5})),
            Kind::Sampler,
            "sampler.max_tokens_per_turn",
        ),
        (
            "a count above the largest whole number the agent's file holds",
            edit(|v| v["sampler"] = json!({"seed": 9_223_372_036_854_775_808_u64})),
            Kind::Sampler,
            "sampler.seed",
        ),
        // A tool other than sh.
        (
            "a tool other than sh",
            edit(|v| v["gate"] = json!({"tools": ["bash"]})),
            Kind::Tool,
            "gate.tools[0]",
        ),
        (
            "sh twice",
            edit(|v| v["gate"] = json!({"tools": ["sh", "sh"]})),
            Kind::Tool,
            "gate.tools[1]",
        ),
        // An election outside its values.
        (
            "a turn context the agent does not hold",
            edit(|v| v["turn_context"] = json!("sometimes")),
            Kind::Election,
            "turn_context",
        ),
        (
            "a turn context that is not text",
            edit(|v| v["turn_context"] = json!(1)),
            Kind::Election,
            "turn_context",
        ),
        (
            "a null turn context",
            edit(|v| v["turn_context"] = Value::Null),
            Kind::Election,
            "turn_context",
        ),
        (
            "a residual readout election that is text",
            edit(|v| v["residual_readout_election"] = json!("false")),
            Kind::Election,
            "residual_readout_election",
        ),
        (
            "a null residual readout election",
            edit(|v| v["residual_readout_election"] = Value::Null),
            Kind::Election,
            "residual_readout_election",
        ),
        (
            "a surprisal election that is a number",
            edit(|v| v["surprisal_election"] = json!(1)),
            Kind::Election,
            "surprisal_election",
        ),
        // An unknown member.
        (
            "an unknown member at the top",
            edit(|v| v["colour"] = json!("red")),
            Kind::Unknown,
            "colour",
        ),
        (
            "an unknown member of a model",
            edit(|v| v["boxes"][0]["models"][0]["engine"] = json!("x")),
            Kind::Unknown,
            "boxes[0].models[0].engine",
        ),
        (
            "a template on a host agent",
            edit(|v| v["placement"] = json!({"kind": "host", "template": "t"})),
            Kind::Unknown,
            "placement.template",
        ),
        (
            "the trace declared",
            edit(|v| v["trace"] = json!({})),
            Kind::Unknown,
            "trace",
        ),
    ];
    for (what, value, kind, field) in cases {
        let refused = Document::parse(&value)
            .err()
            .unwrap_or_else(|| panic!("{what} was admitted"));
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (kind, field),
            "{what}: {refused}"
        );
    }

    // What is admitted: the base; a prompt holding a line feed and a tab; a
    // class spanning capabilities, by family and by list; a family and a
    // model the table does not know; a pair of a card and a class.
    for (what, value) in [
        ("the base", base()),
        (
            "a prompt with a line feed and a tab",
            edit(|v| v["prompt"] = json!("one\n\ttwo")),
        ),
        (
            "a family spanning capabilities",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {"family": "Ampere"}})),
        ),
        (
            "a list spanning capabilities",
            edit(|v| {
                v["boxes"][0]["gpu"] = json!({"class": {"models": ["NVIDIA A100", "NVIDIA H100"]}})
            }),
        ),
        (
            "an unknown family",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {"family": "Nonesuch"}})),
        ),
        (
            "an unknown model",
            edit(|v| v["boxes"][0]["gpu"] = json!({"class": {"models": ["card-model-x"]}})),
        ),
        (
            "a pair of a card and a class",
            edit(|v| {
                v["boxes"][0]["gpu"] =
                    json!({"pair": [{"card": "card-0"}, {"class": {"family": "Hopper"}}]})
            }),
        ),
        (
            "an independent turn context and both elections true",
            edit(|v| {
                v["turn_context"] = json!("independent");
                v["residual_readout_election"] = json!(true);
                v["surprisal_election"] = json!(true);
            }),
        ),
        (
            "a row saved before the elections",
            edit(|v| {
                let top = v.as_object_mut().unwrap();
                for member in [
                    "turn_context",
                    "residual_readout_election",
                    "surprisal_election",
                ] {
                    top.remove(member);
                }
            }),
        ),
    ] {
        let document = Document::parse(&value).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(
            Document::parse(&document.to_value()).unwrap(),
            document,
            "{what}: the canonical form reads back as itself"
        );
    }
}

/// **A class is judged in one pass over what was sent**, since a save's
/// parse runs before the session is read (Spec 6.1): a list of 20,000
/// distinct models is judged well inside its bound, and its first model
/// typed again at its end is refused naming that slot. The scan of every
/// earlier name this replaced took 101 s for 16,000 in the test build
/// (measured 2026-11-12).
#[test]
fn a_long_class_is_judged_in_one_pass() {
    const N: usize = 20_000;
    let mut models: Vec<String> = (0..N).map(|i| format!("model-{i}")).collect();
    let started = Instant::now();
    let document = Document::parse(&edit(|v| {
        v["boxes"][0]["gpu"] = json!({"class": {"models": models.clone()}})
    }));
    let took = started.elapsed();
    assert!(document.is_ok(), "{document:?}");
    assert!(
        took < Duration::from_secs(5),
        "a class of {N} models took {took:?}"
    );
    models.push("MODEL-0".to_owned());
    let refused = Document::parse(&edit(|v| {
        v["boxes"][0]["gpu"] = json!({"class": {"models": models}})
    }))
    .unwrap_err();
    assert_eq!(
        (refused.kind, refused.field.as_str()),
        (
            Kind::NamesNothing,
            format!("boxes[0].gpu.class.models[{N}]").as_str()
        )
    );
}

/// **The agent's file writes every admitted value in its TOML type**
/// (Spec 6.1): a whole real at or above 1e15, which Rust's `{}` writes as
/// digits alone, is a float, the adapter's scale too, and a count at the
/// largest whole number TOML holds is that integer.
#[test]
fn the_agents_file_writes_every_admitted_value_in_its_toml_type() {
    let document = Document::parse(&edit(|v| {
        v["boxes"][0]["models"][0]["adapter"] = json!({
            "artifact": "adapter-a",
            "sha256": "0123456789abcdef".repeat(4),
            "base_blake3": "f".repeat(64),
            "scale": 1e16,
        });
        v["sampler"] = json!({
            "temperature": 1e20,
            "top_p": 1e15,
            "repetition_penalty": -1e300,
            "seed": i64::MAX,
            "top_k": 0,
        });
    }))
    .unwrap();
    let file = render::agent_toml(
        &document,
        &Meta {
            de: "de-0000000000000000",
            version: 1,
        },
    );
    let table: toml::Table = toml::from_str(&file)
        .unwrap_or_else(|e| panic!("the agent's file is TOML 1.0: {e}\n{file}"));
    let decoder = &table["spu-instruction"]["decoder"];
    let values = &decoder["tunable-values"];
    assert_eq!(values["temperature"].as_float(), Some(1e20), "{file}");
    assert_eq!(values["top-p"].as_float(), Some(1e15), "{file}");
    assert_eq!(
        values["repetition-penalty"].as_float(),
        Some(-1e300),
        "{file}"
    );
    assert_eq!(values["seed"].as_integer(), Some(i64::MAX), "{file}");
    assert_eq!(values["top-k"].as_integer(), Some(0), "{file}");
    assert_eq!(
        decoder["model-binding"]["adapter"]["scale"].as_float(),
        Some(1e16),
        "{file}"
    );
    assert!(
        file.contains(&format!(
            "weaver-types-Spec section 2 at WeaverAgent {CORPUS_COMMIT}."
        )),
        "the header names this build's pin: {file}"
    );
}

/// **The screen refuses a field out of shape and saves nothing**, end to
/// end, for every refusal its form can carry: 400, the refusal naming the
/// field by the screen's slot, and neither a row nor an audit record
/// written. The form posts what the screen holds; a class giving both
/// forms and a pair inside a pair are the document's cases above, since no
/// field of the form can give them.
#[tokio::test]
async fn the_screen_refuses_a_field_out_of_shape_and_saves_nothing() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let cases: Vec<(&str, String, &str)> = vec![
        ("no name", posted(&[], &["name"]), "name: is required"),
        (
            "no placement",
            posted(&[], &["placement"]),
            "placement: is required",
        ),
        (
            "a machine with no template",
            posted(&[("placement", "virtual-machine")], &[]),
            "placement.template: is required",
        ),
        (
            "no box",
            posted(
                &[],
                &[
                    "b0.m0.function",
                    "b0.m0.artifact",
                    "b0.m0.format",
                    "b0.gpu1.kind",
                    "b0.gpu1.value",
                ],
            ),
            "boxes: is required",
        ),
        (
            "a box with no requirement",
            posted(&[], &["b0.gpu1.kind", "b0.gpu1.value"]),
            "boxes[0].gpu: is required",
        ),
        (
            "a requirement's kind with no text",
            posted(&[("b0.gpu1.value", "")], &[]),
            "boxes[0].gpu.card: is a card with no identifier",
        ),
        (
            "text with no kind",
            posted(&[("b0.gpu1.kind", "")], &[]),
            "boxes[0].gpu: is required",
        ),
        (
            "a list naming a model twice",
            posted(
                &[
                    ("b0.gpu1.kind", "models"),
                    ("b0.gpu1.value", "NVIDIA A10, NVIDIA A10"),
                ],
                &[],
            ),
            "boxes[0].gpu.class.models[1]: names a model",
        ),
        (
            "a second requirement alone",
            posted(
                &[("b0.gpu2.kind", "card"), ("b0.gpu2.value", "card-1")],
                &["b0.gpu1.kind", "b0.gpu1.value"],
            ),
            "boxes[0].gpu.pair[0]: names nothing",
        ),
        (
            "no decoder",
            posted(&[("b0.m0.function", "classifier")], &[]),
            "boxes: declare no decoder",
        ),
        (
            "a second decoder, in a later slot",
            posted(
                &[
                    ("b0.m3.function", "decoder"),
                    ("b0.m3.artifact", "decoder-b"),
                ],
                &[],
            ),
            "boxes[0].models[3].function: is a second decoder",
        ),
        (
            "a format other than GGUF",
            posted(&[("b0.m0.format", "safetensors")], &[]),
            "boxes[0].models[0].format: is GGUF",
        ),
        (
            "an adapter missing a member",
            posted(&[("b0.m0.adapter.artifact", "adapter-a")], &[]),
            "boxes[0].models[0].adapter.sha256: is missing",
        ),
        (
            "an adapter scale that is not a number",
            posted(&[("b0.m0.adapter.scale", "half")], &[]),
            "boxes[0].models[0].adapter.scale: is a finite number",
        ),
        (
            "a name outside the grammar",
            posted(&[("name", "karl agent")], &[]),
            "name: is letters, digits",
        ),
        (
            "a control character",
            posted(&[("b0.m0.artifact", "decoder\u{7}a")], &[]),
            "boxes[0].models[0].artifact: holds a control character",
        ),
        (
            "a sampler value outside its kind",
            posted(&[("sampler.top_k", "-1")], &[]),
            "sampler.top_k: is a count",
        ),
        (
            "a tool other than sh",
            posted(&[("tools", "bash")], &[]),
            "gate.tools: is a tool other than sh",
        ),
        (
            "a turn context the agent does not hold",
            posted(&[("turn_context", "sometimes")], &[]),
            "turn_context: is continuous or independent",
        ),
        (
            "a residual readout election neither true nor false",
            posted(&[("residual_readout_election", "maybe")], &[]),
            "residual_readout_election: is true or false",
        ),
        (
            "a surprisal election other than its box checked",
            posted(&[("surprisal_election", "yes")], &[]),
            "surprisal_election: is true, its box checked, or absent",
        ),
        (
            "an unknown member",
            posted(&[("colour", "red")], &[]),
            "colour: is not a field",
        ),
        (
            "a box past the bound",
            posted(&[("b16.m0.artifact", "x")], &[]),
            "b16.m0.artifact: is not a field",
        ),
    ];
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    for (what, body, words) in cases {
        let (status, _, page) = send_as(&app, "POST", "/declarations", Some(body), &session).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{what}: {page}");
        assert!(
            page.contains(&words.replace('\'', "&#39;")) || page.contains(words),
            "{what}: the refusal names its field: {page}"
        );
        assert_eq!(count(s, "declaration").await, rows, "{what} saved a row");
        assert_eq!(count(s, "audit").await, audit, "{what} wrote a record");
    }
    // A form the screen refuses is shown again for the admin, with what
    // was typed.
    let (_, _, page) = send_as(
        &app,
        "POST",
        "/declarations",
        Some(posted(
            &[("sampler.top_k", "-1"), ("b0.m0.artifact", "decoder-typed")],
            &[],
        )),
        &session,
    )
    .await;
    assert!(page.contains("value=\"decoder-typed\""), "{page}");
    crate::surfaces::assert_framed(&page);

    // A class spanning capabilities is no refusal: it saves.
    save(
        &app,
        &session,
        "/declarations",
        posted(
            &[("b0.gpu1.kind", "family"), ("b0.gpu1.value", "Ampere")],
            &[],
        ),
    )
    .await;
    assert_eq!(count(s, "declaration").await, rows + 1);
}

/// **A class spanning capabilities is warned of, and an unknown is named
/// unknown and admitted**: a family the table lists under two capabilities
/// (typed in another case), a list of two capabilities and an unknown
/// model, and a family the table does not know, each saved, each class
/// kept in the form typed, and the page naming the span and the unknowns in
/// words, never a blank.
#[tokio::test]
async fn a_class_spanning_capabilities_is_warned_of_and_an_unknown_is_named() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let de = save(
        &app,
        &session,
        "/declarations",
        posted(
            &[
                ("b0.gpu1.kind", "family"),
                ("b0.gpu1.value", "ampere"),
                ("b1.m0.function", "classifier"),
                ("b1.m0.artifact", "classifier-a"),
                ("b1.gpu1.kind", "models"),
                ("b1.gpu1.value", "NVIDIA A100, NVIDIA H100, card-model-x"),
                ("b1.gpu2.kind", "family"),
                ("b1.gpu2.value", "Nonesuch"),
            ],
            &[],
        ),
    )
    .await;
    let row = s.declaration(&de).await.unwrap().unwrap();
    assert_eq!(
        row.body["boxes"][0]["gpu"],
        json!({"class": {"family": "ampere"}})
    );
    assert_eq!(
        row.body["boxes"][1]["gpu"],
        json!({"pair": [
            {"class": {"models": ["NVIDIA A100", "NVIDIA H100", "card-model-x"]}},
            {"class": {"family": "Nonesuch"}},
        ]})
    );
    let readings = Document::parse(&row.body).unwrap().readings();
    assert_eq!(readings.len(), 3);
    assert!(readings[0].spans() && readings[0].form == "family" && readings[0].family_known);
    assert!(readings[1].spans() && readings[1].form == "list");
    assert_eq!(readings[1].models[2], ("card-model-x".to_owned(), None));
    assert!(!readings[2].spans() && !readings[2].family_known && readings[2].models.is_empty());

    let (status, _, page) =
        send_as(&app, "GET", &format!("/declarations/{de}"), None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    crate::surfaces::assert_framed(&page);
    assert_eq!(
        page.matches("spans 2 compute capabilities").count(),
        2,
        "{page}"
    );
    assert!(page.contains("card-model-x, capability unknown"), "{page}");
    assert!(
        page.contains("not a family the table knows: capability unknown"),
        "{page}"
    );
    assert!(
        page.contains("NVIDIA A100, compute capability 8.0"),
        "{page}"
    );
}

/// **The capability table is the vendor's page as read, and every row
/// cites its source**: each model and its capability, in the page's order,
/// against the read kept beside the table; each capability's source and
/// each family's a URL; a family given by statement cites a compatibility
/// guide, and one given by the model's name holds that name.
#[test]
fn the_capability_table_is_the_page_as_read_and_every_row_cites_its_source() {
    let mut read = Vec::new();
    let mut capability = None;
    for line in READ
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        if line.split('.').count() == 2 && line.split('.').all(|p| p.parse::<u8>().is_ok()) {
            capability = Some(line);
        } else {
            read.push((line, capability.expect("a model follows its capability")));
        }
    }
    let table: Vec<(&str, &str)> = TABLE.iter().map(|c| (c.model, c.capability)).collect();
    assert_eq!(table, read, "the table is the page as read, row for row");
    for card in TABLE {
        assert!(
            card.capability_source.starts_with("https://"),
            "{}",
            card.model
        );
        if let Some(family) = card.family {
            assert!(family.source.starts_with("https://"), "{}", card.model);
            match family.rule {
                Rule::Statement => assert!(
                    family.source.contains("compatibility-guide") && family.source.contains('#'),
                    "{}: a statement cites the guide's passage",
                    card.model
                ),
                Rule::Name => assert!(card.model.contains(family.name), "{}", card.model),
            }
        }
    }
    // Lookups fold case and spacing, and a family stands for its rows.
    assert_eq!(
        capability::model("nvidia   a100").map(|c| c.capability),
        Some("8.0")
    );
    assert!(capability::family("AMPERE").len() > 1);
    assert!(capability::family("Nonesuch").is_empty());
}

/// **A save and a reopen are the same document, and the agent's file is
/// derived from it**: a document holding every member saved; the row's
/// body its canonical form; the reopened page's form, posted as a browser
/// would post it, the same document again with nothing moved; both panes
/// what the document derives; and the file TOML, naming the pinned corpus
/// commit and writing each placeless member in its trailing block.
#[tokio::test]
async fn a_save_and_a_reopen_are_the_same_document() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let sha = "0123456789abcdef".repeat(4);
    let blake = "f".repeat(64);
    let prompt = "\nYou are\ta test & <nothing> \"else\".\nLine two.";
    let fields: Vec<(&str, &str)> = vec![
        ("name", "karl-7"),
        ("placement", "virtual-machine"),
        ("template", "template-a"),
        ("b0.m0.function", "decoder"),
        ("b0.m0.artifact", "decoder-a"),
        ("b0.m0.format", "gguf"),
        ("b0.m0.adapter.artifact", "adapter-a"),
        ("b0.m0.adapter.sha256", &sha),
        ("b0.m0.adapter.base_blake3", &blake),
        ("b0.m0.adapter.scale", "0.5"),
        ("b0.gpu1.kind", "card"),
        ("b0.gpu1.value", "card-0"),
        ("b0.gpu2.kind", "family"),
        ("b0.gpu2.value", "Hopper"),
        ("b1.m0.function", "classifier"),
        ("b1.m0.artifact", "classifier-a"),
        ("b1.m0.format", "gguf"),
        ("b1.gpu1.kind", "models"),
        ("b1.gpu1.value", "NVIDIA A10, NVIDIA L40S"),
        ("sampler.temperature", "0.7"),
        ("sampler.top_k", "40"),
        ("sampler.top_p", "0.95"),
        ("sampler.seed", "7"),
        ("sampler.repetition_penalty", "1.1"),
        ("sampler.repetition_window", "64"),
        ("sampler.max_tokens_per_turn", "512"),
        ("sampler.context_capacity", "32768"),
        ("store", "sqlite"),
        ("tools", "sh"),
        ("prompt", prompt),
        ("turn_context", "independent"),
        ("residual_readout_election", "true"),
        ("surprisal_election", "true"),
    ];
    let expected = Form::read(form(&fields).as_bytes())
        .unwrap()
        .document()
        .unwrap();
    assert_eq!(expected.prompt.as_deref(), Some(prompt));
    let de = save(&app, &session, "/declarations", form(&fields)).await;
    let row = s.declaration(&de).await.unwrap().unwrap();
    assert_eq!(
        row.body,
        expected.to_value(),
        "the row holds the canonical form"
    );
    assert_eq!(
        (row.version, row.head, row.corpus_commit.as_str()),
        (1, true, CORPUS_COMMIT)
    );

    // The reopen: the page's form, as a browser posts it, is the document.
    let (status, _, page) =
        send_as(&app, "GET", &format!("/declarations/{de}"), None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    crate::surfaces::assert_framed(&page);
    let reposted = posted_by(&page);
    let reposted: Vec<(&str, &str)> = reposted
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let reopened = Form::read(form(&reposted).as_bytes())
        .unwrap()
        .document()
        .unwrap();
    assert_eq!(
        reopened, expected,
        "the reopened form is the saved document"
    );

    // Both panes are what the document derives.
    let file = render::agent_toml(
        &expected,
        &Meta {
            de: &de,
            version: 1,
        },
    );
    assert_eq!(pre(&page, 0), file);
    assert_eq!(pre(&page, 1), prompt);

    // Saved again from the reopen, unchanged: a version with nothing moved.
    let again = save(
        &app,
        &session,
        &format!("/declarations/{de}"),
        form(&reposted),
    )
    .await;
    let row2 = s.declaration(&again).await.unwrap().unwrap();
    assert_eq!(row2.body, row.body);
    assert_eq!((row2.version, row2.what_moved.len()), (2, 0));
    assert_eq!(row2.parent.as_deref(), Some(de.as_str()));

    // The file is TOML, names the pin and writes the placeless members as
    // its trailing comments.
    let table: toml::Table = toml::from_str(&file).expect("the agent's file is TOML 1.0");
    assert_eq!(table["tool-set"], toml::Value::Array(vec!["sh".into()]));
    assert_eq!(table["state-store"]["engine"].as_str(), Some("sqlite"));
    assert_eq!(table["turn-context"].as_str(), Some("independent"));
    let decoder = &table["spu-instruction"]["decoder"];
    assert_eq!(decoder["residual-readout-election"].as_bool(), Some(true));
    assert_eq!(decoder["surprisal-election"].as_bool(), Some(true));
    // What the screen does not declare is named and never written.
    assert!(
        !table.contains_key("binding-kind")
            && !table.contains_key("state-election")
            && decoder.get("field-election").is_none(),
        "{file}"
    );
    let undeclared = &file[file
        .find("# What the agent's grammar holds and this screen does not declare")
        .unwrap_or_else(|| panic!("the file names what the screen does not declare: {file}"))..];
    for words in [
        "# binding-kind: not declared on this screen; the agent reads its absence as serving, and every declaration this screen saves is serving.",
        "# [state-election]: not declared on this screen; admin resolves its absence to the default election, the envelope of every kind.",
        "# [spu-instruction.decoder] field-election: not declared on this screen; its absence is the ordinary posture, no probability field elected, and the agent reads no default into it.",
    ] {
        assert!(undeclared.contains(words), "{words}\n{file}");
    }
    // The eight tunable values, every one, under the agent's names.
    let values = decoder["tunable-values"].as_table().unwrap();
    let mut names: Vec<&str> = values.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "context-capacity",
            "max-tokens-per-turn",
            "repetition-penalty",
            "repetition-window",
            "seed",
            "temperature",
            "top-k",
            "top-p",
        ]
    );
    assert_eq!(values["top-k"].as_integer(), Some(40));
    assert_eq!(values["temperature"].as_float(), Some(0.7));
    assert_eq!(values["context-capacity"].as_integer(), Some(32768));
    assert!(!file.contains("tunable-values lacks"), "{file}");
    let binding = &decoder["model-binding"];
    assert_eq!(binding["artifact"].as_str(), Some("decoder-a"));
    assert_eq!(binding["adapter"]["sha256"].as_str(), Some(sha.as_str()));
    assert_eq!(
        binding["adapter"]["base-blake3"].as_str(),
        Some(blake.as_str())
    );
    assert_eq!(binding["adapter"]["scale"].as_float(), Some(0.5));
    assert_eq!(
        table["spu-instruction"]["classify"]["model-binding"]["artifact"].as_str(),
        Some("classifier-a")
    );
    assert!(file.contains(CORPUS_COMMIT));
    let trailing = &file[file
        .find("# What the agent's grammar has no place for")
        .unwrap()..];
    for words in [
        "# placement: a virtual-machine agent, cloned from the template \"template-a\".",
        "# boxes[0], holding the decoder: GPU requirement a pair in shard order, the card \"card-0\" then any card of the family \"Hopper\".",
        "# boxes[1], a box of its own, holding the classifier: GPU requirement any card of the models \"NVIDIA A10\", \"NVIDIA L40S\".",
    ] {
        assert!(trailing.contains(words), "{words}\n{file}");
    }
    // A member dropped on the save shows: the reopen is no longer the
    // document.
    for dropped in ["tools", "surprisal_election", "turn_context"] {
        let mut fewer = reposted.clone();
        fewer.retain(|(k, _)| *k != dropped);
        assert_ne!(
            Form::read(form(&fewer).as_bytes())
                .unwrap()
                .document()
                .unwrap(),
            expected,
            "{dropped} dropped"
        );
    }
}

/// **A save from a row that is not its lineage's head is refused and writes
/// nothing**, at the screen (409, no row, no record) and again at the
/// store under the key; and **the schema admits a second child of one
/// parent**, written through the store's own insert.
#[tokio::test]
async fn a_save_from_a_row_not_the_head_is_refused_and_two_children_are_admitted() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let first = save(&app, &session, "/declarations", posted(&[], &[])).await;
    let second = save(
        &app,
        &session,
        &format!("/declarations/{first}"),
        posted(&[("sampler.seed", "7")], &[]),
    )
    .await;
    let row = s.declaration(&second).await.unwrap().unwrap();
    assert_eq!(
        (row.version, row.what_moved.clone()),
        (2, vec!["sampler.seed".to_owned()])
    );
    assert_eq!(row.the_one_thing.as_deref(), Some("sampler.seed"));

    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    let (status, _, answer) = send_as(
        &app,
        "POST",
        &format!("/declarations/{first}"),
        Some(posted(&[("sampler.seed", "8")], &[])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        (count(s, "declaration").await, count(s, "audit").await),
        (rows, audit)
    );
    // The page of a row not the head says so and offers no save.
    let (_, _, page) = send_as(
        &app,
        "GET",
        &format!("/declarations/{first}"),
        None,
        &session,
    )
    .await;
    assert!(page.contains("not the head"), "{page}");
    assert!(page.contains("class=\"primary\" disabled"), "{page}");

    // The store's own check, under the key, without the screen's.
    let de = s.mint_key("de").await.unwrap();
    let principal = Principal::Person {
        person_id: &ada,
        method: PersonMethod::Session,
    };
    let record = s
        .audit_first(principal, Target::Declaration(&de), "declaration save")
        .await
        .unwrap();
    let document = Document::parse(&base()).unwrap();
    let written = s
        .save_declaration(
            &ada,
            session_id(s, &ada).await,
            &record,
            &de,
            Some(&first),
            &document,
        )
        .await
        .unwrap();
    assert!(matches!(written, Err(Refusal::NotHead)), "{written:?}");
    assert_eq!(count(s, "declaration").await, rows);

    // Two children of one parent, through the store's insert.
    let parent = s.declaration(&first).await.unwrap().unwrap();
    let under = Under {
        declaration_id: parent.declaration_id,
        root_id: parent.root_id,
        version: parent.version,
    };
    let mut tx = s.pool.begin().await.unwrap();
    for de in ["de-00000000000000a1", "de-00000000000000a2"] {
        insert_row(
            &mut tx,
            de,
            &parent.name,
            &parent.body,
            Some(under),
            &[],
            &ada,
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    let children: i64 =
        sqlx::query_scalar("SELECT count(*) FROM declaration WHERE parent_declaration_id = $1")
            .bind(parent.declaration_id)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(children, 3, "one parent holds three children");
}

/// **A declaration row's authored half never changes**: an update of each
/// authored member, a delete and a truncate are refused by the trigger, and
/// the last `validate` reading moves. The key is an identity the schema
/// itself lets no update set.
#[tokio::test]
async fn a_declaration_rows_authored_half_never_changes() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let first = save(&app, &session, "/declarations", posted(&[], &[])).await;
    let child = save(
        &app,
        &session,
        &format!("/declarations/{first}"),
        posted(&[("sampler.seed", "7")], &[]),
    )
    .await;
    for (member, value) in [
        ("declaration_ref", "'de-00000000000000ff'"),
        ("name", "'karla'"),
        ("body", "'{}'::jsonb"),
        ("corpus_commit", "'another'"),
        ("parent_declaration_id", "NULL"),
        ("lineage_root_id", "NULL"),
        ("version", "9"),
        ("saved_at", "now() - interval '1 day'"),
        ("what_moved", "'{}'"),
        ("the_one_thing_that_moved", "NULL"),
        ("author", "NULL"),
    ] {
        let mut tx = s.pool.begin().await.unwrap();
        let result = sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE declaration SET {member} = {value} WHERE declaration_ref = $1"
        )))
        .bind(&child)
        .execute(&mut *tx)
        .await;
        tx.rollback().await.unwrap();
        let error = result
            .err()
            .unwrap_or_else(|| panic!("an update of {member} landed"));
        assert!(
            error.to_string().contains("authored half never changes"),
            "{member}: {error}"
        );
    }
    for statement in [
        "DELETE FROM declaration WHERE declaration_ref = $1",
        "TRUNCATE declaration CASCADE",
    ] {
        let mut tx = s.pool.begin().await.unwrap();
        let result = if statement.contains("$1") {
            sqlx::query(statement).bind(&child).execute(&mut *tx).await
        } else {
            sqlx::query(statement).execute(&mut *tx).await
        };
        tx.rollback().await.unwrap();
        let error = result.err().unwrap_or_else(|| panic!("{statement} landed"));
        assert!(
            error.to_string().contains("never removed"),
            "{statement}: {error}"
        );
    }
    let moved = sqlx::query(
        "UPDATE declaration SET last_validate_answer = '{}'::jsonb, last_validate_at = now() \
         WHERE declaration_ref = $1",
    )
    .bind(&child)
    .execute(&s.pool)
    .await
    .unwrap();
    assert_eq!(moved.rows_affected(), 1, "the last validate reading moves");
}

/// **No two lineages share a name, and a lineage keeps its name**: at the
/// screen (409, no row, no record) and again at the store under the key.
#[tokio::test]
async fn no_two_lineages_share_a_name_and_a_lineage_keeps_its_name() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let first = save(&app, &session, "/declarations", posted(&[], &[])).await;
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    for (what, uri, body) in [
        (
            "a second lineage under a held name",
            "/declarations".to_owned(),
            posted(&[], &[]),
        ),
        (
            "a save renaming its lineage",
            format!("/declarations/{first}"),
            posted(&[("name", "karla")], &[]),
        ),
    ] {
        let (status, _, answer) = send_as(&app, "POST", &uri, Some(body), &session).await;
        assert_eq!(status, StatusCode::CONFLICT, "{what}: {answer}");
        assert!(
            answer.contains("karl"),
            "{what}: the refusal names the name: {answer}"
        );
        assert_eq!(
            (count(s, "declaration").await, count(s, "audit").await),
            (rows, audit),
            "{what}"
        );
    }
    // The store's own checks, under the key, without the screen's.
    let principal = Principal::Person {
        person_id: &ada,
        method: PersonMethod::Session,
    };
    let sid = session_id(s, &ada).await;
    for (from, name) in [(None, "karl"), (Some(first.as_str()), "karla")] {
        let de = s.mint_key("de").await.unwrap();
        let record = s
            .audit_first(principal, Target::Declaration(&de), "declaration save")
            .await
            .unwrap();
        let document = Document::parse(&edit(|v| v["name"] = json!(name))).unwrap();
        let written = s
            .save_declaration(&ada, sid, &record, &de, from, &document)
            .await
            .unwrap();
        assert!(
            matches!(written, Err(Refusal::NameHeld(_) | Refusal::NameChanged(_))),
            "{from:?} {name}: {written:?}"
        );
    }
    assert_eq!(count(s, "declaration").await, rows);
}

/// **Only the server-wide admin grant reads or writes a declaration**: a
/// person holding the operator role on an agent is refused at every route,
/// the page naming nothing, and each refused save writes exactly one audit
/// record carrying the refusal and no row; with no session, sign in; an
/// identity out of shape is 400 and an unknown one 404, for the admin.
#[tokio::test]
async fn only_the_admin_grant_reads_or_writes_a_declaration() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store();
    let app = &o.rig.app;
    let first = save(app, &o.ada_session, "/declarations", posted(&[], &[])).await;
    for uri in [
        "/declarations".to_owned(),
        "/declarations/new".to_owned(),
        format!("/declarations/{first}"),
    ] {
        let (status, _, page) = send_as(app, "GET", &uri, None, &o.bea_session).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {page}");
        assert!(
            !page.contains("karl") && !page.contains("decoder-a"),
            "{uri}: the page names nothing: {page}"
        );
        let (status, _, _) = send_as(app, "GET", &uri, None, "no-such-session").await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}");
    }
    let mut refusals = 0;
    for uri in ["/declarations".to_owned(), format!("/declarations/{first}")] {
        let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
        let (status, _, _) = send_as(
            app,
            "POST",
            &uri,
            Some(posted(&[("name", "bea-agent")], &[])),
            &o.bea_session,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
        assert_eq!(count(s, "declaration").await, rows, "{uri}: no row");
        assert_eq!(
            count(s, "audit").await,
            audit + 1,
            "{uri}: exactly one record"
        );
        refusals += 1;
        let carried: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit WHERE person_id = $1 AND target_kind = 'declaration' \
             AND target_id ~ '^de-' AND action = 'declaration save' AND refusal = 'not an admin'",
        )
        .bind(&o.bea)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert_eq!(carried, refusals, "{uri}: the record carries the refusal");
    }
    for (uri, want) in [
        ("/declarations/de-xyz", StatusCode::BAD_REQUEST),
        ("/declarations/de-0000000000000000", StatusCode::NOT_FOUND),
    ] {
        let (status, _, _) = send_as(app, "GET", uri, None, &o.ada_session).await;
        assert_eq!(status, want, "{uri}");
        let (status, _, _) =
            send_as(app, "POST", uri, Some(posted(&[], &[])), &o.ada_session).await;
        assert_eq!(status, want, "{uri}");
    }
    let (status, _, page) = send_as(app, "GET", "/declarations", None, &o.ada_session).await;
    assert_eq!(status, StatusCode::OK);
    crate::surfaces::assert_framed(&page);
    assert!(
        page.contains(&format!("href=\"/declarations/{first}\"")),
        "{page}"
    );
    assert!(
        page.contains("href=\"/declarations\" class=\"here\""),
        "the nav names the page: {page}"
    );
}

/// **The declaration screen reaches no agent and no host**: with a fake
/// connector of each plane connected and admitted, every route exercised,
/// a save, a save from it, a refused form and a refused person's save among
/// them, and neither connector is sent a line.
#[tokio::test]
async fn the_declaration_screen_reaches_no_agent_and_no_host() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    let app = o.rig.app.clone();
    let session = o.ada_session.clone();
    let first = save(&app, &session, "/declarations", posted(&[], &[])).await;
    save(
        &app,
        &session,
        &format!("/declarations/{first}"),
        posted(&[("store", "sqlite")], &[]),
    )
    .await;
    for uri in [
        "/declarations".to_owned(),
        "/declarations/new?boxes=3".to_owned(),
        format!("/declarations/{first}?boxes=4"),
    ] {
        let (status, _, page) = send_as(&app, "GET", &uri, None, &session).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {page}");
    }
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/declarations",
        Some(posted(&[("tools", "bash")], &[])),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = send_as(
        &app,
        "POST",
        "/declarations",
        Some(posted(&[], &[])),
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let within = Duration::from_secs(3);
    let (admin_line, gate_line) = tokio::join!(
        o.admin_con.first_line_within(within),
        gate.first_line_within(within)
    );
    assert_eq!(admin_line, None, "admin-con was sent a line");
    assert_eq!(gate_line, None, "gate-con was sent a line");
}

/// The members of [`crate::declaration::SAMPLER`], each as the form names
/// its field.
fn tunable_fields() -> Vec<String> {
    crate::declaration::SAMPLER
        .iter()
        .map(|(name, _)| format!("sampler.{name}"))
        .collect()
}

/// **Every tunable value is required, and a save lacking one is refused
/// naming it** (Spec 6.1): on the document, each of the eight dropped in
/// turn still parses, since a row saved before they were required must
/// reopen, and is refused at save naming its field; end to end, a form
/// lacking each in turn, left out or posted empty, is answered 400 naming
/// that field, and neither a row nor an audit record is written. Whole, the
/// form saves.
#[tokio::test]
async fn every_tunable_value_is_required_and_a_missing_one_is_refused_naming_it() {
    let whole = Document::parse(&base()).unwrap();
    assert_eq!(whole.serving(), Ok(()));
    for (name, _) in crate::declaration::SAMPLER {
        let value = edit(|v| {
            v["sampler"].as_object_mut().unwrap().remove(name);
        });
        let document = Document::parse(&value).unwrap_or_else(|e| panic!("{name}: {e}"));
        let refused = document
            .serving()
            .err()
            .unwrap_or_else(|| panic!("a document lacking {name} would save"));
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (Kind::Missing, format!("sampler.{name}").as_str()),
            "{refused}"
        );
    }

    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    for field in tunable_fields() {
        for (how, body) in [
            ("left out", posted(&[], &[field.as_str()])),
            ("posted empty", posted(&[(field.as_str(), "")], &[])),
        ] {
            let (status, _, page) =
                send_as(&app, "POST", "/declarations", Some(body), &session).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{field} {how}: {page}");
            assert!(
                page.contains(&format!("{field}: is required")),
                "{field} {how}: the refusal names its field: {page}"
            );
            assert_eq!(count(s, "declaration").await, rows, "{field} {how} saved");
            assert_eq!(
                count(s, "audit").await,
                audit,
                "{field} {how} wrote a record"
            );
        }
    }
    save(&app, &session, "/declarations", posted(&[], &[])).await;
    assert_eq!(count(s, "declaration").await, rows + 1);
}

/// **Every tunable value is judged on the agent's range at the pin**, a
/// real again as narrowed to a 32-bit float (Spec 6.1's table): on each
/// side of every edge one case, the edge admitted and its neighbour refused
/// naming the field, and the two stated edges admitted. The seed and the
/// maximum tokens per turn past the largest whole number TOML holds are
/// refused by their kind at the parse, and by the range on a document built
/// otherwise. End to end, a range refusal is the screen's 400.
#[tokio::test]
async fn every_tunable_value_is_judged_on_the_agents_range() {
    let cases: Vec<(&str, Value, bool)> = vec![
        // The temperature: finite and 0 or more, as written and narrowed.
        ("temperature", json!(0.0), true),
        ("temperature", json!(-0.1), false),
        ("temperature", json!(3.4e38), true),
        ("temperature", json!(1e39), false),
        ("temperature", json!(1e-50), false),
        // A stated edge: narrows to a subnormal, in range by the table.
        ("temperature", json!(1e-40), true),
        // Top-p: more than 0 and at most 1, as written and narrowed.
        ("top_p", json!(0.0), false),
        ("top_p", json!(-0.5), false),
        ("top_p", json!(1e-30), true),
        ("top_p", json!(1e-50), false),
        ("top_p", json!(1.0), true),
        ("top_p", json!(1.000_000_01), false),
        // The repetition penalty: finite and more than 0.
        ("repetition_penalty", json!(0.0), false),
        ("repetition_penalty", json!(-1.0), false),
        ("repetition_penalty", json!(1e-30), true),
        ("repetition_penalty", json!(1e-50), false),
        ("repetition_penalty", json!(1e38), true),
        ("repetition_penalty", json!(1e39), false),
        // Top-k and the window: below 2^31.
        ("top_k", json!(0), true),
        ("top_k", json!(2_147_483_647_u64), true),
        ("top_k", json!(2_147_483_648_u64), false),
        ("repetition_window", json!(0), true),
        ("repetition_window", json!(2_147_483_647_u64), true),
        ("repetition_window", json!(2_147_483_648_u64), false),
        // The seed and the maximum tokens per turn: to i64::MAX.
        ("seed", json!(0), true),
        ("seed", json!(i64::MAX), true),
        // A stated edge: above 2^53, rounded by the agent's number type.
        ("seed", json!(9_007_199_254_740_993_u64), true),
        ("max_tokens_per_turn", json!(0), true),
        ("max_tokens_per_turn", json!(i64::MAX), true),
        // The context capacity: 1 to below 2^32.
        ("context_capacity", json!(0), false),
        ("context_capacity", json!(1), true),
        ("context_capacity", json!(4_294_967_295_u64), true),
        ("context_capacity", json!(4_294_967_296_u64), false),
    ];
    for (name, value, admitted) in cases {
        let document = Document::parse(&edit(|v| v["sampler"][name] = value.clone()))
            .unwrap_or_else(|e| panic!("{name} = {value} does not parse: {e}"));
        match (document.serving(), admitted) {
            (Ok(()), true) => {}
            (Err(refused), false) => assert_eq!(
                (refused.kind, refused.field.as_str()),
                (Kind::Range, format!("sampler.{name}").as_str()),
                "{name} = {value}: {refused}"
            ),
            (judged, _) => panic!("{name} = {value}: {judged:?}"),
        }
    }
    let past = i64::MAX.unsigned_abs() + 1;
    for name in ["seed", "max_tokens_per_turn"] {
        let refused = Document::parse(&edit(|v| v["sampler"][name] = json!(past))).unwrap_err();
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (Kind::Sampler, format!("sampler.{name}").as_str())
        );
        let mut document = Document::parse(&base()).unwrap();
        match name {
            "seed" => document.sampler.seed = Some(past),
            _ => document.sampler.max_tokens_per_turn = Some(past),
        }
        let refused = document
            .serving()
            .err()
            .unwrap_or_else(|| panic!("{name} past i64::MAX was admitted by the range"));
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (Kind::Range, format!("sampler.{name}").as_str())
        );
    }

    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    for (field, text, words) in [
        (
            "sampler.context_capacity",
            "0",
            "sampler.context_capacity: is outside the agent",
        ),
        (
            "sampler.top_p",
            "1.00000001",
            "sampler.top_p: is outside the agent",
        ),
        (
            "sampler.temperature",
            "inf",
            "sampler.temperature: is a finite number",
        ),
    ] {
        let (status, _, page) = send_as(
            &app,
            "POST",
            "/declarations",
            Some(posted(&[(field, text)], &[])),
            &session,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field} = {text}: {page}");
        assert!(
            page.contains(&words.replace('\'', "&#39;")) || page.contains(words),
            "{field} = {text}: {page}"
        );
        assert_eq!(
            (count(s, "declaration").await, count(s, "audit").await),
            (rows, audit)
        );
    }
}

/// **A new declaration's form starts with the five values the SPU froze
/// until the carrier, and the other three empty** (Spec 6.1): the
/// temperature, top-k, top-p, repetition penalty and repetition window
/// carry 0.7, 40, 0.95, 1.1 and 64 as editable inputs, and the seed, the
/// maximum tokens per turn and the context capacity carry nothing. Posted
/// as it starts, the save is refused naming the seed; with the three
/// entered, it saves the five as they started.
#[tokio::test]
async fn a_new_declarations_form_starts_with_five_tunable_values() {
    let starting = [
        ("sampler.temperature", "0.7"),
        ("sampler.top_k", "40"),
        ("sampler.top_p", "0.95"),
        ("sampler.seed", ""),
        ("sampler.repetition_penalty", "1.1"),
        ("sampler.repetition_window", "64"),
        ("sampler.max_tokens_per_turn", ""),
        ("sampler.context_capacity", ""),
    ];
    let form = Form::empty(1);
    let held: Vec<(String, String)> = form
        .sampler
        .iter()
        .map(|(n, v)| (format!("sampler.{n}"), v.clone()))
        .collect();
    let expected: Vec<(String, String)> = starting
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    assert_eq!(held, expected, "the empty form");

    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (status, _, page) = send_as(&app, "GET", "/declarations/new", None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let shown: Vec<(String, String)> = posted_by(&page)
        .into_iter()
        .filter(|(k, _)| k.starts_with("sampler."))
        .collect();
    assert_eq!(shown, expected, "the new page's inputs");
    for (field, _) in &starting {
        let at = page
            .find(&format!("name=\"{field}\""))
            .unwrap_or_else(|| panic!("{field} is an input"));
        let tag = &page[page[..at].rfind('<').unwrap()..at];
        assert!(
            !tag.contains("readonly") && !tag.contains("disabled"),
            "{field} is editable: {tag}"
        );
    }

    // Posted as it starts, beside the base's other fields: the seed is the
    // first value the operator must enter.
    let rest: Vec<(&str, &str)> = BASE_FORM
        .iter()
        .filter(|(k, _)| !k.starts_with("sampler."))
        .copied()
        .collect();
    let mut fields = rest.clone();
    fields.extend_from_slice(&starting);
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/declarations",
        Some(crate::admin_tests::form(&fields)),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(answer.contains("sampler.seed: is required"), "{answer}");

    let mut fields = rest;
    fields.extend(starting.iter().map(|(k, v)| match *k {
        "sampler.seed" => (*k, "7"),
        "sampler.max_tokens_per_turn" => (*k, "512"),
        "sampler.context_capacity" => (*k, "32768"),
        _ => (*k, *v),
    }));
    let de = save(
        &app,
        &session,
        "/declarations",
        crate::admin_tests::form(&fields),
    )
    .await;
    let row = s.declaration(&de).await.unwrap().unwrap();
    assert_eq!(
        row.body["sampler"],
        json!({
            "temperature": 0.7,
            "top_k": 40,
            "top_p": 0.95,
            "seed": 7,
            "repetition_penalty": 1.1,
            "repetition_window": 64,
            "max_tokens_per_turn": 512,
            "context_capacity": 32768,
        })
    );
}

/// **A row saved before the eight were required reopens, is named stale by
/// its pin, and its save is refused until the eight are filled** (Spec 2.4
/// and 6.1): a row under the earlier pin, holding the old document's
/// optional sampler, reopens with what it holds and empty fields for the
/// rest, its file naming each missing name; its save as reopened is refused
/// naming the first missing field and writes nothing; filled, it saves
/// under this build's pin.
#[tokio::test]
async fn a_row_lacking_a_tunable_value_reopens_stale_and_saves_once_filled() {
    const EARLIER: &str = "a3543b31f264b269f66ab55d8767f25290ec9d61";
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let body = edit(|v| {
        v["sampler"] = json!({"temperature": 0.7, "top_k": 40, "max_tokens_per_turn": 512})
    });
    let de = "de-00000000000000b1";
    sqlx::query(
        "INSERT INTO declaration (declaration_ref, name, body, corpus_commit, version, \
         what_moved, author) VALUES ($1, 'karl', $2, $3, 1, '{}', $4)",
    )
    .bind(de)
    .bind(&body)
    .bind(EARLIER)
    .bind(&ada)
    .execute(&s.pool)
    .await
    .unwrap();

    let (status, _, page) =
        send_as(&app, "GET", &format!("/declarations/{de}"), None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("stale"), "{page}");
    assert!(
        page.contains(EARLIER) && page.contains(CORPUS_COMMIT),
        "{page}"
    );
    let reposted = posted_by(&page);
    let shown: Vec<(&str, &str)> = reposted
        .iter()
        .filter(|(k, _)| k.starts_with("sampler."))
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("sampler.temperature", "0.7"),
            ("sampler.top_k", "40"),
            ("sampler.top_p", ""),
            ("sampler.seed", ""),
            ("sampler.repetition_penalty", ""),
            ("sampler.repetition_window", ""),
            ("sampler.max_tokens_per_turn", "512"),
            ("sampler.context_capacity", ""),
        ],
        "the reopen shows what the row holds and no starting value"
    );
    let file = pre(&page, 0);
    assert!(
        file.contains(
            "# tunable-values lacks top-p, seed, repetition-penalty, repetition-window, \
             context-capacity: a serving load refuses this file"
        ),
        "{file}"
    );
    toml::from_str::<toml::Table>(&file).expect("the stale row's file is TOML 1.0");

    let as_reopened: Vec<(&str, &str)> = reposted
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    let (status, _, answer) = send_as(
        &app,
        "POST",
        &format!("/declarations/{de}"),
        Some(crate::admin_tests::form(&as_reopened)),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(answer.contains("sampler.top_p: is required"), "{answer}");
    assert_eq!(
        (count(s, "declaration").await, count(s, "audit").await),
        (rows, audit)
    );

    let filled: Vec<(&str, &str)> = as_reopened
        .iter()
        .map(|(k, v)| match *k {
            "sampler.top_p" => (*k, "0.95"),
            "sampler.seed" => (*k, "7"),
            "sampler.repetition_penalty" => (*k, "1.1"),
            "sampler.repetition_window" => (*k, "64"),
            "sampler.context_capacity" => (*k, "4096"),
            _ => (*k, *v),
        })
        .collect();
    let child = save(
        &app,
        &session,
        &format!("/declarations/{de}"),
        crate::admin_tests::form(&filled),
    )
    .await;
    let row = s.declaration(&child).await.unwrap().unwrap();
    assert_eq!(
        (row.version, row.corpus_commit.as_str()),
        (2, CORPUS_COMMIT)
    );
    let mut moved = row.what_moved.clone();
    moved.sort_unstable();
    assert_eq!(
        moved,
        [
            "sampler.context_capacity",
            "sampler.repetition_penalty",
            "sampler.repetition_window",
            "sampler.seed",
            "sampler.top_p",
        ]
    );
}

/// **The residual readout's election is required, and a save lacking it
/// is refused naming it** (Spec 6.1), since the agent's grammar gives it no
/// default: on the document, a row lacking it parses, so a row saved before
/// the act reopens, and is refused at save naming its field, while one
/// lacking the turn context or the surprisal election is not, the agent
/// reading their absence; end to end, the form lacking it, left out or
/// posted empty, is answered 400 naming it, and neither a row nor an audit
/// record is written. Whole, the form saves, and the row states all three,
/// the surprisal election's unchecked box as false.
#[tokio::test]
async fn the_residual_readout_election_is_required_and_a_missing_one_is_refused_naming_it() {
    let without = |member: &str| {
        let member = member.to_owned();
        edit(move |v| {
            v.as_object_mut().unwrap().remove(&member);
        })
    };
    let lacking = Document::parse(&without("residual_readout_election"))
        .expect("a row lacking the readout's election reopens");
    let refused = lacking
        .serving()
        .expect_err("a document lacking the readout's election would save");
    assert_eq!(
        (refused.kind, refused.field.as_str()),
        (Kind::Missing, "residual_readout_election"),
        "{refused}"
    );
    for member in ["turn_context", "surprisal_election"] {
        let document = Document::parse(&without(member)).unwrap();
        assert_eq!(document.serving(), Ok(()), "{member} is required");
    }

    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    for (how, body) in [
        ("left out", posted(&[], &["residual_readout_election"])),
        (
            "posted empty",
            posted(&[("residual_readout_election", "")], &[]),
        ),
    ] {
        let (status, _, page) = send_as(&app, "POST", "/declarations", Some(body), &session).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{how}: {page}");
        assert!(
            page.contains("residual_readout_election: is required"),
            "{how}: the refusal names its field: {page}"
        );
        assert_eq!(
            (count(s, "declaration").await, count(s, "audit").await),
            (rows, audit),
            "{how} wrote"
        );
    }
    let de = save(&app, &session, "/declarations", posted(&[], &[])).await;
    let row = s.declaration(&de).await.unwrap().unwrap();
    assert_eq!(
        (
            &row.body["turn_context"],
            &row.body["residual_readout_election"],
            &row.body["surprisal_election"],
        ),
        (&json!("continuous"), &json!(false), &json!(false)),
        "the row states the three elections"
    );
}

/// **A new declaration's form starts with the turn context at continuous
/// and the surprisal election unchecked, the agent's readings of their
/// absence, and the residual readout's election unchosen** (Spec 6.1): the
/// empty form and the new page's inputs, each editable, the page offering
/// both turn contexts; posted as it starts, the form is refused naming the
/// readout's election.
#[tokio::test]
async fn a_new_declarations_form_starts_with_two_elections_at_the_agents_absence() {
    let form = Form::empty(1);
    assert_eq!(
        (
            form.turn_context.as_str(),
            form.residual_readout_election.as_str(),
            form.surprisal_election,
        ),
        ("continuous", "", false),
        "the empty form"
    );

    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (status, _, page) = send_as(&app, "GET", "/declarations/new", None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let elections = [
        "turn_context",
        "residual_readout_election",
        "surprisal_election",
    ];
    let shown: Vec<(String, String)> = posted_by(&page)
        .into_iter()
        .filter(|(k, _)| elections.contains(&k.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("turn_context".to_owned(), "continuous".to_owned()),
            ("residual_readout_election".to_owned(), String::new()),
        ],
        "the new page's elections, the surprisal box unchecked"
    );
    for field in elections {
        let at = page
            .find(&format!("name=\"{field}\""))
            .unwrap_or_else(|| panic!("{field} is an input"));
        let start = page[..at].rfind('<').unwrap();
        let tag = &page[start..=page[at..].find('>').unwrap() + at];
        assert!(
            !tag.contains("readonly") && !tag.contains("disabled"),
            "{field} is editable: {tag}"
        );
    }
    assert!(
        page.contains("<option value=\"independent\""),
        "the page offers the independent turn context: {page}"
    );

    let mut fields: Vec<(&str, &str)> = BASE_FORM
        .iter()
        .filter(|(k, _)| !elections.contains(k))
        .copied()
        .collect();
    fields.extend([
        ("turn_context", "continuous"),
        ("residual_readout_election", ""),
    ]);
    let (status, _, answer) = send_as(
        &app,
        "POST",
        "/declarations",
        Some(crate::admin_tests::form(&fields)),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer.contains("residual_readout_election: is required"),
        "{answer}"
    );
}

/// **A row saved before the three elections reopens, and its save is
/// refused until the residual readout's election is chosen** (Spec 2.4 and
/// 6.1): a row under this build's pin lacking the three reopens with the
/// turn context at continuous, the surprisal box unchecked and the readout
/// unchosen, its file saying each absence in a comment and writing none;
/// its save as reopened is refused naming the readout's election and writes
/// nothing; with the readout chosen it saves, the three named as moved.
#[tokio::test]
async fn a_row_without_the_elections_reopens_and_saves_once_the_readout_is_chosen() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let body = edit(|v| {
        let top = v.as_object_mut().unwrap();
        for member in [
            "turn_context",
            "residual_readout_election",
            "surprisal_election",
        ] {
            top.remove(member);
        }
    });
    let de = "de-00000000000000c1";
    sqlx::query(
        "INSERT INTO declaration (declaration_ref, name, body, corpus_commit, version, \
         what_moved, author) VALUES ($1, 'karl', $2, $3, 1, '{}', $4)",
    )
    .bind(de)
    .bind(&body)
    .bind(CORPUS_COMMIT)
    .bind(&ada)
    .execute(&s.pool)
    .await
    .unwrap();

    let (status, _, page) =
        send_as(&app, "GET", &format!("/declarations/{de}"), None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(
        !page.contains("status hold\">stale"),
        "a row under this build's pin is named stale: {page}"
    );
    let reposted = posted_by(&page);
    let shown: Vec<(&str, &str)> = reposted
        .iter()
        .filter(|(k, _)| {
            [
                "turn_context",
                "residual_readout_election",
                "surprisal_election",
            ]
            .contains(&k.as_str())
        })
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("turn_context", "continuous"),
            ("residual_readout_election", "")
        ],
        "the reopen shows the agent's readings of the absences and no readout chosen"
    );
    let file = pre(&page, 0);
    for words in [
        "# turn-context: not declared on this row; the agent reads its absence as continuous.",
        "# residual-readout-election: not declared on this row: a serving load refuses this file, and the row's save is refused until it is chosen.",
        "# surprisal-election: not declared on this row; the agent reads its absence as false.",
    ] {
        assert!(file.contains(words), "{words}\n{file}");
    }
    let table = toml::from_str::<toml::Table>(&file).expect("the row's file is TOML 1.0");
    let decoder = table["spu-instruction"]["decoder"].as_table().unwrap();
    assert!(
        !table.contains_key("turn-context")
            && !decoder.contains_key("residual-readout-election")
            && !decoder.contains_key("surprisal-election"),
        "an absent election is written as a member: {file}"
    );

    let as_reopened: Vec<(&str, &str)> = reposted
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let (rows, audit) = (count(s, "declaration").await, count(s, "audit").await);
    let (status, _, answer) = send_as(
        &app,
        "POST",
        &format!("/declarations/{de}"),
        Some(crate::admin_tests::form(&as_reopened)),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{answer}");
    assert!(
        answer.contains("residual_readout_election: is required"),
        "{answer}"
    );
    assert_eq!(
        (count(s, "declaration").await, count(s, "audit").await),
        (rows, audit)
    );

    let chosen: Vec<(&str, &str)> = as_reopened
        .iter()
        .map(|(k, v)| match *k {
            "residual_readout_election" => (*k, "true"),
            _ => (*k, *v),
        })
        .collect();
    let child = save(
        &app,
        &session,
        &format!("/declarations/{de}"),
        crate::admin_tests::form(&chosen),
    )
    .await;
    let row = s.declaration(&child).await.unwrap().unwrap();
    assert_eq!(row.version, 2);
    let mut moved = row.what_moved.clone();
    moved.sort_unstable();
    assert_eq!(
        moved,
        [
            "residual_readout_election",
            "surprisal_election",
            "turn_context",
        ]
    );
    assert_eq!(
        (
            &row.body["turn_context"],
            &row.body["residual_readout_election"],
            &row.body["surprisal_election"],
        ),
        (&json!("continuous"), &json!(true), &json!(false))
    );
}
