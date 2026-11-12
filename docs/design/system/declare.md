# Declare: two screens

Declare is a loop builder in two steps, on the operator's rulings of 2026-11-11. On the first screen the operator declares the components: which models, on which GPUs, which store, which gate. On the second, the board, the operator wires them, and the board shows only what was declared. The first screen is specified in Spec section 6.1; the board is unbuilt and waits on contracts WeaverAgent has yet to write. An agent is binaries by hash plus wires by hash, and the two screens are a rendering of that card. The trace is the record and not a component, so neither screen declares it.

## The declaration screen

The screen is a form of panes, under the pane grammar every surface uses, and never a canvas. Nothing on it is dragged or placed.

- **The agent pane** holds the name and the one field that says host agent or virtual-machine agent, with the template a virtual-machine agent is cloned from. The console does the clone itself at declaration, once a host plane exists. The template holds no keys; its image digest and the configured machine's own digest are both recorded, the first as the agent's provenance on the run card. There is no Hosts page, and the console never manages a virtual machine as an object of its own.
- **One pane per SPU box.** A box stands for the models it holds: four models declared apart are four boxes, one decoder with its adapter is one box, and the operator chooses which. Each model row carries its function from the roster the framework charters (the decoder, exactly one; the classifier, optional), its artifact, its format and, on a decoder, its adapter. GGUF is the one format offered; the others are named as later and not offered. The box's GPU requirement sits in the box's pane: a card, a class (a family the capability table knows, or a list of models; the form typed is kept), or a pair in shard order. Start satisfies the requirement or refuses; it never substitutes.
- **The sampler pane** holds the tunable values a serving load requires at the pinned corpus commit, eight since 2026-11-12, every one required (Spec 6.1). Six are the sampler's defaults, which every generation carries and the record states: temperature, top-k, top-p, the repetition penalty, the repetition window and the seed. Two are the session's, fixed at load: the maximum tokens per turn, the ceiling each generation's own bound sits under, and the context capacity. A new form starts with five filled, the constants the agent ran with before the declaration carried them (temperature 0.7, top-k 40, top-p 0.95, repetition penalty 1.1, repetition window 64), each editable; the seed and the two session values start empty. Each field is refused at save outside the agent's range, naming it. What else is fixed at load, the sampler's mechanism among it, is shown read-only beside the model once an agent reports it, and until then the pane says what that waits on.
- **The store and gate pane** holds the store (`none` or `sqlite`) and the gate's tool election (no tool, or `sh`). Who the gate admits is its gate-con, filled at Provision, and is not asked here.
- **The prompt pane** holds the system prompt as its own field, never attached to a model. The declaration act writes it into declared state; from then on it is read from state and never from a file, and a change is a reload.

Two panes read the saved document back, beside the form:

- **The agent's file**: `agent.toml` in the agent's grammar at the pinned corpus commit, derived and never edited. What the screen does not declare is a comment naming what fills it, and what the grammar has no place for is a trailing block naming its interface question.
- **The prompt as it will be written**, with its waits-on line, or the word that there is none.

## The rules

- **A saved declaration is a versioned document.** Every save is a new row in its lineage, with a version and a last-save time; reopening a row reopens the same document. A save from a row that is not its lineage's head is refused.
- **The form refuses on its own, naming the field**, only what it can judge alone: a missing required field, a requirement that names nothing, and a field out of its shape (Spec 6.1 lists each). A refusal is in `fail`, at the field.
- **It warns, and saves all the same,** where a class spans more than one compute capability; it never refuses for it. The warning is in `hold`, at the requirement. A card model or a family the repository's table does not know is named "capability unknown" and never left blank.
- **Each field carries what it waits on before it takes effect**, as a line under the field in `dim`: Provision's register, the declaration reaching the agent, `validate`, the host plane. Spec 6.1 holds the table. Beside its label it carries when a mistake there is caught, `.when.now` or `.when.load` (the brand book's controls). A clean `validate` is acceptance for filing, not approval to load.
- **The screen reaches no agent and no host.** It writes the store and nothing else.
- **The path at the bottom of the pane** says where you are and what is left: the components, saved as a document; then the board, unbuilt; then Provision, which `validate` closes before Run opens.

## The board, when it comes

Components are compiled primitives the harness is pre-wired to. A wire is one ask and one return, written in Python by the operator. A function that makes several asks is a node the wires converge on. A wire edit is a new link in the lineage, not a reload of the same agent. None of this is drawn or built yet, and no page carries a wire or Python until the board's own act. The agent's edge, its two sockets and the two connectors outside it are drawn on the board when it is designed, not on the declaration screen.
