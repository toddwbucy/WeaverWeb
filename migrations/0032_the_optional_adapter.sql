-- **The adapter is optional** (Spec 6.2, the operator's ruling of
-- 2026-11-18): a setup with no adapter's pick confirms a tuple with none,
-- and the agent is laid on its base alone.
--
-- 0029 is frozen, so this migration changes its tuple in place: the six
-- adapter columns lose `NOT NULL`, each column's own check is dropped by
-- the name PostgreSQL gave it and added again to hold wherever the column
-- holds a value, and one check keeps the six present together or absent
-- together. The base's identity stays required. **Every existing row keeps
-- its six values**: no row is written here, and each check added is
-- validated against the rows that stand, every one of which holds all six.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule:
-- a column's own check is true where the column is absent, and the
-- whole-or-absent check counts the absent columns, a count that is never
-- unknown.

ALTER TABLE deployment_tuple
  ALTER COLUMN adapter_pull DROP NOT NULL,
  ALTER COLUMN adapter_repository DROP NOT NULL,
  ALTER COLUMN adapter_commit DROP NOT NULL,
  ALTER COLUMN adapter_choice DROP NOT NULL,
  ALTER COLUMN adapter_parts DROP NOT NULL,
  ALTER COLUMN adapter_scale DROP NOT NULL;

ALTER TABLE deployment_tuple
  DROP CONSTRAINT deployment_tuple_adapter_repository_check,
  DROP CONSTRAINT deployment_tuple_adapter_commit_check,
  DROP CONSTRAINT deployment_tuple_adapter_choice_check,
  DROP CONSTRAINT deployment_tuple_adapter_parts_check,
  DROP CONSTRAINT deployment_tuple_adapter_scale_check;

ALTER TABLE deployment_tuple
  ADD CONSTRAINT deployment_tuple_adapter_repository_check CHECK (
    adapter_repository IS NULL
    OR (
      adapter_repository ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?/[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
      AND position('--' IN adapter_repository) = 0
      AND position('..' IN adapter_repository) = 0
    )
  ),
  ADD CONSTRAINT deployment_tuple_adapter_commit_check CHECK (
    adapter_commit IS NULL OR adapter_commit ~ '^[0-9a-f]{40}$'
  ),
  ADD CONSTRAINT deployment_tuple_adapter_choice_check CHECK (
    adapter_choice IS NULL OR octet_length(adapter_choice) BETWEEN 1 AND 1024
  ),
  ADD CONSTRAINT deployment_tuple_adapter_parts_check CHECK (
    adapter_parts IS NULL
    OR (jsonb_typeof(adapter_parts) = 'array' AND jsonb_array_length(adapter_parts) >= 1)
  ),
  ADD CONSTRAINT deployment_tuple_adapter_scale_check CHECK (
    adapter_scale IS NULL
    OR adapter_scale NOT IN ('NaN'::double precision, 'Infinity'::double precision, '-Infinity'::double precision)
  ),
  -- **An adapter is whole or absent**: its pull, repository, commit,
  -- choice, parts and scale stand together, where an adapter was picked, or
  -- none of them does.
  ADD CONSTRAINT deployment_tuple_an_adapter_whole_or_absent CHECK (
    num_nulls(adapter_pull, adapter_repository, adapter_commit, adapter_choice, adapter_parts, adapter_scale)
      IN (0, 6)
  );
