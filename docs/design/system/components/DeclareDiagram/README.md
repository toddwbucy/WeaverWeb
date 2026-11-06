The Declare diagram is stage 1's working pane and the proof that the grammar holds for an unbuilt stage: a Visio-style view of `agent.yaml` where every reachable state is a valid declaration. It is never a free canvas.

- The consumer provides the saved declaration with its version and last save, the roster of pieces the framework charters today, each node's tunables, and for each field whether a mistake is caught now or at load.
- Three columns: the pieces menu and the `agent.yaml` view at the left, the diagram in the middle, the configure pop-up at the right. The pop-up opens on the selected node (`.node.selected`, `accent` stroke) and never over the diagram.
- The edge is `fg` at 2px; the sockets sit on it in `inset`; the connectors are fixed nodes outside it, not pieces of the menu, since they are minted at Provision and not declared; they carry the diagram's only `net` field in `accent-soft`.
- A node's tunables are `.tunable` chips on the node; the pop-up edits them.
- A wire the contracts do not permit is drawn in `fail`, dashed, with a `.refuse` box naming the contract. A piece pulled in by another arrives on a `.wire.pulled` until it is configured.
- The footer is the path: where you are, that the graph saves as `agent.yaml`, and that Provision ends with `validate`. ComfyUI's experience is kept, the node graph, the menu, click-to-configure, tunables on the node; its free wiring is dropped, and the destination is always visible.
- Nothing here reaches an agent. The form refuses on its own only a missing required field and a piece with no configuration. Drawing a declaration saves a document; `validate` closes Provision and gates Run, and is admin's word on whether every element of the installed declaration that admin holds custody of is on the box: acceptance for filing, not a promise the load succeeds.

The agent, kernel user, model and adapter names in the preview are sample data, not the shape of any box's accounts.
