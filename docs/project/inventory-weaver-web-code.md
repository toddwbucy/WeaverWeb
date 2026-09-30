# Inventory: the weaver-web code against its rewritten charter

**Status:** INVENTORY, opened 2026-09-06. A working register rather than a
member of the document set, and **nothing here is decided until the operator
rules on it.** It reads the code that stands in `crates/weaver-web` against
the charter and Spec that replaced the ones it was written to, and says of
each part which of three things it is.

**Why it exists.** `weaver-web-PRD` and `weaver-web-Spec` were rewritten
whole on 2026-09-04 for the instrument, replacing a charter written for an
interface to an individuated agent. **The implementation was not read in that
act and has not been read since.** At the opening of this register it stood
at **4,532** lines of Rust, eleven templates, and a Postgres schema, all
written to the retired text, and no document in the corpus recorded the gap.
The figure read 4,531 until 2026-09-07 and was off by one.

**Two of those acts have now landed.** The crate stands at **4,002 lines**
and one migration, and the register below marks what moved.

**Date filed:** 2026-09-06
**Document ID:** `inventory-weaver-web-code`
**Editorial:** Per the Working Rules. ASCII, absolute dates.
**Landing PR:** #585

## What the reading found

**The charter describes a diagnostic instrument and the code implements a
chat client.** The store's tables are `participants`, `channels`, `members`,
`channel_events` and `sessions`. Among its eleven templates are
`channel.html`, `channels.html` and `sidebar.html`. The rewritten Spec's
section 2 names seven tables and not one of them is among those five.

**Every module cites a document the tree does not hold, and git does.**
`traceview.rs` cites "Spec section 12", `repro.rs` cited "section 17" until
PR #498 took the file, `queue.rs` "section 8", `wire.rs` "section 16",
`store.rs` sections 5 and 14, and `Cargo.toml` "the Spec's section 15". The
rewritten Spec has ten sections and every one of those citations resolves
against **the Spec at `13b8a6a`,
2026-08-25, the last commit before the rewrite**, where section 12 is the
trace view, 13 the HTTP surface, 14 sessions and roles, 15 the open
elections, 16 the link and 17 the confirm view. Git is the archive and the
tree is not, per the Working Rules, so **a reader checking any verdict below
reads that commit** rather than finding nothing.

The exception is the schema. `migrations/0001_init.sql` cites
`docs/SPEC.md section 4`, **a path that exists nowhere in the tree or in its
history**, so the schema alone has a stated authority that cannot be read.

**And the crate has none of the seven directories the Spec draws.** Section
1 names `store/`, `ingest/`, `authoring/`, `queue/`, `surfaces/`, `seams/`
and `link/`. Section 1's tree is a `src/` layout and this compares against it: the
crate's `src/` directories are `adapters/`, `bin/` and `web/`. Beside `src/`
the crate root holds `assets/`, `deploy/` and `migrations/`, which section 1
does not draw and which this register counts elsewhere. What carries
section 1's names are files rather than modules: `store.rs`,
`queue.rs`, `wire.rs`, `traceview.rs`, `repro.rs`, `router.rs`,
`registry.rs`, `lifecycle.rs`, `channel.rs` and `config.rs`. **`repro.rs` is
gone as of PR #498** and the other nine stand, so section 1 is one name
closer to descriptive by subtraction rather than by anything it says.

## The seam the code separates along

**The retired charter's own two roles are where the code splits**, which is
the finding that makes this cheap rather than expensive. That charter named
a user who converses and an operator who drives lifecycle and reads the
record, and `web/mod.rs` says so in its own words: `user` is the gate
surface, channels and messages, and `admin` is the operator surface,
lifecycle verbs and trace views.

**The conversation half is what the rewrite retired. The operator half is
the instrument's ancestor.** The link that carries both was built against
the framework's public boundary rather than against the conversation model,
which is why it survives nearly whole.

## The register

Three verdicts. **Retires** means the rewrite removed its subject.
**Carries** means the new documents describe the same thing it does.
**Wants a ruling** means the mechanism survives and the subject moved, so
the operator says whether it is reworked or rewritten.

### Retires, 882 lines by file and fewer in fact

**These four cannot be lifted out independently**, which this register did
not say and the archive act of 2026-09-06 found by trying. **Ten call
sites cross into them from modules that do not retire**, and one of those
sites is the role model, so `registry.rs` splits rather than going. The
lines below are the files' and the removable subset is smaller.

**Measured 2026-09-13 at `0bca344`**, which is the last commit before the
archive was deleted and is reachable from `main`. This map is the input to
the act that removes them, so it is stated in three parts because the
removal has to touch all three.

**Ten sites**, a site being a use of an item:

| in | crosses to | at |
|---|---|---|
| `bin/weaver-web.rs` | `registry::reconcile_providers` | 37 |
| `bin/weaver-web.rs` | `registry::reconcile_agents` | 60 |
| `queue.rs` | `router::on_agent_message` | 185 |
| `queue.rs` | `channel::messages_since_last_close` | 223 |
| `web/admin.rs` | `registry::Participant`, in a signature | 37 |
| `web/admin.rs` | `Participant::is_admin` | 39 |
| `web/mod.rs` | `user::routes` | 39 |
| `web/mod.rs` | `registry::Participant`, in a signature | 83 |
| `web/mod.rs` | `registry::by_id` | 95 |
| `web/mod.rs` | `channel::EventView`, in a signature | 230 |

**Five imports**, at `bin/weaver-web.rs:10`, `queue.rs:7`,
`web/admin.rs:9`, and `web/mod.rs:12` and `:15`.

**Four module declarations**, which are how the retiring modules are
reachable at all and are the lines that make the crate stop compiling if
they outlive their files: `lib.rs:8`, `:13` and `:14` for `channel`,
`registry` and `router`, and `web/mod.rs:10` for `user`.

**The count has been wrong twice and the rule is why.** This register said
nine from 2026-09-06 to 2026-09-13, counting `queue.rs`'s import as a site
while not counting four others. Corrected to eight the same day, it was
still short: `user::routes` and `Participant::is_admin` are calls that name
no module path, so a search for `channel::`, `registry::` and `router::`
cannot see them. **`web/user.rs` is the largest of the four retiring files
and carried a row of no crossings at all.**

A method call is the case a path search structurally misses, so the
measurement is now taken the other way round - enumerate what the four
modules export, then find each name. That is reproducible, which the
earlier figures were not.

| file | lines | why |
|---|---|---|
| `web/user.rs` | 429 | channels, messages, members, the session open. No surface in section 3 is a conversation |
| `router.rs` | 212 | mention parsing and multi-agent invocation routing. The rewrite has no mentions and no volley |
| `channel.rs` | 130 | channel reads and log pages |
| `registry.rs` | 111 | **splits, and does not retire whole.** The participant as a conversation member goes, and `Participant`'s `role` field with its `is_admin()` is the role model the charter's section 6 keeps as structural, which `web/admin.rs` gates on. Separating them is the section 6 act |

Five of the eleven templates go with them: `channel.html`, `channels.html`,
`sidebar.html`, `name.html` and `event.html`. **The other six follow their
modules** rather than retiring on their own: `base.html` carries, and
`lifecycle.html`, `agent_config.html`, `trace.html`, `trace_event.html` and
`repro.html` want the ruling `web/admin.rs` wants, being that module's
rendering.

**`repro.html` is the first of the six to be answered**, and it did not need
the ruling the other five wait on. It went with `repro.rs` at PR #498,
because a template whose only handler has retired renders nothing. **Ten
templates stand as of that date**, and the five that follow `web/admin.rs`
are `lifecycle.html`, `agent_config.html`, `trace.html`, `trace_event.html`
and the `base.html` that carries.

Four of the five tables go with them too: `participants`, `channels`,
`members` and `channel_events`. **`sessions` wants a ruling and is the one
table that is not simply retired.** It holds a token, an opened and a closed
timestamp, and a `participant_id` referencing a table that retires, so its
foreign key goes whatever else happens. Whether a browser session survives
at all is the charter's section 6 question rather than this register's:
identity and authentication are deferred there with a named trigger, so the
table's subject is deferred with them.

**The second migration is where that question was already anticipated.**
`0002_roles.sql` adds `role` to `participants` on the operator's ruling of
2026-08-19, its own comment saying the two surfaces separate now so that
identity later attaches to standing roles rather than forcing a
rearchitecture. **That is the charter's section 6 argument, made in the
schema nine days before the rewrite made it in prose**, and it is the one
place in the crate where a ruling is cited by date. The column retires with
`participants` and the reasoning does not.

### Carries, 1,770 lines

| file | lines | what the new documents call it |
|---|---|---|
| `wire.rs` | 813 | section 8's dialed link. Its seven services are `turn`, `verb`, `status`, `declaration`, `trace_runs` and `trace_run`, six asks correlated by id, plus `trace` streaming unasked. **Every one maps to a surface or seam the rewrite keeps** |
| `store.rs` | 344 | the Postgres pool, the migration runner and the single writer task. The infrastructure survives whole and the schema it runs does not |
| `adapters/gate.rs` | 150 | section 7.1, the gate seam, dial-per-turn |
| `config.rs` | 128 | section 8's placement: box facts in the box's config, the roster announced in the link's hello |
| `lifecycle.rs` | 101 | section 7.2, the admin verbs, one JSON object rendered verbatim and failure never swallowed |
| `bin/`, `lib.rs`, `adapters/` | 234 | the two processes section 8 names |

**`wire.rs` is the most valuable thing in the crate.** Link loss marked and
never smoothed, pending asks failing typed, discontinuity marks inserted
into every trace view: that is the absent-not-empty discipline the rewritten
charter argues for, already built.

### Wanted a ruling, 1,879 lines, and three of the five no longer do

**Read against the charter on 2026-09-07, module by module.** Three of the
five turn out to be decidable without the identity act, and the two that
remain wait on more than it. **The five stay in one group because the
register counted them as one**, the three ruled in prose below and the two
that remain carrying a table of their own.

**`repro.rs`, 369 lines: retires, and its successor is a different object.
Landed at PR #498**, where the removal came to 595 lines with the surface
that depended on it. The reading below stands as it was made.
Its comparison is seven JSON-pointer equalities at turn grain returning
`reproduced: bool`. The charter's section 4 aligns on turn and position and
compares the emitted token, the surprisal, the entropy and the ranked
alternatives with their mass, claims byte equality only where the tuple is
held, and returns a divergence report naming the differing member **and
never a verdict**. One field overlaps and even that is compared as a whole
vector rather than per position.

**The deeper mismatch is what it does with conditions.** It compares
`/rendered`, `/sampling` and the seed as checks that can fail. The charter
puts those in the tuple and uses them to decide whether equality is the
question being asked at all. Same values, opposite roles: a run under a
different seed is a failed check in the old model and a different compound
in the new one, where the question is malformed rather than false. **It is
built on the premise the rewrite retired, that reproduction is a verdict.**

It is also clean to remove, which the other retirees are not: its imports
are `crate::wire::Link` and `serde_json`, and it touches the channel store
nowhere, which its own comment was careful about. **Its name goes with it**:
`repro` appears in no document of this crate, its own doc comment calls it
the confirm job, and at a glance the file name reads as `repo.rs`, which is
a misreading the operator made in this register's own review.

**`queue.rs`, 311 lines: retires, and its mechanism is not this crate's to
keep.** `Invocation` carries `channel_id` and `agent_participant_id`, and
`AgentState` tracks the channel of the in-flight turn. Every field is the
retired charter's. Single-flight-per-agent and batch-on-drain are worth
keeping and **the charter has already placed them elsewhere**: a runner
drains the queue and the reload that runs it is the branch, per section 3.5,
and that runner is filed against the olympus seat at issue #442. What stays
on this side is the authoring end, a staged experiment written into the
Spec's section 2.5 table through its section 3.2 path, which is a row insert
rather than a queue.

**`traceview.rs`, 327 lines: keeps, and one end is rewritten.** It touches
the retiring half nowhere: no `channel_id`, no participant, no registry, and
its imports are tokio, serde and std. **What it holds has no home upstream
and should not get one**: bounded rings, a per-agent broadcast, sequence ids
kept monotonic across connector restarts, and rotation, truncation, parse
failure and link loss all surfacing as discontinuity marks **never
smoothed**. That is absent-not-empty at the viewer, built twelve days
before the charter argued for it everywhere: `traceview.rs` arrived
2026-08-24 at #334 and the charter was rewritten at `daa12ba` on 2026-09-05.

What changes is what it consumes. It parses `serde_json::Value` by hand off
a raw file tail, and section 7.3 now names `weaver-analysis-web-contract` as
the shape with the position conversion made at ingest, while section 3.4
wants entropy and surprisal timelines with absence stated and a click
pulling a position's alternatives. **The tail becomes an ingest consumer and
the rings, marks and broadcast stay.**

### The two that still do, 713 lines, and what they wait on is a trigger met

| file | lines | the question |
|---|---|---|
| `web/admin.rs` | 440 | **inner routes**, mounted by `web/mod.rs` under `/admin`: `/lifecycle`, `/lifecycle/{agent}/{verb}`, `/agents/{agent}/config`, `/trace/{agent}`, `/trace/{agent}/stream`. Those are Agents, Compose and Open a trace, at three of the ten surfaces, and its handlers gate on `registry::Participant`. **It was 598 lines and six routes at the reading**: the sixth was `/repro/{agent}` and it retired with its module at PR #498, which is why what remains to rule on is smaller than this register first measured |
| `web/mod.rs` | 273 | the HTTP surface's split into `user` and `admin`. The split is the retired charter's two roles, the rewrite has one operator, and the gate is `is_admin()` |

**They read 872 lines at the reading and stand at 713.** `web/admin.rs`
lost its confirm surface at PR #498 and `web/mod.rs` lost the one line that
held it in `AppState`, which is the whole of that module's contact with the
retirement.

**They waited on two things, and as of 2026-09-07 one is discharged and the
other is stated but not met.** What follows is the reading as it was made, with what
closed each.

**The charter's section 6 deferred to a trigger it did not name.** It read
that identity, authentication and transport encryption "are deferred with a
named trigger", and no trigger was named in either document. The retired
Spec's section 14 named the act plainly, "until the IAM act, sessions are
anonymous, so anyone on the LAN can claim an admin name", and said what
changes when it lands: "it becomes access control when the IAM act makes
sessions prove who they are. The gate itself does not move then, only the
proof does." **`IAM` appeared zero times in the rewritten charter and
Spec.** So the rewrite kept the deferral and dropped both the act's name and
the condition, and until section 6 said what it waited on, answering it was
not a ruling anyone could make.

**Closed at PR #493.** Section 6 names the IAM act, cites issue #336 as its
register, and states two triggers rather than one because they are not one
event: identity and authentication are due when the listener becomes
reachable by anyone the operator has not already admitted, and transport
encryption when the traffic leaves the network the listener was placed on.
**Admission is by placement**, which that act had to say outright, because
without it the first trigger reads as met on the day it was written.

**So the question is stated and the answer is not this register's.** What
these two modules wait on now is a trigger being **met**, which is an
operator ruling about where the listener runs rather than a reading of the
code.

**Both modules were also surfaces over a store that did not exist.**
`web/admin.rs` serves three surfaces that read the Spec's section 2 tables,
and the schema was unwritten. **Closed at PR #499**: the Spec's seven tables
stand in one migration and the five conversation tables are gone, so this
condition is discharged rather than merely stated, and it is no longer the
act that comes first.

## What the operator rules

1. **Whether the retiring 882 lines are deleted, kept, or moved. Ruled
   2026-09-06: moved. Re-ruled 2026-09-13: deleted**, the archive at
   `crates/weaver-web/archive/conversation/` removed and git left as the
   copy. **The first ruling stands as what was decided then**, and it
   landed at PR #469 as a frozen copy with a manifest and checksums, the
   build still compiling the originals. What changed is not the argument
   but a fact the argument did not have: **a copy drifts from what it
   copies.** Three of the nine files the manifest hashed no longer matched
   their originals, and the graph work made the cost concrete, an ingest
   walking the tree meeting two `registry.rs` with identical symbols.
   Landed at PR #563, where the reversal is argued in full. The
   options as they stood: deleting
   them is cheap and makes the crate read as what it is. Keeping them means
   the crate builds and serves something while the instrument is written
   beside it. **Moving them is the third answer**: an archive at a named path
   with a checksum, which is the shape the quarry was frozen under, keeping
   the conversation half reachable to whoever writes the chat interface the
   vision still names. Git holds them either way, so the choice is about
   what a reader of the tree meets rather than about loss.
   The live conversation-code deletion landed at PR #643.
2. **Whether the schema is migrated or replaced. Ruled 2026-09-07:
   replaced, and git is the archive. Landed the same date at PR #499.**
   The five conversation tables are gone and the Spec's section 2 seven
   stand in one migration, with three of their properties perturbed against
   a live PostgreSQL before the file entered the tree. **An existing
   development database is dropped rather than migrated**: `sqlx` checksums
   each applied version against its source, so a database that ran the old
   `0001_init` refuses at startup, and that refusal is this ruling working
   rather than a defect. The distinction was empty and this
   register put it as a ruling anyway: the five tables share no column any of
   the seven wants, so a migration between them is a drop and a create
   wearing a migration's name, and the old file's history is in git whichever
   way. **The schema is also virgin**, per the operator: the store reported
   below is a development box's and nothing downstream of the schema stands,
   so the row counts settle nothing and were never a reason to hesitate.
3. **What the five ruling-wanted modules become**, one by one, and in what
   order against the surfaces. **Three are answered as of 2026-09-07** and
   needed no ruling once read against the charter: `repro.rs` and `queue.rs`
   retire and `traceview.rs` keeps with one end rewritten, per the register
   above. The `queue.rs` deletion landed at PR #643.
   `repro.rs` is out at PR #498. **The two that remain are
   `web/admin.rs` and `web/mod.rs`**, and as of 2026-09-07 they wait on one
   condition rather than two: the schema landed at PR #499 and the charter's
   section 6 named its act and its two triggers at PR #493, so what they
   wait on is the **trigger being met** rather than being stated. That is an
   operator ruling about where the listener runs and not a reading of the
   code, which is as far as this register can take them.
4. **Whether the crate's edition alignment rides this work. Ruled
   2026-09-06 and landed at PR #470**, and it was smaller than its comment
   said. `Cargo.toml` pins 2021 against the
   workspace's 2024, calls the migration real work because "the sqlx and
   askama derives are where an edition move bites", and holds it at "the
   Spec's section 15", which is the archived Spec's open elections and is
   section 10 in the rewritten one. **Measured on 2026-09-06 against the
   crate as it stands, including the conversation half**: with `edition` set
   to 2024, `cargo check` and `cargo test` pass with no error, so the
   derives do not bite. What the move costs is twelve `clippy` warnings
   where 2021 gives none, every one `collapsible_if`, and one `cargo fmt`
   run, 2024 formatting differing from 2021's. That is an afternoon rather
   than a project, and the comment should say so or go.

## What stands in the store, as reported

**A dated observation by a named reporter**, which is the shape this crate's
own section 2.3 requires of presence and is the honest shape for this too.
No store stands on the box this register was written from, `postgresql`
being inactive there, and one stands on olympus.

**Reported by the olympus seat on 2026-09-06**, from a `weaver_web` database
with both migrations applied:

| table | rows | |
|---|---|---|
| `participants` | 5 | one admin, four users |
| `channels` | 11 | |
| `members` | 32 | |
| `channel_events` | 259 | turn-open 87, close 71, message 64, member-change 21, app-error 16 |
| `sessions` | 5 | |

**Every event falls between 2026-08-19 and 2026-08-20**, the two days around
the roles ruling `0002_roles.sql` carries. The turn rows name eighteen runs
of the alpha agent by run label and ten turns by turn label.

**No `weaver-web` unit is installed on that box.** That is the observation.
Whether a process, a container, or a hand-started binary serves the database
was not checked, so this register says a unit is absent and does not say
nothing is serving it.

**What this does and does not settle, by row.** The 158 turn rows, opens and
closes, are **a mirror of turns whose canonical record is the trace under
admin's custody**, so dropping them loses a copy rather than a fact, on the
condition that the eighteen runs' traces still stand, **which is a check for
whoever rules on the drop and not a thing this register asserts**. The 64
message rows are the conversation itself and have no record elsewhere, so
whether they are wanted is the operator's.

**The remaining 37 rows are unclassified and the mirror argument does not
reach them.** Twenty-one member changes and sixteen application errors are
neither a turn nor a message: the first is channel membership, whose subject
retires with `members`, and the second is this crate's own faults, which no
trace under admin's custody carries because they never reached an agent.
**Whether either is wanted, and whether the app-error rows are the only
record of a fault worth reading, is a question this register leaves open**
rather than one the mirror answers.
And **which other boxes hold a store, neither seat can see.** This crate
runs on one machine while the agents run on another, per the charter's
section 5, so a store on a box neither seat reaches is the expected case
rather than an unlikely one, and the answer above is one box's and is not
the deployment's.

## What this does not decide

**It proposes no act.** The register is what a reader needs to see before
choosing one, and the choosing is the operator's.

**And it makes no claim about a deployed store**, per the third question
above, which is the one thing in this register that needs a reach this seat
does not have.

**It is not blocked by the artifact identity of issue #465**, which reaches
the artifact table alone. Nothing above waits on it, and the store's schema
act does, in the artifact table and in the run row's identity member.

## What the corpus should say and does not

Two documents carry statements this reading falsifies, and each wants an act
of its own rather than a line here.

- **`weaver-web-Spec` section 1's layout is aspirational and reads as
  descriptive.** It names seven directories and the crate has none of them.
- **Nothing anywhere records that the implementation predates its charter.**
  A reader of the rewritten documents would take the crate for empty, and a
  reader of the crate would take the charter for unwritten.
- **The crate's comments cited documents the tree no longer holds,
  forty-five times. Answered at PR #510 on 2026-09-08**, found 2026-09-07 by the review
  of PR #498, which met one of them in the deleted confirm section reading
  `PRD 4.4, Spec section 17`.

  **The documents are recoverable and that is the point of the defect rather
  than a mitigation of it.** `weaver-web-PRD` states the standing rule, "Git
  is the archive and the tree is not", and this register's own reading at
  its section on the seam says every one of these citations resolves against
  the Spec at `13b8a6a`. A reader who knows to look finds them. **A reader
  who does not, and there is no reason they would, meets a section number
  that resolves in the tree they have open.**

  **This register's own figures were wrong and are corrected here.** It
  recorded thirty-five citations, twenty-two dangling and thirteen
  resolving. Measured by occurrence rather than by line, the Spec citations
  alone were **thirty-seven** - twenty-two dangling at sections 11 through 17
  and **fifteen** resolving - with eight charter citations beside them. The
  undercount came from a grep matching a single digit, which silently drops
  `section 10`, and the same grep is why the register said thirteen where the
  act measured fifteen.

  **Corrected 2026-09-15: the total is forty-six and the dangling half is
  thirty-one.** The last paragraph of this bullet re-derives the dangling
  count as thirty-one live, and thirty-one plus the fifteen resolving is
  forty-six, not thirty-seven. **Thirty-seven is kept above as the historical
  figure** rather than overwritten, because the two readings are what this
  bullet is about. An earlier form of this correction moved the part in place
  and left the sum, which is the count-check rule missed inside the register
  whose own subject is a miscount - the fourth instrument failure recorded
  here, after the single-digit grep, the line-oriented search, and the rows
  lost off the head of a probe's output.

  **The twenty-two that dangled were the safe ones.** They named Spec
  sections the rewritten document does not have, so a reader following one
  found nothing and knew it. **The fifteen that resolved were the defect**:
  each named a section the rewritten Spec does have and meant the retired
  document's, so a reader landed on a real section about a different
  subject. `config.rs` cited section 3 for TOML configuration where section 3
  is the write path, `router.rs` cited section 9 for the invocation router
  where section 9 is what is enforced, and `store.rs` cited section 5 for the
  single writer where section 5 is staged experiments. **A dangling citation
  announces itself and a resolving one does not**, which is why the smaller
  number was the worse one. Ten [corrected 2026-09-15 to **twelve**, of
  which the act repointed ten] of the twenty-two stood in `wire.rs`, which
  this register carries, so the defect outlived every retirement and had to
  be answered rather than waited out.

  **The disposition at that act, 2026-09-08: twenty-seven repointed, one
  redirected, nine [corrected 2026-09-15 to **eleven**] left.**
  They are in the six files this register retires and go with their
  modules, so a sweep reporting zero would be reporting on files that are
  leaving. `store.rs`'s role citation was redirected rather than repointed,
  its subject being what the section 6 act separates, and it now names Spec
  section 2.8, which PR #509 landed. Of the eight charter citations, three
  went to section 5 and two to section 3.6, one retires with `queue.rs`, and
  **two named a claim the rewrite dropped rather than moved**, the
  display-engine constraint, which no standing document carries and which
  those two comments now say plainly rather than citing a section for.

  **These figures were taken with a line-oriented probe and are corrected
  above, 2026-09-15, by PR #585.** The crate wraps comments at about
  sixty-six columns, so a citation whose number falls after the break is two
  lines, and no search that reads one line at a time can see it. Measured
  again whole-file with the newlines normalized away, `b97fd0b^1` carried
  **thirty-four** references to sections 11 through 17, **three** of them in
  the `archive/conversation/` copy deleted since, leaving **thirty-one**
  live rather than twenty-two. **Twelve** stood in `wire.rs` rather than
  ten, and the act repointed ten of the twelve, so the defect this register
  says had to be answered rather than waited out was answered in part:
  `wire.rs` lines 572 and 626 carried section 16 forward to 2026-09-14. The
  left set is eleven rather than nine, `queue.rs` carrying a fourth at its
  line 214 and `adapters/upstream.rs` a second at its line 9.

  **That probe reports thirty-one or thirty-two on one stated rule**, and
  the rule is worth writing down because two seats measured this page and
  differed by one. `adapters/upstream.rs:9` reads "Spec sections 10 and 15",
  one head over two numbers. Counted as two sightings it is thirty-two live
  and one of them falls in this range. Counted as one sighting at its first
  number it is thirty-one and none does. **This page takes the second**,
  the smaller and the one a reader counting citations would take, and names
  the row so the other reading is reachable rather than lost.

  **The instrument failed three times and each failure was cheaper to make
  than to find.** A grep matching a single digit dropped `section 10`, which
  is the correction above. A search reading one line at a time dropped every
  wrapped citation, which is this paragraph. And a first reading of the
  corrected probe was taken through `tail -30`, which silently cut five rows
  off the head of its own output and reported twenty-eight where the probe
  had said thirty-one - **a count from a truncated pipe, which is the
  failure `CLAUDE.md` names and this act then committed.** The probe was
  right the second time and the reading of it was not.

  **PR #585 answers seven of the thirty-one's survivors**, at `README.md:97`,
  `deploy/config.example.toml:5`, `deploy/weaver-web.sudoers:1`,
  `traceview.rs:43`, `wire.rs:572`, `wire.rs:626` and `web/user.rs:83`, the
  last of these being one of the eleven left and the only one of them whose
  target survives its module's retirement. **Two more it answers were never
  in that set**, `config.rs:72` citing section 10 and
  `bin/weaver-web-connector.rs:4` citing PRD section 3, both of them
  resolving to a live section about another subject and both invisible until
  the corrected probe. **Ten stand**, and they stand on this register's own
  ground.

- **Two sites carry code a merged act moved past, and both now say so in
  place.** Found by the reviews of PR #510 rather than by this reading.
  `adapters/gate.rs`'s `socket_exists` infers load state from a socket
  path's existence, which the charter's section 8 retired on 2026-09-04 at
  issue #440 in favour of the harness answering with `observe`. And
  `store.rs`'s `reconcile_roles` writes `participants.role`, where Spec
  section 2.8 puts the role on the session, onto a table the schema no
  longer creates. **Neither is a citation defect and neither is fixed by
  fixing one**: they are the code standing where a document has moved, and
  the comments name the act that moved it so the surface's rewrite meets the
  fact rather than rediscovering it. **These are not filed as issues**, on
  the ground that a comment at the line is where a person is standing when it
  matters and an issue restating it is not.
- **No file in the crate carries a conformance header**, zero of eighteen,
  against seven of seven in `weaver-trace` and four of four in
  `weaver-types`. The header is the middle term of the `code -> assertion ->
  doc` chain that phase three runs on, so the crate stands outside that
  regime entirely rather than partially. It is a consequence of the crate
  being absorbed at PR #334 rather than written under the rule, and it is
  not fixed by any retirement, because the files that carry forward are the
  ones that would need headers.
