# Brief: the alignment with WeaverAgent's A3.2 (one small pull request)

Version: v0.1, 2026-10-08. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. `toddwbucy/WeaverAgent#94` (A3.2, admin's save-point act) merged on 2026-10-08
at `43ba391`. It changed the management plane's door this repository builds against. This
PR brings WeaverWeb in step before act 11 goes on to passkeys. It is narrow: each change
below is named, and nothing else moves.

## 1. What WeaverAgent changed (read `weaver-admin-operator-contract` at `43ba391`)

1. **Three new verbs**, each a fixed command line with no argument, granted by the box's
   operator rule: `save-point` (takes a save point of the running agent and publishes it),
   `restore` (names the save point the next load restores, the one the declaration names,
   never the caller's), and `force-unload` (the unload that completes without its leave
   save point and records the loss as `ForcedUnload`). The observer's rule grants none.
2. **`unload` can now refuse** `ActivityNotAtRest` while a turn runs or gate traffic stands
   (the interim until WeaverAgent's lifecycle-protocol act, which will drain instead), and
   `SavePointNotTaken` where its leave save point did not publish (the run has ended; the
   next load records the loss).
3. **The gate**: a request admitted just before an unload is recorded on the trace as
   refused and its connection closes unanswered. gate-con already answers such a turn as
   an unknown outcome (act 7), so no code moves.
4. **The trace** gains `save_point`, `unload`'s `forced` and two reset reasons. The
   listener maps only `load`, `unload` and the turn events, so no code moves.

## 2. The operator's rulings of 2026-10-08

1. **WeaverWeb's seeded `operator` role carries all three new verbs**, mirroring the box's
   operator rule. An admin can still narrow it with `role set`.
2. **The orderly stop retries `unload` until rest**: while `unload` answers
   `ActivityNotAtRest`, admin-con retries it until the stop's grace runs out. If the grace
   ends first, the agent is left to the containment and the next load records
   `NoCleanUnload`. No state is thrown away by choice: the orderly stop never runs
   `force-unload`.

## 3. Changes

1. **admin-con's vocabulary**: `sudo_invoker::VERBS` gains the three lines, so the hello's
   ceiling (`sudo -n -l` on each exact line) reports them where the box grants them. Check
   every other place that names the verb vocabulary (the frames, the listener's ceiling
   handling, the server's verb type) and widen each, so a ceiling naming a new verb is
   carried, not refused or dropped. **The server asks none of the three**: no surface
   reaches a verb until act 11's PR 6.
2. **Migration `0016`**: the role-verb CHECK admits the three (still null-safe); the
   `operator` role gains them **only where its verbs are still exactly 0015's seed**,
   bumping its version; where an admin has changed it, it is left as it stands and the
   migration says so in its comment. The `admin` role is untouched.
3. **The orderly stop** (`admin_con.rs`, `orderly_stop`): read the refusal's kind from
   `unload`'s answer; on `ActivityNotAtRest`, wait a short interval (state the figure and
   why) and run `unload` again, until it answers otherwise or the stop's deadline passes;
   every other answer is logged as today. One invocation at a time still holds (the slot).
   `SavePointNotTaken` is not retried: the run has ended, and the loss is recorded by the
   next load, per the contract.
4. **Documents**:
   - Spec 7.2: the verbs are eight command lines (the three described as the contract
     does), the "owed" sentence removed, `unload`'s two refusals named; Spec 8's orderly
     stop says it retries on `ActivityNotAtRest` until the grace and never forces.
   - Spec 2.13 and the design note's section 1: the role vocabulary and the `operator`
     seed with the three (the design is a record, but it states the seed, and two copies
     of one fact must agree).
   - Spec 7.1: one sentence that a turn admitted just before an unload closes unanswered
     and is answered as unknown, per the gate contract at `43ba391`.
   - CLAUDE.md where it lists the verbs.

## 4. Tests and perturbations

In the scratch worktree of the pushed commit:
- the ceiling reports a new verb the fake `sudo` grants, and omits one it does not;
- a hello whose ceiling names `force-unload` is carried by the listener (not refused);
- migration `0016`: an untouched `operator` gains the three, an edited one is left as it
  stands; `role set` accepts the three and still refuses a word outside the vocabulary;
- the orderly stop: a fake invoker answering `ActivityNotAtRest` twice and then a clean
  unload sees three invocations, one at a time; one answering `ActivityNotAtRest` past the
  deadline stops retrying at the deadline; `SavePointNotTaken` is run once. Perturbations:
  drop the retry and the first test sees one invocation; drop the deadline check and the
  second runs past it.

## 5. Acceptance

- The full suite with `DATABASE_URL` before the push, `cargo clippy --all-targets --locked
  -- -D warnings`, `cargo fmt --check`; perturbations in the scratch worktree.
- ASCII only, absolute dates; no box path, group or sudoers text (the contract's own
  examples stay in WeaverAgent). Check visibility before the push.
- PR body: `Implements:` naming the Spec sections, the two rulings, "Refs #18", and a line
  citing `toddwbucy/WeaverAgent#94` at `43ba391`.
