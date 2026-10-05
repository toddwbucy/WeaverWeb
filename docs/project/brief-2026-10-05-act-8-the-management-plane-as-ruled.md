# Brief: act 8, the management plane as ruled (documents)

Version: v0.1, 2026-10-05. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. Issue #16, part of epic #18. A `docs:` act, no code: it makes the Spec and
CLAUDE.md true against the operator's rulings of 2026-10-03 and WeaverAgent's merged
documents, so that act 9 (#17, the code) has text to build to. Keep it **narrow**: each
change is named below, and nothing else in the Spec moves. Where a fact is the code act's
to settle, say so and stop.

## 1. Read first

- WeaverWeb #18 (the shape in force), #16 (this act), #14 and #15 (the two interface
  issues and their answers, which this act closes on our side).
- WeaverAgent's merged documents on its `main`, which are the authority for every box
  fact below. Read them from the WeaverAgent checkout beside this repository, never edit
  them:
  - `docs/crates/contracts/weaver-admin-operator-contract.md`, sections 2 (what crosses
    in: the granted lines, the trace request), 3 (what crosses out: the answer object and
    its bounds, `show`'s constituent pids, the lifetime binding, the stream), 5 and 6.
  - `docs/crates/weaver-types/weaver-types-Spec.md` section 3.1 (`TraceRequest`,
    `TraceHeader`, `TraceLine`, `TraceControl`, the framing and the digest rule).
  - `docs/crates/weaver-admin/weaver-admin-Spec.md` sections 2, 3 and 6, for reference.
- Our own Spec passages this act replaces, listed in section 3 with their line numbers at
  `main` `6d92db9`.

## 2. The facts this act writes against

All from the rulings of 2026-10-03 and WeaverAgent #72, #75, #77, #79, #80, merged.

1. **The lifecycle has no socket.** admin-con reaches the verbs by running fixed
   `weaver-admin <verb> <agent>` command lines through a strict per-agent sudo rule the
   box installs: admin-con's own service user, `NOPASSWD`, `sudo -n`, no caller-chosen
   argument, nothing on stdin, `!pam_session`, no `SETENV`. The verbs are `show`,
   `validate`, `load`, `unload`, `stop`; `save-point` and `restore` are owed and no rule
   grants them yet. An observer's rule grants `show`; an operator's adds the other four.
2. **The ceiling comes from the sudo rules.** admin-con asks `sudo -n -l <exact line>`
   for each verb it knows and declares exactly the granted set in its hello. There is no
   `grants` ask and none will be added. Listing without a password holds only while the
   user's sudo entries are all `NOPASSWD`, which the rule's narrowness already requires.
3. **The person never crosses to the box.** The cause the agent records is the uid sudo
   reports, admin-con's. WeaverAgent #51 is closed as superseded. Which person asked is
   this crate's audit record, per Spec 2.13.
4. **The answer's bounds.** One JSON object on stdout, at most 64 KiB. Exit 0 is an
   answer, 1 a refusal; any other status, or a status with no object, is a fault the
   caller answers by reading the next `show`. stderr is diagnostics no caller parses.
   `load` answers once the agent is up or refused, within admin's own bound, 900 s by
   default, which admin-con's bound must exceed. `unload` takes at most 105 s. An
   invocation finishes even when its caller disappears.
5. **`show` names the run's constituents**: the pids of the worker, the state member and
   the trace relay, where a run stands; absent otherwise. (Promised on #15; its landing
   on WeaverAgent's code is owed, so the Spec writes the rule and the code act flags it.)
6. **The trace crosses through a relay socket.** The start step launches a relay beside
   the worker, running as its own account in the trace group. It admits exactly one
   reader, admin-con's service user, named as `trace-reader` in the agent's boundary
   file. The reader sends one `TraceRequest` line (`offset`, `prior_digest`) within five
   seconds, at most 4096 bytes. The relay refuses a position whose prior record does not
   hash to the digest (sha256 hex of the record's bytes from the start of its line
   through its newline; absent only at offset zero). It then writes a `TraceHeader` line
   (device, inode, birth time in ns, or absent), then the trace's own lines byte for
   byte from that position, following at end of file with heartbeat lines. Control lines
   are `TraceLine` objects whose one member is `trace_stream`: `header`, `heartbeat`,
   `truncated`. After `truncated` the stream ends and the reader resumes from offset
   zero. The relay holds the run's file by descriptor, so a rotation is invisible to it
   and shows as a new identity in the next run's header. The newest connection from the
   reader replaces the old. One sink per agent; each run appends and opens with its
   `load` event; a stream resumes across runs within the file.
7. **The relay runs only while the agent runs.** A closed door while the agent is down
   is the normal state.
8. **The agent's lifetime is bound to its admin-con** (#15, operator's ruling).
   admin-con's service contains the agent it starts, under control-group kill, never
   `KillMode=process`. An orderly stop of admin-con unloads its agent first; a kill is an
   unclean stop whose next load resets to the latest save point; the invoker's resource
   limits contain the agent. A lost link never unloads an agent: the trace is the record
   of what the agent did meanwhile, and the server catches up from it. Recovery decisions
   are WeaverWeb's, carried to admin-con as ordinary verbs; admin-con's one act on its own
   initiative is that `unload`. Without a cgroup-capable supervisor only the orderly stop
   holds; the gap is stated, not imitated.
9. **A verb runs to completion.** At its bound admin-con leaves the process running,
   answers `unknown`, and the next `show` reads the real state. A verb counts as started
   only once its invocation begins (already written by #13).
10. **gate-con's admission** is open on WeaverAgent #70 (the allow-list moves out of the
    declaration into the agent's root as boundary). This act leaves Spec 7.1's citation
    of #61 as a citation of #70 and changes nothing else on the data plane.

## 3. Changes to the Spec

Line numbers are at `6d92db9`; find the passages by their text where they have moved.

### 2.13, identity and access (around lines 2425-2452)

- Replace the paragraph "Access to the verbs is a role on the box ... left the tree with
  the act that built admin-con" whole. Its replacement says, in this order:
  - access to the verbs is the box's sudo rule (fact 1), and this crate holds exactly one
    privileged invocation, admin-con's sudo invoker, on the operator's ruling of
    2026-10-03 revising the 2026-10-02 rule that no sudo stands anywhere here;
  - why the operator's reason still holds: the server never sends a command, only an
    abstract verb; admin-con maps it locally to the granted line; nothing that crossed
    the link enters the command; argv is built from constants and the configured agent
    name; `sudo -n`, stdin closed; the invoker is one module, and `tests/no_privilege.rs`
    pins that nothing else in the repository invokes privilege (the test's change is the
    code act's, named as owed in section 9);
  - the person never crosses (fact 3), so section 2.13's audit is the one record of who
    asked;
  - **delete the unreadable-trace rule** ("A trace admin-con cannot read leaves the link
    up and the verb plane closed ...") and its graph node
    `web-an-unreadable-trace-asks-no-verb`. Replace it with: the ceiling comes from the
    sudo rules (fact 2), a verb is asked whenever the ceiling grants it, and the trace
    door's state (open while the agent runs, closed otherwise, fact 7) is shown on the row
    and never gates a verb. The ordering of `show` answers against trace events (the four
    clauses of section 9) holds while the door is open; while it is closed there are no
    events to order against, and a `show` answer lands at receipt with its source.
- Line 1309 (the server principal's asks): drop "`grants` is ...". The server principal
  asks `show` only.

### 7.2, the admin verbs and the trace (around lines 2470-2560)

- Replace the paragraph "What crosses out of the agent is the trace, and admin-con tails
  the file it lands in ... never the data plane's" so that the crossing is the relay
  socket (fact 6). Keep the 2026-10-01 reasoning that a file sink loses nothing to an
  admin-con restart, and add why the relay replaces the tail: admin-con's user reads
  the record through no grant of its own, so the territory's layout gives it nothing,
  and one declared reader is the whole of the box's fan-out; more readers are this
  crate's. Keep the harness's-word sentences as they are.
- Rewrite "The trace file is replayed from an acknowledged position" (around 2494) in
  the relay's terms, keeping the election: the server's acknowledged position is
  (the file's identity from the header; a byte offset at a record boundary; the sha256
  hex of the record ending there), which is exactly a `TraceRequest` plus the header
  check. admin-con resumes the relay from it. **State the digest rule in the relay's
  words** (sha256 hex over the record's bytes from line start through newline). Where
  admin-con's present position differs from that (its own digest definition, its
  generation from `birthtime`), say the mapping is the code act's and name it as owed.
- **The backfill after a server restart.** The relay cannot start mid-file without a
  digest, and admin-con no longer reads the file. Election for this act: admin-con
  requests from offset zero through the relay, skips locally until within
  `backfill_bytes` of the tail, and relays from there, so the link and the server's
  window stay bounded while the local stream is not. State it as this document's
  election of 2026-10-05 with that reason.
- **Truncation and rotation**: `truncated` ends the stream and admin-con resumes from
  zero, marking the discontinuity as today; a rotation shows as a new identity in the
  next run's header and is marked the same way. Keep the "never smoothed" rule.
- **The door closed** (fact 7): admin-con's hello says whether the door is open; the row
  records it with its date; a closed door while the load state is `unloaded` is normal
  and renders as such; a closed door while a run stands is shown as such and is for the
  operator to read, never a fault this crate raises. admin-con redials on its backoff.
- **The bounds** (fact 4): replace every mention of admin-con's verb bound as a fixed
  five minutes (`VERB_BOUND`, around 2658) with a config member the install sets above
  the box's load bound; add the unload bound; the answer's 64 KiB and exit statuses;
  `load` answering once up or refused. A dropped or timed-out verb: fact 9.
- Verbs: `save-point` and `restore` named as owed and not granted (fact 1).

### 8, placement and the link (around lines 2790-2800, 2923-2956, 2984-2992, 3040-3050)

- The service-user paragraph (2790-2800): admin-con's user holds the access group and
  the sudo rule's lines and nothing of the agent's; WeaverAgent's `create-agent.sh`
  creates that user, the rule and the trace-reader declaration (#79). "Neither holds any
  privilege ... no sudo rule" becomes the one-exception sentence, cross-referenced to
  2.13. gate-con's sentence cites #70.
- **The three gates** (2923-2956): first unchanged; second becomes the ceiling derived
  from the sudo rules by `sudo -n -l` on each exact line, declared in the hello, the
  single source (fact 2); third becomes the box's sudo rule itself (fact 1). Delete the
  `grants` paragraph and the "Until #50 lands" sentences. Replace with: the ceiling is
  what the rules grant today, so an agent whose rule grants nothing declares an empty
  ceiling, which is honest and not a placeholder.
- **The principal** (2984-2992): the frame keeps its `principal` member for admin-con's
  own log and for the row; admin-con passes nothing to admin (fact 3); the sentence
  "admin-con passes it to admin for the box's operations log, per #51" goes.
- **The lifetime** (new paragraph, after the reconnect policy around 3040): fact 8,
  whole, with the install's obligations named (the unit's kill mode, the stop timeout
  covering the load bound plus the unload bound plus a margin, the containment check
  after the first load) and the containment check at every load from `show`'s pids
  (fact 5), a failed check logged by name, marked on the row, and admin-con serving no
  verb until a load passes it. The orderly stop's sequence: waiting asks `not_started`,
  the verb in flight finished, `unload` through the invoker, exit.
- The lost link (same place): never an unload; the trace is the record; the server
  catches up from the acknowledged position.

### 10, open elections (around line 3426)

- The observer role is `show`; drop `grants`.

### 9, enforcement

- Delete the owed row "a connection whose trace is unreadable is asked no verb" and the
  sentence in the owed paragraph that names it. Add two owed rows for the code act:
  - "the ceiling declared in the hello is exactly what the box's sudo rules grant":
    perturbation, declare a verb `sudo -n -l` refuses, and the server asks it and the
    box refuses it;
  - "no privileged invocation exists outside admin-con's sudo invoker": the
    `tests/no_privilege.rs` row rewritten to allow exactly that module, shown to fail
    when sudo is planted elsewhere or the invoker builds argv from anything but its
    constants and the agent name.
- The trace-replay row (3256): keep, reword its clauses to the relay (a refused
  position, `truncated`, a new identity in the header). The mid-file backfill clause
  follows the election above.
- Update the owed count and the paragraph in the same commit. Node identifiers stay
  unless a claim moved; keep conformance headers in sync if one does.

### 2.12, the registered agent

- The row gains the trace door's state (open or closed, with its date), beside the link
  states. Spec only; the column is the code act's.

## 4. CLAUDE.md

- Line 56: "admin-con runs the verbs and tails the trace file" becomes runs the verbs
  through the box's sudo rule and reads the trace through its relay socket.
- Lines 78-85, the privilege bullet: rewrite to the one-exception rule, the three gates
  as above, and "Until #50 lands no verb runs" deleted (#50 landed 2026-10-03).
- Lines 171-177, the admin-con paragraph: `trace_file` becomes the relay socket path
  (a required box fact with no default); the tail becomes the relay stream; the ceiling
  from `sudo -n -l`; `NoVerbs` named as what ships until act 9. Say the config members
  are the code act's and name them as such.
- The "What this repository is" section's admin bullet: "the sink admin opens at load"
  becomes the relay the start step launches.

## 5. Out of scope

- Code of any kind, including `tests/no_privilege.rs`, the config members, the column
  for the door's state. All are #17.
- The install script.
- gate-con's admission (#70).
- The PRD. If a PRD sentence contradicts this act, name it in the PR body for a
  follow-up rather than editing it.

## 6. Acceptance

- No sentence in Spec 2.13, 7.2, 8, 9, 10 or CLAUDE.md states the 2026-10-02 shape: no
  "no sudo anywhere", no `grants` ask, no role check, no principal claim to admin, no
  tailed file, no unreadable-trace rule, no fixed five-minute bound.
- Every box fact is cited to WeaverAgent's document and section, and **no group name,
  socket path, mode, uid or sudoers text enters this repository** beyond the words
  "the agent's access group" and "the box's sudo rule". The one example that is
  WeaverAgent's (`weaver-<agent>-admincon`) stays in their documents, not ours.
- Section 9's counts, owed rows and paragraph move in the same commit.
- ASCII only, absolute dates, superseded text removed. Check visibility before pushing.
- PR body with `Implements:` lines per Spec section, `Closes #16`, and "Closes #14,
  #15" since both are answered and this act is what they asked for. Name any PRD
  contradiction found.
