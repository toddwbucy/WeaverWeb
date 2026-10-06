# Brief: act 10, the Replay surface (Open a trace), in three pull requests

Version: v0.1, 2026-10-06. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. The operator chose this act on 2026-10-06, after act 9 closed #17. It is the
first thing a researcher sees: a landed run rendered as a replay, which the handoff of
2026-09-30 put first ("render the deposits first: a trace and its state store as a
replay, before any live view") and the design of 2026-09-30 drew as the `Main` artboard,
"Open a trace (HeroBench Replay)". The PRD calls the surface **Open a trace** (section
3.4) and the Spec serves it with reads 1, 2 and 3 of section 4. This brief uses that
name; "the Replay surface" is the operator's name for the same thing.

**Rulings of 2026-10-06, after the executor's reading, which amend the 10b section below
where they differ.** The section's original text stands below and these govern it:
1. Migration `0014` adds the `generation` table, keyed `(run_id, seq)` in landing order,
   with the turn, the perplexity, the resident count, the output count and the
   generation's seed; 10c reads the summary and the turn order from it.
2. `0014` makes `token_text`, `entropy`, `alternatives` and `realized` nullable, the read
   types becoming `Option`, since the signals wire carries none of the last three and
   entropy only where the generation measured it.
3. A point whose generation lacks its resident count does not land, since no position
   can be derived for it, and the run reads `short` naming the generation, per Spec 3.1.
4. Several runs in one emission each land as their own row with their own status; the
   ingest refuses only disagreement within a run.
5. The status vocabulary is `writing`, `whole`, `short` and `refused`.
6. `run.sampler` is the declared members of the effective sampling with
   `generation_seed` removed, and the seed column is the declared seed.
7. The ingest's answer is per run: the object carries `runs`, one entry per run in the
   emission, with its identity, its status (`whole`, `short` naming the generation, or
   `refused` with the reason), the positions written, the generations landed and the
   members absent. Exit 0 where every run is `whole` or `short`; exit 1 where any run is
   `refused` or the emission itself is (unreadable, no summary line), the object still
   listing what landed, so a caller never loses an outcome behind one status.
8. A branch whose parent is not held still lands. The foreign key on `parent_run_id`
   stays; `0014` adds `parent_reference TEXT`, the lineage's parent run identity as the
   record spelled it, written always for a branch, and `parent_run_id` is set only where
   that parent is in the store. The parting position is derived only where the parent
   is held and is absent otherwise; the surface shows the reference with `absent` for
   the link. Resolving the reference when the parent lands later is owed to a later act.
   Spec 2.2's lineage bullet gains the member in 10b.
9. The parting position is derived only against a whole parent. A parent held but
   `writing`, `short` or `refused` may have an incomplete token path, and Spec 4 reads
   an absent parting position as the arm having reproduced its parent, so the ingest
   walks the two paths only where the parent's status is `whole`. `0014` adds
   `parting_known BOOLEAN NOT NULL DEFAULT false`, true where the walk ran (the position
   set, or null meaning the paths never parted) and false where it did not. The surface
   renders a known null as `absent` and an unknown as a fifth absence word, `unknown`, a
   member not yet derivable; Spec 6's absence rule and the design README's vocabulary
   note gain the word in 10c. Deriving later, when the parent becomes whole, is owed
   with ruling 8's resolution.
10. Spec 3.1 is revised in 10b, not only 2.2. Its paragraph that has a branch's parent
   already in the store becomes: the ingest walks the paths where the parent is held and
   whole; otherwise it stores the parent's reference, leaves `parent_run_id` unset and
   the parting position unknown, and the resolution is owed to a later act. Section 9's
   parting row follows.
11. The new members reach the existing read and surface in 10b, not 10c.
   `parent_reference` and `parting_known` are carried by the run tuple type, read three
   and read five. Read five's parent chip filters on `parent_reference`, which a branch
   names whether or not its parent is held, `parent_run_id` being the resolved link, and
   `0014` indexes `parent_reference` as `parent_run_id` is indexed. Record renders a
   branch's parent as the reference, linked where resolved and `absent` for the link
   where not, and the parting position as the position where known, `absent` (never
   parted) where known null, and `unknown` where not known, so no branch reads as having
   reproduced its parent on an incomplete walk. Spec 4's read-three and read-five
   bullets and its sentence reading a null parting position as reproduction are revised
   in 10b with 2.2 and 3.1; 10c inherits the words.
12. The run's status reaches the existing read and surface in 10b. `ingest_status` is
   carried by the run tuple type and reads three and five, and Record renders it on
   every row: `whole` plainly; `writing`, `short` and `refused` as a marked badge
   carrying the word and, for `short` and `refused`, the reason beside it or on hover.
   So a run whose ingest stopped after its row was written never looks like a completed
   run on the one surface that exists before 10c. Spec 4's read-three and read-five
   bullets name the member with ruling 11's lineage members; 10c inherits it in the
   tuple strip.
13. The reason is persisted beside the status. `0014` adds `ingest_reason TEXT`, null for
   `whole` and `writing`, the reason for `short` (naming the generation and why) and for
   `refused`. The run tuple type and reads three and five carry it, and Record's badge
   renders it from the store, since the ingest's answer object is gone once the process
   exits.
14. Idempotence is equality, never silence. A replayed key (the run, a generation, or a
   run, turn and position) is compared with the stored payload: equal is a no-op and
   counts as written; different is refused by name, the run marked `refused` with a
   reason naming the first key that differed and the stored row untouched. Never
   `ON CONFLICT DO NOTHING` reporting success over stale data, and never an update that
   rewrites a recorded result. The replay test gains the perturbation: replay with one
   point's token changed, and without the check the run reads `whole` over the old row
   or the row is rewritten; with it the run is refused and the row stands. Spec 3.1's
   idempotence bullet reads "idempotent on the key, and a key replayed with a different
   payload is a refusal", and Spec 9's row follows.
15. The session's theme defaults. `theme TEXT NOT NULL DEFAULT 'auto'` with the
   three-value check, so every session before the migration is backfilled by the default
   and every session inserted without the member reads `auto`; the first render always
   carries a value.
16. A point with no turn key does not land. `position.turn` is part of the key and the
   contract makes `turn` optional on a point, so a point whose turn is omitted is skipped
   like a point whose generation has no resident count, and the run reads `short` with
   the reason naming the generation and "no turn key"; the generation's summary still
   lands. Nothing invents a turn. This makes explicit what ruling 4's "or no turn key"
   implied.
17. A conflicting replay never changes a stored run, and amends ruling 14 where they
   differ. Where a replayed key differs from the stored payload, the refusal is reported
   in the command's answer for that run (exit 1 per ruling 7) and nothing in the store
   changes: not the row, not the status, not the reason. A run that was `whole` stays
   `whole`; a run left `writing` by an interrupted ingest completes to `whole` only on a
   replay whose every key is equal, and a differing replay leaves it `writing` with the
   refusal in the answer alone. The `refused` status is written only by the ingest that
   created the row, for a refusal met before the row was closed. Spec 3.1's bullet says
   a refusal on replay is the answer's and never the row's.
18. The link follows presence; only the walk follows completeness. This amends ruling 10
   where they differ. `parent_run_id` is set whenever the referenced parent is held in
   the store, whatever its status, and `parting_known` is true only where the walk ran
   against a `whole` parent. Spec 3.1 reads accordingly: the reference is always stored,
   the link is set where the parent is held, and the walk runs where it is also whole.
   Record then links a held incomplete parent and shows its parting as `unknown`.
19. Absent entropy is drawn as an absence too. The timeline draws each series where its
   points carry the value and hatches the span where they do not, entropy and surprisal
   alike; "the entropy series always" in the 10c section becomes "the entropy series
   where measured". The test counts both the plotted points and the hatched spans for
   each series, and the perturbation "draw a zero" applies to both.
20. `0014` backfills `parting_known`: rows whose `parting_position` is not null are set
   true, since that value was computed, and every other row keeps the false default, a
   pre-existing null parting being unknown rather than never parted, the honest reading
   of a column that did not yet say which.
21. "Never parted" needs both paths whole; this refines ruling 9. The walk runs where the
   parent is `whole`, and a divergence it finds inside the child's retained positions is
   known whatever the child's status, the paths provably having parted there. A walk
   that finds no divergence yields a known null only where the child is `whole` too; a
   `short` child whose retained prefix matches its parent keeps `parting_known` false,
   since its missing span may diverge, and the surface shows `unknown`. Deriving later,
   if the child is ever completed, is owed with ruling 8's resolution. Spec 3.1 and
   section 9's parting row say so.
22. A known divergence needs every child position before it; this refines ruling 21. A
   divergence the walk finds is the first differing position only where no child
   position before it was skipped, so the walk records `parting_known = true` with the
   position only where every child position through the observed divergence is present.
   Where a skipped span precedes it, the parting stays unknown, since the span may hold
   an earlier difference, and the surface shows `unknown`. A known null (never parted)
   still needs both paths whole.
23. Parents in the same emission resolve in the same ingest. After every run of an
   emission has landed and closed, the ingest makes a second pass over the emission's
   branches: where a parent named by `parent_reference` is now held, the link is set and
   the walk runs under rulings 9, 21 and 22, so the result never depends on the order of
   runs within one emission. Resolution across emissions, a parent landing in a later
   ingest, stays owed to a later act as ruling 8 says. Perturbation: a two-run emission
   listing the child first; without the second pass the child lands unlinked, with it
   the link is set and the parting known.
24. `0014` backfills `parent_reference`. Every existing branch has its parent resolved,
   so the migration sets `parent_reference = parent_run_id` where `parent_run_id` is not
   null before read five moves to the new column, and no branch leaves its parent's chip
   or loses its reference at the upgrade.
25. `ingest_status` is not null, backfilled `whole`, with no default. The migration adds
   the column, sets every existing run to `whole` (the old schema had no partial state,
   so every row it holds was written whole), then makes it `NOT NULL` with no default,
   so an insert that forgets the status fails rather than reading as any word. The
   ingest writes `writing` explicitly at the row's creation, and the test seeders write
   a status. `ingest_reason` stays nullable.
26. The fifth absence word lands with its first use. Ruling 11 has Record render
   `unknown` in 10b, so Spec 6's absence rule and the design README's vocabulary note
   gain the word in 10b, not 10c, amending ruling 9's "in 10c"; 10c inherits a word the
   repository already admits.
27. A branch closes only after its resolution; this amends ruling 23 where they differ.
   Ruling 23's second pass runs before the branches close, not after: every run of the
   emission lands and its points are written while its row reads `writing`; then, for
   each branch, the resolution (the link where the parent is held, the walk under
   rulings 9, 21 and 22) and the transition to `whole` or `short` happen in one
   transaction. An ingest that dies between the two leaves the branch `writing`,
   visibly partial, and never `whole` with an unlinked parent or an unknown parting
   that a replay would not repair. Non-branches close as before. Ruling 14's
   equal-replay rule completes such a `writing` branch, resolution included.
   Perturbation: kill the ingest after the points and before the resolution, and the
   branch reads `writing`; without the ordering it reads `whole` unlinked.
28. Branches resolve in dependency order; this refines ruling 27. Within one emission,
   the resolution pass orders the branches topologically over `parent_reference`,
   parents before children, so a child's walk always sees a parent that has already
   closed in this ingest where that parent is in the emission; a chain of any depth
   resolves bottom-up in one pass, and the result never depends on the order of runs in
   the emission. A reference cycle, or a run naming itself, is a defect refused by name
   for the runs in the cycle (`refused` written by this ingest, since their rows are
   still `writing`), the rest of the emission landing. Perturbation: a three-run
   emission listed child, branch-parent, root; without the ordering the child closes
   with its parting unknown, with it every link is set and every walk runs.
29. A cycle's persisted refusal reaches only rows this ingest created; this refines
   ruling 28 by ruling 17. Where a reference cycle includes a run that existed before
   this ingest (a replayed row, `writing` or otherwise), that row keeps its stored
   status and reason, and the refusal is reported for it in the command's answer alone;
   `refused` is persisted only for the cycle's rows this ingest created. A replayed
   `writing` row caught in a cycle therefore stays `writing`, since the cycle blocks the
   equal replay that would complete it, and the answer says why. Perturbation: leave
   A -> B `writing` by ruling 27's hook, then replay A identically with a new B -> A;
   without the fix A's stored status changes, with it A stays `writing` and only B is
   persisted `refused`.
30. A cycle's refusals are persisted in one transaction; this refines ruling 29. The rows
   of one detected cycle that this ingest created move to `refused` together, in a
   single transaction with the reason naming the cycle, so an ingest that dies part-way
   leaves them all `writing` and never one `refused` beside one `writing` that no replay
   could converge. Perturbation: a new A -> B -> A cycle with ruling 27's hook set to
   stop between the two refusals; without the fix A reads `refused` and B `writing`,
   with it both read `writing` and the next ingest refuses both.

**Three pull requests, in order**, each its own act: 10a lands the design, 10b lands the
ingest the surface reads from, 10c lands the surface. 10b exists because nothing writes
a run or a position to the store today outside tests: the analysis ingest of Spec 3.1
is unbuilt (issue #2, W6), and a surface over an empty store proves nothing.

Run the five-class checklist on every code change (two copies of one fact, ambiguous
commit, multi-file state not atomic, filesystem trust, unbounded or unjoined) and name
the classes in each PR body. Every perturbation is shown to fail with its guard removed.
No box path, host name, uid or posture enters the repository.

## Read first

- PRD section 3.4 (Open a trace) and 3.6; Spec sections 2.1, 2.2, 3.1 (the ingest), 4
  (reads 1 to 3 and 5), 6 (the surfaces, the absence rule), 9 (the ingest's two rows).
- `weaver-analysis-web-contract`, in the WeaverAgent checkout under
  `docs/crates/contracts/`: sections 2.1 (the series), 2.2 (the summary), 3 (what the
  emitter owes, the position conversion), 4 (what the reader owes), 7 (absent members are
  omitted, the sentinel crosses as the empty string). The WeaverAnalysis checkout holds a
  duplicate; which copy is authoritative is unruled, so read both and name any drift.
- WeaverAnalysis `src/main.rs`, `run_signals` and `render_point`, and `src/signals.rs`:
  the `signals` command's output is the emitter's half of the contract as it stands.
- The design: branch `origin/claude/loving-feynman-dwwr1u`, `docs/design/README.md` and
  `canvas/Main.dc.html` (light and dark). The surface is built to it, with the
  departures named below.
- `src/surfaces/record.rs` and its templates: the one surface that exists, the pattern
  for routes, askama, the instrument stylesheet and the absence words.

## 10a: the design lands (docs)

Branch from `main`, draft PR, `docs:`. Bring the design branch's fourteen files under
`docs/design/` onto `main` as the record they are, with three changes:
1. The README's section "The ruling the Agents board draws" describes the 2026-09-30
   state of the connectors (web-con, `src/bin/weaver-web-connector.rs`, `wire.rs`, plain
   TCP). Keep it as the dated record it is and add one paragraph under it: superseded by
   acts 1 to 9 (PRs #3 to #21, 2026-10-01 to 2026-10-06), which built what it asked for
   and more; the Spec's sections 7 and 8 and CLAUDE.md are the text; the design's
   drawing of Agents stands.
2. The Agents artboards' sample data names two real boxes of the operator's. Replace
   them with neutral sample names (the design README already says every value on the
   boards is sample data), in both light and dark files and `canvas.json` if it carries
   them.
3. The README's table: say that `Main` is Open a trace, the operator's "Replay
   surface", and that act 10 builds it; the other five are future acts.
No Spec change. The PR body names the canvas URL as the working copy, per the README.

## 10b: the ingest (code), closes #2

Branch from `main` after 10a, draft PR, `code:`, `Closes #2`.

**What it is.** `weaver-web ingest <path | ->`: reads one `weaver-analysis signals`
emission, which is one summary object on the first line (`positions`, `with_entropy`,
`with_surprisal`, `generations[]`, each generation a `GenerationSummary` per contract
2.2) and then one point per line (contract 2.1: `turn?`, `ordinal`, `token`,
`entropy?`, `surprisal?`, absent members omitted), and lands one run and its positions
in the store per Spec 3.1. Answer one JSON object on stdout with the exit status
agreeing, as the register verbs do: the run identity, the positions written, the
generations, what was absent.

**The rules, all in Spec 3.1 and the contract; the brief only points:**
- The run identity keys every row and crosses on every generation; generations that
  disagree on run, session, digest or weights hash are a defect the ingest refuses by
  name, never a run with two of either.
- The position is derived at ingest: `(R - O - 1) + j` from the generation's resident
  count at close, its output count and the point's ordinal; stored, never recomputed.
  A generation whose resident count is absent gives positions the surface shows as
  absent (see the migration below).
- Writes are idempotent on the (run, turn, position) key; bulk per generation, never per
  point; **the run row is written first and closed last**, carrying a status a surface
  can read. There is no status column today: migration `0014` adds `ingest_status`
  (`partial` while writing, `whole` at the close, `refused` with the reason where the
  ingest gave up after the row), so a partially ingested run is visibly partial.
  `0013` is frozen.
- The run row's members land from the summary as Spec 3.1 names them: the record
  identity from the weights hash (the sentinel crosses as the empty string and the
  column holds it, per the 2026-09-xx relaxation of the not-the-sentinel constraint, so
  check `0006` before writing), the session, the digest (absent where the run was not
  whole), the prefix length, the effective sampling as seed and sampler, the field
  depth, the device model, the code identity as the engine, the parent from the
  lineage's `built_from` (WeaverAgent #58 is merged; a lineage without it is a
  continuation and names no parent; never `through`), and the verdict where one
  crossed. The verdict has no column; store it in the run row only if a column exists,
  else name it absent and leave the column to the act that lands its kind, per Spec 2.2.
- The parting position is derived at ingest for a branch whose parent is in the store,
  absent where the paths never part or the parent is not held.
- **The token's surface text is not on the wire** (contract 2.1: detokenizing is the
  reader's, and this crate holds no tokenizer). `position.token_text` is `NOT NULL`
  today. Migration `0014` makes it nullable; the ingest writes it absent; the surface
  shows the identifier and the word `absent` for the text, per section 6. An interface
  question is filed on WeaverAnalysis asking for the text on the wire (cite it by number
  in the PR body; the planner gives it to you). Nothing in this act fakes text from an
  identifier.
- Absent members stay absent (never null-as-zero): entropy where the generation did not
  measure it, surprisal where the election did not stand, per contract 7 and Spec 6.

**Tests.** Against a vendored emission fixture under `tests/fixtures/`, produced once
by running WeaverAnalysis's `signals` on its own `tests/fixtures/diagnostic-certified.ndjson`
(and one more record if it has one with a branch), with a note naming the WeaverAnalysis
commit the fixture came from, as the harness does for its derived fixture. No test runs
the WeaverAnalysis binary. Perturbations, each shown to fail: replay the fixture twice and
the position count doubles (Spec 9's idempotence row, now enforced); alter the summary's
counts after ingest and reread, and the stored position changes (the position row);
drop the close and the run reads `whole` while short; write a null for an omitted
entropy and the surface draws a floor; let two generations disagree on the digest and
one run lands with two. Spec 9: the two ingest rows move from "perturbation" to named
instruments; add rows for the status and the disagreement refusal. CLAUDE.md: the
verb, the fixture's provenance note, and the migration.

## 10c: Open a trace (code)

Branch from `main` after 10b, draft PR, `code:`.

**Route and shape.** `/trace/{run}` under `surfaces::routes()`, state `Store` alone,
one module `src/surfaces/trace.rs` with its templates, the Record surface the pattern.
Record's rows link to it. Built to the `Main` artboard, light and dark, with the
design's foundations (Plex Sans and Mono, the one action color, amber entropy and
teal surprisal, absence as a dashed pill carrying its word, no gradients, no icons).
- **The tuple strip** (read 3): the run's tuple as Record shows it, the status from
  10b, the session and the digest with `absent` where the record was not whole.
- **The turns**: one block per turn key in landing order, the token identifiers in
  order with `absent` for the text, the turn's generation summary (perplexity where
  held, the two counts). The interview toggle of the design is drawn disabled with
  `not served`.
- **The timeline** (read 2): an inline SVG, server-rendered, over the whole run's
  positions: the entropy series always, the surprisal series where any point carries
  one, and a hatched span where its election did not stand (contract 4: an absence is
  plotted as an absence, never a zero). Against an absolute bar in bits, the caller's,
  defaulting to the contract's figure; the series-relative rule is not drawn. A spike
  resolves to its position: clicking a point in the SVG navigates to the position
  panel (htmx swap), which is the PRD's "a spike that resolves to a token is an
  instrument".
- **The position** (read 1): one position's alternatives and their mass where the
  record carried them, else `absent`; the realized token marked; links to the previous
  and next position so the operator "walks upstream".
- **The map and the fog toggle** are HeroBench's and not served: drawn as the design's
  `not served` pill, nothing more.
- **Theme**: the auto/light/dark control, persisted on the session row per the design;
  the session table has no such column, so `0014` (10b's migration, or a `0015` if 10b
  merged first) adds `theme TEXT` with a check on its three values, written by one
  small handler that is the surface's own argument, never through the store's recorded
  half. A page never flashes the wrong theme: the server renders the persisted value.
- **Absence**: every absent member renders its word per Spec 6, never a blank, never a
  zero. The four words are the design's: `absent`, `no run`, `not served`,
  `uncomputable`.

**Tests.** Against the 10b fixture in a scratch database: the page renders every turn
and position of the run; the timeline carries exactly the points with entropy and the
hatched span covers exactly the surprisal-less prefix (perturbation: draw a zero and the
test fails); a position's panel shows `absent` where no alternatives crossed; the theme
round-trips through the session row and the first render carries it. Spec 9: a row for
"an absent member renders its word and never a zero", now enforced, and a row for the
timeline's absence. Spec 6: Open a trace is served, by reads 1 to 3; PRD 3.4's figure
stands.

## Out of scope

- Live, Agents, Matrix, Experiments, Models: the other artboards, future acts.
- The alternatives emission (`weaver-analysis field`) and the lens: read 1 renders
  what the ingest holds, which today is nothing; `absent` is the truth.
- Token text: the WeaverAnalysis question.
- IAM and the install.

## Acceptance, each PR

The gates: `cargo build --locked`, `cargo test --locked` with `DATABASE_URL` set and
every DB-backed test running, `cargo clippy --all-targets --locked -- -D warnings`,
`cargo fmt --check`. ASCII only, absolute dates, superseded text removed. PR body with
`Implements:` lines per Spec section, the checklist's classes, and the perturbation
table. Check visibility before pushing.
