-- Spec 2.12 and 8: the register of agents gains two observed members.
--
-- **0010 is frozen from here**: an operator may have run it, and sqlx refuses
-- a database whose applied migration changed, so what the act that built
-- admin-con adds to the row lands as its own migration.
--
-- **The ceiling admin-con declared**, the verbs its role on the box grants as
-- its hello named them, with the date. It is the copy surfaces read: the
-- listener authorizes every ask against the live connection's own ceiling,
-- held with that connection for its life, and never against this column.
-- Written by the admission that admitted the connection, under the row's
-- lock, so it is the ceiling of the connection the row last recorded
-- connected.
--
-- **Which source the tuple and the load state stand on**, a `show` answer
-- or a live trace event, written in the same observation that sets them and
-- ordered with them on the listener's arrival sequence, so a surface can say
-- a state is unconfirmed since the last admission where the ceiling grants
-- no `show`. Its date is the load state's.

ALTER TABLE agent
  ADD COLUMN admin_ceiling     TEXT[],
  ADD COLUMN admin_ceiling_at  TIMESTAMPTZ,
  ADD COLUMN state_source      TEXT,
  ADD CONSTRAINT agent_ceiling_is_dated
    CHECK ((admin_ceiling IS NULL) = (admin_ceiling_at IS NULL)),
  ADD CONSTRAINT agent_state_source_is_one_of
    CHECK (state_source IS NULL OR state_source IN ('show', 'event'));
