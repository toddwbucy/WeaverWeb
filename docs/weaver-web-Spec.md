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
  store/        the registry: schema, migrations, the reads of section 4
  ingest/       section 3.1, a consumer of the analysis stream
  authoring/    section 3.2, the writes a surface makes to its own table
  queue/        staged experiments and their states
  surfaces/     one module per surface of charter section 3
  seams/        gate client, admin verbs, analysis stream reader
  link/         the connector and server halves, the dialed link
                of section 8 and not the seam kind the clause below names
```

**This crate links no crate of the agent** and reaches it exactly as an
outside consumer does: a socket dialed by path, a binary run by the
operator's verb, and a record read where the operator keeps it. That
property is load-bearing and is not spent by the seam of section 7.3, which
is a stream this crate reads rather than a crate it links.

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
citation of sections 2.6 through 2.8 in this document and outside it. **The grouping is
stated here rather than carried by the numbering**, which is the cost of a document
whose sections are cited by number.

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
their sequences, so the rule above describes three of the six authored rows and not all
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

The saved configurations the charter's section 3.6 lists under Agents,
**loadable at any time**, which is what makes them a table rather than a
draft buffer. Each row carries:

- the declaration as authored, whole, in the shape `weaver-types-Spec`
  section 2 defines
- **the corpus commit its field shape was written against**, which is this
  row's staleness rule per section 7.2: when the floor moves, the pin says
  so and `validate` refuses in a way the surface can name
- the parent declaration where this one is derived, and **the one thing
  that moved**, which is what the charter's section 3.6 draws
- the last answer `validate` gave, with when it was given
- **the author**, per section 3.2
- **the row's version**, per section 3.2, which is not the corpus commit
  above: that one answers whether the field shape is current, this one
  whether the row has moved since it was read

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
somewhere to land.** Section 3.2 gives every authored row a nullable author
and says the member holds the name the authoring surface had. **HTTP carries
no name between requests**, so a surface can only have one if something holds
it, and that something is this row. Without it the author member has a value
and no source, which is the state this document was in until 2026-09-08.

**It is continuity and it is not a person.** A session is opened by an
operator who claims a name, and what is stored is the claim. Nothing here
proves anyone is anyone, per the charter's section 6, and nothing here is
access control.

```graph
node: web-session-carries-a-claim-and-never-a-proof
kind: assertion
tag: review

edge: asserts
from: weaver-web
to: web-session-carries-a-claim-and-never-a-proof
```

Each row carries:

- the session's own identifier
- **the bearer's digest and never the bearer.** The value the browser holds
  is hashed before it is stored and the lookup is on the digest, so a read of
  this table is not a set of live sessions
- **the name claimed at open**, which is what section 3.2's author member
  takes its value from
- **the role**, per the charter's section 6
- when it was opened, and when it closed where it has

**The digest rule is stated before there is a token to migrate.** The prior
schema stored the bearer in the clear and the fix would have wanted a
migration and an invalidation path for everything already issued, per
issue #336. That table retired with the conversation half at PR #499, so this
document states the shape while stating it is free, on the same reasoning
section 3.2's author member was landed under: the cheap moment is before the
first row exists.

```graph
node: web-bearer-is-stored-as-a-digest
kind: assertion
tag: perturbation

edge: asserts
from: weaver-web
to: web-bearer-is-stored-as-a-digest
```

**Before the identity act a role is a property of the session and not of a
person**, because no person is proved and a role has nothing else to attach
to. That is what the charter's section 6 means by roles being structural
while not being access control: the structure stands, and what is missing is
the proof rather than the shape. **When the act's trigger fires, the role
attaches to the proved identity and this row references it.** The column
moves and the gate does not, which is section 6's own promise made checkable
rather than left as a sentence.

**A session is not an authored row and takes no version.** Section 3.2's
ordering rule answers two engineers editing one declaration, and nobody edits
a session: it is opened once, closed once, and read in between. **Nor is it a
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

## 3. The write path

**Five writers, and each owns its tables.** Section 3.1's ingest lands what the
instrument recorded. Section 3.2's authoring path lands what the engineer authored.
**The read path writes too**, one row and only one: section 4 admits an open query on
the condition that the query is recorded, so the read that serves it writes section
2.6's row and nothing else. That is why 2.6 belongs to neither half. **And the surface
writes one row of its own**, section 2.8's session, at the open and at the close and
never in between, which is the fourth and is why that table belongs to neither half
either. **The fifth is queueing**, which writes section 2.11's batch and its entries and
touches nothing else: section 5.1 has registering and queueing as two acts, and the
second of them is the one that records an order, so it is a writer rather than a state
change on a row another writer owns. **It was four until 2026-09-11**, the batch having
had no row and queueing having had nothing to write. No writer touches another's tables,
and **no surface writes through 3.1**, which is what section 6's rule means and all it
means.

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
section 2.10, all four being what one surface composes and registers. **Each writes only
tables of the authored half**, and **none of them may write a position or a run**, which
is the whole of what keeps a recorded fact a recorded fact and is what the pin below
holds. The sentence read that each writes one table until 2026-09-09, which was true
while Stage authored one.

```graph
node: web-no-surface-writes-the-recorded-half
kind: assertion
tag: compile-fail

edge: asserts
from: weaver-web
to: web-no-surface-writes-the-recorded-half
```

- **Every authored row carries a version, and it is the store's own counter rather than
anything the author supplies.** Sections 2.3, 2.4, 2.5, 2.9 and 2.10 each carry it. It
has nothing to do with the declaration's corpus commit, which pins the floor's field
shape and answers staleness against `weaver-types-Spec`: **one says whether the shape is
current, the other says whether this row has moved since you read it.**
- **Every authored row names its author, and the member is nullable.** Sections 2.3,
  2.4, 2.5, 2.9 and 2.10 each carry it. **Null means the store could not
  name an author when the row was written, and it never means the
  operator**, because a default that guesses writes a fact nobody can
  correct later and an unknown that says so can be filled by anyone who
  knows. **The member holds the name the authoring surface had, and section
  2.8 says where the surface had it from**: the session the operator opened,
  which holds a claimed name and not a proved one. That is the whole of why
  a pre-act author is asserted rather than proved, and it is stated in two
  places because the member is in one section and its source in another.
  **The identity act of the charter's section 6 changes what fills it and
  not whether it exists**, the act attaching authentication to the roles that
  already stand rather than rearchitecting around them.

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
wait.

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

**A surface that renders what is kept reads the store and nothing else** - Open a trace,
Record, Experiments, the matrix, and the returned half of Stage. **Section 4's six reads
serve Open a trace, Record, the matrix and the returned half of Stage, and not
Experiments.** That surface is a list and read four takes an experiment's identity, so
it has no query, and its read is owed at the act that builds it. **This sentence counted
four reads and named every surface served until 2026-09-11**, and it was wrong twice:
about Record, which the fifth read answers, and about Experiments, which is named as
owed rather than served. The matrix joined the served on the same day, the sixth read
landing with the plan's schema.

**Three surfaces author, and one of them writes more than one table.** Compose writes a
declaration and Models writes an artifact row on import, each one table. Stage writes
the staged experiment, the plan with its entries, and the refs, per section 3.2, because
a plan and the experiments it registers are one composition. Each writes through section
3.2 and into the tables section 2 gives it. **Models is the only one of the three that
is not on the path**, which is why the group of section 3.6 is named for how it is
reached rather than for what it does.

**A surface that authors or exchanges also holds an exchange outside this
crate**, a call out and an answer back rather than a write left for someone
else to drain. Compose writes its draft and asks `validate`. Live carries a
turn to the gate and reads the measurement that comes back. Agents drives
the lifecycle verbs and reads the observation exchange. Section 7 names
each of those reaches. The gate is section 7.1 and the charter carries no seam
record for it, and `validate`, the lifecycle verbs and the observation
exchange are section 7.2's, the operator's own program run as a
subprocess and carrying no record either. Section 7's third is the analysis
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

## 7. The seams

### 7.1 The gate

A turn crosses at the gate as any client's does. The gate does not stream,
so a whole-turn answer is presented whole and an in-flight state is clear
rather than simulated. Closes render by kind, and an unnamed close is this
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

### 7.2 The admin verbs

`validate`, `load`, `unload`, and as of 2026-09-04 the observation exchange:
`show` answers one agent's load facts and `list` answers one summary per
admitted agent in a single ask. Load state is therefore **the harness's own
word rather than an inference from a socket's existence**, and no surface
labels it as inferred.

**The observation answers from any position, a running turn included**, as
of 2026-09-05. It is served from inside the turn between tokens, touching no
bracket and cancelling nothing, and the one bound is the single token whose
decode is in progress. So **a roster may read while an agent is answering**,
and a surface has no reason to withhold a status read during activity or to
present a stale one as current. A view built against the earlier posture
would have designed around a refusal that no longer stands.

No verb chains another. This crate offers each as a separate act and nothing
composite.

**`validate` is also the composition oracle, and what it can answer is
bounded.** It transitions nothing, refuses an incoherent declaration naming
the field, and reaches **the box facts admin holds custody of**, per
`weaver-admin-PRD` section 4.3 as ruled 2026-09-05 on issue #456: admin
adjudicates what it provisions, asks the owner where one can be asked
before a process exists, and leaves to the organ what only the organ can
judge. So the Compose surface writes its draft and asks rather than
judging, **and a clean `validate` is acceptance for filing rather than
approval to load.** Whether the artifact resolves, whether the family
exposes the taps the declaration elects, and whether these weights load at
this precision are answered at admission under the agent's identity and
not here. **This crate therefore carries no second copy of the
rules**, only a copy of the declaration's field shape written against
`weaver-types-Spec` section 2 **at a named corpus commit**, which is that
copy's staleness rule: when the floor moves, the pin says so and `validate`
refuses in a way the surface can name.

### 7.3 The analysis stream

The emission this crate ingests. **Its shape is
`weaver-analysis-web-contract`**, landed 2026-09-05 per issue #418, and this
document names the seam and restates none of it. Two of that contract's
clauses reach back into this document and are worth naming where a reader
of this section stands.

**The series is addressed by the ordinal and this crate's store by the
position**, per section 2.1, and the conversion is this crate's to make at
ingest. **What it converts from is owed and does not cross yet**,
issue #461: the contract's section 3 asks the emitter for the resident count
at a generation's close and its output token count, on the summary stream,
and until that lands this seam carries the ordinal alone. **A reader in that
state cannot key its store** and stores what crosses without converting,
rather than deriving an address from a figure that does not answer.

**A diagnostic record whose bracket did not close certified is not drawn**,
the refusal naming the outcome. A serving record has no gate and none is
owed.

## 8. Placement and the link

Two processes joined by one dialed link: a connector holding the box-bound
reaches, a server holding the presentation stack. Colocated by default and
separated by changing one address.

**Nothing in the read or write path is box-bound to the agents.** The reader
is a store client, the runner is a queue consumer, and the front end with
its store runs on one machine while the agents run on another. That crossing
is a declared boundary under the charter's section 5 rule and appears in the
trial record like any other.

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
| no surface writes a position or a run | compile-fail: a doctest constructing a recorded-table writer from an authoring path does not compile |
| an authored edit against a stale version refuses | perturbation: drop the version check, the second edit silently wins |
| an authored row names its author or names none | perturbation: default the null to the operator, a row nobody can attribute reads as attributed and is uncorrectable |
| every authoring write passes the member, and the column has no default | perturbation: give the column a null default and drop the member from one write path, the write lands and the row is indistinguishable from one whose author nobody could name |
| a registered experiment's question is immutable with the rest of it | compile-pin: no mutating path off the frozen type reaches it |
| import computes the identity rather than accepting one | perturbation: take the operator's digest, two boxes disagree about one artifact |
| a record identity names at most one catalog row for a file or a directory artifact, a renamed split excepted per sections 2.3 and 10 | perturbation, at the schema: drop the unique index that holds for every shape but a split, a second import of the same file or directory opens a second row and a lookup answers two where it owes one |
| the bearer is stored as a digest and never in the clear | perturbation, at the schema: store the bearer and look up on it, a read of the session table is a set of live sessions |
| a session carries a claimed name and never a proof | review, over the open path: nothing between the posted name and the row tests it, which is the posture section 6 defers and not a defect |
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

**A watch that cannot fail is not a test.** For each perturbation above, the
act that lands it states what removal makes it fail and confirms it does.

**A row marked owed has no instrument and is not counted as enforced.** One
stands so marked as of 2026-09-11, the batch's order, whose table section 2.11
describes and no migration builds. The marking is the point: a row reading like
the thirty-two beside it would tell a reader the claim is held, which is the
same failure as a watch that passes either way and is why this table says which
it is.

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
- **What an author names, which the identity act of the charter's section 6
  settles.** Narrowed 2026-09-08: section 2.8 says the value comes from the
  session's claimed name, so what stays open is what the act makes that name
  mean rather than where it comes from. Section 3.2's member holds the name
  the authoring surface had, and whether that later resolves against an
  identity the act stands up or
  stays a name is that act's ruling and not this document's. **The member
  stands under either answer**, which is why the schema is not blocked on
  the election and why the election is not blocked by the schema.
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
  outside this crate that moved is `docs/technical/weaver-agents/reproducibility.md`,
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
- **What section 7 is called, now that one of its three is not a seam**,
  opened 2026-09-16 by the act that deleted this crate's seam record to
  `weaver-admin`. The heading reads "The seams", and 7.2's admin verbs are
  the operator's own program run as a subprocess under a contract that
  binds this crate to nothing. Renaming the heading, moving 7.2 out from
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
