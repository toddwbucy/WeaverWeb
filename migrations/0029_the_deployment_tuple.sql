-- **The deployment tuple** (Spec 6.2, act of 2026-11-16).
--
-- The placement page keeps the devices on the setup, beside the context
-- length and the turn context 0026 holds. Confirm writes one tuple per
-- setup, immutable, naming the setup, the pulls, the person who confirmed
-- and when, and every member the box is laid from, each from its source.
-- From Confirm on, the setup and its model rows never change either.
-- Confirm is an admin write through `src/store/tuple.rs`, audited with the
-- target kind `setup`, naming the setup.
--
-- **Each CHECK is false, never unknown, on a missing value**, 0014's rule.

-- **The devices' rule** (Spec 6.2): a set of one card's ordinal at least,
-- each from 0, held in ascending order, the shard order, with no ordinal
-- twice and none missing.
CREATE FUNCTION ordinals_ascend(ordinals INTEGER[]) RETURNS BOOLEAN AS $$
  SELECT COALESCE(
    array_ndims(ordinals) = 1
    AND array_position(ordinals, NULL) IS NULL
    AND 0 <= ALL (ordinals)
    AND ordinals = ARRAY(SELECT DISTINCT o FROM unnest(ordinals) AS o ORDER BY o),
    FALSE
  );
$$ LANGUAGE sql IMMUTABLE;

-- The devices the placement page keeps, from the survey's report, and none
-- until it is kept.
ALTER TABLE agent_setup
  ADD COLUMN devices INTEGER[] CHECK (devices IS NULL OR ordinals_ascend(devices));

-- The tuple names the base's identity row it read, by its digest too.
ALTER TABLE base_identity
  ADD CONSTRAINT base_identity_names_its_digest UNIQUE (repository, commit_id, choice, blake3);

CREATE TABLE deployment_tuple (
  tuple_id TEXT PRIMARY KEY CHECK (tuple_id IS NOT NULL AND tuple_id ~ '^dt-[0-9a-f]{16}$'),
  setup_id TEXT NOT NULL REFERENCES agent_setup (setup_id),
  confirmed_by TEXT NOT NULL REFERENCES person (person_id),
  confirmed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  -- **The decoder**: its pull, verified, and the pull's record of each part
  -- in split order, an array of objects naming the part's path under the
  -- models mount, its blob, its SHA-256 and its bytes.
  decoder_pull TEXT NOT NULL REFERENCES model_pull (pull_id),
  decoder_repository TEXT NOT NULL CHECK (
    decoder_repository IS NOT NULL
    AND decoder_repository ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?/[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
    AND position('--' IN decoder_repository) = 0
    AND position('..' IN decoder_repository) = 0
  ),
  decoder_commit TEXT NOT NULL CHECK (decoder_commit IS NOT NULL AND decoder_commit ~ '^[0-9a-f]{40}$'),
  decoder_choice TEXT NOT NULL CHECK (
    decoder_choice IS NOT NULL AND octet_length(decoder_choice) BETWEEN 1 AND 1024
  ),
  decoder_parts JSONB NOT NULL CHECK (
    decoder_parts IS NOT NULL
    AND jsonb_typeof(decoder_parts) = 'array'
    AND jsonb_array_length(decoder_parts) >= 1
  ),
  -- **The adapter**, likewise, and its scale, a finite number.
  adapter_pull TEXT NOT NULL REFERENCES model_pull (pull_id),
  adapter_repository TEXT NOT NULL CHECK (
    adapter_repository IS NOT NULL
    AND adapter_repository ~ '^[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?/[A-Za-z0-9_]([A-Za-z0-9._-]{0,94}[A-Za-z0-9_])?$'
    AND position('--' IN adapter_repository) = 0
    AND position('..' IN adapter_repository) = 0
  ),
  adapter_commit TEXT NOT NULL CHECK (adapter_commit IS NOT NULL AND adapter_commit ~ '^[0-9a-f]{40}$'),
  adapter_choice TEXT NOT NULL CHECK (
    adapter_choice IS NOT NULL AND octet_length(adapter_choice) BETWEEN 1 AND 1024
  ),
  adapter_parts JSONB NOT NULL CHECK (
    adapter_parts IS NOT NULL
    AND jsonb_typeof(adapter_parts) = 'array'
    AND jsonb_array_length(adapter_parts) >= 1
  ),
  adapter_scale DOUBLE PRECISION NOT NULL CHECK (
    adapter_scale IS NOT NULL
    AND adapter_scale NOT IN ('NaN'::double precision, 'Infinity'::double precision, '-Infinity'::double precision)
  ),
  -- **The base's identity**, the console's hash of the decoder's cached
  -- parts, the row of 0028 it was read from.
  base_blake3 TEXT NOT NULL CHECK (base_blake3 IS NOT NULL AND base_blake3 ~ '^[0-9a-f]{64}$'),
  context_length BIGINT NOT NULL CHECK (context_length IS NOT NULL AND context_length >= 1),
  -- **The box's max-tokens-per-turn** the context length was bounded by,
  -- the console's one constant (`toddwbucy/WeaverAgent#580`), carried here
  -- so a later per-agent value changes the tuples it binds and nothing else.
  max_tokens_per_turn BIGINT NOT NULL CHECK (
    max_tokens_per_turn IS NOT NULL AND max_tokens_per_turn >= 1
  ),
  turn_context TEXT NOT NULL CHECK (
    turn_context IS NOT NULL AND turn_context IN ('continuous', 'independent')
  ),
  virtual_machine BOOLEAN NOT NULL,
  devices INTEGER[] NOT NULL CHECK (ordinals_ascend(devices)),
  -- **The box's members**, as the box reported them: the image version,
  -- the driver, the cards by identity, the guest CPU, the machine and the
  -- drives.
  box_members JSONB NOT NULL CHECK (
    box_members IS NOT NULL AND jsonb_typeof(box_members) = 'object'
  ),
  CONSTRAINT deployment_tuple_one_per_setup UNIQUE (setup_id),
  -- A turn never exceeds the whole context.
  CONSTRAINT deployment_tuple_a_context_holds_a_turn CHECK (
    context_length IS NOT NULL AND max_tokens_per_turn IS NOT NULL
    AND context_length >= max_tokens_per_turn
  ),
  CONSTRAINT deployment_tuple_names_the_base_identity_read
    FOREIGN KEY (decoder_repository, decoder_commit, decoder_choice, base_blake3)
    REFERENCES base_identity (repository, commit_id, choice, blake3)
);

-- **A tuple never changes and is never removed**, a trigger of the kind
-- 0023 uses.
CREATE FUNCTION deployment_tuple_is_fixed() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'a deployment tuple never changes: % on deployment_tuple is refused', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER deployment_tuple_refuses_an_update_and_a_delete
  BEFORE UPDATE OR DELETE ON deployment_tuple
  FOR EACH ROW EXECUTE FUNCTION deployment_tuple_is_fixed();

CREATE TRIGGER deployment_tuple_refuses_truncate
  BEFORE TRUNCATE ON deployment_tuple
  FOR EACH STATEMENT EXECUTE FUNCTION deployment_tuple_is_fixed();

-- **A confirmed setup never changes**, nor any of its model rows, so a
-- setup is confirmed once and its rows stay what Confirm read.
CREATE FUNCTION agent_setup_confirmed_is_fixed() RETURNS trigger AS $$
BEGIN
  RAISE EXCEPTION 'a confirmed setup never changes: % on % is refused', TG_OP, TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_setup_refuses_a_change_once_confirmed
  BEFORE UPDATE OR DELETE ON agent_setup
  FOR EACH ROW WHEN (OLD.state = 'confirmed')
  EXECUTE FUNCTION agent_setup_confirmed_is_fixed();

CREATE FUNCTION agent_setup_model_of_a_confirmed_setup_is_fixed() RETURNS trigger AS $$
DECLARE
  touched TEXT[] := ARRAY[]::TEXT[];
BEGIN
  IF TG_OP <> 'INSERT' THEN
    touched := touched || OLD.setup_id;
  END IF;
  IF TG_OP <> 'DELETE' THEN
    touched := touched || NEW.setup_id;
  END IF;
  IF EXISTS (
    SELECT 1 FROM agent_setup WHERE state = 'confirmed' AND setup_id = ANY (touched)
  ) THEN
    RAISE EXCEPTION 'a confirmed setup never changes: % on agent_setup_model is refused', TG_OP;
  END IF;
  IF TG_OP = 'DELETE' THEN
    RETURN OLD;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_setup_model_refuses_a_change_once_confirmed
  BEFORE INSERT OR UPDATE OR DELETE ON agent_setup_model
  FOR EACH ROW EXECUTE FUNCTION agent_setup_model_of_a_confirmed_setup_is_fixed();

-- A truncation fires no row trigger, so a statement trigger refuses one
-- while any setup is confirmed. The setup's own truncation is refused by
-- the references to it, and, cascaded, by the tuple's guard.
CREATE FUNCTION agent_setup_model_truncated_once_confirmed() RETURNS trigger AS $$
BEGIN
  IF EXISTS (SELECT 1 FROM agent_setup WHERE state = 'confirmed') THEN
    RAISE EXCEPTION 'a confirmed setup never changes: TRUNCATE on agent_setup_model is refused';
  END IF;
  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER agent_setup_model_refuses_truncate_once_confirmed
  BEFORE TRUNCATE ON agent_setup_model
  FOR EACH STATEMENT EXECUTE FUNCTION agent_setup_model_truncated_once_confirmed();
