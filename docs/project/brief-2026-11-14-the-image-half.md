# Brief: the image half of the install (2026-11-14)

From the Planner, 2026-11-14, after the Executor's read-only answer on
staging an appliance image. One small act from `main`, built locally, with
no push until the Planner has read its diff. It is queued after the
Declare pane's other-rows sentence.

## The rulings it rests on

- **The operator's written rulings of 2026-11-14:** the appliance is a
  locked-down image, and nothing of an agent rides it.
- **The operator's word of 2026-11-14:** the image's binaries are compiled
  inside it from a named commit, so the commit tells what code was
  compiled in that machine.

## What was measured (read only, at `962d0166`)

- `weaver-web-stage` refuses `[connectors]` with no `[[agent]]`
  (`src/stage.rs`, "[connectors] is given and no [[agent]] is"), and
  `connectors.handover` is required. So the tool cannot stage an image.
- The tool is its own binary and builds with `--no-default-features`. No
  ssl or crypto library is linked, and the graph holds no `openssl-sys`.
  An image build therefore never compiles the server in order to stage.
- With zero agents, the connectors' branch of the plan already lays
  exactly the image's half: the two programs, the two templates rendered,
  and the directories.

## The act

- `[connectors]` with no `handover` and no `[[agent]]` stages the two
  programs, the two templates rendered with the image's accounts and
  locations, and the directories `binaries`, `units`, `config_root` and
  `<config root>/connectors`. Nothing of an agent is staged: no handover,
  config, drop-in or instance.
- A handover with no agent is refused, so none rides an image.
- An agent with no handover is refused. The full path, each agent with its
  handover, stays the single-host install's.
- Tests: the exact manifest of an image, and both refusals, with
  perturbations.
- Spec 8.1 states it, and section 9 gains its row.

## Scope

- `src/stage.rs`, `src/stage_tests.rs`, Spec 8.1 and section 9.
- **The names stay out of this repository** as they do today: the image's
  accounts and locations are parameters with no default. The appliance's
  own specification, in the suite repository, fixes them.
- No enrollment is designed here. How an agent's configuration and
  instances reach an image later is not part of this act.
