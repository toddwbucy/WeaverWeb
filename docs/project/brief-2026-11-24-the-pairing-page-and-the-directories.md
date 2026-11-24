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
