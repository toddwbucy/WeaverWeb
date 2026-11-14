# Brief: the declaration's placement leaves

Written by the WeaverWeb Executor on 2026-11-14, from the suite Planner's word of that day, after the operator ratified the rollback batch. Stack `act/the-placement-leaves`, built on `act/the-path-in-the-masthead` at `393264d904a79ddc0813b148f5e87f1f927388ec` until #102 is on `main`, then on `main`.

## Why

On 2026-11-14 the operator withdrew a direction in writing, in these words:

> "Reversal first. The direction taken last night, Weaver Web driving VM lifecycle through the hypervisor (host-or-VM field, clone at declaration, destroy at retire, start and stop as the agent's load and unload), is withdrawn. Weaver Web never touches a hypervisor."

What replaces it is ruled in writing only this far:

> "Weaver Web manages the agent service running inside an appliance. The appliance is a locked-down VM image. The VM itself, creating it, starting it, stopping it, snapshotting it, destroying it, belongs to the operator and their hypervisor, and Weaver Web has no opinion about it. Starting and stopping the agent means load and unload of the service through admin, which already exist. The single-host install remains only for the Spark class of machine."

The declaration, in the operator's words, is "SPU model and GPU, LoRA adapters, state election, system prompt". It has no host-or-VM field.

The declaration screen of 2026-11-11 (#79) put that field in the console's document as `placement`. The field is `{"kind": "host"}`, or `{"kind": "virtual-machine", "template": text}`. The Spec, the PRD and the design system described the console cloning a virtual-machine agent from that template at declaration. They described load as the machine's start, unload as its stop and retire as its destroy. They spoke of a host plane that would report cards. No code ever cloned, started, stopped or destroyed a machine. The field was declared, saved and written into the derived file as a comment, and nothing acted on it.

## What this act does

1. **The member leaves the document.** It goes from the grammar, the parse, the canonical value, the form and the screen's select and template field. It goes from the derived file's comment and from its trailing block.
2. **Rows already saved stay as they are.** A row's authored half never changes (Spec 2.4; migration `0023`'s trigger refuses any change to `body`). So no migration rides in this act, and no saved body is rewritten.
   - **On read only**, the reader admits a legacy `placement` member on a saved row and discards it.
   - **A posted document, or one about to be saved, that carries it is refused** as an unknown member is.
   - **A child saved from such a row records `placement` once among what moved**, the field leaving.
3. **Spec 6.1** loses the field's bullet, its grammar line, its place in the refusal list, its waits row and its place in the trailing block.
4. **Spec 2.4** loses its two virtual-machine phrases.
5. **"Host plane" leaves the words of Spec 6.1 and section 10.** A card's identifier is held as typed, and what checks it is left open. The open entry on a host's reserved card is kept, by the operator's choice of 2026-11-14, for the single-host install of the Spark class.
6. **PRD section 3.0** loses stage 1's "host or virtual machine" and the virtual-machine sentences under "Declare is two screens".
7. **The design system's** stage 1 Agent row and stage 2 Staging clause are reworded, and `declare.md`'s agent pane and its list of waits with them.

## Ruled out

- **A revert of #79.** It carries the declaration screen, its rows, its lineage and the derived file, which all stand.
- **A migration.** It could not rewrite an immutable body without breaking Spec 2.4's rule.
- **Writing the appliance, its enrollment or its storage into the Spec.** None of them is ruled into text yet.
- **Editing the briefs of 2026-11-11 and 2026-11-13.** They are the record of what was asked then, and this brief records the withdrawal.
- **Closing #71.** The Planner closes it on the operator's word.
- No push until the Planner's word, and no restart of the running console.

## The sub-act

1. **`act/the-placement-leaves-1-removal`**, the declaration's placement leaves:
   - the code, the tests and the reader of a legacy member;
   - Spec 2.4, 6.1 and 10, PRD 3.0, and the design system's `stages.md` and `declare.md`.

## Acceptance

- A saved row carrying `placement` is read, shown and derived without it. A post or a save carrying it is refused, naming it. A child of such a row records `placement` among what moved. Each of these has a test, and each guard fails its test when the guard is removed.
- No word of the withdrawn direction is left in the Spec, the PRD or the design system, other than the briefs named above.
- Each Spec section named under `Implements:` has a verdict.
- The gates pass:
  - `cargo fmt --all -- --check`;
  - `cargo build --locked`;
  - `cargo clippy --all-targets --locked -- -D warnings`, with and without `--no-default-features`;
  - `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
