//! **The survey's reader** (Spec 8.3): the box's survey report, which the
//! setup keeps verbatim, read as `weaver-survey/2`, the format stated in the
//! suite repository's `docs/profiles/weaver-survey-format.md`, on its draft
//! pull request 57 at `1415dea092d53f5bee2cb85bbd704fbb63e4f057`, the
//! document's sha256
//! `a4d45bd3e4c7080c00e6721fe0899ab11d0c65f09ffa9720d202c8912f3abefe`.
//! A report of `weaver-survey/1` is refused by its version's name (the
//! format's section 7), and no member of it is read.
//!
//! A report is read whole or not at all: any cause of the format's section 3
//! refuses it, and no member of a refused report is read. The reader first
//! parses the text into a tree that keeps every name an object gives, twice
//! or not, and how each number was written, so that two of the format's
//! rules are judged on what the box sent and never on what a parser made of
//! it: a name given twice at any depth, and an integer by its written form.
//! Then it reads the format's members from the tree, and ignores every
//! member it does not know, at any depth.
//!
//! **The two mount points are the report's values**, as the operator chose
//! them at the setup script (the operator's rulings of 2026-11-24 and
//! 2026-11-25: "The two mount paths are hard coded nowhere."): the reader
//! holds no place, and judges each by the format's one spelling (its
//! section 1.1), the two different and neither inside the other. **Each
//! plane's address** is parsed and written again in that spelling, and one
//! whose bytes differ is refused, as is one of a kind section 1.1 refuses.

use std::collections::HashSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::value::RawValue;

/// The one version this reader knows.
pub const FORMAT: &str = "weaver-survey/2";
/// The version before it, which a refusal names (the format's section 7).
pub const EARLIER: &str = "weaver-survey/1";
/// The bound on a report, the one admin-con enforces.
pub const REPORT_BOUND: usize = 65536;
/// A place's bound, the whole and each part (the format's section 1.1).
pub const PLACE_BOUND: usize = 255;
/// The largest integer the format admits, 2^53 - 1.
pub const INTEGER_BOUND: u64 = (1 << 53) - 1;
/// At most this many cards.
pub const CARDS_BOUND: usize = 16;
/// At most this many CPU flags, and at least one.
pub const FLAGS_BOUND: usize = 512;

/// **The bytes of a value the report chose that a refusal's detail quotes**,
/// at most: enough to find the value, never the report's text.
pub const QUOTE_BOUND: usize = 64;

/// **A report read whole.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    pub taken: DateTime<Utc>,
    pub image: Image,
    pub kernel_release: String,
    pub machine: Machine,
    pub cpu: Cpu,
    pub memory_total_bytes: u64,
    pub driver: Driver,
    pub cuda_runtime_version: String,
    pub cards: Vec<Card>,
    /// Exactly two, the state drive then the models drive.
    pub drives: Vec<Drive>,
    pub network: Network,
}

/// **The two planes' addresses**, as the operator chose them, and whether
/// the planes are separate or shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub management: Plane,
    pub data: Plane,
    pub planes: Planes,
}

/// A plane's address and the length of its network's prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plane {
    pub address: IpAddr,
    pub prefix_length: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Planes {
    Separate,
    Shared,
}

impl Planes {
    /// The value by the format's word.
    pub fn word(self) -> &'static str {
        match self {
            Planes::Separate => "separate",
            Planes::Shared => "shared",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub version: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    pub virtualization: String,
    pub vendor: String,
    pub product: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cpu {
    pub vendor: String,
    pub model_name: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub logical_cpus: u32,
    /// In byte order, each once.
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    pub version: String,
    pub loaded: bool,
}

/// **A card**, in PCI bus order. Its model is the format's `name`, which
/// names the card in the deployment tuple.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub ordinal: u32,
    pub model: String,
    pub pci_address: String,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    State,
    Models,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    pub role: Role,
    pub mount_point: String,
    pub mount: Mount,
}

/// What a drive carries: nothing more where no filesystem is mounted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mount {
    NotMounted,
    Mounted {
        label: String,
        filesystem: String,
        bytes: u64,
        free_bytes: u64,
        project_quota: bool,
    },
}

/// **The format's causes**, its section 3, numbered as it numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Encoding,
    TooLarge,
    NameTwice,
    Format,
    Member,
    Integer,
    String,
    StageOrTaken,
    Cards,
    Drives,
    Bound,
    Network,
}

impl Cause {
    /// The cause's number in the format's section 3.
    pub fn number(self) -> u8 {
        match self {
            Cause::Encoding => 1,
            Cause::TooLarge => 2,
            Cause::NameTwice => 3,
            Cause::Format => 4,
            Cause::Member => 5,
            Cause::Integer => 6,
            Cause::String => 7,
            Cause::StageOrTaken => 8,
            Cause::Cards => 9,
            Cause::Drives => 10,
            Cause::Bound => 11,
            Cause::Network => 12,
        }
    }

    /// The cause in the format's words.
    pub fn words(self) -> &'static str {
        match self {
            Cause::Encoding => {
                "bytes that are not UTF-8, that are not one JSON object, or that carry \
                 anything after the object except one newline"
            }
            Cause::TooLarge => "more than 64 KiB",
            Cause::NameTwice => "a member name given twice in one object, at any depth",
            Cause::Format => {
                "a format that is missing, or is not exactly a version string the reader knows"
            }
            Cause::Member => {
                "a member of section 1 that is missing or null, or of the wrong JSON type"
            }
            Cause::Integer => {
                "an integer that is negative, written with a sign (-0 included), fractional, \
                 written with an exponent, or above 2^53 - 1"
            }
            Cause::String => {
                "a string outside printable ASCII, over its bound, or not matching its pattern"
            }
            Cause::StageOrTaken => {
                "a stage other than pairing, or a taken not in section 1's form, a leap second \
                 included"
            }
            Cause::Cards => {
                "a card whose ordinal is not its index, or pci_address values that do not \
                 strictly ascend by number"
            }
            Cause::Drives => {
                "drives that is not exactly state then models, a mount_point not in the one \
                 spelling of section 1.1, two equal mount points or one inside the other, or a \
                 drive with mounted false that carries a mounted drive's members"
            }
            Cause::Bound => {
                "an integer outside its bound in section 1, more than 16 cards, or a cpu.flags \
                 with no entry, more than 512 entries, entries out of byte order, or an entry \
                 given twice"
            }
            Cause::Network => {
                "a plane's address not in the one spelling of section 1.1, or of a kind it \
                 refuses; a prefix length outside its bound for its address's family; a \
                 network.planes other than separate or shared; or separate where the two \
                 addresses are equal, or where either address lies in the other's network"
            }
        }
    }
}

/// **A refused report**: the cause, and where the reader met it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub cause: Cause,
    pub detail: String,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "refused by {FORMAT}'s cause {}: {} ({})",
            self.cause.number(),
            self.cause.words(),
            self.detail
        )
    }
}

impl std::error::Error for Refused {}

fn refused<T>(cause: Cause, detail: impl Into<String>) -> Result<T, Refused> {
    Err(Refused {
        cause,
        detail: detail.into(),
    })
}

/// **Read a report**, its bytes as the setup keeps them.
pub fn read(report: &[u8]) -> Result<Survey, Refused> {
    if report.len() > REPORT_BOUND {
        return refused(Cause::TooLarge, format!("{} bytes", report.len()));
    }
    let tree = parse(report)?;
    if let Some(path) = name_twice(&tree, "") {
        return refused(Cause::NameTwice, path);
    }
    let Node::Object(top) = &tree else {
        return refused(Cause::Encoding, "the report is not one JSON object");
    };
    let top = Members::new(top, "");
    format_of(&top)?;
    members(&top)
}

/// **Whether a report reads as this version, by its `format` alone** (Spec
/// 8.2): what the pairing's first frame asks of a report before the box is
/// listed, and the one thing it answers. No where the bytes are past the
/// bound, are not one JSON object in UTF-8 with nothing after it but one
/// newline (the reader's own grammar), or give `format` other than once at
/// the top as this version's string. No other member is judged, so a report
/// of this version refused for another cause passes, and the pages show its
/// cause. It builds no detail of its own, since nothing at the frame would
/// read one (the Planner's ruling of 2026-11-25).
pub fn format_taken(report: &[u8]) -> bool {
    if report.len() > REPORT_BOUND {
        return false;
    }
    let Ok(Node::Object(top)) = parse(report) else {
        return false;
    };
    let mut formats = top.iter().filter(|(name, _)| name.as_slice() == b"format");
    matches!(
        (formats.next(), formats.next()),
        (Some((_, Node::String(format))), None) if format == FORMAT
    )
}

/// **The top's `format`**: this version, or refused naming what it is.
fn format_of(top: &Members<'_>) -> Result<(), Refused> {
    match top.find("format") {
        Some(Node::String(format)) if format == FORMAT => Ok(()),
        // **The version read and the version taken** (the format's section
        // 7: the reader "refuses a report of version 1 by its version's
        // name").
        Some(Node::String(other)) => {
            let earlier = if other == EARLIER {
                ", the earlier version, which this reader does not read"
            } else {
                ""
            };
            refused(
                Cause::Format,
                format!("format {} read{earlier}; {FORMAT} taken", quoted(other)),
            )
        }
        Some(_) => refused(Cause::Format, "format is not a string"),
        None => refused(Cause::Format, "format is missing"),
    }
}

/// **The tree**: every name an object gives, in order and twice where it is
/// given twice, and each number as it was written.
#[derive(Debug)]
enum Node {
    Null,
    Bool(bool),
    /// A number written as digits alone, within 2^64 - 1.
    Whole(u64),
    /// Any other number: a minus sign, `-0` included, a fraction part, an
    /// exponent, or past 2^64 - 1.
    OtherNumber,
    /// A string, decoded, a lone surrogate escape as U+FFFD, which no
    /// string of section 1 admits.
    String(String),
    Array(Vec<Node>),
    Object(Vec<(Name, Node)>),
}

/// **A member's name as its bytes**: UTF-8, a lone surrogate escape kept as
/// its WTF-8 bytes, so two names are one name exactly where their UTF-16
/// code units are (RFC 8259 section 8.3).
type Name = Vec<u8>;

/// Nesting past this many arrays and objects is refused, the bound
/// `serde_json`'s own parser keeps (its 128th level refused), since the
/// tree is built by recursion.
const DEPTH_BOUND: usize = 127;

/// The text into the tree: UTF-8, one JSON value, and nothing after it but
/// one newline. Whitespace before the value is free. `serde_json` checks
/// the grammar and hands each value over as its text, so no number is
/// parsed but by its written form, and a number past any float is a number
/// like another.
fn parse(report: &[u8]) -> Result<Node, Refused> {
    let Ok(text) = std::str::from_utf8(report) else {
        return refused(Cause::Encoding, "the bytes are not UTF-8");
    };
    let mut values = serde_json::Deserializer::from_str(text).into_iter::<&RawValue>();
    let raw = match values.next() {
        Some(Ok(raw)) => raw,
        Some(Err(e)) => return refused(Cause::Encoding, e.to_string()),
        None => return refused(Cause::Encoding, "the report is empty"),
    };
    let rest = &text[values.byte_offset()..];
    if !rest.is_empty() && rest != "\n" {
        return refused(
            Cause::Encoding,
            "something follows the object other than one newline",
        );
    }
    node(raw.get(), 0).map_err(|detail| Refused {
        cause: Cause::Encoding,
        detail,
    })
}

/// One value's text into the tree, its arrays and objects read again
/// member by member.
fn node(text: &str, depth: usize) -> Result<Node, String> {
    let nested = |depth: usize| {
        if depth >= DEPTH_BOUND {
            Err(format!(
                "the report nests past {DEPTH_BOUND} arrays and objects"
            ))
        } else {
            Ok(())
        }
    };
    match text.as_bytes().first() {
        Some(b'{') => {
            nested(depth)?;
            let Entries(entries) = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let mut members = Vec::with_capacity(entries.len());
            for (name, value) in entries {
                members.push((name, node(value.get(), depth + 1)?));
            }
            Ok(Node::Object(members))
        }
        Some(b'[') => {
            nested(depth)?;
            let items: Vec<&RawValue> = serde_json::from_str(text).map_err(|e| e.to_string())?;
            let mut nodes = Vec::with_capacity(items.len());
            for item in items {
                nodes.push(node(item.get(), depth + 1)?);
            }
            Ok(Node::Array(nodes))
        }
        Some(b'"') => serde_json::from_str(text)
            .map(|Bytes(bytes)| Node::String(String::from_utf8_lossy(&bytes).into_owned()))
            .map_err(|e| e.to_string()),
        Some(b't') => Ok(Node::Bool(true)),
        Some(b'f') => Ok(Node::Bool(false)),
        Some(b'n') => Ok(Node::Null),
        _ => Ok(number(text)),
    }
}

/// **A number by its written form**: digits alone that fit a `u64` are a
/// whole number, and every other form is not one.
fn number(text: &str) -> Node {
    if text.bytes().all(|b| b.is_ascii_digit()) {
        text.parse().map_or(Node::OtherNumber, Node::Whole)
    } else {
        Node::OtherNumber
    }
}

/// An object's members as the text gives them, a name given twice kept
/// twice, each value as its text.
struct Entries<'a>(Vec<(Name, &'a RawValue)>);

impl<'de: 'a, 'a> Deserialize<'de> for Entries<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Entries<'a>, D::Error> {
        struct EntriesVisitor<'a>(std::marker::PhantomData<&'a ()>);

        impl<'de: 'a, 'a> Visitor<'de> for EntriesVisitor<'a> {
            type Value = Entries<'a>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entries<'a>, A::Error> {
                let mut entries = Vec::new();
                while let Some((Bytes(name), value)) = map.next_entry::<Bytes, &'a RawValue>()? {
                    entries.push((name, value));
                }
                Ok(Entries(entries))
            }
        }

        deserializer.deserialize_map(EntriesVisitor(std::marker::PhantomData))
    }
}

/// **A JSON string as its bytes**, its escapes decoded and a lone surrogate
/// kept as WTF-8, where decoding to a `String` would refuse it.
struct Bytes(Vec<u8>);

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Bytes, D::Error> {
        struct BytesVisitor;

        impl Visitor<'_> for BytesVisitor {
            type Value = Bytes;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a JSON string")
            }

            fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Bytes, E> {
                Ok(Bytes(bytes.to_vec()))
            }
        }

        deserializer.deserialize_bytes(BytesVisitor)
    }
}

/// The path of the first object, at any depth, that gives a name twice.
fn name_twice(node: &Node, path: &str) -> Option<String> {
    match node {
        Node::Object(entries) => {
            let mut seen = HashSet::new();
            for (name, _) in entries {
                if !seen.insert(name.as_slice()) {
                    return Some(format!(
                        "{} gives {} twice",
                        cut(shown(path)),
                        quoted(&text(name))
                    ));
                }
            }
            entries
                .iter()
                .find_map(|(name, value)| name_twice(value, &join(path, &text(name))))
        }
        Node::Array(items) => items
            .iter()
            .enumerate()
            .find_map(|(i, item)| name_twice(item, &format!("{path}[{i}]"))),
        _ => None,
    }
}

/// A name as text, a lone surrogate as U+FFFD.
fn text(name: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(name)
}

/// **A value the report chose, as a refusal's detail quotes it**, for the
/// console's log and never a page (Spec 6.2): between quotes, as `cut`
/// gives it.
fn quoted(value: &str) -> String {
    format!("\"{}\"", cut(value))
}

/// **A value cut to `QUOTE_BOUND` bytes and escaped**: its first bytes to
/// the bound, at a character's boundary, each byte outside printable ASCII
/// and the backslash written `\xNN` by the box's printer, and where it was
/// cut, the value's length.
fn cut(value: &str) -> String {
    let mut end = value.len().min(QUOTE_BOUND);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    let shown = crate::link::box_line::escaped(&value[..end]);
    if end < value.len() {
        format!("{shown}... (cut, {} bytes)", value.len())
    } else {
        shown
    }
}

fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{path}.{name}")
    }
}

fn shown(path: &str) -> &str {
    if path.is_empty() { "the report" } else { path }
}

/// An object's members, by name, with its path for a refusal's detail.
/// The tree holds no name twice once `name_twice` has passed it.
struct Members<'a> {
    entries: &'a [(Name, Node)],
    path: String,
}

impl<'a> Members<'a> {
    fn new(entries: &'a [(Name, Node)], path: &str) -> Members<'a> {
        Members {
            entries,
            path: path.to_owned(),
        }
    }

    fn find(&self, name: &str) -> Option<&'a Node> {
        self.entries
            .iter()
            .find(|(n, _)| n.as_slice() == name.as_bytes())
            .map(|(_, v)| v)
    }

    fn has(&self, name: &str) -> bool {
        self.find(name).is_some()
    }

    fn at(&self, name: &str) -> String {
        join(&self.path, name)
    }

    /// A member that must stand and not be null.
    fn get(&self, name: &str) -> Result<&'a Node, Refused> {
        match self.find(name) {
            None => refused(Cause::Member, format!("{} is missing", self.at(name))),
            Some(Node::Null) => refused(Cause::Member, format!("{} is null", self.at(name))),
            Some(node) => Ok(node),
        }
    }

    fn object(&self, name: &str) -> Result<Members<'a>, Refused> {
        match self.get(name)? {
            Node::Object(entries) => Ok(Members::new(entries, &self.at(name))),
            _ => refused(Cause::Member, format!("{} is not an object", self.at(name))),
        }
    }

    fn array(&self, name: &str) -> Result<&'a [Node], Refused> {
        match self.get(name)? {
            Node::Array(items) => Ok(items),
            _ => refused(Cause::Member, format!("{} is not an array", self.at(name))),
        }
    }

    fn boolean(&self, name: &str) -> Result<bool, Refused> {
        match self.get(name)? {
            Node::Bool(b) => Ok(*b),
            _ => refused(Cause::Member, format!("{} is not a boolean", self.at(name))),
        }
    }

    /// An integer by its written form, within 2^53 - 1.
    fn integer(&self, name: &str) -> Result<u64, Refused> {
        match self.get(name)? {
            Node::Whole(n) if *n <= INTEGER_BOUND => Ok(*n),
            Node::Whole(_) | Node::OtherNumber => refused(
                Cause::Integer,
                format!(
                    "{} is not an integer from 0 to 2^53 - 1 as written",
                    self.at(name)
                ),
            ),
            _ => refused(Cause::Member, format!("{} is not a number", self.at(name))),
        }
    }

    /// An integer within the bound section 1 gives it.
    fn bounded(&self, name: &str, low: u64, high: u64) -> Result<u32, Refused> {
        let n = self.integer(name)?;
        if n < low || n > high {
            return refused(
                Cause::Bound,
                format!("{} is {n}, outside {low} to {high}", self.at(name)),
            );
        }
        Ok(n as u32)
    }

    fn raw_string(&self, name: &str) -> Result<&'a str, Refused> {
        match self.get(name)? {
            Node::String(s) => Ok(s),
            _ => refused(Cause::Member, format!("{} is not a string", self.at(name))),
        }
    }

    /// A string of printable ASCII within `bound` bytes and matching `pattern`.
    fn string(
        &self,
        name: &str,
        bound: usize,
        pattern: fn(&str) -> bool,
    ) -> Result<String, Refused> {
        let s = self.raw_string(name)?;
        string_checked(s, bound, pattern, || self.at(name))?;
        Ok(s.to_owned())
    }
}

fn string_checked(
    s: &str,
    bound: usize,
    pattern: fn(&str) -> bool,
    at: impl Fn() -> String,
) -> Result<(), Refused> {
    if !s.bytes().all(|b| (0x20..=0x7e).contains(&b)) {
        return refused(Cause::String, format!("{} is not printable ASCII", at()));
    }
    if s.len() > bound {
        return refused(
            Cause::String,
            format!("{} is {} bytes, over {bound}", at(), s.len()),
        );
    }
    if !pattern(s) {
        return refused(
            Cause::String,
            format!("{} does not match its pattern", at()),
        );
    }
    Ok(())
}

/// No pattern beyond printable ASCII and the bound.
fn any(_: &str) -> bool {
    true
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Dot-separated runs of digits, between `low` and `high` of them.
fn dotted(s: &str, low: usize, high: usize) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    (low..=high).contains(&parts.len()) && parts.iter().all(|p| digits(p))
}

/// `^[0-9]+\.[0-9]+$`
fn image_version(s: &str) -> bool {
    dotted(s, 2, 2)
}

/// 64 lowercase hex digits.
fn sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `^[0-9]+(\.[0-9]+){1,2}$`
fn driver_version(s: &str) -> bool {
    dotted(s, 2, 3)
}

/// `^[0-9]+\.[0-9]+(\.[0-9]+)?$`
fn cuda_version(s: &str) -> bool {
    dotted(s, 2, 3)
}

/// `^[a-z0-9_]{1,32}$`
fn flag(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'))
}

/// `^[0-9a-f]{4,8}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]$`
fn pci_address(s: &str) -> bool {
    let hex = |p: &str| p.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    let Some((rest, function)) = s.rsplit_once('.') else {
        return false;
    };
    let parts: Vec<&str> = rest.split(':').collect();
    matches!(function.as_bytes(), [b'0'..=b'7'])
        && parts.len() == 3
        && (4..=8).contains(&parts[0].len())
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts.iter().all(|p| hex(p))
}

/// `taken`: RFC 3339 in UTC, whole seconds, `Z`, a real date and time, the
/// seconds from 00 to 59, so a leap second is refused (chrono's `%S` takes
/// `60`).
fn taken(s: &str) -> Option<DateTime<Utc>> {
    let b = s.as_bytes();
    let shape = b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        });
    if !shape || &s[17..19] > "59" {
        return None;
    }
    NaiveDateTime::parse_from_str(&s[..19], "%Y-%m-%dT%H:%M:%S")
        .ok()
        .map(|t| t.and_utc())
}

/// **Every member of section 1**, the format string already read.
fn members(top: &Members<'_>) -> Result<Survey, Refused> {
    match top.raw_string("stage")? {
        "pairing" => {}
        other => return refused(Cause::StageOrTaken, format!("stage {}", quoted(other))),
    }
    let taken_text = top.raw_string("taken")?;
    let Some(taken) = taken(taken_text) else {
        return refused(Cause::StageOrTaken, format!("taken {}", quoted(taken_text)));
    };

    let image = top.object("image")?;
    let image = Image {
        version: image.string("version", REPORT_BOUND, image_version)?,
        manifest_sha256: image.string("manifest_sha256", 64, sha256_hex)?,
    };
    let kernel_release = top.object("kernel")?.string("release", 64, any)?;

    let machine = top.object("machine")?;
    let machine = Machine {
        virtualization: machine.string("virtualization", 32, any)?,
        vendor: machine.string("vendor", 128, any)?,
        product: machine.string("product", 128, any)?,
        version: machine.string("version", 128, any)?,
    };

    let cpu = top.object("cpu")?;
    let cpu = Cpu {
        vendor: cpu.string("vendor", 32, any)?,
        model_name: cpu.string("model_name", 128, any)?,
        family: cpu.bounded("family", 0, 65535)?,
        model: cpu.bounded("model", 0, 65535)?,
        stepping: cpu.bounded("stepping", 0, 65535)?,
        logical_cpus: cpu.bounded("logical_cpus", 1, 4096)?,
        flags: flags(&cpu)?,
    };

    let memory_total_bytes = top.object("memory")?.integer("total_bytes")?;
    let driver = top.object("driver")?;
    let driver = Driver {
        version: driver.string("version", REPORT_BOUND, driver_version)?,
        loaded: driver.boolean("loaded")?,
    };
    let cuda_runtime_version =
        top.object("cuda_runtime")?
            .string("version", REPORT_BOUND, cuda_version)?;

    Ok(Survey {
        taken,
        image,
        kernel_release,
        machine,
        cpu,
        memory_total_bytes,
        driver,
        cuda_runtime_version,
        cards: cards(top)?,
        drives: drives(top)?,
        network: network(top)?,
    })
}

fn flags(cpu: &Members<'_>) -> Result<Vec<String>, Refused> {
    let items = cpu.array("flags")?;
    let at = cpu.at("flags");
    let mut flags = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let Node::String(s) = item else {
            return refused(Cause::Member, format!("{at}[{i}] is not a string"));
        };
        string_checked(s, 32, flag, || format!("{at}[{i}]"))?;
        flags.push(s.clone());
    }
    if flags.is_empty() || flags.len() > FLAGS_BOUND {
        return refused(
            Cause::Bound,
            format!(
                "{at} has {} entries, outside 1 to {FLAGS_BOUND}",
                flags.len()
            ),
        );
    }
    if let Some(w) = flags
        .windows(2)
        .find(|w| w[0].as_bytes() >= w[1].as_bytes())
    {
        return refused(
            Cause::Bound,
            format!("{at} gives {} before {}", quoted(&w[0]), quoted(&w[1])),
        );
    }
    Ok(flags)
}

fn cards(top: &Members<'_>) -> Result<Vec<Card>, Refused> {
    let items = top.array("cards")?;
    if items.len() > CARDS_BOUND {
        return refused(
            Cause::Bound,
            format!("cards has {} entries, over {CARDS_BOUND}", items.len()),
        );
    }
    let mut cards: Vec<Card> = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let path = format!("cards[{i}]");
        let Node::Object(entries) = item else {
            return refused(Cause::Member, format!("{path} is not an object"));
        };
        let card = Members::new(entries, &path);
        let ordinal = card.integer("ordinal")?;
        let model = card.string("name", 96, any)?;
        let pci_address = card.string("pci_address", REPORT_BOUND, pci_address)?;
        let memory_bytes = card.integer("memory_bytes")?;
        if ordinal != i as u64 {
            return refused(
                Cause::Cards,
                format!("{path}.ordinal is {ordinal}, not its index"),
            );
        }
        if let Some(before) = cards.last()
            && bus_order(&before.pci_address) >= bus_order(&pci_address)
        {
            return refused(
                Cause::Cards,
                format!("{path}.pci_address does not ascend by number"),
            );
        }
        cards.push(Card {
            ordinal: ordinal as u32,
            model,
            pci_address,
            memory_bytes,
        });
    }
    Ok(cards)
}

/// The members a mounted drive carries and a drive not mounted does not.
const MOUNTED_MEMBERS: [&str; 5] = [
    "label",
    "filesystem",
    "bytes",
    "free_bytes",
    "project_quota",
];

fn drives(top: &Members<'_>) -> Result<Vec<Drive>, Refused> {
    let items = top.array("drives")?;
    let expected = [(Role::State, "state"), (Role::Models, "models")];
    let mut drives = Vec::with_capacity(2);
    for (i, item) in items.iter().enumerate() {
        let path = format!("drives[{i}]");
        let Node::Object(entries) = item else {
            return refused(Cause::Member, format!("{path} is not an object"));
        };
        let drive = Members::new(entries, &path);
        let role_text = drive.raw_string("role")?;
        let mount_point = drive.raw_string("mount_point")?;
        let mounted = drive.boolean("mounted")?;
        let Some(&(role, _)) = expected.get(i).filter(|(_, name)| role_text == *name) else {
            return refused(
                Cause::Drives,
                format!("{path} is {}, not state then models", quoted(role_text)),
            );
        };
        if !place(mount_point) {
            return refused(
                Cause::Drives,
                format!(
                    "{path}.mount_point {} is not a place in the one spelling",
                    quoted(mount_point)
                ),
            );
        }
        let mount = if mounted {
            Mount::Mounted {
                label: drive.string("label", 16, any)?,
                filesystem: drive.string("filesystem", 16, any)?,
                bytes: drive.integer("bytes")?,
                free_bytes: drive.integer("free_bytes")?,
                project_quota: drive.boolean("project_quota")?,
            }
        } else {
            if let Some(member) = MOUNTED_MEMBERS.iter().find(|m| drive.has(m)) {
                return refused(
                    Cause::Drives,
                    format!("{path} is not mounted and carries {member}"),
                );
            }
            Mount::NotMounted
        };
        drives.push(Drive {
            role,
            mount_point: mount_point.to_owned(),
            mount,
        });
    }
    if drives.len() != expected.len() {
        return refused(
            Cause::Drives,
            format!("drives has {} entries, not state then models", drives.len()),
        );
    }
    let (state, models) = (&drives[0].mount_point, &drives[1].mount_point);
    if state == models || inside(state, models) || inside(models, state) {
        return refused(
            Cause::Drives,
            format!(
                "the mount points {} and {} are equal, or one is inside the other",
                quoted(state),
                quoted(models)
            ),
        );
    }
    Ok(drives)
}

/// **A place in the one spelling** (the format's section 1.1): `/`, then
/// one or more parts each separated by one `/`, no part empty, `.` or
/// `..`, each 1 to 255 of `A-Z a-z 0-9 . _ -`, the whole at most 255
/// bytes.
fn place(s: &str) -> bool {
    let Some(rest) = s.strip_prefix('/') else {
        return false;
    };
    s.len() <= PLACE_BOUND
        && rest.split('/').all(|part| {
            (1..=PLACE_BOUND).contains(&part.len())
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        })
}

/// **`one` is inside `other`**: `other`'s parts are the first parts of
/// `one`, judged part by part, so `/a/b` is inside `/a` and `/ab` is not.
fn inside(one: &str, other: &str) -> bool {
    let (one, other): (Vec<&str>, Vec<&str>) =
        (one.split('/').collect(), other.split('/').collect());
    one.len() > other.len() && one[..other.len()] == other[..]
}

/// **A card's address by number**: the domain, the bus, the device and the
/// function, compared in that order and not by their bytes, since the
/// domain is 4 to 8 hex digits. The pattern has already held it.
fn bus_order(address: &str) -> (u32, u8, u8, u8) {
    let number = |part: &str| u32::from_str_radix(part, 16).unwrap_or(u32::MAX);
    let (rest, function) = address.rsplit_once('.').unwrap_or((address, ""));
    let mut parts = rest.split(':').map(number);
    let mut next = || parts.next().unwrap_or(u32::MAX);
    let (domain, bus, device) = (next(), next(), next());
    (domain, bus as u8, device as u8, number(function) as u8)
}

/// **`network`** (the format's sections 1 and 1.1, refusal 12).
fn network(top: &Members<'_>) -> Result<Network, Refused> {
    let network = top.object("network")?;
    let management = plane(&network.object("management")?)?;
    let data = plane(&network.object("data")?)?;
    let planes = match network.raw_string("planes")? {
        "separate" => Planes::Separate,
        "shared" => Planes::Shared,
        other => {
            return refused(
                Cause::Network,
                format!("{} {}", network.at("planes"), quoted(other)),
            );
        }
    };
    if planes == Planes::Separate
        && (management.address == data.address
            || in_network(management.address, data)
            || in_network(data.address, management))
    {
        return refused(
            Cause::Network,
            format!(
                "separate planes at {}/{} and {}/{}: equal, or one in the other's network",
                management.address, management.prefix_length, data.address, data.prefix_length
            ),
        );
    }
    Ok(Network {
        management,
        data,
        planes,
    })
}

/// One plane: its address in the one spelling and of an allowed kind, and
/// its prefix length within its family's bound.
fn plane(plane: &Members<'_>) -> Result<Plane, Refused> {
    let at = plane.at("address");
    let text = plane.raw_string("address")?;
    let Some(address) = spelled(text) else {
        return refused(
            Cause::Network,
            format!(
                "{at} {} is not an address in the one spelling",
                quoted(text)
            ),
        );
    };
    if let Some(kind) = refused_kind(address) {
        return refused(Cause::Network, format!("{at} {} is {kind}", quoted(text)));
    }
    let prefix_length = plane.integer("prefix_length")?;
    let most = if address.is_ipv4() { 32 } else { 128 };
    if !(1..=most).contains(&prefix_length) {
        return refused(
            Cause::Network,
            format!(
                "{} is {prefix_length}, outside 1 to {most} for its address",
                plane.at("prefix_length")
            ),
        );
    }
    Ok(Plane {
        address,
        prefix_length: prefix_length as u8,
    })
}

/// **An address in the one spelling** (the format's section 1.1): parsed,
/// and written again as IPv4's dotted decimal or RFC 5952's form, which the
/// standard library's `Display` writes (pinned by the reader's tests on RFC
/// 5952's own examples); an address whose bytes differ is none. A zone does
/// not parse.
fn spelled(text: &str) -> Option<IpAddr> {
    let address: IpAddr = text.parse().ok()?;
    (address.to_string() == text).then_some(address)
}

/// **The kinds section 1.1 refuses**, by name, or none. A loopback address
/// is allowed.
fn refused_kind(address: IpAddr) -> Option<&'static str> {
    match address {
        IpAddr::V4(a) if a.is_unspecified() => Some("unspecified"),
        IpAddr::V4(a) if a.is_link_local() => Some("link-local"),
        IpAddr::V4(a) if a.is_multicast() => Some("multicast"),
        IpAddr::V4(a) if a == Ipv4Addr::BROADCAST => Some("the limited broadcast"),
        IpAddr::V4(_) => None,
        IpAddr::V6(a) if a.is_unspecified() => Some("unspecified"),
        IpAddr::V6(a) if a.segments()[0] & 0xffc0 == 0xfe80 => Some("link-local"),
        IpAddr::V6(a) if a.is_multicast() => Some("multicast"),
        IpAddr::V6(a) if a.to_ipv4_mapped().is_some() => Some("an IPv4 address written as IPv6"),
        IpAddr::V6(_) => None,
    }
}

/// **`address` lies in `plane`'s network**: the same family, and equal to
/// it with every bit past the plane's prefix length cleared. An IPv4
/// address never lies in an IPv6 network, nor the reverse.
fn in_network(address: IpAddr, plane: Plane) -> bool {
    match (address, plane.address) {
        (IpAddr::V4(a), IpAddr::V4(n)) => {
            let mask = u32::MAX
                .checked_shl(32 - u32::from(plane.prefix_length))
                .unwrap_or(0);
            u32::from(a) & mask == u32::from(n) & mask
        }
        (IpAddr::V6(a), IpAddr::V6(n)) => {
            let mask = u128::MAX
                .checked_shl(128 - u32::from(plane.prefix_length))
                .unwrap_or(0);
            u128::from(a) & mask == u128::from(n) & mask
        }
        _ => false,
    }
}
