-- **The session as a person's** (Spec 2.8, `docs/project/design-2026-10-07-iam.md`
-- section 6): the session carries the person and the passkey it was opened
-- with, and the claimed name and the configured role of 0008 retire.
--
-- **The table is rebuilt rather than altered**: no deployment holds data,
-- and the migrations are squashed at the install act (the operator's ruling
-- of 2026-10-08), so there is no row to carry and no guard is owed for one.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.
DROP TABLE session;

CREATE TABLE session (
  session_id     BIGSERIAL PRIMARY KEY,

  -- **The bearer's digest and never the bearer**, as 0008 held it: the
  -- lookup is on the digest, so a read of this table is not a set of live
  -- sessions.
  bearer_digest  TEXT NOT NULL UNIQUE,

  -- The person the session is, whose identity an authored row's author
  -- member takes (Spec 3.2).
  person_id      person_id NOT NULL REFERENCES person (person_id),

  -- **The passkey it was opened with, by credential ID, and no foreign
  -- key**: removing a passkey deletes its row (the host reset does), and a
  -- key would refuse that or cascade into the session, while **nothing but
  -- the surface writes a session**. The session sees the absence at its
  -- next use and ends there.
  credential_id  TEXT NOT NULL,

  opened_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
  -- Written at most once a minute, by one conditional update on the
  -- database's clock, so it never moves backwards (Spec 3's writer model).
  last_used_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  closed_at      TIMESTAMPTZ,

  CONSTRAINT session_bearer_digest_is_sha256_hex
    CHECK (bearer_digest IS NOT NULL AND bearer_digest ~ '^[0-9a-f]{64}$'),
  CONSTRAINT session_credential_id_is_given
    CHECK (credential_id IS NOT NULL AND credential_id <> ''),
  CONSTRAINT session_closes_after_it_opens
    CHECK (closed_at IS NULL
      OR (opened_at IS NOT NULL AND closed_at >= opened_at))
);

-- A use reads by the digest, which the unique constraint above indexes;
-- the open sessions are the only other set read.
CREATE INDEX session_open ON session (closed_at) WHERE closed_at IS NULL;
