# Handoff: the workspace turns to weaver-web

Version: v0.1, 2026-09-30, late afternoon. Written from the suite planning seat at
`WeaverTools_Project/`, the session that split the monorepo's trees into four
repositories and pushed them, for the session that opens inside `WeaverWeb/` next.
Everything measured here was measured today on the thinkpad; every ruling is the
operator's and is dated. The founding handoff,
`HANDOFF-2026-09-30-weaver-web-session.md`, still governs what this repository is and
what it builds against. This document says what changed around it since that was
written, what stands in it now, and what the next session finds on the box.

## 1. The claim

WeaverWeb is unchanged since its root commit and everything around it has moved. The
suite split is done: four repositories, each pushed at a fresh root commit on
2026-09-30. WeaverAgents and WeaverAnalysis development moves to olympus, in sessions
inside those repositories. This workspace on the thinkpad is devoted to WeaverWeb from
here, on the operator's word of 2026-09-30, and a session opened at
`WeaverTools_Project/` does suite-wide work only. The frontend's order of work is the
founding handoff's five steps, none of which has started.

## 2. What changed since the founding handoff

- **The repositories.** `toddwbucy/WeaverAgents` (`main` at `a9d9827`) is the agent, ten
  workspace members. `toddwbucy/WeaverAnalysis` (`12a7243`) is the diagnostic consumer,
  one crate at root. `toddwbucy/WeaverTools` (`44bb1ca`) is the suite repository, holding
  `experiments/` and a README; the contracts, vision and process documents are still to
  come to it. The monorepo is renamed `toddwbucy/WeaverTools-old2`, to be archived, its
  clone beside this workspace on the thinkpad. Every
  `toddwbucy/WeaverTools` reference in this tree's documents predates the rename and
  means the monorepo.
- **Where a connector issue goes.** This repository's `CLAUDE.md` says to file interface
  issues on `toddwbucy/WeaverTools`. That name is now the suite documentation
  repository. The connectors, web-con and admin-con, are WeaverAgents' acts, and the
  monorepo's 31 open issues were transferred to `toddwbucy/WeaverAgents` today with
  their labels, so that is where a connector ask lands until the operator rules
  otherwise. The 22 lane and priority labels of the monorepo exist on WeaverWeb too,
  recreated by name, colour and description, so the five carried issues can be re-filed
  with the labels they had.
- **Visibility.** The founding handoff's step 1 ruled the repository local, no remote,
  private when one is made. It has a remote and it is public, as are the other four. The
  publish boundary applies to every commit: no credential, box path, security posture,
  commercial or strategy material.
- **The contracts the frontend reads** are unchanged in text and now live in
  `WeaverAgents/docs/crates/contracts/`: `weaver-gate-world-contract.md`,
  `weaver-admin-operator-contract.md`, `weaver-analysis-web-contract.md`. The analysis
  contracts are also copied, byte for byte, into `WeaverAnalysis/docs/`; which copy is
  authoritative is unruled.
- **The experiments moved.** `experiments/` is in the suite repository now, including
  `trace-content/herobench-agents/`, whose `code/` is the HeroBench loop the founding
  handoff's first consumer watches.

## 3. What stands in this repository, measured

    commit     4ce4f41, one commit, main in sync with origin, tree clean
    issues     none on GitHub; five carried in docs/project/issues-carried-2026-09-26.md
    build      cargo build --locked passes; cargo clippy --all-targets --locked, no warnings
    tests      24 passed, 0 failed, 1 ignored, 6 binaries. Nineteen of the passes print
               `skipped:` under --nocapture and assert nothing: the DB-backed tests with
               no DATABASE_URL. That is carried issue #589, still true.
    rust       5,567 lines under src/, both halves of the old product still wired together

The founding handoff's step 1 decided the split of that code and nothing has executed
it. What carries forward, about 3k lines, is `src/store/` (`read.rs` 989, `plan.rs` 861,
`key.rs`, `experiment.rs`, `mod.rs`; 16 tables over the nine migrations under
`migrations/`), `src/surfaces/` (the Record surface, `record.rs` 570, and the session
gate, `gate.rs`), the two templates directories `askama.toml` names, and the vendored
htmx and SSE under `assets/`. What leaves, about 2.2k lines, is `src/wire.rs` (823),
`src/web/` (the legacy admin routes, `admin.rs` 439), `src/traceview.rs` (334),
`src/adapters/gate.rs` (162), `src/lifecycle.rs` (101), and
`src/bin/weaver-web-connector.rs`. `src/bin/weaver-web.rs` (110 lines) wires both
halves, so the removal starts there. The Record surface's route is reachable in a test
through `tower::oneshot` without binding a port, which is the pattern the tests use.

**Documents.** `docs/weaver-web-PRD.md` and `docs/weaver-web-Spec.md` were written in the
monorepo on 2026-09-04 for the chat, lifecycle and trace product; the store and read
path sections still describe the code. `README.md` and `docs/technical/` describe that
old product and are stale. `docs/project/inventory-weaver-web-code.md` is the register
of the code against the Spec. `docs/project/mockup-ablation-matrix.html` and
`sketch-ablation-matrix.md` are the ablation-matrix view sketch. `CLAUDE.md` is current
except for where it sends connector issues (section 2 above) and the seat names it
carries (section 6 below).

## 4. What the next session finds on this box

The box's paths, the uid and the database role were removed from this section on
2026-10-01 before it landed, per `CLAUDE.md`'s boundary that no box path or security
posture enters the repository, which is public. Where the layout matters it is in
WeaverAgents' `deploy/REDEPLOY.md` and `deploy/HowToDeployANewAgent.md`.

- **An installed agent stack, redeployed today from WeaverAgents at
  `nogit-eda642e70b70`**, the tree that became `a9d9827`: the installed members, the
  engine libraries, admin's config and the agent config directory, where the two deploy
  documents put them. Two agents declared and verified: `karl` (q6_k gguf, no store) and
  `m1` (postgres store, with its own database and role). Neither is loaded. Loading is
  `sudo deploy/verify-load.sh <name> --keep` from `WeaverAgents/`, and the loop was
  proven: two turns through karl's gate answered in about 0.1 s each. The procedure is
  the two deploy documents; the log is
  `WeaverAgents/docs/project/redeploy-2026-09-30-thinkpad.md`.
- **`WeaverAgents/deploy/turn.py`**, the smallest gate client: one JSON line
  `{"text": ...}` in on the agent's gate socket, one line out, run as a uid the agent's
  declaration admits and with no sudo, per `weaver-gate-world-contract` sections 2 and
  3. **This is the reach the frontend is forbidden.** It exists for proving the loop and
  for the matrix harness; a web-con stub is the frontend's development surface, and
  nothing in this repository dials that socket. The close line the gate returns is worth
  knowing the shape of:
  `{"kind":"answered","run":"<run ref>","text":"...","turn":"t-1"}`, with `finish:
  "length"` only when the generation was cut.
- **The operator's shell needs a fresh login after an agent is created** to carry the
  agent's group; the box was rebooted today, so it does now.
- **A database and a role for the store must be created on the box before the
  DB-backed tests assert anything.** The store's tests and the server itself need a
  database this repository owns; `CLAUDE.md`'s commands assume
  `DATABASE_URL=postgres:///<db>?host=/run/postgresql`. Making them is the first
  environment act, and how the store's tests stop asserting nothing.
- **Deposits to render** are on the bulk store, in the testing directory the founding
  handoff names: the HeroBench sessions of 2026-09-29 and 2026-09-30
  (`herobench-sessions-2026-09-30`, `herobench-agents-2026-09-29`,
  `herobench-positions-2026-09-29`), each with `trace.ndjson` and a state store, and the
  determinism-matrix deposits with `matrix.jsonl` and `summary.json`. Two fresh deposits
  were made today for a matrix run on the new stack,
  `determinism-matrix-thinkpad-2026-09-30-eda642e7` and its `-smoke`, each holding only a
  `config.json` so far. Write to the bulk store as the operator.
- **The old stack's archive** is on the bulk store too, in a directory dated 2026-09-30:
  every trace the previous agents wrote, karl's 3.8 GB among them, the m1 database dump,
  and the old declarations, verified by checksum. Old traces for a replay view are there.

## 5. The order of work, as the founding handoff set it and this seat reads it now

1. Land this handoff and amend `CLAUDE.md` for section 2's two facts, by this
   repository's own loop: branch, draft pull request, the planner's review, the operator's
   merge. The seed is the only direct push to `main`.
2. Make the database role and a database for the store on this box, run the DB-backed
   tests for real, and record in the pull request that they ran rather than skipped.
3. Step 1 of the founding handoff's split: remove the half that reaches the agent,
   starting at `src/bin/weaver-web.rs`, so the binary is the store, the read path and the
   surfaces. `cargo test --locked` with `DATABASE_URL` set is the gate.
4. Step 2: rewrite the charter for this repository and trim the Spec to the store and the
   read path, in the order the HeroBench view needs things.
5. Step 3: a stub of web-con and admin-con answering the door contracts' shapes, in this
   repository, as the only thing the frontend talks to.
6. Step 4: render the deposits first, a trace and its state store as a replay, from the
   HeroBench sessions above, before any live view.
7. Step 5: file each connector need on `toddwbucy/WeaverAgents`, one per interface
   question, with what was measured, what is asked, and which document would move.
8. Re-file the five carried issues on WeaverWeb with their labels, closing the loop the
   carry document opened.

## 6. Open questions for the operator

- **Seat names.** `CLAUDE.md` names `thinkpad-WeaverWeb-planner` and
  `thinkpad-WeaverWeb-executor`. The suite session on this box was named
  `Thinkpad-WeaverTools_Project-Planner`. Whether the new session is the planner, and
  whether an executor session opens beside it, is the operator's.
- **Visibility.** Public today against the founding handoff's ruling of private. Which
  stands.
- **Whether the checkout moves out from under `WeaverTools_Project/`**, as the founding
  handoff's step 1 recorded. The suite `CLAUDE.md` one directory up loads in every
  session opened here; it is short and says nothing that binds this repository, but it
  does load.
- **Which copy of the analysis-web contract the frontend reads**, WeaverAgents' or
  WeaverAnalysis', until the suite repository holds the one copy.
- **Whether the connectors' issues go to WeaverAgents**, as this handoff assumes, or wait
  for the suite repository to hold the contracts.

## 7. What this document does not do

It does not charter the connectors, decide the frontend's stack, amend `CLAUDE.md`, or
re-file the carried issues. It does not touch the code. It records no ruling the
operator did not make today, and the founding handoff's boundaries stand unamended:
consumers never reach the agent, no credential or box path enters the repository, and
the interface moves with a contract's care.

## Manifest

    Base        WeaverWeb main at 4ce4f41, in sync with origin, tree clean before this file
    Measured    cargo build, clippy, test at 4ce4f41 on 2026-09-30: pass, 0 warnings, 24/0/1
    Produced    this document, uncommitted, for the next session to land by the repository's loop
    Around it   WeaverAgents a9d9827, WeaverAnalysis 12a7243, WeaverTools 44bb1ca, all pushed 2026-09-30
    On the box  stack installed, karl and m1 declared and verified, nothing loaded, a database and role for the store still to be made
    Asked       section 6's rulings, then section 5 in order
