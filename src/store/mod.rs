//! The store: the Postgres pool, the migrations, and the reads of
//! `weaver-web-Spec` section 4 over the tables of its section 2.
//!
//! **All six of that section's reads are served here** as of 2026-09-11.
//! The fifth landed with Record and the sixth with the plan's schema, which
//! is where section 4 said that one was owed. The Experiments list's read
//! is the one still owed, at the act that builds it.
//!
//! This module is the first in the crate written to the standing Spec
//! rather than to the charter it replaced, which is why it is the first
//! whose files carry conformance headers. The register at
//! `docs/project/inventory-weaver-web-code.md` records the rest of the crate
//! as written to the retired text.

pub mod experiment;
pub mod key;
pub mod plan;
pub mod read;

pub use experiment::{Arm, Experiment, ExperimentState, Registered, StagedExperiment, Sweep};
pub use key::{ArmId, PlanId, PositionKey, RunId, TurnId};
// **`plan::Arm` is not re-exported and `experiment::Arm` is**, and the path
// is the point rather than a collision worked around. They are one thing at
// two resolutions: the arm as the plan composes it, and the arm as the
// frozen sweep fans it, one row per value with the run it produced. A front
// door that exported both bare would flatten the resolution out of the
// name, so the plan's stays qualified and reads `plan::Arm` where it is
// used.
pub use plan::{Disposition, Entry, Plan, Registration};
pub use read::{Alternatives, Chip, Cursor, PositionPoint, RunPage, RunTuple};

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

#[derive(Clone)]
pub struct Store {
    pub pool: PgPool,
}

impl Store {
    /// Connect and bring the schema to the migrations' head. `sqlx` checksums
    /// each applied version against its source, so a database that ran a
    /// migration this tree no longer carries refuses here rather than
    /// running against a schema it was not written for, per the ruling at
    /// PR #499 that the schema is replaced rather than migrated.
    pub async fn connect(database_url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }
}
