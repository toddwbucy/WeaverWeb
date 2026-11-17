# Brief: the start page and the setup pages

**Date:** 2026-11-16. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the operator's rulings of 2026-11-16 (setup pages), sections 4 and 5, and
items 1 to 20 of their addendum with the Planner's notes after item 17, and the
Planner's decisions of 2026-11-16 on the Executor's plan. Act 3 of the three the rulings
lift the hold for. Act 1 (#120) carries the rulings into the standing texts, and Act 2
(#125) builds the Hugging Face connection and the model cache this act drives.

## Goal

Declare's new path: the start page (the agent's name), then the setup pages in the
settled order, pairing, models, drives, placement and confirm, and the build page as a
one-pane stub. Every pane carries its stage, its backing and its status, healthy states
included. A pane that waits on another side is drawn held, says what it waits on, and
shows no value it did not read.

What Act 3 lights: the start page, the models page through Act 2's client, puller and
cache, the placement page's context length read from each file's header, and the
pairing page's checkbox with its localhost warning. What it draws held: the pairing
page's code and fingerprint, the survey's report, the drives as the box mounted them,
the placement's devices and its planned fit, and the box's members of the deployment
tuple. So Act 3 alone reaches no confirmed tuple. **The rulings' first test** (download
fresh, set up a virtual machine through the pages, confirm the tuple) waits on Act 4,
the enrollment in this repository, and on the image's setup-script act.

## The decisions of 2026-11-16

1. **The order:** Act 3 lights what exists and draws the rest held. The enrollment is
   Act 4, in this repository, and lights the pairing page.
2. **The operator's SSH public key is a console setting**, at `GET /admin/ssh-key`, one
   or more public keys, an admin's write, audited, held in clear as a public half. The
   drives page shows the key that goes down by its fingerprint, with a link to change
   it. The console checks the line's form alone.
3. **The declaration screen stays as built**, reached from the build page's stub for the
   fields no setup page asks. Its saved rows are untouched. Declare's entry in the nav
   goes to the start page.
4. **The setup's record is a mutable setup row until Confirm**, then one immutable
   deployment tuple row. The pages write no declaration row, and the document's grammar
   is not extended.
5. **The precision and the maximum context are read by a bounded range read of each
   file's header** before download, by a GGUF reader of the console's own, and the Spec
   states the bound.
6. **The planned fit comes from the box's room plan**, which runs on the box without a
   load and crosses over TLS as the survey's report does. The placement page draws it as
   "planned, from the box's room plan", held. The Planner puts the room plan to the
   WeaverAgent seat as an ask.
7. **The localhost warning reads the console's own link address** in Act 3, and the
   dialed address too once Act 4 brings it.

## The sub-acts, in order

1. `act/the-setup-pages-1-spec`, docs only: Spec 6.1 recut (the routes, the setup's
   record, each page's live and held parts, the declaration screen reached from the
   build stub, Declare's nav to the start page), 2.4 (the setup row and the immutable
   tuple beside the declaration's lineage), 2.13 (the setup's writes in the admin
   writes' order), 8.3 (the header read by range before download, and its bound), the
   claims in 9 and the cells in 10.
2. `act/the-setup-pages-2-the-frame`, code: the setup row (migration `0026`, audit kind
   `setup`), the start page, the pairing and drives pages drawn, the build stub, the SSH
   key setting (migration `0027`) and the nav's Declare entry.
3. `act/the-setup-pages-3-the-models-page`, code: the GGUF reader (`src/gguf.rs`), the
   client's bounded header read by the commit, and the models page, its pick asking
   `Puller::ask` in the admin writes' order.
4. `act/the-setup-pages-4-placement-and-confirm`, code: the context lengths, Confirm,
   and the deployment tuple (migration `0028`, immutable by trigger).

The code sub-acts build on Act 2's `Client`, `Puller` and `Cache`, so they start from
`main` once Act 2's stack merges. The Spec sub-act may go first.

## Acceptance

- Each code sub-act passes `cargo fmt --all -- --check`, `cargo build --locked`, `cargo
  clippy --all-targets --locked -- -D warnings` with and without
  `--no-default-features`, and `cargo test --locked` with `DATABASE_URL` and `PGUSER`
  set, the report naming the count passed and that nothing was skipped.
- One structural test walks every page's rendered panes: each carries a stage, a backing
  and a status word, and a held pane names what it waits on with no input holding a
  value.
- The GGUF reader refuses hostile headers without a panic or an unbounded allocation: a
  string length past the buffer, an array count past it, a truncated header, and more
  than the bound.
- A crafted post naming a second model, a second adapter role or the CPU is refused,
  saving nothing.
- The tuple refuses any update or delete, and a second Confirm of a setup is refused.
- The tests run against Act 2's fake Hugging Face. No test reaches Hugging Face.

## Scope boundary

No enrollment, no pairing code or fingerprint exchange, no frame on the link (Act 4 and
the other side). No live survey report, drives as mounted, device list or planned fit.
No build page beyond its stub. No host form made live. No Provision rendering of a
loadable declaration. No path, account, group or mode of any box. The sub-acts open as
drafts against the stack and are pushed on the Planner's word.

## Open and not ruled here

- The agent account's name and home on an appliance (Act 1's text waits on the
  operator's answer).
- How the start page's name crosses to the box.
- Which source wins when Provision renders, the declaration screen's artifact text or
  the models page's repository, commit and digests.
