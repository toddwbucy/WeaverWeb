# Design: identity and access on WeaverWeb (act 11)

Version: v0.1, 2026-10-07. The design Spec section 2.13 requires before any code of the
IAM act, written to the brief `brief-2026-10-07-act-11-iam-pr1-the-design.md` beside this
note. The Spec states each rule this note decides and cites this note for the reasons, the
measurements and the threat model, which it does not repeat. Nothing here is code; every
mechanism named is for the pull request the plan in section 12 assigns it to.

## 1. What the operator ruled, 2026-10-07

1. **Sign-in is by passkeys (WebAuthn), and only by passkeys.** No password, no second
   factor, no external identity provider.
2. **The roles.** In the operator's words: "one observer per agent, one operator per agent,
   admin on the weaverweb server side is about controlling the multiple admin-con
   connections, the who and how of accessing as well as the what of individual agent
   lifecycle management." Read onto Spec 2.13:
   - `observer` and `operator` are roles scoped to one agent, and a grant binds a person to
     one of them on one agent.
   - `admin` is the server-wide role 2.13 already fixes. It governs the connections (the
     register verbs and the register's view of who is connected), the who and how
     (persons, enrollment, passkeys, disabling, grants), and the what (the per-agent roles
     are rows the admin writes, so the admin decides which verbs each carries, within the
     vocabulary of Spec 7.2 plus `turn`).
   - Seeded, and the admin may edit them: `observer` carries `show`; `operator` carries
     `show`, `validate`, `load`, `unload`, `stop` and `turn`.
   - The admin role grants no action on any agent by itself; acting on an agent takes a
     per-agent grant.
   - **The reading the operator confirms at review**: "one observer per agent, one operator
     per agent" is read as one role of each kind per agent, held by any number of people.
     If the operator means at most one holder of each, the grant gains a uniqueness rule
     and nothing else in this design moves.
3. **Code acts in small pull requests, one concern each.** This act is the first under that
   rule; section 12 is its plan.

## 2. The WebAuthn library

**Criteria** (the brief's): maintained; server-side registration and authentication
ceremonies; discoverable credentials (passkeys); attestation `none` accepted; and the
dependency weight against this crate's tree, which is rustls-only today (`rustls` and
`tokio-rustls` on `ring`, no C library beyond the platform's own).

**Measured on 2026-10-07**, each candidate resolved alone in a scratch crate outside the
repository, `cargo tree -e normal`, and its crate names compared with the 199 crates of
this crate's own normal tree at `96dd2fa`, this crate included. A candidate's count
excludes the scratch crate:

| Candidate | Crates | Not already in this tree | Native library | Releases |
|---|---|---|---|---|
| `webauthn-rs` 0.5.5 | 92 | 30 | **OpenSSL** (`openssl` 0.10.81, `openssl-sys` 0.9.117) | kanidm project; about 7.5 million downloads; 0.5.5 released 2026-04-30 |
| `webauthn-rs` 0.6.1-dev | 154 | 80 | none (RustCrypto through `crypto-glue`: `p256`, `p384`, `p521`, `rsa`, and also `argon2`, `scrypt`, `aes-gcm`) | a pre-release, 2026-04-30 |
| `webauthn_rp` 0.3.0 | 87 | 34 | none (RustCrypto: `p256`, `p384`, `ed25519-dalek`, `rsa`) | one maintainer on a personal git host; about 31,000 downloads; last release 2025-04-03 |

**What each supports.** All three have server-side registration and authentication, accept
attestation `none` (`AttestationConveyancePreference::None` in `webauthn-rs`;
`AttestationFormat::None` in `webauthn_rp`), and support discoverable credentials. In
`webauthn-rs` 0.5 the passkey ceremonies (`start_passkey_registration`,
`start_passkey_authentication`) are stable, and **the discoverable, name-free sign-in
(`start_discoverable_authentication`) sits behind the `conditional-ui` feature**, which the
crate groups under `preview-features`.

**What OpenSSL costs.** A probe binary on `webauthn-rs` 0.5.5 built here in 6.5 s in
release and links `libcrypto.so.3` dynamically (`ldd`), found through `pkg-config`.
So:
- **the build** needs OpenSSL's headers and `pkg-config` on the build machine;
- **the install** needs `libcrypto` 3 on the server's host;
- `openssl-sys` offers a `vendored` feature that builds OpenSSL from source and links it
  statically, which needs a C compiler and Perl at build time instead.

**Recommendation: `webauthn-rs` 0.5**, the server binary alone carrying it.
- It is the maintained, widely used one, written by an identity-management project, and its
  API is built around passkeys. The ceremonies are security-critical verification code, and
  for them a large user base and an active maintainer weigh more than a pure-Rust tree.
- The cost is OpenSSL on the server's host. **The connectors must not carry it**: gate-con
  and admin-con run on agent boxes, where a new shared library is a provisioning change.
  PR 4 enables the library through a cargo feature that only the server binary requires
  (`required-features`), and shows with `ldd` that neither connector links `libcrypto`.
  **That is a measurement this pull request cannot take without code**, so it is owed by
  PR 4 and named there.
- Sign-in is name-first (section 6), using the stable passkey ceremonies, so no preview
  feature is needed.
- `webauthn-rs` 0.6 drops OpenSSL. When it is released stable, moving to it removes the C
  library at the price of a heavier tree (80 new crates against 30). That is a later
  measurement, not this act's.
- **If the operator rules out a C library on the server**, the alternative is `webauthn_rp`.
  It is pure Rust and adds 34 crates, but rests on one maintainer and has had no release
  since 2025-04-03.

## 3. The relying party and the secure context

WebAuthn runs only in a secure context. WebAuthn Level 2 (W3C Recommendation, 2021-04-08)
says "user agents only expose this API to callers in secure contexts", which for a web page
means https, or the `localhost` origin a browser treats as potentially trustworthy. Its
section 5.1.3 refuses with `SecurityError` where "effectiveDomain is not a valid domain
string", which an IP address is not, and where the relying-party ID "is not a registrable
domain suffix of or is equal to effectiveDomain". Sections 7.1 and 7.2 have the server
"verify that the rpIdHash in authData is the SHA-256 hash of the RP ID". So a passkey
cannot be used from another machine until the browser's listener serves TLS under a name.

**Decided: TLS on the browser's listener lands before passkeys**, as its own pull request
(PR 3), so passkeys are built and tested the way they will run. The charter's section 6
already says transport encryption on the browser's listener lands with the IAM act.
- rustls is already in the tree. The listener takes `tls_certificate` and `tls_key`, paths
  in the server's config, never in the repository, read at start like the authority.
- `rp_id` and `origin` are config too. No hostname, relying-party ID or origin enters the
  repository.

**What the server refuses at start**, before anything listens, where passkeys are on:
**the faults listed here, and nothing beyond them.** The list is the promise:
- no `rp_id` or no `origin`;
- **an `origin` that is not a serialized origin**: scheme, host and a port, and nothing
  else. A value with userinfo, a path (a trailing `/` included), a query or a fragment is
  refused, and so is a scheme or host not in lower case or a port written where it is the
  scheme's default. The value must be exactly what a browser sends in its `Origin` header,
  since the server compares that header with it as a string (section 6), and a trailing
  slash or an explicit `:443` would make every request fail the comparison. Refusing
  rather than stripping keeps the config and the comparison one fact;
- an `rp_id` that is an IP address, or empty;
- an `origin` whose scheme is not `https`, unless it is exactly `http` on the host
  `localhost`, the one plain origin a browser treats as secure, which tests and a
  developer's machine use; any other scheme, or `http` on any other host, is refused;
- an `origin` whose host is neither the `rp_id` nor a subdomain of it;
- an `https` origin on a listener with no certificate and key configured;
- a certificate and key that do not load or do not pair;
- **a certificate not valid for the origin's host**, verified as rustls verifies a server
  name, by webpki's name check against the certificate's subject alternative names, so a
  browser reaching the origin would not refuse the name;
- **a certificate not yet valid, or already expired, against the clock at start**, its
  validity period being in the file. **Nothing is promised about expiry while the server
  runs**: a certificate that expires later is the browser's to refuse. The server logs a
  warning at start where the certificate expires within 14 days, one line that gives the
  operator a renewal's notice at no cost.

Each refusal names the key and what is wrong, as the authority's absence does today.

**Any other fault of the configuration is not refused at start.** Two examples:
- a public suffix as the relying-party ID, which only the Public Suffix List can tell, an
  external list that changes;
- a certificate unfit for server authentication: its extended key usage, its key usage,
  its key's strength or its chain.

Each shows at the first connection or ceremony as the browser's failure, a refused TLS
handshake or the ceremony's `SecurityError`, which the server logs and the sign-in page
reports where it can. A list that grew by one browser rule per review would never be
complete, so the browser's rules beyond it stay the browser's to apply.

## 4. The browser half

The two ceremonies call `navigator.credentials.create` and `navigator.credentials.get` in
the page. There is no node and no SPA. **One hand-written module** does those two
ceremonies and nothing else, vendored into the binary by `include_bytes!` as htmx is and
served from the surfaces' asset route:
- **Expected size**: under 4 KiB unminified. It converts base64url to and from
  `ArrayBuffer`, fetches the server's options, calls the browser, and posts the result.
- **What it sends**, as JSON to the server's two ceremony endpoints:
  - registration: the credential's `id`, `rawId`, `type`, and `response.clientDataJSON`
    and `response.attestationObject`, with `response.transports` where the browser gives
    them;
  - authentication: `id`, `rawId`, `type`, and `response.authenticatorData`,
    `response.clientDataJSON`, `response.signature` and `response.userHandle`.
  - Nothing else: no extension output, no device detail. These are the shapes the library
    parses.
- The page has no inline script for it, so a `Content-Security-Policy` of
  `script-src 'self'` stays possible. Whether the surfaces adopt that header is PR 4's to
  measure against htmx's own use.

## 5. The threat model

**What this server is**: one server, a handful of people, the server's host trusted. It
holds an agent's trace as relayed, the power to ask an agent's verbs and place its turns,
and the identity rows (persons, passkeys, roles, grants) that decide who holds that power.
It holds no secret of a person: a passkey's stored half is a public key.

**Assets**
1. **An agent's trace**, which is read access.
2. **The power to act on an agent**: a lifecycle verb, or a turn that prompts an agent able
   to act on its box with its tools.
3. **The identity rows**: persons, passkeys, roles, grants, enrollment tokens.
4. **The audit record**, which is the one record of who asked, since nothing about a person
   crosses to the box.

**Adversaries, and what IAM promises against each**

| Adversary | Promised | Not promised |
|---|---|---|
| **Someone on the network** between a browser and the server | No credential, cookie or trace crosses in the clear once TLS stands (PR 3); a passkey assertion is bound to the origin and the challenge, so a captured one replays nowhere | Availability: flooding the listener is out of scope |
| **A phishing page** | A passkey answers only its relying party's origin, so a look-alike site cannot collect a usable assertion | A person who installs a malicious extension in their own browser |
| **A stolen session cookie** | The cookie is `HttpOnly`, so page script cannot read it; `Secure`, so it never crosses plain http; and `SameSite=Strict` with an `Origin` check, so another site cannot ride it. It expires idle and absolutely. **It cannot add a passkey**: adding one takes a fresh assertion with an existing passkey (section 7), so a thief cannot make the access outlast the session. Disable and sign-out end it at its next use, and an open live view within 15 seconds (section 6) | A thief holding the cookie can act as the person until the session ends, within the person's grants |
| **A lost or stolen device** | A passkey needs user verification (the device's unlock) at every ceremony; the person removes that passkey from another one, or an admin disables the person, or the host resets them (section 7); each ends every session opened with the passkey at its next use, and an open live view within 15 seconds | A device whose unlock the thief also holds is the person, until one of the above |
| **A malicious or compromised admin** | An admin cannot take a person over: no admin reset, and an enrollment token only for a person with no passkey. An admin cannot widen their own grants, edit a role they hold, or remove the last admin. Every admin write is audited before it lands | An admin can grant another person anything, disable people, rewrite the roles they do not hold, and register or revoke connectors. **Two colluding admins can grant each other anything.** That is the admin role as ruled, and the audit is the remedy |
| **A compromised server host** | Nothing. The host holds the store, the authority and the host commands, which write any grant by design | Out of scope: the host is trusted, as the connectors' link already assumes |
| **A compromised agent box** | Nothing new here: the connectors are mutually authenticated and the box's sudo rule is the third gate (Spec 8) | Out of IAM's scope |

**Out of scope, named so it is not left unsaid**: denial of service; a malicious browser
or extension; the server's host; the box; recovering an audit that someone with the store
rewrote (append-only is enforced in the store against this crate's processes, not against
the host's database owner).

## 6. The session

**The cookie** is `__Host-weaver_session`. The `__Host-` prefix makes the browser refuse
it unless it is `Secure`, has `Path=/` and has no `Domain`, so no subdomain can set or read
it. Its attributes are `Secure`, `HttpOnly`, `SameSite=Strict` and `Path=/`. The bearer is
drawn under the rule below and stored only as its digest (Spec 2.8's rule, unchanged).

**Every bearer the server issues is 32 bytes from the operating system's cryptographic
random source**: the session bearer, the enrollment token and the ceremony identity
alike, and any bearer a later pull request adds. It is never derived from a counter, a
time or a row identity, so holding one bearer tells nothing of another. The source is read
through `getrandom`, already in this crate's tree at two versions through `rand`; taking it
as a direct dependency, or reading it through `rand`'s `OsRng`, is PR 2's to measure.

**Its row** carries the person and the passkey it was opened with, beside the digest, when
it opened, when it was last used and when it closed. **The last-used time is written at
most once a minute per session**: an ordinary request refreshes it when the stored time is
a minute old or more, and an open live view refreshes it at its 15-second re-check under
the same rule. The writes stay bounded whatever a page polls, and the idle check reads the
stored time, so idle expiry is accurate to a minute, which a one-hour idle limit needs no
finer than.

**Lifetime.** It expires after one hour idle and twelve hours absolute, both in the
server's config with those defaults. An open live view (SSE) counts as use while it
streams, so a person watching a run is not signed out mid-run, but the twelve-hour limit
still holds.

**What ends a session**:
- sign-out, which closes the row;
- the idle or absolute expiry;
- disabling its person, seen at the next use as Spec 2.13 already says;
- removing the passkey it was opened with;
- the host reset of its person, by clearing the passkey the session was opened with.

Each is checked at every use, so no end needs a write to every session, and **nothing but
the surface writes a session**: a disable, a passkey's removal and the host reset each
change the person or the passkey, and the session sees it at its next use.

**Sign-in is name-first.** The person gives their name; the server answers with a
challenge for that person's passkeys (`start_passkey_authentication`); the browser
answers; the server verifies, updates the passkey's counter and opens the session.
- Name-free, discoverable sign-in needs the library's preview feature (section 2) and gains
  nothing for a handful of people. **Not every passkey enrolled is discoverable**: the
  stable registration asks the authenticator for residence and does not require it, so a
  later name-free sign-in would serve only the passkeys their authenticators made
  discoverable, and the others would need enrolling again then. Name-first needs none of
  it, so nothing is required now.
- Name-first answers whether a name exists. For a handful of named people on one server,
  that is accepted and stated rather than hidden.

**A person's name is unique, compared in one canonical form**, since name-first sign-in
finds the person by it. The form is Unicode's compatibility caseless form (the Unicode
Standard, section 3.13, D146: `NFKD(casefold(NFKD(casefold(NFD(name)))))`), taken after
trimming leading and trailing white space. So `Ada`, `ada`, `Ada` in full-width
letters, and an accent composed or decomposed are one name, which is how a person types
theirs on any keyboard, and a second person cannot enroll a case or width variant of it. Uniqueness is
checked at enrollment and at rename, under the identity exclusion, and a name whose form
another person's already has is refused; the person's name is kept as given, trimmed, and
sign-in looks the person up by the form. **Confusables across scripts** (a Cyrillic `a`
for a Latin one) are not folded by this form; that would take a UTS #39 skeleton, and for
a handful of people enrolled by an admin it is out of scope, named here. Which crate
computes the form is PR 2's to measure.

**The signature counter.** `webauthn-rs` 0.5.5 already refuses a counter that did not
rise: its builder sets `require_valid_counter_value`, and where the returned counter or
the stored one is nonzero, a returned counter not greater than the stored one fails the
ceremony with `CredentialPossibleCompromise` (`webauthn-rs-core` 0.5.5, `core.rs`, the
check after the signature). What the library leaves to this crate is the stored half, and
**it holds for every assertion the server verifies**, whatever ceremony asked for it: the
sign-in, the fresh assertion that authorizes adding a passkey (section 7), and any
assertion a later pull request adds, none of which needs a sentence of its own:
- **the counter is persisted after every assertion that returned one, in a transaction of
  its own that commits before the authorized action's transaction begins** (a session's
  opening, a passkey's addition), by an update that only raises it (`WHERE` the stored
  counter is below the returned one). Two concurrent assertions with one passkey both
  pass the library's check against the value they loaded; the second update then moves
  no row, and that assertion is refused as the library would have refused it, so the
  comparison is never against a stale value. **An action that then fails leaves the
  counter raised**, which is right: the authenticator did advance, and a counter rolled
  back with a failed action would let the same assertion's counter be replayed;
- **a `CredentialPossibleCompromise` refusal is audited as a possible cloned credential**,
  its principal the passkey's person and its method `passkey assertion`, since the
  signature verified and only the counter failed; what the assertion would have
  authorized does not proceed: no session opens, no passkey is added;
- **the passkey is not disabled automatically**: an attacker replaying a clone could then
  lock its owner out at will. The person removes it, an admin disables the person, or the
  host resets them;
- **where both counters are zero**, as synced passkeys report, there is no counter to
  compare, and the check does not apply. Most passkeys today are synced, so this guard
  covers device-bound authenticators and not most people's passkeys; it is stated so
  nobody reads more into it.

**Ceremony state.** A ceremony's challenge and state stay in the server's memory, keyed by
a ceremony identity, for at most five minutes, and are used once. A restart drops the
ceremonies in flight, and the person begins again. Nothing of a ceremony is stored.
**The ceremonies in flight are capped at 64**, and a new one beyond the cap is refused
until one completes or expires. Name-first sign-in starts a ceremony for any posted name,
before anyone is authenticated, so without a cap an unauthenticated client could fill the
map until the process fails. The figure is a handful of people, each with a ceremony or two
in flight, and a wide margin; a ceremony's state is under a kilobyte, so the cap holds the
map to tens of kilobytes whatever a client sends. Denial of service stays out of scope
(section 5): a client filling the cap delays sign-in for five minutes and crashes nothing.

**An open live view is one request**, so "checked at every use" would never check it
again. **A stream re-checks its session and its person's grant on its agent every 15
seconds**, and closes at the first check that finds either ended: the session closed,
expired (the twelve-hour limit included) or its passkey removed, the person disabled, or
the grant removed. A timer and not each event: a trace can carry many events a second and
a quiet agent none, and the check should cost the same either way, one indexed read per
stream per 15 seconds, with what was revoked visible for at most 15 seconds after.

**Cross-site requests.** `SameSite=Strict` keeps the cookie off every request another
site starts. Every request that changes state (every POST, htmx's included) must also
carry an `Origin` header equal to the configured `origin`, or it is refused before its
handler; the browser sets `Origin` on every POST. Both are owed by PR 4.

**Section 2.8's claimed-name session retires.** Its claimed name and its configured role
go. The session carries the authenticated person, and Spec 3.2's author member takes its
value from the person.
- The member holds **the person's identity** (`pe-`) and not their name, since a person can
  be renamed and an author must not move with them; surfaces render the current name.
- A row written before the act keeps the claimed name it carries, which reads as a claim
  because it resolves to no person.

This settles Spec 10's "What an author names".

## 7. Recovering a lost passkey

**Decided: several passkeys per person, plus the host reset, and nothing else.**
- **A credential ID belongs to one passkey of one person.** The passkey table holds a
  unique constraint on the credential ID across every person, and a registration whose ID
  is already held, by anyone, the registering person included, is refused by that
  constraint at its insert, atomically, as `finish_passkey_registration`'s contract asks
  ("You MUST assert that the registered `CredentialID` has not previously been registered
  to any other account").
- **A person enrolls more passkeys while signed in**, each after a fresh assertion with a
  passkey they already hold, taken within the same ceremony. A stolen session cookie
  therefore cannot add a passkey. The assertion falls under section 6's counter rule like
  every assertion: its counter is raised in a transaction of its own before the addition
  begins, and a counter refusal is audited as at sign-in and adds nothing. A person may
  remove any of their passkeys but the last; removing one ends every session opened with
  it, at that session's next use.
- **The host reset** is a host command. In one write, under the identity exclusion, it
  clears the person's passkeys and issues an enrollment token, and the command prints the
  token once. **It writes no session**: the session table is the surface's alone (Spec
  section 3), and every session opened with a cleared passkey ends at its next use by
  section 6's rule that a session ends where its passkey is removed, an open live view at
  its next 15-second re-check. The token goes to a row that by then holds no
  passkey, so Spec 2.13's rule that no token is issued for a row with a credential stands
  unchanged. It is audited as the host's.
- **No admin reset.** It would let an admin take a person over, issuing a token to a
  person and enrolling it themselves, and the threat model (section 5) promises against
  that.

## 8. The lone admin's grant

Spec 2.13 has no person write a grant on themselves, so on a server with one admin, that
admin can never hold `operator` on an agent.

**Decided: a host command writes any grant.**
- The host already writes the bootstrap admin grant, and it is not a person, so the
  self-grant rule does not reach it. No surface ever writes a grant on its own author.
- The alternative, an exception for the only enabled admin, would put a self-grant path in
  a surface, where a second admin's later disappearance could reopen it.

**The exact self-change rule** that Spec 2.13 left to this act: a person never writes a
grant whose grantee is themselves, granting or removing, and never writes a role that
they hold a grant of on any agent. Both are checked under the identity exclusion, as 2.13
already requires.

**The host writes roles too**, for the same reason. A lone admin who holds `operator` on
any agent, by the host's grant, can never edit the `operator` role, since they hold a
grant of it. The host command that writes grants therefore writes a role's verbs as well,
so a server with one admin can still change what its roles carry, and no surface edits a
role its author holds.

## 9. Read access

**The roles are verb sets, and reading is not a verb.** So the design says which grant
lets a person see an agent at all: its register row, its presence, its ceiling and door,
its load state, and its live trace window.

- **Any grant on an agent, `observer` or `operator`, permits reading everything the server
  holds of that agent**: the register's row, the presence, the ceiling and the door, the
  load state and the run it names, and the live trace window. `show` stays the verb that
  asks the agent afresh; reading what the server already holds asks nothing of the agent
  and needs no verb.
- **A person with no grant on an agent sees nothing of it**: not its name, not its
  presence. A list of agents shows the ones the person holds a grant on.
- **The admin sees the register for the connections it governs**: each agent's name, its
  presence (connected or not, per plane), and its credentials' state (issued, revoked,
  rotated). **The door and the load state are not in the admin's view, and neither is the
  trace**: those are the agent's, not the connection's, and an admin who wants them holds
  a grant on the agent like anyone else, written by another admin or by the host.
- **The check is the server's**, at the read, under the same rule as an ask: the person
  enabled, the session live, the grant standing. An open live view re-checks it every 15
  seconds (section 6).

## 10. The audit record, the exclusion, and what is out

**The passkey table** holds, per passkey: the person, the credential ID (unique across the
table, section 7), the library's serialized passkey (the public key, its algorithm and the
stored counter), a label the person gives, and when it was added and last used. Nothing in
it is secret; it is authentication material under Spec 2.13 all the same, written only by
its own person or by the host reset.

**The audit table** is `audit`, one row per record:
- its identity;
- when;
- **the principal**: the person, the server or the host;
- **how the principal was authenticated**: `session`, `enrollment token` or `passkey
  assertion` for a person,
  `server` for the server acting on its own behalf (the admission's `show`), and `host`
  for access to the server's host. Each value belongs to exactly one principal, so a
  record's principal and its method can never disagree. **`passkey assertion` marks the
  records of any assertion whose signature verified, whatever ceremony asked for it**,
  under section 6's every-assertion rule: a sign-in, audited as the session's opening,
  since a person's authority begins there and the record names the passkey it began
  with; the fresh assertion before a passkey's addition; and any assertion the counter
  refused, whose principal is the passkey's person because the signature proved them.
  **An assertion whose signature failed is not audited**: it proves no principal, and
  auditing it would let anyone fill the audit with requests;
- the host's `--author` claim where the host acts;
- the target's kind and identity;
- the action;
- for a second record, the first record it answers and the outcome; for a refusal, the
  refusal.

**It is append-only in the store**: a row trigger refuses every `UPDATE` and `DELETE` on
the table, and a statement trigger `BEFORE TRUNCATE` refuses a truncate, which no row
trigger sees, so this crate's processes can neither rewrite it nor empty it whatever path
they take. The table's owner can drop the triggers, which section 5 puts out of scope. The two-record
rule is Spec 2.13's, unchanged: a first record before the act, and the outcome as a second
record naming the first. **It never holds** a passkey, a public key, a challenge, a
bearer, an enrollment token or its digest, or a turn's text. It lands in PR 2, since the
host's writes are its first records.

**The identity exclusion** is one transaction-level advisory lock under a class of its own,
distinct from the ingest's two:
- identity writes take it exclusively (`pg_advisory_xact_lock`);
- authorized acts take it shared (`pg_advisory_xact_lock_shared`), from the check to the
  commit point Spec 2.13 names.

It is store-wide and coarse, as the brief asks. Identity writes are rare, and one lock
closes every race between them.

**Out of this act**:
- passwords and TOTP;
- external identity providers (OIDC);
- several servers sharing identity;
- per-person action lists;
- attestation verification and authenticator allow-lists (attestation `none` is accepted);
- self-registration (a person is enrolled only by token);
- mail or any notification;
- anything on an agent's box: its users, its sudo rules and its trace are WeaverAgent's.

## 11. Spec 10's elections

- **How people authenticate to this server** closes: passkeys only, per section 1.
- **The role vocabulary** closes: `observer` and `operator` per agent, the seeded verb sets
  of section 1, editable by the admin, and the server-wide `admin`. The converser the Spec
  had proposed is not among them. A role carrying `turn` without lifecycle verbs remains a
  role the admin may write, which needs no election.
- **What an author names** closes, per section 6.

## 12. The pull request plan

Each later pull request gets its own brief after the one before it merges. **The order is
the brief's, kept**, and each is one concern:

1. **This one**: the design.
2. **Persons, the bootstrap and enrollment tokens**: the migration (persons, passkeys,
   tokens, sessions' new columns, the audit table and its trigger); the host commands, which
   are bootstrap, issue a token, the host reset of section 7, and the host's grants and
   role writes of section 8; and their audit. No browser work. The host grant needs the grant table, so
   PR 2 carries the role and grant tables with the seeded roles, and PR 5 adds the admin's
   writes to them.
3. **TLS on the browser's listener**, with section 3's start refusals that concern the
   certificate.
4. **Passkey enrollment and sign-in**: the ceremonies, the vendored module, the session
   carrying the person, sign-out, the `Origin` check, the cap on ceremonies in flight,
   the remaining start refusals of section 3, and the `ldd` measurement of section 2. The claimed-name session retires
   here.
5. **Roles and grants**: the admin's writes, the identity exclusion, the last-admin and
   self-change rules, their audit.
6. **Server-side authorization**: every verb and turn checked against the person's grants
   and the connection's ceiling before a frame leaves, the shared hold, and the audit of
   every ask; and read access (section 9), every read of an agent checked against a grant
   on it and every open live view re-checked every 15 seconds.

**One change from the brief's plan**: the role and grant tables move into PR 2. The host
grant of section 8 is a host command, and that pull request carries the host commands. PR
5 keeps every write a person makes through a surface.
