-- **An appliance's enrollment** (Spec 8.2, 6.2, 2.12; the enrollment's
-- code act of 2026-11-17).
--
-- The setup row gains its pairing's link to the register row the exchange
-- writes, or whose box a replacement sets, and which of the two it was;
-- and the misses of the pairing page's code field, kept on the row so a
-- restart does not reset them. The register row gains "paired, setup
-- running", and a history of the boxes each row held, which the row's box
-- changes only beside.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

-- **The pairing's link** (Spec 6.2): none while the setup is in its
-- pairing step, and once the exchange commits, the row and which write it
-- was, `pair` or `replace box`, the two together or neither.
ALTER TABLE agent_setup
  ADD COLUMN agent_id TEXT REFERENCES agent (agent_id),
  ADD COLUMN paired_as TEXT CHECK (paired_as IS NULL OR paired_as IN ('pair', 'replace box')),
  ADD COLUMN paired_at TIMESTAMPTZ,
  ADD CONSTRAINT agent_setup_a_pairing_names_its_row_and_its_write CHECK (
    (agent_id IS NULL) = (paired_as IS NULL) AND (agent_id IS NULL) = (paired_at IS NULL)
  ),
  -- **The code field's misses** (Spec 8.2): five on one setup close the
  -- field for ten minutes from the fifth. The count runs from the field's
  -- last opening, and the fifth sets the close and starts the count again.
  ADD COLUMN code_misses SMALLINT NOT NULL DEFAULT 0 CHECK (
    code_misses IS NOT NULL AND code_misses BETWEEN 0 AND 4
  ),
  ADD COLUMN code_closed_until TIMESTAMPTZ;

-- **"Paired, setup running"** (Spec 2.12): true from the pairing's write
-- until either connector's service is first admitted, which the admission
-- clears in its own transaction. Registration never sets it.
ALTER TABLE agent
  ADD COLUMN setup_running BOOLEAN NOT NULL DEFAULT FALSE;

-- **The boxes a row held** (Spec 2.12, 8.2): one row per replacement, the
-- box the agent's row held until the replace-box write, and the setup
-- whose pairing replaced it. Never edited and never deleted.
CREATE TABLE agent_box (
  agent_id TEXT NOT NULL REFERENCES agent (agent_id),
  box TEXT NOT NULL CHECK (box IS NOT NULL AND octet_length(box) BETWEEN 1 AND 255),
  replaced_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  setup_id TEXT NOT NULL REFERENCES agent_setup (setup_id),
  PRIMARY KEY (agent_id, replaced_at)
);

CREATE FUNCTION agent_box_is_kept() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'a box a row held is kept: % on agent_box is refused', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_box_refuses_a_change
  BEFORE UPDATE OR DELETE ON agent_box
  FOR EACH ROW EXECUTE FUNCTION agent_box_is_kept();

CREATE TRIGGER agent_box_refuses_truncate
  BEFORE TRUNCATE ON agent_box
  FOR EACH STATEMENT EXECUTE FUNCTION agent_box_is_kept();

-- **A row's box changes only beside its history's row** (Spec 2.12): an
-- update that moves the box stands only where the same transaction wrote
-- the box the row held into `agent_box`, `now()` being the transaction's
-- start, so the replace-box write is the one write that moves it.
CREATE FUNCTION agent_box_moves_beside_its_history() RETURNS trigger AS $$
BEGIN
  IF NEW.box IS DISTINCT FROM OLD.box AND NOT EXISTS (
    SELECT 1 FROM agent_box
    WHERE agent_id = OLD.agent_id AND box = OLD.box AND replaced_at = now()
  ) THEN
    RAISE EXCEPTION 'the box of % moves only beside its history''s row', OLD.agent_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_box_changes_only_beside_its_history
  BEFORE UPDATE OF box ON agent
  FOR EACH ROW EXECUTE FUNCTION agent_box_moves_beside_its_history();
