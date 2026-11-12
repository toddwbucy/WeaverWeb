# Brief: the declaration screen

Sent by the suite Planner on 2026-11-11. Stack `act/the-declaration-screen`, from `main` at 1a78733a41ce052e55ea3d9290cf8d397e5f23cf.

## What was ruled (the operator, 2026-11-11)

1. The Declare stage is a loop builder in two steps. Step one, the operator declares the components: which models, on which GPUs, which store, which gate. Step two, the operator wires them on a board that shows only what was declared. This act is step one.
2. The board, when it comes: components are compiled primitives the harness is pre-wired to; a wire is one ask and one return, written in Python by the operator; a function that makes several asks is a node the wires converge on. None of that is drawn or built here.
3. One SPU box stands for the models declared. Four models declared separately are four boxes; one decoder with adapters is one box. Spread out or collapsed is the operator's choice.
4. The engine follows from the model's format. GGUF is supported first; other formats are named as later and not offered.
5. Whether an agent is a host agent or a virtual-machine agent is one field on this screen. There is no Hosts page. A virtual-machine agent is cloned by the console from a template at declaration; then load is the machine's start, unload is its stop, retire is its destroy. The template's image digest is the agent's provenance.
6. A GPU is declared as a requirement, not a device: a specific card, a class, or a pair. A class may span cards of one compute capability; the screen warns where it spans more. Start refuses where nothing satisfies the requirement, and the card actually drawn is stamped on every load event.
7. The system prompt is a field on this screen. The declaration act writes it into whatever state was declared; from then on it is read from state and never from the file, and it is locked for the session. Changing it is a reload.
8. The sampler: its mechanism is fixed at load. Its knobs (temperature, top-k, top-p, seed, and by the same test repetition penalty, repetition window and max tokens per turn) are per-turn: the declaration supplies defaults, the turn call carries the values, the record holds them per event. The test for any engine setting is whether changing it invalidates the cache: if yes it is fixed at load, if no it is a per-turn knob.
9. An agent is binaries by hash plus wires by hash, and the screens are a rendering of that card. A wire edit is a new link in the lineage, not a reload of the same agent.
10. The trace is the record, not a component.

## The goal

A signed-in person with the right role can declare an agent's components on one screen, save the declaration as a versioned document, and read it back as the file it will become. Nothing here reaches an agent or a host.

## Ruled out

- No board, no wires, no Python on any page.
- No Hosts page, no host connector, no clone or start. The virtual-machine field is declared and saved; acting on it waits on the host plane's Spec, which waits on a measurement in progress.
- No reach to an agent: no validate, no install of a declaration, no write of a prompt into state. Each of those crosses to WeaverAgent and is an interface question, listed below.
- No box path, host name, account or address in the repository. A card, a class and a template are named by what the operator types and what an agent reports, never by this box's values.

## What to read

- `docs/weaver-web-PRD.md` section 3.1 (Compose) and the Declare passages; `docs/design/system/declare.md`, `stages.md` stages 1 and 2, and the DeclareDiagram component. They draw one diagram with a system prompt on the decoder and free placement of pieces; the rulings above replace that with two screens.
- The Spec's sections for the agents and admin pages, for the grammar a served page follows.
- WeaverAgent's declaration grammar (`weaver-types-Spec` section 2), read as the grammar the saved document must become. Cite it; copy no box fact from it.

## Sub-acts, in order

1. **The Spec, the PRD and the design system's text.** Declare as two screens. For this screen: each field, what it may hold, what the form refuses on its own (a missing required field, a requirement that names nothing), what it warns of (a class spanning more than one compute capability), and what is saved. The document's version and last save. What each field waits on before it can take effect. The diagram's old text removed where the rulings replace it.
2. **The page.** Served under the signed-in grammar, with the role the Spec names. It saves the declaration as a versioned document in the store (a migration), shows the document as the file it will become, and reopens the same document. Tests for each refusal and each warning, and for a save and a reopen being the same document.

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour has a test.
- The page reaches no agent and no host: a test reads that no link frame is sent.
- fmt, build, clippy with and without default features, and the suite with the database variables and eight test threads, each with its exit status and counts, nothing skipped.

## Interface questions this act will meet (draft each as an issue for the owning repository; file none)

1. The saved document's grammar. WeaverAgent's declaration is one file with one decoder table and device ordinals. It has no place for a GPU requirement, a second function, sampler defaults, or host-or-virtual-machine.
2. How a declaration reaches an agent. Today the operator installs the file on the box. No console verb installs one.
3. How the prompt is written into declared state at declaration. What is built is a seeding turn through the gate.
4. What a declaration with no store does with a prompt, since the ruling writes it into "whatever state was declared".
5. Which settings an agent reports as fixed at load, so the screen can show them read-only beside the model.

## For the operator's ruling, not decided here

- The role that may declare.
- Whether a saved declaration may be edited in place or every save is a new version in a lineage (ruling 9 suggests the second).
