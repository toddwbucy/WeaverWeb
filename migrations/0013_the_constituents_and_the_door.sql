-- Spec 2.12: the register of agents gains the run's constituents and the
-- trace door's state.
--
-- **0012 is frozen from here**, as every applied migration is: an operator
-- may have run it, and sqlx refuses a database whose applied migration
-- changed.
--
-- **The run's constituents as the last `show` named them**, by process id,
-- with that answer's date: a fact for the operator and for the install's
-- one-time containment check, which drives nothing in admin-con. Written by
-- every `show` answer that lands, in the same observation that writes the
-- load state and ordered with it; null where the answer named none, which
-- is where no run holds the agent's run lock.
--
-- **The trace door's state**, open or closed, with its date: admin-con's
-- word at admission and at every change after. The act that builds the
-- relay client writes it; it lands here now so that act adds no migration.

ALTER TABLE agent
  ADD COLUMN constituents     INTEGER[],
  ADD COLUMN constituents_at  TIMESTAMPTZ,
  ADD COLUMN trace_door       BOOLEAN,
  ADD COLUMN trace_door_at    TIMESTAMPTZ,
  ADD CONSTRAINT agent_trace_door_is_dated
    CHECK ((trace_door IS NULL) = (trace_door_at IS NULL));
