# Brief: act 6, admin-con

Version: v0.1, 2026-10-02. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. The management plane's connector, the second client of the link, built on
the shared client half act 3 wrote. **The Spec governs**, as act 5 (PR #9, merged 2026-10-02) leaves it;
where this brief and the Spec disagree the Spec wins and the PR body names it. Read
Spec sections 2.12, 7.2 and 8 whole, `src/link/frames.rs`, `src/link/client.rs` and
`src/link/gate_con.rs` before writing code.

## 1. What this act builds

`admin-con`, a binary of this crate that stands beside an agent as its own unprivileged
service user and connects to the WeaverWeb server with the admin credential the register
verb minted. It:

- **tails the agent's trace file** (group read; never privileged), relays every event
  with its position, replays from the acknowledged position on reconnect, sends
  `caught_up` at the boundary, and marks every discontinuity (Spec 7.2);
- **declares its ceiling** in the hello: exactly what its invoker's `grants` answers,
  which in this act is always empty (Spec 8);
- **answers verb asks** through an abstract invoker: an ask outside the ceiling is
  answered with a typed error, nothing run, connection kept (Spec 8); the ordered stream
  (drain to the recorded tail, invoke with the tailer paused, answer at the invocation,
  one verb at a time per connection) is built now against the invoker so it is ready when
  WeaverAgent #50 lands.

And on the server side, the listener follows the Spec act 4 and act 5 wrote:

- it reads the ceiling from the admin hello, holds it with the live connection, stores a
  copy on the row (an observed member), and checks every ask against the connection's own
  ceiling;
- it asks the admission `show` only where the ceiling grants `show`; otherwise admission
  completes at `caught_up` and the row names live trace events as its source;
- each verb frame carries the principal as a claim (the server, for the admission
  `show`, today).

**There is no privileged code anywhere in this act.** The only invoker implementation
that ships declares an empty ceiling and runs nothing; tests use a fake invoker. The real
invoker waits on WeaverAgent #50.

## 2. Pieces

- **`src/link/admin_con.rs` + `src/bin/admin-con.rs`**, on `link::client`, adding only
  the admin plane, as gate-con does. Config: the register-written link members plus
  `trace_file`, the agent's trace path, a required box fact with no default, and an
  optional backfill bound for the first connection after a server restart (Spec 7.2).
  The same trust rule as gate-con's config.
- **The tailer**, from `traceview.rs`'s tailer half: positions at record boundaries
  only; the generation derived from the file's durable identity (device, inode, birth
  time where reported); the digest of the last acknowledged line; rotation, truncation
  and a regrown file each marked as a discontinuity, never smoothed (Spec 7.2, every
  clause).
- **The invoker**: a trait with `grants()` and `run(verb, principal)`. The shipped
  `NoVerbs` invoker answers an empty `grants` and is never asked to run, since the ceiling
  is empty. A test fake permits a configured set and records what ran.
- **Frames**: `Hello` gains the admin plane's `ceiling`; the verb ask gains the
  principal claim. Move `VerbOutcome` out of `lifecycle.rs` into `link` (frames needs it).
- **Server**: the listener changes above, and a migration `0011` for the row's new
  observed members (the ceiling with its date, the source the tuple and load state stand
  on). `0010` is frozen from here: an operator may run it any day now.
- **The seed goes**: `src/lifecycle.rs` (it invokes sudo) leaves, and `web/admin.rs`'s
  use of its verb list goes with it (the legacy routes answer 503). Then write the test
  the Spec's section 9 owes: the source tree contains no `sudo` invocation, no setuid
  call and no root wrapper, shown to fail when one is planted.

## 3. The checklist

The five classes from act 2, again. How each was met or why it does not apply, in the
PR body. The ones that bite here: **two copies drifting** (the acknowledged position, the
ceiling held with the connection versus the row's copy); **unbounded or unjoined** (the
tailer's reads, the replay's backlog, the verb queue, every task joined on reconnect and
shutdown); **filesystem trust** (the config carries a key; the trace file is read, never
written, and a rotation is a different file, not a path to follow blindly).

## 4. What it proves

Section 9 rows that stand after this act, each perturbation shown to fail with its guard
removed: the trace file is replayed from the acknowledged position (every clause); the
server never asks a verb outside the ceiling (both halves); the admission `show` is
required only where the ceiling grants it; the four tuple-row clauses that are admin-con's
ordering (with the fake invoker); no privileged invocation in the crate. Update the owed
count. The grant and audit rows stay owed to IAM.

**Proved against karl, outside the repository**, as act 3 was: a scratch server,
admin-con tailing karl's real trace file, the replay across a server restart and a link
drop, `caught_up`, the empty ceiling and an admission that completes without `show`. Run
as the executor's uid for the test, which is the one exception to the service-user rule,
and say so. No karl path or box fact in the tree or the PR body.

## 5. Process

Branch from `main`, `code:` commits with the brief landing first, draft PR with
`Implements:` lines (Spec 2.12, 7.2, 8, 9), gates with `DATABASE_URL` set, clippy clean,
fmt, visibility check, tree back on `main`. Message the planner with the PR number, the
checklist, the perturbations and the karl run. Stop and ask before writing around any
Spec sentence.
