# Brief: PR #52, the review round

From the planner seat to the executor, 2026-11-06. PR #52 is the design seat's docs-only PR, "docs: the lifecycle frame and the design system of 2026-11-06 (PRD 3.0)", branch `claude/amazing-pasteur-63yxru`, head `abe9151` (a merge of `main` at `f0c9307`). It is ready for review. Codex gave a thumbs-up on that head. CodeRabbit posted six inline comments (review 5807183520); the planner verified all six against the branch and all six hold. The planner's own review of the PR adds the rest below. This brief lands as `docs/project/brief-2026-11-06-pr52-the-review-round.md` in the fix commit.

## Scope

- **Branch:** `claude/amazing-pasteur-63yxru`. `git fetch origin` and `git switch` to it; do not rebase, never force-push.
- **Commits:** `docs: ...`. As many as the sections need, but **one push at the end.** CodeRabbit's allowance is one review per hour, and the included allowance is spent; every push costs an hour.
- **Hold the push until the planner confirms section 4's wording.** It is the operator's ruling and is pending. Everything else can be done now.
- The diff touches nothing under `src/`, `tests/` or `docs/weaver-web-Spec.md`. Every edit follows the operator's working rule of 2026-11-06: a document states the current state of its subject; a passage that no longer holds is replaced in place; no "formerly", no "as of"; git is the archive. The one exception: a ruling that reverses an earlier ruling says so in one sentence naming what it reversed.
- ASCII only, no em-dash, in every file touched.

Line numbers below are on `abe9151`.

## 1. `tokens.css` (CodeRabbit threads 4497747128 on `bundle.css:1` and 4497747145 on `DeclareDiagram/preview.html:2`)

`docs/design/system/components/bundle.css` opens "Every value is a token from tokens.css" and defines no custom property. The PR carries `tokens.json` and no `tokens.css`, and the nine previews link `bundle.css` alone, so every `var(--...)` resolves to nothing.

- Write `docs/design/system/tokens.css` from `docs/design/system/tokens.json`: the color block as `palettes.md`'s `instrument.css` block already states it (Slate at `:root`, Mist under `@media (prefers-color-scheme: light)`, each of the six themes under `:root[data-theme="..."]`), plus the type, spacing and radius tokens `tokens.json` carries. Every `--name` that `bundle.css` or any preview reads must be defined. Nothing in it that `tokens.json` does not hold.
- `bundle.css` begins with `@import url("../tokens.css");` under its first comment, which then names the import. The previews keep their one `<link>` to `bundle.css`.
- The design system README's file table gains the `tokens.css` row, stating what it is: the tokens of `tokens.json` as CSS custom properties, the file the bundle imports.

**Gate:** the set of names in `var(--...)` across `bundle.css` and every `preview.html`, minus the set of names declared `--...:` in `tokens.css`, is empty. Show the two counts and the empty difference in the PR comment.

## 2. The Workspace preview's rail (thread 4497747151 on `Workspace/preview.html:4`)

`StageRail/README.md` defines the rail as "the seven stages in order, each a stage mark with its backing badge", with the agent in `aria-label`. `Workspace/preview.html` line 4 carries the seven stages with no badge and no label. Give it the seven badges exactly as `StageRail/preview.html` carries them (1 unbuilt, 2 partly, 3 built, 4 built, 5 partly, 6 unbuilt, 7 partly) and `aria-label="Stages of hb-01"`. Keep `here` on 3 Run and the footer "3 of 7 . Observe next".

## 3. Provision names its party and its install step (thread 4497747180 on `stages.md:23`, and the planner's item 4)

Two gaps in one place.

- **The declaration's install.** Spec 7.2 (lines 3933-3941, the operator's ruling of 2026-10-02 on `toddwbucy/WeaverAgent#59`): `validate` takes only an agent's name and judges the declaration installed on the box, never a draft; the operator installs the file Compose produced. Neither PRD 3.0's Provision row (`docs/weaver-web-PRD.md:168`) nor `stages.md`'s Provision table (lines 15-23) has that step, so `validate` at the end of Provision could judge an older file.
- **The party.** The row says "stage binaries on the host, create the kernel account" and `stages.md:19` says "localhost or a remote host over SSH". `CLAUDE.md`'s hard boundaries: the server never reaches the agent, opens no socket and runs no binary on the box. Creating an account is root work. As written, a reader takes staging and the account as server features.

Edits:

- PRD line 168, the Provision row's "what it holds" becomes, in substance: the operator's work on the box, which this server never reaches: stage the binaries, create the kernel account, install the declaration Declare saved; then this server's: register the agent, mint its two connector credentials; then `validate`, which reads the installed declaration. The backing cell keeps "partly" and names the register page and the handover as built, and the box steps and the validate step as not. Drop "SSH".
- `stages.md` Provision table: the Host and Kernel account rows say in "shows" that the work is the operator's on the box and the pane shows what is done and what remains; drop "over SSH" (write "localhost or a remote host"). Add a row before Validate: `Declaration | the operator installs the file Declare saved; validate reads the installed declaration, never a draft (Spec 7.2, WeaverAgent #59) | nothing yet | unbuilt`. The Validate row's "shows" says "the installed declaration".
- `declare.md` line 7 and `docs/design/system/README.md`'s Provision row (about line 61), where they describe `validate`, say the same: the installed declaration.

## 4. The `validate` wording (threads 4497747188 on PRD 219 and 4497747196 on PRD 273; the planner's item 3) - PENDING THE OPERATOR'S RULING

The PR carries two wordings side by side and contradicts itself:

- PRD 217-223 and 286-289, `declare.md:7`, `stages.md:23`: "asks admin whether every element the declaration names exists on the box and can load ... acceptance to load, not a promise the load succeeds" (2026-11-06).
- PRD 278-279, `declare.md:22`: "acceptance for filing and not approval to load" (2026-09-05, `weaver-admin-PRD` 4.3), and Spec 7.2 lines 3946-3950: whether the artifact resolves, whether the family exposes the elected taps and whether the weights load at this precision are answered at admission, not by `validate`.

CodeRabbit's point on 219 is right either way: "can load" is not a question admin answers under the contract. The planner has recommended to the operator that the 2026-11-06 ruling restates the 2026-09-05 one (option A). **Do not push before the planner sends the ruled wording.** Under option A the edits are:

- PRD 218-219: "whether every element the declaration names exists on the box and can load" becomes "whether every element the declaration names that admin holds custody of is on the box". PRD 221-222: "A clean `validate` is acceptance to load, not a promise the load succeeds." becomes "A clean `validate` is acceptance for filing: nothing admin holds custody of stands in the way of a load, and what only the organ can judge is answered at admission. It is not a promise the load succeeds." Keep "(the operator's ruling of 2026-11-06)" on the sentence that places the verb at the end of Provision.
- PRD 278-289: the bold sentence at 278 stays, "per `weaver-admin-PRD` section 4.3 as ruled 2026-09-05 on issue #456" stays; delete the sentence at 286-289 ("On 2026-11-06 the operator ruled ... is a cell of section 9") and the section 9 cell it points at (about line 877-878, the cell on whether the two wordings are one). Section 9's row count drops by one; name it.
- PRD 274-276: "Composition-time refusal is that verb one hop from this surface, and it cannot drift because it is **the contract chain's own shape at a pinned commit** rather than a second copy of it." becomes "The refusal is that verb at the end of Provision, and it cannot drift because it is **the contract chain's own shape at a pinned commit** rather than a second copy of it."
- `declare.md:7` and `stages.md:23`: the same wording as PRD 218-222. `declare.md:22` keeps "acceptance for filing, not approval to load".

If the operator rules option B (a reversal: admin answers loadability), the planner sends the wording instead; it would also name what it reversed in one sentence and open an ask on `toddwbucy/WeaverAgent`, since Spec 7.2 and the admin contract would move.

## 5. `CLAUDE.md`: the bullet the merge dropped

`ba5251c` added a `docs/design/` bullet under "## Inherited documents"; the merge commit `abe9151` took `main`'s file whole and the bullet is gone (`git diff origin/main -- CLAUDE.md` is empty on the branch). Restore it as the second bullet of "## Inherited documents", after the one that ends "The repository's standing positions are in `docs/project/positions-weaver-web.md`.":

```
- `docs/design/` holds the frontend design: the 2026-09-30 canvas under `canvas/`, and since
  2026-11-06 the design system under `system/`, whose README is the brand book, `tokens.json`
  the six themes (Slate the default), `tokens.css` the same as custom properties, and
  `palettes.md` the palette rulings with the `instrument.css` block. The rulings are PRD
  section 3.0. Its two working copies are private artifacts named in `docs/design/README.md`.
```

## 6. Cites

Every claim names its source. Add, in the sentence or as a parenthesis:

- "`load` from the front end is refused upstream for now" (PRD 169, `stages.md:30`): `toddwbucy/WeaverAgent#166` (admin refuses a load whose invoking cgroup is not root-owned, which parks connector-managed loads).
- "`restore` carries no argument" (PRD 171, `stages.md:49`): the invoker's fixed line `sudo -n <weaver_admin> <verb> <agent>`, Spec 7.2.
- "state management, today SQLite in memory only", the decoder/classifier/LoRA roster (PRD 204-206, `stages.md:10`), and "reload under a diagnostic binding with the gate shut" (PRD 172, 348, `stages.md:55, 57`, system README 65): name the WeaverAgent document and section each is read from. Read them under `/home/todd/Projects/WeaverTools_Project/WeaverAgents/docs/crates/` (read only; edit nothing there). If a claim has no source there, the sentence says it is the design session's assumption and waits on the document.
- `Marks/README.md:7`: "carries the mark's reason as the relay or the server wrote it" lists reasons. The code's reasons are `link_outage` (`src/link/client.rs`) and `truncation` (`src/traceview.rs`, `src/link/admin_con.rs`); `rotation` and `parse_failure` are not in `src/`. List the two the code writes. "A replayed row ... carries `replayed`": `replayed` appears in `src/link/tests.rs` only. Grep `src/` for how a replayed row reaches the live window; if nothing in non-test code marks it, the sentence says the mark is not yet carried and waits on the listener.

## 7. The surface mapping (PRD 175-177)

"Compose is stage 1. Live is stages 3 and 4. Measure, Open a trace, Stage and the matrix are stage 6 and the list side that feeds it. The Agents surface is stages 2 and 7. Models is the artifact side of stage 2." PRD 3.0 says all ten surfaces are read through the frame. Record, Experiments and the Experiment view are unmapped, and "the matrix" is not one of section 3.6's five names. Map all ten by the names section 3 gives them. Where a surface is a list that feeds a stage rather than a stage's page, say so, as the sentence already does for the list side.

## 8. The current-state rule on the new text (the planner's item 7)

- PRD 131: `### 3.0 The lifecycle frame, 2026-11-06` becomes `### 3.0 The lifecycle frame`. No other PRD heading is dated; the section's first sentence already dates the session.
- `docs/design/README.md`:
  - Line 1: `# The frontend design` (no date).
  - Line 38, the "One look" election, is rewritten in place to state the current look: the system stacks (named in the design system README), six themes with Slate the default, each theme's accent. The paragraph at 134 ("What this supersedes in the elections of 2026-09-30 above ...") goes once the election says the current state.
  - Line 99, "**Superseded by acts 1 to 9** (PRs #3 to #21 ...)": reduce to one sentence saying what the Agents board's ruling is today and what act built it, no PR list; or, if the ruling it describes still holds as written, delete the paragraph.
  - Lines 22 and 29, "act 10 builds it": act 10 was parked on 2026-10-07 (the operator's scope ruling). State the current state: Open a trace is unbuilt and waits on WeaverAnalysis.

## 9. Vocabulary and a sample name

- PRD 118 and 136, "the top bar": the system's names are the masthead (the server's) and the stage rail under it (the agent's path). Use them.
- `DeclareDiagram/preview.html:50` labels the edge "kernel user weaver-hb-01". `DeclareDiagram/README.md` says the names in the preview are sample data, as the canvas README does for its boards. If `weaver-<agent>` is how a real box names accounts, change the sample to a shape that is not; no posture enters the repository.

## 10. The Mist-with-Slate cell (PRD 874) - PENDING

The operator rules whether the cell closes in this PR. The planner sends the answer with section 4's. If it closes: delete the cell, and the pairing stands where the four files already state it.

## Gates

1. `git diff --name-only origin/main` lists the PR's 25 files plus `tokens.css`, `CLAUDE.md` and this brief, and nothing under `src/`, `tests/` or the Spec.
2. ASCII check over every file in that list, no em-dash.
3. Section 1's token gate: the empty difference.
4. `grep -rn -E 'Composition-time|top bar|over SSH|and can load|cell of section 9' docs/weaver-web-PRD.md docs/design` is empty after section 4 lands.
5. `grep -rn -E 'rotation|parse_failure' docs/design/system/components/Marks` is empty.
6. No `<script`, no external `href` or `src` in any `preview.html` (`grep -rn -E '<script|https?://' docs/design/system/components` finds only the SVG namespace URLs).
7. The PRD's section 9 row count before and after, stated.

## The PR comment

The PR body is the design seat's; do not edit it. Post one comment on the PR with: the fix commit's hash; one line per CodeRabbit thread naming what changed; the token gate's counts; the section 9 row count; each cite added in section 6 with its source; what section 8 rewrote. The planner replies on the six threads after the push. End with the attribution line the session gives.
