# Brief: the enrollment in the Spec

**Date:** 2026-11-17. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the operator's rulings of 2026-11-16 (setup pages) and their addendum,
the operator's ruling of 2026-11-14 that a GPU power cap is a site condition, the
operator's word of 2026-11-17 on the trace, the operator's answers of 2026-11-16 on
`toddwbucy/WeaverAgent#579`, and `toddwbucy/WeaverTools#37`, the pull request stating
the survey's format, open as of 2026-11-17.
The act branches from `main` at `d67f43c7`, after the setup pages' rulings (#120) and
the Act 3 follow-ups (#140) merged.

## Goal

State an appliance's enrollment in `weaver-web-Spec`, so the image's setup script builds
against merged text: the pairing a box makes with the console, the commands the box
runs, and the two setup asks the box makes over the paired link. Docs only. The code
that builds it is a second stack, branched from `main` once this one merges and naming
its merge.

## What the Spec states

1. **The link port tells a pairing apart before the handshake.** A ClientHello offering
   ALPN `weaver-pair/1` meets a pairing configuration: TLS 1.3 alone, no client
   certificate asked, and the server's certificate with the authority's in its chain.
   Every other ClientHello meets the mutual-TLS configuration as built.
2. **The fingerprint is the authority's**, the certificate every connector pins. The box
   reads it from the handshake and prints it, and the pairing page shows the console's
   own in the same grouping.
3. **The code comes from the connection itself.** Both ends compute it from that one
   connection's TLS exporter under a label of this repository's: 50 bits, shown as ten
   Crockford base32 characters in two groups of five. Nothing of the code crosses the
   link, so it is no bearer. The comparison of the fingerprint is a step the operator
   confirms and never skips, and no text claims the code alone stops a party between the
   box and the console, which can grind its own handshake values offline.
4. **The waiting box is held in memory alone**: its claimed name, its peer address and
   its two certificate requests, with no row, audit record or log line before a code
   matches. At most 16 wait, 4 from one peer address, the first frame within 10 seconds
   and 16 KiB, a heartbeat while it waits, and ten minutes from the dial.
5. **Entering the code is an admin write** in section 2.13's order. Five codes that
   match no waiting connection close the setup's code field for ten minutes from the
   fifth, each a refusal record.
6. **The exchange**, under the authority lock, in six steps. The box and name are
   refused where a live row holds them (`box_and_name_live`, the retire offered), by a
   register write of the exchange's own. Each request's own signature is verified,
   ECDSA P-256 with SHA-256 alone, and each public half is signed as its plane. The
   answer is sent: the agent's name, the row's identity, the box, `server_name`, the
   authority's certificate and the two certificates. The box's word that it wrote the
   answer is waited for, bounded. Then the register row with both credentials live,
   the setup row's link to it and the audit (action `pair`, the admin by session, the
   target the agent row) land in one identity transaction. A lost word writes nothing,
   and the box pairs again.
7. **The commands on the box**, each writing JSON lines with an `event` member and one
   set of exit statuses (0 done, 1 fault, 2 usage, 3 unreachable, 4 refused, 5 expired,
   6 unknown):
   - `gate-con request` and `admin-con request`, each under its connector's own
     account: a new config holding the new key alone, and a certificate request
     printed. No dial.
   - `admin-con pair`, unprivileged: the address, the port, the two requests, the
     answer's file and the box's name, which an appliance always passes. It prints
     `waiting` with the code and the fingerprint, then `paired`, `refused`, `expired`,
     `unreachable` or `unknown`.
   - `gate-con adopt` and `admin-con adopt`, each under its own account: the
     certificate checked against its own key and the shown authority, then the
     complete config written in place.
   - Each config moves from the run's directory to the agent's by a rename on one
     filesystem and never a copy, every command names its config, the key never
     leaves its config, and `adopt` refuses a config whose owner or mode is wrong.
8. **The setup connection**, `admin-con setup`, a one-shot mode on admin's own
   credential, refused while admin-con's service holds it, opening no trace door,
   asking no `show` and counting for nothing in presence:
   - `setup describe` answers the agent, the models with their paths under the models
     mount in Hugging Face's layout, the models drive's need, the SSH public keys, and
     once Confirm stands the members the agent is laid from.
   - `setup report --stage pairing|drives` hands the survey's report across. The stage
     is the command's argument, so the console reads nothing of the image's format: it
     checks one JSON object within 64 KiB and keeps it verbatim, the latest of each
     stage. The pages read its members in an act of their own once the format merges.
9. **The register.** A row's box changes only through one audited write after the
   fingerprint and the code, rotate's shape with the box changed, for a new image of an
   agent that stands. The row keeps every box it held as history. While its setup runs
   the row reads "paired, setup running", so a box in setup never shows as not present.
10. **The units** stand as staged, since `adopt` writes the config each unit reads, and
    the connectors' graph holds neither `openssl-sys` nor `x509-parser`.

## What changes from the plan it was drawn from

- **No power cap.** A GPU power cap is a site condition and never the product's (the
  operator, 2026-11-14), so no text names a cap step, question, unit or drop-in.
- **The trace stays off the state drive** (the operator, 2026-11-17): a new image
  carries the agent's folder and save points across, and no text puts the trace there.
- **The cards by model**, each with its ordinal in PCI bus order, wherever the Spec and
  the design system say "the cards by identity".

## Section 10, and the charter's section 9

- **Closed by this act's elections:** the one-time secret, the certificate the
  fingerprint names, how the link port tells an enrollment apart, what the exchange
  answers, the public half's form, the pending enrollment, the connectors' pairing and
  their accounts on this repository's side, the exchange's principal, an enrollment
  over a live row, and how the agent's name reaches the box. The charter's cell on the
  one-time secret closes with them.
- **Narrowed:** what crosses the link for the setup (the box's pull stays open), and the
  box's members of the tuple (the report's form waits on the survey's format).
- **Kept open:** the link port's default, rotation of an enrolled box and of the
  authority with enrolled boxes, registration beside the pairing for a host, and the
  model store's transfer.
- **The trace kept outside the agent's reach**, in both texts: closed by the operator's
  words of 2026-11-17, quoted verbatim, naming the trace field's act as where the
  destination passages are rewritten. Nothing else of the trace is restated.
- **What follows the laying.** The four passages that state the console's Start and
  Stop and that nothing autostarts the agent (the charter's sections 2 and 10.5, Spec
  8.3 twice) stay unchanged. One new cell cites the operator's answers of 2026-11-16 on
  `toddwbucy/WeaverAgent#579` by date and names them, settled by WeaverAgent's act on
  the operator contract, then this repository's act on admin-con's verb map. The act's
  own text says only that what follows the laying (whether the agent starts at once, at
  boot or at the console's Start) is that cell's.

## Out of scope

Code of any kind. The box's pull of the model. Rotation of an enrolled box. The survey's
members on the pages. The trace field. The console's Start and Stop through the agent's
unit. A host's own setup script.

## Acceptance

- Every passage the act moves is consistent with every other, and no section 10 cell
  names "the enrollment's act" for a point this act settles.
- No box path, host name, account, address, mount path or credential in any line. Each
  command runs under an account the image names, and `describe`'s paths are relative to
  the models mount.
- The gates of `CLAUDE.md`, run since `docs/design/system/stages.md` is compiled into
  the rail's test, each reported with its exit status, the tests' count and nothing
  skipped.
