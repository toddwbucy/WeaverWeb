# Brief: the connector's side of the service's socket

**Date:** 2026-11-27. **From:** the WeaverWeb Executor, on the Planner's word of 2026-11-27, from the Executor's plan of 2026-11-26 that the Planner took as its ground. **Stack:** `act/the-connectors-side-of-the-socket`, cut from `main` at `24ac7a3bee09a842a303c9df804ecfe7ab386943`.

**Builds against:** `weaver-admin-operator-contract` section 8, "The service's socket", on WeaverAgent's `main` at `265a332dd399ae633147d580e9602f38dd12de6b` (merged 2026-11-27), sha256 `040e707942b65ee10f986e1559e21db7b835cfae2cb82e304c63c048cd3e3f31`. The four exchanges are the ones this repository filed as toddwbucy/WeaverAgent#645. Every reference below to "section 8.x" is to that contract.

## The goal

admin-con invokes nothing itself. It asks the weaver-admin service four things on the box's local socket: which verbs are granted, run this verb, the trace from this position, and the admin log's lines. The repository then holds **no privileged invocation at all**. Each connector's private key moves into a file of its own.

## The operator's words that bind it (2026-11-26)

- "77 (f). Yes, the second shape. Two conditions. Weaver-admin checks the grant itself on every run-this-verb request and does not trust admincon for it. The local socket's contract leaves room for a fifth message that carries the loop's text for loop install."
- "77 (c). Yes, for both connectors." The question asked for each connector's key in a file of its own, which the service's account cannot read.

## Already ruled out

- **Passing the link through the socket.** admin-con keeps the link, its frames and its state machine. The socket carries a smaller protocol, the contract's.
- **Linking anything of WeaverAgent's.** The fake service is this repository's own, written from section 8.10. It reaches no agent and no privileged program.
- **The fifth message** (section 8.9). The loop's third act brings it. This stack speaks version 1 alone.
- **A path, group, account or mode of the box in the repository.** The socket's path is a box fact in admin-con's config, as `trace_socket` is today, and the install writes it.
- **The gate.** gate-con's turns and `gate_socket` do not change. Only its key moves to a file of its own.

## Where the contract moved the plan of 2026-11-26

1. **A run stands from its grant** (section 8.4). Machine A takes it, with its id drawn, once the grant's read answers granted and before the root program starts. A `run` answer lists it with `started_ms` null until then.
   - In the drop table, a standing run counts as standing whether or not it has started.
   - A run whose root program could not start leaves `standing`, with nothing kept as `ended`. The table reads that as "did not run", so the server hears `not_started`.
2. **A connector that holds no run id** (section 8.4, third reading): the plan's rule, now the contract's.
   - Its run is standing where a standing run names its verb.
   - It ended where `ended` names its verb with a `run` that is not the last id it learned had ended.
   - Otherwise it did not run.
3. **A close before the trace's header has three causes** (section 8.5): the relay's refusal of the position, a relay that died after the dial, or a request that reached the relay after its window. **The connector may confirm** by opening once more at the same position. A second such close is read as the refusal, and is not proof.
4. **The connector sends its whole trace request within 1 second of reading `open`** (sections 8.5 and 8.7).
5. **An older trace connection is closed only when the relay admits a newer request**, never at the newer one's opening (section 8.1). This is the relay's rule today, so the door's handover from the opening's stream to the verifying stream does not change.
6. **The newest log connection replaces the older when it opens** (section 8.6). admin-con holds one log connection at a time, so this asks nothing of it.

## The children, in order

Each child carries its own Spec sections and section 9 rows, passes the repository's gates alone, and measures a perturbation for each claim it adds.

1. **This brief.**
2. **The socket's client and the fake service.**
   - New: `src/link/admin_service.rs` and `src/link/fake_service.rs` (test only).
   - The client: the opening line (`version` 1, `purpose`, the configured agent), the answer's `open` with its `instance` or one `refused` line, the framing (one JSON object a line), and the bounds of section 8.7. The opening's answer is read for 4096 bytes within 11 seconds, and a control line for 256 KiB.
   - The fake: every capability section 8.10 lists, at a temporary path, admitting the test's own uid.
   - Tests: every opening word of section 8.2 read by the client; an `open` past 4096 bytes; an answer past its time.
   - Perturbation: the 4096-byte bound dropped, and its test is red.
3. **The grants and the verbs over the socket.**
   - New: `ServiceInvoker` behind today's `Invoker` trait (`src/link/admin_con.rs`:98-114). The verb plane above the trait does not change: the drain, the one invocation slot, `verb_bound_secs` and `stop_grace_secs`.
   - **Deleted:** `src/link/sudo_invoker.rs`, `src/link/sudo_invoker_tests.rs`, the fake `sudo` and its `PATH` guard.
   - The drop table and the service's words are as stated below. `VerbFault::NOT_GRANTED` lands in `src/link/frames.rs`, and the server adds it to `NEVER_RAN` (`src/link/listener.rs`:2732-2736).
   - `link::admin_con_tests` move from the fake invoker to the fake service.
   - Perturbations, each turning its test red:
     - another `instance` read as "did not run";
     - the service's `busy` read as `fault`;
     - the last-known id dropped from the reading with no id held;
     - `not_granted` left out of `NEVER_RAN`.
4. **The trace through the socket.**
   - `relay::dial` (`src/link/relay.rs`:129) opens a `trace` connection, reads `open`, and writes today's request within 1 second. The header and stream code that follows does not change.
   - `relay_unreachable`, and every other refusal of the opening, is a closed door (`Dial::Closed`), never `Dial::Refused`.
   - A close before the header is confirmed once (decision 5).
   - `src/link/fake_relay.rs` folds into the fake service.
   - Perturbations: `relay_unreachable` mapped to `Refused`, so the door-closed test is red; the confirmation dropped, so the window-passed test marks a refusal.
5. **The log through the socket.**
   - `src/link/admin_log.rs`, the polling thread on the file, is replaced by an async reader of a `log` connection.
   - `line` and `text` become today's entries, with `own` equal to `through_service`. `oversized` and `discontinuity` keep today's markers.
   - Any close, or an `open` after a refusal, is a discontinuity entry. Reconnection backs off from 1 to 30 seconds.
   - Perturbations: `own` judged by uid again; the discontinuity at a reopen dropped.
6. **Each connector's key in a file of its own, both connectors.**
   - `LinkConfig.key` becomes `key_file`, an absolute path (`src/link/client.rs`). The key file holds the PKCS #8 PEM alone, and the certificate is checked against it at every load.
   - **The trust rule, on both files:** root owns them, their group is the connector's own, and their mode is 0640 or tighter, with nothing for others. Their folder is root's and writable by neither group nor others. Each is opened without following a link and must be a regular file. This replaces "of its own uid at 0600" (`client.rs`:101-113).
   - **`request`:** run as root with `--group`, it sets the owner, group and mode by descriptor before the first key byte is written.
   - **`adopt`:** runs unprivileged. It checks the answer against the request's public half, never opening the key, and writes `--out`, which a fixed root program installs.
   - **`register` and `rotate`:** they write four files, and the admin's agents page hands over four.
   - Perturbations: a group-writable key admitted; a config of the connector's own uid admitted.
7. **The Spec and the last guard.**
   - Spec 7.2, 8, 2.13 and 2.14, and the section 9 rows: no privileged invocation in the repository.
   - `tests/no_privilege.rs` admits no exception. Perturbation: the invoker's file restored, and the test is red.

**The shared transcript fixture.** The other seat carries one file with its service's child: each message of the four exchanges once, and each `refused` word, as JSON lines.
- It lands here byte for byte as `tests/fixtures/admin-socket-transcript.jsonl`, pinned by sha256 on both sides, as the loop's wires fixture is.
- Child 2's client and fake each read every line of it.
- Until it arrives, child 2 builds against the contract alone, and the child that takes it is named when it does.

## The service's words at the server (child 3)

| The service says | admin-con answers the server | The server's audit |
|---|---|---|
| `ran` | the outcome, as today | failed or done by exit code, as today |
| `fault` | `fault` | unknown, as today |
| `not_granted` | `not_granted` (new) | failed: "was not run" |
| `grant_unread`, `not_started`, `full` at the opening | `not_started`, the message naming the word | failed |
| `busy` | `busy`, the message naming the run | failed |
| `ask_malformed`, `ask_outstanding`, `ask_unknown`, `verb_unknown`, `name_refused`, `opening_malformed` | `fault`, and a connector failure entry, since each is admin-con's own defect or its config's | unknown |
| `grants_unread`, or no socket, at the hello | an empty ceiling and today's `ceiling_not_read` entry | |

## The connector after a drop (child 3)

admin-con asks one verb at a time and keeps `last_known`, the id of the last run it learned had ended. When a control connection drops with a verb outstanding, admin-con keeps the invocation slot, opens a new control connection and asks `run`, every 2 seconds:

| The `run` answer | admin-con |
|---|---|
| same `instance`, its run standing (by id, or with no id held, a standing run of its verb), `started_ms` null or not | polls on. At `verb_bound_secs` it answers `unknown`, as today, and holds the slot until the run ends |
| same `instance`, `ended.run` its id (or, with no id held, `ended` naming its verb and a `run` that is not `last_known`) | answers that `ran` or `fault`, and frees the slot |
| same `instance`, nothing of its own | answers `not_started` (it did not run, and may be asked again), and frees the slot |
| another `instance` | answers `unknown`, frees the slot. The next `show` settles it, and the root program's own lock guards a run from before |
| no socket within `verb_bound_secs` | answers `unknown` |

An orderly stop polls `run` until `standing` holds none of its runs or its grace ends, and invokes nothing, as today.

## Decisions the contract leaves open, each the Executor's

1. **The names:** the stack `act/the-connectors-side-of-the-socket`, its children `act/the-connectors-side-of-the-socket-<n>-<topic>`, and the fixture's name above.
2. **One control connection per ask.** A verb holds its own connection until its final line. The hello's grants ask and a `run` poll each open their own. At most three stand at once, under section 8.1's eight.
3. **The hello's grants keep today's bound**, `grants_bound`, 10 seconds, over the opening and the ask together. Past it the ceiling is empty with today's entry.
4. **Config:** one box fact, `admin_socket`, absolute, with no default, replaces `trace_socket`, `weaver_admin` and `admin_log`.
   - Absent, admin-con starts as today's unlaid connector: no relay dialed, no verb invoked, an empty ceiling.
   - A config that names any of the three retired members is refused at load, with a sentence that names the member, so an old config never starts silently unlaid.
5. **The trace confirmation is taken**, once, where the opening's deadline leaves room for it. Otherwise the close is read as the refusal, as today.
6. **The log reopens** on a backoff of 1 to 30 seconds, and every reopen marks a discontinuity.
7. **`grant_unread` and a `full` opening map to `not_started`**, not to kinds of their own. Nothing ran in either case, and the message carries the service's word.
8. **`last_known` lives in admin-con's memory alone.** A restarted admin-con holds no outstanding ask, so it needs none.

## A server and a connector of different ages

- **The server installs first.** The link's frames do not change, except the one new fault kind. An older server that receives `not_granted` writes no outcome and raises "whether admin acted is unknown" (`listener.rs`:2725-2745). That is safe but overstated.
- **The new admin-con and the service ship in one appliance install, never apart.**
  - A new admin-con on a box with no service finds no socket. It degrades to an empty ceiling with its entry, a closed door and no log entries, and does not crash.
  - An old admin-con on a box with the service holds no sudo rule and is not the relay's reader. It reads nothing granted and is refused at the relay.
- **The configs move with the binaries.** A new connector refuses an old config, whose key is inline and owned by the connector's uid at 0600, with a sentence naming why. The install lays the new files in the same act.

## The candidate text for the repository's `CLAUDE.md`

This text is the operator's to place. This stack edits no instruction set.

- In place of the hard boundary "One privileged invocation in this repository, admin-con's sudo invoker, and nothing else":

  > **No privileged invocation in this repository** (operator's rulings of 2026-11-26, revising the 2026-10-03 rule of one; Spec 7.2). admin-con runs nothing through sudo: it asks the weaver-admin service on the box's local socket for the grants, a verb, the trace and the admin log's lines, under `weaver-admin-operator-contract` section 8, and the service reads the grant itself on every verb. The server sends an abstract verb and a principal for the record. Nothing from the link enters any command, since the service runs fixed lines alone. Every verb passes three gates: this server's IAM, the ceiling the service's grants answer gives admin-con for its hello, and the service's own read of the grant at the verb. No socket path, group, account, mode or sudoers text enters this repository; cite WeaverAgent's documents instead.

- In the Commands paragraph on admin-con, the three box facts become one, `admin_socket`, and "runs verbs through `link::sudo_invoker`" becomes "asks the service through `link::admin_service`". The tree's paragraph on the sudo invoker goes.

## The acceptance

- `tests/no_privilege.rs` admits no exception, and fails where any file invokes a privileged program.
- Every exchange of section 8 is driven against the fake service, with every refusal word read and every row of the drop table tested.
- The shared fixture's every line is read by the client and the fake, once it is carried.
- Both connectors refuse an inline key and a config of the old ownership, with sentences.
- The repository's gates at each child's head, the suite run with `DATABASE_URL` and `PGUSER`, nothing skipped.

## The scope boundary

- Not in this stack: the fifth message, the gate, the pairing's protocol, the server's link frames beyond `not_granted`, and any edit of an instruction set.
- The service, its unit and the box's layout are the other repository's.
