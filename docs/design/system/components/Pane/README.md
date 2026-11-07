The pane is the unit of every surface. The consumer provides a stage, a title, a backing state, a source line and a body; the pane provides the frame.

- Header (`inset`): the stage mark, the title in `pane-title`, the backing badge, and at the right the source line in `label` and `dim` saying which plane, which connector, which verb and when. A pane with no source line is showing nothing from the agent.
- Body: a table (`.flush`), a form, a stream (`.flush`), a diagram or prose. A verb's answer is shown in `pre.raw` verbatim.
- Footer: how the body stays current, or what the pane waits on.
- A decision pane (`.pane.decision`, `hold` border) offers each option as its own form and never acknowledges the alert. It is pinned first in the main column. On a served page its prompt is a `<dialog open>` in the body, drawn in place as the body by a served-surfaces rule of `bundle.css`, not as the browser's floating box.

On a served page nothing in a pane is clipped by the frame's hidden overflow, by rules of `bundle.css`'s served-surfaces section: every body scrolls what is wider than it (a table's columns, a long select or pill), `.flush` being its padding alone; a header that does not fit wraps, its source line to a row of its own at narrow width, and a source line longer than its pane is wide wraps at any width; an identifier (an enrollment token, an agent id, a digest in a verb's answer) wraps anywhere, as `code`, `.mono` and `pre` in a pane and the lede do, and a `pre` wraps its lines; and a body's first and last child carry no margin of their own.

Do not nest panes. Do not float a pane over another. Do not give a pane a title that names a model where it means the agent.
