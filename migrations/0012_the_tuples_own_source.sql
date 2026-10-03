-- Spec 2.12: the tuple gains its own source.
--
-- **0011 is frozen from here**, as 0010 was before it: an operator may have
-- run it, and sqlx refuses a database whose applied migration changed.
--
-- **The tuple's source is its own**, a `show` answer or a live trace event.
-- A turn's start or close moves the load state and keeps the tuple, so one
-- source member for both would relabel a `show`-shaped tuple as an event's
-- the moment a turn began, and the source is how a reader knows the opaque
-- tuple's shape. Written only by an observation that writes the tuple, and
-- dated with it. `state_source` from here names the load state's source
-- alone. Existing rows are backfilled from `state_source`, which until now
-- named both and so was the tuple's too.

ALTER TABLE agent
  ADD COLUMN tuple_source TEXT,
  ADD CONSTRAINT agent_tuple_source_is_one_of
    CHECK (tuple_source IS NULL OR tuple_source IN ('show', 'event'));

UPDATE agent SET tuple_source = state_source WHERE state_source IS NOT NULL;
