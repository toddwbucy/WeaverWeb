//! **The declaration's document and its rows** (`src/declaration/`,
//! `src/store/declarations.rs`; Spec 2.4 and 6.1, section 9's rows for
//! them): every refusal by its field and reason; the one warning, and the
//! word for an unknown; the capability table against the vendor's page as
//! read; the agent's file TOML; the state election; the withdrawn
//! placement; and a row's authored half held by the store. The declaration
//! screen that saved rows retired on 2026-11-28 (the Planner's brief, part
//! 5), so [`save`] writes a row through the store's own audited write for
//! the Declare pane's tests. Each DB-backed test runs on a database of its
//! own. No name, card, artifact or digest here is a real deployment's.

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::admin_tests::{admin, form};
use crate::declaration::capability::{self, READ, Rule, TABLE};
use crate::declaration::render::{self, CORPUS_COMMIT, Meta};
use crate::declaration::state_election::{KINDS, StateElection};
use crate::declaration::{Document, Form, Kind};
use crate::passkeys_tests::{app, authenticator, passkeys};
use crate::store::Store;
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::read::tests::fresh_store;

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
const BASE_FORM: [(&str, &str); 17] = [
    ("name", "karl"),
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
pub(crate) fn posted(set: &[(&str, &str)], drop: &[&str]) -> String {
    let mut fields: Vec<(&str, &str)> = BASE_FORM
        .iter()
        .filter(|(k, _)| !drop.contains(k) && !set.iter().any(|(s, _)| s == k))
        .copied()
        .collect();
    fields.extend_from_slice(set);
    form(&fields)
}

/// **A row saved through the store's own audited write**, as the
/// declaration screen saved one until it retired (the Planner's brief of
/// 2026-11-28, part 5): the form read to its document, a first record by
/// the session's person, then `save_declaration` from `from`, or as a new
/// lineage. Answers the new row's identity.
pub(crate) async fn save(s: &Store, bearer: &str, from: Option<&str>, body: String) -> String {
    let document = Form::read(body.as_bytes())
        .and_then(|form| form.document())
        .unwrap_or_else(|e| panic!("the form reads: {e}"));
    let (person_id, session_id): (String, i64) =
        sqlx::query_as("SELECT person_id, session_id FROM session WHERE bearer_digest = $1")
            .bind(crate::surfaces::gate::digest(bearer))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    let de = s.mint_key("de").await.unwrap();
    let principal = Principal::Person {
        person_id: &person_id,
        method: PersonMethod::Session,
    };
    let first = s
        .audit_first(principal, Target::Declaration(&de), "declaration save")
        .await
        .unwrap();
    s.save_declaration(&person_id, session_id, &first, &de, from, &document)
        .await
        .unwrap()
        .unwrap_or_else(|refusal| panic!("the save is refused: {refusal}"));
    de
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
            "the trace declared",
            edit(|v| v["trace"] = json!({})),
            Kind::Unknown,
            "trace",
        ),
        // The state election, beside the store `sqlite`.
        (
            "a state election that is not an object",
            stored(json!("all")),
            Kind::Shape,
            "state_election",
        ),
        (
            "a null state election",
            stored(Value::Null),
            Kind::Shape,
            "state_election",
        ),
        (
            "a state election lacking all_kinds",
            stored(json!({"keys": []})),
            Kind::Missing,
            "state_election.all_kinds",
        ),
        (
            "all_kinds neither true nor false",
            stored(json!({"all_kinds": "yes", "keys": []})),
            Kind::Election,
            "state_election.all_kinds",
        ),
        (
            "a state election lacking keys",
            stored(json!({"all_kinds": true})),
            Kind::Missing,
            "state_election.keys",
        ),
        (
            "keys that are not a list",
            stored(json!({"all_kinds": true, "keys": {}})),
            Kind::Shape,
            "state_election.keys",
        ),
        (
            "an entry that is not an object",
            stored(json!({"all_kinds": true, "keys": ["message.user"]})),
            Kind::Shape,
            "state_election.keys[0]",
        ),
        (
            "an entry lacking its kind",
            stored(json!({"all_kinds": true, "keys": [{"paths": []}]})),
            Kind::Missing,
            "state_election.keys[0].kind",
        ),
        (
            "a kind that is not a text",
            stored(json!({"all_kinds": true, "keys": [{"kind": 3, "paths": []}]})),
            Kind::Shape,
            "state_election.keys[0].kind",
        ),
        (
            "a kind outside the kinds that cross",
            elected(&[("message.users", &[])]),
            Kind::Election,
            "state_election.keys[0].kind",
        ),
        (
            "the save-point kind, which never crosses",
            elected(&[("save_point", &[])]),
            Kind::Election,
            "state_election.keys[0].kind",
        ),
        (
            "a kind named twice",
            elected(&[
                ("message.user", &["content"]),
                ("message.assistant", &[]),
                ("message.user", &[]),
            ]),
            Kind::Twice,
            "state_election.keys[2].kind",
        ),
        (
            "an entry lacking its paths",
            stored(json!({"all_kinds": true, "keys": [{"kind": "message.user"}]})),
            Kind::Missing,
            "state_election.keys[0].paths",
        ),
        (
            "paths that are not a list",
            stored(
                json!({"all_kinds": true, "keys": [{"kind": "message.user", "paths": "content"}]}),
            ),
            Kind::Shape,
            "state_election.keys[0].paths",
        ),
        (
            "a path that is not a text",
            stored(json!({"all_kinds": true, "keys": [{"kind": "message.user", "paths": [3]}]})),
            Kind::Shape,
            "state_election.keys[0].paths[0]",
        ),
        (
            "an empty path",
            elected(&[("message.user", &["content", ""])]),
            Kind::Path,
            "state_election.keys[0].paths[1]",
        ),
        (
            "a path with an empty segment",
            elected(&[("message.user", &["content..text"])]),
            Kind::Path,
            "state_election.keys[0].paths[0]",
        ),
        (
            "a path with a leading dot",
            elected(&[("message.user", &[".content"])]),
            Kind::Path,
            "state_election.keys[0].paths[0]",
        ),
        (
            "a path with a trailing dot",
            elected(&[("message.user", &["content."])]),
            Kind::Path,
            "state_election.keys[0].paths[0]",
        ),
        (
            "a control character in a path",
            elected(&[("message.user", &["con\u{7}tent"])]),
            Kind::Control,
            "state_election.keys[0].paths[0]",
        ),
        (
            "a path past 256 characters",
            elected(&[("message.user", &[&"a".repeat(257)])]),
            Kind::Bound,
            "state_election.keys[0].paths[0]",
        ),
        (
            "more than 32 paths for a kind",
            stored(json!({"all_kinds": true, "keys": [{
                "kind": "message.user",
                "paths": (0..33).map(|i| format!("p{i}")).collect::<Vec<_>>(),
            }]})),
            Kind::Bound,
            "state_election.keys[0].paths",
        ),
        (
            "an unknown member of the election",
            stored(json!({"all_kinds": true, "keys": [], "depth": 1})),
            Kind::Unknown,
            "state_election.depth",
        ),
        (
            "an unknown member of an entry",
            stored(
                json!({"all_kinds": true, "keys": [{"kind": "message.user", "paths": [], "depth": 1}]}),
            ),
            Kind::Unknown,
            "state_election.keys[0].depth",
        ),
        (
            "the default election beside the store none",
            edit(|v| v["state_election"] = json!({"all_kinds": true, "keys": []})),
            Kind::Declined,
            "state_election",
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
        (
            "the default election beside the store sqlite",
            stored(json!({"all_kinds": true, "keys": []})),
        ),
        (
            "a kind with no paths, only the kinds named crossing",
            stored(json!({"all_kinds": false, "keys": [{"kind": "message.user", "paths": []}]})),
        ),
        (
            "32 paths of 256 characters",
            stored(json!({"all_kinds": true, "keys": [{
                "kind": "message.user",
                "paths": (0..32).map(|i| format!("{i:03}{}", "a".repeat(253))).collect::<Vec<_>>(),
            }]})),
        ),
        (
            "a path of 256 characters of two bytes each, measured in characters",
            elected(&[("message.user", &[&"\u{e9}".repeat(256)])]),
        ),
        (
            "every kind that crosses, once each",
            stored(json!({
                "all_kinds": false,
                "keys": KINDS.iter().map(|k| json!({"kind": k, "paths": ["content"]})).collect::<Vec<_>>(),
            })),
        ),
        (
            "a nested path, a path with a space and one beyond ASCII",
            elected(&[("message.assistant", &["content.0.text", "a b", "caf\u{e9}"])]),
        ),
    ] {
        let document = Document::parse(&value).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_eq!(
            Document::parse(&document.to_value()).unwrap(),
            document,
            "{what}: the canonical form reads back as itself"
        );
    }

    // The two refusals the Planner worded: a kind twice says what the
    // agent would do with it, and an election beside no store names
    // admin's refusal.
    let twice = Document::parse(&elected(&[("message.user", &[]), ("message.user", &[])]))
        .expect_err("a kind twice was admitted");
    assert!(
        twice
            .why
            .contains("the agent would take the first entry for a kind and drop the rest"),
        "{twice}"
    );
    let declined = Document::parse(&edit(|v| {
        v["state_election"] = json!({"all_kinds": false, "keys": []})
    }))
    .expect_err("an election beside no store was admitted");
    assert!(
        declined.why.contains("which admin refuses at load"),
        "{declined}"
    );
}

/// The base document with the store `sqlite` and `election` its state
/// election.
fn stored(election: Value) -> Value {
    edit(|v| {
        v["store"] = json!("sqlite");
        v["state_election"] = election;
    })
}

/// The base document beside the store `sqlite`, every kind crossing and
/// each `(kind, paths)` named.
fn elected(keys: &[(&str, &[&str])]) -> Value {
    stored(json!({
        "all_kinds": true,
        "keys": keys
            .iter()
            .map(|(kind, paths)| json!({"kind": kind, "paths": paths}))
            .collect::<Vec<_>>(),
    }))
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

/// The base form beside the store `sqlite`, every kind crossing, with one
/// row of the state election for each `(kind, paths)`.
fn kind_rows(rows: &[(&str, &str)]) -> String {
    let named: Vec<(String, &str)> = rows
        .iter()
        .enumerate()
        .flat_map(|(i, (kind, paths))| {
            [
                (format!("state_election.k{i}.kind"), *kind),
                (format!("state_election.k{i}.paths"), *paths),
            ]
        })
        .collect();
    let mut set: Vec<(&str, &str)> =
        vec![("store", "sqlite"), ("state_election.all_kinds", "true")];
    set.extend(named.iter().map(|(k, v)| (k.as_str(), *v)));
    posted(&set, &[])
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
    let first = save(s, &session, None, posted(&[], &[])).await;
    let child = save(
        s,
        &session,
        Some(&first),
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

/// **The acceptance of 2026-11-13: an agent's state election, typed into
/// the screen, derives as its own file holds it.** The agent's file elects
/// every kind, with the content of each user and each assistant message
/// beside its envelope. Typed as the two rows it is, beside the store
/// `sqlite`, the derived file's `[state-election]` parses equal to that
/// file's, and the file no longer names the table as undeclared.
#[test]
fn an_agents_state_election_derives_as_its_file_holds_it() {
    const ITS_FILE: &str = "[state-election]\n\
        all-kinds = true\n\
        keys = [\n  \
        { kind = \"message.user\", paths = [\"content\"] },\n  \
        { kind = \"message.assistant\", paths = [\"content\"] },\n\
        ]\n";
    let body = kind_rows(&[
        ("message.user", "content"),
        ("message.assistant", "content"),
    ]);
    let document = Form::read(body.as_bytes())
        .unwrap()
        .document()
        .unwrap_or_else(|e| panic!("{e}"));
    let file = render::agent_toml(
        &document,
        &Meta {
            de: "de-0000000000000001",
            version: 1,
        },
    );
    let derived: toml::Table = toml::from_str(&file)
        .unwrap_or_else(|e| panic!("the agent's file is TOML 1.0: {e}\n{file}"));
    let its: toml::Table = toml::from_str(ITS_FILE).unwrap();
    assert!(derived.contains_key("state-election"), "{file}");
    assert_eq!(
        derived.get("state-election"),
        its.get("state-election"),
        "{file}"
    );
    assert!(!file.contains("# [state-election]"), "{file}");
}

/// **An election beside no store is refused, and none is written**: admin
/// refuses a present `[state-election]` beside the store `none`, the
/// default's spelling among them. Beside `none` the form's default, every
/// kind and no rows, writes no member and the file says why; any other
/// election is refused naming the member and admin's refusal. Beside
/// `sqlite` the default is written as the agent spells it.
#[test]
fn an_election_beside_no_store_is_refused_and_none_is_written() {
    let none = Form::read(
        posted(
            &[
                ("state_election.all_kinds", "true"),
                ("state_election.k0.kind", ""),
                ("state_election.k0.paths", ""),
            ],
            &[],
        )
        .as_bytes(),
    )
    .unwrap()
    .document()
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(none.state_election, None, "the default beside no store");
    assert_eq!(none.to_value().get("state_election"), None);
    let meta = Meta {
        de: "de-0000000000000001",
        version: 1,
    };
    let file = render::agent_toml(&none, &meta);
    let table: toml::Table = toml::from_str(&file).unwrap();
    assert!(!table.contains_key("state-election"), "{file}");
    assert!(
        file.contains(
            "# [state-election]: not written beside the store none, which admin refuses an election beside."
        ),
        "{file}"
    );

    for (what, set) in [
        (
            "only the kinds named",
            vec![("state_election.all_kinds", "false")],
        ),
        (
            "a kind's row",
            vec![
                ("state_election.all_kinds", "true"),
                ("state_election.k0.kind", "message.user"),
                ("state_election.k0.paths", ""),
            ],
        ),
    ] {
        let refused = Form::read(posted(&set, &[]).as_bytes())
            .unwrap()
            .document()
            .expect_err(what);
        assert_eq!(
            (refused.kind, refused.field.as_str()),
            (Kind::Declined, "state_election"),
            "{what}: {refused}"
        );
        assert!(
            refused.why.contains("which admin refuses at load"),
            "{refused}"
        );
    }

    let sqlite = Form::read(
        posted(
            &[("store", "sqlite"), ("state_election.all_kinds", "true")],
            &[],
        )
        .as_bytes(),
    )
    .unwrap()
    .document()
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        sqlite.state_election,
        Some(StateElection::agents_default()),
        "the default beside sqlite is written"
    );
    let file = render::agent_toml(&sqlite, &meta);
    let table: toml::Table = toml::from_str(&file).unwrap();
    let election = table["state-election"].as_table().unwrap();
    assert_eq!(election["all-kinds"].as_bool(), Some(true), "{file}");
    assert_eq!(election["keys"].as_array().map(Vec::len), Some(0), "{file}");
}

/// The base document as a row saved before 2026-11-14 holds it: the
/// placement member the operator's rulings of that date withdrew, as a
/// host agent or as a virtual-machine agent with its template.
pub(crate) fn legacy(placement: Value) -> Value {
    edit(|v| {
        v.as_object_mut()
            .unwrap()
            .insert("placement".into(), placement);
    })
}

/// **A document or a form carrying `placement` is refused as an unknown
/// member** (Spec 6.1; the operator's rulings of 2026-11-14): the field
/// left the grammar, and a new save never writes it again.
///
/// Perturbation: admit the member on a parse, and a document carrying it
/// is judged a declaration; on the form, and a post carrying it is.
#[test]
fn a_document_or_a_form_carrying_placement_is_refused_as_an_unknown_member() {
    for placement in [
        json!({"kind": "host"}),
        json!({"kind": "virtual-machine", "template": "template-a"}),
    ] {
        let refused = Document::parse(&legacy(placement.clone()))
            .expect_err("a document carrying placement is admitted");
        assert_eq!(
            (refused.field.as_str(), refused.kind),
            ("placement", Kind::Unknown),
            "{refused}"
        );
    }
    for set in [
        vec![("placement", "host")],
        vec![("placement", "virtual-machine"), ("template", "template-a")],
    ] {
        let refused = Form::read(posted(&set, &[]).as_bytes())
            .expect_err("a form carrying placement is admitted");
        assert_eq!(refused.kind, Kind::Unknown, "{refused}");
        assert!(
            ["placement", "template"].contains(&refused.field.as_str()),
            "{refused}"
        );
    }
}

/// **A saved body carrying `placement` reads without it, and nothing else
/// unknown is admitted** (Spec 2.4): a row's authored half never changes,
/// so a row saved before 2026-11-14 keeps the member, and the reader
/// discards it on read alone.
///
/// Perturbations: read a saved body with the parse alone, and a legacy row
/// does not read; discard every unknown member, and a body carrying a
/// member no grammar held reads.
#[test]
fn a_saved_body_carrying_placement_reads_without_it() {
    let current = Document::parse(&base()).unwrap();
    for placement in [
        json!({"kind": "host"}),
        json!({"kind": "virtual-machine", "template": "template-a"}),
    ] {
        let read = Document::read_saved(&legacy(placement)).expect("a legacy row reads");
        assert_eq!(read, current);
        assert_eq!(read.to_value(), base(), "the member is gone from its value");
    }
    assert_eq!(Document::read_saved(&base()).unwrap(), current);
    let refused = Document::read_saved(&edit(|v| v["host"] = json!("box-a")))
        .expect_err("a member no grammar held reads");
    assert_eq!(
        (refused.field.as_str(), refused.kind),
        ("host", Kind::Unknown)
    );
}
