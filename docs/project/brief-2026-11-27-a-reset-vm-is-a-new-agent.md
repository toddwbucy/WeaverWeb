# Brief: a reset VM is a new agent

**Date:** 2026-11-27. **From:** the WeaverWeb Executor, on the Planner's word of 2026-11-27, from the Executor's plan of the same day for taking replace box out, which the Planner accepted with eight answers. **Stack:** `act/a-reset-vm-is-a-new-agent`, cut from `main` at `24ac7a3bee09a842a303c9df804ecfe7ab386943`.

## The goal

No write moves an agent's row to another box any more. A reset VM that pairs with any Weaver Web is a new agent: a `pair`, a new row and a new `ag-` identity. The replace-box write leaves the code, the pages, the frames and the Spec. The rows it wrote before stay as history, readable and never added to. The state disk's origin is to be recorded as a fact, and no page says what the fact means. Carrying the origin waits on the appliance's half.

## The operator's words that bind it

On 2026-11-27, the ruling:

> "Ruled, 9 (d) B: a reset VM that pairs with any Weaver Web is a new agent, whoever the operator is. Remove "replace box". Record the state disk's origin as provenance. The sender's record ends where the VM left them, and the receiver's begins where they got it. A reset kept local follows the same rule. The suite records the fact and does not tell the operator what it means."

On 2026-11-27, on the reset that comes before such a pairing, which the appliance's setup script will get:

> "Reset is run by the sender, before a VM leaves. The use is moving an agent to another hypervisor, or sharing the VM with a trusted person or organization. The operator clones the VM, runs weaver-setup --reset, and then ships it. The reset drops the network settings and the drive assignment, and it tears down the pairing: the admincon certificate and the mutual TLS with this Weaver Web. That link is one to one and must never travel with the VM. The loop in the agent and whatever is on the drives stay. The receiver runs the normal first setup, the same as on a fresh appliance."

On 2026-11-24, when a review asked what happens where a replacement appliance carries a state disk:

> "that is a new agent with a state disk from a different agent. different agent and the state file while orignating from a different agent that is the provanance of what is fundamentally a new agent."

## The Planner's answers to the plan (2026-11-27)

1. **A new image for an agent that stands is a new agent**, on the operator's words of 2026-11-24 above. Section 10's closed election, that such an image "takes the replace-box write", is closed the other way, and the Spec says so with both dates. An upgrade in place that keeps the root disk is no pairing at all, and is not this act's.
2. **Rotation of an enrolled appliance** stays an open election, with "a path of its own" as its one candidate.
3. **The `replacement` member of `setup describe` is dropped now.** Both ends of the frame live in this repository, nothing is released, and the server and the connectors are installed together.
4. **The schema:** a new migration with three refusals. The rows already written are kept, and `agent_box` is closed and kept. No count of an installed store's rows is taken.
5. **`Store::boxes_held` goes**, with its test. No page reads it.
6. **The reset does not dial the sender's Weaver Web.** The Spec states it as a fact: the sender's Weaver Web learns nothing from a reset, and its row ends only at the admin's retire.
7. **A reset kept local** pairs as a new row, and a name a live row holds is refused `box_and_name_live`, as today. The Spec says so in one sentence.
8. **The origin waits.** What the origin holds, whether the reset writes a record on the state disk, and what the sender lets travel are the operator's and the other repository's. Child 2 names the gap and fills nothing. The label's wording is asked of the operator before child 4.

## Already ruled out

- **Editing a migration that has run.** `migrations/0030_the_enrollment.sql` stays as it is. The change is a new migration.
- **Deleting history.** Every audit record whose action is `replace box`, every `agent_setup` row with `paired_as = 'replace box'`, and every `agent_box` row stays.
- **A link between rows.** Nothing on the receiver's side links its new row to the sender's. The origin is a fact about the disk, not a link between rows.
- **A word on what the origin means.** The page shows the fact with a label, and no sentence such as "inherited", "continues" or a warning.
- **Dialing the sender at a reset.** Answer 6.
- **A machine path, host name, account or address of any box** in the repository, the pull request bodies or the reports' text that reaches them.

## Where replace box lives on `main`

**Code.**

| Site | What it is |
| --- | --- |
| `src/link/register.rs` | `PairingAct::Replace(AgentId)` and its action `"replace box"`; `PairingRefusal::Retired`; the `Replaced` struct; the `Replace` arm of `pairing_stands`; the `Some(replaced)` arm of `pairing_write_on` (the `agent_box` insert, the update of `agent.box` that revokes both planes and sets admin-con's new fingerprint, and the notification of the revoked fingerprints); `Store::boxes_held`; the doc of `live_rows_named`. |
| `src/link/exchange.rs` | The conforms header `web-a-rows-box-changes-only-through-the-replace-box-write`, step 6's doc, and the close of the revoked fingerprints' connections after the write. |
| `src/surfaces/setup/pair.rs` | The form's `replace` member, its parse and resolution, `Matched.replace`, and `PairingAct::Replace(id)`; the module doc's "`pair` or `replace box`". |
| `src/surfaces/templates/setup_pairing.html` | The fieldset "A live row holds this agent's name", shown only on the Pair page of a setup begun before New agent. |
| `src/link/frames.rs` | `Described.replacement`, with no `serde(default)`. |
| `src/store/setup_ask.rs` | The conforms header `web-setup-describe-answers-replacement-true-after-a-replace-box-write-alone`; `replacement` read from `paired_as`; the module doc. |
| `src/store/setup.rs` | The doc of the setup's link ("`pair` or `replace box`"). |

**Schema.** `migrations/0030_the_enrollment.sql`: `agent_setup.paired_as` with its `CHECK (paired_as IN ('pair', 'replace box'))`; the table `agent_box` and its two triggers; the trigger `agent_box_changes_only_beside_its_history` on `agent`.

**Tests.** In `src/enrollment_tests.rs`: `on_unknown_the_pairing_page_shows_what_the_write_made` (driven by a committed `replace box`), `a_replacement_keeps_the_box_the_row_held`, and `setup_describe_answers_replacement_after_replace_box_alone`. In `src/link/tests.rs`: the "box moves outside the replace-box write" case of `at_most_one_live_row_per_box_and_name`, and the doc of `an_admission_on_a_plane_set_to_none_is_refused_not_live`. Other hits of "replacement" in `listener.rs`, `verbs.rs` and `verbs_audit_tests.rs` are a re-registration's retire-and-replace or a replacement listener, and stay.

**The Spec.** Section 2.12 (a row's box changes only through the replace-box write, and its history); 2.13 (`pair` and `replace box` as audited writes); 6.2 and the stage-1 passages; 8.2 (the passage electing replace box, the unknown answer, the re-pairing); 8.3 (`setup describe`'s `replacement`); three rows of section 9; and section 10's open election on rotation and its closed election on a new image.

## The children, in order

Each child carries its own Spec sections and section 9 rows, passes the repository's gates alone, and measures a perturbation for each claim it adds.

1. **This brief.**
2. **The Spec.**
   - Take replace box out of sections 2.12, 2.13, 6.2, 8.2, 8.3, 9 and 10, superseded text removed.
   - Write the ruling into 8.2 as the operator's words of 2026-11-27, and the words of 2026-11-24 into section 10's closed election, closed the other way with both dates.
   - **The two records** as facts. The sender's: a reset leaves the sender's Weaver Web no record and no frame, and its row ends only at the admin's `retire`. The receiver's: the record begins at the `pair` first record, the new register row, the setup row with `paired_as = 'pair'` and the pairing report landed in the same transaction. One sentence for a reset kept local: a new row, and a name a live row holds is refused `box_and_name_live`.
   - **The origin, a Spec gap named and not filled**: what is recorded today (the pairing report, `weaver-survey/2`, which says nothing of the disk's origin), and that the rest waits on the appliance's half.
   - Rotation's election stays open with one candidate.
3. **The removal, code and schema.**
   - Remove what the table above lists, `boxes_held` and its test among it, and the `replacement` member of `Described`.
   - **The new migration**, the next free number on `main` when this child is built, renumbered at the merge where another act's migration lands first. It refuses an insert or update of `agent_setup` that sets `paired_as` to `replace box`, by trigger, so the rows already written stay valid; it refuses an insert into `agent_box`; and it replaces the box trigger on `agent` with one that refuses any update of `agent.box`.
   - **The tests:** a pre-New-agent Pair page offers no replacement; a pairing whose name a live row holds is refused `box_and_name_live` and writes nothing; the schema refuses a new `replace box`, an `agent_box` insert and any move of a box, while rows written before stay readable; `setup describe` carries no `replacement`; the unknown-answer test is rewritten over a `pair`.
   - A perturbation for each: drop each trigger's refusal and its case lands; offer the fieldset again and the page test finds it.
4. **The origin, carried and shown.** Waits until the other repository's format change has merged, and the label's wording has been asked of the operator.
   - Read the new member, land it with the pairing in the pairing's own transaction, never edited, and show it on the Pair page's report and on the agent's page as the fact alone.
   - A test that the shown text is the member's value under its label and nothing else.

Children 2 and 3 can go up before the appliance's half is settled. Child 4 waits on it.

## What a row a replace-box write made becomes

- **The agent row:** its `ag-`, its last box and its live credentials stay. It can no longer be moved.
- **Its setup rows:** each `paired_as` stays as written. `setup describe` answers from the row's latest setup, as today.
- **Its `agent_box` rows:** kept and readable, never added to. No page reads them.
- **Its audit records:** unchanged.

## For the other repository (carried by the Planner)

This repository cannot answer these. They are listed so the Spec's gap names them:

1. What the origin is: for example whether the state disk was kept or formatted at this setup, the folder's agent name, and a record the reset writes on the disk.
2. Whether the reset writes such a record at all.
3. Whether `weaver-survey` moves to a third version, with a member under the state drive, or the origin travels as a report stage of its own. The format's owner moves first.
4. A filesystem's identity survives a clone, so it cannot be the origin's identity alone.
5. A clone holds the sender's admin-con key until the reset runs, so the reset's documents say the clone is reset before its network comes up.

## The acceptance

- No code path, page, frame or test writes or offers `replace box`, and the schema refuses one.
- The rows written before stay readable, and every audit record stands.
- The gates pass alone at each child's head, with the suite's count and nothing skipped.
- The Spec carries the operator's words of 2026-11-24 and 2026-11-27 as quoted here, the two records, and the origin's gap.

## The scope boundary

- The origin's content and the label's wording: the operator's and the other repository's. Child 4 builds only on their merged text and the operator's word.
- Rotation of an enrolled appliance: open, not this act's.
- An upgrade in place that keeps the root disk: no pairing, not this act's.
- `CLAUDE.md`: the Planner's.
