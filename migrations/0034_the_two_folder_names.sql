-- **Two more names no agent takes** (Spec 6.2; the Planner's lean of
-- 2026-11-25, which the operator may strike): the appliance keeps a folder
-- named `admin` and one named `stack` beside the folder it makes for each
-- agent, named after the agent, so an agent of either name would collide
-- with one of them. The start page's rule refuses both
-- (`store::setup::agent_name`), and the store's CHECK holds the same rule,
-- so a page that took either would meet a fault, never a row.
--
-- 0026 is frozen, so this migration drops the name's check by its name and
-- adds it again with the two names beside admin's two. No row is written,
-- and the check added is validated against the rows that stand: nothing is
-- deployed, so none holds either name.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

ALTER TABLE agent_setup
  DROP CONSTRAINT agent_setup_name_check,
  ADD CONSTRAINT agent_setup_name_check CHECK (
    name IS NOT NULL
    AND name ~ '^[a-z][a-z0-9]{1,15}$'
    AND name NOT IN ('gatecon', 'admincon', 'admin', 'stack')
  );
