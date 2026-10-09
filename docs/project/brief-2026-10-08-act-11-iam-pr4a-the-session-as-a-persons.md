# Brief: act 11, IAM, PR 4a: the session as a person's

Version: v0.1, 2026-10-08. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 3 (TLS, #27) and the A3.2 alignment (#28) merged; main is at `b253bbc`. The
design's PR 4 (passkeys) is split into three, one concern each:

- **4a (this one): the session as a person's.** What a session is once identity stands,
  with no WebAuthn in it yet.
- 4b: sign-in. webauthn-rs, the ceremonies, the vendored module, a token's redemption
  enrolling the first passkey, name-first sign-in.
- 4c: adding and removing passkeys.

Epic #18 carries the split. **The operator ruled on 2026-10-08 that no deployment holds
data and the migrations are squashed at the install act**, so this PR's migration needs no
guard for rows that cannot exist: no backfill, no "only where untouched".

## 1. Read first

- The design's section 6 (the cookie, the row, lifetime, what ends a session, the
  last-used refresh, cross-site requests, the claimed-name session's retirement) and
  section 3's two relying-party refusals.
- Spec 2.8 (the session), 3.2 (the author member) and 2.13's person item.
- `src/surfaces/gate.rs` (today's claim) and `migrations/0008_the_session.sql`. **Nothing
  in production opens a session today**: the one insert is the Record surface's test
  helper. So retiring the claimed-name session breaks nothing that runs.

## 2. Scope

1. **`rp_id` in the config** and the design's two refusals that are its: no `rp_id` where
   an `origin` is configured (and the reverse), an `rp_id` that is an IP address or empty,
   and an origin whose host is neither the `rp_id` nor under it. The rest of section 3's
   list stands from PR 3.
2. **Migration `0017`: the session as a person's.** The session row carries the person,
   the passkey it was opened with (by credential ID), the bearer's digest, when it opened,
   when it was last used and when it closed. The claimed name and the configured role go.
   With no data, the table may simply be rebuilt.
3. **The cookie** `__Host-weaver_session`: `Secure`, `HttpOnly`, `SameSite=Strict`,
   `Path=/`, no `Domain`. On a plain `http://localhost` origin a browser will not keep a
   `Secure` cookie; say what you do there (the planner's lean: the `__Host-` cookie
   everywhere, since browsers treat `http://localhost` as secure for `Secure` cookies;
   measure it rather than assume).
4. **A session's checks at every use**: closed, idle past its limit (1 hour by default),
   past its absolute limit (12 hours by default), its person disabled, or its passkey
   removed, each ending it for this request (and closing the row, by the surface, the
   session's one writer). Both limits are config with those defaults.
5. **The last-used refresh**: one conditional update on the database's clock, at most once
   a minute, per design section 6 and Spec 3's writer model.
6. **The `Origin` check**: every state-changing request (every method but GET and HEAD)
   carries an `Origin` header equal to the configured `origin`, or it is refused before its
   handler. With no `origin` configured, say what holds (the planner's lean: a
   state-changing request is refused, since nothing can sign in without an origin).
7. **Sign-out**: a POST that closes the session's row and clears the cookie.
8. **The author member**: an authored row's author takes the session's person identity
   (`pe-`), per design section 6 and Spec 3.2. Surfaces that render an author resolve the
   person's current name; a row written before carries its old claim and renders as a
   claim.
9. **Out of this PR**: every ceremony, webauthn-rs, the vendored module and the asset
   route (4b); adding and removing passkeys (4c); the live view's 15-second re-check and
   read access by grant (PR 6).

## 3. Tests and perturbations

In the scratch worktree of the pushed commit, sessions opened directly in the store:
- the rp_id refusals, each its own case;
- the cookie's attributes on the response that sets it, and a request carrying only the
  old `weaver_session` name finds no session;
- each end at use: closed, idle, absolute, person disabled, passkey removed; perturbation:
  drop each check and that session is served;
- the last-used refresh: two requests within a minute write once; a request a minute later
  writes again; it never moves backwards;
- the `Origin` check: a POST with a foreign origin, with none, and with the configured one;
  a GET with none is served;
- sign-out closes the row and the next request with that cookie finds no session;
- an authored row written in a session names its person, and renders their current name
  after a rename (rename through the store, since the surface is PR 5's).

## 4. Spec and documents

- Spec 2.8 rewritten to the session as built (its claim assertion retires, as the section
  9 table already marks); 3.2's author; the section 9 rows this PR enforces move from owed
  to enforced, the owed count and summary swept.
- CLAUDE.md: the session's description in "The seed tree" (the claimed name goes), and the
  config's `rp_id`, idle and absolute keys.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings`, `cargo fmt --check`, `tests/startup.rs --ignored`;
  perturbations in the scratch worktree.
- ASCII only, absolute dates; no host name, rp_id or origin in the repository beyond
  `localhost` and the reserved test names. Check visibility before the push.
- PR body: `Implements:` naming design section 6 and Spec 2.8, 3.2 and 2.13, the
  perturbation table, "Refs #18".
