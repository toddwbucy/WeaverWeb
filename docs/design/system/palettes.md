# Palettes, 2026-11-06

The built stylesheet's purple was rejected on 2026-11-06 as a default, not a ruling. From the proposals that followed, six themes were chosen the same day: three dark, Slate as the default with Graphite and Umber, and three light, Chalk, Mist and Paper. Each keeps the same roles, the same contrast rules and hue reserved for meaning: the accent is the one action hue, `ok`, `hold` and `fail` belong to status alone, and the neutrals carry at most a slight bias toward the accent. Every pair below was measured; the floors are 4.5:1 for text on `bg` and `panel`, 3:1 for `faint` and for the accent as a ring.

## The dark themes

| role | Slate | Graphite | Umber |
| --- | --- | --- | --- |
| bg | `#15181d` | `#121212` | `#191613` |
| panel | `#1b1f25` | `#191919` | `#201c18` |
| inset | `#22272f` | `#212121` | `#28231e` |
| line | `#303742` | `#2f2f2f` | `#38312a` |
| line-soft | `#272d36` | `#262626` | `#2e2823` |
| fg | `#e6e9ed` | `#e8e8e8` | `#ebe6df` |
| dim | `#9ba4ae` | `#a0a0a0` | `#a69d92` |
| faint | `#6c7580` | `#6e6e6e` | `#746b61` |
| accent | `#7cb8e0` | `#5fc6da` | `#62c4b3` |
| ok | `#8ad39c` | `#8ed49a` | `#8fd49b` |
| hold | `#e2ab4e` | `#e3aa49` | `#e4ab4a` |
| fail | `#ee8a7c` | `#ef8a7b` | `#ee8b7b` |

| measured pair | floor | Slate | Graphite | Umber |
| --- | --- | --- | --- | --- |
| fg on bg | 4.5 | 14.6 | 15.3 | 14.5 |
| dim on bg | 4.5 | 7.0 | 7.2 | 6.7 |
| dim on inset | 4.5 | 5.9 | 6.2 | 5.8 |
| faint on bg | 3.0 | 3.8 | 3.7 | 3.4 |
| accent as text on bg | 4.5 | 8.3 | 9.4 | 8.6 |
| accent as ring on panel | 3.0 | 7.7 | 8.9 | 8.1 |
| ok on panel | 4.5 | 9.4 | 10.1 | 9.7 |
| hold on panel | 4.5 | 8.0 | 8.5 | 8.2 |
| fail on panel | 4.5 | 6.7 | 7.2 | 6.9 |

- Slate, the default: neutrals biased toward blue, a sky accent. The nearest to the 2026-09-30 canvas's instrument look.
- Graphite: true neutral greys, a cyan accent. The quietest ground; the accent carries the whole identity.
- Umber: neutrals biased warm, a teal accent against them. The most distinct from every terminal theme.

Carbon (bg `#0e1013`, cobalt accent `#6ea4ff`) was proposed beside them and not chosen.

## The light themes

| role | Chalk | Mist | Paper |
| --- | --- | --- | --- |
| bg | `#f5f5f5` | `#f3f5f8` | `#f7f4ee` |
| panel | `#ffffff` | `#ffffff` | `#fffdf9` |
| inset | `#ebebeb` | `#e8ecf1` | `#eee9e0` |
| line | `#d2d2d2` | `#cdd4dc` | `#d8d0c4` |
| line-soft | `#e3e3e3` | `#dde2e9` | `#e5dfd4` |
| fg | `#1f1f1f` | `#1c2128` | `#25211c` |
| dim | `#5e5e5e` | `#59636e` | `#625a4f` |
| faint | `#858585` | `#7e8b98` | `#8a8176` |
| accent | `#0c7a8a` | `#1c6ea6` | `#0f7c6d` |
| ok | `#1e7a3a` | `#1e7a3a` | `#1e7a3a` |
| hold | `#87580a` | `#87580a` | `#87580a` |
| fail | `#b0392d` | `#b0392d` | `#b0392d` |

| measured pair | floor | Chalk | Mist | Paper |
| --- | --- | --- | --- | --- |
| fg on bg | 4.5 | 15.1 | 14.8 | 14.6 |
| dim on bg | 4.5 | 5.9 | 5.6 | 6.2 |
| dim on inset | 4.5 | 5.4 | 5.2 | 5.6 |
| faint on bg | 3.0 | 3.4 | 3.2 | 3.5 |
| accent as text on bg | 4.5 | 4.6 | 5.0 | 4.6 |
| accent as ring on panel | 3.0 | 5.0 | 5.5 | 5.0 |
| ok on panel | 4.5 | 5.4 | 5.4 | 5.3 |
| hold on panel | 4.5 | 6.1 | 6.1 | 6.0 |
| fail on panel | 4.5 | 6.1 | 6.1 | 6.0 |

- Chalk: true neutral, a deep cyan accent. Graphite's partner.
- Mist: cool blue-grey, a steel-blue accent. Slate's partner, and so the light a browser preferring light gets under the default.
- Paper: warm off-white, a deep teal accent. Umber's partner.

Frost (bg `#fafbfd`, cobalt accent `#2a5bd7`) was proposed beside them and not chosen. The status hues are one set across the lights, darkened to read as text on white, and one set across the darks, so a status reads the same under any theme.

## For instrument.css

The six themes in the built stylesheet's shape. Slate is `:root`; a browser preferring light gets Mist, Slate's partner, which is the one election here the ruling did not make; a `data-theme` on the root names any of the six. `focus` is `var(--accent)`.

```css
:root {
  --bg: #15181d; --panel: #1b1f25; --inset: #22272f;
  --line: #303742; --line-soft: #272d36;
  --fg: #e6e9ed; --dim: #9ba4ae; --faint: #6c7580;
  --accent: #7cb8e0; --accent-soft: #7cb8e02e;
  --ok: #8ad39c; --hold: #e2ab4e; --fail: #ee8a7c;
}
@media (prefers-color-scheme: light) {
  :root {
    --bg: #f3f5f8; --panel: #ffffff; --inset: #e8ecf1;
    --line: #cdd4dc; --line-soft: #dde2e9;
    --fg: #1c2128; --dim: #59636e; --faint: #7e8b98;
    --accent: #1c6ea6; --accent-soft: #1c6ea622;
    --ok: #1e7a3a; --hold: #87580a; --fail: #b0392d;
  }
}
:root[data-theme="slate"] {
  --bg: #15181d; --panel: #1b1f25; --inset: #22272f;
  --line: #303742; --line-soft: #272d36;
  --fg: #e6e9ed; --dim: #9ba4ae; --faint: #6c7580;
  --accent: #7cb8e0; --accent-soft: #7cb8e02e;
  --ok: #8ad39c; --hold: #e2ab4e; --fail: #ee8a7c;
}
:root[data-theme="graphite"] {
  --bg: #121212; --panel: #191919; --inset: #212121;
  --line: #2f2f2f; --line-soft: #262626;
  --fg: #e8e8e8; --dim: #a0a0a0; --faint: #6e6e6e;
  --accent: #5fc6da; --accent-soft: #5fc6da2e;
  --ok: #8ed49a; --hold: #e3aa49; --fail: #ef8a7b;
}
:root[data-theme="umber"] {
  --bg: #191613; --panel: #201c18; --inset: #28231e;
  --line: #38312a; --line-soft: #2e2823;
  --fg: #ebe6df; --dim: #a69d92; --faint: #746b61;
  --accent: #62c4b3; --accent-soft: #62c4b32e;
  --ok: #8fd49b; --hold: #e4ab4a; --fail: #ee8b7b;
}
:root[data-theme="chalk"] {
  --bg: #f5f5f5; --panel: #ffffff; --inset: #ebebeb;
  --line: #d2d2d2; --line-soft: #e3e3e3;
  --fg: #1f1f1f; --dim: #5e5e5e; --faint: #858585;
  --accent: #0c7a8a; --accent-soft: #0c7a8a22;
  --ok: #1e7a3a; --hold: #87580a; --fail: #b0392d;
}
:root[data-theme="mist"] {
  --bg: #f3f5f8; --panel: #ffffff; --inset: #e8ecf1;
  --line: #cdd4dc; --line-soft: #dde2e9;
  --fg: #1c2128; --dim: #59636e; --faint: #7e8b98;
  --accent: #1c6ea6; --accent-soft: #1c6ea622;
  --ok: #1e7a3a; --hold: #87580a; --fail: #b0392d;
}
:root[data-theme="paper"] {
  --bg: #f7f4ee; --panel: #fffdf9; --inset: #eee9e0;
  --line: #d8d0c4; --line-soft: #e5dfd4;
  --fg: #25211c; --dim: #625a4f; --faint: #8a8176;
  --accent: #0f7c6d; --accent-soft: #0f7c6d22;
  --ok: #1e7a3a; --hold: #87580a; --fail: #b0392d;
}
```
