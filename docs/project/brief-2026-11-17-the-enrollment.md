# Brief: the enrollment's code

**Date:** 2026-11-17. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** `weaver-web-Spec` 8.2 and 8.3, with 2.12, 2.13, 6.2 and section 9's
rows, as merged to `main` by #144 at `0972d156` on 2026-11-17. The act branches from
`main` at `123331af`.

## Goal

Build the enrollment the merged Spec states: the pairing a box makes with the console
over the link port, the exchange that writes the register row or replaces its box, the
commands the box runs, and the setup connection's two asks. Every row the merge added to
section 9 as owed is turned into its instrument by the sub-act that builds its claim.

## The sub-acts, in order

Each branches from the stack's head once the one before it merges in, and each is a
draft against the stack.

1. **`act/the-enrollment-2-the-link-port`.** Both ends of the pairing connection, up to
   the code.
   - The listener reads each ClientHello before it chooses a configuration. One that
     offers ALPN `weaver-pair/1` meets a pairing configuration: TLS 1.3 alone, no client
     certificate asked, the chain the server's certificate then the authority's. Every
     other meets the mutual-TLS configuration as built.
   - The waiting connections are held in memory alone, within the merged bounds: 16 in
     all, 4 from one peer address, the first frame within 10 s and 16 KiB, a heartbeat
     while waiting, and ten minutes from the dial. Nothing reaches the store, the audit
     or the log per connection.
   - The code at both ends: the TLS exporter under `EXPERIMENTAL-weaver-pair-code`, 32
     bytes, an empty context, its first 50 bits in Crockford's base32, and the page's
     reading of an entry.
   - The box's half as a library: the dial, a verifier that checks the server's
     certificate chains to the authority certificate presented with it and checks no
     name, the first frame, and the code. Later sub-acts' tests and `pair` use it.
   - The pairing frames, apart from the plane frames, none carrying the code.
2. **`act/the-enrollment-3-the-exchange`.**
   - A default feature, `enrollment`, carrying rcgen's request parser, which the
     server's binary requires and the connectors' build leaves out.
   - Each request verified, ECDSA P-256 with SHA-256 alone, and each half signed as its
     plane with the server's own parameters.
   - A migration for the setup's link, its pairing's write, its misses and the box
     history.
   - The pairing page's entry in section 2.13's order, with the five misses kept on the
     setup row.
   - The six steps for `pair` and `replace box`, the box's word bounded by
     `HANDSHAKE_SECS`.
   - The replacement asked on the pairing page for a setup whose name a live row holds.
   - The "paired, setup running" label.
   - `unknown` shown by what the write made.
3. **`act/the-enrollment-4-the-box-commands`.**
   - `request`, `pair` and `adopt` on the connectors' binaries, beside the service,
     which runs as built.
   - The answer file created new and readable by the connectors' accounts, since it
     holds certificates alone.
   - The setup connection, `FromClient::Setup` as its own first frame, admitted by the
     hello's rules and never through presence.
   - `setup describe` and `setup report`, and a migration for the reports, kept verbatim
     as text.

## Decisions, as ruled

1. **`enrollment` is a default feature of its own**, which the server's binary
   requires. The connectors build without it, and row 16 measures that.
2. **The box's half is a library in sub-act 2.**
3. **The misses are kept on the setup row**, so a restart does not reset them.
4. **The box's word is bounded by `HANDSHAKE_SECS`**, 10 s.
5. **Every file a command rewrites goes by a temporary file and a rename**: created new
   in the same directory without following a link, synced, renamed over the old, and the
   directory synced, so a crash leaves the old file or the new one whole. The merged
   Spec keeps the key inside the config `request` creates (section 8.2, "the key never
   leaves its config"), so the one file stands, and `adopt` completes it at its own path
   by this rename. The rename needs the connector's account to create a file in the
   directory that holds its config, which sub-act 4 names as an ask of the image.
6. **The setup ask is its own first frame**, `FromClient::Setup`, never through
   admission's presence.
7. **The answer file is readable by others**, since it holds certificates alone.
8. **The replacement is asked on the pairing page.**

## Readings of the merged text

Each is what the text says. None moves the Spec.

- "`setup_closed` and `box_and_name_live` checked by a register write of its own" is a
  transaction that locks the setup row and reads the live row for the box and name,
  apart from registration's insert, which retires that row.
- "the authority the answer carries, whose fingerprint is the one `pair` printed" holds
  because `pair` refuses an answer whose authority is not the one the handshake
  presented, so `adopt` verifies under the answer's.
- "verbatim" is text, never a parsed column that would rewrite the report.
- The setup report is a connector's report landing, as an observation does, and is not
  among section 2.13's audited writes.
- `adopt` writing "in place" is the config completed at its own path, decision 5.

## Section 9's owed rows, by sub-act

| sub-act | rows, in the merged section 9's order |
| --- | --- |
| 2, the link port | 1 (no pairing protocol meets mutual TLS as built), 2 (nothing written before the code matches), 3 (the code in no frame), 4 (the code's derivation), 6 (a party between meets no match) |
| 3, the exchange | 5 (the entry's reading), 7 (signed only over a verified P-256 request), 8 (a live box and name refused, nothing retired), 9 (a lost word writes no row), 10 (`unknown` shows what the write made), 11 (a box changes only by `replace box`, history kept), 16 (the connectors' graph holds no request parser) |
| 4, the box commands | 12 (`describe`'s `replacement`), 13 (a setup ask refused while the service holds the credential, no presence), 14 (the latest report of each stage, verbatim), 15 (an adopted pair connects) |

Each row moves from **owed** to its instrument in the sub-act's own commit, a
perturbation where the row names one.

## Out of scope

The image's script and its drive setup. The box's pull of the model. Rotation of an
enrolled box or of the authority with enrolled boxes. The survey's members on the pages,
which follow this stack as the survey reading act, the survey's format having
merged. What follows the laying. The link port's default. A host's own setup script.

## Acceptance

- Each sub-act's gates are those of `CLAUDE.md`, run at its head with `--locked`: fmt,
  build, clippy with and without the default features, and the suite at eight threads
  with `DATABASE_URL` and `PGUSER` set, reported with the count passed and that nothing
  was skipped, and a perturbation shown to fail for each row the sub-act turns.
- No credential, box path, account, mode or address of a box enters any line.
- The stack's body names `0972d156` as the merged text it implements, with a verdict per
  Spec section.
