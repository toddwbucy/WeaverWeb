# The state election, as read at WeaverAgent `5ec7ecf8fe9cb37a45e2d65298a02fc395aa4291`

Held by `src/declaration/state_election.rs` (WeaverWeb Spec 6.1, the test of membership). Each passage below is the named lines of the named file at that commit, byte for byte, inside its fence. A reread at a new pin replaces this file with one named for that commit, and the diff between the two is the reread.

## `crates/weaver-types/src/config.rs`, lines 43 to 45

The head of `AgentConfig`, the agent's declared surface: its members are kebab-case in the file and sit at its top level.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct AgentConfig {
```

## `crates/weaver-types/src/config.rs`, lines 68 to 74

The state election: an optional member of `AgentConfig`, its absence the default election, which admin resolves at inventory.

```rust
    /// The tee's election, per `weaver-types-Spec` section 2: the one
    /// optional field, optional by the required-field rule's own exception
    /// because `weaver-state-PRD` section 4 rules what absence means - the
    /// default election, the envelope of every kind. Admin resolves the
    /// absence at inventory, so the worker never re-derives it.
    #[serde(default)]
    pub state_election: Option<StateElection>,
```

## `crates/weaver-types/src/config.rs`, lines 207 to 238

The election's two structs: both members required inside a present table, closed to unknown members, and the default's spelling.

```rust
/// The operator's election of payload key paths for the state tee, per
/// `weaver-types-Spec` section 2. The resolved default's spelling is fixed
/// there so two resolvers cannot disagree: `all_kinds` true and `keys`
/// empty. The empty list is only the default's spelling: `keys` stays
/// meaningful beside `all_kinds` true, each named kind adding payload
/// paths on top of the envelope every kind already crosses with. When the
/// block is present in the file, both members are required, the
/// required-field discipline resuming inside it.
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

## `crates/weaver-trace/src/event.rs`, lines 117 to 212

The canonical kinds, closed at twenty-six by ruling, each spelled by its rename: the kinds an election's `kind` names.

```rust
/// The twenty-six event kinds, exhaustive, matching the charter's section 3.1
/// exactly. Every kind carries an explicit rename because no scheme produces
/// the charter's dotted names, and the enum is exhaustive because the set is
/// closed by ruling: an attribute that let a consumer absorb a further kind
/// into a wildcard would defeat the closure the corpus keys on. **The count
/// is pinned in `tests/kinds.rs`**, an array of twenty-six checked against an
/// exhaustive match over this enum, so a kind added here and answered in the
/// crate's own matches stops at that file's match. An act that writes the new
/// arm there too and leaves the array alone still passes, which is issue
/// #633's to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Kind {
    #[serde(rename = "load")]
    Load,
    #[serde(rename = "unload")]
    Unload,
    #[serde(rename = "session.closed")]
    SessionClosed,
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "turn.closed")]
    TurnClosed,
    #[serde(rename = "message.system")]
    MessageSystem,
    #[serde(rename = "message.user")]
    MessageUser,
    #[serde(rename = "message.assistant")]
    MessageAssistant,
    #[serde(rename = "message.tool_result")]
    MessageToolResult,
    #[serde(rename = "tool.call.started")]
    ToolCallStarted,
    #[serde(rename = "tool.call.completed")]
    ToolCallCompleted,
    #[serde(rename = "fault")]
    Fault,
    #[serde(rename = "elision")]
    Elision,
    /// A typed refusal on any seam, per charter section 3.1's twenty-first
    /// kind: which ask was refused, which seam answered, and that seam's own
    /// case with the values it carries.
    #[serde(rename = "refusal")]
    Refusal,
    #[serde(rename = "flush")]
    Flush,
    #[serde(rename = "model.request")]
    ModelRequest,
    #[serde(rename = "model.output")]
    ModelOutput,
    #[serde(rename = "model.measurement")]
    ModelMeasurement,
    /// The probability field at one decode position, per charter section
    /// 3.1's nineteenth kind. The first kind recorded per position rather
    /// than per turn, and the first recorded only while an election
    /// stands.
    #[serde(rename = "model.field")]
    ModelField,
    #[serde(rename = "classify.request")]
    ClassifyRequest,
    #[serde(rename = "classify.output")]
    ClassifyOutput,
    /// An answered ask on the state seam, per charter section 3.1's
    /// twenty-second kind: which ask the harness made and the identities of
    /// the events custody answered with, never their contents.
    #[serde(rename = "recall")]
    Recall,
    /// One message of a restored conversation, seated at a restoring load's
    /// open ahead of every turn, per charter section 3.1's twenty-third
    /// kind: the message whole, its role and its content, turnless as the
    /// identity is. A kind of its own so the four turned message kinds stay
    /// turn-required and a restored message never reads as the identity.
    #[serde(rename = "message.restored")]
    MessageRestored,
    /// A task's verdict on its run, per charter section 3.1's twenty-fourth
    /// kind (#523): the predicate the task answered, whether it held, and the
    /// ratio over the task's denominator where one exists. Authored at the
    /// run's close, between turns, so it belongs to none.
    #[serde(rename = "score")]
    Score,
    /// A save point taken, per charter section 3.1's twenty-fifth kind, on
    /// the operator's ruling of 2026-10-02 on #58's fourth question: which
    /// save point, by digest, and the trace position it covers. Authored
    /// after the holdings it names were taken, turnless, and outside every
    /// election, so the tee never sends it to state and no save point holds
    /// its own event.
    #[serde(rename = "save_point")]
    SavePoint,
    /// A call the emission attempted and the family parse could not
    /// recover, per charter section 3.1's twenty-sixth kind, on the
    /// operator's ruling of 2026-10-13 on #31: the fragment as written, the
    /// parse's reason, and the grammar that judged it, beside the verbatim
    /// `model.output` keeps. Turned, an emission belonging to the turn that
    /// drew it, and a fact rather than a fault.
    #[serde(rename = "model.unrecovered_call")]
    ModelUnrecoveredCall,
}
```

## `crates/weaver-trace/src/tee.rs`, lines 124 to 131

The save-point kind never crosses the tee, whatever the election, so no election can name it to effect.

```rust
    // **The save-point kind never crosses the tee, whatever the election**,
    // per `weaver-trace-PRD` section 3 on the operator's ruling of 2026-10-02
    // on #58: it is provenance about the holdings, authored after the
    // holdings it names were taken, so a save point could never hold its own
    // event and a rebuild that landed it would hold what no restore holds.
    if event.kind == "save_point" {
        return None;
    }
```

## `crates/weaver-admin/src/inventory.rs`, lines 292 to 301

Admin refuses a present election beside the store engine `none`, naming `state-election`.

```rust
    let store = config.state_store.clone().unwrap_or_default();
    let store_invalid = |field: &str| LifecycleRefusal::ConfigInvalid {
        field: Some(FieldName(field.into())),
    };
    match store.engine {
        StoreEngine::None if config.state_election.is_some() => {
            return Err(store_invalid("state-election"));
        }
        StoreEngine::None | StoreEngine::Sqlite => {}
    }
```
