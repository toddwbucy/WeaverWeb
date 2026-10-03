# Brief: the alignment with WeaverAgent (documents, with five small code changes)

Version: v0.1, 2026-10-03. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. On 2026-10-02 the two planning seats reviewed each other's day of work.
WeaverAgent's review of WeaverWeb is issue #12; ours of WeaverAgent is
`toddwbucy/WeaverAgent#59`, answered there the same evening. This act makes WeaverWeb's
Spec true against both, and lands the five code changes that need no ruling. It closes
#12. It comes before the Replay surface and before IAM.

Keep it **narrow**: each change below is named, and nothing else moves. Where a fact is
still being ruled on WeaverAgent's side, the Spec names the issue and stops; it does not
guess the answer.

## 1. Read first

- WeaverWeb #12, the whole body. Every finding marked for WeaverWeb was verified by the
  planning seat against `main` at `f28c66c` and holds.
- `toddwbucy/WeaverAgent#59` and its answer comment of 2026-10-03, especially the
  operator's rulings on C3, Q2 and Q4, and the confirmations of Q1, Q3, Q5 and Q6.
- `toddwbucy/WeaverAgent#60` (a dropped verb can strand the worker), `#61` (connector
  access on the box), `#62` (the sink's hardening), and the 2026-10-03 comment on `#50`.

## 2. What WeaverAgent settled or confirmed (the facts this act writes against)

1. **A gate close may name no turn.** When the agent's working structure holds a hole, the
   close is `kind: "stopped"` with no turn. Stopped and refused closes carry **`reason`**,
   not `text`. `finish` has one value, `"length"`.
2. **The load event and its `wall_ms` are the harness's**, on the harness's clock, which
   is not monotonic. They reach WeaverWeb through the sink admin opens, but the word is
   the harness's, not admin's. `show` is admin's.
3. **`show`'s `load` member and the load event's payload are different shapes, and stay
   so** (C2). `show` answers `LoadFacts`; the event carries the elections.
4. **An unclean stop writes no `unload`.** The writer's queued tail can be lost too.
5. **A turn whose request has crossed to the agent runs to its end** whether or not its
   caller is still waiting. A gate-con that drops it loses only the delivery.
6. **All gate connections land in one conversation.** Turns from different callers
   interleave in the agent's one session working structure.
7. **A dropped admin invocation is not safe today** (#60). What a drop means is ruled in
   #50; WeaverWeb's invoker follows whatever is ruled.
8. **Declarations are the operator's to place** (Q2, operator's ruling). WeaverWeb
   proposes files the operator installs. `validate` takes only an agent's name and judges
   the declaration placed on the box; no verb judges a supplied declaration.
9. **A restore from a save point is a new run, and turn keys restart at 1** (C3,
   operator's ruling). The gate contract does not move, and WeaverWeb's turn keys hold.
10. **The prompt file's digest (`identity_file`) and the `[state-management]` values in
    effect are run conditions** (Q3), members of a run's tuple.
11. **After WeaverAgent #58, a lineage's parent is read from `built_from` only**; a
    lineage without it is a continuation (Q1). A branch at a turn is made by WeaverAgent's
    offline builder and then restored.
12. **`no_such_agent` means "not registered on this box"**, never "unloaded" (Q6).
13. **Connector access on the box is being ruled in #61.** Neither connector's needs are
    what WeaverWeb's Spec says today.

## 3. Changes to the Spec

### 7.1, the gate

- Replace "an unnamed close is this crate's own defect" (around line 2297). A close that
  names no turn is the agent's `stopped` where it carries a `reason`, and renders as the
  agent's stop with that reason. It is this crate's defect only where the close is
  neither: no turn, and not a stop the agent explains.
- Replace "what gate-con needs on the box is a uid that list names and nothing more"
  (around line 2322). Say what gate-con needs is what the box grants it for the gate, as
  ruled on `toddwbucy/WeaverAgent#61`, and that an allow-list entry alone does not reach
  the socket. Name no group, mode or path.
- Add, in a sentence each: a turn whose request crossed runs to its end whatever happens
  to the caller (fact 5), so a turn gate-con abandons at shutdown has an **unknown
  outcome**, and the trace through admin-con is where its outcome is read. And every gate
  connection lands in the agent's one conversation (fact 6).

### 7.2, the admin verbs and the trace

- Replace "user reads the trace file through group read access" (around line 2358) and
  the matching clause in the section 1 module note (around lines 55-57) with the same
  shape as 7.1: what the box grants for the trace, per #61.
- **The trace unreadable** (operator's decision, 2026-10-03): admin-con that cannot read
  the trace still connects. Its hello marks the trace unreadable, the server records
  that on the row and treats the connection's ceiling as empty, and no verb is asked
  until a later admission reads the trace. The reason: the ordering of `show` answers
  against trace events rests on the replay boundary, so verbs without the trace would
  lose the ordering the four tuple clauses of section 9 enforce. **This act writes the
  rule; the code is the next code act.** Add the section 9 row as owed, not as
  enforced.
- Replace "Compose writes its draft and asks `validate`" (around line 2203, section 6) and
  the oracle paragraph (around lines 2605-2620) per fact 8. Compose keeps drafts and
  produces a declaration file for the operator to install. `validate` judges only what is
  placed on the box, so its answer is a reading of the installed declaration, not of a
  draft. Section 2.4's "the last answer `validate` gave" member stays, and says it is
  meaningful only where the operator installed this row's declaration. Keep the corpus
  pin; it is still the draft's staleness rule.
- Wherever the Spec names a dropped or timed-out verb, say that what a drop leaves on the
  box is ruled in #50 and #60, and that WeaverWeb assumes nothing about it until then.
- `no_such_agent` lands as "not registered on this box" and never as "unloaded" (fact
  12), if the Spec maps refusals anywhere.

### 2.12, the registered agent, and every "admin's word"

- **Whose word.** The load event and its date are the harness's, carried by the sink
  (fact 2). Sweep "admin's word" and "admin's date" (around lines 1095, 1113, 1170, 1192,
  2387, 2505, 3009 and the section 9 row at 3121) so that each says what it means: a
  `show` answer is admin's word, a trace event is the agent's own record, and both are
  the agent's side and never the data plane's. The section 9 node names
  (`web-tuple-is-admins-word-...`) are identifiers; rename one only if its claim moved,
  and keep the conformance headers in sync if you do.
- **The tuple on the row is opaque.** Say it is kept exactly as its source gave it, so its
  shape differs by source (fact 3), and the source is recorded beside it (migration
  0011). Nothing compares it. **The run's tuple of section 2.2 is the compared one**, and
  the two are not the same thing.
- **The load state's freshness.** Add that an unclean stop writes no `unload` (fact 4), so
  a load state from an event can outlive the process. The state is never fresher than its
  date, and a turn's start and close refresh it (see the code changes). The full answer
  waits on #58's reset event, named as owed.

### 2.2, the run

- Add `identity_file` and the state-management values in effect as members of the tuple
  (fact 10), with one line each on what they are and where they come from (the load
  event). **Spec only**: the migration lands when WeaverAgent's load event carries both,
  and the Spec says so.

### 3.1 and 5, lineage and branches

- Add one paragraph to 3.1 naming #58's lineage change (fact 11): once it lands, the parent
  is read from `built_from` only, and a lineage without it is a continuation. A restore
  is a new run (fact 9). Do not rewrite the ingest's rules now; name the change as owed
  when #58 merges.
- Section 5: one sentence that a branch at a turn is made on the box by WeaverAgent's
  offline builder and restored, so a staged arm that branches is a proposal for the
  operator, like a declaration (fact 8).

### 2.13, identity and access

- One paragraph: an agent holds one conversation, so turns granted to two people on one
  agent interleave in one context (fact 6). The console says so wherever more than one
  person holds `turn` on an agent. This is a rule for the surfaces; nothing enforces it
  yet.

### The live trace is not a record

- Section 7.2 (around line 2456) already says the live trace is a window. Scope "no
  durable copy" to the live trace, and say that derived series reach the store lawfully
  through `weaver-analysis-web-contract` (section 7.3).

## 4. Code changes

Each is small. Run the five-class checklist (two copies of one fact, ambiguous commit,
multi-file state, filesystem trust, unbounded or unjoined) over each and say in the PR
body which classes it touched.

1. **`GateClose` gains `reason: Option<String>`** (`src/adapters/gate.rs`), read from the
   close, skipped when absent. `raw` stays.
2. **Delete `GateAdapter::socket_exists`** and its doc comment. It is unused and carries
   the retired socket-existence inference.
3. **A turn abandoned at gate-con's shutdown has an unknown outcome.** Today the server
   answers its caller as not connected. Give `TurnError` a variant for a turn that was
   sent and never answered, distinct from a turn that never left, and use it where the
   connection ends with turns in flight. Say in its doc that the trace holds the outcome.
4. **`turn.started` lands as `active` and `turn.closed` as `idle`** in the listener's
   observation mapping (`src/link/listener.rs`, around line 1881), under the same rules
   as `load` and `unload`: not while behind the replay boundary, the event's own time, and
   source `event`. **Perturbation-verify it**: remove the mapping and a test that reads
   `active` between the two events fails. Check the interplay with a `show` answer that
   lands between them against the four ordering clauses.
5. **The `Invoker` doc comment** (`src/link/admin_con.rs`, around line 81) promises a
   cancel-safety today's admin cannot give. Reword it: the real invoker must not end a
   verb it has started until WeaverAgent #50 rules what a drop means (#60), and admin-con's
   dropping at `VERB_BOUND` and at the shutdown grace is held for review against that
   ruling. `NoVerbs` is unaffected. Do not change the behaviour in this act.

## 5. Litter

- `docs/project/issues-carried-2026-09-26.md`: generalize the sentence naming a box and
  what its store holds (around line 56). Say the seed's session table stores tokens in
  the clear, with no box named and no count.
- Spec 9 rows and section 1's module note: update any count or row the changes above
  move, in the same commit as the change, per the house sweep rule.
- `CLAUDE.md`: if any sentence there repeats a corrected claim (the trace "by group
  read", "admin's word"), correct it in the same act.

## 6. Out of scope

- The code for the trace-unreadable mark (next code act).
- The migration adding the two run-tuple members (when WeaverAgent's load event carries
  them).
- Ingest changes for #58's lineage and new event kinds (when #58 merges).
- Anything on the box: provisioning, groups, the install script (after #61 is ruled).
- The real invoker (after #50).

## 7. Acceptance

- Every WeaverWeb-side finding in #12 is either changed by this act or named in the PR
  body as owed, with the issue that holds it.
- `cargo build --locked`, `cargo test --locked` with `DATABASE_URL` set,
  `cargo clippy --all-targets --locked` clean, `cargo fmt`.
- The `turn.started` mapping's test fails with the mapping removed.
- ASCII only, absolute dates, superseded text removed. **No group name, mode, path or uid
  enters the repository**; the Spec cites #61 instead. Check visibility before pushing.
- PR body with `Implements:` lines naming each Spec section changed, `Closes #12`, and
  the litter named.
