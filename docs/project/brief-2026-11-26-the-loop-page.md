# Brief: the loop page

**Date:** 2026-11-26. **From:** the Planner, on the Executor's plan of 01:23 UTC, the Planner's decisions of 01:35 UTC and the Planner's read of this brief at 02:31 UTC. **Stack:** `act/the-loop-page`, cut from act A's base, `act/the-account-after-the-pairing`, at `3c0a7bf2194e4f4065141a7bba2646e8da24f129` once its two children were merged into it (the Planner, 02:35 UTC).

## The goal

Weaver Web keeps the operator's loop for each agent as the text of one wire file, shows it in a text editor with its digest and which loop ran at the last load, and reloads the agent. This is stack B1. Laying, checking and removing a loop on the appliance, and gatecon's enrollment, are stack B2, which waits on the other repository's contract and on the operator.

## The operator's words that bind it (2026-11-25)

- "January runs a loop the operator created in Weaver Web. Any loop."
- "The first created loop is today's compiled loop written as wires, with the trace shown identical."
- "Test is functional: is the loop functional. Evaluate is the evaluation of the agent's behavior in context. Create and test are on the January path. Evaluate follows, one to two weeks."
- "The loop page for January is a text editor on the wire file, plus reload."
- "Eight stages stay. New agent is the first page of stage 1."

## The Planner's decisions this stack carries (2026-11-26, each the operator's to overrule)

- **Decision 5, keeping today's loop in step: no test in either repository reads a sibling checkout.** Each seat sees its own repository alone, so such a test would skip on every seat, and a skip cannot fail. The Planner compares the two files by sha256 at each push word of either seat. The pin test of B1.3 stands.
- **The fixture is taken now** (the Planner, 02:31 UTC, changing decision 5's wait). The attachment of 2026-11-26 01:17 UTC is the other seat's file at its pushed head, both measured at sha256 `e083d6decb4abb1480947ed8d6c8eecb14bf2db877e5f894d0697cf143fa1af0`. B1.3 carries it byte for byte with its pin test. When the other seat's file changes, the Planner sends the new one, and the pin moves in one small commit.
- **Decision 6, who may edit a loop and reload:** a person holding the operator role on that agent. Observers read.

## The other seat's load record and a break's reason

- **The `composer` in the load event, confirmed** (the Planner's ruling to the other seat, 2026-11-26 01:46 UTC): the wires' form is `binary`, `file` and `sha256`, and no more; the three optional members are cut. `sha256` is the wire file's digest, the one the runner sent and the relay accepted. The compiled loop stays `{binary: "worker"}`. B1.5 reads `sha256` by allow-list, and reads a `composer` with no `file` as the compiled loop. Both members are in the trace's vocabulary as built.
- **A break's reason, not final** (the other seat's brief, reported 01:29 UTC): the runner's last line becomes one JSON object, kept in the worker's log, which the agent's admin group reads. It is not in `admin.log`. Weaver Web's ask, a break's reason where a feed of Weaver Web's can read it, stands with that seat. B1 draws no break's reason.

## Already ruled out

- **Laying, checking or removing a loop on the appliance from this stack.** Those are B2's three lines, named by the other seat, and wait on the contract and on the operator's answer on the wire file reaching a root line's standard input.
- **Checking the wire grammar in Weaver Web.** The grammar is the runner's and the laying line's. Weaver Web checks what it can without Python: UTF-8, at most 256 KiB, no NUL.
- **The loop's text in any audit record, log line or error.** The audit holds its digest and its byte count.
- **Script on the page.** The surfaces answer `script-src 'self'`; the editor is a plain `<textarea>` form.
- **Evaluate.** It follows January.

## Where the page sits

Stage 1 Declare, in the Build page's place once Confirm stands: `GET /agents/{agent}/loop`, one page per agent. Its text panes carry `1 Declare`; its reload pane carries `3 Run`, since reload is Run's verbs. The Build pane links to it once Confirm stands. Eight stages stay.

## The children, in order

Each carries its own Spec sections and section 9 rows, passes the repository's gates alone, and measures a perturbation for each claim it adds.

1. **The documents.** Spec 6.2 (the loop page, its place, panes and words), 2.12 (the row's last-load loop), 2.13 (the save's grant and its audit, the text in no record), 9 and 10; PRD stage 1.
2. **The store.** Migration `0035`: each version of an agent's loop text, its sha256, byte count, author and time, and the audit's target kind `loop`. Migration `0036`: the row's last-load loop digest. `store::loop_text`.
   - Tests on `fresh_store`: a save; a stale version refused 409; 262,144 bytes read and 262,145 refused; a byte sequence not UTF-8 refused; a NUL refused; the audit holding the digest and the length and not the text.
   - Perturbations: the text written into the audit; the bound off by one.
3. **The fixture and the page, read only.** `tests/fixtures/loop-wires-today.py`, the attachment of 01:17 UTC, byte for byte, with a test checking its sha256 first. `GET /agents/{agent}/loop`: the stored version's text, or the fixture as the unsaved draft where none is stored; the stored version's digest and number; **beside the stored digest, one plain sentence saying a saved loop is kept in Weaver Web and is not on the appliance**, until the laying lines exist, with a test that pins it, so an operator who saves and reloads is told why the compiled loop runs; which loop ran at the last load. Grant checks as the reading pages take them: no grant answers as an unknown agent, 404.
4. **The save.** `POST /agents/{agent}/loop` in the admin writes' order: identities parsed, the operator grant checked with its one refusal record, the agent resolved, the version checked, then the audited write. Each refusal on a save is one sentence, for example "The file is 262,145 bytes; a wire file is at most 262,144."
5. **Which loop ran.** The load event's `composer` read by allow-list into `0036`'s column, live and replayed: the wires' `sha256`, or the compiled loop where the `composer` has no `file`. Tests against the real listener and a fake relay.
6. **Reload on the page.** `unload` then `load` through the held ask, the second asked only once the first answers `done`, each answer drawn as it came (Spec 7.2), and once the load event lands, which loop ran by its digest. Tests with the fake invoker: each answer drawn; a refused unload stops the reload.
7. **The fix round.**

## The acceptance

- The page shows the wire file as text, the stored digest, the sentence that a saved loop is not on the appliance, and which loop ran at the last load.
- A save by an operator holder stands as a new version; an observer's save is refused; a stale save is refused 409; each byte bound refuses one byte past it.
- No audit record, log line or error holds the loop's text.
- The fixture's sha256 is checked first by its test.
- **The gates:** the repository's gates, alone, with `DATABASE_URL` and `PGUSER` set, the count reported and nothing skipped.

## The scope boundary

- **In:** the page, its store, its save, which loop ran, and reload.
- **Out:** the three laying lines and their buttons; gatecon's enrollment and the data plane's test; the wire grammar; evaluate; the chat console, sign-in, the replacement of an appliance and the host's own install.
