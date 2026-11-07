The masthead is the server's bar on every page: brand, the agent switcher, the server pages, the open-alert count, who is signed in and sign out. Beneath it, an availability banner for each plane the server cannot reach.

- The consumer provides the current agent's name (in `id`, inside `.agent-switch`), the pages the person's grants reach, the open-alert count and the person's name and highest role.
- The current page is `.here`: `fg` with an `accent` underline. Every other link is `dim`.
- The alert count is a `.status.fail` pill only when it is above zero; at zero it is not shown.
- The availability banner (`.unavailable`) is `accent-soft` on a `line` rule and names the agent, the plane's connector and since when, in the words the server uses: "agent unavailable, admin-con down", "turns unavailable, gate-con down".
- Do not put the stages here; they belong to the agent's stage rail.
- The served masthead carries no agent switcher and names no agent until multi-agent management; its alert count is `.masthead a.status`, a served-surfaces rule of `bundle.css` that keeps the link's padding and underline off the pill. Two more served rules keep the brand and the sign-out button on one line each, and at phone width (760px and under) let the masthead's row wrap, its links to a second line, rather than run past the page's edge.
