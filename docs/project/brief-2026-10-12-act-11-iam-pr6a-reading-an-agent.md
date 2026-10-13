# Brief: act 11, IAM, PR 6a: reading an agent

Version: v0.1, 2026-10-12. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 5d (#37) merged at `3087490`; the admin's register work is done. The design's
PR 6 (server-side authorization) is split in three, one concern each:

- **6a (this one): reading an agent.** What a person with a grant on an agent may see, and
  the live trace view's re-check.
- 6b: lifecycle verbs, asked by a person through the server and authorized there.
- 6c: turns, placed by a person through the server and authorized there.

The operator's backend-first ruling stands: plain functional pages, no styling. These pages
are the thin shells the authorization needs; the designed agent view comes after the
backend is done.

## 1. Read first

- The design's section 9 (read access: any grant on an agent permits reading everything the
  server holds of it; no grant, nothing, not even its name; the admin's register view
  excludes the door, the load state and the trace) and section 6 (an open live view
  re-checks its session and grant every 15 seconds and closes when either has ended).
- Spec 2.12 (the register's row: presence, ceiling, door, load state and its source, the
  run's tuple and constituents) and 2.13's read-access sentences.
- `src/link/listener.rs` (`windows()`, the live trace rings, `connected()`), the traceview
  module the listener uses, and `src/surfaces/agents.rs` (the admin's register page, which
  stays as it is).

## 2. Scope

1. **`GET /agents`**: the agents the session's person holds a grant on (any role), each by
   box and name, with presence per plane. An agent the person holds no grant on does not
   appear. A person with no grants sees an empty list, never an error that reveals there
   are agents.
2. **`GET /agents/{ag}`**: one agent, for a person holding a grant on it: the register's
   row (presence per plane, the ceiling the admin plane declared, the trace door, the load
   state and its source and date, the run's tuple and constituents where held), and the
   person's own roles on it. **A person without a grant gets the same answer as for an
   agent that does not exist** (404), so the page is no oracle for agent identities. The
   identity is parsed to its shape first (400).
3. **The live trace view**: `GET /agents/{ag}/trace`, an SSE stream of the agent's live
   window, the same rings the listener keeps. It re-checks **every 15 seconds** that the
   session still stands (4a's checks) and the person still holds a grant on the agent, and
   closes at the first check that fails; the twelve-hour session limit closes it too. Its
   last-use refresh follows the once-a-minute rule. The view is the live window and says
   so: not a record, per the operator's ruling of 2026-10-01.
4. **The check is the server's**, at every read, under the identity exclusion shared for
   the read's own check (the same shared hold an ask takes, released before any I/O with a
   peer). No read is audited (reads ask nothing of an agent).
5. **Navigation**: Agents for every signed-in person (beside Record and Passkeys); the
   admin's register page stays under Admin.
6. **Out of this PR**: asking a verb (6b), placing a turn (6c), issue #38 (show's
   in_transition constituents, a small follow-up after 6c).

## 3. Tests and perturbations

- A person with an observer grant on one agent and none on another: the list shows one;
  the page of the other answers 404 identical to an unknown agent; its trace stream too.
- The page shows the row's facts for a granted agent; an admin with no grant on an agent
  sees it on the register page and not here.
- The live view: events relayed into the listener's window reach the stream; revoking the
  person's grant (by the host) closes the stream within the re-check interval (a test
  interval, set small); disabling the person and closing the session each close it too.
- A malformed `ag-` answers 400.
- Perturbations for each guard (the list's grant filter, the page's grant check, the
  stream's re-check, the identical-404), in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.13's read access as built; section 9's read-access row (owed) and the live view's
  re-check row (owed) move to enforced; count and summary swept.
- CLAUDE.md: the agents pages and the live view.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in a fresh scratch worktree of the pushed
  commit (`git worktree prune`, a new name per run, `|| exit 1` after the add, `git log -1`
  printed inside it).
- ASCII only, absolute dates. Check visibility before the push.
- PR body: `Implements:` naming design sections 6 and 9 and Spec 2.12 and 2.13, the
  perturbation table, "Refs #18".
