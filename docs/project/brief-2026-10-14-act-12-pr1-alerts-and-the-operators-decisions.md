# Brief: act 12, PR 1: alerts, and the operator's decisions

Version: v0.1, 2026-10-14. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. Act 11's PR 6 closes with 6c (#44). This act carries the operator's rulings of
2026-10-14, made while reading WeaverWeb #43. **Two PRs, one concern each**:

- **PR 1 (this one): the server's alerts and the operator's decisions.** The rule, the
  alerts and their acknowledgement, the operations log, the save-point prompt, lost
  contact, and admin-con's own unload removed.
- PR 2: the connectors send their log entries upstream (admin's `admin.log` entries and
  their own failures), which the server turns into alerts through PR 1's mechanism.

Backend first still holds: the alerts and the prompt are plain, server-rendered and
script-free.

## 1. The operator's rulings, 2026-10-14

1. **Any failure that could lead to the loss of data becomes a prompt to the operator**:
   the problem, the current status, and the immediate options. The operator chooses.
   **Nothing retries in the background, and nothing picks a recovery on its own.** What
   to do after a possible loss depends on context only the operator has.
2. **An unload stopped at its save point** (`unload` refusing `SavePointNotTaken` naming
   a leg other than `published`, the contract's S9: the run standing at rest, the gate
   lowered, nothing lost yet) gets a prompt with **two options**:
   - **save point, then unload**: `save-point`, and `unload` **only once the save point
     answers its hash-confirmed write**: exit 0, `save_point_taken`, with its report's
     digest. Since WeaverAgent #158 (merge `55fbc66`), `SavePointTaken` means the
     published copy was read back and digests to its manifest line
     (`weaver-admin-operator-contract` section 5). **The prompt never promises more than
     that**: the read-back is served from the page cache, so durability on the device
     rests on admin's syncs before it (`weaver-admin-Spec` section 6). Any other answer
     stops the sequence, sends no `unload`, and becomes its own alert;
   - **force-unload**, which the prompt says loses the state since the last published
     save point.
3. **A connector is a conduit.** It creates the pipe and passes traffic along; it never
   acts on the agent. **admin-con's orderly-stop `unload` and its `ActivityNotAtRest`
   retry go**: a connector that exits leaves the agent exactly as it stands. To the
   server, a connector's exit and a broken route or firewall rule look the same: **lost
   contact, raised to the operator, the agent possibly still running**.
4. **Why a thing failed reaches the operator**: the server writes each failure's reason
   to an **operations log under `/var/log`** and shows it on the agent's page.
5. **Alerts**: everyone holding a grant on the agent (any role) sees its alerts; **an
   alert stays until acknowledged**, by a person holding the `operator` role on that
   agent, the acknowledgement audited.

## 2. Read first

- Spec 2.13 (audit, the target kinds), 7.2 (the verbs, `SavePointNotTaken`), 8 (the
  orderly stop paragraph, about line 3870) and section 9's orderly-stop row.
- `weaver-admin-operator-contract` section 5 at `55fbc66`: the state table's S9 row, the
  `SavePointNotTaken` legs, `SavePointTaken`'s read-back. `weaver-types` `wire.rs`:
  `SavePointReport` (`save_point` is the digest), `SavePointLeg`.
- Code: `admin_con::orderly_stop`, `refused_not_at_rest`, `REST_RETRY`; the listener's
  two doors (`Answer::audited`, `TurnAnswer::audited`) and the teardown's reasons;
  `surfaces::reading` (the page, the verb page); migration `0018`'s target CHECK.

## 3. Scope

1. **The rule in the Spec**: a subsection after 2.13 for alerts and the operator's
   decisions, stating rulings 1 to 5, with section 9 rows for what this PR enforces.
2. **Alerts in the store** (a new migration): an alert has its own identity (a prefix of
   its own, per Spec 2's convention), the agent, its kind, when it was raised and last
   occurred, how many times, its source (the server for this PR; PR 2 adds the
   connectors'), a bounded summary and detail, the first record it concerns where there
   is one (`au-`), and its acknowledgement (when, by whom). **Never a turn's text** and
   no secret.
   - **One open alert per agent and kind**: while unacknowledged, a repeat updates the
     open alert (last occurrence, count) instead of adding one, under a unique partial
     index and an insert that resolves its conflict, so a flapping link raises one alert
     and not hundreds.
   - **An acknowledgement names the version it read**, refusing on a stale one, so a
     repeat landing after the person read the page is never acknowledged unseen. An
     occurrence after an acknowledgement opens a new alert.
3. **What the server raises in this PR**:
   - **A failed or unknown outcome** of a verb or a turn, at the door where the outcome
     is written, with the reason: admin's refusal kind and message (its object's), or
     admin-con's or gate-con's fault kind and message, or the gate's close kind and
     reason, plus admin's stderr where it came (bounded). An outcome that is unknown
     (the connection ended with the act in flight) is an alert too.
   - **A refusal for a system reason**: not connected, busy, stalled, the audit
     unwritable, outside the ceiling. **Not** an authorization refusal (no grant, a grant
     without the verb), which is the audit's and not an operational failure, and not a
     shed request, which by design leaves no record.
   - **Lost contact**: a connection on either plane that ends for any reason other than
     the server's own act (a revocation, rotation, retirement, the halt, a duplicate's
     refusal). Say in the PR body which teardown reasons raise it. The alert notes when
     contact was restored, if it was, and stays until acknowledged.
4. **The decision prompt**, a kind of alert that carries options. **This PR builds one**:
   the S9 prompt of ruling 2, raised at the door when an `unload` answers
   `SavePointNotTaken` with a leg other than `published`, read from admin's object by
   allow-list.
   - It is shown on the agent page and on the verb page that met it, as a
     server-rendered `<dialog open>` (the CSP allows no inline script). It states the
     problem, the status (the run standing, its gate lowered, nothing lost yet) and the
     options, **offering only those the person's grants and the connection's ceiling
     carry**.
   - **"Save point, then unload"** is one person's choice, run as two of that person's
     asks through `ask_as_person`, each admitted, held, recorded and audited as any ask.
     The sequence runs in the admitted task, and the `unload` is asked only on the
     hash-confirmed answer above. The page shows each step's status.
   - **Choosing an option does not acknowledge the alert**: the alert stays until
     acknowledged.
   - The other refusals that may lose data (`SavePointNotTaken` naming `published`;
     `BoundaryUnverified`, cleared on the box) are alerts that explain the problem and
     the status, with no web option. **Put a table in the PR body** of every refusal in
     the contract's section 5 that could lose data, with what this PR does for each, for
     the operator to confirm.
   - **`show` cannot tell S9 from S2 on the wire** (`State { state: Idle }` either way),
     so an S9 entered outside WeaverWeb is not detected. That is named as residue; the
     operator asked for no new WeaverAgent work now, so no issue is filed.
5. **The operations log**: a config key naming its path, **with no default in the code**
   (the install sets it under `/var/log`; no host path enters the repository), required
   at start, refused there where it cannot be opened for appending.
   - One JSON line per failure: the time, the agent, the plane, the act (the verb or
     `turn`), the first record (`au-`), the principal, the outcome or refusal, the
     reason's fields, and the alert's identity.
   - **Never a turn's text.**
   - **Opened per write with append**, so logrotate needs no signal.
   - **Written by a task of its own behind a bounded queue**: the connection task and an
     ask never wait on the file, a full queue drops the line and counts the drops, and
     the count is logged at error.
6. **admin-con never acts on the agent**: remove the orderly stop's `unload`, its retry,
   `REST_RETRY` and `refused_not_at_rest`, so admin-con parses no refusal class at all.
   Whether it still waits for an in-flight child before exiting (the child is never
   killed) is yours to decide; say in the body. Spec 8's paragraph and section 9's
   orderly-stop row are rewritten to the conduit rule, with an instrument: a shutdown
   with the ceiling granting `unload` invokes nothing.
7. **The surfaces**:
   - the agent page lists its open and recent alerts, newest first, each with its
     reason, an acknowledge form for `operator` holders, and the prompt where one is
     open;
   - `GET /agents` shows each agent's open-alert count;
   - the nav shows the person's open alerts across their agents.
   - **Acknowledge** is `POST /agents/{ag}/alerts/{al}/ack` with the version, in the
     admin writes' order: parse; the session; the grant check (no grant 404 as unknown,
     no `operator` role 403, each one refusal record); resolve; then one transaction
     under the identity exclusion, re-checking the session and the grant and writing the
     acknowledgement with its audit record (target kind `alert`, action `acknowledge`;
     the target CHECK gains the kind).
8. **Out of this PR**: the connectors' entries (PR 2); any alert delivery outside the
   panel (mail and the like, out of act 11's scope and still out).

## 4. Review checklist (distributed state)

- **No wait added to the connection task or the held section**: raising an alert and
  writing the log line are bounded or queued; a stalled store or a full disk never holds
  a connection, a teardown or an ask past its bound.
- **Dedupe under concurrency**: two teardowns, or two failed outcomes, raising the same
  kind at once leave one open alert, its count right.
- **Acknowledgement races**: a repeat landing between the page's read and the
  acknowledgement is not acknowledged.
- **The prompt's sequence**: the `unload` is asked only on the allow-listed confirmation.
  A cancellation table for the sequence task covers every await and what a drop there
  leaves (no `unload` without the confirmation).
- **The text**: a turn's text reaches no alert, no log line and no prompt.
- **Grants**: who sees and who acknowledges are read at the request; observers see and
  cannot acknowledge.

## 5. Tests and perturbations

Against the real listener and fake connectors, each on a `fresh_store`:
- a refused `load` raises one alert carrying admin's kind and message and stderr, and
  one operations-log line; a second identical refusal updates the same alert (count 2);
- an `unknown` outcome (link cut with a verb in flight) raises an alert;
- lost contact on each plane raises one alert, and a revocation raises none;
- an `unload` answering `SavePointNotTaken` naming `finished` raises the prompt;
  `published` raises an explaining alert without options;
- the prompt's "save point, then unload": `save_point_taken` with its digest leads to
  `unload`; a `SavePointNotTaken` to the `save-point`, or a `save_point_taken` with no
  report, sends no `unload` and raises its own alert;
- an observer sees the alert and is refused acknowledging it (403, one record); an
  `operator` acknowledges (one audit record), and a stale version is refused;
- admin-con's shutdown with `unload` in the ceiling invokes nothing;
- the operations log holds no turn's text (the distinctive-text capture of 6c);
- a full log queue drops and counts, never blocking the door.

Perturbations, each shown to fail with its guard removed, in a fresh scratch worktree of
the pushed commit:
- drop the dedupe index, and a flapping link raises many alerts;
- drop the acknowledgement's version, and a repeat is acknowledged unseen;
- ask `unload` on any `save-point` answer, and a missed save point is followed by an
  unload;
- restore the orderly stop's `unload`, and a shutdown invokes it;
- raise an alert for an authorization refusal, and the test that a refused person
  raises none fails;
- write the log line in the connection task without a bound, and a stalled file holds
  the door.

## 6. Spec and documents

- The new subsection; Spec 8's conduit rule; 7.2's `SavePointNotTaken` legs (only
  `published` ends the run; the other legs leave it standing at rest, gate lowered,
  per the contract since 2026-10-09) and `BoundaryUnverified` at the publishing verbs
  (WeaverWeb #43); section 9 rows, with the count and summary swept in one commit.
- CLAUDE.md: the config key, admin-con's paragraph (no orderly-stop unload), the alerts.
- Closes WeaverWeb #43 (say so in the body).

## 7. Acceptance

- The full suite with `DATABASE_URL` before every push; clippy with and without the
  default features, `-D warnings`; `cargo fmt --check`; `tests/startup.rs --ignored`
  (its config gains the operations log's key, at a temporary path). Perturbations in a
  fresh scratch worktree of the pushed commit, each log read against its claim.
- ASCII only, absolute dates; no host path, mode or posture in the repository (the
  log's path is config). Check visibility before the push.
- PR body: `Implements:` naming the new subsection, Spec 7.2 and 8; the rulings of
  section 1; the alert-raising table (what raises, what does not); the data-loss
  refusal table; the cancellation table for the prompt's sequence; the perturbation
  table; "Closes #43".
