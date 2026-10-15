# Brief: act 12, PR 2: the connectors' entries, upstream as alerts

Version: v0.1, 2026-10-15. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 1 (#45) merged at `c086bbe`: the server's alerts, their acknowledgement, the
save-point prompt, lost contact, the unavailable banner, and the operations log. This
PR feeds that mechanism from the box side. Read Spec 2.14 as built first; this PR adds
sources to it and changes none of its rules.

## 1. The operator's rulings

From 2026-10-14:
- **A connector is a conduit.** It creates the pipe and passes the traffic along, never
  touching it.
- **It sends log entries, not logs, so that alerts can be generated for the operator.**
- **What it meets is written locally first, then sent upstream.**

From 2026-10-15:
1. **Every entry goes to the operations log; an alert is raised only for an entry that
   did not come through WeaverWeb.**
   - That means an act on the box by someone else, such as the operator using admin
     directly.
   - Entries of admin-con's own invocations already raise their alerts at the door (PR 1),
     and must not raise them twice.
   - **admin-con tells them apart by the entry's caller uid against its own.**
2. **A loss that already happened is an alert**, explaining what was lost, with no
   options: a forced unload, and a load that reset the agent to its latest save point.
   These answer exit 0, so PR 1 raises nothing for them.

## 2. Read first

- Spec 2.14 as built (#45), Spec 7.2, 8.
- `weaver-admin-Spec` section 8 (the operations log, `admin.log`), at WeaverAgent
  `55fbc66` or later:
  - one NDJSON line per act, admin's own schema and not the trace's;
  - **boundary activity that changes nothing in the agent**: a refused verb, a read-only
    verb, `show`, a `stop` answered at rest, and the trace relay's connects,
    replacements, refusals and disconnects;
  - each line carries the wall time, the command line, the caller's uid, what was asked,
    the boundary file's digest and the outcome;
  - a verb that changes the agent goes on the trace instead.
- `weaver-types` `wire.rs`: `LoadFacts.reset` (`Reset`, `ResetReason`), and the unload's
  answer and event shapes (which member, if any, says an unload turned forced).
- Code: `alerting.rs` (the queue, the worker, `write_line`), `store/alerts.rs`
  (`raise_alert`, the kinds, the source CHECK in `0019`), the doors' `raised()`,
  `link::relay` (how admin-con already reads a box-side stream with positions),
  `admin_con`, `gate_con`.

## 3. Scope

1. **admin-con reads `admin.log`.**
   - A new **required box fact `admin_log`**: absolute, with no default, like
     `trace_socket`.
   - Opened read-only, never following a link, judged a regular file.
   - Each line appended after admin-con starts is read: **no history at start**, because
     the backlog below covers outages and the box's file keeps the rest.
   - Lines are bounded; a line past the bound is replaced by an entry saying so.
   - A truncation or replacement of the file is detected (size or identity) and read
     from its start, with a marked discontinuity entry.
2. **Each line becomes an entry, passed along untouched.**
   - The line is kept as admin wrote it (a JSON object; a line that does not parse is
     sent as a string marked unparsed).
   - Beside it go the connector's own fields: the source (`admin_log`), when read, and
     **`own`**, true where the line's caller uid is admin-con's own (`getuid()`). That
     comparison is the one thing admin-con judges; it reads no other field.
   - Name the uid member you compare against admin's schema in the PR body. A line
     without one is not own.
3. **The connectors' own failures are entries too**, from source `admin-con` or
   `gate-con`. Each is written to the connector's own log (its `tracing` at warn or
   error, as today) and queued upstream. Examples:
   - admin-con: an invocation that faulted, the relay unreachable or refusing, the
     ceiling not read, an outage of the link with its length;
   - gate-con: the gate unreachable outside a turn, an outage of the link.
   List them in the body.
4. **The upstream path.**
   - A new frame on both planes, `FromClient::Entry`, with an identity (the connector's
     run, a random id drawn at start, plus a sequence) and a bounded object. It is
     acknowledged by `ToClient::EntryAck`.
   - **At least once across a link loss**: unacknowledged entries are resent after
     reconnection, and the server dedupes by the entry's identity.
   - **A bounded backlog while disconnected**: a count and a byte bound, oldest dropped
     first. The drops are counted and sent as one entry naming the count once the link
     is back.
   - **The connector never waits on the server for an entry**: the send rides the same
     bounded write path as everything else, never blocks reading `admin.log` or relaying
     the trace, and never holds a verb.
   - **At the server's door, the entry's shape is checked once** (the #41 lesson): its
     identity, its source on an allow-list per plane (`admin_log` and `admin-con` on the
     admin plane, `gate-con` on the gate plane), and its bound. Anything else is refused
     as malformed, like any frame.
5. **What the server does with an entry.**
   - **Every entry is one line in the operations log**: the source, the agent, the
     plane, `own`, and the entry's object. Never a turn's text, which no entry carries.
   - **An alert, through PR 1's queue, only for an entry that did not come through
     WeaverWeb.** A `not own` `admin_log` line whose outcome is a refusal or a fault is
     classified by the doors' rules, by allow-list:
     - **an `unload` refusing `save_point_not_taken` on a leg at rest raises the
       save-point prompt** (this closes PR 1's residue that an S9 entered on the box was
       not detected);
     - `published` and `boundary_unverified` raise `data_at_risk`;
     - another refusal raises `failed`.
     - A `not own` relay refusal raises an alert too (another party trying the trace's
       door).
     - Own lines and successes are the log's alone.
   - **A connector's own failure** raises a `failed` alert, subject the failure's kind,
     deduped by PR 1's index.
   - The alert's `source` names where it came from (`admin`, `admin-con`, `gate-con`).
     The `0019` CHECK widens in a new migration.
6. **A loss that already happened** (ruling 2), read by allow-list at the door, and from
   a `not own` line or the trace where the wire carries it:
   - a `load` whose answer carries `reset` raises a new kind, **`data_lost`**, with no
     options, saying the agent was reset to its latest save point and naming the reason;
   - an `unload` that turned forced raises the same kind.

   **Read the wire for each**: where the unload's answer carries no member that says
   forced, use the trace's unload event if it carries one, and otherwise name it as
   residue in the body. Don't infer it.
7. **The surfaces**: an alert's source is shown, and the agent page's alert list says
   when an alert came from the box rather than through WeaverWeb. Nothing else changes.
8. **Out of this PR**: any history of `admin.log` from before admin-con started; mail or
   any delivery outside the panel; reading the trace for anything beyond item 6.

## 4. Review checklist (distributed state)

- **No new wait on the connector's hot paths**: reading `admin.log`, the relay, and the
  verb plane never wait on an entry's send or ack.
- **The backlog is bounded and its drops are visible.**
- **At least once, deduped**: resend after reconnection and a restart of the server; the
  server's dedupe holds across a reconnection, and a connector restart starts a new run
  id.
- **The door checks an entry once**: nothing past it sees an unchecked entry, and only
  allow-listed fields decide an alert.
- **Never twice**: an act asked through WeaverWeb raises its alert at the door and never
  again from its `admin.log` line.
- **The file**: a link planted at `admin_log`, a FIFO, a truncation and a replacement
  each behave as stated.

## 5. Tests and perturbations

Against the real listener, a fake `admin.log` the test appends to, and fake connectors:
- an own line (the fake's uid) lands in the operations log and raises nothing;
- a not-own `unload` refusal naming `finished` raises the save-point prompt;
- not-own `published` and `boundary_unverified` raise `data_at_risk`;
- a not-own relay refusal raises an alert;
- a load answered with `reset` raises `data_lost`;
- an entry sent, the link cut before its ack, and reconnected is resent and logged once;
- a backlog past its bound drops the oldest and reports the count;
- a malformed entry (an unknown source, past the bound, the wrong plane's source) is
  refused at the door;
- a connector's own failure raises a `failed` alert with its source;
- a truncated `admin.log` is reread from its start with a discontinuity entry.

Perturbations, each shown to fail with its guard removed, in a fresh scratch worktree of
the pushed commit:
- drop the `own` check, and an act asked through WeaverWeb raises its alert twice;
- drop the dedupe, and a resent entry is logged twice;
- drop the backlog's bound, and an outage grows it without limit;
- drop the door's source allow-list, and a gate-con entry claiming `admin_log` is
  classified;
- drop the `reset` check, and a reset load raises nothing.

## 6. Spec and documents

- Spec 2.14: the entries as sources, the own rule, `data_lost`, and the source shown;
  Spec 7.2 and 8: `admin_log` as a box fact and the entry frames; section 9 rows, with
  the count and summary swept in one commit.
- CLAUDE.md: admin-con's new required key, the frames.
- Epic #18: this PR closes act 12.

## 7. Acceptance

- The full suite with `DATABASE_URL` before every push; clippy with and without the
  default features, `-D warnings`; `cargo fmt --check`; `tests/startup.rs --ignored`.
  Perturbations in a fresh scratch worktree of the pushed commit, each log read against
  its claim.
- ASCII only, absolute dates. **No box path, uid, mode or posture in the repository**:
  `admin_log` is config, and tests make their files at test time. Check visibility
  before the push.
- PR body: `Implements:` naming Spec 2.14, 7.2 and 8; the rulings of section 1; a table
  of every entry source and what it raises; the connectors' own failures listed; the
  uid member named; the `data_lost` wire table (what is read, from where, or residue);
  the perturbation table; "Refs #18".
