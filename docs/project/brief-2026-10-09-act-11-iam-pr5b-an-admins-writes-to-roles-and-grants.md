# Brief: act 11, IAM, PR 5b: an admin's writes to roles and grants

Version: v0.1, 2026-10-09. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 5a (#33) merged at `6d94a7c`. This PR lets an admin grant and revoke roles and
edit a per-agent role's verbs through the admin page, beside the host's `grant` and `role`
commands. 5c (agents through the web) follows. The backend-first ruling stands: a plain
functional page, no styling.

## 1. Read first

- Spec 2.13's role and grant item: a role is a named set of actions; a grant binds a person
  to a role on one agent, or `admin` server-wide; roles and grants are authored rows with a
  version; **no person writes a grant on themselves, granting or removing**; **no person
  writes a role they hold a grant of on any agent**; the last-admin rule; every write under
  the identity exclusion and audited; the console says so wherever more than one person
  holds `turn` on an agent (an agent holds one conversation).
- The rule 4d wrote and 5a used: every write a session authorizes re-checks its authority
  inside its own transaction (here: the session standing and its person holding a live
  admin grant), and every submitted identity is parsed at the boundary before any record.
- `src/store/identity.rs` (insert_grant, grant_is_live, revoke_grant, set_role_verbs,
  admins_remaining_without, the versioned writes) and `src/host.rs` (the host's grant and
  role commands), `src/surfaces/admin.rs` (5a's page and its writes).

## 2. Scope

1. **A page of roles and grants** (`GET /admin/grants`, admin only): every live grant (the
   person, the role, the agent by its register name or server-wide), every role with its
   verbs and version, and the agents of the register to grant on. **Where more than one
   person holds a role carrying `turn` on one agent, the page says so beside that agent**
   (Spec 2.13: their turns interleave in the agent's one conversation).
2. **The writes**, each a POST under the Origin guard, audited (the admin as principal,
   method `session`), each in one identity transaction that first re-checks the authority:
   - **grant**: a person (by `pe-`), a role, and an agent (by `ag-`) or server-wide for
     `admin` only; one live grant per person, role and agent, as the store holds;
   - **revoke** a grant (by `gr-`), carrying the version the page read; the last-admin rule
     refuses removing the last enabled admin's grant;
   - **set a role's verbs** (`observer`, `operator`, never `admin`), carrying the version,
     within the vocabulary of Spec 7.2 plus `turn`;
   - **the self-change rules, checked under the exclusion**: an admin never grants or
     revokes a grant whose grantee is themselves, and never sets the verbs of a role they
     hold a grant of on any agent; each refused before any record. Say on the page why a
     lone admin who needs either goes to the host's commands (Spec 2.13, design section 8).
3. **Out of this PR**: agents through the web (5c); authorization of verbs, turns and reads
   by these grants (PR 6).

## 3. Tests and perturbations

- A non-admin is refused at the page and every write.
- Each write end to end with its audit pair; a stale version refused (two edits of one role
  or one grant, the second held until the first commits).
- The self-change rules: a self-grant, a self-revoke, and a role edit by its holder, each
  refused before any record; perturbation: drop each and it lands.
- The last-admin rule through the revoke; two concurrent revokes of the last two admins'
  grants, one refused.
- The authority re-check: an admin whose grant is revoked by the host between the page and
  the write is refused inside the transaction.
- A malformed `pe-`, `ag-` or `gr-` answers 400 with no audit row.
- The shared-conversation note appears where two people hold `turn` on one agent and not
  otherwise.
- Perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.13 as built; section 9's writer-check row (now roles and grants) and any other row
  this PR enforces move from owed to enforced; count and summary swept.
- CLAUDE.md: the grants page beside the persons page.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates. Check visibility before the push.
- PR body: `Implements:` naming Spec 2.13's items and design section 8, the perturbation
  table, "Refs #18".
