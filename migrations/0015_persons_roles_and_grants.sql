-- Spec 2.13 and `docs/project/design-2026-10-07-iam.md` sections 6 to 10:
-- the person, their passkeys and enrollment tokens, the roles and the
-- grants, and the audit's foreign key to the person, which 0014 left out
-- because no person table existed yet.
--
-- **0014 is frozen from here**, as every applied migration is.
--
-- **Each CHECK is written to be false, never unknown, on a missing value**,
-- 0014's rule: every member a CHECK compares is guarded by an explicit
-- `IS NOT NULL`, the columns declared NOT NULL included.
--
-- **No secret is stored**: a token as its digest only, and a passkey's
-- stored half is its public key. **Every write here is an identity write**,
-- taken under the identity exclusion (`src/store/identity.rs`).

-- Spec 2: a key this store generates is two letters and sixteen hex.
CREATE DOMAIN person_id AS TEXT CHECK (VALUE ~ '^pe-[0-9a-f]{16}$');
CREATE DOMAIN grant_id AS TEXT CHECK (VALUE ~ '^gr-[0-9a-f]{16}$');

-- **The person** (Spec 2.13). The name as given, trimmed, and its
-- canonical form (design section 6: Unicode's compatibility caseless form),
-- unique, so name-first sign-in finds one person and no second person
-- enrolls a case, width or composition variant of a name. An authored row
-- (Spec 3.2), carrying the author and the version.
CREATE TABLE person (
  person_id    person_id PRIMARY KEY DEFAULT weaver_key('pe'),
  name         TEXT NOT NULL,
  name_key     TEXT NOT NULL,
  enabled      BOOLEAN NOT NULL DEFAULT true,
  enrolled_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  author       TEXT,
  version      BIGINT NOT NULL DEFAULT 1,
  CONSTRAINT person_name_is_given CHECK (
    name IS NOT NULL AND name <> '' AND octet_length(name) <= 1024
  ),
  CONSTRAINT person_name_key_is_formed CHECK (name_key IS NOT NULL AND name_key <> '')
);
CREATE UNIQUE INDEX person_one_name_key ON person (name_key);

-- **A passkey** (design section 10): the credential ID unique across every
-- person's passkeys, the library's serialized passkey (its public key, its
-- algorithm and its counter), a label the person gives, and when it was
-- added and last used. Written in this act by the host reset alone, which
-- clears a person's; the passkey pull request adds the rest.
CREATE TABLE passkey (
  credential_id  TEXT PRIMARY KEY,
  person_id      person_id NOT NULL REFERENCES person (person_id),
  credential     JSONB NOT NULL,
  label          TEXT,
  added_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_used_at   TIMESTAMPTZ,
  CONSTRAINT passkey_credential_id_is_given
    CHECK (credential_id IS NOT NULL AND credential_id <> '')
);

-- **An enrollment token** (Spec 2.13, design section 7): bound to one
-- person, held only as its digest beside its expiry, its lifetime never
-- more than seven days. **A person holds at most one live token**: issuing
-- one ends the earlier, so the newest token printed is the one that works.
-- A token ends redeemed (the passkey pull request), superseded by a newer
-- one, or by the host reset.
CREATE TABLE enrollment_token (
  token_digest  TEXT PRIMARY KEY,
  person_id     person_id NOT NULL REFERENCES person (person_id),
  issued_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at    TIMESTAMPTZ NOT NULL,
  ended_at      TIMESTAMPTZ,
  ended         TEXT,
  CONSTRAINT enrollment_token_is_a_digest
    CHECK (token_digest IS NOT NULL AND token_digest ~ '^[0-9a-f]{64}$'),
  CONSTRAINT enrollment_token_lives_at_most_seven_days CHECK (
    issued_at IS NOT NULL AND expires_at IS NOT NULL
    AND expires_at > issued_at AND expires_at <= issued_at + interval '7 days'
  ),
  CONSTRAINT enrollment_token_ends_with_a_reason CHECK (
    (ended_at IS NULL AND ended IS NULL)
    OR (ended_at IS NOT NULL AND ended IS NOT NULL
      AND ended IN ('redeemed', 'superseded', 'reset'))
  )
);
CREATE UNIQUE INDEX enrollment_token_one_live_per_person
  ON enrollment_token (person_id) WHERE ended_at IS NULL;

-- **The roles** (Spec 2.13, the operator's ruling of 2026-10-07): `admin`
-- server-wide, fixed by the store and written by nothing, carrying no agent
-- verb; `observer` and `operator` per agent, authored rows whose verbs are
-- written within the vocabulary of Spec 7.2 plus `turn`.
CREATE TABLE role (
  name     TEXT PRIMARY KEY,
  scope    TEXT NOT NULL,
  verbs    TEXT[] NOT NULL,
  author   TEXT,
  version  BIGINT NOT NULL DEFAULT 1,
  CONSTRAINT role_scope_is_the_admins_alone CHECK (
    name IS NOT NULL AND scope IS NOT NULL
    AND scope IN ('agent', 'server') AND ((name = 'admin') = (scope = 'server'))
  ),
  -- `<@` is false, never unknown, where an element is NULL (measured on
  -- 2026-10-07), so a missing verb is refused by the containment itself.
  CONSTRAINT role_verbs_are_the_vocabulary CHECK (
    verbs IS NOT NULL
    AND verbs <@ ARRAY['show', 'validate', 'load', 'unload', 'stop', 'turn']::TEXT[]
  ),
  CONSTRAINT role_admin_carries_no_agent_verb
    CHECK (name IS NOT NULL AND verbs IS NOT NULL AND (name <> 'admin' OR cardinality(verbs) = 0))
);
INSERT INTO role (name, scope, verbs) VALUES
  ('admin', 'server', '{}'),
  ('observer', 'agent', '{show}'),
  ('operator', 'agent', '{show,validate,load,unload,stop,turn}');

-- **The admin role is fixed by the store** (Spec 2.13): nothing updates or
-- deletes it, whatever path a statement of this crate takes.
CREATE FUNCTION role_admin_is_fixed() RETURNS trigger AS $$
BEGIN
  IF OLD.name = 'admin' THEN
    RAISE EXCEPTION 'the admin role is fixed by the store: % refused', TG_OP;
  END IF;
  RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$ LANGUAGE plpgsql;
CREATE TRIGGER role_admin_is_fixed
  BEFORE UPDATE OR DELETE ON role
  FOR EACH ROW EXECUTE FUNCTION role_admin_is_fixed();

-- **A grant** binds a person to a role on one agent of the register, or to
-- `admin` server-wide and only to `admin`. An authored row (Spec 3.2); a
-- revoked grant keeps its row with when it was revoked, and at most one
-- live grant binds a person to one role on one agent.
CREATE TABLE role_grant (
  grant_id    grant_id PRIMARY KEY DEFAULT weaver_key('gr'),
  person_id   person_id NOT NULL REFERENCES person (person_id),
  role        TEXT NOT NULL REFERENCES role (name),
  agent_id    agent_id REFERENCES agent (agent_id),
  granted_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  revoked_at  TIMESTAMPTZ,
  author      TEXT,
  version     BIGINT NOT NULL DEFAULT 1,
  CONSTRAINT role_grant_admin_is_server_wide_alone
    CHECK (role IS NOT NULL AND ((role = 'admin') = (agent_id IS NULL)))
);
CREATE UNIQUE INDEX role_grant_one_live
  ON role_grant (person_id, role, coalesce(agent_id, ''))
  WHERE revoked_at IS NULL;

-- **The audit's person**, now that persons exist, and the identities its
-- new targets carry.
ALTER TABLE audit
  ADD CONSTRAINT audit_person_is_a_person
    FOREIGN KEY (person_id) REFERENCES person (person_id),
  ADD CONSTRAINT audit_a_person_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'person'
      OR (target_id IS NOT NULL AND target_id ~ '^pe-[0-9a-f]{16}$')
    )
  ),
  ADD CONSTRAINT audit_a_grant_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'grant'
      OR (target_id IS NOT NULL AND target_id ~ '^gr-[0-9a-f]{16}$')
    )
  ),
  ADD CONSTRAINT audit_a_role_target_names_one CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'role' OR (target_id IS NOT NULL AND target_id <> '')
    )
  );
