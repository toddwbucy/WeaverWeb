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

-- **Every target the audit writes is checked by its kind** (Spec 2.13),
-- swept on 2026-10-09: an agent's (0014), a person's and a grant's (0015)
-- are checked by their identity's shape and a role's as a name present,
-- and three checks were missing. A passkey target names its row by its
-- `pk-` identity, as its neighbours do, so a value out of shape is refused
-- by name rather than written; the authority target names no row, being
-- the server's one authority; and a target is one of the six kinds the
-- writer knows, so no kind lands that no check covers.
ALTER TABLE audit
  ADD CONSTRAINT audit_a_passkey_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'passkey'
      OR (target_id IS NOT NULL AND target_id ~ '^pk-[0-9a-f]{16}$')
    )
  ),
  ADD CONSTRAINT audit_an_authority_target_names_no_row CHECK (
    target_kind IS NOT NULL AND (target_kind <> 'authority' OR target_id IS NULL)
  ),
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN ('authority', 'agent', 'person', 'grant', 'role', 'passkey')
  );
