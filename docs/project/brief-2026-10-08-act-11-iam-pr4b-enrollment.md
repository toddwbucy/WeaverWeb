# Brief: act 11, IAM, PR 4b: enrollment by token

Version: v0.1, 2026-10-08. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 4a (#29) merged at `09b73c6`. The passkey work is split once more, one concern
each:

- **4b (this one): enrollment.** The library, the ceremony table, the browser module, and
  a token's redemption registering a person's first passkey.
- 4c: sign-in. The assertion, the counter rule, the session's opening.
- 4d: adding and removing passkeys.

Epic #18 carries the split.

## 1. Read first

- The design's section 2 (the library and what OpenSSL costs), 4 (the browser half), 6
  (ceremony state, the cap, the bearer rule) and 7 (the token, its lifetime and redemption).
- Spec 2.13's person item, in particular: **a valid token authenticates the person it is
  bound to, for that one write and nothing else**. So redeeming a token registers a passkey
  and opens no session; the person signs in afterwards (4c).
- `src/store/identity.rs` (tokens, persons, the identity exclusion) and
  `src/surfaces/gate.rs` (the session, the Origin guard).

## 2. Scope

1. **webauthn-rs 0.5 in the server binary alone.** The connectors run on agent boxes and
   must not link `libcrypto`. A cargo feature (`passkeys`) carries the dependency, and the
   server binary requires it. **Measure, do not assume**, how the connector binaries link
   under each build (`cargo build` of the whole package with the feature on, and the bins
   built separately): `ldd` on gate-con and admin-con. State the build commands that
   guarantee connectors without `libcrypto`, and put them in CLAUDE.md. If the only
   guarantee is building the connectors in their own invocation without the feature, say so
   plainly; the install act will follow it. Section 9's library row is this measurement.
2. **The ceremony table**, in the server's memory: keyed by a ceremony identity (a bearer:
   32 bytes from the OS random source), holding the library's state, **at most 64 in
   flight** (a new one past the cap refused), **expiring after five minutes**, and **used
   once**. A restart drops what is in flight.
3. **The browser module**: one hand-written ES module under 4 KiB, vendored by
   `include_bytes!` and served from the surfaces' own asset route (`/assets/surfaces/...`,
   beside `instrument.css`; the legacy `/assets/{file}` route is web/'s and leaves with it).
   It sends exactly the registration members design section 4 lists, nothing else. No inline
   script. Measure whether the surfaces can carry `Content-Security-Policy: script-src
   'self'` against htmx's own use, and adopt it if they can; say which.
4. **Redemption**, two endpoints and a page:
   - the page asks for the token (pasted, never in a URL, so it lands in no log or
     history);
   - **options**: the token is checked (unexpired, unused, its person enabled and holding
     no passkey), a registration ceremony starts, and its options are answered;
   - **finish**: the library verifies the registration; then, **in one transaction under the
     identity exclusion**, the token is checked again, the passkey row is inserted (its
     credential ID unique, the library's serialized credential, a `pk-` identity), and the
     token ends as `redeemed`. Audited with method `enrollment token`, first record before
     the write.
   - The answer sends the person to sign in. No session is opened.
5. **Out of this PR**: sign-in, the counter rule, opening a session (4c); adding or
   removing passkeys (4d); the live view's re-check and read access (PR 6).

## 3. Tests and perturbations

- **A software authenticator for the ceremonies.** Measure `webauthn-authenticator-rs`'s
  soft passkey (or the smallest equivalent) as a dev-dependency, its weight and whether it
  pulls anything into the non-test build; or build the registration by hand if that is
  smaller. Say which and why.
- Redemption end to end: a token registers a passkey, the token ends `redeemed`, no session
  cookie is set, the audit has the two records.
- Refused: an expired token, a used one, a token whose person is disabled, a token for a
  person who already holds a passkey (and the race: the passkey appearing between options
  and finish is refused at the transaction's re-check), a credential ID already held.
- The ceremony table: the 65th ceremony refused; a ceremony past five minutes refused (a
  paused clock); a ceremony identity used twice refused the second time.
- The module: served with the right type, under 4 KiB; no inline script on the page.
- `ldd`: the connectors as the stated build produces them carry no `libcrypto`.
- Perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.13 and 2.8 where enrollment is described; section 9's rows this PR enforces (the
  token's redemption, the library's binary, the ceremony cap, the bearer randomness for the
  ceremony identity) move from owed to enforced, the count and summary swept.
- CLAUDE.md: the feature, the build commands, the enrollment page.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` (with and without the feature), `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates; no host name, rp_id or origin beyond `localhost` and the
  reserved test names; no token in any log line. Check visibility before the push.
- PR body: `Implements:` naming design sections 2, 4, 6 and 7 and the Spec sections, the
  measurements (ldd, the test authenticator, the CSP), the perturbation table, "Refs #18".
