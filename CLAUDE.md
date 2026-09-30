# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this repository is

**Weaver-Web is a new repository: the frontend for the entire WeaverTools suite.** WeaverTools
(the agent framework, a separate repository) stays on the backend: state management, the
trace-content classifier, rerankers, and the agent's organs. Everything a human sees of the
suite is built here.

The tree you are in was pulled out of the WeaverTools monorepo as a starting point (subtree
split #689, destination commit `f2f01d8`) and pushed here as a single fresh root commit
without `deploy/`. **It is a seed, not an authority.** The split's 55 commits carry the
reasoning behind each election. They are kept locally on the `seed-history` branch, which is
**never pushed**, and in WeaverTools' own history. This repository owns its own charter,
stack, editorial rules, and review process. Nothing in WeaverTools' process documents binds it. The founding document
is `docs/project/HANDOFF-2026-09-30-weaver-web-session.md`. Read it before structural work.

### The big picture (operator's whiteboard, 2026-09-30)

```text
                 WeaverTools (the agent)
                  /                 \
               gate                admin <-> diagnostic
                 |                   |           |
            (web-con)           (admin-con)      v
                 \                  /      weaver-analysis
                  v                v           |
                     Weaver-Web  <-------------+
```

Weaver-Web has three inputs and no others:

- **gate (the data plane)** carries the work entering an agent and the answers leaving it.
  Contract: `weaver-gate-world-contract`. Reached only through **web-con**.
- **admin (the management plane)** carries the lifecycle verbs (load / unload / validate, and
  later quiesce / resume) and agent state. Contract: `weaver-admin-operator-contract`. Reached
  only through **admin-con**.
- **weaver-analysis** reads the diagnostic record and sends finished records here: a
  per-position series (turn, ordinal, token, entropy, surprisal) and a per-generation summary.
  Contract: `weaver-analysis-web-contract`. They land in this repository's own Postgres store.

The connectors (web-con, admin-con) are network clients of the agent's two sockets. They are
not on the whiteboard. They live in WeaverTools, **and neither they nor their Specs exist
yet.** Build against stubs that answer the door contracts' shapes. All three contracts live in
WeaverTools under `docs/crates/contracts/`, not here.

The first concrete consumer is the **HeroBench view**, where a researcher watches and
interviews an agent on a long-horizon benchmark. The suggested order is to render the landed
deposits first (a trace plus its state store, as a replay), then build live views.

## Hard boundaries

- **Consumers never reach the agent.** Open no agent socket and run no agent binary, even
  during development. A connector stub is the development surface.
- **No credential, box path, or security posture enters the repository.** Credentials are
  issued server-side and dropped into a client's config file. The connectors authenticate;
  the gate and admin authorize.
- **Do not edit WeaverTools.** When a connector interface lacks something, file an issue on
  `toddwbucy/WeaverTools`, one issue per interface question. Say what was measured, what is
  asked, and which document would have to move. The olympus Planning seat answers there.
- Do not link the agent's interior crates (`weaver-harness`, `weaver-spu`, `weaver-admin`,
  `weaver-gate`, `weaver-state`).
- **The remote is `git@github.com:toddwbucy/WeaverWeb.git`.** A different repository,
  `toddwbucy/Weaver-Web` (with a hyphen), is the archived v1 from August 2026. It shares no
  commits with this tree. Never push to it.

## How we work

Two Claude Code sessions share this workspace, and the operator (Todd) closes every loop.

- **thinkpad-WeaverWeb-planner** plans the work, directs the executor, reviews its PRs, and
  handles the third-party review.
- **thinkpad-WeaverWeb-executor** implements what the planner directs. It works on a branch
  and opens a **draft** PR when the work is done.

Each unit of work goes through this loop:

1. The planner sends the executor a task brief covering scope, acceptance, and the documents
   and boundaries that apply.
2. The executor branches from `main`, implements, verifies, pushes, and opens a **draft PR**.
   Then it tells the planner.
3. The planner reviews the draft. Findings go to the executor, which fixes them on the same
   branch. Repeat until the planner clears it.
4. Once it is cleared, the planner marks the PR ready (`gh pr ready`). That triggers **Codex's**
   third-party review.
5. The planner monitors the PR for Codex's comments and verifies each one against the code. A
   real finding goes to the executor to fix. A comment that does not hold up gets a reply on
   the PR saying why.
6. When Codex's comments are resolved, the planner hands the PR to the operator. **Only the
   operator does the final review and merges.**

The executor never undrafts or merges. The planner never merges. Nobody pushes directly to
`main` after the initial seed.

## Commands

The toolchain is pinned to `nightly-2026-02-13` with rustfmt and clippy (the WeaverTools pin,
carried over). The edition is 2024.

```sh
cargo build --locked
cargo test --locked                      # DB-backed tests pass by skipping without DATABASE_URL
cargo clippy --all-targets --locked
cargo fmt
```

- **DB-backed unit tests** (`store::read`, `store::plan`, `surfaces::record`) connect to the
  database in `DATABASE_URL` and run the migrations. Without that variable they print
  `skipped:` and **pass without testing anything**. To really exercise them, set
  `DATABASE_URL=postgres:///<db>?host=/run/postgresql`.
- **Single test:** `cargo test --lib store::read::tests::<name>`. Add `-- --nocapture` to see
  skip notices.
- **Compile-fail pins** are doctests in `src/store/key.rs` and `src/store/experiment.rs`. Run
  them with `cargo test --doc`.
- **Startup acceptance** (`tests/startup.rs`) is `#[ignore]`d. It spawns the real binary and
  needs `curl` plus a **never-migrated** disposable database:
  `DATABASE_URL=... cargo test --test startup -- --ignored`.
- **Run the server:** `cargo run -- --config <config.toml>`, where the config sets `listen`,
  `link_listen`, and `database` (see `ServerConfig` in `src/config.rs`). Keep configs out of
  the repository. Logging uses `RUST_LOG`, which defaults to `weaver_web=info,sqlx=warn`.

## The seed tree: what carries forward and what leaves

The handoff's step 1 (decided 2026-09-30) splits the roughly 5.5k lines of Rust in two.

**Carries forward (about 3k lines): the store and the read path.** This is where
weaver-analysis's arrow lands.
- `src/store/` is Postgres through `sqlx` runtime queries, not compile-time macros.
  `Store::connect` applies `migrations/` at startup. sqlx checksums every applied migration, so
  editing a migration that has already run makes the server refuse to start on that database.
  `read.rs` and `plan.rs` hold the six reads. `key.rs` holds typed identities (`RunId`,
  `TurnId`, `PositionKey`, `ArmId`, `PlanId`). `experiment.rs` holds staged and registered
  experiments; a registered one is immutable by type.
- `src/surfaces/` holds the browser surfaces; Record is the only one so far. The router's
  state is `Store` alone. A surface reads the store and never writes the recorded half, since
  runs and positions land only by ingest. A surface that needs a seam takes it as its own
  argument rather than widening the router state.
- `surfaces/gate.rs` is the session gate. The `weaver_session` cookie is a bearer, stored only
  as a SHA-256 digest. It carries a claimed name and a configured role and proves nothing;
  real identity waits on the credential model the connectors bring.
- The rendering approach is server-rendered askama templates (dirs in `askama.toml`) with htmx
  and SSE vendored into the binary via `include_bytes!`. There is no node toolchain and no
  SPA, and the browser is a display engine. This is inherited. The handoff leaves the stack to
  this repository, so treat it as the current choice, not a ruling.

**Leaves (about 2.2k lines): everything that reaches the agent directly.** This is the
`weaver-web-connector` binary, `wire.rs`, `lifecycle.rs` (`sudo weaver-admin`),
`adapters/gate.rs` (dials the gate socket), the trace tailers in `traceview.rs`, and `web/`
(the legacy `/admin` routes, already answering 503). `deploy/` was left out of the fresh root
because it held box paths and a sudoers rule. The
boundaries above forbid it, and web-con and admin-con replace it. Do not extend it.
`src/bin/weaver-web.rs` currently wires both halves together (legacy router, link listener,
and trace pump beside `surfaces::routes()`), so removing the legacy half starts there.

## Inherited documents

- `docs/weaver-web-Spec.md` and `docs/weaver-web-PRD.md` were written inside WeaverTools
  (2026-09-04). They describe the store and the read path well. The handoff's step 2 rewrites
  the charter for this repository and trims the Spec to the store and read path. Until then,
  use them as reference for the store; where they conflict with the handoff, the handoff wins.
  `README.md` and `docs/technical/` describe the older chat/lifecycle/trace product and are
  stale.
- `docs/project/inventory-weaver-web-code.md` is a register of the code against the Spec.
  `docs/project/issues-carried-2026-09-26.md` holds five frontend issues carried whole from
  WeaverTools.
- Issue and PR numbers in inherited code and docs (`#499`, `#585`, `#689`, ...) are
  WeaverTools numbers.
- **Conformance headers.** Files written to the Spec open with `//! conforms: web-<assertion>`
  lines, naming the claims in the Spec's section 9 table that the file is enforced against. A
  perturbation instrument must be shown to fail when its guard is removed. Keep the headers
  and the table in sync when moving an enforced claim.
- Inherited house style: ASCII only; absolute dates; each election's reasoning stated where it
  is made; superseded text removed, since git is the archive; and commit subjects like
  `code: ...`, `docs: ...`, `process: ...`. This repository may keep or change these.
