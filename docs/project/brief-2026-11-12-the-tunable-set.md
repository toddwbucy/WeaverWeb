# Brief: the declaration screen saves the whole tunable set a serving load requires

Written by the WeaverWeb Executor on 2026-11-12. It draws on the suite Planner's brief of that day, the Executor's measurement of WeaverAgent at `5ec7ecf8`, and the Planner's call on that measurement. Stack `act/the-tunable-set`, from `main` at `a6e5b9be6677f2c43e97bea5c387e41eb8067bdf`.

## Why

WeaverAgent #539 merged on 2026-11-12, and its `main` is now `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`. From the next stack install, a serving load reads six sampling defaults from the declaration's `[spu-instruction.decoder] tunable-values`: temperature, top-k, top-p, repetition-penalty, repetition-window and seed. It refuses the load as `ConfigInvalid` on `tunable-values.<name>` where one is missing or out of range. The sources are `weaver-types-Spec` section 2 and `weaver-harness-Spec` section 6.1, with the ranges in `weaver-spu-Spec` section 8.

The SPU also refuses a load whose declaration lacks `context-capacity` or `max-tokens-per-turn`, which it already did at `a3543b31`. The screen on `main` writes a sampler member only where the operator set one, and never writes `context-capacity`. A file derived from it would therefore be refused for a missing tunable value.

## What was ruled

- **The operator, 2026-11-12**, on the Planner's recommendation: a missing key refuses, on the agent's side as built, and WeaverAgent adds no default. So the console must not save a serving declaration that lacks one.
- **The Planner, 2026-11-12, call (a):** the tunable values the screen saves are the whole set a serving load requires at the pinned commit. That is eight names at `5ec7ecf8`, so no file from the screen is refused for a missing tunable value.
- **The Planner's defaults**, each stated in the Spec as the Planner's and open to the operator's overrule:
  1. All eight are required when a declaration is saved. A save lacking one is refused naming its field, like every other refusal in section 6.1, and saves nothing.
  2. **The form shows five starting values**, the constants the SPU ran with until the carrier: temperature 0.7, top-k 40, top-p 0.95, repetition-penalty 1.1, repetition-window 64. They are visible and editable, and they are written because the operator saves them, so the file states every value. The Spec says where the five come from, by commit.
  3. **The seed, `max-tokens-per-turn` and `context-capacity` have no starting value.** The operator enters each. Whether the screen should show a starting value for either session parameter is one open point for the operator. It quotes the SPU's own comment on 32768 without adopting it. A screen that offers to draw a seed would be a question, and is not built.
  4. **The ranges are the agent's, mirrored exactly and no further**, judged on the screen, so a value the load would refuse is refused at save, naming the field. Where the screen's existing bound is tighter than the agent's (the seed and `max-tokens-per-turn` at TOML's largest integer), it stays and the Spec says so. The two known edges are stated and not solved: a seed above 2^53 is rounded by the declaration's number type, and a temperature small enough to narrow to a 32-bit subnormal is in range by the table. That is one below 32-bit float's least normal value, about 1.18e-38, which the Planner put at about 1e-37.
  5. **The membership is tested**, so the list cannot drift silently. The required set is read from the agent at the pin, in three places. The repository cannot link the agent's crates, so it carries a dated copy of those places with its commit, as #87 carried the state table, and a test over the copy fails when the screen's set and the copy's differ.
  6. **`max-tokens-per-turn` is required**, and the Spec says what it is since the carrier: the session's ceiling, fixed at load, with each generation carrying its own bound under it. **`context-capacity` is added to the screen** and required.
  7. **The per-turn paragraph and the waits-on row are rewritten**, not appended to. Each generation now carries its six values, read from the declaration. What still waits is the console sending a turn that carries values of its own.
  8. **The partial-by-design members stay as they are**: the session, the trace sink, the gate's access rule and each binding's devices. The Spec says so in one sentence, so nobody reads this act as making the file loadable by itself.
  9. **Rows saved before the new build** are named stale by their pin under section 2.4. Reopening one refuses its save until the eight are filled. No migration.
- **The operator, 2026-11-12:** WeaverWeb #23 stays a draft and is ignored for now. It is not this act's.

## Measured, read only

All of this was read from WeaverAgent at `5ec7ecf8`, in the Executor's own clone; no WeaverAgent tree was edited.

- **The required set: eight names, read in three places.**
  - `crates/weaver-types/src/config.rs:345`, `SAMPLING_DEFAULT_NAMES`: the six. These are read by `DecoderInstruction::sampling_defaults` (`config.rs:372`) and judged at load by the harness's `judge_sampling_defaults` (`crates/weaver-harness/src/lifecycle.rs:3470`), under a serving binding.
  - `crates/weaver-spu/src/main.rs:186`, `SESSION_PARAMETERS`: `context-capacity` and `max-tokens-per-turn`, both `Disposition::OperatorTunable`. `SessionParameters::resolve` (`crates/weaver-spu/src/sampling.rs`) refuses a missing one as `Unsupplied`, naming it, at the SPU's admit.
  - The conditional rule: where `spu-instruction.decoder.field-election` is declared, a `top-k` of zero or above its depth refuses on `field-election.depth`. The screen declares no field election, so it cannot reach this rule.
- **The ranges**, as the agent judges them:

  | name | the agent's judgement |
  | --- | --- |
  | temperature | a real, finite and 0 or more as written. A nonzero value stays nonzero once narrowed to 32 bits, and the narrowed value is finite. |
  | top-k | a whole number, 0 or more, below 2^31 |
  | top-p | a real, more than 0 and at most 1 as written. A nonzero value stays nonzero once narrowed. |
  | repetition-penalty | a real, finite and more than 0 as written. It stays nonzero once narrowed, and the narrowed value is finite. |
  | repetition-window | a whole number, 0 or more, below 2^31 |
  | seed | a whole number, 0 or more, below 2^64. The agent holds it as a 64-bit float, so it rounds above 2^53. |
  | max-tokens-per-turn | a whole number, 0 or more, below 2^64. Zero is not refused. |
  | context-capacity | a whole number, 0 or more, below 2^32, at resolve. A zero capacity is refused when the session opens, by both engines (`crates/weaver-spu/src/decoder/gguf.rs:454-461`, `crates/weaver-spu/src/decoder/native.rs:405`). |

- **Relations:** the agent judges none between the two session parameters. It judges none between a session parameter and a model, short of the devices. The context capacity sizes the KV term of each device's share at the SPU's admit, so a capacity the devices cannot hold refuses there. That needs the model and the devices, which the screen does not know, so the screen does not judge it.
- **Diagnostic declarations:** the screen composes none. It writes no `binding-kind`, and `AgentConfig.binding_kind`'s absence means serving (`config.rs:54-59`). Every declaration it saves is serving.
- **The five starting values:** WeaverAgent `a3543b31f264b269f66ab55d8767f25290ec9d61`, `crates/weaver-spu/src/main.rs:177-181`, the `KNOBS` the SPU froze. They are identical at `61d99dce`, the last `main` before #539.
- **Moving the pin to `5ec7ecf8`**:
  - **The state table is unchanged.** The admin-operator contract's section 4.1 table and its two paragraphs, at lines 282 to 327, are byte-identical to the copy #87 holds.
  - **The `[restore]` rule is unchanged.** Every line of `weaver-admin-Spec` section 4 that names `[restore]` is identical.
  - **The grammar members the screen writes are unchanged.** `config.rs` gained the sampling defaults and nothing else (89 lines added).

  So `STATE_TABLE_READ_AT` and `RESTORE_RULE_READ_AT` move with `CORPUS_COMMIT`, once reread.

## Ruled out

- No default for the seed, `context-capacity` or `max-tokens-per-turn`, and no drawn seed.
- No migration. The body is stored whole, and no schema member moves.
- No edit in WeaverAgent, and no link to its crates.
- **Not a loadable file.** The session, the trace sink, the access rule and the devices stay Provision's and start's.
- No judgement the agent does not make, and none that needs a device's or a model's facts.
- The screen reaches no agent and no host. Nothing touches the console's unit or its database.

## The sub-acts

1. **`act/the-tunable-set-1-spec`, the Spec (docs).**
   - Section 6.1:
     - the document's `sampler` member;
     - the sampler field, rewritten;
     - the refusals;
     - the waits-on rows;
     - the agent's file paragraph, with the partial-by-design sentence.
   - Section 2.4: the pin, restated.
   - Section 9: the rows.
   - Section 10: the open point on the session parameters' starting values.
   - The design system's `declare.md` sampler pane.
   - The hub's draft of interface question 5, rewritten to what remains.
2. **`act/the-tunable-set-2-page`, the page and its tests (code).**
   - The sampler's eight members, required, with `context_capacity` new.
   - The ranges.
   - The five starting values in a new declaration's form.
   - `CORPUS_COMMIT`, `STATE_TABLE_READ_AT` and `RESTORE_RULE_READ_AT` at `5ec7ecf8`, and the state table's copy renamed for that commit.
   - The membership copy and its test.
   - The refusal, starting-value and stale-row tests.

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test.
- **Every guard fails its test with the guard removed:**
  - each name required;
  - each range;
  - the narrowing rules;
  - the starting values;
  - the membership against the copy;
  - the three pins.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
