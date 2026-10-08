-- Spec 2.13 and `docs/project/design-2026-10-07-iam.md` section 10: the audit
-- record, the one record of who asked, since nothing about a person crosses
-- to the box.
--
-- **0013 is frozen from here**, as every applied migration is.
--
-- **Two records per act and never a rewrite.** A first record is written
-- before the act and an outcome record naming it after, so an act whose
-- outcome never came is visible as a first record with no second; a refusal
-- at the first gate is one record carrying its refusal. Only
-- `src/store/audit.rs` writes here.
--
-- **The audit never holds** a credential, a key, a passkey or its public
-- key, a challenge, a bearer, an enrollment token or its digest, or a
-- turn's text. A record names its target and its action, and the outcome
-- record says whether the act succeeded, never the act's error text, which
-- can carry a path.
--
-- **Each CHECK is written to be false, never unknown, on a missing value.**
-- A CHECK passes when its expression is NULL, so a comparison on a member
-- that may be missing is guarded by an explicit `IS NOT NULL` beside it,
-- even where the column is declared NOT NULL today, so no CHECK leans on a
-- nullability another migration could change. The one exception is the
-- identity's domain, whose check passes a NULL by design: the column using
-- it decides nullability, the primary key refusing one and `answers` being
-- absent on a first record.

-- Spec 2: a key this store generates is two letters and sixteen hex.
CREATE DOMAIN audit_id AS TEXT CHECK (VALUE ~ '^au-[0-9a-f]{16}$');

CREATE TABLE audit (
  audit_id        audit_id PRIMARY KEY DEFAULT weaver_key('au'),
  at              TIMESTAMPTZ NOT NULL DEFAULT now(),

  -- **The principal and how it was authenticated** (design section 10).
  -- Each method belongs to exactly one principal, so a record's principal
  -- and its method can never disagree. The person's identity has no
  -- foreign key yet: the persons table is act 11's next pull request, whose
  -- migration adds it.
  principal       TEXT NOT NULL,
  person_id       TEXT,
  method          TEXT NOT NULL,
  -- The name the host's command was given with `--author`: an unverified
  -- claim, recorded as one, and only the host's.
  claimed_author  TEXT,

  -- **What was acted on and how** (Spec 2.13): for a register verb, the
  -- server's authority or the agent's row, and the verb.
  target_kind     TEXT NOT NULL,
  target_id       TEXT,
  action          TEXT NOT NULL,

  -- **A second record names the first it answers**, with the outcome; a
  -- refusal carries its refusal; a first record carries neither.
  answers         audit_id REFERENCES audit (audit_id),
  outcome         TEXT,
  refusal         TEXT,

  CONSTRAINT audit_principal_and_method_agree CHECK (
    principal IS NOT NULL AND method IS NOT NULL AND (
      (principal = 'person' AND person_id IS NOT NULL
        AND method IN ('session', 'enrollment token', 'passkey assertion'))
      OR (principal = 'server' AND person_id IS NULL AND method = 'server')
      OR (principal = 'host' AND person_id IS NULL AND method = 'host')
    )
  ),
  CONSTRAINT audit_only_the_host_claims_an_author CHECK (
    claimed_author IS NULL OR (principal IS NOT NULL AND principal = 'host')
  ),
  CONSTRAINT audit_a_record_is_first_outcome_or_refusal CHECK (
    (answers IS NULL AND outcome IS NULL AND refusal IS NULL)
    OR (answers IS NOT NULL AND outcome IS NOT NULL
      AND outcome IN ('ok', 'failed') AND refusal IS NULL)
    OR (answers IS NULL AND outcome IS NULL AND refusal IS NOT NULL)
  ),
  CONSTRAINT audit_an_agent_target_names_its_row CHECK (
    target_kind IS NOT NULL AND (
      target_kind <> 'agent'
      OR (target_id IS NOT NULL AND target_id ~ '^ag-[0-9a-f]{16}$')
    )
  )
);

-- At most one outcome per first record.
CREATE UNIQUE INDEX audit_one_outcome_per_first ON audit (answers)
  WHERE answers IS NOT NULL;

-- **Append-only, against this crate's code paths.** A row trigger refuses
-- every UPDATE and DELETE, and a statement trigger refuses a TRUNCATE, which
-- no row trigger sees. **What the triggers guard is this crate's code
-- paths**: no statement this crate issues, the server's or a host
-- command's, can update, delete or truncate an audit record. They do not
-- guard against a process that deliberately drops them with the owner's
-- rights, which is a compromised server, out of the threat model as the host
-- is; a migration role owning the table, the server connecting with INSERT
-- and SELECT on it alone, would close that, and is the operator's
-- provisioning decision outside act 11.
CREATE FUNCTION audit_is_append_only() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'the audit is append-only: % on audit is refused', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER audit_refuses_update_and_delete
  BEFORE UPDATE OR DELETE ON audit
  FOR EACH ROW EXECUTE FUNCTION audit_is_append_only();

CREATE TRIGGER audit_refuses_truncate
  BEFORE TRUNCATE ON audit
  FOR EACH STATEMENT EXECUTE FUNCTION audit_is_append_only();
