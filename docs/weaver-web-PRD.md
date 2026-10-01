# weaver-web - PRD (crate charter)

**Status:** MERGED. In `main` and the source of truth. **Ratification is not
claimed by the act that landed this text**: the charter it replaces was
ratified on its own terms under the per-charter rule of 2026-08-23, and a
rewrite does not inherit that. Whether this text has cleared its gates is
the operator's to say.

**This is a rewrite of this crate's charter, not a new one.** The document
ID, the crate, and its node are unchanged, and the prior text is archived
rather than amended, per the operator's ruling of 2026-09-04, because the
purpose clause is what changed and every section below it was that clause's
consequence.

**Placement is held.** The deployment topology is ruled: this crate and its
store run on one machine, the agents on another, and the only crossing is
store traffic and a queue. Whether these papers eventually move to a
separate corpus follows from that but is not identical to it, and stays
decision two of #439.

**Date filed:** 2026-09-04
**Document ID:** `weaver-web-PRD`
**Parent:** the WeaverTools suite, per `weaver-agents-PRD` section 0. **The apex is
in another repository since 2026-09-26**: `weaver-agents-PRD` lives in
`toddwbucy/WeaverAgents` since the suite split of 2026-09-30, and before that in
the monorepo, now `toddwbucy/WeaverTools-old2`, whose commit `112bc65` is the tree
this crate and its documents left on the operator's ruling of 2026-09-26, so the
parent edge crosses a repository and the graph that holds both ends is that
repository's.
**Editorial:** Per the Working Rules. ASCII, absolute dates.
**Landing PR:** #632

## 1. What this crate is

WeaverTools is a diagnostic and analytic instrument. It lets an engineer
stage an entire agent on one machine, run it under controlled conditions,
and take baseline readings of its behavior and cost that a second person can
reproduce. Having taken the baseline, the engineer loosens the arrangement
one variable at a time and reads what each loosening did.

**The back end already takes these readings, and nobody but their author can
operate it.** This crate is what makes them operable by an engineer who did
not build the apparatus. That is the whole of its claim, and it is why the
crate exists at all. It is a crate of WeaverTools, the instrument this section
opens on, and of no domain inside an agent, which is what its parent edge says.

```graph
node: weaver-web
kind: crate

edge: parent
from: weaver-web
to: WeaverTools
```

**The gate reach is real and carries no seam edge**, as of 2026-09-16. A seam
edge runs between two crate nodes and the far end of that boundary is the world,
which the graph carries no node for, so `weaver-gate-world-contract` names one
crate and signs the world in prose, per its section 0. A record ran from this
crate to `weaver-gate` until that date and named that page as its `via`, which
asserted a signature this crate never gave. The boundary is unchanged and only
the record went. **The reach itself is argued in `weaver-web-Spec` section 7.1,
which is the authority on it under G5**, and this charter states the absence
rather than arguing the reach.

The one-sentence job: **compose a configuration, run it, and return behavior
and cost together, with the configuration declared well enough that a second
person can rerun it.** Every surface serves that sentence or it does not
ship.

The person in front of it is an engineer deploying a local model as an
agent, who needs to know before shipping what the arrangement can tolerate,
and who has been working on intuition because **nothing accumulates**.
Readings exist and are taken every day. What has not existed is a reading
that comes back as a trial carrying its declared tuple, which a second person
reruns and gets again.

**The positioning rule reaches the interface.** The tool measures. It does
not testify. Every reading this crate shows is labelled as a reading of
something. A surprisal spike is a property of a distribution over tokens. A
lens readout is availability of a feature for downstream computation.
Neither is an inner state, and no copy on any surface may imply one. This is
the standing overclaiming discipline pointed outward, and its purpose is
that anyone quoting the tool as evidence for a ghost in the machine can be
shown, from the tool's own labels, to be wrong.

## 2. What it is not

- **Not a chat client with a settings page.** A conversation is one way to
  produce a reading, not the product's centre.
- **Not an operator console for a production agent.** Production deployment
  is a private derivation and out of scope.
- **Not a door to a hosted inference endpoint.** The instrument measures a
  model whose process the operator compiles. Point it at a foundry API and
  the trace is a transcript, the residual stream is gone, and every surface
  past the first is dead.
- **Not a mutator of a loaded agent.** No in-RAM edit of behavior,
  parameters, or loop logic exists in any path. Every change is a reload,
  and the interface makes the reload the visible event it is.
- **Not a judge of a declared boundary.** It requires the declaration and
  displays it. It does not grade it.
- **Not a logprob viewer.** Per-token logprobs are drawn by a dozen tools,
  and an engineer meeting section 3.4 will name one they already use. The
  difference is not the picture. It is that a reading here is a trial with
  its declared tuple, addressed by run, turn and position, and rerunnable by
  someone who was not there. Claiming the picture as the novelty would be
  the overclaim section 1 forbids, pointed at ourselves.
- **Not a knowledge-graph governance layer.** No axioms, smells, gates, or
  conformance as product features.

## 3. The surfaces

Ten, in two groups. **Every surface has a direction**: you enter somewhere,
you end somewhere, and the screen says where you are on that path. The
failure to avoid is the interface that exposes its graph before it shows the
artifact, so that nothing on screen says what you came to produce. Each
surface below states its destination in its own header, and the top bar
carries the whole path on every screen.

Clean does not mean barren. A screen with nothing on it is not clean, it is
empty. Clean means the next step is obvious and the numbers appear when
asked for.

**Each surface below is drawn.** The figures are the design canvas of
2026-09-04, authored from the thinkpad seat against measurements taken on
that box, and they are a reading aid rather than an authority: where a
figure and this text disagree, this text governs and the figure is owed a
redraw.

### 3.1 Compose, ends with a runnable declaration

You cannot load an agent you have not created, so the first surface authors
one. The artifact is the declaration, presented as a composition surface
rather than as a text file, with the text always reachable and always the
authority: edits in either place round-trip to the other.

The declaration is a roster plus a wiring. Each declared component resolves
its artifact, sets its elections, and takes its own socket. The wiring is
the loop, and it is real topology the operator draws.

**The roster offers the organ kinds the framework charters, and carries no
enumeration of its own.** As of 2026-09-05 that is the decoder, mandatory,
and the classifier, optional since 2026-08-19. Encoders and further kinds
arrive as their own charters land. This is a dated fact rather than a
definition, and the direction of authority is the point: **a new kind is a
charter act before it is a row on this surface**, so a list maintained here
would be a second place for a fact the charters already hold, and would
drift from them silently.

Offering a kind the framework will not load is therefore the failure to
avoid, not offering too few.

**Whether a given declaration will load is not this crate's judgment
either.** It writes the draft and asks `validate`, which transitions
nothing, refuses an incoherent declaration naming the field, and reaches the
box facts admin holds custody of. Composition-time refusal is that verb one
hop from this surface, and it cannot drift because it is **the contract
chain's own shape at a pinned commit** rather than a second copy of it.

**A clean `validate` is acceptance for filing and not approval to load**,
per `weaver-admin-PRD` section 4.3 as ruled 2026-09-05 on issue #456. Admin
adjudicates what it provisions and asks the owner where one can be asked
before a process exists, and what only the organ can judge waits for
admission under the agent's identity. **This surface therefore owes the
engineer, per field, whether a mistake there is caught now or at load**, and
that annotation is derived rather than authored: caught now where admin
holds custody or can ask synchronously, caught at load where only the organ
can answer. The artifact is the clearest field of the second kind.

<!-- figure: Main | Compose -->

### 3.2 Live, ends with an exchange you can read the numbers on

The exploratory surface. An engineer types, the agent answers, and **the
per-token measurement rides beside the reply rather than behind a role**.
This is the ruling that supersedes the archived charter's split of a
conversation surface from a record surface: an engineer cannot troubleshoot
a prompt when the thing they typed and the numbers it produced sit on
different pages.

Behavior and cost arrive together here as they do everywhere. **An exchange is not yet a
trial**, its task being ad hoc until the operator saves it: the save is what gives it
one, a session promoted from Live being the fourth of section 3.3's task sources, and an
exchange saved that way becomes repeatable, which is the promotion this crate turns on.

<!-- figure: Chat | Live -->

### 3.3 Measure, ends with behavior and cost, together

The scripted surface. **A trial is a declared configuration, a task, and a
run**, and what it returns is a pair: what the agent did, and what it cost
to do it. Both axes appear on every result and every comparison. A loop that
reaches the right action four times slower has moved the trade rather than
won it, and a result that shows correctness without latency is half a
reading.

**The task is a first class element with four sources**: a turn list, a
benchmark suite and an item within it, a corpus walk, or a session promoted
from Live. The task joins the run's tuple, because two runs of "the same
task" are not the same task unless it matches.

The comparison this crate exists to make cheap is one trial against another
differing in one declared variable, and the difference is legible at a
glance: this trial is that trial with the device moved, or the transport
moved, or the seed moved.

<!-- figure: Trial | Measure -->

### 3.4 Open a trace, ends with a located position

**Text is the surface. Numbers are on demand.** The transcript of a run,
token-addressable, with the measurement drawn as a timeline over it.
**Per-token entropy comes off the ordinary logit stream and rides every
generation unconditionally**, so an entropy timeline is populated on every
run at no cost. **Surprisal rides only where its election stands**, so a run
that did not elect it has no surprisal timeline, and the surface says the
election did not stand rather than drawing a floor.

A spike on the graph resolves to the token that produced it and jumps the
transcript there. **A surprisal graph nobody can click is decoration, and a
spike that resolves to a token is an instrument.** Clicking a position pulls
that position's alternatives and their mass from the capture, and the
operator walks upstream watching the mass move until the position where the
fork was decided is found, which is usually earlier than the spike.

The lens readout is the deep view and it is elected rather than free. It
requires a reload under the diagnostic binding and captures at every
position of the prefix, because the choice at position N was built by
everything before N. The cheap signal tells the operator where to spend the
expensive one, and that funnel is the honest reason diagnostics are off by
default.

<!-- figure: Trace | Open a trace -->

### 3.5 Stage, ends with a registered experiment

**This crate branches nothing.** The viewer holds no model. A click on an
alternative authors a staged experiment carrying the parent run reference,
the branch position, the forced token where one is forced, and the parent
declaration with its diff. A runner drains the queue, and **the reload that
runs it is the branch**.

That is not a new rule. The load boundary is the only change boundary, a
branch is a change, and therefore a branch is a load. An interface that
branched directly would be mutating a loaded agent, which section 2
refuses.

**Two features author from this surface, and a branch is a load in both.**
An **ablation** moves one member of the tuple and runs the arm whole from
the load. The mechanism carries the cut and not the continuation: a
restoring load's lineage names the parent, the run the cut falls in, and the
turn the holdings stop at, and the re-issue of the parent's turns to a fresh
arm is the queue runner's, which issue #442 holds open. An arm run whole
from the beginning is a fresh load whose parent the store remembers, and the
Spec's section 5 says what its branch position is: the parent's identity
prefix's length, **which crosses the analysis seam per
`weaver-analysis-web-contract` section 2.2 as of issue #527's act** and
lands on the run's row, so the Spec's section 5.3 validates it from the
parent's row. Issue #527 is that prerequisite's register. A **fork**, the click on
an alternative above, forces a token at a position inside a turn and
continues from it, so its branch position is that position. **No contract yet carries
a cut inside a turn**, and the fork waits on one, per the operator's ruling
of 2026-09-09 that a branch occurs at load time, each arm a separate run
under its moved value and the model reloaded for the next. The ablation
ships first and the fork after it. The row's members are not narrowed for
the wait: the forced token and its position stay, and serve the fork alone.

**A forced token is not a sampled one**, and a trajectory reached by forcing
is never quotable as something the model produced on its own. The forcing is
declared on the branch run and the record carries it.

Fork the same position many times under fresh seeds and the result is the
distribution that one repeatable line was drawn from. A deterministic run is
not an average of anything. It is one sample that can be fetched again, so
determinism buys reproducibility rather than centrality. Because the seed is
drawn per generation from the declared seed, the turn's reference and the
ordinal, **a branch that changes nothing draws what its parent drew**, and
the control arm is free.

**Many experiments against one parent are authored here too, and the object that holds
them before any is registered is the plan.** An ablation moves one member of the tuple
per column and asks which members the outcome depended on, so what the operator composes
is a parent, a set of columns and a disposition for every member of every column, and
the Spec's section 2.9 gives it a row. **A plan is not a batch.** Registering it freezes
each column as a staged experiment and puts the claim on the record, queueing hands them
to a runner, and section 3.6's pre-registration is exactly what would be spent by an
interface that did both in one gesture.

**A column's diff is stated against the parent and never against another column.** Two
columns that each moved two members are two readings against one baseline, and a reader
who wants the one member between them reads them against each other, which is an act of
reading rather than a member of the plan. The alternative was weighed: a column naming
another as its baseline makes the plan a graph, makes registration freeze a reference as
well as a value, and orphans dependents when a column is dropped. **The plan stays a
tree.**

**What a bound cites outlives what produced it.** A batch that establishes a failure
envelope produces arms whose value was to be measured once, and the runs are
scaffolding. The Spec's section 2.10 gives a person a named reference to a run, so what
the envelope cites is reachable and the rest is discardable, which makes the discard a
question about reachability rather than a judgment exercised twice.

<!-- figure: Branch | Stage -->

### 3.6 The five on the list side

Global and filtered. **No surface writes what the instrument recorded.** A position and
a run land by the ingest alone, per the Spec's section 3.1. Three surfaces do author,
writing the tables section 4 names as theirs: Compose a declaration, Models an artifact,
and Stage the staged experiment together with the plan and the refs of section 3.5.
These lists read the store and filter it, which is why a chip can be a query rather than
a location. **Four of the five are lists and the fifth is not.** Agents, Models,
Experiments and Record are list destinations. The Experiment view is the detail surface
one of them opens into, and it is a surface rather than a mode of the list above it: its
own destination, its own state, and its own module under the Spec's rule of one module
each. **The group is named for how it is reached** - by a chip-query rather than along
the path - and not for the shape of what it holds, so no implementer reads the detail
view as a fifth list. Five on the path and five here are the ten this section opens
with.

**One navigational grammar: a chip is a query rather than a location.**
Clearing it widens the list where the operator stands, and nothing
navigates, so it is always visible what is not being seen. A card carries
the operator into a list with a chip already set.

**Agents** holds the saved declarations, loadable at any time, each carrying
its parent and the one thing that moved where it is derived.

<!-- figure: Library | Agents -->

**Models** holds the artifacts, which is the catalog two surfaces above
already assume. Section 3.1's roster resolves each component's artifact and
section 5's qualification runs a reference against one, and neither says
where the artifact came from or what is known about it. This surface does:
**the artifact's identity per file**, following the model's own index and
verifying each shard under its own name. Its provenance, downloaded from a
repository at a revision or converted from another artifact by a named
converter at a pin. Which box holds it, since this crate runs on one
machine and the agents on another. And its relations, the lens
artifacts fitted to these weights, the readings taken through a lens fitted
to other weights where the operator elected that reuse, and the reference
trials taken against them.

**Models is the one surface on this side that authors, and its motion is
import.** The others list what other surfaces produced. Import registers an
artifact, which is what gives Compose something to resolve against and
qualification something to run a reference against, and **it ends with an
artifact other surfaces can name.**

**An identity is computed and never accepted, and where it is computed
depends on which box holds the weights.** An operator who types a digest has
told the catalog what they believe, and a catalog that records a belief
cannot later say two boxes hold one artifact. **On this crate's own box the
surface reads the files and derives the identity**, following the artifact's
own membership rule, which is an index where the container carries one and a
name pattern where it does not, per the Spec's section 2.3. **On the agents'
box it cannot read anything**, this crate running on one machine and the
agents on another, so an artifact there registers from what the record
reports of it rather than from a read, and until that path stands an entry
for it is a presence observation and not a registration. Which of the two
this surface offers first is section 9's open cell on presence, and the
answer chosen there is the answer here.

**An artifact is the complete set its own index names, and one rule decides
identity** - the corpus's own. `weaver-analysis-PRD` section 3 carries the
rule, the weights by content hash, and that charter's Spec section 3 carries
the shape it takes on disk: one digest for a model kept in one file, and a
digest per shard keyed by shard name for a sharded one. Equality is set
equality, so a set missing a file the index names is unequal to the complete
one and is not that artifact, and the catalog dedupes on that and nothing
else. **This crate defines no digest of its own**, a second rule for one
subject being what the paragraph below refuses for the lens. **The record
carries a second identity at a second grain and the row holds it too**, as of
2026-09-06 per issue #465: the weights hash the SPU computes at admit per
`weaver-spu-Spec` section 3 covers everything the binding named, a sidecar the
index never names included, so it determines the catalog's key and the key does
not determine it, and import computes both over the files it registers. That is
not a third rule. It is the record's, held where a run's row can resolve to a
row of this catalog. **The
provenance chain is recorded and is not a second identity**: it says what an
artifact was made from, which is what makes an elected lens reuse across a
conversion legible afterward, and it does not version a lens. The lens
identity is `weaver-analysis-PRD` section 3's, and a conversion changes it.

**Presence is an observation and never a gate.** Which box holds an
artifact is dated, attributed, and advisory, and a box that has not
reported is unknown rather than empty. No load consults it - the load
resolves the artifact on the box it runs on and is refused there - because
a gate built on a stale observation refuses a box that has the artifact and
admits one that lost it.

It is a surface rather than a mode of Agents because an agent is a
configuration and a model is an artifact: **many agents share one model and
many lenses attach to one model**, and a many-to-one relation held inside
the wrong noun is where a catalog goes wrong.

**It catalogs artifacts and not organ kinds.** The charters hold the roster
per section 3.1, and a kind is a charter act before it is a row anywhere, so
an artifact acquires a kind when a declaration references it and admit
judges the family. Encoders and further kinds need no special case here,
they arrive with their charters.

<!-- figure: Models | Models -->

**Experiments** holds every experiment and where it stands across five
states: draft, registered, queued, running, returned. **Registering freezes
the configuration and puts the claim on the record whether or not it ever
runs**, and queueing is a separate act. So an experiment registered and
never run stays visible, saying what was meant to be asked, and
pre-registration falls out of the interface rather than being imposed on it.

Experiments belong to no agent. An experiment's parent is a run, its diff
may move the declaration, and the run it produces can belong to a different
agent than the one it branched from, so it appears under both.

<!-- figure: Experiments | Experiments -->

Opening one goes to its own view, which is where the configuration is
changed and registered. **The diff is split by when it takes effect**,
because a load-time move and a per-generation move differ in what the result
licenses: a load-time move re-feeds the prefix under different weights or a
different window, so the parent's internal state is not reproduced, the text
upstream matches and the state does not, and the comparison is structural
rather than byte-exact. A load-time move also derives a declaration, which
stands in Agents beside its parent.

**An experiment is validated when it is authored, not when it is drained.**
A capacity that cannot hold the prefix, a forced token absent from the
capture, an artifact that will not resolve: each refuses at the point of
authoring rather than at three in the morning by a runner that cannot ask.

<!-- figure: Experiment | Experiment -->

**Record** holds every run, branch and deposit with the tuple that produced
it. A reading without its tuple is a reading of an unnamed compound, and a
registry that cannot hold a failure is a marketing surface.

<!-- figure: Lineage | Record -->

## 4. The record this crate holds

**It holds two things and they are not the same kind of thing.** What the instrument
recorded is a fact about a run that happened, and a second write of it is a replay. What
the engineer authored is a thing someone is still making, and a second write of it is an
edit. **The recorded half is described below. The authored half is what three surfaces
write, each named with what it writes**: Compose the declarations, per section 3.1.
Stage the staged experiments and, since the ablation, the plans with their entries and
the refs, per section 3.5. Models the artifact catalog, per section 3.6. The Spec's
section 2 gives them their rows and its section 3.1 keeps the ingest away from all of
them, a position and a run being the ingest's alone.

**Naming the second half is not a widening of scope.** Three surfaces have
listed those objects since this charter was rewritten, and a surface that
lists what nothing stores is a surface drawn against nothing. What the
distinction buys is that the two halves cannot be written by one path: an
authored row edited under a replay's idempotence would silently lose an
edit, and a recorded row written by a screen would stop being a record.

**An authored row is someone's work and the record says whose, or says it
could not be named.** The recorded half is about an instrument, so it names
a run and no person. The authored half is about a person, so attribution is
a member of the row rather than an annotation on it, and the Spec's section
2 gives it the member and its shape. **It is charter rather than schema
because it cannot be added later**: who authored a row is knowable while the
row is being written and unknowable afterward, so a record that omits
attribution has answered the question rather than deferred it. What an
attribution is worth before the identity act of section 6 is that act's to
change, and the member does not wait on it.

The front end holds the trial registry, on its own store. The grain is the
grain the interface clicks at: **one row per position per run, addressed by
the run, the turn, and the position**. All three are needed because turn
keys repeat across a serving record's runs, and because a position is the
resident length at the draw rather than an ordinal within its turn.

Each position carries the emitted token by identifier and surface text, the
surprisal and entropy there, and the ranked alternatives with their
probability mass. The alternative count is a declared capture parameter
rather than a fixed number, because the number one wishes had been kept is
discovered after the run. Raw residual rides alongside rather than a
projected readout, so a future refitted lens can read an old run.

**Everything identifying the conditions lives in the run's own row**:
**the record identity** at a grain fine enough to catch a quantization
difference, the weights hash the SPU computed at admit per
`weaver-spu-Spec` section 3 and the analysis seam carries per generation,
**the engine's identity at a grain fine enough to catch a library
revision**, seed, the full sampler configuration, device, **the field election's
depth**, the task, the declared boundary set, and **whether a token was forced and
which**. **Precision and the batching election left it on 2026-09-09**, per the
operator's rulings at issue #532: neither had a producer anywhere in the corpus, the
record identity above is already at a grain fine enough to catch a quantization
difference, and a batching election records a choice the architecture refuses. The
Spec's section 2.3 carries precision as the artifact's label. The engine is in
the compound the tuple names, and a divergence between two runs differing in
both silicon and library revision names neither cause unless both are held.

**The field election's depth is in the tuple because the projection below
compares what it decides.** A run keeping forty alternatives and one keeping
fifty differ in the length of every position's list by declaration rather
than by behaviour, and a comparison that held the rest of the tuple would
report that as a divergence. Forcing is in for the same reason under the
rule of section 3.5: a forced token is not a sampled one, so two rows
differing in it are not two readings of one condition.

**The row holds more than the tuple, and the difference is five members none of which is
a condition.** The parent run reference and the branch position, which are lineage, say
where a run came from rather than what it ran under. The emission's signature says what
the run produced. The record's session and digest say which record the row descends
from. The seated prefix's length says where on the tape the run's own doing begins, per
the Spec's section 5. All five are the row's and not the compound's, so two rows are
comparable across them, which is what makes a branch measurable against its parent at
all, and the Spec's section 2.2 carries each with its ground.

The write path is a consumer rather than a step in the loop, and **the
decoder never waits on a database**. The read path is what the schema is
for, and nothing is computed at read time: a value computed in the interface
is a value nobody else can reproduce.

**Reproduction is a measurement this record supports, and its projection is
stated here.** Two rows align on turn and position, which are the address
rather than the subject, and compare on the emitted token, the surprisal,
the entropy, and the ranked alternatives with their mass. **Byte equality
is claimed only where the tuple is held.** Where any member of the tuple
differs the result is a divergence report naming which member differs and
never a verdict, because a decode under a different engine build is a
different compound and equality was not the question asked of it.

## 5. Placement, and the boundary rule

Composition includes placement. Each declared component has its own socket
and its own surface, so each can be placed independently, and moving one
while holding the rest still is the loosening the instrument is for. **This
crate offers a ladder of presets**, whose shape section 9 holds open.

**A preset stages a runnable declaration the operator can then edit by
hand**, and placement is one axis of that mechanic rather than the whole of
it. The other axis is **qualification: establishing what a reference is
worth on weights nobody has measured** - a quantization, a fine-tune, a
merge, a different conversion. The trial is the same shape in each case: the
family's reference task, the artifact moved, the control compared.

**Post-training is a first-class use of that axis** and needs no machinery
of its own. What a fine-tune changed becomes a trial with its tuple that a
reviewer reruns, rather than a claim about a training run.

**Whether the lens is reused or refitted is the operator's election, and
this crate records which was made.** The lens identity is
`weaver-analysis-PRD` section 3's and is not restated here: the weights by
content hash, recomputed against the model in hand, and a reader refuses a
lens whose manifest names other weights. That refusal is the refit's
trigger, and it fires on a conversion as readily as on a fine-tune, because
a quantization is different bytes.

**Reuse across a conversion is therefore an election and not an
inference.** The operator names the fitted weights to the reader, which is
exactly what was done at the 8B: GGUF columns read through a
safetensors-fitted lens scored a fraction above the torch reference on the
same tokens at both bf16 and q8_0. **That measurement shows the projection
survives the encoding. It does not show a mechanism**, because the reader
was handed the safetensors and never saw the GGUF's identity. The two cases
differ in what the election is worth: post-trained weights are a different
function and a reuse there is unsound, while a quantization is the same
trained weights in a lossy encoding and the measurement above is the ground
for electing reuse.

**This surface's job is to make the election visible rather than to make
it.** A reading taken through a lens fitted to other weights carries that
fact beside it, so a reader downstream sees an elected reuse rather than a
fit. Whether the corpus should carry a trained-weights lineage that would
let the tool answer this instead is section 9's, and it is the analysis
charter's to state if it is ever stated.

**A refit is a new artifact, and two readings through two lenses are not
directly comparable.** Base against fine-tune compares two models through
two lenses, so what carries across is the control and the trajectory's
shape rather than a rank. A surface that placed those ranks side by side
would invite the comparison the refit makes invalid.

**This axis is why an unmeasured reading is offered rather than withheld.**
It is not this crate's burden to have measured every artifact in advance,
it is this crate's job to make measuring one cheap. Where a reading has no
control on the artifact in hand, the surface says so and offers the trial
that takes one, because a caveat leaves the operator holding a doubt and a
preset hands them the answer.

The placement axis is a ladder of three rungs.

```text
rung one    everything local, Unix sockets only
rung two    everything local, loopback network
rung three  any service off the host
```

**Every service reachable other than by kernel-enforced peer identity
carries a declared boundary.** The socket rung is exempt because the kernel
is the boundary. Loopback is not exempt, because a bound port is a bound
port. This crate refuses to load a declaration with an undeclared boundary
on a reachable service, and that is a load refusal rather than a warning.

The rule sets no minimum on what the boundary is. A username and password
over a public address is a legitimate declaration and this crate holds no
opinion about it, because refusing it would mean the instrument cannot
measure the arrangement most people ship. **The mechanic is the requirement
to declare**, and it is a completeness requirement on the record rather than
a security feature: whatever is declared becomes part of the trial's
identity, so a reading can never be quoted without saying what was holding
the boundary when it was taken.

**This crate's own crossing is a declared boundary like any other.** Nothing
in the read or write path requires the viewer, the store, or the queue to
sit on the machine that runs the agents. The reader is a store client and
the runner is a queue consumer, so the front end and its store run on one
machine while the agents run on another, and that crossing appears in the
trial record.

**What stands on the agents' machine is this crate's own two connectors**,
gate-con on the data plane and admin-con on the management plane, per the
design session of 2026-09-30 and the operator's ruling of 2026-10-01. They
are the one party that reaches an agent, each a client of the server's
listener, and the server never reaches one. The link between them and the
server is the Spec's section 8, and the register of agents it rests on is the
Spec's section 2.12. The crossing is this crate's at both ends, which is what
lets it be declared whole.

## 6. Identity and roles

Roles exist and are structural, so the identity act attaches authentication
to standing roles rather than a rearchitecture. **Co-locating a reading with
an exchange is a presentation ruling and spends neither the role separation
nor the gate**: the reach stays gated where it was.

Identity, authentication, and transport encryption are deferred with named
triggers, and roles are not among the deferrals and are not access control
either.

**The act is the IAM act, and this section names it because the rewrite
dropped the name.** The prior Spec's section 14 named it plainly and stated
the caveat it still carries: until the act, a session is an anonymous
server-minted cookie whose only job is continuity, so a name from the
config's admin list is an admin session with no proof. Issue #336 is the
register and carries the interim path for the case where the act is far off.

**Two triggers, and either is enough.** Identity and authentication are due
when the listener becomes reachable by anyone the operator has not already
admitted, which is when a claimed name stops being continuity and becomes an
assertion. **Admission is by placement**: the operator admits a network when
the listener is put on it, so the trigger is the listener becoming reachable
from beyond that network and not the fact that a second person shares it.
Without that clause the trigger reads as met on the day it was written,
because the caveat above says anyone on the network can claim an admin name.

Transport encryption is due when the traffic leaves the network the listener
was placed on. **The two are close and they are not one event.** Identity
fails when someone claims a name that is not theirs, encryption when someone
reads traffic that is not theirs, and the same placement can meet one
without the other. Naming a single trigger would leave the second deferral
standing against nothing, which is the state this section was in until
2026-09-07. **The connectors' link is not under either deferral.** It
crosses machines by construction and carries the agent's record, so it is
mutually authenticated and encrypted from its first act, per the Spec's
section 8; the deferrals above are the browser's listener's and no other's.

**The act changes the proof and not the gate.** Roles are structural and the
reach stays gated where it was, so the act makes a session prove who it is
and moves nothing else, which is what attaching
authentication to standing roles means above. **One member does not wait on
the act**: the Spec's section 3.2 gives every authored row a nullable
author, an author being knowable while a row is written and unknowable
afterward, so the member is written now and the act decides what fills it.

## 7. What the rewrite keeps, and what it retires

**The individuated-agent direction is not retired here.** The archived charter this
text replaces, the one filed 2026-08-19 and amended through 2026-08-25, the referent
everywhere this document says "the archived charter", described a web surface over
the suite's agents serving a user who converses and an operator who drives lifecycle
and reads the record, which was written for an interface to an individuated agent.
That remains the program's research direction, held open rather than closed by
anything this section retires, and it is not what this crate is for.

**Nothing survives because the prior charter had it.** The crate is the
same crate and its code is where it was, so no boundary is being crossed
here - what is being decided is what this charter still charters. Each item
is ruled on its own:

- **The connector shape and its link** - the box-bound reaches held by a
  client beside the agent, the presentation stack by a server elsewhere.
  Carried, and recut on 2026-10-01: two connectors rather than one,
  gate-con and admin-con, each this crate's binary and each a client of the
  server, under the Spec's section 8, which replaced the one dialed link
  between two colocated processes this bullet carried before. The placement
  ruling of section 5 makes it the shape rather than a mode.
- **The trace tail** - the record follow, the per-agent rings, and loss and
  discontinuity marks rendered as first-class objects. Carried: the
  completeness claim of a view is exactly the stream's.
- **The field-by-field comparison** of the confirm view. Carried, and
  generalized by Measure in section 3.3.
- **The lifecycle verbs** and their verbatim refusal rendering. Carried.
- **The multi-party channel**, with upstream models as guests. **Retired
  from this charter.** The Live surface of section 3.2 is a different
  construct that resembles it. A channel belongs to the individuated-agent
  direction this section opens on, so what is retired is this charter's
  cover of it and never the direction itself. The built channel code remains
  in the crate and is unchartered until a later act rules on it, which is a
  state this document names rather than resolving.

## 8. Asks upstream

Each is filed as its own issue against the seat that owns the apparatus, and
this crate designs around the gap until it closes.

- **The observation exchange**, #435, landed 2026-09-04 at #440. Load state
  is answered by the harness's own word rather than inferred from a socket's
  existence. The archived charter's sentence naming this an inference is
  retired by that act.
- **The position's alternatives**, #436, landed 2026-09-05 at #444. They
  were never missing: `model.field` carries the ranked candidates and their
  mass at every generated position under the declaration's field election,
  and the act added the per-position read. **The election's depth is the
  operator's ruling** and is open, section 9.
- **The component catalogue**, #437, answered by a finding rather than a
  catalogue on 2026-09-05, and the finding is adopted above: the charters
  hold the roster, `validate` is the composition oracle, and this crate
  carries the declaration's field shape written against `weaver-types-Spec`
  section 2 **at a named corpus commit, that commit being its staleness
  rule**. What remains worth an act is narrow - a describing invocation
  answering the family registry and the per-family knob names, which the
  crates hold and do not publish, so that a control labelled with a knob's
  name is labelled from the framework rather than from a copy here.
- **The declared boundary and its load refusal**, #438. Without it section
  5's ladder has rungs that mean nothing.
- **A session stands from a record**, #432. It is section 3.5 entire.
- **The queue runner and forced decoding**, #442.

## 9. Open cells, each named rather than implied

- **The name and the placement of these papers**, decisions one and two of
  #439.
- **Whether this crate scores a correctness verdict.** Reproduction is not
  in this list: it is measurement, it is in scope and drawn, and section 4
  states its projection. **Correctness is testimony** and needs a scorer.
  Section 1's discipline forbids this crate from scoring correctness, so
  what stays open is which of three answers that takes. This crate does not
  score and records the emission for an external scorer. Or it records a
  scorer's verdict with **the scorer named on the verdict and never in the
  run's
  tuple**, so the judgment is attributable to a named thing rather than to
  the instrument, and a second scorer applied to the same run adds a
  verdict rather than changing what the run was or duplicating its row.
  Or scoring lives in the benchmark harness and this crate cites it. **The
  two are separate columns and the correctness one stays unfilled until a
  scorer is named**, because a column that mixes them would let a
  reproduction verdict be read as a verdict on the answer.
- **The word "cell", closed 2026-09-09.** This bullet read it as two senses, a matrix
  coordinate and a declaration plus a task plus a run, and there were three. The third
  is the corpus's own **cell**, a named open question, which this section's own heading
  uses and which stands at a hundred and fifty uses across thirty documents, counting
  whole-word cell and cells over the markdown of `docs` and `process` outside the frozen
  archive and outside the four files this act sweeps, in every crate. On the operator's
  ruling of that date the count settles it. **A cell is a named open question**,
  unchanged everywhere. **A matrix coordinate is an entry**, per the sketch at
  `docs/project/sketch-ablation-matrix.md`. **A declaration plus a task plus a run is a
  trial**, which this document now says, the word being free in the corpus and sitting
  beside the experiment that authors it and the run that executes it. The
  matrix-coordinate sense also stands in code and in deposits, the driver
  `confirm_cells.py` by name and a `cells` key the configs and every deposit written
  against them carry. **Those are weighed and left**: a filename and a stored key are
  not prose, and renaming them would strand the deposits that already spell it.
- **The field election's depth**, which sets how many alternatives a
  position keeps. The records on hand carry forty and fifty. It is the
  operator's ruling and the number one wishes had been kept is discovered
  after the run.
- **Whether the describing invocation of section 8 is an admin verb or an
  invocation of the SPU binary**, each a Spec question for its own crate.
- **Whether surprisal and entropy draw as one timeline or two.**
- **Whether the preset ladder is a picker or a wizard.**
- **The registry's schema**, which is `weaver-web-Spec` section 3's and
  not this document's.
- **How presence on another box is answered.** Section 3.6 records which
  box holds an artifact as a dated observation, and who reports it is
  unchosen. Three answers stand. An operator registers it, which is a
  claim. Admin answers it on the observation exchange's pattern, which
  #457's rule argues against, admin adjudicating only what it holds custody
  of and the artifact being the organ's. Or **the SPU hashes the weights at
  admit and the record carries them**, making presence derivable from
  recorded admits, a fact rather than a claim, with "never admitted here"
  the honest reading of an artifact no box has loaded. The third fits this
  crate's measure-rather-than-testify rule and is the seat's
  recommendation, and the choice is the operator's.
- **Whether import fetches.** Section 3.6 records provenance by repository
  and revision, which is what a fetch would need, and this crate has no
  ruling that it may reach the network. Until one lands, import records an
  artifact already on a box.
- **Whether a trained-weights lineage should exist at all.** Section 5
  makes reuse across a conversion an operator election because
  `weaver-analysis-PRD` section 3 versions a lens by the weights content
  hash. A lineage would let the tool answer instead. It would be that
  charter's fact to state and its Spec's record to change, so this cell is
  named here and owned there.
