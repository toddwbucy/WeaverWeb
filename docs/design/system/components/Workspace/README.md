The workspace is the composition every stage page uses: masthead, stage rail, a page title naming the agent, then panes in a grid. Nothing on it is a detached window.

- The consumer provides the agent, the current stage, and the panes in two lists: the main column and the side stack.
- `.grid.two` gives the main column two thirds and the side stack one third; at phone width the two stack. `.grid.three` is for a row of equal panes, such as the three connectors-and-alerts panes on Observe.
- The title is the agent's name and its box, in `title`; the lede is its id and the person's roles, in `id` and `dim`.
- Panes from another stage may sit in a side stack (Observe's connectors beside Run's verbs); each still carries its own stage mark, so the composition never blurs which stage a fact belongs to.
- Multi-agent management later composes the same panes under the agent switcher; no pane may assume it is the only agent.
- On a served page the title is `.workspace > h1` and the lede `.workspace > .lede`, a page of one short form (sign-in, enroll) is `.workspace.narrow`, and a grid's columns stack under 760px; these are rules of `bundle.css`'s served-surfaces section, since the preview sets them inline and the served pages carry no inline style.
