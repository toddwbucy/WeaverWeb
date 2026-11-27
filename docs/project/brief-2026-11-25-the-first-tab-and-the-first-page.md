# Brief: the first tab and the first page

Written by the WeaverWeb Executor on 2026-11-25, from the plan the Planner accepted that day at 19:50 UTC, with the Planner's answers and eight points folded in. It lands as the first commit of `act/the-first-tab`, on the merged base of `act/the-appliance-reaches-out`.

## Goal

After this act, the first page under Agents is where an appliance becomes an agent. With no click before it, the page holds:
- the requirements;
- the steps at the appliance;
- Weaver Web's address on the management plane and its fingerprint;
- the appliances awaiting final initialization.

The operator picks theirs, names the agent, enters the code the SSH session shows, and finalizes the pairing in one form. The agent's own Pair page stays: it shows the survey's report and leads into the declaration.

## The operator's words that bind it, 2026-11-25

- "A. Eight stages stay. New agent is the first page of stage 1." The tab order is free.
- His yes of 15:40 UTC: "the first page of Weaver Web lists the appliances waiting, and the operator clicks theirs, names the agent and enters the code".
- The script: "One script, run by command over SSH. No auto-run at first login." Its last steps: "Ask for Weaver Web's address on the management plane." "Dial Weaver Web. Weaver Web lists the appliance as awaiting final initialization. The SSH session shows the code." "The operator pastes the code into Weaver Web, picks the appliance, names the agent, and finalizes the pairing."
- The network's premise: "this is not meant to be deployed on a network you don't trust. This is meant to be deployed on a trusted network, the same place you would deploy a hypervisor, for instance."
- At the Pair page: "when we create an agetn we get all the verbage that should have been on the first tab instead of the second after a click through". He said yes to: "The first tab holds the requirements, then the name and Begin, with no click before them."
- Rulings of 2026-11-24 that stand: "After a pairing succeeds, the Pair page shows the hardware report, read only. The next page carries it into the declaration." "A. The Pair page leads straight into the next page for that agent." "A. The checkbox starts checked."
- 72 (w) is held: the fingerprint's comparison and its tick box stay.

## The children, in order

1. **The tab's order.** The admin's Agents tab opens `/pair`, and New agent is the first sub-tab. New agent carries the mark `1 Declare`, as the first page of stage 1. No stage is renumbered; the rail keeps "of 8".
2. **The bare address.** `act/the-first-tab-3-the-bare-address` as built (`15a7b23940bee29ff2d30ce2bfa09e5147536c4b`), merged up, not rebuilt. The bare address and every sign-in send a session to `/pair` or `/agents`.
3. **The first page.** It comes after this brief is read, on the Planner's word.

## The first page, as the operator reads it, top to bottom

**New agent**

This page pairs an appliance with Weaver Web and names its agent.

Weaver Web and its appliances are not meant to be deployed on a network you do not trust. They are meant for a trusted network, the same place you would deploy a hypervisor.

**What an agent needs**
- An agent runs in a VM made from the appliance. This version covers VMs only. One agent per VM is the recommendation.
- The operator holds the hypervisor, the cards, the disks and the network between VMs.
- The appliance comes with no state disk and no models disk. It comes with itself.
- The operator adds both in the hypervisor, sized to their need: a models disk at least 10 GB larger than the model, and a state disk of at least 10 GiB. The 10 GB margin is guidance. No page judges it.
- The setup script lists the drives. The operator picks the models drive and the state drive. Each mount path is preset to our default, and the operator may type another. Where a disk already holds data, the script asks whether to keep it or erase it.
- GB10 Spark-class devices are hardware we want to deploy to and do not have yet.

**At the appliance**
1. Start the appliance with its two disks attached, and connect to it over SSH as the appliance's starting user. Its first password is changed at the first login. (See answer 9 for what this line may name.)
2. Run `sudo weaver-setup`. The command stands in one place in the templates.
3. The script prepares and mounts the two disks, marks each card for a plane, and asks for Weaver Web's address on the management plane.
4. Give it this address: <the management door's address and port>. Both planes share one door. (The last sentence shows only where one door serves both planes.)
5. The script asks for a name for the appliance, then dials Weaver Web. The appliance is listed below under the name you gave it, and the SSH session shows Weaver Web's fingerprint and a code.

**Weaver Web's fingerprint**

<the authority's fingerprint, in four rows of four groups>

[ ] The fingerprint the appliance shows is Weaver Web's, above, row by row and group for group.

**Appliances awaiting final initialization**, each by the name typed in its setup script

( ) appliance-a, from <peer address>, waiting 2 min

( ) appliance-b, from <peer address>, waiting 40 s

Where none waits: "No appliance is waiting. Run the setup script on the appliance; it is listed here once it dials."

[Refresh the list]

**Name the agent:** [ ] (a lowercase letter, then lowercase letters and digits, 2 to the most characters in all; the appliance names the agent's accounts, its directories and its admin entry after it)

**Code from the SSH session:** [ ]

[Finalize the pairing]

Once paired, the page goes to the agent's Pair page. That page shows the survey's report at the pairing stage, read only, and leads straight into the next page for that agent.

## The eight points, settled

1. **A miss writes nothing.**
   - The post makes no setup until the code matches.
   - A matched code runs the exchange, whose write makes the setup with the row and the reports in one identity transaction.
   - A miss, or any refusal, writes no setup and no row. The page comes back with the name and the chosen appliance still in their fields.
   - Spec 2.13's order holds:
     1. parse;
     2. the admin gate;
     3. resolve: the name checked free of a live setup and of a declaration's hold, and the handle resolved to a waiting connection;
     4. the audited write.
   - The name is checked again under the exclusion inside the write.
   - So no named agent stands with no appliance, and the next try with the same name is a fresh post. No list of setups that stand without an appliance is needed.
   - The setup's `code_misses` and `code_closed_until` (migration `0030`) stay unused. Their Spec row says why.
2. **Replace box needs a design choice, and nothing of it is built.**
   - Today the code form offers "Replace the box of ..." where a live row holds the agent's name.
   - On the first page the name is typed in the same form as the code, so which rows a name holds is not known when the form is drawn.
   - Bringing the choice along needs one of two designs, the operator's to choose:
     - (a) the first page lists every live row as a replace target;
     - (b) a post naming a held name comes back once, offering the replacement.
   - Until then, the first page pairs as a new row. A box and name that hold a live row are refused `box_and_name_live`, as today, with "retire it on the register".
   - The replace-box code (`replace` in `src/surfaces/setup/pair.rs`, the exchange's `replace box` write) stands untouched, reached by no page.
3. **Virtual machine or host.**
   - The checkbox is not on the first page. A pairing from the first page makes its setup as a virtual machine, which is the checkbox's ruled start ("A. The checkbox starts checked") and the requirements' "This version covers VMs only".
   - The loopback warning reads the setup's kind. On the first page that is always a virtual machine.
   - The agent's Pair page keeps showing the kind as information.
4. **Each outcome of Finalize the pairing:**
   - **Paired:** the page goes to the agent's Pair page, which shows the report.
   - **A miss:** "That code does not match the appliance you chose. 2 tries are left on it." The name and choice stay.
   - **The third miss:** "That code missed three times, so the appliance's connection is closed. Run the setup script again to dial anew."
   - **The code expired** (the connection's ten minutes passed): "The appliance's wait ended. Run the setup script again to dial anew."
   - **The chosen appliance is gone before the post:** "The appliance you chose is no longer waiting. Refresh the list."
   - **The name refused or taken:** the name rule's sentence, or "An agent named <name> is being set up already" / "A declaration holds this name", as today's refusals word them.
   - **The tick box not ticked:** "Compare Weaver Web's fingerprint with the one the appliance shows, and tick the box."
5. **The list while the operator waits.** "Refresh the list" is a plain link to `/pair`, which reloads the page. No polling is added, and an open page shows a new appliance once reloaded.
6. **The words.** Every word the operator reads on this page says appliance, not box. Identifiers stay as they are.
7. **The names the script prints:**
   - the page, "New agent";
   - the list, "Appliances awaiting final initialization";
   - the action, "Finalize the pairing";
   - where the port is shown, step 4 of "At the appliance", under the heading "At the appliance".
8. **The tests,** six in all, beside those the moved code keeps:
   - the first page gives the management door's address and never the data door's, red when the step reads `gate_address`;
   - with one door, the first page says both planes share one door beside that address;
   - nothing of a report is drawn on the first page before the code matches;
   - an appliance that dialed before its agent was named is listed on the first page and pairs;
   - a code that matches another waiting appliance's connection and not the chosen one is a miss and pairs nothing, red when the code is matched across the table;
   - three misses close the chosen connection alone, and the other appliance still waits.

## One point more, from the Executor

9. **The starting user's first password is a credential, and no credential enters this repository** (`CLAUDE.md`, hard boundaries).
   - The operator's 72 (v) lets the page print the user and the first password. Printing them puts both in a template in a public repository.
   - The user's name is a product constant once the appliance's profile states it, as the ruling of 2026-11-17 admits the profile's mount points.
   - The password is a credential, even one changed at first login.
   - Until the operator rules, the page names neither. It reads "the appliance's starting user", "its first password", and that the password is changed at the first login.

## What is not this act

- The host's `register` under Provision stays as it stands.
- The reader of `weaver-survey/2`, the agent's account over admin-con, and the loop page each come in their own act.
- Nothing is pushed until the Planner's word.
