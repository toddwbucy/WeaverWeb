# Brief: cleanup PR 1, the Spec corrected against built code

From the planner seat to the executor, 2026-11-06. The operator approved the plan
(`plan-2026-11-06-cleanup`) on 2026-11-06 with rulings D1 to D4. This is the first of three
PRs: docs, then code (the legacy half leaves), then docs (what the code PR made stale). Land this
brief as `docs/project/brief-2026-11-06-cleanup-pr1-the-spec-against-built-code.md`, the PR's
first commit.

## Scope

Documents only. **The diff touches no file under `src/`, `tests/`, `migrations/` or `assets/`,
and no `Cargo.*`.** Base: `main` at `3380f97`. Branch: `docs/spec-against-built-code`. Commit
subjects: `docs: ...`. One commit per item below, each naming its ground in the message body.
Open a **draft** PR and tell the planner.

## The rule every edit follows (operator, 2026-11-06)

A document states the current state of its subject. A passage that no longer holds is
replaced in place, not kept beside its replacement. **Add no "as of", "changed on",
"was asked", "the earlier sentence said" or struck-through text.**
- Where a touched passage already carries such a change-note, replace it with current state too.
- The one exception is a ruling that reverses an earlier ruling, which says so in one sentence,
  naming what it reversed.
- None of the items below is a reversal.

House style: ASCII only, ` - ` and never an em-dash, absolute dates. **No box path, uid, group,
socket path, sudoers text or box install state** enters any file. The relay's install state on
any box (WeaverAgent#178) stays out.

## The items

### 1. Spec 3417-3420: the server's asks of the last three verbs

- **Stands:** "The server asks none of the last three yet: admin-con's ceiling reports them
  where the rule grants them, and the listener carries such a ceiling, but no surface reaches a
  verb before the authorization of section 2.13 stands."
- **State instead:**
  - The agent's page offers each lifecycle verb that both the person's grants on the agent and
    the connection's ceiling carry, `save-point`, `restore` and `force-unload` among them.
  - The prompt of section 2.14 asks `save-point` then `unload`, or `force-unload`, as the
    person's own asks.
  - Every ask is authorized per section 2.13 before it is sent.
- **Ground:** `src/surfaces/reading.rs:585-589` (the forms filter `VOCABULARY` by grant and
  ceiling); `src/surfaces/alerts.rs:268,450`; `src/store/identity.rs:30-40`.
- **Sweep:** `CLAUDE.md:46-47` says the same thing ("which the server does not yet ask");
  correct it in the same commit.

### 2. Spec 4436-4438: revocation and rotation

- **Stands:** "Both are register verbs run on the server rather than surface edits".
- **State instead:**
  - Revoke and rotate are register verbs, run as the host's commands or through the admin's
    register page per section 2.13.
  - Retire runs through the page alone.
  - The rest of the sentence (an authored edit under 3.2, ordered on the row's version) stays
    as it is.
- **Ground:** Spec 2.13 at 1959-1990; `src/surfaces/agents.rs` routes `/admin/agents/rotate`,
  `/revoke` and `/retire`; `src/bin/weaver-web.rs:53-70` (the host's `revoke` and `rotate`).
- **Checked, leave alone:** Spec 4056 ("Registration is a register verb run on the server")
  holds, since the page runs it on the server too.

### 3. Spec 3206-3212 and 4872-4874: the verbs as a subprocess

- **Stands:** at 3211-3212, "the operator's own program run as a subprocess and carrying no
  record either"; at 4873, "the operator's own program run as a subprocess under a contract that
  binds this crate to nothing".
- **State instead:** the verbs and the observation exchange reach admin through admin-con's
  sudo invoker on the agent's box, as fixed command lines per section 7.2, and never as a
  process the server runs.
- **At 4872-4874:** correct only the clause. The election ("what section 7 is called") stays
  open and unanswered.
- **Ground:** Spec 7.2 at 3407-3424; `src/link/sudo_invoker.rs`; `tests/no_privilege.rs`.

### 4. Spec 4008-4010: the browser listener's encryption

- **Stands:** "The charter's section 6 defers transport encryption on the browser's listener to
  a named trigger; this listener is a different one, crossing machines by construction, and is
  not under that deferral."
- **State instead:**
  - Where a certificate and key are configured, the browser's listener serves TLS only, per
    section 2.13.
  - The link's encryption is its own and holds without configuration.
  - Do not describe what the listener does without a certificate beyond what 2.13 already says:
    listener posture is held for an operator ruling (survey question 18).
- **Ground:** PR #27, merged 2026-10-08; `src/listen.rs:240-327`;
  `src/bin/weaver-web.rs:504-515`.
- **Note for the PR body:** PRD section 6 still carries the deferral. The PRD is out of scope
  for this act; name the line as left.

### 5. Spec 3586, 3690, 3742-3745: the relay's length

- **Stands:**
  - 3586: "while the relay names no length (`toddwbucy/WeaverAgent#88`)".
  - 3690: "since admin-con reads no file and the relay names no length".
  - 3742-3745: "This crate has asked WeaverAgent for the file's length in the relay's header
    at the moment of the request, on `toddwbucy/WeaverAgent#88`, which would make the boundary
    and the backfill's start exact."
- **State instead:**
  - The relay's header names `length`, a required member: the byte offset of the end of the
    last complete record at the relay's acceptance of the request.
  - admin-con does not read it, so the boundary and the drain keep the bounds this section
    elects.
  - With the length, the boundary and the backfill's start would be exact. The drain's moment
    in the middle of a stream rests on the heartbeat either way.
  - Replace the "has asked" narration. Cite `toddwbucy/WeaverAgent` PR #174 for the header;
    PR #174 closed #88.
- **Ground:** WeaverAgent PR #174, merged 2026-10-15 (`TraceHeader` gains `length`);
  `src/link/relay.rs:76-81` (admin-con's `TraceHeader` has `device`, `inode` and `birth_ns`,
  and no `length`).
- **Checked, leave alone:**
  - Spec 4531's and 4709's "the residue `toddwbucy/WeaverAgent#88` narrows" still hold, since
    admin-con does not read the length.
  - Reword them only if the edit above makes them read wrong, and say so in the PR body.

### 6. Spec 3951-3955: the conversion's inputs

- **Stands:** "What it converts from is owed and does not cross yet, issue #461: ... until that
  lands this seam carries the ordinal alone. A reader in that state cannot key its store and
  stores what crosses without converting".
- **State instead:**
  - The contract's summary stream carries the resident count at a generation's close and its
    output token count, and the emitter writes both.
  - The conversion is this crate's at ingest (section 3.1), which is unbuilt.
  - Where a generation's resident count is absent, its positions cannot be keyed, and what
    crosses is stored without an address derived from a figure that did not answer.
- **Ground:** `weaver-analysis-web-contract` section 3 (around line 221) and its lines 375-380;
  `WeaverAnalysis/src/signals.rs:59-60` (`resident: Option<u64>`, `output_count`).
- **Removed with it:** the monorepo issue number #461 goes with the sentence it supported.

### 7. Spec 32-63: the source tree and its paragraph

Replace the tree with the tree at this PR's base:

- **Directories:**
  - `store/`
  - `surfaces/`
  - `link/` (both halves of section 8)
  - `adapters/` (`gate.rs`, gate-con's client of the gate; `upstream.rs`, leaving in the code PR)
  - `bin/` (`weaver-web`, `gate-con`, `admin-con`)
  - `web/` (the legacy router, leaving in the code PR)
- **Top-level modules:** `alerting`, `config`, `fault`, `host`, `listen`, `passkeys`,
  `traceview`, and `registry` (leaving in the code PR).
- **The `*_tests.rs` modules beside them:** one line, not each file.
- Mark the three leaving items as leaving. PR 3 drops them.

Replace the paragraph "Two entries changed on 2026-10-01 and one was added..." (49-63) with
current state, keeping its claims that still hold:
- the box-bound reaches are linked by the two connector binaries and never by the server;
- the reaches live in `adapters/gate.rs`, `link/relay.rs` and `link/sudo_invoker.rs`;
- admin-con's sudo invoker is the one privileged invocation.

The planned homes `ingest/`, `authoring/`, `queue/` and `seams/` go. Sections 3.1, 3.2 and 5
state what is unbuilt.

**Ground:** `ls src src/link src/adapters`; `src/lib.rs`.

### 8. README.md (D1 (c))

Replace the whole file with two lines: the project name, and that it is pre-release with
`CLAUDE.md` as the guide. For example:

```text
# weaver-web
Pre-release. Read CLAUDE.md.
```

`LICENSE` and `Cargo.toml`'s `license` are untouched. A full README is written near release.

### 9. docs/technical/weaver-web/weaver-web.md, deleted, and the positions carried (D2)

Delete the file and its now-empty directories. In the same PR, add
`docs/project/positions-weaver-web.md`, carrying the five positions the operator named.

**State each in present tense as it holds in the built tree.** Give each one sentence of what
it is and a "held by" line naming where the tree holds it. Do not cite the deleted file; the PR
body names it and its last commit.

1. **A consumer outside the agent's boundary.**
   - It links no crate of the agent.
   - The two connectors on the agent's box are the only party that reaches the agent; the
     server reaches nothing past them.
   - Held by: Spec 1; `CLAUDE.md`'s hard boundaries; the `cargo tree` checks of
     `tests/connectors_link.rs`.
   - Source in the deleted file: 33-37.
2. **The browser is a display engine.**
   - It receives a rendered projection and submits what a person authored. It holds no
     protocol state of the agent's or the link's, and no routing or ordering logic.
   - **Reconcile with passkeys, and state it precisely:** a passkey is held by the person's
     authenticator, and the page's one module (`assets/surfaces/passkeys.js`) only carries a
     ceremony's challenge and answer between the authenticator and the server. No page code
     makes or holds a key or a signature.
   - Held by: Spec 2.13 (the passkey module, `script-src 'self'`); `src/surfaces/mod.rs:107-125`.
   - Source: 97-102 and 172-174.
3. **Verb answers are rendered as they came.**
   - admin's answer is shown whole, its JSON pretty-printed and nothing renamed or summarized.
   - admin-con parses no refusal class.
   - **State the one standing exception** rather than hiding it: the server reads an `unload`'s
     `save_point_not_taken` to raise the prompt of section 2.14, and still renders the answer
     whole beside it.
   - Held by: `src/surfaces/reading.rs:903-923`; Spec 2.14; `CLAUDE.md`'s conduit rule (Spec
     2.14, ruling 3).
   - Source: 73-85, 147-150 and 165-167.
4. **Gaps in the record are marks, never smoothed.**
   - A discontinuity, truncation, replacement or lost contact reaches the live view as a mark.
     It is never filtered out or bridged.
   - Held by: Spec 7.2 (replay and marked discontinuities); `src/surfaces/reading.rs:1198-1203`
     (the `mark` event); `src/link/admin_con_tests.rs`.
   - Source: 92-95 and 152-155.
5. **A missing contract is an ask upward, never a workaround.**
   - When a door contract lacks something, the need is filed as an issue on the contract's
     repository, one per interface question. It is never parsed around, linked around or reached
     around.
   - Held by: `CLAUDE.md`'s "Do not edit WeaverAgent" boundary.
   - Source: 141-145 and 159-163.

Open the file with one sentence saying what it is: the standing positions of this repository,
binding until the operator changes one. Add no history section. The positions are not a Spec
and carry no section 9 rows.

**Sweep for item 9:** `CLAUDE.md:436-438` says "`README.md` and `docs/technical/` describe the
older chat/lifecycle/trace product and are stale". State instead that the README is a
pre-release stub and the positions are in `docs/project/positions-weaver-web.md`. Leave the
handoffs, briefs and the inventory alone; they are dated records.

## Not in this PR

- **The legacy half** (PR 2), and what PR 2 makes stale (PR 3: Spec 918, Spec 4565, the tree's
  three leaving items, `CLAUDE.md` 419-427, and the `instrument.css` clause in section 6).
- **Owed, not this act:**
  - monorepo apparatus (graph blocks, monorepo issue numbers);
  - the PRD rewrite toward agent lifecycle management, PRD section 6's deferral included;
  - listener posture;
  - conformance header mismatches.

## Gates, before the draft

1. **Paths.** `git diff --name-only main...` lists only `docs/`, `CLAUDE.md` and `README.md`
   paths, and the deleted technical file.
2. **Old wording gone.** Re-grep the Spec and `CLAUDE.md` for each item's old wording. Each
   must return nothing:
   - `none of the last three`, `not yet ask`, `rather than surface`;
   - `run as a\s*subprocess` (whitespace-normalized);
   - `defers transport encryption`, `names no length`, `has asked WeaverAgent`,
     `does not cross yet`;
   - `ingest/`, `authoring/`, `queue/`, `seams/`.
3. **Mentions of each touched fact.** Grep the Spec, the PRD and `CLAUDE.md` for every other
   mention (`save-point`, `restore`, `force-unload`, `revoke`, `rotate`, `subprocess`, `TLS`,
   `length`, `#88`, `resident`, `src/`). Fix each one that now contradicts an edit, or name it
   in the PR body as left with its reason. The PRD is left by rule.
4. **Section 9.** No row's claim changes meaning; if one does, it moves in the same commit.
5. **ASCII and em-dash.**
   - `LC_ALL=C grep -nP '[^\x00-\x7F]'` over the changed files finds nothing.
   - No em-dash.
6. **No box facts.** The changed text holds no path outside the repository, uid, group or
   socket path.
7. **Tests.** `cargo test --locked --test no_privilege` passes; it reads the repository's
   documents.

## The PR body

Each item gets its old wording (short), its new statement, and its ground. Also:

- Name what was checked and left: Spec 4056, 4531, 4709, and PRD section 6.
- **Under `Implements:`**, name Spec 1, 6 (item 3), 7.2, 7.3, 8 and 10, and `CLAUDE.md`.
- End with the attribution line the session gives.
