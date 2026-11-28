# Brief: the workflow round

**Date:** 2026-11-28. **From:** the Planner, on the operator's walk of the pages on the night of 2026-11-27, written into the repository by the WeaverWeb Executor. **Stack:** `act/the-workflow-round`, cut from `main` at `f2964d75e1a9b063b130c6cdec316fd2949f2af9`.

## Why

The operator walked the pages on a freshly sealed appliance copy and paired an agent end to end. The pages work, but the path through them is wrong in places. This round changes the order and the path, not the look.

## The operator's words that bind it

- "on the first tab we should have a list of agents and teh create new agent button that is all. if you click the create agent button THEN you go to what is currently called the new agent tab, if you pick an agent that already exists then you should be sent to the declaration for that agent with all its information there."
- On the older declaration screen of 2026-11-11: "retire teh older stuff".
- On the SSH key page: "yes drop it".
- "the adapter should come right after the choosing of the model, infact there is no reason why you can declare the model then immeidately go to declareing the adapters adn then let them download and dont forget the download/progress indicator"
- "can you add a theme selector, perhaps a dropdown on the upper right corner"
- "for now we will leave the design alone and will focus mostly on the workflow."

## Already ruled out

- **The look.** No change to the design system's documents or to the pane grammar. The status marks stay as they are.
- **Replace box.** The stack `act/a-reset-vm-is-a-new-agent` removes it, so this round leaves it alone where it still stands.
- **A weaker Origin check.** Part 3 changes how its refusal is drawn, and the check stays exactly as strict.

## The parts, one child each, stacked in this order

**1. Tab 1.** The Agents tab opens one page: a list of every agent and every open setup, each by name with one plain state word (for example: waiting for its appliance, paired, declaring, confirmed, loaded), and one button, Create new agent. Nothing else on that page. Create opens today's New agent page. Picking a row opens that agent's declaration (part 4). The sub-tab row under Agents shrinks to match. The register's actions (rotate, revoke, retire) stay reachable from somewhere an admin can find. No page loses its last way in.

**2. New agent, console first.** The appliance ships with SSH off, and setup runs at its console. Every line on New agent that says SSH says console instead, the code field's label among them. A refused Finalize draws its reason at the top of the form, in a fail status an operator sees without scrolling. Each refused Finalize is recorded, an audit row or a log line saying which refusal and when, so it can be read afterwards.

**3. Refusals with a way back.** The Origin check's refusal is a bare text page with no way back: an operator who opens Weaver Web by an address other than its configured origin has every post refused with no hint why. That refusal becomes a page with the masthead, the reason in plain words, the address Weaver Web expects (the configured origin), and a link to it.

**4. The agent's declaration, in the order of the work.** The setup pages become the agent's declaration: directories, then models, then placement, then confirm. In models:
- the operator picks the model, then goes straight to the adapter step, which picks an adapter from a repository or none. The adapter is optional.
- the downloads start once the model, and the adapter where one is picked, are chosen.
- a download shows its progress: bytes done of the total and a percentage, with a bar, per file and in all, refreshing on its own often enough to watch. The house rule of no inline script holds, or the report says why it must bend.
- a repository's files are listed by name, each opening to show its details on a click.

In placement, the turn context says what it means in plain words: continuous, the conversation carries over from turn to turn; independent, each turn starts fresh from the starting prompt. The setup rail's repeated "1 Declare" on each step may stay in this round.

**5. Retire the older declaration screen and the SSH key page.** The screen of 2026-11-11 (`/declarations` and its routes, `declaration.html`) and the Build stub's link to it go, and so do the SSH key page and the Directories page's SSH key pane. First, what depends on them is read: whether a load, the agent page, the derived agent configuration or any test reads the screen's rows or the stored keys. That list is reported before anything a load or the agent page needs is removed, and such a need moves to the setup pages.

**6. A theme selector.** A drop-down at the upper right of the masthead naming the six themes of the design system and auto, kept per session or per browser, so a page never flashes the wrong theme on load. Small, and last.

## Acceptance

Each child passes the repository's gates: fmt, build, clippy with and without default features, and the full suite with the database set. Each child's report gives its head, the files it touches, the tests it adds with a perturbation that turns each red, the counts, and anything chosen that this brief leaves open. Part 1 is reported before part 2 begins.

## Scope

Under 100 files for the whole stack. Draft pull requests only, each child against the stack's base, pushed on the Planner's word.
