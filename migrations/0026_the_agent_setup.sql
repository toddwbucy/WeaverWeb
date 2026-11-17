-- **The agent's setup** (Spec 6.2, act of 2026-11-16).
--
-- One row per setup, mutable until Confirm, written by an admin alone
-- through `src/store/setup.rs`, and beside it one row per model role. The
-- setup pages write no declaration row. Every write is audited with the
-- target kind `setup`, naming the row.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE agent_setup (
  setup_id TEXT PRIMARY KEY CHECK (setup_id IS NOT NULL AND setup_id ~ '^st-[0-9a-f]{16}$'),
  -- **The start page's rule** (Spec 6.2): a lowercase letter, then
  -- lowercase letters and digits, 2 to 16 characters in all, the box's
  -- agent-creation rule, which refuses every character admin's name rule
  -- refuses and every name ending in one of its reserved suffixes, each
  -- holding a `-`. The two names admin's rule reserves whole are refused
  -- here too, since the character rule admits them.
  name TEXT NOT NULL CHECK (
    name IS NOT NULL
    AND name ~ '^[a-z][a-z0-9]{1,15}$'
    AND name NOT IN ('gatecon', 'admincon')
  ),
  -- The pairing page's checkbox: true where the box is a virtual machine,
  -- false where it is a host. A checkbox drawn unchecked says host.
  virtual_machine BOOLEAN NOT NULL DEFAULT FALSE,
  begun_by TEXT NOT NULL REFERENCES person (person_id),
  begun_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  state TEXT NOT NULL DEFAULT 'open' CHECK (
    state IS NOT NULL AND state IN ('open', 'confirmed')
  ),
  -- The context length the placement page keeps, bounded there by the
  -- decoder's header, and none until it is kept.
  context_length BIGINT CHECK (context_length IS NULL OR context_length >= 1),
  -- The turn context, `continuous` or `independent`, never defaulted:
  -- none until the placement page keeps one.
  turn_context TEXT CHECK (
    turn_context IS NULL OR turn_context IN ('continuous', 'independent')
  ),
  -- Every write checks the version the page read, a stale write answering
  -- 409 and writing nothing. A setup is never deleted, so the version
  -- counts up from 1.
  version BIGINT NOT NULL DEFAULT 1 CHECK (version IS NOT NULL AND version >= 1)
);

-- **One open setup holds a name at a time.**
CREATE UNIQUE INDEX agent_setup_one_open_per_name ON agent_setup (name) WHERE state = 'open';

-- **One row per model role** (Spec 6.2): the role, the repository, the
-- commit, the choice, the pull of Spec 2.15 that fetched it and the facts
-- read from its header. A pick for a role the setup holds replaces that
-- role's row. The first agent's roster is one decoder and one adapter on
-- it.
CREATE TABLE agent_setup_model (
  setup_id TEXT NOT NULL REFERENCES agent_setup (setup_id),
  role TEXT NOT NULL CHECK (role IS NOT NULL AND role IN ('decoder', 'adapter')),
  -- `<owner>/<name>` in the client's `RepoId` grammar, as `model_pull`'s.
  repository TEXT NOT NULL CHECK (
    repository IS NOT NULL
    AND repository ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?/[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
    AND position('--' IN repository) = 0
    AND position('..' IN repository) = 0
  ),
  commit_id TEXT NOT NULL CHECK (commit_id IS NOT NULL AND commit_id ~ '^[0-9a-f]{40}$'),
  -- The choice, named by its first part's path at the commit: a split
  -- model's `-00001-of-` part, or the one file.
  choice TEXT NOT NULL CHECK (
    choice IS NOT NULL AND octet_length(choice) BETWEEN 1 AND 1024
  ),
  pull_id TEXT NOT NULL REFERENCES model_pull (pull_id),
  -- **The adapter's scale**, a finite number with no default, and the
  -- decoder's none.
  scale DOUBLE PRECISION,
  -- The facts read from the choice's header, a fact the header does not
  -- carry being absent. jsonb, so the row commits to no fact set before
  -- the console's GGUF reader does.
  header JSONB NOT NULL CHECK (header IS NOT NULL AND jsonb_typeof(header) = 'object'),
  picked_by TEXT NOT NULL REFERENCES person (person_id),
  picked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (setup_id, role),
  CONSTRAINT agent_setup_model_an_adapter_carries_its_scale CHECK (
    role IS NOT NULL AND ((role = 'adapter') = (scale IS NOT NULL))
  ),
  CONSTRAINT agent_setup_model_a_scale_is_finite CHECK (
    scale IS NULL
    OR scale NOT IN ('NaN'::double precision, 'Infinity'::double precision, '-Infinity'::double precision)
  )
);

-- **The audit's targets gain the setup**, by its `st-` identity, for every
-- write of the setup pages (Spec 2.13, 6.2).
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN (
      'authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert', 'declaration',
      'hugging_face', 'model_pull', 'setup'
    )
  ),
  ADD CONSTRAINT audit_a_setup_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'setup'
      OR (target_id IS NOT NULL AND target_id ~ '^st-[0-9a-f]{16}$')
    )
  );
