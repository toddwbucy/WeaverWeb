# weaver-web - Spec

**Status:** MERGED. In `main` and the source of truth. **Merged is not
ratified.** This text inherits no ratification from the Spec it replaces,
and takes whatever ratification the rewritten `weaver-web-PRD` is granted,
which is the operator's to say.

**A rewrite of this crate's Spec, not a new one.** Authored 2026-09-04 from
the thinkpad seat beside the rewritten `weaver-web-PRD` of the same date.
The prior text is replaced whole rather than amended, and git is its
archive.

**Date filed:** 2026-09-04
**Document ID:** `weaver-web-Spec`
**Parent:** `weaver-web-PRD`
**Editorial:** Per the Working Rules. ASCII, absolute dates.
**Landing PR:** #628

## 0. What this document is

The charter says what this crate is for. This document says what it is made
of, in the order a reader needs it: the store first, because every surface
reads from it, then the paths into and out of that store, then the seams,
then what holds each claim.

**Where this document elects something the charter left open, it says so.**
Where it leaves something open, section 10 names it rather than letting
silence settle it.

## 1. The crate

```text
src/
  store/        the store: schema, migrations, the reads of section 4, the
                identities, the audit and the alerts
  surfaces/     the browser surfaces of sections 6, 2.13 and 2.14, one module
                each
  link/         the link of section 8, both halves: the listener and the
                register verbs the server runs, and the client each
                connector runs, admin-con's relay client and sudo invoker
                among them. The link of this entry is a connection and
                not the seam kind the clause below names
  adapters/     gate.rs, gate-con's client of the gate (section 7.1)
  bin/          weaver-web, the server, and gate-con and admin-con, the two
                connectors of section 8
  alerting.rs   the alerts' worker and the operations log of section 2.14
  config.rs     each process's configuration
  fault.rs      the one answer to a fault, shared by every surface
  host.rs       the host's identity commands of section 2.13
  listen.rs     the browser's listener of section 2.13
  passkeys.rs   the passkey ceremonies' server half of section 2.13
  traceview.rs  the live window of the trace
  *_tests.rs    the tests beside the modules they exercise
```

**The box-bound reaches are linked by the two connector binaries and never
by the server**, which is how the server's never reaching an agent is a
build fact rather than a sentence. The reaches live in `adapters/gate.rs`,
gate-con's client of the gate socket per section 7.1, and in
`link/relay.rs` and `link/sudo_invoker.rs`, admin-con's reader of the trace
relay and its invoker of admin's verbs per section 7.2. The two connectors
are named as binaries under `bin/` because the split between them is a
split of posture and not of module: admin-con runs as a service user the
box's sudo rule grants the agent's fixed command lines and the trace relay
admits as its one reader, and gate-con as one the gate admits, and a module
tree cannot say which process carries which. admin-con's sudo invoker is
the one privileged invocation in this crate, per section 7.2. The ingest of
section 3.1 and the staged experiments' queue of section 5 are unbuilt and
hold no module. The authored writes of section 3.2 that are built are
section 2.13's identity writes, which are `store/`'s, and section 2.12's
register writes, which are `link/register.rs`'s.

**This crate links no crate of the agent** and reaches it exactly as an
outside consumer does: a socket dialed by path, a binary run by the
operator's verb, and a record read where the operator keeps it. **The party
that does each of those is one of the two connectors of section 8 and never
the server**, per the design session of 2026-09-30 and the operator's ruling
of 2026-10-01: the connectors are this crate's, they stand on the agent's
box, they reach the server, and the server reaches nothing past them. That
property is load-bearing and is not spent by the seam of section 7.3, which is a stream
this crate reads rather than a crate it links.

**The line is the agent's process boundary and not the cargo workspace**, as
of 2026-09-07. `axiom-floor-is-vocabulary-behavior-is-socket` forbids "a
behavior reached by path dependency across a process line, not a crate
calling a crate inside one binary", and it names the library boundary as a
seam in its own right, tagged `link` rather than `socket` so the difference
is stated. Two such seams already stand in the base set, the harness to
`weaver-trace` and the harness to `weaver-diagnostic`, each under its own
contract, and `weaver-trace` is the standing proof that a crate outside the
floor may be linked.

**So what this crate may not link is the agent's interior**, whose crates
run behind a process line this one never crosses: `weaver-harness`,
`weaver-spu`, `weaver-admin`, `weaver-gate`, `weaver-state`. Linking any of
them would make this crate a compile-time dependent of the interior, which
is the reading `weaver-analysis-Spec` section 6 gives the same axiom for
itself.

**A crate that also runs outside the agent is not the interior.**
`weaver-analysis` parses a finished record as an operator principal on the
operator's own storage, which is this crate's own posture, so a link to it
crosses no process line and takes nothing from the agent. **This document
does not take that link.** It states that the axiom permits it, so a later
act that wants it argues about whether the work belongs there rather than
about whether the boundary allows it, and such an act owes a `link`-tagged
seam and the contract half that governs it.

**The earlier sentence said "the agent workspace" and its own reason said
the agent.** A cargo workspace is a build fact and the interior is a
process fact, and the scope this property protects is the second.

## 2. The store

The grain is the grain the interface clicks at.

**The store has two halves and they are written by different paths.** What the
instrument recorded is sections 2.1 and 2.2, the position and the run, landed by the
ingest of section 3.1 and never by a surface. What the engineer authored is sections 2.3
through 2.5, the artifact, the declaration and the staged experiment, landed by the
authoring path of section 3.2 and never by the ingest. Section 2.6's recorded query is
the read's own trace and belongs to neither, written by the read that serves it and by
nothing else. **Section 2.7 is not a table**: it is how the rows above are reached, and
it carries the rule that a derived value is derived once at ingest rather than at any
read. **Section 2.8's session belongs to neither half either**, and it sits after the
indexes because it is the only row here that is about the operator rather than about the
instrument or the work. It is admitted on the same ground section 2.6 was: a rule stated
elsewhere needs somewhere to land. **Sections 2.9, 2.10 and 2.11 are authored rows
sitting outside their half**, the plan, the refs and the batch, appended after the
session rather than placed among 2.3 through 2.5 because renumbering would break every
citation of sections 2.6 through 2.8 in this document and outside it. **Section 2.12 is
the registered agent**, appended 2026-10-01 on the same reasoning: an authored row the
register verb of section 8 writes and the link reads, about an agent rather than about
the work, sitting outside both halves as the session does. **Section 2.13 is identity and
access**, appended 2026-10-02 on the same reasoning: the person, the role and the grant,
and the audit record of every verb asked, about who may act on an agent rather than about
the work. **The grouping is stated
here rather than carried by the numbering**, which is the cost of a document whose
sections are cited by number.

**An authored row's identity says what it addresses.** A key this store generates is
spelled as two letters naming the kind and sixteen hex: `pl-` a plan, `ar-` an arm,
`rf-` a ref. **A row this crate received keeps the identity the record spelled** -
section 2.2's run, whose identity is the record's and never this store's to respell.
So a key's shape says whether its row was authored here or landed here, and says it in
the one place a reader always has, which is the value itself.

**Section 2.3 is the case that shows the rule has two axes and not one.** The artifact
is an authored row, landed by the import of section 3.2, so the rule above reaches it -
and the *record identity* it carries is a digest the SPU computed, which is a member
rather than a key and is not this convention's business. **A row's half and a member's
provenance are different questions** and the answer to one is not the answer to the
other.

**The prefix is what a bare sequence could not do.** Every authored key here was one
before 2026-09-11, so the integer `3` was a valid plan, arm, artifact, experiment and
declaration at once and nothing caught one handed where another was owed - not the
compiler, not the schema, and not a link. The kind is in the value now: the schema holds
it through a domain per kind, and the crate's key types hold it at the boundary a caller
meets, so a key of the wrong kind is refused before a read answers `None` and a reader
takes that for a row that is not there.

**What the shape holds and what it does not.** It holds the kind. **It does not hold
that this store generated the key**: a text key with a default is one a writer may
supply, where the sequence it replaced could be overridden only explicitly. That is a
real loss against what was there before, and it is stated rather than papered over,
since the kind in the value is what the change buys and provenance is not.

**The name is not the key**, for the rows that carry both. Section 2.9's arm has a name
the operator gives and relabels, and a key that changes when a label changes is a key
that cascades through every row referring to it. **It is stated here because it cannot
be retrofitted cheaply**: a key's shape is the one decision every reference to a row
depends on, which is the same reasoning section 3.2's author member was landed under.

**Three authored tables do not carry it yet.** Sections 2.3, 2.4 and 2.5 - the artifact,
the declaration and the staged experiment - were built before this convention and keep
their sequences, so the rule above describes four of the seven authored rows and not all
of them. **A convention with an undeclared exception is worse than none**, so the
exception is declared and the act that closes it is owed.

**The halves differ in what a rewrite means.** A recorded row is a fact
about a run that happened, so a second write of it is a replay and must be
idempotent. An authored row is a thing a person is still making, so a
second write of it is an edit and must be ordered. Section 3 states each.

**They differ in a second way, and this document carried only the first
until 2026-09-07.** A recorded row is about an instrument, so it names a run
and no person. An authored row is about a person's work, so **it carries who
is making it, or says it could not be named**, per section 3.2. The recorded
half needs no such member and carries none.

### 2.1 The position

**The address is the run, the turn, and the position**, and the composite of
the three is the primary key.

```graph
node: web-position-is-addressed-by-run-turn-position
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-web
to: web-position-is-addressed-by-run-turn-position
```

**This corrects the two-part key this document first carried.** The record's
own spelling is what settles it: turn keys repeat across a serving record's
runs, so an address without the run answers one line per run, which is why
`weaver-analysis field` takes `<turn>:<position>` and an optional `--run`.

**And `position` is the resident length at the draw, not an ordinal within
the turn.** The first generated token of a sixty-token prompt is position
sixty. Verified 2026-09-05 against a field-bearing record on the pool, whose
first `model.field` event carries `position: 60`. A surface that treats the
two as one word will address the wrong token, and section 6 states the
conversion.

Each row carries:

- the emitted token, by identifier and by surface text
- the surprisal and the entropy at that position
- the ranked alternatives with their probability mass
- the raw residual where the capture holds one

**`realized` is a rank and not a token.** It names which rank the draw
landed on, so a row carries `realized` as the record spells it and the drawn
token resolved beside it, and neither is presented as the other.

**The alternative count is the declaration's field election, not this
crate's parameter.** `model.field` carries the ranked candidates under
`field-election: { depth }`, and the records on hand carry forty and fifty.
The depth is the operator's ruling, open at the charter's section 9, and
this crate stores what the election kept rather than choosing a number.

**Raw residual rides alongside rather than a projected readout**, so a lens
refitted later can read a run captured earlier. This is the standing
raw-residual ruling and this document does not disturb it.

### 2.2 The run

Everything identifying the conditions lives in the run's own row:

- **the record identity** at a grain fine enough to catch a quantization
  difference, which is the weights hash the SPU computed
  at admit per `weaver-spu-Spec` section 3, taken from the summary the
  analysis seam carries per `weaver-analysis-web-contract` section 2.2 and
  never from the declaration, a declaration naming a path and being editable
  after the run
- seed, and the full sampler configuration, both taken from the effective sampling the
  record's `model.request` carries, per `weaver-analysis-web-contract` section 2.2
- **the device**, the device model a deposit names, per that contract's same section,
  **absent where the caller named no deposit** and never defaulted, a run whose silicon
  nobody recorded being a run this store holds rather than refuses. **Precision is not
  here and was until 2026-09-09**, having no producer anywhere in the corpus, and
  section 2.3 carries it as the artifact's label because the record identity above
  already catches a quantization difference
- **the engine's identity at build grain**: the libraries the decode ran through, each
  by name and digest, with the build that produced them. **It has two sources and is
  absent for neither half where one is missing**: the record's `load` event carries the
  organ binaries in its `stack`, and a deposit the caller names carries the engine
  libraries, the commit, the toolchain and the driver, per
  `weaver-analysis-web-contract` section 2.2. A run ingested with no deposit named
  carries what the record held and says the rest is absent
- **the field election's depth**, because it sets the length of every
  position's alternative list and the charter's section 4 compares them
- **the task, by source and identity**, which nothing upstream produces and which this
  crate asserts the reader's half of, as it does for the forced mark below. The task is
  a first-class element the charter's section 3.3 gives four sources, and it is composed
  here rather than recorded: no organ reports it and no seam carries it, so a run
  ingested from a record alone carries none and says so. **It is filled where a staged
  experiment produced the run**, that row naming the task the operator composed, which
  is the one path either member has today
- **the declared boundary set**, which is empty on every run this document can be
  written against and complete rather than short. Every organ in the base is reached by
  kernel peer identity over a Unix socket, per `weaver-admin-PRD` section 4.3, so no
  organ owes a boundary and a declaration naming a loopback or off-host organ is refused
  by the floor's parse. **The set stops being empty at the first act that charters a
  reachable organ**, which carries the declaration member from birth and which the load
  event then records, per issue #438
- **the prompt file's digest, `identity_file`**, the identity the agent was given to
  hold, which the load event records per `toddwbucy/WeaverAgent#57`, so two runs under
  different prompts are never one condition
- **the state-management values in effect**, the agent's `[state-management]` settings
  as the load event records them at that load, per `toddwbucy/WeaverAgent#1` and `#58`,
  so two runs under different state-management settings are never one condition
- the parent run reference and branch position, where the run is a branch,
  **which are lineage and stand outside tuple equality**
- **the parting position** where the run is a branch, the first at which its
  token path left its parent's, derived at ingest per section 3.1 and
  absent where the paths never part, which is also lineage and also outside
  the compound
- **the emission's signature**, a set of shingles over the emitted text
  derived once at ingest, **which is a property of the result and stands
  outside tuple equality with lineage**, per section 5.4
- **the record's session**, the identity the trace's runs share per
  `weaver-trace-PRD` section 2.1, which is not section 2.8's session, that
  one being a person's, and which stands outside tuple equality with
  lineage since it says which conversation the run belongs to and not what
  it ran under
- **the record's digest**, sha256 over the run's own lines of the record as
  the emitter drained them, per `weaver-analysis-web-contract` section 2.2,
  which stands outside tuple equality with the signature since it says
  which bytes the row descends from, and is absent where the emitter did
  not drain the run whole
- **the task's verdict**, the predicate the task answered and the ratio over the task's
  denominator where the task supplies one, per issue #523's ruling of 2026-09-09, which
  stands outside tuple equality beside the signature since it says how the run was
  scored and not what it ran under, and which is absent where no task scored the run
- **the seated prefix's length**, the resident length before the run's
  first turn's input, per `weaver-analysis-web-contract` section 2.2, which
  stands outside tuple equality as an address on the run's tape and not a
  condition, the prefix's content being the declaration's, and which is
  what section 5 makes a whole-run arm's branch position
- whether a token was forced, and which

**The verdict is the task's and this crate scores nothing.** The task supplies the
predicate and the denominator, per the charter's section 9 and issue #523, and a verdict
is a fact about a run that happened rather than a reading taken over it, which is why it
lands on this row and not in section 2.6. **It reaches the record on a trace event the
task authors at the run's close**, so a record carries its own score. **The seam that
carries it exists as of 2026-09-09** and `weaver-analysis-web-contract` section 2.2
states its shape, crossing on the entry for the generation the close names and once per
run in this row. **The kind that produces it does not**, being `weaver-trace`'s to add
per issue #523, so this crate asserts the reader's half and not the writer's, as it does
for the forced mark of the paragraph above. The shape is fixed on both sides so that act
lands the kind rather than electing it.

**The column arrives with the kind and not before**, which is the same rule that took
the batching election and precision off this row: a member nobody can fill is a reserved
slot, and the apex refuses one whether it is an interface or a column. So this row's
member is stated here and the schema gains it in the act that gives it a producer, which
is when a reader could first meet one.

So a reader asking this row for a verdict before that act finds the member is not there
to ask for, which is what a member with no producer honestly is, and section 3.1 lands
it from the day it crosses. **Absent
says no task scored the run or no path yet carried what it said**, and never that the
run failed its predicate. A verdict the task recorded as failed is present and says so,
which is the whole distinction this member exists to keep.

**It lands with the run row and takes that row's rules, not rules of its own.** The
verdict crosses once per run beside the record's session, its digest and the seated
prefix's length, so it is written when section 3.1 writes the row, a replayed ingest
writes what it wrote before, and **a run whose generations disagree on it is the defect
the reader names** exactly as they are for the three beside it. There is no second write
path for a verdict and none is owed: a member that arrived on its own would need an
ordering against the row it belongs to, and this one has none to need. **A scorer
that is not the task's is a second verdict beside this one and never a change to it**,
per section 10.

```graph
node: web-verdict-is-landed-and-never-scored-here
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-verdict-is-landed-and-never-scored-here
```

**The row names the record it came from, twice, because the store is a
projection and never the record.** Section 3.1 says so of every row in this
half. The session says which family of runs the row belongs to, so a
session's runs are one read rather than a walk up parent references, and
the digest says which bytes, so a reader holding the operator's record can
recompute it and a row that does not match is a row to refuse rather than
to trust. Neither is derived here: both are read from the emitter that
drained the record, which is the only party on this seam that saw the
bytes.

**Three members of this row had no producer and two of them have left it.** The batching
election is gone: every use of the word in the corpus outside this document is
continuous batching as a serving technique the architecture refuses, `weaver-spu-Spec`
having one forward per prompt and never a batch, so **a member recording a choice the
program does not offer is a member nobody can fill**. Precision is gone to section 2.3
as a label on the artifact, the record identity above already being at a grain fine
enough to catch a quantization difference, so the tuple loses no distinction by losing
the word. The boundary set stays and is empty for the reason its own bullet gives. All
three were found by issue #532 while the ingest was being written, and the operator
ruled them on 2026-09-09.

**The prompt's digest and the state-management values joined the tuple on
2026-10-03**, per WeaverAgent's answer to question 3 on
`toddwbucy/WeaverAgent#59`: both are run conditions. Both come from the
load event and neither crosses yet, so **their columns arrive with the
load event that carries them and not before**, by this section's rule for
a member with no producer: the schema gains them in the act that meets
WeaverAgent's load event carrying both, and until then a reader finds the
members are not there to ask for.

**The engine is in the compound, so the row holds it.** The tuple is a model
on a device under a kernel, and a divergence between two rows
differing in both silicon and library revision names neither cause unless
both are recorded. **The regime that deposits these identifiers is
`weaver-analysis-PRD` section 3's**, which carries code identity beside the
device model, the commit by hash, the toolchain and driver by pinned
version, and the binaries and engine libraries by the driver's sha256. This
row carries the same fact so a reading drawn from the store needs no
deposit beside it to be read.

**The forced member is read and never inferred.** Whether a token was forced
is the record's to say, and **no agent-side document says it yet**: the
forcing has no home in `weaver-spu-Spec`, the decode contract, `weaver-trace`
or the harness, which is the documents act issue #442 still owes. **So this
crate asserts the reader's half and not the writer's.** A run whose record
does not carry the mark is ingested as a run whose forcing is unknown, never
as an unforced one, absence refusing rather than defaulting, which is section
6's rule applied at the ingest. When #442 lands the writer's half, the claim
that the mark is written belongs to whichever document defines it.

```graph
node: web-absent-forced-mark-does-not-default
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-absent-forced-mark-does-not-default
```

**The row holds more than the tuple, and the difference is lineage and the signature.**
The parent run reference and the branch position say where a run came from rather than
what it ran under, per the charter's section 4, so **two rows differing only in them
hold the same tuple** and remain comparable. **The signature is outside it for the
opposite reason and the same effect**: lineage is not a condition because it says where
the run came from, and the signature is not a condition because it says what the run
produced. Two runs under one tuple whose emissions differ hold different signatures and
**the same tuple**, which is the whole of what a sweep and the device arm both measure,
and a branch holds its parent's tuple where the diff moved nothing. The record's
session, its digest and the seated prefix's length are outside it on their own bullets'
grounds, and nothing else in this list is. Were lineage inside it, a branch could never
be reproduced against its parent, which is the comparison the staging surface exists to
make.

**The signature is derived and the similarity is not.** A signature is a
property of one run and is derived once at ingest, which section 2.7
requires. **A similarity is a property of two runs and of which one a reader
chose as the baseline**, so it can be derived neither at the read, which
section 2.7 forbids, nor at the ingest, which would need every pair before
the second run exists. Storing the signature moves the cost: the comparison
becomes a set operation over two stored values rather than a walk over two
emissions, and section 4's fourth read records it where section 2.6 records
any other, which is what section 4 already says keeps section 2.7's rule
rather than spending it. **What section 2.7 forbids is a derivation nobody
can find**, and a similarity computed from two stored signatures and recorded
beside its query is findable by anyone holding the runs.

**A reading without its tuple is a reading of an unnamed compound.** The
task is in this list for the same reason the artifact is: two runs of the
same benchmark item under different suite versions are not the same task,
and nothing else in the row would say so.

### 2.3 The artifact

The catalog the charter's section 3.6 surfaces. **Keyed by the artifact's
weights identity rather than by a path**, because a path is where a file
sits and an identity is what it is, and the same weights under two paths on
two boxes are one artifact.

**The identity is the corpus's existing one and this document defines no
second**, because a catalog that dedupes on one rule while a lens refuses
on another is two rules for one subject. **Its identity is the set of
per-file content digests keyed by file name**, which is the shape
`weaver-analysis-Spec` section 3 gives `fitted_for`'s
`model_safetensors_sha256`: one digest for a model kept in one file, and a
digest per shard keyed by shard name for a sharded one.

**What the complete set is depends on the container, and both cases are
named.** A safetensors model enumerates its shards in its own index,
`model.safetensors.index.json`, which the analysis reader follows. **A
sharded GGUF carries no index.** Its set is the siblings matching the
`-NNNNN-of-NNNNN` pattern, which is how `weaver-spu-Spec` section 3's pin
collects them, and a single-file GGUF is the one-file case unchanged. This
catalog holds GGUFs, so the rule that names only an index would leave the
format the box actually runs without one.

**Equality is set equality and no order is imposed**, the map being keyed
rather than sequenced. **A set missing a file the index names is unequal to
the complete set** and therefore joins to nothing, which is the whole of
what the incompleteness rule needs and asks for no rolled-up digest of this
crate's own devising. The catalog dedupes on that set and on nothing else,
and **this document calls it the weights identity** to keep it apart from
the record identity beside it, which is the SPU's hash and is what section
2.2's tuple holds. The weights identity is fine enough to separate two
quantizations because their files differ.

```graph
node: web-incomplete-shard-set-joins-to-nothing
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-incomplete-shard-set-joins-to-nothing
```

**The provenance chain is recorded and is not an identity.** A conversion
or a quantization records its source artifact with the converter and the
pin, so the chain says what an artifact was made from. It does not version
a lens: `weaver-analysis-PRD` section 3 owns that and versions by the
weights content hash, which a conversion changes. The chain is what makes
an elected reuse across a conversion legible after the fact, per the
charter's section 5, and this crate records the election rather than
deriving it.

Each row carries:

- **the weights identity**, this table's key, derived as above
- **the precision label**, the word an operator reads to tell one row from another,
  authored by Models at import because that surface is the one that knows and no organ
  reports a dtype. **It is a label and never an identity**: section 2.2's record
  identity is at a grain fine enough to catch a quantization difference, so two
  artifacts are told apart by their hashes and this member is what a person calls the
  difference. It is nullable, an import that did not say carrying none.
- **the record's identities**, the
  weights hashes `weaver-spu-Spec` section
  3 states, one computed at each import over the files that import
  registers and each unique across rows, a renamed split GGUF excepted per
  section 10: the member is a set because the same weights imported again
  from a directory differing in a file the index never names compute a new
  value, and that value joins this row rather than opening one.
  **The sentinel is never a record identity here**: an import that cannot
  compute the value registers nothing, per the charter's section 3.6, and
  a run carrying the empty string joins to nothing and is named as one
  whose identity the SPU could not compute, which is a different fact from
  a run no row carries
- the identity **per file**, following the model's own index, each shard
  under its own name and verified against it
- the provenance: a repository and revision, or a source artifact with the
  converter and pin that produced it
- where it is present, by box, as an observation rather than a fact
- the lens artifacts fitted to these weights, each versioned as
  `weaver-analysis-PRD` section 3 versions them, by the weights content
  hash
- the reference trials taken against it
- **the author**, per section 3.2, which is not presence's reporter: the
  reporter says who saw these weights on a box, the author says who
  imported the row
- **the row's version**, per section 3.2

**Two identities name one artifact and the table carries both**, as of
2026-09-06 per issue #465. **The weights identity is this table's key**, the
per-file digest set above, at the grain of the weights alone, and it is what
a lens refuses on. **The record identity is the SPU's weights hash**, at the
grain of everything the binding named, and it is what section 2.2's run row
holds and what a measurement carries.

**The join from a run is a lookup on the record identity, not on the key.**
The row carries every record identity its weights have been admitted under,
so the lookup is against that set and the key is what the row is stored by.
**The record identity determines the weights identity and not the reverse**,
because a directory's hash fixes its shards. **So one record identity names
at most one row for a file or a directory artifact, and a renamed split GGUF
is the one exception, where it may name two**: the split's hash covers its
shards' bytes and not their names, per `weaver-spu-Spec` section 3, so two
sets of identical bytes under different stems are one record identity and
two weights identities. **The lookup reports that as ambiguous rather than
picking**, per section 10, which is where the exception is held open. The
conformance row of section 9 watches the rule with its exception rather than
the rule alone.

```graph
node: web-record-identity-names-one-row-a-split-excepted
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-record-identity-names-one-row-a-split-excepted
```

**A run whose record identity no row carries joins to nothing and registers
nothing**, a record being a fact and not an import, until an import on that
box registers the files.

```graph
node: web-sentinel-joins-to-nothing
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-sentinel-joins-to-nothing
```

**What does hold is the catalog's own shape.** A lookup by this table's key
resolves to the complete file set or to nothing, so a lens relation and a
reference trial relation each name one unambiguous artifact and never a
shard of one, and an elected lens reuse is legible here with the fitted
weights and the read weights both resolving by the same rule.

**Presence is a dated observation by a named reporter, and it is
advisory.** Each entry carries the box, the reporter, and when it was last
confirmed. A box that has not reported is unknown rather than empty,
because this crate runs on one machine and the agents on another, and
silence from a box that is merely offline is not evidence about its disk.

**No load consults it.** The load resolves the artifact on the box it runs
on and is refused there under that box's own rules. Presence here is an
index for an operator choosing where to place a run, never the thing
deciding whether the run may proceed: a gate built on a stale observation
refuses a box that holds the artifact and admits one that lost it.

```graph
node: web-presence-never-gates-a-load
kind: assertion
tag: review

edge: asserts
from: weaver-web
to: web-presence-never-gates-a-load
```

**Nothing here fetches.** An entry says an artifact was seen, not that it
can be obtained. Whether a missing one is fetchable is the provenance's
question, answered by the repository and revision the row already carries.

### 2.4 The declaration

The saved declarations the charter's section 3.6 lists under Agents,
**loadable at any time**, which is what makes them a table rather than a
draft buffer. **A row is one saved version of a declaration**, written by
the declaration screen of section 6.1 and by nothing else, and it carries:

- **the declaration as authored, whole, in this console's own document**,
  the grammar section 6.1 states. It is a superset of the agent's grammar,
  `weaver-types-Spec` section 2, and **the agent's file is derived from it**
  at the corpus commit below, never stored beside it. **Better way, ruled
  by the Planner on 2026-11-11**: the row held the agent's grammar whole
  until then, and that grammar has no place for what the operator's
  rulings of 2026-11-11 put on the declaration screen, a GPU requirement, a
  further model function, or whether the agent is a host agent or a
  virtual-machine agent. So the console's document holds them, and what
  the agent's grammar has no place for is shown as such (section 6.1)
  rather than dropped
- **its identity, `de-` and sixteen lowercase hexadecimal digits**, minted
  by the server before the save's first audit record, as an agent's `ag-`
  is at register, so the record can name the row it is about to write
- **the agent's name**, in the grammar `register` holds a name to (section
  8), the declaration naming no box: a host agent's box is Provision's, and
  a virtual-machine agent's box is the machine the console will clone
- **the corpus commit its derived file was written against**, which is
  this row's staleness rule per section 7.2: when the agent's grammar
  moves, the pin says so, and the draft is named stale before it is offered
  for install. It is the WeaverAgent commit whose `weaver-types-Spec` the
  rendering of section 6.1 follows, a constant of the build, and not the
  row's version below. **The pin is WeaverAgent
  `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291` since the act of 2026-11-12**,
  moved from `a3543b31f264b269f66ab55d8767f25290ec9d61` when the agent
  began to require the sampler's six defaults at every serving load. A row
  saved under the earlier pin is named stale, and its reopen refuses its
  save until the eight tunable values of section 6.1 are filled, the
  Planner's default of 2026-11-12. No row is migrated, since the body is
  stored whole and no member of the schema moves
- **its parent, the row it was opened from**, none for the first save of a
  declaration, and **its lineage's root**, none for the root itself, so a
  lineage is identified by its root row
- **its version, the row's ordinal in its lineage**: 1 at the root and one
  more than its parent's at every later save, assigned by the store
- **when it was saved**, the store's clock at the save
- **what moved**: the field paths of section 6.1's document whose values
  differ from the parent's, computed by the server and never authored,
  empty at a root
- **the one thing that moved**, the member the charter's section 3.6 draws
  and the ablation matrix of section 2.9 reads, one parent's arms side by
  side: set to the one path where exactly one moved, and none otherwise
- the last answer `validate` gave, with when it was given, **meaningful only
  where the operator installed this row's declaration**: `validate` judges
  the declaration placed on the box and never a draft, per section 7.2. No
  act writes it yet
- **the author**, per section 3.2

**Every save appends a row, and a row's authored half never changes**, on
the Planner's default of 2026-11-11 following the operator's ruling 9 of
that date, that a wire edit is a new link in the lineage and not a reload
of the same agent; the operator's ruling on this is open (section 10). A
trigger refuses every update to a row's authored members and every delete,
the last `validate` reading being the one member a later act may write
onto a row. **A save is made from the row it was opened from**, and the
declaration screen refuses a save from a row that is no longer its
lineage's head, so the lineage stays linear in this act. **The refusal is
the handler's and not the schema's**: the schema admits a second child of
one parent, since the ablation matrix draws one parent's arms side by side
and the parent and the one thing that moved were chartered for that, and a
later act opens that door without a migration.

**Section 3.2's version is, for this table, the row's ordinal.** A row never
moves, so whether it has moved since it was read becomes whether its
lineage has moved, which the head check answers. It still has nothing to do
with the corpus commit: one says whether the derived file's shape is
current, the other where the row stands in its lineage.

**No two lineages share a name while both exist, and a lineage keeps its
name**: a save that would give a second lineage a name another lineage
holds is refused, and so is a save that changes its lineage's name, a new
name being a new declaration. Since no act retires a declaration yet,
"exist" is every row the table holds.

**Nothing is migrated.** The table was built by migration `0001` and, as of
2026-11-11, no code reads or writes it; its one reference is the staged
experiment's parent declaration of section 2.5. The act of that date adds
its members by an additive migration and moves no row.

**The last answer is a reading and not a verdict.** It is what the verb said
at a moment against a corpus commit, so a surface presents it with its date
and re-asks rather than treating it as current. **A declaration this crate
holds is not a declaration a box will load.** Admin stores none and takes
none in: its verbs read a configuration file the operator placed, and what a
box will load is that file, judged by `validate` and then by the SPU at
admission, per `weaver-admin-PRD` section 4.3. This table is where an
engineer keeps what they are composing, and the two agree only where the
operator has placed one.

### 2.5 The staged experiment

The claims the charter's section 3.5 registers and section 3.6 lists, in the
five states of section 5.1. Each row carries:

- the state, and when it last changed
- the parent run reference and the branch position
- the forced alternative token where one is forced
- the parent declaration and its diff, split by when the diff takes effect
- **the swept member and its values where the diff is a sweep**, one member
  of the tuple and the set of values it takes, per section 5.4
- **the question the engineer meant to ask**, which nothing upstream knows
  and nothing else in this store holds
- the runs it produced, where it ran
- **the author**, per section 3.2, which is who the question above was
  meant by
- **the row's version**, per section 3.2, frozen with the rest at
  registration

```graph
node: web-registered-question-is-immutable
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-web
to: web-registered-question-is-immutable
```

**Registration freezes the row and not the table.** A registered experiment
is immutable per section 9's pin, and the rows around it go on being
edited, which is why the state is a member rather than a table each.

**A sweep's value set freezes with the rest of the row.** Section 5.4 states
the ruling and argues it from the freeze above, and this section carries the
consequence rather than the argument.

```graph
node: web-registered-experiment-is-immutable
kind: assertion
tag: compile-pin

edge: asserts
from: weaver-web
to: web-registered-experiment-is-immutable
```

**An experiment that never ran keeps its row.** That is pre-registration
falling out of the interface rather than being imposed on it, per the
charter's section 3.5, and it is the one thing here that would be lost by
storing only what returned.

### 2.6 The recorded query

Section 4 admits an open query surface on the condition that the query is
recorded beside its result, and that condition needs somewhere to land.
Each recorded query is its own row under its own identifier, carrying:

- the query text as issued, verbatim
- the runs it addressed, each by identity
- the reader that served it, by name and version
- **the lens artifact it read through, where it read one, and the weights
  that lens was fitted to**
- when it was taken

**The result is not stored beside it.** A stored result would be a second
truth about positions section 2.1 already holds, and it would go stale the
moment a later ingest completed the run. The row stores what reruns the
query, and the rerun is what produces the result again - which is the whole
of what a quotable reading claims.

**An elected reuse is recorded here and is visible in the reading.** Where
the weights a lens was fitted to are not the run's own artifact, the
operator elected the reuse rather than the tool inferring it, per the
charter's section 5, and both identities stand in the row so a reader
downstream sees an election rather than a fit. The catalog resolves each of
them, which is what makes the election legible after the fact. **A reading
whose lens was fitted to other weights and does not say so is the one thing
this row exists to prevent.**

**The reproduction verdict is one of these rows and not a column on a run.** Section 10
has it as the projected comparison of two rows this crate holds, on the fields the
charter's section 4 names, so it is a property of two runs and of which two a reader
chose, which is the shape section 2.7's exception exists for and the shape a sweep's
similarity already has. **A surface that renders it renders a recorded query's rerun**,
naming both runs, and a matrix reading it names the parent and the arm. Storing
it on a run would be the second truth this section refuses, and would go stale the
moment either run was re-ingested.

**A recorded query serving a reproduction verdict names exactly two runs**, which is
narrower than the rule above and is this reading's own. Section 10 has the verdict as
the comparison of two rows, so a query naming one has nothing to compare and a query
naming three has not said which comparison it made, and neither produces the one verdict
the reading claims. **The cardinality is the verdict's and not this section's**: a
sweep's similarity is recorded here too and names as many runs as the sweep had arms.

**A query that cannot name every run it addressed is not recorded and not
quotable.** Section 4's condition is that a second person can rerun it, and
a reader that cannot say what it read cannot be rerun by anyone.

```graph
node: web-recorded-query-names-every-run
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-recorded-query-names-every-run
```

### 2.7 The indexes

```text
primary       (run, turn, position)
secondary     (run, surprisal)
artifact      run (record_identity)
family        run (record_session)
lineage       run (parent_run_id)
ingest order  run (ingested_at DESC, run_id DESC)
plan roots    plan (parent_run_id)
ref roots     ref (run_id)
queued roots  staged_experiment (parent_run_id)
```

**The last three carry section 2.10's reachability**, whose roots are a ref, a plan's
parent run, and the parent run of a staged experiment that has not returned. A root that
had to be found by a walk is a root a sweep would skip rather than honor.

**The lineage index is not the third of them and this section said it was.** That one is
on the run's own parent and answers a branch's siblings; the third root is the staged
experiment's parent, which had no index, a foreign key constraining without indexing.
**That is the finding of PR #540 made twice** - once against the lineage reference in
0007, and once here by the review of PR #547, in the same act that cited the first as
settled.

The secondary index exists so the largest spikes in a run are reachable without pulling
the run down. The family index exists so a session's runs are one read rather than a
walk up parent references, per section 2.2, the session being the identity the trace's
runs share.

**Four of these carry section 4's fifth read - the artifact, the family, the lineage and
the ingest order - and this list named one of the four until 2026-09-11.** The artifact
index has stood since the first migration and was missing here. The lineage index is
new: a foreign key constrains and does not index, so a branch's siblings were a
sequential scan under a clause that called them an index hit.
The ingest order is new and **carries the run's identity beside the clock because the
clock is not a total order**: `ingested_at` defaults to the transaction's, so every run
of one ingest shares a value and a page keyed on it alone drops the rest of a tie. Found
by the review of PR #540. **Nothing is computed at
read time except where the query is recorded.** A value that must be
derived is derived once at ingest and stored, because a value computed in
the interface is a value nobody else can reproduce.

**The exception is section 4's and it is not new**, only made live by the
fourth read of 2026-09-08. Section 4 admits an open query on the condition
that the query is recorded beside its result, and says in its own words that
**what this section forbids is a derivation nobody can find**. A sweep's
similarity is that case exactly: it is a property of two runs and of which
one a reader chose, so it can be derived neither here nor at the ingest, and
what makes it reproducible is section 2.6 holding the query rather than this
section holding the value. **Section 2.6 stores no result**, so nothing
computed under the exception becomes a second truth about a run.

```graph
node: web-nothing-is-computed-at-read-time-unless-the-query-is-recorded
kind: assertion
tag: review

edge: asserts
from: weaver-web
to: web-nothing-is-computed-at-read-time-unless-the-query-is-recorded
```

### 2.8 The session

**Admitted on the same ground section 2.6 was: a rule stated elsewhere needs
somewhere to land.** Section 3.2 gives every authored row a nullable author,
and **HTTP carries no identity between requests**, so a surface can only know
who asks if something holds it, and that something is this row.

**A session is a person's**, as act 11 built it on 2026-10-08 to its design
(`docs/project/design-2026-10-07-iam.md` section 6): it is opened at sign-in
by one of the two methods section 2.13 names and the server's config admits,
a passkey of the person's or, since 2026-11-10, a sign-in token the host
issued them, and carries the person, the method that opened it and, where a
passkey opened it, that passkey. It is what a surface reads to know who asks,
and what an authored row's author member takes its value from.

```graph
node: web-session-carries-a-person
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-session-carries-a-person
```

Each row carries:

- the session's own identifier
- **the bearer's digest and never the bearer.** The value the browser holds
  is hashed before it is stored and the lookup is on the digest, so a read of
  this table is not a set of live sessions
- **the person**, whose identity section 3.2's author member takes
- **the method that opened it**, `passkey assertion` or `sign-in token`, the
  audit's own names for the two (section 2.13), so the session and the
  record of its opening say the same thing
- **the passkey it was opened with, where a passkey opened it**, by the
  passkey's own identity (`pk-`, drawn at random and never reused) and not
  its credential ID, and with no foreign key: removing a passkey deletes its
  row, which a key would refuse or cascade into the session, and the session
  sees the absence at its next use; the same credential re-enrolled after a
  host reset is a new row with a new identity, so it revives no session
  opened before the reset. **A session a sign-in token opened names no
  passkey**, and the store refuses a row whose method and passkey disagree:
  a passkey's method with no passkey, or a token's with one
- when it opened, when it was last used, and when it closed where it has

**The digest rule was stated before there was a token to migrate.** The
prior schema stored the bearer in the clear and the fix would have wanted a
migration and an invalidation path for everything already issued, per
issue #336. That table retired with the conversation half at PR #499, so this
document stated the shape while it was free, on the same reasoning section
3.2's author member was landed under: the cheap moment is before the first
row exists.

```graph
node: web-bearer-is-stored-as-a-digest
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-bearer-is-stored-as-a-digest
```

**The cookie is `__Host-weaver_session`**: `Secure`, `HttpOnly`,
`SameSite=Strict` and `Path=/` with no `Domain`, the prefix making a browser
refuse it otherwise, so no script reads it, no other site's request carries
it, and no other host can set it. The bearer is drawn under section 2.13's
rule for every bearer the server issues. **The one name serves every origin
the server may run on**: measured on 2026-10-08, Chromium 153 and Firefox 156
keep a `__Host-` `Secure` cookie on `http://localhost`, which they treat as a
secure context, and drop it on a plain origin elsewhere, which section 2.13's
scheme rule already refuses. A cookie under the claimed-name session's old
name, `weaver_session`, names nothing.

**Each end is checked at every use**, in one read on the database's clock:
the session closed, its person disabled, the passkey it was opened with
removed where a passkey opened it (the host reset among the ways, clearing
the person's passkeys), open past the absolute limit, or unused past the
idle limit. **A session a sign-in token opened has no passkey to lose**: it
ends at sign-out, at either limit or at its person's disable, and a
passkey's removal or the host reset leaves it standing, as neither changes
what opened it. **The limits are
twelve hours open and one hour idle**, the server's config
(`session_absolute_secs`, `session_idle_secs`) with those defaults, **the
absolute limit never past seven days** (`SESSION_ABSOLUTE_MAX_SECS`, refused
at load past it), since an absolute limit longer than a week stops being one
and the bound keeps every limit an interval the database can represent; the
idle limit is never past the absolute, so it is bounded too. The use that
finds a session ended closes its row, **the close restating the reason it
found in its own condition**, so a close decided on a read another request
has since overtaken, a refresh above all, moves no row; the use then reads
the session once more and serves or refuses it on that read. Sign-out closes
unconditionally, the person having asked. So **no end needs a write to
every session**, and **nothing but the surface writes a session**: a
disable, a passkey's removal and the host reset change the person or the
passkey, and the session sees it at its next use. An open live view counts
as use while it streams and is re-checked every 15 seconds, per section 2.13.
**Its re-check refreshes the session's last use, by the operator's ruling in
the design's section 6** (`docs/project/design-2026-10-07-iam.md`), so a
person watching a long run is not signed out mid-run; the twelve-hour limit
and sign-out still bound an unattended view.

**The last use is written at most once a minute**: one conditional update on
the database's clock, `last_used_at = now()` where the stored time is a
minute old or more, so concurrent uses refresh it once, the writes stay
bounded whatever a page polls, and it never moves backwards. **Idle expiry is
honoured to within a minute**, which an hour's limit needs no finer than, and
**the idle limit is never under five minutes** (`SESSION_IDLE_FLOOR_SECS`,
refused at load below it): a limit near the refresh's one-minute grain would
end a session in active use, so it must be several times that grain, and
five minutes is that with margin.

**Every request that changes state carries an `Origin` equal to the
configured origin**, exactly one such header, or is refused before its
handler: every method but `GET` and `HEAD`, over the whole app, the
fallback included, beside `SameSite=Strict`. **With no origin configured,
nothing that changes state is served**, since nothing can sign in without
one. **Sign-out** is a `POST` that closes the session's row, clears the
cookie and sends the browser to the sign-in page. **A signed-in page's
navigation** links to Record and to the person's own passkeys and carries
the sign-out as a plain form; a page without a session links to sign-in.
**`GET /` sends the browser on**, as built on 2026-11-10: to `/sign-in` where
the request names no session that serves, and to Record where it names one,
Record being where every sign-in lands. It reads the session as any use
does, and is a redirect (`303 See Other`) and never a page of its own.

**Sign-in by passkey is name-first**, as built on 2026-10-09 (`/sign-in`): the person
gives their name, found by its canonical form, and answers a challenge for
their own passkeys alone; an unknown name, a disabled person and one holding
no passkey get one answer, the name's existence being all a name-first
sign-in tells. **The ceremony records the passkeys it challenged, by
credential ID to their own `pk-` identities**, and the finish uses that
mapping alone: the counter rule reads and writes the passkey by its
identity, the session opens naming it, and every audit of the attempt names
it. A passkey removed between the start and the finish (a reset) refuses the
sign-in as removed, a possible clone still audited against the challenged
identity; a credential re-enrolled meanwhile is a new row under a new
identity the ceremony never challenged, so nothing is counted or opened on
it. **The library verifies the assertion**, and a failed
signature is refused unaudited, proving no principal. **Then the counter
rule, in a transaction of its own committed before the session's opening**:
the stored passkey read under its row's lock, a nonzero returned counter not
greater than that fresh copy's refused as a possible cloned credential
(audited, the person by `passkey assertion`, nothing opened, the passkey left
standing), otherwise the assertion merged into the fresh copy and written
back, always where the returned counter is zero. **Then the session's
opening, in a transaction of its own**: the person checked again as enabled
and the passkey as standing under share locks, a bearer drawn under section
2.13's rule and stored as its digest, the session naming the person and the
passkey's `pk-` identity, the `__Host-` cookie set, audited by `passkey
assertion` on the passkey it began with, its first record before the write. A surface that finds no
session links to `/sign-in`. **A ceremony's state is held in the server's memory**, in the ceremony table
built with enrollment on 2026-10-08 (`src/passkeys.rs`): keyed by a ceremony
identity drawn as every bearer the server issues is, for at most five
minutes, used once, **with at most 64 ceremonies in flight**, a new one
beyond that refused, since a ceremony starts before anyone is authenticated
and an unbounded map would be a crash. A restart drops what is in flight.

**Sign-in by token**, as built on 2026-11-10 (`POST /sign-in/token`), is
served only where the config's `sign_in` names `token`, and is otherwise no
route at all: the sign-in page then carries a second pane, a plain form
taking the person's name and the sign-in token the host printed for them,
which needs no script. **One answer refuses every token that cannot sign
in**, unaudited, whether the token is unknown, expired, ended or another
person's, the name's canonical form not the token's person's, or the person
disabled: a wrong token proves no principal, as a failed signature does not,
and auditing it would let anyone fill the audit. **Then the session's
opening, in a transaction of its own**, its first record written before it
(the person, by `sign-in token`, on the person, the action `session open`):
the token ended `redeemed` by one conditional update that matches only a
token still live and unexpired, so of two redemptions at once one opens a
session and the other is refused; the person checked again as enabled under
a share lock and the name again as theirs; a bearer drawn under section
2.13's rule and stored as its digest; and the session naming the person,
the method `sign-in token` and no passkey, its outcome committed with it.
The browser is sent to Record (`303 See Other`) with the `__Host-` cookie
set. **The session it opens is an ordinary session** in every other way:
the `Origin` check, the cookie, both limits, the grants and the audit of
everything done in it hold as for one a passkey opened.

**A session is not an authored row and takes no version.** Section 3.2's
ordering rule answers two engineers editing one declaration, and nobody edits
a session: its one writer, the surface, opens it once and closes it once, and
between the two writes only its last-used time, at most once a minute under
section 3's writer model, which no two writers can race. **Nor is it a
recorded row**, carrying no run and no position, which is why it stands here
after the indexes rather than in either half.

### 2.9 The plan

**A run row says what a run did and a staged experiment says what was registered, and
the matrix of `docs/project/sketch-ablation-matrix.md` needs a row for what an operator
is still composing.** One parent run, a set of arms, and a disposition for every
member of the tuple in every arm. **An arm that is never registered leaves no
staged experiment**, and without this row it would leave nothing at all, which is the
ablation's own throwaway problem read one level up: a plan an operator trimmed is a fact
about what was considered.

Each plan carries:

- **its identity**, spelled `pl-` per section 2's convention
- the parent run it ablates, which every arm branches from
- **the author**, per section 3.2, and **the row's version**, being an authored row like
  the three above it
- its arms, **each with an identity spelled `ar-` and a name the operator gives and may
  change**, and each of which becomes at most one staged experiment and holds a nullable
  reference to the one it became

Each entry of a plan carries:

- **its arm's identity**, which with the member below is its address, and not the plan's
  identity paired with the arm's name, since a name the operator relabels would carry a
  rename into every entry beneath it
- the member of the tuple it names
- **its disposition**, which is held or freed and is the entry's own, per the sketch's
  section 5
- the value where it is held, and the value set where it is freed, which registration
  freezes as section 5.4's sweep

**An entry carries a disposition and an arm carries a status, and the two are
different facts about different objects.** An entry that carried both could not say what
a freed entry in a queued arm is, which is both at once. The disposition is above and
the status is section 5.1's, per the paragraph this act adds there.

**An arm reaches its runs through its staged experiment and never directly**, per
section 2.5, which carries the runs it produced, one per value where the arm freed a
member. So an arm is never one run row.

**An arm frees at most one member and holds the rest, and an arm that frees none is
not a sweep.** Section 5.4 has a sweep name one member and its value set, so one freed
member is the ceiling and the staged experiment an arm registers carries it as the
swept member. **An arm that frees none registers a staged experiment with no swept
member and produces one run**, which is the ordinary point experiment and not a sweep of
one value, a phrase this section carried until the review of PR #534 and which read a
sweep onto a row that has none. **How many members an arm holds away from the parent
is not bounded here**: one is section 6's generated default and the matrix's interaction
arms move two or more on purpose, and section 5.2 says what a compound arm's
divergence is attributable to.

**An arm that moves no member at all is refused at authoring**, per section 5.3's
refusal of a sweep naming the parent's own value, read one level up: such an arm
declares the run the parent already is, and section 5.4 has the parent as the control
rather than as an arm.

**Declared and achieved are two facts landing at two times.** The entry carries what was
intended and keeps carrying it. The run row carries what ran, and where the two disagree
**the run row is right and the entry is a plan that did not survive contact**, which is
a reading worth having rather than a row to correct.

```graph
node: web-an-authored-identity-says-what-it-addresses
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-an-authored-identity-says-what-it-addresses

node: web-an-arm-registers-at-most-once
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-an-arm-registers-at-most-once

node: web-an-arm-frees-at-most-one-member
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-an-arm-frees-at-most-one-member

node: web-entry-states-the-value-its-disposition-names
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-entry-states-the-value-its-disposition-names
```

**Two of these three are the schema's and one is the write's**, which is why they land
at two acts. The bound on freed members and the entry's value are properties of a row,
so migration 0009 holds them and its watches perturb them. **Registering at most once is
a property of a transaction** - section 5.1 has the reference set only where it was null
and in one commit with the staged experiment - so the constraint 0009 builds is what
that write leans on and the record is claimed at the act that writes.

### 2.10 The refs

**A named reference from a person to a run, and the one thing this document takes from a
commit graph.** The ablation's runs are scaffolding by default: a batch that establishes
a bound produces arms whose value was to be measured once, and the schema should make
discarding them cheap rather than guard against it. **What must survive is what the
bound cites**, and a ref is how a run says so.

Each ref carries:

- **its identity**, spelled `rf-` per section 2's convention
- its name, which is the operator's word for why this run is kept
- the run it pins
- **the author**, per section 3.2, and **the row's version**
- when it was made

**A run reachable from no root is scaffolding**, which is what a sweep of the store may
discard, and the question of what to keep reduces to reachability rather than to
judgment exercised twice. An exemplar tag is a ref. A citation from a published bound is
a ref. **Nothing in this document deletes a run**, and this row exists so that a later
act which does can tell what it is deleting.

**A ref is not the only root, and reading it as the only one would discard work in
flight.** A plan names a parent run and a staged experiment names one too, and neither
is a ref, so a sweep rooted on refs alone would collect the parent of a plan an operator
is composing or of an arm already queued, and the queue would drain against a run that
had gone. **The roots are three**: a ref, the parent run of any plan, and the parent run
of any staged experiment that has not returned, per section 5.1's five. A plan the
operator discards stops being a root by being discarded, which is the same reachability
answering both questions.

**A ref names a run and never a plan or an arm.** A bound cites what ran, and an arm
that was registered and never queued has nothing to pin, which section 5.1's
registration already records without a ref's help.

### 2.11 The batch

**Registering freezes an arm and queueing runs it, and the order it runs in is
neither the plan's nor the runner's.** Section 5.1 already has those as two acts.
This row is what the second one writes: an ordered list of staged experiments handed
to a runner together.

Each batch carries:

- **the author**, per section 3.2, and **the row's version**
- when it was composed

Each entry of a batch carries:

- its batch and its **position**, which are its address together
- the staged experiment it queues

**The order is authored and is not a property of the plan.** Section 2.9's arms are
generated from the closed tuple space rather than placed by an operator, so their
order carries no intent and a batch that ran them in it would be running them in the
generator's order. The operator's intent arrives when they choose what to run, which
is queueing, so that is where the order is recorded. **Ruled 2026-09-11.**

**A batch spans more than one parent run.** A staged experiment carries its own
parent per section 2.5, so a set of them may come from several plans and several
agents, and nothing here constrains them to one. That is the whole reason this is a
row rather than a flag on a plan.

**So a batch is a scheduling unit and not a comparison unit.** Section 5.4 has the
parent as the control arm for its own matrix, and a batch holding arms of two parents
holds two controls. A reader who compared across a batch would be reading two
baselines as one, which is the error this paragraph exists to forbid rather than to
repair later.

**The position is a member and not a timestamp.** A clock looks sufficient and is
not: `now()` is the transaction's, so arms queued in one gesture share a value and
carry no order at all, which is the same tie section 2.7's ingest order was rebuilt
around after PR #540. A position that a re-queue does not disturb is a number the
author supplies.

**The word is taken and this row does not take it back.** The sketch's section 6 uses
"batch composition" for the co-resident sequences a run is drawn alongside, which is a
member of the tuple and not this. **Nothing here is that**, and a later act that finds
the collision costly renames this row rather than that field, the tuple's vocabulary
being the older of the two.

```graph
node: web-a-batch-orders-what-it-queues
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-a-batch-orders-what-it-queues
```

**This row has no migration yet.** It is stated here because section 5.1's two acts
already implied it and nothing wrote it down, and because the interface that queues is
act 4's and act 5's at issue #434. The act that builds the queueing gesture builds the
table, and its assertion above gets its instrument there.

### 2.12 The registered agent

**The register of agents, which is not what section 1's tree calls the
registry.** That entry names the whole store, every table and the reads over
them, and the word stood there before any agent was registered here. This
section is the one table that holds agents, and the operator's word for it on
2026-10-01 was the registry too, so the two readings are told apart here
rather than left to collide: the store is the registry, this row is the
register of agents, and section 8's verbs are the register verbs.

**It exists because the server serves many agents at once and must tell them
apart positively.** Per the operator's ruling of 2026-10-01, a row says which
agent this is, where it runs and what it declared, and the two credentials of
section 8 are bound to it, so that a connection is an agent's connection and
never a bare socket's. Each row carries:

- **the agent's identity, `ag-` and sixteen hex**, generated here under the
  convention this section opens with, since the row is authored here and the
  name is not the key, for the reason section 2.9's arm gives
- **the agent's name**, the name admin knows it by on its box, which is what
  every verb of section 7.2 takes
- **the box it reports from**, as registered, **and the address the server
  observed** its connectors connecting from, with its date. The two are kept
  apart because one is a claim the operator made at registration and the
  other is a fact the listener saw
- **the declared tuple as the agent's side reported it, and when**, kept
  exactly as its source gave it: the `load` member of a `show` answer,
  which is admin's word, or the payload of the load event the trace
  carries, which is the agent's own record, per section 7.2. **It is
  opaque**: the two sources give different shapes and stay so, per
  `toddwbucy/WeaverAgent#59`, so the row's shape differs by source, which
  the tuple's source member below records (migration `0012`), and nothing compares
  it. **The run's tuple of section 2.2 is the compared one**, written only
  by ingest, and the two are not the same thing. Nothing on the data plane
  may fill it, per section 8
- **the two credentials**, each as the fingerprint of its client certificate
  and never the key, each with its plane, gate or admin, its state, live or
  revoked, and the date the state was set. The posture is section 2.8's
  digest rule applied to a certificate: a read of this table is not a set of
  credentials anyone can present
- **each connector's link state**, connected or not, with the date it last
  changed, which is what a surface renders where the agent is not present:
  the plane that is missing, per section 8. **It is the listener's word
  about its own connections and is therefore reset when the listener
  starts**: a persisted flag outlives the process that set it, so a server
  that died with both planes connected would otherwise restart reading both
  connected with no socket behind either, and the heartbeat cannot clear
  what no connection holds. Section 8 states the reset, and it binds every
  link-state write to the connection it describes, so a stale teardown
  cannot mark a plane missing after its replacement was admitted
- **the trace door's state**, open or closed, with the date it last
  changed: admin-con's word at admission, in its hello, and at every change
  after, in a frame on the live connection, each write bound to the
  connection that reports it like a link-state write, per section 7.2. The relay runs
  only while the agent runs, so a closed door beside an `unloaded` load
  state is the normal state, and a closed door while a run stands is shown
  for the operator to read and is never a fault this crate raises. The door
  never gates a verb. **Trace events the agent writes while the door is
  closed write no member of the row**: they are replayed at the next
  opening, per section 7.2. Verb answers during that interval land at
  receipt, as section 7.2 says, so a `show` asked while the door is closed
  still refreshes the tuple and the load state. Its columns are written at
  admission from the hello and at every change from admin-con's `door`
  frame (migration `0013`)
- **the load state, with its date**, which is section 7.2's rule restated at
  the row: the state is what `show` last answered, admin's word, or what the
  trace last carried, the agent's own record (a load, an unload, or a turn's
  start as `active` and its close as `idle`), never an inference from a
  socket's existence, and never fresher than its date says. **An unclean
  stop writes no `unload`**, and the trace writer's queued tail can be lost
  with it, per `toddwbucy/WeaverAgent#59`, so a state from an event can
  outlive the process: a turn's start and close keep it no older than the
  agent's last turn, and the full answer, the reset event of
  `toddwbucy/WeaverAgent#58`, is owed until that lands
- **which source each of the tuple and the load state stands on**, one
  source per member, a `show` answer or a live trace event, each dated with
  its member. The load state's source lets a surface say a state is
  unconfirmed since the last admission where the agent's ceiling grants no
  `show`, per section 7.2. **The tuple's source says which shape the opaque
  tuple has**, so it moves only with the tuple: a turn's start or close
  moves the load state and its source and leaves the tuple and its source
  as they were (migration `0012`)
- **the ceiling admin-con declared**, the verbs the box's sudo rules grant
  its user as its hello named them, with the date, per section 8. It is the box's word
  about itself and an upper bound, never a grant, and this copy is what
  surfaces read: the server authorizes against the live connection's own,
  per section 8
- **the run's constituents as the last `show` named them**, by process id,
  with that answer's date, per section 8: a fact for the operator and for
  the install's one-time check after the first load (section 8.1), which drives no behaviour of
  admin-con's. A `show` that names none writes none, so the member reads
  null where no run holds the agent's run lock, dated still (migration
  `0013`)
- **a transition standing beside the last state**, the date a `show`
  answering `in_transition` was seen (migration `0021`), its constituents
  in the member above. **`in_transition` claims no state**: the load state,
  its source, the tuple and its source are kept as they were, and the
  transition is recorded beside them; **only a lifecycle state ends it**, a
  `show` answering `state` or a live load or unload event. A turn's start
  or close moves the load state and keeps the transition, neither set nor
  cleared: it says where the turn stands, not that the invocation holding
  the agent ended, a graceful unload waiting for a turn (S3) among them. A transition
  takes the load's arrival sequence and leaves its state, so it stands only
  over states older than itself and the next state, newer by the same
  sequence, ends it. The surfaces say it as it is: with no constituents,
  another command is in progress; with them, the processes still stand, a
  run starting or ending or an ended run's processes still being ended,
  which the wire does not tell apart, so the page names both and picks
  neither, and never says loading. **A transition raises no alert**: it is
  state and not a failure, and a stuck one is the operator's to judge from
  the page and the trace

**It is an authored row and takes section 3.2's members.** Registration is a
write the operator makes, so the row carries the author member and the
store's version like sections 2.3, 2.4, 2.5, 2.9 and 2.10, and an edit to
what registration authored is ordered on the version like any edit. **The
edits registration permits are the box and a credential's state, and never
the name**: admin knows the agent by that name on its box and both
connectors' configs carry it, so a renamed row would refuse its own
connectors at the hello of section 8, and a different name is a different
registration. **At most one row per box and name holds live credentials**,
held at the schema by a partial unique index over the rows whose
credentials are live, so one physical agent cannot be registered twice into
two rows with independent credentials; re-registering a live pair retires
the previous row by revoking its credentials in the same act. **A row
holding no live credential is retired, and never live again**: an admin
retires one through the server by revoking every live plane in one write,
**a rotation refuses one, the host's `rotate` as the web's**, read under the
row's lock inside the rotation's transaction, and the agent comes back only
as a new registration, a new row with a new identity. The host's recovery
of a replacement row's staged pair, asked by the retired row's identity,
publishes a pair already committed to the replacement and revives nothing.

```graph
node: web-one-live-row-per-box-and-name
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-one-live-row-per-box-and-name
```

**What the link observes is not an edit.** The observed address, the link states, the ceiling, the tuple, the load state
and the source those two stand on are facts the listener and the agent's side reported, and the link writes them as it
learns them, each with its own date, so a reader can tell when the server
last knew each one. **Observations are ordered on the server's own arrival
sequence and never on their source date.** The sequence has two members
ordered lexicographically: the listener's epoch, a counter the server
persists and increments once at every listener start, and the arrival
number within that epoch, which the listener assigns to every observation
in the order it arrived over the link. A member takes the observation with
the higher sequence, and an observation from a later epoch always orders
above one from an earlier, so a restarted server's first `show` answer
supersedes whatever the last process left and nothing persisted is reset.
A process-local counter alone would begin below the sequences already on
the row after a restart and reject every new observation until it caught
up. The epoch is the server's own and is written once per start, in the
same act as the startup reset of section 8. The source date stays on the
row for display and decides nothing, because the server's own epoch and
counter cannot step backward or collide and a date from the box can do
both. **A gap named 2026-10-01 by the act that built the listener**: a
trace event carries its own time, the harness's `wall_ms` on a clock that
is not monotonic, and the row takes it as the event's date, but admin's
`show` answers carry none, so for those the date stored is the server's
receipt and not admin's until the answer carries one, which
`toddwbucy/WeaverAgent#52` asks. **A replayed event never writes a
member of this row at all**: every event admin-con relays from behind the
file's tail, at a reconnection or a backfill, is marked as replayed on the
link per section 7.2 and feeds the server's live window and nothing else.
The tuple and the load state are written only by `show` answers and by
events relayed live, which the arrival sequence orders, and the arrival
sequence carries the source's own order because admin-con sends
everything on its connection as one ordered stream, per section 7.2, so a
`show` answer can never overtake a lifecycle event that followed it. A rule
that
only kept replayed events behind what this process observed would still
let a backfilled load event after a restart overwrite the state a newer
`show` answer had set before it, which is why the rule is absolute rather
than ordered. **And on every admission of an admin-con connection whose
door is open, and at every later opening of the door, the server first
fixes the replay boundary and then, where the agent's ceiling grants
`show`, asks `show` for that row**, per section 7.2's rule for the door. An
admission whose door is closed carries no boundary and is admitted with the
replay ended at once; where the ceiling grants `show` it is still asked,
and its answer lands at receipt. Where the door is open:
the boundary is the file position admin-con reports as its tail at that
moment, every event beyond it is live and every event behind it is
replayed, "beyond" and "behind" read as section 7.2's sentence on a
position fixes them, and `show`'s answer, taken after the boundary, is admin's word
from after it, so a change before the boundary is in the answer and a
change after it arrives as a live event. Asked before the boundary, a load
or unload landing between the answer and the boundary would be classified
replayed and could never reach the row. So where `show` is granted the
row's tuple and load state are admin's current word at the moment the
window reopens rather than whatever the last process left; where it is not,
they stand on the last live event, and the row names that source and its
date until the next one arrives. Section 8's startup reset is excepted from the ordering: it
is the listener's own act on its own state and lands unconditionally. They are observations rather than the operator's
authorship, which is why each carries its date in the row rather than riding
the row's version.

**The row holds no key material and no path of the agent's own.** The server
holds fingerprints and the agent's box holds keys, per section 8, and the box
is named by the name the operator registered rather than by a host or a path
the repository would have to carry, which is the publish boundary every
document here stands under.

### 2.13 Identity and access

**Who may act on an agent, chartered 2026-10-02 at the level the store needs
and without electing the mechanism.** Per the operator's rulings of that
date every verb asked of a registered agent passes three gates, section 8
names them, and the first is this crate's: a person authenticated to the
server, holding a role on the agent, may ask a verb or place a turn that
role permits. **A turn is an action like a verb**: it prompts the agent, and
an agent with tools can act on its box, so placing one needs a grant as much
as `stop` does; it passes this section's gate and then the gate's own
admission, per section 8. The register verbs act on the server and not on an agent, and take the path the
last item below states. How people authenticate and what
the roles are called were section 10's open elections, and the operator
closed both on 2026-10-07: **people authenticate by passkey (WebAuthn) and
by nothing else**, and the roles are the ones the item below names. **The
operator relaxed the first for the pre-release stage on 2026-11-10**: a
sign-in token the host issues is a second method where the server's config
admits it, and the passkey stays the method a config naming none admits;
the sign-in token's item below states the ruling, its reasons and what
release decides. **This
section charters identity and access at the level the store needs, and no
further**: the act that builds it, act 11, owed its full design before any
code, including a threat model of the person, the session, enrollment and
recovery. That design is `docs/project/design-2026-10-07-iam.md`, which
holds the threat model and the measurements the choices rest on; the rules
it decides are stated here and in section 2.8, and the reasons are there.

- **The person**: an identity authenticated to the server, whose session,
  section 2.8, carries the person, the method that opened it and, where a
  passkey opened it, that passkey. A person row
  carries its own identity under the convention section 2 opens with, the
  name it is known by, and whatever the authentication mechanism keeps,
  never a secret in the clear, which is section 2.8's digest rule carried to
  the person. Section 3.2's author member takes its value from the
  session's person's identity, the surfaces rendering the person's current
  name. **The person authenticates by a passkey, or by a sign-in token where
  the server's config admits one, and by nothing else**:
  no password, no second factor and no external identity provider, the
  relying party's identity and origin coming from the server's config and
  never from the repository. **The browser's listener serves TLS before any
  passkey is enrolled**, WebAuthn running only in a secure context and a
  relying party's identity being a domain and never an address, and **the
  server refuses to start, where passkeys are on, on the faults the design's
  section 3 lists and on nothing beyond them**: the relying party
  missing or an address, or not its domain's ASCII serialization (refused
  rather than normalized, as the origin is, naming the form to configure),
  an origin that is not a serialized origin, not
  `https` (bar `http` on `localhost`) or not under the relying party, a
  certificate and key that are missing, do not load or do not pair, or a
  certificate not valid for the origin's host or not valid at the clock at
  start, nothing being promised about its expiry while the server runs. Any
  other fault of the configuration, a public suffix as the relying party's
  identity or a certificate unfit for server authentication among them,
  shows at the first connection or ceremony as the browser's failure, which
  the server logs. **The listener's own refusals, the origin's and the
  certificate's, hold wherever an origin or a certificate is configured**,
  passkeys on or not: with a certificate and key the listener serves TLS
  alone and no plain listener beside it, and without them plain HTTP, which
  only an absent origin or `http://localhost` admits; a certificate with no
  origin has its pair and its period checked, its name check waiting for an
  origin. **The TLS handshakes in flight are capped at 256**, each bounded at
  ten seconds, and a connection past the cap waits in the kernel's backlog
  for a permit rather than in memory, so the memory held before a byte is
  authenticated is bounded whatever a client opens; denial of service stays
  out of scope, as with the ceremonies' cap. **A handshake a client ends
  with a certificate alert is logged at warn**, naming the alert, being the
  browser's refusal the design promises the server logs, and at most once a
  minute with a count of those not logged, since any client can send one;
  every other failed handshake is a scanner's noise, logged at debug. **The
  origin is parsed by the WHATWG URL Standard**, as a browser parses one,
  and must equal its origin's ASCII serialization, the browser's own
  algorithm, so no rule of the standard is approximated by hand. HSTS and a
  redirect from plain to TLS are not built. The ceremonies are the library the design's section 2 measured,
  `webauthn-rs` 0.5, **carried by the server binary alone**: the `passkeys`
  cargo feature, on by default, carries it and its OpenSSL, the server binary
  requires it, and the connectors are built for agent boxes without it
  (`--no-default-features`), their dependency graph then holding no
  `openssl-sys` at all; measured on 2026-10-08, `ldd` finds no `libcrypto` in
  either connector under that build or the default one, where the linker
  drops the library the connectors never call. The browser's half is one
  vendored module doing the two ceremonies and nothing else, under 4 KiB and
  served from the surfaces' own asset route, and **the surfaces answer with
  `Content-Security-Policy: script-src 'self'`**, carrying no inline script
  or inline handler, every script a file of the binary's (htmx, the passkey
  module and the trace module of the live view below);
  measured on 2026-10-08, htmx's own `hx-get` and `hx-post` hold under it in
  Chromium 153 and Firefox 156, its evaluating attributes (`hx-on`) being
  what no surface may use while it stands. **A person's name is
  unique among persons in one canonical form**, Unicode's compatibility
  caseless form of the name trimmed of surrounding white space, since
  sign-in finds the person by it: checked at enrollment and at rename under
  the exclusion below, and a name whose form another person's already has is
  refused. **A person is named by their identity only in its whole shape**,
  `pe-` and sixteen hex, and by name otherwise, and **no name takes that
  shape**, so no name shadows the person whose identity it spells. **A credential ID belongs to one passkey of one person**: unique
  across every person's passkeys in the store, and a registration whose ID is
  already held, by anyone, the registering person included, is refused
  atomically at its insert. **A counter that did not rise refuses the
  assertion, for every assertion the server verifies**, the sign-in, the
  fresh assertion that authorizes adding a passkey, and any later one alike:
  where the returned signature counter or the stored one is nonzero, a
  returned one not greater than the stored one authorizes nothing, opening
  no session and adding no passkey, and is audited as a possible cloned
  credential, the passkey staying enrolled so a clone cannot lock its owner
  out. **After every assertion the stored passkey is updated by one locked
  read, merge and write**, in a transaction of its own that commits before
  the authorized action's begins, an action that then fails leaving it
  updated, since the authenticator did advance: the stored passkey is read
  under its row's lock; where the returned counter is nonzero and not
  greater than that fresh copy's, the assertion is refused, which is the
  refusal of a concurrent use; otherwise the library's update of the
  credential (its counter, backup state and backup eligibility) is applied
  to that fresh copy and written back, always where the returned counter is
  zero, as synced passkeys report, two concurrent assertions of such a
  passkey both succeeding. So concurrent assertions serialize on the row,
  each merging into the other's result, and none erases another's update. **A person may hold
  several passkeys**: adding one takes a fresh assertion with a passkey the
  person already holds, so a session alone never adds one. **The assertion
  and the registration are two ceremonies, bound by a one-time add grant**:
  the verified assertion yields a grant held with the ceremonies, bound to
  that session and that person, expiring within the ceremony's five minutes
  and counting toward its cap, and the registration requires and consumes it
  at its start, a failed registration consuming it too, so one assertion
  serves one registration from the session that made it. A person removes
  any of their own passkeys but the last, the count taken under the
  exclusion below so two concurrent removals of the last two cannot both
  land, and every session opened with the removed passkey ends at its next
  use. **As built on 2026-10-09** (`/passkeys`): the page lists the
  session's person's own passkeys alone, which one opened this session, and
  their labels; the fresh assertion records its challenged passkeys by
  their own identities as sign-in does, falls under the counter rule, and
  is audited by `passkey assertion` on the asserting passkey; the addition
  is audited by `passkey assertion` on the new passkey, its insert under the
  exclusion refusing a credential ID held by anyone; a removal is audited
  by `session`. **The person row is authored
  through section 3.2, and its writes split three ways**: enrolling,
  disabling, enabling again or renaming a person is written by a holder of
  the server-wide admin grant, an enable under the same rules as a disable;
  the bootstrap person, the first admin, is written by the host
  principal below; and a person may write only their own authentication
  material on their own row, enrolling or removing their own passkeys, and
  never their name, their state or their grants, so an admin writes no
  other part of their own row either.
  Every one of those writes is audited as a verb is. **As built on
  2026-10-09** (`/admin/persons`): the page, for a session whose person
  holds a live admin grant and is enabled, lists every person, enabled or
  not, whether they hold a passkey, whether a token is outstanding and
  whether they hold the admin grant, and nothing of an agent; it offers no
  write on the admin's own row. Each write is a form's post under the
  `Origin` check, audited with the admin as principal by `session` and the
  person as target, its action `person enroll`, `person token`, `person
  disable`, `person enable` or `person rename`, its first record before and
  its outcome after; a session whose person holds no live admin grant is
  refused at the page and, at a write, with one record carrying the refusal;
  an admin's write to their own row is refused before any record. **Every
  admin write, here and on the grants page below, takes one order.** (1)
  **Parse**: every identity it submits is parsed into its exact shape at
  the surface's boundary, and a role's name into a name's, as at a
  passkey's removal too; a malformed one is refused as the ask's fault and
  recorded nowhere, so a request's text never becomes an audit target, and
  the audit, which checks every target by its kind, never turns a
  malformed ask into the server's failure. (2) **The first gate**: a
  session whose person holds no live admin grant is refused with one
  record carrying the refusal, **before any reference is looked up**, so a
  non-admin learns nothing of which persons, agents, grants or roles
  exist: this is why such a refusal names a reference unresolved, and why
  it answers alike for a known reference and an unknown one. (3)
  **Resolve**: every reference the write names is resolved for the admin
  by the store, the person, the role, the agent and the grant; an unknown
  one is answered as not found and a role whose scope disagrees with the
  agent named or not as the ask's fault, each recorded nowhere, and the
  own-row and self-change rules are refused here too. (4) **The audited
  write**: its first record, the write in its transaction, which
  re-checks under its locks the authority and **every fact step three
  resolved**, since step three read them before the lock, and mints or
  writes anything only once they pass, and **its `ok` outcome, written in
  the same transaction, so the effect and its outcome commit together or
  not at all**. **Where the commit's answer is lost, the write reads back
  that one record, once its transaction has ended**: the act holds a
  transaction-level advisory lock keyed by its own first record from
  before its COMMIT is sent until the COMMIT ends (the two-key form, under
  a class of its own beside the identity exclusion's, the listener's and
  the authority's keys), and the read-back takes the same lock on a fresh
  connection before it reads, so a backend still completing the COMMIT is
  waited for. An `ok` outcome answering its first record then stands, and
  the write answers its success; none, and it failed, its `failed` outcome
  written after. **The wait is bounded at thirty seconds**, far past any
  commit and short of a browser's patience; past it the act's fate is
  unknown, answered as the server's failure with no outcome written, so a
  later read of the audit, once the original transaction ends, settles
  it. A `failed` outcome written there would not stand beside the `ok`
  its commit writes, the one-outcome index refusing it once the commit
  lands, but the write would wait on the original until then and record
  an error; leaving it unwritten spares both. An error may come after PostgreSQL applied the commit, and
  an act that landed recorded as failed could never be put right, a retry
  meeting step three's refusal; and reading back the act's effect instead
  would read state a later act may already have overwritten (a rotation
  over a revocation, another admin's edit past the version), where the
  audit is append-only and its record cannot be.
  Each write
  lands in one transaction under the exclusion below that first re-checks
  its authority, the session standing and its person still holding a live
  admin grant, and a disable, an enable and a rename carry the version the
  page read, refusing on a stale one. An enrollment's token, and a token
  issued, is shown once, in the write's answer and under `Cache-Control:
  no-store`, never in a URL or a log, and lives the config's
  `enrollment_token_hours`. **A person's first
  credential comes by a one-time enrollment token**, neutral to the
  mechanism: an admin, or the host principal for the bootstrap, issues a
  single-use, expiring token bound to one person row, **living 24 hours by
  default, configurable and never more than seven days**, **and only for a
  person row with no authentication material**: redemption refuses if
  material has appeared on the row since, so a token can never replace a
  credential, which would let an admin take a person over. **Every bearer the
  server issues, the session bearer, the enrollment token, the sign-in token
  and the ceremony identity alike, is 32 bytes from the operating system's cryptographic
  random source**, never derived from a counter, a time or a row identity. **A lost passkey
  is recovered by another passkey the person holds, or by the host reset**:
  a host command that, in one write under the exclusion below, clears the
  person's passkeys and issues an enrollment token, so the token still goes
  to a row with no credential. **It writes no session**, the session being
  the surface's alone under section 3: each session opened with a cleared
  passkey ends at its next use, as one whose passkey is removed does, and an
  open live view at its next 15-second re-check. **There is no admin
  reset**, which would be the takeover this rule exists to prevent, so no
  token is ever issued for a row that holds a passkey. It is held on that
  person row, only as a digest beside its expiry, under section 2.8's rule, and it is consumed by that person's first
  write of authentication material, which is the one write a person makes
  before they have authenticated otherwise. **A valid token authenticates the
  person it is bound to, for that one write and nothing else**: the
  redemption is attributed to that person, who is its principal, and its
  audit record notes that the authentication was the token. No fourth
  principal stands behind it; the token is how the person proved who they
  are, once. Issuing a token is an identity write like
  any other, audited and taken under the exclusion below. **A disabled person
  acts on nothing**: every authenticated operation checks that its person is
  enabled, so a disabled person's open sessions authorize nothing, and the
  disable writes no session: the surface closes such a session at its next
  use, as the session's own writer under section 3. Disabling a person does
  revoke their outstanding enrollment tokens and sign-in tokens in the same
  write, under the same exclusion, the tokens being part of the person row,
  each ending `revoked`. Redeeming a token
  checks that its person is enabled too, so a token issued before a disable
  can never land a credential on a disabled row. **Redemption, as built on
  2026-10-08**: the page `/enroll` takes the token pasted, never in a URL;
  its options endpoint checks the token (unexpired, not ended, its person
  enabled and holding no passkey) and starts a registration ceremony; its
  finish verifies the registration, writes the first audit record (the
  person, by enrollment token, on the passkey by its `pk-` identity), and in
  one transaction under the exclusion checks the token again, refuses a
  credential ID already held, inserts the passkey and ends the token
  `redeemed`. **It opens no session**: the person signs in with the passkey
  afterwards.
- **The sign-in token, a second method for the pre-release stage, by the
  operator's ruling of 2026-11-10.** In his words: "the passkey should be
  optional not manditory at this stage. we can always tighten this stuff up
  before release". Earlier the same evening: "I already have way too many
  ways to log into way to many ... services ai WILL NOT STAND TO ADD
  ANOTHER." **The reasons.** The ruling reads with his standing statement
  that WeaverWeb is "the operator's console at the same abstraction level as
  a Proxmox console, and never faces the public". A console of that kind is
  reached by the people who run the machines it manages, from the network
  they run, and its trust root is already the host: whoever holds a shell on
  the server's host writes any grant, as the host's item below says. And the
  passkey did not carry a person through sign-in when it was tried on
  2026-11-10: a password manager extension answered enrollment's `create`
  and a passkey was stored, three times, and at sign-in the extension never
  offered the credential and the browser fell through to its cross-device
  prompt, so no session opened. A method whose trust root is the host's adds
  no new root and nothing for a person to remember, and lets the work on an
  agent's lifecycle go on while the passkey's path is settled. **What it
  is**:
  - **Selected by the server's config**: `sign_in` lists the methods the
    server admits, `passkey` and `token`, and is `["passkey"]` where the
    config names none, so a server configured as before behaves as before:
    the sign-in page byte for byte, no route for the token, and the token's
    host command refusing. A list that is empty or names a method twice is
    refused at load. A config naming `token` alone serves no sign-in by
    passkey, while enrollment and a person's passkeys stay served.
  - **Issued by the host alone**, by `person sign-in-token <person>`, for a
    named person who is enabled, holding passkeys or not, and refused before
    any record where the config does not name `token`. **No surface issues
    one**: an admin able to issue a sign-in token could sign in as any
    person, the takeover the enrollment token's rule above exists to
    prevent, and the host is already trusted with every grant.
  - **A bearer like the others**: 32 bytes under the rule above, printed
    once in the command's answer and held only as its digest beside its
    expiry, **living ten minutes by default** (`--minutes`) **and never more
    than an hour**, the store refusing a row past it, since a token is
    redeemed by the person standing at the host's side and a short life
    leaves a lost one little time; **one live per person**, a new one ending
    the earlier `superseded`; ended `redeemed` at its one use, `revoked` by a
    disable, and `reset` by the host reset, so a reset leaves no token
    printed before it to sign in after. **A session a token already opened
    the host reset leaves standing**, as it opened by no passkey the reset
    clears: the person's disable ends it at its next use, and the idle and
    absolute limits bound it meanwhile, which release reviews with the
    method.
  - **A person issued one takes an enrollment token from the host alone.**
    Such a person may hold no passkey and still hold access, which the
    enrollment token's rule above did not have to foresee: an admin's
    enrollment token for them would let the admin enroll their own
    authenticator onto that person and sign in as them, the takeover that
    rule exists to prevent. So **an admin's enrollment token is refused for
    a person ever issued a sign-in token**, and **issuing a sign-in token
    revokes the person's live enrollment token**, so one an admin issued
    before cannot be redeemed after. The host's `person token` still issues
    one, the host being trusted with every grant. A server whose config
    never named `token` holds no sign-in token, so neither rule moves
    anything there.
  - **Redeemed at sign-in** with the person's name, by section 2.8's sign-in
    by token, opening an ordinary session that names its method and no
    passkey. **It authenticates its person for that opening and nothing
    else**: what the person then does is done by `session`, as after a
    passkey's sign-in, and a session it opened adds no passkey by itself,
    adding one taking a fresh assertion with a passkey the person holds.
  - **Audited**: its issue as the host's command, on the person; its
    redemption as the session's opening, the person by `sign-in token`, the
    audit's fourth method for a person.

  **What it gives up**, stated so nobody reads more into it: a passkey's
  assertion is bound to the server's origin and to a challenge, and a
  sign-in token to neither, so a look-alike page that collects one, or
  anyone who reads it, signs in as its person within its minutes; and the
  path that carries it from the host's shell to the person's browser is part
  of what it trusts. **Release tightens it**: before release the operator
  decides whether the method stays, and under what config, with the
  passkey's own path settled beside it.
- **The server, one of the two principals that are not a person**, for the
  asks the server makes itself, of which the admission's `show` of section 7.2 is the
  one today. It may ask `show` only, the one observation verb, and never a
  lifecycle verb or a turn. It holds no grant and needs none, and it is
  still bounded by the agent's ceiling, the second gate, like any ask. Its
  asks are audited as a person's are, the first record naming the server as
  the principal, and the link frame's principal names the server too, per
  section 8, a member that never reaches the box. **As built on 2026-10-13**
  every verb frame the server sends, the admission's `show`, each opening's
  and a `show` through `Listener::verb`, has its first record (principal and
  method `server`, the agent its target, the verb its action) committed
  before the frame enters the channel, through one record step held to the
  silence bound; a record not written sends no frame, and at an admission
  or an opening closes the connection refused as the store unavailable.
  **The duty to answer a first `failed` is owned from before its insert
  until its frame is in the channel**: the server draws the record's
  identity, its insert runs in a task its asker cannot cancel and takes the
  record's outcome lock, and an ask ended anywhere between, its caller gone
  or its insert's answer lost among them, leaves no entry and has its
  first, where it landed, answered `failed` under the same lock once the
  insert's transaction has ended. The
  outcome names it when the answer lands: `ok` for admin's answer (exit 0),
  `failed` for admin's refusal (exit 1) or a fault whose kind proves the
  verb never ran (`busy`, `not_started`, `outside_ceiling`, an allow-list),
  and none for any other exit status or none, any other fault, known or
  not, nor where the connection ends with the frame in flight: only the
  contract's evidence writes an outcome. **An answer's shape is checked
  once, at the door, against the contract and the ask it answers**:
  exactly one of an outcome and a fault, and an outcome of the verb asked
  and the connection's agent, exit 0 or 1 with one answer object; the
  audit, the landing and the asker read that one classification, an answer
  outside it or to no ask is refused as malformed with its first given no
  outcome, and only admin's answer (exit 0) lands as an observation. Any other verb asked as the server is refused before any frame,
  as one refusal record. **It needs no person behind it because it acts on nothing**:
  it re-confirms state the row already holds, so its asks change no agent,
  and every lifecycle verb, which does change one, has a person behind it.
- **The host, the other principal that is not a person**, for the writes an
  operator makes by command on the server's host: the bootstrap's three
  writes, the bootstrap person row, its admin grant below and its enrollment
  token; the host reset above; the sign-in token below, where the config
  admits it; **any grant, and any per-agent role's verbs**, which is how a server's only admin comes to hold a role on an
  agent and can still change what a role they hold carries, a person never
  writing a grant on themselves or a role they hold; and the register verbs
  while they stay host commands. **The identity commands are `person
  bootstrap`, `person token`, `person reset`, `person sign-in-token`, `grant
  add`, `grant remove` and `role set`**, each under the exclusion below, and **the last-admin rule
  binds them too**: the host's removal of the last enabled admin's grant is
  refused like anyone's. It asks no
  verb of an agent and never places a turn. It is
  authorized by access to the store and the authority's directory and not by
  this section's grants, since it is how the first grant comes to exist. Its
  audit records name the host as the principal and carry the name the
  command was given with `--author` as an unverified claim, as the link
  frame's principal is one, per section 8. **It is the one path that writes a grant
  without holding the admin grant, which is why no surface can reach it**: a
  surface writing as the host would land a grant with no admin behind it.
  **These two are the only principals that are not a person**, and each is
  bounded by its reach: the server to observation, the host to the server's
  own host.
- **The role and the grant**: a role is a named set of actions, the verbs of
  section 7.2 and `turn`; a grant binds a
  person to a role on one agent of section 2.12, or server-wide for
  registering agents with the verbs of section 8. A grant names actions only
  through its role, so there is no per-person action list to drift from the
  vocabulary. **The roles, per the operator's ruling of 2026-10-07**: per
  agent, `observer`, seeded with `show`, and `operator`, seeded with `show`,
  `validate`, `load`, `unload`, `stop`, `save-point`, `restore`,
  `force-unload` and `turn`, the last three added by migration `0016` on the
  operator's ruling of 2026-10-08 to mirror the box's operator rule (where
  an admin had already written `operator`, it is left as written), each an authored row whose
  verbs the admin may change within the vocabulary above; server-wide,
  `admin`, which governs the connections (the register verbs), the who and
  how (persons, enrollment, passkeys, disabling, grants) and the what (the
  per-agent roles' verbs), and **grants no action on any agent by itself**,
  acting on an agent taking a per-agent grant. **"One observer per agent, one
  operator per agent" is read as one role of each kind per agent, held by any
  number of people**; that reading is the operator's to confirm, and the
  other, at most one holder of each, would add a uniqueness rule to the grant
  and move nothing else. **Reading is not a verb, and reading an agent takes
  a grant on it**: any grant on an agent, `observer` or `operator`, permits
  reading everything the server holds of that agent (its register row, its
  presence, its ceiling and door, its load state and the run it names, and
  its live trace window), `show` staying the verb that asks the agent
  afresh; a person with no grant on an agent sees nothing of it, not even
  its name. **The admin sees the register for the connections it governs**,
  each agent's name, presence and credentials' state, and not its door, its
  load state or its trace, which are the agent's and take a grant like
  anyone else's. **Per-agent roles and every grant are authored rows** under
  section 3.2, carrying the author member and the version, so two concurrent
  edits of one grant refuse on the stale version rather than one silently
  winning. **The server-wide admin role is fixed by the store and is the one
  role that is not an authored row**: the act's migration seeds it, nobody
  writes, edits or deletes it, and the bootstrap grant names it, which is
  how the first grant can be formed before any admin exists to author a
  role. The rule below against editing a role one holds therefore has
  nothing to guard for the admin role, since nobody can edit it.
  **Writing a role or a grant requires the server-wide admin grant**, the
  same grant the register verbs need, and each write is audited as a verb
  is: a first record before the write and the outcome as the second. Without
  this a person could grant themselves the operator role and pass the first
  gate. **The first admin is the bootstrap**, written by the host principal
  above, by an operator command on the server's host, authorized as the
  host's register verbs are, by access to the store and the authority's
  directory; no surface can create it. **No person writes a grant whose
  grantee is themselves**, granting or removing, so an
  admin cannot widen their own grants and the last admin cannot remove their
  own admin grant; a grant a lone admin needs is the host's to write. **Nor does a person
  write a role they hold a grant of on any agent**, because widening a role
  widens every grant of it, theirs included, without a grant row being
  written. **As built on 2026-10-09** (`/admin/grants`): the page, for a
  session whose person holds a live admin grant, lists every live grant
  (its person, its role, and its agent by register name or server-wide),
  every role with its verbs and version, and the register's agents, and
  beside an agent on which more than one enabled person holds a role
  carrying `turn` it says their turns interleave in the agent's one
  conversation. It offers no revocation of the admin's own grants and no
  edit of `admin` or of a role the admin holds, and says a lone admin who
  needs either goes to the host's commands. Its three writes are 5a's
  shape, audited with the admin as principal by `session`: `grant add` (a
  person by `pe-`, a role, and an agent by `ag-` or none for `admin` alone,
  one live grant per person, role and agent), `grant remove` (a grant by
  `gr-` at the version read, the last-admin rule counted first) and `role
  set` (a per-agent role at the version read, its verbs within the
  vocabulary). Each takes the four steps the person item states, the role
  named by the store at step three with its scope checked against the
  agent named or not; the self-change rules are refused at step three, before
  any record, and checked again in the write's transaction under the
  exclusion, so a grant of the role landing on its editor mid-edit refuses
  the edit. **Every write to a person row, a role or a grant takes one
  store-wide exclusion for identity and access exclusively**, one
  transaction-level advisory lock on a key of its own (`IDENTITY_LOCK_KEY`,
  beside the listener's and the authority's), held for the write's
  whole check and commit; every act a principal is authorized for takes the
  same exclusion shared, as the next item says. Such writes are rare, so serializing them all costs nothing, and it
  closes the class of race that serializing only some of them leaves open.
  Two rules are checked under it. **At least one enabled person holding a
  live server-wide admin grant remains**: every write that disables a
  person or removes or narrows an admin grant counts what would remain and refuses if none would, so two admins
  disabling or removing each other at once serialize and the second finds
  itself the last, and the last admin grant is never stranded on a disabled
  person. **And the self-change rules above are checked under the same
  exclusion**, so a grant of a role to its editor cannot land between the
  check that the editor holds no grant of it and the edit. **And the host principal can always
  write a new bootstrap admin grant**, so even a store edited outside this
  crate is recoverable from the server's host and never needs a surface.
- **Server-side authorization**: a verb is asked of an agent only if its
  principal may ask it, the person's grants on that agent permitting it or,
  for the server's own asks, the verb being an observation verb, and the
  ceiling declared at the hello of the connection the frame will be
  enqueued on, held with that live connection per section 8, contains it.
  **A connection's ceiling is fixed for its life**, so a reconnection that
  narrows it cannot race an ask; the row's copy of section 2.12 is the
  observation surfaces read and never the authorization input. **A turn frame leaves the server only if the person's
  grants on that agent permit `turn`**; no ceiling applies to a turn, the
  data plane's box gate being the gate's own admission per section 8, and
  neither principal that is not a person ever places one. Otherwise either
  is refused on the server before any frame leaves it, and the refusal is
  audited. **Every act a principal is authorized for holds the
  identity-and-access exclusion shared, from its check to its commit point**,
  and releases it there; identity writes hold it exclusively, as the item
  above says. For a verb or a turn the commit point is the enqueue of the
  frame on the connection's ordered stream, after the first audit record is
  written; for a register verb asked through the server it is the store
  commit and, for `authority rotate`, the switch of the authority's files.
  So a revocation or a disable either commits before the check and is seen,
  or waits until the commit point, and without the hold one could commit
  between the two and the act would land on authority that no longer stood.
  **Every write a session authorizes re-checks its authority inside its own
  transaction**, under the identity exclusion: the session as standing
  (open, its person enabled, and where a passkey opened it, that passkey
  still theirs),
  and any passkey that earned the write, as an addition's grant was earned,
  as still standing. Otherwise the write is refused as its authority gone,
  audited as failed, and nothing lands, so a disable, a removal or a host
  reset committed after the request read its session refuses the write
  rather than landing beneath it: an addition never plants the device a
  reset meant to cut off, nor strands the reset's token.
  **For an ask the guarantee is about order**: an ask is authorized at its
  enqueue, and a revocation or disable that commits after that is ordered
  after the ask, exactly as it is for an ask already in flight at the agent,
  and governs every ask authorized after it. **The hold never spans I/O with
  a peer**, neither the socket write nor any wait on the agent, because a
  peer's backlog must not block identity writes: one slow connector would
  otherwise hold every revocation on the server, and a long turn never
  blocks one. **The host path is exempt**, being authorized by access to the
  server's host rather than by grants, so it holds no grant that a
  concurrent identity write could take away. **The check is the server's and never a surface's**: a surface may
  hide what a person cannot do, and hiding is presentation and not the gate.
  **A read of an agent is checked the same way**, against a grant on it, at
  the read. **An open live view is one request**, so it re-checks its
  session and its person's grant on its agent every 15 seconds and closes at
  the first check that finds either ended, a disable, a removed passkey or
  grant, or the session's expiry, the twelve-hour limit included. **As
  built on 2026-10-12**: `GET /agents` lists the agents the session's
  person holds a live grant on, any role, by box and name with each plane's
  connection, and no other, a person with none seeing an empty list;
  `GET /agents/{ag}` shows one agent's register row (presence per plane,
  the declared ceiling, the trace door, the load state with its source and
  date, the run's tuple and constituents) and the person's roles on it;
  `GET /agents/{ag}/trace` is the live view, an event stream of the
  listener's window, which says it is that window and not a record. Each
  event carries the window's members as the listener landed them and its
  `replayed` member, the listener's verdict against the opening's boundary
  (section 7.2), set from the replay's end and never from admin-con's flag:
  a mark behind the boundary is the replay's as a record is, and the
  server's own two marks, a reconnection and a lost link, are live. **The
  agent page renders the stream in its Live trace pane** through the trace
  module, a file of the binary that opens the stream the pane's list names
  in `data-stream` and places each event's members as they came, a row the
  listener judged the replay's marked `replayed`, a mark with its reason,
  an absent member as an absent pill, and the view's closing reason in the
  pane's footer, the pane bounding the list and keeping the newest row in
  view while the reader is at its end, and holding at most the ring's
  10,000 events, the oldest leaving as they leave the window; a stream the
  browser reopened sends the window again, and the pane shows it once,
  cleared at the window's word that opens every connection; it computes
  nothing the stream did not send, and the stream itself is unchanged. The
  identity is parsed to its shape first (400); an agent the person holds no
  grant on answers exactly as one that does not exist (404), page and view
  alike, so neither is an oracle for agent identities. The grant check
  runs at every read in a short transaction holding the identity
  exclusion shared, released before the row is read and before any byte of
  the stream; no read is audited. The live view re-checks every fifteen
  seconds by reading its session as every use does, the last use refreshed
  at most once a minute, and the grant, and closes with its reason at the
  first that fails. **No wait in the view outlasts its re-check**: every
  send races a slot's reservation against the re-check's tick and the
  viewer's going, so a reader that does not read cannot hold the view
  open, and an event pending when access ends is dropped, never sent.
  **And the response's body is gated by the view's cut**: once the
  re-check finds access ended, the body yields the closing line and ends,
  the events already queued for it dropped unread, so nothing reaches the
  reader after the cut, whenever it reads. The admin's register page is unchanged, showing no
  agent's door, load state or trace. **As built on 2026-10-13, a person's
  verbs**: `POST /agents/{ag}/verb` parses the identity and the verb, the
  roles' vocabulary without `turn` (400, no record), then **admits the ask
  before anything is resolved**, a place among sixty-four outstanding asks
  and within four of the person's, taken without waiting: past them the
  request is not taken (503), a known agent and an unknown one alike, and
  **shedding writes no record**, being the server not taking a request and
  no refusal of authority, where a record per shed request would rebuild
  the growth the bound prevents. It then answers an agent the register
  does not hold as unknown (404, no record). The ask then runs in a task
  the request cannot cancel, carrying its admission until it ends: its slot on the admin connection's
  stream reserved first, held to the silence bound, which decides nothing,
  and at most sixteen persons' slots on one connection, admin-con's own
  queue, so they never take
  the channel the connection's own frames need (an ask past them is
  refused as busy, 503, after the grant check); a place among three held
  asks, so held asks never take every connection the store has; and from
  the place to the commit one deadline, the silence bound, so a stalled
  store answers the ask as the server's failure (503) and frees its place;
  then the hold, the identity exclusion shared in one
  transaction, under which the session standing and the person's grants on
  the agent, the per-agent ones alone, are checked, then the connection the
  slot is on and its ceiling, each refusal one record (no grant, or a
  session no longer standing, answered exactly as an unknown agent, 404; a
  grant without the verb 403; outside the ceiling 403, naming it; not
  connected 503); then the first record, durable; then the enqueue through
  the reserved slot, and the hold's commit. **A refusal's record is held to
  the ask's one deadline, read before the record is sent** (section 7.2;
  the Planner's default of 2026-11-12). Recorded, the person is answered
  the refusal itself, as above. Refused by the store, they are answered the
  server's failure (500). Not recorded when the deadline comes, they are
  answered the bound's failure (503, the ask not made within its bound),
  and its statement was never sent, so the audit holds no refusal the
  person was not told of, one statement's flight excepted (section 10).
  Past the grant check either failure is an alert besides (section 2.14),
  and before it neither is. The frame names the person by
  their `pe-` identity. The page offers the verbs the person's grants carry
  and the connection's ceiling grants, and after an ask waits for admin's
  answer to the silence bound and shows it, or answers 202 naming the first
  record, whose outcome is written when admin answers. **Beside the answer,
  admin's own words** (on the Planner's ruling of 2026-11-12): the outcome's
  stderr, cut at 2048 bytes at a character boundary as an alert's reason
  cuts it (section 2.14), the page saying where it cut. They are
  escaped as every value on the page is, shown and never parsed or read for
  meaning, so a refusal's remedy that admin writes on stderr, a stale
  `[restore]`'s among them, is read where the verb was asked. **Beside
  `load` and `restore`, admin's `[restore]` rule** (on the Planner's ruling
  of 2026-11-12): a declared `[restore]` loads only while the save point it
  names is the manifest's latest; any save point or unload publishes past
  it, and admin then refuses the load (`config_invalid`, naming `restore`)
  until `restore` runs again or the key is removed on the box. The page
  restates it from `weaver-admin-Spec` section 4 at the corpus commit
  section 2.4 pins, which the admin-operator contract does not yet carry,
  and names the commit it was read at; a test holds that commit to the
  build's pin, so the pin does not move until the rule is reread. It was
  reread at `5ec7ecf8` on 2026-11-12, every line of that section naming
  `[restore]` unchanged. The page guards nothing on it: whether a
  declaration carries a stale `[restore]` is the box's, and nothing the
  console reads says so. **Beside a verb
  admin's state table refuses, a warning** (on the Planner's ruling of
  2026-11-12 and the operator's "warn, do not refuse" of 2026-11-11):
  where every state of the admin-operator contract's section 4.1 table
  that the row's last reading covers refuses an offered verb, the page
  names the refusal, the reading and its date beside the verb's form, and
  the verb stays offered. The row reads one of four things, each covering
  the states whose `show` prints it: unloaded (S0, S0d and S8b once
  nothing of the agent stands), idle (S2 and S9), active (S3), or in
  transition while one stands (section 2.12), the transition being the
  reading then, a turn's events keeping it (S0d and S8b while processes
  stand, S1, S4 to S8a, S10, S11, any state while another command holds
  the agent, and a command killed mid-command). Six cells warn: `load`
  read idle or active (`AgentRunning`) or in transition
  (`InvocationInFlight` where a command holds the agent, `AgentRunning`
  where none does), and `save-point` read unloaded (`OutOfOrder`), active
  (`ActivityNotAtRest`) or in transition (`InvocationInFlight` where a
  command holds the agent, `OutOfOrder` where none does). `unload`,
  `force-unload` and `restore` are answered in some state of every
  reading, and the table gives `show`, `validate` and `stop` no column, so
  none of them is warned of. The table is read at the corpus commit
  section 2.4 pins, and the repository holds the table and the two
  paragraphs beneath it as read; a test derives every cell from that copy
  and holds the commit to the build's pin, so the pin does not move until
  the table is reread. It was reread at `5ec7ecf8` on 2026-11-12, the table
  and its two paragraphs byte-identical to the copy. Where the contract and
  the page differ, the contract wins. A reading is the row's and can be older than the agent's
  state, which is why the warning carries its date and the box's own rule
  decides last. **Run's two panes re-read by themselves** (on the
  Planner's ruling of 2026-11-12): the load state and the verbs are one
  fragment, `GET /agents/{ag}/run`, which the vendored htmx re-fetches
  every 15 s with no inline script, so a state that a load answered past
  the silence bound writes reaches the page without a reload, and a
  warning never outlives the reading it was drawn from. It is read as
  section 2.14's fragments are: through the session gate without
  refreshing the session's last use, an ended session answering 401, and
  through the grant scoping as at the page's load, an agent without a
  grant answered as an unknown one (404) and a malformed identity 400.
  **As built on
  2026-10-14, a person's turn** goes the same way, one mechanism serving
  both planes: `POST /agents/{ag}/turn` parses the identity and the text,
  not empty and its request line within the gate's own bound (400, no
  record), then the session, the admission, the resolve, and the held ask
  on the gate plane, where the grant must carry `turn` and no ceiling
  applies; its first record names the person, `session`, the agent and
  `turn`, and **never the text, which goes into the frame and nowhere
  else**, no record, log line or error. The gate's answer is checked once
  at the door against the ask and the contract's shape, and only the
  contract's evidence writes the outcome: `ok` for an `answered` close,
  `failed` for a `stopped` or `refused` close and for a fault that proves
  the turn never reached the gate (`busy`, `not_started`, `unloaded`,
  `line_too_long`), none otherwise. A socket write that cannot complete
  within the silence bound is the peer gone silent, on either plane.
- **The audit record, append-only**: every record names its principal, the
  person, the server or the host, how that principal was authenticated (a
  session, an enrollment token or a passkey assertion for a person, the last
  marking any assertion whose signature verified, whatever ceremony asked
  for it, the session's opening and a counter's refusal among them, an
  assertion whose signature failed proving no principal and being audited
  never; `server` for the server
  acting on its own behalf, access to the host for the host, each method
  belonging to one principal alone), its target and its action, and when.
  **The records are rows of a table of their own that store triggers keep
  append-only against this crate's code paths**: no statement this crate
  issues, the server's or a host command's, can update, delete or truncate a
  record. A process that drops the triggers with the table owner's rights is
  a compromised server, outside the threat model as the host is; a migration
  role owning the table, the server connecting with `INSERT` and `SELECT` on
  it and never `UPDATE`, `DELETE` or `TRUNCATE`, would close it, and is the
  operator's provisioning decision outside act 11. For
  a verb asked of an agent, or a turn placed with it, the target is the
  agent and the action the verb or `turn`;
  for a write to a person row, a role or a grant the target is the row's
  kind and identity and the action the mutation: enroll, issue an enrollment
  token, disable, enable, rename, set authentication material, write a role, grant,
  or revoke; **a host command's action is the command**, `person bootstrap`,
  `person token`, `person reset`, `person sign-in-token`, `grant add`,
  `grant remove` or `role set`,
  its target the person, the grant or the role, as a register verb's action
  is the verb; a register verb's target and action are the last item's. **A record
  holds no secret**: a write of authentication material records that it
  happened, never the material, and **a turn's text is not in the audit
  record**: the agent's trace on its box is the record of what was said.
  This crate writes the first record before
  the ask leaves or the write lands, so no ask or write exists without one.
  **The outcome is a second record naming the first**, written when the
  answer lands, the write commits, or either fails, so nothing is ever
  rewritten and an act whose outcome never came is visible as a first
  record with no second. **Where the act is one store transaction**, every
  admin write through the server, a person's own (a token's redemption, a
  session's opening at sign-in, a passkey's addition and its removal), the
  host's identity commands and the host's `revoke`, **its `ok` outcome
  commits with it**, in the same
  transaction, and a lost commit answer is read back by that record; its
  `failed` outcome is written after, a rolled-back act having taken
  nothing with it, so one outcome per first record holds. An act whose
  commit point is not one store transaction, the host's `register` and
  `rotate`, which publish their client configs to files after the store
  commits, the authority's verbs, and the fresh assertion's add grant,
  whose effect is a one-time grant in the ceremony table in memory and no
  store transaction, has its outcome written after it, and
  the host's `register` and `rotate` read back a lost commit by the new
  fingerprint, which decides whether the staged configs are published. A refusal at the first gate is one record
  carrying the refusal as its outcome, since no ask left, and **a refusal
  whose record cannot be written is answered as the server's failure**,
  logged at error, never as the ordinary refusal, which would read as one
  the audit holds: the refusal's half of the rule that an act whose first
  record cannot be written does not act. **Nothing about
  the person crosses to the box**: the box records the uid sudo reports,
  admin-con's own, per section 7.2, so these records are the one record of
  who asked. **Audit records are written by this crate's
  own processes alone, the server or a host command, and never edited**: they are not authored rows and take no version,
  and they are not observations of an agent, so they sit outside both halves
  as the session does.
- **The register verbs take their own path.** `authority`, `register`,
  `revoke` and `rotate` act on the server, its authority and its store, and
  nothing they do crosses to a box, so the three gates do not apply: there
  is no box ceiling and no sudo rule for them, and registering an
  agent could not pass gates that need a registered agent. **Today they are
  the operator's commands on the server's host**, written as the host
  principal above and audited as its writes are, authorized by access to
  the authority's directory and the store, per section 8: each writes its
  first record before it acts and does not act where it cannot, and
  `authority init` reaches the store for it, refusing before it writes
  anything where the store cannot be reached. **Once this section's
  identity stands, a register verb asked through the server requires the
  server-wide admin grant**, is checked under the shared hold the
  server-side authorization item states, its commit point the store commit
  or the authority's file switch, and is audited the same way. **A register verb's
  audit target is the server's authority for `authority init` and `authority
  rotate`, and the agent's row for `register`, `revoke` and `rotate`, and its
  action is the verb**, in the host's records as in a person's. **As built on
  2026-10-09** (`/admin/agents`, on the operator's ruling of that date that
  everything done with agents belongs in the web page): the page, for an
  admin, lists the register's rows, each agent's box and name, its identity,
  each plane's credential state and presence, and not its door, load state
  or trace. `register` (a box and a name, parsed to section 2.12's rule) and
  `rotate` (an `ag-`, resolved by the store, a row holding no live credential
  refused as retired) take the persons item's four steps; the audited write
  takes the authority lock as the host's verbs do, refuses where the
  config's name moved or the authority on disk is not the one the server
  loaded (the server is restarted after an authority rotation), and holds
  the identity exclusion shared, inside the register's own transaction,
  from the admin's re-check to the store's commit; a rotation re-reads its
  row there under the row's lock and refuses one retired since step three
  read it, rather than making it live again, and the credentials are minted
  only once both re-checks pass. **The store keeps
  fingerprints only, and the two client configs are handed over and kept
  nowhere**: held in the server's memory alone, each under a 32-byte handle
  from the operating system's random source, taken once, within five
  minutes, by the session that minted them, under `Cache-Control:
  no-store`, never in the store, a file, a log line or an audit record;
  at most 64 wait or are reserved, a write reserving room for its two at
  step three atomically and releasing what it does not hold on every path,
  so the bound counts the writes in flight; and a restart drops any not
  taken, after which the admin rotates. The
  page says each holds its connector's private key and goes to the box with
  the install script, readable only by the connector's own user at 0600.
  **`revoke` (one plane) and `retire` (every live plane in one write)**, as
  built on 2026-10-09, take the same four steps: the `ag-` and the plane
  parsed, the first gate, the agent resolved for the admin (unknown: not
  found; a plane revoked already, or a retire of a retired row: refused),
  and the audited write, which holds the identity exclusion shared and
  re-reads the row under its lock inside its transaction, refusing a plane
  or a row revoked since step three, and tells the running listener each
  credential it revokes, so the live connection closes in the act. **A
  grant on a retired agent stands**: the row is never live again, so the
  grant names an agent that will not answer, and the grants page marks it
  for an admin to revoke. `authority init` and `authority rotate` stay host
  commands, being the server's own authority.

**An agent holds one conversation.** Every gate connection lands in the
agent's one session working structure, per `toddwbucy/WeaverAgent#59`, so
turns granted to two people on one agent interleave in one context, and
each person's turns are read by the agent beside the other's. **The console
says so wherever more than one person holds `turn` on an agent**, beside
the grant and wherever a turn is placed. This is a rule for the surfaces,
and no grant is narrowed for it. **As built on 2026-10-09** the admin's
grants page says so beside each agent on which more than one enabled person
holds a role carrying `turn`, which `src/grants_tests.rs` shows appearing
and not otherwise; **as built on 2026-10-14** the agent page's turn form
says so too, by the grants page's own query, which `src/verbs_tests.rs`
shows appearing with two enabled holders and not with one or once the
second is disabled.

```graph
node: web-verb-refused-before-an-ask-unless-granted
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-verb-refused-before-an-ask-unless-granted
```

```graph
node: web-every-verb-asked-is-audited
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-every-verb-asked-is-audited
```

```graph
node: web-the-servers-asks-are-audited
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-the-servers-asks-are-audited
```

```graph
node: web-a-bounded-write-is-never-sent-once-its-deadline-has-come
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-a-bounded-write-is-never-sent-once-its-deadline-has-come
```

### 2.14 Alerts and the operator's decisions

**What the server does with a failure, ruled by the operator on
2026-10-14** while reading `toddwbucy/WeaverWeb#43`. Five rulings, each
binding this crate:

1. **Any failure that could lose data becomes a prompt to the operator**:
   the problem, the current status and the immediate options, and the
   operator chooses. **Nothing retries in the background and nothing picks
   a recovery on its own**, since what to do after a possible loss depends
   on context only the operator has.
2. **An `unload` stopped at its save point**, refusing `SavePointNotTaken`
   naming a leg other than `published` (the contract's S9, per section 7.2:
   the run standing at rest, its gate lowered, nothing lost yet), gets a
   prompt with two options: **save point, then unload**, and **force-unload**,
   which loses the state since the last published save point.
3. **A connector is a conduit**: it makes the pipe and passes traffic along,
   and never acts on the agent on its own initiative, per section 8. A
   connector's exit and a broken route look the same to the server: **lost
   contact, raised to the operator, the agent possibly still running**.
4. **Why a thing failed reaches the operator**: the server writes each
   failure's reason to an operations log and shows it on the agent's page.
5. **Everyone holding a grant on the agent, any role, sees its alerts, and an
   alert stays until acknowledged** by a person holding `operator` on that
   agent, the acknowledgement audited.

**An alert** is a row of its own (migration `0019`), its identity `al-` under
the convention section 2 opens with: the agent, its kind, its subject (the
act, a verb or `turn`, or the plane for lost contact), its source (the
server's own door and teardown, `admin` for a line of the box's `admin.log`,
`admin-con` or `gate-con` for a connector's own failure, `trace` for the
agent's trace, migration `0020`), a summary of at most
1024 bytes and a detail of at most 8192, the first record of the act it
concerns where there is one, the options of a prompt, when it was raised and
last occurred, how many times, when contact was restored where it was lost,
a version, and its acknowledgement, when and by whom. The first record is
named by its `au-` shape and no foreign key, since a key on the audit would
let a `TRUNCATE audit` be refused by the key before the audit's own trigger,
which is the guarantee. **Never a turn's text and never a secret**: the reason
is admin's refusal object and stderr, bounded to 2048 bytes, a connector's
fault, the gate's close kind and reason, or a teardown's reason.

**The kinds, and what raises each**, at the door where the outcome is
written or at the teardown:

- `failed`: admin refused a verb (exit 1), a fault proving the act never ran
  (the door's allow-list of section 7.2), the gate stopping or refusing a
  turn, or a save point answered without its confirmation in the prompt's
  sequence below;
- `unknown`: a fault or a cut connection leaving the act's outcome unknown,
  each act still in flight at a teardown among them, **an act whose answer
  the door refused as malformed included** (its ask stays in flight, since
  what the agent did cannot be said), **the server's own `show` excepted**: it acts on nothing, and the teardown's lost contact
  stands for the connection;
- `refused`: a person's ask refused for a system reason, no connection, the
  persons' slots taken, a stalled wait, the audit unwritable, the connection
  lost, outside the ceiling, or a fault of the server's own, **and only where
  the act's authorization stood**, the session standing and a grant carrying
  the act. A failure before that check, a stall or a store error on the way
  to it, is the server's log line at error and never an alert, so **no alert
  is ever raised on behalf of a person holding no grant on the agent**. No
  connection, the slots taken, outside the ceiling and the connection lost
  are reached only past the check. **Not** an authorization refusal (no
  grant on the agent, a grant without the verb), which is the audit's and
  not an operational failure, and **not** a shed request, which by design
  leaves no trace;
- `lost_contact`: a connection on either plane ending for any reason but the
  server's own act. Revocation, rotation, retirement and the halt (one
  reason, "closed by revocation, rotation or halt") and a row naming another
  connection raise nothing; a duplicate's refusal is never admitted and has
  no teardown. The connector dropping, a silent peer, a malformed frame, the
  store unavailable and an admission left incomplete each raise it. An
  admission of the plane notes the restoration on the open alert, which
  stays until acknowledged. **The loss is raised where the connection's
  entry comes out, under the row's lock**, and a new admission of the plane
  is decided under that lock too, so a restoration is queued after the loss
  it restores and the one worker applies them in that order; a note dropped
  on a full queue drops alone and reorders nothing;
- `save_point_missed`: the prompt of ruling 2, read from admin's object by
  allow-list: an `unload` refused `save_point_not_taken` whose `missed` is
  `answer`, `finished` or `member_dead`;
- `data_at_risk`: a refusal that could have lost data and leaves no web
  option, `save_point_not_taken` naming `published` and `boundary_unverified`,
  each explaining the problem and the status, cleared on the box;
- `data_lost`: **a loss that already happened**, on the operator's ruling of
  2026-10-15, with no options: the trace's `load` event carrying `reset`,
  the load having reset the agent to its latest save point, its reason
  (`no-clean-unload`, or `forced-unload` where a forced unload ended the prior
  run without its leave save point) named. Read by allow-list from the
  event's payload, live or replayed, and **raised once ever per prior run**,
  so a replay or a backfill of the same event raises nothing again. The load
  verb's own answer carries no reset, and the trace's `unload` event's
  `forced` says by its own definition nothing of a save point lost, so a
  forced unload's loss is the `forced-unload` reset of the load after it and
  is not inferred from the unload.

**The connectors' entries** (act 12, PR 2; the operator's rulings of
2026-10-14 and 2026-10-15): a connector sends log entries, not logs, so
that alerts can be generated for the operator; what it meets is written to
its own log first and then sent upstream as an `entry` frame, identified by
the connector's run, drawn at its start, and a sequence, and acknowledged by
`entry_ack`. **Every entry is one line of the operations log**; **an alert is
raised only for an entry that did not come through WeaverWeb**:

- **admin-con reads the box's `admin.log`**, the box fact `admin_log` of
  section 8, from its start (no history: the file as it stands when
  admin-con starts, its baseline taken before the first look and, where it
  falls inside a line admin was writing, skipping through that line's end;
  a file absent
  or unreadable then is read from its start once it opens; a size below what
  was read, a replacement, or the digest of the last 4 KiB read no longer
  matching, a copy-truncate regrown between two looks among them, is a
  discontinuity read from its start, never resumed mid-line), each line, a
  blank one too, passed along as admin
  wrote it beside one field admin-con judges: **`own`, true where the line's
  caller uid is admin-con's own**, the `uid` admin writes for an invocation
  (the uid sudo reported) or the `peer_uid` the relay writes for a peer. A
  line that is not own is an act on the box by another party, the operator
  using admin directly, and its refusal is classified by the doors' own
  allow-lists: **an `unload` stopped at its save point on a leg at rest
  raises the save-point prompt**, `published` and `boundary_unverified`
  raise `data_at_risk`, any other refusal `failed`, and the relay's refusal
  of another party `failed` too. An answer, by its kind, raises nothing. An
  own line raises nothing, the act asked through WeaverWeb having raised its
  alert at the door, so **no act raises twice**. **The alert says what
  happened and admin's own word on it, and no more**: the verb, admin's
  refusal object or outcome, and admin's time, and for the relay that a peer
  other than admin-con was refused; never the line's caller uid, command
  line or boundary digest, facts about the box every grant holder's panel
  has no need of, which the operations log keeps whole;
- **a connector's own failure** raises `failed` from its source, its subject
  the failure's kind: admin-con's `ceiling_not_read`, `trace_relay_refused`,
  `admin_log_unreadable`, and either connector's `link_outage` (the outage's
  length, once the link is back) and `entries_dropped`.

**At least once, taken once, bounded.** An entry stays in the connector's
backlog until acknowledged, and is resent on the next connection; the
server holds the highest sequence it took of each run (migration `0020`), so
a resend after a lost acknowledgement, a reconnection or its own restart is
acknowledged and taken no more, and **an entry the store did not take closes
the connection**, so no later entry moves the run's mark past it. **An entry
is acknowledged only once durable**: its row enters an inbox in the
transaction that moves its run's mark, and a worker applies it from that
copy, **each row claimed in a transaction of its own** (`FOR UPDATE SKIP
LOCKED`, so no two drainers, of one process or two, ever apply one row) and
held to the store's bound: its alert raised and kept on the row, then its
line written, then the row deleted, all committed with the claim; a drainer
that fails, dies or passes the bound mid-row ends the transaction, its
commit never sent once the bound has come (section 7.2), the claim and any
raise rolling back and the row due again. A row's back-off is bounded and
read the same way: logged as not backed off, it was not sent, and the row is
due again as it stood. **The worker and the drainer live and
die with their listener**, ended at its stop before its lock is released, **in
the order the work flows**: the connections closed and drained, their
teardowns raising, then the worker writing what was queued, held to a
bound, then the drainer, its unfinished row standing for the next start; a step that fails leaves the row for the next look, the alert
never raised twice, the line written at least once, and a restart drains
what stands. **A row that keeps failing holds no other**: its next try backs
off, doubling to five minutes, and the drain goes on past it; it is kept,
never deleted. **The drain is fair**: due rows are taken by when each is
due, a fresh row's take or a failed row's backed-off try, so a row that
failed is never chosen ahead of one not tried since, however slowly its
failures come. **The row keeps the entry as the text of its JSON**, as the
door's parser rendered it, so no second parser refuses what the door took
(`jsonb` refuses a NUL escape the door's takes), and an alert's text
carries U+FFFD for a NUL, which PostgreSQL's text holds none of. The server's own raises from the doors and teardowns stay
best-effort, dropped and counted, being its observations and no delivery. The
backlog holds at most 1024 entries and 1 MiB, the oldest dropped first, the
drops counted and reported as one entry once there is room. **An
acknowledgement the server dropped never holds a window**: the server sends
it without waiting and drops it where its write queue is full, and a
connector whose window has waited a silence bound with nothing acknowledged
sends it again, which the server's mark takes once and acknowledges again;
the bound runs from the last acknowledgement or resend, an instant no push
moves, so a steady stream of entries never postpones it.
**Every shape a connector sends is one the door takes for its source**, its
fallbacks included: an `admin_log` entry past the bound is `admin_log`'s own
`oversized` marker, and a connector's own entry past it the failure shape. **Nothing on a
connector waits on an entry**: `admin.log` is read on a thread of its own,
an entry is queued without waiting, and the connection's pump sends at most
sixteen unacknowledged at once on the write path every frame takes. **The
door checks an entry once**: its run and sequence, its source on the
plane's allow-list (`admin_log` and `admin-con` on the admin plane,
`gate-con` on the gate plane), an object within 16 KiB, and its source's
shape; anything else is refused as malformed.

**One open alert per agent, kind and subject**: a unique index over the
three where the alert is unacknowledged, and one insert that resolves its
conflict on that index, so raisers at once leave one alert counting them all,
and a flapping link raises one alert and not hundreds. A repeat takes the
latest reason, moves the last occurrence, the count and the version, and
clears a restoration; an occurrence after an acknowledgement opens a new
alert. The subject is in the key so that a refused `load` and a refused
`stop` stand as two alerts rather than one rewritten.

**Raising never waits.** The connection task, the teardown and an ask hand a
failure to a bounded queue of 256 and go on; a worker of its own writes the
alert, its store write bounded at ten seconds, then the operations log's
line naming it. A full queue drops the failure and counts the drops, the
count logged at error, so a stalled store or a full disk never holds a
connection, a teardown or an ask. **The line and the store agree in each
outcome of the raise** (the Planner's default of 2026-11-12):

- **Raised**: the line names the alert, and the alert stands.
- **Refused by the store**: the line names none (`"alert": null`), no
  alert stands, and the worker logs the refusal at error.
- **The bound passes first**: the raise is never sent, its deadline read
  before it is polled (section 7.2). The line names none, no alert stands,
  and the worker logs the bound at error, one statement's flight excepted
  (section 10).

The worker's note of a restoration on the open lost-contact alert is
bounded and read the same way: logged as not noted, it was not sent.

**The operations log** is a file the server appends one JSON line to per
failure: the time, the agent, the plane, the act, the first record, the
principal, the outcome or refusal, the alert's kind and summary, the
reason's fields and the alert's identity. Its path is the config's
`operations_log`, **with no default in the code**: the install sets it under
`/var/log`, and no host path enters the repository. The server refuses to
start where it is missing or cannot be opened for appending, and opens it
per write with append, so a rotation needs no signal. A line that cannot be
written is logged at error.

**Seen by any grant, acknowledged by `operator`**: the agent page lists the
agent's alerts, open first, newest first, each with its reason; `GET
/agents` shows each agent's open count; the reading surfaces' nav shows the
person's open alerts across the agents they hold a grant on. **An
acknowledgement names the version it read** (`POST
/agents/{ag}/alerts/{al}/ack`) and takes the admin writes' order of section
2.13: the identities parsed; the session; the grant check, no grant answered
as an unknown agent (404) and a grant without `operator` refused (403), each
with one refusal record; the alert resolved on that agent (unknown: 404, no
record); the first record, target kind `alert`, action `acknowledge`; then
one transaction under the identity exclusion re-checking the session and
the `operator` grant and acknowledging only where the version still stands
and nothing acknowledged it first, with its outcome. **A repeat landing
between the page's read and the acknowledgement is refused as stale**
(409, the outcome `failed`), so nothing is acknowledged unseen.

**A plane the server cannot reach is state on every page, not a record**,
on the operator's ruling of 2026-10-14 on `toddwbucy/WeaverWeb#15`: how
the agent behaves when its connector goes away is the box's, and what this
crate owns is what it shows when it cannot reach a connector, whatever the
cause (the connector stopped, a route or firewall changed, the box down).
For each live agent the person holds a grant on (any role, an enabled
person's, as the alerts are scoped), a plane whose credential is live and
whose link is not connected, one never connected since registration
included, is a banner above every signed-in page naming the agent by box
and name: **"agent unavailable, admin-con down"** for the admin plane and
**"turns unavailable, gate-con down"** for the gate plane, with since when
where the row knows it. A retired agent's planes hold no live credential
and show none, and nothing shows of an agent the person holds no grant on.
The banner is derived from the register at the read and clears by itself
once the plane is admitted again; the lost-contact alert stands beside it
until acknowledged. **The page re-reads it by itself**: the banners are a
fragment (`GET /availability`) that the vendored htmx re-fetches every 15
s with no inline script, the agent page's connectors are another
(`GET /agents/{ag}/availability`), and its load state and verbs a third
(`GET /agents/{ag}/run`, section 2.13), each through the session gate and the
grant scoping as at the page's load, an agent without a grant answered as
an unknown one. **A re-fetch is not the person's use**: it checks every end
of the session but leaves its last use as it was, so an open tab never
holds an idle session open, and an ended session's re-fetch answers 401,
which the page's htmx configuration swaps in, its banners going. **"Last
heard"**: the agent page shows, for a plane whose connection stands, how
long since the server last read any frame from it, heartbeats included,
kept per live connection in the listener's memory and never written per
heartbeat; for a plane that is down, since when. The connector's heartbeat
and the silence bound of section 8 stay the check; the page only reflects
what the listener knows, within its 15-second re-fetch.

**The prompt** is shown on the agent page as a `<dialog open>` with the
problem, the status and **only the options the person's grants and the
connection's ceiling carry**, server-rendered with no script, and on the verb
page that met it, which states the same and sends the person to the agent
page to choose, the alert landing through the worker. **Choosing an option
does not acknowledge the alert.** A choice (`POST
/agents/{ag}/alerts/{al}/choose`) is admitted as any ask, answers a person
without a grant on the agent as an unknown agent before the alert is
looked up, resolves the open prompt carrying the option, and runs in a task
the request cannot cancel:

- **save point, then unload** is the person's two asks through section
  2.13's path, each admitted, held, recorded and audited as any ask. The
  `unload` is asked **only on the save point's hash-confirmed answer**:
  exit 0, kind `save_point_taken`, and a report naming its save point's
  digest, of at most 256 bytes, no space and no control. Since
  `toddwbucy/WeaverAgent#158` that answer means the published copy was read
  back and digests to its manifest line, per the contract's section 5, and
  **the prompt promises no more**: the read-back is served from the page
  cache, so durability on the device rests on admin's syncs before it. Any
  other answer stops the sequence with no `unload`; a refusal or a fault has
  raised its alert at the door, and an exit 0 without the confirmation
  raises one of its own;
- **force-unload** is the person's one ask.

**A recovery holds exactly one of the person's places for its whole life**:
the choice's one admission is handed to each of its asks and back, held
across the wait for each answer and the next ask, and given back when the
choice ends. Released at an ask's enqueue, a choice would wait on its
answer, unbounded, holding nothing, and a person could pile such waits up
past their share; admitted again for each step, one recovery would take
two places, and the person's other asks could shed its own steps.

The page shows each step's status, or, past the silence bound, that the
choice runs on and its outcomes are written when admin answers.

**`show` cannot tell S9 from S2 on the wire**, a run at rest after a missed
save point and an idle one both answering `State { state: Idle }`; **an S9
entered on the box raises the prompt from its `admin.log` line** since act
12's PR 2, the refusal of another party's `unload` being classified by the
same allow-list as the door's.

```graph
node: web-raising-an-alert-never-waits
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-raising-an-alert-never-waits
```

```graph
node: web-one-open-alert-per-agent-kind-and-subject
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-one-open-alert-per-agent-kind-and-subject
```

```graph
node: web-an-acknowledgement-names-the-version-it-read
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-an-acknowledgement-names-the-version-it-read
```

```graph
node: web-unload-follows-only-a-confirmed-save-point
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-unload-follows-only-a-confirmed-save-point
```

```graph
node: web-an-alert-only-for-what-did-not-come-through-weaverweb
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-an-alert-only-for-what-did-not-come-through-weaverweb
```

```graph
node: web-a-connectors-entry-is-taken-once
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-a-connectors-entry-is-taken-once
```

```graph
node: web-a-connector-never-acts-on-the-agent
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-a-connector-never-acts-on-the-agent
```

## 3. The write path

**Seven writers, and each owns its tables, one table being owned at the member by two
of them as the paragraph below explains.** Section 3.1's ingest lands what the
instrument recorded. Section 3.2's authoring path lands what the engineer authored.
**The read path writes too**, one row and only one: section 4 admits an open query on
the condition that the query is recorded, so the read that serves it writes section
2.6's row and nothing else. That is why 2.6 belongs to neither half. **And the surface
writes one row of its own**, section 2.8's session, at the open and at the close and,
once act 11's session stands, **its last-used time at most once a minute per session**:
an ordinary request refreshes it when the stored time is a minute old or more, and an
open live view at its 15-second re-check under the same rule, each by one conditional
update on the database's clock (`last_used = now()` where it is a minute old or more), so
concurrent requests refresh it once and it never moves backwards. The writes stay bounded
whatever a page polls, and idle expiry needs no finer grain than a minute against an
hour's idle limit. That is the fourth writer, and why that table belongs to neither half
either. **The fifth is queueing**, which writes section 2.11's batch and its entries and
touches nothing else: section 5.1 has registering and queueing as two acts, and the
second of them is the one that records an order, so it is a writer rather than a state
change on a row another writer owns. **It was four until 2026-09-11**, the batch having
had no row and queueing having had nothing to write. **The sixth is the link**, as of
2026-10-01, which writes the observed members of section 2.12's registered agent, the
address it saw, each connector's link state, the ceiling admin-con declared in its
hello, the tuple and the load state as admin reported them, and the source those two
stand on, and touches nothing else. It is a
writer rather than an author because nobody authored what it writes: a link state is
what the listener saw and a tuple is what admin said, each with its own date, and
section 2.12 keeps them off the row's version for that reason. **Its writes are ordered
on the listener's own arrival sequence, its epoch and its number within it, rather than
on the row's version or the observation's date**, and a replayed event writes no member
of the row at all, per section 2.12. **The seventh is the audit writer**, as of
2026-10-02: this crate's own processes alone, the server for what is asked through it
and a host command for the host principal's writes, writing section 2.13's append-only
audit records and nothing else. It is neither an author nor an observer: nobody composes an audit record,
and what it records is not an agent's state but what the server was asked to do and
what came of it, written before and after the fact, which is why section 2.13 keeps
the records off any version and never edits them. The register verb is not an eighth:
it authors the row's other members, the name, the box and the credentials, through
section 3.2 like any author, and so do the writes of section 2.13's person rows,
per-agent roles and grants, which is why they add no eighth writer either; the
server-wide admin role is seeded by the migration that builds section 2.13 and written
by no writer after. **So one table has two writers at disjoint members**, and the rule below
holds at the member for that row rather than at the table, which is stated here rather
than left for a reader to find: the register verb never writes what the link observed
and the link never writes what the operator registered. No writer otherwise touches
another's tables, and **no surface writes through 3.1**, which is what section 6's rule
means and all it means.

### 3.1 The ingest

**The ingest is a consumer rather than a step in the loop.** The
analysis emission leaves over its own socket, a process on this side reads
it and lands it in the store, and **the decoder never waits on the store.**

**And the source of that emission is the trace, read after the fact, which
this document assumed until 2026-09-08 and now states.** The harness writes
the trace. `weaver-analysis` reads the finished record, "parses a finished
record outside the agent as an operator principal" and "drains a file or a
stream" in `weaver-analysis-web-contract`'s own words, and emits derived
readings over the analysis seam this crate consumes. **Nothing on that path
reaches the agent**, which the contract states of both parties, and the kinds
the readings are named against are `weaver-trace-PRD` section 3.1's.

**The harness's tee is a different surface and is not this one.** It feeds
the state member under the declaration's `state-election`, its parties being
the harness and `weaver-state`, and it has fed a member live. It is named
here only to be set aside, because a document that says "the trace" without
saying which reader would leave two candidate producers standing.

**Stating it is not a formality, because the store is a derivative and reads
as a source.** The trace is the program's primary artifact and every row in
section 2's recorded half descends from it through that reader. A reader
meeting this section without that sentence could take the store for a second
record of the same events, and two records of one fact is the shape the
corpus refuses everywhere else. **This crate holds a projection of the trace and never a
rival to it**, which is also why section 2.6 stores a query rather than its
result: a stored result would be a second truth about positions the trace
already fixed.

**And this crate's ingest of that seam has not run.** The emission is
chartered and its shape is contracted, and no deposit in the record was
produced by consuming it into a store. **The act that builds the ingest is
integrating rather than consuming something proven, and the producer it
integrates against is `weaver-analysis`**, which is worth naming because a
first consumer that goes looking for faults in the wrong crate spends the
difference. That is a fact about the schedule and not a defect.

- Writes are **bulk per turn or per window**, never per token.
- Writes are **idempotent on the run, turn and position key**, so a replayed
  or partially failed ingest cannot produce two truths about one position.
- **The run row is written first and closed last**, carrying a status a
  surface can read, so a partially ingested run is visibly partial rather
  than quietly short.

```graph
node: web-ingest-is-idempotent-on-the-key
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-ingest-is-idempotent-on-the-key
```

**The position is derived here and nowhere later.** The stream is addressed
by the ordinal within a generation and section 2.1's key is the position,
the resident length at the draw. Where `R` is the generation's resident
count at close, `O` its output token count, and `j` the ordinal, the
position is `(R - O - 1) + j`, the subtracted one being the turn terminator
the SPU makes resident before the answer returns.

**The rule is measured rather than reasoned**, checked on 2026-09-05 against
three records across nine generations, two precisions and two finish kinds,
each derivation compared to the position `model.field` reports directly. The
alternative that suggests itself, the previous generation's resident count
plus the turn's input delta, is exact from the second generation and wrong
on the first by the session prefix, which the first turn's delta does not
carry. **This crate takes the first rule and states the second's failure so
no later act rediscovers it.**

```graph
node: web-position-is-stored-at-ingest
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-position-is-stored-at-ingest
```

**The parting position is derived here too, and only for a branch.** A run
carrying a parent reference has its parent already in the store, so the
ingest walks the two token paths once and stores the first position they
differ at, or stores nothing where they never part. **Deriving it here is
what makes section 4's fourth read a read.** A comparison of two token paths
is a walk whatever else it is called, and a walk at the read is the thing
section 2.7 refuses: doing it once at the ingest costs one pass over a run
that is being written anyway, and doing it at every read costs one pass per
reader.

**It is absent rather than zero where the paths never part**, per section
6's rule, because a branch that changed nothing draws what its parent drew
and an arm that reproduced its parent is a different fact from one that
parted at position zero.

**The run row's members land from the emission, and the two derivations here are
named.** The run identity keys every row this ingest writes and crosses on the summary
per `weaver-analysis-web-contract` section 2.2, with the field election's depth, the
device model, the code identity the row calls the engine, and the task's verdict where
one crossed, each stored as it crossed. **The verdict has no column until the act that
lands its kind gives it one**, per section 2.2, so this clause binds that act rather
than describing a write available today.

**The effective sampling becomes two columns and that is a derivation this section
permits.** The row's seed is the sampling's `seed` member as the record spells it, which
crosses as text because the record spells it unsigned and the column holds it exact, and
the row's sampler is the sampling whole, the seed among it, so a reader that wants what
the sampler was reads one member rather than reassembling it. **A sampling that names no
seed leaves the seed absent and the sampler present**, a declared seed being a condition
a run may not have had.

**The lineage becomes the parent reference and nothing else.** Its `through` is a turn,
per `weaver-trace-Spec` section 3, and the branch position of section 2.2 is a position
in section 2.1's coordinate, so **the ingest never writes `through` as a branch
position**: it would name a place in the parent's tape the number does not mean. Section
5 says what a branch position is and the authoring path writes it, this ingest writing
the parent reference alone and leaving the branch position to the row that authored the
branch.

**The lineage's shape moves with `toddwbucy/WeaverAgent#58`, and this ingest's change is
owed until it merges.** Per WeaverAgent's answer to question 1 on
`toddwbucy/WeaverAgent#59`, once #58 lands the parent is read from the lineage's
`built_from` only, and a lineage without it is a continuation of its run and names no
parent. **A restore from a save point is a new run, and its turn keys restart at 1**,
per the operator's ruling of 2026-10-02 on the same issue, so `weaver-gate-world-contract`
does not move and the turn keys of section 2.1 hold. The rules above stand until the act
that meets #58 rewrites them.

**The boundary set is written empty**, per section 2.2, which is a fact about every run
this ingest can meet rather than a default standing in for one. **Precision is written
nowhere**, being section 2.3's label on the artifact the record identity resolves.

**The record's session and digest land here and are not derived here.** The
emitter carries both per generation and this ingest stores them once per
run, per `weaver-analysis-web-contract` section 2.2, refusing a run whose
generations disagree on either as the defect the contract names. **The
digest is the emitter's to compute because the emitter drained the bytes
and this crate never sees them**, which is section 3.1's own first
paragraph applied to a hash: a digest this crate computed over its own
rows would be a digest of the projection and would vouch for nothing. Both
are absent rather than defaulted where the emitter sent none, per section
6's rule, so a row landed from a record cut short says so.

```graph
node: web-record-members-are-absent-where-unsent
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-record-members-are-absent-where-unsent

node: web-record-members-agree-across-a-run
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-record-members-agree-across-a-run
```

**The seated prefix's length lands here and is derived by the emitter, not
here.** The emitter forms it from the run's first generation per
`weaver-analysis-web-contract` section 2.2, and this crate could not: the
summary's two counts reach the first draw's position and no earlier one,
which the rule above measured on 2026-09-05. It lands once per run, absent
rather than defaulted where the emitter sent none, per section 6's rule,
and **absent is what a whole-run arm reads as a branch position it cannot
yet take**, section 5.3 refusing the arm at authoring rather than the
ingest inventing a floor.

```graph
node: web-prefix-length-is-landed-and-never-derived
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-prefix-length-is-landed-and-never-derived
```

**A generation whose closing count the record does not carry has no
position, and its points do not land, though its summary entry does.** The
count is absent rather than derived where no `model.output` reported one, so
the address section 2.1 requires cannot be formed, and this crate stores
nothing it cannot address rather than storing rows under an invented key.
The run's row carries the status, so a run short of a generation is visibly
partial by the rule above rather than quietly short. **This is
absent-not-empty at the write path**: the same discipline that forbids
drawing a missing surprisal as zero forbids addressing a position that was
never established.

Deriving here rather than at the read is section 2.7's rule and not a
preference: a value derived once at ingest and stored is one a second reader
reproduces, and one computed in a view is one nobody can.

### 3.2 The authoring path

**A surface that authors writes the rows it authors and nothing else.** Compose writes a
declaration. Models writes an artifact row on import. Stage writes a staged experiment,
and since the plan of section 2.9 it writes that row with its entries and the refs of
section 2.10, all four being what one surface composes and registers. **The register
verb of section 8 writes the authored members of section 2.12's registered agent and
nothing else**, as of 2026-10-01, the observed members being the link's as section 3's
opening counts them. It is the operator's verb on the server rather than a surface: its
author member holds the name the verb was given, or names none, which is the rule below
with no session behind it. **Section 2.13's person rows, per-agent roles and grants are written through this path
too**, as of 2026-10-02, the server-wide admin role excepted, which the store fixes and
nobody writes: roles, grants, and enrolling, disabling or renaming a person by
a holder of the server-wide admin grant; the bootstrap admin by the host principal; and
a person's own authentication material by that person alone, as section 2.13 states.
Each write is audited by the audit writer. **Each writes only tables of the authored half**, and **none
of them may write a position or a run**, which is the whole of what keeps a recorded
fact a recorded fact and is what the pin below holds. The sentence read that each
writes one table until 2026-09-09, which was true while Stage authored one.

```graph
node: web-no-surface-writes-the-recorded-half
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-web
to: web-no-surface-writes-the-recorded-half
```

- **Every authored row carries a version, and it is the store's own counter rather than
anything the author supplies.** Sections 2.3, 2.4, 2.5, 2.9, 2.10 and 2.12, and section
2.13's person rows, per-agent roles and grants, each carry it. It
has nothing to do with the declaration's corpus commit, which pins the floor's field
shape and answers staleness against `weaver-types-Spec`: **one says whether the shape is
current, the other says whether this row has moved since you read it.**
- **Every authored row names its author, and the member is nullable.** Sections 2.3,
  2.4, 2.5, 2.9, 2.10 and 2.12, and section 2.13's person rows, per-agent roles and grants, each
  carry it. **Null means the store could not
  name an author when the row was written, and it never means the
  operator**, because a default that guesses writes a fact nobody can
  correct later and an unknown that says so can be filled by anyone who
  knows. **The member holds the identity of the person whose session wrote
  the row, and section 2.8 says where the surface had it from**: the session,
  which carries the person. **It holds the identity (`pe-`) and never the
  name**, since a person can be renamed and an author must not move with
  them, and a surface renders the person's current name. **A host's
  `--author` claim of the whole `pe-` shape is refused** at every host
  command, the register verbs and the identity commands alike, before any
  record is written, since an identity is never a claim: so a stored `pe-`
  author can only have come from a person's session, and a claim merely
  starting `pe-` stays a claim. **A member written
  before persons stood keeps the claimed name it carries**, which reads as a
  claim because it resolves to no person; the store's `author` read answers
  a person with their current name, a claim, or no author, which is how a
  surface renders the member. It is stated in two places because the member
  is in one section and its source in another. **The identity act of the
  charter's section 6 changed what fills it and not whether it exists**,
  attaching authentication to the roles that already stood rather than
  rearchitecting around them.

```graph
node: web-authored-row-names-its-author-or-names-none
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-authored-row-names-its-author-or-names-none
```
- **The column carries no default, so every write passes the member and a
  null is passed rather than fallen into.** A nullable column with a null
  default cannot tell an author nobody could name from an author nobody
  asked for, and those are the two facts this member exists to keep apart.
  A write that omits it is refused at the boundary and names the member,
  which is the same refusal shape as the stale version below. **No sentinel
  is added**: a second unknown value would be the absent-not-empty failure
  moved from the view into the store.

```graph
node: web-author-member-is-passed-and-the-column-has-no-default
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-author-member-is-passed-and-the-column-has-no-default
```
- **It is stated now because it cannot be added later**, which is the
  charter's section 4 and is restated here rather than argued. A row's
  author is knowable while the row is being written and unknowable
  afterward, so a schema that omits the member is not deferring the
  question, it is answering it as no for every row written before it changes
  its mind. Section 2.5's question is the case that shows it: **the question
  the engineer meant to ask is not a whole fact without the engineer.**
- **Before the identity act a named author is asserted and not proved.** A
  session is continuity rather than a proof until the act the charter's
  section 6 names, so where a row written before it carries a name, that
  name is who claimed the row and a reader treats it as a claim. **A null is
  not a weak claim, it is no claim**, and the two do not collapse into each
  other before the act any more than after it. **The act changes what the
  member is worth and not whether it is there**, which is the same shape as
  the null rule above: an unknown that says so and a claim that says so are
  both correctable, and a value presented as more than it is is not.
- **Writes are ordered on the row rather than idempotent, and the order is
  the version's.** An authored row is a thing a person is still making, so a
  second write is an edit. **An edit carries the version it read.** Where
  that version is the stored one the write is accepted and **the stored
  version advances by one**. Where the stored version has moved on the write
  refuses, naming both versions, and the surface says so rather than
  merging. The recorded half's idempotence answers a replay and there is no
  replay here, and a last-write-wins rule would lose the case this one is
  for, two engineers on one declaration.

```graph
node: web-stale-version-edit-refuses
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-stale-version-edit-refuses
```
- **Registration freezes the row at the version it holds and advances
  nothing.** It is a state change rather than a write of its own: the row
  moves from draft to registered and becomes immutable per section 9's pin,
  and what made it immutable is a member the row already carried. An edit
  arriving against a registered row refuses on the state and not on the
  version, which is a different refusal and says so.
- **Import registers what is present and fetches nothing.** An artifact row
  names weights already on a box, its identity computed by the rule of
  section 2.3 rather than accepted from the operator, and whether this crate
  may ever fetch is open at the charter's section 9.

```graph
node: web-import-computes-the-identity
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-import-computes-the-identity
```

**Nothing here reaches an agent.** The authoring path writes this crate's
own store, and a declaration becomes something a box will load only where
the operator places it as that box's configuration file, which is an act
outside this crate and outside admin's verbs. **Writing a declaration here
changes no agent**, which is what makes composing safe and what makes the
last `validate` answer a reading rather than a promise.

## 4. The read path

Six queries, and the schema of section 2 exists to make each an index hit.

1. **One position's alternatives**, by run, turn and position. This is the
   click, and it is `weaver-analysis field` served from the store rather
   than re-derived.
2. **A contiguous range of positions** carrying the emitted token, surprisal
   and entropy. This is the timeline and the transcript.
3. **The run's tuple.** This is the label on every reading taken from it.
4. **One staged experiment's value set, each value with its run where one
   exists.** The unit of this read is the **value and not the run**, because
   section 5.4 has an arm that never ran keep its place in the set, and a
   read whose unit were the run would return six rows for eight values and
   erase the absence the frozen set exists to record. A value with no run
   returns itself and nothing else, which is the answer.

   **An arm that frees nothing returns one arm whose value is absent.**
   Section 2.9 has such an arm register with no swept member and produce one
   run, so there is no set for this read's unit to walk. It returned nothing
   at all until 2026-09-11, which left that run reachable only by a reader
   who already held its identity - **the one reader this section says it does
   not serve**, and the sketch calls an arm that moves two or three members
   and frees none "the ordinary hypothesis", so it is not a case the matrix
   may decline to draw. The value is **absent and not null**: a point arm was
   produced under no value, which is a different fact from a sweep whose
   value is JSON null, and section 6 has the view name the absence rather
   than draw it. The swept member is absent for the same reason and is not an
   empty string.

   Where a run exists it carries its tuple, its signature per section 2.2,
   and **its parting position where it has one**, the first at which its
   token path left its parent's, derived once at ingest per section 3.1 and
   stored beside the branch position. **An arm whose path never parted
   carries none**, which the read returns as the absence it is. All three
   are index hits: the arms by their parent reference, and the other two by
   their own columns.

   **Neither the similarity nor the parting position is computed here.** A
   similarity is the open query of the condition below, computed over what
   this read returns and recorded in section 2.6. **The parting position is
   not computed at all**, being stored, which is what a comparison of two
   token paths must be for this to be a read rather than a walk.

**For a per-generation sweep the parting position is the reading and the
signature is not needed.** Same weights and same window mean the arms are
comparable byte for byte, so the parting position section 3.1 stored says
everything the arm was authored to ask.

**An arm with no parting position is the strongest reading the set can
return, not a missing one.** It means the arm moved a value and reproduced
its parent anyway, which for a per-generation sweep is the finding rather
than the absence of one, and the charter's own control arm is why: a branch
that changes nothing draws what its parent drew, so an arm that changed
something and still drew it says the value it moved reached nothing. Read as
a gap it would look like an arm that failed to run, which section 4's value
unit already keeps apart. **The signature earns its place
on a load-time sweep**, where section 5.2 has the comparison structural
rather than byte-exact and a parting position says only that two different
models wrote different essays.

5. **Every run's tuple, filtered.** This is Record, and it is the one read whose unit
   is the set rather than a member of it. The charter's section 3.6 has that surface
   hold every run and branch with the tuple that produced it, and a chip narrows the set
   rather than naming a place, so what the surface asks for is the rows a filter admits
   and never a run it already knows. **The four above answer a reader who has a run or
   an experiment, and this one answers a reader who has neither**, which is why none of
   them could serve Record and why no arrangement of them would: read three takes a
   run's identity as its argument and read four takes an experiment's.

   **The filters are four and section 2.7 indexes each.** The record identity for the
   runs admitted under one, the record's session for a session's family, the parent
   reference for a branch's siblings, and the ingest's order for the newest first. **Two
   of those indexes did not exist until this act** and section 2.7 named only one of the
   two that did, which is the state a claim of an index hit would have been false
   against. **A filter this document does not index is not a chip this crate offers**,
   which keeps section 2.7's rule where the surface would otherwise spend it.

   **An artifact's runs are a join and not this filter.** Section 2.3 has an artifact
   carry every record identity its weights have been admitted under, so a chip naming an
   artifact resolves that row's identities first and filters on the set of them. **The
   filter here is one record identity**, and a surface that folded the two would show
   one identity's runs where the operator asked for an artifact's and read the rest as
   absent, which is what the act of 2026-09-06 separated the two identities to prevent.

   **The page is a bound and not a reading.** A list is walked rather than quoted, so
   this read takes a limit and a cursor and records nothing. **The cursor is the
   ingest's order and the run's identity together**, because `ingested_at` defaults to
   the transaction's clock and a whole ingest shares one value: a cursor on the
   timestamp alone drops the rest of a tie larger than the page, and the identity breaks
   the tie into a total order the index carries.

   **Section 2.6's condition reaches a derivation and not a list.** A chip is a query in
   the charter's sense of naming what is not being seen, and section 2.6 makes quotable
   a reading a second person reruns, which is a value derived over runs rather than the
   rows themselves. **This read derives nothing**: every column it returns is one
   section 2.2 already holds. So it records no row, and **a reader who wants to quote
   what a list showed quotes the runs it named**, each by the identity this read
   returned. The condition below is stated for the surface that derives.

```graph
node: web-a-chip-filters-only-on-an-indexed-column
kind: assertion
tag: review

edge: asserts
from: weaver-web
to: web-a-chip-filters-only-on-an-indexed-column

node: web-the-run-list-is-paged-and-records-nothing
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-the-run-list-is-paged-and-records-nothing
```

6. **One plan whole**, by its identity: the parent run every arm branches from, its
   arms, and for each arm the members it names with the disposition it gives each.
   This is the matrix of `docs/project/sketch-ablation-matrix.md` reading what an
   operator is still composing, and it is the read section 2.9's row exists to be the
   subject of.

   **The arms come back in a stated order and not an authored one.** Section 2.9
   gives a plan its arms and records no order, because the sketch generates them from
   the closed tuple space rather than having an operator place them - so there is no
   authoring order to return and the store holds none. What this read owes instead is
   the *same* order on every box, which a collation does not give: an ICU collation
   sorts `col-10` before `col-2` and ignores punctuation, so two boxes would render one
   plan two ways. The order is the arm's key in byte order, which is a total order.
   **Whether the matrix owes an operator an order of its own is open at issue #549**,
   and the queue's order is a second question that issue names and does not settle.

   **An arm's status is not a member and is not computed.** Section 5.1 has the arm
   take the five states of the staged experiment it became, so the status is that row's
   state read across the arm's reference, and an arm whose reference is null has
   not been registered - which the null records rather than a sixth word this document
   would have to name. **The read joins and does not derive**, which is the difference
   between reading a fact stored once and inventing a vocabulary for the absence of one.

   **An arm is not a run and this read reaches none.** Section 2.9 has an arm reach
   its runs through its staged experiment, so a matrix that wanted the arms of a
   registered arm asks read four with that arm's experiment. Folding the two here
   would return a plan whose unregistered arms looked like experiments that had not
   run, which is the absence section 4's fourth read already keeps honest.

   **It records nothing**, section 2.6's condition reaching a derivation rather than a
   plan an operator is editing.

**The fourth read is this document returning rather than a surface proceeding.** The
three before it stood alone until 2026-09-08, when the rule below was met rather than
bypassed: the tuple ablation and the essay exemplars were assembled by hand from
deposits because no query answered them, and a sweep that cannot be read is a sweep
nobody would author.

**The fifth is that rule working twice.** This document counted four reads sufficient
for Record until 2026-09-11, and the seat that came to build Record found the store
could not list. The surface returned here before it was built and the read landed first,
which is the whole of what the rule below asks.

**The sixth is the rule working a third time, and from the other direction.** The fifth
was found missing by a seat that came to build a surface. This one was named owed by
this section before the schema it reads existed, and it landed in the act that built
that schema rather than in the act that wants it, which is the cheaper of the two ways
the rule can be met.

A surface that needs a seventh query is a surface this document has not described, and
it returns here before it is built. **The ordinal moves with the count**, this sentence
having read fifth until the fifth landed and sixth until the sixth did, where a rule
naming an ordinal that has arrived binds nothing.

**One is owed and named rather than discovered.** **The Experiments list**: read four
takes an experiment's identity, so the surface that holds every experiment has no query,
and its read is owed at the act that builds it. **Naming it here is this rule met rather
than deferred**, and section 6 counts these six against the surfaces they serve rather
than against all of them. **Two were owed until 2026-09-11**, the plan's read being the
sixth above.

**A deposit is not a row here and Record holds one through its run.** The charter's
section 3.6 says every run, branch and deposit, and section 2.2 has a deposit reach this
store as the device and the engine a run's row carries, per
`weaver-analysis-web-contract` section 2.2. **So a deposit is shown where its run is
shown**, and a deposit beside no ingested run is outside this store entirely, which is a
fact about the ingest's reach rather than a read this section owes.

**An open query surface is admissible on one condition: the query is
recorded beside its result.** A reading is a thing a second person reruns,
so a result whose query text was not stored is not quotable and this crate
does not present it as one. **Section 2.6 is where it lands.** That keeps
section 2.7's rule rather than spending it: the derivation is recorded
rather than absent, and what section 2.7 forbids is a derivation nobody can
find.

## 5. Staged experiments

**This crate has no model behind it and forks nothing.** A click authors a
staged experiment holding the parent run reference, the branch position, the
forced alternative token where one is forced, and the parent declaration
with its diff. A runner drains the queue, and each result returns as a run
in the schema of section 2 carrying its parent reference and branch
position, so the comparison needs no reconstruction.

**Two features share this row, and a branch is a load in both**, per the operator's
ruling of 2026-09-09 on issue #524. Section 5.4's **sweep** moves one member of the
tuple and runs each arm whole from the load. **An arm's branch position is stated in the
parent's coordinate and is one of two things.** An arm restored at a turn carries the
resident length at the end of the turn the load's lineage stops at, per
`weaver-types-Spec` section 4's `through`. An arm run whole from the beginning, which is
the ruling's own case and the matrix's, is a fresh load whose parent the store alone
remembers, and its branch position is the resident length of the parent's identity
prefix, the position its first turn's input begins at, which is the first at which the
arm's tape can differ from its parent's by the run's own doing. The figure is read from
the parent's row, where section 2.2 holds it as of issue #527's act, and never stated in
its place: a branch position written as zero where the row holds a larger figure would
say the arm departs before a prefix it shares, so section 5.3 refuses a stated figure
that is not the row's and refuses an absent row figure as a branch position the arm
cannot yet take. A parent whose seated prefix's length is zero, having no prefix, yields
a branch position of zero read from the row, which is the row's figure and stands. **The
mechanism carries the cut and not the continuation**: a restoring load stands the
session at the turn the lineage names, and the re-issue of the parent's turns to a fresh
arm is the queue runner's, which issue #442 holds open. The charter's section 3.5
**fork** forces a token at a position inside a turn and continues from it, so its branch
position is that position and the forced token is the member section 2.5 holds for it.
**No contract yet carries a cut inside a turn**, which section 10 holds open, so the
fork waits on that contract. **The charter's sentence that section 5.4 grounds on, "fork
the same position many times under fresh seeds", covers both features at their own
grain**: the sweep forks the run at its branch position and the fork forks a position
inside a turn, so 5.4 stands on it as a sweep does. Section 3.1 derives the parting
position for the sweep's arms, which the mechanism makes, and leaves it absent on a fork
until the fork has one, per section 6's rule. The row's members are not narrowed for the
wait. **A branch at a turn is made on the box**: WeaverAgent's offline builder rebuilds the
record up to the cut and writes it as a save point, which the agent then restores, per
`toddwbucy/WeaverAgent#59`. So a staged arm that branches is a proposal for the operator,
as a declaration is per section 7.2, and this crate places nothing on the box.

### 5.1 The five states

```text
draft       editable in every field
registered  frozen, the claim on the record, eligible for a batch
queued      handed to a runner, waiting
running     a runner holds it
returned    results in the store
```

**Registration is the freeze**, not the launch. A draft is a draft and
editing one is what drafts are for, because a draft has no result its author
could have seen. Registering puts the claim on the record whether or not it
ever runs, and queueing is a separate act. **What was registered and never
run stays in the record**, which is what makes pre-registration a property
of the interface rather than a discipline imposed on it.

**A plan's arm has no state of its own and takes these five.** Section 2.9's arm
holds a nullable reference to the staged experiment it became, and **the arm's status
is that row's state**, which is why this document names no sixth vocabulary for the
matrix to render. An arm whose reference is null has not been registered, which the
null itself records.

**So the matrix's two acts are this section's two acts and not one.** Registering a plan
freezes each arm as a staged experiment and puts the claim on the record. Queueing hands
them to a runner, **in an order the operator gives and section 2.11 records**. An
interface that registered and queued in one gesture would spend the pre-registration
this section buys, because an arm an operator registered and then thought better of
queueing is exactly the case the paragraph above exists to keep.

**Registering one arm is one transaction and registering it twice writes nothing the
second time.** The staged experiment is written and the arm's reference to it is set
together or neither is, so a registration that fails part way leaves an arm
unregistered rather than leaving a frozen experiment no arm points at. **The arm's
reference is what holds the claim at most once**: it is unique and it is set only where
it was null, so two registrations racing on one arm leave one staged experiment and
the loser finds the arm already registered. Without that, one arm would carry two
frozen experiments and the matrix would read two arms where the operator authored one,
which is what section 9's row for this claim perturbs.

### 5.2 The diff is split by when it takes effect

Both kinds reach the agent through a reload, since the load boundary is the
only change boundary. They differ in **what the result licenses**:

- **Per generation** - seed, temperature, top-p, top-k, repetition,
  maximum tokens. Same weights, same window. The recorded prefix re-feeds to
  the state the parent had, so divergence below the branch position is
  attributable to the one value moved.
- **Load-time** - artifact, devices, context capacity, and the elections. **Precision
  left this list on 2026-09-09 with the tuple**, per issue #532: an arm that freed it
  would differ in no column the run's row holds, the record identity being what
  distinguishes one quantization from another and section 2.3's label being what a
  person reads. **An arm that moves the quantization moves the artifact**, which this
  list already names. The prefix re-feeds **under different weights or a different
  window**, so the parent's internal state is not reproduced. The text upstream matches
  and the state does not, and **the comparison is structural rather than byte-exact.** A
  load-time move also derives a declaration, which stands in Agents beside its parent.

The interface states both consequences where the change is made.

### 5.3 Validation at authoring

An experiment is refused when it is authored, not when it is drained.

```text
the parent record is readable, and holds the branch position
the context capacity holds the prefix
the forced token is present in the capture at that position
every artifact the diff names resolves
```

**The third check is the fork's alone.** A sweep's arm forces nothing, so
the check holds vacuously on it, and a fork is refused here where the token
it would force is not in the parent's capture at that position, before any
contract carries the cut that would run it.

**Each check holds per value of a sweep's set**, per section 5.4, because a
load-time sweep over the artifact or the context capacity resolves a
different value at each arm and a check read once would pass on the first and
say nothing about the rest.

**A sweep naming the parent's own value is refused here, naming the value.**
Section 5.4 has the parent as the control, so such an arm declares a run the
store already holds. **The refusal is the honest answer of the three**:
admitting it writes a second row for one run, and dropping it silently leaves
the frozen set differing from what the author registered, which spends the
freeze that section 5.1 buys pre-registration with. An author who meant the
control meant the parent, and the interface says so before the row is frozen
rather than after.

**A runner cannot ask.** A refusal discovered at three in the morning costs
a batch window, and the same refusal at authoring costs nothing.

### 5.4 A sweep is one member and its values, in one row

**The charter describes a sweep and this document gave it one diff.** Section
3.5 of the charter has an operator fork the same position many times under
fresh seeds so the result is the distribution one repeatable line was drawn
from, and section 5.2 above names the members a per-generation diff moves and
states what a divergence below the branch position is attributable to. What
was missing is a row that can hold **one member and the set of values it
takes**.

**A sweep is one row.** It names the member, it names the values, and
registration freezes both with the rest of the row per section 5.1.

**The alternative is what rules it out.** A sweep held as many rows sharing a
parent is the same experiment with a different property: an engineer can
register three arms, read how they land, and add a fourth. **That is fitting
after the fact**, and section 5.1's freeze exists to refuse exactly it,
saying that what was registered and never run stays in the record so
pre-registration is a property of the interface rather than a discipline
imposed on it. A discipline can be forgotten. **One row whose value set is
frozen cannot be added to**, so the interface holds the property rather than
the person holding it.

**An arm that never ran keeps its place in the set**, which is section 5.1's
rule about a registered experiment applied one level down, and it is what
makes a sweep's absences legible: a set of eight values with six runs against
it says two arms did not run, where six rows say nothing about the two that
were never written.

**The two kinds of diff sweep differently and section 5.2 already says why.**
A per-generation sweep holds the weights and the window, so the recorded
prefix re-feeds to the state the parent had and every arm is comparable to
the parent byte for byte. A load-time sweep re-feeds under different weights
or a different window, so the comparison is structural rather than
byte-exact, and the interface states that consequence where the sweep is
authored rather than where its results are read.

**The control arm is free and is not an arm.** The charter's section 3.5 has
the seed drawn per generation from the declared seed, the turn's reference
and the ordinal, so a branch that changes nothing draws what its parent drew.
A sweep therefore needs no arm at the parent's own value: **the parent is the
control**, and a sweep that lists the parent's value among its own is
declaring a run the store already holds. **Section 5.3 refuses it at
authoring** and says why the refusal is the honest answer of the three.

## 6. The surfaces

One module each, the Experiment view and Models included: each is a surface
with its own destination rather than a mode of a list beside it. Their
destinations are the charter's section 3 and are not restated.

**The surfaces serve their own assets from `/assets/surfaces/`**: the
stylesheet `instrument.css`, the vendored htmx, the passkey module and the
trace module of section 2.13's live view, four assets. The
stylesheet is the design system's: `docs/design/system/tokens.css` followed
by `docs/design/system/components/bundle.css` without its import line, and
nothing else, pinned byte for byte by a test in `src/surfaces/mod.rs`, so
the served pages wear the pane grammar of PRD section 3.0 and the copy
served cannot drift from the copy that is the record. Each
is a file of the binary, each is served under section 2.13's
`Content-Security-Policy: script-src 'self'`, and no other asset route
stands.

**A surface that renders what is kept reads the store and nothing else** - Open a trace,
Record, Experiments, the matrix, and the returned half of Stage. **Section 4's six reads
serve Open a trace, Record, the matrix and the returned half of Stage, and not
Experiments.** That surface is a list and read four takes an experiment's identity, so
it has no query, and its read is owed at the act that builds it.

**Three surfaces author, and one of them writes more than one table.** Compose writes a
declaration and Models writes an artifact row on import, each one table. Stage writes
the staged experiment, the plan with its entries, and the refs, per section 3.2, because
a plan and the experiments it registers are one composition. Each writes through section
3.2 and into the tables section 2 gives it. **Models is the only one of the three that
is not on the path**, which is why the group of section 3.6 is named for how it is
reached rather than for what it does.

**A surface that authors or exchanges also holds an exchange outside this
crate**, a call out and an answer back rather than a write left for someone
else to drain. Compose keeps versioned declarations and shows the file
each will become (section 6.1); the operator installs that file, and
reading `validate`'s judgment of what is installed is section 7.2's.
Live carries a
turn to the gate and reads the measurement that comes back. Agents drives
the lifecycle verbs and reads the observation exchange. Section 7 names
each of those reaches. The gate is section 7.1 and the charter carries no seam
record for it, and `validate`, the lifecycle verbs and the observation
exchange are section 7.2's, reaching admin through admin-con's sudo
invoker on the agent's box as fixed command lines, never as a process the
server runs, and carrying no record either. Section 7's third is the analysis
stream, which no surface reaches and section 3.1's ingest does, and the
charter records no seam for it either: the reach is into this crate rather
than one of its own, so the record is the emitter's and sits in
`weaver-analysis-PRD` section 1, as of 2026-09-16. **Stage submits a registered
experiment to the queue, and section 7 names no queue exchange**, the
queue being the harness's per the charter's section 3.5 and this crate's
part in it a write the runner drains rather than an exchange it holds
open. **None of them writes the recorded half** - section 3.1's ingest is
the only writer of a position or a run - and each writes only the authored
table it owns, through section 3.2. None reads the agent except where
section 7 names the reach.

**The state a surface holds is a query, never a location.** A filter chip is
a clause, and clearing it widens the list in place. A card carries the
operator into a list with a chip already set, and no surface has a variant
that differs only by a pre-applied filter.

**A timeline is drawn on ordinals and a click addresses a position, and the
surface converts between them.** The per-generation series is indexed by the
ordinal within its generation, and the field is addressed by the resident
length at the draw. They are different coordinates and the record spells
both, so the conversion is the surface's to make and to make once. A view
that passes an ordinal where a position is expected reads a different token
and says
nothing about it.

**Absence renders as absence.** Entropy rides every generation
unconditionally, and surprisal rides only where its election stands. A
surface that plots an absent surprisal as zero is lying about the election,
so where the election did not stand the surface says so rather than drawing
a floor.

**The rule is every member's and not surprisal's alone.** A member the record
did not carry renders as absent and never as a zero, a blank cell or an empty
string, on every surface and for every member section 2 admits a null in: the
device and the engine a deposit did not send, the record's session and digest,
a seed nobody recorded, an author the store could not name. **A blank is
indistinguishable from a value that happens to be empty**, and the two are the
fact and its absence, which is the distinction sections 2.2 and 3.2 spend
columns and constraints to keep apart in the store. A view that spends it at
the last step has thrown away what the store was built to hold.

**It is stated here rather than left to each surface**, because the
alternative is one surface at a time deciding, and the first one that draws a
blank teaches every reader that a blank means zero. The record identity's
sentinel is the case that shows the shape: an empty identity is a hash the SPU
could not compute, so Record draws what it is rather than an empty cell, per
section 2.3.

```graph
node: web-the-view-names-an-absent-member
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-the-view-names-an-absent-member

node: web-absent-surprisal-renders-as-absent
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-absent-surprisal-renders-as-absent
```

**A reading is produced only where the record's own bracket permits it.** A
serving record carries no gate. A diagnostic record carries one, and a
reading from an uncertified replay is a picture of an unknown run. This
crate honors that gate rather than re-deciding it.

```graph
node: web-uncertified-diagnostic-record-is-not-drawn
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-uncertified-diagnostic-record-is-not-drawn
```

### 6.1 The declaration screen

**Declare is two screens**, on the operator's rulings of 2026-11-11. On the
first, the operator declares the components: which models, on which GPUs,
which store, which gate. On the second, the board, the operator wires them,
and the board shows only what was declared. **This section is the first
screen; the board is unbuilt** and waits on contracts WeaverAgent has yet to
write. When it comes, a component is a compiled primitive the harness is
pre-wired to, a wire is one ask and one return written in Python by the
operator, and a function that makes several asks is a node the wires
converge on. No page draws a wire or carries Python until then. **An agent
is binaries by hash plus wires by hash**, and the screens render that card;
**the trace is the record and not a component**, so it is declared on
neither screen. The agent's edge, its two sockets and the two connectors
outside it are not on this screen.

**The routes**, each under section 2.13's session gate and `Origin` check:

- `GET /declarations`: every lineage of section 2.4, by name, with its
  head's version, last save and author
- `GET /declarations/new`: the screen, empty
- `POST /declarations`: save the first row of a new lineage
- `GET /declarations/{de}`: reopen that row, its document filled into the
  screen, with its version, its last save, its author, whether it is its
  lineage's head, its lineage's versions with what moved at each, and the
  two panes below
- `POST /declarations/{de}`: save from that row, a new row whose parent it
  is

**The screen's slots are the form's, not the document's.** Each `GET` takes
`?boxes=`, the box slots offered in all, one by default and at most sixteen,
a reopened row's own boxes first; each box offers one model slot past its
last filled one, at least two and at most eight. A slot left empty is no
member of the document, and no slot moves when the screen is shown again,
so a refusal names the slot the operator sees. Asking for more slots is a
link and carries nothing typed.

**Who may declare: the server-wide admin grant**, for reading and writing
alike, on the Planner's default of 2026-11-11; the operator's ruling is open
(section 10). A declaration comes before Provision's register, so no agent
row exists for a per-agent role to attach to. A session whose person holds
no live admin grant is refused at every route, and at a save with one
record carrying the refusal. Reads are not audited. **A save is an admin
write and takes section 2.13's one order**: (1) the form is parsed into the
document below and the path's identity into its `de-` shape, a refusal of
either being the ask's fault and recorded nowhere, the screen shown again
with what was typed and the refusal where the session's person is an
admin, and the refusal's words alone otherwise. This step runs before the
session is read, so it is one pass over what was sent: a list's repeated
model is found by its folded name, never by a scan of every name before
it; (2) the admin gate, with
its one refusal record, before any row is looked up; (3) the row saved from
is resolved, unknown answering not found, a row that is not its lineage's
head answering conflict, and the name checked against every other lineage
and against its own lineage's, each refusal recorded nowhere; (4) the
audited write: the first record, target kind `declaration` naming the row
the save mints by its `de-` identity, a kind the audit checks as it
checks an agent's or an alert's, action `declaration save`; then, in one
transaction under the declarations' own advisory key, the admin grant, the
head and the name re-checked, the row inserted, and the `ok` outcome
written, so the row and its outcome commit together; a lost commit answer
read back by that record. That transaction holds section 2.13's identity
exclusion shared, for the grant's re-check, and then the declarations' own
key exclusively, which no other act takes.

**The document**, the body section 2.4 stores, in this grammar and no
other: a JSON object whose every member is listed here, an unknown member
refusing.

```text
name        text: letters, digits, _ and -, the grammar register holds a name to
placement   {"kind": "host"}
          | {"kind": "virtual-machine", "template": text}
boxes       one or more SPU boxes, each
              {"models": [model, ...], "gpu": requirement}
model       {"function": "decoder" | "classifier",
             "artifact": text,
             "format": "gguf",
             "adapter": {"artifact": text, "sha256": hex64,
                         "base_blake3": hex64, "scale": number}}   adapter optional
requirement {"card": text}
          | {"class": {"family": text}}
          | {"class": {"models": [card model, ...]}}
          | {"pair": [card-or-class, card-or-class]}
sampler     {"temperature", "top_k", "top_p", "seed",
             "repetition_penalty", "repetition_window",
             "max_tokens_per_turn", "context_capacity"}, each a number,
             every one required
store       "none" | "sqlite"
gate        {"tools": [] | ["sh"]}
prompt      text, or null for no prompt
```

**The fields**, and what each may hold:

- **The agent's name**, required. It names the lineage and never changes
  within it (section 2.4).
- **Host agent or virtual-machine agent**, required, ruling 5. A
  virtual-machine agent names the template it will be cloned from, as the
  operator types it. **The console does the clone itself, at declaration**,
  once a host plane exists, with no operator script between; after it, load
  is the machine's start, unload its stop and retire its destroy. **The
  template holds no keys.** Its image digest and the configured machine's
  own digest are two facts, and both are recorded: the configured machine
  hashes differently from the template, and the template is the parent it
  descends from, so the template's digest goes on the run card as the
  agent's provenance and the machine's is recorded as descending from it
  (the operator's clarification of 2026-11-11). There is no Hosts page, and
  **the console never manages a virtual machine as an object of its own**: a
  machine is an agent's, made and ended by that agent's own steps.
- **SPU boxes**, one or more, ruling 3: one box stands for the models it
  holds, so four models declared apart are four boxes and one decoder with
  its adapter is one box, spread out or collapsed as the operator chooses.
  **Each model** has a function from the roster the framework charters,
  carrying no enumeration of its own (PRD section 3.1): the decoder, exactly
  one across every box, and the classifier, at most one. Its artifact is
  text in the agent's artifact grammar, resolved by admin and read by the
  SPU. **Its format is GGUF**, ruling 4: the engine follows from the format,
  GGUF is offered first and the screen names the other formats as later
  without offering them. A decoder may carry one LoRA adapter in the four
  members of the agent's grammar (`weaver-types-Spec` section 2, ADP-1): the
  adapter's artifact, its SHA-256 and its base's BLAKE3, each 64 lowercase
  hexadecimal digits, and a finite scale.
- **Each box's GPU requirement**, required, ruling 6: a requirement and not
  a device. **A card** is one physical card, named by an identifier the
  operator types and held as opaque text: a card's identity is a fact a host
  reports, nothing reports one yet, and it is checked against what the host
  reports once a host plane exists. **A class** is the card models any one
  of which may be drawn, given in one of two forms, and the stored form says
  which was typed: **a family**, a name the capability table below lists
  models under, standing for every model the table lists under it, or **a
  list of models**, each named once. A class is one form or the other and
  never both, so no class names a model twice over, once through its family
  and once by name: the operator's examples need no mixed class, and a mixed
  one would need a rule for that double naming which nothing asks for. A
  family the table does not know is admitted with the same word for unknown
  as a model. **A pair** is two requirements, each a card or a class, in
  shard order, the order the agent's `devices` hold. The operator's examples
  of 2026-11-11 were one of each: one named card every time, any card of one
  family, and two named cards as a pair. **Start satisfies the requirement
  or refuses; it never substitutes** a card the requirement does not admit,
  and it stamps the card drawn on every load event. **A host may reserve a
  card for itself**, its display card for one, and a reserved card
  satisfies no requirement and belongs to no class, whatever the table
  lists it under (the operator's clarification of 2026-11-11); the
  reservation is the host's fact, and this screen declares nothing of it
  (section 10).
- **The tunable values**, ruling 8 and the act of 2026-11-12: every name a
  serving load requires in the agent's `tunable-values` at this build's
  corpus commit (section 2.4), eight at WeaverAgent `5ec7ecf8`, **each
  required**. That is the Planner's default of 2026-11-12, following the
  operator's ruling of that date that a missing name refuses the load on
  the agent's side as built and WeaverAgent adds no default, so the console
  saves no serving declaration lacking one. The document's member keeps
  its name, `sampler`, from the act of 2026-11-11, so a row saved before
  this act still reads. The eight are two groups:
  - **the sampler's six defaults**: the temperature, top-k, top-p, the
    repetition penalty, the repetition window and the seed. Since
    WeaverAgent's carrier (#539), the harness reads the six from the
    declaration at every serving load and refuses the load naming the
    first missing or out of range, and every generation carries its own
    six values, the record stating them per generation
    (`weaver-harness-Spec` section 6.1, `weaver-spu-Spec` section 8). What
    still waits is a turn carrying values of its own: the console sends
    none (question 6).
  - **the session's two parameters**: the maximum tokens per turn, the
    session's ceiling, fixed at load, under which each generation carries
    its own bound (`weaver-spu-Spec` sections 4.2 and 8); and the context
    capacity, the positions the session opens with, which sizes each
    device's share of the KV cache at the SPU's admit.

  **Five start filled in a new declaration's form**, the Planner's default:
  temperature 0.7, top-k 40, top-p 0.95, repetition penalty 1.1 and
  repetition window 64. These are the constants the SPU froze until the
  carrier, at WeaverAgent `a3543b31f264b269f66ab55d8767f25290ec9d61`,
  `crates/weaver-spu/src/main.rs` lines 177 to 181, unchanged at
  `61d99dce`, the last commit before #539. They are visible and editable,
  and they are written because the operator saves them, so the file states
  every value. **The seed, the maximum tokens per turn and the context
  capacity start empty**, and the operator enters each (section 10). A
  reopened row shows what it holds, a member it lacks empty.

  **The ranges are the agent's at the pin, and no further**, judged at
  save, so a value the load would refuse is refused there, naming its
  field. A count is a whole number. A real is judged as written, and again
  as the agent narrows it to a 32-bit float: a nonzero value that narrows
  to zero is refused, and so is one that narrows past the float's range.

  | name in the file | the document's member | kind | the screen admits |
  | --- | --- | --- | --- |
  | `temperature` | `temperature` | real | finite, 0 or more |
  | `top-k` | `top_k` | count | 0 to 2147483647, below 2^31 |
  | `top-p` | `top_p` | real | more than 0 and at most 1 |
  | `repetition-penalty` | `repetition_penalty` | real | finite, more than 0 |
  | `repetition-window` | `repetition_window` | count | 0 to 2147483647, below 2^31 |
  | `seed` | `seed` | count | 0 to 9223372036854775807 |
  | `max-tokens-per-turn` | `max_tokens_per_turn` | count | 0 to 9223372036854775807 |
  | `context-capacity` | `context_capacity` | count | 1 to 4294967295, below 2^32 |

  - **Two counts are bounded tighter than the agent bounds them.** The
    seed and the maximum tokens per turn stop at 9223372036854775807, the
    largest integer TOML 1.0 holds, since the agent's file could not hold
    a larger one. The agent admits either below 2^64. The bound is kept,
    and a value past it is refused naming its field.
  - **A context capacity of zero is refused**, as both of the agent's
    engines refuse it when the session opens.
  - **Two edges are stated and not solved**, on the Planner's word:
    - a seed above 2^53 is rounded, since the agent holds every tunable
      value as a 64-bit float;
    - a temperature below the 32-bit float's least normal value, about
      1.18e-38, narrows to a subnormal and is in range by the table.
  - **The screen judges no relation between the values**, since the agent
    judges none: not between the two session parameters, nor between a
    session parameter and a model. One judgement is the agent's alone:
    whether the devices drawn at start hold the context capacity's KV
    cache, which the SPU's admit judges against the model and those
    devices.
  - **The `top-k` rule of a field election**, a `top-k` of zero or above
    the field's depth refusing, is not mirrored, since the screen declares
    no field election.

  **The test of membership.** The required set is read from the agent at
  the pin, in three places:
  - `SAMPLING_DEFAULT_NAMES` in `crates/weaver-types/src/config.rs`;
  - `SESSION_PARAMETERS` in `crates/weaver-spu/src/main.rs`, whose
    `Disposition::OperatorTunable` members are required, under the names
    `SessionParameters::tunable_names` in `crates/weaver-spu/src/sampling.rs`
    gives them;
  - the conditional field-election rule in `judge_sampling_defaults`,
    `crates/weaver-harness/src/lifecycle.rs`, which the screen cannot reach.

  The repository links no crate of the agent (section 1). It carries those
  three passages as read at the pin, a dated copy named for its commit, and
  a test fails where the screen's set and the copy's differ, so a reread at
  a new pin is a diff of the copy. **The screen composes no diagnostic
  declaration**: the file writes no `binding-kind`, whose absence the
  agent reads as serving, so every row is a serving declaration and needs
  the eight. **The sampler's mechanism and every setting fixed at load
  beyond the session's two parameters** are shown read-only beside the
  model once an agent reports them. Until then the screen says what that
  waits on (question 5).
- **The store**, required: `none` or `sqlite`, the engines the agent's
  grammar names.
- **The gate**: its tool election, no tool or `sh`. Who it admits is its
  gate-con, filled at Provision; the access rule names the box's accounts
  and is not asked here.
- **The system prompt**, ruling 7: text, or none. The declaration act writes
  it into whatever state was declared; from then on it is read from state
  and never from a file, and it is locked for the session, a change being a
  reload.

**What the form refuses on its own, saving nothing and naming the field**:
a missing required field (the name, the placement, a virtual-machine
agent's template, a box, a box with no model, a model's function, artifact
or format, a box's GPU requirement, the store); a requirement that names
nothing (a card with no identifier, a class giving neither a family nor a
list, a family with no name, a list with no model or a model with no name,
a pair that is not two, a pair inside a pair, a list naming a model twice,
judged with case and spacing folded); a class giving both a family and a
list; a requirement naming more than one of a card, a class and a pair; a
roster the framework would not load (no decoder, a second decoder, a
second classifier, a function the roster does not hold, an adapter on a
classifier); a format other than GGUF; an adapter missing a member, with a
digest of another shape or a scale that is not finite; a name outside its
grammar; a control character in any text, the prompt alone admitting a
line feed and a tab, since a prompt is lines; a tunable value missing,
outside its kind or outside its range in the table above; a tool other
than `sh`; and an unknown member. **A refusal names its
field by the document's path**, its box and model numbered as the screen
numbers its slots. At save it also refuses, as section 2.4 rules, a name another lineage holds, a
changed name within a lineage, and a save from a row that is not its
lineage's head. **A class spanning more than one compute capability is not
among these refusals**, under any of them: on the operator's word of
2026-11-11, "warn, do not refuse".

**What it warns of, saving all the same**: a class whose models span more
than one compute capability, a family's models being those the table lists
under it. The capability comes from a small table in this repository, each
card model with its family and its compute capability, taken from the
vendor's published documentation with the source cited beside each row; a
model whose capability no citable source confirms is left out of it. The
table is the vendor's page as read on 2026-11-11, kept beside it and
checked against it row for row. **A family is the vendor's architecture
name**, and one name may span capabilities, as Ampere spans two in the
vendor's guides, which is the warning's commonest cause. Each row records
which rule gave its family: a statement in the vendor's compatibility
guide tying its capability to the architecture, cited to that passage, or
the architecture's word in the vendor's own model name; a model neither
rule ties to a family has none, and a list may still name it. **The
table's lookups fold case and runs of spacing**, of a model and of a
family alike.
**A model or a family the table does not know is admitted**, named on the
screen with the word that its capability is unknown, so the screen cannot
warn for it; an unknown is a word and never a blank (section 6).

**What each field waits on before it takes effect**, shown beside the
field:

| field | takes effect when |
| --- | --- |
| the name | Provision's register names the agent's row; the box's accounts are the box's |
| host or virtual machine, the template | a host agent: nothing more. A virtual-machine agent: the host plane's Spec, which waits on a measurement in progress; then the console's clone at declaration, and the template's digest and the machine's recorded |
| the boxes, models, artifacts, formats and adapters | the declaration reaches the agent (interface question 2), then `validate` and the SPU's admission; a second box has no place in the agent's grammar (question 1) |
| a GPU requirement | the agent's grammar holds device ordinals, not requirements (question 1); start satisfies the requirement or refuses, never substituting, and stamps the card drawn on every load event; a card's identifier is checked once a host reports cards |
| the tunable values | they derive into the agent's `tunable-values`, every name a serving load requires at the pin, so no file is refused for a missing one; the harness reads the sampler's six at load and every generation carries them, the record stating them; values a turn carries of its own wait on the console sending a turn with them (question 6) |
| the sampler's mechanism and the other settings fixed at load | an agent reports them (question 5) |
| the store and the gate's tools | the declaration reaches the agent (question 2); the gate admits its gate-con, filled at Provision |
| the system prompt | the declaration act writes it into declared state (question 3), and a declaration with no store has no answer yet (question 4); on the agent's side today the prompt is seeded by a turn through the gate |

**What is saved and shown.** A save writes one row of section 2.4, and the
screen shows the row's version, its last save and its author, and whether
the row is its lineage's head, a row that is not naming the head and
offering no save, and a row saved under another corpus commit named
stale; a reopen fills the screen with the same
document, and a save and a reopen of it are the same document. **Two panes
read the document back**:

- **The agent's file**: `agent.toml` in the agent's grammar,
  `weaver-types-Spec` section 2 at this build's corpus commit, the pin
  section 2.4 records on every row the build saves, which the file's
  header names; derived from the document and never stored. A row saved
  under another pin is named stale, its own pin shown beside this build's,
  and its file is this build's rendering of it, its header naming this
  build's pin and never the row's. What the document places is
  written as the grammar's members: the decoder's and the classifier's
  model bindings with their artifacts, the adapter, the eight tunable
  values as `tunable-values`, every one, the store's engine and the tool
  set. A stale row lacking a tunable value is rendered with a comment
  naming each name it lacks, since that file is one the load would refuse
  and its save is refused until they are filled. A member the grammar
  requires and the screen does not declare is written as a comment naming
  what fills it (the session, the trace sink and the gate's access rule at
  Provision, each binding's devices at start from the requirement, and any
  member the screen does not declare). **The file is not loadable
  by itself, by design**: the session, the trace sink, the gate's access
  rule and each binding's devices stay such comments, Provision's and
  start's to fill, and requiring the tunable values changes none of them.
  A trailing block of comments lists what the grammar has no place for,
  each naming its interface question: the placement and its template, each
  box's GPU requirement, and every box beyond the first. The file is TOML
  1.0, the grammar every reader of a declaration shares, and it is checked
  by a TOML parser; the agent's `weaver-types` is not linked, since this
  crate links no crate of the agent (section 1).
- **The prompt**: the text the declaration act will write into declared
  state, with its waits-on line, or the word that there is none.

**The screen reaches no agent and no host.** A save writes the store and
nothing else: no frame leaves on the link, no verb is asked, no file is
written, and nothing is installed, validated or seeded.

## 7. The seams

### 7.1 The gate

A turn crosses at the gate as any client's does. The gate does not stream,
so a whole-turn answer is presented whole and an in-flight state is clear
rather than simulated. Closes render by kind. **A close that names no turn
is the agent's own stop where it is `stopped` and carries a `reason`**: the
agent closes that way when its working structure holds a hole, and stopped
and refused closes carry `reason` in place of `text`, per
`toddwbucy/WeaverAgent#59`. It renders as the agent's stop with that reason.
A close that names no turn and is not a stop the agent explains is this
crate's own defect and surfaces as an application error rather than as an
agent's words.

**"As any client's does" is the whole of the relation and the charter carries
no seam record for it**, as of 2026-09-16. `weaver-gate-world-contract` names
one party in the graph, `weaver-gate`, and signs the world in prose because the
graph carries no node for it, so a record naming that page as a `via` claimed
this crate was a signatory in its own name. It is not: a crate that happens to
be a client is an instance of the world rather than a party beside it, per that
page's section 0. What this crate builds against here is that page, which is
what an external contract is for, and **the reach is a seam that carries no
seam edge**, the far end having no node for an edge to reach. The channel is a
named local Unix socket, per `weaver-gate-PRD` section 2 and that page's
section 0, so the client end stands on the box the agent runs on by
construction rather than by a placement clause.

**gate-con is that client, and it is this crate's binary**, per the design
session of 2026-09-30 and the operator's ruling of 2026-10-01 that the two
connectors are this crate's. It is the operator's name for what the
whiteboard called web-con. It stands on the agent's box, dials the gate's
socket by path once per turn as the seed's gate adapter does, carries one
request line in and one close line out, and is admitted by the kernel's peer
credential. **What gate-con needs on the box is what the box grants it for
the gate, as `toddwbucy/WeaverAgent#70` rules**: an entry in the
declaration's allow list alone does not reach the socket, and this document
names no group, mode or path for it. Everything it carries crosses the link
of section 8 to the server, which never dials the gate itself.
**The server sends a turn to gate-con only for a person whose grants on the
agent permit `turn`**, per section 2.13, and gate-con relays what it is sent
and checks nothing of the person, the box's own gate for the data plane
being the gate's admission of gate-con's user. **As built on 2026-10-14 the
server sends a turn only through a person's path** (section 2.13),
admitted, checked and recorded under the shared hold, its text in the
frame and in no record or log line.
**It learns nothing of the interior and the server learns nothing through
it**: the tuple and the load state come by admin-con alone, per sections 7.2
and 8, and a gate-con that reported either would be reporting what the
contract's section 6 says it cannot know. **A turn whose request has crossed
to the agent runs to its end whatever becomes of its caller**, per
`toddwbucy/WeaverAgent#59`, so a turn gate-con abandons at its shutdown, or
one in flight when the link ends, has an unknown outcome: the server
answers it as sent and never answered, never as not connected, and the
agent's trace, relayed by admin-con, is where its outcome is read. **A turn
the gate admitted just before an unload closes unanswered**: the gate drops
every connection it serves as it is lowered, the turn recorded on the trace
as refused, per `weaver-gate-world-contract` section 5 at `43ba391`
(`toddwbucy/WeaverAgent#94`), and gate-con answers it as it answers any turn
whose connection closed after it crossed, unknown, the trace again being
where its outcome is read. **A turn
still waiting behind gate-con's in-flight bound at its shutdown never
reached the gate**, so gate-con answers it `not_started`, its own fault like
`busy`, while the link still stands, and its caller knows it did not run
and may ask again. On a link loss the connector cannot answer and the
server cannot tell a turn started from one queued, so every turn the link
held answers unknown, which is the conservative answer. **Every
gate connection lands in the agent's one conversation**, per the same
issue: turns from different callers interleave in its one session working
structure, which section 2.13 carries to the surfaces.

### 7.2 The admin verbs

**There is no admin socket as of 2026-10-02.** It retired on 2026-08-05, per
`weaver-admin-operator-contract` section 1, and nothing crosses into admin by
a channel. Each verb answers one JSON object, a `lifecycle-answer` or a
`lifecycle-refusal`, per `weaver-admin-Spec` section 2. A refusal is
admin's answer about the verb and never a load state: `no_such_agent` means
the agent is not registered on that box, never that it is unloaded, per
`toddwbucy/WeaverAgent#59`. **Eight are command lines as of 2026-10-08**, per
`weaver-admin-operator-contract` section 2 at `43ba391`
(`toddwbucy/WeaverAgent#94`): `show`, `validate`, `load`, `unload` and
`stop`, and `save-point`, which takes a save point of the running agent and
publishes it at once, `restore`, which names the save point the next load
restores, the one the agent's declaration names and never one the caller
chooses, and `force-unload`, the unload that completes without its leave
save point and records the loss. **`unload` refuses `ActivityNotAtRest`**
while a turn runs or gate traffic stands, touching nothing, and
**`SavePointNotTaken` naming the leg its leave save point missed**, per the
contract's section 5 since 2026-10-09: **only `published` ends the run**,
its save point standing unpublished and the next load recording the loss;
every other leg leaves the run standing at rest with its gate lowered and
nothing lost yet (the contract's S9), for a `save-point` and then an
`unload`, or a `force-unload`, which section 2.14's prompt offers the
operator. A `save-point` whose publication does not land, or whose copy does
not read back to its manifest line, refuses `SavePointNotTaken` naming
`published` too, the run still open. **`BoundaryUnverified` is refused at
every verb that publishes**, a `load`, an `unload` and a `save-point`, where
a save point's boundary cannot be verified, until the operator clears it on
the box (`toddwbucy/WeaverWeb#43`). Both are section 2.14's `data_at_risk`
alerts, with no web option. **`list` was retired by `toddwbucy/WeaverAgent#45`**,
because enumerating agents is this crate's register of section 2.12 and
weaver-admin is one agent's organ. **admin-con runs them, and it is this
crate's binary**, the management plane's one reach, and the server asks
admin-con over the link of section 8 rather than running anything. **admin-con
also reads the agent's `admin.log`**, admin's NDJSON record of the
boundary's activity (`weaver-admin-Spec` section 8: refused and read-only
verbs, `show`, a `stop` at rest, the relay's connects and refusals, each
line naming its caller's uid and its outcome as the answer or refusal
object), from the absolute path its config names as the box fact
`admin_log`, no default; each line it reads is an entry of section 2.14.

**Access to the verbs is the box's sudo rule, and this crate holds exactly
one privileged invocation: admin-con's sudo invoker**, on the operator's
ruling of 2026-10-03, which revised the rule of 2026-10-02 that no sudo
stands anywhere here (`toddwbucy/WeaverWeb#14`). The box installs one strict
rule per agent: it names admin-con's own service user and grants it exactly
the fixed `weaver-admin <verb> <agent>` command lines for that one agent,
with no argument the caller chooses, nothing read on standard input, and no
login session opened for them, per `weaver-admin-operator-contract` sections
1 and 2. An observer's rule grants `show`, and an operator's adds
`validate`, `load`, `unload`, `stop`, `save-point`, `restore` and
`force-unload`. **The agent's page offers each lifecycle verb that both the
person's grants on the agent and the connection's ceiling carry**,
`save-point`, `restore` and `force-unload` among them; the prompt of section
2.14 asks `save-point` then `unload`, or `force-unload`, as the person's own
asks; and every ask is authorized per section 2.13 before it is sent.
**The operator's reason for the old
rule still holds**, that a root process parsing arguments which arrived over
a network is where a CVE comes from, because nothing that crossed the link
reaches the command. The server sends admin-con an abstract verb and never a
command. admin-con maps it locally to the granted line, builds the command
from constants and the agent's configured name alone, runs sudo
non-interactively, and closes the invocation's standard input. The invoker
is one module, `src/link/sudo_invoker.rs`, and `tests/no_privilege.rs`
pins that nothing else in the repository invokes privilege, per section 9.
**The person never crosses to the
box**: the cause the agent records is the uid sudo reports, admin-con's own,
per `weaver-types-Spec` section 3.1, and which person asked is section
2.13's audit record alone. The child runs in a session of its own, so
admin-con's own signals never reach it. The seed's `lifecycle.rs`,
which ran the verbs through an unscoped sudo, left the tree with the act
that built admin-con.

**The ceiling comes from the box's sudo rules, and a verb is asked whenever
the ceiling grants it.** admin-con asks sudo, for each verb it knows,
whether that exact line is granted without running it (`sudo -n -l` on the
line), and declares exactly the granted set in its hello, per section 8 and
`toddwbucy/WeaverWeb#14`. The listing needs no password only while every
sudo entry admin-con's user holds is `NOPASSWD`, which the rule's narrowness
already requires. **The trace door's state never gates a verb.** The relay
of the paragraphs below runs only while the agent runs, so a closed door
while the agent is unloaded is the normal state, and `load` must be askable
while the door is closed. The door's state is shown on the row, per section 2.12. The
ordering of `show` answers against trace events, the four clauses of section
9, holds while the door is open. While it is closed, verb answers land at
receipt with their source, and nothing the agent writes to the trace while
it is closed ever writes the row, per the rule for a door's opening below.

```graph
node: web-ceiling-is-what-the-sudo-rules-grant
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-ceiling-is-what-the-sudo-rules-grant
```

```graph
node: web-no-privilege-outside-the-sudo-invoker
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-no-privilege-outside-the-sudo-invoker
```

`show` answers the agent's load facts, the observation exchange of
2026-09-04. Load
state is therefore **the agent's side's own word rather than an inference
from a socket's existence**: a `show` answer is admin's word, a trace event
is the agent's own record, and no surface labels either as inferred.

**What crosses out of the agent is the trace, and admin-con reads it
through the trace door, the relay the agent's start step launches.** The
trace crosses as NDJSON, one event per line, to a sink admin opens at load
under root, per `weaver-admin-Spec` section 5, and the contract's section 3
names a file as a conforming sink and the record behind it as the
operator's own. **Per the operator's ruling of 2026-10-01, the agent's
declaration names a `File` sink, root-owned and append-only**, and the
reason that ruling gave still stands: a file sink loses nothing to an
admin-con restart, a load needs nothing of this crate's to succeed, and the
file is the durable record itself, so nothing needs teeing. **Per the
operator's rulings of 2026-10-03, admin-con reads that file through the
relay and never by a grant of its own**, per `weaver-admin-operator-contract`
sections 1 to 3 and `weaver-admin-Spec` section 6. The relay runs beside the
worker for the run's life, holds the run's file by descriptor, and listens
on a socket only the agent's access group can reach, admitting exactly one
reader: admin-con's own service user, declared as the agent's
`trace-reader` in its boundary file. **The relay replaces the tail** because
a reader of the record through the relay reads it through no grant of its
own, so the territory's layout gives admin-con nothing, and because one
declared reader is the whole of the box's fan-out: more readers are this
crate's to serve, from the server, never the box's. The stream is one-way
and nothing behind the sink reaches back; what admin-con decides from
reading it comes back by running a verb, per the contract's section 6.
**The load event in that stream, and its `wall_ms`, are the harness's**,
written on the harness's clock, which is not monotonic, and carried by the
sink admin opens, per `toddwbucy/WeaverAgent#59`. It carries the
declaration's digest, so the trace and `show` are the two sources of
section 2.12's tuple: a `show` answer is admin's word, a trace event is the
agent's own record, and both are the agent's side and never the data
plane's.

**The relay's stream, per `weaver-types-Spec` section 3.1.** admin-con
sends one `TraceRequest` line, an offset on a record boundary and the digest
of the record ending there, within five seconds of connecting and at most
4096 bytes. The relay refuses a position whose prior record does not hash to
that digest, before a byte is sent, so **only a clean close before any byte
is a refusal**: a partial header or a read error is a closed door, and the
next opening applies the admission's rule. Otherwise it writes a `TraceHeader` line
naming the file's identity, its device, inode and birth time in nanoseconds
where the filesystem reports one, and its `length`, the measure below,
then the trace's own lines byte for byte from that position, and at the
end of the file it keeps following, with a heartbeat line while idle. Every line the relay adds is a `TraceLine` whose
one member is `trace_stream`: `header`, `heartbeat` or `truncated`. **After
`truncated` the stream ends**, and admin-con requests again from offset
zero. The newest connection from the reader replaces the old. **One sink
serves every run**: each run appends to the same file and opens with its
`load` event, and a stream resumes across runs within the file, per
`weaver-admin-Spec` section 6. **The relay runs only while the agent
runs**, so the door is closed while it is unloaded, which is the normal
state: admin-con redials on its backoff. A relay that accepts a request
and drops it before carrying anything opens and closes the door on that
backoff, each opening asking `show`. **The door's state is admin-con's
word at admission and at every change**: the hello carries it at
admission, and because the connection outlives loads and unloads, every
change after crosses on the live connection as a frame from admin-con with
its date, and lands on the row like a link-state write, bound to the
connection that reports it, per section 2.12. That frame is `door`, with
its date, and at an opening the boundary the opening took. **A frame and
not a fresh hello**: the ceiling is fixed for a
connection's life as a security bound, while the door's state carries no
authorization, so it moves as the replay's marks do. A closed door while the load state is `unloaded` renders
as the normal state. A closed door while a run stands is shown as such for
the operator to read, and is never a fault this crate raises.

**Every opening of the door is an admission of the trace, and a closed door
writes nothing to the row.** At each opening, at the hello or later on the
live connection when the relay is reached, admin-con takes a boundary at
the opening and relays everything from the server's acknowledged position,
or from the backfill's start after a server restart, to that boundary,
marked replayed, which feeds the window and writes no member of the row.
It ends that with `caught_up`, and events after the boundary are live.
**Where the ceiling grants `show`, the opening asks it exactly as at
admission, including when it is asked**: the server asks it at the
opening, and admin-con serves it during the replay with no drain, per the
paragraph below on the server principal's `show`, so its snapshot is taken
after the boundary and every live event written before its invocation is
relayed after its answer. The opening completes only when both that answer
and `caught_up` have arrived. **An opening on the live connection serves
its own `show` before any ordinary verb**: from the opening, where the
ceiling grants `show`, admin-con serves only a `show` the server asked, as
during a replay, so a person's verb queued at the opening cannot hold that
`show` past the opening's deadline and close the connection
`admission_incomplete`, losing that person's answer. **The hold is set at
the hello as at every opening**: the hello is an admission, and where the
ceiling grants `show` its `show` is owed whatever the door's state, so a
hello with the door closed, which has no replay to restrict it, holds
ordinary asks the same way; one rule decides the hold at both. **The hold
ends when the server reports the opening's `show` landed, or with the
connection, and runs on no clock of admin-con's**: the report is section
8's `landed` frame, sent only once the store took the observation, so a
`show` that faults, answers anything but a state, or whose landing fails or
stalls keeps the hold until the server closes the connection. Sending the
answer is not the end of it, since a send only queues the frame and the
server may still fail to land it, and no deadline admin-con could compute
is proven to outlast the server's own, which is the one clock. A new opening waits
until that hold resolves, so an earlier opening's `show` never clears a
later one's. **An opening is taken only while no invocation of this
crate's is in flight.** The door opens during a `load`,
since the start step launches the relay before the load's command exits,
and admin-con may reach the relay at any time; but it takes the opening's
boundary, reports the door open, and runs the opening's replay and `show`
only once the invocation slot is free, so the opening's deadline starts
when the opening is reported and never while a verb runs. The invocation
that opened the door is the one the opening's `show` would wait behind, and
a load may run far past the silence bound, so an opening taken at once
would close a normal long load's connection as incomplete and answer its
caller unknown. Nothing is lost by waiting: what the agent wrote meanwhile
stands behind the opening's boundary, and the opening's `show`, asked once
the load has answered, reads the state the load left. This is the admission rule below applied at every opening, not a
new mechanism, and the boundary's measure is the one elected below, the
header's `length` where the header names one. **A hello while the door is closed carries no
boundary**: the server admits it with the replay ended at once, `caught_up`
with nothing behind it, serves verbs per the ceiling, and their answers land
at receipt. So `load` is askable while the agent is unloaded, and nothing
deadlocks: the door opens after the load, that opening runs the rule above,
and the file's earlier contents never arrive as live events. **While a run
stands and the door is closed**, as when the relay died, which keeps it
closed until the next load per the contract's section 5, the agent may
still be writing. Verbs still serve per the ceiling and their answers land
at receipt, but **nothing the agent writes to the trace while the door is
closed ever writes the row**: at the next opening every event from the acknowledged position to
that opening's boundary stands behind the boundary and is marked replayed,
so it reaches the window and never the row, and the opening's `show`
re-establishes the row. That is the same discipline as a server
outage, and it is why landing at receipt is safe: the stale-event inversion
the four clauses of section 9 guard against needs an event from the closed
interval to land live, and none can. The drain before a verb applies while
the door is open. With the door closed there is nothing to drain, and the
row says the door is closed, so an operator reading a `show` answer knows
the trace behind it is not being followed. **One rule serves both seams**:
the acknowledged position and the replay boundary already exist to separate
what the server holds from what is new, and a door's opening is the same
seam as a reconnection, so it takes the same rule rather than a second
ordering regime.

**The trace is replayed from an acknowledged position, and this is this
document's election of 2026-10-01 and not a ruling.** A reader that relays
what it reads and sends none of what it read while the link was down leaves
the server's copy of the trace permanently short of every outage. So the
server answers admin-con's hello with the last position it holds for that
row's trace, admin-con resumes the relay from there, and the server
acknowledges as it lands, so the position advances. **The acknowledged
position has three members, and together they are exactly what the relay
verifies**: the file's identity, as the relay's header names it; a byte
offset that is always a record boundary, per the rule stated below, so that
no offset in this document names the inside of a record; and **the sha256
hex of the record that ends at that offset, over the record's bytes from the
start of its line through its terminating newline**, absent only at offset
zero, which is the relay's own digest rule. A resumption is a
`TraceRequest` of the offset and the digest, and a check of the header's
identity against the position's. **admin-con's position is the relay's**:
it names a file by the identity the header gives, its device, inode and
birth time, `-` where the filesystem reports none and never zero, and
digests the whole record, in pieces where it passes the record bound, so a
position after any record is one the relay verifies. **The identity and the
digest each close what the other leaves open.** The identity tells a file
rotated between runs from the old one, which an offset alone could not: a
replacement grown past the acknowledged offset would otherwise resume past
its own prefix with no mark. The digest catches a file truncated in place
and regrown past the offset, which keeps its identity, and the relay refuses
that position before a byte is sent. **The bound is stated**, as the
relay's own documents state it: a file with no birth time rests on device,
inode and the digest, and a replacement reusing the inode that repeats the
record at the same offset reads as the old file. The identity and the
offset are admin-con's and the relay's, never a trace field, so resuming
reads none of the event schema this document restates none of. The
position crosses the link with every event, the server acknowledges it, and
the hello's answer carries it; **an acknowledgement never blocks the
server's read**, and one that finds the connection's write queue full is
dropped, since each names a later position than the last and the hello's
answer carries the position the server recorded rather than the last frame
it sent. **An acknowledgement never names an event the store does not
hold**, which the act that built the listener reads as a rule on the
connection: a store failure while landing anything on a connection closes
it with a typed refusal, the acknowledged position standing at the last
success, so the reconnection's replay resends the failed event and
everything after and the admission's `show`, where the ceiling grants it, is
asked again, and a pending ask whose answer could not be landed answers an
error and never the outcome.

**The acknowledged position lives for the life of a server process and is
not persisted**, per the operator's ruling of 2026-10-01: the server's copy
of the live trace is a live window and not a record, and durability stays
on the agent's box, per the contract's section 3 and the file-sink ruling
above. **The live trace has no durable copy here, and that is scoped to the
live trace**: series derived from the record reach this crate's store
lawfully through `weaver-analysis-web-contract`, per section 7.3. A
persisted position would outlive the events it names, since the server
holds relayed events in memory and section 3.1's store holds a projection
and not the trace, so admin-con would resume past a hole no mark covers.
So within one server process a reconnection resumes from the acknowledged
position as written here. **A server that restarts answers the hello with no
position, and admin-con then relays a bounded tail**, with a discontinuity
mark at the front saying what was not relayed. **The tail's shape is this
document's election of 2026-10-05**: the relay cannot start mid-file
without a digest, and admin-con no longer reads the file, so admin-con
reads from offset zero, discards locally until it stands within
`backfill_bytes` of the file's end, and relays from the first record
boundary there; the file's end is the opening's boundary, the first of the
measures below. The link and the server's window stay bounded, while the
local read is not, which is the price of reading only through the door.
This is right because the record is the file on the box and the Replay
surface renders landed deposits through the analysis seam of section 7.3,
so the server's window owes completeness only to the process that holds it.
**Truncation and rotation are marked, and nothing is smoothed.** A
`truncated` line ends the stream, and admin-con requests again from offset
zero, marking the discontinuity. A rotation is invisible within a run,
since the relay holds the run's file by descriptor, and reaches admin-con as
a new identity in the next run's header, or as a position the relay refuses:
either way the old file's unrelayed tail is lost to the reader, sent as a
marked discontinuity, and the new file is relayed from its start. That is
the contract's rule in section 3 that nothing is shed silently. **Three
measures, each "everything the file held at that moment"**: the opening's
boundary, the backfill's start and the drain before a verb. **The first
two are taken at the header's `length`** where the header names one
(`toddwbucy/WeaverAgent` PR #174): the end of the last record complete at
the relay's size read for the request, read once the position verifies, a
record boundary like every position on the wire. The member is required
upstream and read as optional here, so an older relay's header, which
names none, takes both on the relay's heartbeat, which comes only once the
relay has sent everything the file held and has stood idle; one admin-con
serves either relay with no order of rollout. **The opening's boundary**,
the hello's or a later opening's, is the position the read reaches when
its offset is at or past the length, and the dialed position itself where
that already is, nothing read; under a header naming no length it is the
position at the first heartbeat after the opening's request. A heartbeat
before the length is the relay's word that the file ended first, and the
boundary is taken there, logged. **The length is a lower bound**: a
record completed after the size read, before the header reaches
admin-con, lies past it and is relayed live after `caught_up`. A redial
after a truncation reads under its new header's length. The opening reads from
admin-con's last relayed position, or from offset zero where the relay
refuses it or serves another file, and keeps the last `backfill_bytes` of
what it read, so the replay behind the boundary is sent from what it holds
where that covers the span from the server's position, and read again from
that position where it does not. **A position the server holds past a
boundary taken at its bound, or at the length, is caught up already**, once verified before
the replay ends, the verification running beside the connection's reads so
the admission's `show` is served meanwhile: the opening dials the relay at it, and where the relay
answers, nothing is behind it, `caught_up` goes at once, and the stream
resumes live from there. Where the relay refuses it, the position names
nothing the file still holds, so a mark goes at the replay's front and the
replay is a backfill to the boundary, the file's history staying behind it. **The backfill's start** is the first
record boundary within `backfill_bytes` of that boundary. **The drain
before a verb** reads the stream until a heartbeat dated at or after the
drain's start, never to the length: the contract places a reader's own
act by the trace's own events, never by the length alone, and takes a
boundary mid-stream by a new request, which would replace the live stream
for a lower bound. **Each falls back to a bound**: a relay that stalls
mid-read reaches neither measure, and a writer that never idles, or a file
that ends inside a record, gets no heartbeat, since the relay heartbeats
only at the file's end and never mid-line, which leaves the drain, and an
opening under a header naming no length, without one. **Where the
opening's read reaches neither by 30 seconds from the opening's start, the
boundary is taken at the position read so far**, the bound
covering the opening whole, its dials and its verification included, the
last part of it, 5 seconds, reserved for verifying the server's position,
and on the hello's opening the server connection's handshake and hello
not counted against it, since they fall between its read and its
verification,
so a relay that holds its header past it leaves the door closed, retried on
the backoff; and a verification that fails leaves the next opening reading
on from the boundary this one measured, so each attempt moves toward the
server's position: an earlier
boundary only makes more of the backlog live, the agent's own record in
order, so the row may read a transient older state and converges to the
trace's tail, and the opening's `show` re-establishes it. **Where none
dated at or after the drain's start comes within 10 seconds, the verb is
invoked anyway**: what is unread then is read after the answer and ordered
behind it. The inversion that matters, an unload written before a `show`'s
snapshot and relayed after its answer, cannot pass through that gap, since
an unload holds the box's invocation lock for its whole run and a `show`
meeting it answers `InTransition`, which claims no state (`toddwbucy/WeaverWeb#38`:
`{"kind":"in_transition"}` where another invocation holds the agent or the
harness answers it, and with `constituents` where a run holds the run lock
and no worker answers `Idle` or `Active`, or, per `toddwbucy/WeaverAgent#161`,
where the run lock is free and an ended run's processes still stand in the
agent's cgroup, `Unloaded` printed only once none stands; landed beside the
last state per section 2.12); a turn's event
ordered behind an answer is a transient the next event corrects. So a
writer that never idles never holds the link down or a verb back, and the
operator can always unload a runaway agent. **The residue narrows to the
order inside those windows**: the drain's, and an opening's under a header
naming no length or behind a relay that stalls. Section 8's one-connection
paragraph refers here for what a reconnection carries. **Every event
relayed from behind the file's tail at a reconnection is marked as replayed
on the link, and the replay ends with a frame admin-con sends when it
reaches the boundary**, immediately after the hello's answer where there
is nothing to replay; the server classifies every event before that frame
as replayed and every event after it as live, the boundary's offset rule
and the client's flag standing as checks that log a disagreement. **A
position is the byte after its record, so the event whose position equals
the boundary is the last one behind it, and an event beyond the boundary
is one whose position is past it**; the offset rule holds marks to this as
it holds records, since a mark carries the position relaying resumes at.
The
frame decides because the stream's order cannot: a file rotated after the
hello before any event of the boundary's generation reached the boundary
would leave every live event of the new generation looking like an older
generation's tail. Replayed events reach the server's live window only,
which says of each event whether it was replayed: they write no member of section 2.12's row, whose tuple and load state come
from `show` answers and live events alone. On every admission of an
admin-con connection whose door is open, and at every later opening of the
door per the rule above, the server first fixes the replay boundary, the
file position admin-con reports as its tail at that moment, so that every event
beyond it is live and every event behind it is replayed, in the reading of
a position above, and then,
where the agent's ceiling grants `show`, asks `show` for the row, so the
answer is admin's word from after the boundary and nothing that lands in
the file between the two can be classified as history, per section 2.12.
**The admission's `show` is required only where the ceiling grants
`show`**, per the operator's rulings of 2026-10-02 and section 8. Where it
does, it must land a usable observation within the silence bound: until it
does the row carries the last process's tuple and load state, and an
admitted connection standing on them is presence asserted on stale facts,
so a connection whose `show` errors, answers no state, or has not answered
by the bound is closed with a typed refusal (`admission_incomplete`) and
the reconnection asks again. **A landing is never polled once its
deadline has come**: the listener reads the deadline, kept as an instant,
at every poll of the landing and before it, since a single-statement write
that goes out after the deadline commits whether or not its answer is
waited for, and the row would carry a `show` whose admission the server
refused. That holds for a landing handed a deadline already past and for
one still waiting when the deadline comes, on the pool for a connection or
anywhere short of its statement. A connection closed as incomplete has
written none of that answer, and the reconnection's `show` lands in its
place. A statement that goes out before the deadline and is answered after
it still commits under a connection closed as incomplete, a window of one
statement's flight that section 10 holds open. **The rule is every bounded
write's** (the Planner's default of 2026-11-12, after a sweep of the
server's store writes under tokio's timeouts): each first record, outcome
and refusal the listener writes under a bound (section 2.13), the `failed`
outcome of a first whose frame never left among them, and each write of the
alerts' worker and drainer (section 2.14) reads its deadline before its
write is polled, through one helper (`crate::deadline::before`).
A write still waiting when its deadline comes, on the pool, on a lock or on
an earlier statement's answer, is then dropped unsent, so a caller that
reports a write as not made at its bound is telling the truth, the same one
statement's flight excepted. Where the ceiling does not grant `show`, the
server asks nothing, admission completes at `caught_up`, and the row's
tuple and load state stand on live trace events alone, the row naming that
source and its date per section 2.12, so a surface says the state is
unconfirmed since the reconnection rather than presenting it as current.
Asking a verb outside the ceiling to complete an admission would be the
defect section 8 refuses. **admin-con emits everything it
sends over its connection, verb answers and trace events alike, as one
ordered stream**, and a verb answer takes its place in that stream at the
invocation rather than at the receipt: before invoking a verb admin-con
drains the relay's stream to its next heartbeat, per the measure above,
emitting every event read, then invokes with the stream unread, receives,
emits the answer, and reads on. The drain
is what makes "every event read before the invocation is ahead of the
answer" true of every event written before it and not only of those
admin-con happened to have read: without it an unread older load event
behind the tail would be emitted after a newer `show` answer and overwrite
it. Every event read after the receipt is behind the
answer, and nothing is read in between, so the snapshot the verb took somewhere inside that
span is newer than everything ahead of it and older than everything
behind. This holds for every verb answer and not only the admission-time
`show`. **Only a `show` asked by the server principal is served during the
replay**, the admission's among them, at once and between the replay's
frames, so it waits behind at most one, with no
drain, since held behind a long backfill it would miss the admission's
deadline. Its snapshot is taken after the boundary, and every live event
written before its invocation is relayed after its answer, in order: the
row may briefly read a state older than the snapshot, but it converges to
the snapshot's state, because the last of those events is what the
snapshot saw. A transient regression in a re-confirmation is acceptable,
and an inversion around a person's load or stop is not, so **every other
ask waits for `caught_up` and then takes the drain**: before `caught_up`
nothing live has been read, but a record appended after the hello is live
and merely unread, and an ordinary verb answered during the replay could
overtake it. admin-con knows the exempt ask by the frame alone, a `show`
asked by the server principal, which may ask only observation verbs: any
such re-confirmation acts on nothing and converges the same way, so the
rule needs no knowledge of where the admission's `show` falls among the
asks. The connector's socket holds a bounded number of
bytes unsent, so on a slow link the answer waits behind no send buffer
grown to megabytes either. **admin-con runs one verb at a time**, in one
invocation slot per admin-con process and not per connection, each from
its pre-invocation drain, where it takes one, through the emission of its
answer, and a second
ask that arrives while one is in flight waits its turn in arrival order,
so invocation spans never overlap and the ordering above holds for every
answer; two verbs run at once would let an older answer be emitted after a
newer one. The gate contract's rule for the data plane, one turn in flight
per agent and a second request waits, is the same shape on this plane. The
pause and the wait are bounded because the invocation is, the invoker's
bound capping it, and the drain by the relay's next heartbeat, which only
the measures' residue withholds. A relay that drops admin-con while it
reads nothing, during a verb, costs nothing: admin-con redials from its
position, which the relay verifies, and reads on.

**The answer and the invocation have named bounds**, per
`weaver-admin-operator-contract` section 3. A command line prints one JSON
object on standard output, at most 64 KiB. Exit 0 is an answer and 1 a
refusal; any other status, or a status with no object, is a fault the caller
answers by reading the next `show`. Standard error carries diagnostics no
caller parses. **`load` answers once the agent is up or refused**, within
the box's own bound, 900 seconds unless the agent's root names another, and
**`unload` takes at most 105 seconds**, per `weaver-admin-Spec` sections 2
and 3. **admin-con's verb bound is a member of its config that the install
sets above the box's load bound**, `verb_bound_secs`, 960 unless set, not a
constant of this crate. **A verb runs to completion**, per the operator's rulings
of 2026-10-03 on `toddwbucy/WeaverWeb#14`: an invocation finishes even when
its caller disappears, so at its bound admin-con leaves the process running,
answers `unknown`, and the next `show` reads the real state. **The
invocation slot stays occupied until that process exits and is reaped**,
even after its caller was answered `unknown`: verbs that arrive meanwhile
queue behind it under the existing bound and `busy`, and admin-con's
orderly stop waits for it too, per section 8. Answering at the bound and freeing
the slot would end the one-verb-at-a-time span while the verb still ran, so
the next verb could run beside it. **The slot is admin-con's own, one per
process, and is held across connection attempts**: a link that drops while
a timed-out verb's process still runs leaves the slot occupied, so a verb
asked on the fresh connection, admitted at once with a closed door, queues
behind it under the same bound and `busy` and never runs beside it. **The
bound counts ordinary asks only**: a `show` the server asked is never
answered `busy`, since it completes an admission or an opening and the
server asks at most one for each. The box has its own guard beside this
one: weaver-admin holds an invocation lock for the whole of every verb but
`show` and refuses a concurrent one `InvocationInFlight`, per
`weaver-admin-Spec` section 3, so the box would refuse the overlap, and
admin-con's slot is what keeps this crate's ordering of answers against the
trace. A dropped verb
answers `unknown` and never a guess at what admin did, and a verb lost with
the link answers unknown too, since it may have run. **A verb counts as
started only once its invocation begins.** So a verb still waiting behind
the one in flight at admin-con's stop, or one taken from the queue and still
in its pre-invocation drain, was never invoked, and admin-con answers it
`not_started` while the link still stands, and its caller knows it did not
run. The invocation checks the stop as its first act, so a verb pushed but
not yet begun when the stop came is declined rather than run, and only a
verb whose run began is waited for, within its bound, and, outlasting it,
answers unknown; on a link loss nothing can answer it and the server cannot
tell it from the verb in flight, so it answers unknown. **Every position admin-con records
or acknowledges is a record boundary, the byte after a delimiter**, so the
drain before a verb ends at the end of a complete record, since the relay
heartbeats only between records, and a record still unterminated at that
moment is read after the answer and ordered behind it. That order
is right rather than a concession: an event is in the file only once its
delimiter is, so a record unterminated at the invocation was not yet
written when the verb ran and is concurrent with the snapshot rather than
before it, which is what the gate contract's own framing rule says of a
line without its delimiter, and what the seed's `traceview.rs` already
says of an unterminated tail. A byte tail recorded inside an unfinished
record would make neither draining through it nor resuming from it well
defined. Placed at the receipt
instead, a trace event read between the verb's snapshot and the
answer's arrival would stand ahead of the answer and the older snapshot
would overwrite it. The server's arrival number then carries the source
order, so a `show` answer still in flight when a load or unload lands
after the boundary cannot reach the row after the live event and overwrite
it, which two sources ordered only by server arrival would allow.

```graph
node: web-trace-file-is-replayed-from-the-acknowledged-position
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-trace-file-is-replayed-from-the-acknowledged-position
```

**The observation answers from any position, a running turn included**, as
of 2026-09-05. It is served from inside the turn between tokens, touching no
bracket and cancelling nothing, and the one bound is the single token whose
decode is in progress. So **a roster may read while an agent is answering**,
and a surface has no reason to withhold a status read during activity or to
present a stale one as current. A view built against the earlier posture
would have designed around a refusal that no longer stands.

No verb chains another. This crate offers each as a separate act and nothing
composite.

**`validate` judges only what is placed on the box**, per the operator's
ruling of 2026-10-02 on `toddwbucy/WeaverAgent#59`: declarations are the
operator's to place, `validate` takes only an agent's name and judges the
declaration installed for it, and no verb judges a supplied declaration. So
the Compose surface keeps drafts and produces a declaration file for the
operator to install, judging nothing itself, and **a `validate` answer is a
reading of the installed declaration and never of a draft**, meaningful for
a row of section 2.4 only where the operator installed that row's
declaration. What it can answer is bounded: it transitions nothing, refuses
an incoherent declaration naming the field, and reaches **the box facts
admin holds custody of**, per `weaver-admin-PRD` section 4.3 as ruled
2026-09-05 on issue #456: admin adjudicates what it provisions, asks the
owner where one can be asked before a process exists, and leaves to the
organ what only the organ can judge. **A clean `validate` is acceptance for
filing rather than approval to load.** Whether the artifact resolves,
whether the family exposes the taps the declaration elects, and whether
these weights load at this precision are answered at admission under the
agent's identity and not here. **This crate therefore carries no second
copy of the rules**, only a copy of the declaration's field shape written
against `weaver-types-Spec` section 2 **at a named corpus commit**, which is
the draft's staleness rule: when the floor moves, the pin says so, and a
draft written against the old floor is named stale before it is offered for
install. **The one presence it judges is the tunable set** (section 6.1):
every name a serving load requires at the pin, read as a dated copy of the
agent's three places and held to the screen's set by a test, with each
value's range, so the derived file is never refused for a missing or
out-of-range tunable value. Every other rule stays the organ's.

### 7.3 The analysis stream

The emission this crate ingests. **Its shape is
`weaver-analysis-web-contract`**, landed 2026-09-05 per issue #418, and this
document names the seam and restates none of it. Two of that contract's
clauses reach back into this document and are worth naming where a reader
of this section stands.

**The series is addressed by the ordinal and this crate's store by the
position**, per section 2.1, and the conversion is this crate's to make at
ingest, section 3.1's, which is unbuilt. **What it converts from crosses on
the summary stream**: the contract's summary carries the resident count at
a generation's close and its output token count, and the emitter writes
both. **Where a generation's resident count is absent, its positions cannot
be keyed**, and what crosses is stored without an address derived from a
figure that did not answer.

**A diagnostic record whose bracket did not close certified is not drawn**,
the refusal naming the outcome. A serving record has no gate and none is
owed.

## 8. Placement and the link

**Three processes: the server, and two connectors beside each agent.** The
server holds the presentation stack, the store and the listener. gate-con and
admin-con hold the box-bound reaches of sections 7.1 and 7.2 and stand on the
box the agent runs on, by the construction section 7.1 names. The server
never reaches an agent, so the one party that does is a client of this
crate's own, and **both connectors are clients of the server**: they connect
and the server listens, per the operator's ruling of 2026-10-01. Nothing of
the interior crosses by either plane. The seed's one dialed link between two
colocated processes, which this section described until 2026-10-01, is
superseded. **This is the bulk of the backend**, on the operator's word of
the same date: everything after it is presentation of what comes out of the
two connectors.

**Each connector runs as a dedicated service user**, one per agent and plane
and never the operator's own uid, per the operator's rulings of 2026-10-02
and 2026-10-03, and holds only what its plane needs. **admin-con's user
holds the agent's access group and the lines the box's sudo rule grants it,
and nothing of the agent's own**: WeaverAgent's `create-agent.sh` creates
that user, the rule and the trace-reader declaration, per
`toddwbucy/WeaverAgent#79`. **gate-con's user holds what the box admits it
by for the gate, and nothing else**, as `toddwbucy/WeaverAgent#70` rules: an
entry in the declaration's allow list alone does not reach the gate's
socket. This document names no group, socket path, mode or uid for either.
**One privileged invocation stands in this crate, admin-con's sudo
invoker**, per section 7.2; no other sudo, no root wrapper and no setuid
binary stands anywhere in it, per section 9's row. Placing each connector's
config at 0600 under its own user is the install's, and carries no box path
into the repository.

**Nothing in the read or write path is box-bound to the agents.** The reader
is a store client, the runner is a queue consumer, and the front end with
its store runs on one machine while the agents run on another. That crossing
is a declared boundary under the charter's section 5 rule and appears in the
trial record like any other.

**The link is mutual TLS with the server as its own certificate authority.**
Registration mints one client certificate per connector, two per agent, each
bound to the agent's row of section 2.12 and to its plane, so a gate-con
credential cannot speak as admin-con. The client's config carries its own
key and the server's certificate; the server stores the fingerprint of each
client certificate and never the key, per section 2.12. The client holds the
server's identity and the server holds the client's, which is the operator's
two-way confirmation, and the traffic is encrypted by the same handshake, so
**nothing crosses the link in the clear** and no plaintext accept path
exists. Where a certificate and key are configured, the browser's listener
serves TLS only, per section 2.13; the link's encryption is its own and
holds without configuration.

**The server's authority is durable state and is never minted at start.**
Its key and certificate are created once by an explicit register verb, the
first of them, run before any agent is registered; they are stored at a
path the server's config names and never in a repository, loaded before
the listener starts, and **the server refuses to start rather than mint a
new one when they are absent**. An authority minted at start would strand
every installed connector after the first restart, since each client
config pins the server's certificate and each credential is signed by that
authority. Rotation of the authority is its own register verb and, because
every client config pins the old certificate, it is by definition a
re-registration of every agent, said plainly rather than hidden.

```graph
node: web-servers-authority-is-loaded-and-never-minted-at-start
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-servers-authority-is-loaded-and-never-minted-at-start
```

```graph
node: web-nothing-crosses-the-link-in-the-clear
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-nothing-crosses-the-link-in-the-clear
```

```graph
node: web-client-credential-stored-as-fingerprint-never-key
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-client-credential-stored-as-fingerprint-never-key
```

**Keys are made on the server at registration and handed to the client
through its config file at setup**, per the operator's ruling of 2026-10-01.
Registration is a register verb run on the server: it writes the row of
section 2.12, mints both credentials, and writes a client config to a path
the operator names. An install script carries that config to the agent's
box, and nothing of it enters any repository. **A rename is a new
registration**, the name being immutable per section 2.12, and the old row's
credentials are revoked by it, as they are by any re-registration of a live
box and name, so one agent never holds live credentials on two rows. **A
credential is bound to one instance of this server**: it names the server
whose authority signed it and no other, so moving an agent to another
instance is a re-registration there rather than a copy.

**The hello is refused before its roster is read when the credential is not
live in the register.** A connection presents its certificate at the
handshake; the server looks the fingerprint up in section 2.12 and refuses
one that is absent or revoked at that layer, before any byte of the
connector's roster, its agent name and plane, is read. The order is the
point: a roster read first is a claim the server has to hold while deciding
whether to believe it, and the gate's own contract refuses a failed peer
before any content is read for the same reason.

**Identity is the certificate's binding, and the hello carries no identity
claim the server acts on.** Registration bound the certificate to one row
and one plane, so the server knows which agent and which plane is connecting
before the roster arrives, and the roster's name and plane are a check and
not a source: a hello naming an agent or a plane other than its credential's
is refused as a mismatch and logged against the row. The seed's link took a
names-only hello and let the first one win, which is the failure this
sentence keeps out: a credential in two hands could otherwise name any
agent, and the register would hold a fact the certificate never vouched for.

```graph
node: web-link-identity-is-the-certificates-binding-never-the-roster
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-link-identity-is-the-certificates-binding-never-the-roster
```

```graph
node: web-link-refuses-a-credential-not-live-before-the-roster
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-link-refuses-a-credential-not-live-before-the-roster
```

**Every verb asked of a registered agent passes three gates**, per the
operator's rulings of 2026-10-02 and 2026-10-03, and each is held by a
different party; the
register verbs, which act on the server and reach no box, take section
2.13's own path. **First, this crate's identity and
access**: a person authenticated to the server, holding a grant on the agent
whose role permits the verb, or the server itself asking an observation verb
for its own purposes, per section 2.13, checked on the server before any ask
leaves it. **Second, the ceiling**: the verbs the box's sudo rules grant
admin-con's user, which admin-con derives by asking sudo, for each exact
line, whether it is granted (`sudo -n -l`), declares in its hello, and the
server never exceeds. **Third, the box's sudo rule itself**: a line the rule
does not grant never reaches weaver-admin, sudo refusing it, per
`weaver-admin-operator-contract` section 5. **A turn on the data plane
passes the first gate and then the gate's own admission**: the person's
grants on the agent must permit `turn`, and on the box gate-con's user is
admitted as `toddwbucy/WeaverAgent#70` rules, which is the box's gate for
the data plane. No
verb ceiling applies to a turn, since gate-con declares none. Each gate bounds what the one before it can
reach, so a fault in one is caught by the next rather than becoming the
box's. The link between them is already cryptographic per agent and per
plane by the rules above: the server knows which agent and plane each
connection is, and each connector knows it is talking to its own server.

**The ceiling is declared in admin-con's hello.** admin-con's hello names
exactly the verbs the box's sudo rules grant its user, each found by asking
sudo whether that exact line is granted without running it, per section
7.2. admin-con keeps no list beyond the verbs it knows, so the rules are the
single source, and **there is no `grants` ask and none will be added**, per
`toddwbucy/WeaverWeb#14`. **An agent whose rule grants nothing declares an
empty ceiling**: the server asks nothing, and admission completes at
`caught_up` per section 7.2. The empty ceiling is the honest declaration and
not a placeholder: it is what the rules grant today. **The server holds it with that live
connection, fixed for the connection's life, and checks every ask against
the ceiling of the connection the frame will be enqueued on**, so a
reconnecting admin-con that narrows its ceiling cannot race an ask. It also
stores a copy on the row as an observation with its date, per section 2.12,
which is what surfaces read and never the authorization input. It **treats
the ceiling as an upper bound and never a grant**, and never asks a verb outside it: a person's grant
cannot widen the ceiling, and the ceiling grants no person anything. **An
ask outside it reaching admin-con is the server's own defect, and admin-con
answers it**: a typed error on its verb answer naming the ceiling, the ask
logged against the row, nothing run, and the connection kept. It is not a
refusal frame, which is the server's typed close of a connection and travels
from the server only, and closing would put a compromised server's connector
into a reconnect loop and gain nothing, the ask having been refused already.
**The reason is the server's position**: the
connectors trust their server by design, so a compromised server reaches
whatever a connector can do, and the ceiling, which the box sets and the
server only reads, bounds that to the verbs the box chose. The declaration
is the box's statement about itself and not its authorization, which stays
the sudo rule's: a ceiling that over-declares is still refused at the third
gate.

**The server reports a `show` it asked landed, in a `landed` frame naming
the ask's id**, at an admission and at every opening, sent only once the
store took that `show`'s observation, and **delivered or the connection
closes**: it is enqueued as an ask is, held to the silence bound, and a
write path still full at the bound closes the connection, so admin-con's
hold ends with it and the reconnect admits again rather than holding every
ordinary verb on a connection the server keeps. A landing that fails or stalls, or an
answer that carries no state, gets no `landed`, and the connection closes
`store_unavailable` or `admission_incomplete` instead. It is the one signal
that ends admin-con's hold on ordinary asks, per section 7.2, since a
connector's own send of the answer only queues it and says nothing of
whether the server took it. admin-con ignores a `landed` for any other id.

**A connector's entries ride the link as their own frames** (section 2.14):
`entry`, from either connector, carrying its run, drawn at the connector's
start, a sequence from one, its source and the entry's object, and
`entry_ack` from the server, naming the run and the sequence it took
through. An entry is resent until acknowledged and taken once; its pump is
the connection's own task, so no read of the trace, no answer and no turn
waits on it, and an acknowledgement never blocks the server's read loop.

```graph
node: web-server-never-asks-a-verb-outside-the-ceiling
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-server-never-asks-a-verb-outside-the-ceiling
```

**Each verb ask names its principal, as a claim**: the requesting person, or
the server for its own asks, per section 2.13. **The frame keeps the member,
and admin-con passes nothing of it to admin**: it serves admin-con's own log
and the row, and the box records the uid sudo reports, admin-con's own, per
section 7.2. `toddwbucy/WeaverAgent#51`, which would have carried it to
admin, is closed as superseded. **It is never an authorization input
anywhere on the box**, and section 2.13's audit is the one record of who
asked.

**One connection at a time per connector, ever.** A second connection on a
credential already connected is refused rather than replacing the first, per
the operator's ruling of 2026-10-01. Replacement would let a credential in
two hands displace the live connector silently, with the row reading
connected throughout; refusal makes the second hand visible as a refusal the
server logs against the row. A reconnection on the admin plane carries the
replay of section 7.2: within one server process the hello's answer names
the acknowledged position and admin-con resumes from it, so a link drop is
a delay in the server's window and never a hole in it, and across a server
restart the window begins at a marked discontinuity.

```graph
node: web-one-live-connection-per-credential
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-one-live-connection-per-credential
```

**A heartbeat on the link lets the server close a silent connection after a
bounded interval**, so a dropped connector can reconnect under the rule above
rather than being refused against its own dead connection. **The silence
bound is the one tunable, a member of the server's config and set nowhere
else**: the server tells each connector its send cadence in the answer to
its hello, and the cadence is the bound divided by four, so an operator who
changes the bound changes both and a client can never be configured to send
slower than the server tolerates. **The answer names the bound itself too**,
`silence_secs` beside `cadence_secs`, which a connector checks the cadence
against; an answer naming a cadence and no bound, or a bound under four
cadences, is a protocol fault. No connector times an opening's hold by it,
since that hold runs on no clock of the connector's. The server refuses a bound under four
seconds or over four days, so the cadence lies between one second and the
one day a connector accepts, both ends holding the ceiling as one constant. **This document elects the bound at sixty
seconds, as the planner's election of 2026-10-01 and not the operator's
ruling**, which puts the cadence at fifteen. The reason: a turn through the
gate takes seconds to minutes and never rides the heartbeat's path, so the
bound answers only how long a dropped connector shows as present on every
surface. A minute is short enough that a surface is not wrong for long and
long enough that a link's brief loss does not churn the row. The operator
confirms or resets it in review.

**A connector reconnects on its own and never exits on a refusal**, the
client side of the rules above, elected by the act that built gate-con on
2026-10-01 and not a ruling. After any end of its connection, a loss or a
refusal, a connector closes every exchange that connection carried, since
none of them can answer on it anymore, and dials again after a backoff
that starts at one second and doubles per failure, with jitter, capped at
sixty seconds; a connection that stayed admitted for one cadence resets
the count, so a server restart is met at once and a server that drops
every connection at admission is backed off. **`not_live` and
`roster_mismatch` mean the credential is revoked or wrong, and a server
certificate the pinned authority did not sign means the server's authority
was rotated; all three are retried only at the cap and never faster**, each
logged with what the operator must do, which is to re-install the agent's
config from a registration or a rotation. **The connector re-reads its
config before each retry at the cap**, under the same trust rule as at
start, so a config re-installed at its path is dialed with at the next
attempt and no restart is needed; without the re-read the guidance could
never take effect. **Only the link's members change at a re-install**: a
re-read config naming another agent or plane than the one the connector
runs for is refused and logged, and the credential in hand kept, since the
rest of the connector (gate-con's socket, admin-con's relay socket and
invoker) stays bound to the agent it started for, and dialing as another
would file one agent's traffic under another's row. **A client config
names its row by identity**: `register` and `rotate` write the row's
identity beside the agent's name, rotation keeps it since it keeps the row,
and the re-install check compares that identity and never the name, which
another box may share. Every other refusal retries on the normal backoff,
`malformed` and `wrong_plane` logged as the connector's own defect. **It
does not exit**, because a supervisor would restart it into the same loop;
it ends only on its own shutdown, which lets exchanges in flight finish
within a short grace before closing the link; asks still waiting are
answered `not_started` before the grace begins, and a turn still in flight
at the grace's end is answered to its caller as unknown, per section 7.1;
admin-con's stop is the orderly stop below. **gate-con's turns in flight
are bounded, and so are the asks waiting behind them**; an ask arriving to
a full queue is answered at once with the fault `busy`, gate-con's own
back-pressure and not one of the gate's kinds of section 7.1, so a surface
renders it as the connector's load and never as the agent's word. **A dead server is noticed within a bound and
not only by its close**: every write the connector makes, its heartbeats
included, is held to the cadence, and TCP keepalive with a user timeout of
two cadences ends a connection whose peer stopped acknowledging. A
connection that is merely quiet is not dead: the server sends nothing
unasked on the gate plane, and its kernel still answers.

**The agent's lifetime is its own scope's, and admin-con's end ends neither
the agent nor a load's start step**, on the operator's ruling of 2026-11-07
on `toddwbucy/WeaverAgent#166`, per `weaver-admin-operator-contract`
sections 2 and 3. That ruling replaced the binding of 2026-10-03 on
`toddwbucy/WeaverWeb#15`, under which stopping an agent's admin-con stopped
that agent: admin's start step now makes a scope of root's for the load,
delegated, and the agent lives there until `unload`, `force-unload`, root
stopping that scope, or its own processes ending, whoever invoked the load.
Each agent still has its own admin-con. **admin-con's service no longer
contains the agent**: the limits of its control group do not reach the
agent, and what does reach it is admin-con's execution context, which no
move between control groups changes (the contract's section 2), so section
8.1 names what admin-con's unit leaves unrestricted. An upgrade of
admin-con is a stop, the upgrade and a start, with the agent running
throughout. **admin-con is a conduit, and its stop asks nothing of the
agent**, on the operator's ruling of 2026-10-14 (section 2.14, ruling 3),
which retired the orderly stop's `unload` and its retry until rest of
2026-10-08: admin-con answers its waiting asks `not_started`, waits for the
verb in flight, until its process exits and is reaped, within the stop's
grace, never killing it, and exits, invoking nothing. It parses no
refusal class of admin's. A verb whose process outlasts the grace runs on
unkilled as far as admin-con is concerned; what the unit's own end does to
it is section 8.1's. **The stop's grace is a member of admin-con's config**,
`stop_grace_secs`, 1080 unless set: the box's load bound, the unload bound
and a margin, timed from the stop's signal, so a stop that meets a `load`
in flight waits for it, and the unit's stop timeout stands above the grace
(section 8.1). The server sees admin-con's end as lost contact, raised to
the operator, the agent still running. An operator who wants the agent at
rest before a stop unloads it first, through section 2.14's path.
**The containment is a property of how the box is provisioned, and
admin-con enforces none of it**, on the operator's ruling of 2026-10-05.
admin-con is a connector that abstracts the agent's primitives and polices
nothing on the box: it does exactly three things, runs a granted line when
asked, reads the relay, and passes the lines of `admin.log` and its own
failures upstream as entries, and acts on its agent on its own initiative
never. There is no adversary between admin-con and weaver-admin, both being
the operator's; the one adversary is a hijacked management plane, which the
containment answers by provisioning: the agent's scope and every containment
above it are root's, so no account but root, admin-con's included, can move
a process of the agent out of it (`toddwbucy/WeaverAgent#166`'s guard, the
operator's ruling of 2026-10-15), and admin's start step refuses
`BoundaryUnverified` where the host does not let it make that scope (the
contract's section 2, `weaver-admin-Spec` section 6.1). A check at runtime
in admin-con would guard only against a misprovisioned box, which is the
install's question, so **the install checks once, after the first load**,
that every constituent `show` names sits in the agent's own scope and none
in admin-con's service, as the contract's section 2 asks of the invoker
(section 8.1). `show`'s pids reach the row as a fact for the operator and
for that check, per section 2.12, and drive no behaviour of admin-con's;
`show` carries them on WeaverAgent's side, per `weaver-types`
`LifecycleAnswer::State`. **Without a service manager that makes such a
scope, there is no containment and no load**: admin's start step refuses
rather than start an agent nothing contains. The gap is the box's, and is
stated rather than imitated.

**A lost link never unloads an agent**, on the operator's confirmation of
2026-10-03 on `toddwbucy/WeaverWeb#15`: a network disruption must never
cause an agent failure, so the management plane is admin-con's process and
not its connection, and a server restart takes no agent down. The trace is
the record of what the agent did meanwhile, and the server catches up from
the acknowledged position when the link returns, per section 7.2. **Recovery
decisions are the operator's**, made through this crate's surfaces and
never chosen on their own, per section 2.14's ruling 1, and reach admin-con
as ordinary verbs, `show` and then `unload` and `load`.

**At listener start the server sets every plane recorded as connected to
disconnected, with the start's date, before it accepts a connection.** A
plane already disconnected keeps its date, so a surface still shows how long
a connector has been gone. The listener's epoch of section 2.12 is
incremented in the same act, so every observation this process lands orders
above every one the last process left. **The reset is not an observation and is not
subject to section 2.12's ordering rule**: it is the listener's own act on
its own state and is unconditional for every plane recorded as connected,
so a wall clock that moved backward across the restart, leaving the reset's
date no newer than the stored connected date, cannot leave a row connected
with no socket behind it.
The link state of section 2.12 is persisted in the row and presence is
derived from it, so without the reset a server that died and restarted
would find both planes recorded connected with no socket behind either,
and the heartbeat cannot clear a connection that does not exist to go
silent. With it, **presence is only ever asserted by a connection this
server process admitted**, and a connector that was up through the restart
reconnects under the heartbeat's rule and is recorded again by the process
that is actually holding it. **One listener per store, held at the store**:
before the epoch and the reset the listener takes a session-level advisory
lock on a constant key of this crate's, holds it for its life, and refuses
to start when it is held, naming the other listener. The one-connection
rule lives in a process's live set, so two listeners on one store would each
admit the same credential, and the second's reset would mark the first's
connections disconnected; the lock is what makes the rule a store's and not
a process's.

```graph
node: web-link-state-is-reset-when-the-listener-starts
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-link-state-is-reset-when-the-listener-starts
```

**An agent is present only when both its connectors are connected from
credentials on its row.** An agent must have both connectors on the same
server and they must match, per the operator's ruling of 2026-10-01:
positive identification of an agent to a server is both keys. Each
connection is bound to one row by its certificate, so the server never has
two planes of one agent to correlate across rows: a gate credential of one
row and an admin credential of another are, to the server, two rows each
missing a plane, and nothing authenticated says they were deployed
together. So there is no cross-row mark to set. The row says which plane is
missing, with the date, and every surface renders that rather than
presence. One plane alone is not presence: an agent with a live gate-con
and no admin-con has a tuple and a load state nobody has confirmed since
the link last dropped. The address each connection came from stands in
section 2.12 as an observation a surface may show, and never as a mark.

```graph
node: web-agent-present-only-when-both-planes-match-one-row
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-agent-present-only-when-both-planes-match-one-row
```

**The tuple and the load state arrive through admin-con only**, by `show`
and by the trace's events, per section 7.2. gate-con is forbidden by its
contract from learning any of it, so a server that accepted either from the
data plane would be writing a fact from a party that cannot know it, and
section 2.12's row would carry a value with no source. The row's tuple and
load state are the agent's side's: a `show` answer is admin's word and a
trace event is the agent's own record, each with its own date, and neither
is ever the data plane's. The node below keeps its identifier, since its
claim, that the tuple comes by admin-con and never by gate-con, has not
moved.

```graph
node: web-tuple-is-admins-word-and-never-gate-cons
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-tuple-is-admins-word-and-never-gate-cons
```

**Revocation is per credential and server-side, and rotation mints a fresh
pair.** Both are register verbs, run as the host's commands or through the
admin's register page per section 2.13, and retire runs through the page
alone; each is an authored edit under section 3.2, ordered on the
row's version, so a concurrent revoke, rotate or re-registration refuses on
a stale version rather than overwriting a fingerprint or a state. Revoking
sets one credential's state in section 2.12 and the next hello on it is
refused before its roster is read, and rotating mints two new credentials
for the row, revokes the old two, and writes a new client config for the
install script to carry. Per the founding handoff's rule that credentials are
revocable one at a time, and the operator's ruling that both connectors must
match, which is why rotation is a pair and not one.

**Revoking a credential closes its live connection at once, and this is
this document's election of 2026-10-01 and not a ruling.** The reason: a
revocation is the operator saying a credential is no longer trusted, and a
connection that stays up until the heartbeat's sixty seconds expire, or
until the connector chooses to reconnect, is sixty seconds or longer of a
party the operator distrusts still relaying. So the server closes the link
in the revoking act, the row's link state for that plane reads disconnected
with the act's date, and the agent is no longer present under the rule
above. Rotation therefore drops both planes until the install script carries
the new config, and the row reads disconnected on both planes in between.
The operator confirms or resets this in review. **Admission and revocation
are serialized per credential**: a connection is installed as the
credential's live connection under the same exclusion the revoking act
takes, so the revoking act either finds the connection installed and
closes it, or runs first and the admission rechecks liveness after
installing and closes itself. Without that, a revocation racing a
connection that had passed the live-fingerprint lookup but was not yet
installed would find nothing to close, and the admission would complete on
a revoked credential and relay until the next hello. No interleaving leaves
a revoked credential relaying. **The exclusion is the store's own row lock,
and the revoking act's close rides the store's notification**, as built on
2026-10-01: the host's register verbs run as the operator's process, the
page's in the server's own, and the listener as the server's, and the one
exclusion every path shares without a second channel is the agent's row
they all write, so every admission, teardown and revocation takes the
agent's row for update and mutates the listener's live set while it holds
it, and every revocation raises the notification the listener hears and
closes on. A second channel between the
two processes would be a second thing to configure and secure for one
message the store already carries.

**A link-state write is bound to the connection it describes, under the
same exclusion.** Every connection the listener admits carries an
incarnation of the listener's own, a link-state write names the incarnation
it describes, and a disconnect lands only while that incarnation is still
the credential's live connection; admission, the connected write, teardown
and the disconnected write all run under the per-credential exclusion the
paragraph above takes. Without the binding, a dropped connection removed
from the live set and replaced before its socket's teardown finished would
have the replacement record connected and the old teardown then land
disconnected with a higher arrival number, so the row would read the plane
missing while it was relaying.

### 8.1 The install

**This repository carries the install's units and a tool that stages them,
and nothing that applies them.** Until 2026-11-11 the server and both
connectors had only run as units typed by hand at a shell, so their
properties were written down nowhere here and nothing survived a restart of
the machine; the act of that date (`docs/project/brief-2026-11-11-the-install.md`)
writes them down. Three rules from the hard boundaries shape it. **No
location, account, group or socket is written into the repository**: each is
a named parameter with no default, read from a file the operator keeps
outside it. **No privileged program stands in it**: the staging tool runs as
the operator's own user, refuses to run as root, and writes a directory the
operator applies; the one privileged invocation stays admin-con's sudo
invoker, per section 7.2 and section 9's row. **No account, group or sudo
rule is created**: those are WeaverAgent's (`create-agent.sh`), so the
install requires them and refuses by name where one is missing at apply.
Whether the repository's own locations may become overridable defaults, and
whether a script the operator runs as root to apply the staged files stands
outside the one-privileged-invocation rule, are the operator's, named in
section 10.

**Three units, two of them templates.** `weaver-web.service` runs the
server. `weaver-web-gate-con@.service` and `weaver-web-admin-con@.service`
run the connectors, the instance being the agent's name, so one file per
plane serves every agent on a box and each agent's connectors start, stop
and fail apart. A template is one text to review and to verify, where
units generated per agent would need a generator and its tests for no gain
while the connectors' accounts are the box's and not the agent's (the
Planner's default of 2026-11-11, recorded as such and not as the operator's
ruling). The templates stand under `install/units/` with each location and
account as a placeholder the staging tool fills, and it refuses a
template carrying a placeholder it does not fill, so none stages
half-rendered.

**What every unit carries, and why:**

- **`User=`, the unit's account parameter, never the operator's uid**, per
  the operator's rulings of 2026-10-02 and 2026-10-03 for the connectors;
  the server takes the same rule as a system unit under its own account
  (the Planner's default of 2026-11-11), so the console survives a restart
  of the machine with nobody signed in. No `Group=` is set: each runs with
  its account's own groups, which the box gives it, and section 8's service
  users hold exactly what their planes need through them.
- **`UMask=0077`**: every file a program creates is its own alone, the
  server's authority and operations log among them.
- **`Wants=` and `After=network-online.target`**: the server listens on
  configured addresses and reaches its store, and each connector dials the
  server. Where the store is local, ordering after it is the box's, by a
  drop-in.
- **`Restart=on-failure` with `RestartSec=5`**, a new behaviour for the
  operator's ruling (section 10): a connector that exits on a fault comes
  back on its own, and its own reconnect policy (above) carries it from
  there; a stop asked of the service manager is not a failure, so a stopped
  unit stays stopped.
- **`WantedBy=multi-user.target`**: an enabled unit starts at boot, the gap
  the brief measured.
- **`ExecStart=`** the program under the binaries location with `--config`
  naming its config: the server's at `<config root>/config.toml` with
  `serve`, a connector's at `<config root>/connectors/<agent>/gate-con.toml`
  or `admin-con.toml`.

**What admin-con's unit adds, and why:**

- **`KillMode=mixed`**: a stop signals admin-con alone, so the one process
  that knows a verb is in flight decides how long to wait, within its
  grace (section 8). What remains in the unit after admin-con exits is a
  verb that outlasted the grace, which the box's own bounds limit; the
  service manager's final kill reaches it there, and never the agent, which
  runs in its own scope (section 8). A `load`'s admin has left the unit for
  the agent's scope at its start step (`weaver-admin-Spec` section 6.1);
  any other verb's admin runs in the unit until it ends. **Measured
  nowhere yet**: the moment the final kill lands, when admin-con exits
  before its stop timeout with a process left in the unit, is the service
  manager's and is to be measured on a box before the operator rules on the
  restart policy.
- **`TimeoutStopSec=`, the agent's `stop_grace_secs` plus 60 seconds**, so
  the service manager's timeout never cuts admin-con's own grace short.
  The grace is a member of each agent's config, so the staging tool writes
  the timeout as a drop-in per instance,
  `weaver-web-admin-con@<agent>.service.d/stop-timeout.conf`, from the
  value it stages into that agent's config (1080 unless the parameter
  file sets it, so 1140), and the template carries 1140 for an instance
  started without one.
- **`Environment=PATH=`, admin-con's path parameter**, since the invoker
  finds sudo by `PATH` and a path to it is a box path this repository does
  not carry (section 7.2), so the unit's environment is part of what the
  invocation trusts.
- **No `Delegate=`.** `toddwbucy/WeaverWeb#46` asked for it on
  2026-10-14, when admin made the agent's control group beneath the
  invoker's, which then had to be delegated. Since `toddwbucy/WeaverAgent#166`
  admin's start step leaves the invoker's control group for a scope of
  root's that the service manager makes, and the invoker's containment is no
  longer judged (`weaver-admin-Spec` section 6.1), so admin-con's unit needs
  no delegation and asks for none.
- **Nothing the contract forbids.** `weaver-admin-operator-contract`
  section 2 requires the invoker's execution context to leave admin and the
  agent what they need (the operator's ruling of 2026-11-09 on
  `toddwbucy/WeaverAgent#166`), and calls providing it the invoker's
  install's. So admin-con's unit sets none of these, and a test reads the
  rendered unit for each:
  - **no new privileges, under which sudo cannot gain root**:
    `NoNewPrivileges`, and every directive the service manager documents as
    implying it for a unit that runs as an account other than root:
    `SystemCallFilter`, `SystemCallArchitectures`, `RestrictAddressFamilies`,
    `RestrictNamespaces`, `PrivateDevices`, `ProtectKernelTunables`,
    `ProtectKernelModules`, `ProtectKernelLogs`, `ProtectClock`,
    `MemoryDenyWriteExecute`, `RestrictRealtime`, `RestrictSUIDSGID`,
    `DynamicUser` and `LockPersonality`;
  - **no capability bounding set narrower than root's**:
    `CapabilityBoundingSet`, `SecureBits` and `PrivateUsers`;
  - **no system call filter** (`SystemCallFilter` and its siblings above,
    and `SystemCallLog`), and **no address family filter** at all, which
    admits `AF_UNIX` by setting none;
  - **no private or read-only view** of `/sys/fs/cgroup`, `/run`, the
    agent's territory, the admin config root or the stack's prefix:
    `ProtectSystem`, `ProtectHome`, `ProtectControlGroups`, `ProtectProc`,
    `ProcSubset`, `ReadOnlyPaths`, `InaccessiblePaths`, `TemporaryFileSystem`,
    `BindReadOnlyPaths`, `RootDirectory` and `RootImage`;
  - **no resource limit**: no `Limit` directive of any kind, since a limit
    set on admin-con passes through sudo and admin to the agent, whose
    model loading it must not cut, so the box's defaults apply.

  gate-con's unit and the server's take none of these either. They need
  none, and hardening them is security posture, which the hard boundaries
  keep out of this repository.

**The parameters**, one TOML file the operator writes and keeps outside the
repository, read by the staging tool alone:

- **`[locations]`, always required**: `binaries`, the directory the
  programs are placed in; `units`, the directory the unit files and their
  drop-ins are placed in; `config_root`, the directory holding the server's
  `config.toml` and `connectors/<agent>/` beneath it.
- **`[server]`, where the server's unit is staged**: `account`, and
  `binary`, the built `weaver-web` to place.
- **`[connectors]`, where any agent is staged**: `gate_account`,
  `admin_account`, `admin_path` (admin-con's `PATH`), `gate_con_binary`,
  `admin_con_binary`, and `handover`, the directory `register` wrote into
  (its `--out`).
- **`[[agent]]`, one per agent on the box**: `box` and `name` as registered,
  the four box facts the connectors' configs require (`gate_socket`,
  `trace_socket`, `weaver_admin`, `admin_log`), and optionally
  `stop_grace_secs` and `verb_bound_secs`, which default as admin-con's
  config does.

**What each must be**, refused otherwise: every path absolute, not the root
directory, with no empty, `.` or `..` component and no trailing separator,
and of printable ASCII outside the characters a unit or a `PATH` list would
read (whitespace, quotes, the backslash, `%`, `$`, `;` and `:`), so each is
written into a unit as it stands; `admin_path` one or
more such directories joined by `:`; every account a portable user name (a
lowercase letter or `_`, then lowercase letters, digits, `_` or `-`, at most
32); `name` letters, digits, `_` and `-`, and `box` those and `.`, not `.`
or `..`, the grammar `register` holds them to, so a name is also a systemd
instance name that needs no escaping; no `name` twice; no member
the tool does not know, so a misspelled member is a refusal and not a
default; and no file's destination may be a directory's or lie above
another destination, since a program is not copied into the staged tree and
the manifest would list one path as both, which no order applies.

**The staging tool**, `weaver-web-stage --params <file> --out <dir>`,
built without default features so it runs on an agent's box as the
connectors do. It runs as the operator's own user and refuses to run as
root. It reads and checks every parameter, and renders every unit and
composes every connector config in memory, before it writes anything; it
then writes, loads each staged connector config through that connector's
own loader, and where any step refuses removes everything it wrote, so a
refusal leaves `<out>` as it found it. It makes `<out>` where absent, makes
the tree under it, `files/`, by an exclusive make, and on a refusal removes
only what that run made, so a second run on the same `<out>`, whose check
passed before the first wrote, is refused at the tree's make and removes
nothing of the first's. It writes, under `<out>`:

- **`files/`**, a tree mirroring each destination: the three units, each
  admin-con instance's stop-timeout drop-in, and each agent's two
  connector configs. **A connector config is its handover file, unchanged,
  then the box facts from the parameter file**, and the tool loads the
  staged file through that connector's own config loader (`client::read_private`'s
  trust rule on the staged copy, the plane, the members, the bounds and the
  absolute paths), refusing where
  it does not load, where its `agent` is not the parameter's `name`, or
  where the handover file is missing or is not a regular file. A refusal
  names the member, never the file's text, which carries a key. Each
  handover file and each program is opened without following a symlink and
  judged and read through the descriptor that open gave, as the
  connectors' trust rule reads a config, so the check and the read are one
  look at the name. **A handover file is held to that trust rule whole**:
  the invoking user's, at `0600` or tighter, since it names the server the
  connector will trust and carries its key, and the staged copy's `0600`
  must not stand for a source anyone else could have written or read. The
  cost is the operator's: a handover downloaded through a browser from the
  admin's agents page arrives under the browser's umask, often `0644`, so
  the operator tightens it (`chmod 600`) before staging, and the refusal
  names the mode found.
- **`manifest.tsv`**, one line per directory and file in the order they are
  applied, parents first: the kind (`dir` or `file`), the source (the staged
  file, or for a program the built binary the parameter names), the
  destination, the owner, the group, the mode, and the SHA-256 of the source
  (`-` for a directory). The binaries, units and directories are `root`'s,
  the directories and binaries at `0755` and the units at `0644`; each
  connector config is its plane's account's at `0600`, as the connector's
  trust rule requires, under an agent directory of `root`'s at `0755`. A
  group of `-` is the owner's primary group.
- **Its answer**, one JSON object on stdout with the exit status agreeing,
  the register verbs' shape (section 8): `verb` `stage`, `ok`, and on
  success the manifest's path and its line count, or on refusal the reason.

**`<out>` holds keys**, the connectors' configs, so it must not exist or be
an empty directory of the operator's own at `0700` or tighter, and the tool
makes it `0700` (a relative `<out>` is taken against the working
directory, and its `.` components and repeated or trailing separators are
dropped, lexically, with no symlink resolved; a `..` is refused as in any
path); the operator removes it after applying, as the handover
copy is removed once both planes connect.

**Applying is the operator's, as root, in the manifest's order**: each
directory made where it is absent, with its owner, group and mode, and left
as it stands where it is present, since a location may be a directory the
box already keeps; each file placed with its
owner, group and mode, a program only where the binary still has the
manifest's sum; then the service manager reloaded and each unit enabled and
started, the connectors as `weaver-web-gate-con@<agent>` and
`weaver-web-admin-con@<agent>`. Placing a file where one stands is a
replacement, so a running connector is stopped before its program or config
is replaced. This repository carries no script that does it.

**The check after the first load.** After the first `load` through
admin-con, the operator reads the constituents the register holds for the
agent (section 2.12, the `agents` verb), and for each process id reads its
control group as the kernel reports it in `/proc/<pid>/cgroup`, which needs
no privilege where `/proc` is mounted without hiding other users'
processes. Each must lie in the agent's own scope, as `weaver-admin-Spec`
section 6.1 names it, and none in `weaver-web-admin-con@<agent>.service`. A
constituent anywhere else fails the install, named: the operator unloads the
agent and examines the box's provisioning before another load. The check is
the install's, once, and drives no behaviour of admin-con's (section 8).

**Not in this act**, each waiting on what is named: the locations as
overridable defaults, and a script that applies the staged directory
(the operator's rulings, section 10); the restart policy (the same);
squashing the migrations into one schema file, which epic #18's install
item carried under the operator's ruling of 2026-10-08 that no deployment
held data before it, and which waits on his ruling now that one does; and
section 8's sentence on a service user per agent and plane, which
WeaverAgent's layout of two connector accounts per box would change, waiting
on `toddwbucy/WeaverAgent#533`'s merge, since no repository builds against
an unmerged contract.

## 9. What is enforced, and by which instrument

| claim | instrument |
|---|---|
| a position is addressed by run, turn and position | compile-pin on the key type |
| ingest is idempotent on that key | perturbation: replay one window twice |
| nothing is computed at read time except where the query is recorded | review, over the four reads the crate served, the fifth joining them with Record and the sixth with the plan |
| a recorded query names every run it addressed | perturbation: drop one, the row refuses |
| an incomplete shard set joins to nothing | perturbation: drop one file the index names, the join returns none |
| presence never gates a load | review, over the load path: this crate's catalog is not read there |
| a registered experiment is immutable | compile-pin: no mutating path off the frozen type |
| an absent forced mark is unknown and never unforced | perturbation: default an absent mark to unforced, an unmarked run ingests as a sampled one |
| a member the record did not carry renders as absent | perturbation: render an absent member as an empty cell, a device no deposit sent is indistinguishable from one the record sent empty |
| an absent surprisal renders as absent | perturbation: zero-fill, the view is wrong |
| the position is stored at ingest | perturbation: after ingest, alter the summary's counts and reread, the stored position is unchanged |
| an uncertified diagnostic record is not drawn | perturbation: drop the outcome check, an unknown run renders |
| the served stylesheet is the design system's and every served page wears the frame | the byte-for-byte pin `the_stylesheet_is_the_design_systems` in `src/surfaces/mod.rs`, perturbation: append a rule to `bundle.css` without regenerating `instrument.css`, and the pin fails "has drifted"; and `assert_framed` (`src/surfaces/mod.rs`) in each page's test, perturbation: drop `workspace` from the base template's `main`, and 94 tests fail, every page's among them, each on the frame's assertion |
| the agent page carries the eight stages in order with Run current, offers or disables every lifecycle verb and never hides one, and says each connector's status in a word | tests in `src/reading_tests.rs`, `src/alerts_tests.rs` and `src/surfaces/reading.rs`: drop a stage from the rail's table, or swap two, and `the_agent_page_carries_the_eight_stages_with_run_current` fails on the order; change a stage's backing in `stages.md` without the table, and `the_rails_stages_are_stages_md` fails; hide the verbs not offered, and `a_verb_not_offered_is_disabled_with_why_and_never_absent` finds them absent; render a connector's hue without its word, and `the_agents_connectors_show_last_heard_and_the_cut` fails |
| every script a served page runs is a file of this binary's, none inline, and the Live trace pane reads its stream through the trace module, holding at most what the window holds | `the_live_trace_pane_reads_its_stream_through_the_module` in `src/reading_tests.rs` and `the_module_is_small_and_the_page_has_no_inline_script` in `src/passkeys_tests.rs`: drop the pane list's `data-stream`, and the list naming the agent's stream is not found; put an inline script in the agent page or the enrollment page, and the page's script check finds it. Set the module's `RING_CAP` apart from `traceview`'s, or the ring's apart from the 10,000 section 2.13 states, and the module's test finds them differ. **Measured in the browser on 2026-11-07**, headless Chromium 154, against a stand-in and not the server: a local server answering with the agent page the server rendered (dumped from that test), the repository's assets and the policy's header, and a stream in the server's format: an inline script and an inline `onerror` handler put in the Live trace pane were both refused by `script-src 'self'` and reported, neither ran, and the trace module ran beside them and rendered every event |
| no surface writes a position or a run | compile-fail: a doctest constructing a recorded-table writer from an authoring path does not compile |
| an authored edit against a stale version refuses | perturbation: drop the version check, the second edit silently wins |
| an authored row names its author or names none | perturbation: default the null to the operator, a row nobody can attribute reads as attributed and is uncorrectable |
| every authoring write passes the member, and the column has no default | perturbation: give the column a null default and drop the member from one write path, the write lands and the row is indistinguishable from one whose author nobody could name |
| a registered experiment's question is immutable with the rest of it | compile-pin: no mutating path off the frozen type reaches it |
| import computes the identity rather than accepting one | perturbation: take the operator's digest, two boxes disagree about one artifact |
| a record identity names at most one catalog row for a file or a directory artifact, a renamed split excepted per sections 2.3 and 10 | perturbation, at the schema: drop the unique index that holds for every shape but a split, a second import of the same file or directory opens a second row and a lookup answers two where it owes one |
| the bearer is stored as a digest and never in the clear | perturbation, at the schema: store the bearer and look up on it, a read of the session table is a set of live sessions |
| the sentinel joins to nothing | perturbation: register the empty string as an identity, a run whose hash failed joins to an artifact it never named |
| the record's session and digest are absent where unsent | perturbation: fill an absent digest from the landed rows, a row from a record cut short vouches for bytes nobody drained |
| the record's session and digest agree across a run | perturbation: land a run whose generations name two sessions, the row holds two truths about which record it came from |
| a chip filters only on a column section 2.7 indexes | review, over Record's filters: a chip on an unindexed column is a sequential scan the surface offers as though it were cheap |
| the run list is paged and records nothing | perturbation: page on the ingest's clock alone, a tie larger than the page drops its remainder, and record a query row per page, section 2.6 fills with a list nobody reruns |
| the seated prefix's length is landed and never derived | perturbation: derive it here from the two counts, every row reads the first draw's position as the prefix and every whole-run arm branches one input too late |
| an authored identity says what it addresses, and a received one is left as the record spelled it | compile-pin on the key types, and perturbation at the schema: drop a kind's domain and a plan takes an arm's identity, which a sequence could not have refused at all |
| an arm registers at most once | perturbation: register a plan twice, the second pass writes a second staged experiment against one arm and the matrix reads two arms where the operator authored one |
| a batch runs what it queues in the order it was queued in | perturbation, **owed**: drain by identity or by the state's clock, arms queued in one gesture share a timestamp and the operator's order is lost. **No instrument stands**: section 2.11 has no table and no writer yet, and the act that builds queueing builds both |
| an arm frees at most one member | perturbation, at the schema: drop the partial index, one arm frees two and registers a sweep whose row carries one member and one value set |
| an entry states the value its disposition names | perturbation, at the schema: drop the check, an entry says held and carries nothing, which is the absent-not-empty failure moved from the view into the store |
| the task's verdict is landed and never scored here | perturbation: score a run in this crate, the verdict carries no scorer and the row claims a reading it did not receive |
| a connection whose credential is not live is refused before its roster is read | perturbation: accept the hello and check the register after, a revoked connector's roster lands before the refusal and a surface renders an agent nobody admitted |
| one live connection per credential | perturbation: let a second connection replace the first, a credential in two hands displaces the live connector silently and the row reads connected throughout; drop the serialization of admission and revocation, revoke while a hello is between its lookup and its installation, and the revoked credential relays until the next hello; let a stale teardown write disconnected after its replacement was admitted, and the row reads the plane missing while it relays |
| a hello's identity is its certificate's binding and never its roster | perturbation: act on the roster's name and plane, a hello on a gate credential naming another agent, or naming admin, is believed, and the seed's first-hello-wins returns through the roster |
| at most one row per box and name holds live credentials | perturbation, at the schema: drop the partial index, register one agent twice, and two rows each hold live credentials for one agent, so the server attributes one agent's observations and verbs to two rows |
| the link state is reset when the listener starts | perturbation: skip the reset, restart the server with no connector up, and a surface reads an agent present whose sockets are gone |
| the trace is replayed from the acknowledged position | perturbation, each clause against the real listener and a fake relay (`src/link/fake_relay.rs`, a Unix socket server built to the relay's wire) in `src/link/admin_con_tests.rs`: drop the replay, break the link during a run and reconnect, and the server's window has a hole with no mark; drop the digest from the position, truncate the file in place during the outage and regrow it past the offset, and the window carries the new prefix nowhere and marks nothing, where the relay refuses the position and admin-con marks it; ignore a `truncated` line, and the window carries the shrunk file as a continuation with no mark; drop the identity from the position, replace the file between runs and grow the replacement past the offset, and the window carries its prefix nowhere and marks nothing, where the next run's header names a new identity; derive the identity from process state, restart admin-con against the unchanged file, and the server receives a false discontinuity and the file again from its start; restart the server and resume from a remembered position, and the window has a hole with no mark; start the backfill anywhere but the first record boundary within `backfill_bytes` of the end, and the window carries half a record or more than the bound; record a boundary inside an unterminated record and resume from it, and the window carries half a record and a parse failure; end the replay on a step that sent nothing, and a load behind a record past the bound is taken as live and writes the row; send a record's frame unmeasured, and a 1 MiB record of NUL bytes passes the line bound; block the read on an ack, and a backfill of many small records stalls the link and readmits; exempt marks from the offset rule, and a mark past the boundary lands before `caught_up`; replay an opening from what it read where that does not hold the span from the server's position, and an outage longer than the backfill bound leaves a hole with no mark; drop the marks an opening's read met, a truncation it read through, a position the relay refused, or a file other than the one admin-con last read, and under a server holding no position the window carries the new content as continuous; mark a discontinuity again from the server's position after the opening's read marked it, and the window carries it twice; let an opening's dial outlast its bound, and a relay holding its header holds queued verbs past it; serve a person's verb queued at an opening ahead of the opening's `show`, and a verb longer than the opening's deadline closes the connection `admission_incomplete` and loses that person's answer; count the opening's `show` against the queue's bound, and asks filling the queue during an opening have it answered `busy` and the connection closed `admission_incomplete`; replay toward a boundary taken at its bound from a server position past it, and a mark past the boundary is refused as malformed and the opening never completes; take that position as caught up unverified, rewrite the file below it, and the replacement is relayed live and its old load writes the row; give the verification a bound of its own, and an opening takes two bounds while asks wait behind it; leave the verification no reserve, or retry a failed one from the same start, and against a relay slower than the bound the door never opens; charge the hello's handshake to its opening, and a server slow to answer leaves the verification nothing and closes the door just admitted; verify the hello's opening without reading the connection, and a relay slow to answer the verification holds the admission's `show` past its deadline; open the door again while an earlier opening's hold stands, and that opening's `show` clears the later hold, so an ordinary verb runs ahead of the later `show`; give the hold a clock of admin-con's, or none at all, and a server that asks the opening's `show` late finds an ordinary verb ahead of it; release the hold on the local send of the `show`'s answer rather than the server's `landed`, and a `show` answered with a state that the server never reports landed lets a queued `load` run, and against the real listener a landing that stalls lets it run before the connection closes; release it on a `landed` for any id, and another ask's landing lets the `load` run; send `landed` before the store takes the observation, and the same stalled landing lets it run; send `landed` on the best-effort path rather than delivered or the connection closed, and an admin-con whose write path is full of acks as the landing completes stays connected with the frame dropped and its hold stuck; set no hold at the hello, at the hello as at every opening, and a closed door's hello whose admission `show` faults lets a queued `load` run before the listener closes the connection; compare the header's identity only past offset zero, and a file rotated while the position stood at zero after a truncation is relayed as the same file, the rotation unmarked; take a partial header at a redial for a refused position, and the file is marked and relayed live from zero, its history landing on the row. **Re-shown against the relay client on 2026-10-05**, by the act that built it; the act that built admin-con showed each clause against the tailer it then had, and the tailer's own clauses, a skipped record's digest, the tail's search, the replaced generation, the symlinked path, a rewrite regrown between two polls, and the two guards held by review for the live switch and the open, left with the tailer. **The door's three clauses**: open the door mid-connection without a boundary, and an event from the closed interval writes the row; admit a closed door with a boundary it cannot take, and the hello never completes; take an opening while a verb is in flight, and a long load closes the connection `admission_incomplete`; carry on past a door frame the row no longer takes, its incarnation another connection's, and the retired connection's next event writes the row. **The measures' bounds**, against a fake writer that never idles: remove the boundary's bound, and admin-con behind such a writer, under a header naming no length, is never admitted; remove the drain's bound, and a verb behind it never runs. **The header's length**, against a fake relay naming it at its size read: ignore it, and admin-con behind a writer that never idles is admitted only at the boundary's bound, a record completed between the size read and the header is replayed and never writes the row, an opening whose position is at the length waits on a relay that sends nothing until its bound, and after a server restart a record past the length is replayed in the backfill; read a header naming no length as one at the dialed position, and a record read before the first heartbeat is live and writes the row; reset the door's backoff on an opening whose stream it keeps unread, and a relay that sends a header at the length and drops is redialed without pause. **The window's verdict**, against the real listener in `src/reading_tests.rs`: take the window's `replayed` from admin-con's flag rather than the replay's end, and an event sent unflagged before `caught_up` reaches the live view as live; exempt marks from the verdict, and the backfill's mark behind the boundary reaches it as live. **The residue `toddwbucy/WeaverAgent#88` narrows** is the order inside the bounds' windows that remain, the drain's and an opening's under a header naming no length or behind a relay that stalls, a transient the convergence of section 7.2 covers, with no instrument |
| an agent is present only when both planes connect from one row | perturbation: mark present on either plane alone, an agent whose admin-con is down reads present with a tuple and a load state nobody has confirmed |
| the server's authority is loaded before the listener starts and never minted at start | perturbation: mint the authority at start instead of loading it, restart the server, and every connector's hello is refused against a certificate it does not pin |
| the client credential is stored as a fingerprint and never the key | perturbation, at the schema: store the key, a read of the register is a set of credentials anyone can present |
| a register verb through the server is an admin's under the exclusion, a retired row is never live again, and its client configs are handed over once and kept nowhere | perturbation in `src/agents_tests.rs`, against a real listener and an authority minted at test time: drop the admin's re-check from the register's transaction, revoke the grant between the surface's read and the write, and the agent is registered; drop the shared identity exclusion from it, and a revocation commits beneath a register held after its re-check; drop the take's session check, and another session of the same admin takes a config; drop the take's removal, and a config is taken twice; drop the lifetime, and a config past five minutes is taken; drop the reservation, and a registration with no room for its configs is recorded and written; count the configs held and not those reserved, and five concurrent registers against room for four all land, past the bound; drop a rotation's re-check of its row inside the transaction, and a row the host retired after step three is live again; drop the check of the authority on disk, and a register mints under an authority rotated since the server loaded it; drop a rotation's notification, and the old credential's live connection stays open; drop a rotation's resolution of its agent, and an admin's unknown agent leaves a begun and failed pair; log a config at its take, and a private key is in a log line; drop the retired check from the host's rotation, and a row retired through the web is made live again by the host's `rotate` of its identity; drop the re-read of a revocation's row, and a revocation the host's overtook after step three answers and is audited as landed, though it revoked nothing; drop a revocation's notification, and the revoked plane's live connection stays open; revoke each plane by its own statement, and a retirement advances its row's version twice for one authored edit; drop the shared identity exclusion from a revocation, and a grant removal commits beneath a revocation held after its re-check; drop the grants page's mark, and a grant on a retired agent reads as any other |
| an act's `ok` outcome commits with it, and a lost commit answer is classified by that record | perturbation in `src/admin_tests.rs`, `src/grants_tests.rs`, `src/agents_tests.rs`, `src/keys_tests.rs` and `src/host_tests.rs`, the commit's answer lost by a test fault after PostgreSQL applied it: write the `ok` outcome after the commit's answer instead of in the transaction, and each of the five person writes, the three grant and role writes, a revocation and a retirement, a registration and a rotation through the server, a token's redemption, a session's opening, a passkey's addition and removal (`src/keys_tests.rs`), and the host's bootstrap and token answers as failed (the web's writes with the server's failure, the host's commands with a refusal), its outcome `failed`, for an act that landed; and where a later act commits between the lost answer and the read-back, a rotation over a revocation or another admin's enable over a disable, the act is still classified as landed, which a read-back of its effect would not do; drop the read-back's wait on the act's own lock, give up on a disable's answer while its COMMIT is still completing, and the read-back finds no outcome and answers failed for an act that then lands |
| the tuple and the load state come by admin-con and never by gate-con | perturbation: let the data plane fill the tuple, the row carries a declared tuple from a party the gate's contract forbids to know it and nothing says which party wrote it; let a replayed load event write the tuple, restart the server after an unload, backfill, and the row reads loaded; ask `show` before fixing the replay boundary, unload between the two, and the row reads loaded until the next `show`. **Four clauses are admin-con's ordering**, against the real listener with a fake invoker in `src/link/admin_con_tests.rs`: let a `show` answer cross the link out of order with the file events around it, unload during the `show`, and the row reads loaded until the next `show`; place the answer in the stream at receipt and unload between the snapshot and the receipt, and the row reads loaded; skip the drain, leave an unread load event behind the tail, invoke `show` after an unload, and the row reads loaded; run two verbs at once on one connection, and an older answer lands after a newer one. **The slot's clause**, against the real listener with the sudo invoker over a fake `sudo` in `src/link/sudo_invoker_tests.rs`: free the slot at a reconnection while a timed-out verb's process still runs, and the new connection's admission `show` runs beside it. **A turn's start and close refresh the load state**, against the real listener in `src/link/tests.rs`: drop the `turn.started` mapping, and the row never reads `active` between a turn's start and close; let a replayed `turn.started` land, and the row reads `active` from history; let a turn's event write the tuple, and the row loses the tuple it held; let a turn's event write the tuple's source, and a `show`-shaped tuple reads as an event's |
| nothing crosses the link in the clear | perturbation: offer a plaintext hello to the listener, it is refused below any roster; and review, over the listener, that no plaintext accept path exists |
| the server never asks a verb outside the agent's ceiling | perturbation: drop the ceiling check, ask a verb admin-con's hello did not declare, and it leaves the server; and admin-con's half, drop its typed error answer, and it reaches the invoker. **One clause is owed**: check against the row's copy, narrow the ceiling by reconnecting between the check and the enqueue, and an ask outside the new ceiling leaves. The race has no deterministic staging, and the guard is held by review: the check reads the live connection's ceiling under the live map's lock that finds the connection |
| a verb its principal may not ask is refused before an ask | perturbation, in `src/verbs_tests.rs` against the real listener and a fake admin-con: drop the grant check, and an observer's `stop` leaves the server; drop the session's re-check under the hold, disable the person between the gate and the check, and the ask leaves; release the hold before the enqueue, and a revocation commits while the frame of the ask it revoked is still to be sent; reserve the slot under the hold, and a grant's removal waits on a stalled connector; run the ask in the request's own future, and a dropped request loses an authorized ask after its first, and a refusal its record; admit every ask, and a client posting past its share detaches tasks without bound; drop the per-connection cap on persons' slots, and asks parked on the server's own waits take the channel and an opening's `show` waits behind them; read the grants without the deadline, and an ask stalled on the store never answers and keeps its place; answer no grant with 403, and the page tells a stranger the agent exists. The server's `show`-only rule stands in the row of its asks, and the register verbs through the server in their own row |
| a turn its principal may not ask is refused before an ask | perturbation, in `src/verbs_tests.rs`, the verb's held ask on the gate plane: drop the grant check, and an observer places a turn; release the hold before the enqueue, and a revocation commits while the turn is still to be sent. The session's re-check, the admission, the slots and the deadline are the verb row's own instruments, the one mechanism serving both planes, and `src/verbs_tests.rs` shows the hold, the admission, the slots and the deadline on the gate plane once each |
| a role, a grant, or another person's authentication material written by a principal not permitted to write it is refused | perturbation in `src/grants_tests.rs` and `src/keys_tests.rs`: drop the write's admin check at the surface, and a person holding no admin grant has a write audited as begun rather than refused; drop the live admin grant from the transaction's re-check, revoke the grant between the surface's read and the write, and the grant lands; drop the self-grant refusal at the surface, and it is refused only after a record, and drop the store's too, and an admin grants themselves a role; the same for revoking one's own grant, and for editing a role one holds, the admin holding the observer role then adding `stop` to it; take the role's edit outside the store's check, grant the role to its editor between the surface's read and the edit, and the editor widens a role they hold; take a revocation's transaction without the exclusion, have two admins revoke each other's admin grant at once, and no admin remains; drop the last-admin count from a revocation, and the store's refusal of the last enabled admin's grant rests on the own-grant rule alone, and drop that too, and the grant is revoked; drop the version from a role's edit or a revocation, and a second edit from the same page overwrites the first; write a grant as the host principal, and the record names no admin behind it; drop a removal's ownership check, and a person holding two passkeys removes another person's; drop the parse of a submitted person, agent, grant or role name, or the vocabulary's check, and a malformed ask is recorded or is the server's failure rather than refused before any record; drop the resolution of step three (a grant's role, scope, person and agent, a revocation's grant, a role edit's role, or a person write's person), and an admin's unknown reference leaves a begun and failed pair; move it before the first gate, and a non-admin's unknown reference answers not found where a known one answers the refusal, telling them what exists; drop the page's admin check, and a person holding no admin grant reads every grant |
| the host's identity commands write their audit record before any mutation, and an outcome naming it | perturbation in `src/host_tests.rs`: drop any one command's first record (bootstrap, token, reset, grant add, grant remove, role set), and with the first record refused that command writes its rows unaudited |
| the last enabled admin's grant is never revoked, the host's removal included, and two removals cannot both pass | perturbation in `src/host_tests.rs`: drop the count of the admins that would remain, and the last admin's grant is revoked; take the identity transaction without the exclusion, hold one removal between its count and its revocation and ask the other meanwhile, and both land, leaving no admin |
| a role's verbs stay within the vocabulary, and the admin role is never written | perturbation in `src/host_tests.rs` and `src/store/identity_tests.rs`: drop the command's vocabulary check, and a verb outside it reaches the store, refused there only as a fault; drop the command's admin check, and the same; drop the store's vocabulary check, and a role carrying `fly` lands; drop the trigger fixing the admin role, and its verbs are rewritten |
| a grant names an agent exactly where its role is per agent | perturbation in `src/host_tests.rs` and `src/store/identity_tests.rs`: let a server-wide role name an agent in the command, and an admin grant on an agent is taken as server-wide; drop the store's check, and a per-agent grant naming no agent lands |
| every identity check is false, never unknown, on a missing value, and the audit's person is a person | perturbation in `src/store/identity_tests.rs`, one row per check with its guarded member missing, each rolled back: drop the guard on a token's ending, and a token ended with no reason lands; drop it on a person target's identity, and a person target naming no row lands; drop the audit's foreign key to the person, and a record naming no person lands. A role's missing verb needs no guard: `<@` is false, not unknown, on a NULL element, so the vocabulary check refuses it as it stands |
| every target the audit writes is checked by its kind | perturbation in `src/store/identity_tests.rs`, each insert rolled back: drop the passkey target's check, and a passkey target out of its `pk-` shape, or naming none, lands; drop the alert target's, and an alert target out of its `al-` shape, or naming none, lands; drop the declaration target's, and a declaration target out of its `de-` shape, or naming none, lands; drop the authority target's, and an authority target naming a row lands; drop the known kinds, and a target of a kind no check covers lands. The agent's, person's, grant's and role's checks stand in the rows of the audit's checks and the identity checks |
| every verb a person asks has its first record before its frame, and every refusal of one its record | perturbation, in `src/verbs_tests.rs`: send the frame before the record step, and it reaches admin-con though the audit holds no record of it; drop a refusal's record, and a refused verb leaves no trace. The outcome naming the first is written at the door of the server's asks' row, which a person's answer passes through alike |
| every turn a person places has its first record before its frame, naming `turn`, and its outcome only by the gate contract's evidence | perturbation, in `src/verbs_tests.rs`: write `ok` for any close, and a stopped turn reads `ok`; write `failed` for `delivery_lost`, and an unknown outcome reads `failed`; drop the door's shape check, and an answer carrying both a close and a fault is taken. The first before the frame is the record step's, shown on the verb's row and the server's asks' |
| the verb's answer page shows admin's own words, escaped and cut at the alert's bound, never parsed | perturbation, in `src/verbs_tests.rs`, `the_answer_page_shows_admins_own_words`, against the real listener and a fake admin-con: a refused `load` whose stderr carries markup and runs past 2048 bytes, a two-byte character straddling the bound, shows the words escaped, ending before that character and saying it cut, beside the refusal object, and a blank stderr is not shown; drop the stderr from the page, the cut or the escaping, and the case fails |
| the agent page states admin's `[restore]` rule with its source, read at the build's pin | `the_agent_page_states_admins_restore_rule_with_its_source` in `src/reading_tests.rs`: the page carries the rule, its refusal and its two remedies beside the verbs, naming the commit it was read at; drop the statement, and the case fails; and `the_restore_rule_was_read_at_the_builds_pin`: move `CORPUS_COMMIT` alone, and the case fails |
| a verb admin's state table refuses in the row's last reading is warned of beside its form and offered still, read at the build's pin | perturbation, in `src/transition_tests.rs`, `the_agent_page_warns_of_a_verb_the_state_table_refuses`, against the real listener and a fake admin-con: read idle, `load` is warned `AgentRunning` and `save-point` and `unload` are not; read active, `load` and `save-point` are warned; read in transition, both are warned with the lock held and free told apart, dated by the transition; read unloaded, `save-point` alone; each warning names the reading and its date and every form stays; drop the warning from the template, change a cell, or ignore a standing transition, and the case fails; `the_state_table_cells_follow_the_contract_as_read` in `src/surfaces/admin_table.rs`: every cell stands on the words of the contract's section 4.1 as held in `admin-table-5ec7ecf8.md`, and every warning is the derivation's; change a cell or a copy's word, and the case fails; and `the_state_table_was_read_at_the_builds_pin`: move `CORPUS_COMMIT` alone, and the case fails |
| a turn's text is in no audit record and no log line | perturbation, in `src/verbs_tests.rs`: put the text in the first record, and the row carries it; log it at debug in the held section, and the process's capture of every line finds it |
| every person, role or grant written has an audit record naming its principal, and its outcome is a second record naming the first | perturbation, **owed**: update the first record with the outcome instead of appending a second, and an ask whose answer is lost reads as never answered with no trace of the rewrite; write a grant with the audit write dropped, and the grant lands with no record; add a passkey with the audit write dropped, and the write lands with no record; record the material in an authentication write's record, and a read of the audit table is a set of credentials. The server's own asks and a person's verbs and turns stand in their own rows. Lands with the IAM act |
| the server's own asks are audited: every verb frame it sends has a first record committed before the frame enters the channel and an outcome naming it when the answer lands, and it asks `show` only | perturbation, in `src/link/server_asks_tests.rs` against the real listener and fake admin-cons: enqueue the frame before the record step, or where the record failed, at an admission and through `Listener::verb`, and a `show` reaches admin-con with no record; drop the record step's bound, and a store that stalls holds the ask past the silence bound; take the oldest entry of the connection's map of first records rather than the one the answer's id names, and an opening's answer lands as the admission's outcome; race an answered ask's outcome against the close signal, and a revocation landing mid-write leaves its first alone; disarm the guard between the record and the channel as soon as it is installed, and an ask cancelled while its send waits on a full channel keeps its entries and leaves its first alone; arm the guard only once the insert has answered, and a first whose answer is lost after its commit stands alone; drop the outcome lock from the `failed` write, and a first whose commit lands after its answer was lost stands alone; go back to a deny-list of the fault kinds that write no outcome, and a kind a connector invents writes `failed`; drop the door's verb check, and a `load` answer to a `show` ask writes `ok`; land a `show` answer's state whatever its exit status, and a refusal's object lands as the row's state; compute what remains of the admission's deadline before the outcome's write, and an admission whose write passes its deadline completes after it; read the deadline after the landing's poll rather than before it, as tokio's `timeout_at` does at every poll, and a `show` answered past the admission's deadline lands on the row while the connection closes `admission_incomplete` (6 of 20 runs alone, measured 2026-11-11), a landing handed a deadline already past is polled and read as done, and one still waiting when its deadline comes is polled at the timer's wake, where it could take a pool connection and send its statement (`link::listener::bounded_tests`, every run); read the clock at the first poll alone, and the timer's wake polls a waiting landing past its deadline; keep the timer and drop the clock, and a deadline at the instant the bound is asked, which the timer rounds up to its next tick, lets the landing be polled; drop the timer and keep the clock, and a landing that never answers holds the bound past its deadline; poll the landing before the close signal, and a close standing at a poll lets it be polled; classify a verb frame carrying both an outcome and a fault as admin's outcome, and the asker reads a success and the audit `ok` for a contradictory answer; drop the `show`-only refusal, and the server's `load` leaves with no refusal record |
| a store write the server bounds by a deadline is never sent once its deadline has come, so a caller that reports it as not made at its bound is telling the truth | perturbation, in `src/deadline.rs` with no database: poll the write before reading its deadline, as tokio's `timeout_at` does, and a write whose wait ends at its deadline sends at the timer's wake (`a_write_waiting_at_its_deadline_is_never_sent`); keep the timer and drop the clock, and a write handed a deadline at this very instant is polled; tokio's own order is held as a test (`tokios_timeout_sends_a_write_waiting_at_its_deadline`), so an upgrade that reorders it is seen. And at each site, in `src/link/deadline_tests.rs` against the real listener and fake admin-cons, the write held just before the statement that commits it until the very deadline its site bounds it by (`deadline::hold`, a test's lever alone): bound the site's write with tokio's `timeout_at` again, and its row lands while the caller reports it not made, every run: a person's refusal while the ask answers the bound's failure, the server's refusal of a verb but `show` while the verb answers the audit's failure, and a verb's outcome, a first record and the `failed` outcome of a first whose frame never left while each is logged as not written. And in `src/alerting_deadline_tests.rs`, against the alerts' own worker and drainer on a store of their own, the same perturbation lands an alert while the operations log's line names none, a restoration on the lost-contact alert while the worker logs it as not noted, an inbox row's commit, the row gone and its alert raised, while the drainer logs it as not applied, and a row's back-off while the drainer logs it as not backed off. And the class, in `src/timeouts_tests.rs` with no database, the Planner's ruling of 2026-11-12: every call of tokio's `timeout` or `timeout_at` in code compiled outside test is held to a list, each entry naming its function and one reason it bounds no write that could commit unreported; add such a call, or put any of the act's nine sites back in tokio's order, and the scan finds it unlisted |
| a person authenticates by passkey, or by a sign-in token where the config admits one, and by nothing else | perturbation, through a soft passkey in `src/sign_in_tests.rs`: let the options load every person's passkeys, the counter rule find the credential among anyone's and the opening take any person's passkey, and another person's authenticator answering this person's challenge opens this person's session, each of the three guards holding it alone; take a ceremony without removing it, and the ceremony table's own test in `src/passkeys_tests.rs` finishes one twice, a captured sign-in assertion replayed being refused besides by the counter rule; audit the failed signature, and a stream of bad assertions fills the audit; drop the opening's check of its person, disable the person between the counter rule and the opening, and a disabled person's session opens; look the passkey up by its credential ID at the finish rather than by the identity the ceremony challenged, reset the person and re-enroll the same credential between the options and the finish, and a session opens on the re-enrolled row, and a possible clone whose passkey was removed goes unaudited. No path opens a session but the finish of a verified assertion and, where the config's `sign_in` names `token`, the redemption of a live sign-in token, whose own row is the sign-in token's below |
| the server refuses to start on the listener's faults the design lists, and promises nothing beyond them | perturbation, each start refusal of `docs/project/design-2026-10-07-iam.md` section 3 that concerns the origin and the certificate, against certificates minted at test time in `src/listen_tests.rs`: the origin parsed by `url`, the WHATWG URL Standard a browser parses by, and required to equal its origin's ASCII serialization: drop the equality, and an origin with userinfo, a path or a trailing slash, a query, a fragment, a scheme or host in upper case, a default port written, a port with a leading zero, an IPv6 address not in its canonical form, a host a browser reads as an IPv4 address written otherwise than in dotted decimal, or an internationalized name not in its ASCII form starts, each a value no browser sends, so every state-changing request would fail the `Origin` comparison; compare the Unicode serialization instead, and an internationalized name starts in its Unicode form; keep the brackets of an IPv6 host in the name check, and a certificate for that address is refused; let `http` through off `localhost`, and the server listens on an origin where no ceremony can run; drop the refusal of an `https` origin with no certificate and key, and the listener serves it in the clear; drop the pair check, and a key that does not pair with its certificate starts; drop the name check, and a certificate for another host starts against the origin; drop either half of the validity check, and an expired or a not-yet-valid certificate starts; drop the warning, and one expiring within fourteen days starts silently. And the listener itself: let the binary serve plain where a certificate and key are configured, and `tests/startup.rs` (ignored, the real binary, started with a certificate minted at test time) gets no answer over TLS and a surface's answer in the clear; take each handshake on the accepting task, and a client that connects and never speaks stalls the next client's handshake in `src/listen_tests.rs`; drop the permit taken before each accept, and with the cap held by silent clients the next connection is accepted at once rather than waiting for a silent one's bound, at a small cap and bound the test sets; the same file shows a TLS client a surface's answer and a plain request none. And the listener's log, against a log captured in `src/listen_tests.rs`: log a client's certificate alert at debug, and a browser refusing the certificate leaves the operator nothing at the default level; warn of every one, and a second alert within the interval warns again; take every failed handshake for a certificate alert, and a plain client warns. With a certificate and no origin, the pair and the period are checked and the name check waits for an origin. A fault the list does not name, a certificate unfit for server authentication among them, is not this row's: the browser's refused handshake shows it |
| the server refuses to start on the relying party's faults the design lists | perturbation, each refusal its own case in `src/listen_tests.rs`: drop the refusal of an `rp_id` with no `origin`, or of an `origin` with no `rp_id`, and the server listens with passkeys no browser will use; drop the refusal of an empty `rp_id`, or of one that is an IP address, bracketed or not, and every ceremony fails with the browser's `SecurityError`; drop the refusal of an origin whose host is neither the `rp_id` nor under it, or take a bare suffix for a domain under it, and an origin of another registrable domain, or one the `rp_id` sits under, starts; drop the requirement that the `rp_id`, parsed by `url` as a browser parses a host, equal its domain's ASCII serialization, and an `rp_id` in upper case or with a Unicode label starts, the config and what the ceremonies send the browser no longer one fact. A public suffix as the `rp_id` is not this row's: the browser's failure at the first ceremony shows it |
| a person's name is unique in its canonical form at enrollment | perturbation in `src/host_tests.rs` and `src/store/identity_tests.rs`: compare names as given, and a second person bootstraps as `ADA` beside `Ada`, as full-width letters, or with an accent composed where the first decomposed it, so name-first sign-in would find two |
| a person is named by identity only in an identity's whole shape, and no name takes it | perturbation in `src/host_tests.rs`: read any `pe-` argument as an identity, and a person named `pe-alice` is not found by name; take a name of an identity's whole shape, and it lands, shadowing the person whose identity it spells |
| the host's write of a role refuses a stale version | perturbation in `src/host_tests.rs`: drop the version from the role's update, hold one write after its read while another commits, and the held write overwrites the other's verbs. A grant's removal carries its version too; the re-check of its liveness under the exclusion already refuses a grant another removal revoked, so the version adds no instrument of its own there |
| a person's name stays unique in its canonical form at rename | perturbation in `src/admin_tests.rs`: drop the check at rename, and a rename to a name another person's form already holds, in case or in width, fails at the store's key as a fault rather than refusing; drop that key too, and it lands; leave the person's own row in the check, and a person renamed to their own name in another case is refused |
| an admin's write to a person is refused without a live admin grant, re-checked in its transaction, never on their own row, never leaves no enabled admin, and a disable revokes the outstanding token | perturbation in `src/admin_tests.rs`: drop the page's admin check, and a person holding no live admin grant reads every person; drop the write's admin check at the surface, and such a person's write is audited as begun rather than refused; drop the live admin grant from the transaction's re-check, revoke the grant between the surface's read and the write, and the write lands; take the disable's transaction without the exclusion, have two admins disable each other at once, and no enabled admin remains; drop the session's standing from the re-check, and the second of those disables is refused only as the last admin; drop the count of the admins that would remain, and the last enabled admin is disabled; drop the disable's or the rename's own-row refusal, and an admin disables or renames themselves; drop the token's revocation from the disable, and a disabled person's token stays live; drop the passkey check at the admin's token, and a token is issued for a person holding a credential; drop the name's check at the admin's enroll, and a taken name fails at the store's key as a fault rather than refusing; drop the version from the rename, and a rename from a stale page lands; drop the identity shape's refusal from the rename, and a name spells an identity; drop the first record, and a write lands unaudited; drop `no-store` from the token's answer, and it may be cached; drop the parse of a submitted identity at the boundary, at any of the four writes naming a person, and a malformed one is the server's failure (500) rather than the ask's (400); drop it at a passkey's removal, and the audit's passkey target check refuses a malformed one as the server's failure (500); drop that check too, and a malformed passkey lands an audit row as the removal's target |
| a credential ID belongs to one passkey of one person | perturbation in `src/passkeys_tests.rs` and `src/keys_tests.rs`, through a soft passkey: drop the addition's check of a credential ID already held, under the identity exclusion, and a registration whose ID another person holds fails at the store's key rather than refusing, its grant spent either way; drop that key too (the passkey table's primary key on the credential ID) with the checks, and the ID lands a second passkey row, so one credential signs in as either person |
| a signature counter that did not rise refuses the sign-in, and the stored passkey is updated by a locked read, merge and write | perturbation in `src/sign_in_tests.rs`, the counter rule driven through its `Assertion` seam where a soft passkey cannot reach: drop the library's `CredentialPossibleCompromise` arm, and a clone's sign-in is refused unaudited; swallow the error where the possible clone's refusal cannot be recorded, and it answers the ordinary refusal over a record the audit lost; drop the rule's own comparison, and a counter raised between the options and the finish signs in; drop the write of the merged passkey, and the stored counter never rises; drop the row's lock, and of two concurrent assertions with one nonzero counter both succeed, and a concurrent assertion erases another's backup-eligibility upgrade; apply the refusal to a zero counter, and two zero-counter assertions are refused; drop the rule's commit, and the counter does not persist where the opening then fails |
| a signature counter that did not rise refuses the fresh assertion before adding a passkey, its update committed before the addition's | perturbation in `src/keys_tests.rs`: drop the library's `CredentialPossibleCompromise` arm at the fresh assertion, and a clone's assertion is refused unaudited; the rule's own comparison is the one sign-in's row shows, by the same function. The counter's transaction commits before the grant is minted and the grant is held in memory alone, so no addition's failure can roll it back |
| a session carries a person and the method that opened it, ends where its person does or, where a passkey opened it, its passkey does, and its cookie is neither readable by script nor carried by another site | perturbation, against sessions opened in the store in `src/surfaces/gate_tests.rs`: drop `HttpOnly`, `Secure`, `SameSite=Strict` or `Path=/`, or set the cookie under another name, and the cookie's test finds it changed; read the old `weaver_session` name too, and a claimed-name cookie names a session; drop the check of a session's close, of its person's disable, of its passkey's presence, of the absolute limit or of the idle limit, and a session so ended is served; read a session no passkey opened as one whose passkey is removed, and a session a sign-in token opened ends at its first use, and re-check a write's session by its passkey alone, and every write under such a session is refused as its authority gone (`src/sign_in_token_tests.rs`); name the passkey by its credential ID rather than its own identity, reset the person and enroll the same credential again, and the session opened before the reset is served; leave the row open where a use finds it ended, and the next read finds it open; drop the `Origin` check, admit a request with no origin configured, or take the first of two `Origin` headers, and a POST from another origin changes state; refresh the last use unconditionally, and two uses within a minute write twice and a time ahead of the clock moves back; close unconditionally where a read found a session ended, hold a use between its read and its close, refresh the session from another request meanwhile, and the refreshed session is closed on the stale read (`src/surfaces/gate_tests.rs`); drop the absolute limit's seven-day bound, and an absolute limit of the largest integer a config holds loads (`src/config.rs`), an interval the database cannot make at every use; drop the idle limit's five-minute floor, and an idle limit of 30 seconds is accepted at load (`src/config.rs`), where a session in active use would end; and a session used every 50 seconds is held open at the floor itself (`src/surfaces/gate_tests.rs`); let sign-out leave the row open, and the cookie it cleared still names a session; write the person's name as the author, or resolve every member as a claim, and an authored row stops naming its person or their current name; let a host's `--author` claim of the whole `pe-` shape through, and each register verb and identity command acts on it (`src/link/verbs_audit_tests.rs`, `src/host_tests.rs`), its claim then resolving as that person. `tests/startup.rs` (ignored, the real binary) reads Record under a person's session, refuses `POST /sign-out` at the `Origin` check with the session's cookie and without, and refuses a `POST` to a path no route serves at the same check |
| a passkey is added only after a fresh assertion, and a person's last is never removed | perturbation in `src/keys_tests.rs`: start a registration with no grant, and a session alone adds a passkey, a stolen cookie gaining access that outlasts it; drop the grant's binding to its session, and another session of the same person spends a grant it did not earn; drop the refusal of the last, and a person removes their last passkey; drop the addition's re-check of its session and of the passkey that earned its grant, hold the addition after its session read, run the host reset, and the device the reset cut off lands while the reset's token is stranded; drop the re-check of the passkey that earned the grant alone, remove that passkey while the session stands, and the addition lands on a removed passkey's grant; drop the removal's re-check of its session, and a removal asked under a session that no longer stands lands; take the removal's count outside the identity exclusion, hold one removal after its count, and two concurrent removals of the last two both land; list passkeys without the session's person, and the page shows another person's. A grant is taken once by the ceremony table, whose own test shows it |
| an enrollment token is issued only for a person holding no passkey, lives at most seven days, is kept as its digest, and is its person's one live token | perturbation in `src/host_tests.rs`: drop the passkey check, and a token is issued for a person holding a credential, which an admin could redeem over theirs; drop the command's lifetime bound, and a token past seven days is refused only by the store, as a fault; drop the store's lifetime check, and a row past seven days lands; store the token in the clear, and a read of the token table is a set of usable tokens; leave an earlier token live when issuing, and the newest is refused against the one live token per person. And in `src/config.rs`: configure a lifetime past seven days, and it is taken |
| a sign-in token is issued by the host alone and only where the config names `token`, lives at most an hour, is kept as its digest, is its person's one live token, is revoked by a disable or the host reset, and opens one session, by its person's name, naming its method and no passkey; and a person issued one takes an enrollment token from the host alone | perturbation in `src/sign_in_token_tests.rs`: drop the command's refusal where the config names no `token`, and a server configured as before issues one; mount the token's route whatever the config, and such a server serves it; drop the store's lifetime check, and a row past an hour lands; store the token in the clear, and a read of the token table is a set of usable tokens; leave an earlier token live when issuing, and the newest is refused against the one live token per person; drop the redemption's match on a token still live, and one token opens two sessions, one after the other or two at once; drop the check of the name, and a token signs in under another person's name; drop the expiry, and an expired token signs in; drop the check of its person, at the lookup and at the opening, and a disabled person's token opens a session; drop the disable's revocation, and a token issued before a disable signs in after an enable; drop the host reset's revocation, and a token printed before a reset signs in after it; drop the admin's refusal, and an admin issues an enrollment token for a person who signs in by token; drop the revocation at a sign-in token's issue, and an admin's enrollment token issued before it still starts an enrollment after; drop the session's check of its method against its passkey, and a session of a passkey's method naming no passkey lands. And in `src/config.rs`: take an empty `sign_in` or one naming a method twice, and it loads |
| `GET /` sends a browser to sign-in without a session and to Record with one | test in `src/sign_in_token_tests.rs`: a request with no cookie, one whose cookie names no session and one naming a live session each get their redirect, and the first two never Record |
| an enrollment token is single-use and redeemed only within its lifetime | perturbation, against a soft passkey (`webauthn-authenticator-rs`, a dev-dependency) in `src/passkeys_tests.rs`: drop the token's expiry, its end, its person's enable or the person's holding no passkey from what makes a token redeemable, and that token starts a ceremony; drop the transaction's re-check, and a passkey appearing between the options and the finish leaves the person holding two; leave the token unended at the redemption, and it redeems again; drop the check of a credential ID already held, and the finish fails at the store rather than refusing. A redemption sets no cookie and writes no session row, which the end-to-end test asserts |
| an enrollment token is 32 bytes of the operating system's cryptographic randomness | perturbation in `src/store/identity_tests.rs`: draw bearers from a counter, and consecutive bearers share a prefix, the next guessed from the last |
| a ceremony identity is drawn as every bearer the server issues is | perturbation in `src/passkeys_tests.rs`: draw ceremony identities from a counter, and consecutive identities share a prefix, the next guessed from the last |
| the session bearer is drawn the same way | perturbation in `src/sign_in_tests.rs`: draw session bearers from a counter, and consecutive bearers share a prefix, the next guessed from the last |
| no statement this crate issues can rewrite, remove or truncate an audit record | perturbation in `src/store/audit_tests.rs`, each statement in a transaction the test rolls back: drop the row trigger, and an update rewrites a record and a delete removes one; drop the truncate trigger, and a truncate empties the audit. The triggers guard this crate's code paths, not a process dropping them with the owner's rights, which section 2.13 puts outside the threat model |
| every register verb writes its audit record before it acts, and an outcome naming it | perturbation in `src/link/verbs_audit_tests.rs`: drop any of the five verbs' first record, and with the first record refused that verb acts anyway, unaudited; write `authority init`'s first record after the act, and with it refused the authority is created without one; drop the outcome, and a verb's first record stands unanswered; let the inspection before the first record make the agent's directory, and with the first record refused a directory stands that nothing audited; discard a stale half pair during that inspection, and with the first record refused the pair is gone unaudited; name the agent a rotation was asked by instead of the row whose retained pair it publishes, and the record names a retired row. And in `src/store/audit_tests.rs`: insert into the audit from a second file, and the test reading the tree for one writer finds it; drop the index of one outcome per first record, and a first record is answered twice |
| a record's principal and its method agree | perturbation in `src/store/audit_tests.rs`: drop the check, and a host record carrying a person's method lands |
| every check of the audit is false, never unknown, on a missing value | perturbation in `src/store/audit_tests.rs`, one row per check with its guarded member missing, each in a transaction rolled back: drop the guard on the outcome, and an outcome record with no outcome lands and takes its first record's one outcome; drop it on an agent target's identity, and an agent target naming no row lands; drop it on a person's identity, and a person nobody can name lands |
| the admin role grants no action on any agent | perturbation, in `src/verbs_tests.rs`: let the server-wide admin grant carry verbs on every agent, and an admin holding no grant on an agent asks `show` of it |
| reading an agent takes a grant on it | perturbation in `src/reading_tests.rs`, against a real listener: drop the list's grant filter, and a person sees an agent they hold no grant on; drop the page's grant check, and a person holding no grant on an agent reads its row and its live window; answer the agent without a grant differently from an unknown one, and the page is an oracle for agent identities; give the admin's register view the door and the load state, and an admin with no grant reads an agent's state |
| an open live view ends within its bound of what ended its session or grant | perturbation in `src/reading_tests.rs`, the re-check's interval set short: drop the stream's re-check, and a view stays open after its grant is revoked by the host, its person disabled, or its session signed out; drop the grant from the re-check, and the revocation alone leaves it open; drop the view's watch on its viewer, and a view dropped on a quiet agent keeps its task and its subscription to the window until its session or grant ends; send plainly instead of racing each send against the re-check, fill the channel with a reader that never reads, and revoke the grant, and the view stays open past its interval; drop the body's gate, and the events queued before the cut drain to a reader resuming after it |
| a trace kind or content block this repository does not know passes through the relay, the window and the live view, and raises nothing | perturbation in `src/reading_tests.rs`, karl's admin-con run on its real config against a fake relay and the real listener, relaying a `model.unrecovered_call` record and a `message.assistant` record whose content holds an `unrecovered_call` block (`toddwbucy/WeaverAgent#136`): refuse at the listener a kind outside the ones it reads, and the window and the live view never hold the record; turn a record holding a content block of a type admin-con does not know into a mark where it parses the record, and the window and the view carry the mark in place of the record |
| the ceremonies in flight are bounded, each expiring and used once | perturbation in `src/passkeys_tests.rs`, on a paused clock: drop the cap, and a sixty-fifth ceremony starts; take a ceremony without removing it, and it finishes twice; drop the expiry, and a ceremony past five minutes is taken; keep expired ceremonies against the cap, and the table refuses while none is live |
| the WebAuthn library links into the server binary alone | measurement and perturbation in `tests/connectors_link.rs`: `ldd` on the connectors as `cargo test` builds them finds no `libcrypto`, and on the server binary finds it, which shows the reading; and the package's normal graph without the default features holds no `openssl-sys` and no `webauthn-rs`, the build CLAUDE.md gives for agent boxes. Make the library a dependency the feature does not carry, and the graph holds it |
| the admission's `show` is required only where the ceiling grants it | perturbation: ask `show` at every admission, and an agent whose ceiling grants no `show` is closed `admission_incomplete` at every reconnection and never stays admitted; skip it where it is granted, and the row reads the last process's state as current |
| the ceiling declared in the hello is exactly what the box's sudo rules grant | perturbation, against a fake `sudo` generated at test time in `src/link/sudo_invoker_tests.rs`: answer `grants` from anything but each verb's own `sudo -n -l` line, and a verb the rule refuses is declared, or one it grants is not |
| a connector never acts on the agent: admin-con's stop invokes nothing | perturbation in `src/link/admin_con_tests.rs`: restore the orderly stop's `unload`, and a shutdown with `unload` in the ceiling invokes it. And against a fake `sudo` in `src/link/sudo_invoker_tests.rs`, where the rules grant `unload`, the stop runs no line, and with a child in flight it waits for the child and runs none |
| raising an alert never waits on the store or the log | perturbation in `src/alerts_tests.rs`: wait on the full queue in the raise instead of dropping, and with the worker held a refused verb's answer is held behind it and the raises time out |
| one open alert per agent, kind and subject, its count right under concurrency | perturbation in `src/alerts_tests.rs`: drop the open alert's index and the insert's conflict clause, and sixteen raisers at once leave sixteen alerts; drop the conflict clause alone, and a repeated refusal is never counted on the open alert |
| an acknowledgement names the version it read, and only a person holding `operator` on the agent acknowledges | perturbation in `src/alerts_tests.rs`: drop the version from the acknowledgement's write, and an acknowledgement naming a version a repeat has moved lands; drop the `operator` check, before the record and in the transaction, and an observer's acknowledgement lands |
| an authorization refusal raises no alert, and no alert is raised on behalf of a person holding no grant on the agent | perturbation in `src/alerts_tests.rs`: raise a `refused` alert for no grant or a grant without the verb, past the authorization's gate as well (either guard alone holds), and a refused person's ask raises one; raise a system refusal whether or not the authorization stood, and a person holding no grant whose ask stalls before the check raises a `refused` alert and an operations-log line on the agent |
| the server's own `show` in flight at a teardown raises no `unknown` | perturbation in `src/alerts_tests.rs`: keep it among the in-flight unknowns, and a cut with the server's `show` in flight raises an `unknown` beside the lost contact |
| choosing a prompt's option without a grant answers as an unknown agent | perturbation in `src/alerts_tests.rs`: resolve the alert before the grant check, and a person holding no grant tells a real prompt from an unknown one |
| a plane the server cannot reach is a banner for the agent's grant holders alone, re-read by the page | perturbation in `src/alerts_tests.rs`: drop the grant scoping of the unavailable planes, and a person holding no grant sees another agent's banners; drop the grant check from the connectors' fragment, and such a person reads its connectors and their last-heard time |
| a fragment's re-fetch leaves the session's last use as it was | perturbation in `src/alerts_tests.rs`, `a_poll_leaves_the_sessions_last_use_as_it_was`, over the banners, the connectors and Run's panes: refresh the last use at a background read, or read Run's panes through the page's session read, and a poll moves it, so an open tab holds an idle session open |
| the agent page's load state and verbs are one fragment, re-fetched every 15 s through the grant scoping | perturbation, in `src/transition_tests.rs`, `the_run_panes_re_read_by_themselves`, against the real listener and a fake admin-con: the page wraps both panes in the fragment's trigger; the fragment is the page's own panes, kept from every cache; a state written between two fetches shows at the second, its warnings with it; a person holding no grant reads 404, a malformed identity 400, an ended session 401; drop the trigger from the page, drop the grant check, or render the fragment from another view, and the case fails |
| an answer refused at the door leaves its act's outcome unknown, and raises its alert | perturbation in `src/alerts_tests.rs`: leave the refused answer's ask out of the teardown's drain, on the admin plane or on the gate plane, and a `load` answered naming another verb, or a turn answered with both a close and a fault, raises only the lost contact |
| a plane's loss is ordered before the restoration that follows it | perturbation in `src/alerts_tests.rs`, the teardown held after its row is released: raise the loss after the teardown rather than where the entry comes out, readmit in the gap, and the lost-contact alert ends unrestored while a connection stands |
| a line of the box's `admin.log` raises an alert only where it did not come through WeaverWeb | perturbation in `src/entries_tests.rs` and `src/link/admin_con_tests.rs`: drop the own check at the server, and a refusal admin-con's own invocation met raises its alert again from its line; take every line as another party's at admin-con, and an own line sent end to end through the real admin-con raises one; put the whole line back in the alert's detail, and another party's refusal shows the line's uid and command line to every grant holder |
| a connector's entry is taken once, though it is sent again, and acknowledged only once durable | perturbation in `src/entries_tests.rs`: drop the run's mark, and an entry resent on a new connection is logged twice; read on past an entry the store did not take, and the next entry moves the mark past it, so its resend is taken for a duplicate and never logged; acknowledge without writing the entry's row, and 1,000 entries acknowledged while the drainer is held are never logged; delete a row whose line failed, and that entry's line is lost; drop the inbox's backoff, and rows that keep failing hold the entry behind them forever; keep the entry as `jsonb`, or leave a NUL in an alert's text, and an entry carrying a NUL is never applied; raise a row's alert in a transaction apart from its record on the row, fail the record, and the retry raises it again; drop the claim, and two drainers on one store write rows' lines twice; leave the drainer out of its listener's tasks, and a stopped listener's drainer applies rows after its stop; end the alerts' worker before the connections at a stop, and a teardown's unknown and its line are lost; skip a blank line of `admin.log`, and it is no entry; drop the connector's resend, and a window whose acknowledgements the server dropped stalls the pump for good; restart the resend's timer at every wake, and a line every quarter of the bound postpones it for as long as the stream lasts; choose due rows by their identity alone, and an entry behind seventy slowly failing rows starves; send an oversized `admin_log` entry in the failure shape, and the door refuses it at every resend |
| admin-con reads `admin.log` from its start's baseline, a truncation or a regrowth seen | perturbation in `src/entries_tests.rs`: take every first open as the start's baseline, and lines written to a file absent at the start are skipped; take a baseline inside a line with no skip, and that line's rest is sent as a line of its own; drop the digest of the last bytes read, and a copy-truncate regrown between two looks is resumed mid-line |
| a connector's backlog of entries is bounded, its drops counted and reported | perturbation in `src/entries_tests.rs`: drop the bound, and a backlog past it holds every entry |
| the door checks an entry once, its source on the plane's allow-list | perturbation in `src/entries_tests.rs`: give the gate plane the admin plane's sources, and gate-con's entry claiming `admin_log` is taken and classified rather than refused as malformed |
| a load that reset the agent is a `data_lost` alert, raised once | perturbation in `src/entries_tests.rs`: drop the reset check, and the trace's `load` event carrying `reset` raises nothing |
| a `show` answering `in_transition` claims no state, and the next state ends it | perturbation in `src/transition_tests.rs`: write the load state from an `in_transition` answer, and the row reads a state admin never claimed; let a state leave the transition standing, and the page shows a transition over a newer state; let a turn's close end it, and the page reads `idle` with no transition while the unload still runs; drop the constituents from the transition, and the leftovers' pids vanish from the row and the page |
| a recovery holds exactly one of the person's places for its whole life | perturbation in `src/alerts_tests.rs`: admit again inside the sequence, and with the person's other three asks outstanding the choice's save-point is shed |
| the prompt's `unload` follows only a hash-confirmed save point | perturbation in `src/alerts_tests.rs`: ask `unload` on any `save-point` answer, and a `save_point_not_taken`, or a `save_point_taken` with no report, is followed by an unload |
| no privileged invocation exists outside admin-con's sudo invoker | perturbation, two instruments. A test, `tests/no_privilege.rs`: it reads every tracked file outside `docs/`, never following a symlink, comment lines aside by each file's syntax and Markdown read in its fences only, for a privilege-escalating program named as a word, a setuid family call, a child's user or group set on a command, and a setuid or setgid mode bit, with the program's word allowed in `src/link/sudo_invoker.rs` and its test module alone; shown to fail when one of each family is planted, in Rust and in a README fence, when the program is planted in another file of the invoker's name, and when another family is planted in the invoker's own files. And the argv instrument in `src/link/sudo_invoker_tests.rs`: plant the principal's name from the ask into the command, and the line the fake records is no longer exactly `sudo -n <weaver_admin> <verb> <agent>` |
| the staged units carry section 8.1's properties, and admin-con's sets nothing the contract forbids | perturbation in `src/stage_tests.rs`, rendering with invented parameters into a temporary directory: drop `UMask`, `KillMode=mixed`, the stop timeout or `PATH` from a template, and the read of the rendered unit fails; plant one directive of each of section 8.1's five forbidden groups in admin-con's template, and the read fails; drop the renderer's refusal of a placeholder it does not fill, and its test renders such a template instead of refusing it. And the service manager's own verifier: each rendered unit, instanced, passes `systemd-analyze verify` with nothing on its error stream, since the verifier warns and still exits 0 on an unknown key or a value it cannot parse (measured 2026-11-11, systemd 262); the test prints a `skipped:` line where the verifier is absent |
| a staged connector config is its handover file and the box facts, and loads under the connector's own rule | perturbation in `src/stage_tests.rs`: drop a box fact from the composition, and the connector's loader refuses the staged file; stage an admin plane's handover file as gate-con's, or one naming another agent, and the tool refuses at the staged file's load and leaves `<out>` as it found it; drop that load, or the agent's check, or the removal after a refusal, and the case stages or leaves `<out>` behind |
| the staging tool refuses every parameter out of shape, and a refusal writes nothing | perturbation in `src/stage_tests.rs`, one case per refusal: a missing member, an unknown member, a relative path, a `..` component, a trailing separator, a character a unit would read, a relative element of admin-con's `PATH`, an account or a name out of its grammar, a name twice, an agent without `[connectors]` or `[connectors]` without an agent, a file that stages nothing, a program that is a directory or a symlink, a handover file that is missing, a symlink, a directory, open to others or not the operator's own, a location that is a program's destination or lies under one, an `<out>` that is not empty, open to others or not the operator's own; drop each check and its case stages instead, or is refused for another reason, which the case's expected reason catches; after every refusal `<out>` is absent, or as it stood, and a refusal after the writes began empties an `<out>` that stood empty. And a second run on the same `<out>`, its check passed before the first wrote, with `<out>` absent and with it standing empty: refused, the first run's files and manifest unchanged; make `files/` without exclusion, or let a run that made nothing remove `<out>`, and the test fails. And a relative `<out>` with a `.` component and repeated and trailing separators is taken against the working directory and stages, a `..` kept and refused: drop the normalization, and the case is refused for its `.` |
| the manifest names every directory and file with its owner, group, mode and sum, parents first | perturbation in `src/stage_tests.rs`, the manifest read line by line against the parameters: give a connector config another mode or owner, list a file before its directory, or leave a binary's sum out, and the read fails |
| the staging tool never runs as root | perturbation in `src/stage_tests.rs`: the refusal is a function of the effective uid it is handed, and dropping it lets a uid of 0 stage |
| the declaration screen refuses every field out of shape and saves nothing | perturbation in `src/declarations_tests.rs`: `every_refusal_names_its_field_and_its_reason`, one case per refusal of section 6.1 judged on the document by its field and its kind (a missing required field, a requirement that names nothing, a class giving both a family and a list, a requirement naming more than one kind, a roster the framework would not load, a format other than GGUF, an adapter out of shape, a name outside its grammar, a control character, a tunable value outside its kind (a count above the largest whole number TOML holds among them), a tool other than `sh`, an unknown member): drop each check and its case is admitted, or refused for another reason, which the case's kind and field catch; and `the_screen_refuses_a_field_out_of_shape_and_saves_nothing`, every refusal the form can carry posted end to end, each answered 400 naming its field by the screen's slot, after which the table and the audit hold what they held; a class spanning two compute capabilities is no refusal's case, and saves (the next row); and `a_long_class_is_judged_in_one_pass`: a class of 20,000 distinct models, parsed before the session is read, is judged inside 5 s, and its first model typed again at its end is refused naming that slot; restore the scan of every earlier name, and the judgement takes minutes (101 s for 16,000 in the test build, measured 2026-11-12) |
| a declaration the screen saves carries every tunable value a serving load requires at the pin, and a save lacking one is refused naming it and writes nothing | perturbation in `src/declarations_tests.rs`, `every_tunable_value_is_required_and_a_missing_one_is_refused_naming_it`: one case per name of the eight, the document otherwise whole, each refused naming that name's field, and posted end to end each answered 400 by the screen's slot with the table and the audit holding what they held; drop the requirement of any one name, and its case saves |
| every tunable value is judged on the agent's range at the pin, and a real again as narrowed to a 32-bit float | perturbation in `src/declarations_tests.rs`, `every_tunable_value_is_judged_on_the_agents_range`: on each side of every edge of section 6.1's table one case, the edge itself admitted and its neighbour refused naming the field (a negative or infinite temperature, a temperature narrowing to infinity and a nonzero one narrowing to zero, a top-p of 0 and above 1, a repetition penalty of 0, a top-k and a repetition window at 2^31, a context capacity of 0 and at 2^32, a seed and a maximum tokens per turn above 9223372036854775807), the two stated edges admitted; drop or loosen any bound, and its case saves; tighten one, and its edge is refused |
| a new declaration's form starts with the five values the SPU froze until the carrier, and the other three empty | perturbation in `src/declarations_tests.rs`, `a_new_declarations_form_starts_with_five_tunable_values`: the new form's temperature, top-k, top-p, repetition penalty and repetition window carry 0.7, 40, 0.95, 1.1 and 64, editable, and the seed, the maximum tokens per turn and the context capacity carry nothing; change a value or fill one of the three, and the case fails |
| the screen's required tunable set is the agent's at the build's pin, as read | perturbation in `src/declaration/tunable_set.rs`: `the_tunable_set_is_the_agents_as_read`, the eight names read from the passages held in `tunable-set-5ec7ecf8.md` (the six of `SAMPLING_DEFAULT_NAMES`, and the `OperatorTunable` members of `SESSION_PARAMETERS` under the names `tunable_names` gives them; the conditional field-election rule held and not required); add a name to the copy or drop one from the screen, and the case fails; and `the_tunable_set_was_read_at_the_builds_pin`: move `CORPUS_COMMIT` alone, and the case fails |
| a row saved before the eight were required reopens, is named stale by its pin, and its save is refused until the eight are filled | perturbation in `src/declarations_tests.rs`, `a_row_lacking_a_tunable_value_reopens_stale_and_saves_once_filled`: a row placed under the earlier pin with the old document's optional sampler reopens, names its pin stale and shows what it holds and empty fields for the rest, its file naming each missing name in a comment; its save as reopened is refused naming the first missing field and writes nothing, and filled it saves under this build's pin; make the reopen refuse, or the save admit the row as it stood, and the case fails |
| a class spanning more than one compute capability is warned of, and a model the capability table does not know is named unknown and admitted | perturbation in `src/declarations_tests.rs`, `a_class_spanning_capabilities_is_warned_of_and_an_unknown_is_named`: drop the warning, and a class of two capabilities saves with none; make it a refusal, and the case fails, since the class must save; a family spanning two capabilities by the table's rows, typed in another case, is warned of, and each saved class keeps the form typed, family or list; render an unknown capability, of a model or of a family, as a blank, or refuse it, and the case fails; and `the_capability_table_is_the_page_as_read_and_every_row_cites_its_source`: the table against the vendor's page as read, row for row, every capability and family citing its source, a family by statement citing a guide's passage and one by name holding the name |
| a save and a reopen are the same document, and the agent's file is derived from it | perturbation in `src/declarations_tests.rs`, `a_save_and_a_reopen_are_the_same_document`: a document holding every member saved, the row holding its canonical form, and the reopened page's form posted as a browser posts it read back as the same document, its re-save moving nothing; drop a member on the save or on the reopen, and the comparison fails; both panes are what the document derives; the agent's file parses as TOML, names the pinned corpus commit, writes the eight tunable values in `tunable-values` under the agent's names, and writes each placeless member in its trailing block; and `the_agents_file_writes_every_admitted_value_in_its_toml_type`: a whole real at or above 1e15 and the adapter's scale read back as floats, a count at the largest whole number TOML holds as that integer, and the header names this build's pin; write a whole real as its digits alone, and the case fails |
| a save from a row that is not its lineage's head is refused and writes nothing, and the schema admits a second child of one parent | perturbation in `src/declarations_tests.rs`, `a_save_from_a_row_not_the_head_is_refused_and_two_children_are_admitted`: drop the screen's head check, or the store's under the key, and the stale save lands, each check exercised alone; and two children of one parent inserted through the store's own insert, which succeeds |
| a declaration row's authored half never changes | perturbation in `src/declarations_tests.rs`, `a_declaration_rows_authored_half_never_changes`: an update of each authored member, a delete and a truncate are refused by the trigger, and the last `validate` reading moves; drop the trigger, and the update lands. The key is an identity column no update may set |
| no two lineages share a name, and a lineage keeps its name | perturbation in `src/declarations_tests.rs`, `no_two_lineages_share_a_name_and_a_lineage_keeps_its_name`: drop the screen's check, or the store's under the key, and a second lineage saves under a held name, or a save renames its lineage, each check exercised alone; the index on a root's name stands beneath both |
| only the server-wide admin grant reads or writes a declaration, and a refused save is one record | perturbation in `src/declarations_tests.rs`, `only_the_admin_grant_reads_or_writes_a_declaration`: drop the gate, and a person holding only a per-agent role reads the list and saves; a refused save writes exactly one audit record, carrying the refusal, and no row |
| the declaration screen reaches no agent and no host | `the_declaration_screen_reaches_no_agent_and_no_host` in `src/declarations_tests.rs`, with a real listener and a fake connector of each plane connected and admitted: every route is exercised, a save, a save from it and two refusals among them, and neither connector is sent a line, the server sending a connector no heartbeat of its own |

**A watch that cannot fail is not a test.** For each perturbation above, the
act that lands it states what removal makes it fail and confirms it does.

**A row marked owed has no instrument and is not counted as enforced.**
Two stand so marked as of 2026-10-16. The pull request of the in-transition
`show` of 2026-10-16 stood up an `in_transition` claiming no state and ended
by the next state, each clause shown to fail with its guard removed. Act 12's pull request of the
connectors' entries of 2026-10-15 stood up an alert only for a box line that
did not come through WeaverWeb, an entry taken once, the backlog's bound,
the door's per-plane sources and `data_lost` for a reset load, each shown to
fail with its guard removed. Act 12's pull request of alerts
and the operator's decisions of 2026-10-14 stood up alerts raised without
waiting, one open alert per agent, kind and subject, the acknowledgement at
its version and by `operator` alone, no alert for an authorization refusal
or on behalf of a person holding no grant, none for the server's own `show`
in flight at a teardown, the unreachable plane's banner for grant holders
alone, a fragment's re-fetch that is not the person's use, a recovery
holding one place for its whole life, an answer refused at the door raising
its act's unknown, a loss ordered before its restoration,
a prompt's choice answered as unknown without a grant, the prompt's
`unload` only on a hash-confirmed save point, and a connector that never
acts on the agent, each shown to fail with its guard removed, retiring the
orderly stop's retry until rest with the stop's `unload`. Act 11's pull request of a
person's turns of 2026-10-14 stood up the principal check for turns (the
verb's held ask on the gate plane), the audit of a person's turns (the
first naming `turn`, the outcome by the gate contract's evidence alone)
and a turn's text in no record or log line, each shown to fail with its
guard removed, narrowing the audit record row to the identity writes, and
closed act 11's PR 6. Act 11's pull request of a
person's verbs of 2026-10-13 stood up the principal check for verbs (the
grant, the session and the ceiling under the hold from the check to the
enqueue, the slot reserved before it, the ask run where a dropped request
cannot cut it, no grant answered as an unknown agent), the audit of a
person's verbs (the first before the frame, each refusal's record) and
the admin role's lack of agent actions, each shown to fail with its guard
removed, narrowing the principal check row and the audit record row to a
person's turns, owed to the pull request of turns. Act 11's pull request of the
server's own asks of 2026-10-13 stood up their audit (each verb frame the
server sends recorded before it enters the channel, its outcome naming
its own first) and the server's `show`-only rule, each shown to fail with
its guard removed, narrowing the audit record row and the principal check
row to a person's asks, owed to the pull request of a person's verbs. Act 11's pull request of reading
an agent of 2026-10-12 stood up read access (the list, the page and the
live view by grant, an agent without one answered as an unknown one) and
the live view's bound (its fifteen-second re-check of the session and the
grant), each shown to fail with its guard removed. Act 11's pull request of registering
agents through the web of 2026-10-09 stood up a register verb through the
server as an admin's under the exclusion and the hand-over of its client
configs, once and kept nowhere, each shown to fail with its guard removed,
and narrowed the principal check's register clause to `revoke`'s, which the
pull request of revoking and retiring agents through the web of 2026-10-09
then stood up in the same row (the row re-read in the transaction, the
connection closed in the act, the shared hold, and the mark on a grant
whose agent is retired). The
batch's order is owed because
section 2.11 describes its table and no migration builds it. One row of
the role shape ruled on 2026-10-02 stays owed to the IAM act, narrowed to
the identity writes: the audit record. Act 11's pull request of
an admin's writes to roles and grants of 2026-10-09 stood up the writer's
check for roles, grants and another person's authentication material (the
admin grant at each write and re-checked in its transaction, the
self-change rules before any record and under the exclusion, the last
admin, the versions, and a passkey's removal by its owner alone), each
shown to fail with its guard removed. Act 11's pull
request of an admin's writes to persons of 2026-10-09 stood up the unique
name at rename and an admin's writes to persons (the admin grant at the
page and re-checked in each write's transaction, the own row, the last
admin under the exclusion, and the disable's revocation of the token)
and the audit's check of every target by its kind,
narrowing the writer's check to roles, grants and another person's
authentication material, each shown to fail with its guard removed; the
token clauses that row also carried stand in the enrollment token's rows.
Act 11's passkeys pull
request of 2026-10-09 stood up the credential ID's uniqueness, the signature
counter at the fresh assertion before adding a passkey, and the fresh
assertion's grant with the last passkey's keeping, each shown to fail with
its guard removed. Act 11's sign-in pull request of
2026-10-09 stood up passkey-only sign-in, the signature counter at sign-in
(narrowing that row to the fresh assertion's, owed to the pull request that
adds passkeys), and the session bearer's randomness, each shown to fail with
its guard removed. Act 11's enrollment pull request of 2026-10-08 stood up the enrollment
token's redemption, the ceremony table's cap, expiry and single use, the
ceremony identity's randomness (narrowing the bearer row to the session
bearer, owed to sign-in), and the WebAuthn library's place in the server
binary alone, each shown to fail with its guard removed and the last
measured with `ldd`. Act 11's session pull request of 2026-10-08 retired the row that a session
carries a claimed name, and stood up the relying party's start refusals and
the session as a person's (its cookie, its ends at every use, its last-used
refresh, the `Origin` check, sign-out and the author member), each shown to
fail with its guard removed. Act 11's TLS pull request of 2026-10-08 stood up
the listener's start refusals, those of the origin and the certificate, and the
listener that serves TLS alone, each shown to fail with its guard removed, and
narrowed the start-refusal row to the relying party's two.
Act 11's persons pull request of 2026-10-07 stood
up the host's identity commands' records, the last admin and the exclusion, the
roles' vocabulary and the fixed admin role, a grant's agent, the identity checks
and the audit's person, and the issuing halves of three owed rows, which it
narrowed to what remains owed: a name's uniqueness at rename, a token's
redemption, and the session bearer's and the ceremony identity's draw. Act 11's audit pull request of 2026-10-07 stood
up the append-only audit, the register verbs' records and the agreement of
principal and method, each shown to fail with its guard removed. The
act that built the relay client on 2026-10-05 re-showed the replay row
against the relay, its three clauses for the door's opening among them,
and the bounds under its measures, each shown to fail with its guard
removed, and names inside the row the residue `toddwbucy/WeaverAgent#88`
narrows, with no instrument. The act that built the sudo invoker on 2026-10-05 stood up the two rows of the
management plane ruled on 2026-10-03, the ceiling as the sudo rules grant
it and no privileged invocation outside that invoker, which replaced the
row that held no privileged invocation anywhere, and the tuple row's
clause of the slot held across connections, each shown to fail with its
guard removed. The act
that built admin-con stood up the trace file's replay from the acknowledged
position, the ceiling on both halves, the conditional admission `show`, the
four clauses of the tuple row that are admin-con's ordering, and the absence
of a privileged invocation then, each shown to fail with its guard removed; one
clause of the ceiling row, a race with no deterministic staging, is marked
owed inside the row, and two guards of the replay row are held by review for
the same reason. The alignment act of 2026-10-03 added four clauses to the
tuple row, a turn's start and close on the load state, and a fourth, the
tuple's own source, each shown to fail with its guard removed. The other ten rows of the link landed with the act
that built the listener and the register, each shown to fail with its guard
removed. The marking is the point: a row
reading like the enforced ones beside it would tell a reader the claim is held,
which is the same failure as a watch that passes either way and is why this
table says which it is.

**One row left this table on 2026-09-08 rather than becoming an assertion.**
It read that an undeclared boundary refuses the load, with a perturbation at
the admit path. **This crate does not hold that admit path.** The claim is
`weaver-admin`'s, whose Spec refuses a declaration whose reachable organ
carries no boundary and whose section 4.3 argues it, open at issue #438.
Section 2.3 of this document already says the shape: a load is refused on
the box it runs on under that box's own rules, and this crate honors the
gate rather than re-deciding it. **An enforcement section that lists a
property its crate does not enforce puts one claim under two owners**, which
is the duplication G5 exists to refuse, so the row goes rather than becoming
a `web-` assertion beside `weaver-admin`'s.

## 10. Open elections

- **The store engine and its migrations.** Postgres is the charter's
  election, and the schema's expression is this document's and lands with
  the first code act.
- **The field election's depth**, which is the charter's open cell and not
  this crate's to set.
- **Whether surprisal and entropy draw as one timeline or two.**
- **Whether the preset ladder is a picker or a wizard.**
- **Who reports an artifact's presence on a box**, which the charter's
  section 9 holds open. Section 2.3 stores the entry with its reporter and
  its date whichever answer lands, and no read of it gates anything, so
  this document is not blocked on the choice.
- **Which identity the catalog and the record share, closed 2026-09-06.**
  Two rules stand at two grains and the row carries both: section 2.3 keys
  on the per-file map the lens refuses on and carries the record's weights
  hash beside it, computed at import by the rule `weaver-spu-Spec` section 3
  now states, and the run row fills its member from the summary the analysis
  seam carries as of the same date. Neither rule was dropped, because the
  lens's refusal rests on one and every record on the other, and the
  derivation between them runs one way. Per issue #465.
- **The split GGUF's names, which the record's identity does not cover.**
  `weaver-spu-Spec` section 3's hash of a split covers the shards' bytes in
  split order and not their names, so two split sets of identical bytes
  under different stems are one record identity and two rows of section
  2.3, and the lookup reports both rather than picking. Two answers stand:
  the SPU's split manifest carries the names, which changes the identity
  every later admit of a split records and is that Spec's to rule, or this
  catalog keys a GGUF by the record's identity alone, a GGUF's shard names
  being the operator's and not the artifact's where a safetensors index is
  the artifact's own. A split BF16 of a thirty-five billion parameter model
  stands staged on bulk-store since 2026-07-29 and in no record yet, so the
  first import of it is where the ambiguity lands, and the ruling is owed
  before that import rather than after.
- **What an author names, closed 2026-10-07 by act 11's design** and built
  on 2026-10-08: section 3.2's member holds the session's person's identity
  and not their name, a person being
  renamable and an author not moving with them, and the surfaces render the
  current name; a row written before the act keeps the claimed name it
  carries, which reads as a claim because it resolves to no person. Per
  `docs/project/design-2026-10-07-iam.md` section 6.
- **The licence boundary**, carried forward 2026-09-06 from the Spec this
  one replaced, where it stood at that document's section 15 and where the
  rewrite dropped it. This crate carries `Apache-2.0` with its own `LICENSE`
  file and the workspace carries `UNLICENSED`. **The suite's licence
  boundary is an open operator question, so neither inheritance nor silence
  settles it**, and both facts stand. It is restored rather than answered,
  the edition act of this date having found `Cargo.toml` still pointing at
  the section it used to live in.
- **The signature's representation**, opened 2026-09-08 with the signature
  itself at section 2.2. The shingle's unit and width, whether the shingles
  run over the whole emission or over the body after the model's reasoning
  block, and what similarity is taken over two of them. **This one is
  load-bearing for the schema rather than for a reading**: the signature is
  derived at ingest, so a later answer re-derives every stored row, and it is
  the one open cell here whose cost grows with the store. The choice is not
  academic. The exemplar essays of 2026-09-08 carry a reasoning block holding
  a fifth of their assertions, and the 0.5B validation pair carries none at
  all, so a signature over the whole emission and one over the body answer
  differently about the same two runs.
- **Whether `device` resolves the card from the architecture**, opened
  2026-09-08 by the same act. Section 2.2 holds `device` as one member, and
  the ablation of that date separates the two: two A6000s produce
  byte-identical essays under one seed, and an A6000 against an Ada shares
  three headings of eight. **A sweep over a member the schema cannot resolve
  reports a divergence it cannot attribute**, so section 5.4's reading is
  only as fine as this answer. Whether the member splits or carries an
  architecture beside the card is the ruling.
- **Whether this crate scores a correctness verdict, closed 2026-09-09.** It does not,
  and it holds one: per issue #523's ruling the task scores its own run and the verdict
  rides a trace event the task authors at close, so section 2.2 carries the member and
  section 3.1 lands it, this crate computing nothing. The reproduction verdict was never
  this question and is not a score: it is the projected comparison of two rows, which
  section 2.6 now holds as a recorded query. **A scorer that is not the task's is named
  on its own verdict and never in the run's tuple**, so a second scorer adds a reading
  rather than changing what the run was.
- **The word "cell", closed 2026-09-09.** It carried three senses rather than the two
  the charter's section 9 named, and the count settled it on the operator's ruling of
  that date. A **cell** is a named open question, which is what this section's own
  heading calls its items and what a hundred and fifty uses across thirty documents,
  counting whole-word cell and cells over the markdown of `docs` and `process` outside
  the frozen archive and outside the four files this act sweeps mean. A matrix
  coordinate is an **entry**. A declaration plus a task plus a run is a **trial**, which
  the charter's section 3.3 now says. Four sites moved in this document, the reference
  trials of sections 2.3 and 8 and section 8's trial record, and the one document
  outside this crate that moved is `docs/technical/weaver-agent/reproducibility.md`,
  whose six uses are the matrix coordinate. The matrix-coordinate sense also stands in
  code and in deposits, the driver `confirm_cells.py` by name and a `cells` key the
  configs and every deposit written against them carry. **Those are weighed and
  left**: a filename and a stored key are not prose, and
  renaming them would strand the deposits that already spell it.
- **The fork's cut inside a turn.** The restoring load's lineage names the
  turn the holdings stop at and nothing finer, and the charter's section 3.5
  fork needs a position within one and a token forced there. It is owed at
  a contract when the fork is built, per issue #524 and the ruling of
  2026-09-09 that a branch occurs at load time, and nothing here is blocked
  on it: section 5.4's arm branches at a turn boundary the mechanism
  carries, and section 5 says which feature each row member serves.
- **The identity prefix's length, closed 2026-09-09 by the third act of that
  date.** Section 5 has a whole-run arm's branch position be the resident
  length of the parent's identity prefix, and nothing that crossed
  `weaver-analysis-web-contract` carried it: the summary held the closing
  count and the output count, from which section 3.1 derives the first
  draw's position and no earlier one. The SPU records the figure at open,
  per its section 4.2, and no event carries it, so the emitter derives it
  from the run's first generation and the contract's section 2.2 now
  carries it once per run, section 2.2 above holding it on the row. Per
  issue #527, found by the review of PR #526.
- **How people authenticate to this server, closed 2026-10-07** by the
  operator: by passkey (WebAuthn), and by nothing else, no password, no
  second factor and no external identity provider. Section 2.13 states the
  rules and `docs/project/design-2026-10-07-iam.md` the mechanism, the
  library and the threat model. **Relaxed for the pre-release stage on 2026-11-10** by
  the operator: a sign-in token the host issues is a second method where the
  server's config admits it, the passkey staying the default; section 2.13's
  item on the sign-in token states the ruling in his words, its reasons and
  what release decides.
- **The role vocabulary, closed 2026-10-07** by the operator: per agent,
  `observer` (seeded with `show`) and `operator` (seeded with `show`,
  `validate`, `load`, `unload`, `stop` and `turn`, and with `save-point`,
  `restore` and `force-unload` since 2026-10-08, section 2.13), authored rows whose verbs
  the server-wide admin writes; server-wide, `admin`, which governs the
  connections, the persons and the grants, and the per-agent roles' verbs,
  and grants no action on any agent by itself. The converser this document
  had proposed is not among them. **One reading is the operator's to
  confirm**: one role of each kind per agent, held by any number of people,
  per section 2.13. Whether a grant here and a sudo rule on the box share one
  vocabulary was part of this election, and the answer is that they need
  not: the box's rules carry no `turn`, the gate's own admission governing
  the data plane, and a role here names `turn` as an action this crate's
  gate checks.
- **What section 7 is called, now that one of its three is not a seam**,
  opened 2026-09-16 by the act that deleted this crate's seam record to
  `weaver-admin`. The heading reads "The seams", and 7.2's admin verbs are
  reached through admin-con's sudo invoker on the agent's box, as fixed
  command lines and never as a process the server runs, under a contract
  that binds this crate to nothing. Renaming the heading, moving 7.2 out from
  under it, and widening the word are three answers and this document
  elects none of them. Nothing in the section's content is wrong and no
  read is blocked on the choice, so it is named here rather than settled.
  **The act that deleted 7.1's seam record the same day left this count
  where it stood.** A world-facing boundary is a seam that carries no seam
  edge, per `weaver-gate-world-contract` section 0 and `weaver-admin-PRD`
  section 6, so the gate reach is still one of the two the heading names
  correctly and only its record went. **The act that moved 7.3's record to
  the emitter's charter leaves it the same way**, a reach the initiating side
  declares being no less a seam for being declared elsewhere, and what it
  does change is that this crate's charter now records none of the three.
- **The install's locations and its applier, opened 2026-11-11** by the act
  of that date (section 8.1). Two questions are the operator's, each
  changing a hard boundary he wrote: whether this repository's own
  locations (the binaries, the units, its config root, the operations log)
  may be written as overridable defaults, the box facts that are
  WeaverAgent's staying out either way; and whether a script the operator
  runs as root to apply the staged directory, which the product never runs
  and nothing from the network reaches, stands outside the
  one-privileged-invocation rule. The act is built to lose nothing either
  way: a default fills a parameter that already exists, and an applier
  applies the manifest the tool already writes.
- **The connectors' restart policy, opened 2026-11-11.** Section 8.1 writes
  `Restart=on-failure` for each unit as a new behaviour for the operator's
  ruling. The interaction he rules on is admin-con's: at admin-con's fault,
  a `load`'s admin runs on in the agent's scope, any other verb's admin
  sits in admin-con's unit and ends with it as the service manager ends
  what the unit holds (the moment measured nowhere yet), and the restarted
  admin-con knows of neither, admin's own locks refusing a second verb
  while one runs. What a restarted admin-con owes a verb that was in flight
  is the open question.
- **Squashing the migrations, held 2026-11-11.** Epic #18's install item
  carried it under the operator's ruling of 2026-10-08 that no deployment
  held data before it. A deployment now holds data, and a squash makes the
  server refuse a store that applied the migrations one by one, since every
  applied migration is checksummed, so it waits on his ruling.
- **The declaration's lineage, opened 2026-11-11** (sections 2.4 and 6.1).
  The Planner's default, following the operator's ruling 9 of that date:
  every save appends an immutable row whose parent is the row it was opened
  from, the lineage kept linear by the handler and the schema admitting a
  second child for the ablation's arms. Whether a saved declaration may
  instead be edited in place is the operator's ruling.
- **Who may declare, opened 2026-11-11** (section 6.1). The Planner's
  default is the server-wide admin grant, since no agent row exists before
  register for a per-agent role to attach to. A role of its own for
  declaring would be an act in section 2.13's role shape; the choice is the
  operator's.
- **The gate's field, opened 2026-11-11** (section 6.1). The screen
  declares the gate's tool election alone, its admission being its
  gate-con's, filled at Provision. The operator's words for the first step
  were "which gate", and he has spoken of an agent with several callers
  through one gate, so the field may grow.
- **A card's identity, opened 2026-11-11** (section 6.1). A specific card is
  held as the text the operator typed until a host plane reports the cards
  a host holds, and only then is it checked. A class's family is the
  capability table's word, so a host that names a card's family otherwise
  would need the table to map it.
- **A host's reserved card, opened 2026-11-11** (section 6.1). The
  operator's clarification is that a host may reserve a card for itself,
  which then satisfies no requirement and belongs to no class. Where the
  reservation is declared and how start reads it wait on the host plane's
  Spec; the declaration screen declares nothing of it.
- **The session parameters' starting values, opened 2026-11-12** (section
  6.1). The Planner's default: the maximum tokens per turn and the context
  capacity start empty in a new declaration's form, and the operator enters
  each. The SPU's own comment on the context capacity, at WeaverAgent
  `5ec7ecf8`, reads "The declaration now governs, sized by the operator
  against the KV cache's device footprint, 32768 being the ruled starting
  point on this deployment." The screen does not adopt that value, since
  the capacity's fit is judged at admit against devices the screen does
  not know. Whether either parameter shows a starting value is the
  operator's ruling. The seed starts empty too, and a screen that offered
  to draw one would be a question for the operator, not built.
- **A statement sent before its deadline and answered after it, opened
  2026-11-11.** The admission's landing is one statement, and section 7.2
  polls no landing once its deadline has come, so none waiting on the pool
  then sends its statement. One sent just before the deadline and answered
  just after it still commits while the connection closes
  `admission_incomplete`, so the row carries a `show` whose admission the
  server refused until the reconnection's `show` lands over it. The window
  is that statement's flight, from going out to its answer. Measured
  2026-11-11 from the call to the answer, an idle pool's hand-over
  included, so an upper bound on the flight: a median of 0.95 ms and a
  longest of 5.2 ms over 6000 landings. Closing it means the store refusing
  a landing past the deadline on the store's own clock, which changes what
  the store promises, so it waits on the operator's ruling. The same window
  stands, unclosed, at every write section 7.2 bounds by a deadline since
  2026-11-12, section 2.13's first records, outcomes and refusals and section
  2.14's raises, restorations, claims and back-offs, each one statement's
  flight, and the ruling that closes it here closes it there.
