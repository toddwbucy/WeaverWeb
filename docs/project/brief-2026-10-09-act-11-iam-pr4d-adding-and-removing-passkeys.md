# Brief: act 11, IAM, PR 4d: adding and removing passkeys

Version: v0.1, 2026-10-09. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 4c (#31, sign-in) merged at `fe408f3`. This is the last of the passkey pull
requests: a signed-in person manages their own passkeys. It is what makes the design's
recovery work (several passkeys per person, section 7), so a lost device is survivable
without the host.

## 1. Read first

- The design's section 7 (several passkeys, the two ceremonies bound by the one-time add
  grant, the credential ID's uniqueness, removal never of the last), section 6 (the counter
  rule for every assertion, the ceremony table and its cap) and section 10 (the audit
  methods).
- `src/surfaces/sign_in.rs` (the challenged `pk-` identities, the counter rule's call, the
  clone refusal), `src/surfaces/enroll.rs` (a registration and its insert under the
  identity exclusion), `src/passkeys.rs` (the ceremony table).

## 2. Scope

1. **A page of one's own passkeys** (`GET /passkeys`, a session required): each passkey's
   label, when it was added and last used, and which one this session was opened with.
   Nothing of another person's.
2. **Adding, two ceremonies bound by a one-time add grant**, exactly as design section 7
   states:
   - an **authentication** ceremony for the session's person, recording the challenged
     `pk-` identities as sign-in does; its assertion under the counter rule by the
     challenged `pk-`; a possible clone audited and granting nothing;
   - the verified assertion yields **a one-time add grant** in the ceremony table, **bound
     to that session and that person**, expiring within the five-minute window and counting
     toward the cap of 64;
   - a **registration** ceremony that **requires and consumes the grant at its start**
     (excluding the person's existing credentials), so it serves exactly one registration
     from the session that earned it; a parallel session, the person's own included, cannot
     use it;
   - the finish: the library verifies; the passkey is inserted under the identity exclusion
     (a credential ID held by anyone refused at its insert, the grant already consumed, so
     the person begins again); a label given at the add. Audited per design section 10:
     the fresh assertion's records with method `passkey assertion`, the addition's first
     record before the insert and its outcome after.
3. **Removing**: a person removes one of their own passkeys, **never their last**, the count
   taken under the identity exclusion so two concurrent removals of the last two cannot both
   land. Every session opened with the removed passkey ends at its next use (4a's rule), this
   session included if it was opened with it; say so on the page. Audited, method
   `session`.
4. **Out of this PR**: an admin's view or removal of another person's passkeys (PR 5);
   read access by grant and the live view's re-check (PR 6).

## 3. Tests and perturbations

Through the software authenticator:
- the add end to end: a second passkey, then signing in with it;
- a grant used from another session of the same person: refused; a grant used twice:
  refused; a grant past five minutes: refused (paused clock); a registration started with
  no grant: refused;
- a registration that fails (its credential ID already held) consumes the grant, and the
  next attempt needs a fresh assertion;
- a counter refusal at the add's assertion: audited, no grant;
- removal: of the last refused; of another ends its sessions at their next use; two
  concurrent removals of a person's last two passkeys, one held until the other commits:
  one refused;
- the passkeys page lists only the session's person's passkeys;
- perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.13 and 2.8 as built; section 9's credential-ID row and the counter row's addition
  half move from owed to enforced, with the add-grant and removal rows; count and summary
  swept.
- CLAUDE.md: the passkeys page.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates; no bearer, digest or credential in a log line. Check
  visibility before the push.
- PR body: `Implements:` naming design sections 6, 7 and 10 and the Spec sections, the
  perturbation table, "Refs #18".
