-- The indexes Record's chips and its page need, per weaver-web-Spec
-- sections 2.7 and 4 as revised 2026-09-11.
--
-- The read-five act claimed its filters were index hits on what section 2.7
-- already carried. Two of the four were not: a foreign key creates no index
-- in Postgres, so a branch's siblings were a sequential scan, and the
-- ingest's order had none at all. The review of PR #540 found it. These are
-- the indexes that make the claim true.

-- A branch's siblings, per section 4's read five. The reference is declared
-- REFERENCES run(run_id) in 0001, which constrains and does not index.
CREATE INDEX run_by_parent ON run (parent_run_id);

-- The page's key, newest first. **The identity is in the index because the
-- timestamp is not a total order**: `ingested_at` defaults to now(), which
-- is the transaction's clock, so every run of one ingest shares a value and
-- a cursor on the timestamp alone drops the rest of a tie larger than the
-- page. The pair is unique because run_id is the primary key, so the cursor
-- it carries is exact.
CREATE INDEX run_by_ingest_order ON run (ingested_at DESC, run_id DESC);
