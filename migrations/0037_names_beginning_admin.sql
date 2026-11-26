-- **Every name beginning `admin` is refused, and `stack` stands ruled**
-- (Spec 6.2; the operator's rulings of 2026-11-26): the appliance's
-- agent-creation script refuses every name beginning `admin`, since the
-- appliance keeps its administrative configuration in folders so named
-- beside each agent's own, and the operator ruled `stack` refused ("75 (e).
-- Yes, refuse stack."). The start page's rule refuses the same
-- (`store::setup::agent_name`), and the store's CHECK holds the same rule,
-- so a page that took such a name would meet a fault, never a row.
--
-- 0034 is frozen, so this migration drops the name's check by its name and
-- adds it again: `admin` and `admincon` are now inside `admin%`. No row is
-- written, and the check added is validated against the rows that stand. A
-- setup is named only on New agent, which no appliance has yet paired
-- through, so no row holds a name beginning `admin`; where one did, this
-- migration would refuse to apply rather than leave it unchecked.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

ALTER TABLE agent_setup
  DROP CONSTRAINT agent_setup_name_check,
  ADD CONSTRAINT agent_setup_name_check CHECK (
    name IS NOT NULL
    AND name ~ '^[a-z][a-z0-9]{1,15}$'
    AND name NOT LIKE 'admin%'
    AND name NOT IN ('gatecon', 'stack')
  );
