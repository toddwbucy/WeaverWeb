# The eight stages, pane by pane

Each row is one pane the grammar expects, with its stage mark, its backing today and where its facts come from. A pane with `unbuilt` backing is a drawing; a pane with `partly` names what is missing in its footer.

Every pane also carries its status, healthy states included, on the operator's rulings of 2026-11-16 (setup pages).

## 1 Declare, partly

Declare is a start page, the setup pages and the build page, on the operator's rulings of 2026-11-16 (setup pages) and their addendum, which replace the two screens of the rulings of 2026-11-11 as the stage's path (Spec 6.1). None of them is built, and the act that builds them ends at the confirm page. The setup pages run in this order: the pairing, the models, the drives, the placement and the confirm page. The declaration screen of the rulings of 2026-11-11, in the second table, is built and stands until that act recuts Declare, and the agent page's rail links this stage to the declaration saved under the agent's name, for an admin.

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Hugging Face account | the operator's account, connected once as a console setting before the start page, with a read-only or a read-write token; the console never writes except on an explicit push in Train | the console's store | unbuilt |
| Start | the agent's name, one field | the form | unbuilt |
| Pairing | the box waiting, which dialed this server from its setup; the fingerprint the box shows, compared with this server's own, and the short one-time code the box shows, entered here; the running report of the box's survey; a checkbox saying whether the box is a virtual machine or a host, and a warning where the address is localhost and the checkbox says virtual machine | the box over its enrollment connection, then the connectors | unbuilt |
| Models | a Hugging Face repository, then a pick from its files, a split model shown as one choice with all its parts, the precision read from the file; a gated model the account lacks shown with the link to accept it and a button to check again; what the first agent does not need (extra models, adapter roles beyond the one decoder adapter, a classifier-only agent, placement on the CPU) drawn and marked unbuilt, never live | Hugging Face through the console, and the pull record (Spec 8.3) | unbuilt |
| Drives | on an appliance, the command to run at the appliance's console that starts its drive setup, then the state drive, at least 10 GiB, which holds the agent account's home, a folder named after the agent, and in it the agent's own folder, and the models drive, sized from the chosen files, as the box mounted them at the state mount and the models mount; on a host, the state directory and the models directory the operator named, the state directory writable with at least 10 GiB free and the models directory readable, with no separate disk asked, the models directory prefilled with that user's default Hugging Face cache so that models already pulled there are found, and beside it a command for the operator granting this server's account read-write on that cache and a models group read, since this server alone downloads there and agents read the model in place, never copied, the host attaching and formatting nothing, drawn and marked unbuilt since the host's own script is later | the box's report over TLS | unbuilt |
| Placement | each model on a device, the context length chosen against the model's maximum read from the file's header; the fit called planned, from the room plan without a load, since only a load measures; "in shard order" naming the layer split across cards | the room plan | unbuilt |
| Confirm | the deployment tuple in full: model repository, commit and per-part digests, the adapter likewise, image version, driver, cards by identity, guest CPU, machine, drives, placement, context length; Confirm goes to the build page | the pages before it | unbuilt |
| Build | a one-pane stub marked unbuilt, where the board and the prompt tuple panel will stand, both held | nothing yet | unbuilt |

The declaration screen of the rulings of 2026-11-11, as built:

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Agent | the name, which names the lineage and never changes within it | the saved declaration | built |
| SPU box | one per box: its models, each with its function from the roster the framework charters (decoder, exactly one; classifier, optional), artifact, format (GGUF offered, the rest named as later) and a decoder's adapter; the box's GPU requirement, a card, a class (a family or a list of models) or a pair, which start satisfies or refuses, never substituting | the saved declaration; the compute-capability table for the warning | built |
| Sampler | the eight tunable values a serving load requires, each required: the six sampler defaults every generation carries and the session's two parameters; what else is fixed at load read-only once an agent reports it, and until then what that waits on | the saved declaration | partly: nothing else fixed at load is shown, since no agent reports it yet |
| Store and gate | the store (`none` or `sqlite`) and the gate's tool election | the saved declaration | built |
| Prompt | the system prompt, its own field, written into declared state by the declaration act | the saved declaration | built |
| agent.toml | the file the declaration becomes, derived at the pinned corpus commit and never edited; what the screen does not declare as comments naming what fills it, what the grammar has no place for as a trailing block | derived from the saved declaration | built |
| Version | the row's version, last save and author, and whether it is its lineage's head | the store (Spec 2.4) | built |
| Refusals and warning | what the form refuses on its own (a missing required field, a requirement that names nothing, a field out of its shape), each naming the field; the warning where a class spans more than one compute capability; per field, what it waits on | the form | built |
| Declaration, on the agent page | the declaration saved under the agent's name, the head of that lineage: its version, last save and author; its parts, each the agent reports set beside the agent's report (equal, differs, or not in this report) and each it never reports marked so; the prompt by its length, its digest to an admin alone; the other live register rows of the name; the declaration screen's link for an admin | the saved declaration; the register row's tuple and its source | partly: which saved version the agent's file was derived from is not known until Provision writes the file and records its digest |

## 2 Provision, partly

The pairing and the drives are setup pages (section 1), and on an appliance the box's side is the appliance's setup script, which this server never runs (the operator's rulings of 2026-11-16, setup pages, and their addendum, `weaver-web-PRD` section 10). The pairing belongs with the connectors and serves a host too, and the drive setup is the appliance's script alone. A host install points at a state directory and a models directory and attaches and formats nothing. The panes below stand as built, for registration and the steps on the box.

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Staging | staging the binaries is the operator's work on the box, which this server never reaches, and the pane shows what is done and what remains | nothing yet | unbuilt |
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
