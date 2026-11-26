-- **The gate plane not enrolled** (Spec 2.12, 8.2; the operator's rulings
-- of 2026-11-25): an appliance's pairing enrolls admin-con alone, and the
-- row's gate plane reads `none` until the loop's install enrolls gate-con.
-- A plane in `none` holds no fingerprint and no authority, and is neither
-- live nor revoked.
--
-- 0010 is frozen, so this migration changes the register in place: the
-- gate plane's fingerprint and authority lose `NOT NULL`, the checks that
-- named them are dropped by name and added again to hold wherever the
-- column holds a value, the state's check admits `none`, and two checks
-- bind the state to the two absences, so a fingerprint is absent exactly
-- where the plane is not enrolled. **Every existing row keeps its values**:
-- no row is written here, and each check added is validated against the
-- rows that stand, every one of which holds a gate fingerprint and a gate
-- authority with its state `live` or `revoked`.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule:
-- each check on a column that may be absent is true where it is absent by
-- an explicit `IS NULL`, and the binding checks compare two truth values,
-- neither of which is ever unknown. The unique index on the gate
-- fingerprint admits any number of absent ones, as PostgreSQL's unique
-- indexes do, and the one-live-row index reads the admin plane's state for
-- a row whose gate is `none`.

ALTER TABLE agent
  ALTER COLUMN gate_fingerprint DROP NOT NULL,
  ALTER COLUMN gate_authority DROP NOT NULL,
  DROP CONSTRAINT agent_gate_fingerprint_is_sha256_hex,
  DROP CONSTRAINT agent_fingerprints_differ,
  DROP CONSTRAINT agent_gate_authority_is_sha256_hex,
  DROP CONSTRAINT agent_gate_state_is_one_of;

ALTER TABLE agent
  ADD CONSTRAINT agent_gate_fingerprint_is_sha256_hex
    CHECK (gate_fingerprint IS NULL OR gate_fingerprint ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT agent_fingerprints_differ
    CHECK (gate_fingerprint IS NULL OR gate_fingerprint <> admin_fingerprint),
  ADD CONSTRAINT agent_gate_authority_is_sha256_hex
    CHECK (gate_authority IS NULL OR gate_authority ~ '^[0-9a-f]{64}$'),
  ADD CONSTRAINT agent_gate_state_is_one_of
    CHECK (gate_state IN ('live', 'revoked', 'none')),
  ADD CONSTRAINT agent_gate_fingerprint_absent_exactly_where_not_enrolled
    CHECK ((gate_state = 'none') = (gate_fingerprint IS NULL)),
  ADD CONSTRAINT agent_gate_authority_absent_exactly_where_not_enrolled
    CHECK ((gate_state = 'none') = (gate_authority IS NULL));
