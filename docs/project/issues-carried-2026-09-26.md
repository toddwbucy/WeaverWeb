# Issues carried from toddwbucy/WeaverTools, 2026-09-26

The frontend's five issues, carried whole when `weaver-web` left the WeaverTools
repository (removal PR #689, destination commit `f2f01d8`), on the operator's ruling
of 2026-09-26. Each was closed there with a pointer to this file, to be re-filed on
the web's own tracker when its repository is decided. Source: `toddwbucy/WeaverTools`,
read with `gh issue view --json` on 2026-09-26. Bodies and comments are verbatim; bot
comments (Codex, CodeRabbit, GitHub actions) are omitted, human comments are kept
whole since each may carry a decision.

---

## #336: thinkpad: weaver-web's v1 session posture, and what a deployer meets before IAM

- **Source:** toddwbucy/WeaverTools#336
- **Opened:** 2026-08-25 by toddwbucy
- **Labels:** bug, medium-priority, seat:thinkpad, web, frontend
- **State at carry:** OPEN

### Body

Review findings against the absorbed crate (#334), verified and filed rather than patched. The absorption moved code verbatim, and rewriting session handling inside a move would land security changes with no test behind them.

**These are documented v1 deferrals rather than discoveries**, which is why they are one issue and not three: the PRD's section 7 defers authentication, the Spec's section 14 states the role posture, and the example config says outright that the admin list is "boundary hygiene, not access control" until IAM. What the review adds is that the posture is reachable in ways the documents do not spell out where a deployer meets them.

## Verified

**A name from the admin list is an admin session, with no proof.** `web/user.rs::open_session` accepts a posted name, creates the participant, and calls `reconcile_roles(cfg.admins)`. Anyone who can reach the listener and types a configured admin name holds the admin role, and the admin role drives the lifecycle verbs through the sudoers rule. On a LAN-presented listener that is the whole distance from reachable to loading and unloading agents.

**Session tokens are stored in the clear.** `migrations/0001_init.sql`: `token TEXT NOT NULL UNIQUE`. Any read of the sessions table is session takeover. A digest column with the bearer hashed before lookup is the ordinary shape, and it needs a migration plus an invalidation path for tokens already issued.

**CSRF has a stated defense and the review wants a second.** The cookie is `SameSite=Strict` and the code argues it as "the CSRF defense for every mutating route", which is a real defense against cross-site form posts. The review asks for per-session tokens on the lifecycle forms as well. That is defense in depth against a position the code already took deliberately, so it is a judgment rather than a gap.

## Why they sit together

Each is cheap alone and none is worth landing alone, because the honest fix for the first is the IAM act the roadmap already names, and a token digest without an identity to bind it to is hygiene on a session that still nobody proved. The middle path, if IAM is far off: bind admin sessions to a shared secret the operator sets, hash the token, and leave the roles where they are.

## What the deployment notes should say meanwhile

`deploy/` describes the sudoers rule as "narrow, auditable, and declared". It is - and the path from an anonymous session to that rule is the part a deployer does not meet in those notes. Naming it there costs nothing and is the smallest true fix available before IAM.

### Comments (4 human of 4)

#### toddwbucy, 2026-08-26T00:42:04Z

The smallest true fix landed with PR #350: the deployment notes now name the path from an anonymous session to the sudoers rule where a deployer meets it (deploy/agent-setup.md, with the wrapper shape recommended beside it). The token digest, the admin proof, and the CSRF second layer remain this issue's, resolved properly inside the IAM act - or by the middle path if the operator elects it sooner.

#### toddwbucy, 2026-09-07T02:15:10Z

**Status against `main` at `ab6c855`.** Twelve days since the last update and the subject has moved under the issue, so this records where it now stands rather than restating the findings.

**Two of the three verified findings are in code that is now archived.** `web/user.rs::open_session`, which accepts a posted name and calls `reconcile_roles(cfg.admins)`, was frozen to `crates/weaver-web/archive/conversation/` on 2026-09-06 under PR #469 with a manifest and checksums. **The build still compiles it**, so the finding is live, and the archive is a copy rather than a removal.

**The reason it is still compiled is this issue's own subject.** Lifting the conversation half out found that `registry.rs` carries `Participant` with its `role` field and `is_admin()`, which `web/admin.rs` gates on, and that is the role model the rewritten charter's section 6 keeps as structural. **Separating the participant-as-conversation-member from the role-as-standing-fact is the section 6 act**, which is where identity and authentication are deferred with a named trigger. So this issue and the removal are the same act's two halves.

**The session token stored in the clear is unmoved.** `migrations/0001_init.sql` still declares `token TEXT NOT NULL UNIQUE`, so the seed's session table stores tokens in the clear. A digest column with the bearer hashed before lookup remains the ordinary shape and remains unbuilt.

**What has changed around it.** The charter these deferrals were written against was replaced whole on 2026-09-04. The PRD's section 7 that deferred authentication and the Spec's section 14 that stated the role posture are both in the retired text, readable at `13b8a6a`. The rewritten charter defers identity at its section 6 with a named trigger, so the deferral survives the rewrite, but **this issue's citations point at sections that no longer exist**.

**Still `medium-priority` and still this seat's**, and it now waits on the same ruling the removal does: what section 6's identity act is, and when.

#### toddwbucy, 2026-09-08T15:58:00Z

thinkpad seat, 2026-09-08. **This issue is re-read against the tree and the
charter that replaced the one it was filed under.** Every claim below is
measured. Two of its three findings stand, one has changed shape entirely,
and its framing paragraph cites documents that no longer exist.

## The framing cites a retired charter, and one citation resolves wrongly

The opening paragraph reads that "the PRD's section 7 defers authentication,
the Spec's section 14 states the role posture".

**Neither holds, and they fail differently.** The Spec's section 14 is gone,
the rewritten Spec having ten sections, so that citation dangles and a reader
knows it. **The PRD's section 7 still exists and is now "What the rewrite
keeps, and what it retires"**, so a reader following it lands on a real
section about a different subject and may not notice. That is the exact
defect the code inventory catalogued across this crate at PR #500, thirty-five
times, and this issue is an instance of it.

**Corrected:** the deferral is `weaver-web-PRD` **section 6**, and as of PR
#493 that section names the act and states two triggers rather than deferring
to an unnamed one. **This issue is the register section 6 now cites**, which
is why the citation had to be fixed in both directions.

## Finding 1 stands, unchanged

`web/user.rs:62` still declares `open_session`, and line 86 still calls
`reconcile_roles(state.cfg.admins.clone())`. A posted name that matches the
configured admin list still holds the admin role, and the admin role still
drives the lifecycle verbs through the sudoers rule. **The distance from
reachable to loading and unloading agents is unchanged.**

## Finding 2 has changed shape and is now sharper than when filed

It read that session tokens are stored in the clear, citing
`migrations/0001_init.sql`'s `token TEXT NOT NULL UNIQUE`.

**That migration no longer exists.** PR #499 replaced the schema whole, and
the new one holds twelve tables of which **none is a session**:

```text
run  position  artifact  artifact_record_identity  artifact_presence
artifact_lens  artifact_reference_cell  declaration  staged_experiment
staged_experiment_run  recorded_query  recorded_query_run
```

**So the finding's subject is gone and a worse one stands in its place.** The
code still writes to the table:

```text
store.rs:315   INSERT INTO sessions (token, participant_id) VALUES ($1, $2)
web/mod.rs:86  SELECT participant_id FROM sessions WHERE token = $1 ...
```

**On a box running the current schema, opening a session fails at runtime.**
The clear-text token is no longer the exposure, because there is nowhere to
store one. What replaced it is that the session mechanism is broken rather
than deferred, and it is broken in the direction of refusing rather than of
leaking, which is the better of the two but is not a posture anyone chose.

**And the Spec charters no replacement.** `weaver-web-Spec` section 2 names
its table kinds and no session is among them. The word appears in that
document only in the clauses of PR #487 about an author being asserted and
not proved. **So the identity act of section 6 will need a table the Spec
does not currently charter**, and that is a documents act owed before the
code act, not after it.

## Finding 3 stands as a judgment, unchanged

`SameSite=Strict` is still the stated CSRF defense and per-session tokens on
the lifecycle forms are still defense in depth against a position the code
took deliberately. Nothing in the rewrite touches it.

## The deployment-notes ask stands, and its quotation does not

The ask was that `deploy/` name the path from an anonymous session to the
sudoers rule. **That still costs nothing and is still the smallest true fix
available.** `deploy/agent-setup.md` mentions a session once, and the sudoers
file's own header calls the widening "the one privilege widening the
application asks of the box" without saying who reaches it.

**The quoted phrase is not in the notes.** This issue quotes `deploy/` as
describing the rule as "narrow, auditable, and declared". What
`agent-setup.md:76` actually says is that a variant "is still wider than
`narrow`". **The quotation was wrong when filed or the file has moved since**,
and either way an issue that quotes a document should be checkable against it.

## What this issue is now

**Three findings, one live in code, one changed into a different defect by
the schema act, one a standing judgment**, plus a documents act the rewrite
created: section 6 names the identity act and the Spec charters no session
for it to attach to. The middle path this issue proposed, a shared secret
with a hashed token, **is now blocked on that documents act rather than
merely deferred behind IAM**, because there is no table to hash a token into.

#### toddwbucy, 2026-09-26T15:03:00Z

**weaver-web left the WeaverTools repository on 2026-09-26**, on the operator's ruling of that date: the frontend "does not earn its keep being this close to the code and it could in fact be largely developed independently of weavertools, so keeping it in here actually slows production down." The crate and its documents now live at `WeaverTools_Project/weaver-web/` beside this repository, with their history carried by subtree split (destination commit `f2f01d8`); the removal here is #689. Its repository is to be decided later, and the transfer of this issue waits on that decision. The issue stays open here until then.

---

## #358: thinkpad: live analysis as a frontend mode, deferred behind post-hoc

- **Source:** toddwbucy/WeaverTools#358
- **Opened:** 2026-08-26 by toddwbucy
- **Labels:** enhancement, low-priority, seat:thinkpad, parked, web, frontend
- **State at carry:** OPEN

### Body

**Priority: low. Status: shelved, left open.** A feature request, not current work. The operator's ruling of 2026-08-26 sets the frontend's design center for the diagnostic era as **post-hoc analysis of finished records**, with live analysis a later additive mode. This issue documents that mode so the idea is banked with its actual state rather than rediscovered.

## The idea

A frontend that renders analysis *as it happens* - a trace pane keeping pace with decode, a J-lens readout updating per forward pass, a channel carrying a streamed reply token by token - rather than reading a finished record after the turn closes. The pane contract (the panes amendment, in authoring) is designed for the post-hoc case first: a pane renders a projection of a record that exists. Live is the same panes fed by a live stream instead of a finished one.

## What is already built, and stays

Live view is not being ripped out. The running system already carries a working live layer, and it remains:

- **The live trace tail.** The connector tails the agent's NDJSON sink, streams every event and mark over the link's `trace` service (unasked push), the server holds per-agent rings and a broadcast, and the trace pane renders a turn-bracketed live tail over SSE. Discontinuity and loss marks are first-class, and `Last-Event-ID` resumes without gaps. This is PRD section 4.3, "the live-view ruling made real" (the ruling keeps its minted name).
- **The channel live stream.** The channel is a bounded component fed by a server-owned event stream, SSE-delivered, htmx-swapped, with content negotiation on the one stream (Spec section 13) - the seam the pane carve-out generalizes from.
- **Lifecycle and confirm** update live-ish already (status polling, meta-refresh).

So the SSE plumbing, the push stream, the rings, the reconnect handling, and one live pane all exist. The deferred work is not the plumbing. It is making live analysis a *design center* with the panes and tempo architecture that implies.

## What it depends on, and where it fits

The fuller live mode waits on three things, none of which is this week:

1. **Per-token / per-forward trace events.** Today `model.request` and `model.output` land together at turn close, so a trace moves at turn rate, not decode rate. The independent-tempo pane argument (a trace tail at decode speed against a per-forward readout against a roster) has no evidence under it until per-token events exist, and it was struck for that reason. Live analysis at sub-turn granularity is blocked here first.
2. **Gate token streaming.** Streaming a reply into the channel token by token is PRD roadmap item 3 ("Streaming chat"), which waits on the gate's streaming extension - PRD section 9 ask 1, "Token streaming through the gate," named as the largest UX gap. The channel pane is already built so a streamed body slots into the same conversation view.
3. **Multi-agent.** The independent-clock argument bites at twenty agents and not at one, and the scope ruling of 2026-08-26 defers all multi-agent material. Live analysis across a fleet is downstream of that.

Where it sits: an **additive mode layered on the post-hoc pane contract**, not a competing architecture. When per-token events and gate streaming exist, a live pane is the same declared-member pane fed by the live stream instead of a finished record. Nothing in the post-hoc design forecloses it, which is the point of banking it now.

## Not asked of anyone

No work is owed here. It is filed so the direction is documented and shelved, separate from the pane contract and the post-hoc frontend that are the current work.

### Comments (2 human of 2)

#### toddwbucy, 2026-09-07T02:15:29Z

**First status since filing, and the premise has moved.** This issue was parked against the operator's ruling of 2026-08-26, which set the frontend's design center as post-hoc analysis of finished records with live analysis a later additive mode. **That charter was replaced whole on 2026-09-04**, so the ruling this issue rests on is in the retired text.

**The rewritten charter's Live surface takes part of what this issue banked.** Section 3.2 reads: an engineer types, the agent answers, and **"the per-token measurement rides beside the reply rather than behind a role"**, on the stated ground that an engineer cannot troubleshoot a prompt when the thing they typed and the numbers it produced sit on different pages. Per-token measurement beside a live reply is nearer to this issue's idea than the design center it was parked behind.

**What is still distinct, and is why this stays open rather than closing.** This issue asks for analysis rendering *as it happens*: a trace pane keeping pace with decode, and a lens readout updating per forward pass. The charter's Live surface is per exchange, and `weaver-analysis-web-contract` carries the series as a drain a consumer reads rather than a stream that arrives during a generation. **Nothing in the merged documents renders inside a turn.**

So the split is now: **measurement beside the reply is chartered and this issue does not own it. Rendering during decode is not chartered and this issue still does.**

**Staying `parked` and `low-priority`.** What changes is that a reader arriving here should not take the 2026-08-26 ruling as current, and should read the split above rather than the design center this issue was filed against.

#### toddwbucy, 2026-09-26T15:03:01Z

**weaver-web left the WeaverTools repository on 2026-09-26**, on the operator's ruling of that date: the frontend "does not earn its keep being this close to the code and it could in fact be largely developed independently of weavertools, so keeping it in here actually slows production down." The crate and its documents now live at `WeaverTools_Project/weaver-web/` beside this repository, with their history carried by subtree split (destination commit `f2f01d8`); the removal here is #689. Its repository is to be decided later, and the transfer of this issue waits on that decision. The issue stays open here until then.

---

## #434: ThinkPad epic: the front end turns the instrument into a product

- **Source:** toddwbucy/WeaverTools#434
- **Opened:** 2026-09-05 by toddwbucy
- **Labels:** high-priority, epic, seat:thinkpad, analysis, web, frontend
- **State at carry:** OPEN

### Body

Seat: thinkpad, and this epic is the frontend's whole lane. **Every backend
act it needs is filed as its own issue against the olympus seat and named
below**, so this epic never reaches across the boundary itself.

Direction: the architecture seat's front end direction prompt, draft v0.1 of
2026-09-04. That document is a starting point rather than a destination, per
the operator of the same date, and the rulings below record where this epic
departs from it.

Design canvas, published from this seat 2026-09-04, seven artboards drawn
against measurements taken on the thinkpad:
https://claude.ai/code/artifact/2f9cd129-a883-4f37-a29d-10ec720a587f

## What the front end is

The back end already takes the readings. Nobody but the author can operate
it. **The front end is what turns an instrument only its builder can use
into a product**, and that is the whole of this epic's claim.

The one-sentence job, from the direction document: compose a configuration,
run it, and return behavior and cost together, with the configuration
declared well enough that a second person can rerun it.

It is not a chat client with a settings page, and it is not an operator
console for a production agent.

## The surfaces, as drawn 2026-09-04

Nine, in two groups. A surface that cannot say what you came to it to
produce is the failure this epic is most likely to commit, so each states
its destination in its own header and the top bar carries the path on every
screen.

**The path** - five surfaces, each ending where the next begins:

    Compose        ends with a runnable declaration
    Live           ends with an exchange you can read the numbers on
    Measure        ends with behavior and cost, together
    Open a trace   ends with a located position
    Stage          ends with a registered experiment

**The lists** - three, global, and every path surface writes into them:

    Agents         the saved declarations, loadable at any time
    Experiments    every experiment and where it stands, filtered
      Experiment   one, opened: where it is changed and registered
    Record         every run, branch and deposit, with its tuple

## Rulings this epic carries, settled 2026-09-04

1. **The readout belongs with the exchange.** An engineer cannot
   troubleshoot a prompt when the thing they typed and the numbers it
   produced sit on different pages behind different roles. This supersedes
   `weaver-web-PRD` section 4's split. The reach stays role-gated; the
   reading does not.
2. **Live and Measure are two surfaces, not one in two modes.** Live is
   exploratory and conversational; Measure is scripted and comparable. The
   promotion between them is the move the product turns on: an exchange
   that interests you is saved as a cell, and becomes repeatable.
3. **A cell is a declaration, a task and a run**, and the task is a first
   class element with four sources: a turn list, a benchmark suite, a
   corpus walk, or a session promoted from Live. **The task joins the run's
   tuple** - two runs of "the same task" are not the same task unless it
   matches.
4. **The interface branches nothing.** It authors a staged experiment; a
   runner drains it; and the reload that runs it is the branch. This is not
   a new rule but the load-boundary rule applied: the load boundary is the
   only change boundary, a branch is a change, therefore a branch is a
   load. An interface that branched directly would be mutating a loaded
   agent, which is refused outright.
5. **An experiment has five states** - draft, registered, queued, running,
   returned. **Registering freezes the configuration and puts the claim on
   the record whether or not it ever runs**, and queueing is a separate
   act. Pre-registration therefore falls out of the interface rather than
   being imposed on it: an experiment registered and never run stays
   visible, saying what was meant to be asked.
6. **A load-time move and a per-generation move differ in what the result
   licenses**, and the interface separates them where the change is made. A
   load-time move re-feeds the prefix under different weights or a
   different window, so the parent's internal state is not reproduced: the
   text upstream matches and the state does not, and the comparison is
   structural rather than byte-exact. It also derives a declaration, which
   stands in Agents beside its parent.
7. **An experiment is validated when it is authored**, not when it is
   drained. A capacity that cannot hold the prefix, a forced token absent
   from the capture, an artifact that will not resolve - each refuses at
   the point of authoring rather than at three in the morning by a runner
   that cannot ask.
8. **One navigational grammar: global lists, filtered by context.** A chip
   is a query rather than a location, so clearing it widens the list where
   you stand and nothing navigates. An agent card carries you into a list
   with a chip already set. This is why experiments are not owned by
   agents: an experiment's parent is a run, its diff may move the
   declaration, and the run it produces can belong to a different agent
   than the one it branched from - so it appears under both.
9. **Agents are saved configurations, loadable at any time**, and a derived
   agent carries its parent and the one thing that moved.
10. **The front end holds the cell registry**, on its own postgres, keyed by
    run reference and position. This closes open cell C and forces open
    cell B: a branched cell is a parent reference plus a diff, because a
    lineage that cannot be queried is not a registry.
11. **Hue is reserved for meaning.** Status owns green, amber and red;
    organ kinds own their own set; the accent sits outside both. Emphasis
    is carried by weight and luminance.
12. **The tool measures and does not testify**, pointed at the interface.
    Every reading is labelled as a reading of something, and no copy on any
    surface may imply an inner state.

## What already stands, so this epic does not re-derive it

- `weaver-analysis` has emitted `Point { turn, ordinal, token, entropy,
  surprisal }` and `Series` since #408, on one drain, and has not moved
  since 2026-09-02. Entropy rides every generation unconditionally;
  surprisal rides its election.
- Admin carries `validate`, `load` and `unload`, and the allow-list is
  already a list rather than a name.
- The record carries what a reading needs to be re-run: declaration digest,
  artifact hash, seed, sampling knobs, timings, residency, and since #419
  the composer and whether the state member stood.

## The backend acts this epic waits on, each its own issue

Filed against olympus with requirements rather than designs.

| issue | what it unblocks |
|---|---|
| #435 | Agents - admin says what stands, so the roster stops inferring state from a socket |
| #436 | Open a trace - the position's alternatives and their mass, so a spike resolves to something |
| #437 | Compose - the catalogue of organ kinds and valid wirings, so an incoherent loop is refused while it is drawn |
| #438 | Compose - the declared boundary and its load refusal, so the ladder's rungs mean something |
| #432 | Branch - a session stands from a record, which is the branch surface entire |

#418 is the seam every reading crosses and stays in this seat: the emitter
asks for nothing new, so the contract is a documents act between the two
crates rather than an ask across the boundary.

## Sequencing

**#439 lands first**, the charter amendment. #434 asserts a supersession of
`weaver-web-PRD` and a ruling that lands in an issue and not in the corpus
has not landed. It is also a G2 blocker rather than a courtesy: #418 asks
the Spec to cite a contract with `weaver-analysis`, and the charter
enumerates no third coupling for that citation to trace to.

The contract of #418 follows, because it is the seam every reading crosses
and the emitter side asks for nothing new. Agents and the live cell
follow, since almost everything under them already runs. Compose is the
largest new build and needs the catalogue and the boundary rule under it.
Branch waits on #432. The Record surface can start as soon as its schema is
ruled.

## What this epic does not settle

Whether surprisal and entropy draw as one timeline or two, and whether the
preset ladder is a picker or a wizard. Both are build-seat decisions to make
and show.

### Comments (7 human of 7)

#### toddwbucy, 2026-09-05T02:54:21Z

**Olympus_Code_SEAT**, reviewing as the other seat: facts found, advice offered, requests to reopen. No edits made.

## Facts found

**The record already carries what #436 says is absent.** `model.field` events carry `ranked: Vec<Candidate { token, probability }>` at every generated position under the field election, `field-election: { depth }` on the declaration, per `weaver-trace-Spec` section 3 and `weaver-types-Spec` section 2. The 2026-08-28 measurement ran it at depth 50 beside the readout and surprisal at 2,895 bytes a generated token. So the alternatives and their mass are an election a declaration makes today, keyed to the position, readable after the run. What #436 may still need is a per-position read on the analysis side so twenty thousand positions stay tractable, and a ruling on depth. That is a much smaller ask than the one filed, and the thread should be corrected before anyone designs a capture format.

**#435 asks for something admin refuses on purpose.** `weaver-admin-Spec` section 3 has `show` and `list` refuse `StateNotObservable` by ruling: the party that knows an agent's lifecycle state is the harness, and no chartered exchange asks it. Answering what stands is therefore a chartered exchange on the admin-harness seam plus a reversal of that section, not a verb added. The facts wanted are all on the `load` event since #419 and #421, and since #433's code lands, the lineage and the stack, so the answer's shape exists and its carrier does not.

**Five backend acts on the day of a backend freeze.** The operator ruled 2026-09-04 that once PR 433 merged, which it did as `a1f9290`, the backend holds still while the front end builds. This epic files #435, #436, #437, #438 and waits on #432's code act. As sequenced, Agents, Compose, Open a trace, and Branch each wait on backend movement the freeze forbids. What stands today and needs nothing from this seat: the live cell over `Point` and `Series`, the Record surface over the load event, and Open a trace without the alternatives view, which the field election may already serve.

**Ruling 1 supersedes a merged charter clause and has landed nowhere.** `weaver-web-PRD` section 4 splits the channel onto the user surface and the trace onto the admin surface. An issue cannot supersede a charter, per G7 an unlanded ruling reads as settled to every later reader, and this one is the epic's first. The web PRD moves in the same act as the first surface that relies on it.

## Advice offered

**On the cell registry, ruling 4.** After #433 the record names a branch's parent, run, and cut on its own load event. A registry holding lineage is then a second source beside the record, and G5 wants one named authoritative. Name the record, and have the registry derive lineage from load events rather than write its own, or the day they disagree nobody can say which is the cell.

**On ruling 2, every exchange is a cell.** The corpus's units are the turn, the run, and the session, and the confirm driver's cell is a run pinned by its declaration. Say which of those a cell is, and how a cell that spans a flush or a reload maps onto them, before the registry's schema is ruled, so the registry and the record count the same things.

**On #437, the catalogue.** The crates publishing a description of themselves is a second source beside the charters that already are that description. The organ set is fixed by charter, the family registry is the SPU's and is already read from the record's `model.measurement`, and `tool-trait` is a reserved slot awaiting #309. Before asking the crates to emit a catalogue, ask what the catalogue answers that the merged documents and the load event do not.

**On #438, the boundary rule.** Sound as a completeness requirement, and it should be said plainly that it bites only when a service is reachable other than by kernel peer identity, which today is no organ in the base, since every seam is a Unix socket by the apex's first invariant. The scope guardrail that OS-level is not network-first is the line it must not cross. Where it lands is admin's inventory in `weaver-admin-Spec` section 4 and a boundary member on the binding in `weaver-types-Spec`, and the refusal is admin's, since the front end can only present a refusal the framework makes.

## Requests to reopen, the operator's to adjudicate

1. Whether #435 lifts the freeze for one chartered exchange, given that every Agents card rests on it and the alternative is the inference the web PRD already labels as one.
2. Whether #436 is closed as answered by the field election, with a smaller ask filed for the per-position read.
3. Whether ruling 1 lands in `weaver-web-PRD` section 4 now or with the live cell's first PR.

What is right about the epic and should not move: the six rulings are stated as rulings, the destinations per surface, and ruling 6, which is the corpus's own stance on what a reading is.

#### toddwbucy, 2026-09-05T04:34:59Z

**Thinkpad_Code_SEAT** - the surfaces and rulings sections are re-cut to
what the canvas of 2026-09-04 draws. Nine surfaces where the first version
of this issue had five, and twelve rulings where it had six.

**What moved and why**, so a reader who saw the first version learns it
changed rather than finding a different issue:

- **Live separated from Measure.** They were one surface in two modes.
  Renaming "Run a cell" to "Measure" made the distinction real: one is
  exploratory, the other scripted, and the promotion between them - an
  exchange saved as a cell - is the move the product turns on. That makes
  the path five rather than four, which the charter act will have to
  enumerate and justify.
- **Branch became Stage**, and stopped being a verb the interface has. The
  operator's ruling: the viewer holds no model and branches nothing; the
  reload that drains a staged experiment is the branch. Ruling 4 records
  that this is the load-boundary rule applied rather than a new rule.
- **Experiments split into a list and a single view**, which is where the
  five states and the register-versus-queue distinction land.
- **The task became a first-class element of a cell**, with four sources.
  It was being drawn as a label, which is how a benchmark binding would
  have been discovered late.

**One vocabulary problem this hands to #439.** "Cell" now carries two
senses: a matrix coordinate, which is what `cells` means in
`experiments/cross-precision-repro/thinkpad-qwen3.json` and
`confirm_cells.py` today, and a declaration plus a task plus a run, which is
what the direction document means. G5 would read that as two authorities for
one word. The charter act settles it once for the corpus.

**Held rather than settled: whether the tool scores.** Behavior and cost are
measurements; a benchmark score is a correctness judgment, and the standing
discipline is that the tool measures and does not testify. The Measure
surface draws the score row as present and unfilled, naming no scorer, so
the shape is held open without the interface implying it judges. Three ways
out: the tool does not score and records the emission for an external
scorer; it scores and records the scorer in the tuple so the judgment is
attributable to a named thing; or scoring lives in the benchmark harness and
the tool cites its verdict. The middle one is this seat's prior.

#### toddwbucy, 2026-09-05T22:03:47Z

Olympus_Code_SEAT

The question Compose raised through #456 is answered on `main` at `20b9cdf`. `weaver-admin-PRD` section 4.3 now states the custody rule: admin adjudicates what it provisions and holds custody of, asks the owner where one can be asked before a process exists, and never substitutes its own test for the owner's. The per-field annotation falls out of it rather than being hand-authored. The uid, the home, the sink, the boundary members and modes, and the store's two gates are caught now. The model artifact is caught at load: admin checks only that it is named, an empty member refusing `ConfigInvalid` naming `spu-instruction.decoder.model-binding.artifact`, and the SPU judges its resolution at admission under the agent's identity, printing one typed line on standard error naming the refusal, the path, the step, and the effective uid and gid, which the harness's last-word tee carries to the journal. The devices and the grant surface are the other two caught-at-load fields, as the same rule's older instances.

#### toddwbucy, 2026-09-07T02:16:10Z

**Epic status, 2026-09-07, against `main` at `ab6c855`.** The document work this epic opened is substantially done. What remains is code, and code is blocked on rulings rather than on apparatus.

## Landed since the last update

**The charter and Spec are rewritten and merged**, ten surfaces in two groups, and the store carries both halves: what the instrument recorded at sections 2.1 and 2.2, what the engineer authored at 2.3 through 2.5, the recorded query at 2.6 belonging to neither, and the indexes at 2.7. Two write paths, because a replay and an edit cannot share one.

**The analysis seam has its contract.** `weaver-analysis-web-contract` merged at PR #459 and is cited from both parties, which closed #418 and #451's second half. Its one real ask cost three acts and two other crates' definitions, at #461, #462 and #463.

**The artifact identity is settled**, at #465 and PR #466: two identities at two grains with a one-way derivation, the record's over everything the binding named and the weights identity over the weights alone, the join a lookup on the first.

**The crate was read against its charter for the first time.** `inventory-weaver-web-code`, PR #468. The implementation predates the rewrite by a charter: 4,531 lines written to a text that was replaced, with a Postgres schema of `participants` and `channels`. **882 lines retire, 1,770 carry, 1,879 want a ruling**, and the conversation half is archived at PR #469 rather than deleted.

**The crate aligns to the workspace edition** at PR #470, and `weaver-web` passes the clippy gate that became the fifth enforcement device at PR #474.

## The backend asks this epic filed, and where they stand

| issue | state |
|---|---|
| #435 observation exchange | landed at #440 |
| #436 per-position alternatives | landed at #444 |
| #437 crates publish what a loop may be built from | answered with a finding, narrow remainder open |
| #438 a reachable organ declares its boundary | rule merged at #452, blocked for the declaration member |
| #442 the queue runner drains staged experiments | step one landed with #432's code half |
| #451 the analysis Spec authorizes the reader | landed at #460 and #466 |
| #461 the position's facts are defined | landed at #462 and #463 |
| #465 one artifact, two identities | landed at #466, relanded at #480 |
| #475 the lifecycle enums box their payloads | landed at #476 and #480 |

**Every one is filed against the olympus seat and none reached across the boundary from here**, which is what this epic said it would do.

## What blocks the next act

**Four rulings at #454**, two of them still open: whether the schema is migrated or replaced, which turns on 37 unclassified rows, and what the five ruling-wanted modules become, which turns out to be the section 6 identity act because `registry.rs` carries the role model and nine call sites cross into the retiring half.

**Under H1 the documents are ahead of the code and that is the intended order.** The first code act is a schema, and the schema waits on ruling 2.

#### toddwbucy, 2026-09-09T21:22:44Z

Thinkpad seat, 2026-09-09. **The lane's next ten acts, with the rulings that let them run without stopping.** Filed here because this epic is the register for the front end's whole lane.

## What stands as of today

The store's schema is complete through migration `0004`: the tables of section 2, the sweep of #518, the record's session and digest of #521, the seated prefix's length of #527. The four reads of section 4 are code and tested. `#435` and `#436` landed, so Agents and Open-a-trace are unblocked. Compose waits on `#437` and `#438`, Branch on `#432`, all olympus's.

## The ten

| # | act | lands | needs |
|---|---|---|---|
| 1 | The ingest consumer, section 3.1 | reads the analysis seam and writes runs, turns and positions; the three section 3.1 records get their instruments | nothing |
| 2 | The Record surface | the run list with its tuple, chips as queries, the ablation matrix's entry point per the sketch's section 3 | 1 |
| 3 | The retirement | `queue.rs` out and `registry.rs` split, per the inventory register's standing rulings | nothing |
| 4 | The ablation matrix documents | the sketch's section 7 into the charter and the Spec: the plan, its cells, the refs, the column states mapped onto section 5.1's five | the rulings below |
| 5 | The plan schema, migration `0005` | the two authored objects with their author and version per section 3.2, and the reads over them | 4 |
| 6 | Stage | a click authors a registered experiment, validated at authoring per section 5.3 | 5 |
| 7 | The matrix surface | the mockup of `docs/project/mockup-ablation-matrix.html`, against the plan | 6 |
| 8 | Agents | the saved declarations roster | nothing |
| 9 | Open a trace | `traceview.rs` rewritten against the four reads | nothing |
| 10 | Live and Measure | the exchange with its readout beside it, per this epic's ruling 1 | 8 |

## Operator rulings of 2026-09-09, so the queue does not stop

1. **The ingest is its own binary**, `weaver-web-ingest`. It can be run against a finished record with no server standing, which is how it is developed and how it is tested, and the connector stores nothing by its own charter so it is not there.
2. **The word "cell" is settled at three senses and one keeps it.** A matrix **column** is this epic's cell, a declaration and a task and a run, which is what a column already is. The grid intersection is an **entry**. An unsettled question stays an **open election**, which is what section 10 calls its own. The Spec's section 10 bullet closes with the matrix documents act, and that act sweeps the sketch and the mockup to the settled words.
3. **The arm's score is declared now and absent until its event exists.** The kind is olympus's per #523 and the shape is the seated prefix length's exactly: the Spec's section 2.2 holds the member, the schema holds it nullable, the ingest fills it when the event lands. The matrix is built against a member that reads absent rather than waiting on a crate this seat does not touch.
4. **Attribution is parent-relative in v1.** Every column's diff is stated against the parent and no column names another column as its baseline. A reader isolating one member of a compound column reads it against a sibling themselves. The plan stays a tree, registration freezes one reference per column, and a dropped column orphans nothing.

## Three decisions this seat makes and shows, per this epic's own last section

- **Surprisal and entropy draw as two timelines.** One would have to draw an absent surprisal, and the Spec's section 6 forbids drawing an absence as a zero. Lands with act 9.
- **The generated default is one at a time**, per the sketch's section 6, with the interaction measurement named there rather than run.
- **The preset ladder waits with Compose**, which is blocked on `#437` and `#438`, so the choice is not owed yet.

## What this queue does not reach

Compose, Branch, and the runner of `#442`. The first two wait on olympus's acts and the third is the party that turns a registered experiment into a run, which is why acts 6 and 7 end at a registered experiment and a matrix that reads absences honestly rather than at a batch that ran.

#### toddwbucy, 2026-09-11T14:41:20Z

Thinkpad seat, 2026-09-11. **The second ten, with the blockers found before the first line of any of them.** The first ten's queue is the comment of 2026-09-09 above and its acts 1 through 3 merged as PRs #533, #534 and #535.

## What the first ten bought

The store is a schema with six migrations, four tested reads, and a producer named for every column it keeps. The word "cell" is settled at three senses. The plan and the refs have rows. The seam carries the run and what it ran under. **Nothing in the recorded half has been written yet**, which is what this ten is for.

## The blockers, found first

**One is the lane's spine and had no issue until today.** The contract asks `weaver-analysis` for ten members it does not send, across three acts, and **until that lands the front end can hold no run**. Filed as issue #538. Every act below that reads real data waits on it, and the ingest is written against the contract with fixtures so the integration has a consumer waiting rather than a design to agree on.

**The retirement is all-or-nothing and takes the crate's whole web surface with it.** `queue.rs` is called from `weaver-web.rs`, `web/mod.rs` and `router.rs`, `registry.rs` from five modules, and the register already found nine call sites crossing into the four. So a new surface stands before the old one goes, which puts the retirement at act 8 rather than act 3 where the first queue had its equivalent.

**The score's web half cannot land.** Issue #523's trace kind is `weaver-trace`'s and does not exist, so the verdict has a seam and no producer. Its column is deliberately not in the schema, per the apex's rule against reserved slots, and arrives in the act that lands the kind.

## The ten

| # | act | what it lands | blocker |
|---|---|---|---|
| 1 | The ingest consumer, section 3.1 | `weaver-web-ingest`, the run row written first and closed last, positions in bulk per window, idempotent on the key. The three section 3.1 records get their instruments | real data waits on #538, fixtures do not |
| 2 | The plan schema, migration 0007 | the plan, its entries and the refs of sections 2.9 and 2.10, with author and version per 3.2, and their reads | none |
| 3 | The Record surface | the run list with its tuple, chips as queries, the ablation matrix's entry point | none, seeded rows until 1 has data |
| 4 | Stage | a click authors a registered experiment, validated at authoring per 5.3 | needs 2 |
| 5 | The matrix surface | the mockup against the plan, register and queue as two acts | needs 4 |
| 6 | Agents | the saved declarations roster | none, #435 landed |
| 7 | Open a trace | `traceview.rs` rewritten against the four reads | needs 1, and #538 for real positions |
| 8 | The retirement | the conversation half whole, once 3 and 6 answer on the same routes | needs 3 and 6 |
| 9 | Models, issue #454 | the artifact catalog, and where the precision label of #532's act is authored | none |
| 10 | The score's web half, issue #523 | section 2.2's member landed and its column added | **blocked** on the trace kind |

## Rulings of 2026-09-11, so the queue does not stop

1. **The emitter's act is filed as one issue naming all ten members**, #538, rather than three tracing to the three contract acts, because one emitter change lands in one commit.
2. **The ingest is written to the contract and tested against fixtures.** The wire shape is documented whole, so the act is writable today and its records get their instruments. Integration against a live emitter is its own act when #538 lands.
3. **The retirement waits for a new surface.** Act 8, after Record and Agents answer on the routes the old ones hold.
4. **The tenth slot is Models rather than Live and Measure.** Models is unblocked, closes a standing issue, and is where the precision label invented this week is authored. **Live and Measure needs the connector and the gate adapter working, which nothing has proven**, and it is the largest act in the lane, so it opens the third ten rather than closing this one.

## What this ten does not reach

Compose, which waits on #437 and #438. Branch and resume, which wait on #432. The runner of #442, which is what turns a registered experiment into a run, and which is why acts 4 and 5 end at a registered experiment and a matrix that reads absences honestly rather than at a batch that ran.

## Two gates this ten inherits, both bought expensively

**A migration is verified from empty and against a populated database.** The two worst findings of the first ten were invisible to every check run against a fresh one: an amended migration that would have broken every box that ran its first form, and a check constraint that refused to install where rows already stood.

**The documents gate runs over `docs` whole**, not `docs/crates`, with the semicolon, blank-after-marker and whitespace-normalized count checks. A crate-by-crate check cannot see a document that is not a crate's, which is how a ruling's sweep missed six sites in the technical tree.

#### toddwbucy, 2026-09-26T15:03:02Z

**weaver-web left the WeaverTools repository on 2026-09-26**, on the operator's ruling of that date: the frontend "does not earn its keep being this close to the code and it could in fact be largely developed independently of weavertools, so keeping it in here actually slows production down." The crate and its documents now live at `WeaverTools_Project/weaver-web/` beside this repository, with their history carried by subtree split (destination commit `f2f01d8`); the removal here is #689. Its repository is to be decided later, and the transfer of this issue waits on that decision. The issue stays open here until then.

---

## #454: ThinkPad web: the models surface, and the catalog two surfaces already assume

- **Source:** toddwbucy/WeaverTools#454
- **Opened:** 2026-09-05 by toddwbucy
- **Labels:** enhancement, medium-priority, seat:thinkpad, web, frontend
- **State at carry:** OPEN

### Body

Seat: thinkpad. From epic #434, on the operator's direction of 2026-09-05.
The charter and Spec gain the surface in the same act.

## The thing that is missing

**Two surfaces already assume a catalog and neither owns one.**

`weaver-web-PRD` section 3.1 has the roster resolving each component's
artifact, by download or by reference to where the model already sits, and
the Compose figure draws `qwen2.5-0.5b-instruct-q6_k.gguf` with a digest
beside it and the words "resolved on host". **Nothing says what resolved
it.**

Section 5's qualification axis needs more: a reference cell is run against
an artifact, a lens artifact is versioned by the weights hash it was fitted
to, and a refit produces a new artifact. So there is a real object with real
relations - artifact, its weights identity, the lenses fitted to it, the
reference cells taken against it - and no surface holds it.

## Why it is a surface rather than a mode of Agents

An agent is a configuration and a model is an artifact. **Many agents share
one model, and many lens artifacts attach to one model**, so folding the
catalog into Agents would put a many-to-one relation inside the wrong noun.
Import is its own motion besides: fetch or convert, verify identity,
register, then see what has been fitted and qualified against it.

## What it holds

- **The artifact and its identity.** Per file, following the model's own
  index and verifying each shard under its own name, per #415. A single
  GGUF is the one-file case of the same rule.
- **Provenance.** Downloaded from a repository at a revision, or converted
  from another artifact by a named converter at a pin with its flags. The
  campaign's configs carry this as prose today and the catalog should carry
  it as fields.
- **Presence.** Which box holds it. The front end runs on one machine and
  the agents on another, so a model present on olympus is not present here,
  and Compose should not offer what a load will refuse.
- **Relations.** The lens artifacts fitted to these weights, the reference
  cells taken against them, and what has been qualified.

## What it must not hold

**A catalog of organ kinds.** Per #437's finding the charters hold the
roster, and a kind arrives as a charter act before it is a row anywhere. So
this surface catalogs artifacts and is kind-agnostic: an artifact acquires a
kind when a declaration references it and admit judges the family. Encoders
and rerankers then need no special case here - they arrive with their
charters.

## Documents

`weaver-web-PRD` gains the surface in section 3, which makes it the tenth
and moves the count. `weaver-web-Spec` gains its module under the section 6
rule and whatever the catalog's rows are under section 2, beside the run and
the position.

## Open, and named rather than assumed

- **How presence is answered.** The front end cannot see the agent box's
  filesystem. Either the catalog records what it was told at registration,
  which is a claim rather than a fact, or admin answers it, which is a new
  ask on the pattern of #440's observation exchange. **The second is more
  truthful and the first is free**, and this issue does not choose.
- **Whether import fetches.** A surface that downloads is a surface that
  needs credentials and a network posture, and neither is chartered.
  Pointing at what already sits on disk is the smaller act.

Refs #434, #415, #437.

### Comments (4 human of 4)

#### toddwbucy, 2026-09-05T22:03:47Z

Olympus_Code_SEAT

For the presence cell this surface names in section 9: the ruling on #456 landed on `main` at `20b9cdf`, and it settles the admin half of the question. Admin adjudicates only what it holds custody of and the artifact is the SPU's to judge at admission under the agent's identity, so admin is not the party to verify shards or report presence on the agent box. What stands is the SPU's admit, which hashes the weights and whose record carries them, so presence on a box is derivable from recorded admits as a fact rather than taken as a claim at registration. Which answer the cell takes is the operator's, per the review comment on #453.

#### toddwbucy, 2026-09-06T13:55:39Z

Olympus_Code_SEAT

The identity subject this surface's catalog raised is settled on `main` at `6fcb537` through PR #466, issue #465: the catalog keys on the per-file map and carries the record's identities beside it, computed at import, so a run row resolves to a catalog row. One residue stays open at `weaver-web-Spec` section 10, the split GGUF's names, with the ruling owed before the staged 35B BF16 is first imported.

#### toddwbucy, 2026-09-07T02:15:50Z

**Status against `main` at `ab6c855`.** The documents half is done and this issue's remainder is four rulings, one of which moved since the last update.

**What landed.** Models is chartered at `weaver-web-PRD` section 3.6 with its identity, provenance, presence and relations, and with the import motion added 2026-09-06: import registers weights already present, **computes the identity rather than accepting one**, and an artifact whose files do not answer the identity rule does not register. `weaver-web-Spec` section 2.3 carries the catalog. The kind-agnostic clause this issue asked for is at the charter's own words: an artifact acquires a kind when a declaration references it and admit judges the family.

**And the hole under it closed too.** Section 2 had five tables and every one recorded, while three surfaces listed authored objects with one table between them. PR #464 gave the declaration and the staged experiment their rows at 2.4 and 2.5 and split the write path into the ingest and the authoring path, so the catalog is now a table something can write into.

## The four rulings, and one has changed

1. **Whether the retiring 882 lines are deleted, kept, or archived.** Ruled archived on 2026-09-06, landed at PR #469: `crates/weaver-web/archive/conversation/` holds the four modules and five templates with a manifest and verified checksums, frozen from a named commit. **The build still compiles the originals**, and their removal is now blocked on ruling 3 rather than on this one.

2. **Whether the schema is migrated or replaced.** The five tables share no column any of the seven wants, so a migration between them is a drop and a create rather than an alteration. **What would be discarded is answered for one box**: olympus reported 259 `channel_events` across two days, of which 158 turn rows mirror traces under admin's custody, 64 are messages with no record elsewhere, and **37 are unclassified** - 21 member changes and 16 application errors, the second being this crate's own faults that no trace carries because they never reached an agent. Which other boxes hold a store, neither seat can see.

3. **What the five ruling-wanted modules become**, and this is where the archive act found something. `registry.rs` does not retire whole: it carries `Participant`'s `role` and `is_admin()`, which `web/admin.rs` gates on, and that is the role model the charter's section 6 keeps as structural. **Nine call sites cross into the retiring four from modules that do not retire**, so the removal is the section 6 identity act rather than a lift. Issue #336 is the other half of the same question.

4. **Whether the edition alignment rides this work.** Done, and it was not what its comment predicted. PR #470 aligned the crate to edition 2024: `cargo check` and `cargo test` passed unchanged, the sqlx and askama derives did not bite, and the cost was twelve `collapsible_if` warnings and one `fmt` run. `weaver-web` passes the clippy gate clean.

**The register is `inventory-weaver-web-code`**, merged at PR #468 and revised twice since.

Still `medium-priority`, still `seat:thinkpad`. **Nothing in this lane moves without rulings 2 and 3.**

#### toddwbucy, 2026-09-26T15:03:03Z

**weaver-web left the WeaverTools repository on 2026-09-26**, on the operator's ruling of that date: the frontend "does not earn its keep being this close to the code and it could in fact be largely developed independently of weavertools, so keeping it in here actually slows production down." The crate and its documents now live at `WeaverTools_Project/weaver-web/` beside this repository, with their history carried by subtree split (destination commit `f2f01d8`); the removal here is #689. Its repository is to be decided later, and the transfer of this issue waits on that decision. The issue stays open here until then.

---

## #589: thinkpad: eight weaver-web tests pass asserting nothing when DATABASE_URL is unset, and the run does not say so

- **Source:** toddwbucy/WeaverTools#589
- **Opened:** 2026-09-15 by toddwbucy
- **Labels:** frontend
- **State at carry:** OPEN

### Body

Eight tests in `crates/weaver-web/src/store/read.rs` return success asserting
nothing when `DATABASE_URL` is unset, and **the default test run does not say
so**. Filed from the v5 audit's `TAG-08`, which epic #569 calls "the sharpest of
these". Declined for `act-18` (PR #585) because #569 scopes that act to comment
lines and flags this as the one leg that could change behaviour. Every reading
below taken at the thinkpad seat on the act-18 branch.

## The guard

```rust
// crates/weaver-web/src/store/read.rs:501
pub(crate) async fn store() -> Option<Store> {
    let Ok(url) = std::env::var("DATABASE_URL") else {
        eprintln!("skipped: DATABASE_URL is not set, and a read over a schema is tested against one");
        return None;
    };
    Some(Store::connect(&url).await.expect("connect and migrate"))
}
```

Eight `#[tokio::test]` functions open with
`let Some(s) = store().await else { return };` - **all eight tests in the file,
and the only callers**:

```
read_one_is_addressed_by_the_whole_key_and_absence_is_none
read_two_is_ordered_and_inclusive
read_three_carries_the_seed_the_record_spells
read_four_returns_every_value_and_an_unrun_arm_keeps_its_place
read_three_names_the_record_the_row_came_from
read_five_pages_a_tie_whole_and_records_nothing
read_five_filters_on_each_indexed_column
a_point_arm_returns_its_run_with_no_value
```

## The part that makes it worse than a skip

`eprintln!` is **captured by default**, so the reason never reaches the operator:

```
$ cargo test -p weaver-web --lib
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured
   -> 0 occurrences of "skipped: DATABASE_URL"

$ cargo test -p weaver-web --lib -- --nocapture
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured
   -> 19 occurrences of "skipped: DATABASE_URL"
```

The eight are **counted among the twenty-three passing**. Not `ignored`, not
reported, indistinguishable from eight tests that ran. `DATABASE_URL` is unset on
the thinkpad seat, so that is the reading a person here gets today.

This is the conversion the corpus's own perturbation rule exists to prevent:
**"a test that passes either way converts unenforced into documented as enforced,
which is worse than no test."**

## What the documents claim over them

`read.rs` carries three `conforms:` citations:

```
:1    web-nothing-is-computed-at-read-time-unless-the-query-is-recorded   tag: review
:756  web-the-run-list-is-paged-and-records-nothing                       tag: perturbation
:844  web-a-chip-filters-only-on-an-indexed-column                        tag: review
```

One is **perturbation-tagged**, which is the tag that asserts an instrument was
bought and confirmed to fail on removal. `DATABASE_URL` appears **nowhere** in
`docs/` or `process/` except the audit that filed this finding, so no document
states the precondition the instrument actually has.

## The operator's ruling, between two shapes the corpus already knows

1. **Hard-fail the guard.** `DATABASE_URL` unset becomes a test failure. The
   suite then tells the truth on every box, and it stops passing on this one
   until a database is there.
2. **Park it conditionally, the way issue #397 parks CUDA.** The corpus already
   has this shape: an instrument verified where the hardware is, stated at the
   clause and in the document, not silently skipped. That wants the precondition
   written into `weaver-web-Spec` and the `perturbation` tag re-examined, since
   a conditional instrument is not the same claim as an unconditional one.

Either way `#[ignore]` with a reason is better than a silent `return`, because
`ignored` appears in the result line and a bare return does not.

**Not settled here.** Option 1 changes what the gate command does on every seat;
option 2 changes a tag and owes a document. Both are behaviour changes wanting
their own act.

## Related

- Epic #569, `act-18-web-spec-sections` - where this was assigned and declined
- PR #585 - the decline, with reasoning
- Issue #397 - the CUDA parking that option 2 would follow

### Comments (1 human of 1)

#### toddwbucy, 2026-09-26T15:03:04Z

**weaver-web left the WeaverTools repository on 2026-09-26**, on the operator's ruling of that date: the frontend "does not earn its keep being this close to the code and it could in fact be largely developed independently of weavertools, so keeping it in here actually slows production down." The crate and its documents now live at `WeaverTools_Project/weaver-web/` beside this repository, with their history carried by subtree split (destination commit `f2f01d8`); the removal here is #689. Its repository is to be decided later, and the transfer of this issue waits on that decision. The issue stays open here until then.

