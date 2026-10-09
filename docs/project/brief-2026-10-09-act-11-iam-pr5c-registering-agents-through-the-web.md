# Brief: act 11, IAM, PR 5c: registering agents through the web

Version: v0.1, 2026-10-09. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 5b (#34) merged at `77e5a4d`. **The operator ruled on 2026-10-09 that
WeaverWeb is private infrastructure (a protected network or a VPN, never public) and that
everything done with agents belongs in the web page**, the register verbs included. Spec
2.13 already says that once identity stands, a register verb asked through the server
requires the admin grant. The agent work through the web is split in two:

- **5c (this one)**: the agents page, **registering** an agent and **rotating** its
  credentials, the two verbs that mint credentials and so hand over client configs.
- 5d: **revoking** one plane's credential and **retiring** an agent (both planes revoked).

The host commands stay beside them for bootstrap and recovery. The backend-first ruling
stands: plain functional pages, no styling.

## 1. Read first

- Spec 2.12 (the register of agents), 2.13 (the register verbs' item, the admin's four-step
  order for every write: parse, the first gate, resolve, the audited write, and the
  authority re-check inside the write's transaction) and 8 (the authority, the two
  credentials per agent, fingerprints kept and never keys).
- `src/link/verbs.rs` and `src/link/register.rs`: `register` and `rotate` as host commands
  (the authority lock, the store's `register_agent_on` and `rotate_credentials_on`, the
  staged config pair and its recovery, the audit), which this PR reuses without the config
  directory.
- `src/surfaces/grants.rs` (an admin page and its writes in the four-step order).

## 2. Scope

1. **The agents page** (`GET /admin/agents`, admin only): the register's rows, each agent's
   box and name, its `ag-` identity, each plane's credential state and presence, per Spec
   2.13's admin view (names, presence and credentials' state; not the door, the load state
   or the trace, which read access by grant shows in PR 6).
2. **Register** (box and name, each parsed to the register's own rules) and **rotate** (an
   `ag-`), each in the four-step order, under the authority lock as the host commands take
   it, audited with the admin as principal (method `session`) and the agent as target:
   - the server mints the two credentials as it does now; **the store keeps fingerprints
     only**, as now;
   - **the hand-over**: the two client configs (gate and admin, the same files `register
     --out` writes) are held **in the server's memory only**, keyed by a random 32-byte
     handle (the bearer rule), **each downloadable once**, **within five minutes**, by the
     same session that registered, `Cache-Control: no-store`, a download name that names
     the agent and plane; then dropped. Never written to the store, a file, a log or an
     audit record. A restart drops any not yet taken, and the admin rotates.
   - say on the page, once the configs are taken, that they hold private keys and go to the
     agent's box with the install script, at 0600 owned by the connector's user (no path,
     no user name).
3. **Out of this PR**: revoke and retire (5d); `authority init` and `authority rotate` stay
   host commands (the server's own authority; say so in the PR body); creating or
   decommissioning the agent on its box (WeaverAgent's provisioning, outside admin-con's
   verbs).

## 3. Tests and perturbations

- Register end to end: the row with fingerprints, the two configs each taken once and
  then refused, each config valid for its plane (a connector built from it dials the test
  listener, as the link's tests do), the audit pair naming the agent.
- The hand-over: a second take refused; a take after five minutes refused (a paused
  clock); a take from another session refused; no private key in the store, the audit, a
  log line or the response headers.
- Rotate: the old credentials stop admitting (a live connection closed as the host's
  rotate closes it), the new ones admit, the hand-over as above.
- The four-step order: a malformed box, name or `ag-` answers 400 with no record; a
  non-admin is refused with one record before any lookup; an unknown agent at rotate
  answers 404 with no record.
- The authority re-check: an admin whose grant is revoked between the page and the write
  is refused inside the transaction.
- Perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.13's register-verbs item as built (through the server, the hand-over); section 9's
  rows this PR enforces; count and summary swept.
- CLAUDE.md: the agents page, and that the register verbs are host commands and web writes
  alike.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates; **no key, certificate, box path or user name in the
  repository, a log line, an audit record or a fixture**; test credentials minted at test
  time. Check visibility before the push.
- PR body: `Implements:` naming Spec 2.12, 2.13 and 8, the perturbation table, "Refs #18".
