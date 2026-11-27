-- **A reset VM is a new agent** (Spec 2.12, 8.2; the operator's ruling of
-- 2026-11-27, and his words of 2026-11-24 on a new image for an agent that
-- stands): every pairing writes a new row, and no write moves a row to
-- another box. The replace-box write leaves the code, and this migration
-- closes it in the schema with three refusals.
--
-- 0030 is frozen, so nothing of it is edited. The rows a replace-box write
-- made stay as written: an `agent_setup` row whose `paired_as` reads
-- `replace box` stays valid under 0030's CHECK, and each `agent_box` row
-- stays readable, 0030's triggers still refusing its update, delete and
-- truncate. The refusals are triggers and not a narrowed CHECK, so the rows
-- that stand need no exception.

-- 1. No setup records a new replace-box write: an insert, or an update that
-- sets `paired_as` to `replace box` where it read otherwise, is refused. An
-- update of a setup written before, leaving its `paired_as` as it was,
-- passes.
CREATE FUNCTION agent_setup_pairs_as_new() RETURNS trigger AS $$
BEGIN
  IF NEW.paired_as = 'replace box'
     AND (TG_OP = 'INSERT' OR OLD.paired_as IS DISTINCT FROM NEW.paired_as) THEN
    RAISE EXCEPTION 'every pairing writes a new row: setup % is not paired by replace box', NEW.setup_id;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_setup_pairs_as_new
  BEFORE INSERT OR UPDATE OF paired_as ON agent_setup
  FOR EACH ROW EXECUTE FUNCTION agent_setup_pairs_as_new();

-- 2. `agent_box` takes no new row: it is the history of boxes rows held
-- before the ruling, and nothing moves a box since.
CREATE FUNCTION agent_box_is_closed() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'no row moves to another box: agent_box takes no new row';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_box_refuses_an_insert
  BEFORE INSERT ON agent_box
  FOR EACH ROW EXECUTE FUNCTION agent_box_is_closed();

-- 3. A row's box never moves: 0030's trigger, which let it move beside its
-- history's row, is replaced by one that refuses any change of it.
DROP TRIGGER agent_box_changes_only_beside_its_history ON agent;
DROP FUNCTION agent_box_moves_beside_its_history();

CREATE FUNCTION agent_box_never_moves() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'the box of % never moves: a VM on another box pairs as a new agent', OLD.agent_id;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_box_never_moves
  BEFORE UPDATE OF box ON agent
  FOR EACH ROW WHEN (NEW.box IS DISTINCT FROM OLD.box)
  EXECUTE FUNCTION agent_box_never_moves();
