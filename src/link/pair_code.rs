//! conforms: web-the-pairing-code-is-the-first-50-bits-of-a-32-byte-export
//!
//! **The pairing's one-time code** (Spec 8.2, "The one-time secret"). Both
//! ends of one pairing connection derive it from that connection's TLS
//! exporter (RFC 8446 section 7.5), and nothing of it crosses the link. One
//! module serves the console's listener and the box's half, so the two ends
//! cannot encode it two ways. It builds without the default features, since
//! the connectors' commands carry it.
//!
//! The code is a confirmation bound to one TLS session, not a bearer: a
//! party that terminates TLS between the box and the console holds two
//! sessions whose codes differ. **The console holds no code**: at an entry
//! it computes each waiting connection's code and compares (`Code::of`), and
//! a code's `Debug` shows nothing of it.

use std::fmt;

/// The exporter's label. **Private to this link**, so it takes the prefix
/// RFC 5705 section 4 reserves for private use without registration.
pub const LABEL: &[u8] = b"EXPERIMENTAL-weaver-pair-code";

/// The export's length, fixed: `key_length` enters the HKDF label (RFC 8446
/// sections 7.1 and 7.5), so an export of another length begins with other
/// bits and every entry would miss.
pub const EXPORT_BYTES: usize = 32;

/// The code's length: ten characters of five bits, the first fifty bits of
/// the export.
pub const CHARACTERS: usize = 10;

/// Crockford's base32.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A pairing code, ten characters of Crockford's base32, shown in two groups
/// of five. **Compared only by `matches`**, which takes the same time
/// whatever the characters.
#[derive(Clone)]
pub struct Code([u8; CHARACTERS]);

/// An entry that is not ten characters of the code's alphabet once read in
/// Crockford's form. The page answers it as malformed, writes no record and
/// counts no miss (Spec 8.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

impl Code {
    /// The code an export's bytes give: the first 50 bits, most significant
    /// bit first, cut in order into ten groups of 5 bits, each one character.
    pub fn from_export(export: &[u8; EXPORT_BYTES]) -> Self {
        let mut head = [0u8; 8];
        head.copy_from_slice(&export[..8]);
        let bits = u64::from_be_bytes(head);
        let mut code = [0u8; CHARACTERS];
        for (i, c) in code.iter_mut().enumerate() {
            let group = (bits >> (64 - 5 * (i + 1))) & 0x1f;
            *c = ALPHABET[group as usize];
        }
        Code(code)
    }

    /// The code of one TLS connection, from its exporter under `LABEL`, 32
    /// bytes and an empty context. Either end calls it on its own side of
    /// the same connection and gets the same code.
    pub fn of<D>(connection: &rustls::ConnectionCommon<D>) -> Result<Self, rustls::Error> {
        let export = connection.export_keying_material([0u8; EXPORT_BYTES], LABEL, Some(&[]))?;
        Ok(Self::from_export(&export))
    }

    /// **An entry read in Crockford's form**: uppercased, spaces and hyphens
    /// dropped, I and L read as 1 and O as 0, and then exactly ten
    /// characters of the alphabet, or `Malformed`.
    pub fn read(entry: &str) -> Result<Self, Malformed> {
        let mut code = [0u8; CHARACTERS];
        let mut n = 0;
        for c in entry.chars() {
            let c = match c.to_ascii_uppercase() {
                ' ' | '-' => continue,
                'I' | 'L' => '1',
                'O' => '0',
                c => c,
            };
            if !c.is_ascii() || !ALPHABET.contains(&(c as u8)) || n == CHARACTERS {
                return Err(Malformed);
            }
            code[n] = c as u8;
            n += 1;
        }
        if n != CHARACTERS {
            return Err(Malformed);
        }
        Ok(Code(code))
    }

    /// Whether two codes are the same, in time that does not depend on where
    /// they differ.
    pub fn matches(&self, other: &Code) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

/// Two groups of five, as the box shows it.
impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = std::str::from_utf8(&self.0).map_err(|_| fmt::Error)?;
        write!(f, "{} {}", &text[..5], &text[5..])
    }
}

/// Nothing of the code, so no log line or panic message carries it.
impl fmt::Debug for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Code(..)")
    }
}

/// A fingerprint as the pairing shows it at both ends: sixty-four hex
/// digits in sixteen groups of four, separated by spaces.
pub fn grouped(fingerprint: &str) -> String {
    fingerprint
        .as_bytes()
        .chunks(4)
        .map(|group| String::from_utf8_lossy(group).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec 8.2's illustration: the first seven of 32 made-up bytes.
    fn example() -> [u8; EXPORT_BYTES] {
        let mut export = [0xffu8; EXPORT_BYTES];
        export[..7].copy_from_slice(&[0x8f, 0x3a, 0x5c, 0x01, 0xe7, 0x22, 0xb4]);
        export
    }

    /// **The encoding is Spec 8.2's example**: the first 50 bits of the
    /// export, most significant first, in ten groups of five, shown in two
    /// groups. The bytes after the seventh are 0xff here, so a code that
    /// read past the fiftieth bit would differ from the Spec's.
    ///
    /// Perturbation: take the groups least significant bit first, or start
    /// at the export's second byte, and the code is not `HWX5R 0F74A`.
    #[test]
    fn the_example_encodes_as_the_spec_shows() {
        assert_eq!(Code::from_export(&example()).to_string(), "HWX5R 0F74A");
    }

    /// The fifty-first bit and every one after it enter no code, and the
    /// fiftieth does.
    #[test]
    fn only_the_first_fifty_bits_enter_the_code() {
        let base = Code::from_export(&example());
        let mut past = example();
        past[6] ^= 0x20; // bit 51 of the export, counting from 1
        assert!(base.matches(&Code::from_export(&past)));
        let mut last = example();
        last[6] ^= 0x40; // bit 50
        assert!(!base.matches(&Code::from_export(&last)));
    }

    /// **An entry is read in Crockford's form** (Spec 8.2, section 9): the
    /// Spec's lowercase hyphenated and spaced forms read as its example, I
    /// and L as 1 and O as 0, and a character outside the alphabet, a short
    /// entry or a long one is malformed.
    ///
    /// Perturbation: drop the uppercasing, and `hwx5r-0f74a` is malformed.
    #[test]
    fn an_entry_is_read_in_crockfords_form() {
        let example = Code::from_export(&example());
        for entry in [
            "hwx5r-0f74a",
            "HWX5R 0F74A",
            "HWX5R0F74A",
            " h-w-x-5-r 0f74a ",
        ] {
            let read = Code::read(entry).unwrap_or_else(|_| panic!("{entry:?} is malformed"));
            assert!(read.matches(&example), "{entry:?} read as another code");
        }
        let ones = Code::read("ilIL0oO000").unwrap();
        assert!(ones.matches(&Code::read("1111000000").unwrap()));
        for entry in [
            "HWX5U0F74A",
            "HWX5R0F74",
            "HWX5R0F74AA",
            "",
            "HWX5R_0F74A",
            "HWX5R0F74\u{c4}",
        ] {
            assert_eq!(
                Code::read(entry).err(),
                Some(Malformed),
                "{entry:?} was read"
            );
        }
    }

    /// A code's `Debug` carries nothing of it, so a log line or a panic
    /// message never does.
    #[test]
    fn a_codes_debug_shows_nothing_of_it() {
        let shown = format!("{:?}", Code::from_export(&example()));
        assert_eq!(shown, "Code(..)");
    }

    /// The fingerprint's grouping: sixteen groups of four.
    #[test]
    fn a_fingerprint_shows_in_sixteen_groups_of_four() {
        let fp = "0123456789abcdef".repeat(4);
        let shown = grouped(&fp);
        assert_eq!(shown.split(' ').count(), 16);
        assert!(shown.split(' ').all(|g| g.len() == 4));
        assert_eq!(shown.replace(' ', ""), fp);
    }
}
