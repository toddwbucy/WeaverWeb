-- **An agent's loop, versioned** (Spec 6.3, the loop page's act of
-- 2026-11-26).
--
-- One wire file per version: the text a person holding `operator` on the
-- agent saved through the loop page, written by `src/store/loop_text.rs`
-- alone. **The text enters no audit record**: a save is audited with the
-- target kind `loop`, naming the version's `lp-` identity, its action
-- naming the digest and the byte count. The grammar is the agent's side's
-- and is not judged here; the bound is the contract's, 256 KiB.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE agent_loop (
  loop_id TEXT PRIMARY KEY CHECK (loop_id IS NOT NULL AND loop_id ~ '^lp-[0-9a-f]{16}$'),
  agent_id agent_id NOT NULL REFERENCES agent (agent_id),
  -- The agent's versions count from 1, one more than the head at each save.
  version INTEGER NOT NULL CHECK (version IS NOT NULL AND version >= 1),
  -- The wire file as stored, each CRLF the browser sent stored as LF.
  text TEXT NOT NULL CHECK (
    text IS NOT NULL AND octet_length(text) BETWEEN 1 AND 262144
  ),
  bytes INTEGER NOT NULL CHECK (bytes IS NOT NULL AND bytes = octet_length(text)),
  -- **The digest is the text's own**, lowercase hex, so no row names a
  -- digest its text does not have.
  sha256 TEXT NOT NULL CHECK (
    sha256 IS NOT NULL
    AND sha256 = encode(sha256(convert_to(text, 'UTF8')), 'hex')
  ),
  saved_by TEXT NOT NULL REFERENCES person (person_id),
  saved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  CONSTRAINT agent_loop_one_row_per_version UNIQUE (agent_id, version)
);

-- **A version is never changed or removed** (Spec 6.3): an update, a
-- delete and a truncate are refused, the last by a statement trigger, which
-- no row trigger sees.
CREATE FUNCTION agent_loop_is_append_only() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'an agent''s loop is append-only: % on agent_loop is refused', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_loop_refuses_update_and_delete
  BEFORE UPDATE OR DELETE ON agent_loop
  FOR EACH ROW EXECUTE FUNCTION agent_loop_is_append_only();

CREATE TRIGGER agent_loop_refuses_truncate
  BEFORE TRUNCATE ON agent_loop
  FOR EACH STATEMENT EXECUTE FUNCTION agent_loop_is_append_only();

-- **The audit's targets gain the loop**, by its version's `lp-` identity,
-- for its save (Spec 2.13, 6.3).
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN (
      'authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert', 'declaration',
      'hugging_face', 'model_pull', 'setup', 'ssh_key', 'loop'
    )
  ),
  ADD CONSTRAINT audit_a_loop_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'loop'
      OR (target_id IS NOT NULL AND target_id ~ '^lp-[0-9a-f]{16}$')
    )
  );
