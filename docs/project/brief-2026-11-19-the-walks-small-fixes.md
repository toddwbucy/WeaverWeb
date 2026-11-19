# Brief: the walk's small fixes

**Date:** 2026-11-19. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the operator's findings from walking the setup pages by hand against a
real appliance on 2026-11-19, and the operator's word of the same day to proceed with
the next iteration of the front end. The operator's verdict on the walk: the
functionality was there, and the layout and the workflow need work. This act takes the
findings that change no flow. The findings that change the flow, or the stage a page
belongs to, are a plan for the operator and are not built here. The act is one stack of
three sub-acts from `main` `e0575e65`.

## Goal

Four things the operator met on the pages are put right, with no route, no migration
and no Spec decision changed.

- The text typed into several inputs could not be read. Those inputs carry no `type`,
  or `type="number"`, so the stylesheet's field rule misses them and the browser's own
  field colors meet the theme's text color.
- The models page asks for a "revision". The operator calls it a branch. The code takes
  a branch, a tag or a 40-hex commit.
- The pane that shows the console's link address reads like a field to fill in. The
  operator took the address for the box's. It is the console's own address, for a box
  to dial.
- The SSH key form takes the operator out of the setup with no way back in view.

## The sub-acts, in order

1. **The inputs.**
   - Every text-like input in the templates carries a `type` the stylesheet's field
     rule names. The context length becomes a text input with a numeric input mode,
     and its range stays in the page's words. The server's own refusals of a length
     that is not whole or is out of range, and of a scale that is not finite, are
     unchanged.
   - The stylesheet is the design system's and does not change. The body says why the
     fix is in the templates.
   - A test reads every input in every template and fails on an input with no `type`,
     or with a type outside the ones the page design admits. It also checks that the
     stylesheet's field rule names the text-like types. It is shown failing with a
     `type` removed, and with `type="number"` put back.
2. **The words.**
   - The models page labels the field "Branch", with a note that a tag or a 40-hex
     commit is taken too. The posted field's name is unchanged. Spec 6.2's Models row
     gains the sentence.
   - The pairing page states the console's link address as plain information: that it
     is Weaver Web's own address, for a box to dial, and never the box's. The pane's
     title, body and footer change. Its data, its warnings and its status do not. The
     pane's stage mark stays where the pane grammar puts it.
   - Where a page's footer states a wait that the page's own state shows is over, the
     footer says so. This joins the sub-act only where the words are chosen by a state
     the page already holds and no Spec decision moves. Otherwise it is filed as an
     issue.
3. **The SSH key page in a new tab.**
   - The link to the SSH key page from the setup opens a new tab, with a sentence in
     the pane saying so. No route changes.

## Shown running

Each sub-act leaves draft only when it is shown through the real listener and store:
the pages fetched with the new types and words, and the tests red under their
perturbations and green after. The gates:

- `cargo fmt`, then the build and both clippys with `--locked`.
- The suite under `setpriv --no-new-privs`, with `NoNewPrivs: 1` logged from inside it.
- The journal: every `sudo` line in the suite's window fails the gate. The body's gate
  line names the window and the count.
- The temporary directory: nothing of the suite's is left behind.

## Ruled out

- No change to the order of the setup's pages, to which stage a page belongs, or to
  the stage marks.
- No command for the box on any page, and no change to what a connector prints.
- No change to the drives page's held panes, to Confirm, or to placement's fields,
  checks and refusals beyond the type of the context length input.
- No route, no migration, no change to a posted field's name.
- No change to the design system's files.

## Review

Each sub-act is a draft child of the stack. A child goes to review when it is shown
running, and merges into the stack on a clean read. The stack then goes to both
reviewers, and the operator takes it to `main`.
