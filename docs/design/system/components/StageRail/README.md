The stage rail is the agent's path: the eight stages in order, each a stage mark with its backing badge, the current one underlined in `accent`, and at the right what is left. It sits under the masthead on every page about one agent and is the answer to "where am I and what remains".

- The consumer provides the current stage, and per stage whether its backing is `built`, `partly` or `unbuilt` today.
- Stages are always all eight and always in order; a stage is never hidden because it is unbuilt. Clicking an unbuilt stage opens its drawing with every pane marked `unbuilt`.
- The rail names the agent in its `aria-label`, never a model.
- The served rail is on the agent page alone, Run current, its backing a table the server keeps from `stages.md`; its marks are `.step` spans, not links, until a stage has a page of its own, and the drawing an unbuilt stage opens is a later act.
- The stage mark (`.stage`) is reused alone in every pane header: a boxed numeral in `stage-numeral` and the stage's name in `label`.
