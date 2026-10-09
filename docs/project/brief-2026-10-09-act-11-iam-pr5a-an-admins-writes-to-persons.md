# Brief: act 11, IAM, PR 5a: an admin's writes to persons

Version: v0.1, 2026-10-09. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 4d (#32) merged at `a95dad6`, which finishes the passkey work. The design's
PR 5 (an admin's writes through a surface) is split in two, one concern each:

- **5a (this one): persons.** An admin enrolls a person, issues a token, disables and
  re-enables, and renames, through a page.
- 5b: roles and grants. An admin grants and revokes roles and edits a role's verbs.

**The operator's ruling of 2026-10-09 stands over both: the backend is finished before any
frontend presentation.** The admin page is a plain functional shell in the style of the
passkeys page: no styling work, no design applied.

## 1. Read first

- Spec 2.13's person item: who writes a person row (an admin enrolls, disables and renames;
  a person writes only their own authentication material, **never their own name, state or
  grants**; the host writes the bootstrap), enrollment tokens (single-use, expiring,
  digest-only, only for a person holding no passkey, revoked by a disable in the same
  write), the identity exclusion, the last-admin rule, and the audit.
- Spec 2.13's rule from 4d: **every write a session authorizes re-checks its authority
  inside its own transaction** under the identity exclusion. For an admin's write the
  authority is the session standing **and its person still holding a live admin grant**.
- `src/store/identity.rs` and `src/host.rs` (the host's versions of these writes, whose
  store functions this PR reuses with the person's rules added), `src/surfaces/keys.rs`
  (a session-gated page and its writes).

## 2. Scope

1. **An admin page** (`GET /admin/persons`, a session whose person holds a live admin
   grant, refused otherwise): every person, enabled or not, whether they hold a passkey,
   whether a token is outstanding. Nothing of an agent.
2. **The writes**, each a POST under the Origin guard, each audited with the admin as
   principal and method `session` (first record before the write, outcome after), each
   inside one identity transaction that re-checks the authority first:
   - **enroll a person**: a new person row (the canonical name unique, the identity shape
     refused, as the host's bootstrap does) and a token, **shown once** in the answer
     (`Cache-Control: no-store`), never in a URL or a log;
   - **issue a token** to a person holding no passkey (refused otherwise, as the host's);
   - **disable** a person: their outstanding tokens revoked in the same write, refused
     where it would leave no enabled person holding a live admin grant;
   - **enable** a disabled person (say whether the Spec names it; if not, add one sentence:
     an admin's write, under the same rules);
   - **rename** a person: the canonical name unique, the identity shape refused.
   - **An admin never writes their own row** (Spec 2.13: never their own name or state): a
     self-disable or self-rename is refused before any record.
3. **Out of this PR**: roles and grants (5b); the register verbs through the server are 5c's;
   read access by grant (PR 6).

## 3. Tests and perturbations

- A session without a live admin grant is refused at the page and at every write.
- Each write end to end with its audit pair; the token shown once and never logged.
- Refused: a taken name (case and width variants), an identity-shaped name, a token for a
  person holding a passkey, the last admin's disable, an admin's write to their own row.
- **The authority re-check**: an admin whose grant is revoked (by the host) between the
  page and the write is refused inside the transaction; a hold like 4d's shows it.
- Two concurrent disables of the last two admins: one refused (the exclusion).
- A disable revokes the outstanding token in the same write.
- Perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.13 as built; section 9's rows this PR enforces move from owed to enforced; count
  and summary swept.
- CLAUDE.md: the admin page, and that the host commands remain beside it.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates; no token, digest or name of a real person in a log line or
  fixture. Check visibility before the push.
- PR body: `Implements:` naming Spec 2.13's items, the perturbation table, "Refs #18".
