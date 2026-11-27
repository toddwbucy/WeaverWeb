# Handoff: the hold before the round trip

Version: v0.1, 2026-11-09, evening. Written from the WeaverWeb planning seat on the
thinkpad after the second stacked act merged, for the sessions that reopen this
repository later in the week of 2026-11-09, when the operator returns from HADES work
with the connectors on the agent's box. Everything measured here was measured today on
`main`; every ruling is the operator's and is dated. The two founding handoffs of
2026-09-30 still govern what this repository is and what it builds against; `CLAUDE.md`
is current and says how the seats work. This document says where the repository stands,
what it waits on, what the next act is and what is held for the operator's word.

## 1. The claim

The repository is complete for the agent it has: every surface the front end needs to
register an agent, read its page, ask a lifecycle verb, place a turn and watch its trace
is built, tested with the database and merged. Nothing is in flight. The repository is
**on hold** by the operator's word of 2026-11-09: "everything waits on us being able to
load the model with the LoRA adapter and us having the very basic agent we need for the
training data STE100 re-writer." The first act when the hold lifts is one live round
trip through the front end against a real agent on the box, and that act is briefed
only after the operator says what the box showed.

## 2. What landed in the week of 2026-11-06

Nineteen pull requests reached `main` between 2026-11-06 and 2026-11-09, the last two
of them as stacks under the ruling of 2026-11-07 (`CLAUDE.md`, "How we work"):

- **The cleanup** (#49, #50, #51): the Spec corrected against the built code, the seed's
  legacy half removed, the documents brought after it.
- **The lifecycle frame and the design system** (#52, #53): PRD section 3.0's eight
  stages (Train the seventh, #55), the six themes under `docs/design/system/`, and
  `instrument.css` served as the design system's tokens and components, pinned byte for
  byte by `the_stylesheet_is_the_design_systems`.
- **The trace header's `length` in admin-con** (#56, #58), the reconciled teardown's
  counter waited for (#57, closing #35), and the live window carrying `replayed` (#59).
- **The pane grammar** (stack #61, sub-PRs #62, #63, #65; brief
  `brief-2026-11-07-the-pane-grammar.md`): the frame on every page, the agent page as
  panes under the stage rail, and the Live trace pane reading its stream through the
  browser module `assets/surfaces/trace.js`, the repository's one script under
  `script-src 'self'`. The module places words, computes nothing, clears its list on the
  stream's `window` event so a reconnect does not duplicate the snapshot, and holds at
  most what the server's window holds (Spec 2.13, the ObservePane README).
- **The moved contracts** (stack #66, sub-PRs #67, #68, #69; brief
  `brief-2026-11-09-the-moved-contracts.md`): the gate adapter's request line bound at
  49,041 octets, the organ envelope's exact capacity under `toddwbucy/WeaverAgent#508`,
  so the R1 adapter's seeding line fits (closes #64); a test pinning that a
  `model.unrecovered_call` record and an `unrecovered_call` content block pass the
  relay, the window and the live view untouched, raising nothing (the reader half of
  #40, `toddwbucy/WeaverAgent#136`); and the trace module's list bounded at
  `traceview::RING_CAP`, Codex's one finding from the pane grammar stack, pinned by
  `the_live_trace_pane_reads_its_stream_through_the_module`.

`main` is `6ef1f41`, the merge of #66, made 2026-11-09 18:02 UTC.

## 3. What stands in this repository, measured

    commit     6ef1f41 on main, in sync with origin, tree clean before this file
    issues     six open: #1, #2, #11, #18, #40, #46 (section 6 says which wait on whom)
    PRs        one open: #23 (act 10b, the analysis ingest), a draft parked since
               2026-10-07 for the WeaverAnalysis seam
    build      cargo build --locked passes; clippy --all-targets -D warnings passes with
               and without --no-default-features
    tests      cargo test --locked with DATABASE_URL set, 2026-11-09 at 6ef1f41:
               438 passed, 0 failed, 1 ignored, nothing skipped. The lib binary holds
               428 (249 s, the link's tests one at a time), connectors_link and
               no_privilege 2 each, the doctests 6 (three compile-fail pins among them);
               the ignored one is tests/startup.rs's acceptance, run by hand
    branches   the eight merged act branches still stand on the remote
               (act/the-pane-grammar and its three, act/the-moved-contracts and its
               three); deleting them is the operator's call

Both reviewers were clean on the last two stacks' final passes: CodeRabbit found no
actionable comment on #61 or on #66, and Codex's one finding across the two stacks (the
unbounded trace list, on #61) landed as #69.

## 4. What the hold waits on

Two things, neither in this repository:

- **The Granite agent with its LoRA adapter answering through the gate.** WeaverAgent's
  planning seat estimated it (relayed by the operator 2026-11-09) for 2026-11-10, or
  2026-11-11 if its check C bites: the emission-kept stack (`toddwbucy/WeaverAgent#499`)
  collapses, the install tuple work (#513, #514) follows, then the root steps on the box
  and the R1 agent is loaded and answering. The cutover from WeaverTrain to WeaverAgent
  began 2026-11-08.
- **The connectors on the box.** The operator installs gate-con and admin-con beside the
  agent at the end of the week, with the configs `register` writes. Nothing about that
  install enters this repository (section 6, ruling 1).

Behind the round trip's `load` step stands `toddwbucy/WeaverAgent#166`, ruled
2026-11-07 (every load runs in a root-owned transient scope of its own; a connector
restart does not end the agent; WeaverWeb's sudo lines and Spec 7.2 do not change) but
with its code not yet landed. Until
it lands, `load` through admin-con is refused upstream, and the PRD 3.0 Run row, the
design system README's Run row and `stages.md`'s Verbs row keep their `#166` note. The
ruling says the note goes when the code lands; that is a docs PR of one sentence in three
places when it happens.

## 5. The next act: one live round trip

Ruled 2026-11-07 and held 2026-11-09: the first act after the connectors stand is one
live round trip from the front end, read on the pane grammar's agent page:

1. `register` the agent through the admin's agents page; the two client configs taken
   from the handover once.
2. The connectors connect; the register row shows both planes present, the ceiling
   admin-con declared, and the trace door.
3. `validate` and `show` from the Verbs pane; the Load state pane reads the answer.
4. A turn from the Turn pane, through the gate's line bound; its events in the Live
   trace pane as they come, the window's `replayed` flag honoured.
5. `unload`, with the save-point prompt if the run is not at rest.
6. `load` from the page only once WeaverAgent's #166 code lands; until then its form stays
   disabled with its reason, and the act says so.

The act's deliverable is a measured record under `docs/project/`, with the agent page's
panes as the evidence and no box fact in it. Before briefing it, the planner asks the
operator three things from the box: where the agent's scope landed under the #166
ruling, whether a connector restart left the agent running, and what `sudo -n -l`
answered on admin-con's lines (reported as the ceiling the hello declared, never as the
lines themselves). The brief then lands as the stack's first commit, and the stack runs by
`CLAUDE.md`'s loop with one CodeRabbit pass at the end.

## 6. Rulings held, and open questions for the operator

Held on 2026-11-09, each to be ruled when the connectors are in hand:

1. **No install document enters the repository for now.** Install steps stay local;
   a document for the public repository comes later, if at all, stated in terms of the
   configs `register` writes and the connector configs' named facts, with placeholders
   and no value.
2. **The unit shape** (#46 item 1) is held until the end-of-week box work. Under the #166
   ruling admin-con's unit needs no cgroup delegation, so item 1 is moot in substance, but
   the issue closes only with the unit decision.
3. **`load` from the front end** joins the round trip only after #166's code lands
   (section 5, step 6).

Open questions, put to the operator on #40 on 2026-11-09, unanswered:

- Whether `model.unrecovered_call` becomes an alert reason under Spec 2.14, and if so with
  what subject and for whom. Today's reasons are a door's failed or unknown outcome, a
  person's ask refused for a system reason, lost contact, the connectors' entries and
  `data_lost`.
- That no surface renders the `unrecovered_call` block as the model's attempted call,
  since the repository renders no conversation yet; the issue stays open for a surface
  that does.

## 7. Later work noted, not briefed

- **The live view and `Last-Event-ID`.** The stream ignores a reconnecting browser's
  `Last-Event-ID` and resends the window; the module dedupes by clearing its list. A later
  act could resume after the id with a discontinuity mark where the ring moved past it
  (#65's body). It changes the tested stream and waits for a reason.
- **The `#166` note** leaves the PRD, the design system README and `stages.md` when
  WeaverAgent's code lands (section 4).
- **#11** (answers behind the writer queue during a replay of near-bound frames) and
  **#18** (the management plane epic) stay open as the tracking issues they are; **#1**
  and **#2** wait on the WeaverAnalysis seam, as does #23.

## 8. How the seats worked this week, for the next planner

- **A stack costs one CodeRabbit pass**, taken when the operator undrafts the stack PR;
  sub-PRs against the stack branch are skipped by CodeRabbit and read by Codex alone. The
  operator undrafted #61 early once, with a sub-PR still to come, and had the planner
  re-draft it; a planner undraft of any base-`main` PR needs the operator's explicit word
  each time (given once, for #66, after a one-hour timer).
- **At most one CodeRabbit-costing event an hour**: an undraft, or a push to a PR already
  out of draft. The last was #66's undraft at 16:53 UTC on 2026-11-09.
- **When `main` moves under a stack, the executor merges `main` into the stack branch on
  the planner's word** before the next sub-PR branches from it; a sub-PR built on a stale
  stack head is rebased, not reopened.
- **A reviewer's finding is verified against the code before it is routed.** The one
  Codex finding this week was real (an unbounded list in the browser) and landed as its
  own sub-PR rather than a push to the stack, by the operator's choice, so the stack's
  pass stayed one.
- **Measure the right volume.** On this box `df /home` reports the root filesystem, not
  the volume the repositories sit on. The executor's housekeeping after an act drops the
  merged sub-PRs' target directories and worktrees and keeps `main`'s target.

## 9. What this document does not do

It does not brief the round trip, rule on the install, the unit shape or the alert, or
touch the code. It records no ruling the operator did not make, and the founding
boundaries stand: the server never reaches the agent, no credential, box path or posture
enters the repository, and admin-con's sudo invoker is the one privileged invocation.

## Manifest

    Base        WeaverWeb main at 6ef1f41 (#66 merged 2026-11-09 18:02 UTC), tree clean before this file
    Measured    cargo build, clippy (both feature sets), test with DATABASE_URL at 6ef1f41 on 2026-11-09: pass, 0 warnings, 438/0/1, nothing skipped
    Produced    this document, as a draft docs PR the operator undrafts and merges
    Around it   WeaverAgent #499 collapsing, #513/#514 open, #166 ruled 2026-11-07 and unlanded; the cutover begun 2026-11-08
    Waiting on  the Granite agent with its LoRA adapter answering through the gate (2026-11-10/11), the connectors on the box at the end of the week
    Asked       section 6's three held rulings and the two #40 questions, then section 5's three box questions, then the round trip's brief
