# Brief: act 11, IAM, PR 6b2: a person's lifecycle verbs, asked through the server

Version: v0.1, 2026-10-13. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 6b1 (#41) merged at `aa3e3ba`: every verb frame the server sends has its
first record and its outcome, and the server asks `show` only. This PR opens the person's
path that 6b1 left under `cfg(test)`. One concern: **a person asks a lifecycle verb of an
agent from the agent's page, authorized by the server, audited, and enqueued under the
shared hold.** Turns are 6c. Backend first, as ruled: the forms are plain.

## 1. Read first

- Spec 2.13, the server-side authorization item (about lines 1743-1790): "a verb is asked
  of an agent only if its principal may ask it, the person's grants on that agent
  permitting it", the ceiling of the connection the frame will be enqueued on, "**Every
  act a principal is authorized for holds the identity-and-access exclusion shared, from
  its check to its commit point** ... For a verb or a turn the commit point is the enqueue
  of the frame on the connection's ordered stream, after the first audit record is
  written", "Every write a session authorizes re-checks its authority inside its own
  transaction", and "**The hold never spans I/O with a peer**, neither the socket write
  nor any wait on the agent".
- Spec 2.13, the role and the grant: "grants no action on any agent by itself" (the
  server-wide admin role), and the seeded roles.
- Spec 8 (about line 3726): "Each verb ask names its principal, as a claim ... admin-con
  passes nothing of it to admin ... It is never an authorization input anywhere on the
  box, and section 2.13's audit is the one record of who asked."
- Design sections 9 and 10 (the exclusion, `pg_advisory_xact_lock_shared` for authorized
  acts).
- Code: `Listener::verb_for_person`, `ask_verb`, `Inner::ask` and `Inner::record_ask` in
  `src/link/listener.rs` (the record step's comment already names this PR); the answer's
  door at the `(Plane::Admin, FromClient::Verb ..)` arm, which writes the outcome from the
  `asks` entry whatever the asker does; `src/store/access.rs` (`shared_hold`, `roles_on`);
  `identity::session_stands`; `src/surfaces/reading.rs` (the page, its `Seams`);
  `src/store/admin.rs` (`authorized`, how an admin write re-checks in its transaction).

## 2. Scope

1. **The route**: `POST /agents/{ag}/verb` with a form field `verb`, behind the session
   gate and the app's `Origin` guard, on the reading surface. The listener reaches it as a
   seam of its own (`Seams` grows an asker), not through the router's state.
2. **One order** (Spec 2.13, as the admin writes take theirs):
   1. Parse: the agent's identity to its shape and `verb` against the lifecycle
      vocabulary (`VOCABULARY` without `turn`); a malformed one is 400, no record.
   2. Resolve: an agent the register does not hold is 404, no record.
   3. **Under the shared hold**, in one transaction holding
      `pg_advisory_xact_lock_shared(IDENTITY_LOCK_KEY)`:
      - the session standing (`session_stands`) and the person's live grants on this
        agent, the union of their roles' verbs. **No grant on the agent** (or a session
        no longer standing): one refusal record, answered 404 exactly as an unknown agent,
        so the page is no oracle. **A grant whose roles lack the verb**: one refusal
        record, 403. **The server-wide admin grant authorizes nothing here**: only
        per-agent grants count.
      - the live connection on the admin plane, ready, and its ceiling containing the
        verb (the check `Inner::ask` already makes, under the live map's lock). Not
        connected: one refusal record, 503. Outside the ceiling: one refusal record, 403
        naming the ceiling.
      - **the first record** (principal the person by `pe-` identity, method `session`,
        target the agent, action the verb), through the existing record step, **durable
        before the frame**.
      - **the enqueue**, on the incarnation the ceiling was read from.
      - release: the hold's transaction commits.
   4. The outcome comes at the answer, written at the door as today.
   A refusal the audit cannot record is the server's failure (as the admin writes answer
   it), and nothing is asked.
3. **No wait on a peer under the hold.** Today `Inner::ask` waits on `tx.send` to the
   silence bound when the channel is full; under the hold that would let one backlogged
   connector stall every identity write. **Reserve the channel slot before the hold**
   (`Sender::reserve`, held to the silence bound with today's silent-peer close), and send
   through the permit inside it; a refused ask drops its permit unused. **The reservation
   decides nothing**: no connection to reserve on goes into the hold all the same, where
   the grant check runs first and only then the refusal as not connected, so a person
   with no grant still meets 404 and never learns the agent's presence. Store I/O under
   the hold (the checks, the first record) is not peer I/O and is allowed. If you find a
   cleaner shape that keeps every peer wait outside the hold, say so in the PR body and
   take it.
4. **The section from the hold to the enqueue is cancellation-proof.** An HTTP handler's
   future is dropped when the client disconnects. **Run the ask, from the reservation
   through the enqueue and its refusal records, in a task the handler cannot cancel**,
   the handler awaiting its result. A dropped request then either never started the
   task or leaves it to finish: authorized and enqueued with its first, or refused with
   its record. The `Unsent` guard still answers a first `failed` wherever the task itself
   ends between the record and the channel.
5. **The answer to the person**: the handler waits for the agent's answer up to the
   silence bound and renders it: admin's answer (exit 0, its object shown escaped),
   admin's refusal (exit 1), admin-con's fault (kind and message), or the connection
   ended with the verb in flight (unknown here; the trace and the next `show` hold it).
   **Past the bound it answers 202**, naming the first record's `au-` identity and saying
   the outcome is written when admin answers. The task keeps waiting for the answer; the
   handler's wait ending changes nothing.
6. **The frame's principal**: `Principal::Person` carries the person's `pe-` identity,
   which is stable across a rename and is the audit's own name for them. If you rename
   the member, do it in both halves and in admin-con's log line. Nothing of it reaches the
   box (Spec 8, and `sudo_invoker_tests`' argv instrument stands).
7. **Lift `verb_for_person`'s `cfg(test)`**: the person's path passes the `pe-` identity
   and session into the record step as `audit::Principal::Person`, so no build can enqueue
   a person's verb frame without its first. Remove the `record: None` path; every verb
   frame now has a record.
8. **The agent page** (`GET /agents/{ag}`): one plain form per verb the person's roles
   carry on this agent and the live connection's ceiling grants. Hiding is presentation;
   the POST checks everything again. No confirmation step, no script.
9. **Out of this PR**: turns (6c), any audit read surface, register verbs through the
   server (they stand), #38, #35.

## 3. Review checklist (distributed state)

Pre-review against these before you tell me the draft is up. Each one has bitten this act.
- **The hold, from the check to the enqueue**: the grant, the session and the ceiling are
  read under it, the first record is durable before the frame, and the frame is in the
  channel before the hold's transaction commits. A revocation either commits before the
  check or waits until the enqueue.
- **No peer I/O under the hold**: no `send`, `reserve` or answer wait between taking the
  hold and committing it.
- **Cancellation**: list every `.await` between the reservation and the enqueue, and
  say for each what a drop there leaves: no frame without a first, no first without a
  frame or a `failed`, no refusal unrecorded.
- **Every refusal is one record and no more**; no refusal path also writes a first.
- **Only per-agent grants authorize a verb**; a CHECK or predicate passes on NULL
  nowhere new (an agent-less grant has `agent_id IS NULL`).
- **Deadlines are instants read at their await**; the answer's wait is the handler's
  alone and bounds nothing in the task.

## 4. Tests and perturbations

In a test module of their own (`src/surfaces/verbs_tests.rs` or similar), against the
real listener and a fake admin-con, each test on a `fresh_store`:
- A person holding `operator` asks `load`: one first record (the person, `session`, the
  agent, `load`), the frame reaches admin-con carrying the person's `pe-`, and admin's
  answer writes `ok` naming it; the page shows the answer.
- Holding `observer` (show only) and asking `stop`: 403, one refusal record, no frame.
- No grant on the agent: 404 indistinguishable from an unknown agent (same status, same
  body), one refusal record; an unknown agent: 404, no record.
- The server-wide admin grant alone, asking `show`: 404 and a refusal record, no frame.
- A person disabled after the gate read their session and before the check (a lever):
  refused under the hold, a record, no frame. (A session already ended at the gate is the
  gate's, as on every page.)
- A verb outside the ceiling: 403, a refusal record, no frame. Not connected: 503, a
  refusal record.
- A malformed verb or identity: 400, no record.
- The answer past the bound: 202 naming the first, and the outcome lands later.
- **The hold**: stall the task between the check and the enqueue (a test lever), revoke
  the grant from another task, and the revocation does not commit until the frame is in
  the channel; then the next ask is refused.
- **No peer wait under the hold**: a fake admin-con that stops reading fills the channel,
  an ask waits for its reservation, and a `grant remove` meanwhile commits within a bound.
- **Cancellation**: drop the request at a stall inside the section; the frame still
  reaches admin-con with its first, or (the stall before the check) the ask is refused
  with its record; never a frame without a first, never a first left alone.

Perturbations, each shown to fail with its guard removed, in a fresh scratch worktree of
the pushed commit:
- drop the grant check, and an observer's `stop` leaves the server;
- drop the session re-check under the hold, disable the person between the gate and the
  check, and the ask leaves;
- let the admin grant authorize, and an admin with no grant on the agent asks `show`;
- take the check outside the hold (the hold released before the enqueue), and a
  revocation committed between them lets the ask leave on a revoked grant;
- reserve the slot under the hold, and the `grant remove` waits on the stalled connector;
- run the section in the handler's future, drop the request while a refusal's record is
  being written, and the refusal leaves no trace; drop it between the check and the
  enqueue, and an authorized ask is lost after its first (answered `failed` by `Unsent`,
  but never asked);
- drop a refusal's record, and a refused verb leaves no trace;
- answer no-grant with 403, and the page tells a stranger the agent exists.

## 5. Spec and documents

- Spec 2.13, the server-side authorization item: an "as built on 2026-10-13" sentence for
  a person's verbs (the order, the hold from the check to the enqueue with the slot
  reserved before it, the task, the statuses). No other sentence moves unless it is now
  false.
- Section 9, in one commit with the count and the summary (the consistency sweep):
  - "a verb or turn its principal may not ask is refused before an ask": the verb clauses
    move to enforced with their instruments; **the turn clause stays owed to 6c**.
  - "every verb or turn a person asks ... has an audit record": the verb-ask clauses move
    to enforced; any clause no instrument stands for stays owed, named in the PR body.
  - "the admin role grants no action on any agent": enforced.
  - Each moved clause names its instrument and file.
- CLAUDE.md: the reading paragraph gains the POST and the forms in one clause, and
  `verbs_tests` (or your name for it) joins the DB-backed list.

## 6. Acceptance

- The full suite with `DATABASE_URL` before every push; `cargo clippy --all-targets
  --locked -- -D warnings` with and without the default features; `cargo fmt --check`;
  `tests/startup.rs --ignored`. Perturbations in a fresh scratch worktree of the pushed
  commit (`git worktree prune`, a new name per run, `|| exit 1` after the add, `git log -1`
  printed inside it), each log read against its claim.
- ASCII only, absolute dates. Check visibility before the push.
- PR body: `Implements:` naming Spec 2.13 (server-side authorization, the audit record,
  the role and the grant) and Spec 8 (the ask's principal), the cancellation table of
  section 3, the perturbation table, "Refs #18".
