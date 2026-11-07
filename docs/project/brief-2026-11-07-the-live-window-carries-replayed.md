# Brief: the live window carries whether an event was replayed

From the planner seat to the executor, 2026-11-07. The listener classifies every event admin-con sends as replayed or live against the opening's boundary (`land_event` in `src/link/listener.rs`: the frame decides, before `caught_up` an event is the replay's, after it live; the client's flag and the offset rule are checks). The window it serves (`traceview::TraceEvent`) does not carry the verdict, so the live view cannot say which rows are replayed, and four design documents say the mark waits on the listener. This act carries it. Land this brief as `docs/project/brief-2026-11-07-the-live-window-carries-replayed.md`, the PR's first commit.

## Scope

- **Base:** `main` at `b0abbb2` (#56 and #57 merged).
- **Branch:** `code/the-live-window-carries-replayed`.
- **Commits:** the brief; `code: ...` for the field, its setting and its tests; `docs: ...` for the four design sentences and the Spec. One commit per section.
- **The diff touches `src/traceview.rs`, `src/link/listener.rs`, `src/reading_tests.rs` (and `src/link/tests.rs` where the window's contents are asserted), the four design documents of section 3, `docs/weaver-web-Spec.md` and this brief. Nothing else.** No template changes, no rendering: the pages stay plain (the operator's ruling of 2026-10-09, backend before presentation); the pane grammar act renders the row `dim` with `replayed` in `hold`. No change to admin-con, the frames or the fake relay.
- The working rule of 2026-11-06 governs every document edit. Grep the docs diff for "formerly", "as of", "superseded", "for now", "now", "no longer" and dated headings before pushing.
- ASCII only, no em-dash. Open a **draft** PR and tell the planner.

## 1. The field

`traceview::TraceEvent` gains `pub replayed: bool`, serialized as the other members are (`serde_json::to_string(event)` in `surfaces::reading::trace_event` already carries every member to the SSE `data:` line, so nothing in the surface changes).

**The listener sets it, from its own verdict and not from admin-con's flag.** In `land_event`, `behind` (`!caught_up`) is the frame's decision; the client's `replayed` is a check that is logged where it disagrees. The window carries `behind`. Marks ride the same frame (`admin_con::frame_of`'s `Item::Mark` arm sends a `TraceEvent` with `mark: Some`), so a mark behind the boundary is `replayed: true` like a record; the server's own two marks (`windows.mark(...)` at the reconnect and at the link's loss, `TraceViews::mark`) are the server's acts on the live side and carry `false`. `parse_line` and every other constructor of a `TraceEvent` (`reading_tests::event`, `admin_con::mark_event`, `traceview::parse_line`) gain the member: `false` where the event is not the replay's, since only `land_event` knows. Name each constructor in the PR body.

**One rule, two places:** where the listener classifies is where it marks. Do not compute `replayed` again anywhere else, and do not read admin-con's flag for it.

## 2. Tests

Each stated against the real listener, each with its perturbation shown and logged in a detached worktree of the pushed head:

1. **A replayed event reaches the live view saying so** (`src/reading_tests.rs`, in the shape of `the_live_view_relays_the_window_and_closes_when_the_grant_ends`, or in `src/link/tests.rs` where the window is read directly after a fake admin-con's replay): an event sent before `caught_up` arrives on the SSE stream with `"replayed":true`, one sent after with `"replayed":false`. Perturbation: set it from admin-con's flag instead of the frame's decision, send a frame flagged live before `caught_up`, and the view says live for an event the row never took.
2. **A mark behind the boundary is replayed, the server's own marks are not**: a backfill's front mark arrives `replayed: true`; the reconnect mark `replayed: false`. Perturbation: default marks to `false`, and the backfill's mark reads as live.
3. The existing suites pass unchanged apart from the member added to their constructors. `admin_con_tests` has assertions on the window's contents through `ns(...)` and `marks(...)`; where one of them can cheaply assert the verdict too (a replayed backfill, a live record after it), add it; do not rewrite them.

## 3. The four design documents and the Spec

Each says the mark waits on the listener. State the current state: the window carries `replayed`, the listener's verdict against the opening's boundary; what the pane does with it (render the row `dim` with `replayed` in `hold`) is the pane grammar act's.

- `docs/design/system/README.md:16`: "a replayed event is to say `replayed`, which waits on the listener: it classifies each event against the opening's boundary, and the window it serves does not yet carry which."
- `docs/design/system/stages.md:42` (the Live trace pane): "the listener classifies each event as replayed or live against the opening's boundary, and the window does not yet carry which".
- `docs/design/system/components/ObservePane/README.md:4`: "a mark that waits on the listener: the window does not yet carry which events were replayed."
- `docs/design/system/components/Marks/README.md:7`: "that mark is not yet carried and waits on the listener, which classifies each event as replayed or live against the opening's boundary but serves a window that does not say which."
- **Spec:** section 2.13's live-view sentence (about line 1822, "an event stream of the listener's window, which says it is that window and not a record") names what each event carries: the window's members and the listener's verdict, replayed or live. Section 7.2's sentence "Replayed events reach the server's live window only" (about line 3790) is unchanged in substance; if it gains anything, it is that the window says which. Section 9: the row "reading an agent takes a grant on it" is not this claim; find the row the window's claim belongs to (the trace replay row of 7.2, or a reading row) and add the two perturbations of section 2 to it. If no row fits, say so in the PR body rather than forcing one.

## Gates, in a fresh detached worktree of the pushed head

Run `git worktree prune` first; use a new worktree name with `|| exit 1` after the add; print `git log -1` inside it and check it is the PR head.

1. `cargo fmt --check`; `cargo clippy --all-targets --locked -- -D warnings` and the same with `--no-default-features`.
2. `DATABASE_URL=... cargo test --locked`, the whole suite, the `test result` lines in the PR body.
3. Each perturbation of section 2 run and its log read against its claim, one line each in the PR body.
4. The docs grep of the scope section, empty or each hit explained.
5. The PR body names: `Implements: Spec 2.13 (the live view), Spec 7.2 (the window), Spec 9 (the row)`, every `TraceEvent` constructor touched, and the design sentences restated.
