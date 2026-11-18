# Brief: the survey's reading

**Date:** 2026-11-17. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the survey's format `weaver-survey/1`, as merged to the suite
repository's `main` by `toddwbucy/WeaverTools#37` at `75ae7c54`
(`docs/profiles/weaver-survey-format.md`). `weaver-web-Spec` 6.2, 8.3, 9 and 10 as on
`main`. The operator's rulings of 2026-11-17 on the act's plan, and the Planner's of the
same day. The act branches from `main` at `7c768cae`.

## Goal

The box's survey reports land verbatim on the setup, the latest of each stage, and
nothing reads them. Read `weaver-survey/1` and feed what the setup pages hold on it:
the pairing page's survey report, the drives page's drives as mounted, the placement
page's cards by ordinal, and Confirm's box members. Each pane carries its status,
healthy states included. `box_report::Card`'s `identity` becomes `model`, as 6.2 and
the format name the card. The Spec states six things the merged text leaves open, so it
moves first, in the stack's first sub-act.

## The sub-acts, in order

Each is a draft against the stack. Sub-acts 2 to 4 branch from the stack's head once
the one before them merges in. The reader in 2 is written beside 1, since it needs only
the format and the ruling on the mount points.

1. **`act/the-survey-reading-1-spec`**, docs only. The rulings below written into 6.2
   (the Pairing, Drives, Placement and Confirm rows, the box report passage, and the
   tuple's box members), 8.3 (the reader), 10 (the box's members cell and the state
   drive's cell close) and 9 (the rows below, each owed). The text cites the rulings by
   their date.
2. **`act/the-survey-reading-2-the-reader`.** `src/survey.rs`, with no new dependency.
   - The report is read only as `weaver-survey/1`, every member of the format's section
     1 typed, a drive with `mounted` false carrying its three members alone.
   - Every refusal of the format's section 3 refuses the whole report, and no member of
     it is read. Refusal 10 applies as written.
   - A member name given twice at any depth refuses the report, seen by a visitor over
     the deserializer that keeps each object's names, since a parsed JSON value keeps
     the last name silently.
   - Integers are judged by their written form: a fraction, an exponent, a sign or a
     value past 2^53 - 1 is refused.
   - Unknown members are kept and ignored, at any depth.
   - Unit tests: one per refusal, and the format's worked example read whole, carried
     as written.
3. **`act/the-survey-reading-3-the-source`.**
   - The box seam answers from the setup's kept reports, read through the reader, in
     place of `NoBox`, and keeps its seam for the setup pages' tests. A store error is
     answered apart from no report.
   - `Card { ordinal, model }`, and the box's members in the shape ruled. The tuple's
     JSONB takes it with no migration, and the applied migration `0029` keeps its
     comment.
   - Placement and Confirm read the cards and members as ruled, Confirm refusing a kept
     device the drives report lacks.
   - An end to end test: `setup report` at each stage through the real listener, the
     placement page offering the drives report's cards, and Confirm standing and
     writing the tuple.
4. **`act/the-survey-reading-4-the-panes`.** The pairing page's report pane and the
   drives page's drives as mounted, with the drives page's judgments, each pane with its
   status, and a refused report shown as ruled. Page tests for each status: read,
   refused and waiting.

## Rulings

1. **The tuple's box members** (the operator, 2026-11-17), typed.
   - The image: its version and manifest digest.
   - The driver: its version, and the CUDA runtime's.
   - The cards: each card's ordinal, model, PCI address and memory.
   - The guest CPU: vendor, model name, family, model, stepping, logical CPUs and flags.
   - The machine: virtualization, vendor, product and version, the memory total and the
     kernel release.
   - The drives: each drive's role, label, filesystem, size and project quota.

   Not a drive's free space, and not whether the driver is loaded. The tuple names the
   report it read: its stage and `taken`.
2. **Which report feeds what** (the Planner, 2026-11-17). The pairing page shows the
   pairing stage's report, and the drives page the drives stage's. The cards the
   placement page offers come from the drives stage's report where it stands and reads,
   else from the pairing stage's. Every box member Confirm reads comes from the drives
   stage's report alone. A kept device that a later report lacks is refused at Confirm,
   naming the ordinal, and the placement page says so.
3. **The drives page** (the operator, 2026-11-17). A state drive under 10 GiB holds
   Confirm. A drive not mounted at the drives stage holds Confirm. A models drive whose
   free space is under the picks' need is a warning, not a refusal.
4. **The mount points** (the operator, 2026-11-17). The appliance profile's two fixed
   mount points are the format's constants and may stand in the reader and its
   fixtures. The reader applies refusal 10 as written. `CLAUDE.md` states the clause at
   `7c768cae`.
5. **The pairing page's report** (the Planner, 2026-11-17). The report's arrival and
   `taken`, then its members grouped: the image, the kernel, the machine, the CPU (its
   flags as a count, the list folded), the memory, the driver and whether it is loaded,
   the CUDA runtime, and the cards. A box with no card says so as a reading, not a
   fault.
6. **A refused report** (the Planner, 2026-11-17). The pane names the stage, the arrival
   and the refusal's cause in the format's words, with the status "refused". Every page
   and member the report would feed stays held, naming the refusal. The verbatim text is
   not drawn.

## Readings of the merged text

- "the cards by model" (6.2, section 10) is the format's `cards[].name`, "the card's
  model, which names the card in the deployment tuple". The seam's `identity` drifted
  from it.
- "The pages read its members in an act of their own once it merges" (8.3) and "Its
  source over the reports the setup keeps ... is the survey's reading act's" (6.2) are
  this act.
- A pairing stage report with both drives not mounted is a valid report, read whole. It
  feeds no drive of Confirm's.

## Section 9's rows, by sub-act

Sub-act 1 writes each as owed, and the sub-act named turns it to its instrument.

| sub-act | rows |
| --- | --- |
| 2, the reader | a report is read only where it passes every refusal of the format's section 3, and a refused one is read as none; an unknown member is kept and ignored at any depth; a member name given twice refuses the report; an integer is judged by its written form |
| 3, the source | the devices offered are the ruled report's cards by ordinal; Confirm's box members are the drives report's, in the ruled shape, and Confirm holds while it is absent or refused; a kept device a later report lacks is refused at Confirm |
| 4, the panes | the drives page's judgments as ruled; each survey-fed pane carries its status, healthy included; a refused report is shown by its cause and its verbatim text is not drawn |

## Out of scope

The card's architecture, which the report does not carry. The placement's fit, the room
plan's answer. The image's setup script that runs the survey. The box's pull of the
model. Rotation. #150 and #151.

## Acceptance

- Each sub-act's gates are those of `CLAUDE.md`, run at its head with `--locked`: fmt,
  build, clippy with and without the default features, and the suite at eight threads
  with `DATABASE_URL` and `PGUSER` set, reported with the count passed and that nothing
  was skipped, and a perturbation shown to fail for each row the sub-act turns.
- No credential, box path, account, mode or address of a box enters any line. The
  format's constants are admitted by the ruling of 2026-11-17, and a real machine's
  readings stay out of every fixture.
- The stack's body names `75ae7c54` as the format it reads, with a verdict per Spec
  section.
