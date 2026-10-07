# Brief: act 11, IAM, PR 1: the design (documents only)

Version: v0.1, 2026-10-07. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. Act 11 builds identity and access on this server: the first of epic #18's three
gates. It is the first act under the operator's ruling of 2026-10-07 that code acts go in
**small pull requests, one concern each**. This is PR 1 of the act, and it lands documents
only: Spec section 2.13 requires the act's full design, threat model included, before any
code.

WeaverWeb's current work is the frontend for WeaverAgent (gate-con, admin-con, and the
surfaces a person uses to watch and act on an agent). IAM is what every one of those
surfaces stands on. Nothing in this act touches the weaver-analysis seam.

## 1. Read first

- Spec 2.8 (the session), 2.12 (the registered agent), 2.13 (identity and access, the
  whole section: it already charters persons, enrollment tokens, the two non-person
  principals, roles, grants, the identity exclusion, server-side authorization and the
  audit), 7.2 and 8 (the verbs, the ceiling, the link), 10 (the two open elections at
  about lines 3722-3735).
- Epic #18, its IAM item and its two open questions.
- `src/surfaces/gate.rs` (today's claimed-name session) and `src/bin/weaver-web.rs` (the
  browser listener, plain TCP today).
- CLAUDE.md's hard boundaries: no credential, hostname, certificate path or posture enters
  the repository.

## 2. What the operator has ruled (write these as closed, dated 2026-10-07)

1. **Sign-in is by passkeys (WebAuthn), and only by passkeys.** No passwords, no TOTP, no
   external identity provider. Spec 10's election "How people authenticate to this server"
   closes with this.
2. **The roles**, in the operator's words: "one observer per agent, one operator per
   agent, admin on the weaverweb server side is about controlling the multiple admin-con
   connections, the who and how of accessing as well as the what of individual agent
   lifecycle management." Mapped onto Spec 2.13:
   - `observer` and `operator` are roles scoped to one agent; a grant binds a person to one
     of them on one agent.
   - `admin` is the server-wide role 2.13 already fixes, and it governs three things: the
     connections (the register verbs: register, revoke, rotate, and the register's view of
     who is connected), the who and how (persons, enrollment, passkeys, disabling, grants),
     and the what (roles are rows the admin writes, so the admin decides which verbs each
     per-agent role carries, within the vocabulary of 7.2 plus `turn`).
   - Seeded defaults, which the admin may edit: `observer` = `show`; `operator` = `show`,
     `validate`, `load`, `unload`, `stop`, `turn`.
   - The admin role grants no action on any agent by itself; acting on an agent takes a
     per-agent grant.
   - **One reading for the operator to confirm at review**: "one observer per agent, one
     operator per agent" is read as one role of each kind per agent, held by any number of
     people. State the reading in the Spec and flag it in the PR body; if the operator means
     at most one holder each, the grant gains a uniqueness rule and nothing else moves.
   Spec 10's election "The role vocabulary" closes with this.

## 3. What the design decides

Propose each with its reasons and what was measured; the planner reviews, the operator
rules at review. **Measure, do not assume**: where a choice turns on a crate's dependencies
or a browser's behaviour, quote the evidence (a `cargo tree` summary, the specification
clause).

a. **The WebAuthn library.** Candidates include `webauthn-rs`; criteria: maintained,
   server-side registration and authentication ceremonies, discoverable credentials
   (passkeys) supported, attestation `none` accepted, and its dependency weight against this
   crate's tree, which is rustls-only today. If a candidate pulls OpenSSL or another C
   library, say so and what it costs the build and the install. Recommend one.

b. **The relying party and the secure context.** WebAuthn runs only in a secure context
   (https, or http on localhost), and the relying party ID must be a domain, not an IP
   address. So a passkey cannot be used from another machine until the browser listener
   serves TLS. Decide the order: **the planner recommends TLS on the browser listener
   ahead of the passkey PR** (rustls is already in the tree; the certificate and key are
   config paths, never in the repository), so passkeys are built and tested the way they
   will run. The RP ID and origin come from config. Say what the server refuses at startup
   when they are missing or inconsistent with the listener.

c. **The browser half.** The two ceremonies call `navigator.credentials.create` and `.get`
   in the page. No node and no SPA: a small hand-written module, vendored into the binary by
   `include_bytes!` as htmx is, doing those two ceremonies and nothing else. State its
   expected size and what it sends.

d. **The threat model** that 2.13 requires: the assets (an agent's trace, the power to act
   on an agent, the grants), the adversaries (someone on the network, a stolen session
   cookie, a lost or stolen device, a malicious or compromised admin, a compromised server
   host), and for each what IAM promises and what it does not. **Keep it bounded to what
   this server is**: one server, a handful of people, the host trusted. A threat the design
   does not defend against is named as out of scope, with the reason, rather than left
   unsaid.

e. **The session.** Cookie attributes (`Secure`, `HttpOnly`, `SameSite=Strict`), lifetime
   and idle expiry, sign-out, what ends a session (disable, removal of the passkey it was
   opened with), and cross-site request protection for htmx's posts (SameSite plus an
   `Origin` check is the planner's suggestion). Say what becomes of section 2.8's
   claimed-name session: retired, with the author member taking the person.

f. **Recovering a lost passkey.** Today 2.13 forbids an enrollment token for a row that
   already holds a credential. Options: several passkeys per person (enrolled while signed
   in); a host command that clears a person's passkeys and issues a token (the host is
   already trusted with the store); an admin-issued reset (which lets an admin take a
   person over, so say that if it is chosen). **The planner recommends several passkeys per
   person plus the host reset only.**

g. **The lone admin's grant.** 2.13 says no person writes a grant on themselves, so on a
   server with one admin, that admin can never hold `operator` on an agent. Options: a host
   command that writes any grant (the host already writes the bootstrap admin grant); or an
   exception for the only enabled admin. **The planner recommends the host command**, so no
   surface ever writes a grant on its own author.

h. **The audit record's shape**: the table, its two-record rule from 2.13, append-only, what
   it never holds.

i. **The identity exclusion's mechanism** (2.13 leaves it to this act): keep it coarse, one
   store-wide lock, since identity writes are rare.

j. **What is out**: passwords, OIDC, several servers sharing identity, per-person action
   lists, and anything on the box. Name them once.

## 4. Where it lands

- Spec 2.8 (the session carries the authenticated person), 2.13 (each mechanism decided
  above, and the role reading), 10 (the two elections closed, with today's date; any new
  open question opened there), 9 (rows for what the code PRs will enforce, marked owed).
- A design note `docs/project/design-2026-10-07-iam.md` holding the threat model and the
  library measurement, which the Spec cites rather than repeats.
- CLAUDE.md: the sentence that "real identity waits on the credential model" becomes a
  pointer to the design.
- Epic #18's body: the IAM item gains the PR plan below. (The planner edits the epic; the
  PR body says it was done.)

## 5. The PR plan (each gets its own brief after the one before it merges)

1. **This one**: the design.
2. **Persons, the bootstrap and enrollment tokens**: the migration, the host commands
   (bootstrap admin, issue a token, the host grant of 3g, the host reset of 3f), and their
   audit. No browser work.
3. **TLS on the browser listener** (if ruled ahead, per 3b).
4. **Passkey enrollment and sign-in**: the ceremonies, the vendored module, the session
   carrying the person, sign-out.
5. **Roles and grants**: the seeded roles, the admin's writes, the identity exclusion, the
   last-admin and self-grant rules, their audit.
6. **Server-side authorization**: every verb and turn checked against the person's grants
   and the connection's ceiling before a frame leaves, the shared hold, the audit of every
   ask.

The design may reorder or merge these; say why where it does. Keep each one a single
concern a reviewer can hold in mind.

## 6. Acceptance

- Documents only; no code, no migration.
- Each decision in section 3 stated with its reasons and, where it turns on a measurement,
  the measurement.
- The Spec swept in the same commit: every count, list and section 9 row the change moves.
- ASCII only, absolute dates, superseded text removed. **No hostname, RP ID, origin,
  certificate path or key path enters the repository**; they are config. Check visibility
  before the push.
- PR body: `Implements:` lines naming each Spec section changed, the role reading flagged
  for the operator, and "Refs #18" (the epic stays open).
