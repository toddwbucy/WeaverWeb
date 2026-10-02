# Brief: act 2, the link and the registry

Version: v0.1, 2026-10-01. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. Act 1 (PR #3, merged 2026-10-01 at `74fc414`) wrote the design into
`docs/weaver-web-Spec.md`; this act builds the server's half of it. **The Spec governs.
Where this brief and the Spec disagree, the Spec wins, and the disagreement is named in
the PR body.** Read Spec sections 2.12, 3, 3.2, 7.2, 8 and 9 whole before writing code.

## 1. What this act builds

The server side of the link of Spec section 8 and the register of agents of section
2.12, so that acts 3 and 4 (gate-con and admin-con) have something to connect to and
build against. After this act the `weaver-web` binary:

- holds a durable certificate authority and refuses to start without it;
- offers the register verbs that mint credentials and write client configs;
- listens for the two connectors over mutual TLS, admitting by fingerprint, refusing
  before the roster, holding one connection per credential, heartbeating, resetting
  link state and bumping its epoch at start, deriving presence, and closing a revoked
  connection at once;
- lands observations on the agent row under section 2.12's rules (arrival sequence,
  `list` on its own row only, replayed events never writing the row, `show` asked after
  the replay boundary on every admin-con admission);
- keeps the per-agent live window the seed's `traceview.rs` server half already holds,
  with the acknowledged position per process and nothing persisted.

The connectors themselves, the trace tailer, the legacy `web/` removal and every
surface are later acts. **Tests drive the server with fake connectors holding minted
credentials.** No agent binary runs and no agent socket is opened.

## 2. The register

**Migration `0010`** builds section 2.12's table and what it needs. Carry the row's
members as the Spec lists them, each with its date where the Spec gives it one, and:

- the identity `ag-` and sixteen hex, through the `identity!` macro in `src/store/key.rs`
  as `AgentId`, with the compile-fail doctest the other kinds carry;
- the author and version members of section 3.2, the version the store's counter;
- two credentials per row, each a fingerprint, a plane (`gate` or `admin`), a state
  (`live` or `revoked`) and the state's date, **never a key**;
- per plane: the link state with its date and the incarnation that wrote it, and the
  observed address with its date;
- the tuple and the load state as admin reported them, each with admin's date and the
  arrival sequence (epoch, number) that landed it;
- **a partial unique index over (box, name) where a credential is live**, which is the
  schema instrument of `web-one-live-row-per-box-and-name`;
- **a server-state table of one row** holding the listener's epoch, incremented once
  per start in the same transaction as the startup reset.

The name is immutable; the schema should make that true (no update path for it) and the
verbs below never offer it. Migrations are checksummed by sqlx at connect: this is a new
file, and nothing in `0001` to `0009` moves.

## 3. The register verbs

Subcommands of the `weaver-web` binary, run on the server by the operator, each an
authored edit under section 3.2 (the author member holds the name the verb is given,
or null; the version refuses a stale edit). The set, in the order the Spec implies:

1. **authority init**: creates the server's key and certificate at the path the server
   config names (`authority_dir` or the name the act chooses). Refuses to overwrite.
2. **authority rotate**: a new authority. The Spec says this is by definition a
   re-registration of every agent; the verb says so in its answer and revokes every
   credential, and the operator re-registers each agent after.
3. **register `<box>` `<name>` --out `<path>`**: writes the row, mints the two client
   certificates signed by the authority, stores their fingerprints, and writes two
   client config files (one per connector) to `<path>`, each carrying its own key and
   certificate, the server's certificate, the server's link address, the agent's name
   and the plane. Re-registering a live (box, name) retires the previous row by revoking
   its credentials in the same transaction, which the partial index forces.
4. **revoke `<agent>` `<plane>`**: sets the credential's state, and if that credential's
   connection is live in a running server, closes it at once (section 8's election).
   State how the verb reaches a running server's connection: the act's choice, but the
   Spec's serialization rule means the verb and the listener share one exclusion per
   credential, so the simplest shape is that the verb is served by the running server
   (a local admin socket or a signal the server handles) rather than by a second process
   writing the row behind the listener's back. Name the choice and its reason.
5. **rotate `<agent>` --out `<path>`**: mints two new credentials, revokes the old two,
   writes a new client config pair. Both planes drop until the install script carries
   the new config.

Every answer is one JSON object on stdout with the exit status agreeing, the shape
`weaver-admin` uses, so the install script and the operator read one convention. **No
verb writes anything into the repository's tree, and no test leaves a key behind
outside the test's temporary directory.**

## 4. The listener and the link

**Dependencies.** `rustls`, `tokio-rustls` and `rcgen` (or their equivalents), added
with `--locked` and the reason for each stated in `Cargo.toml` as the file's other
entries do. Fingerprints are SHA-256 over the certificate's DER through the `sha2`
already present. Pin the TLS configuration so that the only accept path is mutual TLS
with the server's authority as the sole trust root for client certificates; there is no
plaintext listener, and the seed's `link_listen` TCP accept in `src/bin/weaver-web.rs`
and `wire::serve` go with this act (section 7 below).

**Framing.** NDJSON over the TLS stream, one frame per line, the seed's `wire.rs` shape
with `svc` as the tag, rewritten under `src/link/` rather than extended. The vocabulary
this act fixes, which acts 3 and 4 then implement from the client side:

- **hello** (client to server): the roster, which is the agent's name and the plane, a
  check against the certificate's binding and never a source. On the admin plane it
  also carries the file position admin-con reports as its tail, which fixes the replay
  boundary.
- **hello answer** (server to client): the send cadence (the silence bound divided by
  four), and on the admin plane the acknowledged position this server process holds for
  the row, or none after a restart.
- **heartbeat** (client to server), at the cadence.
- **turn** ask and answer on the gate plane (the seed's `Turn` frames: id, text in;
  close or typed error out).
- **verb** ask and answer on the admin plane (id, verb, agent; the answer is admin's
  JSON object verbatim plus exit status, the seed's `VerbOutcome` shape).
- **event** (admin to server): one trace event with its position (generation, offset at
  a record boundary, digest of the last acknowledged line) and a `replayed` flag.
- **ack** (server to admin): the position the server has landed through.
- **refusal** (server to client), typed, before the connection closes: not live, roster
  mismatch, already connected, silence.

A frame on the wrong plane (a tuple or load state member arriving on a gate
connection, a turn on an admin connection) is refused and logged against the row.

**Admission**, under one exclusion per credential: TLS handshake; fingerprint lookup in
the register; refuse if absent or revoked before reading a byte of the hello; read the
hello; refuse on a roster that disagrees with the binding; refuse if a connection is
already installed for this credential; install with a fresh incarnation; write the
connected link state and the observed address naming that incarnation; recheck liveness
and close if revoked meanwhile. On the admin plane, after installation: fix the replay
boundary from the hello, then ask `show` for the row over this connection before
accepting the connection's first replayed event. Answer the hello.

**The heartbeat.** The silence bound is a member of the server's config, default sixty
seconds, and nothing else configures it. A connection silent for the bound is closed
and its link state written disconnected, the write naming the incarnation and landing
only while that incarnation is still the live one.

**Start.** Before the listener accepts: load the authority or refuse to start; in one
transaction, increment the epoch and set every plane recorded connected to
disconnected with the start's date, leaving already-disconnected planes' dates alone.

**Presence** is derived, never stored: both planes connected on one row. Expose it as a
read beside the register's others (the Agents surface will want the whole row); a
surface is not this act's.

**Observations** land per section 2.12: numbered (epoch, arrival) by the listener, a
member taking the higher sequence; a `list` answer writing only the connection's own
row; a replayed event writing nothing on the row; the tuple and load state from the
admin plane alone, by `show`, `list` and live events. The live window takes every event,
replayed or not, into the row's ring with discontinuity marks as the seed does.

## 5. What this act proves, and how

Each of these section 9 rows is **owed to this act** and flips from owed to standing
when its instrument lands. For every perturbation, show in the PR body that the test
fails when the guard is removed, and name the guard:

- a connection whose credential is not live is refused before its roster is read
- one live connection per credential (all three perturbations on the row: replacement,
  revocation racing admission, stale teardown)
- a hello's identity is its certificate's binding and never its roster
- at most one row per box and name holds live credentials (at the schema)
- the link state is reset when the listener starts
- an agent is present only when both planes connect from one row
- the server's authority is loaded before the listener starts and never minted at start
- the client credential is stored as a fingerprint and never the key (at the schema)
- the tuple is admin's word and never gate-con's (every perturbation on the row that
  the server can stage with fake connectors: the data-plane write, the replayed write
  after a restart, show before the boundary, out-of-order answer, list on another row,
  skipped drain and overlapping verbs where the fake admin-con can stage them; name the
  ones that wait for the real admin-con of act 4)
- nothing crosses the link in the clear (a plaintext hello refused; and the review row)

**Stays owed after this act:** the trace file is replayed from the acknowledged
position, which is admin-con's (act 4), and the batch's order (unrelated). Update the
owed paragraph's counts and mark each landed row's instrument as standing. Every file
written to these claims opens with `//! conforms: web-<assertion>` naming the graph
nodes section 8 and 2.12 carry.

**Tests need a database.** The DB-backed tests connect through `DATABASE_URL` and skip
without it; the register's tests must not skip in the PR's run. Record in the PR body
that they ran against a database, with the count.

## 6. What leaves with this act, and what stays compiling

- `wire.rs` and `src/bin/weaver-web-connector.rs` leave: their frames are superseded by
  `src/link/`, and keeping a plaintext dialed link in the tree contradicts the clear-link
  assertion. `src/bin/weaver-web.rs` loses the TCP link listener and its event pump and
  gains the TLS listener, the start sequence of section 4, and the register verbs.
- `adapters/gate.rs` and `lifecycle.rs` stay as library code, untouched, the seeds of
  acts 3 and 4. `traceview.rs` stays: its server half (rings, marks) is used by this act,
  its tailer half is act 4's seed and is not spawned.
- `web/` (the legacy `/admin` routes) stays compiling until act 5. It reads the roster
  from `wire::Link` today; take it from the register instead, or leave the legacy
  roster empty with a one-line note, whichever is smaller. Name the choice.
- `src/registry.rs` is the legacy participant model and is not the register of agents;
  leave it for act 5.
- `CLAUDE.md`'s commands block gains the register verbs and the config members this act
  adds, and its "Becomes the connectors" paragraph is updated to say what landed. Pick
  up any litter walked past and name it.

## 7. Process

Branch from `main`, one `code:` commit or a few, draft PR with `Implements:` lines
naming Spec sections 2.12, 3.2, 8 and 9 and the 7.2 clauses the server enacts. Gates
before review: `cargo build --locked`, `cargo test --locked` with `DATABASE_URL` set,
`cargo clippy --all-targets --locked` clean, `cargo fmt`. Check visibility before
pushing: `gh repo view toddwbucy/WeaverWeb --json visibility`. No box path, credential
or key enters the tree; test fixtures mint their own under a temporary directory.
Message the planner with the PR number and a summary of what changed, what stands with
its reason, and which perturbations were shown to fail.

If the Spec as landed cannot be built as written at some point, stop and tell the
planner before writing around it; the same act changes whichever of code and Spec is
wrong, and the PR body names it.
