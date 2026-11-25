# Brief: the reader of `weaver-survey/2`

**Date:** 2026-11-25. **From:** the Planner, on the Executor's plan of 21:45 UTC and the Planner's decisions of 21:51 UTC. **Stack:** `act/the-survey-reader`, cut from `act/the-first-tab` at `eb95d1c362f45e027aa486b1215bc50810a12846` once its four children were merged into it (the Planner, 22:45 UTC).

**Builds against:** the suite repository's `docs/profiles/weaver-survey-format.md`.
- It stands on the suite repository's draft pull request 57, at commit `1415dea092d53f5bee2cb85bbd704fbb63e4f057`, sha256 `a4d45bd3e4c7080c00e6721fe0899ab11d0c65f09ffa9720d202c8912f3abefe`. The reader's header names that commit and that digest.
- Its section 7 reads "then Weaver Web's reader of version 2", and "That reader refuses a report of version 1 by its version's name." The copy of 21:15 UTC differed from it there alone.
- Its worked example of version 2 has sha256 `fb6801694af2ebccedf3a2a52915f2610205fa20ac7defc16ada88a58ab59d95`.
- The stack builds against the document as carried, and merges only after pull request 57 merges (the suite workflow, section 7). Where review changes a sentence, the stack follows it, and a later commit gives the header the merged document's commit and digest.

## The goal

Weaver Web reads the survey's report as `weaver-survey/2` and nothing else, so that the survey's own act can land after it (the format's section 7).
- **One report.** The survey runs once, at the end of the setup script, under the stage `pairing`, and the `drives` stage goes.
- **The two places.** The report carries the two mount places as the operator chose them, and every page that names a place reads it from the report.
- **The network.** The report carries the two planes' addresses, their prefix lengths and `planes`, and the Pair page shows them.
- **The pairing** takes version 2 alone, and the box hears `format_refused` otherwise.

## The operator's words that bind it (2026-11-25)

- "[The survey's file] holds the survey report. [...] It is a fact about the VM, not about an agent. It carries the two mount paths and the plane of each NIC. The bytes sent to Weaver Web on the pairing call are that file, so one digest names both."
- "The two mount paths are hard coded nowhere. One value, set once by the script, read by everything else."
- "72 (i). Yes. The survey reports a blank appliance and says what it lacks."
- Standing: "The survey's report is the receipt." "After a pairing succeeds, the Pair page shows the hardware report, read only. The next page carries it into the declaration."

The file's location is the appliance's fact, elided from the operator's words above. No code, test or document of this stack names it.

## Already ruled out

- **Reading version 1 into the pages.** A stored version 1 report is refused under cause 4, its detail naming `weaver-survey/1`, and it feeds nothing.
- **Comparing the pairing's peer address with the reported management address.** Not built (the Planner, 2026-11-25).
- **Touching migration `0031`.** Its CHECK keeps `drives`, since it is frozen, and no row moves. A stored `drives` row is read as a stage this format no longer has.
- **A network member in the deployment tuple.** None was ruled into it.
- **Anything of a report on New agent.** Spec 8.2 holds that nothing of a report is drawn before the code matches, and that stands.

## The children, in order

Each carries its own Spec sections and section 9 rows, passes the repository's gates alone, and measures a perturbation for each claim it adds.

1. **The reader of version 2.** Files: `src/survey.rs`, `src/survey_tests.rs`, and `tests/fixtures/weaver-survey-2-worked-example.json`, byte for byte.
   - `FORMAT` is `weaver-survey/2`, and `stage` is `pairing` alone.
   - `STATE_MOUNT` and `MODELS_MOUNT` are removed. The places are judged by section 1.1's one spelling, the two different and neither inside the other, part by part.
   - `network` is read: each address respelled and compared byte for byte, the five refused kinds, the prefix lengths by family, `planes`, and `separate` checked.
   - A leap second is refused. Today chrono's `%S` takes `60`.
   - `pci_address` must ascend by number: domain, bus, device, function. Today it is compared by bytes.
   - A version 1 report is refused under cause 4, naming the version read and the version taken.
   - The module's header names the document by its sha256 and the suite's head.
   - The version 1 fixture stays, for the test that names it.
2. **One stage.** Files: `src/link/frames.rs`, `src/link/box_commands.rs`, `src/box_report.rs`, `src/surfaces/setup/placement.rs`, and the templates that say "either stage".
   - `Stage` holds `pairing` alone. A first frame naming `drives` does not parse and is refused `frame_refused`.
   - `admin-con pair --drives-report` is removed, and `admin-con setup report --stage` takes `pairing` alone. clap answers either old form as `usage` (2), before any dial.
   - `Reports` holds the one report, and `latest()` goes.
   - **A report run again replaces the one row,** by the setup's latest-per-stage rule. That is the path for an appliance whose drives were laid after its first report.
3. **The pairing reads the version.** Files: `src/link/pairing.rs` and `src/link/frames.rs`.
   - At the first frame, before the box is listed, the console reads the `pairing` report's `format` member alone. Any version but `weaver-survey/2` is refused **`format_refused`**, a new word in Spec 8.2's list, which the box prints.
   - The refusal's detail names the version read and the version taken. It is the console's log line, since the pair protocol's refusal frame carries the reason word alone, as every reason does. Giving the box the detail would need a new member in that frame, which the script seat reads.
   - Nothing else of the report is judged at the frame. A version 2 report refused for another cause still pairs, and the Pair page shows its cause, as today.
4. **The pages.** Files: `src/surfaces/setup/report.rs`, `setup_pairing.html`, `setup_directories.html`, and `src/surfaces/setup/models.rs` where it names a place.
   - **The Pair page's report pane** gains a Network group: the management plane's address and prefix length, the data plane's, and "separate" or "shared". It is read only and judges nothing.
   - **The directories page** names each place as the report gives it. The default is named nowhere, and "at their default paths" leaves lines 14 and 17.
   - **A blank appliance** pairs. The Pair page says what it lacks, one sentence per case, the places as the report names them:
     - the state drive alone: "The appliance reports no drive mounted at `<state place>`, where its state drive goes."
     - the models drive alone: "The appliance reports no drive mounted at `<models place>`, where its models drive goes."
     - both: "The appliance reports no drive mounted at `<state place>` and none at `<models place>`, where its state and models drives go."
     - These are working text. The operator may reword them.
   - The directories page then shows each drive not mounted, status fail, and Confirm holds until a later report shows both mounted.

## The document's section 3, check by check

A reader refuses the whole report and reads no member of it. Each row is one test, named for its refusal, with a case for each clause. "Today" means the reader at `91fb7ae8`.

| # | Refusal | The reader's check | Its test |
| --- | --- | --- | --- |
| 1 | not UTF-8, not one JSON object, or anything after the object but one `\n` | `parse`: `from_utf8`, one value by `serde_json`'s stream, the rest empty or `"\n"`, and the top a JSON object. Whitespace before the object is free. Today: kept | `refusal_1_...`, with cases: a byte-order mark; two objects; `\n\n` after; a space after; a top that is an array; whitespace before it read |
| 2 | more than 64 KiB | the length before parse, the newline counted. Today: kept | `refusal_2_more_than_64_kib`, the bounds below |
| 3 | a member name twice in one object, at any depth | the tree's own read of every object. Today: kept | `refusal_3_...`, at the top, in `network.management` and in `cards[0]` |
| 4 | `format` missing, or not a version the reader knows | `format == "weaver-survey/2"`. The detail names `weaver-survey/1` where that is what it read. New: version 1 is refused here | `refusal_4_...`, cases: missing, `weaver-survey/1` (named), `weaver-survey/3`, `weaver-survey/2 ` |
| 5 | a member missing, `null` or of the wrong type; a drive not mounted carries three members alone, and the rest are not missing | every member of section 1, `network` and its five included. Today: kept, with `network` new | `refusal_5_...`, one case per new member: `network`, each plane, each address, each prefix length, `planes` |
| 6 | an integer negative, signed (`-0`), fractional, with an exponent, or above 2^53 - 1 | by its written form. Today: kept, `-0` among it | `refusal_6_...`, gains `-0` in a prefix length |
| 7 | a string outside printable ASCII, over its bound, or off its pattern | each string by its bound and pattern. Today: kept | `refusal_7_...`, the bounds and empty strings below |
| 8 | `stage` other than `pairing`, or `taken` out of form, a leap second included | `stage == "pairing"`; `taken` as today, plus seconds `00` to `59` before chrono. **New: `60` refused** | `refusal_8_...`, cases: `drives`, `Pairing`, `2026-12-31T23:59:60Z`, `2026-02-30T00:00:00Z`, a lowercase `t` |
| 9 | an `ordinal` not its index, or `pci_address` not strictly ascending by number | **New:** domain, bus, device and function parsed as numbers and compared in order | `refusal_9_...`, gains `ffff:...` before `10000:...` refused (byte order would accept it) and the reverse accepted, and two equal |
| 10 | `drives` not exactly `state` then `models`; a `mount_point` not in the one spelling; two equal, or one inside the other; a drive not mounted carrying a mounted drive's members | **New:** the place grammar of section 1.1, equality, and insideness part by part. The constants go | `refusal_10_...`, cases: `/`, `//a`, `/a/`, `/a/./b`, `/a/../b`, a part of 256 bytes, the whole of 256 bytes, a space, `/a` and `/a`, `/a` and `/a/b` refused; `/a` and `/ab` accepted |
| 11 | an integer outside its bound, more than 16 cards, `cpu.flags` empty, over 512 entries, out of byte order or repeated | each integer's bound. Today: kept | `refusal_11_...`, the bounds below |
| 12 | **New.** A plane's address off the one spelling or of a refused kind; a prefix length off its family's bound; `planes` neither value; `separate` with equal addresses or one inside the other's network | each address parsed by `std::net` and respelled: IPv4 by its `Display`, IPv6 by RFC 5952. A respelling that differs is refused; a `%` zone fails the parse. The five kinds; prefix 1 to 32 and 1 to 128; `planes`; for `separate`, equal or either in the other's network, mixed families never in each other's | `refusal_12_...`, cases: `192.0.2.010`, `2001:DB8::1`, `2001:db8:0:0:0:0:0:1`, `2001:db8::0:1` with a single-zero `::`, `::ffff:192.0.2.1`, `fe80::1%eth0`, `0.0.0.0`, `::`, `169.254.1.1`, `fe80::1`, `224.0.0.1`, `ff02::1`, `255.255.255.255`, prefix 0 and 33 on IPv4 and 129 on IPv6, `planes` `both`, `separate` with equal addresses, `separate` with `192.0.2.10/24` and `192.0.2.20/24`, `separate` with IPv4 and IPv6 accepted, `shared` with one address accepted, and a loopback address accepted |
| (after 3) | a member the reader does not know is kept and ignored, at any depth | today: kept | `a_member_the_reader_does_not_know_...`, gains one inside `network.data` |

**RFC 5952's own examples pin the IPv6 respelling** in one test: `2001:db8::1`, `2001:db8:0:1:1:1:1:1` (a single zero group not shortened), `2001:0:0:1::1` (the longest run), and `2001:db8::1:0:0:1` (the first of two equal runs). Each reads, and each other spelling of the same address is refused.

## The bounds, one case at each and one past it

| What | At the bound, read | Past it, refused under |
| --- | --- | --- |
| the report, the newline counted | 65,536 bytes, ending in `\n` | 65,537 bytes (2). Also 65,536 bytes plus a `\n`, so the newline is counted |
| `kernel.release` | 64 bytes | 65 (7) |
| `machine.virtualization` | 32 | 33 (7) |
| `machine.vendor`, `machine.product`, `machine.version` | 128 each | 129 each (7) |
| `cpu.vendor` | 32 | 33 (7) |
| `cpu.model_name` | 128 | 129 (7) |
| `cpu.family`, `cpu.model`, `cpu.stepping` | 65535 each, and 0 | 65536 (11) |
| `cpu.logical_cpus` | 1 and 4096 | 0 and 4097 (11) |
| `cpu.flags` | 1 entry, and 512 entries | 0 entries and 513 entries (11) |
| a `cpu.flags` entry | 32 bytes | 33 bytes (7) |
| `cards` | 0 cards, and 16 | 17 (11) |
| `cards[].name` | 96 bytes | 97 (7) |
| `cards[].pci_address` domain | 4 and 8 hex digits | 3 and 9 (7) |
| `drives` | exactly 2 | 1 and 3 (10) |
| `drives[].label`, `drives[].filesystem` | 16 bytes each | 17 each (7) |
| a `mount_point` part | 255 bytes, the whole within 255 | a part of 256 (10) |
| a `mount_point` whole | 255 bytes | 256 bytes (10) |
| an integer member | 2^53 - 1 | 2^53 (6) |
| a plane's address | the longest IPv6 spelling the form allows, 39 bytes | none longer exists in the form; a 40-byte string is refused as off its spelling (12) |
| `prefix_length` IPv4 | 1 and 32 | 0 and 33 (12) |
| `prefix_length` IPv6 | 1 and 128 | 0 and 129 (12) |

`image.version`, `driver.version` and `cuda_runtime.version` have no byte bound of their own (section 1). A case reads each at a length past 64 bytes within the report's bound, so no bound is invented for them.

## Which strings may be empty

Section 1: a string whose bound names no pattern may be empty, and one with a pattern matches it whole.

- **Read empty, one case each:** `kernel.release`, `machine.virtualization`, `machine.vendor`, `machine.product`, `machine.version`, `cpu.vendor`, `cpu.model_name`, `cards[].name`, `drives[].label`, `drives[].filesystem`.
- **Refused empty, one case each, under 7 or under the refusal its pattern belongs to:**
  - `format` (4), `stage` and `taken` (8);
  - `image.version`, `image.manifest_sha256`, `driver.version`, `cuda_runtime.version` and a `cpu.flags` entry (7);
  - `cards[].pci_address` (7);
  - `drives[].role` and `drives[].mount_point` (10);
  - each plane's address and `network.planes` (12).

## The acceptance

- **The worked example** reads whole from `tests/fixtures/weaver-survey-2-worked-example.json`. The test first checks the file's sha256 against `fb6801694af2ebccedf3a2a52915f2610205fa20ac7defc16ada88a58ab59d95`, then pins each member, the example's models place `/srv/weaver-models` among them, a place other than the version 1 constant.
- **The image proof's shape reads:** both drives not mounted and one address as both planes under `shared`. The pairing succeeds, and the Pair page names what is lacking.
- **Every row of the two tables above has its test.** Every check the act adds has a measured perturbation.
- **The places stand in the code nowhere:** a search of `src/` and the templates for the two default places finds them in no file a build ships, only in tests and the two fixtures. The version 2 example's state place is one of them, and the tests use the image proof's places for a report whose drives are not mounted.
- **The gates:** the repository's gates, alone, with `DATABASE_URL` and `PGUSER` set, the count reported and nothing skipped.

## The scope boundary

- **In:** the reader, the stage, the pairing's version check, and the Pair and directories pages.
- **Out:** the deployment tuple's members; New agent; the survey program and the setup script, which are the suite's and the WeaverAgent seat's; the format document, which is the suite seat's; and migration `0031`.
- **No box path, host name, address of ours, account or credential** enters the brief, the code or a pull request body. The addresses in tests are the documentation ranges (RFC 5737, RFC 3849) and the format's own example.
