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
Issue #6, the role-based identity epic, records the operator's rulings of 2026-10-02 on
privilege and identity and the ordered work after them (admin-con act 6, IAM act 7); the
brief that wrote them into the Spec is `docs/project/brief-2026-10-02-act-4-the-role-shape.md`.
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
- **admin (the management plane)** carries the lifecycle verbs (`show`, `validate`, `load`,
  `unload`, `stop`, and since WeaverAgent's A3.2 `save-point`, `restore` and `force-unload`,
  which the server does not yet ask) and the agent's trace, which lands in
  the sink admin opens at load and is read through the relay the agent's start step
  launches. Contract: `weaver-admin-operator-contract`. Reached only through **admin-con**.
  There is no admin socket; the verbs are fixed command lines the box's sudo rule grants,
  and the relay is the trace's door.
- **weaver-analysis** reads the diagnostic record and sends finished records here: a
  per-position series (turn, ordinal, token, entropy, surprisal) and a per-generation summary.
  Contract: `weaver-analysis-web-contract`. They land in this repository's own Postgres store.

**The connectors are this repository's** (design session 2026-09-30, operator 2026-10-01).
gate-con and admin-con are two binaries that stand on the agent's box and are the one party
that reaches it: gate-con dials the gate socket, admin-con runs the verbs through the box's
sudo rule and reads the trace through its relay socket. gate-con is the operator's name for what the whiteboard called web-con. Both are
**clients** of this server's listener over a mutually authenticated link, and the server
keeps a register of agents with two credentials per agent. The design is Spec section 8 and
the brief named above. Both binaries stand: `gate-con` (act 3) and `admin-con` (act 6),
on the shared client half `link::client`. The three contracts live in WeaverAgent
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
- **One privileged invocation in this repository, admin-con's sudo invoker, and nothing
  else** (operator's ruling of 2026-10-03, revising the 2026-10-02 rule of no sudo at all;
  #14, Spec 7.2). The box's strict per-agent sudo rule grants admin-con's own service user
  exactly the fixed `weaver-admin <verb> <agent>` lines for its agent. The server sends an
  abstract verb, never a command; admin-con maps it to the granted line, builds the command
  from constants and the configured agent name alone, runs `sudo -n` with stdin closed, and
  passes nothing about the person to the box. A root process parsing arguments that arrived
  over a network is still where a CVE comes from, which is why nothing from the link enters
  the command. The connectors run as dedicated service users, one per agent and plane, never
  the operator's uid. Every verb passes three gates: this server's IAM, the ceiling admin-con
  derives from the sudo rules (`sudo -n -l` on each exact line) and declares in its hello,
  and the box's sudo rule itself (Spec 2.13 and 8). A turn needs a grant too and then passes
  the gate's own admission. The register verbs act on the server and take their own path
  (Spec 2.13). No group name, socket path, mode, uid or sudoers text enters this repository;
  cite WeaverAgent's documents instead.
- **Do not edit WeaverAgent.** When a door contract lacks something, file an issue on
  `toddwbucy/WeaverAgent`, one issue per interface question, until the operator rules
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
cargo build --locked                     # the `passkeys` feature is on by default
cargo build --locked --release --no-default-features --bin gate-con --bin admin-con   # connectors for an agent box: no OpenSSL
cargo test --locked                      # DB-backed tests pass by skipping without DATABASE_URL
cargo clippy --all-targets --locked
cargo fmt
cargo run -- --config <config.toml>                                 # serve
cargo run -- --config <config.toml> authority init                  # once, before any agent; needs the store; refuses to overwrite
cargo run -- --config <config.toml> register <box> <name> --out <dir>   # two client configs, written to <dir>
cargo run -- --config <config.toml> revoke <ag-id|box/name> <gate|admin>
cargo run -- --config <config.toml> rotate <ag-id|box/name> --out <dir>
cargo run -- --config <config.toml> agents                          # the register, presence derived
cargo run -- --config <config.toml> person bootstrap <name>         # a person, their admin grant and a token, printed once
cargo run -- --config <config.toml> person token <pe-id|name>       # a token for a person holding no passkey, printed once
cargo run -- --config <config.toml> person reset <pe-id|name>       # clear a person's passkeys and print a token
cargo run -- --config <config.toml> grant add <person> <role> [--agent <ag-id|box/name>]
cargo run -- --config <config.toml> grant remove <person> <role> [--agent <ag-id|box/name>]
cargo run -- --config <config.toml> role set <observer|operator> [<verb>...]
cargo run --bin gate-con -- --config <gate-con.toml>                # the data plane's connector, on the agent's box
cargo run --bin admin-con -- --config <admin-con.toml>              # the management plane's connector, on the agent's box
```

- **DB-backed unit tests** (`store::read`, `store::plan`, `store::audit_tests`,
  `store::identity_tests`, `host_tests`, `surfaces::record`, `passkeys_tests`,
  `sign_in_tests`, `keys_tests`, `admin_tests`, `link::tests`,
  `link::client_tests`, `link::admin_con_tests`, `link::verbs_audit_tests`)
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
- **Passkeys and OpenSSL**: the `passkeys` feature (default on) carries `webauthn-rs` 0.5 and
  its OpenSSL, and the `weaver-web` binary requires it. **Build connectors for an agent box with
  `--no-default-features`**: their dependency graph then holds no `openssl-sys`
  (`tests/connectors_link.rs` checks both that and `ldd` on the test build's binaries; measured
  2026-10-08, the default build's connectors carry no `libcrypto` either, the linker dropping
  it). Clippy runs both ways (`--no-default-features` too). The passkey tests use
  `webauthn-authenticator-rs`'s soft passkey, a dev-dependency. `GET /enroll` is the page where a
  person pastes their enrollment token to register their first passkey (`surfaces::enroll`,
  `src/passkeys.rs` for the ceremony table: 64 in flight, five minutes, used once); it opens no
  session. `GET /sign-in` is the sign-in page; Record links to it without a session.
  `GET /passkeys` (`surfaces::keys`) is a signed-in person's own passkeys: adding one is a
  fresh assertion whose verified answer is a one-time grant (in the ceremony table, bound to
  the session and person), spent by the registration that starts with it; removing one is
  never of the last, counted under the identity exclusion. The
  library's `danger-credential-internals` feature is on for typed read access to a stored
  passkey's counter. The surfaces answer `Content-Security-Policy: script-src 'self'`, so no inline script
  and no `hx-on`.
- **Run the server:** `cargo run -- --config <config.toml>`, where the config sets `listen`,
  `link_listen`, `database` and `authority_dir`, with `silence_bound_secs` (60), `link_address`
  and `server_name` (`weaver-web`) optional (see `ServerConfig` in `src/config.rs`). The server
  refuses to start without an authority at `authority_dir`; `authority init` makes one.
  Optional `tls_certificate` and `tls_key` (PEM paths) make the browser's listener serve TLS
  only, no plain listener beside it; optional `origin` is the listener's serialized origin
  (`https`, or `http` on `localhost`), and the certificate must be valid for its host;
  optional `rp_id` is the relying party's domain, configured with `origin` or not at all, the
  origin's host or a domain it is under; `session_idle_secs` (3600) and
  `session_absolute_secs` (43200) are a session's limits, the idle one never under 300, since
  the last use is written at most once a minute, and the absolute never over 604800 (seven
  days). The start refusals are
  `src/listen.rs`'s, design section 3. Keep configs, certificates, keys,
  authorities and client configs out of the repository. Logging uses `RUST_LOG`,
  which defaults to `weaver_web=info,sqlx=warn`.
- **gate-con** reads the file `register` wrote (`server`, `server_name`, `agent`, `agent_id`, `plane`,
  `server_certificate`, `certificate`, `key`) plus `gate_socket`, the agent's gate socket, a
  required box fact with no default, and optional `turns_in_flight` (4). `--config` has no
  default either. It refuses to start on a config that is not a regular file of its own uid at
  0600 or tighter, opened without following a symlink, or one minted for the admin plane. Its
  tests (`link::client_tests`) run it in-process against a fake gate and the real listener;
  no test reaches an agent.
- **admin-con** reads the file `register` wrote plus two required box facts with no default,
  `trace_socket` (the agent's trace relay, absolute) and `weaver_admin` (the absolute path
  the box's rule names), and optional `backfill_bytes` (1 MiB, at most 256 MiB, the tail
  relayed after a server restart and what an opening holds to replay without reading again),
  `verb_bound_secs` (960, set above the box's load bound) and `stop_grace_secs` (1080, the
  load bound, the unload bound and a margin). Same trust rule as gate-con's. It reads the
  trace through the relay (`link::relay`: one request line, the header's identity, the
  whole-record digest), reports the trace door in its hello and in `door` frames, takes every
  opening as an admission of the trace (boundary at the relay's first heartbeat, replay,
  `show`, `caught_up`; only while no invocation is in flight), drains to a heartbeat before a
  verb, relays with replay and marked discontinuities, and runs verbs
  through `link::sudo_invoker`, the repository's one privileged invocation: `sudo -n
  <weaver_admin> <verb> <agent>`, the hello's ceiling from `sudo -n -l` on each line, stdin
  null, the child in its own session, never killed, and holding admin-con's one invocation
  slot across reconnections until it is reaped. An orderly stop waits for that child within
  the grace, then unloads the agent where the ceiling grants `unload`, asking again every
  5 s (`REST_RETRY`) while it refuses `ActivityNotAtRest` until the grace runs out, and
  never runs `force-unload`. A writer that never
  idles gives the relay no heartbeat, so an opening takes its boundary at 30 s and a drain
  invokes at 10 s (`BOUNDARY_BOUND`, `DRAIN_BOUND`; `toddwbucy/WeaverAgent#88`). Its tests (`link::admin_con_tests`) run it against a fake
  relay (`link::fake_relay`) serving a temporary trace file, the real listener and a fake
  invoker, and
  `link::sudo_invoker_tests` against a fake `sudo` generated at test time and first on the
  child's `PATH` (a test build refuses to run without it); no test reaches an agent or a real
  `sudo`.
- **Register verbs** answer one JSON object on stdout with the exit status agreeing, the shape
  `weaver-admin` uses. `revoke` closes a live connection in a running server through the
  store's notification channel; nothing else links the verb's process to the server's.
  **Each is audited as the host's** through `src/store/audit.rs`, the audit's one writer
  (migration `0014`, append-only by trigger): a first record before it acts, an outcome
  naming it after, and no act where the first cannot be written; `authority init` needs the
  store for this. The answer names its first record under `audit`.
- **The host's identity commands** (`src/host.rs`, over `src/store/identity.rs` and
  migration `0015`): `person bootstrap`, `person token`, `person reset`, `grant add`,
  `grant remove` and `role set`, each audited as the host's like the register verbs (an
  `--author` of a person's whole `pe-` shape refused before any record, at every host command) and each
  writing under the identity exclusion, one advisory key (`IDENTITY_LOCK_KEY`). A token is
  printed once and stored as its digest; its lifetime is the config's
  `enrollment_token_hours` (24, at most 168) or `--hours`. A person is named by `pe-` or by
  name, matched in its canonical form (Unicode's compatibility caseless match). The
  last-admin rule binds the host. `store::identity_tests` and `host_tests` each run on a
  database of their own (`fresh_store`, named `wwt_...`, dropped at the end), since the
  last-admin rule is store-wide.
- **The admin's page** (`GET /admin/persons`, `src/surfaces/admin.rs` over
  `src/store/admin.rs`; act 11, PR 5a) stands beside the host's commands, which remain: for a
  session whose person holds a live admin grant, every person and five writes, `enroll`,
  `token`, `disable`, `enable` and `rename`, each a form's post audited with the admin as
  principal by `session`, each in one identity transaction that first re-checks the session
  and the admin grant. An admin never writes their own row; a disable revokes the person's
  outstanding token (migration `0018`, ending `revoked`) and never leaves no enabled admin; a
  token is shown once under `no-store`. Roles and grants are not on it yet (PR 5b), and the
  register verbs stay host commands. `admin_tests` runs each test on a `fresh_store`.

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
- `src/surfaces/` holds the browser surfaces: Record, enrollment, sign-in, one's own
  passkeys, and the admin's persons. The router's
  state is `Store` alone. A surface reads the store and never writes the recorded half, since
  runs and positions land only by ingest. A surface that needs a seam takes it as its own
  argument rather than widening the router state.
- `surfaces/gate.rs` is the session gate. A session is a person's (migration `0017`): it
  carries the person and the passkey it was opened with, under the `__Host-weaver_session`
  cookie (`Secure`, `HttpOnly`, `SameSite=Strict`, `Path=/`), its bearer stored only as a
  SHA-256 digest. Each use checks it closed, its person disabled, its passkey removed, open
  past `session_absolute_secs` or unused past `session_idle_secs`, closing the row where it
  ended, and refreshes its last use at most once a minute; `POST /sign-out` closes it. Every
  request but `GET` and `HEAD` must carry `Origin` equal to the configured `origin`, over the
  whole app (`gate::guard`). An authored row's author is the person's `pe-` identity, and
  `Store::author` renders it. A session is opened by sign-in alone (`GET /sign-in`,
  `surfaces::sign_in`): name-first, the library's verification, the counter rule
  (`passkeys::count`, a locked read, merge and write committed first), then `gate::open`. Per `docs/project/design-2026-10-07-iam.md`
  section 6 and Spec 2.8.
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
the reconnect policy of Spec 8). **admin-con landed on 2026-10-02** (act 6):
`src/bin/admin-con.rs` over `link::admin_con`: the replay and `caught_up`, and the verb
plane, one verb at a time with its answer placed at the invocation, behind an `Invoker`.
**The sudo invoker landed in act 9a** (#17): `link::sudo_invoker`, the process-wide slot and
the orderly stop's `unload`. **The relay client replaced the file tailer in act 9b**
(#17): `link::relay`, the door and its openings. The listener holds each connection's
ceiling, asks `show` only where it is granted and at every opening of the door, and records
the ceiling and the load state's source on the row (migration `0011`), the tuple's own source
(`0012`, since a turn moves the state and not the tuple), and the run's constituents from
`show` and the trace door's state (`0013`); `0010` through `0013` are frozen. `traceview.rs` keeps the rings, the listener's live window; its seed
tailer and `lifecycle.rs` (which ran the verbs through sudo) left the tree.

**Still leaves: `web/`**, the legacy `/admin` routes, already answering 503, and
`src/registry.rs`, the legacy participant model (not the register of agents). `deploy/` was
left out of the fresh root because it held box paths and a sudoers rule, both of which the
hard boundaries forbid. Two facts for the
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
