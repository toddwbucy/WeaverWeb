# Brief: the declaration states the agent's three elections, and staging sets the backfill

Written by the WeaverWeb Executor on 2026-11-13, from the suite Planner's word of that day on the Executor's scope of a first agent deployed in a virtual machine. Stack `act/the-three-elections`, from `main` at `ced3db5a8ea29301e46eea220300e5c317d8599e`.

## Why

The first agent the console will drive declares `turn-context = "independent"`, `surprisal-election = true` and `residual-readout-election = false`. All three are members of the agent's grammar at this build's pin, WeaverAgent `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291` (`crates/weaver-types/src/config.rs`). The declaration screen holds none of them:

- **`turn-context`.** The agent reads its absence as `continuous`. A file derived from a saved row therefore declares an agent whose every turn sees the turns before it, which is not the agent declared. Measured on that agent's own 86-turn run, the accumulated context would pass its 32,768 capacity at about turn 75.
- **`surprisal-election`.** The agent reads its absence as false, so the derived file elects away the per-position surprisal. That is the reading the record view exists to show.
- **`residual-readout-election`.** The agent requires it, with no default. Today the file writes only a comment saying it is not declared, so the file refuses at the parse for that reason too.

The scope also found that a console restart refills an agent's live window only from admin-con's backfill, 1 MiB by default. One 86-turn run is 1,025,719 bytes, so it barely fits, and a seed plus a run does not. admin-con takes `backfill_bytes` up to 256 MiB, but the staging tool offers no parameter for it, so a staged install cannot set it.

## What was ruled

- **The Planner, 2026-11-13:** build the three members as one sub-act, on the document, the form, the canonical value, the derived `agent.toml` and Spec 6.1. Hold them to the pin's `config.rs` by a test, as `tunable_set.rs` holds the tunable set.
  - A row saved before the act reopens, and its save asks for the required member.
  - `binding-kind` and `[state-election]` are named in the file's comment block as not declared on this screen.
- **The Planner, 2026-11-13:** `backfill_bytes` becomes one optional staging parameter for admin-con, bounded as admin-con bounds it, as its own sub-act on the same stack.
- **The Planner, 2026-11-13:** the turn form's note that turns "interleave in the agent's one conversation" is false under an independent turn context. It stays as it is in this act, and the stack's body lists it as known-false, waiting on the join from a register row to its declaration. That join is the operator's question and is not designed here.

## Measured, read only

Read from WeaverAgent at `5ec7ecf8`, in the Executor's own clone. No WeaverAgent tree was edited.

- **`AgentConfig.turn_context`**: `Option<TurnContext>` with `#[serde(default)]`, at the top level. The enum `TurnContext` is kebab-case and closed at `continuous` and `independent`, and a third value refuses at the parse naming `turn-context`. Absence means `continuous`, which admin resolves at the enter.
- **`DecoderInstruction.residual_readout_election`**: `bool`, with no serde default. Its absence refuses the parse at any depth (the no-defaulting rule of `weaver-types-Spec` section 2).
- **`DecoderInstruction.surprisal_election`**: `bool` with `#[serde(default)]`. Absence is false, "which is what omitting the field says".
- **`binding-kind`**: absent means serving. **`[state-election]`**: absent means admin's default election, the envelope of every kind. Neither is declared on the screen.
- **admin-con**: `backfill_bytes` defaults to 1 MiB (`DEFAULT_BACKFILL_BYTES`). `AdminConConfig::load` refuses one above `MAX_BACKFILL_BYTES` (256 MiB) and admits zero.

## Ruled out

- No migration. The body is stored whole, and a row saved before the act still parses.
- No edit in WeaverAgent, and no link to its crates.
- Not a loadable file: the session, the trace sink, the access rule and the devices stay Provision's and start's.
- No change to the turn form's note, and nothing toward the join from a register row to its declaration.
- No turn route for a program.
- The screen still reaches no agent and no host. Nothing touches the console's unit or its database.

## The sub-acts

1. **`act/the-three-elections-1-document`, the three members (code and Spec).**
   - `turn_context`, `residual_readout_election` and `surprisal_election` on the document, the form, the canonical value and `agent.toml`.
   - The residual readout's election required at save. The other two keep the agent's absence meaning at the parse and are written at every save.
   - `binding-kind` and `[state-election]` named in the file's comment block.
   - A dated copy of the three passages, and its membership test.
   - Spec 6.1 and section 9.
2. **`act/the-three-elections-2-backfill`, the staging parameter (code and Spec).**
   - An optional `backfill_bytes` for an `[[agent]]`, refused above admin-con's own bound before anything is written, and composed into admin-con's config.
   - Its test, and Spec 8.1 and section 9.

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test.
- **Every guard fails its test with the guard removed:**
  - the required election;
  - each election's values;
  - each member in the canonical value and in the file;
  - the membership against the copy;
  - the pin;
  - the stale row;
  - the backfill's bound.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
