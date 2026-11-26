-- **Which loop ran at the agent's last load** (Spec 2.12, 6.3; the loop
-- page's act of 2026-11-26).
--
-- Written from the live load event's `composer` alone, in the tuple's own
-- write and under its arrival sequence: `compiled` where the `composer`
-- holds no `file`, the wire file's digest where it holds `file` and a
-- `sha256` of 64 lowercase hex, and none, its date kept, where the event
-- names no loop the page reads. Both absent: no load event is recorded.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

ALTER TABLE agent
  ADD COLUMN loop_ran TEXT,
  ADD COLUMN loop_ran_at TIMESTAMPTZ,
  ADD CONSTRAINT agent_loop_ran_is_its_shape CHECK (
    (loop_ran IS NULL)
    OR (loop_ran IS NOT NULL AND (loop_ran = 'compiled' OR loop_ran ~ '^[0-9a-f]{64}$'))
  ),
  ADD CONSTRAINT agent_loop_ran_has_its_date CHECK (
    loop_ran IS NULL OR loop_ran_at IS NOT NULL
  );
