Weaver-Web is the front end for an agent's whole lifecycle: declare it, provision it, run it, watch it, preserve it, diagnose it, train it, retire it. One agent at a time, done to a finished standard, from pieces that do not assume one agent. This book is the one visual language and the one layout grammar every later surface uses: agents, state, analysis, chat, hardware.

## The first principle: the unit is the agent

Teach the difference, never hide it. A local developer arrives from LM Studio, Ollama and vLLM, where you pick a model, load it, get an endpoint and chat. Here an agent is a set of processes under a kernel-enforced user identity. It is declared, provisioned and loaded, and its trace is the artifact. The model is one field inside the declaration.

- Name the agent first, everywhere: the masthead's switcher, a page's `title`, a pane's header. The model appears as a field inside the decoder, in `id`, and never as the thing being managed.
- Say what kind of thing a word is: a verb is a verb (`load`, `unload`, `stop`), a state is a state (`unloaded`, `in transition`), a plane is a plane (gate, admin), a connector is a connector (gate-con, admin-con). Set each in `id`.
- There is no endpoint. The gate and admin are Unix sockets on the agent's edge, off the network. The network belongs to the two connectors outside the edge and to nothing else. Declare's board will draw this when it is designed; every other surface keeps the words.

## Standing positions, kept on every surface

- Weaver-Web is a consumer outside the agent's boundary. The server never reaches the agent; the connectors do. A pane that shows the agent names the plane and the connector it heard through, in its source line (`label`, `dim`): "as admin answered `show` at 14:02:11", "through admin-con, last heard 3 s ago".
- The browser is a display engine. It holds no key and no protocol state. Nothing is computed in the page that the server did not send; a re-fetch is a fetch, every 15 s, and says so in the pane's footer.
- Answers render as they came. A verb's answer is admin's JSON object verbatim, in `pre.raw`. A state word is the agent's word. A refusal's kind is its kind. Nothing is interpreted into a friendlier shape; a friendlier shape may stand beside it, never instead of it.
- A gap in a record surfaces as a mark, never smoothed. A discontinuity in a trace is a hatched row with its reason (`.stream li.mark`); an absent member is an `absent` pill; a replayed event says `replayed`: the window carries the listener's verdict on every event, replayed or live against the opening's boundary, and the agent page's Live trace pane renders it as a `dim` row with `replayed` in `hold`.
- A missing contract is an ask upward, never a workaround. A pane whose backing is not built says so with the `unbuilt` badge and names the contract or the act it waits on, in its footer.

## Color

Set the page in `bg`, every pane in `panel` bordered by `line`, and a pane's header, a table head and an input's field in `inset`. Text is `fg`; secondary text is `dim`; a note under a control is `faint`, and because `faint` is below 4.5:1 in both themes, a fact a person must act on is never set in it.

Six themes stand, chosen on 2026-11-06 from the proposals in Palettes: Slate is the default, Graphite and Umber the other darks, and Chalk, Mist and Paper the lights. Every rule in this section holds under each of them.

Hue is reserved for meaning. `accent` is the one action hue: the current stage's underline, the focus ring, a primary button's border, a link under the pointer, the network field on a connector node. `ok`, `hold` and `fail` belong to status and to nothing else, and each always stands beside its word: `connected` in `ok`, `in transition` in `hold`, `revoked` in `fail`. Green and red are never the only difference between two things. No stage, no pane kind and no backing state owns a hue: those are marks (below), told apart by weight, outline and luminance.

Every theme keeps each token's role and lightness order. Slate is the default; a browser preferring light gets Mist, Slate's partner; a theme control, when one is added, names the six in the masthead with auto beside them.

## Type

Two families, no files: `sans` is the system stack for text and `mono` is the system monospace for every identifier. Set a page's one title in `title`, a pane's title in `pane-title`, prose in `body`, the line under a title in `lede` and `dim`, navigation in `nav`, table cells and stream rows in `cell`, a table head or a field's label in `label`.

Everything that came from the agent or the store is an identifier and is set in `id`: agent ids, run ids, process ids, verbs, state words, fingerprints, paths, raw values. The text inside a mark is `pill`. A stage mark's numeral is `stage-numeral` in a boxed square.

Emphasis is weight and luminance: 600 for a title, 500 for a label, `dim` to recede. No italics, no all-caps, no letterspacing.

## Spacing, radius, size

Everything sits on the 4px scale. A pane header pads `space-2` `space-3`; a pane body pads `space-4`; panes are `space-4` apart; the workspace pads `space-6` at the sides and `space-12` at the bottom. A control is `control-h` tall, and `control-h-verb` where it changes the agent. Panes and dialogs take `radius-md`; controls and diagram nodes `radius-sm`; every mark `radius-pill`.

## The pane grammar

Every surface is composed from panes in a grid, never from detached windows. A pane is `.pane`: a header on `inset`, a body, an optional footer.

- The header carries, left to right: the stage mark (`.stage`: numeral and stage name), the title (`pane-title`), the backing badge (`.backing`: `built`, `partly` or `unbuilt`), and at the right the source line (`.source`): which plane, which connector, which verb, when.
- The body is a table, a form, a stream, a diagram or prose. A table or stream body is `.flush`.
- The footer says how the body stays current ("re-fetched every 15 s") or what the pane waits on ("waits on `weaver-analysis-web-contract`").
- A decision pane (`.pane.decision`) is a pane whose border is `hold`. It is pinned at the top of the main column and offers each option as its own form; it never acknowledges the alert it answers. Today it is a `<dialog open>`; the grammar keeps it in the flow.

Compose with `.grid.two` (a main column and a side stack) for a stage's working view, `.grid.three` for a row of equal panes, and `.stack` inside a column. Nothing floats, nothing overlaps, and the pane that owns a fact is the one whose header names its source.

## Navigation across the eight stages

The masthead (`.masthead`) is the server's, in the path's order (the operator's word of 2026-11-13): brand, the agent switcher (`.agent-switch`, the current agent's name in `id`), then on the left the work in the order a person does it, each page carrying the stage marks it serves (Declarations, 1 Declare; Register, 2 Provision; Agents, 3 Run and 4 Observe, with the open-alert count), then Record, last and unmarked, the record of the whole lifecycle and no stage's own; on the right, who is signed in, Persons and Grants for an admin, Passkeys, sign out. A person sees the same order less what they may not open. Under it the stage rail (`.rail`) is the agent's: the eight stages in order, each a stage mark, the current one underlined in `accent`, and at the right what is left ("4 of 8, Preserve next"). The rail is the path: it always shows where you are and what remains, so a person never has to know the stages to find the next one.

The stages, in order, with what backs each today:

| stage | the pane kinds | backing today |
| --- | --- | --- |
| 1 Declare | two screens (rulings of 2026-11-11): the declaration screen, its panes for the agent, each SPU box with its models and GPU requirement, the tunable values a serving load requires, the store and gate, the prompt, the derived `agent.toml`, the version and the form's own refusals and warning; then the board, which wires what was declared | partly: the declaration screen is built (Spec 6.1); the board waits on WeaverAgent's contracts |
| 2 Provision | the operator's work on the box, which this server never reaches (staging the binaries, the kernel account, installing the file derived from the declaration Declare saved), then the register row, the one-time config handover, then `validate`, which reads the installed declaration, closes the stage and gates Run | partly: the register page and the handover are built; the box steps and the validate step are not |
| 3 Run | the verb forms, the turn form, the load state | built; `load` is offered as every verb is, where the person's grants and the connection's ceiling carry it |
| 4 Observe | availability, the two connectors, alerts, the live trace | built |
| 5 Preserve | `save-point`, `restore`, the decision on a missed save point | partly: both verbs are offered on the agent page where the role carries them and the ceiling grants them; no list of save points is served, and `restore` names none, carrying no argument (the invoker's fixed line `sudo -n <weaver_admin> <verb> <agent>`, Spec 7.2) |
| 6 Diagnose | the diagnostic binding, a reload with no gate (WeaverAgent's `diagnostic-replay-loop` section 1: under a diagnostic binding there is no gate; `weaver-diagnostic-PRD` section 3), the analysis feed, stepped replay, counterfactual branches | unbuilt; waits on weaver-analysis |
| 7 Train | the selection from the trace, the dataset, the training run, the adapter handed back to Declare | unbuilt; training runs in WeaverTrain, reached through a contract that does not exist yet |
| 8 Retire | revoke a plane, retire the row, archive | partly: revoke and retire are built; archive is not |

Every pane is marked with its stage and its backing, in its header, so a person reading an unbuilt pane knows it is a drawing.

The trace is the record of the whole lifecycle and no stage's own (ruling of 2026-11-07): Observe reads it live, Diagnose and Train read it landed. Train selects from it, builds a dataset from the selection, trains an adapter and hands it back to Declare; gathering is part of Train; no frontier-model or reviewer text enters a dataset; the adapter enters through Declare and Provision at the load boundary.

`validate` closes Provision and gates Run (ruling of 2026-11-06). It asks admin, through admin-con, whether every element the installed declaration names that admin holds custody of is on the box, so it has nothing to ask before the agent is provisioned. Declare keeps only what its form can refuse on its own: a missing required field, a requirement that names nothing and a field out of its shape (Spec 6.1). A clean `validate` is acceptance for filing: nothing admin holds custody of stands in the way of a load, and what only the organ can judge is answered at admission. It is not a promise the load succeeds.

## Marks

A mark is a word in a pill, never a bare color and never a blank.

- Absence (`.absent`, dashed): `absent` (the record did not carry the member), `not served` (a seam with no party yet), `no state` (no `show` has answered), `none` (admin named none).
- Status (`.status`): `connected`, `live`, `loaded`, `answered` in `ok`; `in transition`, `waiting`, `replayed`, `decision standing` in `hold`; `not connected`, `revoked`, `retired`, `failed`, `lost contact` in `fail`. Each carries a dot and its word.
- Backing (`.backing`): `built` (solid, filled), `partly` (half filled), `unbuilt` (dashed). These say whether a pane's backing exists today and nothing about the agent.
- Query (`.pill.query`): a chip on Record is a query clause; clearing it widens the list in place.
- A discontinuity in a stream is a hatched row (`.stream li.mark`) carrying the mark's reason as admin-con or the server wrote it, a sentence naming what was not relayed and why: a record past the relay's bound, a backfill begun inside the file, a file truncated, rewritten or replaced, admin-con reconnected, the link to admin-con lost (`src/link/admin_con.rs`, `src/link/listener.rs`).

A zero, a blank cell and an empty string are values. None of them stands in for an absence.

## Controls

Every control is a real button, link, input, label or form. A verb is a form with one button in `.verb` (`control-h-verb`, `id`); the button's text is the verb's word. A verb that does not apply is disabled, not hidden, with a note saying why, one of three: "not in your roles", "admin-con not connected", "above the ceiling". A turn is a `textarea` in `mono` and a `.verb` button reading "Place the turn". The focus ring is `focus`, 2px, offset 2px, on every control. Targets are 32px or taller, 36px where the action changes the agent.

A field on the declaration screen carries, beside its label, when a mistake there is caught: `.when.now` ("caught now", admin holds custody or can ask) or `.when.load` ("caught at load", only the organ can answer). The artifact is the clearest field of the second kind. Under the label, in `dim`, the field says what it waits on before it takes effect (Spec 6.1). Every field is edited in place on the screen; there is no pop-up.

## Iconography

None. No icon font, no emoji, no glyphs. The stage numeral, the status dot and the hatch are the whole set of non-text marks.

## Voice

Plain sentences, absolute dates, ASCII. Say what was measured and who said it. A page's lede says what the page is in one sentence. A refusal names the field. A note says why a control is disabled rather than hiding it.
