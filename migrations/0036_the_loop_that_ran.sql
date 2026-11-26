-- **Which loop ran at the agent's last load** (Spec 2.12, 6.3; the loop
-- page's act of 2026-11-26).
--
-- Written wherever the tuple is written with a load, from a live load
-- event's `composer` or a `show` answer's `load.composer`, in the tuple's
-- own write and under its arrival sequence: `compiled` where the
-- `composer` holds no `file`, the wire file's digest where it holds `file`
-- and a `sha256` of 64 lowercase hex, and none, with a date, where it
-- names no loop the page reads. The date is a live load event's always,
-- and a `show`'s only where it changes the member. Both absent: no load is
-- recorded.
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
