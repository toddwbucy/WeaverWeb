---
title: weaver-web
summary: the first consumer: channel, lifecycle, and trace surfaces reaching an agent across the two external contracts
version: v0.2
date: 2026-08-25
commit: unreleased
parent: WeaverTools Technical Documentation
---

# weaver-web

**Status:** technical documentation. Describes, decides nothing.

**Rough draft, first pass.**

The code described here is unreleased and is scheduled for release in the first
quarter of 2027.

## What it is

**The suite's frontend, and the first thing to reach an agent from outside.**
Two processes serving three surfaces over a browser: a channel where humans and
agents converse, a lifecycle view where an operator drives the verbs, and a
live view over an agent's record. The **connector** runs on the agents' own box
and holds every box-bound reach - the gate sockets, the verb invocation, the
trace sinks. The **server** presents HTTP and holds everything else - the
channel store, the routing, the rendering. They meet over one link the
connector dials, loopback when both share a box, and moving the whole
presentation stack to another device on the network is deploying the server
there and changing that one address. What gets processed where is a deployment
concern, and the architecture treats it as exactly that.

**It is a consumer, and that word is structural here rather than descriptive.**
It sits outside the agent boundary. It links no crate of the agent domain, holds
no weights, authors nothing in the record, and knows the agent only as a socket
to dial and a binary to run. The agent never grips it and does not know it
exists.

**Two of its three reaches are contracted and the third is not**, which this
page states rather than rounds off. Turns cross the gate's world contract and
the record arrives under the operator contract, both on
[the contracts page](../contracts.md). **The lifecycle verbs are the exception**:
the operator contract governs the record and says outright that running the admin
binary is running the crate rather than a channel it governs, so there is no page
written for an outside party to build that surface against. weaver-web builds it
on deployment fact and has filed the ask for the page, which is the last section's
business.

That makes it the working proof of a claim the rest of this site makes
repeatedly: that the boundary is real, that a consumer needs nothing but the
contracts, and that both are true in practice rather than only on paper. **It
lives in the same repository as the agent it talks to, and that says nothing
about the seam** - sharing a source tree is not sharing a boundary. Every
crossing of the agent's boundary is one the contracts name, and the one reach
past them, the verb invocation, is the deployment's and is stated above as
the exception it is.

## What it owns

**The channel, and its own record of it.** Multi-party conversation where agents
are participants rather than features: invoked by mention, answering whole turns,
each answer carrying the run and turn labels that link it to the agent's own
record. The channel log is weaver-web's, on weaver-web's disk, with one writer -
its own custody discipline, borrowed from the one the agent keeps over its trace.
**It is not the trace, never writes to the trace, and links to it by label only.**

**The lifecycle view.** The three verbs an operator drives - `validate`, `load`,
and `unload`. **Those three names are a deployment fact rather than a published
surface**, the reach the contracts do not cover: they are what the installed
binary accepts, observed rather than read off a page written for an outside
party.

**What survives an answer's rendering is its meaning, not its bytes.** The verb's
output is decoded as text first, with any invalid sequence replaced rather than
carried, and from there it takes one of two paths. Output that parses as JSON is
re-serialized and pretty-printed, so the formatting and the key order on the page
are the renderer's rather than the binary's. Output that does not parse is shown
as the decoded text it became, which is what keeps a malformed answer surfacing as
itself rather than as an error page. Empty output is shown as nothing at all.

**What that buys is that nothing is interpreted**: no field is renamed,
summarized, or translated into a friendlier shape, and no failure is swallowed.
What it does not buy is a byte-exact reproduction of what the binary wrote, and
the page says so rather than letting verbatim carry a promise the renderer
does not make.

Load state is shown from the gate socket's existence and **labeled as the
inference it is**, because the program has no
status verb to ask and inventing one on the client side would be the frontend
answering a question the framework has not.

**The trace view.** A live, turn-bracketed reading of an agent's record, with
field selection and search, faults prominent, and discontinuities marked rather
than smoothed. A rotation or truncation surfaces as a mark, which mirrors the
record's own honesty rule: a gap is never filtered out of sight.

**The browser as a display engine, which is a constraint rather than a
preference.** The browser receives a rendered projection and submits authored
text, and holds nothing else - no keys, no signatures, no protocol state, no
routing or ordering logic. Everything that means anything happens server-side,
where the boundary and the operator's trust already live. Any future
architecture change is tested against this constraint first.

## Seams

**None into the agent, and that is the point.** It holds no seam in the sense
the crate papers use the word: no contract binds it to any component, because
contracts bind parties inside the program and this sits outside.

What it has instead is **two external contracts, which are pages rather than
partners** - written for whoever builds against them and owing nothing back.
See [the contracts page](../contracts.md). Across the first it dials the gate
socket and speaks one line per turn. Across the second it reads the record the
operator holds.

**The third reach has no page.** It runs the lifecycle verbs as a subprocess,
which is the operator's own command run on the operator's behalf under a rule
the deployment declares - not a seam, and not contracted either. What it builds
against there is the deployment's observed shape, and the gap is filed rather
than papered over.

**What that rule permits is worth naming rather than summarizing.** It admits
one identity, running the admin binary, under each of the three verbs - and it
admits any arguments after the verb, plus the configuration location through the
environment. A wrapper accepting only a verb and a bare agent name would make the
rule as narrow as the description usually given for it, and that narrowing is
filed rather than done.

## How it works

**One turn.** A mention in a channel enqueues an invocation for that agent. The
worker assembles context from the channel log and asks across the link, where
the connector dials the agent's gate socket, writes one JSON line, and waits
for one line back. The close is appended to the
channel with its kind and its labels, and the browser learns about it through a
server-sent event rather than a page turn. One turn in flight per agent: a
second mention waits, because the agent serves one turn at a time and queueing
in front of it is more honest than discovering the serialization inside the
socket.

**Nothing streams, because nothing can yet.** A long generation returns no bytes
until its close, so the interface is built honestly around whole-turn latency
rather than around a progressive rendering the boundary cannot supply. That gap
is filed upward rather than worked around, which is the rule the whole
relationship runs on.

**The verbs.** A lifecycle request runs the admin binary as a subprocess with a
generous ceiling, and whatever it answers is rendered as it came, parsed when it
parses and shown raw when it does not. Nothing is swallowed and nothing is
interpreted into a friendlier shape.

**The record.** The connector tails the file the agent's declaration names,
read-only, and streams it over the link for the server to project. Neither
side ever writes there, and a dropped link surfaces in the view as a
discontinuity mark, exactly as a rotation does.

## What it refuses

**Reaching around the socket.** When a contract lacks something this frontend
needs, the answer is an ask into the framework by the contract's own change
protocol - never a workaround, never parsing something a contract calls opaque,
never linking a crate to get at an internal. Four such asks are open, and the
designing-around is itself the evidence each carries.

**Interpreting what it relays.** Verb answers render verbatim. Trace events
render as what they are. The frontend has opinions about layout and none about
meaning.

**Writing to the record.** The agent's account is the agent's, authored by one
writer inside the boundary. This side holds a read.

**Keys and signatures in the browser, permanently.** Not a v1 deferral but an
exclusion at every horizon, per the display-engine constraint: identity is
adjudicated where the operating system adjudicates it, not by a page.

**Its own opinion about who may pass.** The gate admits it by credential or
refuses it, and being refused is a fact it reports rather than one it routes
around.

## What is not built

- **Identity, authentication, and transport encryption.** No login and no TLS.
  The roles exist - user and admin, assigned from the deployment's own list -
  and they are boundary hygiene over anonymous sessions rather than access
  control, which is worth knowing exactly: **before that act lands, anyone who
  can reach the listener and knows a configured admin name holds the admin
  role**, and the admin role drives the lifecycle verbs. The deployment shape
  is a LAN, the deferral is deliberate and on the roadmap with a named
  mechanism, and it is stated here rather than in a footnote because a deployer
  meets it before they meet the roadmap.
- **Streaming through the gate.** The largest gap, and the ask with the most
  weight behind it: turns serialize per agent, so a busy room multiplies
  whole-turn waits.
- **A status verb to ask.** Load state is inferred from socket existence, which
  is why the interface labels it as an inference.
- **A page for the verb invocation surface.** The two-contract rule intends
  that an outside consumer builds against published pages and nothing else,
  and the lifecycle verbs fall outside both: their shape lives in documents an
  outside consumer is not meant to read. Until a page exists, this one reach
  is built on deployment fact, and saying so is the difference between a gap
  and a quiet exception.
- **An operator read on agent state.** Session state exists inside the agent and
  the operator has no window on it.
- **Upstream model participants.** The adapter seam is named and the first
  provider is its own act - a mention of a model participant is ignored today
  rather than half-answered.
