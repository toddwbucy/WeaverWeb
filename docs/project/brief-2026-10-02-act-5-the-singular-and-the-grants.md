# Brief: act 5, the singular and the grants (documents)

Version: v0.1, 2026-10-02. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. A small `docs:` act, no code, that brings WeaverWeb's documents in line
with three things WeaverAgent settled on 2026-10-02, after PR #7 merged. admin-con is
act 6 and is built against the text this act lands; IAM becomes act 7. Keep the act
**mechanical and narrow**: each change below is named, and nothing else in the Spec
moves.

## 1. What WeaverAgent settled, 2026-10-02

Read the operator's ruling on `toddwbucy/WeaverTools#6` (two comments of 2026-10-02) and
the adjustment comment on `toddwbucy/WeaverAgent#50`. In short:

1. **The repository is one agent and is renamed `WeaverAgent`** (GitHub renamed; the old
   name redirects). The apex document is `weaver-agent-PRD` (WeaverAgent PR #53 merged),
   and `docs/technical/weaver-agents/` became `docs/technical/weaver-agent/`.
2. **`list` is retired** (WeaverAgent #45, merged): weaver-admin is one agent's organ, and
   enumerating agents is WeaverWeb's register. The verbs are `load`, `unload`,
   `validate`, `stop` and `show`.
3. **The ceiling comes from a `grants` ask** (added to WeaverAgent #50): read-only,
   permitted to any role holder, answering which verbs the caller's role permits on this
   agent. admin-con declares exactly that answer as its ceiling, so the box's role map is
   the single source. The proposed observer role becomes `show` plus `grants`.

## 2. Changes

### The rename sweep

- Replace `WeaverAgents` with `WeaverAgent` and `weaver-agents-PRD` with
  `weaver-agent-PRD` in the living documents: the Spec, the PRD, `CLAUDE.md`, `README.md`
  and any doc comment in `src/`. Cross-repository links `toddwbucy/WeaverAgents#NN`
  become `toddwbucy/WeaverAgent#NN`.
- Dated records stand as written: the handoffs, the briefs (this one included), the
  inventory and the carried issues. GitHub's redirect keeps their links working.
- A path to the local checkout, where one appears in a living document, follows the
  operator's local directory rename; say "the WeaverAgent checkout" rather than a path.
- End with a whitespace-normalized sweep for both retired wordings, as the ruling asks,
  and list in the PR body what remains and why (dated records only).

### `list` retired

- Spec 7.2: the verbs are five, `load`, `unload`, `validate`, `stop` and `show`, per
  WeaverAgent #45. Remove the paragraph on a `list` answer landing only on the
  connection's own row, and every other clause that exists only because of `list`. Name
  the reason once: enumeration is WeaverWeb's register.
- Spec 2.12 and section 3: the tuple and the load state are written by `show` answers and
  live events (no `list`).
- Spec 2.13: the server principal may ask `show` and `grants`, the observation asks, and
  never a lifecycle verb or a turn.
- Spec 9: remove the tuple row's `list` clause.
- Spec 10, the role vocabulary proposal: the observer role is `show` plus `grants`; the
  converser and operator add to that.

### The ceiling from `grants`

- Spec 8, the ceiling paragraph: admin-con asks admin's `grants` and declares exactly its
  answer in the hello; it keeps no list of its own. Replace "read from its own config,
  which the box owns and the install writes" with that. Until WeaverAgent #50 lands there
  is no `grants` ask and no verb runs, so admin-con declares an empty ceiling: the server
  asks nothing, and admission completes at `caught_up` per section 7.2. State that the
  empty ceiling is the honest declaration, not a placeholder: it is what the box grants
  today.
- Spec 2.12's ceiling member and 2.13's authorization item: wording only, if either says
  where the ceiling comes from.
- `grants` is an ask admin-con makes of admin for itself, not a verb the server asks
  through the link; say so, so nobody adds it to the frame vocabulary as a verb.

## 3. Acceptance

The sweep finds neither retired wording in a living document. No `list` verb survives in
the Spec except in the one sentence saying it was retired and why. The ceiling has one
source. ASCII only, absolute dates, superseded text removed, no box path. PR body with
`Implements:` lines and the sweep's residue named. Check visibility before pushing.

## 4. What follows

- **Act 6, admin-con**: the trace tailer, the replay and `caught_up`, the verb plane
  behind an abstract invoker whose only implementation in the act declares an empty
  ceiling and runs nothing, and the tests. Briefed when this merges.
- **Act 7, IAM**, waits on the operator's two elections.
