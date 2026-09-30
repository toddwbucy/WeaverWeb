# Handoff: the weaver-web session

Received 2026-09-30 from the olympus Planning seat of `toddwbucy/WeaverTools`, base
`ad4ede6f` (main on 2026-09-30, with WeaverTools #758, the tee reading, open). Kept here
verbatim below the rule because it is this repository's founding document. The
decision it asked for is recorded at the end.

---

Version: v0.1, 2026-09-30. Written from the olympus Planning seat for a new session
that creates the weaver-web repository and works there, with its own objectives and
concerns. This repository stays on the backend: state management, the trace-content
classifier, rerankers, and the agent's organs.

Base: WeaverTools at ad4ede6f, main on 2026-09-30, with #758 (the tee reading) open.

## The claim

weaver-web is its own repository, on the operator's ruling of 2026-09-26 that the
frontend does not earn its keep close to the code and slows production down when it
shares one queue with the backend. The tree already left this repository in #689: a
subtree split of 52 commits, destination commit f2f01d8, with its documents, its own
Cargo.lock derived from this one and the toolchain pin, placed at
WeaverTools_Project/weaver-web/ beside the two clones in the thinkpad's workshop. The
repository was left to be decided, and this handoff is the deciding: the new session
creates it from that tree, or fresh if the tree is not worth carrying, which is the
new session's first call.

What it builds against is fixed by the operator's ruling of 2026-09-27, in
docs/project/weaver-tools-vision.md section 13, and this section restates the parts
the frontend meets:

- The agent keeps two doors and gains no others. weaver-gate carries the data plane
  and weaver-admin the management plane, at weaver-gate-world-contract and
  weaver-admin-operator-contract.
- Each door gets a network connector outside it, a client of the door's socket:
  web-con outside the gate and admin-con outside admin. Consumers talk to the
  connectors and never to the agent. The connectors live in this repository, outside
  the weaver-agents domain, each with its own planning session.
- This repository stops at the connectors. Each connector's Spec is its network
  interface and is what this repository publishes. weaver-web builds against those
  Specs and nothing deeper, from the day a connector's Spec merges, against a stub
  that answers it.
- Credentials are issued on the server and dropped into a client's configuration. One
  credential per client per connector, revocable one at a time, and a credential file
  never enters a repository. A connector authenticates and never authorizes: it
  passes a verified principal across the door, and the gate and admin decide what
  that principal may do.

## What exists today, and what does not

Neither connector exists. crates/ holds eleven packages and no web-con or admin-con,
and no connector Spec is written. That is the frontend's first dependency and it is
this repository's act, not the new session's: each connector's charter and Spec, the
principal the two door contracts will carry (one amendment), and the credential
model. The new session can start before they merge, on the contracts the connectors
will expose, and it names what it needs from them as issues here.

Three contracts the frontend reads now, all under docs/crates/contracts/:

- weaver-gate-world-contract: what crosses into the agent, one turn's work as
  newline-delimited JSON, one request per line, the field list the gate Spec's (today
  exactly one text member, unknown members refused), and what crosses out. web-con
  will front this.
- weaver-admin-operator-contract: the lifecycle verbs (load, unload, validate) and
  their answers. admin-con will front this, and the quiesce and resume verbs of the
  rewind sketch join it once the apex rules.
- weaver-analysis-web-contract: the seam a reader of a finished record already has,
  two streams that stay two, a per-position series (turn, ordinal, token, entropy,
  surprisal) and a per-generation summary, the emitter draining a file or a stream
  outside the agent as an operator principal, no third party on it. The frontend's
  reader end is what this contract's reader node names.

The data the frontend can render today sits on the shared bulk store under
weaver-testing/, per the evidence rule of this repository's CLAUDE.md: every run's
trace (trace.ndjson, the canonical events of weaver-trace-Spec section 3), the state
member's store as landed (state.sql, the typed tables of weaver-state-Spec section 3:
event, field, message, part, measurement, series), box facts, and result notes. The
HeroBench deposits of 2026-09-29 and 2026-09-30 hold fifty single-task sessions and
several multi-run sessions of an agent playing action by action, with score events
per task.

A live-view precedent exists in the operator's HeroBench fork: its weaver/bench
dashboard renders any agent registered with herobench adopt from a results file in
the scoring pipeline's shape plus the arm's log endpoint, showing state, the task in
progress, actions taken, score and elapsed time. It is the fastest way to see rusty
and pyra play, and it is the fork's, not this repository's.

## The first consumer: the HeroBench view

The frontend's first concrete consumer is the researcher watching and interviewing an
agent on a long-horizon benchmark. What that view needs, and where each need is
settled:

- The agent's state and its turns as they happen: through admin-con (load, unload,
  validate, state) and the trace as it grows, which the analysis-web seam
  contemplates as a drained stream. Whether a live stream is a fourth contract or the
  analysis-web seam's own is a question for the connector planning here.
- The map with the fog toggle: the agent-visible map is derivable from the record
  (the game dump rides the task's user turn), and the full map is not in the record,
  it is the environment's table, so showing it needs a data path from the environment
  to the web seam that today has no party. An open cell of
  docs/project/sketch-rewind-and-interrupt.md section 6.
- The interview: the researcher's interrupt enters through the gate as ordinary
  traffic under a harness-minted bearer capability returned over admin, every event
  authored while quiesced carrying a public interrupt identifier, no tool running
  while quiesced, per the rewind sketch's section 3. The frontend needs the quiesce
  and resume verbs on admin-con and the capability on the request line through
  web-con, both owed to Specs here.
- Replay with the interview in or out, and branch lineage: a replay excludes by the
  interrupt identifier on every event, and a branch is a new run naming its parent
  run and position, which the analysis-web contract's branch position is written to
  carry. The rewind sketch's sections 1 and 3.
- The classifier's labels at positions: once the trace-content classifier serves,
  classify.output events carry scored labels per position, and a frontend that shows
  a turn can show what the routing stage said of it. The sketch is
  docs/project/sketch-the-trace-content-classifier.md, and nothing serves yet.

## What the new session owns, and what it does not

The new session owns weaver-web whole: its repository, its stack, its objectives, its
editorial rules, its review process. Nothing in this repository's process documents
binds it except the connector interfaces and the suite rules above. It does not edit
this repository. What it needs from the connectors it states as issues here, in the
shape this repository's issues take: what was measured, what is asked, and which
document would have to move. The olympus Planning seat answers those issues and keeps
the backend.

Three boundaries hold across both:

- Consumers never reach the agent. No socket of the agent's is opened to the
  frontend, and no shortcut past a connector stands, however convenient during
  development. A stub of the connector is the development surface.
- No credential, box path, or security posture enters a repository. The repositories
  are public unless the operator says otherwise, and the report hub this workshop
  keeps is local and never published.
- The interface moves with a contract's care. A change a frontend needs to a
  connector's interface is proposed as an issue here and lands as a Spec act,
  reaching every party.

## Suggested first steps, the new session's to accept or refuse

1. Create the repository, from the split tree at f2f01d8 if it carries its keep and
   fresh if not, and record which and why.
2. Read the three contracts above and the two sketches, and write the frontend's own
   charter: what it shows, to whom, and what it asks of the connectors, in the order
   the HeroBench view needs it.
3. Stand a stub of web-con and admin-con from the door contracts' shapes, so the
   frontend builds against the interface it will get rather than against the agent.
4. Render the deposits first: a trace and its state store as a replay, with the
   positions the classifier sketch defines, before any live view.
5. State the connector needs found on the way as issues here, one per interface
   question, so the connector planning sessions start from the consumer's list.

## What this document does not do

It does not charter the connectors, decide the frontend's stack, or promise a date
for either connector's Spec. It does not carry the operator's rulings still open on
the backend (the sampler tunables, the rewind sketch's apex questions, the
classifier's labeling), which the frontend meets only through the interfaces those
rulings shape.

---

## Step 1, decided 2026-09-30

**Carried, not fresh.** Of this tree's roughly 5.5k lines of Rust, about 3k are
written to the standing `weaver-web-Spec`: the Postgres store (16 tables, nine
migrations), the six reads of Spec section 4, the Record surface, the session gate with
the bearer stored as a digest, and the compile-fail pins. That store is where
weaver-analysis's record lands, one of the three arrows into weaver-web on the
operator's whiteboard sketch of 2026-09-30:

```text
                 WeaverTools (the agent)
                  /                 \
               gate                admin <-> diagnostic
                 |                   |           |
            (web-con)           (admin-con)      v
                 \                  /      weaver-analysis
                  v                v           |
                     weaver-web  <-------------+
```

The connectors in parentheses are the handoff's, not drawn on the board. The 55
commits carry the reasoning behind every election, which a fresh start would drop.

**What does not carry forward is the half that reaches the agent**, about 2.2k lines:
the `weaver-web-connector` binary, `wire.rs`, `lifecycle.rs` (`sudo weaver-admin`),
`adapters/gate.rs` (dials the gate socket), the trace tailers in `traceview.rs`, and
the legacy admin routes of `web/`, already answering 503. The first boundary above
forbids exactly that reach, and web-con and admin-con replace it, so it leaves in the
first act rather than being a reason to start over. The charter is rewritten per step
2, and the Spec trimmed to the store and the read path.

**The operator's rulings of 2026-09-30 on placement:** the repository stays local for
now with no remote, private when one is made, and the checkout moves out from under
`WeaverTools_Project/` so the backend's `CLAUDE.md` stops loading here.
`toddwbucy/Weaver-Web` on GitHub is the archived v1 repository of 2026-08-19 to
2026-08-24, private, sharing no commits with this tree, and GitHub names are
case-insensitive, so any remote named weaver-web is a decision about that repository.
