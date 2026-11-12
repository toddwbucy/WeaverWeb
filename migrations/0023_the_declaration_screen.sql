-- **The declaration screen's rows** (Spec 2.4 and 6.1; act of 2026-11-11).
--
-- Migration 0001 built `declaration` for the agent's grammar whole, and no
-- code read or wrote it. The act of 2026-11-11 makes its body the console's
-- own document and every save an immutable row in a lineage. **Nothing is
-- migrated**: the members are added to the table as it stands and no row
-- moves. The new members are NOT NULL where Spec 2.4 says every row carries
-- them, which holds because the table holds no row: a deployment where one
-- had been written by hand refuses this migration by name rather than
-- holding a row the screen cannot read.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

ALTER TABLE declaration
  -- **Its identity**, `de-` and sixteen lowercase hex, minted by the
  -- server before the save's first audit record so the record can name the
  -- row it is about to write. The BIGINT key stays the table's own, since
  -- the staged experiment's parent references it.
  ADD COLUMN declaration_ref TEXT NOT NULL,
  -- **The agent's name**, in the register's grammar (0010), naming no box.
  ADD COLUMN name TEXT NOT NULL,
  -- **Its lineage's root**, none for the root itself.
  ADD COLUMN lineage_root_id BIGINT REFERENCES declaration (declaration_id),
  -- **When it was saved**, the store's clock.
  ADD COLUMN saved_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  -- **What moved**: the document's paths whose values differ from the
  -- parent's, computed by the server, empty at a root.
  ADD COLUMN what_moved TEXT[] NOT NULL DEFAULT '{}',
  ADD CONSTRAINT declaration_ref_is_its_shape CHECK (
    declaration_ref IS NOT NULL AND declaration_ref ~ '^de-[0-9a-f]{16}$'
  ),
  ADD CONSTRAINT declaration_ref_is_one UNIQUE (declaration_ref),
  ADD CONSTRAINT declaration_name_is_the_registers CHECK (
    name IS NOT NULL AND name ~ '^[A-Za-z0-9_-]+$'
  ),
  -- **A root has neither parent nor root, and every other row has both**,
  -- its version one more than the root's count would allow being the
  -- store's assignment at the insert.
  ADD CONSTRAINT declaration_root_or_child CHECK (
    (parent_declaration_id IS NULL AND lineage_root_id IS NULL AND version = 1)
    OR (parent_declaration_id IS NOT NULL AND lineage_root_id IS NOT NULL
        AND version IS NOT NULL AND version > 1)
  ),
  -- **Nothing moved at a root**, and the one thing that moved is set
  -- exactly where one path moved, to that path.
  ADD CONSTRAINT declaration_root_moved_nothing CHECK (
    parent_declaration_id IS NOT NULL OR cardinality(what_moved) = 0
  ),
  ADD CONSTRAINT declaration_the_one_thing_is_the_one_path CHECK (
    (cardinality(what_moved) = 1 AND the_one_thing_that_moved IS NOT NULL
      AND the_one_thing_that_moved = what_moved[1])
    OR (cardinality(what_moved) <> 1 AND the_one_thing_that_moved IS NULL)
  ),
  ADD CONSTRAINT declaration_body_is_an_object CHECK (
    body IS NOT NULL AND jsonb_typeof(body) = 'object'
  );

-- **No two lineages share a name** (Spec 2.4): one root per name. The
-- screen refuses first, under the declarations' advisory key, naming the
-- name; this index is the schema's own word beneath it. A lineage keeping
-- its name is the screen's, which saves a child under its parent's name
-- alone.
CREATE UNIQUE INDEX declaration_one_lineage_per_name
  ON declaration (name) WHERE parent_declaration_id IS NULL;

-- **A lineage's rows by its root**, on the expression every lineage read
-- filters on (`store::declarations`'s `LINEAGE`), since a root is its own
-- lineage and carries no root of its own.
CREATE INDEX declaration_by_lineage
  ON declaration ((coalesce(lineage_root_id, declaration_id)), declaration_id);

-- **A row's authored half never changes** (Spec 2.4): an update may move
-- the last `validate` reading and nothing else, and no row is deleted or
-- truncated. The schema admits a second child of one parent, for the
-- ablation's arms; the head check is the screen's.
CREATE FUNCTION declaration_authored_half_is_fixed() RETURNS trigger AS $$
BEGIN
  IF TG_OP <> 'UPDATE' THEN
    RAISE EXCEPTION 'a declaration row is never removed: % on declaration is refused', TG_OP;
  END IF;
  IF NEW.declaration_id IS DISTINCT FROM OLD.declaration_id
     OR NEW.declaration_ref IS DISTINCT FROM OLD.declaration_ref
     OR NEW.name IS DISTINCT FROM OLD.name
     OR NEW.body IS DISTINCT FROM OLD.body
     OR NEW.corpus_commit IS DISTINCT FROM OLD.corpus_commit
     OR NEW.parent_declaration_id IS DISTINCT FROM OLD.parent_declaration_id
     OR NEW.lineage_root_id IS DISTINCT FROM OLD.lineage_root_id
     OR NEW.version IS DISTINCT FROM OLD.version
     OR NEW.saved_at IS DISTINCT FROM OLD.saved_at
     OR NEW.what_moved IS DISTINCT FROM OLD.what_moved
     OR NEW.the_one_thing_that_moved IS DISTINCT FROM OLD.the_one_thing_that_moved
     OR NEW.author IS DISTINCT FROM OLD.author THEN
    RAISE EXCEPTION 'a declaration row''s authored half never changes; only its last validate reading may';
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER declaration_refuses_an_authored_update_and_a_delete
  BEFORE UPDATE OR DELETE ON declaration
  FOR EACH ROW EXECUTE FUNCTION declaration_authored_half_is_fixed();

CREATE TRIGGER declaration_refuses_truncate
  BEFORE TRUNCATE ON declaration
  FOR EACH STATEMENT EXECUTE FUNCTION declaration_authored_half_is_fixed();

-- **The audit's targets gain the declaration**, by its `de-` identity, for
-- its save (Spec 2.13, 6.1).
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN (
      'authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert', 'declaration'
    )
  ),
  ADD CONSTRAINT audit_a_declaration_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'declaration'
      OR (target_id IS NOT NULL AND target_id ~ '^de-[0-9a-f]{16}$')
    )
  );
