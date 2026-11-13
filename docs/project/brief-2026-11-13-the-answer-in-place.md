# Brief: the answer in place

Written by the WeaverWeb Executor on 2026-11-13, from the suite Planner's word of that day. Stack `act/the-answer-in-place`, built from `main` at `8bf397fa6efc305984264c0485798730f75c5193`, after the bar in two rows (#105) merged.

## Why

The operator made the first real load and the first turn from the agent page on 2026-11-13, on a virtual-machine agent. Relayed by the Planner:

- "when I clicked load there was no change in the screen, no visual output so I thoght I dindt click it and clicked again". The audit holds two first records for `load` from the operator, at 22:10:52.940Z and 22:10:58.111Z. The load took 93 s on the box. admin-con queued the second behind the first, and admin refused it with `agent_running` 9 ms after the first answered. No harm this time, but a second press of a turn would have placed a second turn.
- After the first turn: "why does getting tha anser take me to a new page...that seems unnecessary".

## Measured, read only

- **The forms are plain posts.** Every verb button and the turn form is a `<form method="post">`. The server holds the request through the ask and waits for the answer up to the silence bound (60 s by default), then draws a page of its own (`verb_answer.html`) with a way back. Until then nothing on the page changes. Past the bound the page answers 202, "asked, and not answered within the bound".
- **The verb forms sit inside `#run`**, which htmx re-fetches every 15 s and swaps whole, so the page redraws the same enabled buttons under a pending post.
- **A refusal is a bare text body** at its status (403, 404, 503), not a page.
- **The page's `htmx-config`** (`instrument.html`) swaps 2xx, 3xx and 401 and nothing else.
- **The vendored htmx is 2.0.10.** It carries `hx-post`, `hx-target`, `hx-swap`, `hx-disabled-elt`, `hx-indicator` with the `htmx-request` class, and `hx-sync`, all attributes, so `script-src 'self'` holds (measured 2026-10-08 for `hx-get` and `hx-post`). No surface may use `hx-on` or `js:` values.
- **The listener already holds each verb ask's record until its answer**: the connection's `asks` map (the verb, its first record, its principal), filled before the frame and taken by the answer.
- **The audit's outcome record holds `ok` or `failed` alone**, not admin's words. A refusal record keeps `answers` null (migration `0014`), and its refusal is free text.

## What was ruled (the Planner, 2026-11-13)

The scope was split in two, so that what bit the operator lands without waiting on a ruling.

1. **The answer in place.** The agent page's verb forms, the decision prompt's two options and the turn form post through htmx. The answer is drawn in the pane that asked, from one fragment that the plain answer page also renders, with everything the answer must show verbatim:
   - admin's answer as JSON, pretty-printed and escaped;
   - the exit status as a mark, and "admin's answer" or "admin's refusal";
   - admin's stderr cut at 2048 bytes at a character boundary, saying where it was cut;
   - the first record's identity;
   - the decision dialog where an `unload` stopped at its save point;
   - for a turn, the close's text, marked truncated at `finish = length`, or its stopped or refused reason;
   - gate-con's or admin-con's fault, or that the connection ended with the ask in flight.
2. **At the press:** the button disabled for the request's life, a second submit from the same form dropped in the browser, and the asking line shown. One stylesheet rule for the indicator, with `instrument.css` regenerated; the PR body names it as the operator's design call by the merge.
3. **The slot outside `#run`,** and the poll sharing the queue so it is dropped while an ask is in flight.
4. **Refusals drawn at their own status:** the page-wide swap of those codes, and the three polls drawing their refusal in their pane. The status stays the truth. The PR body names it as the operator's to rule by the merge.
5. **The console holds a second identical verb itself:** the same person, the same verb, the same agent, while the listener holds the first in flight. It is answered at once past the grant check with the time and the first record's identity: no frame sent, one refusal record (`already_asked`), no alert. The hold lives in the listener's memory and is gone at teardown. A different verb is not held. Another person's same verb is not held in this act; the PR body names it as an option the operator may take.
6. **Past the silence bound,** on the htmx path, the pane says when it was asked, the first record's identity, that it was not answered within the bound and that admin-con still holds it, and that the outcome is written to the audit and the load state will follow on this page. It draws no 202 page on the htmx path. It does not re-read a kept answer, because none is kept yet.
7. **A browser check under the CSP header**, the tests, and perturbations for these parts.

## Ruled out

- **Act 2 waits for the operator's ruling:** the answer kept in memory for the asker past the bound (it sits beside the rule that a turn's text is in no record), and the turn form's one-time token. The hub note drafts the two questions for the operator.
- No chat surface: it belongs to Agent Operations and is parked.
- No migration.
- No push until the Planner's word, after the bar and the declaration pane have gone up.
- No restart of the running console.

## The sub-act

1. **`act/the-answer-in-place-1-pane`: the answer in its pane.**
   - The answer fragment, rendered by the pane and by the plain page.
   - The htmx forms, the asking line, the slot, the poll's queue.
   - The page-wide swap of refusals and the polls' refusal fragments.
   - The listener's hold of a second identical verb.
   - The line past the bound.
   - Spec 2.8, 2.13, 2.14 and section 9; the brand book and the Controls guideline; `bundle.css` and `instrument.css`.

## Acceptance

- Each ruled behaviour held by a test, and every guard failing its test with the guard removed.
- The browser check under the CSP header, with what was run and what was seen.
- Each Spec section named under `Implements:` has a verdict.
- The gates pass: `cargo fmt --all -- --check`, `cargo build --locked`, and `cargo clippy --all-targets --locked -- -D warnings` with and without `--no-default-features`. So does `cargo test --locked` with `DATABASE_URL` and `PGUSER` set, on a scratch database, reporting what passed and that nothing was skipped.
- No credential, box path or security posture in the repository, the brief or a body.
