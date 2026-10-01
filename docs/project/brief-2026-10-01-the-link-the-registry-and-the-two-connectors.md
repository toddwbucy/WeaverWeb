# Brief: the link, the registry, and the two connectors

Version: v0.1, 2026-10-01. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. Every ruling below is the operator's and is dated; everything measured
was measured on the thinkpad on 2026-10-01 against `main` at `4ce4f41`. The founding
handoff and the second handoff of 2026-09-30 still govern what this repository is;
this brief says what the operator ruled since and what the first acts are.

## Superseded in part during PR #3's review, 2026-10-01

The body below is a dated record and is not rewritten. Four of its rulings changed
while act 1 was under review, and the Spec section named governs each:

- The cross-row mismatch mark of section 3 is dropped; presence alone satisfies the
  both-must-match ruling, and a row says which plane is missing (Spec section 8).
- admin-con tails the agent's `File` sink and nothing is teed; the sink listener and
  the tee of sections 2, 3 and 5 are withdrawn (Spec section 7.2).
- The server's copy of the trace is a live window and not a record, and the
  acknowledged replay position is per server process (Spec section 7.2).
- The registered agent's name is immutable, and at most one row per box and name
  holds live credentials (Spec section 2.12).

Where this brief and the Spec disagree, the Spec governs, and act 4 is briefed afresh
from the Spec.

## 1. The rulings

**2026-09-30, the design session** (recorded in `docs/design/README.md` on the branch
`claude/loving-feynman-dwwr1u`, pushed and not yet merged): the two connectors are
Weaver-Web's and not the agent's. The frontend's server never reaches an agent; a
client beside the agent is the one party that does, and that client is this
repository's. The seed is `src/bin/weaver-web-connector.rs`, `src/wire.rs`,
`src/lifecycle.rs`, `src/adapters/gate.rs` and `src/traceview.rs`, which the founding
handoff's step 1 had listed as leaving.

**2026-10-01, the operator**, in the planning session:

- The lane's first priority is the two connectors. **admin-con** is the management
  plane: it takes the agent's trace through the sink handoff and runs the lifecycle
  verbs. **gate-con** is the data plane: the agent's conduit to the outside world,
  tools and user input included. gate-con is the operator's name for what the
  whiteboard and the handoffs called web-con.
- Both connectors are **clients**. They connect to the Weaver-Web service; the service
  listens.
- The service keeps a **registry** of agents, because it serves many at once. A row
  positively identifies the agent, where it runs, and its declared tuple.
- **Keys are created on the server** at registration and handed to the client through
  its config file at setup. The client holds the server's key and the server holds the
  client's: two-way identity confirmation, and the traffic is encrypted.
- **One connection at a time per connector, ever.**
- **An agent must have both connectors on the same server and they must match.**
  Positive identification of an agent to a Weaver-Web instance is both keys.
- This is the bulk of the backend. Everything after is presentation of what comes out
  of the two connectors.

## 2. What the contracts say, measured 2026-10-01

Both door contracts live in `../WeaverAgents/docs/crates/contracts/` and are the
pages a client builds against. Read them before writing a word.

- **`weaver-gate-world-contract`.** A named local Unix socket between raise and
  lower. NDJSON, one request per line; the request is one JSON object with one
  required member, `text` (`weaver-gate-Spec` line 824), and a line bound the Spec
  sets. One close line out, naming its kind, the run and the turn, with `finish:
  "length"` only when the generation was cut. Admission is by kernel peer credential
  against the declaration's `allowed-uids`. One turn in flight per agent; a second
  request waits. Streaming is deferred. The client learns nothing of the interior
  (section 6). The seed's `src/adapters/gate.rs` already implements this, dial per
  turn, with the section 5 refusals typed.
- **`weaver-admin-operator-contract`.** There is no admin socket: it retired on
  2026-08-05. Nothing crosses in. The verbs are invocations, `sudo weaver-admin
  <verb> <agent>`, one JSON object on stdout, exit status agreeing
  (`weaver-admin-Spec` section 2). Six verbs parse today: `load`, `unload`,
  `validate`, `stop`, `show`, `list` (`crates/weaver-admin/src/surface.rs`). The seed's
  `lifecycle.rs` runs three and still infers load state from the gate socket's
  existence, which `show` and `list` replaced on 2026-09-04.
- **The sink is the handoff.** What crosses out is the trace, NDJSON, one event per
  line, to a sink admin opens at load under root (`weaver-admin-Spec` section 5).
  Three kinds: `File`, `Pipe`, `Socket { path }`. For a socket, admin connects to a
  listener the operator's tooling already holds, and a load with nothing listening is
  refused. **admin-con is that listener.** The seed tails a file instead
  (`traceview.rs`), so the listener is new. Durability of the record is the
  operator's, not the program's (contract section 3), so what admin-con does with
  the stream before relaying it is this repository's to decide.

## 3. The design these rulings fix

- **Two binaries, `admin-con` and `gate-con`**, sharing the link code. gate-con needs
  only a uid in the agent's allow list; admin-con needs the sudo rule and must be up
  before any agent loads with a socket sink. Splitting keeps the privileged posture
  off the data plane.
- **The link is mutual TLS with the Weaver-Web server as its own certificate
  authority.** Registration on the server mints one client certificate per
  connector, two per agent, each bound to the agent's row and to its plane, so a
  gate-con credential cannot speak as admin-con. The client's config carries its own
  key and the server's certificate; the server stores the fingerprint of each client
  certificate and never the key. An install script writes the client config on the
  agent's box, and nothing of it enters any repository.
- **The hello is refused before its roster is read** when the certificate is not live
  in the registry. A second connection on a credential already connected is refused
  rather than replacing the first. A heartbeat on the link lets the server close a
  silent connection after a bounded interval, so a dropped connector can reconnect;
  that interval is the one tunable.
- **An agent is present only when both its connectors are connected from credentials
  on the same row.** A mismatch marks the row and every surface says so.
- **The tuple and the load state arrive through admin-con only**, by `show` and by
  the load event in the trace, which names the declaration's hash. gate-con is
  forbidden by its contract from learning any of it.
- **Revocation is per credential, server-side; rotation mints a fresh pair.** Both are
  registry verbs rather than edits. A credential is bound to one Weaver-Web instance;
  moving an agent to another instance is a re-registration.
- **admin-con tees the stream to an append-only file beside the agent before
  relaying it**, so the record survives a link drop. Recommended by the planner on
  2026-10-01, not yet ruled; the Spec text should state it as this crate's election
  with the reason.

## 4. Act 1, this brief: the documents

No code. A `docs:` act on a branch from `main`, opened as a draft PR.

0. **The second handoff lands in this act.**
   `docs/project/HANDOFF-2026-09-30-the-workspace-turns-to-weaver-web.md` sits in
   the tree uncommitted, as its own manifest says, and its section 5 step 1 asks to
   be landed together with the `CLAUDE.md` amendment. This brief lands with it.
1. **`docs/weaver-web-Spec.md` section 8** is rewritten to say section 3 above: the
   link, registration, the credentials, the refusals, one connection per connector,
   both-must-match, the heartbeat, revocation and rotation, the per-instance bound.
   The present text (two processes, one dialed link, one address) is superseded and
   removed, since git is the archive.
2. **A new subsection of section 2 for the registered agent**: the row's members (the
   agent's name, the box it reports from and the address the server observed, the
   declared tuple as admin reported it and when, the two credentials as fingerprints
   with their plane and their state, each connector's link state, the load state as
   admin's word with its date), written by the registration path under section 3.2's
   rules, so it carries the author and the store's version like every authored row.
   Say in the text that "registry" in section 1's tree means the whole store and that
   this is the register of agents, so the two readings do not collide.
3. **Sections 7.1 and 7.2** are amended: gate-con and admin-con are this crate's
   binaries, the one party that reaches the agent; 7.2 names all six verbs and the
   sink listener. Every `web-con` in the tree becomes `gate-con`.
4. **Section 1's tree** names the two binaries under `link/` and `seams/`, or
   replaces those two entries with what the act decides, with the reason stated.
5. **Section 9** gains one row per new claim, each with its instrument: a connection
   whose credential is not live is refused before the roster is read; one live
   connection per credential; an agent is present only when both planes match one
   row; the client credential is stored as a fingerprint and never the key; the tuple
   is admin's word and never gate-con's; nothing crosses the link in the clear. A
   perturbation row states what removal makes it fail.
6. **`CLAUDE.md`** is amended: the big picture and the boundaries say the connectors
   are this repository's and are the one reach; the "Leaves" paragraph is replaced by
   what section 3 keeps and what still goes (`web/`, the legacy `/admin` routes);
   interface issues go to `toddwbucy/WeaverAgents` until the operator rules otherwise;
   the second handoff is named beside the first; a line says the suite `CLAUDE.md`
   one directory up loads here and does not bind; and two facts for the removal act,
   that `askama.toml` lists `src/web/templates` and that the legacy router is the only
   thing serving htmx and sse.js. The seat names take the operator's capitalisation,
   `Thinkpad-WeaverWeb-Planner` and `-Executor`.
7. **`docs/weaver-web-PRD.md` section 5** (placement and the boundary rule) is amended
   where it places the connectors outside this crate. Grep the PRD for `web-con`,
   `connector` and `WeaverTools` and amend each hit or say in the PR body why it
   stands.
8. **`docs/design/README.md`** lives on the unmerged design branch. If that branch has
   landed when this act starts, rename web-con there too. If not, leave it and name
   the gap in the PR body; the design branch is the operator's to land.

**Acceptance.** Every ruling in section 1 is traceable to a sentence in the Spec.
Every new section 9 row names an instrument. ASCII only, absolute dates, no history
banners, superseded text removed. No box path, credential or security posture beyond
what the Spec must say to be a Spec: the fingerprint posture is already in section
2.8 and is the model. Check visibility before pushing: `gh repo view
toddwbucy/WeaverWeb --json visibility`. The PR body carries `Implements:` lines naming
the Spec sections touched, and names the litter picked up on the way.

## 5. The acts that follow, in order

Each is its own branch and draft PR, briefed when the one before it is cleared.

- **Act 2, the link and the registry.** Dependencies (`rustls`, `tokio-rustls`,
  `rcgen` or equivalent, chosen with `--locked` and the reason stated in
  `Cargo.toml`). Migration `0010` for the registered agent. A server verb that
  registers an agent, mints both credentials, and writes a client config to a path
  the operator names, never into the tree. The listener refusing a hello before the
  roster is read, the one-connection rule, the heartbeat, the both-match rule. Tests
  drive the server with fake clients holding minted credentials; the perturbations
  of section 9 are shown to fail when their guard is removed.
- **Act 3, gate-con.** Its own binary carrying `adapters/gate.rs`, on the link. A test
  with a Unix listener speaking the gate's line shapes; no agent binary runs.
- **Act 4, admin-con.** The sink listener with the local tee, all six verbs, `show`
  and `list` replacing the socket-existence inference. A test with a fake admin that
  connects to the sink socket and writes NDJSON.
- **Act 5, the removal.** `web/`, the legacy `/admin` routes and their assets route go;
  `askama.toml` loses `src/web/templates`; the surfaces get their own asset route for
  htmx and sse.js.
- **Then the surfaces**, Agents first since it renders the registry, then Live.

**Environment, before act 2's tests can assert anything:** a database and a role for
the store must be created on the box, which is the first environment act, per the
second handoff's section 5.

## 6. What this brief does not do

It does not rule the tee (section 3's last item) or the heartbeat interval; both are
stated as elections for the operator to confirm in review. It does not touch the
design branch. It does not change the hard boundaries: the server never reaches an
agent, no credential or box path enters a repository, and nothing links the interior.
