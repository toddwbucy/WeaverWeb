# The tunable set a serving load requires, as read at WeaverAgent `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`

Held by `src/declaration/tunable_set.rs` (WeaverWeb Spec 6.1, the test of membership). Each passage below is the named lines of the named file at that commit, byte for byte, inside its fence. A reread at a new pin replaces this file with one named for that commit, and the diff between the two is the reread.

## `crates/weaver-types/src/config.rs`, lines 343 to 352

The six sampling defaults, read by `DecoderInstruction::sampling_defaults` and judged at every serving load.

```rust
/// The six sampling defaults' names in `tunable_values`, per
/// `weaver-types-Spec` section 2: the harness loop's, never the SPU's.
pub const SAMPLING_DEFAULT_NAMES: [&str; 6] = [
    "temperature",
    "top-k",
    "top-p",
    "repetition-penalty",
    "repetition-window",
    "seed",
];
```

## `crates/weaver-spu/src/main.rs`, lines 186 to 189

The session parameters' dispositions. An `OperatorTunable` member is required in `tunable-values`; a missing one refuses the SPU's admit as `Unsupplied`.

```rust
const SESSION_PARAMETERS: SessionParameters = SessionParameters {
    context_capacity: Disposition::OperatorTunable,
    max_tokens_per_turn: Disposition::OperatorTunable,
};
```

## `crates/weaver-spu/src/sampling.rs`, lines 202 to 213

The names the session parameters take in `tunable-values`.

```rust
    /// The names of the parameters this binary left tunable, derived from the
    /// dispositions rather than written beside them, so the two cannot drift.
    pub fn tunable_names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if !self.context_capacity.is_frozen() {
            names.push("context-capacity");
        }
        if !self.max_tokens_per_turn.is_frozen() {
            names.push("max-tokens-per-turn");
        }
        names
    }
```

## `crates/weaver-harness/src/lifecycle.rs`, lines 3461 to 3497

The conditional rule: a declared field election refuses a `top-k` of zero or above its depth. The screen declares no field election, so this rule is held and not required.

```rust
/// The declaration's six sampling defaults, judged as every generation would
/// judge them, per `weaver-harness-Spec` section 6.1.
///
/// A missing name, a value that is not a count where a count belongs, or a
/// value outside its range in `weaver-spu-Spec` section 8 refuses as
/// `ConfigInvalid` on `tunable-values.<name>`. Where the field is elected, a
/// `top-k` above the field's depth, or zero, refuses as `ConfigInvalid` on the
/// field's depth, with one typed line on `diagnostic` naming both numbers
/// (`weaver-spu-Spec` section 7.5).
pub(crate) fn judge_sampling_defaults(
    decoder: &weaver_types::DecoderInstruction,
    diagnostic: &mut impl std::io::Write,
) -> Result<weaver_types::Sampling, LifecycleRefusal> {
    let sampling = decoder
        .sampling_defaults()
        .map_err(|field| LifecycleRefusal::ConfigInvalid { field: Some(field) })?;
    if let Some(election) = decoder.field_election.as_ref()
        && weaver_types::field_below_cutoff(election.depth, sampling.top_k)
    {
        let _ = writeln!(
            diagnostic,
            "{}",
            serde_json::json!({
                "organ": "harness",
                "refusal": "field_depth_below_cutoff",
                "depth": election.depth,
                "top_k": sampling.top_k,
            })
        );
        return Err(LifecycleRefusal::ConfigInvalid {
            field: Some(weaver_types::FieldName(
                "spu-instruction.decoder.field-election.depth".to_string(),
            )),
        });
    }
    Ok(sampling)
}
```
