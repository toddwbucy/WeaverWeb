-- What the review of PR #535 asked of the schema, per weaver-web-Spec
-- section 2.2 and weaver-analysis-web-contract section 2.2, both as revised
-- 2026-09-10.
--
-- **This is 0006 and not an amendment to 0005.** sqlx verifies the stored
-- checksum of every applied version at connect, so editing a migration a
-- box has already run refuses that box's next connect with a mismatch and
-- takes the server and every test with it. 0005 was applied and recorded
-- before these lines were written, so they land as their own version.

-- The device model and the engine libraries come from a deposit the caller
-- names, and a caller may name none: the sink may be a pipe, and a record
-- written before deposits were kept has one nowhere. Absent rather than
-- defaulted, per Spec section 6, a run whose silicon nobody recorded being
-- a run this store holds rather than refuses.
ALTER TABLE run ALTER COLUMN device DROP NOT NULL;
ALTER TABLE run ALTER COLUMN engine DROP NOT NULL;

-- Spec 2.3's own sentinel clause says a hash the SPU could not compute
-- crosses as the empty string and joins to nothing, which is a run recorded
-- as one whose identity failed. The check on this column refused exactly
-- that, making such a run unrecordable rather than recorded, where the
-- catalog's own check is what keeps the sentinel out of the catalog.
ALTER TABLE run DROP CONSTRAINT run_record_identity_is_not_the_sentinel;

-- Spec 3.1 has the ingest write the parent reference alone, the branch
-- position being the authoring path's per section 5. The whole-or-neither
-- check refused exactly that insert, so no branched run could be ingested
-- at all. The rule that survives is the one 0002 already states for the
-- parting position: a branch position needs a parent, and a parent may
-- stand before the row that authored the branch joins its position.
ALTER TABLE run DROP CONSTRAINT run_branch_is_stated_whole;
ALTER TABLE run ADD CONSTRAINT run_branch_needs_a_parent
  CHECK (branch_position IS NULL OR parent_run_id IS NOT NULL);

-- Spec 3.1 makes "written empty" normative for the declared boundary set,
-- and a column with no type check took an object in one test and an array
-- in another. 0002 established this pattern for the sweep's values.
--
-- **The rows that stand are written first, because a check validates them.**
-- Postgres validates every existing row when a CHECK is added, so this
-- constraint refuses to be created on any database holding a row whose
-- boundary set is not an array, which is every database that ran this
-- crate's tests before this act. **The empty array is the fact and not a
-- default**, per Spec section 2.2: every organ in the base is reached by
-- kernel peer identity over a Unix socket, so no run that exists ran with a
-- boundary owed by anything, and a row spelling that as an object spelled
-- the same emptiness in the wrong shape.
UPDATE run SET boundary_set = '[]'::jsonb
  WHERE jsonb_typeof(boundary_set) <> 'array';

ALTER TABLE run ADD CONSTRAINT run_boundary_set_is_an_array
  CHECK (jsonb_typeof(boundary_set) = 'array');

-- The run row's columns, named once, so the two reads that select them are
-- static strings and agree by construction. They were a const list rendered
-- two ways in Rust, which needed sqlx's AssertSqlSafe escape and rebuilt the
-- string on every read. A view is where a column set belongs: the reads
-- select from it, `RunTuple` reads it back by name, and a member added here
-- reaches both reads or neither.
--
-- `seed` carries its cast: the record spells it as an unsigned 64-bit value
-- and the column holds NUMERIC(20,0), so a reader takes it as text and never
-- as an i64 it would not fit.
CREATE VIEW run_tuple AS
SELECT
  run_id,
  record_identity,
  seed::text AS seed,
  sampler,
  device,
  engine,
  field_depth,
  task_source,
  task_identity,
  boundary_set,
  forced_position,
  forced_token,
  parent_run_id,
  branch_position,
  parting_position,
  record_session,
  record_digest,
  prefix_length,
  signature,
  ingested_at
FROM run;
