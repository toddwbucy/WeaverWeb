# Brief: the agent page for a first load

Written by the WeaverWeb Executor on 2026-11-12, from its proposal, which the suite Planner accepted that night with the rulings below. Stack `act/the-first-load`, from `act/the-declaration-screen` at `a1f431d3bf34dc389362f776bf35af03be615adf`, since `main` does not carry the declaration screen until the operator merges #79.

## What was ruled

- **The operator, 2026-11-11, late:** push forward and stack the pull requests. The Planner takes a sub-act out of draft when it is fit. A base stays a draft, and the operator collapses the stack.
- **The operator, 2026-11-11:** "Warn, do not refuse".
- **The Planner, 2026-11-12, on the four sub-acts:**
  1. admin's stderr on the verb's answer page, bounded at 2048 bytes, escaped as every other value on the page, and stated as admin's own words, shown and never parsed;
  2. a warning beside a verb that the contract's state table says admin will refuse in the row's last-read state. The verb is still offered, and the warning names the state and the reading's date. The table is read from the admin-operator contract. Where the contract and the page's table differ, the contract wins, and the difference is listed for the Planner;
  3. a statement of admin's `[restore]` rule, citing its source by section at the corpus commit the build pins (`CORPUS_COMMIT`), as the declaration screen cites the grammar, with a test that fails if the pin moves before the sentence is reread;
  4. the load-state pane re-fetched every 15 s without refreshing the session's last use, with a test that reads that the poll does not extend a session.

## The goal

The agent page, as the operator uses it for a first load through the connector: load, show, a turn, a save point, unload. A refused verb shows admin's own words where it was asked. A verb that admin's state table says it will refuse is warned of and still offered. The `[restore]` rule is stated with its source. The load state re-reads by itself, so a load answered past the silence bound arrives on the page without a reload.

## Ruled out

- **No new verb, and no answer from WeaverAgent built against a guess.** The console cannot tell beforehand that a load will refuse on a stale `[restore]`, since `show` names no save point and `validate` selects none. That is an interface question for WeaverAgent, drafted on the hub and not filed: which save point the next load would seat, and that the contract carry the rule.
- **No reading of the box:** not the declaration, the manifest nor the save points.
- **No guard that refuses or hides a verb.** A warning only.
- **No load or unload of any agent.** The tests use the fakes at the connectors, as every test here does.
- **The six "refused upstream" lines** are not this act's: #83 replaced them in the stack.
- No box path, host name, account or address in the repository, the brief or a pull request body.

## What to read

- `docs/weaver-web-Spec.md`:
  - section 2.13, reading an agent, and the verbs as built on 2026-10-13: the page offers the verbs, waits for admin's answer to the silence bound and shows it, or answers 202;
  - section 2.14, alerts: an alert's reason is admin's refusal object and its stderr, bounded to 2048 bytes; and "a re-fetch is not the person's use";
  - section 7.2, the admin verbs.
- WeaverAgent at `CORPUS_COMMIT`, `a3543b31f264b269f66ab55d8767f25290ec9d61`, read and never edited:
  - `docs/crates/contracts/weaver-admin-operator-contract.md` section 4.1, "What each command line answers, in each state of the agent", the state table;
  - `docs/crates/weaver-admin/weaver-admin-Spec.md` section 4, "The inventory": "A `[restore]` is honoured only while the save point it names is the manifest's latest". The contract does not carry this rule, and the hub question asks that it does.
- The code:
  - `src/surfaces/reading.rs` (`verb_shown`, the agent page);
  - the templates `agent_page.html`, `verb_answer.html` and `agent_planes.html`;
  - `gate::session_in_background`, the session read of the availability fragments;
  - `src/link/frames.rs` `VerbOutcome` (`stderr`, `raw_stdout`);
  - `src/declaration/render.rs` `CORPUS_COMMIT`.

## Sub-acts, in order

The sub-acts are linear. Each is a draft against the one below it, and the first against the stack.

1. **admin's own words on the answer page.**
   - **What it shows:** the verb's answer page shows the outcome's stderr, and the raw answer where it did not parse as one object, each bounded at 2048 bytes at a character boundary. It is escaped as every value on the page is, stated as admin's own words, and shown and never parsed.
   - **The Spec:** section 2.13's sentence on the answer page says so.
   - **Tests:** a fake invoker's refusal carrying stderr with markup and a multibyte character across the bound is shown escaped and bounded, and the answer object as before.
2. **admin's `[restore]` rule, stated with its source.**
   - **Where:** beside `load` and `restore` on the agent page.
   - **What it says:** a declared `[restore]` loads only while its save point is the manifest's latest. Any save point or unload publishes past it, and then admin refuses the load until `restore` runs again or the key is removed on the box. It cites `weaver-admin-Spec` section 4 at `CORPUS_COMMIT`.
   - **The pin:** a constant naming the commit the sentence was read at, and a test that fails when `CORPUS_COMMIT` moves away from it.
3. **Warnings from the contract's state table.**
   - **The table:** the page holds the contract's section 4.1 table for the verbs it offers and the states the row can hold (unloaded, idle, active, in transition), and cites it at `CORPUS_COMMIT`, with the same pin test as sub-act 2.
   - **When it warns:** a verb is warned of only where every contract state that the row's reading covers refuses it. For example, `load` while the row reads idle (S2 and S9 both answer `AgentRunning`). The warning names the refusal, the state read and its date. The verb's form stays.
   - **Differences:** where the contract and the table differ, the contract wins and the difference is listed in the pull request.
   - **Tests:** each warned cell, a cell with no warning, and the verb still offered.
4. **The load state re-read by itself.**
   - **What re-fetches:** the load-state pane and the verbs pane, so that a warning never outlives its reading. They are one fragment, re-fetched every 15 s by the vendored htmx with no inline script, through the session gate and the grant scoping as at the page's load.
   - **The session:** read by `gate::session_in_background`, so the poll never extends a session.
   - **The answers:** an agent without a grant answers as an unknown one, and an ended session answers 401.
   - **Tests:** the poll leaves the session's last use as it was; a state written between two fetches shows at the second; and the 404 and 401.

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test. Each test fails with its guard removed, shown by a perturbation.
- The two pin tests fail when `CORPUS_COMMIT` moves.
- No test reaches an agent or a box. The connectors are fakes.
- The gates of `CLAUDE.md`, each with its exit status, the suite with `DATABASE_URL` and `PGUSER` set and eight test threads, its counts, and nothing skipped.

## The interface question this act meets (drafted on the hub, not filed)

Which save point the next load would seat, readable before the load through `show` or `validate`, and the contract carrying the rule `weaver-admin-Spec` section 4 holds: `2026-11-12-interface-question-the-next-loads-save-point.md` on the hub. The Planner puts it to the operator with the declaration screen's six.

## Scope boundary

WeaverWeb alone. Nothing in WeaverAgent is edited. A gap found in the contract is added to the hub question, not built around.
