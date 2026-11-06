# Brief: the six themes in instrument.css

From the planner seat to the executor, 2026-11-06. PR #52 merged at `a68d367` and carries the operator's palette rulings of 2026-11-06 (PRD section 3.0, "The palette"): the purple of `assets/surfaces/instrument.css` was rejected as a default, and six themes were chosen, Slate the default. PR #52 left the write of the stylesheet to the planner; this brief is that write. Land it as `docs/project/brief-2026-11-06-the-six-themes-in-instrument-css.md`, the PR's first commit.

## Scope

- **Base:** `main` at `a68d367`.
- **Branch:** `code/the-six-themes-in-instrument-css`.
- **Commits:** `code: ...` for the stylesheet, `docs: ...` for section 3. One commit per section.
- **The diff touches `assets/surfaces/instrument.css`, the three document lines in section 3, and this brief. Nothing else.** No template changes, no new route, no theme switcher, no server behaviour: the pages stay plain (the operator's ruling of 2026-10-09, backend before presentation). The pane grammar of `docs/design/system/components/bundle.css` is a later act and is not this one.
- The operator's working rule of 2026-11-06 governs every edit: a document states the current state of its subject; a passage that no longer holds is replaced in place; git is the archive.
- ASCII only, no em-dash.
- Open a **draft** PR and tell the planner.

## 1. The palette

`assets/surfaces/instrument.css` lines 11-26 are the purple palette: a `:root` block and a `@media (prefers-color-scheme: light)` block. Replace them with the block at the end of `docs/design/system/palettes.md` (lines 80-137, inside its fence), **verbatim**: the `:root` Slate block, the `@media (prefers-color-scheme: light)` Mist block, and the six `:root[data-theme="..."]` blocks. That block is the operator's record of the six themes in the stylesheet's shape, every pair measured against the floors in PRD 3.0.

Then one block of the stylesheet's own, after the pasted block:

```css
:root {
  --sans: system-ui, -apple-system, "Segoe UI", sans-serif;
  --mono: ui-monospace, "SF Mono", "Menlo", "Consolas", monospace;
}
```

The two font stacks keep their current names, `--sans` and `--mono`, since every rule below reads them. The design system names them `--font-sans` and `--font-mono` in `tokens.css`; the rename belongs to the pane grammar act, not this one. Name this in the PR body.

What the pasted block adds that no rule reads yet: `--ok`, `--hold`, `--fail`. They stay. They are the status roles the ruling names, and the rule that reads them arrives with the first status word a page colors. Nothing in this PR colors one. Name this in the PR body too.

The `data-theme` blocks are inert until something sets the attribute on the root. Nothing does, and this PR adds nothing that does. They are the record's shape and they stay, so the stylesheet carries all six as `palettes.md` says it does. Name it.

Every rule from line 27 down (`* { box-sizing ... }` to `.planes`) is unchanged.

## 2. The header comment

Lines 1-10 narrate the retired conversation half: "The conversation half's `style.css` is served by `web/`'s router, which retires with that half ...". `web/` and `style.css` left in #50. Litter under the working rule. The header states the current state, in substance:

- the instrument's surfaces' own stylesheet, served by the surfaces' own route (`src/surfaces/mod.rs`, `/assets/surfaces/instrument.css`), a file of the binary (Spec section 6);
- the palette is the six themes of PRD section 3.0: Slate at `:root`, Mist for a browser preferring light, and the other four under `data-theme`; the hex values and measured pairs are in `docs/design/system/palettes.md`;
- hue is reserved for meaning: the accent is the one action hue, `ok`, `hold` and `fail` belong to status and stand beside their word, and emphasis is weight and luminance.

Cite PRD 3.0, not "the epic's ruling 11": section 3.0 now states the rule and the stylesheet reads the charter.

## 3. Three document lines

Each says the stylesheet is still to be written. State the current state.

- `docs/weaver-web-PRD.md:240-241`: "which also carries the six themes in the built stylesheet's shape for the planner to write into `instrument.css`" becomes "which also carries the six themes in the shape `instrument.css` carries them".
- `docs/design/README.md:123`: "the six themes in `instrument.css`'s shape for the planner" becomes "the six themes in `instrument.css`'s shape".
- `docs/design/system/palettes.md:77`: "The six themes in the built stylesheet's shape." becomes "The six themes as `instrument.css` carries them."

The PR body cites each against `a68d367`.

## Gates, in a fresh detached worktree of the pushed head

Run `git worktree prune` first; use a new worktree name with `|| exit 1` after the add; print `git log -1` inside it and check it is the PR head.

1. **The verbatim gate:** `diff <(sed -n '80,137p' docs/design/system/palettes.md) <(sed -n '<a>,<b>p' assets/surfaces/instrument.css)` is empty, where `<a>` to `<b>` are the pasted block's lines in the stylesheet. Give the line numbers and the empty diff in the PR body.
2. `grep -c -E 'c9527e|a63d68|1a161d|faf9fb' assets/surfaces/instrument.css` is 0: no purple value survives.
3. `grep -n -E 'web/|style.css|conversation half|epic' assets/surfaces/instrument.css` is empty.
4. The variables gate: every `var(--name)` the stylesheet reads is declared in it. `comm -23 <(grep -o 'var(--[a-z-]*' assets/surfaces/instrument.css | sed 's/var(//' | sort -u) <(grep -o -- '--[a-z-]*:' assets/surfaces/instrument.css | sed 's/://' | sort -u)` is empty.
5. `cargo fmt --check`; `cargo clippy --all-targets --locked`, and again with `--no-default-features`.
6. `cargo build --locked` (the stylesheet is `include_str!`, so the build is the parse of its path, not its content; say so).
7. The full suite with `DATABASE_URL` set, **no `skipped:`**, and `cargo test --doc`.
8. `DATABASE_URL=<never-migrated db> cargo test --locked --test startup -- --ignored`.
9. **Seen in a browser:** run the server against a scratch store, open `GET /sign-in` (no session needed) with the browser's color scheme dark and then light, and state in the PR body what the page's background and the accent on the masthead's `here` link were, read from the devtools computed style: `#15181d` and `#7cb8e0` dark, `#f3f5f8` and `#1c6ea6` light. The config, certificate and authority stay out of the repository.
10. ASCII check over the changed files, no em-dash.
11. `git diff --name-only main` lists the stylesheet, the three documents and this brief.
12. Clean up the scratch database and the worktree.

## The PR body

- Section 1: the verbatim gate's line numbers and empty diff; the three named facts (`--sans`/`--mono` keep their names until the pane grammar act; `--ok`/`--hold`/`--fail` declared and unread; the `data-theme` blocks inert and kept).
- Section 2: the litter in the header, named under the working rule.
- Section 3: the three lines with their citations.
- The browser readings of gate 9.
- **`Implements:`** PRD section 3.0, "The palette" (the stylesheet carries the six themes as ruled). Spec section 6 is unchanged: it names the asset and claims nothing about its content. No section 9 row is added or removed.
- End with the attribution line the session gives.
