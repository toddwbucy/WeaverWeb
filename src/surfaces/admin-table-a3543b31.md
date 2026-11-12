| State | `load` | `unload` | `force-unload` | `save-point` | `restore` | `show` |
|---|---|---|---|---|---|---|
| S0, down, clean | the load's answer, or its refusal | `Unloaded`, nothing to do | `Unloaded`, nothing to do | `OutOfOrder` | `RestoreNamed` | `Unloaded` |
| S0d, down, a run ended without closing | as S0, the load recording the reset, or `AgentRunning` naming them where processes of the ended run are still standing (`weaver-admin-Spec` section 6.1) | ends any process of the ended run still standing, then publishes the save points the run left as recovered, then `Unloaded`, or `SavePointNotTaken` naming `published` where that fails. The marker is left as it stands, and the next load records the cause it carries (`NoCleanUnload` where the run was left open, `ForcedUnload` where a force closed it) | ends them and publishes the same way, then `Unloaded`, or refuses as `unload` does. The marker is left as it stands, a force on an agent already down having ended no run, and the next load records the cause it carries | `OutOfOrder` | `RestoreNamed` | `Unloaded`, or `InTransition` naming the processes of the ended run still standing |
| S1, loading | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight`, a load being no unload to join | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S2, serving at rest | `AgentRunning` | the graceful unload, below | the forced unload, below | `SavePointTaken` | `RestoreNamed` | `Idle` |
| S3, serving, a turn in flight | `AgentRunning` | the graceful unload, after the turn | the forced unload, the turn cancelled | `ActivityNotAtRest` | `RestoreNamed` | `Active` |
| S4 to S7, a graceful unload in progress | `InvocationInFlight` | `InvocationInFlight` | joins, below | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S8a, an unload concluding | `InvocationInFlight` | `InvocationInFlight` | waits for the unload to finish, then answers as `show` would: `Unloaded` once nothing of the agent stands | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S8b, the worker gone with no answer (it died), a process still holding the run | `AgentRunning` until the run ends, then as S0d | ends what still holds the run, then publishes what the run left as recovered. The next load records `NoCleanUnload` | the same as `unload`, the marker left as it stands | `OutOfOrder` | `RestoreNamed` | `InTransition` with the run's processes named while a process holds the run, and after that while any process of the agent stands, naming those. `Unloaded` only once none does |
| S9, an unload stopped at its save point | `AgentRunning` | retried: the save point, then down | the forced unload | `SavePointTaken` | `RestoreNamed` | `Idle`, the gate lowered |
| S10, a force in progress | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |
| S11, a silent worker being ended | `InvocationInFlight` | `InvocationInFlight` | `InvocationInFlight` where a force is ending it; beside an `unload` ending it, joins, below, and ends it beside the unload, the second ending changing nothing | `InvocationInFlight` | `InvocationInFlight` | `InTransition` |

**The table is for one command at a time** (`weaver-agent-PRD` section 6.1, the
operating envelope): a caller issues one command and waits for its answer, the one
admitted overlap being `force-unload` joining an `unload`. **A command killed
mid-command is outside the envelope** and gets no takeover: the run finishes or dies on
its own, and a later command finding the agent's lock free mid-transition answers
conservatively (`weaver-admin-Spec` section 3, outside the envelope): `load` refuses
`AgentRunning`; `unload` refuses `OutOfOrder` where a leave is already in progress, or
ends a run whose agent no longer listens; `force-unload` ends whatever stands, marking
it forced where it ended a live agent mid-unload, and leaving the marker as it stands
where the agent had already died (S8b), the crash and not the force having ended the run; `save-point` refuses `OutOfOrder`. The next load recovers any save point
left in the room, and its reset may be conservative but is never clean where the state
was not kept.

`restore` names the save point the next load restores in every state that answers it,
the live restore being A5's. `stop` and `validate` take the invocation lock as every
line but `show` does, so each refuses `InvocationInFlight` wherever the table shows an
invocation holding it (S1, S4 to S7, S8a, S10, S11); a `stop` in S9 answers `AtRest`,
and in S8b, where no invocation holds it, both take the free lock: `stop`, no worker
listening, refuses `Unanswered` once the dial's bound passes, and `validate` answers as
it does in any state, starting nothing. The
lock is taken before the state is read, so a line that meets a held lock refuses
`InvocationInFlight` whatever the state behind it. `show` alone reads beside a holder, and
what it prints is one ladder (`weaver-admin-Spec` section 3): `InTransition` at once
where another command holds the agent; otherwise the agent's own word where it answers
(`Idle` or `Active`, or `InTransition` while an unload is in progress); `Unanswered`
where it accepts and does not answer in time, a worker wedged; `InTransition` with the
run's processes named where no agent listens and a process still holds the run, a run
that never started or one ending. It prints `Unloaded` only once nothing holds the run
and no process of the agent stands. **Every `Unloaded` this section names is printed only once
nothing of the agent stands** (`weaver-admin-Spec` sections 3 and 6.1): where its
processes do not all end within the containment bound, the command refuses
`WorkerWouldNotExit` in its place and publishes nothing (section 5).
