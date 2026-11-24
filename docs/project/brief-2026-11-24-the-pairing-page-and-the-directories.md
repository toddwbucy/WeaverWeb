# Brief: pairing as a page of its own, and a declaration that starts at directories

Date: 2026-11-24. From the Planner, to the WeaverWeb Executor. This brief is the stack's
first commit, so that what was asked stands beside what was built.

## Goal

Make Weaver Web's pages follow the operator's rulings of 2026-11-23, for issues 177 to
187. Pairing becomes a page of its own under Provision, where the agent is named. The
declaration starts at directories. Weaver Web lists and formats no disk. No page waits on
"the image's setup script".

## Rulings

The operator's, 2026-11-23, in the operator's words:

- "Pairing is its own page."
- "the agent is named at pairing. The agent must be identified at every level where it
  interacts."
- "The declaration starts at directories, not drives."
- "The operator lays the disks before pairing, with our help: a models disk at least 10 GB
  larger than the model, and a state disk, mounted at our default paths unless they choose
  others."
- "At first console login the appliance shows a setup prompt. One command starts an
  interactive shell script. It walks the operator through finishing setup, the disks
  included, and it ends in pairing with Weaver Web."
- "One agent per VM is the recommendation."

**Five points are the operator's and are not settled by this brief.** Each is built to the
lean given, and each is a marked placeholder in the Spec child until the operator rules.
The stack does not leave draft while a placeholder stands.

1. Where the pairing stage's report shows, once it leaves the pairing page (#181). Lean:
   on the Directories page.
2. How the declaration begins after a pairing. Lean: the Pair page's success leads into
   Directories for that agent, and Declarations lists the paired agents whose declaration
   is open.
3. The models disk's margin of 10 GB. Lean: the Models and Directories pages warn where
   the margin is short, and nothing is refused for the margin alone.
4. Confirm's "image version". Lean: it reads "the appliance's version", from the survey's
   `image` members as now.
5. How the Pair page shows the console's fingerprint. Lean: four rows of four groups, as
   the appliance's setup script shows it, so that the two are compared row by row.

The Planner's rulings on the Executor's plan of 2026-11-24:

1. The children and their order stand as planned.
2. **No box command changes.** `pair`, `request`, `adopt`, `setup describe` and
   `setup report` keep their arguments and their printed lines as on `main` at
   `4263a2db`. The appliance's setup script is being built against them in another seat.
3. Confirm and Directories read the latest report of either stage. The drives stage is not
   retired, and a drives-stage report that is sent is still read.
4. The Pair page names "the setup prompt at the appliance's first console login", with no
   command name. The name joins once the script is merged.
5. The Directories page shows what the box reports at the two default paths. Other paths
   need a new survey format in the suite repository, which is outside this act.
6. One box carries one setup. The replace-box write stays as it is.
7. The Spec and the design system move in this stack, so that pages, Spec and design
   system do not drift apart.
8. Issues 182 and 184: nothing is built. The address pane is carried onto the Pair page as
   it stands.
9. This act closes and edits no issue. Each pull request body says which issues its child
   answers. Closing them is the operator's.

## What the Executor reads first

- Issues 177 to 187, and the Executor's own plan of 2026-11-24.
- Spec 6.1, 6.2, 8.2, 8.3 and 9.
- `src/surfaces/setup.rs`, and `pair.rs`, `report.rs`, `confirm.rs` and `models.rs` under
  `src/surfaces/setup/`.
- The templates `setup_start.html`, `setup_pairing.html`, `setup_frame.html`,
  `setup_drives.html`, `setup_models.html`, `setup_confirm.html` and `instrument.html`.
- `docs/design/system/README.md`, `declare.md` and `stages.md`.

## The sub-acts and their order

The stack is `act/the-pairing-page-and-the-directories`. It starts from the merged head of
the sign-in switch stack (#195), `2e3ba95d8bfc3994f029a11e5120db0d05c77766`, and not from
`main`: this act's tests build `Policy` values, and the switch adds a member to `Policy`.
Its base pull request targets `main` and is opened once #195 is merged there.

| Child | Branch | What |
|---|---|---|
| 0 | `act/the-pairing-page-and-the-directories-0-spec` | Spec 6.1, 6.2, 8.2, 8.3 and 9: the Pair page, the declaration from directories, the appliance's setup script and the first console login, with the placeholders |
| 1 | `act/the-pairing-page-and-the-directories-1-pair` | Pair a box, under Provision: the agent is named there, the page says what to do at the appliance, and the setup's rail loses Pairing (#177, #178, #184 carried) |
| 2 | `act/the-pairing-page-and-the-directories-2-directories` | Directories as the declaration's first page, the held "setup command" pane gone, the need pane on the Models page, and Confirm standing on either stage's report (#180, #181 to the lean, #183, #187's page text) |
| 3 | `act/the-pairing-page-and-the-directories-3-design-system` | The design system's rows (#186) |

Children 0, 1 and 3 branch from the base. Child 2 sits on child 1.

## Acceptance

- Every child passes the gates: format, build, both clippy runs and the suite. A suite
  report gives the passed, failed and ignored counts and says that no test was skipped.
- Every changed behaviour has a test that fails without it, and the report says which
  tests were seen to fail with the change removed.
- **Child 1:**
  - The setup's rail and every declaration page show no Pairing step.
  - Provision carries a Pair page beside Register. An admin names the agent there and sees
    the box that dialled.
  - The Pair page holds the start page's rule for an agent's name,
    `store::setup::agent_name`, unchanged. A name the appliance could not take is refused
    there, as it is on the start page today.
  - The page says what to do at the appliance, and shows Weaver Web's address and port.
  - A pairing through the page, against a fake admin-con, writes the register row and the
    setup's link as on `main`. No migration is added.
  - The pairing stage's report no longer shows on the Pair page.
- **Child 2:**
  - The order is Directories, Models, Placement, Confirm, Build.
  - No page holds "the image's setup script" or "first boot", and the held "setup command"
    pane is gone.
  - A setup that holds a pairing-stage report alone reaches Confirm with the box's members
    shown. A drives-stage report that arrives later is read.
  - The Models page shows the models directory's need.
- **The box commands are untouched:** `git diff main` over `src/link/box_commands.rs`,
  `box_line.rs`, `frames.rs`, `key_config.rs` and `pair_code.rs` is empty at every child's
  head.
- **The pages as text.** The report carries the rendered text of the Pair page and the
  Directories page, from a test's fixture, so that the operator can read what a person
  sees.
- Spec verdicts are given per section, with the ruling each rests on.
- No diff, body or commit message holds a box path, a host name, an account or a
  credential.

## Scope boundary

About 32 to 37 files, against a ceiling of 100. Draft pull requests only. Nothing is
pushed without the Planner's word naming the head.

Not in this act: the model's way onto the appliance, sign-in or a sign-in page, any change
to the survey's format, and anything on the appliance's side.

## Addendum, 2026-11-24: a small fix picked up as child 4

CodeRabbit's review of #195 (2026-11-24) found that the last loop of
`link::client_tests::turns_past_the_in_flight_bound_wait_in_arrival_order` awaits each
close with no deadline. A stalled reply would hang the suite and fail nothing.

| Child | Branch | What |
|---|---|---|
| 4 | `act/the-pairing-page-and-the-directories-4-close-deadline` | The test's final close waits get a deadline, in `src/link/client_tests.rs` alone |

Child 4 branches from the base.

Acceptance:

- The per-request text assertions and the bound's assertion stay as they are.
- With the last two permits withheld in the worktree, the test fails within its deadline
  and does not hang. The report says how long it took.
- The test passes 20 of 20 alone, and the suite passes at 8 threads.

## Addendum 2, 2026-11-24: the review round

The Planner's review of children 1 and 2 found seven points, and the Executor's report
four more. They land as three more children and a second commit on child 0.

| Child | Branch | What |
|---|---|---|
| 5 | `act/the-pairing-page-and-the-directories-5-review` | On child 2: one report for the whole tuple, the machine's kind reachable after pairing, the margin's warning in measured words, the declaration closed until its box is paired, and the tests the review found missing |
| 6 | `act/the-pairing-page-and-the-directories-6-waiting-bound` | On the base: the waiting-bound test enqueues in sequence, in `src/link/client_tests.rs` alone |
| 7 | `act/the-pairing-page-and-the-directories-7-prd` | On the base: the PRD's lines follow the Pair page and the directories |

Planner rulings:

- The cards Placement offers and Confirm checks come from the latest report of either
  stage, as the box's members do.
- After pairing, the operator sees and changes "virtual machine or host" from the
  declaration.
- The margin's warning states the directory's size, the picks' need and the room left.
- A declaration page of an unpaired setup leads to its Pair page, and a write is refused.
- In the Pair page's visible text the server is "Weaver Web", and "console" is the
  appliance's alone.

Acceptance:

- A pairing-stage report that arrives after a drives-stage report is the one Directories,
  Models, Placement and Confirm read, each shown by a test.
- Confirm's post on a pairing-stage report alone writes a tuple, and a test reads it.
- A test reads the waiting-box table on the Pair page.
- `git diff main` stays empty over the five box-command files at every head.

## Addendum 3, 2026-11-24: the second review

The Planner's review of child 5 found eight points. They land as one more commit on
child 5 and one more on child 0.

Planner rulings:

- Where the placement page offers no card, its save leaves the kept devices as stored.
- A confirmed setup's Pair page shows the machine's kind as text, and a post of it is
  refused as a confirmed setup's other writes are.
- The margin's warning states the shortfall in a unit that shows it.
- A fragment of an unpaired setup's declaration is refused as its writes are.
- The answers to the Pair page's forms call the server "Weaver Web".

Acceptance:

- With two reports that both read and differ in cards, the pairing one the later,
  Placement offers the later report's cards, and Confirm's post is refused naming a kept
  device the later report lacks.
- A placement saved while the latest report does not read keeps the devices stored
  before it.

## Addendum 4, 2026-11-24: the operator's rulings, and the requirements page

The operator ruled on 2026-11-24, in the operator's words:

- On point 1, where the report shows (#181): "After a pairing succeeds, the Pair page
  shows the hardware report, read only. The next page carries it into the declaration.
  This replaces my ruling of 2026-11-20."
- On point 2, how the declaration begins: "A. The Pair page leads straight into the next
  page for that agent."
- On point 3, the margin: "C. The 10 GB margin is guidance only, stated on the
  requirements page. No page warns."
- On point 4, Confirm's "image version" reading "the appliance's version": "Yes."
- On point 5, the fingerprint in four rows of four groups: "Yes."
- On the Pair page's checkbox, raised in the second review: "A. The checkbox starts
  checked."
- A new page: "'Create new agent' opens a requirements page first. It covers VMs only at
  this stage. No host install is offered. One note: GB10 Spark-class devices are hardware
  we want to deploy to and do not have yet."
- "Rebuild the layout stack to these. Then it leaves draft."

**No placeholder stands after this addendum.** Each of the Spec's five markers gives way
to the ruling, cited by its date. The PRD's sentences that call these points leans say
what was ruled.

What changes:

1. **The report moves to the Pair page.** Once the pairing has succeeded, the agent's
   Pair page shows the pairing stage's report in a pane that holds no form. Before a
   report arrives, the pane says that none has arrived and what sends it. A report that
   is refused shows its cause, as the pane does today.
2. **The Directories page no longer shows the report in full.** It keeps "The directories
   as mounted", read from the latest report of either stage as now. This is the Planner's
   reading of "The next page carries it into the declaration", and the operator is asked.
3. **No page judges the margin.** No judgment, warning, footer or held state names the
   10 GB margin, on any page. A models directory short of the picks' need is judged as
   now. The requirements page states the margin, as guidance.
4. **The checkbox starts checked** for a new setup. A migration that is on `main` is not
   edited.
5. **Points 2, 4 and 5 are built already.** Only their markers go.
6. **The requirements page is new,** as below.

The requirements page:

- **The entry reads "Create new agent".** It stands on Pair a box, in the place of the
  name's field and of "New agent". It opens the requirements page.
- **The page ends in the name's field and Begin,** which lead to the agent's Pair page as
  now. No other way names an agent.
- **The page states only what a ruling or the Spec already states.** It invents no
  number. In this order:
  - an agent runs in a VM made from the appliance, and this version covers VMs only;
  - "One agent per VM is the recommendation.";
  - the operator's part, in the operator's words of 2026-11-23: "The operator holds the
    hypervisor, the cards, the disks and the network between VMs.";
  - the two disks, in the words of the ruling quoted in this brief, with the 10 GB margin
    stated as guidance and never judged;
  - the VM must reach Weaver Web's address and port, which the agent's Pair page shows;
  - one note, in the operator's words: "GB10 Spark-class devices are hardware we want to
    deploy to and do not have yet."
- **The page names no CPU requirement.** The appliance carries its own VM settings.
- **The page offers no host install,** and no page offers one.
- The page is read by the same grant as Pair a box, and it holds no form but the name's.

Where the entry stands, that the name's field sits at the page's end, and what the page
leaves out are the Planner's readings. The operator is asked, and the page is built to
them meanwhile.

Not in this addendum:

- The name of the page "Pair a box". The operator is asked, and nothing is built for it.
- Whether pairing comes before the disks. The operator is asked. The sentences that say
  the operator lays the disks before pairing stand.
- The Pair page's checkbox itself. It stays, since the ruling was that it starts checked.

Acceptance:

- A new setup's checkbox reads checked, and a setup saved unchecked stays unchecked.
- A paired setup's Pair page shows the report pane in each of three states: none yet,
  read, and refused. An unpaired setup's Pair page shows no report pane.
- A models directory whose filesystem is short of the margin, and holds the picks' need,
  draws no warning on any page.
- The requirements page is reached from "Create new agent" alone, and an agent is named
  from it alone. A post of a name is refused or taken exactly as before.
- No command on the appliance's side changes, as ruling 2 of this brief says.
- Every changed behaviour has a test that was seen red with the change removed.
