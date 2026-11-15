# Brief: the console's side of the appliance (2026-11-15)

From the Planner, 2026-11-15, on the operator's ruling of that date. A docs-only act
from `main` at `6219f16c`, local commits only, no code, with no push until the Planner
gives the word.

## The ruling it rests on

The appliance charter the Planner drafted on 2026-11-14 (`toddwbucy/WeaverTools` pull
request 30, head `c23f503`, `docs/weaver-appliance-PRD.md`) is recast. The environment
inside an appliance becomes a Spec under `weaver-admin-PRD` in `toddwbucy/WeaverAgent`,
which the WeaverAgent seat writes. The console's side is split out for `weaver-web-PRD`
and its Spec, and that split is this act.

The source is that draft's sections 3.6, 5, 6, 8 and 9 and the WeaverWeb rows of its
section 10. Every requirement in it is the operator's ruling of 2026-11-14 unless it
says proposal. The act adds no ruling the source does not carry. Two later rulings of
the operator bear on the writing. The first, of about 20:15Z on 2026-11-14: a document
never says the agent has, owns or controls an operating system, kernel or firewall,
since the agent is bounded by the operating system under a UID. It names the component
that holds a privilege, and it states the deployment shape as a recommendation and never
a rule. The second, of the same day: the pairing line carries the one-time secret on
the line itself, the single exception ("a one-time use password on the line after that
none"), and Proxmox is out of scope as of that ruling (`toddwbucy/WeaverTools` issue
29).

## The act

1. **The amendment to `weaver-web-PRD`**, a requirement and its reason for each. The
   console begins pairing and the appliance makes the connection: the console listens,
   and the appliance only dials out. The pairing line the console shows carries its
   address, a one-time secret and its certificate fingerprint, and the secret has three
   bounds: single-use, short-lived, and bound to one pending enrollment the operator
   opened. An appliance's enrollment is the console's one unauthenticated surface, named
   as one. Keys are made inside the appliance and signed by the console, then every
   connection is mutual TLS. A second live holder of a certificate is refused. The model
   store: the console fetches from four sources, pins each model by revision and digest
   and verifies it, keeps access tokens in its own store only, and the appliance pulls
   from the console's cache. The image download, with its hash check. The storage
   report, and the console's "add storage" answer. Load and unload, asked through
   admin-con. No hypervisor reach of any kind.
2. **The matching Spec sections**, to the level the rulings fix. Where the source says
   "open for spec" (the secret's lifetime and form, the link port's default, which the
   operator suggested in the range 6000 to 7000, the cache layout, what happens to the
   appliance that connected second), state it where a ruling or the built code fixes it,
   and otherwise carry it as an open cell with what settles it. Say plainly how an
   appliance's enrollment stands against registration as built on 2026-11-15: what
   stays, what it replaces, what coexists.
3. **A short section naming what is held elsewhere**: the environment (the WeaverAgent
   Spec), the image build and its proof (WeaverTools), the evidence and measurements
   (WeaverTools issue 28).

**Also checked and reported, not fixed in this act:** whether any text left in the tree
still states the withdrawn direction (a host-or-VM field, a template an agent is cloned
from, the console touching a hypervisor).

## The rules

The charter's and the Spec's own format, the Working Rules' editorial rules, the
phrasing rule above, and the publish boundary: no box path, host name, hub path, mount
or account enters the repository. The posture is stated at the level of design, since
this is infrastructure that never faces the public, and no rule of a box's firewall or
hardening enters, which the hard boundaries keep out.

## Acceptance

The Executor reports the branch, the commits, the map from the source's sections to this
repository's documents, every open cell left, and the check above. Push on the Planner's
word only.
