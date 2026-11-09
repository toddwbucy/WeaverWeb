# Brief 2026-11-09: the moved contracts

The second stacked act. Two WeaverAgent rulings moved the door contracts under the
connectors, and the cutover (begun 2026-11-08) puts gate-con and admin-con on Olympus
within days. Before they stand there, the repository's mirrors and readers meet the
contracts as they are today. Three sub-PRs, each small: two each close or advance one
open issue, and the third bounds the Live trace pane's list, a finding on the pane grammar
stack that lands here.

## The stack

- Stack branch `act/the-moved-contracts` from `main` (`f2487dd`), draft stack PR against
  `main`; this brief lands at `docs/project/brief-2026-11-09-the-moved-contracts.md` as
  the stack's first commit. The pane grammar stack (#61) touches templates, the
  stylesheet and `reading.rs`; sub-PRs 1 and 2 touch `adapters/gate.rs`, the connectors'
  tests and the Spec's section 9. When `main` moves under this stack, the stack branch
  merges `main` in; it did once #61 reached `main`, and sub-PR 3 builds on the trace module
  that brought.
- Sub-PR branches from the stack's head, each a draft against the stack branch:
  `act/the-moved-contracts-1-the-gate-line-bound`, `act/the-moved-contracts-2-the-unrecovered-call`,
  `act/the-moved-contracts-3-the-trace-list-bound`.
- Working rules as before: ASCII; no box fact, path, posture or sudoers text; nothing in
  the store, the link's frames, the register or the surfaces changes; one target directory
  per worktree; the gates chained with `&&` into the push; the full suite with
  `DATABASE_URL` before every push; perturbations in a detached worktree of the pushed head.
- The operator is away for the morning of 2026-11-09. Everything opens as a draft; the
  planner undrafts a cleared sub-PR for Codex; the operator merges on return.

## Sub-PR 1: the gate's line bound is 49,041 octets (#64)

WeaverAgent's ruling of 2026-11-08 (`toddwbucy/WeaverAgent#508`, merged 2026-11-09
13:30 UTC) raises the gate's request line bound from 32 KiB to **49,041 octets** before
the delimiter, inclusive: the organ envelope's exact capacity, since the line rides the
64 KiB envelope base64-encoded with at most 147 octets of other members (`weaver-gate-Spec`
section 4; `weaver-gate-world-contract` section 2's seeding sentence states the number,
section 5 the refusal). The reason is the R1 adapter's 48,454-octet seeding line, and the
bound is an interim until the seed verb.

- `src/adapters/gate.rs`: `LINE_BOUND` becomes `49_041`. Its comment names where the number
  is stated (the gate Spec's section 4 and the contract's section 2) and that the refusal is
  the contract's section 5; it says the bound is the envelope's capacity in one clause, so
  a reader knows why it is not a round number. Nothing else in the file changes; the
  pre-send refusal at `request_line` stays as it is. `frames::LINE_BOUND` (the link's 4 MiB)
  and `admin_log::LINE_BOUND` (8 KiB) are other bounds and do not move.
- A test, where the adapter's tests are (or `link::client_tests` if the adapter has none):
  a request line whose JSON is exactly 49,041 octets is built, and one of 49,042 is refused
  `LineTooLong`. Perturbation: set the constant back to `32 * 1024`, and the 49,041 case
  fails. `verbs_tests::a_malformed_or_unconnected_turn_is_refused` keeps passing unchanged
  (its text of `LINE_BOUND` bytes wraps to a longer JSON line).
- Spec: check whether any sentence states the old number (the grep for `32 KiB`, `32,768`
  and `32 * 1024` under `docs/` found none on 2026-11-09; the Spec says "the gate's own
  bound" at about line 1878). If section 9 has a row on a turn's text within the gate's
  bound, it names the new test; if none, add none, since the claim is the contract's and
  the mirror is a number.
- PR body: `Implements:` the contract's sections 2 and 5 as mirrored; `Closes #64`.

## Sub-PR 2: the record carries an unrecovered call, and the readers take it (#40)

WeaverAgent #136 (merged) adds two things to the trace, both additive under
`weaver-trace-PRD` section 6: a content block `{"type":"unrecovered_call","fragment":...}`
in `message.assistant`, and a trace kind `model.unrecovered_call` (payload `fragment`,
`reason`, `grammar`), authored after `model.output`. Issue #40 asks that no reader here
refuse either, and that a conversation render the block as the model's attempted call.

- **Measure first**, as a test: a trace file served by `link::fake_relay` carrying a
  `model.unrecovered_call` record and a `message.assistant` record whose content holds an
  `unrecovered_call` block, relayed by admin-con to the real listener; assert the window
  (`Listener::windows`, `TraceEvent.kind` and `raw`) carries both records whole, the live
  view's stream sends them, the entries drainer raises no alert for them and refuses
  nothing, and the row is unchanged by them (they are not lifecycle events). Put it where
  the relay-to-window tests live (`link::admin_con_tests` or `src/reading_tests.rs`),
  following their rig. If anything refuses or drops either record, that is the finding and
  is fixed at its cause in the same sub-PR; if nothing does, the test is the pin and the
  body says so. A perturbation that makes the claim mean something: a reader that rejected
  an unknown `kind` or an unknown content `type` would fail it, so show the test fails when
  such a rejection is inserted at the reader (then remove it).
- **Rendering.** This repository renders no conversation today: the trace pane places an
  event's `kind`, `run` and `turn` as they came (sub-PR 3 of the pane grammar), and Record
  reads the analysis record, not the trace. So "show the block as the attempted call" has
  no surface yet; the body says so, and the ObservePane README gets no sentence. Do not
  build one.
- **The alert.** The issue calls `model.unrecovered_call` "the event to alert on". An
  alert on a trace kind is a new alert reason under Spec 2.14 and a ruling the operator
  makes; it is not in this sub-PR. The PR body carries it under "For the operator", and the
  planner puts the question on #40.
- PR body: `Implements:` Spec 2.13 (the window carries the trace as it came) and section 9
  with one new row: "a trace kind or content block this repository does not know passes
  through the relay, the window and the live view, and raises nothing", the test and its
  perturbation. `Refs #40` (the planner closes it after the operator's ruling on the alert).

## Sub-PR 3: the Live trace pane holds what the window holds

Codex's review of stack #61 on 2026-11-09 found that a connected live view keeps receiving
live events after the window's snapshot, and the trace module (`assets/surfaces/trace.js`)
kept every row, so a tab left open on a busy agent grew the page without bound. The ring
bounds the window at `RING_CAP` (10,000, `src/traceview.rs`). The operator ruled that #61
merges as reviewed and the bound lands here.

- **The rule:** the list holds what the window holds, not everything the window has sent.
  The module names the ring's cap once, `RING_CAP` at its top, and after each row removes
  the oldest past it, as the ring evicts. `pre.raw` stays the last event. This sub-PR
  changes one surface asset, the module, which the working rules above otherwise leave
  alone.
- **The pin:** the reading test that reads the module asserts that its `RING_CAP` is
  `traceview::RING_CAP` and that this is the 10,000 the docs state; perturbation: change
  either one.
- **Docs:** Spec 2.13's live-view clause, the ObservePane README and the stylesheet's
  comment on `.pane .stream` say the list holds at most the ring's 10,000 events, the oldest
  leaving as they leave the window; the section 9 script-policy row names the pin.
- **Measured** on the stand-in: 10,005 events sent, 10,000 held, the first seq 6.

## What this act does not do

- The PRD 3.0 Run row and `stages.md`'s Verbs row keep their `#166` note ("`load` refused
  upstream for now") until WeaverAgent's code for the 2026-11-07 ruling lands; the ruling
  says the note goes then. Admin-con's unit shape (#46 item 1) is no longer needed, since
  the agent runs in a root scope of its own and not under its invoker; that is the install
  act's to state when it writes the unit.
- Nothing about the install: no unit, no script, no box fact. The install act is the next
  brief, after the operator's rulings.

## Gates, per sub-PR, in a fresh detached worktree of the pushed head

`cargo fmt --check`; `cargo clippy --all-targets --locked -- -D warnings` with and without
`--no-default-features`; the full suite with `DATABASE_URL` (nothing skipped); the
perturbations named above, each log read against its claim; a grep of the diff for
non-ASCII, dated words, box facts. The body lists the gates as run.
