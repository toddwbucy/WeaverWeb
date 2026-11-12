//! conforms: web-the-declaration-screen-refuses-every-field-out-of-shape
//! conforms: web-a-class-spanning-capabilities-is-warned-of
//! conforms: web-a-saved-declaration-carries-every-tunable-value
//! conforms: web-every-tunable-value-is-judged-on-the-agents-range
//! conforms: web-a-new-form-starts-with-five-tunable-values
//!
//! **The declaration's document** (Spec 6.1): the console's own grammar, a
//! superset of the agent's, which the declaration screen saves and the
//! store keeps (Spec 2.4). This module holds the grammar and nothing that
//! reaches a store, an agent or a host: [`Document::parse`] judges a JSON
//! value and refuses, naming the field, everything out of shape; [`Form`]
//! reads the screen's form and turns it into that value, a refusal naming
//! the screen's own slot; [`Document::serving`] judges, at save, that every
//! tunable value a serving load requires is present and in the agent's
//! range; [`Document::to_value`] writes the canonical form a save stores;
//! [`Document::readings`] reads each class against the capability table for
//! the one warning; and [`moved`] names the paths whose values differ
//! between two documents.
//!
//! **The form refuses on its own only what it can judge alone**: a missing
//! required field, a requirement that names nothing, and a field out of its
//! shape, each listed in Spec 6.1. **A class spanning more than one compute
//! capability is never refused**: it is warned of and saved.

pub mod capability;
pub mod render;
pub mod tunable_set;

use serde_json::{Map, Value, json};
use std::collections::HashSet;

/// **Why a document was refused**, by kind, so a test reads the reason and
/// not its wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A required field is missing or empty.
    Missing,
    /// A requirement, or a part of one, names nothing.
    NamesNothing,
    /// A class gives both a family and a list.
    BothForms,
    /// The roster the framework would not load.
    Roster,
    /// A format other than GGUF.
    Format,
    /// An adapter out of its shape.
    Adapter,
    /// The agent's name outside its grammar.
    Name,
    /// A control character in a text.
    Control,
    /// A tunable value outside its kind: a count that is not a whole number,
    /// a real that is not a finite number.
    Sampler,
    /// A tunable value of its kind but outside the agent's range at the pin
    /// (Spec 6.1's table), judged at save.
    Range,
    /// A tool other than `sh`, or `sh` twice.
    Tool,
    /// A member the grammar does not hold.
    Unknown,
    /// A value of another shape than its field's.
    Shape,
}

/// **A refusal, naming the field** by its path in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub field: String,
    pub kind: Kind,
    pub why: String,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.why)
    }
}

fn refused(field: impl Into<String>, kind: Kind, why: impl Into<String>) -> Refused {
    Refused {
        field: field.into(),
        kind,
        why: why.into(),
    }
}

/// The most SPU boxes, and models in one box, the form offers slots for.
pub const MAX_BOXES: usize = 16;
pub const MAX_MODELS: usize = 8;

/// The document, judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub name: String,
    pub placement: Placement,
    pub boxes: Vec<SpuBox>,
    pub sampler: Sampler,
    pub store: Engine,
    /// The gate's tools: none, or `sh`.
    pub tools: Vec<String>,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    Host,
    VirtualMachine { template: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpuBox {
    pub models: Vec<Model>,
    pub gpu: Requirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    Decoder,
    Classifier,
}

impl Function {
    pub fn word(self) -> &'static str {
        match self {
            Function::Decoder => "decoder",
            Function::Classifier => "classifier",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub function: Function,
    pub artifact: String,
    /// GGUF, the one format offered (ruling 4).
    pub adapter: Option<Adapter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Adapter {
    pub artifact: String,
    pub sha256: String,
    pub base_blake3: String,
    pub scale: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Requirement {
    Card(String),
    Class(Class),
    Pair(Box<[Part; 2]>),
}

/// One side of a pair: a card or a class, never a pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Card(String),
    Class(Class),
}

/// **A class as it was typed**: a family the capability table lists models
/// under, or a list of models, never both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class {
    Family(String),
    Models(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    None,
    Sqlite,
}

impl Engine {
    pub fn word(self) -> &'static str {
        match self {
            Engine::None => "none",
            Engine::Sqlite => "sqlite",
        }
    }
}

/// **The tunable values** (Spec 6.1): every name a serving load requires in
/// the agent's `tunable-values` at the build's pin, the sampler's six
/// defaults and the session's two parameters. **Each is held as an option**
/// so that a row saved before the eight were required still reads and
/// reopens; a save requires all eight, each in its range
/// ([`Document::serving`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sampler {
    pub temperature: Option<f64>,
    pub top_k: Option<u64>,
    pub top_p: Option<f64>,
    pub seed: Option<u64>,
    pub repetition_penalty: Option<f64>,
    pub repetition_window: Option<u64>,
    pub max_tokens_per_turn: Option<u64>,
    pub context_capacity: Option<u64>,
}

/// The tunable values' members, in the grammar's order, each a real or a
/// count. `src/declaration/tunable_set.rs` holds this set to the agent's at
/// the pin.
pub const SAMPLER: [(&str, bool); 8] = [
    ("temperature", true),
    ("top_k", false),
    ("top_p", true),
    ("seed", false),
    ("repetition_penalty", true),
    ("repetition_window", false),
    ("max_tokens_per_turn", false),
    ("context_capacity", false),
];

/// **The five values a new declaration's form starts with** (the Planner's
/// default of 2026-11-12, Spec 6.1): the constants the SPU froze until the
/// carrier, at WeaverAgent `a3543b31f264b269f66ab55d8767f25290ec9d61`,
/// `crates/weaver-spu/src/main.rs` lines 177 to 181. The seed and the two
/// session parameters start empty, and the operator enters each.
pub const STARTING: [(&str, &str); 5] = [
    ("temperature", "0.7"),
    ("top_k", "40"),
    ("top_p", "0.95"),
    ("repetition_penalty", "1.1"),
    ("repetition_window", "64"),
];

/// The counts below 2^31: the GGUF engine takes `top-k` and the window as
/// signed 32-bit integers (`weaver-spu-Spec` section 8).
const BELOW_2_31: u64 = 1 << 31;
/// The context capacity is below 2^32, the SPU's resolve's ceiling.
const BELOW_2_32: u64 = 1 << 32;

/// **Whether a tunable value is in the agent's range at the pin**, Spec
/// 6.1's table: `Sampling::out_of_range` and its three real predicates in
/// `crates/weaver-types/src/wire.rs`, `sampling_defaults` in `config.rs`,
/// and the SPU's resolve for the session's two parameters. A context
/// capacity of zero is refused as both engines refuse it at open. The seed
/// and the maximum tokens per turn stop at `i64::MAX`, tighter than the
/// agent's 2^64, since the agent's file holds no larger integer.
fn in_range(member: &str, value: Value64) -> bool {
    match (member, value) {
        ("temperature", Value64::Real(x)) => narrowed(x, |v| v.is_finite() && v >= 0.0),
        ("top_p", Value64::Real(x)) => narrowed(x, |v| v.is_finite() && v > 0.0 && v <= 1.0),
        ("repetition_penalty", Value64::Real(x)) => narrowed(x, |v| v.is_finite() && v > 0.0),
        ("top_k" | "repetition_window", Value64::Count(n)) => n < BELOW_2_31,
        ("seed" | "max_tokens_per_turn", Value64::Count(n)) => i64::try_from(n).is_ok(),
        ("context_capacity", Value64::Count(n)) => (1..BELOW_2_32).contains(&n),
        _ => false,
    }
}

/// **A real judged as written, then as the agent narrows it to a 32-bit
/// float** (`DecoderInstruction::sampling_defaults`): in range as written, a
/// nonzero value staying nonzero once narrowed, and the narrowed value,
/// widened back exactly, in range again, which refuses a value that
/// narrowed past the float's range to infinity.
fn narrowed(x: f64, in_range: fn(f64) -> bool) -> bool {
    let n = x as f32;
    in_range(x) && !(x != 0.0 && n == 0.0) && in_range(f64::from(n))
}

/// **What the screen admits for each tunable value**, Spec 6.1's table in
/// words, for the form's hints and the range refusal.
pub fn admits(member: &str) -> &'static str {
    match member {
        "temperature" => "a finite number, 0 or more",
        "top_k" | "repetition_window" => "a whole number from 0 to 2147483647",
        "top_p" => "a number more than 0 and at most 1",
        "repetition_penalty" => "a finite number more than 0",
        "seed" | "max_tokens_per_turn" => "a whole number from 0 to 9223372036854775807",
        "context_capacity" => "a whole number from 1 to 4294967295",
        _ => "not a tunable value",
    }
}

/// One sampler value, as the screen and the agent's file read it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value64 {
    Real(f64),
    Count(u64),
}

impl Sampler {
    /// Each value present, by its member's name, in the grammar's order.
    pub fn present(&self) -> Vec<(&'static str, Value64)> {
        let reals = [
            ("temperature", self.temperature),
            ("top_p", self.top_p),
            ("repetition_penalty", self.repetition_penalty),
        ];
        let counts = [
            ("top_k", self.top_k),
            ("seed", self.seed),
            ("repetition_window", self.repetition_window),
            ("max_tokens_per_turn", self.max_tokens_per_turn),
            ("context_capacity", self.context_capacity),
        ];
        SAMPLER
            .iter()
            .filter_map(|(name, real)| {
                if *real {
                    reals
                        .iter()
                        .find(|(n, _)| n == name)
                        .and_then(|(_, v)| v.map(Value64::Real))
                        .map(|v| (*name, v))
                } else {
                    counts
                        .iter()
                        .find(|(n, _)| n == name)
                        .and_then(|(_, v)| v.map(Value64::Count))
                        .map(|v| (*name, v))
                }
            })
            .collect()
    }

    fn set(&mut self, name: &str, value: Value64) {
        match (name, value) {
            ("temperature", Value64::Real(v)) => self.temperature = Some(v),
            ("top_p", Value64::Real(v)) => self.top_p = Some(v),
            ("repetition_penalty", Value64::Real(v)) => self.repetition_penalty = Some(v),
            ("top_k", Value64::Count(v)) => self.top_k = Some(v),
            ("seed", Value64::Count(v)) => self.seed = Some(v),
            ("repetition_window", Value64::Count(v)) => self.repetition_window = Some(v),
            ("max_tokens_per_turn", Value64::Count(v)) => self.max_tokens_per_turn = Some(v),
            ("context_capacity", Value64::Count(v)) => self.context_capacity = Some(v),
            _ => {}
        }
    }

    /// The members the document lacks, in the grammar's order.
    pub fn missing(&self) -> Vec<&'static str> {
        let present = self.present();
        SAMPLER
            .iter()
            .map(|(n, _)| *n)
            .filter(|n| !present.iter().any(|(m, _)| m == n))
            .collect()
    }
}

/// **Whether a name is in the register's grammar** (Spec 8): letters,
/// digits, `_` and `-`, migration `0010`'s rule.
pub fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// **Whether a text holds a control character**: any of Unicode's `Cc`, the
/// prompt alone admitting a line feed and a tab.
fn has_control(text: &str, prompt: bool) -> bool {
    text.chars()
        .any(|c| c.is_control() && !(prompt && (c == '\n' || c == '\t')))
}

fn is_hex64(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An object, its members within `allowed`, or the refusal naming the
/// first member it does not hold.
fn object<'a>(
    value: &'a Value,
    path: &str,
    allowed: &[&str],
) -> Result<&'a Map<String, Value>, Refused> {
    let Some(map) = value.as_object() else {
        return Err(refused(path, Kind::Shape, "is not an object"));
    };
    if let Some(member) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(refused(
            join(path, member),
            Kind::Unknown,
            "is not a member of the declaration's grammar",
        ));
    }
    Ok(map)
}

fn join(path: &str, member: &str) -> String {
    if path.is_empty() {
        member.to_owned()
    } else {
        format!("{path}.{member}")
    }
}

/// A required text: present, a string, not empty, and free of control
/// characters.
fn required_text(map: &Map<String, Value>, path: &str, member: &str) -> Result<String, Refused> {
    let field = join(path, member);
    match map.get(member) {
        None | Some(Value::Null) => Err(refused(field, Kind::Missing, "is required")),
        Some(Value::String(s)) if s.is_empty() => Err(refused(field, Kind::Missing, "is required")),
        Some(Value::String(s)) if has_control(s, false) => {
            Err(refused(field, Kind::Control, "holds a control character"))
        }
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(refused(field, Kind::Shape, "is not a text")),
    }
}

/// A text that names a part of a requirement: a string free of control
/// characters, empty answering that it names nothing.
fn naming_text(value: &Value, field: String, nothing: &str) -> Result<String, Refused> {
    match value {
        Value::String(s) if s.trim().is_empty() => Err(refused(field, Kind::NamesNothing, nothing)),
        Value::String(s) if has_control(s, false) => {
            Err(refused(field, Kind::Control, "holds a control character"))
        }
        Value::String(s) => Ok(s.clone()),
        _ => Err(refused(field, Kind::Shape, "is not a text")),
    }
}

impl Document {
    /// **The document a JSON value is, or the refusal naming its field.**
    pub fn parse(value: &Value) -> Result<Document, Refused> {
        let top = object(
            value,
            "",
            &[
                "name",
                "placement",
                "boxes",
                "sampler",
                "store",
                "gate",
                "prompt",
            ],
        )?;
        let name = required_text(top, "", "name")?;
        if !is_name(&name) {
            return Err(refused(
                "name",
                Kind::Name,
                "is letters, digits, _ and - alone, the register's grammar",
            ));
        }
        let placement = placement(top.get("placement"))?;
        let boxes = boxes(top.get("boxes"))?;
        let sampler = sampler(top.get("sampler"))?;
        let store = match top.get("store") {
            None | Some(Value::Null) => return Err(refused("store", Kind::Missing, "is required")),
            Some(Value::String(s)) if s.is_empty() => {
                return Err(refused("store", Kind::Missing, "is required"));
            }
            Some(Value::String(s)) if s == "none" => Engine::None,
            Some(Value::String(s)) if s == "sqlite" => Engine::Sqlite,
            Some(_) => {
                return Err(refused(
                    "store",
                    Kind::Shape,
                    "is none or sqlite, the engines the agent's grammar names",
                ));
            }
        };
        let tools = gate(top.get("gate"))?;
        let prompt = match top.get("prompt") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if s.is_empty() => None,
            Some(Value::String(s)) if has_control(s, true) => {
                return Err(refused(
                    "prompt",
                    Kind::Control,
                    "holds a control character other than a line feed or a tab",
                ));
            }
            Some(Value::String(s)) => Some(s.clone()),
            Some(_) => return Err(refused("prompt", Kind::Shape, "is a text or null")),
        };
        let document = Document {
            name,
            placement,
            boxes,
            sampler,
            store,
            tools,
            prompt,
        };
        document.roster()?;
        Ok(document)
    }

    /// **The roster the framework charters** (PRD 3.1): exactly one decoder
    /// across every box, at most one classifier, an adapter on a decoder
    /// alone.
    fn roster(&self) -> Result<(), Refused> {
        let mut decoder = false;
        let mut classifier = false;
        for (i, b) in self.boxes.iter().enumerate() {
            for (j, m) in b.models.iter().enumerate() {
                let field = format!("boxes[{i}].models[{j}]");
                match m.function {
                    Function::Decoder if decoder => {
                        return Err(refused(
                            format!("{field}.function"),
                            Kind::Roster,
                            "is a second decoder; an agent declares exactly one",
                        ));
                    }
                    Function::Decoder => decoder = true,
                    Function::Classifier if classifier => {
                        return Err(refused(
                            format!("{field}.function"),
                            Kind::Roster,
                            "is a second classifier; an agent declares at most one",
                        ));
                    }
                    Function::Classifier => classifier = true,
                }
                if m.function == Function::Classifier && m.adapter.is_some() {
                    return Err(refused(
                        format!("{field}.adapter"),
                        Kind::Roster,
                        "is an adapter on a classifier; an adapter is a decoder's",
                    ));
                }
            }
        }
        if !decoder {
            return Err(refused(
                "boxes",
                Kind::Roster,
                "declare no decoder; an agent declares exactly one",
            ));
        }
        Ok(())
    }

    /// **Whether a save may store this document** (Spec 6.1): every tunable
    /// value a serving load requires at the pin present, each in the agent's
    /// range, or the refusal naming the first that is not, in the grammar's
    /// order. [`Document::parse`] admits a document lacking one, so that a
    /// row saved before the eight were required still reopens; a save judges
    /// it here as well, through [`Form::document`].
    pub fn serving(&self) -> Result<(), Refused> {
        let present = self.sampler.present();
        for (name, _) in SAMPLER {
            let field = format!("sampler.{name}");
            match present.iter().find(|(m, _)| *m == name) {
                None => {
                    return Err(refused(
                        field,
                        Kind::Missing,
                        "is required: a serving load refuses a declaration lacking it",
                    ));
                }
                Some((_, value)) if !in_range(name, *value) => {
                    return Err(refused(
                        field,
                        Kind::Range,
                        format!("is outside the agent's range: {}", admits(name)),
                    ));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// **The canonical form a save stores**: every member written, absent
    /// sampler values left out, the prompt null where there is none.
    pub fn to_value(&self) -> Value {
        let placement = match &self.placement {
            Placement::Host => json!({"kind": "host"}),
            Placement::VirtualMachine { template } => {
                json!({"kind": "virtual-machine", "template": template})
            }
        };
        let boxes: Vec<Value> = self
            .boxes
            .iter()
            .map(|b| {
                let models: Vec<Value> = b
                    .models
                    .iter()
                    .map(|m| {
                        let mut model = json!({
                            "function": m.function.word(),
                            "artifact": m.artifact,
                            "format": "gguf",
                        });
                        if let Some(a) = &m.adapter {
                            model["adapter"] = json!({
                                "artifact": a.artifact,
                                "sha256": a.sha256,
                                "base_blake3": a.base_blake3,
                                "scale": a.scale,
                            });
                        }
                        model
                    })
                    .collect();
                json!({"models": models, "gpu": requirement_value(&b.gpu)})
            })
            .collect();
        let mut sampler = Map::new();
        for (name, value) in self.sampler.present() {
            sampler.insert(
                name.to_owned(),
                match value {
                    Value64::Real(v) => json!(v),
                    Value64::Count(v) => json!(v),
                },
            );
        }
        json!({
            "name": self.name,
            "placement": placement,
            "boxes": boxes,
            "sampler": sampler,
            "store": self.store.word(),
            "gate": {"tools": self.tools},
            "prompt": self.prompt,
        })
    }

    /// **Every class the document names, read against the capability
    /// table**, in the order the boxes and their requirements hold them.
    pub fn readings(&self) -> Vec<Reading> {
        let mut out = Vec::new();
        for (i, b) in self.boxes.iter().enumerate() {
            let field = format!("boxes[{i}].gpu");
            match &b.gpu {
                Requirement::Card(_) => {}
                Requirement::Class(class) => out.push(read_class(format!("{field}.class"), class)),
                Requirement::Pair(parts) => {
                    for (k, part) in parts.iter().enumerate() {
                        if let Part::Class(class) = part {
                            out.push(read_class(format!("{field}.pair[{k}].class"), class));
                        }
                    }
                }
            }
        }
        out
    }
}

fn part_value(part: &Part) -> Value {
    match part {
        Part::Card(card) => json!({"card": card}),
        Part::Class(class) => json!({"class": class_value(class)}),
    }
}

fn class_value(class: &Class) -> Value {
    match class {
        Class::Family(family) => json!({"family": family}),
        Class::Models(models) => json!({"models": models}),
    }
}

fn requirement_value(requirement: &Requirement) -> Value {
    match requirement {
        Requirement::Card(card) => json!({"card": card}),
        Requirement::Class(class) => json!({"class": class_value(class)}),
        Requirement::Pair(parts) => json!({"pair": [part_value(&parts[0]), part_value(&parts[1])]}),
    }
}

fn placement(value: Option<&Value>) -> Result<Placement, Refused> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Err(refused("placement", Kind::Missing, "is required"));
    };
    let map = object(value, "placement", &["kind", "template"])?;
    let kind = required_text(map, "placement", "kind")?;
    match kind.as_str() {
        "host" => {
            if map.contains_key("template") {
                return Err(refused(
                    "placement.template",
                    Kind::Unknown,
                    "is a virtual-machine agent's member, not a host agent's",
                ));
            }
            Ok(Placement::Host)
        }
        "virtual-machine" => Ok(Placement::VirtualMachine {
            template: required_text(map, "placement", "template")?,
        }),
        _ => Err(refused(
            "placement.kind",
            Kind::Shape,
            "is host or virtual-machine",
        )),
    }
}

fn boxes(value: Option<&Value>) -> Result<Vec<SpuBox>, Refused> {
    let list = match value {
        None | Some(Value::Null) => return Err(refused("boxes", Kind::Missing, "is required")),
        Some(Value::Array(list)) if list.is_empty() => {
            return Err(refused(
                "boxes",
                Kind::Missing,
                "is required: at least one SPU box",
            ));
        }
        Some(Value::Array(list)) => list,
        Some(_) => return Err(refused("boxes", Kind::Shape, "is not a list")),
    };
    let mut out = Vec::new();
    for (i, b) in list.iter().enumerate() {
        let path = format!("boxes[{i}]");
        let map = object(b, &path, &["models", "gpu"])?;
        let models = match map.get("models") {
            None | Some(Value::Null) => {
                return Err(refused(
                    format!("{path}.models"),
                    Kind::Missing,
                    "is required: a box holds at least one model",
                ));
            }
            Some(Value::Array(list)) if list.is_empty() => {
                return Err(refused(
                    format!("{path}.models"),
                    Kind::Missing,
                    "is required: a box holds at least one model",
                ));
            }
            Some(Value::Array(list)) => list,
            Some(_) => {
                return Err(refused(
                    format!("{path}.models"),
                    Kind::Shape,
                    "is not a list",
                ));
            }
        };
        let models = models
            .iter()
            .enumerate()
            .map(|(j, m)| model(m, &format!("{path}.models[{j}]")))
            .collect::<Result<Vec<_>, _>>()?;
        let gpu = match map.get("gpu") {
            None | Some(Value::Null) => {
                return Err(refused(
                    format!("{path}.gpu"),
                    Kind::Missing,
                    "is required: the box's GPU requirement",
                ));
            }
            Some(gpu) => requirement(gpu, &format!("{path}.gpu"))?,
        };
        out.push(SpuBox { models, gpu });
    }
    Ok(out)
}

fn model(value: &Value, path: &str) -> Result<Model, Refused> {
    let map = object(value, path, &["function", "artifact", "format", "adapter"])?;
    let function = match required_text(map, path, "function")?.as_str() {
        "decoder" => Function::Decoder,
        "classifier" => Function::Classifier,
        _ => {
            return Err(refused(
                join(path, "function"),
                Kind::Roster,
                "is a function the roster does not hold: decoder or classifier",
            ));
        }
    };
    let artifact = required_text(map, path, "artifact")?;
    if required_text(map, path, "format")? != "gguf" {
        return Err(refused(
            join(path, "format"),
            Kind::Format,
            "is GGUF, the one format offered; the others come later",
        ));
    }
    let adapter = match map.get("adapter") {
        None => None,
        Some(value) => Some(adapter(value, &join(path, "adapter"))?),
    };
    Ok(Model {
        function,
        artifact,
        adapter,
    })
}

fn adapter(value: &Value, path: &str) -> Result<Adapter, Refused> {
    let Some(map) = value.as_object() else {
        return Err(refused(
            path,
            Kind::Adapter,
            "is a whole adapter or absent, never null",
        ));
    };
    object(value, path, &["artifact", "sha256", "base_blake3", "scale"])?;
    for member in ["artifact", "sha256", "base_blake3", "scale"] {
        if map
            .get(member)
            .is_none_or(|v| v.is_null() || v.as_str() == Some(""))
        {
            return Err(refused(
                join(path, member),
                Kind::Adapter,
                "is missing: an adapter carries its artifact, its SHA-256, its base's BLAKE3 and its scale",
            ));
        }
    }
    let artifact = required_text(map, path, "artifact")?;
    let mut digests = Vec::new();
    for member in ["sha256", "base_blake3"] {
        match map.get(member) {
            Some(Value::String(s)) if is_hex64(s) => digests.push(s.clone()),
            _ => {
                return Err(refused(
                    join(path, member),
                    Kind::Adapter,
                    "is a digest of another shape: 64 lowercase hexadecimal digits",
                ));
            }
        }
    }
    let scale = match map.get("scale").and_then(Value::as_f64) {
        Some(scale) if scale.is_finite() => scale,
        _ => {
            return Err(refused(
                join(path, "scale"),
                Kind::Adapter,
                "is a finite number",
            ));
        }
    };
    let base_blake3 = digests.pop().unwrap_or_default();
    let sha256 = digests.pop().unwrap_or_default();
    Ok(Adapter {
        artifact,
        sha256,
        base_blake3,
        scale,
    })
}

fn requirement(value: &Value, path: &str) -> Result<Requirement, Refused> {
    let map = object(value, path, &["card", "class", "pair"])?;
    match map.len() {
        0 => Err(refused(
            path,
            Kind::NamesNothing,
            "names nothing: a card, a class or a pair",
        )),
        1 => {
            if let Some(card) = map.get("card") {
                Ok(Requirement::Card(naming_text(
                    card,
                    join(path, "card"),
                    "is a card with no identifier",
                )?))
            } else if let Some(class) = map.get("class") {
                Ok(Requirement::Class(self::class(
                    class,
                    &join(path, "class"),
                )?))
            } else {
                let pair = join(path, "pair");
                let parts = match map.get("pair") {
                    Some(Value::Array(parts)) if parts.len() == 2 => parts,
                    _ => {
                        return Err(refused(
                            pair,
                            Kind::NamesNothing,
                            "is a pair that is not two requirements",
                        ));
                    }
                };
                let first = self::part(&parts[0], &format!("{pair}[0]"))?;
                let second = self::part(&parts[1], &format!("{pair}[1]"))?;
                Ok(Requirement::Pair(Box::new([first, second])))
            }
        }
        _ => Err(refused(
            path,
            Kind::Shape,
            "is one of a card, a class or a pair, not more",
        )),
    }
}

fn part(value: &Value, path: &str) -> Result<Part, Refused> {
    if value.get("pair").is_some() {
        return Err(refused(
            join(path, "pair"),
            Kind::NamesNothing,
            "is a pair inside a pair; each side is a card or a class",
        ));
    }
    match requirement(value, path)? {
        Requirement::Card(card) => Ok(Part::Card(card)),
        Requirement::Class(class) => Ok(Part::Class(class)),
        Requirement::Pair(_) => unreachable!("a pair is refused above"),
    }
}

fn class(value: &Value, path: &str) -> Result<Class, Refused> {
    let map = object(value, path, &["family", "models"])?;
    match (map.get("family"), map.get("models")) {
        (None, None) => Err(refused(
            path,
            Kind::NamesNothing,
            "gives neither a family nor a list of models",
        )),
        (Some(_), Some(_)) => Err(refused(
            path,
            Kind::BothForms,
            "gives both a family and a list; a class is one or the other",
        )),
        (Some(family), None) => Ok(Class::Family(naming_text(
            family,
            join(path, "family"),
            "is a family with no name",
        )?)),
        (None, Some(models)) => {
            let field = join(path, "models");
            let list = match models {
                Value::Array(list) if list.is_empty() => {
                    return Err(refused(
                        field,
                        Kind::NamesNothing,
                        "is a list with no model",
                    ));
                }
                Value::Array(list) => list,
                _ => return Err(refused(field, Kind::Shape, "is not a list")),
            };
            // Each name folded once and looked up, so that the judgement is
            // one pass over what was sent: it runs before the session is
            // read (Spec 6.1).
            let mut out: Vec<String> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            for (k, m) in list.iter().enumerate() {
                let name = naming_text(m, format!("{field}[{k}]"), "is a model with no name")?;
                if !seen.insert(fold(&name)) {
                    return Err(refused(
                        format!("{field}[{k}]"),
                        Kind::NamesNothing,
                        "names a model the list names already",
                    ));
                }
                out.push(name);
            }
            Ok(Class::Models(out))
        }
    }
}

fn fold(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn sampler(value: Option<&Value>) -> Result<Sampler, Refused> {
    let mut out = Sampler::default();
    let Some(value) = value else {
        return Ok(out);
    };
    let names: Vec<&str> = SAMPLER.iter().map(|(n, _)| *n).collect();
    let map = object(value, "sampler", &names)?;
    for (name, real) in SAMPLER {
        let Some(v) = map.get(name) else {
            continue;
        };
        let field = format!("sampler.{name}");
        let value = if real {
            match v.as_f64() {
                Some(x) if x.is_finite() && v.is_number() => Value64::Real(x),
                _ => return Err(refused(field, Kind::Sampler, "is a finite number")),
            }
        } else {
            match v.as_u64() {
                Some(x) if i64::try_from(x).is_ok() => Value64::Count(x),
                Some(_) => {
                    return Err(refused(
                        field,
                        Kind::Sampler,
                        "is a count above 9223372036854775807, the largest whole number the agent's file holds",
                    ));
                }
                None => {
                    return Err(refused(
                        field,
                        Kind::Sampler,
                        "is a count: a whole number at or above zero",
                    ));
                }
            }
        };
        out.set(name, value);
    }
    Ok(out)
}

fn gate(value: Option<&Value>) -> Result<Vec<String>, Refused> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let map = object(value, "gate", &["tools"])?;
    let tools = match map.get("tools") {
        None => return Ok(Vec::new()),
        Some(Value::Array(tools)) => tools,
        Some(_) => return Err(refused("gate.tools", Kind::Shape, "is not a list")),
    };
    let mut out: Vec<String> = Vec::new();
    for (k, tool) in tools.iter().enumerate() {
        let field = format!("gate.tools[{k}]");
        match tool.as_str() {
            Some("sh") if out.is_empty() => out.push("sh".to_owned()),
            Some("sh") => return Err(refused(field, Kind::Tool, "names sh twice")),
            _ => {
                return Err(refused(
                    field,
                    Kind::Tool,
                    "is a tool other than sh, the one registered tool",
                ));
            }
        }
    }
    Ok(out)
}

/// **A class read against the capability table** (Spec 6.1): each model it
/// stands for with its capability, or the word that the table does not
/// know it, and whether it spans more than one capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reading {
    /// The class's path in the document.
    pub field: String,
    /// What was typed: `family` or `list`.
    pub form: &'static str,
    /// The family typed, where the class is one.
    pub family: Option<String>,
    /// Whether the table knows the family typed; true of a list.
    pub family_known: bool,
    /// Each model the class stands for, with its capability, or none where
    /// the table does not know it.
    pub models: Vec<(String, Option<&'static str>)>,
    /// The distinct capabilities the known models hold, in the order met.
    pub capabilities: Vec<&'static str>,
}

impl Reading {
    /// **The one warning**: the class spans more than one capability.
    pub fn spans(&self) -> bool {
        self.capabilities.len() > 1
    }
}

type ClassRead = (
    &'static str,
    Option<String>,
    bool,
    Vec<(String, Option<&'static str>)>,
);

fn read_class(field: String, class: &Class) -> Reading {
    let (form, family, family_known, models): ClassRead = match class {
        Class::Family(name) => {
            let rows = capability::family(name);
            let known = !rows.is_empty();
            let models = rows
                .into_iter()
                .map(|card| (card.model.to_owned(), Some(card.capability)))
                .collect();
            ("family", Some(name.clone()), known, models)
        }
        Class::Models(list) => {
            let models = list
                .iter()
                .map(|name| (name.clone(), capability::model(name).map(|c| c.capability)))
                .collect();
            ("list", None, true, models)
        }
    };
    let mut capabilities: Vec<&'static str> = Vec::new();
    for (_, cc) in models.iter() {
        if let Some(cc) = *cc
            && !capabilities.contains(&cc)
        {
            capabilities.push(cc);
        }
    }
    Reading {
        field,
        form,
        family,
        family_known,
        models,
        capabilities,
    }
}

/// **The paths whose values differ between two documents** (Spec 2.4's
/// "what moved"), each a leaf, or the member or element one holds and the
/// other does not, in the canonical form's order.
pub fn moved(before: &Value, after: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff("", before, after, &mut out);
    out
}

fn diff(path: &str, before: &Value, after: &Value, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().collect();
            for k in b.keys() {
                if !a.contains_key(k) {
                    keys.push(k);
                }
            }
            for k in keys {
                let p = join(path, k);
                match (a.get(k), b.get(k)) {
                    (Some(x), Some(y)) => diff(&p, x, y, out),
                    _ => out.push(p),
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for i in 0..a.len().max(b.len()) {
                let p = format!("{path}[{i}]");
                match (a.get(i), b.get(i)) {
                    (Some(x), Some(y)) => diff(&p, x, y, out),
                    _ => out.push(p),
                }
            }
        }
        (x, y) if x == y => {}
        _ => out.push(if path.is_empty() {
            "(the document)".to_owned()
        } else {
            path.to_owned()
        }),
    }
}

/// **The screen's form, as its fields read**, kept as typed so a refused
/// save shows the screen again with what was typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Form {
    pub name: String,
    pub placement: String,
    pub template: String,
    pub boxes: Vec<FormBox>,
    /// Each sampler member's text, in [`SAMPLER`]'s order.
    pub sampler: Vec<(&'static str, String)>,
    pub store: String,
    pub sh: bool,
    pub prompt: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormBox {
    pub models: Vec<FormModel>,
    /// The requirement's two parts, each a kind (`card`, `family`,
    /// `models`, or empty) and its text; the second filled makes a pair.
    pub gpu: [(String, String); 2],
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormModel {
    pub function: String,
    pub artifact: String,
    pub adapter_artifact: String,
    pub adapter_sha256: String,
    pub adapter_base_blake3: String,
    pub adapter_scale: String,
}

impl FormModel {
    fn is_empty(&self) -> bool {
        self.function.is_empty()
            && self.artifact.is_empty()
            && self.adapter_artifact.is_empty()
            && self.adapter_sha256.is_empty()
            && self.adapter_base_blake3.is_empty()
            && self.adapter_scale.is_empty()
    }

    fn has_adapter(&self) -> bool {
        !(self.adapter_artifact.is_empty()
            && self.adapter_sha256.is_empty()
            && self.adapter_base_blake3.is_empty()
            && self.adapter_scale.is_empty())
    }
}

impl FormBox {
    fn is_empty(&self) -> bool {
        self.models.iter().all(FormModel::is_empty)
            && self.gpu.iter().all(|(k, v)| k.is_empty() && v.is_empty())
    }
}

/// An index in a field's name, `b3` read as 3, within its bound.
fn index(part: &str, prefix: char, bound: usize) -> Option<usize> {
    let i: usize = part.strip_prefix(prefix)?.parse().ok()?;
    (i < bound).then_some(i)
}

impl Form {
    /// **The form a request's body carries**, every field by its name; a
    /// field the screen does not hold is refused as an unknown member.
    pub fn read(body: &[u8]) -> Result<Form, Refused> {
        let mut form = Form {
            sampler: SAMPLER.iter().map(|(n, _)| (*n, String::new())).collect(),
            ..Form::default()
        };
        for (key, value) in url::form_urlencoded::parse(body) {
            let value = value.into_owned();
            let parts: Vec<&str> = key.split('.').collect();
            match parts.as_slice() {
                ["name"] => form.name = value,
                ["placement"] => form.placement = value,
                ["template"] => form.template = value,
                ["store"] => form.store = value,
                ["tools"] if value == "sh" && !form.sh => form.sh = true,
                ["tools"] => {
                    return Err(refused(
                        "gate.tools",
                        Kind::Tool,
                        "is a tool other than sh, the one registered tool, or sh twice",
                    ));
                }
                ["prompt"] => form.prompt = value.replace("\r\n", "\n"),
                ["sampler", member] if SAMPLER.iter().any(|(n, _)| n == member) => {
                    if let Some(slot) = form.sampler.iter_mut().find(|(n, _)| n == member) {
                        slot.1 = value;
                    }
                }
                [b, rest @ ..] if b.starts_with('b') => {
                    let Some(i) = index(b, 'b', MAX_BOXES) else {
                        return Err(unknown_field(&key));
                    };
                    if form.boxes.len() <= i {
                        form.boxes.resize(i + 1, FormBox::default());
                    }
                    let fbox = &mut form.boxes[i];
                    match rest {
                        [g, which] if *g == "gpu1" || *g == "gpu2" => {
                            let slot = &mut fbox.gpu[if *g == "gpu1" { 0 } else { 1 }];
                            match *which {
                                "kind" => slot.0 = value,
                                "value" => slot.1 = value,
                                _ => return Err(unknown_field(&key)),
                            }
                        }
                        [m, field @ ..] if m.starts_with('m') => {
                            let Some(j) = index(m, 'm', MAX_MODELS) else {
                                return Err(unknown_field(&key));
                            };
                            if fbox.models.len() <= j {
                                fbox.models.resize(j + 1, FormModel::default());
                            }
                            let model = &mut fbox.models[j];
                            match field {
                                ["function"] => model.function = value,
                                ["artifact"] => model.artifact = value,
                                ["format"] => {
                                    if value != "gguf" {
                                        return Err(refused(
                                            format!("boxes[{i}].models[{j}].format"),
                                            Kind::Format,
                                            "is GGUF, the one format offered; the others come later",
                                        ));
                                    }
                                }
                                ["adapter", "artifact"] => model.adapter_artifact = value,
                                ["adapter", "sha256"] => model.adapter_sha256 = value,
                                ["adapter", "base_blake3"] => model.adapter_base_blake3 = value,
                                ["adapter", "scale"] => model.adapter_scale = value,
                                _ => return Err(unknown_field(&key)),
                            }
                        }
                        _ => return Err(unknown_field(&key)),
                    }
                }
                _ => return Err(unknown_field(&key)),
            }
        }
        Ok(form)
    }

    /// **The document's JSON value the form describes**: empty slots left
    /// out, text kept as typed, numbers parsed by their kind. The value is
    /// then judged by [`Document::parse`], so the form and a JSON document
    /// meet one set of refusals.
    pub fn to_value(&self) -> Result<Value, Refused> {
        let mut top = Map::new();
        if !self.name.is_empty() {
            top.insert("name".into(), json!(self.name));
        }
        if !self.placement.is_empty() {
            let mut placement = json!({"kind": self.placement});
            if self.placement == "virtual-machine" {
                placement["template"] = json!(self.template);
            }
            top.insert("placement".into(), placement);
        }
        let mut boxes = Vec::new();
        for (i, b) in self.boxes.iter().enumerate() {
            if b.is_empty() {
                continue;
            }
            let mut models = Vec::new();
            for (j, m) in b.models.iter().enumerate().filter(|(_, m)| !m.is_empty()) {
                let mut model = Map::new();
                if !m.function.is_empty() {
                    model.insert("function".into(), json!(m.function));
                }
                if !m.artifact.is_empty() {
                    model.insert("artifact".into(), json!(m.artifact));
                }
                model.insert("format".into(), json!("gguf"));
                if m.has_adapter() {
                    let scale = if m.adapter_scale.trim().is_empty() {
                        Value::Null
                    } else {
                        match m.adapter_scale.trim().parse::<f64>() {
                            Ok(x) if x.is_finite() => json!(x),
                            _ => {
                                return Err(refused(
                                    format!("boxes[{i}].models[{j}].adapter.scale"),
                                    Kind::Adapter,
                                    "is a finite number",
                                ));
                            }
                        }
                    };
                    model.insert(
                        "adapter".into(),
                        json!({
                            "artifact": m.adapter_artifact,
                            "sha256": m.adapter_sha256,
                            "base_blake3": m.adapter_base_blake3,
                            "scale": scale,
                        }),
                    );
                }
                models.push(Value::Object(model));
            }
            let mut fbox = json!({"models": models});
            let parts: Vec<Option<Value>> = b
                .gpu
                .iter()
                .enumerate()
                .map(|(k, (kind, text))| form_part(i, k, kind, text))
                .collect::<Result<_, _>>()?;
            match (&parts[0], &parts[1]) {
                (None, None) => {}
                (Some(first), None) => fbox["gpu"] = first.clone(),
                (first, Some(second)) => {
                    fbox["gpu"] = json!({"pair": [first.clone().unwrap_or(json!({})), second]})
                }
            }
            boxes.push(fbox);
        }
        top.insert("boxes".into(), Value::Array(boxes));
        let mut sampler = Map::new();
        for (name, text) in &self.sampler {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            let real = SAMPLER.iter().any(|(n, r)| n == name && *r);
            let field = format!("sampler.{name}");
            let value = if real {
                match text.parse::<f64>() {
                    Ok(x) if x.is_finite() => json!(x),
                    _ => return Err(refused(field, Kind::Sampler, "is a finite number")),
                }
            } else {
                match text.parse::<u64>() {
                    Ok(x) => json!(x),
                    Err(_) => {
                        return Err(refused(
                            field,
                            Kind::Sampler,
                            "is a count: a whole number at or above zero",
                        ));
                    }
                }
            };
            sampler.insert((*name).to_owned(), value);
        }
        top.insert("sampler".into(), Value::Object(sampler));
        if !self.store.is_empty() {
            top.insert("store".into(), json!(self.store));
        }
        let tools: Vec<&str> = if self.sh { vec!["sh"] } else { Vec::new() };
        top.insert("gate".into(), json!({ "tools": tools }));
        top.insert(
            "prompt".into(),
            if self.prompt.is_empty() {
                Value::Null
            } else {
                json!(self.prompt)
            },
        );
        Ok(Value::Object(top))
    }

    /// **The document the form describes, judged for a save**:
    /// [`Form::to_value`], then [`Document::parse`] and
    /// [`Document::serving`], a refusal's box and model renumbered to the
    /// screen's slots, since the value leaves an empty slot out. Every
    /// refusal the screen shows names a field by the document's path and
    /// the screen's own numbering.
    pub fn document(&self) -> Result<Document, Refused> {
        let value = self.to_value()?;
        Document::parse(&value)
            .and_then(|document| document.serving().map(|()| document))
            .map_err(|mut refusal| {
                refusal.field = self.slot_path(&refusal.field);
                refusal
            })
    }

    /// A document path's box and model as the screen numbers its slots.
    fn slot_path(&self, field: &str) -> String {
        let renumber = || -> Option<String> {
            let (i, rest) = field.strip_prefix("boxes[")?.split_once(']')?;
            let i: usize = i.parse().ok()?;
            let (bi, fbox) = self
                .boxes
                .iter()
                .enumerate()
                .filter(|(_, b)| !b.is_empty())
                .nth(i)?;
            match rest.strip_prefix(".models[") {
                Some(rest) => {
                    let (j, tail) = rest.split_once(']')?;
                    let j: usize = j.parse().ok()?;
                    let (mj, _) = fbox
                        .models
                        .iter()
                        .enumerate()
                        .filter(|(_, m)| !m.is_empty())
                        .nth(j)?;
                    Some(format!("boxes[{bi}].models[{mj}]{tail}"))
                }
                None => Some(format!("boxes[{bi}]{rest}")),
            }
        };
        renumber().unwrap_or_else(|| field.to_owned())
    }

    /// **The form a saved document fills**, with `extra_boxes` empty boxes
    /// after its own and one empty model slot in each box, at least two.
    pub fn of(document: &Document, extra_boxes: usize) -> Form {
        let mut form = Form {
            name: document.name.clone(),
            placement: match &document.placement {
                Placement::Host => "host".to_owned(),
                Placement::VirtualMachine { .. } => "virtual-machine".to_owned(),
            },
            template: match &document.placement {
                Placement::Host => String::new(),
                Placement::VirtualMachine { template } => template.clone(),
            },
            boxes: document
                .boxes
                .iter()
                .map(|b| FormBox {
                    models: b
                        .models
                        .iter()
                        .map(|m| FormModel {
                            function: m.function.word().to_owned(),
                            artifact: m.artifact.clone(),
                            adapter_artifact: m
                                .adapter
                                .as_ref()
                                .map(|a| a.artifact.clone())
                                .unwrap_or_default(),
                            adapter_sha256: m
                                .adapter
                                .as_ref()
                                .map(|a| a.sha256.clone())
                                .unwrap_or_default(),
                            adapter_base_blake3: m
                                .adapter
                                .as_ref()
                                .map(|a| a.base_blake3.clone())
                                .unwrap_or_default(),
                            adapter_scale: m
                                .adapter
                                .as_ref()
                                .map(|a| a.scale.to_string())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    gpu: match &b.gpu {
                        Requirement::Card(_) | Requirement::Class(_) => {
                            [form_slot_of(&b.gpu), (String::new(), String::new())]
                        }
                        Requirement::Pair(parts) => [part_slot(&parts[0]), part_slot(&parts[1])],
                    },
                })
                .collect(),
            sampler: SAMPLER
                .iter()
                .map(|(n, _)| {
                    let text = document
                        .sampler
                        .present()
                        .into_iter()
                        .find(|(m, _)| m == n)
                        .map(|(_, v)| match v {
                            Value64::Real(x) => x.to_string(),
                            Value64::Count(x) => x.to_string(),
                        })
                        .unwrap_or_default();
                    (*n, text)
                })
                .collect(),
            store: document.store.word().to_owned(),
            sh: !document.tools.is_empty(),
            prompt: document.prompt.clone().unwrap_or_default(),
        };
        form.boxes
            .extend(std::iter::repeat_n(FormBox::default(), extra_boxes));
        form.slotted();
        form
    }

    /// **The empty form**, with `boxes` empty boxes, at least one, and the
    /// five [`STARTING`] values filled in, the other three tunable values
    /// empty.
    pub fn empty(boxes: usize) -> Form {
        let mut form = Form {
            sampler: SAMPLER
                .iter()
                .map(|(n, _)| {
                    let start = STARTING.iter().find(|(m, _)| m == n).map(|(_, v)| *v);
                    (*n, start.unwrap_or_default().to_owned())
                })
                .collect(),
            boxes: vec![FormBox::default(); boxes.max(1)],
            ..Form::default()
        };
        form.slotted();
        form
    }

    /// Every box given one empty model slot past its last filled one, at
    /// least two in all, within the bounds. **No slot moves**, so a refusal
    /// naming `b0.m3` names the row the screen shows as `b0.m3`.
    pub fn slotted(&mut self) {
        self.boxes.truncate(MAX_BOXES);
        for b in &mut self.boxes {
            let used = b
                .models
                .iter()
                .rposition(|m| !m.is_empty())
                .map_or(0, |last| last + 1);
            b.models.truncate(used);
            b.models.resize(
                (used + 1).clamp(2, MAX_MODELS).max(used),
                FormModel::default(),
            );
        }
    }
}

fn unknown_field(key: &str) -> Refused {
    refused(
        key,
        Kind::Unknown,
        "is not a field of the declaration screen",
    )
}

fn form_slot_of(requirement: &Requirement) -> (String, String) {
    match requirement {
        Requirement::Card(card) => ("card".to_owned(), card.clone()),
        Requirement::Class(class) => class_slot(class),
        Requirement::Pair(_) => (String::new(), String::new()),
    }
}

fn part_slot(part: &Part) -> (String, String) {
    match part {
        Part::Card(card) => ("card".to_owned(), card.clone()),
        Part::Class(class) => class_slot(class),
    }
}

fn class_slot(class: &Class) -> (String, String) {
    match class {
        Class::Family(family) => ("family".to_owned(), family.clone()),
        Class::Models(models) => ("models".to_owned(), models.join(", ")),
    }
}

/// One part of a requirement as the form gives it: a kind and its text,
/// none where both are empty.
fn form_part(i: usize, k: usize, kind: &str, text: &str) -> Result<Option<Value>, Refused> {
    let field = if k == 0 {
        format!("boxes[{i}].gpu")
    } else {
        format!("boxes[{i}].gpu.pair[{k}]")
    };
    Ok(match kind {
        "" if text.is_empty() => None,
        "" => {
            return Err(refused(
                field,
                Kind::Missing,
                "is required: a card, a family or a list of models",
            ));
        }
        "card" => Some(json!({"card": text})),
        "family" => Some(json!({"class": {"family": text}})),
        "models" => {
            let models: Vec<String> = text.split(',').map(|m| m.trim().to_owned()).collect();
            Some(json!({"class": {"models": models}}))
        }
        _ => {
            return Err(refused(
                field,
                Kind::Shape,
                "is a card, a family or a list of models",
            ));
        }
    })
}
