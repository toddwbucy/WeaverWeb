-- **The base's identity** (Spec 6.2, act of 2026-11-16).
--
-- An adapter binds to its base by `base-blake3`: one BLAKE3 hasher over
-- every byte of the base's first part, then of each later part in split
-- order. The console computes it from its cache once a decoder's pull is
-- verified (`src/hugging_face/base_identity.rs`) and keeps one row per
-- repository, commit and choice, since a file at a commit never changes:
-- two setups of one model share one row. The row names the verified pull
-- whose cached parts it hashed, its source. It is the server's own write,
-- about a pull an audited pick began, and carries no audit record.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE base_identity (
  -- `<owner>/<name>` in the client's `RepoId` grammar, as `model_pull`'s.
  repository TEXT NOT NULL CHECK (
    repository IS NOT NULL
    AND repository ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?/[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
    AND position('--' IN repository) = 0
    AND position('..' IN repository) = 0
  ),
  commit_id TEXT NOT NULL CHECK (commit_id IS NOT NULL AND commit_id ~ '^[0-9a-f]{40}$'),
  -- The choice, named by its first part's path, as `agent_setup_model`'s.
  choice TEXT NOT NULL CHECK (
    choice IS NOT NULL AND octet_length(choice) BETWEEN 1 AND 1024
  ),
  -- The digest, 64 lowercase hex, BLAKE3's 32 bytes.
  blake3 TEXT NOT NULL CHECK (blake3 IS NOT NULL AND blake3 ~ '^[0-9a-f]{64}$'),
  -- Every byte hashed, the parts' sizes summed.
  bytes BIGINT NOT NULL CHECK (bytes IS NOT NULL AND bytes >= 0),
  pull_id TEXT NOT NULL REFERENCES model_pull (pull_id),
  computed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (repository, commit_id, choice)
);

-- **A row never changes and is never removed**, a trigger of the kind
-- 0023 uses: a file at a commit never changes, so neither does its hash.
CREATE FUNCTION base_identity_is_fixed() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'a base identity row never changes: % on base_identity is refused', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER base_identity_refuses_an_update_and_a_delete
  BEFORE UPDATE OR DELETE ON base_identity
  FOR EACH ROW EXECUTE FUNCTION base_identity_is_fixed();

CREATE TRIGGER base_identity_refuses_truncate
  BEFORE TRUNCATE ON base_identity
  FOR EACH STATEMENT EXECUTE FUNCTION base_identity_is_fixed();
