# Brief: the console's side of the appliance (2026-11-15)

From the Planner, 2026-11-15, on the operator's rulings of 2026-11-14 and 2026-11-15. A
docs-only act from `main` at `6219f16c`, local commits only, no code, with no push until
the Planner gives the word.

## The rulings it rests on

The appliance is one deployment profile for a Weaver agent, beside the host install and
the Spark-class machine, on the operator's ruling of 2026-11-15. A profile is neither a
charter nor a Spec, decides nothing about any component and binds no Spec, and what it
asks lands in the charters. This act lands the console's side in `weaver-web-PRD` and
its Spec. The documents cite the operator's rulings by date and their own charter, and
cite neither the profile nor the image build as authority.

The rulings are the operator's of 2026-11-14 on the appliance and of 2026-11-15. Two
rulings of 2026-11-14 bear on the writing. The first, of about 20:15Z: a document never
says the agent has, owns or controls an operating system, kernel or firewall, since the
agent is bounded by the operating system under a UID. It names the component that holds
a privilege, and it states the deployment shape as a recommendation and never a rule.
The second: the pairing line carries the one-time secret on the line itself, the single
exception ("a one-time use password on the line after that none"), and Proxmox is out of
scope as of that ruling (`toddwbucy/WeaverTools` issue 29).

**The operator's answers of 2026-11-15, about 18:25Z**, relayed by the Planner, close
most of the act's open cells. The act lands them in one commit:
- The trace's destination is a field of the declaration page, set when the agent is
  created. A file is one destination and `/dev/null` another, and what stands behind the
  destination is the operator's. It amends the charter's "declared on neither screen"
  for the destination alone, and the Spec's sink filled at Provision and its file-sink
  ruling. Where the destination is not a file on the agent's machine, the console holds
  no trace for that agent and says so, and whatever keeps the trace past its destination
  is the operator's. The field is the next act's to build, not this act's.
- An agent that elects no state, with a trace path on a missing disk: a destination that
  cannot be opened refuses the load, admin's standing rule.
- State and save points on an appliance are later, since the first production agent
  keeps no state.
- The Spark-class machine is later.
- The run card is the operator's term for a JSON file, defined in the operator's paper,
  which is in no repository as of 2026-11-15.

Declared ports against the connectors' network stays an open cell for the operator, and
nothing waits on it while no declaration can name a port.

## The act

1. **The amendment to `weaver-web-PRD`**, a requirement and its reason for each.
   - Section 2's sentence that set production agents aside gives way to the operator's
     description of the console: management-plane software in the sense that Proxmox's
     console is, which never faces the public.
   - The stance, as the operator ruled it.
   - Pairing. The console begins pairing and the appliance makes the connection: the
     console listens, and the appliance dials it. The pairing line carries the console's
     address, a one-time secret and its certificate fingerprint, and the secret has
     three bounds: single-use, short-lived, and bound to one pending enrollment the
     operator opened. An appliance's enrollment connection is the link's one
     unauthenticated surface, named as one, and the browser's sign-in and
     person-enrollment pages are named apart as pages that answer before a session
     exists.
   - Keys are made inside the appliance, kept on its root disk and signed by the
     console, and then every connection is mutual TLS. A new image is a new enrollment.
     A second live holder of a certificate is refused, and a clone is a note in the Spec
     and nothing more.
   - The model store: the console fetches from four sources, pins each model by revision
     and digest and verifies it, keeps access tokens in its own store only, and the
     appliance pulls from the console's cache. The image download, with its hash check.
     The mounts, refused at the load where a required one is not a separate filesystem,
     the size report, and the console's "add storage" answer.
   - Load and unload, asked through admin-con, with nothing that autostarts the agent.
     An upgrade moves state and model, and the run card records the tuple as shipped. No
     hypervisor reach of any kind.
   - The trace's destination, a field of the declaration page, a null sink included,
     with its member an ask on WeaverAgent and the field itself the next act's. Ports a
     declaration opens are the operator's arrangement.
2. **The matching Spec sections**, to the level the rulings fix. Where a point is open
   (the secret's lifetime and form, the link port's default, which the operator
   suggested in the range 6000 to 7000, the cache layout), state it where a ruling or
   the built code fixes it, and otherwise carry it as an open cell with what settles it.
   Say plainly how an appliance's enrollment stands against registration as built on
   2026-11-15: what stays, what it replaces, what coexists.
3. **A short section naming what is not this crate's**: the environment inside an
   appliance (WeaverAgent's), the image build and its proof (WeaverTools), the evidence
   and measurements (WeaverTools issue 28).

**Withdrawn:** the claim that an appliance never reaches the internet.

**The term owed.** "Run card" is the operator's term, defined in a paper that is in no
repository as of 2026-11-15, and the act coins no definition. Its open cell closes when
the definition is placed.

**A conflict with a standing charter** goes back through the Planner to the operator,
listed and not resolved, and stands in the documents as an open cell naming the
operator.

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

The Executor reports the branch, the commits, the map from the rulings to this
repository's documents, every open cell left, and the check above. Push on the Planner's
word only.
