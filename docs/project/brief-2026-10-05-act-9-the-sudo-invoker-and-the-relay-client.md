# Brief: act 9, the sudo invoker and the relay client (code)

Version: v0.1, 2026-10-05. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. Issue #17, part of epic #18. The code for the management plane as the Spec now
states it (act 8, PR #19, merged as `6926e3b`). **Two pull requests, in order**, each a
`code:` act, because the two halves are separable and Codex's passes scale with an act's
size: 9a is the invoker and the stop, built against the file tailer admin-con has today;
9b replaces the tailer with the relay client and lands the door. Both are "Part of #17";
9b closes it.

The Spec is the text: sections 2.12, 7.2, 8 and 9 at `6926e3b`. Where this brief and the
Spec disagree, the Spec wins and the disagreement is a finding for the PR body. Run the
five-class checklist (two copies of one fact, ambiguous commit, multi-file state not
atomic, filesystem trust, unbounded or unjoined) on every change and name the classes
in the PR body. Every perturbation is shown to fail with its guard removed.

## Facts on WeaverAgent's `main` that the code builds against

Read them in the WeaverAgent checkout (fetch it first; it is behind) or through `gh api`:
- `crates/weaver-types/src/wire.rs`: `LifecycleAnswer::State { state, load, constituents }`.
  `constituents` is the run's pids (worker, state member, relay), present where a run
  holds the lock, absent otherwise. `InTransition` is an answer `show` gives while
  another invocation holds the box's invocation lock, and claims no state.
- `docs/crates/contracts/weaver-admin-operator-contract.md` sections 2, 3, 5: the granted
  lines `weaver-admin <verb> <agent>` for `show`, `validate`, `load`, `unload`, `stop`;
  one JSON object on stdout of at most 64 KiB; exit 0 an answer, 1 a refusal, anything
  else or no object a fault; stderr diagnostics; `load` returns once up or refused within
  the box's bound (900 s default); `unload` at most 105 s; an invocation finishes even
  when its caller disappears.
- `docs/crates/weaver-types/weaver-types-Spec.md` section 3.1: the relay's wire.
  `TraceRequest {offset, prior_digest}` as one JSON line within 5 s, at most 4096 bytes;
  then `TraceHeader {device, inode, birth_ns}` as `{"trace_stream":{"header":{...}}}`;
  then the trace's own lines byte for byte; `{"trace_stream":{"heartbeat":{"wall_ms"}}}`
  while idle; `{"trace_stream":{"truncated":{"size"}}}` ends the stream. The digest is
  sha256 hex over the record's bytes from line start through newline, absent at offset
  zero. A position that does not verify is refused before a byte is sent. The newest
  connection replaces the old.
- `toddwbucy/WeaverAgent#88` (open): the header may gain the file's length. Build
  without it; see "the three measures".

**We link none of their crates.** Re-declare the shapes we read in our own types, as
`frames.rs` already does for ours.

## 9a: the sudo invoker, the stop, the slot, the test

Branch from `main`, draft PR, `Part of #17`.

1. **The invoker module**, `src/link/sudo_invoker.rs`, the one place in the repository
   that invokes privilege. It implements `Invoker` for a configured agent:
   - `grants`: for each verb it knows (the five), run `sudo -n -l <weaver_admin> <verb>
     <agent>` and take exit 0 as granted. Answer exactly the granted set. Say in its doc
     that listing without a password holds only while every entry of the user is
     `NOPASSWD`, which the box's rule guarantees and the install must not widen.
   - `run`: `sudo -n <weaver_admin> <verb> <agent>`. argv is built from three constants
     (`sudo`, `-n`, the verb from a fixed table) plus two config values (`weaver_admin`, an
     absolute path, and `agent`). **Nothing that crossed the link enters argv or the
     environment**: the `Principal` is logged by admin-con and never passed. stdin is
     `/dev/null`. The child is started in its own session (`setsid`) so admin-con's own
     signals never reach it. stdout is read to at most 64 KiB plus one byte; more is a
     fault. stderr is captured and logged at the end, never parsed. Exit 0 with one JSON
     object is an answer; exit 1 with one object is a refusal; anything else, or no
     object, is `VerbFault` with a kind of its own (`fault`), the message naming the
     status, and the caller reads the next `show`.
   - **The child is never killed.** At `verb_bound` the future answers `unknown` (as
     `answer()` already does) and the child keeps running; a reaper owns it until it
     exits, so no zombie remains and no second child of the same verb starts. `verb_bound`
     is a config member (below), not the constant `VERB_BOUND`.
   - A fake `sudo` for tests is **generated at test time** in a temporary directory and
     placed first on the child's `PATH` by the test, never tracked, so `tests/no_privilege.rs`
     never meets it. It records argv and stdin, and answers scripted objects, exit
     statuses, delays and oversized output.
2. **The invocation slot is admin-con's own**, one per process, held across connection
   attempts until the child exits and is reaped (Spec 7.2). Today it is per connection
   (`VERBS_IN_FLIGHT` inside `relay`). Move it to the process: a verb asked on a fresh
   connection while a child from the old one runs queues behind it under `VERB_QUEUE`
   and `busy`. Perturbation: free the slot at reconnection and a verb runs beside the
   detached child (the fake sudo sees two argv records overlapping).
3. **The orderly stop** (Spec 8): waiting asks `not_started`; the verb in flight waited
   for until its child exits and is reaped; then `unload` through the invoker, its answer
   logged and relayed if the link still stands; then exit. All within `stop_grace`. A
   child that outlasts the grace means the `unload` is not issued, logged by name. The
   `unload` is admin-con's one act on its own initiative; it is not issued where the
   ceiling grants no `unload`, and that is logged. A lost link never unloads: the stop
   is the shutdown signal alone.
4. **Config members** (`AdminConConfig`, its `MEMBERS` list, and `register`'s written
   template): `weaver_admin` (absolute path, required, no default); `verb_bound_secs`
   (default 960, must exceed the box's load bound, documented as such); `stop_grace_secs`
   (default 1080: load bound plus unload bound plus a margin). `trace_file` stays in 9a.
   Refuse a `weaver_admin` that is not absolute.
5. **The constituents on the row.** `land_verb` already lands a `show` answer's `state`
   and `load`; land `constituents` too, with the answer's date, as the row's new member
   (Spec 2.12). Migration `0013_the_constituents_and_the_door.sql` adds `constituents`
   (an integer array, null where none) and `constituents_at`, **and** the door columns
   9b will write (`trace_door` boolean null, `trace_door_at`), so one migration serves the
   act and 9b adds none. `0012` is frozen. `Agent` gains the members; the `agents` verb
   prints them.
6. **`tests/no_privilege.rs`**: allow the word `sudo` in exactly `src/link/sudo_invoker.rs`
   and its own test module, and nowhere else, keeping every other check. Add a second
   instrument in the invoker's tests: argv is exactly the five-member shape from
   constants and config, shown to fail when a string from a `ToClient::Verb` frame (the
   principal's name) is planted into argv. Both perturbations named in the PR body.
7. **CLAUDE.md**: the admin-con paragraph's "Today's build" and "As ruled" sentences
   become one description of what ships; the config members named.

Acceptance for 9a: the executor's gates (`cargo build --locked`, `cargo test --locked`
with `DATABASE_URL` set and the link tests running, `cargo clippy --all-targets --locked
-- -D warnings`, `cargo fmt --check`); the fake-sudo tests cover grants, a run that
answers, a refusal, a fault status, oversized stdout, a timed-out run that is reaped
later with the slot held, the stop's `unload`, and the stop meeting a child that
outlasts the grace; no real `sudo` is ever executed by a test (assert `PATH` order); no
group name, socket path, mode or uid in the repository. PR body: `Implements:` lines for
Spec 7.2 (the invoker, the bounds, the slot), 8 (the stop), 2.12 (the constituents), 9
(the two privilege rows now enforced, the slot clause now enforced), and the checklist
classes.

## 9b: the relay client, the door, the openings

Branch from `main` after 9a merges, draft PR, `Closes #17`.

1. **The relay client replaces `Tailer`.** Config: `trace_socket` (absolute path,
   required, no default) replaces `trace_file`; `backfill_bytes` stays. The client dials
   the socket, sends one `TraceRequest` line, reads the header into the position's
   identity (device, inode, birth_ns; a missing birth time is `None`, never zero), then
   the lines. **The position's digest becomes the relay's**: sha256 hex over the whole
   record ending at the offset, absent at zero; `DIGEST_WINDOW` and the 64 KiB window
   leave. A refused request (the relay closes before a header) is a discontinuity: mark
   it and request from zero. `truncated` ends the stream: mark it and request from zero.
   A new identity in a header against the position's is a discontinuity: mark it and
   relay from zero. Nothing is smoothed.
2. **The door.** The hello gains `door: bool`. `tail` is `None` while the door is closed.
   A new `FromClient::Door { open: bool, wall_ms }` crosses every change on the live
   connection; the listener lands it on the row (`trace_door`, `trace_door_at`) bound to
   the connection, like a link-state write. A closed door is never a fault: the client
   redials on the connector's backoff.
3. **Every opening is an admission of the trace** (Spec 7.2). On the listener: a hello
   with the door closed is admitted with `caught_up` taken as sent (the admission's
   `show`, where granted, is still asked and lands at receipt). A `Door { open: true }`
   frame on a live connection reopens the replay phase: the listener resets its
   `caught_up` state for that connection, treats events until the next `CaughtUp` as
   replayed (window only, no row write), and asks `show` where granted as at admission,
   the `show` served during the replay with no drain exactly as the server principal's
   `show` is today, the opening complete when both its answer and `caught_up` arrive. On
   admin-con: an opening is taken only while no invocation is in flight. Perturbations
   from the Spec 9 replay row: open mid-connection without a boundary and a
   closed-interval event writes the row; admit a closed door with a boundary it cannot
   take and the hello never completes; take an opening while a verb is in flight and a
   long load closes the connection incomplete.
4. **The three measures, the election for this act.** The relay names no length
   (`toddwbucy/WeaverAgent#88`), and a heartbeat comes only while it is idle. Build on the
   heartbeat, and name the residue:
   - the opening's boundary is the position at the first heartbeat after the request;
   - the drain before a verb reads until the next heartbeat;
   - the backfill after a server restart: read from the resume point to the first
     heartbeat keeping only the last `backfill_bytes` in a ring, then emit the ring behind
     a discontinuity mark, then continue live. On admin-con's own restart the resume
     point is zero; while the process lives it is its own last relayed position.
   The residue is a writer that never idles: `caught_up`, the drain and the backfill wait
   on it, bounded by nothing but the writer. State it in the PR body and in the Spec 9
   row as the clause #88 closes; everything else in those rows is re-shown against the
   fake relay.
5. **The fake relay** for tests: a Unix socket server in the test module speaking the
   wire above, scriptable for a refused request, a header with or without birth time, a
   backlog, idle heartbeats, `truncated`, and a replacement connection. The `admin_con_tests`
   that drove the file tailer are rewritten against it; the tailer-only clauses leave with
   the tailer, as the Spec 9 row says.
6. **The register's template** (`register --out`) writes `trace_socket` and `weaver_admin`
   placeholders and no `trace_file`. CLAUDE.md follows.

Acceptance for 9b: the gates; every Spec 9 clause of the replay row either re-shown or
named as the #88 residue; the door's frame and the openings perturbation-verified; the
constituents and door columns written; no box fact in the repository.

## Out of scope for both

- The install script (its own act).
- gate-con's admission (WeaverAgent #70).
- A live run against an agent: the agent on this box was provisioned under the old
  layout, and a live run waits on the operator redeploying it under WeaverAgent's new
  scripts. The acts prove themselves against fakes.
- IAM.
