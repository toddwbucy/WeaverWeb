# Brief: the Act 3 follow-ups

**Date:** 2026-11-17. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** issues #136, #138 and #139, and the Planner's decisions of 2026-11-17 on
the Executor's plan of the same day. The act branches from `main` after Act 3 (#129)
merges, and lands before Act 4's code sub-acts.

## Goal

Three small repairs to what Acts 2 and 3 built, each its own sub-act so each closes and
reverts on its own. No new surface and no new Spec section.

## The sub-acts, in order

1. **#136, the flaky fixture.** The test fixture that plants a model pull writes the
   pull row and its file row in one transaction, as `Store::ask` does. Acceptance: the
   test passes 50 runs of 50 alone. The perturbation (the file row outside the
   transaction) is shown by repetition, since the race has no seam to force it.
2. **#139, leaked test databases.** Both parts of the issue, and no signal handler.
   - Each database is named `wwt_<unix seconds>_<16 hex>`, with a `COMMENT ON DATABASE`
     naming the test and the latest migration it ran.
   - Once per process, `fresh_store` drops databases of the new shape that are older
     than six hours, with a plain `DROP DATABASE` and no `FORCE`, so a connected
     database is refused by the server itself.
   - Databases of the old shape never match the sweep.
   - The DB-tests passage in `CLAUDE.md` moves on the operator's word, straight to
     `main`, by the Planner. The act does not edit it.
3. **#138, the blob name.** `RepoFile` carries Hugging Face's LFS flag. `identity()`
   becomes `blob()`, and `in_git()` answers from the flag rather than from a missing
   SHA-256. `PullFile`'s own flag folds in, and round one's mapping in the base
   identity's hasher goes, its test kept. Every site the plan tables reads the new
   methods. Spec section 2.15 already states the blob rule by the kind of file, so it
   conforms and does not move.

## Gates

The repository's own: `fmt`, `build`, both `clippy` runs and the test suite on a scratch
database, `--locked` throughout, on each sub-act's head and on the stack's head. A
perturbation per new test. The suite workflow's PR path: draft sub-PRs into the act's
base, the Planner's grade and Codex per sub-PR, CodeRabbit and Codex on the base, the
operator's merge to `main`.
