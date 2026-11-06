# The seven stages, pane by pane

Each row is one pane the grammar expects, with its stage mark, its backing today and where its facts come from. A pane with `unbuilt` backing is a drawing; a pane with `partly` names what is missing in its footer.

## 1 Declare, unbuilt

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Diagram | the agent's edge, its two sockets, the two connectors outside as fixed nodes, the organs inside with their tunables on them, the wires | a view of the saved `agent.yaml`; every reachable state is a valid declaration | unbuilt |
| Pieces | the menu of pieces that exist today: decoder (mandatory), state (SQLite, in memory), classifier (optional, pulled in by state), LoRA adapter | the organ kinds the framework charters; a kind is a charter act before it is a row here. The connectors are not pieces: they are minted at Provision | unbuilt |
| Configure | one piece's settings in a pop-up: system prompt, code, tunables; per field, caught now or caught at load | the piece's contract | unbuilt |
| agent.yaml | the declaration as a saved document: the same state the diagram shows, with its version and last save | the saved file | unbuilt |
| Refusals | what the form refuses on its own, and nothing more: a missing required field, a piece with no configuration; each names the field | the form | unbuilt |

## 2 Provision, partly

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Host | localhost or a remote host; staging the binaries is the operator's work on the box, which this server never reaches, and the pane shows what is done and what remains | nothing yet | unbuilt |
| Kernel account | the agent's user and the connectors' service users; creating them is the operator's work on the box, and the pane shows what is done and what remains | nothing yet | unbuilt |
| Register | box and name, the row minted with two credentials | `POST /admin/agents/register` | built |
| Handover | the two client configs, offered once, five minutes, this session only | `agents::Handover` | built |
| Declaration | the operator installs the file Declare saved; validate reads the installed declaration, never a draft (Spec 7.2, WeaverAgent #59) | nothing yet | unbuilt |
| Validate | closes Provision and gates Run: asks admin whether every element the installed declaration names that admin holds custody of is on the box; a refusal names the field; a clean answer is acceptance for filing: nothing admin holds custody of stands in the way of a load, and what only the organ can judge is answered at admission; it is not a promise the load succeeds | admin through admin-con | unbuilt as a step; the verb itself is built on the agent page |

## 3 Run, built

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Load state | the agent's last word on its state, with its source (`show` or `event`) and date, and a transition standing beside it | the register row | built |
| Verbs | one form per verb the person's roles carry and the ceiling grants; the rest disabled with why | `POST /agents/{ag}/verb` | built; `load` refused upstream for now |
| Turn | the turn form; a note when more than one person holds `turn` | `POST /agents/{ag}/turn` through gate-con | built |
| Answer | admin's JSON verbatim with its exit status, or the typed reason it was not run | the verb's outcome | built |

## 4 Observe, built

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Availability | a plane the server cannot reach, as a banner on every signed-in page | `Store::unavailable_for`, every 15 s | built |
| Connectors | gate-con and admin-con, each with its status and last heard | the listener's memory, every 15 s | built |
| Alerts | every alert on the agent: kind, subject, from, why, when, occurrences; acknowledge by an `operator` holder | `store::alerts` | built |
| Live trace | the listener's window as an SSE stream: trace events and marks, replayed or live | `GET /agents/{ag}/trace` | built |
| Decision | a `<dialog open>` where `unload` refused `save_point_not_taken` with the run at rest | the alert's prompt | built |

## 5 Preserve, partly

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Save points | `save-point` as a verb; the manifest line read back | admin through admin-con; offered where the role carries it and the ceiling grants it | partly: the verb is offered, no list of save points is served |
| Restore | `restore` as a verb | admin through admin-con; offered where the role carries it and the ceiling grants it | partly: the verb is offered; it carries no argument, so the page cannot name a save point |

## 6 Diagnose, unbuilt

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Diagnostic binding | reload under a diagnostic binding with the gate shut | admin | unbuilt |
| Analysis | the per-position series and the per-generation summary fed into the trace | weaver-analysis into the store, `weaver-analysis-web-contract` | unbuilt |
| Stepped replay | the trace replayed one position at a time under the diagnostic binding, the gate shut, each step's series beside it | the landed record and the diagnostic binding | unbuilt |
| Branches | counterfactual branches of a position, side by side | the store's reads | unbuilt |

## 7 Retire, partly

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Planes | revoke gate or admin; the live connection closes in the act | `POST /admin/agents/revoke` | built |
| Retire | retire the row; never live again; its grants stand until revoked | `POST /admin/agents/retire` | built |
| Archive | the trace and the state store preserved as a replay | nothing yet | unbuilt |
