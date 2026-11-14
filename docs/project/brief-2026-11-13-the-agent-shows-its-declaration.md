# Brief: the agent page shows its declaration

Written by the WeaverWeb Executor on 2026-11-13, from the suite Planner's word of that day, after the Executor's scope reply. Stack `act/the-agent-shows-its-declaration`, built on `act/the-placement-leaves-1-removal` at `1012a181bb0d31f2b078fd79a4fb2b997b3ca64b` (#106) until #107 is on `main`, then on `main`, since the pane reads a saved head as the declaration screen does once the placement leaves.

## Why

The operator ruled on 2026-11-13, relayed by the Planner: "yes the agent page should show its declaration". This is the join from a register row to a declaration lineage that earlier acts left to him. It is the parts half of his word of 2026-11-12, that the agent page defines the agent by its parts and its wires.

## Measured, read only

Read from WeaverAgent at this build's pin, `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`, in the Executor's own clone. No WeaverAgent tree was edited.

- **The declaration's digest** that admin reports is sha256 of the agent's file, as the bytes were read (`crates/weaver-admin/src/inventory.rs`, `declaration_digest`).
- **The file the console derives is not loadable as it stands.** It writes `session`, `[trace-sink]`, the gate's access rule and the decoder binding's `devices` as comments naming what fills them. So no file an agent loads can have the digest of a derived file, and the page cannot say which saved version an agent was loaded from. It could say that only if the console wrote the complete file and recorded its digest. That is a later act, Provision.
- **What `show` reports of a standing run** (`LoadFacts`, `crates/weaver-types/src/wire.rs`):
  - the declaration's digest and the model binding's `artifact`;
  - `residual_readout`, with the field's depth, and `surprisal`;
  - `state_election`, spelled in kebab-case;
  - `state_store`, `state_member` and `composer`;
  - the drain and wind-down bounds.

  It reports no turn context, no adapter, no tunable value, no GPU requirement, no tool and no prompt.
- **What a load event carries.** Two payloads stand at the pin:
  - `Elections` (`crates/weaver-trace/src/event.rs`), which the harness's lifecycle authors. It carries `turn_context` and `tee`, the state election in snake_case.
  - `LoadRecord` (`crates/weaver-trace/src/serving/payloads/records.rs`), which the serving flow authors. Its own doc comment calls it "the former load elections with the tee election removed". It carries neither the turn context nor the tee.

  Neither carries the artifact. Both carry the declaration's digest, the readout, the surprisal and the store.
- **The register row keeps one tuple.** The listener writes the tuple from the latest `show` or the latest live load event (Spec 2.12). The admission's `show` runs at every reconnect, so after a console restart the row holds `show`'s tuple, which carries no turn context.

## Measured in this repository

- The register's name grammar (migration `0010`) and the declaration's (`0023`) are the same pattern. `0023`'s unique index allows one lineage root per name. So a lookup by name finds no lineage or exactly one.
- **A declaration names no box**, so one lineage stands behind every register row of that name.
- **The declaration screen is admin-only** (`surfaces::declarations`, `reader`: 403 to a non-admin). The agent page is for every grant holder (Spec 2.13).
- **Turns interleave in "one conversation"**: three places say so wherever more than one person holds `turn`.
  - the turn form's note, `src/surfaces/templates/agent_page.html`;
  - the grants page's note, `src/surfaces/templates/admin_grants.html`;
  - Spec 2.13's paragraph "An agent holds one conversation", which cites WeaverAgent #59.

  WeaverAgent #423 later added the turn-context election, and under `independent` the sentence is false.

## What was ruled (the Planner, 2026-11-13, on the scope reply)

- **(a) by name. No migration.**
  - The pane says "the declaration saved under this name" and lists the other live register rows of that name.
  - It may say "loaded from" only in a state that does not exist yet: a file the console wrote, its digest recorded.
- **The truth line, state by state:**
  1. No lineage of that name.
  2. A lineage saved, and the row holds no load.
  3. A lineage saved, and the agent loaded from a file the console did not write. The pane compares the reported members with the lineage's head, member by member, each reading:
     - "equal";
     - "differs" (declared X, reported Y);
     - or "not in this report".

     The pane says that a difference may only mean an older version was the source.
- **Members the agent does not report** are shown as declared, and marked "not reported by the agent".
- **The comparison is held to a dated copy** of the `LoadFacts`, `Elections` and `LoadRecord` passages by a test, as the state election's members were.
- **The comparison stays.** It sets what the standing agent was built from beside what was declared: definition against definition, not how a run is going. If the operator reads it otherwise, the smaller form is one deletion.
- **Access is a Spec line for the operator to rule by the merge:**
  - every grant holder of the agent sees the declared parts on the agent page;
  - the link to the declaration screen and the prompt's text stay admin-only;
  - the prompt is shown by its length and digest only.
- **The notes:** both sites and Spec 2.13's paragraph. Each follows the turn context and names its source:
  1. the reported turn context;
  2. otherwise the declared one, in version N, not reported;
  3. otherwise both cases.

  The grants page reads every listed agent's turn context in one query.
- **An interface question for WeaverAgent**, drafted to the report hub and not filed. It asks two things, one question each:
  - whether `show`'s `LoadFacts` should carry the turn context;
  - whether it should carry the adapter and the sampler values.

## Ruled out

- No migration.
- No edit in WeaverAgent, and no link to its crates.
- The server still reaches no agent. The page reads the store alone.
- No stored link from a register row to a lineage, and no match by digest. Both wait on Provision.
- Nothing about a run's progress: no results, and nothing that belongs to the trace viewer.

## The sub-acts

1. **`act/the-agent-shows-its-declaration-1-pane`: the Declare pane on the agent page (code and Spec).**
   - The read by name.
   - The pane's parts and the other rows of that name.
   - The truth line and the comparison over the tuple's shapes, with the dated copy and the test that holds the comparison to it.
   - The access line.
   - Spec 2.4, 2.12, 2.13, 6.1 and section 9; `docs/design/system/stages.md`.
2. **`act/the-agent-shows-its-declaration-2-notes`: the interleave notes follow the turn context**, branched from sub-act 1.
   - The turn form's note and the grants page's note, the grants page reading every listed agent in one query.
   - Spec 2.13's paragraph, and section 9.

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test.
- **Every guard fails its test with the guard removed:**
  - each state of the truth line;
  - each member's verdict;
  - a member marked not reported;
  - the access line;
  - the prompt shown by its length and digest only;
  - the other rows of the name;
  - the copy's membership;
  - the pin;
  - each source of the turn context in the notes.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
