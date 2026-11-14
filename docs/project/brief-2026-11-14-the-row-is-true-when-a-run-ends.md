# Brief: the row is true when a run ends

Written by the WeaverWeb Executor on 2026-11-14, from the suite Planner's word of that day. Stack `act/the-row-is-true-when-a-run-ends`, built on `act/the-answer-in-place-1-pane` at `25aec54d72914013ad4ad3140ada24a9bbf59862`, since both touch the agent page, until that act is on `main`, then on `main`.

## Why

On 2026-11-14 the Planner read the register row of an agent that had been unloaded from the agent page the evening before. The audit holds the person's `unload`, asked and answered `ok`, and the agent stayed unloaded. The row still said `idle`, by a trace event. That event was the close of the unload's wind-down turn, 0.5 s before the trace's `unload` event. The trace door closed right after it. So the agent page told the operator the agent was idle, and warned on `load`, while the agent was unloaded.

## Measured, read only (at `main` `164980ee`)

- **A live trace `unload` event writes `unloaded`** (`land_event`): the tuple cleared, the source `event`, any transition ended. The store orders writes by arrival, not by date (`land_observation`), so the event would have replaced the wind-down's `idle`. The event was not received, and the code gives no other way to lose it:
  - An event before `caught_up` is replayed and writes nothing, but the turn's close landed as live.
  - A failed write closes the connection, and the reconnect's admission would have asked `show`. No `show` ran.
- **admin-con does not read the door while an invocation is in flight.** After the verb's answer it reads the relay's stream in order. Each read sends what it read before any closing.
  - The closing comes only after the stream has ended, when a redial finds no relay. It is dated by admin-con's own clock.
  - So the server sees the `unload` event before the close only where the relay put it on its stream before ending.
  - admin-con reads the trace only through the relay. A record the relay never sent is not read in that run. The next opening's replay lies behind its boundary and writes no row.
- **A door's close writes the door's state and date alone** (`land_door`). `show` is asked only at an admission and at each opening of the door.
- **A verb's answer writes the row only for `show`** (`land_verb`). The `unload`'s exit 0 writes nothing.
- **After a `load` the row is already true.** The new run opens the door, and the opening asks `show`. The trace's `load` event lies behind the opening's boundary.
- **A crashed run writes no `unload` event at all.** Its row keeps its last turn's state for good, so it is the same class of bug.
- **The page shows the age and source, but nothing joins them.** The load state reads "idle, by event (since 2026-11-13 22:20:40 UTC)" and the trace door "closed (since 2026-11-13 22:20:40 UTC)". `load` is warned `AgentRunning`. Both are dated to the second, so the order of the two cannot be read.

## What was ruled (the Planner, 2026-11-14)

1. **The server asks `show` at a door's close** where the ceiling grants it. It is the server's own ask, audited as the opening's is, and it holds nothing. Only `show` and a live event write the state, as Spec 2.12 has it. The `unload` verb's answer still writes nothing.
2. **A bounded re-ask** while the door stays closed and `show` answers `in_transition`: three tries, five seconds apart. After that the row keeps the transition with its date, and nothing asks again until the next admission or opening. The bound is stated in the Spec.
3. **The page's line** where the state's source is an event and the door closed at or after it. Both are dated to the millisecond, with the sentence that no event has moved the state since and that `show` reads it. The line goes on the load state, the load's warning and the constituents.
4. **Tests against the fake relay and the fake invoker:**
   - a run that ends with its `unload` event relayed;
   - one that ends without it;
   - a crash with no `unload` event;
   - a relay hiccup's close followed by a reopening;
   - `show` answering `in_transition` once, twice, and past the bound.

   Each guard gets a perturbation.
5. **The pull request body says:**
   - this covers a run ended from the box's shell and a crashed run, which no fix on the verb's side would;
   - the one thing it leans on: admin-con serves the `show` right after the close.

## Ruled out

- Taking a verb's exit-0 answer as a state, and a `show` after every exit-0 verb. Both miss a run ended from the box's shell and a crash, and the first moves Spec 2.12's rule.
- Whether the relay drains to the file's end before it ends with the run. The Planner asks the WeaverAgent seat. If it does not, that is an issue for that repository, and this act does not wait on it.
- No migration. No push until the Planner's word. No restart of the running console.

## The sub-act

1. **`act/the-row-is-true-when-a-run-ends-1-close`**, the row is true when a run ends:
   - the listener's `show` at a door's close and its bounded re-ask;
   - the agent page's line on a state by event under a closed door;
   - Spec 2.12, 7.2 and section 9.

## Acceptance

- Each ruled behaviour is held by a test, and every guard fails its test when the guard is removed.
- Each Spec section named under `Implements:` has a verdict.
- The gates pass:
  - `cargo fmt --all -- --check`;
  - `cargo build --locked`;
  - `cargo clippy --all-targets --locked -- -D warnings`, with and without `--no-default-features`;
  - `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
