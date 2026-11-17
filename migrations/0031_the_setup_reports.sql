-- **The setup's reports** (Spec 8.3, and the enrollment's code act of
-- 2026-11-17).
--
-- `admin-con setup report --stage pairing|drives` hands the survey's report
-- across the paired link, and the console keeps it on the setup the row's
-- pairing linked: the latest of each stage, verbatim, with the time it
-- arrived. **Verbatim is text**, never a parsed column that would rewrite
-- the report, so the console reads nothing of the image's format here.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE TABLE agent_setup_report (
  setup_id TEXT NOT NULL REFERENCES agent_setup (setup_id),
  stage TEXT NOT NULL CHECK (stage IS NOT NULL AND stage IN ('pairing', 'drives')),
  -- One JSON object within 64 KiB, as admin-con checked it.
  report TEXT NOT NULL CHECK (report IS NOT NULL AND octet_length(report) <= 65536),
  arrived_at TIMESTAMPTZ NOT NULL,
  -- **The latest of each stage**: a report run again replaces its stage's
  -- and leaves the other's.
  PRIMARY KEY (setup_id, stage)
);

-- **A confirmed setup never changes** (Spec 6.2, 8.3), so a report for one
-- is refused here too, beside the console's `setup_closed`.
CREATE FUNCTION agent_setup_report_of_a_confirmed_setup_is_refused() RETURNS trigger AS $$
BEGIN
  IF EXISTS (SELECT 1 FROM agent_setup WHERE setup_id = NEW.setup_id AND state = 'confirmed') THEN
    RAISE EXCEPTION 'a confirmed setup never changes: a report for % is refused', NEW.setup_id;
  END IF;
  RETURN NEW;
END
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_setup_report_refuses_a_confirmed_setup
  BEFORE INSERT OR UPDATE ON agent_setup_report
  FOR EACH ROW EXECUTE FUNCTION agent_setup_report_of_a_confirmed_setup_is_refused();
