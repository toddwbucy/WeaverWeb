# Brief: `show` in transition, on the register (#38, #46 item 2)

Version: v0.1, 2026-10-16. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. Act 12 merged at `ff95f99`. One small concern: the register and the agent page
read a `show` that answers `in_transition`, with or without its constituents, for what
it is. **#46 item 1** (admin-con's unit with `Delegate=yes`) is the install act's and
stays on hold until WeaverAgent #166 is ruled. This PR does not touch it.

## 1. What WeaverAgent says (read the contract, don't infer)

- **#38 (WeaverAgent B3).** `in_transition` gains an optional `constituents`, the run's
  process ids, serialized only when non-empty:
  - `{"kind":"in_transition"}`: another invocation holds the agent (the ladder's rung
    0), or the harness answers it;
  - `{"kind":"in_transition","constituents":[...]}`: a run holds the run lock and no
    worker answers `Idle` or `Active` (a run never entered, one ending, or a leave
    pending).

  `show` no longer answers `State{Unloaded}` with constituents.
- **#46 item 2 (WeaverAgent #161).** With the run lock free but an ended run's processes
  still in the agent's cgroup, `show` also answers `in_transition` with those
  `constituents`. `Unloaded` is printed only once no process of the agent stands.
- **`in_transition` claims no state.** `admin_con::drain`'s reasoning already rests on
  that, and so does Spec 7.2 (about line 3713).

The two `in_transition` answers with constituents cannot be told apart on the wire, so
the page never claims which one it is.

## 2. Read first

- Spec 2.12: the row's members (the load state and its source, the tuple and its
  source, the constituents with their date, migration `0013`).
- Spec 7.2 and 8, where `show` lands.
- `weaver-admin-operator-contract` section 5 and `weaver-types-Spec` section 3.1, at
  WeaverAgent's current main.
- Code:
  - `Inner::land_verb` and `constituents_of` in `src/link/listener.rs`. Today only
    `kind: state` lands, so `in_transition` lands nothing.
  - `register.rs`: `Observation`, `ConstituentsWrite`, the landing statement.
  - `land_event`, where a live load or unload writes the row.
  - `surfaces/reading.rs`: the agent page's load state and constituents.

## 3. Scope

1. **A `show` answering `in_transition` is an observation that claims no state.**
   - The row's load state, its source, the tuple and its source are **kept**.
   - **The transition is recorded beside them**: when it was seen (the receipt's date,
     as for every `show`) and its constituents, written from the answer by the same
     reading as `State`'s (`constituents_of`), or none where it names none.
   - Read by allow-list: exit 0, `kind: in_transition`, and an optional `constituents`
     array of pids. A member the parser does not know is ignored, as today. Nothing
     else lands.
2. **An observation that writes a state ends the transition**: a `show` answering
   `state`, and a live load or unload event that writes the row.
   - Say in the PR body which observations end it, and why none can leave a transition
     standing over a newer state. The arrival sequence that orders landings already
     orders this.
3. **The store**: one new migration for the transition's date (the constituents column
   is reused). It is squashed at install like the rest, per the operator's ruling of
   2026-10-08.
4. **The agent page** shows the last state as today, and beside it, where a transition
   stands, one of:
   - **"In transition since T: another command is in progress"** (no constituents);
   - **"In transition since T: processes P still stand: a run starting or ending, or an
     ended run's processes still being ended"** (with constituents).

   **Never "loading"**, and nothing that picks one reading of the constituents.
   `GET /agents` shows the transition in its row too. No alert: a transition is state,
   not a failure. A stuck one is the operator's to judge from the page and the trace.
5. **admin-con**: confirm, and state in the body, that it passes `show`'s object
   verbatim and parses no member of it. The opening's `show` and `drain` already treat
   `in_transition` as claiming no state. No code change is expected; say so if one is.
6. **Out of this PR**: #46 item 1 (on hold), the trace header's `length` (its own PR),
   any alert for a transition.

## 4. Tests and perturbations

Against the real listener and a fake admin-con, each on a `fresh_store`:
- an agent at `Idle`, then `show` answers `{"kind":"in_transition"}`: the row stays
  `Idle` from `show` with its tuple, and a transition stands with no constituents. The
  page says another command is in progress;
- then `{"kind":"in_transition","constituents":[4100,4101]}`: the transition's
  constituents are 4100 and 4101, and the page names them without claiming which kind
  of transition it is;
- then a `show` answering `state` (`Unloaded`): the transition ends, the state is
  `Unloaded`, and the constituents read none;
- a live unload event during a transition ends it;
- an admission whose `show` answers `in_transition` completes, and the row records the
  transition;
- a refusal (exit 1) whose object says `in_transition` lands nothing.

Perturbations, each shown to fail with its guard removed, in a fresh scratch worktree of
the pushed commit:
- write the load state from an `in_transition` answer, and the row reads a state admin
  never claimed;
- let a state answer leave the transition standing, and the page shows a transition
  over a newer state;
- drop the constituents from the transition, and the leftovers' pids vanish from the
  page.

## 5. Spec and documents

- Spec 2.12: the transition member beside the load state ("claims no state; kept beside
  the last state; ended by the next observation that writes a state").
- Spec 7.2: `show`'s `in_transition` with its optional constituents, and both meanings
  per WeaverAgent #161.
- Section 9: a row for "an `in_transition` claims no state", with the count and summary
  swept in one commit.
- CLAUDE.md, if its register paragraph names the members.
- **Closes #38.** On #46, a comment that item 2 is done here and item 1 waits on
  WeaverAgent #166 (I'll post it after the merge).

## 6. Acceptance

- The full suite with `DATABASE_URL` before every push, on a scratch database fresh for
  the new migration; clippy with and without the default features, `-D warnings`;
  `cargo fmt --check`; `tests/startup.rs --ignored`. Perturbations in a fresh scratch
  worktree of the pushed commit, each log read against its claim.
- ASCII only, absolute dates; no box facts in the repository. Check visibility before
  the push.
- PR body: `Implements:` naming Spec 2.12 and 7.2; the table of `show`'s answers and
  what each writes; which observations end a transition; the perturbation table;
  "Closes #38", "Refs #46".
