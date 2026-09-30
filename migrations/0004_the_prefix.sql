-- The seated prefix's length, per weaver-web-Spec section 2.2 as revised
-- 2026-09-09, third of that date, on issue #527.
--
-- The resident length before the run's first turn's input, which the
-- emitter derives from the run's first generation per
-- weaver-analysis-web-contract section 2.2 and this crate never derives.
-- An address on the run's tape and not a condition, so outside tuple
-- equality. Absent where the emitter sent none, and section 5's whole-run
-- arm reads absent as a branch position it cannot yet take.
ALTER TABLE run ADD COLUMN prefix_length INTEGER;
ALTER TABLE run ADD CONSTRAINT run_prefix_length_is_a_length
  CHECK (prefix_length IS NULL OR prefix_length >= 0);
