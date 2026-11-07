The pane is the unit of every surface. The consumer provides a stage, a title, a backing state, a source line and a body; the pane provides the frame.

- Header (`inset`): the stage mark, the title in `pane-title`, the backing badge, and at the right the source line in `label` and `dim` saying which plane, which connector, which verb and when. A pane with no source line is showing nothing from the agent.
- Body: a table (`.flush`), a form, a stream (`.flush`), a diagram or prose. A verb's answer is shown in `pre.raw` verbatim.
- Footer: how the body stays current, or what the pane waits on.
- A decision pane (`.pane.decision`, `hold` border) offers each option as its own form and never acknowledges the alert. It is pinned first in the main column.

On a served page the body's first and last child carry no margin of their own, and a `.flush` body scrolls a table wider than the pane rather than clipping it, both rules of `bundle.css`'s served-surfaces section.

Do not nest panes. Do not float a pane over another. Do not give a pane a title that names a model where it means the agent.
