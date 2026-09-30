# Weaver-Web

The frontend of the WeaverTools suite: the user's and operator's surface
over running weaver agents. It is the suite's first consumer application  - 
built against the framework's public boundary, not inside it.

Two binaries, one crate, serve three surfaces. The **server**
(`weaver-web`) presents HTTP and holds everything that is not
box-bound. The **connector** (`weaver-web-connector`) runs on the
agents' own box, holds every box-bound reach, and dials the server -
loopback when colocated, an IP when the presentation stack lives on
another device. Offloading is one changed address, never an
architectural event. The surfaces:

- **Channel** - multi-party chat: humans and local weaver agents (and
  eventually upstream models) in shared channels. Agents are first-class
  participants, invoked by `@mention`, answering whole turns with their
  run/turn labels linking each answer to the trace that produced it.
- **Lifecycle** *(admin)* - the operator's verbs (`validate` / `load` /
  `unload`) over declared agents, each answer rendered verbatim, load
  state shown from the gate socket's existence and labeled as the
  inference it is.
- **Trace** *(admin)* - a live, turn-bracketed view over each agent's
  NDJSON record, with field selection and search, fault events
  prominent, and discontinuities marked, never smoothed.

## What it talks to

Weaver-Web reaches the agent the way any outside consumer does: over
Unix sockets and by running its binaries. It links no crate of this
workspace. It builds against three contract documents under
`docs/crates/contracts/`, of which two bind it and one does not:

- **The gate contract** (`weaver-gate-world-contract.md`) - the client
  boundary, and a seam of this crate's. How work enters a loaded agent
  and how answers return.
- **The analysis contract** (`weaver-analysis-web-contract.md`) - the
  other seam. The emitter sends a finished record and this crate lands
  what crosses in its own store.
- **The operator contract** (`weaver-admin-operator-contract.md`) - the
  operator boundary. How the trace, the program's one output, leaves it.
  **This one is not a seam of this crate's.** It binds `weaver-admin`
  and names the operator, a human role, as its second party, so this
  crate drives the verbs by running the operator's own program as a
  subprocess and reads the trace as any outside consumer does.

Nothing else is API. The frontend never links the framework's crates,
never imports its internals, and never parses anything the contracts call
opaque. When a contract lacks something the frontend needs, the need is
filed as an ask through the contract's own change protocol rather than
worked around.

## Architecture, in one paragraph

The browser is a display engine - a hard constraint, not a preference.
Server-rendered HTML (askama) with htmx and SSE vendored into the
binary, no SPA, no node toolchain, no client-side state beyond a
session cookie and an SSE cursor. All processing is server-side Rust:
a Postgres-backed channel log with a single writer task, per-agent
single-flight queues that batch queued mentions into one turn, and a
mention router that routes agent-to-agent mentions with
self-invocation suppressed. The connector holds the box-bound half -
a dial-per-turn gate adapter, the sudo verb invocation, and per-agent
trace tailers - and speaks to the server over one NDJSON link it
dials, announcing its agent roster in a hello, streaming trace events,
and answering turn, verb, status, and declaration asks. Link loss
fails turns typed and marks every trace view, never smoothed. The
user surface (gate boundary) and admin surface (operator boundary)
are separate modules behind separate route prefixes with a role gate,
so the coming IAM act attaches authentication to standing roles
instead of rearchitecting.

## Documents

- [`weaver-web-PRD.md`](../../docs/crates/weaver-web/weaver-web-PRD.md)  - 
  what and why: the surfaces, what the crate is not, the record it
  holds, the placement and identity rulings, what the rewrite keeps and
  what it retires, the asks filed upstream, and the open cells.
- [`weaver-web-Spec.md`](../../docs/crates/weaver-web/weaver-web-Spec.md)  - 
  how: every representation election, cited from the code that implements
  it.
- [`deploy/`](deploy/) - the sudoers fragment, example config, and the
  verified agent-setup runbook for this box.

## Running

```sh
cargo build
weaver-web --config /etc/weaver-web/config.toml                # the server
weaver-web-connector --config /etc/weaver-web/connector.toml   # on the agents' box
```

The server requires Postgres (local socket, peer auth, database named
in its config). The connector requires the scoped sudoers fragment
from `deploy/` for lifecycle verbs, and declares the agents with
their gate socket and trace paths. Colocated, the link rides
loopback and both binaries run on one box. See
`deploy/agent-setup.md` for adding an agent.

## Status

The one-binary v1 was built and live-proven: real turns against real
local agents, including two agents answering one message concurrently
from separate GPUs. The two-process split of 2026-08-25 (PRD section
5, Spec section 8) compiles clean, passes clippy and tests, and its
link protocol is exercised end to end against the real connector
binary - hello, trace backfill and live tail, turns with typed
refusals, verbs, declarations, and redial with fresh backfill.
Redeployment of the split shape on the agents' box is the remaining
live proof. v1 trusts its LAN - no authentication or TLS by
deliberate, documented deferral (PRD section 6, which names the IAM
act and two triggers rather than one: identity when the listener
becomes reachable beyond the network the operator admitted, and
transport encryption when the traffic leaves it).

## License

Apache 2.0. See [LICENSE](LICENSE).
