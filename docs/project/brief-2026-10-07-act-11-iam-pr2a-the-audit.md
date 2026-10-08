# Brief: act 11, IAM, PR 2a: the audit

Version: v0.1, 2026-10-07. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. The design merged as PR #24 (`docs/project/design-2026-10-07-iam.md`). Its plan's
PR 2 holds two concerns, so under the operator's small-PRs ruling it is split:

- **PR 2a (this one): the audit.** The append-only table, its triggers, the two-record
  writer, and its first consumers: the register verbs that already exist, which Spec 2.13
  says are audited as the host's writes and today are not.
- **PR 2b (next): persons and the host's identity commands.** Persons, passkeys, tokens,
  roles and grants, and the host commands that write them, each audited through this PR's
  writer.

Epic #18 carries the plan with the split.

## 1. Read first

- The design's section 10 (the audit record, the triggers and what they guard) and
  section 5 (the threat model's out-of-scope list).
- Spec 2.13's audit item and its register-verbs item (each verb's target and action), and
  section 9's audit rows (owed today).
- `src/bin/weaver-web.rs` and `src/link/verbs.rs`: the five register verbs (`authority
  init`, `authority rotate`, `register`, `revoke`, `rotate`) and their `--author` claims.
  `authority init` today runs without the store.

## 2. Scope

1. **Migration `0014`**: the `audit` table with the members design section 10 lists, its
   identity under the convention Spec section 2 opens with, and two triggers: a row trigger
   refusing `UPDATE` and `DELETE`, and a statement trigger `BEFORE TRUNCATE`.
   - The principal is a kind (`person`, `server`, `host`) plus the person's identity where
     the kind is `person`. **No foreign key to a persons table, since none exists yet**;
     PR 2b's migration adds it. A CHECK ties principal and method as the design states
     (each method belongs to exactly one principal).
   - Parked PR #23 also numbered a migration `0014`; it never reached main, and it
     renumbers when it resumes (noted on #23). Main's next is `0014`.
2. **The writer**, one module in the store: a first record written before the act, which
   returns its identity; an outcome record naming its first; and a refusal as one record
   carrying its refusal. Nothing else in the crate inserts into `audit`.
3. **The register verbs audited as the host's**, each with its target and action as Spec
   2.13 gives them, and the `--author` claim recorded as the host's unverified claim:
   - **the first record is written before the verb acts, and a verb whose first record
     cannot be written does not act** (it answers the store's error, exit 1);
   - the outcome record is written when the verb's act commits or fails;
   - **`authority init` gains the store**: it connects, writes its first record, writes
     the authority, then its outcome. Where the store cannot be reached it refuses before
     writing anything. Say this in CLAUDE.md's command list, since it changes the verb's
     requirements.
4. **Out of this PR**: persons and everything that names one; the server principal's
   audited `show` (PR 6, with every ask); any surface reading the audit; the provisioning
   hardening of a separate owner role (an operator decision outside the act).

## 3. Rules to hold

- **The audit never holds** a credential, a key, a fingerprint's private half, a bearer or
  a token, per design section 10. A register verb's record names the agent's row and the
  action; the minted credentials go only to the client configs, as today.
- The triggers guard this crate's code paths and nothing more; say so in the migration's
  comment, in the design's words.
- One writer module, so "every act audited" is checkable by reading one call graph: each
  register verb's act sits between its first record and its outcome.

## 4. Tests and perturbations

Shown failing with the guard removed, run in your scratch worktree of the pushed commit:
- drop the row trigger, and an `UPDATE` rewrites a record; and a `DELETE` removes one;
- drop the truncate trigger, and a `TRUNCATE` empties the table;
- each of the five register verbs writes a first record before its act and an outcome
  naming it (one test per verb, or one table-driven test): remove the first record from a
  verb, and its act lands unaudited;
- a verb whose first record cannot be written does not act (a store refusing the insert,
  e.g. a test-only CHECK violation, or the table renamed in a scratch schema): remove the
  ordering, and the act lands without its record;
- the principal and method CHECK: insert a `host` principal with method `session`, and it
  is refused.

## 5. Spec and documents

- Section 9: the audit rows this PR enforces move from owed to enforced, naming their
  instruments; update the owed count and its summary in the same commit, and grep the Spec
  for any other count that moves.
- Spec 2.13: nothing changes in substance; where it says the register verbs are audited
  "today", it is now true. Note `authority init` needing the store.
- CLAUDE.md: the command list says `authority init` needs the store.

## 6. Acceptance

- `cargo build --locked`, `cargo test --locked` with `DATABASE_URL` set, `cargo clippy
  --all-targets --locked -- -D warnings`, `cargo fmt --check`; the perturbations run on the
  pushed commit in a scratch worktree.
- ASCII only, absolute dates. No credential, path or host name in any record or test
  fixture. Check visibility before the push.
- PR body: `Implements:` lines naming design section 10 and Spec 2.13's audit and
  register-verb items, the perturbation table, and "Refs #18".
