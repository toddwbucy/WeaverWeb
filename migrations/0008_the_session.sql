-- The session of weaver-web-Spec section 2.8, which that section has
-- specified since 2026-09-08 and which no migration built. Found while
-- giving the Record surface a gate: the gate had no table to read.
--
-- **It is continuity and it is not a person.** A session is opened by an
-- operator who claims a name, what is stored is the claim, and nothing
-- here proves anyone is anyone, per the charter's section 6. Nothing here
-- is access control.
CREATE TABLE session (
  session_id   BIGSERIAL PRIMARY KEY,

  -- **The bearer's digest and never the bearer.** The value the browser
  -- holds is hashed before it is stored and the lookup is on the digest,
  -- so a read of this table is not a set of live sessions.
  bearer_digest TEXT NOT NULL UNIQUE,

  -- The name claimed at open, which is what section 3.2's author member
  -- holds for a row this session authored. A claim and never a proof.
  claimed_name TEXT NOT NULL,

  -- The role, per the charter's section 6: structural while not being
  -- access control, so the column stands and the proof is what is missing.
  role         TEXT NOT NULL DEFAULT 'user',

  opened_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  closed_at    TIMESTAMPTZ,

  CONSTRAINT session_role_is_one_of
    CHECK (role IN ('user', 'admin')),
  CONSTRAINT session_bearer_digest_is_sha256_hex
    CHECK (bearer_digest ~ '^[0-9a-f]{64}$')
);

-- A gate reads by the digest the browser's bearer hashes to, so the unique
-- constraint above is the index it hits. The open sessions are what a
-- close walks, which is the only other read this table owes.
CREATE INDEX session_open ON session (closed_at) WHERE closed_at IS NULL;
