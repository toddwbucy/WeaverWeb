# The frontend design, 2026-09-30

This directory holds the design of the Weaver-Web surfaces as drawn on 2026-09-30 in a
Claude Design canvas, together with the decisions the drawing made. The canvas is the
working copy for visual review:

    https://claude.ai/artifact/4SZBkNM5rTeNCagPoJy79V

`canvas/` is that canvas's own files, copied whole: `canvas.json` is the index (one
entry per artboard with its frame on the canvas), and each `*.dc.html` is one
artboard's complete source. They are self-contained HTML in the canvas editor's
component format. A browser opens them as pages, but the `{{hole}}` bindings and
`<sc-for>` loops render only inside that editor, so read them as source and review
them on the canvas. Nothing here is served by the crate.

## What is drawn

Six surfaces, each in a light and a dark artboard, at 1440 px wide:

| artboard | surface | what it shows |
| --- | --- | --- |
| `Main` | Open a trace (HeroBench Replay), the operator's "Replay surface"; act 10 builds it | a landed run as a replay: the tuple strip, the agent-visible map with the fog toggle, the turns with the interview in or out, one position's alternatives (read 1), the entropy and surprisal timeline (read 2) |
| `Live` | Live | an agent playing now: state and the lifecycle verbs over admin-con, turns as they land, score by task, the interview composer under an interrupt identifier |
| `Record` | Record | every run and branch with its tuple, chips as query clauses (read 5) |
| `Matrix` | the ablation matrix | one parent's arms side by side: swept value, parting position, outcome, exemplar tags (reads 4 and 6) |
| `Agents` | Agents | the roster of registered agents with their client, key fingerprint, link and load state, and the four-step registration of a client |
| `Foundations` | none | the palette, type, the absence vocabulary, control states, and the table of all eight surfaces with what each reads and reaches |

`Main` is the surface act 10 builds (brief
`docs/project/brief-2026-10-06-act-10-the-replay-surface.md`), named Open a trace as the
PRD's section 3.4 names it. The other five are future acts.

Run identifiers, turn text and scores on the boards are sample data in the shape of
the HeroBench deposits of 2026-09-29 and 2026-09-30. None is a reading from the store.

## Elections made in the drawing

- **One look, a research instrument.** IBM Plex Sans for text and IBM Plex Mono for
  every identifier, token, position and tuple value. A near-white ground with white
  panels; one action color (a teal-blue) for links, buttons and the surprisal series;
  amber for the entropy series and the quiesced state; green and red for pass and fail,
  each always carrying its word. No gradients, no emoji, no icon fonts.
- **Absence is a word, never a blank.** The Spec's section 6 rule is drawn as a dashed
  outline pill carrying one of four words: `absent` (the record did not carry the
  member), `no run` (a staged value that never produced a run), `not served` (a seam
  with no party yet, today the classifier's labels and the full map), `uncomputable`
  (the SPU's identity sentinel). A fifth word joined on 2026-10-06 with act 10b,
  `unknown` (a member not yet derivable, today a branch's parting position where the
  walk did not run), drawn the same way; the boards predate it and do not show it.
  A stub-answered connector shows as a `stub` badge in the header of every surface.
  Surprisal on a prefix where its election did not stand is drawn as a hatched column,
  not a zero bar.
- **Dark mode, both ways.** The header of every surface carries an auto / light / dark
  control. Auto follows the browser's `prefers-color-scheme`; light or dark is an
  override persisted on the session row, so a page never flashes the wrong theme on
  load. The dark palette keeps each color's role and lightness order: the two series
  still differ in lightness, primary buttons flip to a light fill with dark text, and
  the absence pill keeps its dashed outline. The dark hex values are in
  `Foundations-dark.dc.html`.
- **A chip is a query.** Record's filter chips are clauses; clearing one widens the
  list in place. No surface exists only as a pre-applied chip.
- **A verb that does not apply is disabled, not hidden.** Load state is admin's own
  word over the client, never inferred from the link being up.
- **The session pill says what the gate can prove.** `claimed, not proven`, until the
  credential model lands.

## The ruling the Agents board draws, 2026-09-30

The operator ruled on 2026-09-30, in the design session, that **web-con and admin-con
are Weaver-Web's**, not WeaverTools'. Each agent is its own user of the frontend. A
client runs on the agent's box beside it, holds the only reach to the gate and admin
sockets, and dials out to the Weaver-Web server under a key the server issued. The
deployment story is: deploy the agent; deploy Weaver-Web anywhere on the network; run
the client's install script on the agent's box with the key; the agent is then
registered with that server and controlled from the frontend.

This reverses part of the handoff's step 1 decision and two lines of `CLAUDE.md`,
and the reversal is recorded here first so the seed is not deleted before it is:

- The tree already holds this client. `src/bin/weaver-web-connector.rs` runs on the
  agent's box as the operator's uid, holds every socket reach, stores nothing, and only
  dials out. `src/wire.rs` is the link: one TCP connection carrying NDJSON frames, a
  hello with the box's agent roster, then turns, verbs, status, declarations and the
  trace streamed unasked. `ServerConfig` already has `link_listen`. The handoff's step
  1 listed all of it as leaving because it reaches the agent; under this ruling the
  Weaver-Web **server** still never reaches an agent, the **client** is the one party
  that does, and the client is this repository's. So those files are the seed of
  web-con and admin-con, not the half to remove.
- What the seed lacks, and the Agents board adds: the hello today carries agent names
  and nothing else, the first hello wins a name, and the link is plain TCP. The design
  has a key issued server-side and stored only as a fingerprint (the digest posture the
  session gate already uses), TLS on the link with the server's certificate pinned in
  the client's config, a hello refused before its roster is read when its key is not
  live, revocation one client at a time, and an install script that writes the client
  config and never lands in a repository.
- The handoff's decision section, `CLAUDE.md` ("the connectors live in WeaverTools",
  "do not extend it") and the Spec's section 7 are owed an amendment saying this. That
  is a separate `docs:` act after the operator confirms the wording.

The third arrow on the whiteboard, weaver-analysis into the store, is untouched.

**Superseded by acts 1 to 9** (PRs #3 to #21, 2026-10-01 to 2026-10-06), which built
what this section asked for and more. The seed's link (`src/wire.rs`,
`src/bin/weaver-web-connector.rs`, plain TCP) left the tree; in its place stand the
register of agents with two server-minted credentials per agent, a mutually
authenticated link, and two connectors, gate-con and admin-con (web-con is the
whiteboard's name for gate-con). The Spec's sections 7 and 8 and `CLAUDE.md` are the
text. The section above stays as the dated record of the ruling, and the design's
drawing of Agents stands.

## Reviewing and changing the design

Review on the canvas; comments there reach the design session. To change an artboard,
edit its file under `canvas/` and republish it to the canvas, or edit on the canvas and
copy the file back. Keep the two in step by committing after each round. The files in
`canvas/` are the record; the canvas is the working copy.
