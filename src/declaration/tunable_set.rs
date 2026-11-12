//! conforms: web-the-screens-required-tunable-set-is-the-agents-as-read
//!
//! **The tunable set a serving load requires, as read at the pin** (Spec
//! 6.1, the test of membership). The agent names it in three places:
//! `SAMPLING_DEFAULT_NAMES` in `crates/weaver-types/src/config.rs`, the
//! `OperatorTunable` members of `SESSION_PARAMETERS` in
//! `crates/weaver-spu/src/main.rs` under the names
//! `SessionParameters::tunable_names` gives them in
//! `crates/weaver-spu/src/sampling.rs`, and the conditional field-election
//! rule in `judge_sampling_defaults`, `crates/weaver-harness/src/lifecycle.rs`,
//! which the screen cannot reach. This crate links no crate of the agent
//! (Spec 1), so the passages are held beside this module as read
//! (`tunable-set-5ec7ecf8.md`, byte for byte inside its fences), and a test
//! fails where the screen's set, [`super::SAMPLER`] under the agent's names,
//! and the copy's differ. A reread at a new pin is a diff of the copy.

/// **The WeaverAgent commit the tunable set was read at**. A test holds it
/// to the build's pin, [`super::render::CORPUS_COMMIT`], so the pin does not
/// move until the set is reread at the new commit.
pub const TUNABLE_SET_READ_AT: &str = "5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291";

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::TUNABLE_SET_READ_AT;
    use crate::declaration::SAMPLER;
    use crate::declaration::render::{CORPUS_COMMIT, tunable};

    /// The passages as read at [`TUNABLE_SET_READ_AT`].
    const READ: &str = include_str!("tunable-set-5ec7ecf8.md");

    /// **The fenced text under the heading naming `path`**, the one passage
    /// of that file the copy holds.
    fn passage(path: &str) -> &'static str {
        let heading = format!("## `{path}`, lines ");
        let at = READ
            .find(&heading)
            .unwrap_or_else(|| panic!("the copy holds no passage of {path}"));
        let rest = &READ[at..];
        let open = rest.find("```rust\n").expect("a passage opens its fence") + "```rust\n".len();
        let close = rest[open..]
            .find("\n```")
            .expect("a passage closes its fence");
        &rest[open..open + close]
    }

    /// Every string literal in a passage, in order.
    fn quoted(text: &str) -> Vec<&str> {
        text.split('"').skip(1).step_by(2).collect()
    }

    /// **The required set the copy states**: the six sampling defaults, and
    /// each session parameter the binary leaves `OperatorTunable` under the
    /// name `tunable_names` pushes for it.
    fn required_as_read() -> BTreeSet<String> {
        let mut out: BTreeSet<String> = quoted(passage("crates/weaver-types/src/config.rs"))
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(out.len(), 6, "SAMPLING_DEFAULT_NAMES holds six names");
        let names = passage("crates/weaver-spu/src/sampling.rs");
        for line in passage("crates/weaver-spu/src/main.rs").lines() {
            let Some((field, disposition)) = line.trim().split_once(": Disposition::") else {
                continue;
            };
            if disposition != "OperatorTunable," {
                continue;
            }
            // `if !self.<field>.is_frozen() {` then `names.push("<name>");`.
            let guard = format!("if !self.{field}.is_frozen() {{");
            let at = names
                .find(&guard)
                .unwrap_or_else(|| panic!("tunable_names names no {field}"));
            let name = quoted(&names[at..])[0];
            out.insert(name.to_owned());
        }
        out
    }

    /// **The screen's required set is the agent's at the pin, as read**: the
    /// eight names of [`SAMPLER`], each under the agent's name, are exactly
    /// the six sampling defaults and the `OperatorTunable` session
    /// parameters the copy holds. The conditional field-election rule is
    /// held and not required, since the screen declares no field election.
    #[test]
    fn the_tunable_set_is_the_agents_as_read() {
        let screen: BTreeSet<String> = SAMPLER
            .iter()
            .map(|(member, _)| tunable(member).to_owned())
            .collect();
        assert_eq!(
            screen.len(),
            SAMPLER.len(),
            "each member has a name of its own"
        );
        assert_eq!(
            screen,
            required_as_read(),
            "the screen's tunable set and the agent's as read at {TUNABLE_SET_READ_AT} differ"
        );
        let rule = passage("crates/weaver-harness/src/lifecycle.rs");
        assert!(
            rule.contains("decoder.field_election.as_ref()")
                && rule.contains("field_below_cutoff(election.depth, sampling.top_k)"),
            "the copy holds the conditional field-election rule"
        );
        assert!(
            READ.lines()
                .next()
                .is_some_and(|title| title.contains(TUNABLE_SET_READ_AT)),
            "the copy names the commit it was read at"
        );
    }

    /// **The pin moves only with a reread**: the set was read at the commit
    /// the build pins.
    #[test]
    fn the_tunable_set_was_read_at_the_builds_pin() {
        assert_eq!(
            TUNABLE_SET_READ_AT, CORPUS_COMMIT,
            "the build's pin moved: reread the agent's three places at the new commit, \
             refresh tunable-set-<commit>.md, then move TUNABLE_SET_READ_AT"
        );
    }
}
