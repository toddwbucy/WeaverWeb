# What an agent reports of its load, as read at WeaverAgent `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`

Held by `src/declaration/reported.rs` (WeaverWeb Spec 2.13, the agent page's Declare pane). Each passage below is the named lines of the named file at that commit, byte for byte, inside its fence. A reread at a new pin replaces this file with one named for that commit, and the diff between the two is the reread.

## `crates/weaver-types/src/wire.rs`, lines 486 to 519

`show`'s facts of a standing run: the member `load` of a `state` answer, which the listener writes as the register row's tuple with the source `show`. Its members keep their field names, since the struct names no rename.

```rust
/// **What a standing run was built from**, per `weaver-types-Spec` section
/// 4.2, answered to the observation exchange from the run and never read from
/// the record. It overlaps what the `load` event names without being its shape:
/// it carries the session, run and artifact, and lacks the event's stack,
/// lineage, reset and prompt digest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadFacts {
    pub session: SessionId,
    pub run: RunId,
    /// The declaration file's digest as admin read it at the inventory.
    pub declaration: String,
    pub artifact: crate::ArtifactRef,
    pub residual_readout: bool,
    /// The field election's depth where one stands, as the load event's
    /// `field` member carries it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    pub surprisal: bool,
    pub state_election: crate::config::StateElection,
    pub state_store: crate::config::StateStore,
    pub state_member: bool,
    pub composer: Composer,
    /// **The declared drain bound the run holds**, in seconds, as the enter
    /// carried it (`EnterPayload.drain_bound`); absent where the declaration
    /// is silent, so a run that declared none answers as before. Admin reads
    /// it at the observation before every leave, so its deadline and the
    /// harness's timing agree by construction (`weaver-admin-Spec` 3.0, the
    /// bounds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drain_bound: Option<u64>,
    /// The declared wind-down bound the run holds, as `drain_bound`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind_down_bound: Option<u64>,
}
```

## `crates/weaver-types/src/config.rs`, lines 150 to 175

The store election as `show` reports it: `engine`, kebab-case, `none` or `sqlite`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StateStore {
    pub engine: StoreEngine,
}

impl Default for StateStore {
    /// The embedded engine, which is what an absent election means.
    fn default() -> Self {
        StateStore {
            engine: StoreEngine::Sqlite,
        }
    }
}

/// The engines the state charter charters, and the declared absence of a
/// member. Closed at the engines this build provides, and a declaration
/// electing any other refuses at the parse naming `state-store.engine`, per
/// `weaver-types-Spec` section 2. A further engine is a state act before it
/// is a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StoreEngine {
    None,
    Sqlite,
}
```

## `crates/weaver-types/src/config.rs`, lines 215 to 238

The state election as `show` reports it: kebab-case, so `all-kinds` and `keys`, each key a `kind` and its `paths`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct StateElection {
    pub all_kinds: bool,
    pub keys: Vec<ElectedKindConfig>,
}

impl Default for StateElection {
    fn default() -> Self {
        StateElection {
            all_kinds: true,
            keys: Vec::new(),
        }
    }
}

/// One kind's election: the kind as the canonical form spells it, and the
/// payload key paths elected for it, dotted from the payload root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct ElectedKindConfig {
    pub kind: String,
    pub paths: Vec<String>,
}
```

## `crates/weaver-types/src/config.rs`, lines 535 to 540

The artifact `show` reports: a newtype over a string, so a string on the wire.

```rust
/// What an operator writes to name a model artifact. Resolution and readability
/// are the SPU's at admission, under the agent's identity, admin asking only that
/// it is named, per `weaver-admin-PRD` section 4.3. This crate answers well-formed
/// only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRef(pub String);
```

## `crates/weaver-trace/src/event.rs`, lines 224 to 242

The trace's payloads: untagged, and `load` carries `Elections`.

```rust
/// The kind-to-payload mapping is total, twenty-six kinds and twenty
/// dispositions, the payload-free case counting as one of them.
/// **`pairing_licensed` in `writer.rs` enforces the mapping and is the
/// authority on it**, this comment naming only which variant of this enum
/// each kind reaches. Variants throughout rather than the shapes they hold,
/// so a row reads against the licensing match with no translation step, and
/// because six variants hold `Box<RawValue>` and a shape cannot tell them
/// apart: `session.closed` and `turn.started` carry nothing, `unload`
/// carries `Unload` where a member stood and nothing where none did, `load`
/// carries `Elections`, the five message kinds carry `Message`,
/// `turn.closed` carries `TurnClosed`, `fault` carries `Fault`, `flush`
/// carries `Flush`, `elision` carries `Elision`, `refusal` carries
/// `Refusal`, the four model kinds carry their four own variants, the
/// classify pair carries its two, `recall` carries `Recall`, `score` carries
/// `Score`, `save_point` carries `SavePoint`, and the tool bracket's two
/// carry `Deferred`.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Payload {
```

## `crates/weaver-trace/src/event.rs`, lines 269 to 274

The `Elections` variant of the trace's payload.

```rust
    /// The diagnostic elections the load declared, carried by its `load`
    /// event so a record declares the posture it was written in, per
    /// charter section 3.2. Boxed, as the lineage inside it is (#475): the
    /// boundary, the cause and the library path of 2026-10-03 took the case
    /// past the enum's size bound, and a box crosses as its contents do.
    Elections(Box<Elections>),
```

## `crates/weaver-trace/src/event.rs`, lines 653 to 660

The store as a load event names it: `engine`, the engine's name.

```rust
/// The store the state member stands on, per `weaver-trace-PRD` section 3.1:
/// the engine by its name. Written whole on the load. This crate spells the
/// shape itself, as it does the loop's, because the floor's election type is
/// the declaration's and the record names what was resolved.
#[derive(Default, Debug, Clone, PartialEq, Serialize)]
pub struct StoreIdentity {
    pub engine: String,
}
```

## `crates/weaver-trace/src/event.rs`, lines 707 to 801

The load event the harness's lifecycle authors, which the listener writes as the tuple with the source `event`: no rename, so snake_case members; the tee's election as `tee`; the turn context, kebab-case; no artifact.

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Elections {
    pub residual_readout: bool,
    /// The field's declared depth where its election stood, absent where
    /// it did not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    /// Whether the per-position surprisal was elected, per
    /// `weaver-spu-PRD` section 13.12.
    ///
    /// **Serialized even when false**, which is the opposite election from
    /// the field's option above. The field's absence is the whole of what
    /// an unelected field has to say. This one distinguishes a record
    /// whose operator declined the vector from a record written before the
    /// election existed, where the member is absent altogether: absent,
    /// false, and true are three states and the shape keeps them three.
    pub surprisal: bool,
    /// Whether the state member's end arrived on the enter, per
    /// `weaver-trace-Spec` section 3 as of 2026-09-03: the harness's own
    /// knowledge and never a read of the deployment. **Serialized even
    /// when false**, for the same three-state reason as the surprisal: a
    /// record written before the member existed is absent, not false.
    pub state_member: bool,
    /// The store the member stands on, per `weaver-trace-PRD` section 3.1
    /// as of 2026-09-04: the engine by its name, copied from the enter.
    /// Written beside `state_member` because the two answer different
    /// questions: the election is what the deployment asked for and the
    /// member is whether an end arrived.
    pub state_store: StoreIdentity,
    /// The declaration file's digest the load was built from, as admin read
    /// it at the inventory and the enter carried it, per `weaver-trace-PRD`
    /// section 3.1 as of 2026-09-04: a record names its own declaration
    /// rather than leaning on a deposit beside it.
    pub declaration: String,
    /// The loop that assembled this run's prompts, per `weaver-trace-Spec`
    /// section 3: the binary that ran it, and the file and its digest at
    /// the load where the loop is a file. Two loops assemble different
    /// prompts from one declaration, so a record that does not name its
    /// composer cannot be compared with one that does.
    pub composer: LoopIdentity,
    /// The tee's rule, this crate's own `Election`, written whole on every
    /// `load` the harness authors: every load has an election, a deployment
    /// that elects nothing running under the default, per
    /// `weaver-trace-Spec` section 3. The `Option` is the reader's, per the
    /// compatibility rule: absence means the record predates the member and
    /// the rule that built the state is unrecoverable, never that the
    /// election was the default, so such a record replays its token path
    /// and cannot be certified for state.
    pub tee: Option<crate::tee::Election>,
    /// Where the load restored a save point, per `weaver-trace-Spec`
    /// section 3 on the operator's rulings of 2026-10-02 on #58: the save
    /// point's digest, the position it covers, whether the operator
    /// supplied it and where the builder cut it from, copied from the enter.
    /// Absent otherwise, never null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Box<Lineage>>,
    /// The reset the load records, beside the lineage and apart from it,
    /// per the same section: present only where the agent's last run did
    /// not end in a clean unload, whether or not a save point stands, the
    /// prior run and the reason as admin resolved them and the save point
    /// reset to being the lineage's. Absent otherwise, never null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<Reset>,
    /// The digests of the organ binaries admin started, keyed by name, per
    /// the same section, so a record is sufficient for its own conditions.
    pub stack: std::collections::BTreeMap<String, String>,
    /// The sha256 hex of the agent's `roles.toml` as admin read it, per
    /// `weaver-trace-Spec` section 3 as of 2026-10-03: a member marked
    /// boundary and never constitution, so each run's record declares who
    /// could read it without the boundary file joining the declaration.
    pub boundary: String,
    /// Who asked for this load.
    pub cause: Cause,
    /// The engine libraries' directory the run loaded, where the agent's
    /// root names one. Absent otherwise, never null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_path: Option<String>,
    /// **The run's turn-context election**, per `weaver-trace-Spec` section 3
    /// (#423): whether every turn was answered from the identity prefix and
    /// its own request, `independent`, or from the session's accumulation,
    /// `continuous`, copied from the enter's resolved election. **Serialized
    /// even where it is the default**, for the surprisal's three-state reason:
    /// a record written before the election is absent, not `continuous`.
    pub turn_context: TurnContext,
}

/// The turn-context election as the load event names it, per
/// `weaver-trace-Spec` section 3: this crate's spelling, the floor's own
/// being `weaver-types`'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnContext {
    Continuous,
    Independent,
}
```

## `crates/weaver-trace/src/tee.rs`, lines 22 to 44

The tee's election as the load event carries it: no rename, so `all_kinds` and `keys`.

```rust
/// The election, as `weaver-harness-state-contract` defines the term: the
/// elected kinds and their payload key paths, fixed at load. The default is
/// the envelope of every kind and nothing more, per `weaver-trace-PRD`
/// section 11, so a deployment that elects nothing still holds the session's
/// shape and pays for no payload it never asked to keep.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Election {
    /// Every kind crosses with its envelope. When false, only the kinds
    /// named in `keys` cross at all.
    pub all_kinds: bool,
    /// Payload key paths per kind, each path dotted from the payload root,
    /// on top of the envelope. An entry with no paths is a meaningful
    /// election: presence itself is state.
    pub keys: Vec<ElectedKind>,
}

/// One kind's election: the kind as the canonical form spells it, and the
/// payload key paths elected for it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ElectedKind {
    pub kind: String,
    pub paths: Vec<String>,
}
```

## `crates/weaver-trace/src/serving/payloads/records.rs`, lines 170 to 200

The load event the serving flow authors, the other shape a tuple with the source `event` may have: no tee, no turn context, no artifact.

```rust
/// The former load elections with the tee election removed and explicit proof inputs.
#[derive(Debug, Clone, Serialize)]
pub struct LoadRecord {
    pub residual_readout: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<u32>,
    pub surprisal: bool,
    pub state_member: bool,
    pub state_store: StoreIdentity,
    pub declaration: String,
    pub composer: LoopIdentity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Box<Lineage>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<Reset>,
    pub stack: std::collections::BTreeMap<String, String>,
    pub boundary: String,
    pub cause: Cause,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_path: Option<String>,
    pub state_flow: FlowDeclarationRecord,
    pub state_start: StateStartRecord,
    pub clock_source: ClockSource,
    pub origin_ns: DecimalU64,
    pub state_member_observation: MemberObservationIdentity,
    pub template_inventory: TemplateInventory,
    /// Exact scheme artifact text. JSON string escaping preserves its whitespace
    /// without putting physical newlines inside the canonical trace record.
    pub ste100_scheme: String,
    pub ste100_binary_sha256: Digest,
}
```

## `crates/weaver-trace/src/serving/event.rs`, lines 37 to 49

The serving flow's payloads: untagged, its `Elections` variant boxing `LoadRecord`.

```rust
/// Untagged by design. The envelope kind is the discriminant. Serving recall
/// keeps its read and boot wrappers separate from the legacy RecallAccount.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Payload {
    Message(Box<RawValue>),
    TurnClosed(crate::TurnClose),
    Unload(Box<UnloadClose>),
    Fault(Box<RawValue>),
    ModelRequest(Box<RawValue>),
    ModelOutput(crate::ModelOutput),
    ModelField(crate::ModelField),
    Elections(Box<LoadRecord>),
```
