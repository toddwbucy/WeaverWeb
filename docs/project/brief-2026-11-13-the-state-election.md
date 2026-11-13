# Brief: the declaration states the agent's state election

Written by the WeaverWeb Executor on 2026-11-13, from the suite Planner's word of that day, after the Executor's scope reply. Stack `act/the-state-election`, from `main` at `03fcb9b0ce0055c1858f1cecaf4d4c79e0d7b7b6`.

## Why

The first agent the console will drive declares a state election: every kind crosses with its envelope, and the content of each user and each assistant message crosses beside it. The screen holds no state election. A file derived from a saved row leaves `[state-election]` out, and admin reads that absence as the default election, the envelope of every kind and nothing more. That is not this agent's election. Act `the-three-elections` (#97) named `[state-election]` as the next member the screen needs before a saved declaration describes that agent.

## Measured, read only

Read from WeaverAgent at this build's pin, `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`, in the Executor's own clone. No WeaverAgent tree was edited.

- **`AgentConfig.state_election`** (`crates/weaver-types/src/config.rs`): `Option<StateElection>` with `#[serde(default)]`. Absence means the default election, which admin resolves at inventory. A serving load requires nothing here.
- **`StateElection`**: kebab-case and closed to unknown members, with `all_kinds: bool` and `keys: Vec<ElectedKindConfig>`, neither defaulted. When the table is present, both are required. `Default` is spelled `all_kinds` true and `keys` empty.
- **`ElectedKindConfig`**: kebab-case and closed, with `kind: String` and `paths: Vec<String>`, neither defaulted.
- **The kind** is a free string at the parse, meant "as the canonical form spells it". That set is closed: weaver-trace's `Kind`, 26 kinds by ruling (`crates/weaver-trace/src/event.rs`, pinned in its `tests/kinds.rs`). Nothing refuses a kind outside it, neither the parse, admin nor the tee; such an entry elects nothing. `save_point` never crosses the tee, whatever the election (`crates/weaver-trace/src/tee.rs`).
- **The paths** are open, each dotted from that kind's payload root. A path the payload lacks costs nothing and is never refused. An empty list is meaningful: presence itself is state, which matters once `all-kinds` is false.
- **A kind named twice** is refused nowhere. The tee takes the first entry for a kind (`find`), so a later entry is dead.
- **Cross-field**: admin refuses a present `[state-election]` beside the store engine `none` (`crates/weaver-admin/src/inventory.rs`, `ConfigInvalid` naming `state-election`).

## What was ruled (the Planner, 2026-11-13, on the scope reply)

- **Shape B**: rows of a kind and its paths. The kind is closed to the 25 kinds that can cross (the 26 less `save_point`), held to the pin by a copy and a test. The paths are open, one dotted path per line.
- **Bounds**: at most 32 paths for a kind, at most 256 characters for a path.
- **Refused**: a kind named twice, the refusal saying that the agent would take the first entry and drop the rest; an empty segment in a path; a control character; the two bounds.
- **Store none**: an election other than the default beside it is refused, naming admin's own refusal, and no table is written. This is the screen saying early what the agent refuses at load, on the same footing as the tunable ranges. The operator's "warn, do not refuse" was about a class spanning capabilities, which the agent itself does not refuse.
- **An empty paths list** is admitted and written. The note under the rows says so, since it matters once `all-kinds` is unchecked.
- **What shape B cannot state**, said in one sentence in the screen's note and in full in the pull request body:
  - a path is not checked against the kind's payload;
  - a payload key holding a dot cannot be spelled;
  - a kind outside the set cannot be named.
- **Stale rows and the default**: no new requirement. A row saved before the act reopens, not named stale. The form opens at the default, and a save with `sqlite` writes it explicitly, which the agent reads as the same election.

## Ruled out

- No migration. The body is stored whole, and a row saved before the act still parses.
- No edit in WeaverAgent, and no link to its crates.
- Not in this act: the join from a register row to its declaration, the turn route for a program, and an audit page. Those wait on the operator.
- The screen still reaches no agent and no host.

## The sub-act

1. **`act/the-state-election-1-rows`, the state election (code and Spec).**
   - `state_election` on the document, the form, the canonical value and `agent.toml`.
   - The kinds a row may name, a dated copy of the passages read at the pin, and the membership test.
   - The refusals above, store none's rule, and the rows' slots (`?kinds=`).
   - Spec 2.4, 6.1 and section 9.

## Acceptance

- **The Planner's:** a declaration typed with that agent's real values derives a file whose `[state-election]` parses equal to the agent's own file. The pull request body's table of what the derived file still lacks shrinks to `binding-kind` and the members Provision and start fill.
- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test.
- **Every guard fails its test with the guard removed:**
  - the kind's set;
  - each refusal;
  - store none's rule;
  - each member in the canonical value and in the file;
  - the membership against the copy;
  - the pin;
  - the stale row's reopen.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
