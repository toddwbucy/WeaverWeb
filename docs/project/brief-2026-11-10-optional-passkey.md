# Brief 2026-11-10: the optional passkey

From the suite Planner, 2026-11-10. A new act, `act/optional-passkey`: make the passkey
optional for sign-in at this stage, on the operator's ruling. This is now the blocker for
the lifecycle round trip. The brief arrived as a message; it is carried here as the
stack's first commit, with the database role in the test command replaced by `<role>`,
since an account name does not enter the repository.

## The ruling

The operator, 2026-11-10, verbatim: "the passkey should be optional not manditory at this
stage. we can always tighten this stuff up before release". Earlier the same evening: "I
already have way too many ways to log into way to many ... services ai WILL NOT STAND TO
ADD ANOTHER." Read with his standing statement that WeaverWeb is "the operator's console at
the same abstraction level as a Proxmox console, and never faces the public". Write that
reasoning into the Spec in full; the repository being public is not a reason to leave
deployment posture out.

## What was measured

On the operator's instance, served with TLS on a LAN address; its config and certificates
are outside every repository and are the Planner's, and neither they nor the running unit
are touched by this act.

- Enrollment stored a passkey server side three times (a password manager extension
  answered `create`), and no sign-in ever completed: the browser first refused WebAuthn on
  an untrusted certificate, then with trust fixed fell through to its cross-device QR
  prompt because the manager never offered the credential. Tried under a single-label name
  and a dotted name, with a non-default port. `last_used_at` is null on every passkey row.
- `GET /` returns 404; the operator expected it to land on sign-in.
- A person can only get a session through `gate::open`, which requires a standing passkey;
  `session.passkey_id` is NOT NULL. There is no other sign-in path.

## What is asked

1. **Spec first, same act.** Amend the sign-in sections (2.13, design sections 3, 6, 7 and
   wherever "passkey" is stated as the only way in) so that the passkey is one of two
   sign-in methods, selected by server configuration, with the passkey still the default
   when nothing is configured. State plainly that the second method is a pre-release
   posture the operator will tighten before release, dated, with his words.
2. **The second method**, the Planner's proposed shape (the report says so if the Spec or
   code makes a better one obvious, without stopping to ask): a host-issued sign-in token,
   the same trust root as `person bootstrap` (whoever has a shell on the host). A host verb
   prints a one-time, short-lived token for a named enabled person; the sign-in page, when
   the configuration permits it, takes the person's name and that token and opens an
   ordinary session. No password is stored, nothing new for the operator to remember.
   Everything else stays: the origin check, the `__Host-` cookie, idle and absolute limits,
   grants, audit. The session and the audit record say which method opened it; the
   migration that lets a session stand without a passkey is part of the act. With the
   configuration absent or set to passkey-only, behavior is byte-for-byte today's, and the
   token verb refuses.
3. **`GET /` redirects**: to sign-in when there is no session, to the signed-in landing
   page when there is.
4. **Draft, do not file, the text of one issue**: "tighten sign-in before release" (what
   the second method is, why it exists, what release needs to decide, including the default
   port and bring-your-own browser certificate questions). The draft goes in the report;
   the operator files issues.

## Constraints

- WeaverWeb `CLAUDE.md` and the suite workflow bind this: branch `act/optional-passkey`, a
  stack of draft sub-PRs (Spec, then code), `Implements:` with a verdict per section, this
  brief committed as the workflow says, ASCII, no em-dashes, absolute dates, no credential
  or box path or instance detail in the repository (no hostnames, no LAN addresses, no
  password manager product names; say "a password manager extension").
- Tests with `PGUSER=<role> DATABASE_URL='postgres:///<scratch db>?host=/run/postgresql'
  cargo test --locked -- --test-threads=8`; the baseline on `main` 875b44d4 is 438 passed,
  0 failed, 1 ignored. Any new test that waits on a notify takes a timeout, the shape at
  `src/keys_tests.rs:508`. A private `CARGO_TARGET_DIR` outside the home directory.
- Push nothing until the Planner says so. One push per round. Do not merge. Do not create
  or restart any service.
- When the code is green locally, build `weaver-web` release from the stack head into the
  Executor's scratch target and give the Planner the path and the commit; the Planner
  installs it into the instance for the live check before the operator undrafts.

## The report back

Hashes from git output, the test counts, the Spec sections changed with one line each,
anything found that contradicts this brief, and the issue draft.
