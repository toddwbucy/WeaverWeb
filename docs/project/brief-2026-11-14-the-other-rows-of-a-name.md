# Brief: the other rows of a name claim only what was read (2026-11-14)

From the Planner, 2026-11-14, on the Executor's note after #109 merged. One
small pull request against `main`, after #110 is on `main`.

## What was found

The agent page's Declare pane (#109, Spec 2.13) lists the other live
register rows that carry the agent's name. Its sentence reads "The same
declaration stands behind the other live register rows of this name". It
is printed whatever was read of the head. With nothing saved under the name
the pane says "No declaration is saved under the name X." and then claims a
declaration behind the other rows. The sentence is false on a path that is
reached as soon as one name stands on two boxes. It does not show today:
the console's register holds one live row of each name.

## The ruling

- Under nothing saved, the other rows' sentence says they share the name
  and that nothing is saved under it.
- A test, red first.

## Picked up on the way

The same claim is printed where the head could not be read, which states a
fact the page did not read. The sentence then names the rows and says
nothing of a declaration. Where a declaration is saved, including a head
this build does not read, the sentence is unchanged and true.

## Scope

- `src/surfaces/declare_pane.rs` and its template, the Declare pane's tests,
  and Spec 2.13's sentence on the other rows with its section 9 row.
- No migration, no change to the link, the connectors or the stage.
