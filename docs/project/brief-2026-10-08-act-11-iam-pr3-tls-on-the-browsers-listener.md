# Brief: act 11, IAM, PR 3: TLS on the browser's listener

Version: v0.1, 2026-10-08. From the planning seat (`Thinkpad-WeaverWeb-Planner`) for the
executor. PR 2b merged as #26 at `f1fe09d`. This PR makes the browser's listener serve TLS
under a name, so PR 4's passkeys are built and tested the way they will run (design
section 3: WebAuthn needs a secure context, and a relying party is a name, never an
address).

**The edge between this PR and PR 4.** The design's certificate checks need the origin (a
certificate must be valid for the origin's host), so this PR takes the `origin` setting
together with the certificate: it is the listener's name on the web. PR 4 adds `rp_id`, its
two refusals (absent or an IP address; the origin's host neither the rp_id nor under it)
and everything passkey.

## 1. Read first

- The design's section 3, the whole list of start refusals and the sentence after it ("any
  other fault of the configuration is not refused at start"). **The list is the promise**:
  implement the items below and nothing beyond them.
- `src/bin/weaver-web.rs`'s `serve` (the browser's listener is plain TCP into
  `axum::serve` today) and `src/config.rs`'s `ServerConfig`.
- The link listener (`src/link/listener.rs`), which already serves rustls; reuse its loading
  of PEM material where it fits.

## 2. Scope

1. **Config**: `tls_certificate` and `tls_key` (paths, read at start as the authority is),
   and `origin`. All three optional in this PR. No value, path or name enters the
   repository; tests mint their own certificates at test time (rcgen is in the tree).
2. **Serving**: with a certificate and key configured, the browser's listener serves TLS
   only (no plain listener beside it). Without them, it serves plain HTTP as today, which
   the `http://localhost` origin and the tests use. Say which way you serve axum over
   rustls (an `axum::serve::Listener` over `tokio-rustls`, or another), measured against
   adding a crate; the planner's lean is no new crate.
3. **The start refusals this PR owns**, from design section 3, each naming the key and what
   is wrong, before anything listens:
   - an `origin` that is not a serialized origin (scheme, host and port only; no userinfo,
     path, trailing slash, query or fragment; scheme and host in lower case; no default
     port written);
   - an `origin` whose scheme is not `https`, unless it is exactly `http` on the host
     `localhost`;
   - an `https` origin with no certificate and key configured;
   - a certificate and key that do not load or do not pair;
   - a certificate not valid for the origin's host, by webpki's name check (as rustls
     verifies a server name);
   - a certificate not yet valid or already expired against the clock at start; and the
     one warning line where it expires within 14 days.
   A certificate and key with no `origin` configured: serve TLS and check the pair and its
   validity period; the name check waits for an origin. Say so.
4. **Out of this PR**: `rp_id` and its refusals, the `Origin` header check on requests,
   the cookie's `Secure` and `__Host-` attributes, the ceremonies (all PR 4); HSTS and a
   plain-to-TLS redirect (not in the design; name them out).

## 3. What to measure

- How the certificate's validity period is read (rustls-webpki does not expose it; rcgen
  writes but does not parse): a parser already in the tree, or the smallest one to add.
  Quote the weight.
- The axum-over-rustls serving path, as in 2.2.

## 4. Tests and perturbations

Shown failing with the guard removed, in the scratch worktree of the pushed commit, each
start refusal its own case with a certificate minted at test time:
- each malformed origin form (userinfo, path, trailing slash, query, fragment, upper case,
  default port written) refused;
- a non-https origin other than `http://localhost` refused; `http://localhost` accepted;
- an https origin with no certificate refused;
- a key that does not pair with its certificate refused;
- a certificate for another name refused against the origin;
- an expired certificate and a not-yet-valid one refused; one expiring within 14 days
  starts with the warning;
- a TLS client completes a handshake against the listener and gets a surface's answer, and
  a plain HTTP request to a TLS listener gets no surface.

`tests/startup.rs` (ignored, the real binary) gains a TLS start: run it with `--ignored`
before the push and say so.

## 5. Spec and documents

- Section 9: the start-refusal row is enforced for the items this PR owns, and narrowed to
  the rp_id items as owed to PR 4; update the owed count and summary, grep for any other
  count that moves.
- CLAUDE.md: the server's config gains `tls_certificate`, `tls_key` and `origin`.

## 6. Acceptance

- `cargo build --locked`, `cargo test --locked` with `DATABASE_URL` set (the full suite
  before each push), `cargo clippy --all-targets --locked -- -D warnings`, `cargo fmt
  --check`, `tests/startup.rs --ignored`; perturbations in the scratch worktree.
- ASCII only, absolute dates. No certificate, key, host name or path in the repository;
  test certificates are minted at test time and never written outside a temporary
  directory. Check visibility before the push.
- PR body: `Implements:` naming design section 3 and the Spec sections, the perturbation
  table, the two measurements, and "Refs #18".
