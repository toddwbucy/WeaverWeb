# Brief: act 4, the role shape (documents)

Version: v0.1, 2026-10-02. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for
the executor. A `docs:` act, no code. It writes the operator's rulings of 2026-10-02
into the Spec, the PRD and `CLAUDE.md`, so that admin-con (act 5) and IAM (act 6) are
built against them. The rulings and the ordered work are tracked in WeaverWeb issue #6;
read it first. **The Spec governs once this lands**; until then this brief and #6 do.

## 1. The rulings, 2026-10-02

- **No sudo and no root wrapper anywhere in WeaverWeb.** The operator's reason: a root
  process parsing arguments that arrived over a network is where a CVE comes from. The
  operator had chosen a root-owned wrapper that morning and reversed it the same day.
- **Access to the agent's verbs is a role**: a group grants specific verbs and nothing
  else. Authorizing a verb is weaver-admin's act, on its box, under the caller's role.
  That is WeaverAgents #50; until it lands, no verb runs from WeaverWeb.
- **The connectors run as dedicated service users**, one per agent and plane, never the
  operator's uid. gate-con's user holds only the agent's gate role; admin-con's user
  holds only read access to the agent's trace file and its admin role.
- **Every verb passes three gates**: WeaverWeb's IAM (the person's role on the agent),
  the box's ceiling (the verbs admin-con's role grants, declared in its hello and treated
  by the server as an upper bound, never a grant), and weaver-admin's role check.
- **The person behind a verb travels with it as a claim** for the box's operations log
  (WeaverAgents #51), labelled as the server's claim and never an authorization input.

## 2. What changes, by document

### Spec section 7.2 (the admin verbs)

- Replace "Each verb is an invocation, `sudo weaver-admin <verb> <agent>`" and "the sudo
  rule stands on admin-con's box and never on the server's" with the role shape:
  admin-con reaches the verbs through the interface WeaverAgents #50 settles, as a
  service user whose role the box grants, and holds no privilege of its own. State that
  until #50 lands no verb runs from WeaverWeb, and that the trace's tail and replay need
  none (group read access to the trace file).
- Say that the seed's `lifecycle.rs`, which invokes sudo, is not carried into admin-con.

### Spec section 8 (placement and the link)

- **The ceiling in the admin hello.** admin-con's hello names the verbs its role on the
  box grants, read from its own config, which the box owns. The server stores the
  ceiling on the row (section 2.12, an observed member with its date), treats it as an
  upper bound and never a grant, and never asks a verb outside it. An ask outside it is
  the server's own defect, refused by admin-con as `wrong_plane` or a new typed refusal
  if a clearer one is wanted. Name the reason: a compromised server reaches at most the
  ceiling.
- **The admission's `show` is required only where the ceiling grants `show`.** Where it
  does not, admission completes at `caught_up`, and the row's tuple and load state come
  from live trace events alone. The row says which source stands. This replaces the
  unconditional sentence act 2 added.
- **Each verb ask names the requesting person** as a claim the box records (WeaverAgents
  #51), labelled as the server's claim. The frame member is added in the admin-con act;
  the Spec says it here.
- **The connectors run as dedicated service users.** Amend the three-processes paragraph
  and section 1's tree note ("admin-con holds the sudo rule and the trace tailer") to
  say what each connector's user holds and nothing more.

### Spec section 2 (the store), a new section 2.13

Charter IAM at the level the store needs, without electing the mechanism:

- **The person**: an identity authenticated to the server, distinct from section 2.8's
  session, which today carries a claim and no proof. Say how 2.8 changes once
  authentication stands: the session carries the authenticated person.
- **The role and the grant**: a role is a named set of verbs; a grant binds a person to
  a role on one agent, or server-wide for registering agents. Authored rows under
  section 3.2.
- **The audit record**: every verb asked names the person, the agent, the verb, the
  outcome and when, written by the server before the ask leaves it and completed when
  the answer lands.
- **Server-side authorization**: a verb is asked only if the person's grants on that
  agent permit it and the agent's ceiling contains it; otherwise refused before any
  frame leaves the server, the refusal audited.

### Spec section 9

Add owed rows, each with its perturbation, for at least: the server never asks a verb
outside the agent's ceiling; a verb the person's grants do not permit is refused before
an ask; every verb asked has an audit record naming the person; no privileged
invocation exists in the crate (instrument: review, plus a test that the source tree
contains no `sudo` invocation, if you find one honest). Update the owed count.

### Spec section 10 (open elections)

Two new entries, each the operator's: **how people authenticate** (passkeys, local
passwords with TOTP, or an external identity provider), and **the role vocabulary**
(proposed: observer with `show` and `list`, operator adding `validate`, `load`,
`unload` and `stop`, per agent; and a server-wide admin who registers agents).

### PRD section 6 (identity and roles)

The section defers identity, authentication and transport encryption to named
triggers. Rule that the trigger is met for anything that can act on an agent: no
surface that asks a verb ships before IAM, and TLS on the browser's listener lands
with it, since credentials will cross it. Reading surfaces may stand before IAM as they
do today. Keep what still holds; remove what this supersedes.

### CLAUDE.md

- A hard boundary: no sudo, no root wrapper, no privileged invocation anywhere in this
  repository; the connectors run as dedicated service users; verbs are authorized by
  role on the box (WeaverAgents #50).
- The "Becomes admin-con" paragraph: drop `sudo weaver-admin` as the seed to carry, and
  say the verb half waits on WeaverAgents #50 behind an abstract invoker.
- Point at issue #6 beside the briefs.

## 3. Acceptance

Every ruling in section 1 traceable to a sentence in the Spec, the PRD or `CLAUDE.md`.
A grep of the Spec, the PRD and `CLAUDE.md` for `sudo` finds only sentences saying
there is none, or dated records. Every new section 9 row has an instrument, owed until
its act. ASCII only, absolute dates, superseded text removed, no box path or uid. The
PR body carries `Implements:` lines and names the litter picked up. Check visibility
before pushing.

The briefs of acts 1 to 3 and the handoffs are dated records and stand; where one
mentions a sudo rule, it is superseded by this act, which the PR body says once.

## 4. What follows

- **Act 5, admin-con**: the trace tailer, the replay and `caught_up`, the ceiling in the
  hello, the principal member on the verb frame, and the verb plane against an abstract
  invoker with a test fake and no privilege code. Proven live on a loaded agent's trace.
- **Act 6, IAM on WeaverWeb**: the person, authentication, roles and grants, the audit
  record, server-side verb authorization, TLS on the browser listener. Waits on the
  operator's two elections.
- **Later**: the install act (service users, configs, units, no box paths), the legacy
  removal, and the real invoker when WeaverAgents #50 lands.
