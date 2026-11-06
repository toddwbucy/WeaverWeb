Weaver-Web is the front end for an agent's whole lifecycle: declare it, provision it, run it, watch it, preserve it, diagnose it, retire it. One agent at a time, done to a finished standard, from pieces that do not assume one agent. This book is the one visual language and the one layout grammar every later surface uses: agents, state, analysis, chat, hardware.

## The first principle: the unit is the agent

Teach the difference, never hide it. A local developer arrives from LM Studio, Ollama and vLLM, where you pick a model, load it, get an endpoint and chat. Here an agent is a set of processes under a kernel-enforced user identity. It is declared, provisioned and loaded, and its trace is the artifact. The model is one field inside the declaration.

- Name the agent first, everywhere: the masthead's switcher, a page's `title`, a pane's header. The model appears as a field inside the decoder, in `id`, and never as the thing being managed.
- Say what kind of thing a word is: a verb is a verb (`load`, `unload`, `stop`), a state is a state (`unloaded`, `in transition`), a plane is a plane (gate, admin), a connector is a connector (gate-con, admin-con). Set each in `id`.
- There is no endpoint. The gate and admin are Unix sockets on the agent's edge, off the network. The network belongs to the two connectors outside the edge and to nothing else. The Declare diagram draws this; every other surface keeps the words.

## Standing positions, kept on every surface

- Weaver-Web is a consumer outside the agent's boundary. The server never reaches the agent; the connectors do. A pane that shows the agent names the plane and the connector it heard through, in its source line (`label`, `dim`): "as admin answered `show` at 14:02:11", "through admin-con, last heard 3 s ago".
- The browser is a display engine. It holds no key and no protocol state. Nothing is computed in the page that the server did not send; a re-fetch is a fetch, every 15 s, and says so in the pane's footer.
- Answers render as they came. A verb's answer is admin's JSON object verbatim, in `pre.raw`. A state word is the agent's word. A refusal's kind is its kind. Nothing is interpreted into a friendlier shape; a friendlier shape may stand beside it, never instead of it.
- A gap in a record surfaces as a mark, never smoothed. A discontinuity in a trace is a hatched row with its reason (`.stream li.mark`); an absent member is an `absent` pill; a replayed event says `replayed`.
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

## Navigation across the seven stages

The masthead (`.masthead`) is the server's: brand, the agent switcher (`.agent-switch`, the current agent's name in `id`), the server pages (Agents, Register, Persons, Grants, Passkeys), the open-alert count, who is signed in, sign out. Under it the stage rail (`.rail`) is the agent's: the seven stages in order, each a stage mark, the current one underlined in `accent`, and at the right what is left ("4 of 7, Preserve next"). The rail is the path: it always shows where you are and what remains, so a person never has to know the stages to find the next one.

The stages, in order, with what backs each today:

| stage | the pane kinds | backing today |
| --- | --- | --- |
| 1 Declare | the diagram with its tunables on the nodes, the configure pop-up, the saved `agent.yaml`, the form's own refusals | unbuilt |
| 2 Provision | staging on the host, the kernel account, the register row, the one-time config handover, then `validate`, which closes the stage and gates Run | partly: the register page and the handover are built; staging, SSH, the account and the validate step are not |
| 3 Run | the verb forms, the turn form, the load state | built; `load` from the front end is refused upstream for now |
| 4 Observe | availability, the two connectors, alerts, the live trace | built |
| 5 Preserve | `save-point`, `restore`, the decision on a missed save point | partly: both verbs are offered on the agent page where the role carries them and the ceiling grants them; no list of save points is served, and `restore` names none |
| 6 Diagnose | the diagnostic binding, the analysis feed, stepped replay, counterfactual branches | unbuilt; waits on weaver-analysis |
| 7 Retire | revoke a plane, retire the row, archive | partly: revoke and retire are built; archive is not |

Every pane is marked with its stage and its backing, in its header, so a person reading an unbuilt pane knows it is a drawing.

`validate` closes Provision and gates Run (ruling of 2026-11-06). It asks admin, through admin-con, whether every element the declaration names exists on the box and can load, so it has nothing to ask before the agent is provisioned. Declare keeps only what its form can refuse on its own: a missing required field, a piece with no configuration. A clean `validate` is acceptance to load, not a promise the load succeeds.

## Marks

A mark is a word in a pill, never a bare color and never a blank.

- Absence (`.absent`, dashed): `absent` (the record did not carry the member), `not served` (a seam with no party yet), `no state` (no `show` has answered), `none` (admin named none).
- Status (`.status`): `connected`, `live`, `loaded`, `answered` in `ok`; `in transition`, `waiting`, `replayed`, `decision standing` in `hold`; `not connected`, `revoked`, `retired`, `failed`, `lost contact` in `fail`. Each carries a dot and its word.
- Backing (`.backing`): `built` (solid, filled), `partly` (half filled), `unbuilt` (dashed). These say whether a pane's backing exists today and nothing about the agent.
- Query (`.pill.query`): a chip on Record is a query clause; clearing it widens the list in place.
- A discontinuity in a stream is a hatched row (`.stream li.mark`) carrying the mark's reason as it came: `link_outage`, `truncated`, `rotation`, `parse_failure`.

A zero, a blank cell and an empty string are values. None of them stands in for an absence.

## Controls

Every control is a real button, link, input, label or form. A verb is a form with one button in `.verb` (`control-h-verb`, `id`); the button's text is the verb's word. A verb that does not apply is disabled, not hidden, with a note saying why ("refused upstream", "not in the ceiling", "your roles do not carry it"). A turn is a `textarea` in `mono` and a `.verb` button reading "Place the turn". The focus ring is `focus`, 2px, offset 2px, on every control. Targets are 32px or taller, 36px where the action changes the agent.

A field in the Declare pop-up carries, beside its label, when a mistake there is caught: `.when.now` ("caught now", admin holds custody or can ask) or `.when.load` ("caught at load", only the organ can answer). The artifact is the clearest field of the second kind. A node's tunables (seed, temperature, a threshold, an adapter's scale) sit on the node itself as `.tunable` chips, so the diagram shows them without opening the pop-up; the pop-up is where they are edited.

## Iconography

None. No icon font, no emoji, no glyphs. The stage numeral, the status dot and the hatch are the whole set of non-text marks.

## Voice

Plain sentences, absolute dates, ASCII. Say what was measured and who said it. A page's lede says what the page is in one sentence. A refusal names the field. A note says why a control is disabled rather than hiding it.
