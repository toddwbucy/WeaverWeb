# Brief: #35, the teardown counter settles before the test asserts it

From the planner seat to the executor, 2026-11-07. Issue #35 (2026-10-09): `link::tests::a_teardown_the_store_refused_is_reconciled` failed once in a full suite run, its last line `assert_eq!(lab.listener.failed_teardowns(), 0)` finding 1. It passes alone and passed the suite when re-run. The issue's diagnosis: the test waits for the row to read gate-disconnected by the reconciliation and then asserts the counter, and the row's write and the counter's decrement are separate steps, so a loaded suite can let the assertion run between them. Land this brief as `docs/project/brief-2026-11-07-35-the-teardown-counter-settles.md`, the PR's first commit.

## Scope

- **Base:** `main` at `381858e`. PR #56 is in review and touches no file this one does.
- **Branch:** `code/35-the-teardown-counter-settles`.
- **Commits:** the brief, then one `code:` commit. Two in all.
- **The diff touches `src/link/tests.rs` and this brief, and `src/link/listener.rs` only if section 1 finds the order is meant to hold and does not.** Nothing else.
- ASCII only, no em-dash. Open a **draft** PR and tell the planner.

## 1. Read the order first

Read the reconciliation path in `src/link/listener.rs`: where the deferred teardown writes the row's disconnected state and where `failed_teardowns` is decremented. State in the PR body which comes first and whether the order is a claim of the listener's or an accident of the code.

- **If the row's write comes before the decrement, or they are not ordered**, the test is asserting a step it never waited for. Make the test wait on the counter itself: poll `failed_teardowns() == 0` under the same bound (`SOON`) the test already uses for the counter's rise, in the shape of the loop above it, and keep the `wait_for` on the row as the step before it. The assertion on `connected` stays.
- **If the decrement is meant to come first and does**, the flake has another cause. Stop, say what you found, and do not change the test to fit.

Fix the cause, not the instance: if other tests in `src/link/tests.rs` assert `failed_teardowns()` after a `wait_for` on the row, they carry the same race; list them and fix them in the same commit.

## 2. The perturbation

Show the race, not just the fix: insert a delay (a `tokio::time::sleep` of a few hundred milliseconds) between the row's write and the counter's decrement in the listener, run the test as it was, and it fails at the assertion; run it as fixed, and it passes. Remove the delay. Both runs' lines in the PR body.

## Gates, in a fresh detached worktree of the pushed head

Run `git worktree prune` first; use a new worktree name with `|| exit 1` after the add; print `git log -1` inside it and check it is the PR head.

1. `cargo fmt --check`; `cargo clippy --all-targets --locked -- -D warnings` and the same with `--no-default-features`.
2. `DATABASE_URL=... cargo test --locked`, the whole suite, the `test result` lines in the PR body.
3. The test run ten times alone (`cargo test --lib link::tests::a_teardown_the_store_refused_is_reconciled` in a loop), all passing.
4. The PR body names: `Implements: none` (a test's own correctness; no Spec section moves), the order found in section 1, the perturbation's two runs, and `Closes #35`.
