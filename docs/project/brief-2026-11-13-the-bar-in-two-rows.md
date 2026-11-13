# Brief: the bar in two rows

Written by the WeaverWeb Executor on 2026-11-13, from the suite Planner's word of that day. Stack `act/the-bar-in-two-rows`, built on #102's head `393264d904a79ddc0813b148f5e87f1f927388ec` until #102 merges, then on `main`.

## Why

After #102 ordered the masthead's links by the path, the operator looked at the bar again. Today it reads: Record, Agents, the open-alerts pill, Passkeys, Persons, Grants, Register, Declarations. The operator said, relayed by the Planner:

- The operator asked why Passkeys, Persons and Grants each stand on the top row, when all three are identity and access management: "put that under a IAM tab and they are subtabs of that."
- "why is the alert pill in the middle of the top level directory?"
- "then you have agent register and declaraions all spread out and the all belong together as sub tabs. do you see what I mean, this is the disorganization that I see first when I log in".

Then, on Record:

- "Record should be The Record or it sounds like to record music instead of a record of an event...and that realy is about Agent Operations and that should go under there."
- "that is where we will have our experimetns, chat window, multi-agnet workflow coordination, and it is where we will probably introduce Cloud/Foundation model support down the line. everything outside of Agent Operations needs to be largely complete before we proceed with it."

So the bar gets two levels. #102's flat order is the first half of the answer, not the whole of it.

## Measured, read only

- **One template draws the bar.** `src/surfaces/templates/instrument.html` is the base of every served page (15 templates extend it). Each page declares its place by one field, `here`: `declarations`, `register`, `agents`, `record`, `admin` (Persons), `grants`, `passkeys`, `sign-in` or `enroll`.
- **The stylesheet has no second row.**
  - `.masthead` is one flex row. `.who` splits it with an automatic left margin.
  - `.rail` is a second row in the panel's ground, but it is the agent's stage rail, and on the agent page it stands under the masthead already.
  - The design system has no tabs or sub-navigation component.
  - Rules the served pages need and no preview draws sit in `bundle.css`'s served-surfaces section. Earlier acts added them in the repository, with `instrument.css` regenerated and pinned byte for byte.
- **The design system's working copy lags the repository.** The private artifact named in `docs/design/README.md` was read, without being changed, on 2026-11-13. Its `bundle.css` still says "the seven stages" and has no served-surfaces section. It holds none of the served rules added since 2026-11-07 (the masthead's `a.status`, its wrap and nowrap rules, the pane and stream rules), and no `.rail .step`. Nothing has carried a repository change back to it. PRD section 3.0 says the repository's text governs where the two disagree.
- **The Record is read under a session.** `GET /record` without one answers 401 with a page of its own, "This surface is read under a session. Sign in and ask again.", drawn without the base template. A reader who is not signed in does not reach it, so it is not a tab open to them.

## What was ruled (the Planner, from the operator's words, 2026-11-13)

1. **Top level, three tabs and no more:** the agents' work, Agent Operations, and IAM. The alert pill leaves the row of links. It is a status, so it stands at the right with who is signed in, and still links to the agents with open alerts.
2. **Under the agents' tab, sub-tabs in the path's order,** with their stage marks as #102 has them: Declarations, Register, Agents. A person who is not an admin sees the one sub-tab they may open.
3. **Under Agent Operations, The Record.** Record is renamed The Record. Nothing else is built under the tab in this act: experiments, the chat window, multi-agent coordination and foundation-model support wait until the work outside it is largely complete.
4. **Under IAM, sub-tabs:** Persons, Grants, Passkeys. A person who is not an admin sees Passkeys alone, their own.
5. **The second row shows the sub-tabs of the tab the page belongs to,** with the current one marked. Every existing page declares which tab and sub-tab it is. No page moves and no route changes. Sign in and sign out stay where they are.
6. **Names:** the Executor proposes the top-level label for the agents' tab. It must not read "Agents" twice in two rows without saying why. "IAM" and "Agent Operations" stay as the operator said them. The report says what was chosen and what else was weighed, and the operator rules by the merge.
7. **The stylesheet:** the report says what the design system's own components offer for a sub-navigation, what the least change is, and how `bundle.css`'s working copy outside the repository is kept in step. Where the change needs the operator's design ruling, the report names the line.
8. **Tests as for #102:** the two rows for an admin and for a person who is not, each page's tab and sub-tab, and the pill's place, with perturbations. The Masthead guideline and Spec 2.8 change where they describe the bar.

## Ruled out

- No route changes, no page moves, no migration and no stored state.
- No change to sign in or sign out.
- Nothing new under Agent Operations: no turn route, no ingest, no chat surface.
- No push before #102 is on `main` and the Planner's word.
- No restart of the running console until the Planner says the twin's pass is over.

## The sub-act

1. **`act/the-bar-in-two-rows-1-tabs`: the bar's two rows (code, stylesheet, Spec and design documents).**
   - The top row's three tabs, and the pill and who at the right.
   - The second row's sub-tabs for the page's tab.
   - Each page's place.
   - Record renamed The Record, where the served pages and the Spec name it.
   - The least stylesheet change.
   - Spec 2.8 and section 9.
   - The Masthead guideline and the brand book's navigation section.

## Acceptance

- The two rows for an admin and for a person who is not, each held by a test.
- Each page's tab and sub-tab, and the pill's place, each held by a test.
- **Every guard fails its test with the guard removed:**
  - the top row's three tabs and their order;
  - the pill's place;
  - each sub-tab's order and stage marks;
  - each admin-only sub-tab, absent for a person who is not an admin;
  - each page's tab and sub-tab.
- Each Spec section named under `Implements:` has a verdict.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
