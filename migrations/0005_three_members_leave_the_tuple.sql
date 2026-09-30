-- Two members of the run's tuple had no producer anywhere in the corpus and
-- leave it, per weaver-web-Spec section 2.2 as revised 2026-09-09 on the
-- operator's rulings at issue #532. The third, the declared boundary set,
-- stays and is written empty, which is a fact about every run this ingest
-- can meet rather than a default.

-- The batching election records a choice the program does not offer:
-- weaver-spu-Spec has one forward per prompt and never a batch, and every
-- use of the word elsewhere in the corpus is the serving technique the
-- architecture refuses. A member nobody can fill is not a member.
ALTER TABLE run DROP COLUMN batching;

-- Precision is the artifact's and not the run's. No organ reports a dtype
-- and the declaration excludes it, quantization being a deployment's
-- knowledge per weaver-spu-Spec, while section 2.2's record identity is
-- already at a grain fine enough to catch a quantization difference. So the
-- tuple loses no distinction, and what a person reads is a label on the
-- artifact, authored by Models at import.
ALTER TABLE run DROP COLUMN compute_precision;
ALTER TABLE artifact ADD COLUMN precision_label TEXT;
