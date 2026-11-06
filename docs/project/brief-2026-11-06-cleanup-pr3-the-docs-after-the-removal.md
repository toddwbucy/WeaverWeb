# Brief: cleanup PR 3, the docs after the removal

From the planner seat to the executor, 2026-11-06. This is the third and last PR of the cleanup act the operator approved on 2026-11-06.
- PR 1 (#49) merged at `b12ce77`.
- PR 2 (#50) merged at `96106d6`.

This PR is docs only. It brings the Spec and `CLAUDE.md` to the tree PR 2 left, and it carries ruling D4. Land this brief as `docs/project/brief-2026-11-06-cleanup-pr3-the-docs-after-the-removal.md`, the PR's first commit.

## Scope

- **Base:** `main` at `96106d6`.
- **Branch:** `docs/the-docs-after-the-removal`.
- **Commit subjects:** `docs: ...`. One commit per section below.
- **The diff touches no file outside `docs/weaver-web-Spec.md`, `CLAUDE.md` and this brief.**
- **The operator's working rule of 2026-11-06 governs every edit:** a document states the current state of its subject. A passage that no longer holds is replaced in place. Nothing narrates the removal: no "formerly", no "the legacy routes left in PR 2", no "as of". Git is the archive.
- Each correction cites the code it is corrected against, in the PR body (file:line on `96106d6`).
- Open a **draft** PR and tell the planner.

## 1. Spec section 1, the source tree

At `docs/weaver-web-Spec.md:43-55`, against `src/` on `96106d6`:
- The `adapters/` entry becomes `gate.rs, gate-con's client of the gate (section 7.1)`. The `upstream.rs (leaving)` line goes. Ground: `src/adapters/mod.rs` declares `gate` alone.
- The `web/` line goes. Ground: `src/web/` is gone and `src/lib.rs` declares no `web`.
- The `registry.rs` line goes. Ground: as above, for `registry`.

Confirm the remaining entries against `ls src/` and `src/lib.rs`. If any other entry is wrong, fix it in this commit and name it.

## 2. Spec section 2.13, the `Origin` check (about line 929)

"over the whole app, the legacy routes included" becomes "over the whole app, the fallback included".

Ground:
- `src/bin/weaver-web.rs` lays `gate::guard` over the whole router.
- `tests/startup.rs` measures that a `POST` to an unrouted path is refused at the check (403) and never reaches the fallback's 404.

## 3. Spec section 6, the surfaces' assets (ruling D4)

Add one paragraph to section 6, after its first paragraph. It must say:
- the surfaces serve their own assets from `/assets/surfaces/`: the stylesheet `instrument.css`, the vendored htmx and the passkey module;
- each is a file of the binary;
- each is served under the script policy of section 2.13;
- no other asset route stands.

Ground:
- `src/surfaces/mod.rs:99-102`, the three routes. The passkey module's route is under the `passkeys` feature, which the server binary requires.
- `src/surfaces/mod.rs:103`, `script_policy` over the whole surfaces router.
- `src/surfaces/mod.rs:130-157`, `include_str!` and `include_bytes!`.

One paragraph, no history.

**Litter in the same section.** Section 6's second paragraph ends in two sentences that narrate a correction:
- "**This sentence counted four reads and named every surface served until 2026-09-11**, and it was wrong twice: ...";
- "The matrix joined the served on the same day, the sixth read landing with the plan's schema."

The sentences before them already state the current state. Delete the two, and name them in the PR body as litter under the working rule. Leave the rest of section 6 as it is. Its Compose, Live, Stage and Models prose is the PRD rewrite's, which is owed outside this act.

## 4. Spec section 9, the session row (about line 4584)

The row's last sentence, "`tests/startup.rs` (ignored, the real binary) reads Record under a person's session and refuses a legacy POST at the `Origin` check", becomes a statement of what the test does on `96106d6`:
- it reads Record under a person's session;
- it refuses `POST /sign-out`, with the session's cookie and without, at the `Origin` check;
- it refuses a `POST` to a path no route serves at the same check.

Ground: `tests/startup.rs:259-281`. The claim of the row does not change; only its instrument's description does.

## 5. `CLAUDE.md`

- **Lines 388-391:** "server-rendered askama templates ... with htmx and SSE vendored into the binary via `include_bytes!`" becomes htmx alone. Ground:
  - `assets/sse.js` left in PR 2;
  - the live trace view is an event stream a link opens (`src/surfaces/templates/agent_page.html:28`), with no script of its own;
  - `src/surfaces/mod.rs:145` is the one `include_bytes!`.
- **Lines 420-427, "Still leaves: `web/`...":** the paragraph goes. One fact in it still holds and stays, as a sentence of its own: `deploy/` was left out of the fresh root because it held box paths and a sudoers rule, both of which the hard boundaries forbid.
- **The section heading and lead (359-363):** "## The seed tree: what carries forward and what leaves" and "The handoff's step 1 ... splits the roughly 5.5k lines of Rust in two. **Carries forward (about 3k lines): ...**" describe a split that is finished. State what the tree is instead:
  - a heading such as "## The tree";
  - the bold lead "**The store and the read path.**";
  - no line counts.

  Keep the rest of the section, which is current. Where its own sentences narrate a landing ("landed on 2026-10-01", "left with it"), leave them in this PR. They are the operator's act record in `CLAUDE.md`, and rewriting them is a larger edit than this act asked for. Name them in the PR body as seen and not touched.
- **Grep `CLAUDE.md` for every other mention** of `web/`, `src/web`, the legacy router, `registry.rs`, `upstream.rs`, `ProviderConfig`, `providers`, `agent_hop_budget`, `admins` (the config field), `sse.js` and `style.css`. State each in the current state, or delete it. The planner's grep found only the passages above. The file name `brief-2026-10-01-the-link-the-registry-and-the-two-connectors.md` is a document's name and stays.

## Not touched

- **`docs/project/inventory-weaver-web-code.md` and `docs/project/issues-carried-2026-09-26.md`** still name `registry.rs`, `web/admin.rs` and `adapters/upstream.rs`. They are dated registers inherited from WeaverTools. Whether they are kept current or retired is the operator's question; the planner raises it. Do not edit them.
- **The brief files under `docs/project/`** are records of their acts.
- **Everything the cleanup act names as owed:** monorepo apparatus, the PRD rewrite, listener posture, conformance headers, admin-con reading `length`.

## Gates

1. `git diff --stat main` names only the three files.
2. ASCII check over the changed files, no em-dash.
3. A grep of `docs/weaver-web-Spec.md` and `CLAUDE.md` for `legacy`, `src/web`, `web/ `, `registry.rs`, `upstream.rs`, `sse.js`, `style.css` and `(leaving)` finds nothing but the brief file's name.
4. Each Spec edit's citation, checked against `96106d6`.
5. Section 9's row count is unchanged: no row is added or removed.

No cargo gate: the diff touches no code.

## The PR body

- One section per commit above, each correction with its citation.
- The litter: section 6's two sentences.
- What was seen and not touched, and why: the landing narration in `CLAUDE.md`, and the two dated registers.
- **`Implements:`** Spec sections 1, 2.13, 6 and 9 (descriptions corrected to the built code; D4's paragraph in 6). No claim is added or removed.
- End with the attribution line the session gives.
