**The board's earlier drawing, owed a redraw.** This preview drew Declare as one diagram (design session of 2026-11-06). The operator's rulings of 2026-11-11 make Declare two screens: the declaration screen, a form of panes under the pane grammar with no component of its own (`../../declare.md`, Spec 6.1), and the board, which wires what was declared and is unbuilt. The preview stands as the board's earlier drawing, kept for its grammar (the edge, the sockets, the connectors outside it, the refused wire), until the board's own act redraws it. What the rulings replace in it:

- the configure pop-up and the system prompt attached to the decoder: the prompt is the declaration screen's own field, written into declared state, and every field is edited in place;
- the free placement of pieces from a menu: the components are declared on the declaration screen, and the board shows only what was declared;
- the `agent.yaml` view: the agent's file is `agent.toml`, derived from the saved declaration and shown read-only;
- the state piece and the tunables as chips on the nodes: the store is a declared field, and the sampler's knobs are per-turn values whose defaults the declaration supplies;
- a wire drawn between organs: on the board a wire is one ask and one return written in Python by the operator, and none is drawn yet.

What the preview still shows that holds:

- The edge is `fg` at 2px; the sockets sit on it in `inset`; the connectors are fixed nodes outside it, minted at Provision and not declared, carrying the drawing's only `net` field in `accent-soft`.
- A wire the contracts do not permit is drawn in `fail`, dashed, with a `.refuse` box naming the contract.
- Nothing here reaches an agent. `validate` closes Provision and gates Run, and is admin's word on whether every element of the installed declaration that admin holds custody of is on the box: acceptance for filing, not a promise the load succeeds.

The agent, kernel user, model and adapter names in the preview are sample data, not the shape of any box's accounts.
