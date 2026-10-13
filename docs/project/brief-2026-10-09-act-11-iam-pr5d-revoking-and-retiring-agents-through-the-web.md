# Brief: act 11, IAM, PR 5d: revoking and retiring agents through the web

Version: v0.1, 2026-10-09. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 5c (#36) merged at `f9ed988`. This finishes the register through the web, on
the operator's ruling of 2026-10-09 (everything done with agents belongs in the page): an
admin **revokes one plane's credential** of an agent, or **retires** an agent (both planes
revoked). The host's `revoke` stays beside it. Plain functional page, no styling.

## 1. Read first

- Spec 2.12 (the register, a credential's `live` and `revoked` states, a row retired when no
  credential is live) and 2.13 (the register verbs through the server, the admin's
  four-step order, step four re-checking the authority and every fact step three resolved,
  the shared identity hold).
- `src/link/verbs.rs`'s `revoke` and `src/link/register.rs`'s `revoke_credential`: the host's
  path, and how a revocation closes a live connection in a running server through the
  store's notification channel.
- `src/surfaces/agents.rs` (5c's page, the four steps, the admin variants of the store's
  writes taking the identity exclusion shared from re-check to commit).

## 2. Scope

1. **On the agents page**, for a row with a live credential: **revoke** a plane (gate or
   admin), and **retire** the agent (every live plane revoked in one write).
2. **Each in the four steps**: parse the `ag-` and the plane (400, no record); the first
   gate (one refusal record before any lookup); resolve the agent (unknown 404; a plane
   already revoked 409; a retire of a row with no live credential 409), no record for any;
   the audited write, the admin as principal and the agent as target, under the shared
   identity hold, **re-reading the row inside its transaction** (the plane still live, the
   row still holding one for a retire), then revoking, then the outcome.
3. **The live connection closes in the act**, by the same notification the host's revoke
   sends, so a connector on a revoked plane is cut off at once, not at its next reconnect.
4. **Grants on a retired agent stand**, since a retired row is never live again (5c's rotate
   refuses it) and a new registration is a new `ag-` row; the grants page marks a grant
   whose agent is retired, so an admin can revoke it. Say so in Spec 2.13.
5. **Out of this PR**: the authorization of verbs, turns and reads by grant (PR 6);
   creating or decommissioning the agent on its box (WeaverAgent's provisioning).

## 3. Tests and perturbations

- Revoke one plane end to end: the row's plane revoked, its live connection closed in the
  act, the other plane untouched, the audit pair; the revoked credential refused at the
  next dial.
- Retire end to end: both planes revoked, both connections closed, the row reads retired
  on the page, rotate then refused (5c's rule).
- The four steps: malformed `ag-` or plane 400 with no record; a non-admin refused with
  one record before any lookup, identical whether the agent exists; an unknown agent 404,
  an already revoked plane 409, a retire of a retired row 409, none recorded.
- The re-check: the host revokes the plane while the web revoke is held after step three;
  the web revoke is refused 409, audit failed.
- The grants page marks a grant on a retired agent.
- Perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.12 and 2.13 as built; section 9's principal-check row's revoke clause (owed since
  5c) and any other row this PR enforces move to enforced; count and summary swept.
- CLAUDE.md: the agents page's revoke and retire.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates; no key, certificate, box path or user name anywhere. Check
  visibility before the push.
- PR body: `Implements:` naming Spec 2.12 and 2.13, the perturbation table, "Refs #18".
