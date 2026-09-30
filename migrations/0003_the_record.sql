-- The run row names the record it came from, per weaver-web-Spec section
-- 2.2 as revised 2026-09-09 on issue #521.
--
-- Both members arrive over the analysis seam per
-- weaver-analysis-web-contract section 2.2, once per run, and are absent
-- rather than defaulted where the emitter could not vouch for them. Neither
-- is a condition the run ran under, so neither joins tuple equality.

-- The identity the trace's runs share, per weaver-trace-PRD section 2.1.
-- Not the session of Spec section 2.8, which is a person's.
ALTER TABLE run ADD COLUMN record_session TEXT;

-- sha256 over the run's own lines as the emitter drained them, lowercase
-- hex. Absent where the emitter did not drain the run whole.
ALTER TABLE run ADD COLUMN record_digest TEXT;
ALTER TABLE run ADD CONSTRAINT run_record_digest_is_sha256_hex
  CHECK (record_digest IS NULL OR record_digest ~ '^[0-9a-f]{64}$');

-- Spec 2.7's family index: a session's runs are one read rather than a
-- walk up parent references.
CREATE INDEX run_by_record_session ON run (record_session);
