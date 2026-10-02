# Brief: act 3, gate-con

Version: v0.1, 2026-10-01. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. Act 2 (PR #4, merged 2026-10-01 at `bd5e937`) built the server's half of
the link; this act builds the first client of it. **The Spec governs.** Where this brief
and the Spec disagree the Spec wins, and the disagreement is named in the PR body. Read
Spec sections 7.1 and 8 whole, `weaver-gate-world-contract` in
`../WeaverAgents/docs/crates/contracts/`, and `src/link/frames.rs`, before writing code.

## 1. What this act builds

`gate-con`, the data plane's connector: a binary of this crate that stands on the
agent's box, connects to the Weaver-Web server over the mutual-TLS link of Spec section
8 with the gate credential the register verb minted, and relays each turn the server
asks for to the agent's gate socket and its close back. After this act a turn can travel
from `Listener::turn` on the server, across the link, through gate-con, into a real gate
and back.

The client half of the link that gate-con needs (dial, verify, hello, heartbeat,
reconnect, bounded frame reads and writes) is written once under `src/link/client.rs`,
so admin-con in act 4 reuses it and adds only its plane.

Not this act: admin-con, the trace tailer, an install script, the legacy removal, any
surface.

## 2. What the contract and the code already fix

- **The gate** (`weaver-gate-world-contract`, `weaver-gate-Spec` section 4): a named
  local Unix socket, admission by kernel peer credential against the declaration's
  allow list, one JSON request line in with one required member `text`, bounded at 32
  KiB of octets before the delimiter, inclusive, one close line out naming its kind, the
  run and the turn, `finish: "length"` only when cut. A second request waits rather than
  being refused. The seed's `src/adapters/gate.rs` implements this per turn, with the
  contract's section 5 refusals typed, and stays the adapter; reuse it, do not rewrite
  it.
- **The link** (`src/link/frames.rs`): gate-con sends `Hello { agent, plane: gate }` with
  no tail, then `Heartbeat` at the `cadence_secs` the `HelloAnswer` names, and answers
  each `ToClient::Turn { id, text }` with `FromClient::Turn { id, close }` or
  `FromClient::Turn { id, error: TurnFault }`, the fault's `kind` the seed's mapping
  (`unloaded`, `line_too_long`, `delivery_lost`, `bad_close`, `close_too_long`). It
  sends nothing else; the server refuses any admin-plane frame from it as `wrong_plane`.
- **The config** is the file `weaver-web register` wrote: `server`, `server_name`,
  `agent`, `plane`, `server_certificate`, `certificate`, `key`. gate-con needs one more
  member, the gate socket's path, which is a box fact: add `gate_socket` as a required
  member gate-con reads, with no default, filled at install. `gate-con --config <path>`
  takes the path with no default either; a default would be a box path in the
  repository.

## 3. Behaviour

- **Start**: read the config; refuse to start when the file is not a regular file owned
  by the invoking uid with no group or other bits (it carries a private key), when
  `plane` is not `gate`, or when a member is missing. Build the TLS client with
  `authority::client_tls`, verifying the server under `server_name`, never the dialed
  address.
- **Connect**: dial `server`, handshake, send the hello, read the hello answer. Any frame
  before the answer other than a `Refusal` is a protocol fault.
- **Serve**: heartbeat at the cadence; for each `Turn` ask, relay through the gate
  adapter and answer with its `id`. Turns may be in flight together, each on its own gate
  connection as the contract allows, but **bounded**: at most a small configured number
  (default 4) at once, further asks waiting in arrival order. The gate serializes turns
  for the agent anyway; the bound is gate-con's protection against an unbounded task set.
- **Reconnect**: on connection loss or a refusal, close every in-flight gate exchange
  that cannot answer anymore, then reconnect with exponential backoff and jitter, capped.
  `not_live` and `roster_mismatch` mean the credential is revoked or wrong: keep retrying
  at the cap, never faster, logging each refusal loudly with what the operator must do
  (re-install the config); do not exit, since a supervisor would restart it into the
  same loop. `already_connected`, `silence`, `store_unavailable` and `malformed` retry on
  the normal backoff, `malformed` and `wrong_plane` logged as gate-con's own defect.
- **A dead server is noticed within a bound**: a heartbeat write that does not complete
  within the cadence, or no TCP progress (enable TCP keepalive), ends the connection and
  starts the reconnect. A connection that is merely quiet is not dead; the server sends
  nothing unasked on the gate plane.
- **It learns nothing of the interior and reports nothing of it**: what crosses is the
  close the gate returned, verbatim through `GateClose`, or the typed fault. No tuple, no
  load state, no inference from the socket's existence.
- **Shutdown**: on SIGTERM or ctrl-c, stop taking asks, let in-flight turns finish within
  a short grace, abort the rest, close the link.

Spec section 8 says how the server treats a connector; it does not yet say how a
connector reconnects. Add one paragraph there for the client side: the backoff, the two
refusals that mean the credential is wrong and retry only at the cap, and that a
connector never exits on a refusal. Name it in `Implements:`.

## 4. The checklist from act 2

PR #4 took seventeen Codex passes, and almost every finding was one of five classes.
Build against them from the start and say in the PR body how each was met or why it
does not apply:

1. **Two copies of one fact drifting.** gate-con holds little state; the in-flight turn
   set and the connection are the two copies to keep agreeing on every exit path.
2. **Ambiguous commit.** Not applicable to gate-con; it writes no store. Say so.
3. **Multi-file state that is not atomic.** Not applicable; gate-con writes no files.
4. **Filesystem trust.** The config carries a key: refuse a config that is not a regular
   file owned by the caller with mode 0600 or tighter, opened without following a
   symlink. The gate socket path is the operator's.
5. **Unbounded or unjoined.** Bound every frame read at `frames::LINE_BOUND` per read,
   not after buffering; bound every write against the cadence and the close signal;
   bound the in-flight turn set; join every task on reconnect and on shutdown so no
   gate exchange outlives its link.

## 5. What this act proves, and how

**Tests in the repository use fakes, never an agent** (CLAUDE.md's boundary):

- **A fake gate**: a `UnixListener` in a temporary directory speaking the gate's line
  shapes: an answered close, a close with `finish: "length"`, a malformed close, a
  connection that closes without a line, a line past the bound.
- **The real server**: act 2's `Listener` against the scratch database, an agent
  registered by the verbs, gate-con's client loop run in-process with the minted config
  and the fake gate's path. Through `Listener::turn`, a turn crosses the link, the fake
  gate answers, the close comes back intact; each fake-gate failure comes back as its
  typed fault.
- **Refusals and reconnect**: revoke the gate credential and see gate-con closed and
  retrying at the cap; restart the listener and see it reconnect and answer the next
  turn; a second gate-con on the same credential refused `already_connected` while the
  first stands.
- **Bounds**: a server frame past the line bound ends the connection without unbounded
  buffering; more asks than the in-flight bound queue and all answer.
- **Config trust**: a config readable by group or other, a symlinked config, and a
  config whose plane is `admin`, each refused at start.

Each behavioural claim gets a perturbation shown to fail with its guard removed, listed
in the PR body as in act 2.

**Proved against karl, outside the repository.** karl is loaded on the thinkpad and its
gate socket is reachable from your uid. After the suite passes, run gate-con for real:
start a `weaver-web` server against your scratch database, register karl with the verbs,
add `gate_socket` to the written config at the path karl's gate binds, start `gate-con`,
and drive one turn through `Listener::turn` from a small program you write in your
scratchpad that depends on this crate by path. Record in the PR body what crossed: the
close's members as karl returned them, the time a turn took, and that nothing else
crossed. **No karl path, uid or box fact goes into the tree or the PR body**; describe
them ("karl's gate socket") rather than naming them. If a close member differs from what
`GateClose` expects, that is a finding: fix the type and say so.

## 6. Process

Branch from `main`, `code:` commits, the brief landing in the first. Draft PR with
`Implements:` naming Spec 7.1 and 8. Gates before review: `cargo build --locked`,
`cargo test --locked` with `DATABASE_URL` set (every DB-backed test running), `cargo
clippy --all-targets --locked` clean, `cargo fmt`. Update `CLAUDE.md`'s commands block
with `gate-con --config <path>` and its config members. Check visibility before pushing.
Tree back on `main` when pushed. Message the planner with the PR number, what changed,
what stands with its reason, the perturbations, and the karl run.

If the Spec or the contract cannot be built as written at some point, stop and tell the
planner before writing around it.
