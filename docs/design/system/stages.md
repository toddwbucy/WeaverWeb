# The eight stages, pane by pane

Each row is one pane the grammar expects, with its stage mark, its backing today and where its facts come from. A pane with `unbuilt` backing is a drawing; a pane with `partly` names what is missing in its footer.

On the setup pages every pane also carries its status, healthy states included, on the operator's rulings of 2026-11-16 (setup pages).

## 1 Declare, partly

Declare is a start page, the setup pages and the build page, on the operator's rulings of 2026-11-16 (setup pages) and their addendum, which replace the two screens of the rulings of 2026-11-11 as the stage's path (Spec 6.1), and on the operator's rulings of 2026-11-23. They are built to the confirm page, each part live or held (Spec 6.2). The agent is named where its box is paired, on Pair a box (section 2). The declaration starts at directories, and the setup pages run in this order: the directories, the models, the placement and the confirm page. The declaration screen of the rulings of 2026-11-11, in the second table, is built and stays beside them, reached from the build page's stub, and the agent page's rail links this stage to the declaration saved under the agent's name, for an admin.

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Hugging Face account | the operator's account, connected once as a console setting before the start page, with a read-only or a read-write token, held sealed; the console never writes except on an explicit push in Train | the console's store (Spec 2.15) | built |
| SSH public keys | the operator's SSH public keys, a console setting, one or more, each shown by its SHA-256 fingerprint; the appliance's setup script fetches them over the paired link | the console's store (Spec 6.2) | built |
| Start | Declarations: the paired agents whose declaration is open, each a link to its directories | the store's open setups | built |
| Directories | the two directories as the operator laid them before pairing, a state disk and a models disk, each mounted at its default path, this server listing and formatting no disk; the state directory, under which each agent has a directory of its own holding its folder and its home side by side, and the models directory, as the box mounted them at the state mount and the models mount, from the latest report of either stage, a directory not mounted or a state directory whose filesystem is under 9 GiB holding Confirm and a models directory with less free space than the need warned of, no page judging the models disk's margin (the operator's ruling of 2026-11-24); the SSH keys that go down; on a host, which attaches and formats nothing, one row drawn and marked unbuilt, since the host's own script is later: the models directory, from this server's `model_cache`, or `not set`. Planned and not drawn: the state directory and the models directory the operator names, the state directory writable with at least 10 GiB free and the models directory readable, with no separate disk asked, the models directory prefilled with that user's default Hugging Face cache so that models already pulled there are found, and beside it a command for the operator granting this server's account read-write on that cache and a models group read, since this server alone downloads there and agents read the model in place, never copied | this server's store and the box's latest report of either stage | partly |
| Models | the models directory's need, the bytes of the picks' distinct blobs, judged against the models directory the box reports, free space short of it a warning only, the margin judged nowhere; a Hugging Face repository, then a pick from its files, a split model shown as one choice with all its parts, the precision read from the file; a gated model the account lacks shown with the link to accept it and a button to check again; the pulls' progress and record, and the base's identity | Hugging Face through the console, the pull record (Spec 2.15) and the box's latest report | built |
| Beyond the first agent | what the first agent does not need (extra models, adapter roles beyond the one decoder adapter, a classifier-only agent, placement on the CPU) drawn and marked unbuilt, never live | nothing | unbuilt |
| Placement | the context length, from the box's max-tokens-per-turn to the model's maximum read from the file's header; the turn context; the block count and the parts in shard order, from the header; each model on a device, by the ordinals of the cards the box reports, held until it reports them, and a kept card a later report lacks named; held until the room plan: the fit, called planned, from the room plan without a load, since only a load measures; "in shard order" naming the layer split across cards | the form, the header and the box's survey report; then the room plan | partly |
| Confirm | the deployment tuple in full: model repository, commit and per-part digests, the adapter likewise, the appliance's version (the survey's image members; the operator's ruling of 2026-11-24), driver, cards by model, each with its ordinal in PCI bus order, guest CPU, machine, drives, placement, context length, the box's members from its latest report of either stage; Confirm stands only once every member is read, and goes to the build page | the pages before it and the box's report | built |
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

Pair a box is Provision's page, where the agent is named and its box paired (the operator's rulings of 2026-11-23: "Pairing is its own page", and "the agent is named at pairing"). On an appliance the box's side is the appliance's setup script, which this server never runs (the operator's rulings of 2026-11-16, setup pages, and their addendum, and of 2026-11-23, `weaver-web-PRD` section 10): at the first console login a setup prompt's one command starts it, it walks the operator through finishing setup, the disks included, and it ends in pairing. The pairing belongs with the connectors and serves a host too, and laying the disks is the operator's, with the script. A host install points at a state directory and a models directory and attaches and formats nothing. One agent per virtual machine is the recommendation. The panes below stand as built, for pairing, registration and the steps on the box.

| pane | shows | source | backing |
| --- | --- | --- | --- |
| Staging | staging the binaries is the operator's work on the box, which this server never reaches, and the pane shows what is done and what remains | nothing yet | unbuilt |
| Kernel account | the agent's user and the connectors' service users; creating them is the operator's work on the box, and the pane shows what is done and what remains | nothing yet | unbuilt |
| Pair a box | Create new agent, which opens the requirements page, and the named agents whose box is not yet paired; an agent's pairing: what to do at the appliance, the setup prompt at its first console login naming the command; the console's link address and port, which the script asks for, and a warning where that address is a loopback address and the checkbox says virtual machine, and where it is an unspecified address; the checkbox saying whether the box is a virtual machine or a host, starting checked (the operator's ruling of 2026-11-24); where this server's build serves the enrollment, the box waiting, which dialed this server from the script, the fingerprint the box shows, compared row by row with this server's own in four rows of four groups (the operator's ruling of 2026-11-24), and the short one-time code the box shows, entered here; once paired, the box's survey report at the pairing stage, read only, its members grouped, or the cause it was refused, and the way on to the agent's directories (the operator's rulings of 2026-11-24) | the form and this server's config; then the box over its enrollment connection and the register | built |
| Requirements | Create new agent's page (the operator's ruling of 2026-11-24): an agent runs in a VM made from the appliance, this version covering VMs only; one agent per VM, the recommendation; the operator's part, the hypervisor, the cards, the disks and the network between VMs; the two disks, the 10 GB margin stated as guidance and never judged; that the VM must reach this server's address and port; the note on GB10 Spark-class devices; then the agent's name, one field, refused where admin's name rule or the box's agent-creation rule refuses it, the one place an agent is named. No CPU requirement and no host install | the operator's rulings and the form | built |
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
