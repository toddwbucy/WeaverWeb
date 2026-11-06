# The Declare diagram

The diagram is the core interaction of stage 1 and the proof that the grammar holds for an unbuilt stage. It is a view of `agent.yaml`, Visio-style: drag a piece from the menu, click a piece to configure it in a pop-up. It is never a free canvas.

Take the experience from ComfyUI and drop the free wiring. The experience: a node graph a person composes by hand, a menu of pieces, click a node to configure it, the node's tunables visible on the node. Dropped: wire anything to anything, and a canvas with no visible destination. Here a wire attaches only where a contract permits, and the path under the diagram always says where you are and what is left.

The graph is a saved document. The diagram is `agent.yaml` and `agent.yaml` is the diagram: every save is a valid declaration, the document carries a version and a last-save time, and reopening the document reopens the same graph. The form refuses on its own only a missing required field and a piece with no configuration, naming the field. Everything else waits for `validate`, which closes Provision and gates Run: it asks admin whether every element the installed declaration names that admin holds custody of is on the box, never of a draft. A clean `validate` is acceptance for filing: nothing admin holds custody of stands in the way of a load, and what only the organ can judge is answered at admission. It is not a promise the load succeeds.

## What is drawn

- The agent's edge is a rounded rectangle in `fg`, 2px, `radius-md`, labelled with the agent's name and its kernel user in `id`. Everything inside the edge runs under that identity.
- On the edge sit two sockets in `inset` with an `fg` stroke: the gate (ordinary traffic) and admin (control). Both are Unix sockets. Neither is on the network. Configuring the gate sets which peer it admits, by kernel identity, never by port.
- Outside the edge sit two connectors as their own nodes: GateCon and AdminCon. They are fixed, never dragged from the menu: they are not declared in `agent.yaml` but minted at register, in Provision, and the diagram draws them so the edge is seen to be crossed by them alone. Network settings live on them only, drawn as the one `accent-soft` field in the diagram. They are separate nodes because access attaches per connector: today, per-agent `observer` and `operator` roles plus a server-wide `admin`. No access-control screen in this pass.
- Inside: the decoder on the input path with its system prompt attached, and the model as one field inside it. State management, today SQLite in memory only. Adding state pulls in a classifier, which can point at a LoRA adapter on the model.
- Each node carries its tunables as `.tunable` chips on the node: the decoder's seed and temperature, the classifier's threshold, the adapter's scale, state's memory flag. They are read on the node and edited in the pop-up.

## The rules

- Every state the diagram can reach is a valid declaration. A piece lands only where it may; a wire the contracts do not permit does not attach and says why, in `fail`, naming the contract.
- Adding a piece pulls in its requirements. The pulled-in piece appears with a dashed wire (`.wire.pulled`) until it is configured.
- Only pieces that exist today appear in the menu. The roster offers the organ kinds the framework charters, and carries no enumeration of its own: decoder (mandatory) and classifier (optional). A kind the framework will not load is never offered.
- The pop-up annotates each field with when a mistake there is caught: now, where admin holds custody or can ask synchronously, or at load, where only the organ can answer. A clean `validate` is acceptance for filing, not approval to load.
- The path at the bottom of the pane says where you are and what is left: Declare, saved as a document, then Provision, which `validate` closes before Run opens.
