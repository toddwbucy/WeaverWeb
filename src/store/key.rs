//! conforms: web-position-is-addressed-by-run-turn-position
//!
//! The address of a position, per `weaver-web-Spec` section 2.1: the run,
//! the turn, and the position, and the composite of the three is the key.
//!
//! **The pin is that the reads take this type and nothing looser.** A read
//! addressed by three bare values could be handed a turn and a position
//! without a run, which answers one line per run rather than one, because
//! turn keys repeat across a serving record's runs. A read that takes
//! `&PositionKey` cannot be handed less than the whole address, and the
//! compiler holds that rather than a test.
//!
//! **`position` is the resident length at the draw and not an ordinal within
//! the turn**, per section 2.1: the first generated token of a sixty-token
//! prompt is position sixty. The type carries the number as the record
//! spells it and converts nothing, the conversion between ordinal and
//! position being the surface's to make and to make once, per section 6.

use serde::{Deserialize, Serialize};

/// A plan's identity, spelled `pl-` and sixteen hex per section 2.
///
/// An arm's identity does not resolve to a plan's, and the compiler says so
/// rather than the database:
///
/// ```compile_fail
/// use weaver_web::store::{ArmId, Store};
/// # async fn f(store: Store, arm: ArmId) {
/// store.plan(&arm).await.unwrap();
/// # }
/// ```
///
/// The same call with a `PlanId` compiles, which is the pin's other half:
///
/// ```no_run
/// use weaver_web::store::{PlanId, Store};
/// # async fn f(store: Store, plan: PlanId) {
/// store.plan(&plan).await.unwrap();
/// # }
/// ```
///
/// **The kind is in the type and not only in the bytes.** The identities of
/// section 2's authored rows are all text, so a bare `String` lets an arm's
/// identity be handed where a plan's is owed: the schema refuses it, but not
/// until a round trip, and a read so addressed answers `None`, which every
/// caller reads as "no such plan" rather than as "wrong kind of key". The
/// newtype is what makes section 2's claim - that a key of the wrong kind is
/// refused at the boundary - true of the boundary a caller actually meets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct PlanId(String);

/// **The shape is checked where a string becomes an identity**, which is the
/// only place it can be checked once. A tuple struct with a public member is
/// a newtype the compiler holds and nothing else does: `PlanId(s)` takes any
/// string, and a derived `Deserialize` takes any string off a query or a
/// body, so section 2's claim that a key of the wrong kind is refused at the
/// boundary would hold for the type and not for the value. No surface
/// constructs one yet - section 4's sixth read takes it from a database
/// column a domain already governs - and the act that gives the matrix a
/// link is the act that would have found this the expensive way.
fn shaped(prefix: &str, s: &str) -> bool {
    let Some(hex) = s.strip_prefix(prefix) else {
        return false;
    };
    hex.len() == 16
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

macro_rules! identity {
    ($t:ty, $prefix:literal, $what:literal) => {
        impl $t {
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::str::FromStr for $t {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if shaped($prefix, s) {
                    Ok(Self(s.to_owned()))
                } else {
                    Err(format!(concat!("not ", $what, ": {}"), s))
                }
            }
        }

        impl TryFrom<&str> for $t {
            type Error = String;
            fn try_from(s: &str) -> Result<Self, Self::Error> {
                s.parse()
            }
        }

        impl<'de> Deserialize<'de> for $t {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

identity!(PlanId, "pl-", "a plan's identity");
identity!(ArmId, "ar-", "an arm's identity");

impl std::fmt::Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An arm's identity, spelled `ar-` and sixteen hex per section 2.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ArmId(String);

impl std::fmt::Display for ArmId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A run's identity as the record spells it.
///
/// **It carries no prefix and that is the convention rather than an
/// exception to it**, per section 2: a run is a row this crate received, so
/// its identity is the record's spelling and never one this store invented.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub String);

/// A turn's key within its run, repeated across a serving record's runs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TurnId(pub String);

/// The composite address of one position.
///
/// ```
/// use weaver_web::store::{PositionKey, RunId, TurnId};
///
/// let key = PositionKey {
///     run: RunId("2026-09-08T05:06:48.865Z-karl-646bf4eb0582c253".into()),
///     turn: TurnId("t-1".into()),
///     position: 154,
/// };
/// assert_eq!(key.position, 154);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PositionKey {
    pub run: RunId,
    pub turn: TurnId,
    pub position: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A string becomes an identity only if it is spelled for its kind**,
    /// per `weaver-web-Spec` section 2, and that is what makes the section's
    /// claim true of the value and not only of the type.
    ///
    /// Perturbation: return `Ok(Self(s.to_owned()))` unconditionally and an
    /// arm's identity parses as a plan's, a query string's nonsense parses
    /// as either, and section 4's sixth read answers `None` for all of them
    /// - which a caller reads as no such plan.
    ///
    /// conforms: web-an-authored-identity-says-what-it-addresses
    #[test]
    fn a_string_becomes_an_identity_only_when_it_is_spelled_for_its_kind() {
        let plan: PlanId = "pl-0123456789abcdef".parse().unwrap();
        assert_eq!(plan.as_str(), "pl-0123456789abcdef");
        assert!("ar-0123456789abcdef".parse::<ArmId>().is_ok());

        for wrong in [
            "ar-0123456789abcdef",  // another kind's, which is the whole point
            "pl-0123456789ABCDEF",  // upper hex, which the domain does not admit
            "pl-0123456789abcde",   // fifteen
            "pl-0123456789abcdef0", // seventeen
            "pl-0123456789abcdeg",  // not hex
            "pl-",
            "",
            "0123456789abcdef",
        ] {
            assert!(
                wrong.parse::<PlanId>().is_err(),
                "{wrong} must not be a plan's identity"
            );
        }

        // **Deserialization is the same boundary.** A derived impl would take
        // any string off a query or a body, which is the path act 5's matrix
        // opens when it links to a plan.
        assert!(serde_json::from_str::<PlanId>("\"pl-0123456789abcdef\"").is_ok());
        assert!(serde_json::from_str::<PlanId>("\"ar-0123456789abcdef\"").is_err());

        // **A run's identity is not shaped and must not be**, per section
        // 2.2: it is the record's spelling, so there is nothing to validate
        // against and a check here would refuse real runs.
        let spelled = "2026-09-08T05:06:48.865Z-karl-646bf4eb0582c253";
        assert_eq!(
            RunId(spelled.into()).0,
            spelled,
            "taken as the record spells it"
        );
        assert!(
            spelled.parse::<PlanId>().is_err(),
            "and no authored shape would have admitted it"
        );
    }
}
