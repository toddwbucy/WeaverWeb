-- **The connectors' entries, and a loss that already happened** (Spec 2.14;
-- act 12, PR 2; the operator's rulings of 2026-10-14 and 2026-10-15): a
-- connector sends what it meets upstream as entries, every one a line of
-- the operations log and an alert only where it did not come through
-- WeaverWeb; and a load that reset the agent to its latest save point is
-- an alert of its own kind, with no options.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

-- **A loss that already happened**: `data_lost`, read from the trace's
-- `load` event where it carries `reset`.
ALTER TABLE alert DROP CONSTRAINT alert_kind_is_known;
ALTER TABLE alert ADD CONSTRAINT alert_kind_is_known CHECK (
  kind IS NOT NULL AND kind IN (
    'failed', 'unknown', 'refused', 'lost_contact', 'save_point_missed', 'data_at_risk',
    'data_lost'
  )
);

-- **Where an alert came from**: the server's own door and teardown; the
-- box's `admin.log`, an act that did not come through WeaverWeb; a
-- connector's own failure; the agent's trace.
ALTER TABLE alert DROP CONSTRAINT alert_source_is_known;
ALTER TABLE alert ADD CONSTRAINT alert_source_is_known CHECK (
  source IS NOT NULL AND source IN ('server', 'admin', 'admin-con', 'gate-con', 'trace')
);

-- **The highest sequence taken of each connector run**, so an entry
-- resent after a lost acknowledgement, a reconnection or a server restart
-- is taken once: a connector sends a run's entries in sequence, and one
-- at or below this is a resend. A connector's restart draws a new run.
CREATE TABLE entry_run (
  run_id    TEXT PRIMARY KEY,
  agent_id  agent_id NOT NULL REFERENCES agent (agent_id),
  plane     TEXT NOT NULL,
  last_seq  BIGINT NOT NULL,
  last_at   TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT entry_run_is_drawn CHECK (run_id IS NOT NULL AND run_id ~ '^[0-9a-f]{16}$'),
  CONSTRAINT entry_run_plane_is_known CHECK (plane IS NOT NULL AND plane IN ('admin', 'gate')),
  CONSTRAINT entry_run_sequence_is_positive CHECK (last_seq IS NOT NULL AND last_seq >= 1)
);

-- **The entries taken and not yet applied** (Spec 2.14): an entry's row is
-- written in the transaction that moves its run's mark, before the
-- connector is acknowledged, so an acknowledged entry is durable here; a
-- worker applies it from this copy, its operations-log line and the alert
-- it raises, and deletes the row once both are done. The alert's identity
-- is kept on the row once raised, so a retry after a failed line raises it
-- no second time. A server's restart drains what stands.
CREATE TABLE entry_inbox (
  inbox_id  BIGSERIAL PRIMARY KEY,
  run_id    TEXT NOT NULL REFERENCES entry_run (run_id),
  seq       BIGINT NOT NULL,
  agent_id  agent_id NOT NULL REFERENCES agent (agent_id),
  plane     TEXT NOT NULL,
  source    TEXT NOT NULL,
  entry     JSONB NOT NULL,
  alert_id  alert_id,
  taken_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

  CONSTRAINT entry_inbox_once UNIQUE (run_id, seq),
  CONSTRAINT entry_inbox_plane_is_known CHECK (plane IS NOT NULL AND plane IN ('admin', 'gate')),
  CONSTRAINT entry_inbox_source_is_given CHECK (
    source IS NOT NULL AND source <> '' AND octet_length(source) <= 64
  )
);
