# Declare: the setup pages

Declare is a start page, the setup pages and the build page, on the operator's rulings of 2026-11-16 (setup pages) and their addendum, which replace the two screens of the rulings of 2026-11-11 as the stage's path. The start page asks the agent's name. The setup pages run the pairing, the models, the drives, the placement and the confirmed deployment tuple, in that order, on their addendum. The build page holds the board, where the operator wires what was declared and sees only that, and the prompt tuple panel, both held. Spec section 6.1 carries the pages. An agent is binaries by hash plus wires by hash, and the setup pages and the board are a rendering of that card. The trace is the record and not a component, so no page declares it.

## The setup pages

None of the pages is built. The act that builds them ends at the confirm page, a confirmed deployment tuple, so the first agent can be declared and set up through Confirm, and a load refuses it until the prompt tuple carries the six sampling values. Every page is panes under the pane grammar, each pane carrying its stage, its backing and its status, healthy states included.

- **Before the start page**, the console connects the operator's Hugging Face account once, as a console setting. The setting accepts a read-only or a read-write token, and the console never writes to Hugging Face except on an explicit push in Train.
- **The start page** asks the agent's name, one field.
- **The pairing.** The page shows the box waiting, which dialed this server from its setup. The operator compares the fingerprint the box shows with this server's own and enters the short one-time code the box shows, and the running report of the box's survey follows. A checkbox says whether the box is a virtual machine or a host, and the page warns where the address is localhost and the checkbox says virtual machine.
- **The models.** A Hugging Face repository, then a pick from its files, a split model shown as one choice with all its parts, and the precision read from the file. Hugging Face is the one source, and a local source is later. What the first agent does not need, extra models, adapter roles beyond the one decoder adapter, a classifier-only agent and placement on the CPU, is drawn and marked unbuilt, never live, so a page never accepts what a load refuses.
- **The drives.** On an appliance, once the model is picked, the page shows the command to run at the appliance's console, which starts its drive setup, then the state drive, at least 10 GiB, which holds the agent account's home and in it the agent's own folder (its declaration, logs, trace and save points), and the models drive, sized from the chosen files, as the box mounted them at the state mount and the models mount. On a host, which attaches and formats nothing, the page shows the state directory and the models directory the operator named, the state directory writable with at least 10 GiB free and the models directory readable, with no separate disk asked, on the operator's addendum. The models directory is prefilled with that user's default Hugging Face cache, so models already pulled there are found, and beside it the page shows a command for the operator granting this server's account read-write on that cache and a models group read. This server alone talks to Hugging Face and writes there, and agents on the machine read the model in place, read-only, through the group, since several may share one model. The host form is drawn and marked unbuilt, since the host's own script is later.
- **The placement.** Each model on a device, and the context length chosen against the model's maximum, read from the file's header. The fit is called planned, from the room plan without a load, since only a load measures. "In shard order" names the layer split across cards and says so.
- **Confirm.** The deployment tuple in full: the model's repository, commit and per-part digests, the adapter likewise, the image version, the driver, the cards by identity, the guest CPU, the machine, the drives, the placement and the context length. Confirm goes to the build page.
- **The build page** is a one-pane stub marked unbuilt until the board and the prompt tuple panel come.

One unit, GiB, serves every size in the flow.

## The declaration screen, as built

The declaration screen of the rulings of 2026-11-11 is built and stands until the setup pages' act recuts Declare (Spec 6.1). The screen is a form of panes, under the pane grammar every surface uses, and never a canvas. Nothing on it is dragged or placed.

- **The agent pane** holds the name, which names the lineage and never changes within it.
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
- **Each field carries what it waits on before it takes effect**, as a line under the field in `dim`: Provision's register, the declaration reaching the agent, `validate`. Spec 6.1 holds the table. Beside its label it carries when a mistake there is caught, `.when.now` or `.when.load` (the brand book's controls). A clean `validate` is acceptance for filing, not approval to load.
- **The screen reaches no agent and no host.** It writes the store and nothing else.
- **The path at the bottom of the pane** says where you are and what is left: the components, saved as a document; then the board, unbuilt; then Provision, which `validate` closes before Run opens.

## The board, when it comes

Components are compiled primitives the harness is pre-wired to. A wire is one ask and one return, written in Python by the operator. A function that makes several asks is a node the wires converge on. A wire edit is a new link in the lineage, not a reload of the same agent. None of this is drawn or built yet, and no page carries a wire or Python until the board's own act. The agent's edge, its two sockets and the two connectors outside it are drawn on the board when it is designed, not on the declaration screen.
