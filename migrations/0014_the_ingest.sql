-- The ingest of weaver-web-Spec section 3.1, act 10b (issue #2), and the
-- schema it needs, per the rulings of 2026-10-06 in
-- docs/project/brief-2026-10-06-act-10-the-replay-surface.md. Migrations
-- 0001 through 0013 are frozen; everything this act changes is here.

-- =====================================================================
-- The run's ingest status and its reason. Spec 3.1: the run row is
-- written first and closed last, carrying a status a surface can read, so
-- a partially ingested run is visibly partial rather than quietly short.
-- =====================================================================

-- Added nullable, backfilled, then made NOT NULL with no default: every row
-- the old schema holds was written whole, since it had no partial state, and
-- an insert that forgets the status afterwards fails rather than reading as
-- any word. The ingest writes `writing` at the row's creation.
ALTER TABLE run ADD COLUMN ingest_status TEXT;
UPDATE run SET ingest_status = 'whole';
ALTER TABLE run ALTER COLUMN ingest_status SET NOT NULL;
ALTER TABLE run ADD CONSTRAINT run_ingest_status_is_a_status
  CHECK (ingest_status IN ('writing', 'whole', 'short', 'refused'));

-- The reason persists beside the status, since the ingest's answer is gone
-- once its process exits: which generation's points could not be addressed
-- and why for `short`, what was refused for `refused`, and nothing for the
-- two statuses that need none.
ALTER TABLE run ADD COLUMN ingest_reason TEXT;
ALTER TABLE run ADD CONSTRAINT run_ingest_reason_goes_with_its_status
  CHECK ((ingest_status IN ('short', 'refused')) = (ingest_reason IS NOT NULL));

-- =====================================================================
-- The parent, as the record names it and as the store links it.
-- =====================================================================

-- A branch names its parent whether or not the store holds it, so the
-- reference is its own member and the foreign key stays the resolved link:
-- `parent_run_id` is set where the parent is held, and only ever to the
-- reference. Every branch the old schema holds had its parent resolved, so
-- the reference is backfilled from the link before read five filters on it.
ALTER TABLE run ADD COLUMN parent_reference TEXT;
UPDATE run SET parent_reference = parent_run_id WHERE parent_run_id IS NOT NULL;
ALTER TABLE run ADD CONSTRAINT run_link_is_the_reference
  CHECK (parent_run_id IS NULL OR parent_run_id = parent_reference);
CREATE INDEX run_by_parent_reference ON run (parent_reference);

-- Whether the walk of section 3.1 ran. A null parting position means the
-- paths never parted only where the walk ran; where it did not (no parent
-- held, a parent or a child not whole, a tape with no coordinate), the
-- position is unknown, and the surface says `unknown` rather than reading
-- the null as the arm having reproduced its parent. A parting position the
-- old schema holds was computed, so those rows are known; every other null
-- stays unknown, since the column did not yet say which.
ALTER TABLE run ADD COLUMN parting_known BOOLEAN NOT NULL DEFAULT false;
UPDATE run SET parting_known = true WHERE parting_position IS NOT NULL;
ALTER TABLE run ADD CONSTRAINT run_parting_is_known_where_set
  CHECK (parting_position IS NULL OR parting_known);
ALTER TABLE run ADD CONSTRAINT run_parting_known_needs_a_parent
  CHECK (NOT parting_known OR parent_run_id IS NOT NULL);

-- The tuple view gains the four members at its end, which is the only place
-- CREATE OR REPLACE VIEW may add columns.
CREATE OR REPLACE VIEW run_tuple AS
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
  ingested_at,
  ingest_status,
  ingest_reason,
  parent_reference,
  parting_known
FROM run;

-- =====================================================================
-- The position's members the analysis seam does not carry.
-- =====================================================================

-- Each was NOT NULL in 0001, which was written against `model.field`, a
-- record that carries all four. The ingest's seam is the signals emission,
-- `weaver-analysis-web-contract` sections 2.1 and 6, which carries none of
-- the last two and the first only where measured:
-- - token_text: the token's surface text does not cross (contract 2.1);
--   detokenizing is the reader's and this crate holds no tokenizer, so it is
--   written absent (toddwbucy/WeaverAnalysis#10 asks for it on the wire).
-- - entropy: crosses "or absent" (contract 2.1), absent where the generation
--   did not measure it, and absent is never zero.
-- - alternatives and realized: the ranked candidates are `model.field`'s and
--   never cross this seam (contract 6), so a position landed from it holds
--   neither, and read one says `absent` rather than an empty list or a rank.
ALTER TABLE position ALTER COLUMN token_text DROP NOT NULL;
ALTER TABLE position ALTER COLUMN entropy DROP NOT NULL;
ALTER TABLE position ALTER COLUMN alternatives DROP NOT NULL;
ALTER TABLE position ALTER COLUMN realized DROP NOT NULL;

-- =====================================================================
-- The generation: the summary entry of contract 2.2, which has no other
-- home. Spec 3.1 lands it even where its points cannot land, and a surface
-- reads a turn's perplexity and counts from it. Keyed by the run and its
-- landing order, because a turn key is text and sorts wrong ("t-10" before
-- "t-2"), and a turn may hold more than one generation.
-- =====================================================================

CREATE TABLE generation (
  run_id          TEXT NOT NULL REFERENCES run(run_id) ON DELETE CASCADE,
  seq             INTEGER NOT NULL,
  -- The turn key, where the record carries one.
  turn            TEXT,
  -- Absent where the record holds no perplexity, never computed here.
  perplexity      DOUBLE PRECISION,
  -- The resident count at the generation's close, absent where no
  -- `model.output` reported one; its points then have no position.
  resident        INTEGER,
  output_count    INTEGER NOT NULL,
  -- The seed this generation drew from, derived from the declared one by the
  -- SPU, per generation by construction (contract 2.2), so it lives here and
  -- not on the run, whose seed is the declared one. NUMERIC(20,0) for the
  -- reason 0001 gives the run's seed.
  generation_seed NUMERIC(20,0),
  PRIMARY KEY (run_id, seq),
  CONSTRAINT generation_seq_is_an_order CHECK (seq >= 0),
  CONSTRAINT generation_counts_are_counts
    CHECK (output_count >= 0 AND (resident IS NULL OR resident >= 0))
);
