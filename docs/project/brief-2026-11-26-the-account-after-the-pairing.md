# Brief: the agent's account right after the pairing

**Date:** 2026-11-26. **From:** the Planner, on the Executor's plan of 01:23 UTC and the Planner's decisions of 01:35 UTC. **Stack:** `act/the-account-after-the-pairing`, cut from `act/the-survey-reader` at `4c76a06690fea62ee56dfb58f713efc6e678bcd4`.

## The goal

The documents and the pages say when the agent's own account and its folder of per-agent files are laid: right after the pairing, once the name returns from Weaver Web, by the appliance's setup script. Today they say the account is laid at setup, after Confirm. Weaver Web adds no ask, no frame and no write.

## The operator's words that bind it (2026-11-25)

- "The admincon and gatecon accounts are in the template, made on the development side. Their names never change. The script does not make them. The agent's own account is laid right after the pairing, over admincon, once the name returns from Weaver Web."
- "[The per-agent folder] holds the per-agent files, laid once the name is known: gatecon's config and the loop directory. Root owns, and the agent's group reads."
- "The two mount paths are hard coded nowhere. One value, set once by the script, read by everything else."
- His step 2 of the setup script, which stays on New agent word for word: "Each mount path is preset to our default, and the operator may type another."

The folder's location is the appliance's fact, elided from his words above. No code, test or document of this stack names it.

## The reading this act builds on

**Reading 1, the Planner's decision of 2026-11-26, put to the operator to confirm.** "Over admincon" names the pairing that returns the name: `admin-con pair` answers `paired` with the agent's name (Spec 8.2), and the setup script then lays the agent's account and its folder at the appliance's screen, with root's authority, as it lays the disks. Weaver Web asks nothing for it. Should the operator rule otherwise, this act's sentences change and nothing else.

## Already ruled out

- **An ask from Weaver Web that lays the account** (reading 2), a new privileged line or verb. Neither planned nor built.
- **A witness on the agent page** that admin's lines are granted. Left out.
- **Any place sent to the appliance.** The survey's report is the one source of the two places, and Weaver Web sends neither back.
- **Changing PRD stage 2's "Every root act runs on the box at a screen the operator is sitting at" or Spec 8.2's matching sentence.** They change in act B2's documents, after the operator answers on loop install, not here.
- **The rules file's sentences** on the two mount points, the connector accounts and standard input. They are the operator's, and this act edits none of them.

## Correction 1, carried

New agent's step 2 keeps "Each mount path is preset to our default, and the operator may type another." It is the operator's own sentence. His sentence that the mount paths are hard coded nowhere is about code: the script holds the preset, and everything else reads what the script set. So:
- `pair_start.html` line 29 does not change, nor does the test that pins it (`src/setup_tests.rs`).
- Spec 6.2's New agent row keeps "its mount path preset to the default", in step with the page.
- Spec 8.2's "each mounted at its default path" becomes the place the script set, its default preset.

## The children, in order

Each passes the repository's gates alone.

1. **The documents** (`act/the-account-after-the-pairing-1-docs`). Files: `docs/weaver-web-Spec.md`, `docs/weaver-web-PRD.md`.
   - **Spec 6.2, the paragraph on the agent's accounts** (today "the box's agent-creation tool lays them at setup, named from `weaver-<name>`"): the appliance's setup script lays the agent's own account right after the pairing, once the name returns, named from `weaver-<name>`; the connectors' two accounts are the image's, made before it ships, their names never changing; with the account the script lays the agent's folder of per-agent files, laid once the name is known, root's and read by the agent's group, which will hold gate-con's config and the loop's directory; Weaver Web asks nothing for either.
   - **Spec 8.3, the storage paragraph's same sentence:** the same change, citing section 6.2.
   - **Spec 6.2's table:**
     - New agent: the Name the agent pane's footer gains the account and folder sentence (below).
     - Pair: once the box is paired, the paired line and its sentence (below).
     - Build: the pane's sentence (below).
   - **Spec 8.2, "each mounted at its default path":** each mounted at the place the setup script set, its default preset and another typed where the operator chooses, with his sentence on the one value.
   - **PRD stage 1's row:** after the survey's report crosses, the setup script lays the agent's account and its folder once the name returns.
   - **PRD section on the box's side, the accounts' sentence and "mounted at the default paths":** as in the Spec.
   - Section 9 gains no row: no claim of the server's behaviour changes.
2. **The pages** (`act/the-account-after-the-pairing-2-pages`). Files: `src/surfaces/templates/pair_start.html`, `src/surfaces/templates/setup_pairing.html`, `src/surfaces/templates/setup_build.html`, and their tests.
   - One test per page pins the new sentence and the old one's absence. One perturbation each: the old sentence put back, the test measured failing.

## Each page sentence, before and after

| Page | Before | After |
| --- | --- | --- |
| New agent, the Name the agent pane's footer (`pair_start.html`:95) | "Once paired, the agent's Pair page shows the appliance's report and leads on to its declaration." | "Once paired, the appliance's setup script lays the agent's account and its folder under this name, and the agent's Pair page shows the appliance's report and leads on to its declaration." |
| Pair, the paired line (`setup_pairing.html`:91) | "The appliance's setup script finishes the box's side." | "The appliance's setup script now lays the agent's account and its folder under this name, then finishes the box's side." |
| Build (`setup_build.html`:8) | "The box lays the agent from the deployment tuple once Confirm stands." | "The appliance's setup script lays the agent's account and its folder at the pairing. The box lays the rest of the agent from the deployment tuple once Confirm stands." |
| New agent, step 2 of what an agent needs (`pair_start.html`:29) | "Each mount path is preset to our default, and the operator may type another." | unchanged (correction 1) |

The three new sentences are working text. The operator may reword them.

## The acceptance

- Every sentence of the table above reads as its "After" column, and each page's test fails with the "Before" sentence put back.
- No document of this stack says the agent's account is laid after Confirm, and none names the per-agent folder's location.
- **The gates:** the repository's gates, alone, with `DATABASE_URL` and `PGUSER` set, the count reported and nothing skipped.

## The scope boundary

- **In:** the documents' sentences on when the account and the folder are laid, Spec 8.2's mount sentence, and the three page sentences.
- **Out:** any ask, frame, store write or migration; the loop page and loop install (acts B1 and B2); gate-con's enrollment; the rules file; the chat console, sign-in, the replacement of an appliance and the host's own install.
