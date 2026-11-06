# Brief: cleanup PR 2, the legacy half leaves

From the planner seat to the executor, 2026-11-06. This is the second of three PRs in the cleanup act the operator approved on 2026-11-06. PR 1 (#49) merged at `b12ce77`. This PR is code only: it removes what `CLAUDE.md` has scheduled to leave, and adds nothing. Land this brief as `docs/project/brief-2026-11-06-cleanup-pr2-the-legacy-half-leaves.md`, the PR's first commit.

## Scope

- **Base:** `main` at `b12ce77`.
- **Branch:** `code/the-legacy-half-leaves`.
- **Commit subjects:** `code: ...`. One commit per section below.
- **The diff touches no file under `docs/` except this brief, and neither `CLAUDE.md` nor `README.md`.** Every document this PR makes stale is PR 3's (listed at the end). Name each in the PR body; do not edit it.
- Open a **draft** PR and tell the planner.

## 1. The legacy router and its templates

Delete:
- `src/web/mod.rs`
- `src/web/admin.rs`
- `src/web/templates/` (`base.html`, `lifecycle.html`, `agent_config.html`, `trace.html`, `trace_event.html`)

Seven routes leave with them (the six legacy page routes listed here all answered 503 on `main`):
- `GET /admin`
- `GET /admin/lifecycle`
- `POST /admin/lifecycle/{agent}/{verb}`
- `GET /admin/agents/{agent}/config`
- `GET /admin/trace/{agent}`
- `GET /admin/trace/{agent}/stream`
- `GET /assets/{file}`

Then:
- `src/lib.rs`: drop `pub mod web;`.
- `askama.toml`: `dirs = ["src/surfaces/templates"]`.
- `src/bin/weaver-web.rs`:
  - Line 13's `use weaver_web::traceview::TraceViews;` goes, if nothing else in the file uses it. Line 14 becomes `use weaver_web::{host, store};`.
  - Delete the `web::AppState` block (488-494) and its comment.
  - The `Origin` guard wraps the instrument alone (500): `gate::guard(instrument, policy)`. **It stays over the whole app**, which is now the instrument.
  - Rewrite the comments at 437-440 ("rather than the legacy admin `AppState`") and 497-499 ("the legacy routes' included") to state what holds now.
- `src/surfaces/mod.rs`: the module comment at 4-6 ("These stand apart from the legacy admin routes in `web/`...") and the stylesheet's doc comment at 149-150 ("independent of the legacy assets retained by `web/`") state what holds now. They do not narrate the removal.
- `assets/sse.js` and `assets/style.css`: delete. Nothing references them once `web/` is gone; grep to confirm.
- `assets/VENDORED.md`: drop the `sse.js` row and the `style.css` sentence.
- `assets/htmx.min.js` **stays**: `src/surfaces/mod.rs:145` serves it at `/assets/surfaces/htmx.min.js`.

**Not removed:**
- `src/traceview.rs` is the listener's live window (`src/link/listener.rs:50`, `src/surfaces/reading.rs:56`, `src/link/admin_con.rs:37`).
- `src/adapters/gate.rs` is gate-con's client.
- `src/fault.rs` stays; its module comment names `web/mod.rs` as history (`src/fault.rs:4-12`). Restate that comment as current state.

## 2. The legacy participant model and the upstream adapter

Delete:
- `src/registry.rs`, and `pub mod registry;` in `src/lib.rs`.
- `src/adapters/upstream.rs`, and `pub mod upstream;` in `src/adapters/mod.rs`.

## 3. The legacy config fields

In `src/config.rs`, delete:
- `admins` (47-51), `agent_hop_budget` (52-55), `providers` (56-57);
- `ProviderConfig` and the comment block above it (100-113);
- `default_agent_hop_budget`.

`src/bin/weaver-web.rs:386` logs `cfg.providers.len()`: drop the line. Do not replace it with another log line.

Six test `ServerConfig` literals drop the three fields:
- `src/host_tests.rs:25-27`
- `src/listen_tests.rs:22-24`
- `src/keys_tests.rs:665-667`
- `src/link/tests.rs:2044-2046`
- `src/link/tests.rs:3077-3079`
- `src/agents_tests.rs:61-63`

`ServerConfig` does not deny unknown fields, so a config still carrying the three keys keeps loading. No deployment exists yet. Name this in the PR body as a fact; add no `deny_unknown_fields`, which would be a new behaviour.

## 4. The startup test keeps its Origin instrument

`tests/startup.rs:259-291` asserts the legacy routes' 503s. It also holds the one end-to-end instrument that a state-changing request is refused at the `Origin` check with no origin configured; Spec section 9's row at about line 4565 cites it.

Replace the block with:
- **`POST /sign-out`** (always mounted, `src/surfaces/mod.rs:98`), with and without the session's cookie, answering **403** with the body "no origin is configured, so this server serves no request that changes state". This is the same refusal the old POST met.
- **The six former GET paths answer 404**, and `GET /assets/surfaces/htmx.min.js` still answers 200.
- **The former `POST /admin/lifecycle/{agent}/{verb}` depends on axum.** Whether the `Origin` layer covers axum's default fallback decides whether it answers 403 (refused at the guard) or 404. Measure it, assert what it answers, and state in the PR body which one and why. Either way it never reaches a handler. The latter shows the surfaces' own asset route standing while `/assets/{file}` is gone.
- **Perturbation, shown and named in the PR body:** in a detached worktree of the pushed head (never the shared checkout), remove the `Origin` check (`origin_check` returning `next.run(request).await` unconditionally). The new assertion must then fail. Restore it.

The file's opening comment stays accurate. Edit it only if a sentence in it names the legacy routes.

## 5. Litter on the way: `pulldown-cmark`

`Cargo.toml:99` declares `pulldown-cmark`, and nothing in `src/` or `tests/` uses it on `main` (grepped for `pulldown` and `push_html`). It left with the conversation half and its declaration did not.
- Remove the line.
- Regenerate `Cargo.lock` with `cargo build`, then confirm `--locked` passes.
- Name it in the PR body as litter picked up.

Before removing any further dependency, grep it outside `src/web/`, `src/registry.rs` and `src/adapters/upstream.rs`. The planner's grep found no other dependency that this PR orphans. If yours finds one, remove it in this commit and name it.

## Gates, in a fresh detached worktree of the pushed head on a fresh scratch database

Run `git worktree prune` first. Use a new worktree name with `|| exit 1` after the add, and print `git log -1` inside the worktree to confirm the head (a failed add would otherwise run the gates against the shared checkout).

1. `cargo fmt --check`.
2. `cargo clippy --all-targets --locked`, and again with `--no-default-features`.
3. `cargo build --locked --release --no-default-features --bin gate-con --bin admin-con`.
4. The full suite with `DATABASE_URL` set, **no `skipped:`** (`cargo test --locked -- --nocapture 2>&1 | grep -c skipped:` is 0), and `cargo test --doc`.
5. `DATABASE_URL=<never-migrated db> cargo test --locked --test startup -- --ignored`.
6. `tests/connectors_link.rs` passes. Its OpenSSL and `ldd` checks hold.
7. Greps over `src tests askama.toml Cargo.toml assets` must be empty:
   - `web::`, `crate::web`, `registry`, `Participant`, `ProviderConfig`, `agent_hop_budget`, `providers`, `admins`, `src/web`, `pulldown`;
   - `upstream` **as a module or type**. The word "upstream" in prose about entries sent upstream stays.
8. **Disabled-guard sweep** over the diff, which must be empty: `|_| false`, `if false`, `&& false`, `|| true`, `#[ignore]` (beyond the existing one on `startup`), `todo!`, `dbg!`.
9. ASCII check over the changed files, no em-dash.
10. Clean up the scratch databases and the worktree. No `wwt_` database is left behind.

## What PR 3 owes (name in the PR body; do not edit)

- **Spec 918:** the `Origin` check "over the whole app, the legacy routes included".
- **Spec section 9's row at about 4565:** "`tests/startup.rs` ... refuses a legacy POST at the `Origin` check". The instrument is now `POST /sign-out`.
- **Spec section 1's tree:** the `web/`, `adapters/upstream.rs` and `registry.rs` entries marked leaving.
- **`CLAUDE.md` 419-427:** "Still leaves: `web/`...". Also any other `CLAUDE.md` mention of the legacy router, `registry.rs` or the legacy config fields.
- **The `instrument.css` sentence in Spec section 6** (ruling D4).

## The PR body

- One section per commit above, with what was removed and its ground: `CLAUDE.md`'s schedule, and the survey of 2026-11-06.
- The behaviour change: the six GET paths go from 503, and `/assets/{file}` from 200, to 404. Give the POST path's measured answer.
- The config fact from section 3.
- The perturbation's result.
- The litter.
- The PR 3 list.
- **`Implements:`** none. It is a removal: no Spec section gains or changes a claim, and the one instrument moved keeps its claim (section 9's `Origin` row).
- End with the attribution line the session gives.
