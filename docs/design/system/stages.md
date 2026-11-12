# The eight stages, pane by pane

Each row is one pane the grammar expects, with its stage mark, its backing today and where its facts come from. A pane with `unbuilt` backing is a drawing; a pane with `partly` names what is missing in its footer.

## 1 Declare, unbuilt

Declare is two screens (operator's rulings of 2026-11-11, Spec 6.1): the declaration screen, below, and the board, which wires what was declared and is unbuilt with no panes drawn yet.

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Agent | the name, and host agent or virtual-machine agent with the template a virtual-machine agent is cloned from by the console itself; the template's digest and the machine's, once recorded; no Hosts page, and no virtual machine managed as an object of its own | the saved declaration | unbuilt |
| SPU box | one per box: its models, each with its function from the roster the framework charters (decoder, exactly one; classifier, optional), artifact, format (GGUF offered, the rest named as later) and a decoder's adapter; the box's GPU requirement, a card, a class (a family or a list of models) or a pair, which start satisfies or refuses, never substituting | the saved declaration; the compute-capability table for the warning | unbuilt |
| Sampler | the defaults for the per-turn knobs; what is fixed at load read-only once an agent reports it, and until then what that waits on | the saved declaration | unbuilt |
| Store and gate | the store (`none` or `sqlite`) and the gate's tool election | the saved declaration | unbuilt |
| Prompt | the system prompt, its own field, written into declared state by the declaration act | the saved declaration | unbuilt |
| agent.toml | the file the declaration becomes, derived at the pinned corpus commit and never edited; what the screen does not declare as comments naming what fills it, what the grammar has no place for as a trailing block | derived from the saved declaration | unbuilt |
| Version | the row's version, last save and author, and whether it is its lineage's head | the store (Spec 2.4) | unbuilt |
| Refusals and warning | what the form refuses on its own (a missing required field, a requirement that names nothing, a field out of its shape), each naming the field; the warning where a class spans more than one compute capability; per field, what it waits on | the form | unbuilt |

## 2 Provision, partly

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Staging | staging the binaries is the operator's work on the box, which this server never reaches, and the pane shows what is done and what remains; host agent or virtual-machine agent is the declaration's field, not a pane here, and there is no Hosts page | nothing yet | unbuilt |
| Kernel account | the agent's user and the connectors' service users; creating them is the operator's work on the box, and the pane shows what is done and what remains | nothing yet | unbuilt |
| Register | box and name, the row minted with two credentials | `POST /admin/agents/register` | built |
| Handover | the two client configs, offered once, five minutes, this session only | `agents::Handover` | built |
| Declaration | the operator installs the file derived from the declaration Declare saved, since no console verb installs one yet; validate reads the installed declaration, never a draft (Spec 7.2, WeaverAgent #59) | nothing yet | unbuilt |
| Validate | closes Provision and gates Run: asks admin whether every element the installed declaration names that admin holds custody of is on the box; a refusal names the field; a clean answer is acceptance for filing: nothing admin holds custody of stands in the way of a load, and what only the organ can judge is answered at admission; it is not a promise the load succeeds | admin through admin-con | unbuilt as a step; the verb itself is built on the agent page |

## 3 Run, built

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Load state | the agent's last word on its state, with its source (`show` or `event`) and date, and a transition standing beside it | the register row | built |
| Verbs | one form per verb the person's roles carry and the ceiling grants; the rest disabled with why | `POST /agents/{ag}/verb` | built |
| Turn | the turn form; a note when more than one person holds `turn` | `POST /agents/{ag}/turn` through gate-con | built |
| Answer | admin's JSON verbatim with its exit status, or the typed reason it was not run | the verb's outcome | built |

## 4 Observe, built

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Availability | a plane the server cannot reach, as a banner on every signed-in page | `Store::unavailable_for`, every 15 s | built |
| Connectors | gate-con and admin-con, each with its status and last heard | the listener's memory, every 15 s | built |
| Alerts | every alert on the agent: kind, subject, from, why, when, occurrences; acknowledge by an `operator` holder | `store::alerts` | built |
| Live trace | the listener's window as an SSE stream: trace events and marks, each carrying `replayed`, the listener's verdict against the opening's boundary | `GET /agents/{ag}/trace` | built |
| Decision | a `<dialog open>` where `unload` refused `save_point_not_taken` with the run at rest | the alert's prompt | built |

## 5 Preserve, partly

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Save points | `save-point` as a verb; the manifest line read back | admin through admin-con; offered where the role carries it and the ceiling grants it | partly: the verb is offered, no list of save points is served |
| Restore | `restore` as a verb | admin through admin-con; offered where the role carries it and the ceiling grants it | partly: the verb is offered; it carries no argument (the invoker's fixed line `sudo -n <weaver_admin> <verb> <agent>`, Spec 7.2), so the page cannot name a save point |

## 6 Diagnose, unbuilt

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Diagnostic binding | reload under a diagnostic binding with the gate shut (WeaverAgent's `diagnostic-replay-loop` section 1: under a diagnostic binding there is no gate; `weaver-diagnostic-PRD` section 3) | admin | unbuilt |
| Analysis | the per-position series and the per-generation summary fed into the trace | weaver-analysis into the store, `weaver-analysis-web-contract` | unbuilt |
| Stepped replay | the trace replayed one position at a time under the diagnostic binding, the gate shut (as above), each step's series beside it | the landed record and the diagnostic binding | unbuilt |
| Branches | counterfactual branches of a position, side by side | the store's reads | unbuilt |

## 7 Train, unbuilt

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Select | the turns and positions selected from the trace, the record of the whole lifecycle and no stage's own; gathering is part of this stage | the landed record | unbuilt |
| Dataset | the dataset built from the selection; no frontier-model or reviewer text enters it | the selection | unbuilt |
| Train | the adapter's training run and its result | WeaverTrain (`toddwbucy/WeaverTrain`), through a contract that does not exist yet | unbuilt |
| Hand back | the trained adapter offered to Declare; it enters through Declare and Provision at the load boundary | nothing yet | unbuilt |

## 8 Retire, partly

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Planes | revoke gate or admin; the live connection closes in the act | `POST /admin/agents/revoke` | built |
| Retire | retire the row; never live again; its grants stand until revoked | `POST /admin/agents/retire` | built |
| Archive | the trace and the state store preserved as a replay | nothing yet | unbuilt |
