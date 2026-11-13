# Brief: the path in the masthead

Written by the WeaverWeb Executor on 2026-11-13, from the suite Planner's word of that day, after the Executor's scope reply. Stack `act/the-path-in-the-masthead`, from `main` at `164980eeecdf570d1386115186b7c6ef3f6e0218`.

## Why

On 2026-11-13 the operator signed in and used the console against a real agent for the first time. His words, relayed by the Planner: "the individual pages look ok but the way they are all organized along the top is messed up. there is no logical ordering, no worflow that takes you from one panel to the next its like there are just links in the headers and they were thrown in there haphazardly".

The design system agrees with him:

- **The masthead's order** is Record, Agents, the alerts pill, Passkeys, Persons, Grants, Register, Declarations. The path's first stage is the last link, and links for the account and for administration sit between the stages.
- **The stage rail** states the path in order: Declare, Provision, Run, Observe, Preserve, Diagnose, Train, Retire. But it is served on the agent page alone, and its marks are spans.

## Measured, read only

- **The Masthead guideline** (`docs/design/system/components/Masthead/README.md`) says: "Do not put the stages here; they belong to the agent's stage rail." The brand book (`docs/design/system/README.md`, "Navigation across the eight stages") lists the server pages as Agents, Register, Persons, Grants and Passkeys.
- **Record is no stage's own.** `stages.md` holds no Record pane under any stage. Its Train row calls the landed record "the record of the whole lifecycle and no stage's own", and Diagnose is unbuilt.
- **Record filters by artifact, session or branches.** No filter names an agent, and no landed run is tied to a register row.
- **An admin who registers an agent holds no grant on it.** The agent's page answers 404 without a grant (Spec 2.13).
- **The declaration screen and the register are admin-only.**
- **The masthead's stylesheet splits the bar at `.who`**, whose left margin is automatic.

## What was ruled (the Planner, 2026-11-13, on the scope reply)

- **The masthead, in the path's order, in two groups.**
  - **Left, the work:**
    1. Declarations, marked 1 Declare;
    2. Register, marked 2 Provision;
    3. Agents, marked 3 Run and 4 Observe, followed by the open-alerts pill;
    4. Record, unmarked and last, as no stage's own.
  - **Right, the person and the house:** who, then Persons and Grants for an admin, then Passkeys, then sign out.
  - **A non-admin** sees the same order less what they may not open.
  - **No stylesheet change.**
  - The marks state the order where a person looks. The Masthead guideline's line and the brand book's line are amended in this act, and the operator rules on them by merging.
- **A way forward**, one plain link at the foot of the page's main pane, computed from what the page already knows. No new page, no wizard, no stored state.
  - **From a saved declaration:** to registering an agent of its name. The register form reads `name` from the query.
  - **From the handover:**
    - Where the admin holds a grant on the agent, to the agent's page, with one sentence on what is done on the box between the two steps.
    - Otherwise, to Grants, with the reason.
  - **From the agent page:**
    - to Record, saying plainly that it is the whole record, not filtered by agent;
    - for an admin, back to the declaration saved under the agent's name, or to a new declaration where none is saved.
- **The rail on the agent page:**
  - For an admin, Declare and Provision become links: to the declaration by name, and to Register.
  - Run is the current page.
  - Every other stage stays a span.
- **Nothing about a run on the agent page.**

## Ruled out

- No migration, no new page, no stored state, and no stylesheet change.
- No edit in WeaverAgent.
- No mark on Record, and no link from the rail's Diagnose to Record.

## The sub-act

1. **`act/the-path-in-the-masthead-1-links`: the masthead, the forward links and the rail's links (code, Spec and design documents).**
   - The masthead's order and marks.
   - The register form reading `name`.
   - The forward link from a saved declaration, from the handover and from the agent page, and the agent page's link back to its declaration.
   - The rail's two links.
   - Spec 2.8's navigation sentence, Spec 2.13 and 6.1, and section 9.
   - The design documents: the Masthead and StageRail guidelines, the brand book's navigation section, and `stages.md`.

## Acceptance

- **The masthead's order** for an admin and for a non-admin, each held by a test.
- **Each forward link** is present where its condition holds and absent where it does not, each held by a test.
- **Every guard fails its test with the guard removed:**
  - the order;
  - each mark;
  - Record left unmarked;
  - each admin-only link absent for a non-admin;
  - each forward link's condition;
  - the name read by the register form.
- Each Spec section named under `Implements:` has a verdict.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
