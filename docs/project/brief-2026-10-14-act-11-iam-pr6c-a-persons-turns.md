# Brief: act 11, IAM, PR 6c: a person's turns, placed through the server

Version: v0.1, 2026-10-14. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 6b2 (#42) merged at `248ce2c`: a person's lifecycle verb is admitted,
checked under the shared hold, recorded and enqueued, and only the server's `show` and a
person's verbs are left on the admin plane. **This PR does the same for a turn on the
gate plane, through the same mechanism, and closes act 11's PR 6.** One concern: a
person places a turn on an agent from the agent's page, authorized by the server, audited
without its text, enqueued under the shared hold. Backend first: the form is plain.

## 1. Read first

- Spec 2.13, server-side authorization: "**A turn frame leaves the server only if the
  person's grants on that agent permit `turn`**; no ceiling applies to a turn, the data
  plane's box gate being the gate's own admission per section 8, and neither principal
  that is not a person ever places one." The shared hold to the commit point, "for a verb
  or a turn ... the enqueue of the frame".
- Spec 2.13, the audit record: "for a verb asked of an agent, or a turn placed with it,
  the target is the agent and the action the verb or `turn`" and "**a turn's text is not
  in the audit record**: the agent's trace on its box is the record of what was said".
- Spec 2.13 (about line 1962): "**An agent holds one conversation** ... **The console
  says so wherever more than one person holds `turn` on an agent**, beside the grant and
  wherever a turn is placed ... where a turn is placed waits for the surface that places
  one." That surface is this PR's.
- Spec 7.1 (about lines 2925-2960): gate-con relays and checks nothing of the person; a
  turn that crossed runs to its end (`toddwbucy/WeaverAgent#59`), so a link loss leaves
  it unknown; `not_started` and `busy` are gate-con's own faults and never ran.
- `weaver-gate-world-contract` sections 2 and 3 (in WeaverAgent's tree): the close names
  its kind (`answered`, `stopped`, `refused`), `finish: "length"` on a truncated answer,
  `reason` in place of `text` on a stopped or refused close, and a close that answers no
  turn names none. Section 2's seeding line (`role: "system"`) is admitted from the
  operator's uid alone.
- Code: `Listener::ask_as_person`, `Inner::ask_held`, `reserve`, `ceiling_refusal`,
  `Admission`, `PERSON_SLOTS` and the deadline (6b2); `Listener::turn` and the
  `(Plane::Gate, FromClient::Turn ..)` arm, which today resolves the asker and checks
  nothing; `adapters::gate::{request_line, LINE_BOUND, GateClose}`; `link::gate_con`'s
  faults; `Store::shared_turn` in `src/store/grants.rs`.

## 2. Scope

1. **One mechanism for both planes.** Generalize 6b2's held ask over what is asked (a
   verb on the admin plane, a turn on the gate plane) rather than copying it: the
   admission, the per-connection slots (`PERSON_SLOTS` on either plane), the place among
   `HELD_ASKS`, the deadline, the hold, the refusal records, the first record, the
   enqueue through the permit, the commit. What differs is only the grant's verb (`turn`),
   the ceiling (none on the gate plane: skip it, do not fake one), the plane, and the frame.
2. **The route**: `POST /agents/{ag}/turn` with a form field `text`. The same order as
   the verb's:
   1. Parse: the agent's identity to its shape; the text not empty after trimming, and
      **its request line within the gate's bound** (`request_line(text)`, so
      `line_too_long` is refused here as the ask's fault and never reaches gate-con): 400,
      no record.
   2. The session, then **the admission** (shed: 503, no record), then resolve (unknown:
      404, no record).
   3. The held section, exactly as a verb's, with the grant check for `turn`: no grant on
      the agent 404 as unknown with one record; a grant without `turn` 403 with one
      record; no gate-con connected 503 with one record; Busy and Stalled as 6b2.
   4. The first record: principal the person, method `session`, target the agent, action
      `turn`. **Never the text, nor its length, digest or any part of it.**
3. **The answer at the door** (6b1's lesson, brief it now so Codex does not find it one
   instance at a time): the gate plane's `Turn` answer is checked once, at the door,
   against the ask it answers and the shape:
   - an answer to no ask, or carrying both or neither of `close` and `error`, is
     malformed: `Refusal::Malformed`, teardown, no outcome, the asker unanswered (as the
     admin plane's);
   - **only the contract's evidence writes an outcome, by allow-lists**: `ok` for a close
     of kind `answered`; `failed` for a close of kind `stopped` or `refused` (the gate's
     own word that no answer came) and for gate-con's faults that prove the turn never
     reached the gate (`busy`, `not_started`, `unloaded`, `line_too_long`); **no
     outcome** for every other fault (`delivery_lost`, `bad_close`, `close_too_long`),
     any kind not on a list, and a link that ends with the turn in flight. Say in the PR
     body which field decides each and why `stopped` is `failed` (the contract's own
     close, evidence the turn ended without its answer, unlike a fault that cannot say).
   - The outcome is written at the door, not raced against the close signal, held to the
     record step's bound, whatever the page does (as a verb's).
4. **No record-less turn path is left.** `Listener::turn` and `Inner::ask`'s `None` arm
   (the turn's unaudited ask) go; the tests that used it (`link::client_tests`,
   `link::tests`) place turns as a person through a fixture, as 6b2 did for verbs. After
   this PR every frame the server sends on either plane, verb or turn, has a first record
   committed before it, and `Inner::ask` serves the server's own `show` alone. Grep every
   `ToClient::Turn` and `ToClient::Verb` and say so in the body.
5. **The page's wait**: to the silence bound, then 202 naming the first record, as a
   verb's. Rendered, escaped (askama's default; no `|safe`): an `answered` close's text,
   marked truncated where `finish` is `length`; a `stopped` or `refused` close's reason;
   gate-con's fault kind and message; or the connection ended with the turn in flight
   (unknown here; the agent's trace holds it).
6. **The text goes nowhere but the frame.** Not in the audit, not in a `tracing` line at
   any level, not in an error's `Display`, not in a refusal's reason. The request line
   stays text-only, so no `role` member can cross from the server (the gate would refuse
   a seeding line from gate-con's user anyway; this keeps it from being asked).
7. **The shared-conversation note where a turn is placed**: the agent page's turn form
   says, where more than one enabled person holds a role carrying `turn` on the agent,
   that their turns interleave in the agent's one conversation. Reuse the grants page's
   query; one definition, not a second one.
8. **The agent page**: the turn form appears where the person's grants carry `turn` and
   gate-con is connected. Hiding is presentation; the POST checks everything again.
9. **Out of this PR**: a conversation view or history (the trace is the record), #38,
   #35, any audit read surface.

## 3. Review checklist (distributed state)

Pre-review against these before you tell me the draft is up; each one bit 6b.
- **The hold, from the check to the enqueue**, the first record durable inside it, no
  peer I/O under it, the slot reserved before it and decided nothing by it.
- **Admit before detaching**: no turn's task spawns without an `Admission`, taken with
  `try_acquire` before anything is resolved.
- **A reserved slot is the peer's capacity**: no permit held across a non-peer wait
  beyond the deadline; the per-connection cap holds on the gate plane too.
- **Nothing waits unboundedly on the store**: one deadline, an `Instant` read at each
  await, from the place to the commit.
- **A peer's answer is checked once, at the door**: shape and ask; only allow-listed
  evidence writes an outcome; nothing past the door sees an unchecked answer.
- **Cancellation table** in the body: every `.await` from the admission to the enqueue
  and what a drop there leaves.
- **The text**: grep the diff for every place the text is bound to a name and show where
  each one goes.

## 4. Tests and perturbations

In `src/verbs_tests.rs` (or a sibling for turns), the real listener and a fake gate-con:
- A person holding a role with `turn` places one: one first record (person, `session`,
  agent, `turn`) **whose row holds no part of the text** (assert on the whole row, with a
  distinctive text), the frame reaches gate-con with the text, an `answered` close writes
  `ok` and the page shows the answer; `finish: length` shows truncated.
- A `stopped` close and a `refused` close write `failed`; `busy` and `not_started` write
  `failed`; `delivery_lost` and an unknown close kind write no outcome; a link that ends
  with the turn in flight writes no outcome.
- An answer with both `close` and `error`, or to no ask: malformed, teardown, no outcome.
- A person with only `observer` (no `turn`): 403, one refusal record, no frame. No grant:
  404 as unknown, one record. The admin grant alone: 404, one record.
- Text empty, or past the line bound: 400, no record, nothing reaches gate-con.
- No gate-con connected: 503 with one record.
- The shared-conversation note: shown on the page with two enabled `turn` holders, not
  with one, not where the second is disabled.
- **The text in logs**: capture `tracing` output across a placed turn, a refused one and a
  stalled one, and assert the distinctive text appears in none.
- The hold, the admission, the slots and the deadline need only one test each on the gate
  plane, showing the shared mechanism serves it.

Perturbations, each shown to fail with its guard removed, in a fresh scratch worktree of
the pushed commit:
- drop the grant check for `turn`, and an observer places a turn;
- take the check outside the hold, and a revocation between check and enqueue lets the
  turn leave;
- put the text in the first record (or its length), and the row-assertion fails;
- log the text at debug in the held section, and the capture finds it;
- write `ok` for any close, and a `stopped` turn reads `ok`; write `failed` for
  `delivery_lost`, and an unknown outcome reads `failed`;
- drop the door's shape check, and a both-members answer writes an outcome;
- restore `Listener::turn`'s unaudited path in a test, and a turn frame leaves with no
  first record (or show by grep that no such path compiles; say which).

## 5. Spec and documents

- Spec 2.13: an "as built on 2026-10-14" sentence for a person's turn (the shared
  mechanism, the `turn` grant and no ceiling, the record without the text, the door's
  allow-lists); the one-conversation paragraph's "where a turn is placed waits for the
  surface that places one" becomes the as-built note on the agent page.
- Spec 7.1: one clause that the server now sends a turn only through the person's path.
- Section 9, in one commit with the count and the summary: the turn principal-check row
  and the turn clauses of the audit row move to enforced with their instruments; any
  clause of the audit row no instrument stands for stays owed, named in the body. A new
  enforced row, or a clause, for "a turn's text is not in the audit record or the logs".
- CLAUDE.md: the reading paragraph gains the turn form and the note; the listener
  paragraph says every frame on either plane is audited.
- Epic #18: this PR closes PR 6; say so in the body.

## 6. Acceptance

- The full suite with `DATABASE_URL` before every push; clippy with and without the
  default features, `-D warnings`; `cargo fmt --check`; `tests/startup.rs --ignored`.
  Perturbations in a fresh scratch worktree of the pushed commit (`git worktree prune`, a
  new name per run, `|| exit 1` after the add, `git log -1` printed inside it), each log
  read against its claim.
- ASCII only, absolute dates. Check visibility before the push.
- PR body: `Implements:` naming Spec 2.13 (server-side authorization, the audit record,
  one conversation) and Spec 7.1, the door's outcome table, the cancellation table, the
  text's-path table, the perturbation table, "Refs #18".
