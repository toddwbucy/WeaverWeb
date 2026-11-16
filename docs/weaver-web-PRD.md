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

**Placement is held.** The deployment topology is ruled: this crate's server
and its store run on one machine, the agents on others, each with this crate's
two connectors beside it, and the crossings are store traffic, a queue, and since
2026-10-01 the connectors' link of the Spec's section 8. Whether these papers eventually move to a
separate corpus follows from that but is not identical to it, and stays
decision two of #439.

**Date filed:** 2026-09-04
**Document ID:** `weaver-web-PRD`
**Parent:** the WeaverTools suite, per `weaver-agent-PRD` section 0. **The apex is
in another repository since 2026-09-26**: `weaver-agent-PRD` lives in
`toddwbucy/WeaverAgent` since the suite split of 2026-09-30, and before that in
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
- **Not a product's front end.** Weaver Web is management-plane software in the sense
  that Proxmox's console is, and it never faces the public, on the operator's ruling of
  2026-11-15. The agents it manages may be production agents.
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
- **Not a hypervisor driver.** Weaver Web never touches a hypervisor, on the operator's
  ruling of 2026-11-14. It does not create, clone, start, snapshot, network or destroy a
  virtual machine, and it stops none through a hypervisor. Those acts belong to the
  operator and the operator's hypervisor, and the suite holds no opinion about them. To
  start and stop an agent is to load and unload it through admin, inside a running
  appliance (section 10.5). **The one stop of a machine the console offers is its
  shutdown command, enabled on a virtual machine and disabled entirely on a host**, on
  the operator's rulings of 2026-11-16 (setup pages). The setup flow's checkbox says
  which the machine is (section 3.0). The command reaches no hypervisor, which the
  ruling of 2026-11-14 keeps, and its path into the machine is an ask upstream (section
  8).
- **Not a knowledge-graph governance layer.** No axioms, smells, gates, or
  conformance as product features.

## 3. The surfaces

Ten, in two groups. **Every surface has a direction**: you enter somewhere,
you end somewhere, and the screen says where you are on that path. The
failure to avoid is the interface that exposes its graph before it shows the
artifact, so that nothing on screen says what you came to produce. Each
surface below states its destination in its own header, and the stage rail
under the masthead carries the whole path on every page about an agent.

Clean does not mean barren. A screen with nothing on it is not clean, it is
empty. Clean means the next step is obvious and the numbers appear when
asked for.

**Each surface below is drawn.** The figures are the design canvas of
2026-09-04, authored from the thinkpad seat against measurements taken on
that box, and they are a reading aid rather than an authority: where a
figure and this text disagree, this text governs and the figure is owed a
redraw.

### 3.0 The lifecycle frame

The design session of 2026-11-06, the design seat working with the operator and the
architecture seat, framed this crate's front end as **the agent's whole lifecycle**:
create it, deploy it, run it, watch it, preserve it, diagnose it, train it, retire it.
The ten surfaces below are read through that frame, and the path the stage rail carries
under the masthead, on every page about an agent, is the frame's own: eight stages in order, each a mark on a stage rail, the
current one underlined, with what is left said at the rail's end. The frame binds the
surfaces; the surfaces' own text below is unchanged where it does not conflict.

**The first principle is that the unit is the agent, not the model.** A local developer
arrives from LM Studio, Ollama and vLLM, where one picks a model, loads it, gets an
endpoint and chats. None of that holds here. An agent is a set of processes under a
kernel-enforced user identity; it is declared, provisioned and loaded, and its trace is
the artifact. The model is one field inside the declaration. The design teaches the
difference rather than hiding it: the agent is named first everywhere, a verb is a
verb and a state is a state and each is set as an identifier, and there is no
endpoint, since the gate and admin are Unix sockets on the agent's edge and the network
belongs to the two connectors outside it and to nothing else.

**The standing positions of this charter hold on every surface**, and the design gives
each a visible mechanism: this crate is a consumer outside the agent's boundary, so a
pane that shows the agent names in its source line the plane, the connector and the
verb it heard through; the browser is a display engine holding no key and no protocol
state, so nothing is computed in the page that the server did not send; answers render
as they came, a verb's answer being admin's JSON verbatim and a state word the agent's
own, with a friendlier shape beside it and never instead of it; a gap in a record
surfaces as a mark, never smoothed; and a missing contract is an ask upward, never a
workaround, so a pane whose backing is not built says so and names what it waits on.

**The eight stages, in order, with what backs each on 2026-11-07.** Multi-agent
management comes later and is built from the same pieces, so nothing here assumes one
agent in a way that blocks many.

| stage | what it holds | backing |
| --- | --- | --- |
| 1 Declare | a start page, the setup pages and the build page (the operator's rulings of 2026-11-16, setup pages, and their addendum): the start page asks the agent's name; the setup pages run, in order, the pairing, the models, the drives as the box mounted them, the placement and the confirmed deployment tuple; the build page holds the board, which wires what was declared, and the prompt tuple panel | partly: the declaration screen of 2026-11-11 saves versioned declarations and shows the file each will become (Spec 6.1), and stands until the setup pages' act recuts it; the start page and the setup pages are unbuilt; the build page is held, the board waiting on contracts WeaverAgent has yet to write |
| 2 Provision | the box's side of the setup (the operator's rulings of 2026-11-16, setup pages, and their addendum): an appliance's console goes straight into the appliance's setup script at first boot, which asks this server's address and port; the box dials this server over TLS and shows a short one-time code and this server's fingerprint; on the pairing page the operator compares the fingerprint and enters the code; the connectors send the public halves of keys made in the box, this server signs them, and mutual TLS comes up; the survey's report crosses over TLS. The pairing is the connectors' and serves a host too. Once the model is picked, this server shows a command to run at the appliance's console, which starts the script's drive setup, formatting and mounting the state drive and the models drive, each after the operator confirms the disk; the box then pulls the model from this server's cache over TLS. The drive setup is the appliance's script alone, and a host's own script is later. A host install points at a state directory and a models directory and attaches and formats nothing. Every root act runs on the box at a screen the operator is sitting at, and this server's reach is the two connectors from pairing onward (section 10). Installing the declaration Declare saved is the operator's work on the box as built, and no console verb does it. Then `validate`, which reads the installed declaration. Registration and its one-time config handover stand as built | partly: the register page and the one-time config handover are built, and an appliance's enrollment, the console's side of the pairing and the drive setup, the installing of the declaration and the validate step are not |
| 3 Run | load, serve turns through the gate, unload | built; `load` is offered as every verb is, where the person's grants and the connection's ceiling carry it |
| 4 Observe | availability, alerts, the live trace | built |
| 5 Preserve | `save-point`, `restore`, the decision on a missed save point | partly: both verbs are offered where a role carries them and the ceiling grants them; no list of save points is served, and `restore` carries no argument (the invoker's fixed line `sudo -n <weaver_admin> <verb> <agent>`, Spec 7.2), so the page names no save point |
| 6 Diagnose | reload under a diagnostic binding with the gate shut (WeaverAgent's `diagnostic-replay-loop` section 1: under a diagnostic binding there is no gate; `weaver-diagnostic-PRD` section 3), the analysis feed, stepped replay, counterfactual branches | unbuilt; waits on weaver-analysis |
| 7 Train | select from the trace, build a dataset, train an adapter, hand it back to Declare; gathering is part of the stage | unbuilt; training runs in WeaverTrain (`toddwbucy/WeaverTrain`), reached through a contract that does not exist yet |
| 8 Retire | revoke planes, retire the row, archive | partly: revoke and retire are built; archive is not |

**The trace is the record of the whole lifecycle, not a stage** (the operator's ruling of
2026-11-07, #54): Observe reads it live, Diagnose and Train read it landed, and no stage
owns it. Train draws its selection from it, builds the dataset from the selection, and no
frontier-model or reviewer text enters a dataset. The adapter Train hands back is a piece
of the declaration like any other: it enters through Declare and Provision at the load
boundary, never by a path of its own into a running agent.
**Where the operator sends the trace anywhere but a file on the agent's machine, the
console holds no trace for that agent**, on the operator's ruling of 2026-11-15 (section
10.6). The trace door then has nothing to serve, so Record, the live view and Train have
nothing to read for that agent, and the console says that it holds no trace for that
agent. Whatever keeps the trace past its destination is the operator's, and the console
states nothing about it. That is the operator's choice.

The ten surfaces by stage. On the path: Compose is stage 1, Live is stages 3 and 4, and
Measure, Open a trace and Stage are stage 6. The five on the list side feed a stage
rather than being a stage's page: Agents feeds stages 2 and 8, Models is the artifact
side of stage 2, and Experiments and Record feed stage 6, as does the Experiment view,
the detail surface Experiments opens into.

**One visual language and one layout grammar**, for every later surface: agents, state,
analysis, chat, hardware. Every surface is composed from panes in a grid, never from
detached windows. A pane's header carries its stage mark, its title, a backing badge
(built, partly, unbuilt) and its source line; its footer says how it stays current or
what it waits on. A mark is a word in a pill and never a bare color: absence (`absent`,
`not served`, `no state`, `none`), status (`connected`, `in transition`, `revoked`, each
beside its dot and word), backing, and the hatched discontinuity row in a stream. A
verb that does not apply is disabled with its reason, never hidden. Every pane is marked
with its stage and whether it has backing today, so a drawing is never mistaken for a
surface. **Every pane carries its labels**, on the operator's rulings of 2026-11-16
(setup pages): its stage, built or unbuilt, and its status, healthy states included,
since a sign that things are good is as wanted as a sign that something broke. The
served pages are composed in this grammar, and the stylesheet they load is
the design system's own (Spec section 6).

**Declare is a start page, the setup pages and the build page**, on the operator's
rulings of 2026-11-16 (setup pages) and their addendum, which replace the two screens of
the rulings of 2026-11-11 as the stage's path, those having replaced the single diagram
the design session of 2026-11-06 drew. The act that builds the pages ends at the
confirmed deployment tuple of the confirm page, and the declaration screen of 2026-11-11
stands as built (Spec 6.1) until that act recuts it.

- **The start page** asks the agent's name, one field after Start, because the appliance
  names its directories and admin entry after the agent. Once an agent exists, the start
  page becomes a New agent button.
- **The pairing.** The page shows the box waiting, which dialed this server from its
  setup, and the operator compares the fingerprint the box shows with this server's own
  and enters the short one-time code the box shows (section 10.1). Then the running
  report of what the box's survey sent over TLS. A checkbox says whether the box is a
  virtual machine or a host, with a note on what the checkbox does: on a virtual machine
  the console's shutdown command is enabled, and on a host it is disabled entirely
  (section 2). Where the address is localhost and the checkbox says virtual machine, the
  page warns that the console could lose access to itself.
- **The models.** A Hugging Face repository, then a pick from its files, a split model
  shown as one choice with all its parts, and the precision read from the file (section
  10.4). What is built is what the first agent needs: one decoder and the one adapter on
  it. Extra models, adapter roles beyond the one decoder adapter, a classifier-only
  agent and placement on the CPU are drawn and marked unbuilt, never live, so a page
  never accepts what a load refuses.
- **The drives**, as the box mounted them. Once the model is picked, the page shows the
  command to run at the appliance's console, which starts the appliance's drive setup
  (section 10.4), then the state drive, required for every agent at a minimum of 10 GB,
  and the models drive, its size computed from the chosen files, as the box mounted
  them. On a host, which attaches and formats nothing, the page shows the state
  directory and the models directory the operator named, with free space against the
  need, the models directory prefilled with that user's default Hugging Face cache so
  that models already pulled there are found, on the addendum, and the host form is
  drawn and marked unbuilt, since the host's own script is later. One unit, GiB, serves
  the whole flow.
- **The placement.** Each model on a device, and the context length chosen against the
  model's maximum, read from the file's header. The fit is called planned, from the room
  plan without a load, since only a load measures. The first agent's Granite is a dense
  transformer, so the room plan's KV cache term applies. "In shard order" names the
  layer split across cards and says so.
- **Confirm.** The deployment tuple in full: the model's repository, commit and per-part
  digests, the adapter's likewise, the image's version, the driver, the cards by
  identity, the guest processor, the machine, the drives, the placement and the context
  length. Confirm goes to the build page.
- **The build page** holds the board and the prompt tuple panel. Both are held, and the
  page is a one-pane stub marked unbuilt. The six sampling values live in the prompt
  tuple, so a first agent declared and set up through Confirm cannot load until the
  panel comes, since a load refuses without them.
- **The declaration screen of 2026-11-11**, as built: the operator declares the
  components, which models on which GPUs, which store, which gate, and saves the
  declaration as a versioned document, read back as the file it will become. One SPU box
  stands for the models it holds, four models declared apart being four boxes and one
  decoder with its adapter one box, spread out or collapsed as the operator chooses. The
  engine follows from the model's format, GGUF offered first and the other formats named
  as later. A GPU is declared as a requirement, not as a device: a specific card, a
  class, given as a family (any card of one architecture) or as a list of models, or a
  pair of two requirements in shard order. The screen warns, and does not refuse, where
  a class spans more than one compute capability. Start satisfies the requirement or
  refuses, and never substitutes, and the card drawn is stamped on every load event. The
  system prompt is a field: the declaration act writes it into whatever state was
  declared, from then on it is read from state and never from a file, and it is locked
  for the session. The sampler's mechanism is fixed at load, and its knobs are per-turn,
  the declaration supplying the defaults, the turn call the values and the record each
  event's: the test for any engine setting is whether changing it invalidates the cache,
  fixed at load if it does and a per-turn knob if not. Since 2026-11-12 the screen
  requires every tunable value a serving load requires at its pin, eight: the six
  sampler defaults and the session's two parameters, the maximum tokens per turn and the
  context capacity (Spec 6.1).
- **The board** is held on the build page: the operator wires what was declared, and the
  board shows only that. A component is a compiled primitive the harness is
  pre-wired to, a wire is one ask and one return written in Python by the operator, and
  a function that makes several asks is a node the wires converge on. It waits on
  contracts WeaverAgent has yet to write.
- **An agent is binaries by hash plus wires by hash**, and the setup pages and the board
  are a rendering of that card: a wire edit is a new link in the lineage, not a reload
  of the same agent. **The trace is the record, not a component**, so no page of Declare
  declares a component for it. **Its destination is a field of the declaration page, set
  when the agent is created**, on the operator's ruling of 2026-11-15, which amends the
  rulings of 2026-11-11 for the destination alone (section 10.6).
- The agent's edge, its two sockets and the two connectors outside it are not on the
  pages of Declare. The connectors are minted at register in Provision, or for an
  appliance signed at its enrollment (section 10), and the network belongs to them
  alone, as every other surface keeps saying.
- Only pieces that exist today are offered, section 3.1's roster rule applied: a kind is
  a charter act before it is a row.

**`validate` closes Provision and gates Run** (the operator's ruling of 2026-11-06). It
asks admin, through admin-con, whether every element the installed declaration names
that admin holds custody of is on the box, so it has nothing to ask before the agent is
provisioned. Declare keeps only what its form can refuse on its own: a missing required
field, a requirement that names nothing, and the shape of each field (Spec 6.1). A clean
`validate` is acceptance for filing: nothing admin holds custody of stands in the way of
a load, and what only the organ can judge is answered at admission. It is not a promise the load succeeds. Section 3.1's
sentence that Compose "asks `validate`" is amended below to say where the verb is asked.

**The palette** (the operator's rulings of 2026-11-06). The purple of
`assets/surfaces/instrument.css` was rejected as a default, not a ruling. From the
proposals that followed, six themes were chosen: Slate as the default, Graphite and
Umber as the other darks, and Chalk, Mist and Paper as the lights. Each keeps the same
roles (bg, panel, inset, line, fg, dim, faint, accent, ok, hold, fail), the same
contrast floors (4.5:1 for text on bg and panel, 3:1 for faint and for the accent as a
ring, measured for every pair in every theme), and hue reserved for meaning: the accent
is the one action hue, green, amber and red belong to status and always stand beside
their word, and emphasis is weight and luminance. The hex values and measurements are
in `docs/design/system/palettes.md`, which also carries the six themes in the shape
`instrument.css` carries them. A browser preferring
light gets Mist under the default, Slate's partner, both blue-biased neutrals.

**The record.** The design system (tokens, the brand book, the pane grammar, the stage
rail, the marks, the controls, and the Declare and Observe proofs) is kept under
`docs/design/system/`, its README the brand book and `tokens.json` the six themes. The
working copies are two private artifacts, the system itself and the palette comparison,
named in `docs/design/README.md`. Where a file there and this text disagree, this text
governs.

### 3.1 Compose, ends with a runnable declaration

You cannot load an agent you have not created, so the first surface authors
one. The artifact is the declaration, presented as a composition surface
rather than as a text file, and saved as a versioned document of its own
whose every version is kept. **The file it will become is always shown
beside it**, derived from the saved document at a pinned commit of the
agent's grammar, so the text is always reachable without being a second
place to edit.

The declaration is a roster plus a wiring, **declared on the setup pages and the
board**, per section 3.0 and the operator's rulings of 2026-11-16 (setup pages): the
roster on the setup pages, each model picked from its Hugging Face repository and
placed on a device, and the wiring on the board, where the loop is real topology the
operator draws. The board is held. As built, the roster is declared on the declaration
screen of the rulings of 2026-11-11, each component resolving its artifact and setting
its elections.

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
either.** It writes the draft, refusing on its own only a missing required field,
a requirement that names nothing and a field out of its shape (Spec 6.1), and
`validate` is asked at the end of
Provision once the agent stands on a box (section 3.0, the ruling of
2026-11-06). The verb transitions nothing, refuses an incoherent declaration
naming the field, and reaches the box facts admin holds custody of. The refusal is
that verb at the end of Provision, and it cannot drift because it is **the contract
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
requires a reload under the diagnostic binding (`weaver-diagnostic-PRD` section 3) and
captures at every
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

**Models** holds the artifacts, which is the catalog two surfaces above already assume.
Section 3.1's roster resolves each component's artifact and section 5's qualification
runs a reference against one, and neither says where the artifact came from or what is
known about it. This surface does: **the artifact's identity per file**, following the
model's own index and verifying each shard under its own name. Its provenance,
downloaded from a repository at a revision or converted from another artifact by a named
converter at a pin. Which box holds it, since this crate runs on one machine and the
agents on others. And its relations, the lens artifacts fitted to these weights, the
readings taken through a lens fitted to other weights where the operator elected that
reuse, and the reference trials taken against them.

**Models is the one surface on this side that authors, and its motion is
import.** The others list what other surfaces produced. Import registers an
artifact, which is what gives Compose something to resolve against and
qualification something to run a reference against, and **it ends with an
artifact other surfaces can name.**

**The console fetches models**, on the operator's ruling of 2026-11-14, from the four
sources section 10.4 names, and keeps them as the model store an appliance pulls from.
Whether that fetch is this surface's import, so a fetched model ends as an artifact
other surfaces can name, is not ruled, and section 9 holds it.

**An identity is computed and never accepted, and where it is computed depends on which
box holds the weights.** An operator who types a digest has told the catalog what they
believe, and a catalog that records a belief cannot later say two boxes hold one
artifact. **On this crate's own box the surface reads the files and derives the
identity**, following the artifact's own membership rule, which is an index where the
container carries one and a name pattern where it does not, per the Spec's section 2.3.
**On an agent's box it cannot read anything**, this crate running on one machine and the
agents on others, so an artifact there registers from what the record reports of it
rather than from a read, and until that path stands an entry for it is a presence
observation and not a registration. Which of the two this surface offers first is
section 9's open cell on presence, and the answer chosen there is the answer here.

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

**This crate's own crossing is a declared boundary like any other.** Nothing in the read
or write path requires the viewer, the store, or the queue to sit on a machine that runs
an agent. The reader is a store client and the runner is a queue consumer, so the front
end and its store run on one machine while the agents run on others, and that crossing
appears in the trial record.

**What stands on each agent's machine is this crate's own two connectors**, gate-con on
the data plane and admin-con on the management plane, per the design session of
2026-09-30 and the operator's ruling of 2026-10-01. They are the one party that reaches
an agent, each a client of the server's listener, and the server never reaches one. The
link between them and the server is the Spec's section 8, and the register of agents it
rests on is the Spec's section 2.12. The crossing is this crate's at both ends, which is
what lets it be declared whole. **On an appliance the two connectors ride its sealed
image**, and an appliance's enrollment, section 10, gives them their credentials.

## 6. Identity and roles

Roles exist and are structural, so the identity act attaches authentication
to standing roles rather than a rearchitecture. **Co-locating a reading with
an exchange is a presentation ruling and spends neither the role separation
nor the gate**: the reach stays gated where it was.

**For anything that can act on an agent the deferral is over**, per the
operator's ruling of 2026-10-02. A verb is a hand on a running agent rather
than a reading, and so is a turn, which prompts an agent that may act on its
box with its tools, so **no surface that asks a verb or places a turn ships
before the IAM act**,
and transport encryption on the browser's listener lands with that act,
since credentials will cross it. The Spec's section 2.13 charters what the
act builds: the authenticated person, roles and per-agent grants,
server-side authorization of every verb and turn, and an audit record of
each, the
first of the three gates its section 8 names. **Reading surfaces may stand before the
act**, and for them identity, authentication and transport encryption stay deferred with
the named triggers below. As built, the act has reached them: Record is read under a
person's session since 2026-10-08, and the reading surfaces since 2026-10-12 (the Spec's
section 2.8). The browser's pages that answer before a session exists are sign-in and a
person's enrollment (section 10.3).

**The act is the IAM act, and this section names it because the rewrite
dropped the name.** The prior Spec's section 14 named it plainly and stated
the caveat it still carries: until the act, a session is an anonymous
server-minted cookie whose only job is continuity, so a name from the
config's admin list is an admin session with no proof. Issue #336 is the
register and carries the interim path for the case where the act is far off.

**For reading surfaces, two triggers, and either is enough.** Identity and authentication are due
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
encrypted from its first act and mutually authenticated on every connection but
one, per the Spec's sections 8 and 8.2. The one is an appliance's enrollment
exchange, whose party proves itself with the one-time secret of section 10.1. The
deferrals above are the browser's listener's and no other's.

**The act changes the proof and adds the grant.** A session that proves who
it is attaches to standing roles, as this section promised, and the roles
become grants a person holds on each agent rather than a property of the
session, so a person may operate one agent and only observe another. That
is new structure, chartered at the Spec's section 2.13, and it is the only
structure the act adds: the reading surfaces' reach stays gated where it
was. **One member does not wait on
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
- **The storage report's carrier**, unfiled on 2026-11-15, on `toddwbucy/WeaverAgent`:
  how the WeaverAgent system's storage check at each load reaches admin-con and so the
  console, which is `weaver-admin-operator-contract`'s, so the console can say where an
  appliance's storage is not sufficient (section 10.4).
- **The record of an upgrade**, unfiled and held on 2026-11-15, on
  `toddwbucy/WeaverAgent`: whether WeaverAgent's record of a load names the version that
  a restored state was made under (section 10.5). It was held while state on an
  appliance was later, on the operator's ruling of 2026-11-15. The operator's rulings of
  2026-11-16 (setup pages) bring the save point the unload writes into scope, and
  whether this ask is taken up with it is not ruled (section 9).
- **The trace's destination**, unfiled on 2026-11-15, on `toddwbucy/WeaverAgent`:
  whether a declaration may send the trace to `/dev/null` or to a destination outside
  the agent's territory, as the operator ruled (section 10.6), and what the trace door
  serves where the sink is not a file. The member is the grammar's `trace-sink`, a file,
  a pipe or a socket (`weaver-types-Spec` section 2). As merged, admin refuses a
  declaration whose sink's directory is not the agent's territory, whatever its kind
  (`weaver-admin-Spec` section 9), and `weaver-admin-operator-contract` says "Whatever
  its kind, the sink stands in the agent's territory, and only a file sink is served
  through the trace door". Its refusal of a load whose sink cannot be opened stands, and
  section 10.6 rests on it.
- **The port member**, unfiled on 2026-11-15, on `toddwbucy/WeaverAgent`: the member of
  the agent's grammar that names the ports a declaration opens (section 10.3).
- **The connector accounts on an appliance**, unfiled on 2026-11-15, on
  `toddwbucy/WeaverAgent`: the accounts each connector's key and config stand under on
  an appliance, which WeaverAgent's documents name and this repository never records
  (`weaver-web-Spec` section 10).
- **What tells a stopped service from a machine that is down**, unfiled on 2026-11-15,
  on `toddwbucy/WeaverAgent`: what an appliance exposes, past its two connectors, that
  lets the console tell the two apart (section 10.5).
- **The appliance's setup script**, unfiled on 2026-11-16, on `toddwbucy/WeaverTools`
  and `toddwbucy/WeaverAgent`, on the operator's rulings of 2026-11-16 (setup pages) and
  their addendum: the script an appliance's console goes straight into at first boot,
  run with root's authority, which asks the console's address and port, starts the
  connectors' pairing and shows the short one-time code and the console's fingerprint,
  and later, at the command the console shows, lists the drives and formats and mounts
  the state drive and the models drive after the operator confirms each disk. Beside it,
  the survey the image ships, run locally, with its report sent over TLS, the connectors
  started on every boot, and the box's two asks of the console over TLS, the chosen
  model's size and the model pulled from the cache, and the box taking the operator's
  SSH public key the console sends down the paired link (sections 10.1 and 10.4). The
  pairing itself is the connectors', this crate's (section 10.1). The console answers
  each over the link, and none has its other side as of 2026-11-16.
- **The models mount's name**, unfiled on 2026-11-16, on `toddwbucy/WeaverAgent` and
  `toddwbucy/WeaverTools`, on the same rulings: the plural name they give the models
  mount, which this repository does not record (section 10.7).
- **The state mount for every agent**, unfiled on 2026-11-16, on
  `toddwbucy/WeaverAgent` and `toddwbucy/WeaverTools`, on the same rulings: the state
  mount required for every agent, at a minimum of 10 GB (section 10.4).
- **The save point on the state disk**, unfiled on 2026-11-16, on
  `toddwbucy/WeaverAgent`, on the same rulings: the save point the unload writes, placed
  on the state mount, by the existing save point path and with no new persistent store
  (sections 9 and 10.5).
- **The shutdown command's path**, unfiled on 2026-11-16, on `toddwbucy/WeaverAgent`:
  how the console's shutdown command, which the same rulings enable on a virtual machine
  and disable on a host, reaches the machine through admin-con, since no verb of admin's
  shuts a machine down (section 2). The rulings' list of asks does not name it.

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
- **Whether a fetch is an import.** The operator ruled on 2026-11-14 that the console
  fetches models, and on 2026-11-16 (setup pages) that it pulls from Hugging Face alone
  and records each pull (section 10.4), so this crate reaches the network for them.
  Section 3.6 records provenance by repository and revision. Whether the console's pull
  is this surface's import, so a pulled model ends as an artifact other surfaces can
  name, is not ruled. A local source and the other sources are later, and named as
  later. Settled by the model cache's act, which brings it to the operator.
- **Whether a trained-weights lineage should exist at all.** Section 5
  makes reuse across a conversion an operator election because
  `weaver-analysis-PRD` section 3 versions a lens by the weights content
  hash. A lineage would let the tool answer instead. It would be that
  charter's fact to state and its Spec's record to change, so this cell is
  named here and owned there.
- **The one-time secret's lifetime and form.** Section 10.1 carries its three bounds and
  the order of ten minutes, and the operator's rulings of 2026-11-16 (setup pages) and
  their addendum have the box show it as a short code at its console, for the operator
  to enter on the console's pairing page. The rest is `weaver-web-Spec` section 10's,
  settled by the enrollment's code act.
- **What the run card is.** The term is the operator's, for a JSON file, defined in the
  operator's run card paper (v1.4), which is in no repository as of 2026-11-15. The
  operator's rulings of 2026-11-15 name it three times: it records the ports a
  declaration opens (section 10.3), the tuple as shipped (section 10.5), and the trace's
  destination (section 10.6). This charter coins no definition. What the console holds
  about a run as built is stated as fact in `weaver-web-Spec` section 10. The nearest
  standing use of "card" is section 3.0's, as a fact: an agent is binaries by hash plus
  wires by hash, and "the setup pages and the board are a rendering of that card".
  Closes when the
  operator's definition is placed in a repository.
- **The trace's destination against the declaration screens, closed 2026-11-15** by the
  operator: the destination is a field of the declaration page, set when the agent is
  created, and the trace stays the record and not a component (sections 3.0 and 10.6).
- **The trace's destination against the record, closed 2026-11-15** by the operator:
  where the destination is not a file on the agent's machine, the console holds no trace
  for that agent, so Record, the live view and Train have nothing to read for it, and
  the console says so. Whatever keeps the trace past its destination is the operator's,
  and the console states nothing about it. That is the operator's choice (sections 3.0
  and 10.6).
- **Declared ports against the connectors' network.** Section 10.3 carries the
  operator's ruling of 2026-11-15 that ports opened by a declaration belong to the
  operator's arrangement. Section 3.0 says the network belongs to the two connectors
  alone, which `weaver-web-Spec` section 8.3 cites, and section 5 says the connectors
  are the one party that reaches an agent. Nothing waits on it while no declaration can
  name a port (the next cell). Settled by the operator.
- **The port member.** As built on 2026-11-15, no declaration can name a port, since the
  declaration has no port member (`weaver-web-Spec` section 6.1). The member is the
  agent's grammar's, an ask upstream (section 8). Settled by WeaverAgent's grammar, and
  then by the act that adds the member to the declaration screen.
- **A trace path on the state mount for an agent that elects no state, closed
  2026-11-15** by the operator: an operator who wants an agent that keeps no state
  declares none, and a destination that cannot be opened refuses the load, admin's
  standing rule (section 10.6). The operator's rulings of 2026-11-16 (setup pages)
  require the state mount for every agent, so no agent stands without one (section
  10.4).
- **The Spark-class machine, later, and named as later**, on the operator's ruling of
  2026-11-15, which puts it out of scope until one is had. The setup flow serves a host
  as it serves a virtual machine, on the operator's rulings of 2026-11-16 (setup pages).
  On their addendum the pairing belongs with the connectors and serves a host, a
  Spark-class machine or the console's own machine, as it serves an appliance, and the
  drive setup is the appliance's script alone, a host's own script being later, and
  named as later. That supersedes the ruling of 2026-11-14 that kept the single-host
  install for the Spark class alone (section 3.0). Whether registration and its handover
  (`weaver-web-Spec` sections 8.1 and 8.2) stay beside the pairing for a host is not
  ruled.
- **State and save points on an appliance**, on the operator's rulings of 2026-11-16
  (setup pages), which supersede the ruling of 2026-11-15 that made both later. The
  first production agent declares a store and keeps the system prompt, and it keeps the
  save point the unload writes, on the state disk, which every agent mounts (section
  10.4). The first test exercises it: the prompt into state, load, unload, shut the
  virtual machine down, restart, reload, and the prompt is restored. Operator-managed
  save points stay out of scope, later and named as later. Whether the record of an
  upgrade, held while state on an appliance was later (section 8), is taken up with
  this is not ruled.
- **Trust on first use**, the hazard of section 10.1 while pairing ran over the
  operator's first SSH. The operator's rulings of 2026-11-16 (setup pages) and their
  addendum move the pairing to a short one-time code the box shows beside the
  fingerprint of the certificate it met, which the operator compares with the console's
  own before entering the code, and setup uses no SSH. Whether that closes the cell is
  the operator's to say, then one first boot of the sealed image paired through the
  pages shows it.
- **A host's directories and the separate-filesystem test**, opened 2026-11-16. The
  ruling of 2026-11-15 refuses a load whose required mount is not a separate filesystem
  (section 10.4), and the addendum to the operator's rulings of 2026-11-16 (setup pages)
  has a host install point at a state directory and a models directory. Whether a named
  directory must be a separate filesystem is the operator's.
- **The console as the model store.** The model count or fleet size at which its cache
  stops being tolerable. No measurement exists, because the fleet is one. Settled by a
  measurement on a fleet larger than one.
- **Where images are published for the console to find**, with their published hashes.
  Later, and named as later. Settled by the operator's ruling of the place, which the
  act that builds the image download needs.

## 10. The appliance, the console's side

**The appliance is one deployment profile for a Weaver agent**, on the operator's ruling
of 2026-11-15, beside the host install and the Spark-class machine. A profile is neither
a charter nor a Spec. It decides nothing about any component and binds no Spec, and what
it asks of a component lands in that component's charter. This section is where the
console's side lands. **Every requirement here is an operator's ruling, cited by its
date**: the rulings of 2026-11-14 on the appliance, the operator's later rulings of that
day on how a document names the agent's bound and the deployment shape and on the
one-time secret on the pairing line, the rulings of 2026-11-15, and the operator's
rulings of 2026-11-16 (setup pages) and their addendum, which move the pairing to a
short one-time code the box shows. The setup flow serves a virtual machine and a host
alike, on the rulings of 2026-11-16. On their addendum the pairing belongs with the
connectors and serves a host too, and the drive setup is the appliance's script alone.
An appliance is the case this section works through, and what it says of pairing holds
for a host. The Spark-class machine is later, on the operator's ruling of 2026-11-15
(section 9). The section adds no ruling of its own. A question the rulings leave open is
named in section 9, or in `weaver-web-Spec` section 10 where it is the Spec's.

**The stance**, on the operator's ruling of 2026-11-15. The suite ships a tuple, and
each operator produces tuples of the operator's own. The image is disposable and carries
what runs, and the mounts carry what the agent keeps. Whatever an operator adds or
changes after boot is the operator's to account for, and the suite offers no guarantee
about a changed machine. Inbound is the suite's, and outbound is the operator's. The
suite builds the virtual machine and the service as simply as will work, and anything
that adds machinery for a later need is a later item.

**Five terms carry one meaning each in this section.**

- The **image** is the sealed disk image an appliance boots. One image exists for each
  version, and it is published with its hash.
- An **appliance** is a virtual machine an operator made from the image.
- The **console** is the Weaver Web server, the operator's management surface for many
  agents.
- The **agent** is the bounded thing, the processes of one WeaverAgent under their own
  UIDs.
- The **WeaverAgent system** is everything installed to run and to bound that agent:
  admin, the gate, the harness, the SPU, state management, and this crate's two
  connectors.

**The agent is bounded by the operating system under a UID**, as on any install. Where
this section gives a privilege it names the component that holds it, and a privilege an
operator gives the agent past what the WeaverAgent system needs is the operator's
business and outside this charter. **The deployment shape is a recommendation and never
a rule.** The suite recommends a machine given to one agent, a single-use machine or a
virtual machine, and in the operator's words "the deployment shape is up to the
operator. we just bring the board and the wires".

### 10.1 Pairing

**The box makes the connection, for pairing and always after**, on the operator's
rulings of 2026-11-16 (setup pages) and their addendum. The appliance never listens for
the console. It dials the console's link port, and the console listens, since to listen
is a network service's work. Both connectors have kept that direction since 2026-10-01
(section 5), and pairing carries it to the first connection an appliance makes, so an
appliance opens no port for the console at any point. The connectors dial the console on
every boot and redial a dropped link, and load and unload do not drop them.

**The box shows a short one-time code with the console's fingerprint, and the operator
enters the code in the console**, on the operator's rulings of 2026-11-16 (setup pages)
and their addendum, which supersede the line the operator ran in the appliance over SSH.
An appliance's console goes straight into the appliance's setup script at first boot, at
the hypervisor's console, and the script asks for the console's address and port. The
box dials the console over TLS and shows the code and the fingerprint of the certificate
it met. In the console, after the start page and the agent's name, the pairing page
shows the box waiting, and the operator compares the fingerprint with the console's own
and enters the code. The connectors hand over the public halves of keys made in the box
and receive their certificates. From then on every connection between the two is mutual
TLS, and the code is spent. **The pairing belongs with the connectors, so it serves a
host as it serves an appliance**, and the address may be localhost. **The reason is that
trust has to be carried by hand once, in each direction.** The fingerprint the operator
compares shows that the box reached this console, and the code lets the console know the
party is the one at the operator's screen. No shared key or secret ships in the image to
do this, so the code carried by hand does it and nothing that every copy of the image
carries does.

**The one-time secret is the only secret the operator carries by hand**, shown by the
box as the short one-time code, the single exception the operator accepts, in the
operator's words of 2026-11-14: "a one-time use password on the line after that none".
The rulings of 2026-11-16 and their addendum move it from a command line to the box's
console and the console's pairing page. It is the exception because no other way to
begin exists. **Three bounds hold it.** It is single-use. It is short-lived, on the
order of ten minutes. It is bound to one pending enrollment, the box waiting and the
agent the operator named at the start page, so it cannot enroll a box the operator did
not expect. Each bound narrows what a leaked secret is worth: it is spent once, it
expires in minutes, and it serves only the enrollment the operator is waiting on.

**Every root act of the setup runs on the box, at a screen the operator is sitting at**,
on the operator's rulings of 2026-11-16 (setup pages), so admin's grant does not grow.
Once the model is picked, the console shows a command to run at the appliance's console,
which starts the setup script's drive setup: the script lists the drives it sees, and
formats and mounts the state drive and then the models drive, each after the operator
confirms the disk (section 10.4). The drive setup is the appliance's script alone, and a
host's own script is later, and named as later. A host install points at a state
directory and a models directory and attaches and formats nothing (section 10.4). On a
virtual machine the first setup cannot forgo the hypervisor, in the operator's words of
2026-11-16, where the operator attaches the drives. The survey the image ships runs
locally, and its report crosses to the console over TLS. **Setup uses no SSH, and the
console never uses SSH.** The operator's SSH door is the operator's administrative way
into the machine, separate from Weaver Web. **The setup script's first question sets
root's password, and the operator's SSH public key reaches the box through the console
over TLS after pairing**, on the addendum to the operator's rulings of 2026-11-16 (setup
pages). So the console holds the operator's SSH public key, a public half and no secret,
and sends it down the paired link, and the operator's SSH door takes that key. Where the
console holds it is `weaver-web-Spec` section 10's.

**Trust on first use was a hazard to enrollment** while pairing ran over the operator's
first SSH, which trusts the host key it meets, so a wrong machine at the given address
could have been enrolled. The rulings of 2026-11-16 and their addendum move the pairing
to a code the box shows beside the fingerprint of the certificate it met, which the
operator compares with the console's own before entering the code. Section 9 names the
cell.

**This is an appliance's enrollment, and it is not a person's.** A person's enrollment,
the one-time token that registers a first passkey (`weaver-web-Spec` section 2.13),
keeps its name, and this charter and its Spec write "an appliance's enrollment" wherever
the two could be confused. A host pairs through the same connectors and the same
exchange, and the name covers it.

### 10.2 Keys

**Each connector makes its own private key inside the appliance, and the key never
leaves it.** The console signs the public half. **The reason** is section 5's standing
rule that the server keeps fingerprints and never keys, carried one step further: the
console never holds the private key at all, so no file carries one from the console to
the machine. For an appliance this replaces the making of keys on the server at
registration, the operator's ruling of 2026-10-01, and `weaver-web-Spec` section 8.2
says what stays, what is replaced and what coexists.

**The connectors' keys and certificates live on the appliance's root disk**, on the
operator's ruling of 2026-11-15. **A new image is a new enrollment**, on the same
ruling, made by a new pairing at the image's first boot (section 10.1).

**Each appliance is meant to hold its own certificates, and the console refuses a second
live holder of a certificate it issued**, the operator's rulings of 2026-11-14.
`weaver-web-Spec` section 8.2 says how the built link meets the refusal. A machine
cloned after its enrollment is a note there and nothing more, on the operator's ruling
of 2026-11-15.

### 10.3 The link's one unauthenticated surface, and the posture

**An appliance's enrollment connection is the link's one unauthenticated surface, and
this charter names it as one**, in the wording the operator ruled on 2026-11-15. It is
the one connection the link port admits before its party is known, and its party proves
itself with the one-time secret alone.

**Two pages of the browser's listener answer before a session exists**: the sign-in page
and a person's enrollment page. Each answers to a proof of its own, a passkey or a
one-time token, as built (`weaver-web-Spec` section 2.13).

**The link is encrypted from its first act, the enrollment included**, as section 6
says, and it is mutually authenticated from the end of the enrollment onward.

**The posture.** The console is the operator's infrastructure and never faces the
public, the operator's standing statement (section 2, and `weaver-web-Spec` section
2.13). Its link port is reached by the appliances the operator places, and the
enrollment surface admits a party before that party is known. That is why the secret
carries three bounds. Where the console stands, and who can reach its ports, is the
operator's choice, as the deployment shape is. **Ports opened by a declaration belong to
the operator's arrangement and are recorded on the run card**, on the operator's ruling
of 2026-11-15. As built on 2026-11-15, no declaration can name a port, since the
declaration has no port member (`weaver-web-Spec` section 6.1), and the member is an ask
upstream (section 8). Section 9 holds the term and the member. It also holds for the
operator this ruling's conflict with section 3.0, which gives the network to the two
connectors alone, and with section 5, where the connectors are the one party that
reaches an agent. This charter names no address of a machine and no rule of a firewall.

### 10.4 Models, the image and storage

**The console fetches models from Hugging Face alone**, on the operator's rulings of
2026-11-16 (setup pages), which supersede the four sources of the ruling of 2026-11-14.
Hugging Face's layout is the declared standard for an agent's models: anyone with a
model of their own can put it on Hugging Face, and a local source and the other sources
are later, and named as later. **The console connects the operator's Hugging Face
account once, as a console setting, before the start page.** It then reaches public
models, the operator's private repositories and the gated models the account has been
granted. A gated model the account lacks is shown with the link to accept it on Hugging
Face and a button to check again. The setting accepts a read-only or a read-write token.
**The console never writes to Hugging Face except on an explicit push in Train**, where
training inside WeaverTools will push adapters up, so write stays open for that stage.
**The token lives in the console's store and never in an appliance**, so a credential
for the source stays where the operator administers the console and reaches no machine
an agent runs on.

**A pull records the repository, the commit, every file's SHA-256 and size, one per part
for a split model, the account that pulled, and the time.** A download uses a link
through the commit and never through a branch name, and the console hashes each file
after download and compares. The model's bytes are a member of the deployment tuple
that a replay names, which is why the pin is exact. Whether a pull is the Models
surface's import (section 3.6) is section 9's. **The first agent's decoder is the BF16
GGUF of `ibm-granite/granite-4.2-30b-GGUF`, a two-part file**, and the first test
downloads it fresh through the page.

**The console keeps what it pulled, in its cache inside the Weaver Web install, and
reuses it from there.** An appliance pulls a model from the console's cache over the
same mutual TLS trust, and it checks the digests. That keeps the direction of section
10.1, the appliance dialing the console, and one pull serves every agent that declares
the model. On the operator's rulings of 2026-11-16 (setup pages) the console starts the
download in the background once the model is picked, the appliance asks the console
over TLS for the chosen model's size, and the appliance pulls the model when the
download finishes. **The cost is stated.** The console
becomes the model store, and on a network larger than one site that is a question of
disk and of bandwidth that the console has to answer. Section 9 carries it.

**The console offers the image for download, and it checks the hash of the image against
the published hash before it hands the file over.** An image is the held object of a
deployment tuple, named by its version and its hash, so a file that does not match its
published hash is not that version. One image comes first, for KVM. Proxmox is out of
scope on the operator's ruling of 2026-11-14 and is the first format named as later, and
`toddwbucy/WeaverTools` issue 29 records one condition to settle then. Other formats are
later and are named as later.

**The WeaverAgent system checks storage at every load and reports the sizes to the
console. Where storage is not sufficient, the console says so and asks the operator to
add storage.** On the operator's ruling of 2026-11-15, the models mount is required, and
a load is refused where it is not a separate filesystem. **The state mount is required
for every agent, at a minimum of 10 GB**, on the operator's rulings of 2026-11-16 (setup
pages), because the system prompt lives in state and the save point the unload writes
lands there. That supersedes the ruling of 2026-11-15 that the state mount is required
only where the declaration elects state, and a required mount is held to the same test.
The models drive's size is computed from the chosen files, and the setup flow states
every size in one unit, GiB. **The refusal is admin's, since load is admin's verb, and
the console renders admin's answer as it came** (`weaver-web-Spec` section 7.2). The
check runs at every load and not once at enrollment, because the refusal is a load's: a
required mount that is not a separate filesystem refuses that load. **The console
attaches and mounts nothing.** On a virtual machine the operator attaches the disks in
the hypervisor, and the appliance's setup script, started by a command the console shows
once the model is picked, lists the drives it sees, asks which one is for state, and
formats and mounts each after the operator confirms the disk, on the operator's rulings
of 2026-11-16 (setup pages), which supersede the ruling of 2026-11-15 that the operator
mounts them over SSH. A host install points at a state directory and a models directory,
and attaches and formats nothing, on the addendum to the same rulings, and the drives
page shows the directories the operator named with free space against the need. A host's
models directory defaults to that user's default Hugging Face cache, so models already
pulled there are found, and the appliance keeps its models mount. Whether a named
directory meets the separate-filesystem test a load holds a required mount to is not
ruled (section 9). With no drive attached the script warns that at least one drive is
for state, about 10 GB recommended, and checks again once the operator attaches one. The
models drive it asks for is sized from the chosen model, whose size it asks the console
over TLS (section 10.1). The form of the size report is WeaverAgent's (section 10.7),
the report's way to the console is an ask upstream (section 8), and the sizes that count
as sufficient are open (`weaver-web-Spec` section 10).

### 10.5 Load and unload, and no hypervisor

**Load and unload are the verbs of admin as built, and the console asks for them through
admin-con** (`weaver-admin-PRD` section 4, `weaver-web-Spec` section 7.2). **They need
no management of a machine**, on the operator's ruling of 2026-11-15. **Nothing in this
framework or in the image autostarts the agent**, on the same ruling. After a reboot the
connectors come up and the console sees the appliance, and the agent stays unloaded
until the console loads it. What an operator adds to a machine is the operator's to
account for, as the stance says. To start and stop the agent is to load and unload it
through admin, and the console's whole reach into an appliance is its two connectors.
**That holds from the first minute**, from pairing onward, on the operator's rulings of
2026-11-16 (setup pages), since every root act of the setup runs on the box at a screen
the operator is sitting at (section 10.1).

**An upgrade moves state and model to a new harness version**, on the operator's ruling
of 2026-11-15. State is transferable, and an upgrade moves it. Reproducibility is
claimed within one version on one hardware set. Nothing is claimed about it across
versions. An upgrade that brings a new image is a new enrollment (section 10.2). **An
upgrade of an appliance is a change of the deployment tuple and is recorded as one**,
the ruling of 2026-11-14: state made under an older version is recorded as made under it
and, from the load onward, as used under the newer version. **The run card records the
tuple as shipped**, on the ruling of 2026-11-15, and no guarantee is offered about the
tuple as it runs. The record of an upgrade is WeaverAgent's, an ask upstream that is
held (section 8), and how the console shows it is `weaver-web-Spec` section 10's.
**State is in scope for the first agent**, on the operator's rulings of 2026-11-16
(setup pages), which supersede the ruling of 2026-11-15 that put state and save points
on an appliance out of scope. The first production agent declares a store and keeps the
system prompt, and it keeps the save point the unload writes, on the state disk, by the
existing save point path and with no new persistent store. Operator-managed save points
stay out of scope (section 9).

**What lets the operator tell a stopped service from a machine that is down is open, and
it is the console's to show.** What the appliance exposes for it is an ask upstream
(section 8), and it is settled when the first stop is run from the console
(`weaver-web-Spec` section 10).

**Weaver Web never touches a hypervisor**, per section 2, and nothing in this section
reaches one. The console makes, starts and destroys no machine, attaches no disk and
maps no card, and the one stop it asks of a machine is section 2's shutdown command, on
a virtual machine alone. The console knows the agent and the hypervisor knows the
machine, and neither reaches into the side of the other.

### 10.6 The trace's destination

**The suite's duty is to make the trace available**, on the operator's ruling of
2026-11-15. **The agent does not write the trace to disk. It hands the trace off to
where the operator says**, on the same ruling. **Where the trace goes is a field of the
declaration page, set when the agent is created**, on the operator's ruling of
2026-11-15. The operator may send it to a file, to `/dev/null` or anywhere else, and
what stands behind the destination is the operator's. **A null sink is legitimate**, and
the run card records the choice.

**Where the destination is not a file on the agent's machine, the console holds no trace
for that agent** (section 3.0). The trace door has nothing to serve, so Record, the live
view and Train have nothing to read for that agent, and the console says that it holds
no trace for that agent. Whatever keeps the trace past its destination is the
operator's, and the console states nothing about it. That is the operator's choice. **A
destination that cannot be opened refuses the load**, admin's standing rule
(`weaver-admin-operator-contract` section 5), and an agent that elects no state is held
to it like any other: an operator who wants an agent that keeps no state declares none,
on the operator's ruling of 2026-11-15.

**The field is not built.** As built on 2026-11-15, the declaration renders the trace
sink as a comment, filled at Provision (`weaver-web-Spec` section 6.1). The field is the
next act's work, carried in `weaver-web-Spec` section 10 with what the console's pages
show where the console holds no trace. The field writes the grammar's `trace-sink`,
which names a file, a pipe or a socket (`weaver-types-Spec` section 2). As merged,
WeaverAgent places every sink in the agent's territory and admin refuses one whose
directory stands elsewhere, so `/dev/null` and a destination outside the territory wait
on WeaverAgent's amendment, and section 8 carries the ask.

### 10.7 What is not this crate's

Three bodies of the appliance are not this crate's. This crate builds against each once
it is merged and carries none of its facts, so no location, mount, account or address of
an appliance enters this repository. None of them is cited here as authority, since
every requirement above is an operator's ruling.

- **The environment inside an appliance**: the operating system, SSH, the mounts and the
  storage check, and how an upgrade moves state and model to a new harness version. What
  of it runs or bounds the agent is the WeaverAgent system's, in
  `toddwbucy/WeaverAgent`.
- **The image's build and its proof** are in `toddwbucy/WeaverTools`: the binaries
  compiled inside the image from named commits, one image for each version with its
  hash, and whether a rebuild can confirm an image.
- **The evidence and the measurements**, `toddwbucy/WeaverTools` issue 28: what the
  claim that an image holds a deployment tuple rests on, and what is owed before a Spec
  relies on it.
