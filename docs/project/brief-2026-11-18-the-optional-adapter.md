# Brief: the optional adapter

**Date:** 2026-11-18. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the operator's ruling of 2026-11-18 that the adapter is optional. The
console is the mechanism by which a developer makes an agent from a blank one, and
nothing in the mechanism's Spec, pages or code may require or assume one agent's
arrangement. The scope is the ruling of 2026-11-16: zero or one adapter, from Hugging
Face only. The base's identity is still recorded, and a mismatched pair is still
refused. The act is one stack of three sub-acts from `main`, branched once the act of
the hello and the admin writes (#157) merges. It merges before the act that moves the
model to the appliance, so that act's checks are proven once against the final picks.

## Goal

A setup with no adapter reaches Confirm, writes a tuple with no adapter, and lays an
agent that loads its base alone. At `main` `00545dd4` five places assume an adapter:

- The tuple's six adapter columns are `NOT NULL` (migration `0029`).
- Confirm holds on "no adapter is picked".
- `Laying::of` refuses a tuple whose adapter has no part, so `describe` would answer a
  fault for a tuple with none.
- `describe`'s laying always carries `adapter`.
- The models page's adapter pane holds, and nothing takes a setup from one adapter
  back to none.

The declaration screen already takes a decoder with or without its adapter.
WeaverAgent's agent-creation tool at `be56d8c2` takes all four adapter options or none,
and an absent adapter is a base-only load (the adapter binding's ADP-1). The base's
BLAKE3 reaches the box only to check an adapter against its base, so a laying with no
adapter needs no base digest on the box, and no interface question is filed.

## The sub-acts, in order

1. **The Spec.**
   - Section 6.2: the tuple's adapter members and scale where one is picked. Confirm's
     way forward waits on the adapter's pull only where one is picked. The confirm
     page reads "none picked: the agent loads its base alone" and draws no scale row.
     The laying carries the adapter's members where the tuple holds one. The drop of an
     adapter's pick is among the setup's writes. The base's identity is recorded either
     way.
   - Section 8.3: `describe`'s laying carries `adapter` only where the tuple holds one.
   - Section 9's rows, owed to the code.
   - PRD section 3.0's Confirm line.
2. **The tuple and its pages.**
   - Migration `0032`: the six adapter columns nullable, all present or all absent, each
     column's own check holding where present, `base_blake3` still required. Every
     existing row keeps its six values, and the body says so.
   - The store reads a setup with no adapter row as one with no adapter.
   - `Laying::of` and `describe`'s laying carry the adapter only where the tuple holds
     one.
   - The drop of the adapter's pick: its own write in Spec 2.13's order, versioned, its
     step four under the bound every admin write takes, refused on a confirmed setup.
     The files the pick pulled stay in the cache, as a replaced pick's do.
   - The models page's adapter pane reads optional, never hold, with a Drop form where
     an adapter is picked. Confirm's page reads "none picked: the agent loads its base
     alone" and draws no scale row, and a confirmed setup's page draws the same from
     the tuple's row.
   - Tests:
     - the all-or-none check both ways: five members present is refused, none present
       is accepted
     - the drop dropped at the hold as the dropped-request tests do
     - the drop refused once the setup is confirmed
     - E5, a setup with no adapter from the pairing through Confirm.
3. **The words.** The roster is no longer named one agent's, in:
   - Spec sections 6.1 and 6.2 and section 10's base-identity cell
   - PRD section 3.0
   - the models page's pane heading and footer
   - the two refusals in `src/surfaces/setup/models.rs`, with tests that pin them
   - the conforms tag, renamed with its section 9 row.

   PRD section 3.0's placement passage stops naming one agent's model. Two lines that
   reword the operator's own ruling text ("the first production agent declares a
   store", "the suite ships a tuple") wait on the operator. They stay out of this act
   unless the operator rules before it closes.

## Ruled out

- More than one adapter, an adapter on a classifier, and an adapter from anywhere but
  Hugging Face (the ruling of 2026-11-16).
- A base whose identity is not recorded, and a crossed pick admitted.
- Any change to WeaverAgent. The box's runner passes the adapter options only where
  `describe`'s laying carries an adapter, and that is a sentence in
  `toddwbucy/WeaverTools#39`, for its own act.
- Moving the model to the appliance, which is the next act.
- The developer-framing sweep's other rows, which wait on their own rulings.

## Shown running

- The Spec sub-act leaves draft on the Planner's read.
- The tuple sub-act leaves draft when each of these has run through the real listener
  and store:
  - E1, `the_reports_a_box_sends_feed_placement_and_confirm`, with an adapter picked
  - E2, `the_panes_are_served_in_each_status_as_the_box_reports`
  - E3, `a_refused_reports_arrival_is_logged_once_with_its_detail_escaped`
  - `an_adopted_pair_connects`
  - E5, `a_setup_with_no_adapter_confirms_a_tuple_with_none`, with none.

  The ten dropped-request tests rerun too, since the act adds a write.
- The words sub-act leaves draft with its page tests.

Each body says what ran end to end and what has no path. A real appliance laying an
agent with no adapter waits on the model's move to the appliance and on
`toddwbucy/WeaverTools#39`.

## Gates

The repository's own gates, on each sub-act's head and on the stack's head, `--locked`
throughout, each worktree with a target of its own:

- `fmt` and `build`
- both `clippy` runs
- the suite at eight threads, with `DATABASE_URL` and `PGUSER` set.

Each new test has a perturbation, restored from a scratchpad copy checked by `cmp`.

The suite workflow's path: draft sub-PRs into the stack, the Planner's grade and Codex
per sub-PR, CodeRabbit and Codex on the stack, and the operator's merge to `main`.
