# Signals emissions, vendored for the ingest's tests

The ingest of `weaver-web-Spec` section 3.1 reads one `weaver-analysis signals`
emission: one summary object on the first line, then one point per line, per
`weaver-analysis-web-contract` sections 2.1 and 2.2. The files here are what the tests
ingest. **No test runs the WeaverAnalysis binary**: the three emitter outputs were made
once and are kept as made, so a change on the emitter's side reaches this repository as
an act that replaces them, never as a test that drifts.

## Made by the emitter

Each was made on 2026-10-06 by WeaverAnalysis at commit
`12a72433bb0908ab4b7581f47a7fd3ff23ef1986` (`toddwbucy/WeaverAnalysis`, `main`), built
from that commit's tree, by running the `signals` command on the record of the same name
in that commit's `tests/fixtures/`, with no spike bar and no deposit, keeping its stdout:

```text
weaver-analysis signals tests/fixtures/diagnostic-certified.ndjson > diagnostic-certified.ndjson
weaver-analysis signals tests/fixtures/serving-source.ndjson       > serving-source.ndjson
weaver-analysis signals tests/fixtures/columns-a.ndjson            > columns-a.ndjson
```

| file | run | what it exercises |
| --- | --- | --- |
| `diagnostic-certified.ndjson` | one run, two generations, 455 points | entropy on every point, no surprisal, no digest and no prefix length (the drain did not see the run whole) |
| `serving-source.ndjson` | one run, the same two generations and points | the digest and the seated prefix's length present |
| `columns-a.ndjson` | one run, two generations, 6 points | surprisal on every point and no resident count on either generation, so no point can be addressed; it names no effective sampling either, so as it stands the ingest refuses it before a row, and the test of a `short` run gives it one |

No emitter output here carries a lineage, a verdict, a deposit's device model or code
identity, or a second run.

## Made by hand

`hand-made-branch.ndjson` is **hand-made and is not an emitter's output.** It is
`diagnostic-certified.ndjson` with two changes to its summary line, made by a script on
2026-10-06, its points untouched:

- every generation's `run` is `2026-09-01T02:00:00.000Z-karl-b7a9c3d2e1f00a11`, a run
  identity no record carries, so it lands beside the certified run rather than replaying
  it;
- every generation carries a `lineage` in the shape `weaver-trace-Spec` section 3 gives
  the `load` event's member after `toddwbucy/WeaverAgent#58`: `save_point`, `run`,
  `sequence`, `turn`, `operator_supplied`, and `built_from` with `parent`, `run` and
  `through`. Its `built_from.run` names the run of `serving-source.ndjson`, so the branch's
  parent is held where that emission has landed first.

It exists because no record in WeaverAnalysis's fixtures carries a lineage, and the
ingest's parent reference, its link and its walk need one to be exercised. Its numbers
(the save point, the sequence, the turn and `through`) are sample values and describe no
real save point. Tests that need other shapes of branch (a cycle, a chain, a shorter
child) build them from these files in the test itself and say so where they do.
