# Brief: the sign-in switch

Date: 2026-11-24. From the Planner, to the WeaverWeb Executor. This brief is the stack's
first commit, so that what was asked stands beside what was built.

## Goal

Land the pre-release sign-in switch as a proper act: the config member `no_sign_in_as`,
its Spec text, and a fix for each of the six findings of the Executor's reading of
2026-11-21. The switch exists today as 293 uncommitted lines in ten files.

## Rulings

The operator, 2026-11-21: "Take down the user authentication. Just come to an empty page.
No sign in right now. When we have everything built out and we're ready to do a release,
then we can look at a sign in page and add all that authentication security."

The operator, 2026-11-23, on the order of work: "The sign-in switch as a proper act
first."

**Two sentences are the operator's and are not settled by this brief.**

1. The sentence that states the switch: what it is, whether sign-in is off or on by
   default before release, and what release does with it.
2. What sign-out and the sign-in page do while the switch is on.

Each is a marked placeholder in the Spec until the operator rules. The stack does not
leave draft while a placeholder stands.

The Planner's rulings on the Executor's plan of 2026-11-24:

1. The clippy fix for `main` rides in this stack as child 0.
2. The standing person moves from the process-wide `OnceLock` into `Policy`, so that the
   path with no cookie can be driven in process.
3. The start refuses a person who is unknown or disabled. It does not refuse a person
   who holds no grant: that person is served and sees nothing.
4. A refusal while the server runs is held by a read of the person. It writes no audit
   record, and it logs once per change of reason.
5. The lock covers the opening of a standing session, and nothing else.
6. The 293 lines are carried whole as the first commit of child 2. Each fix after that
   is a commit of its own, with its test.

## What is ruled out

- Any choice of the two sentences above.
- Any change to `CLAUDE.md`. It is an instruction set and goes to `main` on the
  operator's word.
- Any change to the sign-in page, to sign-out or to the bar, until the operator rules on
  sentence 2.
- Any passkey, certificate or token work that the six findings do not need.
- Any change under `src/link/` beyond the one config literal the 293 lines already
  touch. The arrival-order test of `link::client_tests` fails in the full suite on `main`
  today. It is measured apart from this act and is named in each suite report until it is
  settled.

## Read first

- `docs/weaver-web-Spec.md` sections 2.8, 2.13 and 9.
- `src/config.rs`, `src/surfaces/gate.rs` and `src/bin/weaver-web.rs`.
- The sign-in token's path: `host::sign_in_token` and `gate::open_by_sign_in_token`.
- `tests/startup.rs`.

## The sub-acts and their order

The stack is `act/the-sign-in-switch`, from `main`.

| Child | Branch | What |
|---|---|---|
| 0 | `act/the-sign-in-switch-0-clippy` | The two casts at `src/hugging_face/cache.rs` line 265, allowed with a stated reason |
| 1 | `act/the-sign-in-switch-1-spec` | Spec 2.8, 2.13 and 9, with the two placeholders |
| 2 | `act/the-sign-in-switch-2-switch` | The 293 lines, the move into `Policy`, findings 2 to 5, and the in-process half of finding 6 |
| 3 | `act/the-sign-in-switch-3-startup` | Finding 6 at the real binary, in `tests/startup.rs` |

Child 0 goes first. Children 1 and 2 branch from child 0's head. Child 3 depends on
child 2.

## Acceptance

- Every child passes the gates: format, build, both clippy runs and the suite. A suite
  report gives passed, failed, ignored and skipped counts.
- Every fix has a test that fails without it, and the report says which tests were seen
  to fail with the fix removed.
- Finding 2: the start refuses an unknown person and a disabled person, in process and
  at the binary.
- Finding 3: twenty uses by a disabled person leave the audit's row count unchanged, and
  the next use after the person is enabled is served.
- Finding 4: the Spec and the code's doc say what an opening does to the person's other
  tokens, and a test pins it.
- Finding 5: a served use does not wait on an opening, and eight first uses at once
  leave one session row.
- Finding 6: with no cookie, `/record` answers 200 with the standing person's name, in
  process and at the binary. With no person named, `/record` links to `/sign-in` as
  before.
- A server whose config names no standing person behaves as it does on `main`.
- Spec verdicts are "Spec gap" for 2.8, 2.13 and 9, on the ruling of 2026-11-21.
- No diff, body or commit message holds a box path, a host name, an account or a
  credential.

## Scope boundary

About 25 files. Draft pull requests only. Nothing is pushed without the Planner's word
naming the head.
