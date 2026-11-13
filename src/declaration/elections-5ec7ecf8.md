# The three elections, as read at WeaverAgent `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`

Held by `src/declaration/elections.rs` (WeaverWeb Spec 6.1, the test of membership). Each passage below is the named lines of the named file at that commit, byte for byte, inside its fence. A reread at a new pin replaces this file with one named for that commit, and the diff between the two is the reread.

## `crates/weaver-types/src/config.rs`, lines 43 to 45

The head of `AgentConfig`, the agent's declared surface: its members are kebab-case in the file and sit at its top level.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AgentConfig {
```

## `crates/weaver-types/src/config.rs`, lines 97 to 120

The turn-context election: an optional member of `AgentConfig` whose absence the agent reads as the enum's default, and the enum, closed at two.

```rust
    /// **Whether a turn sees the turns before it**, per `weaver-types-Spec`
    /// section 2 (#423, the operator's word of 2026-11-10): absent means
    /// `continuous`, the ruled meaning of the field's absence and what every
    /// load did before the election. Admin resolves the absence at the enter,
    /// so the worker never re-derives it.
    #[serde(default)]
    pub turn_context: Option<TurnContext>,
}

/// **The turn-context election**, per `weaver-types-Spec` section 2 (#423):
/// `continuous` keeps the session's context accumulating turn by turn, which
/// is what every load did before the election, and `independent` returns the
/// decode context to the identity prefix before every turn, so each turn is
/// answered from the prefix the open seated and its own request alone.
/// Closed at two, and a value outside them refuses at the parse naming
/// `turn-context`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnContext {
    /// The absence's meaning.
    #[default]
    Continuous,
    Independent,
}
```

## `crates/weaver-types/src/config.rs`, lines 271 to 306

The head of `DecoderInstruction`, `[spu-instruction.decoder]` in the file, through the surprisal election: the residual readout's election carries no default, so its absence refuses the parse, and the surprisal election defaults to false.

```rust
/// The decode role's declaration: which model serves it and whether the
/// residual readout is elected for the load. Absence of either field refuses
/// the parse at any depth, the no-defaulting rule of `weaver-types-Spec`
/// section 2 surviving the nesting untouched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct DecoderInstruction {
    pub model_binding: ModelBinding,
    pub residual_readout_election: bool,
    /// The probability field's election, per `weaver-types-Spec` section 2
    /// and `weaver-spu-PRD` section 13.11. Optional because the election is
    /// what makes the field exist and its absence is the ordinary posture
    /// rather than a value withheld, which is why it takes no default and
    /// its absence is not read as one.
    ///
    /// **It stands beside the readout's election and is never merged with
    /// it.** Each diagnostic election stands alone and none is bundled
    /// under a name for a set: a named set drifts as members join it, and
    /// every record already carrying the name becomes a record of
    /// something else without any event saying so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_election: Option<FieldElection>,
    /// The per-position surprisal's election, per `weaver-types-Spec`
    /// section 2 and `weaver-spu-PRD` section 13.12. A bare boolean beside
    /// the field's option because a per-position reading has no size to
    /// declare: the vector exists at every position or at none.
    ///
    /// **This election runs the other way from the two above it.** They
    /// elect an observation into existence, so unelected there is nothing.
    /// This one elects a reading out: the vector was unconditional until
    /// 2026-08-21 and the default posture is now the smaller record, with
    /// the generation's perplexity standing in its place. `default` is
    /// therefore `false` and means the vector is not produced, which is
    /// what omitting the field says.
    #[serde(default)]
    pub surprisal_election: bool,
```
