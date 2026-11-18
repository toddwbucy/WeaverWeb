# Brief: the hello's admission and the admin writes

**Date:** 2026-11-18. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** issues #151 and #150, both filed from #146's review, and the Planner's
word of 2026-11-18 to take them as one act of two sub-acts from `main` at `00545dd4`.
The act runs before the stack that moves the model to the appliance, since #151 and
that stack's console sub-act both change `src/link/listener.rs`.

## Goal

Two shapes that #146 hardened on its own paths and left on older ones, each its own
sub-act so each closes and reverts on its own. No migration and no new surface.

## The sub-acts, in order

1. **#151, the ordinary hello's admission.** `serve_connection` awaits `Store::admit`
   with no bound, and the admission's install puts the live entry in before its
   commit. A stalled store holds the hello past the connector's `HELLO_SECS`, and a
   commit that lands late leaves an entry that refuses the connector's next hello
   `already_connected`.
   - The admission is bounded as a committing write (`deadline::before`, never polled
     past its deadline), at a deadline made at the connection's accept and under the
     connector's `HELLO_SECS`, which spans its dial, its handshake, its hello and the
     answer.
   - Past the deadline the hello is refused `store_unavailable`. An entry the install
     put in comes out, and the incarnation-bound teardown is deferred to the
     reconciliation, since a commit may have landed.
   - Spec section 8 states the admission's bound beside the hello's, and section 9
     gains the row.
   - **Shown running before it leaves draft:** a hello whose admission stalls, driven
     through the real listener and store, refused within the connector's bound, with no
     live entry left and the next hello admitted.
2. **#150, the admin writes.** Twelve handlers in `src/surfaces/admin.rs`,
   `grants.rs` and `agents.rs` run their first record, write and outcome in the
   request's future, so a dropped request can leave a first record with no outcome
   and lose what the answer carries.
   - Each handler's continuation, from `first_of` through `settled`, and for register
     and rotate the hand-over into `agents::Handover`, moves into one spawned task the
     request awaits, as #146 did for the pairing entry (`continue_entry`). The session
     and the parsed ask move into the task as owned values.
   - The refusal records the admin gate writes before a write stay in the request's
     future. A dropped request there loses one refusal record and no act.
   - Spec 2.13 says that an admin write runs from its first record to its outcome in a
     task the request cannot cancel, as it says of a person's ask, and section 9 gains
     the row.
   - **Shown running before it leaves draft:** a request dropped at the hold
     (`READ_HOLD`), its write still landing with its outcome recorded, one test per
     file, and for register and rotate the configs held for the session.

## Ruled out

- No change to the setup ask's admission or deadline, which #146 built.
- No change to the audit's shape, the admin writes' order or their refusals.
- No new refusal word on the link.

## Gates

The repository's own: `fmt`, `build`, both `clippy` runs and the suite with
`DATABASE_URL` and `PGUSER` set at eight threads, `--locked` throughout, on each
sub-act's head and on the stack's head, each worktree with a target of its own. A
perturbation per new test, restored from a scratchpad copy checked by `cmp`. The suite
workflow's path: draft sub-PRs into the stack, the Planner's grade and Codex per sub-PR,
CodeRabbit and Codex on the stack, the operator's merge to `main`. Each sub-act's body
closes its issue.
