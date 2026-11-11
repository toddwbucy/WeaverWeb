-- **The sign-in token** (Spec 2.8 and 2.13, the operator's ruling of
-- 2026-11-10; `docs/project/design-2026-10-07-iam.md` sections 1, 6 and 10):
-- a second sign-in method for the pre-release stage, a bearer the host
-- issues where the server's config admits it. A session may now stand
-- without a passkey, and says which method opened it.
--
-- **0021 is frozen from here**, as every applied migration is.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

-- **The session names the method that opened it**, by the audit's own
-- names for the two, so the session and the record of its opening say the
-- same thing. A row standing before this migration was opened by a passkey,
-- which is what the default it is added with says; the default is then
-- dropped, so every session written from here names its method.
ALTER TABLE session ADD COLUMN method TEXT NOT NULL DEFAULT 'passkey assertion';
ALTER TABLE session ALTER COLUMN method DROP DEFAULT;

-- **A session a sign-in token opened names no passkey**, and one a passkey
-- opened names it: the method and the passkey are one fact, so a passkey's
-- method with no passkey, or a token's with one, is refused.
ALTER TABLE session ALTER COLUMN passkey_id DROP NOT NULL;
ALTER TABLE session ADD CONSTRAINT session_method_and_passkey_agree CHECK (
  method IS NOT NULL AND (
    (method = 'passkey assertion' AND passkey_id IS NOT NULL)
    OR (method = 'sign-in token' AND passkey_id IS NULL)
  )
);

-- **The audit's fourth method for a person**: the opening of a session a
-- sign-in token opened.
ALTER TABLE audit DROP CONSTRAINT audit_principal_and_method_agree;
ALTER TABLE audit ADD CONSTRAINT audit_principal_and_method_agree CHECK (
  principal IS NOT NULL AND method IS NOT NULL AND (
    (principal = 'person' AND person_id IS NOT NULL
      AND method IN ('session', 'enrollment token', 'passkey assertion', 'sign-in token'))
    OR (principal = 'server' AND person_id IS NULL AND method = 'server')
    OR (principal = 'host' AND person_id IS NULL AND method = 'host')
  )
);

-- **A sign-in token**, as 0015 holds an enrollment token: bound to one
-- person, held only as its digest beside its expiry, **its lifetime never
-- more than an hour**, since it is redeemed by a person beside the host that
-- printed it and a short life leaves a lost one little time. **A person holds
-- at most one live token**: issuing one ends the earlier, so the newest
-- printed is the one that works. A token ends redeemed at its one use,
-- superseded by a newer one, or revoked by a disable.
CREATE TABLE sign_in_token (
  token_digest  TEXT PRIMARY KEY,
  person_id     person_id NOT NULL REFERENCES person (person_id),
  issued_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at    TIMESTAMPTZ NOT NULL,
  ended_at      TIMESTAMPTZ,
  ended         TEXT,
  CONSTRAINT sign_in_token_is_a_digest
    CHECK (token_digest IS NOT NULL AND token_digest ~ '^[0-9a-f]{64}$'),
  CONSTRAINT sign_in_token_lives_at_most_an_hour CHECK (
    issued_at IS NOT NULL AND expires_at IS NOT NULL
    AND expires_at > issued_at AND expires_at <= issued_at + interval '1 hour'
  ),
  CONSTRAINT sign_in_token_ends_with_a_reason CHECK (
    (ended_at IS NULL AND ended IS NULL)
    OR (ended_at IS NOT NULL AND ended IS NOT NULL
      AND ended IN ('redeemed', 'superseded', 'revoked'))
  )
);
CREATE UNIQUE INDEX sign_in_token_one_live_per_person
  ON sign_in_token (person_id) WHERE ended_at IS NULL;
