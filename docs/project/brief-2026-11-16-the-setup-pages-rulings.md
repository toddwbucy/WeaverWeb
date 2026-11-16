# Brief: the setup pages' rulings in the standing texts (2026-11-16)

From the Planner, 2026-11-16, on the operator's rulings of 2026-11-16 (setup pages), as
revised the same day (revision 2, which withdrew the console's SSH setup window) and
with the operator's addendum in chat the same day, which settled the pairing. A
docs-only act from `main` at `f8de838c`, opened as draft sub-pull-requests on the
Planner's word. No code.

## The rulings it rests on

The operator's prompt of 2026-11-16 answers the return on the four setup pages and its
sixteen conflicts. Every change this act makes cites it as "the operator's rulings of
2026-11-16 (setup pages)". Where the prompt and a standing text disagree, the prompt
wins and the text is corrected in this act, so no document carries a contradiction. What
the prompt rules, in the order it gives:

1. **The finish line.** The act that builds the pages ends at a confirmed deployment
   tuple, the confirm page. The board and the prompt tuple panel are held and come next.
   The first agent can be declared and set up through Confirm, and a load refuses it
   until the prompt tuple carries the six sampling values. The build page stays a
   one-pane stub marked unbuilt.
2. **Hugging Face, before the first agent.** Models come from Hugging Face alone, and a
   local source is later work. The console connects the operator's Hugging Face
   account once, as a console setting, before the start page. The setting accepts a
   read-only or a read-write token, and the token lives in the console's store. The
   console never writes except on an explicit push in Train. A pull records the
   repository, the commit, every file's SHA-256 and size (one per part for a split
   model), the account that pulled and the time. Downloads use a link through the commit
   and never a branch name, and the console hashes after download and compares. The
   pulled model is kept in the console's cache inside the Weaver Web install and reused
   from there. The first agent's decoder is the BF16 GGUF of
   `ibm-granite/granite-4.2-30b-GGUF`, a two-part file.
3. **Setup uses no SSH, and the box dials Weaver Web**, for pairing and always after.
   The operator's SSH door is the operator's administrative way into the machine,
   separate from Weaver Web, and the console never uses it. An appliance's console goes
   straight into the appliance's setup script at first boot, run with root's authority,
   and the script asks for Weaver Web's address and port. The box dials Weaver Web over
   TLS and shows a short one-time code and Weaver Web's fingerprint. In Weaver Web,
   after the start page and the agent's name, the waiting box is shown, and the operator
   compares the fingerprint and enters the code. The connectors send the public halves
   of keys made in the box, the console signs them, and mutual TLS comes up. The box's
   survey runs locally and its report crosses over TLS. The operator picks the model,
   and the download starts in the background. Weaver Web then shows a command to run at
   the appliance's console, which starts the interactive drive setup: the state drive
   and the models drive, sized from the model, each shown and confirmed before it is
   formatted and mounted. The box pulls the model from the console's cache over TLS and
   checks its digests. On a virtual machine the first setup cannot forgo the hypervisor,
   where the operator attaches the drives. The connectors dial Weaver Web on every boot
   and redial a dropped link, and load and unload do not drop them. **The pairing step
   belongs with the connectors and serves a host too. The drive setup is the appliance's
   script alone**, and a host's own script is later. A host install points at a state
   directory and a models directory and attaches and formats nothing, and its form of
   the drives step is drawn and marked unbuilt. Every root act runs on the box at a
   screen the operator is sitting at, so admin's grant does not grow. From pairing
   onward the console's whole reach is its connectors, so `weaver-web-PRD` section 10.5
   holds from the first minute. The setup's first question sets root's password, and the
   operator's SSH public key reaches the box through Weaver Web over TLS after pairing,
   so Weaver Web holds that public key and sends it down the paired link.
4. **The pages.** The start page asks the agent's name. The setup pages follow in this
   order: the pairing (the code, the fingerprint and the survey's report), the models,
   the drives as the box mounted them, the placement, and the confirmed tuple. A
   checkbox says whether the box is a virtual machine or a host, the console's shutdown
   command enabled on a virtual machine and disabled entirely on a host, and the page
   warns where the address is localhost and the checkbox says virtual machine. The state
   mount is required for every agent, at a minimum of 10 GB, and the models mount takes
   a plural name. One unit, GiB, across the whole flow.
5. **Labels.** Every pane carries its stage, built or unbuilt, and its status, healthy
   states included.
6. **Save points.** The first agent keeps the save point the unload writes, on the
   state disk, and the first test exercises it. Operator-managed save points stay out of
   scope.
7. **The order of work.** This act first, then the asks on WeaverAgent and WeaverTools,
   then the Hugging Face connection and the model cache, then the pages, then the first
   test.

No location, mount, account or address of an appliance enters this repository
(`weaver-web-PRD` section 10.7), so the documents name the models mount and the state
mount and never their paths.

## The act

1. **`weaver-web-PRD`.** Section 2's "Not a hypervisor driver", with the console's
   shutdown command on a virtual machine and disabled on a host. Section 3.0's Declare
   and Provision rows, its account of Declare, and the pane's labels. Section 8's asks.
   Section 9's cells on state and save points, the trace path on the state mount, the
   Spark-class machine, trust on first use, the one-time secret and the fetch. Section
   10's opening, 10.1 (pairing by a short one-time code the box shows and the operator
   enters in Weaver Web, in place of a line run over SSH, the pairing the connectors'
   and serving a host too), 10.2, 10.4 (Hugging Face alone, local and other sources
   later, the pull record, links through the commit, the cache inside the install, the
   state mount for every agent, the drives formatted and mounted by the appliance's
   setup script) and 10.5 (the two connectors' reach holds from the first minute with no
   exception, the unload's save point on the state disk). The first agent's "keeps no
   state" becomes "declares a store and keeps the system prompt".
2. **`weaver-web-Spec`.** Section 6.1's account of Declare, section 8.2 (an appliance's
   enrollment, by the code the box shows, the pairing the connectors' and serving a host
   too), section 8.3 and the cells of section 10 the rulings move, the lines at 6101 and
   6787 among them.
3. **The Spec's own elections.** The Spec's lines that cite the Planner as an authority
   become the Spec's own elections, citing the operator and the date where the election
   was the operator's ruling (answer 13 of the operator's prompt of 2026-11-16).
4. **The design system under `docs/design/system/`.** Declare is a start page (the
   agent's name) and the setup pages (pairing, models, drives, placement, confirm)
   ending at the build page (the board and the prompt tuple panel, held). Provision's
   row and `stages.md` section 2 follow the settled flow. Labels on every pane, status
   included, healthy states too.
5. **`docs/project/brief-2026-11-15-the-console-side-of-the-appliance.md` line 37.**

## Out of scope

- The Hugging Face connection, the model cache and the pages themselves. Each is planned
  first, in the hub, before any code.
- The asks on WeaverAgent and WeaverTools. None is filed from this seat.
- The parked branches of the trace's destination field, which are not deleted, rebased
  or pushed.
- Any text the rulings supersede without naming it where it would contradict a standing
  charter. The report lists each, and none is resolved here.

## Acceptance

- Every change cites the operator's rulings of 2026-11-16 (setup pages), or for the
  Spec's own elections the operator's ruling it rests on, by date.
- The editorial checks pass on the added lines: ASCII, 88 columns outside table rows and
  fences, no semicolon outside a table row, no relative time word, no pronoun for the
  operator, no double dash.
- No box path, host name, account, address, mount path or credential in any line.
