# Brief: the appliance reaches out

Written by the Planner on 2026-11-25. For the WeaverWeb Executor. Other seats have parts, listed at the end so the order is whole.

## Goal

The operator ruled the whole first meeting of an appliance and Weaver Web on 2026-11-25 at 15:40 UTC. After this act:

- the appliance's setup script asks where Weaver Web is, as its last question, calls it over the management network and shows a short code;
- Weaver Web's first page under Agents holds the steps, and lists the appliances waiting;
- the operator clicks theirs, names the agent and enters the code;
- that one exchange enrolls admincon alone and brings Weaver Web the appliance's report, and the report is the receipt;
- from then on admincon holds one connection to Weaver Web under mutual TLS, from the appliance's management card, and redials it after a drop;
- Weaver Web has two doors, one on each network: the management door for admincon, the data door for gatecon.

**Most of this is the pairing as built.** The new work is the first page, pairing as admincon alone, the report at pairing, and the two doors.

## The operator's words

The design talk, 2026-11-25, in order:

- "If the user's already there in the initialization screen, he could literally put in the IP as part of the initialization of Weaver Web, and it could reach out. And then when Weaver Web got that connection, or got that initialization, when they go back to the Weaver website, we could literally have a window that says VMs await, or, you know, we could have a list there that says agents awaiting initialization. And then you can click on that agent, finish the initialization, and go through the pairing process. This way, we don't have to leave anything open other than port 22 and SSH."
- "Weaver Web still talks over its own thing, but the user needs to be able to SSH into the VM."
- "this is not meant to be deployed on a network you don't trust. This is meant to be deployed on a trusted network, the same place you would deploy a hypervisor, for instance."
- "Two separate NICs on the VM for both Weaver Web and our agents. Each NIC on a separate subnet so that we bisect the traffic. We separate or segregate the data plane from the management plane traffic."
- "Maybe force is too strong, perhaps highly recommended, and that's what our default setup should be."
- "Why so strict? Think of this in terms of sandboxing."
- "GateCon will be tied to one NIC and AdminCon would be tied to another NIC. on the VM, of course."

The Planner then put six lines to him, and he answered: "I think we got it now. YES."

1. The appliance ships with a starting user with sudo, on SSH. The first login forces a new password.
2. Two cards on two networks is the default: management and data. One card only on purpose, and it shows.
3. The operator logs in and runs the setup script. Its last question is "where is Weaver Web?"
4. The appliance calls Weaver Web over the management network and shows a short code.
5. Weaver Web's first page lists the appliances waiting. The operator clicks theirs, names the agent, enters the code.
6. After that: mutual TLS. admincon holds one line to Weaver Web on the management card. gatecon uses the data card only.

Rulings that stand from before, in his words:

- "Pairing is admincon only. The setup walk starts admincon and nothing else."
- "Gatecon is not started, enrolled or tested at pairing. It is created at loop install, with its config."
- "The survey's report is the receipt."
- The first tab (2026-11-25): the first page holds the steps at the appliance, then the name. Tabs stand in the order of work. The bare address opens the first tab.
- "They both need to have duplex, it needs to be a duplex connection." The standing connection as built is duplex, and stays.

## What this replaces

| Earlier | Now |
|---|---|
| admincon listens during initialization and Weaver Web calls it once (2026-11-24, and this morning) | Nothing on the appliance listens for Weaver Web. The appliance calls. |
| A key of 48 characters, typed or pasted into Weaver Web | The built code: 10 characters, drawn from the TLS exporter of the one waiting connection |
| With one card the script says both planes share it and does not ask (2026-11-24) | One card only on purpose, and it shows: at the script, in the report, and on the agent's pages |
| One listener for both planes | Two doors, one on each network |

## Read by the Planner in the source, at `main` `9e8ddb99`

- `Listener::waiting()` (`src/link/listener.rs` 1220) returns `WaitingBox { box, peer, waited }` (`src/link/pairing.rs` 68), and never the code.
- The code is 10 characters of Crockford's base32, the first 50 bits of the exporter (`src/link/pair_code.rs` 20 to 29).
- A dial waits 600 s (`src/link/pairing.rs` 57).
- `link_address` falls back to `link_listen` (`src/config.rs` 152). There is one link listener.
- From your first-tab plan of 13:24 UTC, verified by me then: the waiting list is server-wide and no test dials first and names the agent after.

## What the plan must settle

**A. The first page.** It is the first tab, `/pair`. In this order:

1. The steps at the appliance, written once:
   - make the VM from the appliance file with two cards on two networks, and put the management card on a network you trust, as you would a hypervisor;
   - attach the two drives, unmounted;
   - log in over SSH as the starting user, and set a new password at that first login;
   - run the setup script, and give it this Weaver Web's management address when it asks.
   The starting user's name and first password are not ruled. Write a placeholder and say where it stands.
2. The appliances waiting: the machine's own name, the address it called from, and how long it has waited.
3. The operator clicks one, names the agent and enters the code. Say how the click, the name and the code are ordered and which requests carry them. Say whether the code is tried against the chosen connection alone or against the whole table as today, and why. Say what a wrong code costs and where the count lives.
4. The result: paired, with the report shown as the receipt.

Say what stands of your first-tab plan's child 2, and whether this act takes it over. Say what becomes of the agent's own Pair page.

**B. Pairing as admincon alone.** The exchange signs one request. The gate plane is none until loop install. Say what changes in the register, in `Agent::present()`, and on the pages that show an agent with no gate plane yet. Say what of this morning's plan carries over, the migration included.

**C. The report at pairing.** When the page says paired, Weaver Web holds the appliance's report. Say whether the report rides the pairing connection or follows on the setup connection as built, and what the page shows between the two.

**D. The two doors.**
- Weaver Web listens for admincon on its management address and for gatecon on its data address. The config names both.
- The same address for both is allowed, on purpose, and a page shows it. The default and the documents describe two.
- **Each door refuses the other plane's certificate.** Test both halves. The operator asked for this test on 2026-11-24.
- The browser's door stays on the management side.
- Propose the data door's port. The network's keeper needs the number.
- Say where the appliance learns Weaver Web's data address. My expectation, not a ruling: at loop install, over admincon, in gatecon's config.
- Say how each connector is tied to its card. Today a connector dials by the machine's routes. Propose the member or flag that names the address it dials from, for `admin-con` and `gate-con` both.

**E. Shared planes shown.** When the newest report says the two planes share one card or one network, the agent's pages say so, for as long as it is true. This needs the reader of `weaver-survey/2`. The format document is another seat's act. Say what member you need in it. My proposal: beside each plane's address and prefix length, one member written by the script from what the operator chose, with two values, separate and shared.

**F. What becomes of what is built.** For each, say stays, changes or goes: `admin-con pair` and its flags, the fingerprint shown at both ends, the 600 s wait, `setup_closed`, `admin-con request` and `adopt`. On the fingerprint: the code is drawn from the TLS exporter, so say what comparing the fingerprint still adds.

**G. The documents.** List each sighting that changes: Spec 6.2, 8.2 and 8.3, the PRD's 10.1 and its stage text, the design system. PRD 10.1 says the appliance never listens for the console. That sentence holds again.

**H. The parts.** Docs first in each part, then code. For each part: files, estimated size, the tests by name, and the removals that turn each test red. One stack or two, your call, with your reason. Under 100 files a stack.

## Boundaries

- Sign-in stays off. No sign-in, passkey or certificate ceremony for the browser.
- The setup script, the survey and the appliance file belong to the WeaverAgent seat. Name what you need from them, and change none of it.
- Loop install and gatecon's creation are a later act. This act builds the data door and its refusal test, and no more of gatecon.
- The tests must not need a machine with two cards.
- No box paths, host names, addresses of ours, accounts or credentials in the plan, in a commit or in a document.

## The other seats' parts

| Seat | Part |
|---|---|
| WeaverAgent | The setup script runs over SSH, asks to confirm one card, and passes the planes to the survey. The appliance file gets the starting user. |
| WeaverAgent, in the suite repository | The format document `weaver-survey/2`, then the survey writes it. |
| WeaverWeb, a small act of its own | The reader of formats 1 and 2, to `main` once the format document merges. |
| The network's keeper | A second network in our development setup. Asked today. |
