# Brief: act 11, IAM, PR 6b1: the server's own asks, audited

Version: v0.1, 2026-10-13. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 6a (#39) merged at `386eaec`. The 6a brief split the design's PR 6 three
ways; **6b is split again, one concern each**:

- **6b1 (this one): every verb frame the server sends has its audit.** Today the only
  verb frames are the server's own `show`, at an admission and at each opening of the
  trace door. This PR puts the audit's two records around every verb frame at the
  listener, exercised by those asks.
- 6b2: a person's verbs asked through the server: the role and the connection's ceiling,
  the shared hold to the enqueue, the refusals, and the agent page's forms.
- 6c: turns.

Backend first, as ruled: 6b1 adds no page.

## 1. Read first

- Spec 2.13: the server as a principal ("It may ask `show` only ... Its asks are audited
  as a person's are, the first record naming the server as the principal"), the audit's
  target and action for a verb ("the target is the agent and the action the verb"), the
  two-record rule ("This crate writes the first record before the ask leaves ... The
  outcome is a second record naming the first, written when the answer lands ... an act
  whose outcome never came is visible as a first record with no second"), and the refusal
  rule (one record, and a refusal that cannot be recorded is the server's failure).
- Design section 10 (`server` as the server's method, the principal and method agreeing).
- `src/link/listener.rs`: the admission's `show` (about line 1340), the door opening's
  `show` (about line 1730), the answer's handling where `admission_show` is matched (about
  line 1504), `Listener::verb` and `Inner::ask`. `src/store/audit.rs`: `audit_first`,
  `audit_outcome`, `audit_refusal`, `Principal`, `Target::Agent`.

## 2. Scope

1. **A first record before the frame enters the channel**, for every verb frame the
   server sends: principal `server`, method `server`, target the agent, action the verb.
   Both `show` sites, and `Listener::verb`. The record is written first and the frame
   follows; **a first record that cannot be written, or whose answer is lost, sends no
   frame** (the existing rule of every first record). At an admission or an opening that
   means the `show` is not asked and the connection closes with a reason that names the
   audit; the connector's reconnect makes the next admission. Say in the PR body which
   existing path the closing takes, and that the row's state is not left claiming an
   admission that did not complete.
2. **The record's write is bounded**: a store that stalls never holds the connection task
   past the bound the task already keeps for its admission (`silence`), and a bound that
   passes closes the connection as the failed write would.
3. **The outcome when the answer lands**, naming the first: `ok` where admin answered a
   `lifecycle-answer`, `failed` where it answered a `lifecycle-refusal` or admin-con
   answered a `VerbFault` of a verb that never ran (`busy`, `not_started`,
   `outside_ceiling`). A `VerbFault` of `unknown` or `fault` leaves the act unknown and
   writes no outcome, the first standing alone. Read the contract's two shapes (WeaverAgent's
   `weaver-admin-operator-contract`) for what tells them apart and say in the PR body
   which field decides. **A connection that ends with the frame in flight writes no
   outcome**: the first stands alone, which is the audit's "unknown", and the trace and
   the next `show` hold what happened. An admission whose `show` passes its deadline is
   that case. An outcome that cannot be written is logged at error and changes nothing
   else; the act already happened.
4. **The server asks `show` only** (Spec 2.13): `Listener::verb` with `Principal::Server`
   and any other verb is refused before any frame leaves, as one refusal record, and
   answered with its own error. `Principal::Person` stays as it is until 6b2, which
   brings the person's check; do not add a person path here.
5. **The seam 6b2 will extend**: 6b2 holds the identity exclusion shared from the
   person's check to the enqueue, with the first record durable inside that hold and no
   wait on a peer under it. Build the record step as one place in the ask path that 6b2
   can put under the hold; **do not restructure the reservation here**.
6. **Out of this PR**: any person's ask, any page, turns, #38 (show's `in_transition`
   constituents), #35 (the link flake).

## 3. Review checklist (distributed state)

Pre-review against these before telling me the draft is up:
- Every verb frame the server sends has a first record that committed before the frame
  entered the channel; no path enqueues a verb frame around it (grep every
  `ToClient::Verb`).
- The ask id to first record mapping is the connection task's, dropped with the
  connection, and an answer is matched to its own record only.
- A CHECK or a predicate passes on NULL nowhere new.
- Nothing waits unboundedly on the store inside the connection task.

## 4. Tests and perturbations

- An admission with `show` in the ceiling writes the first record (server, server, agent,
  `show`) and, on the fake connector's answer, an `ok` outcome naming it; a refusal answer
  writes `failed`; a fault of a verb that never ran writes `failed`, and an `unknown`
  fault writes none.
- A ceiling without `show`: no frame, no record.
- The door's re-opening `show`: its own first record and outcome.
- The first record refused (the existing `FAIL_FIRST_RECORD` fault): no `show` frame
  reaches the fake connector, and the connection closes with the audit's reason.
- A connection that ends with the `show` in flight: the first record has no second.
- `Listener::verb(.., "load", Principal::Server)`: refused, one refusal record, no frame.
- Perturbations, each shown to fail with its guard removed, in a fresh scratch worktree of
  the pushed commit: the record before the frame (enqueue first), the no-frame-without-a-
  record rule, the outcome naming its own first, the server's `show`-only rule.

## 5. Spec and documents

- Spec 2.13: the server's asks audited "as built on 2026-10-13" in the paragraph that
  already names the server principal; no other sentence moves.
- Section 9: the audit record row's server-ask half moves to enforced, the person's half
  staying owed to 6b2; the count and summary swept in the same commit.
- CLAUDE.md: one clause under admin-con or the listener that the server's own `show` is
  audited.

## 6. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in a fresh scratch worktree of the pushed
  commit (`git worktree prune`, a new name per run, `|| exit 1` after the add, `git log -1`
  printed inside it).
- ASCII only, absolute dates. Check visibility before the push.
- PR body: `Implements:` naming Spec 2.13 (the server principal, the audit record) and
  design section 10, the perturbation table, "Refs #18".
