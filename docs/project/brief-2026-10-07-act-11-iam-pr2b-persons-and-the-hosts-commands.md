# Brief: act 11, IAM, PR 2b: persons, and the host's identity commands

Version: v0.1, 2026-10-07. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 2a (the audit) merged as #25 at `ff09684`. This PR lands the identity rows and
the host commands that write them, each audited through 2a's writer. Nothing in a browser:
signing in, enrolling a passkey and redeeming a token are PR 4's; an admin's writes through
a surface are PR 5's.

## 1. Read first

- The design `docs/project/design-2026-10-07-iam.md`: section 6 (the bearer rule, the
  name's canonical form), 7 (the token's lifetime, the host reset), 8 (the host's grants
  and role writes, the self-change rule), 10 (the passkey table, the identity exclusion).
- Spec 2.13: the person item (enrollment tokens, disable, the host principal), the role
  and grant item (the seeded roles, the fixed admin role, the exclusion, the last-admin
  rule), and section 9's rows owed to this PR.
- `src/store/audit.rs` and the register verbs in `src/link/verbs.rs`: the pattern each new
  host command follows (first record before any mutation, outcome after, answer as one JSON
  object with the exit status agreeing).

## 2. Scope

1. **Migration `0015`** (0014 is frozen):
   - `person`: its identity (`pe-` under section 2's convention), its display name as given
     (trimmed), its canonical name **unique** (the compatibility caseless match of design
     section 6), enabled, when it was enrolled.
   - `passkey`: per design section 10, the credential ID unique across the table. Only
     the host reset writes it in this PR (to clear); PR 4 adds the rest.
   - `enrollment_token`: bound to one person, its digest only, its expiry, used or not.
   - `role`: `admin` fixed and seeded (not an authored row; nothing writes it); `observer`
     and `operator` seeded with design section 1's verbs, authored rows carrying the
     author member and the version of Spec 3.2.
   - `grant`: a person, a role, and an agent of the register, or server-wide for `admin`
     only; an authored row with author and version.
   - **The audit's person foreign key**, which 2a left out because no `person` table
     existed: added here by `ALTER TABLE`.
   - Every CHECK false, never unknown, on a missing value (2a's rule).
2. **The identity exclusion**: one transaction-level advisory lock under a class of its own
   (design section 10), taken exclusively by every identity write in this PR. The shared
   form is PR 6's.
3. **The host commands**, each a subcommand answering one JSON object, each audited as the
   host's with its `--author` claim, each under the exclusion, and each writing its first
   record before any mutation:
   - **bootstrap** a person: the person row, the server-wide `admin` grant and an enrollment
     token, in one transaction; the token printed once in the answer and never again.
     The host can always write a bootstrap admin grant (Spec 2.13), so it works on a store
     that already has admins.
   - **issue a token** for a person **holding no passkey**, refused otherwise.
   - **reset** a person (design section 7): clears their passkeys and issues a token, in
     one write; writes no session.
   - **grant** and **revoke** a role to a person on an agent, or `admin` server-wide
     (design section 8). **The last-admin rule binds the host too**: revoking the last
     enabled admin's grant is refused. The self-change rules do not reach the host, which
     is not a person.
   - **set a role's verbs** (design section 8), within the vocabulary of Spec 7.2 plus
     `turn`; refused for `admin`, which nothing writes.
   Name the subcommands to sit beside the register verbs; say in the PR body what you
   chose.
4. **The bearers**: every token is 32 bytes from the operating system's random source
   (design section 6's rule), stored as its digest, its lifetime from config (24 hours by
   default, refused at start or at issue above seven days).
5. **Out of this PR**: redeeming a token, passkeys' enrollment and sign-in, sessions'
   new columns (PR 4); disabling and renaming a person, and every admin write through a
   surface (PR 5); authorization of asks (PR 6).

## 3. What to measure

- **The canonical form's crate** (design section 6: NFKD(casefold(NFKD(casefold(NFD(x)))))
  on the trimmed name): which crates compute it, their weight in our tree, and whether
  the case folding is Unicode's full folding. Quote the evidence.
- **The random source**: `getrandom` directly or `rand`'s `OsRng`, both already in the
  lockfile; pick one and say why.

## 4. Tests and perturbations

Each shown failing with its guard removed, in the scratch worktree of the pushed commit:
- two persons whose names differ only by case, width or composition: the second refused;
- a token for a person holding a passkey: refused; a token past seven days' configured
  lifetime: refused; a token stored as its digest and never in the clear;
- a token drawn from a counter or a time, and the next is guessed (the bearer row);
- bootstrap, grant, revoke, reset and the role write each write their first record before
  any mutation and an outcome naming it; a refused first record leaves the store as it was;
- revoking the last enabled admin: refused; two concurrent revocations of the last two
  admins: one refused (the exclusion), shown with a hold like the ingest's;
- the `admin` role's verbs cannot be set; a role's verbs outside the vocabulary are refused;
- every new CHECK refuses its member missing (2a's table-driven shape);
- the audit's person foreign key refuses a record naming no person.

## 5. Spec and documents

- Section 9: the rows this PR enforces move from owed to enforced with their instruments;
  update the owed count and its summary, and grep for any other count that moves.
- CLAUDE.md: the new host commands in the command list, beside the register verbs.

## 6. Acceptance

- `cargo build --locked`, `cargo test --locked` with `DATABASE_URL` set, `cargo clippy
  --all-targets --locked -- -D warnings`, `cargo fmt --check`; perturbations in the scratch
  worktree of the pushed commit.
- ASCII only, absolute dates. No token, digest, name or path of a real person in a test or
  fixture. Check visibility before the push.
- PR body: `Implements:` lines naming design sections 6, 7, 8 and 10 and Spec 2.13's items,
  the perturbation table, the two measurements, and "Refs #18".
