# Brief: the trace header's length in admin-con

From the planner seat to the executor, 2026-11-07. WeaverAgent #174 (merge `00edfe0`, closing WeaverAgent#88; the notice is the second comment on #16) gave the trace door's header a `length` member: the end of the last record complete at the relay's size read for the request, read once the position verifies, a record boundary like every offset on the wire, and a lower bound for the reader. admin-con's relay client was built on 2026-10-05 against a header that named no length and takes every measure on the relay's heartbeat (Spec 7.2, "Three measures"). This act reads the length for the two measures it is for. Land this brief as `docs/project/brief-2026-11-07-the-trace-headers-length-in-admin-con.md`, the PR's first commit.

## Scope

- **Base:** `main` at `381858e`.
- **Branch:** `code/the-trace-headers-length-in-admin-con`.
- **Commits:** `code: ...` for the relay client, the fake relay and the tests; `docs: ...` for Spec 7.2 and section 9; one commit for the litter in section 4. The brief first.
- **The diff touches `src/link/relay.rs`, `src/link/admin_con.rs`, `src/link/fake_relay.rs`, `src/link/admin_con_tests.rs`, `docs/weaver-web-Spec.md`, `src/traceview.rs` (section 4 only) and this brief. Nothing else.** The listener, the frames, the window and the surfaces are untouched: the server's classification of events against the boundary is unchanged, since the boundary it receives is still a position admin-con reports in its hello or its `door` frame. `CLAUDE.md`'s admin-con paragraph names the bounds; that edit is the planner's, on main, after the merge. Name it in the PR body.
- The operator's working rule of 2026-11-06 governs every document edit: a document states the current state of its subject; a passage that no longer holds is replaced in place; git is the archive. Grep the docs diff for "formerly", "as of", "superseded", "for now", "now" and dated headings before pushing.
- The distributed-state checklist applies (the five classes: a write and its acknowledgement, an order between two steps, a boundary taken under a bound, a reconnection, a restart). Pre-review the diff against it before telling the planner.
- ASCII only, no em-dash. No box fact, path or account shape enters the tree.
- Open a **draft** PR and tell the planner. One push per review round.

## 1. The ruling, and the one place this brief departs from the handoff

The three measures of Spec 7.2, each "everything the file held at that moment":

1. **The backfill's start**, the first record boundary within `backfill_bytes` of the boundary.
2. **The opening's boundary**, the hello's or a later opening's.
3. **The drain before a verb.**

The ruling (the notice on #16, 2026-10-15, and the planner's handoff of 2026-10-16): **the length is for measures 1 and 2 only.** For measure 3, the contract's own sentence governs: "A reader bounding an act of its own (a verb's invocation) places the act by the trace's own events, the act's event, never by the length alone. A boundary mid-stream is taken by a new request." A drain by the length would need a reconnection, which replaces the live stream, for a lower bound. So the drain keeps the heartbeat as its measure and `DRAIN_BOUND` as its floor, untouched. The 2026-11-06 handoff said the length "replaces `BOUNDARY_BOUND` (30 s) and `DRAIN_BOUND` (10 s) where present"; that sentence is wrong on the drain, and on the boundary's bound it is only half right (section 2). Follow this brief.

**Read it as `Option<u64>`.** Upstream the member is required, so an older relay's header has no `length`. `TraceHeader` in `src/link/relay.rs:76` is not `deny_unknown_fields` (the `deny` on `TraceLine` at line 62 does not reach the nested struct), so today's reader already ignores the member. Reading it as optional means one admin-con serves an older relay and a current one with no rollout ordering: a header naming a length takes measures 1 and 2 at it; a header naming none takes them on the heartbeat exactly as today. Both paths stay, both are tested.

## 2. The relay client

**`src/link/relay.rs`.** `TraceHeader` gains `#[serde(default)] length: Option<u64>`. `Stream` gains `pub length: Option<u64>`, set at the dial from the header beside `identity` (line 157). The notice gives the wire shape: `{"trace_stream":{"header":{"device":...,"inode":...,"length":...,"birth_ns":...}}}`, `length` after `inode`. Nothing else on the wire changes: the request, the digest rule, the control lines and the framing are as they are.

**`src/link/admin_con.rs`, `measure` (line 442).** After every dial, the one at `from` and a redial at zero after a truncation, the stream may name a length. Where it does:

- If the dialed position's offset is at or past the length, nothing is behind the boundary: return `Opened` at once with the boundary at that position, reading nothing. (The length is a record boundary and the request's position verified at one, so "at or past" is "equal" on a well-formed relay; past is tolerated, never refused, since the length is a lower bound.)
- Otherwise read records as the loop does now, and return with the boundary at the position reached the first time the offset is at or past the length. The ring keeps the last `cap` bytes read, as today, so the backfill's start (measure 1) is the first record boundary within `backfill_bytes` of this boundary, by the mechanism already there.
- A heartbeat before the offset reaches the length is the relay's word that the file's end came first; it ends the read as today, with a `warn` naming both numbers. Do not refuse it.
- A truncation redial reads the new header's length; the loop continues under it.

Where the stream names no length, the loop is exactly today's: the first heartbeat is the boundary.

**The bound stays.** `BOUNDARY_BOUND` and `VERIFY_RESERVE` cover the opening whole, its dials, its reads and its verification; a relay that stalls mid-read, or a header that names no length under a writer that never idles, still needs the floor. With a length the read ends at the length and the bound is not reached on a healthy relay, which is what the perturbation shows. Rewrite the two doc comments that state the heartbeat as the only measure (lines 55-63 and 425-441, and any other that says "while the relay names no length"), stating the current state: the length where the header names one, the heartbeat where it does not, the bound as the floor under both. The drain's comment at 2148-2160 is unchanged in substance; where it says "while the relay names no length", say instead that the length is a lower bound and a boundary mid-stream would need a new request, so the drain is measured on the heartbeat by the contract's rule.

**A server position past a boundary taken at the length.** Spec 7.2 says a position the server holds past a boundary taken at its bound is caught up already once verified. A record past the length may have completed before the header reached admin-con, so the same holds for a boundary taken at the length. Read the path `an_acknowledgement_past_a_bounded_boundary_is_caught_up` exercises and confirm it keys on "the server's position is past the boundary" and not on how the boundary was taken. If it does, say so in the PR body with the line; if it does not, make it so and cover it (section 3).

## 3. The fake relay and the tests

**`src/link/fake_relay.rs`.** The header (line 241) gains `length`: at the request's acceptance, the file's size and the end of the last complete record at or below it (the byte after the last newline, 0 for an empty file; a last line still being written is not counted). A knob on `Counts`, `omit_length: AtomicBool`, leaves it out, for the older relay. Every existing test runs against a header that names a length, so the existing suite is the measure that nothing else moved.

**Tests, in `src/link/admin_con_tests.rs`**, each stated against the real listener and the fake relay as the suite's are, each with its perturbation shown and logged (in a detached worktree of the pushed head, per the process):

1. **An opening under a writer that never idles is admitted at the length**, well inside `BOUNDARY_BOUND`: the boundary is the header's length, the events behind it are replayed, the events the writer added after the size read are live. Perturbation: ignore the length, and the test's admission waits to the bound (what `a_never_idle_writer_is_admitted_at_the_boundarys_bound` shows today). Rewrite that test against `omit_length`, so it states what it now covers: the older relay's header.
2. **A header naming no length takes the boundary at the heartbeat**, as today, under `omit_length`: the existing opening tests, run once against the knob, or one of them re-stated under it. Say which.
3. **A record completed after the size read and before the header's receipt is live**: hold the fake relay between its size read and its header (a knob, or the `header_delay_ms` already there), append a record in the gap, and the record lands live with `caught_up` before it. This is the "lower bound" sentence of the contract, and it is the clause the distributed-state checklist is for.
4. **A position at the length is admitted with nothing behind it**: the dial's offset equals the length, no record is read, `caught_up` follows the hello's answer at once.
5. **The backfill is bounded at the length**: after a server restart under a long file, the tail relayed is the last `backfill_bytes` before the length, the mark at its front, nothing past the length replayed. Rework `a_server_restart_relays_a_bounded_tail_with_a_mark` or add beside it.
6. **The drain is untouched**: `the_drain_runs_before_the_invocation` and `a_verb_behind_a_never_idle_writer_runs_at_the_drains_bound` pass unchanged. Their passing is the proof measure 3 did not move; name them in the PR body.

`cargo test --locked` whole with `DATABASE_URL` set before every push (the link's tests run one at a time, as the suite does it). Clippy both ways, `--no-default-features` too. `tests/connectors_link.rs` still passes, since no dependency moves.

## 4. Litter, one commit

- `src/traceview.rs:1-6` says admin-con "tails the agent's NDJSON file" and names "Rotation, truncation, and link loss" as the marks; lines 23-25 name "the tailer or the server" and list "rotation, truncation, parse failure, link loss". The tailer left in act 9b. State the current state: admin-con relays through the trace door (`link::relay`), and the marks are the ones `Item::Mark` in `src/link/admin_con.rs` and `windows.mark(...)` in `src/link/listener.rs` write. Grep both and list what they write; do not take the list from this brief. `link_outage` is an entry kind, not a mark reason.
- Nothing else. The live window carrying `replayed` is a separate small PR after this one, so the frame's field does not mix with the relay's.

## 5. Spec 7.2 and section 9

- The "Three measures" paragraph (about lines 3700-3732) states the current state: measures 1 and 2 are taken at the header's length where the header names one, and on the relay's heartbeat where it does not; the length is a lower bound, so a record completed after the size read is live; measure 3 is taken on the heartbeat, with the contract's reason. The 30 second bound and its reserve stay as the floor under the opening, stated as such. The paragraph at 3800-3810 on the drain is unchanged.
- Section 9, the row "the trace is replayed from the acknowledged position" (about line 4549) and its clause "**The measures' bounds**, against a fake writer that never idles": add the length's clauses from section 3, and keep the bound's two. The residue sentence naming `toddwbucy/WeaverAgent#88` narrows further: say what it still covers (the drain's window, and an older relay's opening).
- Any `//! conforms:` header in the touched files that names the claim stays true; check each.
- Section 9's counts, if any count the clauses, follow in the same commit.

## Gates, in a fresh detached worktree of the pushed head

Run `git worktree prune` first; use a new worktree name with `|| exit 1` after the add; print `git log -1` inside it and check it is the PR head.

1. `cargo fmt --check`; `cargo clippy --all-targets --locked -- -D warnings` and the same with `--no-default-features`.
2. `DATABASE_URL=... cargo test --locked`, the whole suite, the `test result` lines in the PR body.
3. Each perturbation of section 3 run and its log read against its claim, one line each in the PR body: what was removed, which test failed, how.
4. `grep -rn 'formerly\|as of\|superseded\|for now' <docs diff>` empty, or each hit explained.
5. The PR body names: `Implements: Spec 7.2 (the measures), Spec 9 (the row)`, the drain's two tests as the proof of measure 3, the caught-up path's line from section 2, the `CLAUDE.md` edit left to the planner, and the litter of section 4.
