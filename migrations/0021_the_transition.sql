-- **A `show` in transition, beside the last state** (Spec 2.12, 7.2;
-- `toddwbucy/WeaverWeb#38`, #46 item 2): admin's `show` may answer
-- `in_transition`, which claims no state: another invocation holds the
-- agent, a run starting or ending, or an ended run's processes still being
-- ended. The row keeps its last load state, its source and its tuple, and
-- records the transition beside them: when it was seen, here, and its
-- constituents, in the column a `State` answer writes. The next
-- observation that writes a state ends it, clearing this.
ALTER TABLE agent ADD COLUMN in_transition_at TIMESTAMPTZ;
