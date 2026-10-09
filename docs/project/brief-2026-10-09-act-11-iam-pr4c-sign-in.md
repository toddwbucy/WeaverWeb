# Brief: act 11, IAM, PR 4c: sign-in

Version: v0.1, 2026-10-09. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 4b (#30, enrollment) merged at `38d1850`. This PR lets a person who has
enrolled a passkey sign in, and opens the first session a person can actually hold. It is
the second of the three passkey pull requests; 4d (adding and removing passkeys) follows.

## 1. Read first

- The design's section 6: name-first sign-in, the signature counter (the locked read, merge
  and write, in its own transaction committed before the session's opening), what is
  audited and what is not, the bearer rule.
- The design's section 10: the audit's `passkey assertion` method.
- Spec 2.8 and 2.13 as 4a and 4b left them; `src/passkeys.rs` (the ceremony table),
  `src/surfaces/enroll.rs` (the pattern of a page, options and finish),
  `src/surfaces/gate.rs` (the session and its cookie).

## 2. Scope

1. **The page and two endpoints**, beside enrollment's: `GET /sign-in`, `POST
   /sign-in/options`, `POST /sign-in/finish`, under the Origin guard and the script policy.
   The browser module gains the authentication ceremony, sending exactly design section 4's
   members.
2. **Options**: the name is looked up by its canonical form; the person must be enabled and
   hold at least one passkey; an authentication ceremony starts with that person's passkeys.
   Name-first answers whether a name exists, which the design accepts and states; keep the
   answer for a disabled person or one with no passkey the same as for an unknown name.
3. **Finish**: the library verifies the assertion. A failed signature is refused and not
   audited (it proves no principal). Then **the counter rule**, exactly as design section 6
   states it: one transaction of its own that reads the stored passkey under `FOR UPDATE`,
   refuses a nonzero returned counter not greater than that fresh copy's, otherwise applies
   `update_credential` and writes it back, always where the returned counter is zero, and
   commits **before** the session's opening. A `CredentialPossibleCompromise` (from the
   library or from the locked read) is audited as a possible cloned credential, principal
   the passkey's person, method `passkey assertion`, and opens nothing; the passkey is not
   disabled.
4. **The session's opening**: in its own transaction after the counter's, with the person
   re-checked as enabled and the passkey as standing: a bearer of 32 bytes from the OS
   random source, stored as its digest; the session row naming the person and the
   passkey's `pk-` identity; the `__Host-` cookie set. Audited as the session's opening,
   method `passkey assertion`, first record before the write.
5. **Record and the other surfaces**: where there is no session, link to `/sign-in`.
6. **Out of this PR**: adding and removing passkeys (4d); read access by grant and the live
   view's re-check (PR 6).

## 3. Tests and perturbations

Through the software authenticator 4b chose:
- end to end: enroll, sign in, a session row and cookie, the session serves Record, the
  audit's opening record;
- refused, nothing opened: an unknown name, a disabled person, a person with no passkey
  (the same answer for each), a failed signature (not audited), a ceremony used twice;
- **the counter rule**: a stored counter set above what the authenticator returns is
  refused and audited as a possible clone, the passkey still standing; two concurrent
  assertions of one nonzero-counter passkey, the second held until the first commits, the
  second refused; a zero-counter assertion pair both succeed (drive the stored and returned
  counters to zero as the test needs); an assertion that upgrades backup eligibility is not
  erased by a concurrent one;
- the counter persisted even where the session's opening then fails;
- the session bearer's randomness (section 9's last bearer row);
- perturbations for each guard, in the scratch worktree of the pushed commit.

## 4. Spec and documents

- Spec 2.8 and 2.13: sign-in as built; section 9's counter, bearer and sign-in rows move
  from owed to enforced; count and summary swept.
- CLAUDE.md: the sign-in page, and the sentence that sessions are opened by tests alone
  goes.

## 5. Acceptance

- The full suite with `DATABASE_URL` before every push, `cargo clippy --all-targets
  --locked -- -D warnings` with and without the feature, `cargo fmt --check`,
  `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates; no bearer, digest or name of a real person in a log line or
  fixture. Check visibility before the push.
- PR body: `Implements:` naming design sections 6 and 10 and the Spec sections, the
  perturbation table, "Refs #18".
