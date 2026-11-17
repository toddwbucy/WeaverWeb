-- **The model pulls** (Spec 2.15, the act of 2026-11-16).
--
-- A pull is one row and each of its files another: the rulings' record of
-- what the console fetched, the repository, the commit, every file's
-- SHA-256 and size, the account that pulled and the time. An admin's ask
-- writes them (`src/store/model_pull.rs`), audited with the target kind
-- `model_pull`, and an admin's cancel ends a running pull, audited alike.
-- The puller writes a file's progress, its end and the pull's end, about a
-- pull that audited ask began.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE model_pull (
  pull_id TEXT PRIMARY KEY CHECK (pull_id IS NOT NULL AND pull_id ~ '^mp-[0-9a-f]{16}$'),
  -- `<owner>/<name>` in the client's `RepoId` grammar.
  repository TEXT NOT NULL CHECK (
    repository IS NOT NULL
    AND repository ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?/[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
    AND position('--' IN repository) = 0
    AND position('..' IN repository) = 0
  ),
  -- The 40-hex commit, never a branch.
  commit_id TEXT NOT NULL CHECK (commit_id IS NOT NULL AND commit_id ~ '^[0-9a-f]{40}$'),
  -- The account's name as `whoami-v2` gave it, or none where the pull ran
  -- without an account, as a public repository's can.
  account TEXT,
  asked_by TEXT NOT NULL REFERENCES person (person_id),
  asked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  finished_at TIMESTAMPTZ,
  state TEXT NOT NULL CHECK (
    state IS NOT NULL AND state IN ('running', 'verified', 'failed', 'cancelled')
  ),
  reason TEXT,
  -- The admin whose cancel ended the pull, its partial kept for a later
  -- pull to resume.
  cancelled_by TEXT REFERENCES person (person_id),
  CONSTRAINT model_pull_ends_once CHECK (
    state IS NOT NULL AND ((state = 'running') = (finished_at IS NULL))
  ),
  CONSTRAINT model_pull_a_failure_says_why CHECK (
    state IS NOT NULL AND ((state = 'failed') = (reason IS NOT NULL))
  ),
  CONSTRAINT model_pull_a_cancel_names_its_admin CHECK (
    state IS NOT NULL AND ((state = 'cancelled') = (cancelled_by IS NOT NULL))
  )
);

-- **One pull runs at a time** (Spec 2.15): the store holds what the ask
-- checks.
CREATE UNIQUE INDEX model_pull_one_runs ON model_pull ((TRUE)) WHERE state = 'running';

CREATE TABLE model_pull_file (
  pull_id TEXT NOT NULL REFERENCES model_pull (pull_id),
  ordinal INTEGER NOT NULL CHECK (ordinal IS NOT NULL AND ordinal >= 0),
  path TEXT NOT NULL CHECK (
    path IS NOT NULL AND octet_length(path) BETWEEN 1 AND 1024
  ),
  size BIGINT NOT NULL CHECK (size IS NOT NULL AND size >= 0),
  blob_id TEXT NOT NULL CHECK (blob_id IS NOT NULL AND blob_id ~ '^[0-9a-f]{40}$'),
  -- Whether the file is kept in LFS: its SHA-256 is Hugging Face's from
  -- the ask, and its blob is named by it. A file kept in git has its
  -- SHA-256 computed by the console as it is written, and none before.
  lfs BOOLEAN NOT NULL,
  sha256 TEXT CHECK (sha256 ~ '^[0-9a-f]{64}$'),
  bytes_done BIGINT NOT NULL DEFAULT 0 CHECK (
    bytes_done IS NOT NULL AND bytes_done >= 0 AND size IS NOT NULL AND bytes_done <= size
  ),
  source TEXT CHECK (source IN ('downloaded', 'found')),
  PRIMARY KEY (pull_id, path),
  UNIQUE (pull_id, ordinal),
  CONSTRAINT model_pull_file_an_lfs_file_names_its_digest CHECK (
    lfs IS NOT NULL AND (NOT lfs OR sha256 IS NOT NULL)
  ),
  CONSTRAINT model_pull_file_a_verified_file_holds_its_digest CHECK (
    source IS NULL OR sha256 IS NOT NULL
  )
);

-- **The audit's targets gain the pull**, by its `mp-` identity, for its
-- ask and its cancel (Spec 2.13, 2.15).
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN (
      'authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert', 'declaration',
      'hugging_face', 'model_pull'
    )
  ),
  ADD CONSTRAINT audit_a_model_pull_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'model_pull'
      OR (target_id IS NOT NULL AND target_id ~ '^mp-[0-9a-f]{16}$')
    )
  );
