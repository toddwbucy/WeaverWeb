//! conforms: web-the-gguf-reader-refuses-what-its-buffer-does-not-hold
//!
//! **A reader of GGUF metadata of the console's own** (Spec 6.2), linking
//! no crate of the agent (Spec 1). It reads the header the models page
//! shows before any download: the precision, the maximum context, the
//! block count, the KV heads, the split's count and number, and
//! `general.type`. It reads versions 2 and 3, little-endian, and every
//! value type of llama.cpp's `ggml/include/gguf.h`, arrays included, an
//! array of arrays one level deep and no deeper.
//!
//! **Every length and count is checked against the bytes held before
//! anything is read or allocated for it**, so a string's length or an
//! array's count past the buffer, and a header cut short, answer
//! `Refused::Short` naming the bytes the read would need, never a panic or
//! an allocation of a claimed size. The caller holds the bound: a header
//! whose need runs past it is "not read".

/// **The bound on a header's read**, 16 MiB (Spec 6.2): the read stops at
/// the end of the metadata or here, whichever comes first.
pub const BOUND: usize = 16 << 20;

/// The facts the models page shows, each `None` where the header does not
/// carry it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Header {
    pub version: u32,
    pub architecture: Option<String>,
    /// `general.file_type`, llama.cpp's file type.
    pub file_type: Option<u64>,
    /// `<general.architecture>.context_length`.
    pub context_length: Option<u64>,
    /// `<general.architecture>.block_count`.
    pub block_count: Option<u64>,
    /// `<general.architecture>.attention.head_count_kv`.
    pub kv_heads: Option<KvHeads>,
    /// `split.count`.
    pub split_count: Option<u64>,
    /// `split.no`, numbered from 0.
    pub split_no: Option<u64>,
    /// `general.type`: `model` or `adapter`.
    pub general_type: Option<String>,
    /// The bytes the metadata took, from the file's start.
    pub metadata_bytes: u64,
}

/// The KV heads: one count, or one per block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum KvHeads {
    One(u64),
    PerBlock(Vec<u64>),
}

/// Why the bytes held answered no header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The bytes end before the metadata does: a read would need `needed`
    /// bytes from the file's start, which saturates where a claimed length
    /// overflows.
    Short { needed: u64 },
    /// The bytes are not a GGUF header this reader reads.
    Malformed(String),
}

/// **The word llama.cpp names a file type by**, from `enum llama_ftype` in
/// its `include/llama.h` read at `1593d5684d077c07fc788e9527ec1bd52287de7f`
/// (2026-06-11), its `LLAMA_FTYPE_` and `MOSTLY_` or `ALL_` dropped, or
/// "file type N" for a number it does not name, never a word guessed. The
/// numbers the header comments out as removed (4, 5, 6, 33 to 35) are not
/// named.
pub fn precision(file_type: u64) -> String {
    let word = match file_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_XXS",
        24 => "IQ1_S",
        25 => "IQ4_NL",
        26 => "IQ3_S",
        27 => "IQ3_M",
        28 => "IQ2_S",
        29 => "IQ2_M",
        30 => "IQ4_XS",
        31 => "IQ1_M",
        32 => "BF16",
        36 => "TQ1_0",
        37 => "TQ2_0",
        38 => "MXFP4_MOE",
        39 => "NVFP4",
        40 => "Q1_0",
        n => return format!("file type {n}"),
    };
    word.to_owned()
}

/// The value types of `gguf.h`.
const UINT8: u32 = 0;
const INT8: u32 = 1;
const UINT16: u32 = 2;
const INT16: u32 = 3;
const UINT32: u32 = 4;
const INT32: u32 = 5;
const FLOAT32: u32 = 6;
const BOOL: u32 = 7;
const STRING: u32 = 8;
const ARRAY: u32 = 9;
const UINT64: u32 = 10;
const INT64: u32 = 11;
const FLOAT64: u32 = 12;

/// The bytes a value of a fixed width takes, `None` for a string or an
/// array.
fn width(kind: u32) -> Option<u64> {
    match kind {
        UINT8 | INT8 | BOOL => Some(1),
        UINT16 | INT16 => Some(2),
        UINT32 | INT32 | FLOAT32 => Some(4),
        UINT64 | INT64 | FLOAT64 => Some(8),
        _ => None,
    }
}

/// The fewest bytes an element of `kind` takes: a string its length, an
/// array its type and its count.
fn least(kind: u32) -> u64 {
    match kind {
        STRING => 8,
        ARRAY => 12,
        kind => width(kind).unwrap_or(1),
    }
}

/// **The bytes held and the place read to.** Every take checks its end
/// against the bytes held first, so nothing is read or allocated past them.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    /// The next `n` bytes, or the need they name where the bytes end
    /// first.
    fn take(&mut self, n: u64) -> Result<&'a [u8], Refused> {
        let needed = (self.at as u64).saturating_add(n);
        if needed > self.bytes.len() as u64 {
            return Err(Refused::Short { needed });
        }
        let taken = &self.bytes[self.at..needed as usize];
        self.at = needed as usize;
        Ok(taken)
    }

    /// Where `count` elements of at least `each` bytes would end, checked
    /// against the bytes held before any is read.
    fn room(&self, count: u64, each: u64) -> Result<(), Refused> {
        let needed = (self.at as u64).saturating_add(count.saturating_mul(each));
        if needed > self.bytes.len() as u64 {
            return Err(Refused::Short { needed });
        }
        Ok(())
    }

    fn u32(&mut self) -> Result<u32, Refused> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn u64(&mut self) -> Result<u64, Refused> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// A string's bytes, its length checked against the bytes held.
    fn string(&mut self) -> Result<&'a [u8], Refused> {
        let len = self.u64()?;
        self.take(len)
    }
}

/// A value the reader keeps: an integer, a string, or a list of integers.
enum Kept {
    Int(u64),
    Text(String),
    Ints(Vec<u64>),
    /// A value of another kind, read past.
    Other,
}

/// An integer of `kind` from its bytes, `None` for a negative one or
/// another kind.
fn integer(kind: u32, raw: &[u8]) -> Option<u64> {
    match kind {
        UINT8 => Some(raw[0] as u64),
        UINT16 => Some(u16::from_le_bytes(raw.try_into().ok()?) as u64),
        UINT32 => Some(u32::from_le_bytes(raw.try_into().ok()?) as u64),
        UINT64 => Some(u64::from_le_bytes(raw.try_into().ok()?)),
        INT8 => u64::try_from(raw[0] as i8).ok(),
        INT16 => u64::try_from(i16::from_le_bytes(raw.try_into().ok()?)).ok(),
        INT32 => u64::try_from(i32::from_le_bytes(raw.try_into().ok()?)).ok(),
        INT64 => u64::try_from(i64::from_le_bytes(raw.try_into().ok()?)).ok(),
        _ => None,
    }
}

/// **One value of `kind`**, kept where `keep` asks and read past where it
/// does not. An array's elements are checked for room before the first is
/// read, and an array within an array may hold no array.
fn value(cursor: &mut Cursor<'_>, kind: u32, keep: bool, depth: u8) -> Result<Kept, Refused> {
    if let Some(width) = width(kind) {
        let raw = cursor.take(width)?;
        return Ok(match integer(kind, raw) {
            Some(n) if keep => Kept::Int(n),
            _ => Kept::Other,
        });
    }
    match kind {
        STRING => {
            let raw = cursor.string()?;
            if !keep {
                return Ok(Kept::Other);
            }
            String::from_utf8(raw.to_vec())
                .map(Kept::Text)
                .map_err(|_| Refused::Malformed("a string the reader keeps is not UTF-8".into()))
        }
        ARRAY => {
            let element = cursor.u32()?;
            let count = cursor.u64()?;
            if element == ARRAY && depth > 0 {
                return Err(Refused::Malformed(
                    "an array of arrays nests one level deep and no deeper".into(),
                ));
            }
            if width(element).is_none() && !matches!(element, STRING | ARRAY) {
                return Err(Refused::Malformed(format!(
                    "a value of type {element}, which GGUF does not define"
                )));
            }
            cursor.room(count, least(element))?;
            if let Some(width) = width(element) {
                let raw = cursor.take(count * width)?;
                if keep {
                    let ints: Option<Vec<u64>> = raw
                        .chunks(width as usize)
                        .map(|c| integer(element, c))
                        .collect();
                    if let Some(ints) = ints {
                        return Ok(Kept::Ints(ints));
                    }
                }
                return Ok(Kept::Other);
            }
            for _ in 0..count {
                value(cursor, element, false, depth + 1)?;
            }
            Ok(Kept::Other)
        }
        kind => Err(Refused::Malformed(format!(
            "a value of type {kind}, which GGUF does not define"
        ))),
    }
}

/// The keys whose values the reader keeps, by what they are.
fn kept(key: &str) -> bool {
    matches!(
        key,
        "general.architecture" | "general.type" | "general.file_type" | "split.count" | "split.no"
    ) || key.ends_with(".context_length")
        || key.ends_with(".block_count")
        || key.ends_with(".attention.head_count_kv")
}

/// An integer fact, or the refusal naming it where it is of another type.
fn int_fact(key: &str, value: Option<&Kept>) -> Result<Option<u64>, Refused> {
    match value {
        None => Ok(None),
        Some(Kept::Int(n)) => Ok(Some(*n)),
        Some(_) => Err(Refused::Malformed(format!(
            "{key} is not an unsigned integer"
        ))),
    }
}

/// A text fact, or the refusal naming it where it is of another type.
fn text_fact(key: &str, value: Option<&Kept>) -> Result<Option<String>, Refused> {
    match value {
        None => Ok(None),
        Some(Kept::Text(t)) => Ok(Some(t.clone())),
        Some(_) => Err(Refused::Malformed(format!("{key} is not a string"))),
    }
}

/// **The header of the GGUF file `bytes` begins**, read to the end of its
/// metadata: the magic, the version, the tensors' count, the pairs' count,
/// then each key and its value. The facts are taken once every pair is
/// read, so their order in the file does not matter.
pub fn read(bytes: &[u8]) -> Result<Header, Refused> {
    let mut cursor = Cursor { bytes, at: 0 };
    if cursor.take(4)? != b"GGUF" {
        return Err(Refused::Malformed(
            "not a GGUF file: its magic differs".into(),
        ));
    }
    let version = cursor.u32()?;
    if !matches!(version, 2 | 3) {
        return Err(Refused::Malformed(format!(
            "GGUF version {version}, where the reader reads 2 and 3"
        )));
    }
    let _tensors = cursor.u64()?;
    let pairs = cursor.u64()?;
    let mut facts: Vec<(String, Kept)> = Vec::new();
    for _ in 0..pairs {
        let key = cursor.string()?;
        let kind = cursor.u32()?;
        let keep = std::str::from_utf8(key).is_ok_and(kept);
        let value = value(&mut cursor, kind, keep, 0)?;
        if keep && !matches!(value, Kept::Other) {
            facts.push((String::from_utf8_lossy(key).into_owned(), value));
        }
    }
    let fact = |key: &str| facts.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v);
    let architecture = text_fact("general.architecture", fact("general.architecture"))?;
    let under = |suffix: &str| architecture.as_ref().map(|arch| format!("{arch}.{suffix}"));
    let arch_int = |suffix: &str| match under(suffix) {
        Some(key) => int_fact(&key, fact(&key)),
        None => Ok(None),
    };
    let kv_heads = match under("attention.head_count_kv").and_then(|key| fact(&key)) {
        None => None,
        Some(Kept::Int(n)) => Some(KvHeads::One(*n)),
        Some(Kept::Ints(ns)) => Some(KvHeads::PerBlock(ns.clone())),
        Some(_) => {
            return Err(Refused::Malformed(
                "the KV heads are not an unsigned integer or a list of them".into(),
            ));
        }
    };
    Ok(Header {
        version,
        file_type: int_fact("general.file_type", fact("general.file_type"))?,
        context_length: arch_int("context_length")?,
        block_count: arch_int("block_count")?,
        kv_heads,
        split_count: int_fact("split.count", fact("split.count"))?,
        split_no: int_fact("split.no", fact("split.no"))?,
        general_type: text_fact("general.type", fact("general.type"))?,
        architecture,
        metadata_bytes: cursor.at as u64,
    })
}

#[cfg(test)]
pub(crate) mod build {
    //! A GGUF header's bytes, written for a test. No header here is a
    //! real model's.

    /// The value types of `gguf.h`.
    pub const U8: u32 = 0;
    pub const U16: u32 = 2;
    pub const U32: u32 = 4;
    pub const STRING: u32 = 8;
    pub const ARRAY: u32 = 9;

    #[derive(Default)]
    pub struct Gguf {
        pub version: u32,
        kvs: Vec<u8>,
        count: u64,
    }

    impl Gguf {
        pub fn new() -> Gguf {
            Gguf {
                version: 3,
                ..Default::default()
            }
        }

        fn key(&mut self, key: &str, kind: u32) -> &mut Vec<u8> {
            self.count += 1;
            self.kvs
                .extend_from_slice(&(key.len() as u64).to_le_bytes());
            self.kvs.extend_from_slice(key.as_bytes());
            self.kvs.extend_from_slice(&kind.to_le_bytes());
            &mut self.kvs
        }

        pub fn string(mut self, key: &str, value: &str) -> Gguf {
            let kvs = self.key(key, STRING);
            kvs.extend_from_slice(&(value.len() as u64).to_le_bytes());
            kvs.extend_from_slice(value.as_bytes());
            self
        }

        pub fn u16(mut self, key: &str, value: u16) -> Gguf {
            self.key(key, U16).extend_from_slice(&value.to_le_bytes());
            self
        }

        pub fn u32(mut self, key: &str, value: u32) -> Gguf {
            self.key(key, U32).extend_from_slice(&value.to_le_bytes());
            self
        }

        pub fn u32s(mut self, key: &str, values: &[u32]) -> Gguf {
            let kvs = self.key(key, ARRAY);
            kvs.extend_from_slice(&U32.to_le_bytes());
            kvs.extend_from_slice(&(values.len() as u64).to_le_bytes());
            for v in values {
                kvs.extend_from_slice(&v.to_le_bytes());
            }
            self
        }

        pub fn strings(mut self, key: &str, values: &[&str]) -> Gguf {
            let kvs = self.key(key, ARRAY);
            kvs.extend_from_slice(&STRING.to_le_bytes());
            kvs.extend_from_slice(&(values.len() as u64).to_le_bytes());
            for v in values {
                kvs.extend_from_slice(&(v.len() as u64).to_le_bytes());
                kvs.extend_from_slice(v.as_bytes());
            }
            self
        }

        /// A key and its type, then `raw` as the value's bytes, whatever
        /// they claim.
        pub fn raw(mut self, key: &str, kind: u32, raw: &[u8]) -> Gguf {
            self.key(key, kind).extend_from_slice(raw);
            self
        }

        /// The header of a model of `architecture`, the facts the page
        /// shows.
        pub fn model(architecture: &str) -> Gguf {
            Gguf::new()
                .string("general.architecture", architecture)
                .string("general.type", "model")
                .u32("general.file_type", 15)
                .u32(&format!("{architecture}.context_length"), 131_072)
                .u32(&format!("{architecture}.block_count"), 40)
                .u32(&format!("{architecture}.attention.head_count_kv"), 8)
        }

        pub fn bytes(&self) -> Vec<u8> {
            let mut out = b"GGUF".to_vec();
            out.extend_from_slice(&self.version.to_le_bytes());
            out.extend_from_slice(&0u64.to_le_bytes());
            out.extend_from_slice(&self.count.to_le_bytes());
            out.extend_from_slice(&self.kvs);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build::{self, Gguf};
    use super::*;

    /// **The facts read from a model's header**, the context length read
    /// under the architecture's own prefix whichever order the keys come
    /// in, and the bytes the metadata took.
    #[test]
    fn a_models_header_answers_its_facts() {
        let gguf = Gguf::new()
            .u32("granite.context_length", 131_072)
            .string("general.architecture", "granite")
            .string("general.type", "model")
            .u32("general.file_type", 15)
            .u32("granite.block_count", 40)
            .u32("granite.attention.head_count_kv", 8)
            .u32("llama.context_length", 4096)
            .u16("split.count", 2)
            .u16("split.no", 0)
            .strings("tokenizer.ggml.tokens", &["a", "b", "c"]);
        let bytes = gguf.bytes();
        let header = read(&bytes).unwrap();
        assert_eq!(header.version, 3);
        assert_eq!(header.architecture.as_deref(), Some("granite"));
        assert_eq!(header.file_type, Some(15));
        assert_eq!(precision(header.file_type.unwrap()), "Q4_K_M");
        assert_eq!(header.context_length, Some(131_072));
        assert_eq!(header.block_count, Some(40));
        assert_eq!(header.kv_heads, Some(KvHeads::One(8)));
        assert_eq!(header.split_count, Some(2));
        assert_eq!(header.split_no, Some(0));
        assert_eq!(header.general_type.as_deref(), Some("model"));
        assert_eq!(header.metadata_bytes, bytes.len() as u64);
        // Bytes past the metadata, the tensors' infos, are not read.
        let mut longer = bytes.clone();
        longer.extend_from_slice(&[0xff; 64]);
        assert_eq!(read(&longer).unwrap(), header);
    }

    /// **A fact the header does not carry is none**, version 2 is read,
    /// and the KV heads per block are kept as a list.
    #[test]
    fn a_fact_not_carried_is_none_and_per_block_heads_are_a_list() {
        let mut gguf = Gguf::new()
            .string("general.architecture", "granitehybrid")
            .u32s("granitehybrid.attention.head_count_kv", &[0, 8, 0, 8]);
        gguf.version = 2;
        let header = read(&gguf.bytes()).unwrap();
        assert_eq!(header.version, 2);
        assert_eq!(header.kv_heads, Some(KvHeads::PerBlock(vec![0, 8, 0, 8])));
        assert_eq!(header.file_type, None);
        assert_eq!(header.context_length, None);
        assert_eq!(header.split_count, None);
        assert_eq!(header.general_type, None);
    }

    /// **Every value type is read past**, an array of arrays one level
    /// deep among them, and an array of arrays of arrays is refused.
    #[test]
    fn every_value_type_is_read_past_and_arrays_nest_one_level() {
        let mut nested = Vec::new();
        nested.extend_from_slice(&build::ARRAY.to_le_bytes());
        nested.extend_from_slice(&2u64.to_le_bytes());
        for _ in 0..2 {
            nested.extend_from_slice(&build::U8.to_le_bytes());
            nested.extend_from_slice(&3u64.to_le_bytes());
            nested.extend_from_slice(&[1, 2, 3]);
        }
        let mut gguf = Gguf::new().string("general.architecture", "t");
        for (kind, width) in [(0u32, 1), (1, 1), (2, 2), (3, 2), (4, 4), (5, 4), (6, 4)] {
            gguf = gguf.raw(&format!("k{kind}"), kind, &vec![0; width]);
        }
        gguf = gguf
            .raw("k7", 7, &[1])
            .raw("k10", 10, &[0; 8])
            .raw("k11", 11, &[0; 8])
            .raw("k12", 12, &[0; 8])
            .raw("nested", build::ARRAY, &nested)
            .u32("t.block_count", 3);
        assert_eq!(read(&gguf.bytes()).unwrap().block_count, Some(3));

        let mut deeper = Vec::new();
        deeper.extend_from_slice(&build::ARRAY.to_le_bytes());
        deeper.extend_from_slice(&1u64.to_le_bytes());
        deeper.extend_from_slice(&build::ARRAY.to_le_bytes());
        deeper.extend_from_slice(&1u64.to_le_bytes());
        deeper.extend_from_slice(&build::U8.to_le_bytes());
        deeper.extend_from_slice(&1u64.to_le_bytes());
        deeper.push(0);
        let refused = read(&Gguf::new().raw("deep", build::ARRAY, &deeper).bytes());
        assert!(
            matches!(&refused, Err(Refused::Malformed(why)) if why.contains("one level")),
            "{refused:?}"
        );
    }

    /// **A hostile header is refused without a panic or an allocation of
    /// its claim**: a string's length and an array's count past the buffer,
    /// each claiming more than memory holds, answer `Short` naming a need
    /// past the bound, and a header cut short answers `Short` within it.
    #[test]
    fn a_hostile_header_is_refused_without_a_panic_or_its_claimed_allocation() {
        let huge = u64::MAX / 2;
        let mut claim = huge.to_le_bytes().to_vec();
        claim.extend_from_slice(b"abc");
        let string = Gguf::new().raw("general.architecture", build::STRING, &claim);
        let mut array = build::STRING.to_le_bytes().to_vec();
        array.extend_from_slice(&huge.to_le_bytes());
        let strings = Gguf::new().raw("tokenizer.ggml.tokens", build::ARRAY, &array);
        let mut array = build::U32.to_le_bytes().to_vec();
        array.extend_from_slice(&huge.to_le_bytes());
        let ints = Gguf::new().raw("granite.attention.head_count_kv", build::ARRAY, &array);
        let mut key = b"GGUF".to_vec();
        key.extend_from_slice(&3u32.to_le_bytes());
        key.extend_from_slice(&0u64.to_le_bytes());
        key.extend_from_slice(&1u64.to_le_bytes());
        key.extend_from_slice(&huge.to_le_bytes());
        for (case, bytes) in [
            ("a string's length", string.bytes()),
            ("an array of strings' count", strings.bytes()),
            ("an array of integers' count", ints.bytes()),
            ("a key's length", key),
        ] {
            match read(&bytes) {
                Err(Refused::Short { needed }) => {
                    assert!(needed > BOUND as u64, "{case}: {needed}")
                }
                other => panic!("{case}: {other:?}"),
            }
        }
        let whole = Gguf::model("granite").bytes();
        for cut in [3, 4, 10, 24, whole.len() - 1] {
            match read(&whole[..cut]) {
                Err(Refused::Short { needed }) => {
                    assert!(needed > cut as u64 && needed <= whole.len() as u64, "{cut}")
                }
                other => panic!("cut at {cut}: {other:?}"),
            }
        }
    }

    /// **What is not a GGUF header this reader reads is refused**: another
    /// magic, a version outside 2 and 3, a value type GGUF does not define,
    /// and a fact of another type than its own.
    #[test]
    fn what_is_not_a_gguf_header_it_reads_is_refused() {
        let mut v1 = Gguf::new();
        v1.version = 1;
        let mut v4 = Gguf::new();
        v4.version = 4;
        for (case, bytes, says) in [
            ("magic", b"GGML\x03\0\0\0".to_vec(), "not a GGUF file"),
            ("version 1", v1.bytes(), "version 1"),
            ("version 4", v4.bytes(), "version 4"),
            (
                "type 13",
                Gguf::new().raw("k", 13, &[0; 8]).bytes(),
                "type 13",
            ),
            (
                "a string file type",
                Gguf::new().string("general.file_type", "15").bytes(),
                "general.file_type",
            ),
        ] {
            match read(&bytes) {
                Err(Refused::Malformed(why)) => assert!(why.contains(says), "{case}: {why}"),
                other => panic!("{case}: {other:?}"),
            }
        }
    }

    /// **The precision is llama.cpp's word or the number**, never a word
    /// guessed: a removed type is not named.
    #[test]
    fn the_precision_is_llama_cpps_word_or_the_number() {
        assert_eq!(precision(0), "F32");
        assert_eq!(precision(7), "Q8_0");
        assert_eq!(precision(32), "BF16");
        assert_eq!(precision(4), "file type 4");
        assert_eq!(precision(34), "file type 34");
        assert_eq!(precision(1024), "file type 1024");
    }
}
