-- **The operator's SSH public keys** (Spec 6.2, act of 2026-11-16).
--
-- A console setting, one or more keys, written by an admin alone through
-- `src/store/ssh_key.rs`, at most 16 held, the bound counted inside the
-- write's identity exclusion. **A key is a public half and no secret**, so
-- the store holds its line in clear. The line's text enters no audit
-- record: a write is audited with the target kind `ssh_key`, naming the
-- key's identity. The console checks the line's form alone
-- (`src/surfaces/ssh_key.rs`).
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE operator_ssh_key (
  key_id TEXT PRIMARY KEY CHECK (key_id IS NOT NULL AND key_id ~ '^sk-[0-9a-f]{16}$'),
  -- The line as given, trimmed, at most 16 KiB.
  line TEXT NOT NULL CHECK (line IS NOT NULL AND octet_length(line) BETWEEN 1 AND 16384),
  -- The key's SHA-256 fingerprint in OpenSSH's form: `SHA256:` and the
  -- digest's base64 without padding. One key is held once.
  fingerprint TEXT NOT NULL UNIQUE CHECK (
    fingerprint IS NOT NULL AND fingerprint ~ '^SHA256:[A-Za-z0-9+/]{43}$'
  ),
  added_by TEXT NOT NULL REFERENCES person (person_id),
  added_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- **The audit's targets gain the key**, by its `sk-` identity, for its add
-- and its removal (Spec 2.13, 6.2).
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN (
      'authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert', 'declaration',
      'hugging_face', 'model_pull', 'setup', 'ssh_key'
    )
  ),
  ADD CONSTRAINT audit_an_ssh_key_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'ssh_key'
      OR (target_id IS NOT NULL AND target_id ~ '^sk-[0-9a-f]{16}$')
    )
  );
