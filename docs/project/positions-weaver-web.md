# The positions of weaver-web

These are the standing positions of this repository, binding until the operator changes
one.

## 1. A consumer outside the agent's boundary

weaver-web links no crate of the agent, and the two connectors on the agent's box, gate-con
and admin-con, are the only party that reaches the agent; the server reaches nothing past
them.

Held by: Spec section 1; `CLAUDE.md`'s hard boundaries; the `cargo tree` checks of
`tests/connectors_link.rs`.

## 2. The browser is a display engine

The browser receives a rendered projection and submits what a person authored, and holds no
protocol state of the agent's or the link's and no routing or ordering logic. A passkey is
held by the person's authenticator, and the page's one module, `assets/surfaces/passkeys.js`,
only carries a ceremony's challenge and answer between the authenticator and the server; no
page code makes or holds a key or a signature.

Held by: Spec section 2.13 (the passkey module, `script-src 'self'`);
`src/surfaces/mod.rs:107-125`.

## 3. Verb answers are rendered as they came

admin's answer to a verb is shown whole, its JSON pretty-printed and nothing renamed or
summarized, and admin-con parses no refusal class of admin's. The one standing exception is
the server's: it reads an `unload`'s `save_point_not_taken` to raise the prompt of Spec
section 2.14, and still renders the answer whole beside it.

Held by: `src/surfaces/reading.rs:903-923`; Spec section 2.14; `CLAUDE.md`'s conduit rule
(Spec section 2.14, ruling 3).

## 4. Gaps in the record are marks, never smoothed

A discontinuity, truncation, replacement or lost contact reaches the live view as a mark,
and is never filtered out or bridged.

Held by: Spec section 7.2 (replay and marked discontinuities);
`src/surfaces/reading.rs:1198-1203` (the `mark` event); `src/link/admin_con_tests.rs`.

## 5. A missing contract is an ask upward, never a workaround

When a door contract lacks something, the need is filed as an issue on the contract's
repository, one per interface question, and is never parsed around, linked around or
reached around.

Held by: `CLAUDE.md`'s "Do not edit WeaverAgent" boundary.
