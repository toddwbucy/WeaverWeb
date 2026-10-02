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
stack, editorial rules, and review process. Nothing in WeaverTools' process documents binds
it. The suite `CLAUDE.md` one directory up, at `WeaverTools_Project/`, loads in every session
opened here; it describes the suite and binds nothing in this repository. The founding
documents are `docs/project/HANDOFF-2026-09-30-weaver-web-session.md` and, beside it, the
second handoff `docs/project/HANDOFF-2026-09-30-the-workspace-turns-to-weaver-web.md`. The
connectors' brief is `docs/project/brief-2026-10-01-the-link-the-registry-and-the-two-connectors.md`.
Read them before structural work.

### The big picture (operator's whiteboard, 2026-09-30)

```text
                 WeaverTools (the agent)
                  /                 \
               gate                admin <-> diagnostic
                 |                   |           |
            (gate-con)          (admin-con)      v
                 \                  /      weaver-analysis
                  v                v           |
                     Weaver-Web  <-------------+
```

Weaver-Web has three inputs and no others:

- **gate (the data plane)** carries the work entering an agent and the answers leaving it.
  Contract: `weaver-gate-world-contract`. Reached only through **gate-con**.
- **admin (the management plane)** carries the lifecycle verbs (`load`, `unload`, `validate`,
  `stop`, `show`, `list`, and later quiesce / resume) and the agent's trace, which leaves the
  agent through the sink admin opens at load. Contract: `weaver-admin-operator-contract`.
  Reached only through **admin-con**. There is no admin socket; the verbs are invocations
  and the sink is the one crossing.
- **weaver-analysis** reads the diagnostic record and sends finished records here: a
  per-position series (turn, ordinal, token, entropy, surprisal) and a per-generation summary.
  Contract: `weaver-analysis-web-contract`. They land in this repository's own Postgres store.

**The connectors are this repository's** (design session 2026-09-30, operator 2026-10-01).
gate-con and admin-con are two binaries that stand on the agent's box and are the one party
that reaches it: gate-con dials the gate socket, admin-con runs the verbs and tails the trace
file. gate-con is the operator's name for what the whiteboard called web-con. Both are
**clients** of this server's listener over a mutually authenticated link, and the server
keeps a register of agents with two credentials per agent. The design is Spec section 8 and
the brief named above. The seed holds the start of each (`src/adapters/gate.rs`,
`src/lifecycle.rs`); neither binary exists yet. The three contracts live in WeaverAgents
under `docs/crates/contracts/`, not here, and are the pages the connectors build against.

The first concrete consumer is the **HeroBench view**, where a researcher watches and
interviews an agent on a long-horizon benchmark. The suggested order is to render the landed
deposits first (a trace plus its state store, as a replay), then build live views.

## Hard boundaries

- **The server never reaches the agent.** Only gate-con and admin-con do, on the agent's
  box, and only through the two door contracts. The server opens no agent socket and runs no
  agent binary, even during development. A connector's tests use a fake at the socket, never
  an agent; the server's tests use fake connectors holding minted credentials.
- **No credential, box path, or security posture enters the repository.** Credentials are
  minted on the server at registration and dropped into a client's config file by an install
  script; the server keeps fingerprints and never keys. The connectors authenticate; the gate
  and admin authorize.
- **Do not edit WeaverAgents.** When a door contract lacks something, file an issue on
  `toddwbucy/WeaverAgents`, one issue per interface question, until the operator rules
  otherwise. Say what was measured, what is asked, and which document would have to move. The
  olympus Planning seat answers there.
- Do not link the agent's interior crates (`weaver-harness`, `weaver-spu`, `weaver-admin`,
  `weaver-gate`, `weaver-state`).
- **The remote is `git@github.com:toddwbucy/WeaverWeb.git`.** A different repository,
  `toddwbucy/Weaver-Web` (with a hyphen), is the archived v1 from August 2026. It shares no
  commits with this tree. Never push to it.

## How we work

Two Claude Code sessions share this workspace, and the operator (Todd) closes every loop.

- **Thinkpad-WeaverWeb-Planner** plans the work, directs the executor, reviews its PRs, and
  handles the third-party review.
- **Thinkpad-WeaverWeb-Executor** implements what the planner directs. It works on a branch
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
cargo run -- --config <config.toml>                                 # serve
cargo run -- --config <config.toml> authority init                  # once, before any agent; refuses to overwrite
cargo run -- --config <config.toml> register <box> <name> --out <dir>   # two client configs, written to <dir>
cargo run -- --config <config.toml> revoke <ag-id|box/name> <gate|admin>
cargo run -- --config <config.toml> rotate <ag-id|box/name> --out <dir>
cargo run -- --config <config.toml> agents                          # the register, presence derived
cargo run --bin gate-con -- --config <gate-con.toml>                # the data plane's connector, on the agent's box
```

- **DB-backed unit tests** (`store::read`, `store::plan`, `surfaces::record`, `link::tests`)
  connect to the database in `DATABASE_URL` and run the migrations. The link's tests run one at
  a time, since a listener's start resets every row's link state, which is the claim. Without that variable they print
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
  `link_listen`, `database` and `authority_dir`, with `silence_bound_secs` (60), `link_address`
  and `server_name` (`weaver-web`) optional (see `ServerConfig` in `src/config.rs`). The server
  refuses to start without an authority at `authority_dir`; `authority init` makes one. Keep
  configs, authorities and client configs out of the repository. Logging uses `RUST_LOG`,
  which defaults to `weaver_web=info,sqlx=warn`.
- **gate-con** reads the file `register` wrote (`server`, `server_name`, `agent`, `plane`,
  `server_certificate`, `certificate`, `key`) plus `gate_socket`, the agent's gate socket, a
  required box fact with no default, and optional `turns_in_flight` (4). `--config` has no
  default either. It refuses to start on a config that is not a regular file of its own uid at
  0600 or tighter, opened without following a symlink, or one minted for the admin plane. Its
  tests (`link::client_tests`) run it in-process against a fake gate and the real listener;
  no test reaches an agent.
- **Register verbs** answer one JSON object on stdout with the exit status agreeing, the shape
  `weaver-admin` uses. `revoke` closes a live connection in a running server through the
  store's notification channel; nothing else links the verb's process to the server's.

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

**The link's server half landed on 2026-10-01** under `src/link/`: the durable authority
and the register verbs (`authority.rs`, `verbs.rs`), the register of agents at the store
(`register.rs`, migration `0010`), the mutual-TLS listener with its admission, heartbeat,
startup reset, epoch and observation landing (`listener.rs`), and the frame vocabulary the
connectors build against (`frames.rs`). The seed's one dialed link, `wire.rs` and
`src/bin/weaver-web-connector.rs`, left with it. **gate-con landed on 2026-10-01** (act 3):
`src/bin/gate-con.rs` over `link::gate_con`, relaying through `adapters/gate.rs`, on the
shared client half `link::client` (dial, verify, hello, heartbeat, bounded reads and writes,
the reconnect policy of Spec 8). **Becomes admin-con, act 4:** `lifecycle.rs` (`sudo weaver-admin`, three verbs, load state still inferred from
the socket's existence) is the seed of admin-con, which gains the other three verbs and the
trace tailer. `traceview.rs`'s tailer half, which tails the trace file tracking its identity,
is the seed of that tailer (operator's ruling of 2026-10-01: the agent's sink is a file, not a
socket); its server half, the rings, is the listener's live window. Build admin-con as its
own binary on `link::client`, adding only its plane, as gate-con does.

**Still leaves: `web/`**, the legacy `/admin` routes, already answering 503, and
`src/registry.rs`, the legacy participant model (not the register of agents). `deploy/` was
left out of the fresh root because it held box paths and a sudoers rule. Two facts for the
removal act: `askama.toml` lists `src/web/templates` beside `src/surfaces/templates`, and
the legacy router in `src/web/mod.rs` is the only thing serving `htmx.min.js` and `sse.js`,
so the surfaces need their own asset route before `web/` goes. `src/bin/weaver-web.rs`
currently wires both halves together (legacy router, link listener, and trace pump beside
`surfaces::routes()`), so the removal starts there.

## Inherited documents

- `docs/weaver-web-Spec.md` and `docs/weaver-web-PRD.md` were written inside WeaverTools
  (2026-09-04). They describe the store and the read path well, and since 2026-10-01 the
  Spec's sections 2.12, 7 and 8 carry the register of agents, the connectors and the link.
  The handoff's step 2 rewrites the charter for this repository and trims the Spec to the
  store, the read path and the link. Until then, use them as reference; where they conflict
  with the handoffs or the brief, the later document wins.
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
