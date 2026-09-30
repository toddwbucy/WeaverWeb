-- weaver-web store, per weaver-web-Spec section 2.
--
-- The store has two halves and they are written by different paths. Sections
-- 2.1 and 2.2 are what the instrument recorded, landed by the ingest of
-- section 3.1 and never by a surface. Sections 2.3 through 2.5 are what the
-- engineer authored, landed by the authoring path of section 3.2 and never by
-- the ingest. Section 2.6 belongs to neither half and is written by the read
-- that serves it.
--
-- The schema of 2026-08-24 is replaced whole rather than migrated, on the
-- operator's ruling of 2026-09-07 recorded at issue #454: its five tables
-- share no column any of these seven wants, so a migration between them would
-- be a drop and a create wearing a migration's name. Git is the archive.

-- The record spells several members as unsigned 64-bit values and SQL has
-- no unsigned integer, so the widths below are chosen rather than inherited
-- and are stated here once. The seed takes NUMERIC(20,0), which holds the
-- full unsigned range exactly, because the records on hand carry seeds
-- above the signed maximum. Positions, ranks and depths take INTEGER: a
-- position is the resident length at the draw, a rank is bounded by the
-- field election's depth, and neither approaches two billion in a session.
-- Token identifiers take BIGINT, a vocabulary being far under it and the
-- width costing nothing on a column that is not the table's key.

-- =====================================================================
-- The recorded half. Written only by the ingest of Spec section 3.1.
-- No surface writes here, which is what Spec section 6's rule means.
-- =====================================================================

-- Spec 2.2. Everything identifying the conditions lives in the run's row.
CREATE TABLE run (
  run_id           TEXT PRIMARY KEY,

  -- The tuple. Spec 2.2: a reading without its tuple is a reading of an
  -- unnamed compound.
  record_identity  TEXT NOT NULL,

  -- The record spells the seed as an unsigned 64-bit value and the suite's
  -- own fixtures carry values above the signed maximum, so BIGINT refuses
  -- them with `bigint out of range` and every run with the high bit set
  -- would be unrecordable. NUMERIC(20,0) holds the full unsigned range
  -- exactly and reads as the number it is, where a BIGINT reinterpreted in
  -- two's complement would cost every reader the knowledge that it was.
  seed             NUMERIC(20,0),
  sampler          JSONB NOT NULL,
  device           TEXT NOT NULL,
  compute_precision TEXT NOT NULL,
  engine           JSONB NOT NULL,
  batching         JSONB NOT NULL,
  field_depth      INTEGER,
  task_source      TEXT,
  task_identity    TEXT,
  boundary_set     JSONB NOT NULL,

  -- Spec 2.2: lineage stands outside tuple equality, so two rows differing
  -- only in these hold the same tuple and stay comparable.
  parent_run_id    TEXT REFERENCES run(run_id),
  branch_position  INTEGER,

  -- Spec 2.2: whether a token was forced, and which.
  forced_position  INTEGER,
  forced_token     TEXT,

  ingested_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

  -- Spec 2.3: the sentinel is never a record identity. An import that
  -- cannot compute the value registers nothing, and a run carrying the
  -- empty string joins to nothing rather than to everything.
  CONSTRAINT run_record_identity_is_not_the_sentinel
    CHECK (record_identity <> ''),
  CONSTRAINT run_forced_is_stated_whole
    CHECK ((forced_position IS NULL) = (forced_token IS NULL)),
  CONSTRAINT run_branch_is_stated_whole
    CHECK ((parent_run_id IS NULL) = (branch_position IS NULL))
);

CREATE INDEX run_by_record_identity ON run (record_identity);

-- Spec 2.1. The address is the run, the turn, and the position, and the
-- composite of the three is the primary key. `position` is the resident
-- length at the draw and not an ordinal within the turn.
CREATE TABLE position (
  run_id       TEXT NOT NULL REFERENCES run(run_id) ON DELETE CASCADE,
  turn         TEXT NOT NULL,
  position     INTEGER NOT NULL,

  token_id     BIGINT NOT NULL,
  token_text   TEXT NOT NULL,

  -- Absent-not-empty, per Spec section 6. Entropy rides every generation
  -- unconditionally and is NOT NULL. Surprisal rides only where its
  -- election stands, so its absence is a null and never a zero.
  entropy      DOUBLE PRECISION NOT NULL,
  surprisal    DOUBLE PRECISION,

  -- Spec 2.1: the ranked alternatives with their probability mass, at the
  -- depth the declaration's field election kept. The depth is the
  -- operator's ruling and not this crate's parameter.
  alternatives JSONB NOT NULL,

  -- Spec 2.1: `realized` is a rank and not a token, so the row carries the
  -- rank as the record spells it and the drawn token resolved beside it,
  -- and neither is presented as the other.
  realized     INTEGER NOT NULL,

  -- Spec 2.1: raw residual rides alongside rather than a projected readout,
  -- so a lens refitted later can read a run captured earlier.
  residual     BYTEA,

  PRIMARY KEY (run_id, turn, position)
);

-- Spec 2.7. The secondary index exists so the largest spikes in a run are
-- reachable without pulling the run down.
--
-- It is partial, and that is absent-not-empty in the index rather than only
-- at the view. `surprisal` is null where its election did not stand, and in
-- PostgreSQL `DESC` implies `NULLS FIRST`, so a plain descending index puts
-- the rows with no surprisal at the head of every run and answers "the
-- largest spikes" with absences. Measured on 200,000 rows: the first row
-- returned for a run was one whose surprisal is null.
--
-- A row whose election did not stand is not a small spike and not a large
-- one. It is outside the question, so it is outside the index, and a reader
-- asking for spikes says `WHERE surprisal IS NOT NULL` and thereby says
-- which rows it is asking about. A query that omits the clause is asking
-- something the election did not answer, and it will sort rather than being
-- served a wrong answer quickly.
CREATE INDEX position_by_surprisal ON position (run_id, surprisal DESC)
  WHERE surprisal IS NOT NULL;

-- =====================================================================
-- The authored half. Written only by the authoring path of Spec 3.2.
--
-- Spec 3.2: every authored row carries a version, and it is the store's own
-- counter rather than anything the author supplies. Writes are ordered on
-- the row rather than idempotent.
--
-- No trigger or rule below advances that counter, and none should. The
-- authoring path reads the version, writes against it, and advances it by
-- one in the same transaction, refusing where the stored version has moved.
-- The column carries the counter and the write path carries the ordering,
-- the same division as the author member below.
--
-- Spec 3.2: every authored row names its author and the member is nullable,
-- its null meaning the store could not name an author when the row was
-- written and never meaning the operator.
--
-- The column carries no default, and that is all the column can carry. SQL
-- gives a nullable column with no default the same row for an omitted
-- member as for an explicit null, so the schema can refuse a default that
-- guesses and cannot refuse a write that forgot to ask. Spec 3.2 puts that
-- refusal at the boundary for this reason: the authoring path passes the
-- member on every write, in the same transaction, and names it when it
-- refuses. Adding a DEFAULT to any author column below is the defect this
-- comment exists to make visible.
-- =====================================================================

-- Spec 2.3. The catalog, keyed by the artifact's weights identity rather
-- than by a path, because a path is where a file sits and an identity is
-- what it is.
CREATE TABLE artifact (
  artifact_id   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

  -- Spec 2.3: the weights identity is the set of per-file content digests
  -- keyed by file name. Equality is set equality and no order is imposed.
  -- jsonb normalizes object key order, so a UNIQUE on this column is set
  -- equality over a keyed map, and this table asks for no rolled-up digest
  -- of its own devising.
  --
  -- What jsonb does NOT do is reject a duplicate key: it keeps the last
  -- value and drops the rest silently. That does not reach this member,
  -- because a directory holds no two files of one name, so a well-formed
  -- digest map has no duplicate to collapse. A malformed one collapses
  -- rather than refusing, and catching that is the import's job at Spec
  -- 3.2 and not this column's.
  file_digests  JSONB NOT NULL UNIQUE,

  -- Spec 2.3: the provenance chain is recorded and is not an identity.
  -- Either a repository and revision, or a source artifact with the
  -- converter and pin that produced it.
  repository    TEXT,
  revision      TEXT,
  source_artifact_id BIGINT REFERENCES artifact(artifact_id),
  converter     TEXT,
  converter_pin TEXT,

  author        TEXT,
  version       BIGINT NOT NULL DEFAULT 1,

  CONSTRAINT artifact_digests_is_an_object
    CHECK (jsonb_typeof(file_digests) = 'object' AND file_digests <> '{}'::jsonb),
  CONSTRAINT artifact_provenance_is_one_shape_or_the_other
    CHECK (
      (repository IS NOT NULL) = (revision IS NOT NULL)
      AND (source_artifact_id IS NOT NULL) = (converter IS NOT NULL)
      AND (converter IS NOT NULL) = (converter_pin IS NOT NULL)
    )
);

-- Spec 2.3: the row carries every record identity its weights have been
-- admitted under, and the join from a run is a lookup on that set.
CREATE TABLE artifact_record_identity (
  artifact_id     BIGINT NOT NULL REFERENCES artifact(artifact_id) ON DELETE CASCADE,
  record_identity TEXT NOT NULL,
  is_split_gguf   BOOLEAN NOT NULL DEFAULT FALSE,
  PRIMARY KEY (artifact_id, record_identity),
  CONSTRAINT artifact_identity_is_not_the_sentinel
    CHECK (record_identity <> '')
);

-- Spec 2.3 and Spec 9. A record identity names at most one catalog row for
-- a file or a directory artifact. The renamed split GGUF is the one
-- exception, per Spec section 10: a split's hash covers its shards' bytes
-- and not their names, so two sets of identical bytes under different stems
-- are one record identity and two weights identities, and the lookup
-- reports that as ambiguous rather than picking.
CREATE UNIQUE INDEX artifact_record_identity_names_one_row
  ON artifact_record_identity (record_identity)
  WHERE NOT is_split_gguf;

-- Spec 2.3: presence is a dated observation by a named reporter, and it is
-- advisory. A box that has not reported is unknown rather than empty. No
-- load consults this.
CREATE TABLE artifact_presence (
  artifact_id  BIGINT NOT NULL REFERENCES artifact(artifact_id) ON DELETE CASCADE,
  box          TEXT NOT NULL,
  reporter     TEXT NOT NULL,
  confirmed_at TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (artifact_id, box)
);

-- Spec 2.3: the lens artifacts fitted to these weights, each versioned as
-- weaver-analysis-PRD section 3 versions them, by the weights content hash.
CREATE TABLE artifact_lens (
  artifact_id   BIGINT NOT NULL REFERENCES artifact(artifact_id) ON DELETE CASCADE,
  lens_identity TEXT NOT NULL,
  fitted_at     TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (artifact_id, lens_identity)
);

-- Spec 2.3: the reference cells taken against it.
CREATE TABLE artifact_reference_cell (
  artifact_id BIGINT NOT NULL REFERENCES artifact(artifact_id) ON DELETE CASCADE,
  cell        TEXT NOT NULL,
  taken_at    TIMESTAMPTZ NOT NULL,
  PRIMARY KEY (artifact_id, cell)
);

-- Spec 2.4. The saved configurations, loadable at any time, which is what
-- makes them a table rather than a draft buffer.
CREATE TABLE declaration (
  declaration_id  BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

  -- The declaration as authored, whole, in the shape weaver-types-Spec
  -- section 2 defines.
  body            JSONB NOT NULL,

  -- Spec 2.4: the corpus commit its field shape was written against. When
  -- the floor moves, the pin says so and `validate` refuses in a way the
  -- surface can name. This is not the row's version below.
  corpus_commit   TEXT NOT NULL,

  parent_declaration_id BIGINT REFERENCES declaration(declaration_id),
  the_one_thing_that_moved TEXT,

  -- Spec 2.4: the last answer is a reading and not a verdict, so it is
  -- stored with when it was given and a surface re-asks rather than
  -- treating it as current.
  last_validate_answer JSONB,
  last_validate_at     TIMESTAMPTZ,

  author          TEXT,
  version         BIGINT NOT NULL DEFAULT 1,

  CONSTRAINT declaration_reading_is_dated
    CHECK ((last_validate_answer IS NULL) = (last_validate_at IS NULL))
);

-- Spec 2.5. The claims registered by the staging surface, in the five
-- states of Spec 5.1.
CREATE TABLE staged_experiment (
  experiment_id   BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

  state           TEXT NOT NULL,
  state_changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),

  parent_run_id   TEXT REFERENCES run(run_id),
  branch_position INTEGER,
  forced_token    TEXT,

  parent_declaration_id BIGINT REFERENCES declaration(declaration_id),

  -- Spec 5.2: the diff is split by when it takes effect.
  diff_at_load    JSONB,
  diff_at_turn    JSONB,

  -- Spec 2.5: the question the engineer meant to ask, which nothing
  -- upstream knows and nothing else in this store holds. Spec 3.2: it is
  -- not a whole fact without the engineer, which is the author below.
  question        TEXT NOT NULL,

  author          TEXT,
  version         BIGINT NOT NULL DEFAULT 1,

  CONSTRAINT staged_experiment_state_is_one_of_the_five
    CHECK (state IN ('draft', 'registered', 'queued', 'running', 'returned'))
);

-- Spec 2.5: the runs it produced, where it ran. An experiment that never
-- ran keeps its row, which is pre-registration falling out of the interface
-- rather than being imposed on it.
CREATE TABLE staged_experiment_run (
  experiment_id BIGINT NOT NULL REFERENCES staged_experiment(experiment_id) ON DELETE CASCADE,
  run_id        TEXT NOT NULL REFERENCES run(run_id),
  PRIMARY KEY (experiment_id, run_id)
);

-- =====================================================================
-- Neither half. Spec 2.6, written by the read that serves it and by
-- nothing else.
-- =====================================================================

CREATE TABLE recorded_query (
  query_id      BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,

  query_text    TEXT NOT NULL,
  reader_name   TEXT NOT NULL,
  reader_version TEXT NOT NULL,

  -- Spec 2.6: the lens artifact it read through, where it read one, and the
  -- weights that lens was fitted to. A reading whose lens was fitted to
  -- other weights and does not say so is the one thing this row exists to
  -- prevent.
  lens_identity        TEXT,
  lens_fitted_weights  JSONB,

  taken_at      TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT recorded_query_lens_is_stated_whole
    CHECK ((lens_identity IS NULL) = (lens_fitted_weights IS NULL))
);

-- Spec 2.6: a query that cannot name every run it addressed is not recorded
-- and not quotable, because a reader that cannot say what it read cannot be
-- rerun by anyone. The rows are required; that at least one exists is the
-- write path's to enforce in the same transaction.
CREATE TABLE recorded_query_run (
  query_id BIGINT NOT NULL REFERENCES recorded_query(query_id) ON DELETE CASCADE,
  run_id   TEXT NOT NULL REFERENCES run(run_id),
  PRIMARY KEY (query_id, run_id)
);
