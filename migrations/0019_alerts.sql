-- **Alerts, and the operator's decisions** (Spec 2.14; act 12, PR 1; the
-- operator's rulings of 2026-10-14). A failure that could lose data, an act
-- that failed or whose outcome is unknown, a refusal for a system reason,
-- and lost contact with a connector each become an alert on the agent:
-- everyone holding a grant on it sees it, and it stays until a person
-- holding `operator` on it acknowledges it. A decision prompt is an alert
-- that carries the options the operator chooses among.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

CREATE DOMAIN alert_id AS TEXT CHECK (VALUE ~ '^al-[0-9a-f]{16}$');

CREATE TABLE alert (
  alert_id         alert_id PRIMARY KEY DEFAULT weaver_key('al'),
  agent_id         agent_id NOT NULL REFERENCES agent (agent_id),

  -- **What happened, and to what**: the kind, and its subject, the plane
  -- for lost contact and the act (a verb, or `turn`) otherwise, so one
  -- open alert stands per agent, kind and subject.
  kind             TEXT NOT NULL,
  subject          TEXT NOT NULL,
  -- Who raised it: the server, in this act; the connectors' own entries
  -- join in the next.
  source           TEXT NOT NULL,

  -- **Why**, bounded, and never a turn's text or a secret: admin's refusal
  -- kind and message, a connector's fault, the gate's close reason, a
  -- link's teardown reason.
  summary          TEXT NOT NULL,
  detail           TEXT NOT NULL DEFAULT '',
  -- The first record of the act it concerns, where there is one, **by its
  -- identity's shape and no foreign key**: a key on the audit would let a
  -- `TRUNCATE audit` be refused by the key before the audit's own
  -- append-only trigger, which is the guarantee, as 0017's session names
  -- its passkey without one.
  first_record     audit_id,
  -- **A decision prompt's options**, empty for an alert that only informs.
  options          TEXT[] NOT NULL DEFAULT '{}',

  raised_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
  occurrences      INTEGER NOT NULL DEFAULT 1,
  -- Lost contact restored, where it was: the alert stays until acknowledged.
  restored_at      TIMESTAMPTZ,

  -- **The version an acknowledgement names**: every repeat and every
  -- restoration moves it, so a repeat landing after a person read the page
  -- is never acknowledged unseen.
  version          BIGINT NOT NULL DEFAULT 1,
  acknowledged_at  TIMESTAMPTZ,
  acknowledged_by  person_id REFERENCES person (person_id),

  CONSTRAINT alert_kind_is_known CHECK (
    kind IS NOT NULL AND kind IN (
      'failed', 'unknown', 'refused', 'lost_contact', 'save_point_missed', 'data_at_risk'
    )
  ),
  CONSTRAINT alert_subject_is_given CHECK (
    subject IS NOT NULL AND subject <> '' AND octet_length(subject) <= 64
  ),
  CONSTRAINT alert_source_is_known CHECK (source IS NOT NULL AND source IN ('server')),
  CONSTRAINT alert_summary_is_bounded CHECK (
    summary IS NOT NULL AND summary <> '' AND octet_length(summary) <= 1024
  ),
  CONSTRAINT alert_detail_is_bounded CHECK (
    detail IS NOT NULL AND octet_length(detail) <= 8192
  ),
  CONSTRAINT alert_occurs_at_least_once CHECK (occurrences IS NOT NULL AND occurrences >= 1),
  CONSTRAINT alert_version_is_positive CHECK (version IS NOT NULL AND version >= 1),
  CONSTRAINT alert_acknowledged_by_someone CHECK (
    (acknowledged_at IS NULL) = (acknowledged_by IS NULL)
  )
);

-- **One open alert per agent, kind and subject**: a repeat while it stands
-- unacknowledged updates it (its last occurrence, its count) through an
-- insert that resolves its conflict on this index, so a flapping link
-- raises one alert and not hundreds; an occurrence after an
-- acknowledgement opens a new one.
CREATE UNIQUE INDEX alert_one_open_per_agent_kind_and_subject
  ON alert (agent_id, kind, subject) WHERE acknowledged_at IS NULL;

CREATE INDEX alert_by_agent_newest ON alert (agent_id, last_at DESC);

-- **The audit's targets gain the alert**, by its `al-` identity, for its
-- acknowledgement.
ALTER TABLE audit DROP CONSTRAINT audit_a_target_is_a_known_kind;
ALTER TABLE audit
  ADD CONSTRAINT audit_a_target_is_a_known_kind CHECK (
    target_kind IS NOT NULL
    AND target_kind IN ('authority', 'agent', 'person', 'grant', 'role', 'passkey', 'alert')
  ),
  ADD CONSTRAINT audit_an_alert_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'alert'
      OR (target_id IS NOT NULL AND target_id ~ '^al-[0-9a-f]{16}$')
    )
  );
