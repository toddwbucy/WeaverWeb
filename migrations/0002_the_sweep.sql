-- The sweep's members, per weaver-web-Spec sections 2.2, 2.5 and 5.4 as of
-- 2026-09-08 at PR #518. The first migration expressed section 2 as it stood
-- on 2026-09-07 and the sweep act moved four members into it after.

-- Spec 2.2: the emission's signature, derived once at ingest and standing
-- outside tuple equality with lineage, because it says what the run produced
-- and not what it ran under. Its representation is section 10's open
-- election, so the column is jsonb, which commits to no shingle unit, width
-- or scope, and the election settles what stands in it.
ALTER TABLE run ADD COLUMN signature JSONB;

-- Spec 2.2 and 3.1: the first position at which a branch's token path left
-- its parent's, derived at ingest and never at the read, because comparing
-- two token paths is a walk whatever it is called. Null where the paths never
-- part, per section 6's rule, an arm that reproduced its parent being a
-- different fact from one that parted at position zero. It is lineage, so it
-- stands only where a parent does.
ALTER TABLE run ADD COLUMN parting_position INTEGER;
ALTER TABLE run ADD CONSTRAINT run_parting_needs_a_parent
  CHECK (parting_position IS NULL OR parent_run_id IS NOT NULL);

-- Spec 2.5 and 5.4: a sweep is one row holding one member of the tuple and
-- the set of values it takes, frozen together at registration. Both are null
-- where the diff is not a sweep and both stand where it is.
ALTER TABLE staged_experiment ADD COLUMN swept_member TEXT;
ALTER TABLE staged_experiment ADD COLUMN swept_values JSONB;
ALTER TABLE staged_experiment ADD CONSTRAINT staged_experiment_sweep_is_stated_whole
  CHECK ((swept_member IS NULL) = (swept_values IS NULL));
ALTER TABLE staged_experiment ADD CONSTRAINT staged_experiment_sweep_values_is_an_array
  CHECK (swept_values IS NULL OR jsonb_typeof(swept_values) = 'array');

-- Spec 4, the fourth read: each value with its run where one exists. The
-- unit of the read is the value and not the run, so the association from a
-- run back to the value it was produced under is explicit rather than
-- recovered by matching the run's tuple against the set. Null for a run an
-- experiment produced under no sweep.
ALTER TABLE staged_experiment_run ADD COLUMN swept_value JSONB;
