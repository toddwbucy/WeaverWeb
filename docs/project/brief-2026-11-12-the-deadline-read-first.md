# Brief: a bounded write reads its deadline before it is polled

Written by the WeaverWeb Executor on 2026-11-12, from the suite Planner's brief of that day.

That brief accepted the Executor's sweep of the store writes awaited under tokio's timeouts: 9 of 55 production sites have the shape #81 fixed, and none loses state. The sweep is the hub note `2026-11-12-sweep-store-writes-under-tokio-timeouts.md`.

The stack is `act/the-deadline-read-first`, from `main` at `a6e5b9be6677f2c43e97bea5c387e41eb8067bdf`. It is independent of `act/the-tunable-set`, so the two collapse in either order. They share one file, `docs/weaver-web-Spec.md`, in hunks apart.

## What was ruled

The operator had the class fixed once, in #81. The Planner made it an act on 2026-11-12, in parallel with the tunable set. Each point below is the Planner's default, stated as such:

1. **All nine move to `bounded_at` or its equivalent.**
   - A write whose deadline has passed is not sent, and the caller's report agrees with what the store holds.
   - The Spec states what is said in each outcome where a caller reports to a person (`listener.rs` 1701) or raises an alert (`alerting.rs` 284).
2. **The in-flight window stays open.** That is a statement sent before the deadline and answered after it, which Spec section 10 holds open.
   - This act does not close it.
   - The Spec says, in one sentence, that it applies to these nine as it does to #81's landing.
   - The open point stays where it is.
3. **Site 1158 has no caller in the binary.** The Executor proposes either moving it with the rest or removing the uncalled path, with the reason, and names any test that would go.
4. **The gates:**
   - the generic deterministic perturbation, with no database;
   - a hook at each of the nine sites, showing rows landed while the caller took its timeout branch before the change, and none after. The Planner named the existing `delay_next_outcome` seam for 1461;
   - test hooks are `cfg(test)` only and add no production branch;
   - the suite with `DATABASE_URL` and `PGUSER` on a scratch database at eight test threads, and the self-check.
5. **A class guard**, so that a tenth site does not appear unnoticed. The Executor proposes the cheapest honest one and says which it would pick.

## The goal

Every store write the server bounds by a deadline reads the deadline before it is polled, as the admission's landing does since #81 (Spec 7.2).

- A write still waiting when its deadline comes is then never sent. It might be waiting on the pool, on a lock, or on an earlier statement's answer.
- So a caller that reports "not written" at its bound is telling the truth, apart from a statement already in flight.

## The nine sites

| site | the write | the caller's report at the bound |
|---|---|---|
| `src/link/listener.rs` 197 | `audit_unsent`, the `failed` outcome of a first record whose frame never left | an error line |
| `src/link/listener.rs` 1158 | `audit_refusal`, the server's refusal of any verb but `show` | `VerbError::Audit` and an error line |
| `src/link/listener.rs` 1427 | `audit_first_as`, a first record | an error line, after which `Unsent` answers the first |
| `src/link/listener.rs` 1461 | `audit_outcome`, a verb's outcome | an error line |
| `src/link/listener.rs` 1701 | `audit_refusal`, a person's refused ask | 503, a stall, and an alert where the refusal came after authorization |
| `src/alerting.rs` 260 | `alert_contact_restored` | an error line |
| `src/alerting.rs` 284 | `raise_alert` | an error line, and the operations log's line with `"alert": null` |
| `src/alerting.rs` 378 | the drainer's `apply`, which commits in the claim's transaction | an error line, then a back-off of the row |
| `src/alerting.rs` 458 | `inbox_failed`, the back-off | an error line |

## Ruled out

- **Closing the in-flight window** (point 2).
- **Any change to `bounded_at`**, which is #81's and keeps its own tests.
- **Any change to the 46 sites the sweep graded without the shape**, beyond the guard's list of them.
- No box path, host name, account or address in the repository, the brief or a pull request body.

## What to read

- **The Spec:**
  - section 7.2, the admission's landing: "A landing is never polled once its deadline has come";
  - section 2.13, a person's verbs: the held ask's one deadline, and each refusal's one record;
  - section 2.14, alerts and the operations log;
  - section 9, the row on the server's own asks;
  - section 10, "A statement sent before its deadline and answered after it".
- **The code:**
  - `src/link/listener.rs`: `bounded_at` and `bounded_tests`, `Unsent`, `record_ask`, `record_outcome`, `verb`, `ask_held`;
  - `src/alerting.rs`: the worker and the drainer;
  - `src/store/audit.rs` and `src/store/alerts.rs`.
- **tokio 1.53.1 `Timeout::poll`** (`src/time/timeout.rs`): "First, try polling the future".
- **sqlx-core 0.9.0's pool defaults:** `acquire_timeout` 30 s and `test_before_acquire`.

## Sub-acts, in order

The sub-acts are linear. Each is a draft against the one below it, and the first is against the stack.

1. **The rule, and the listener's five.**
   - **The helper:** a crate module, `src/deadline.rs`, with `before` (to an instant) and `within` (a span from now). They read the deadline at every poll and before the write, as `bounded_at` does, without its close signal.
   - **Its tests**, with no database:
     - tokio's order, held as a test;
     - the same write under `before`;
     - a deadline already past;
     - a write with time left.
   - **The five listener sites:** 197, 1158, 1427, 1461 and 1701 move to it.
   - **The hold** (`deadline::hold`, `cfg(test)`):
     - a site records the deadline it bounds its write by;
     - the store function waits until that very instant, just before the statement that commits.
   - **The Spec:**
     - 7.2 extends the landing's rule to every write the server bounds, with the in-flight window in one sentence;
     - 2.13 says what a person is told in each outcome of a refusal's record;
     - section 9 gains the row.
2. **The alerting four**, 260, 284, 378 and 458, the same way.
   - The Spec's section 2.14 says what the operations log and the error line say in each outcome of the raise at 284.
3. **The class guard** (point 5).

## Proposals the Planner rules on

**Site 1158: move it with the rest.**

- **Why:** `Listener::verb`'s refusal of a verb other than `show` has no caller in the binary, but it is the guard of a section 9 claim, "it asks `show` only".
  - That claim's instrument is `link::server_asks_tests::the_server_asks_show_only_and_its_show_is_audited`, which asks `load` and reads the refusal record.
  - Moving the site keeps one rule for every bounded write, and no test goes.
- **What removing it would take with it:**
  - the refusal half of that test;
  - the `VerbError::ServerAsksShowOnly` variant;
  - the section 9 perturbation "drop the `show`-only refusal".
- Whether the server keeps a public verb API that only tests call is a design question for its own act, not this one.

**The class guard: a test that reads `src/`.** Two candidates:

- **(a) A test that scans `src/`.** It counts each file's `timeout(` and `timeout_at(` calls and holds every count to a list. The list gives one reason per call, in the file's order, from the sweep's seven reasons.
  - A new call fails the test, and so does a removed one.
  - It touches no production file and survives moved lines.
  - Its weakness: it reads text, not resolved paths.
- **(b) A clippy `disallowed-methods` entry for `tokio::time::timeout` and `timeout_at`.** It resolves paths, so an alias cannot hide a call. But:
  - `--all-targets` would also lint the test files' hundreds of calls, each test module needing an allowance;
  - a lint attribute cannot sit on a bare expression, so a site inside a `match` or a tail expression would take a whole function's allowance. That allowance would hide a second call added later.

The Executor would pick (a).

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test.
- Each test fails with its guard removed, shown by a perturbation:
  - moving a site back to tokio's `timeout` or `timeout_at` lands its row while the caller takes its timeout branch;
  - polling the write before reading the deadline in the helper fails its generic test.
- No test reaches an agent or a box. The connectors are fakes.
- The gates of `CLAUDE.md`, each with its exit status, the suite with `DATABASE_URL` and `PGUSER` set and eight test threads, its counts, and nothing skipped.

## Scope boundary

WeaverWeb alone. Nothing in WeaverAgent is touched, and nothing is filed or written on GitHub beyond this stack's pull requests.
