# Brief: a landing already past its deadline is never polled

Sent by the suite Planner on 2026-11-11. Stack `act/the-admission-deadline`, from `main` at 1a78733a41ce052e55ea3d9290cf8d397e5f23cf. This act goes first, ahead of the declaration screen's second sub-act.

## What was measured

The Executor measured this on 2026-11-11, after the box's update.

- **The failure.** `link::server_asks_tests::an_admission_answered_near_its_deadline_does_not_complete_past_it` fails on every tree.
  - Run alone at `main`, it failed 6 of 10 times.
  - Every run took the same time, pass or fail. The deadline fires on time, so the test's clock is not the cause.
- **The cause** is in the listener's landing of the admission's `show`:
  - The landing is bounded by `bounded_at`, which is `tokio::time::timeout_at`.
  - That polls the wrapped future once before it checks a deadline already past.
  - In that one poll the landing's single-statement write can go out and commit, while the listener closes the connection `admission_incomplete`.
- **The experiment.** With the instant checked before the first poll, the test passed 10 of 10 at `main`.

## The brief

**The finding is its own act on main, and it goes first,** ahead of sub-act 2's pull request. There are two reasons:

- It makes the suite fail on every tree since the update, so every later gate would carry an asterisk.
- What it breaks is a statement the console makes to an operator: an admission reported incomplete whose answer landed anyway.

1. **Fix shape 1 in this act:** the instant is checked inside `bounded_at` before the first poll, for every caller, so a future already past its deadline is never polled. No Spec sentence moves if the Spec already says a landing does not complete past its deadline. Quote that sentence (section 7.2, or wherever it stands) in the report. If the Spec is silent, propose the sentence.
2. **Table every `bounded_at` caller** in the body:
   - which callers wrap a single-statement write;
   - which wrap something that can be cancelled cleanly;
   - for each, whether the past-due poll could have written.
3. **Fix shape 2 is not this act.**
   - **The window it would close:** a write that starts before the deadline and commits after it. This is the narrower window.
   - **Why not here:** closing it at the store (the UPDATE refusing a write past the deadline, on the store's clock) changes what the store promises and moves Spec 7.2. That is the operator's ruling.
   - **What this act does:** the Executor's draft of the finding stays as the record of it. This act's body states that the window remains and how narrow it measures. The Planner puts it to the operator.
4. **Instruments:**
   - The existing test must pass 20 of 20, pinned off any build's cores.
   - A perturbation that removes the pre-poll check must make it fail at about the measured rate. Say how many of how many.
   - Add a deterministic case if one can be made without a sleep racing a deadline: a future handed a deadline already in the past is never polled.
5. **The stack.**
   - Branch from main as a small stack under the workflow, with one sub-act.
   - This brief is the stack's first commit.
   - Gates as always. Report before any push.
   - Then, on the Planner's word, main is merged into the declaration stack, so that sub-act 2's gates read clean.

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour claimed has a test.
- The gates of this repository's `CLAUDE.md` all exit 0, the suite with `DATABASE_URL` and `PGUSER` set, nothing skipped.
- The existing test passes 20 of 20. The perturbation's failure rate is reported with its count. The deterministic case fails every time without the check.

## Scope boundary

- `bounded_at` and its callers in `src/link/listener.rs`, the tests that carry them, and the Spec's sentences on the admission's deadline and its section 9 row.
- No change to the store's statements and no new clock in the store: that is fix shape 2, the operator's ruling.
