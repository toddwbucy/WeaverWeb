-- **A disable revokes the person's outstanding enrollment tokens** (Spec
-- 2.13), in the same write under the identity exclusion, so a token issued
-- before a disable never lands a credential on a disabled row. A token's
-- ending names why it ended, and none of 0015's reasons (`redeemed`,
-- `superseded`, `reset`) is a disable's, so `revoked` joins them.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.
ALTER TABLE enrollment_token DROP CONSTRAINT enrollment_token_ends_with_a_reason;
ALTER TABLE enrollment_token ADD CONSTRAINT enrollment_token_ends_with_a_reason CHECK (
  (ended_at IS NULL AND ended IS NULL)
  OR (ended_at IS NOT NULL AND ended IS NOT NULL
    AND ended IN ('redeemed', 'superseded', 'reset', 'revoked'))
);
