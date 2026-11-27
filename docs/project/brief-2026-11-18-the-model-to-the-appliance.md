# Brief: Weaver Web moves the model to the appliance

**Date:** 2026-11-18. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the operator's rulings of 2026-11-18 on the model's move to the
appliance (R1, the puller is admin-con, and R2, the setup connection as built), and the
operator's word of the same day that the appliance never contacts a model source. The
act is one stack of four sub-acts from `main`, branched once the act that makes the
adapter optional (#165) merges.

## Goal

The console pulls a picked model from Hugging Face into its cache and verifies it (Spec
2.15, built). A box reads each file's path, blob, size and SHA-256 from `setup describe`
(Spec 8.3, built): the decoder's files and the adapter's, or the decoder's alone where
no adapter is picked. Nothing moves the bytes from the cache to the box's models drive,
and the link's frames are single lines bounded at 4 MiB, so a model cannot cross as a
frame. This act builds the third setup ask, `admin-con setup pull --into <dir>`: Weaver
Web moves the model to the appliance, which contacts no model source. The command fills
the directory the box names with the picks' files in Hugging Face's layout, checks each
file against `describe`'s SHA-256, and resumes after a cut. No migration.

## The sub-acts, in order

1. **The Spec.** Section 8.3 states that the appliance contacts no model source, and the
   third ask: its frame, its refusals, its bounds, the box's six steps, its lines and
   statuses. 2.15's sentence on the box's side, 6.2's models drive warning, section
   10's transfer cell (closed), its setup-link cell (narrowed to the three asks
   upstream) and its cell on the laying's three members (closed by WeaverAgent #582),
   and section 9's rows, owed to the code, with the owed count. The prose says Weaver
   Web moves the model to the appliance. `setup pull` keeps its name, and no passage
   says the box downloads. The text reads against the optional adapter's, as merged:
   - the Models row keeps the adapter's pane and its Drop, and the roster's words
   - 8.3's paragraph on `describe`'s paths keeps the base-only laying's sentence
   - the laying's cell keeps the four adapter options all or none at `be56d8c2`
   - section 9 keeps the optional adapter's four rows before this act's owed ones, and
     the `not_picked` row's instrument names a dropped adapter's blob beside another
     setup's.
2. **The console's side.** The body opens on the seam: raw bytes on a stream the setup
   connection's frame writer owns, with a test that no frame is written into a
   transfer's middle and that a transfer's bytes are never read as a frame. Then the
   `pull` ask, the store's check of a blob against the linked setup's verified picks,
   the serving line and the raw bytes, the bound on transfers in flight, the log line,
   and the section 9 rows for the console with their perturbations. The page strings
   that still say "the box's pull" move to the Spec's words. A small fix in a commit of
   its own closes #170: every socket a test binds in a test directory stays under the
   `sun_path` bound for a temporary root as long as the bound allowed before #166, and
   the test binds both, the fake relay's and the fake gate's, under an 85-byte root, one
   byte short of that old bound.
3. **admin-con setup pull.** The six steps, the retries, the lines and statuses, and the
   section 9 rows for the box. It shares the cache's layout and the hasher with
   `src/hugging_face/cache.rs` and builds with no default features.
4. **End to end.** The optional adapter's brief numbers the setup's end-to-end tests E1
   to E5, so these are E6 to E9:
   - E6, a pull fills the directory.
   - E7, a cut transfer resumes, and a flipped byte fails the digest.
   - E8, the refusals. Among them is an adapter dropped through the models page's
     Drop, whose blob stays in the cache and is refused `not_picked`.
   - E9, the built binary against the in-process listener, if an integration test can
     stand the listener up, else dropped with the reason.

   E6 and E9 each run with an adapter picked and with none. A setup with no adapter
   moves the decoder's files alone, set up as E5 sets it up. Full gates in a fresh
   target, and the close-out.

## Ruled out

- No new ALPN and no new connection: the setup connection as built carries the ask.
- No store row and no page for what a box was served. The box's digest check is the
  proof, and the console logs one line per transfer.
- No privileged act on the box, and no write outside `--into`.
- No change to the adapter's being optional, its drop or the roster's wording, which
  the act before this one made (#165).

## Shown running

Each code sub-act leaves draft only when shown running: E6 to E8 through the real
listener and store, with real files in a temporary cache and a temporary `--into`, and
E9 through the built binary where it stands. The setup's five end-to-end tests and the
ten dropped-request tests run again with each code sub-act, since the act adds a stream
to the setup connection.
Each body says what ran end to end and what has no path: a real appliance's pull into
its models drive and the agent's load of the pulled model, until
`toddwbucy/WeaverTools#39`, `toddwbucy/WeaverAgent#615` and `toddwbucy/WeaverAgent#616`
are answered.

## Gates

The repository's own, on each sub-act's head and on the stack's head, `--locked`
throughout, each worktree with a target of its own:

- `fmt` and `build`
- both `clippy` runs
- the suite at eight threads, with `DATABASE_URL` and `PGUSER` set, the whole run under
  `setpriv --no-new-privs`, with `NoNewPrivs: 1` logged from inside it
- the journal: every `sudo` `COMMAND=` line in the suite's window fails the gate, one
  from the worktree or a test's temporary folder included. Lines from another seat's
  counted runs are set aside by their working directory and counted beside the rest.
- `/tmp`: nothing of the suite's left behind.

Each body's gate line names the suite's window, the journal's count and what it set
aside. Each new test has a perturbation, run under the same flag and restored from a
scratchpad copy checked by `cmp`.

The suite workflow's path: draft sub-PRs into the stack, the Planner's grade and Codex
per sub-PR, CodeRabbit and Codex on the stack, and the operator's merge to `main`.
