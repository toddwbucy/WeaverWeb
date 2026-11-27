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
   - The drop table and the service's words are as stated below. `VerbFault::NOT_GRANTED` lands in `src/link/frames.rs`, and the server adds it to `NEVER_RAN` (`src/link/listener.rs`:2762-2766, read at :2773).
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
   - `log_unread` and `log_behind` are placed in the table of the service's words below, each with its test.
   - Perturbations: `own` judged by uid again; the discontinuity at a reopen dropped; `log_unread` raising an entry at every retry rather than once per change.
6. **Each connector's key in a file of its own, both connectors.**
   - `LinkConfig.key` becomes `key_file`, an absolute path (`src/link/client.rs`). The key file holds the PKCS #8 PEM alone, and the certificate is checked against it at every load.
   - **The trust rule**, the one the Executor's document of 2026-11-26 on how a connector's config and key file must be laid proposed, restated here so the two say one thing. Each check is made by descriptor: the folder is opened first, then the file under it with `openat`, as `register` writes today (`src/link/verbs.rs`).

     | | Config | Key file | The folder holding each |
     | --- | --- | --- | --- |
     | Owner | root | root | root |
     | Group | the connector's own effective gid | the same | the connector's own gid, or root |
     | Mode | no bit outside 0640 | no bit outside 0640 | no write for group or others, nothing for others |
     | Access list | see below | **none beyond the three base entries** | not read |
     | Kind | a regular file, no symlink | a regular file, no symlink | a directory, no symlink |
     | Bound | 1 MiB | 64 KiB | none |

     This replaces "of its own uid at 0600" (`client.rs`:101-113). The connector opens the key file only after the config passes. A re-read that is refused keeps the credential in hand, as today (`client.rs`:942-952).
   - **The access list.** The mode cannot see an access list: on a file that carries one, the mode's group bits show the mask, not the owning group's rights. Under the operator's yes of 2026-11-26 to the layout of the agent's folder of settings, the service's account reads each part of it through one access-list entry of its own. So the connector reads the file's access list (`system.posix_acl_access`, by `fgetxattr` on the descriptor) as well as its mode:
     - **The key file is refused where it carries any entry beyond the three base ones** (owner, owning group, others). The key is apart so that the service's account cannot read it, and an entry is exactly how that account would read it. A filesystem that keeps no access lists (`ENOTSUP`) has none to carry, and the file is judged by its mode.
     - **The config admits named entries that grant read alone.** It is refused where any named entry, the owning group's entry or the mask grants write or execute. Others' bits stay from the mode, and must be nothing.
     - Any other error reading the list refuses the file. Nothing is admitted on a list that was not read.
   - **The refusals, with their words** (the 2026-11-26 document's, with two added):
     - `<path> is a symlink, and a file carrying or naming a key is read through no symlink`
     - `<path> is not a regular file, and a connector's config and key are each one`
     - `<path> is owned by uid <n>; a connector's config and key are root's, and this connector only reads them`
     - `<path> has group <g>, not this connector's group <h>; a connector's config and key are read through its own group alone`
     - `<path> has mode <mmmm>; a connector's config and key are 0640 or tighter, with nothing for others`
     - `<path> carries an access-list entry beyond its owner, group and others; a connector's key is read by its own group alone` (new)
     - `<path> carries an access-list entry that grants more than read; a connector's config is written by root alone` (new)
     - `<folder> is writable by group or others, or not root's; a connector's config and key stand in a folder only root writes`
     - `<path>: key_file is not an absolute path`
     - `<path>: the key is not one PKCS #8 ECDSA P-256 PEM block`
     - `<path>: the certificate is not over the key in <key path>`, `adopt`'s `key_mismatch` check, run again at every load.
   - **`request`:** run as root with `--key-file <path>` and `--group <name>`. It creates the key file new, then sets root, that group and mode 0640 by descriptor before the first key byte is written. It prints the request line as today.
   - **`adopt`:** runs unprivileged, with `--request`, `--answer`, `--key-file` (written into the config, never opened) and `--out`. It checks the answer against the request's public half and writes `--out`, which a fixed root program installs.
   - **`register` and `rotate`:** they write four files, and the admin's agents page hands over four.
   - **Tests:** they write the access-list entries by `fsetxattr` on files in a temporary folder of the test's own uid, which a seam takes as the expected owner.
   - **Perturbations, each turning its test red:**
     - a group-writable key admitted;
     - a config of the connector's own uid admitted;
     - the key file's access-list check dropped, so a key carrying a named read entry is admitted;
     - the config's check dropped, so a config carrying a named entry that grants write is admitted.
7. **The Spec and the last guard.**
   - Spec 7.2, 8, 2.13 and 2.14, and the section 9 rows: no privileged invocation in the repository.
   - `tests/no_privilege.rs` admits no exception. Perturbation: the invoker's file restored, and the test is red.

**The shared transcript fixture.** The other seat carries one file with its service's child: each message of the four exchanges once, and each `refused` word, as JSON lines.
- It lands here byte for byte as `tests/fixtures/admin-socket-transcript.jsonl`, pinned by sha256 on both sides, as the loop's wires fixture is.
- Child 2's client and fake each read every line of it.
- Until it arrives, child 2 builds against the contract alone, and the child that takes it is named when it does.

## The service's words, every one placed (children 3, 4 and 5)

**The two final lines of a verb:**
- **`ran`:** admin-con answers the outcome, as today. The server audits it failed or done by exit code. `answer` becomes the outcome's `answer` and `stderr` its `stderr`.
- **`stderr_cut`:** not carried. Where it is true, admin-con logs that the service cut standard error at 16 KiB. Today's invoker cuts at the same 16 KiB and carries no mark of it, so the link's frame does not change for it. The frames change by the one fault kind `not_granted` alone.
- **`fault`:** admin-con answers `fault`. The server audits it unknown, as today, since the root program ran.

**Which control connection a word arrives on.** admin-con opens a control connection for one of three asks (decision 2): **G**, the hello's grants; **V**, a verb; **R**, a `run` poll after a drop.
- At G, every refusal gives an empty ceiling, today's `ceiling_not_read` entry, and a connector failure entry naming the word.
- At R, a word that may pass (`full`, `opening_late`) waits for the next poll. Any other word ends the poll: admin-con answers `unknown` and adds a failure entry.

**Each word, at each purpose it can answer, with what admin-con does at V, on a trace connection (T) and on a log connection (L).** Each cell is a test against the fake service. "Entry" is a connector failure entry, which the server alerts on.

| Word | Control, V | Trace | Log |
|---|---|---|---|
| `opening_malformed` | `not_started` and an entry | a closed door, retried on the backoff, and an entry | no log, reopened on the backoff, and an entry |
| `opening_late` | `not_started` | a closed door, retried | no log, reopened |
| `version_unknown` | `not_started` and an entry | a closed door and an entry | no log and an entry |
| `purpose_unknown` | `not_started` and an entry | a closed door and an entry | no log and an entry |
| `name_refused` | `not_started` and an entry (the config's agent) | a closed door and an entry | no log and an entry |
| `full` | `not_started` | a closed door, retried | no log, reopened |
| `relay_unreachable` | cannot answer | a closed door (`Dial::Closed`), retried, as a failed dial today | cannot answer |
| `log_unread` | cannot answer | cannot answer | no log, reopened on the backoff, and **one** entry per change, not one per retry |
| `ask_unknown` | `not_started` and an entry | cannot answer | cannot answer |
| `ask_malformed` | `not_started` and an entry | cannot answer | the connection closes: a discontinuity, reopened, and an entry (admin-con sends no byte there) |
| `ask_outstanding` | the outstanding verb goes to the drop table, since its run may stand; an entry | cannot answer | cannot answer |
| `grants_unread` | cannot answer (G alone) | cannot answer | cannot answer |
| `verb_unknown` | `not_started` and an entry | cannot answer | cannot answer |
| `not_granted` | `not_granted` (new): the server audits failed, "was not run" | cannot answer | cannot answer |
| `grant_unread` | `not_started`, the message naming the word | cannot answer | cannot answer |
| `busy` | `busy`, the message naming the service's run | cannot answer | cannot answer |
| `not_started` | `not_started` | cannot answer | cannot answer |
| `log_behind` | cannot answer | cannot answer | the connection closes: a discontinuity, reopened at once |

**The count.** All 18 words of section 8.2's table are placed: each of the six opening words at control, trace and log (18 places), `relay_unreachable` at trace, `log_unread` and `log_behind` at log, `ask_malformed` at control and log, and the eight control-only words at control. That is 31 places, each with its test.

**Why a defect maps to `not_started`, not to `fault`.** Every refusal before the root program starts (`verb_unknown`, `ask_malformed`, `ask_unknown`, and the opening's words) is certain that nothing ran. The server should audit it failed, never unknown, and the entry beside it tells the operator that admin-con or its config is at fault. `fault` stays for a root program that ran and ended with no answer to read.

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
7. **Every refusal before a start maps to `not_started`**, `grant_unread`, the opening's words and admin-con's own defects alike, and none gets a kind of its own. Nothing ran in any of them. The message carries the service's word, and a defect adds an entry. Only `not_granted` is a new kind.
8. **`last_known` lives in admin-con's memory alone.** A restarted admin-con holds no outstanding ask, so it needs none.

## What an appliance's install must do differently

One list, for the other repository. **W** marks what this repository builds, and **A** what the appliance's install builds.

1. **The key is made by `request`, run as root** with `--key-file <path>` and `--group <the connector's group>`.
   - W: the command, which sets root, that group and mode 0640 by descriptor before the first key byte.
   - A: running it as root for each connector, and the group, which holds the connector's account alone.
2. **The answer is adopted by `adopt`, run unprivileged** with `--request`, `--answer`, `--key-file` and `--out`. It never opens the key, so no account that runs it reads one.
   - W: the command.
   - A: running it under an account that reads neither key file.
3. **A fixed root program installs `adopt`'s output as the connector's config.** This is new work for the other repository, and asked of it by name. It checks the file, then lays it root's, the connector's group and 0640, in the connector's part of the agent's folder of settings, with the service's access-list entry granting read where the layout gives the service one.
   - A: the whole of it. Its name and arguments are the other repository's to choose.
4. **The files each connector then holds, and who reads each:**

   | File | Owner, group, mode | Who reads it |
   | --- | --- | --- |
   | admin-con's config | root, admin-con's group, 0640 | admin-con, and the service through its one access-list entry where the layout lays one |
   | admin-con's key file | root, admin-con's group, 0640, **no access-list entry** | admin-con alone |
   | gate-con's config | root, gate-con's group, 0640 | gate-con, and the service through its entry where the layout lays one |
   | gate-con's key file | root, gate-con's group, 0640, **no access-list entry** | gate-con alone |

   - Each folder holding them is root's and writable by no one else.
   - A: laying them. W: the trust rule of child 6, which refuses any other shape.
5. **One box fact replaces three.** admin-con's config names `admin_socket`, the service's socket, absolute. It no longer names `trace_socket`, `weaver_admin` or `admin_log`.
   - A: writing `admin_socket` with the contract's path, as `local_address` is appended today.
   - gate-con's `gate_socket` and both connectors' `local_address` stay as they are.
6. **What a new connector says to an old config.** It refuses to start, with one sentence:
   - a config with its key inline: `<path>: the config carries its key inline (key); a connector reads its key from key_file, a file of its own`;
   - a config owned by the connector's uid at 0600: the owner sentence of child 6;
   - admin-con naming a retired member: `<path>: trace_socket is retired; admin-con asks the service at admin_socket` (likewise `weaver_admin` and `admin_log`).
   - W: the sentences. A: laying the new files in the same install as the new binaries.
7. **The new admin-con and the service ship in one install, never apart** (below).

## A server and a connector of different ages

- **The server installs first.** The link's frames do not change, except the one new fault kind. An older server that receives `not_granted` writes no outcome (`listener.rs`:2773-2774) and raises "whether admin acted is unknown" (:3332). That is safe but overstated.
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
