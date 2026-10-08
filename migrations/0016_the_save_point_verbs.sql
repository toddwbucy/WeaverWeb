-- **The save-point verbs** (Spec 7.2, 2.13): WeaverAgent's A3.2
-- (`toddwbucy/WeaverAgent#94` at `43ba391`) adds three fixed command lines at
-- the management plane's door, `save-point`, `restore` and `force-unload`,
-- which a box's operator rule grants and an observer's does not. The role
-- vocabulary admits them, and, on the operator's ruling of 2026-10-08, the
-- seeded `operator` role carries them, mirroring the box's operator rule.

-- The vocabulary, still refusing a missing verb by the containment itself
-- (`<@` is false, never unknown, where an element is NULL).
ALTER TABLE role DROP CONSTRAINT role_verbs_are_the_vocabulary;
ALTER TABLE role ADD CONSTRAINT role_verbs_are_the_vocabulary CHECK (
  verbs IS NOT NULL
  AND verbs <@ ARRAY['show', 'validate', 'load', 'unload', 'stop',
                     'save-point', 'restore', 'force-unload', 'turn']::TEXT[]
);

-- **The `operator` role gains the three only where its verbs are still
-- 0015's seed**, compared as a set, since `role set` keeps the order it was
-- given: a role an admin has written since is the admin's choice and is left
-- as it stands, to be widened, if at all, by `role set`. The version moves
-- with the verbs, so an edit read before this migration is refused as stale.
-- The `admin` role is untouched.
UPDATE role
SET verbs = ARRAY['show', 'validate', 'load', 'unload', 'stop',
                  'save-point', 'restore', 'force-unload', 'turn']::TEXT[],
    version = version + 1
WHERE name = 'operator'
  AND verbs @> ARRAY['show', 'validate', 'load', 'unload', 'stop', 'turn']::TEXT[]
  AND verbs <@ ARRAY['show', 'validate', 'load', 'unload', 'stop', 'turn']::TEXT[];
