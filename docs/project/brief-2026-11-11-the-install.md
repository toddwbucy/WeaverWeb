# Brief: units and a staging tool for the server and the two connectors

Sent by the suite Planner on 2026-11-11. Stack `act/the-install`, from `main` at c8b9c0101e449827d1ba9ed81b005d78ca8f4a86.

## What was measured

- The server and both connectors have only ever run as units typed by hand at a shell. Their properties (the umask, admin-con's kill mode and stop timeout, the account each runs under) are written down nowhere in this repository, and nothing survives a restart of the machine.
- Spec section 8 says the agent's processes land inside admin-con's service, and section 2.12's constituents member names the install's one-time check that rests on it. Since toddwbucy/WeaverAgent#166 the merged contract puts every constituent in the agent's own scope and constrains what the invoker's context may restrict. The Spec has drifted from the contract.
- Issue #46 asks for admin-con's unit shape.

## The goal

A reader of this repository can produce correct units for the server, gate-con and admin-con, and the files to place beside them, without reading a shell history.

## Ruled out, and why

- **No location, account, group or socket is written into the repository.** The hard boundaries stand as they are. Every one is a named parameter with no default, read from a file the operator keeps outside the repository.
- **No privileged program.** The repository's one privileged invocation stays admin-con's. Nothing in this act runs as root or names a privilege-raising command, and `tests/no_privilege.rs` passes unchanged.
- **No account or rule is created.** Those are WeaverAgent's.
- **No migration squash.** It needs the operator's ruling, since a deployment now holds data.
- **Spec 8's sentence on accounts per agent** is not edited: the text it would follow is not merged in WeaverAgent yet.

## Sub-acts, in order

1. **The Spec.** A section for the install: the three units and each property with its reason; the parameters and what each must be; the per-agent config layout relative to a config root; what the unit for admin-con must not restrict, citing the contract; the one-time check after a first load; the refusals. Section 8 brought into line with the merged contract, and section 2.12's constituents member with it. Section 9's rows.
2. **The files and their tests.** Unit templates, one per program, the connectors' instanced by agent. A staging tool that runs unprivileged: it reads the parameter file, renders the units, and writes them with a manifest (source, destination, owner, group, mode for each file and directory) into a directory the operator names. Tests: a render with invented parameters into a temporary directory, the units checked with the system's own verifier where it runs unprivileged, the manifest checked line by line, and each refusal (a missing parameter, a relative path, a name that is not a valid instance).

## Acceptance

- Each Spec section named under `Implements:` has a verdict, and each behaviour claimed has a test.
- The rendered admin-con unit sets nothing the contract forbids; a test reads the rendered text for it.
- fmt, build, clippy with and without default features, and the suite with the database variables and eight test threads: each reported with its exit status and counts, nothing skipped.
- The body says in a table what the act leaves undone and what each item waits on.

## For the operator's ruling, not decided here

1. Whether this repository's own install locations may be written as overridable defaults.
2. Whether a script the operator runs as root to apply the staged files, which the product never runs and nothing from the network reaches, stands outside the one-privileged-invocation rule.
3. The restart policy of each connector, and what a restarted admin-con owes a verb that was in flight.
4. The migration squash.
