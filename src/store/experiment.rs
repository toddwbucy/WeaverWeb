//! conforms: web-registered-experiment-is-immutable
//! conforms: web-registered-question-is-immutable
//!
//! The staged experiment as the store holds it, per `weaver-web-Spec`
//! section 2.5, in the five states of section 5.1, and the sweep of section
//! 5.4.
//!
//! **The pin is `Registered`.** Section 5.1 has registration freeze the row,
//! and section 2.5 has the question frozen with the rest of it. A registered
//! experiment reaches a caller only as `Registered`, which derefs to the row
//! and does not deref mutably, so there is no path from a registered row to
//! an edit that the compiler will accept. The two assertions are one pin
//! because the question is a member of the row and the row is what freezes.
//!
//! ```compile_fail
//! use weaver_web::store::Registered;
//!
//! fn edit(r: &mut Registered) {
//!     r.question = "a different question".to_string();
//! }
//! ```
//!
//! **A draft is not `Registered`** and reaches a caller as the bare row,
//! because editing one is what drafts are for, per section 5.1.
//!
//! **The reads are the door.** `Store::experiment` and `Store::sweep` return
//! `Experiment`, whose `Registered` arm is the only way a frozen row leaves
//! the store, so the pin holds every row a read hands out and not only the
//! rows a caller chose to wrap. `Registered::new` is the one constructor and
//! the read calls it, a draft becoming the `Draft` arm through its refusal.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ops::Deref;

use super::key::RunId;
use super::read::RunTuple;

/// The five states of section 5.1. The spelling is the schema's check
/// constraint's, one to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExperimentState {
    Draft,
    Registered,
    Queued,
    Running,
    Returned,
}

impl ExperimentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Registered => "registered",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Returned => "returned",
        }
    }

    /// Every state past `Draft` is frozen. Registration is the freeze and
    /// queueing, running and returning are what happen to a frozen row.
    pub fn is_frozen(self) -> bool {
        !matches!(self, Self::Draft)
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "draft" => Self::Draft,
            "registered" => Self::Registered,
            "queued" => Self::Queued,
            "running" => Self::Running,
            "returned" => Self::Returned,
            _ => return None,
        })
    }
}

/// One row of section 2.5.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedExperiment {
    pub experiment_id: i64,
    pub state: ExperimentState,
    pub state_changed_at: DateTime<Utc>,
    pub parent_run: Option<RunId>,
    pub branch_position: Option<i32>,
    pub forced_token: Option<String>,
    pub parent_declaration_id: Option<i64>,
    pub diff_at_load: Option<serde_json::Value>,
    pub diff_at_turn: Option<serde_json::Value>,
    /// The question the engineer meant to ask, which nothing upstream knows
    /// and nothing else in the store holds.
    pub question: String,
    /// Section 5.4: the one member a sweep moves, and the values it takes.
    pub swept_member: Option<String>,
    pub swept_values: Option<Vec<serde_json::Value>>,
    pub author: Option<String>,
    pub version: i64,
}

/// A registered experiment. Derefs to the row and never mutably.
#[derive(Debug, Clone, Serialize)]
pub struct Registered(StagedExperiment);

impl Registered {
    /// Wrap a row whose state is frozen. A draft is refused and handed
    /// back, because a `Registered` that holds a draft would be a pin that
    /// pins nothing. The refusal boxes the row rather than moving it, so
    /// the `Ok` arm a caller takes on every frozen row does not pay for the
    /// width of the `Err` arm it takes on none.
    pub fn new(row: StagedExperiment) -> Result<Self, Box<StagedExperiment>> {
        if row.state.is_frozen() {
            Ok(Self(row))
        } else {
            Err(Box::new(row))
        }
    }
}

impl Deref for Registered {
    type Target = StagedExperiment;
    fn deref(&self) -> &StagedExperiment {
        &self.0
    }
}

/// A row as a read returns it: bare where it is a draft, wrapped where it
/// is frozen. There is no third arm and no accessor that hands a frozen row
/// back bare.
#[derive(Debug, Clone, Serialize)]
pub enum Experiment {
    Draft(StagedExperiment),
    Registered(Registered),
}

impl Experiment {
    /// The row, readable in either arm.
    pub fn row(&self) -> &StagedExperiment {
        match self {
            Self::Draft(row) => row,
            Self::Registered(registered) => registered,
        }
    }

    pub fn is_frozen(&self) -> bool {
        matches!(self, Self::Registered(_))
    }
}

/// One value of a sweep with its run where one exists, per section 4's
/// fourth read. **The unit is the value and not the run**: a value with no
/// run is an arm that never ran and keeps its place in the set, which is
/// what makes a sweep's absences legible.
#[derive(Debug, Clone, Serialize)]
pub struct Arm {
    /// The value this arm was produced under. **Absent on a point arm**,
    /// which section 2.9 has register with no swept member and produce one
    /// run - so there is no value rather than an empty one, per section 6.
    ///
    /// **It is omitted rather than serialized as null.** A sweep's value may
    /// itself be JSON null, and a derive that wrote `None` as null would
    /// make the two equal at the one boundary where a reader meets them,
    /// which is the whole distinction this member exists to carry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// The run, with its tuple, its signature and its parting position where
    /// it has one. `None` is the arm that never ran.
    pub run: Option<RunTuple>,
}

/// The fourth read's answer: one experiment's value set, each value with
/// its run.
#[derive(Debug, Clone, Serialize)]
pub struct Sweep {
    pub experiment: Experiment,
    /// The member the sweep moves. **Absent where the experiment is not a
    /// sweep**, rather than an empty string, which would be the
    /// absent-not-empty failure section 6 forbids at the view committed one
    /// layer lower where the view cannot see it. Omitted rather than
    /// serialized as null, for the reason `Arm::value` gives.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<String>,
    pub arms: Vec<Arm>,
}
