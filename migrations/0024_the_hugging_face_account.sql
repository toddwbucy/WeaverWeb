-- **The Hugging Face account** (Spec 2.15, act of 2026-11-16).
--
-- One row, a console setting, written by an admin alone through
-- `src/store/hugging_face.rs`. It holds the account's name and the token's
-- role as Hugging Face's `whoami-v2` answered them, the token sealed, when
-- it was set and by whom, when it was last checked, and when Hugging Face
-- last refused it. **The token is sealed, never kept in clear**: the stated
-- exception to the digest rule of Spec 2.8 and 2.13, since the console
-- presents it outward and a digest cannot serve that. The key is a file of
-- the authority's set and never a row, so a dump of the store holds no
-- usable token.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE hugging_face_account (
  -- **One row at most**: the key is the constant `true`.
  one BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (one IS NOT NULL AND one),
  -- The account's name in Hugging Face's grammar for a name, the client's
  -- `is_repo_half`: 1 to 96 of letters, digits, `-`, `_` and `.`, neither
  -- starting nor ending with `-` or `.`, and holding no `--` or `..`.
  account TEXT NOT NULL CHECK (
    account IS NOT NULL
    AND account ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
    AND position('--' IN account) = 0
    AND position('..' IN account) = 0
  ),
  role TEXT NOT NULL CHECK (
    role IS NOT NULL AND role IN ('read', 'write', 'fine-grained')
  ),
  -- ChaCha20-Poly1305's output, the token and its 16-byte tag, under a
  -- 12-byte nonce.
  sealed BYTEA NOT NULL CHECK (
    sealed IS NOT NULL AND octet_length(sealed) BETWEEN 17 AND 272
  ),
  nonce BYTEA NOT NULL CHECK (nonce IS NOT NULL AND octet_length(nonce) = 12),
  set_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  set_by TEXT NOT NULL REFERENCES person (person_id),
  checked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  -- When a check last met Hugging Face refusing the token. Cleared by a
  -- check it passes and by a connect. The setting stands until a
  -- disconnect.
  refused_at TIMESTAMPTZ,
  version BIGINT NOT NULL DEFAULT 1 CHECK (version IS NOT NULL AND version >= 1)
);

-- **The audit's targets gain the account**, which names no row, as the
-- authority does, being the console's one setting of its kind (Spec 2.13,
-- 2.15).
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN (
      'authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert', 'declaration',
      'hugging_face'
    )
  ),
  ADD CONSTRAINT audit_a_hugging_face_target_names_no_row CHECK (
    target_kind IS NOT NULL AND (target_kind <> 'hugging_face' OR target_id IS NULL)
  );
