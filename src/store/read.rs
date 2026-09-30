//! conforms: web-nothing-is-computed-at-read-time-unless-the-query-is-recorded
//!
//! Five of the six reads of `weaver-web-Spec` section 4, each an index hit over
//! the schema of section 2, and none of them deriving a value: what a read
//! returns was stored at ingest or authored, per section 2.7, and a reader
//! that wants a derived value asks the recorded query of section 2.6.
//!
//! **A read returns rows and not readings.** Position one is the click, the
//! range is the timeline, the tuple is the label on every reading, and the
//! sweep is one experiment's value set with each value's run. What the
//! surface draws from them is the surface's, and the conversion between an
//! ordinal and a position is the surface's too, per section 6.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use super::Store;
use super::experiment::{Arm, Experiment, ExperimentState, Registered, StagedExperiment, Sweep};
use super::key::{PositionKey, RunId, TurnId};

/// Read one: one position's alternatives, per section 2.1. This is
/// `weaver-analysis field` served from the store rather than re-derived.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alternatives {
    pub key: PositionKey,
    pub token_id: i64,
    pub token_text: String,
    /// Rides every generation unconditionally, per section 6.
    pub entropy: f64,
    /// Rides only where its election stands. Absent is absent and never zero.
    pub surprisal: Option<f64>,
    /// The ranked candidates with their mass, at the depth the declaration's
    /// field election kept.
    pub alternatives: serde_json::Value,
    /// A rank and not a token, per section 2.1.
    pub realized: i32,
    pub residual: Option<Vec<u8>>,
}

/// Read two: one point of a contiguous range, per section 2.1. The timeline
/// and the transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionPoint {
    pub position: i32,
    pub token_id: i64,
    pub token_text: String,
    pub entropy: f64,
    pub surprisal: Option<f64>,
}

/// Read three: the run's row, per section 2.2. The tuple is what identifies
/// the conditions, and lineage and the signature stand beside it outside
/// tuple equality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTuple {
    pub run: RunId,
    pub record_identity: String,
    /// The record spells the seed as an unsigned 64-bit value the suite's
    /// fixtures carry above the signed maximum, so it crosses as text and
    /// the schema holds it as `NUMERIC(20,0)`.
    pub seed: Option<String>,
    pub sampler: serde_json::Value,
    /// The device model and the engine, each from a deposit the caller
    /// named, per section 2.2. `None` where the caller named none, which a
    /// piped record and every record older than deposits both are.
    pub device: Option<String>,
    pub engine: Option<serde_json::Value>,
    pub field_depth: Option<i32>,
    pub task_source: Option<String>,
    pub task_identity: Option<String>,
    pub boundary_set: serde_json::Value,
    pub forced_position: Option<i32>,
    pub forced_token: Option<String>,
    /// Lineage, outside the compound.
    pub parent_run: Option<RunId>,
    pub branch_position: Option<i32>,
    /// Lineage, outside the compound: the first position this run's token
    /// path left its parent's, derived at ingest. `None` where the paths
    /// never parted or where there is no parent.
    pub parting_position: Option<i32>,
    /// The record the row came from, twice, per section 2.2: the identity
    /// the trace's runs share, and sha256 over the run's own lines as the
    /// emitter drained them. Both outside the compound, both `None` where
    /// the emitter could not vouch for them.
    pub record_session: Option<String>,
    pub record_digest: Option<String>,
    /// The seated prefix's length, the resident length before the run's
    /// first turn's input, per section 2.2. Derived by the emitter from the
    /// run's first generation and never here. `None` where the emitter sent
    /// none, which section 5 reads as a branch position an arm cannot take.
    pub prefix_length: Option<i32>,
    /// The emission's signature, outside the compound. Its representation is
    /// section 10's open election.
    pub signature: Option<serde_json::Value>,
    pub ingested_at: DateTime<Utc>,
}

/// The fifth read's statements, one per shape it can take.
///
/// **A filter is a column comparison and never a test on a null
/// parameter.** One statement with `$1 IS NULL OR col = $1` reads well and
/// plans badly: Postgres caches a generic plan after a few executions, and
/// under it the parameter is unknown when the plan is built, so the index
/// section 2.7 carries cannot be matched and every chip becomes a
/// sequential scan. The ordering and the limit are the same in all eight,
/// and the cursor is always the row-value pair, which is what the composite
/// index answers.
const SELECT_ALL: &str = "SELECT * FROM run_tuple ORDER BY ingested_at DESC, run_id DESC LIMIT $1";
const SELECT_ALL_AFTER: &str = "SELECT * FROM run_tuple WHERE (ingested_at, run_id) < ($1, $2) \
     ORDER BY ingested_at DESC, run_id DESC LIMIT $3";
const SELECT_BY_IDENTITY: &str = "SELECT * FROM run_tuple WHERE record_identity = $1 \
     ORDER BY ingested_at DESC, run_id DESC LIMIT $2";
const SELECT_BY_IDENTITY_AFTER: &str = "SELECT * FROM run_tuple WHERE record_identity = $1 \
     AND (ingested_at, run_id) < ($2, $3) ORDER BY ingested_at DESC, run_id DESC LIMIT $4";
const SELECT_BY_SESSION: &str = "SELECT * FROM run_tuple WHERE record_session = $1 \
     ORDER BY ingested_at DESC, run_id DESC LIMIT $2";
const SELECT_BY_SESSION_AFTER: &str = "SELECT * FROM run_tuple WHERE record_session = $1 \
     AND (ingested_at, run_id) < ($2, $3) ORDER BY ingested_at DESC, run_id DESC LIMIT $4";
const SELECT_BY_PARENT: &str = "SELECT * FROM run_tuple WHERE parent_run_id = $1 \
     ORDER BY ingested_at DESC, run_id DESC LIMIT $2";
const SELECT_BY_PARENT_AFTER: &str = "SELECT * FROM run_tuple WHERE parent_run_id = $1 \
     AND (ingested_at, run_id) < ($2, $3) ORDER BY ingested_at DESC, run_id DESC LIMIT $4";

/// A chip, per `weaver-web-Spec` section 4's fifth read: a filter the
/// document indexes, and **a filter it does not index is not a chip this
/// crate offers**. Each variant names a column section 2.7 carries an index
/// for, which is what keeps the surface's promise of a cheap list honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chip {
    /// One record identity, per section 2.7's artifact index. **This is not
    /// an artifact**: section 2.3 has an artifact carry every identity its
    /// weights were admitted under, so a surface asking for an artifact's
    /// runs resolves that row's identities first and asks once per identity.
    RecordIdentity(String),
    /// A session's family, per section 2.7's family index.
    Session(String),
    /// A parent's branches, per section 2.7's lineage index.
    Branches(RunId),
}

/// The page's key: **the ingest's order and the run's identity together**,
/// per section 4. `ingested_at` defaults to the transaction's clock, so a
/// whole ingest shares one value and a cursor on the timestamp alone would
/// drop the rest of a tie larger than the page. The identity breaks the tie
/// into the total order section 2.7's index carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub ingested_at: DateTime<Utc>,
    pub run: RunId,
}

/// One page of read five. `next` is `None` where the page reached the end
/// of what the filter admits, and is the key to resume from otherwise.
#[derive(Debug, Clone, Serialize)]
pub struct RunPage {
    pub runs: Vec<RunTuple>,
    #[serde(skip)]
    pub next: Option<Cursor>,
}

impl Store {
    /// **Read one.** One position's alternatives, addressed by the whole key.
    pub async fn alternatives_at(&self, key: &PositionKey) -> anyhow::Result<Option<Alternatives>> {
        let row = sqlx::query(
            "SELECT token_id, token_text, entropy, surprisal, alternatives, realized, residual \
             FROM position WHERE run_id = $1 AND turn = $2 AND position = $3",
        )
        .bind(&key.run.0)
        .bind(&key.turn.0)
        .bind(key.position)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| Alternatives {
            key: key.clone(),
            token_id: r.get("token_id"),
            token_text: r.get("token_text"),
            entropy: r.get("entropy"),
            surprisal: r.get("surprisal"),
            alternatives: r.get("alternatives"),
            realized: r.get("realized"),
            residual: r.get("residual"),
        }))
    }

    /// **Read two.** A contiguous range of positions within one turn,
    /// inclusive at both ends, in position order.
    pub async fn range(
        &self,
        run: &RunId,
        turn: &TurnId,
        from: i32,
        to: i32,
    ) -> anyhow::Result<Vec<PositionPoint>> {
        let rows = sqlx::query(
            "SELECT position, token_id, token_text, entropy, surprisal \
             FROM position WHERE run_id = $1 AND turn = $2 \
             AND position BETWEEN $3 AND $4 ORDER BY position",
        )
        .bind(&run.0)
        .bind(&turn.0)
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PositionPoint {
                position: r.get("position"),
                token_id: r.get("token_id"),
                token_text: r.get("token_text"),
                entropy: r.get("entropy"),
                surprisal: r.get("surprisal"),
            })
            .collect())
    }

    /// **Read three.** The run's row.
    pub async fn tuple(&self, run: &RunId) -> anyhow::Result<Option<RunTuple>> {
        // The `run_tuple` view of migration 0006 names the column set, so
        // this read and read four's below are static and cannot drift from
        // each other or from `run_tuple_from_row`.
        let row = sqlx::query("SELECT * FROM run_tuple WHERE run_id = $1")
            .bind(&run.0)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(run_tuple_from_row))
    }

    /// **Read five.** Every run's tuple, filtered and paged. The one read
    /// whose unit is the set rather than a member of it, per
    /// `weaver-web-Spec` section 4, and the only one that answers a reader
    /// who has neither a run nor an experiment.
    ///
    /// **It derives nothing and records nothing.** Every column it returns
    /// is one section 2.2 already holds, so section 2.6's condition does not
    /// reach it: what that section makes quotable is a value derived over
    /// runs, and a list of rows the store already holds derives none.
    ///
    /// The page asks for one row more than the caller wanted. **The extra
    /// row is discarded and the cursor is the last row kept**, because the
    /// next page resumes strictly below its cursor: keying on the extra row
    /// would skip exactly that row, which is the off-by-one this read's own
    /// watch caught before it shipped. `next` is `Some` exactly when a
    /// further row exists rather than whenever the page came back full.
    pub async fn runs(
        &self,
        chip: Option<&Chip>,
        limit: u32,
        after: Option<&Cursor>,
    ) -> anyhow::Result<RunPage> {
        // The row-value comparison is what makes the cursor exact and what
        // the composite index of section 2.7 answers: a page resumes below
        // the pair it was handed and never below the timestamp alone.
        let asked = limit.saturating_add(1) as i64;
        // **One static statement per shape, and never a null test standing
        // in for a column.** A predicate of the form `$1 IS NULL OR col =
        // $1` cannot be matched to an index once Postgres caches a generic
        // plan, because the parameter is unknown when that plan is built, so
        // every chip would fall to the sequential scan this read's own
        // assertion says it avoids. Eight statements is the price of the
        // claim being true.
        let sql = match (chip, after.is_some()) {
            (None, false) => SELECT_ALL,
            (None, true) => SELECT_ALL_AFTER,
            (Some(Chip::RecordIdentity(_)), false) => SELECT_BY_IDENTITY,
            (Some(Chip::RecordIdentity(_)), true) => SELECT_BY_IDENTITY_AFTER,
            (Some(Chip::Session(_)), false) => SELECT_BY_SESSION,
            (Some(Chip::Session(_)), true) => SELECT_BY_SESSION_AFTER,
            (Some(Chip::Branches(_)), false) => SELECT_BY_PARENT,
            (Some(Chip::Branches(_)), true) => SELECT_BY_PARENT_AFTER,
        };
        let mut query = sqlx::query(sql);
        match chip {
            Some(Chip::RecordIdentity(v)) | Some(Chip::Session(v)) => query = query.bind(v),
            Some(Chip::Branches(run)) => query = query.bind(&run.0),
            None => {}
        }
        if let Some(cursor) = after {
            query = query.bind(cursor.ingested_at).bind(&cursor.run.0);
        }
        let rows = query.bind(asked).fetch_all(&self.pool).await?;

        let mut runs: Vec<RunTuple> = rows.into_iter().map(run_tuple_from_row).collect();
        let further = runs.len() as i64 == asked;
        if further {
            // The extra row proved a further page exists and is not part of
            // this one. It is dropped rather than kept as the key: the next
            // page resumes strictly below its cursor, so keying on this row
            // would skip it.
            runs.pop();
        }
        let next = further.then(|| runs.last()).flatten().map(|last| Cursor {
            ingested_at: last.ingested_at,
            run: last.run.clone(),
        });
        Ok(RunPage { runs, next })
    }

    /// **Read four.** One experiment's value set, each value with its run
    /// where one exists. `None` where no such experiment stands.
    ///
    /// **A point arm returns one arm whose value is absent**, per section
    /// 2.9: an arm that frees nothing registers with no swept member and
    /// produces one run. This read returned no arms for that case until
    /// 2026-09-11, which left the run reachable only by a reader who already
    /// had its identity - the one reader section 4 says it does not serve.
    /// The absent value is an absence the reader can see, which is the same
    /// answer this read already gives for an arm that never ran.
    pub async fn sweep(&self, experiment_id: i64) -> anyhow::Result<Option<Sweep>> {
        let Some(experiment) = self.experiment(experiment_id).await? else {
            return Ok(None);
        };
        let swept = (
            experiment.row().swept_member.clone(),
            experiment.row().swept_values.clone(),
        );

        // The runs this experiment produced, each with the value it was
        // produced under. One query, the association being explicit in the
        // schema rather than recovered by matching tuples against the set.
        let produced = sqlx::query(
            "SELECT ser.swept_value, r.* \
             FROM staged_experiment_run ser JOIN run_tuple r ON r.run_id = ser.run_id \
             WHERE ser.experiment_id = $1 ORDER BY r.ingested_at, r.run_id",
        )
        .bind(experiment_id)
        .fetch_all(&self.pool)
        .await?;

        let mut by_value: Vec<(Option<serde_json::Value>, RunTuple)> = produced
            .into_iter()
            .map(|r| {
                // Kept as an option: a point arm's row carries no value, and
                // flattening that to a JSON null would make it equal to a
                // sweep whose value really is null.
                let v: Option<serde_json::Value> = r.get("swept_value");
                (v, run_tuple_from_row(r))
            })
            .collect();

        let (member, arms) = match swept {
            // The unit is the value. Every value in the frozen set returns,
            // with its run where one was produced under it, so an arm that
            // never ran keeps its place.
            (Some(member), Some(values)) => {
                let arms = values
                    .into_iter()
                    .map(|value| {
                        let run = by_value
                            .iter()
                            .position(|(v, _)| v.as_ref() == Some(&value))
                            .map(|i| by_value.remove(i).1);
                        Arm {
                            value: Some(value),
                            run,
                        }
                    })
                    .collect();
                (Some(member), arms)
            }
            // **A point arm.** There is no set to walk, so the unit falls
            // back to the run this arm produced, and the value it returns is
            // absent rather than empty.
            //
            // **The value is carried and not assumed.** Migration 0002
            // constrains the experiment's member against its set and says
            // nothing about the value a produced run carries, so a row that
            // holds one is a row this read must report rather than overwrite
            // with the absence it was looking for.
            //
            // **An arm that has not run keeps its place**, exactly as a
            // value in a frozen set does: section 2.9 has a point arm
            // produce one run, so before it runs there is one arm with
            // neither a value nor a run, and returning nothing would leave a
            // registered arm invisible to the read that exists to show it.
            (None, None) => {
                let arms: Vec<Arm> = if by_value.is_empty() {
                    vec![Arm {
                        value: None,
                        run: None,
                    }]
                } else {
                    by_value
                        .into_iter()
                        .map(|(value, run)| Arm {
                            value,
                            run: Some(run),
                        })
                        .collect()
                };
                (None, arms)
            }
            // **A half-stated sweep is an error and not a point arm.**
            // Migration 0002's check makes this unreachable from a healthy
            // database, which is the reason to fail here rather than to
            // reinterpret: a restored dump or a decode that lost the set
            // would otherwise render a real sweep as a point arm and drop
            // both the member and every value it froze.
            (member, values) => anyhow::bail!(
                "staged_experiment {experiment_id} states half a sweep: member {member:?}, \
                 values {}",
                values.map_or("absent", |_| "present")
            ),
        };

        Ok(Some(Sweep {
            experiment,
            member,
            arms,
        }))
    }

    /// The experiment's own row, by id, wrapped where it is frozen. Not one
    /// of the reads on its own: read four calls it. **The Experiments list
    /// of the charter's section 3.6 is not served here and has no read
    /// yet**, per `weaver-web-Spec` section 6, that surface being a list
    /// where this is a lookup by identity. The read it is owed will want
    /// this column set, which is why the set is named once.
    pub async fn experiment(&self, experiment_id: i64) -> anyhow::Result<Option<Experiment>> {
        let row = sqlx::query(
            "SELECT experiment_id, state, state_changed_at, parent_run_id, branch_position, \
             forced_token, parent_declaration_id, diff_at_load, diff_at_turn, question, \
             swept_member, swept_values, author, version \
             FROM staged_experiment WHERE experiment_id = $1",
        )
        .bind(experiment_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| {
            let state_text: String = r.get("state");
            let state = ExperimentState::parse(&state_text)
                .ok_or_else(|| anyhow::anyhow!("staged_experiment {experiment_id} holds a state the five do not name: {state_text}"))?;
            let swept_values: Option<serde_json::Value> = r.get("swept_values");
            let swept_values = match swept_values {
                Some(serde_json::Value::Array(items)) => Some(items),
                Some(_) => anyhow::bail!("staged_experiment {experiment_id} holds swept_values that is not an array"),
                None => None,
            };
            let row = StagedExperiment {
                experiment_id: r.get("experiment_id"),
                state,
                state_changed_at: r.get("state_changed_at"),
                parent_run: r.get::<Option<String>, _>("parent_run_id").map(RunId),
                branch_position: r.get("branch_position"),
                forced_token: r.get("forced_token"),
                parent_declaration_id: r.get("parent_declaration_id"),
                diff_at_load: r.get("diff_at_load"),
                diff_at_turn: r.get("diff_at_turn"),
                question: r.get("question"),
                swept_member: r.get("swept_member"),
                swept_values,
                author: r.get("author"),
                version: r.get("version"),
            };
            Ok(match Registered::new(row) {
                Ok(registered) => Experiment::Registered(registered),
                Err(draft) => Experiment::Draft(*draft),
            })
        })
        .transpose()
    }
}

fn run_tuple_from_row(r: sqlx::postgres::PgRow) -> RunTuple {
    RunTuple {
        run: RunId(r.get("run_id")),
        record_identity: r.get("record_identity"),
        seed: r.get("seed"),
        sampler: r.get("sampler"),
        device: r.get("device"),
        engine: r.get("engine"),
        field_depth: r.get("field_depth"),
        task_source: r.get("task_source"),
        task_identity: r.get("task_identity"),
        boundary_set: r.get("boundary_set"),
        forced_position: r.get("forced_position"),
        forced_token: r.get("forced_token"),
        parent_run: r.get::<Option<String>, _>("parent_run_id").map(RunId),
        branch_position: r.get("branch_position"),
        parting_position: r.get("parting_position"),
        record_session: r.get("record_session"),
        record_digest: r.get("record_digest"),
        prefix_length: r.get("prefix_length"),
        signature: r.get("signature"),
        ingested_at: r.get("ingested_at"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! These run against a live PostgreSQL named by `DATABASE_URL`, because
    //! a read over a schema is tested against the schema or not at all.
    //! Without the variable they pass by not running and say so, so a box
    //! with no database still passes the workspace.

    use super::*;
    use crate::store::Registered;
    use serde_json::json;

    pub(crate) async fn store() -> Option<Store> {
        let Ok(url) = std::env::var("DATABASE_URL") else {
            eprintln!(
                "skipped: DATABASE_URL is not set, and a read over a schema is tested against one"
            );
            return None;
        };
        Some(Store::connect(&url).await.expect("connect and migrate"))
    }

    async fn seed_run(s: &Store, run_id: &str, parent: Option<&str>, parting: Option<i32>) {
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, seed, sampler, device, \
             engine, boundary_set, parent_run_id, branch_position, parting_position, signature) \
             VALUES ($1, 'REC', 14458752852352082704, '{}', 'cuda:0', '{}', '[]', $2, $3, $4, $5) \
             ON CONFLICT (run_id) DO UPDATE SET parent_run_id = EXCLUDED.parent_run_id, \
             branch_position = EXCLUDED.branch_position, \
             parting_position = EXCLUDED.parting_position, signature = EXCLUDED.signature",
        )
        .bind(run_id)
        .bind(parent)
        .bind(parent.map(|_| 22))
        .bind(parting)
        .bind(json!({"shingles": [1, 2, 3]}))
        .execute(&s.pool)
        .await
        .expect("seed run");
    }

    #[tokio::test]
    async fn read_one_is_addressed_by_the_whole_key_and_absence_is_none() {
        let Some(s) = store().await else { return };
        seed_run(&s, "r-read1", None, None).await;
        sqlx::query(
            "INSERT INTO position (run_id, turn, position, token_id, token_text, entropy, surprisal, alternatives, realized) \
             VALUES ('r-read1', 't-1', 154, 19026, 'Okay', 0.0024, NULL, '[{\"token\":19026,\"probability\":0.9998}]', 0) \
             ON CONFLICT DO NOTHING",
        )
        .execute(&s.pool)
        .await
        .unwrap();

        let key = PositionKey {
            run: RunId("r-read1".into()),
            turn: TurnId("t-1".into()),
            position: 154,
        };
        let got = s
            .alternatives_at(&key)
            .await
            .unwrap()
            .expect("the row stands");
        assert_eq!(got.token_id, 19026);
        assert_eq!(got.realized, 0);
        // Absent-not-empty: surprisal rode no election here and reads as None, never as zero.
        assert_eq!(got.surprisal, None);

        let elsewhere = PositionKey {
            position: 155,
            ..key
        };
        assert!(s.alternatives_at(&elsewhere).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn read_two_is_ordered_and_inclusive() {
        let Some(s) = store().await else { return };
        seed_run(&s, "r-read2", None, None).await;
        for (p, e) in [(200, 0.5), (202, 0.7), (201, 0.6), (203, 0.8)] {
            sqlx::query(
                "INSERT INTO position (run_id, turn, position, token_id, token_text, entropy, alternatives, realized) \
                 VALUES ('r-read2', 't-1', $1, 1, 'x', $2, '[]', 0) ON CONFLICT DO NOTHING",
            )
            .bind(p)
            .bind(e)
            .execute(&s.pool)
            .await
            .unwrap();
        }
        let pts = s
            .range(&RunId("r-read2".into()), &TurnId("t-1".into()), 201, 203)
            .await
            .unwrap();
        assert_eq!(
            pts.iter().map(|p| p.position).collect::<Vec<_>>(),
            vec![201, 202, 203]
        );
        assert_eq!(pts[0].entropy, 0.6);
    }

    #[tokio::test]
    async fn read_three_carries_the_seed_the_record_spells() {
        let Some(s) = store().await else { return };
        seed_run(&s, "r-read3", None, None).await;
        let t = s
            .tuple(&RunId("r-read3".into()))
            .await
            .unwrap()
            .expect("the row stands");
        // The suite's fixture seed is above i64::MAX and crosses as text unchanged.
        assert_eq!(t.seed.as_deref(), Some("14458752852352082704"));
        assert_eq!(t.parent_run, None);
        assert_eq!(t.parting_position, None);
    }

    #[tokio::test]
    async fn read_four_returns_every_value_and_an_unrun_arm_keeps_its_place() {
        let Some(s) = store().await else { return };
        seed_run(&s, "r-sweep-parent", None, None).await;
        seed_run(&s, "r-sweep-arm-7", Some("r-sweep-parent"), Some(149)).await;
        seed_run(&s, "r-sweep-arm-1000003", Some("r-sweep-parent"), None).await;

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO staged_experiment (state, parent_run_id, branch_position, question, author, \
             swept_member, swept_values) \
             VALUES ('registered', 'r-sweep-parent', 22, 'does the seed reach the sampler', 'todd', \
             'seed', '[7, 1000003, 123456789]') RETURNING experiment_id",
        )
        .fetch_one(&s.pool)
        .await
        .unwrap();
        for (run, v) in [("r-sweep-arm-7", 7), ("r-sweep-arm-1000003", 1000003)] {
            sqlx::query("INSERT INTO staged_experiment_run (experiment_id, run_id, swept_value) VALUES ($1, $2, $3)")
                .bind(id)
                .bind(run)
                .bind(json!(v))
                .execute(&s.pool)
                .await
                .unwrap();
        }

        let sweep = s.sweep(id).await.unwrap().expect("the experiment stands");
        assert_eq!(sweep.member.as_deref(), Some("seed"));
        // The unit is the value: three values return, two with runs and one without.
        assert_eq!(sweep.arms.len(), 3);
        let by_value: Vec<(i64, bool)> = sweep
            .arms
            .iter()
            .map(|a| (a.value.as_ref().unwrap().as_i64().unwrap(), a.run.is_some()))
            .collect();
        assert_eq!(
            by_value,
            vec![(7, true), (1000003, true), (123456789, false)]
        );
        // The arm that parted carries where, and the one that reproduced its parent carries None.
        assert_eq!(
            sweep.arms[0].run.as_ref().unwrap().parting_position,
            Some(149)
        );
        assert_eq!(sweep.arms[1].run.as_ref().unwrap().parting_position, None);

        // The pin: a registered row leaves the store wrapped, and the
        // constructor refuses a draft.
        assert!(matches!(sweep.experiment, Experiment::Registered(_)));
        let mut draft = sweep.experiment.row().clone();
        draft.state = ExperimentState::Draft;
        assert!(Registered::new(draft).is_err());
    }

    #[tokio::test]
    async fn read_three_names_the_record_the_row_came_from() {
        let Some(s) = store().await else { return };
        let digest = "a".repeat(64);
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, sampler, device, engine, \
             boundary_set, record_session, record_digest) \
             VALUES ('r-named', 'REC', '{}', 'cuda:0', '{}', '[]', 'sess-1', $1) \
             ON CONFLICT (run_id) DO NOTHING",
        )
        .bind(&digest)
        .execute(&s.pool)
        .await
        .unwrap();
        let t = s.tuple(&RunId("r-named".into())).await.unwrap().unwrap();
        assert_eq!(t.record_session.as_deref(), Some("sess-1"));
        assert_eq!(t.record_digest.as_deref(), Some(digest.as_str()));

        // The seated prefix's length reads back as landed, and the schema
        // refuses a negative one by name.
        sqlx::query("UPDATE run SET prefix_length = 154 WHERE run_id = 'r-named'")
            .execute(&s.pool)
            .await
            .unwrap();
        let t = s.tuple(&RunId("r-named".into())).await.unwrap().unwrap();
        assert_eq!(t.prefix_length, Some(154));
        let negative = sqlx::query("UPDATE run SET prefix_length = -1 WHERE run_id = 'r-named'")
            .execute(&s.pool)
            .await
            .expect_err("the schema refuses a negative prefix length");
        assert!(
            negative
                .to_string()
                .contains("run_prefix_length_is_a_length"),
            "refused by the wrong rule: {negative}"
        );

        // Absent rather than defaulted where the emitter sent none.
        seed_run(&s, "r-unnamed", None, None).await;
        let t = s.tuple(&RunId("r-unnamed".into())).await.unwrap().unwrap();
        assert!(
            t.record_session.is_none() && t.record_digest.is_none() && t.prefix_length.is_none()
        );
        // A run whose caller named no deposit, per section 2.2: the device
        // model, the engine and the verdict all read absent, and the row
        // stands, which is a run this store holds rather than refuses.
        // **The device is absent and the engine is not**: the record's own
        // `load` event carries the organ binaries in its `stack`, so the
        // engine holds the record's half and says the deposit's is absent,
        // which is section 2.2's "absent for neither half where one is
        // missing".
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, sampler, boundary_set, engine) \
             VALUES ('r-no-deposit', 'REC', '{}', '[]', \
             '{\"organ_binaries\": {\"weaver-spu\": \"ab\"}}') \
             ON CONFLICT (run_id) DO NOTHING",
        )
        .execute(&s.pool)
        .await
        .unwrap();
        let t = s
            .tuple(&RunId("r-no-deposit".into()))
            .await
            .unwrap()
            .unwrap();
        assert!(t.device.is_none(), "no deposit named, so no device model");
        let engine = t
            .engine
            .expect("the record's own stack survives with no deposit");
        assert!(
            engine.get("organ_binaries").is_some() && engine.get("libraries").is_none(),
            "the record's half is held and the deposit's is absent: {engine}"
        );

        // The schema refuses a digest that is not sha256 hex.
        let refused = sqlx::query(
            "INSERT INTO run (run_id, record_identity, sampler, device, engine, \
             boundary_set, record_digest) \
             VALUES ('r-bad-digest', 'REC', '{}', 'cuda:0', '{}', '[]', 'not-hex')",
        )
        .execute(&s.pool)
        .await;
        let err = refused.expect_err("the schema refuses a digest that is not sha256 hex");
        assert!(
            err.to_string().contains("run_record_digest_is_sha256_hex"),
            "refused by the wrong rule: {err}"
        );
    }

    /// **Read five is bounded, ordered, and filtered on what is indexed.**
    ///
    /// The tie is the point: every run here is seeded in one transaction, so
    /// `ingested_at` defaults to one clock value for all of them, which is
    /// the case section 4 says a cursor on the timestamp alone would break.
    /// A page of two over a tie of five walks all five and repeats none.
    ///
    /// conforms: web-the-run-list-is-paged-and-records-nothing
    #[tokio::test]
    async fn read_five_pages_a_tie_whole_and_records_nothing() {
        let Some(s) = store().await else { return };
        // **One transaction, so the five share one clock.** `ingested_at`
        // defaults to `now()`, which is the transaction's timestamp and not
        // the statement's, so five separate `execute` calls would take five
        // distinct values and the tie this watch exists to pin would not
        // exist. Measured on 2026-09-11: five statements, five timestamps.
        //
        // **The seed is this run's alone.** A fixed identity plus `ON
        // CONFLICT DO NOTHING` keeps whatever a previous run left, and what
        // a previous run left carries its own `ingested_at` - which is the
        // member under test here, so the watch would measure a row it did
        // not write. The tag makes the rows, the tie and the chip all
        // unique to this invocation, and the insert is left to fail loudly
        // rather than to skip.
        let tag = format!(
            "tie-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recorded_query")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        let mut tx = s.pool.begin().await.unwrap();
        for suffix in ["a", "b", "c", "d", "e"] {
            sqlx::query(
                "INSERT INTO run (run_id, record_identity, sampler, boundary_set, record_session) \
                 VALUES ($1, 'TIE', '{}', '[]', $2)",
            )
            .bind(format!("{tag}-{suffix}"))
            .bind(&tag)
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();
        let clocks: i64 = sqlx::query_scalar(
            "SELECT COUNT(DISTINCT ingested_at) FROM run WHERE record_session = $1",
        )
        .bind(&tag)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert_eq!(clocks, 1, "the tie is real, or this watch pins nothing");
        let chip = Chip::Session(tag.clone());
        let mut seen: Vec<String> = Vec::new();
        let mut cursor = None;
        for _ in 0..6 {
            let page = s.runs(Some(&chip), 2, cursor.as_ref()).await.unwrap();
            assert!(page.runs.len() <= 2, "the limit bounds the page");
            seen.extend(page.runs.iter().map(|r| r.run.0.clone()));
            match page.next {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        // Perturbation: page on `ingested_at` alone and this walk returns
        // two of the five and then stops, the tie being larger than the
        // page. The identity in the cursor is what carries it through.
        assert_eq!(seen.len(), 5, "every row of the tie is walked: {seen:?}");
        let mut sorted = seen.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5, "and none is repeated: {seen:?}");

        // The read records nothing. Perturbation: write a section 2.6 row
        // per page and this rises by five, section 2.6 filling with a list
        // nobody reruns.
        //
        // **The count is a delta and not a zero.** Section 2.6 is a table
        // other reads may legitimately write, so a zero here asserts a
        // property of the database rather than of this read, and the watch
        // would fail for something that is not the claim.
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recorded_query")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(after, before, "a list is walked rather than quoted");
    }

    /// Each chip filters on the column section 2.7 indexes for it, and a
    /// read with no chip admits every run.
    ///
    /// conforms: web-a-chip-filters-only-on-an-indexed-column
    #[tokio::test]
    async fn read_five_filters_on_each_indexed_column() {
        let Some(s) = store().await else { return };
        seed_run(&s, "f-parent", None, None).await;
        seed_run(&s, "f-child", Some("f-parent"), Some(9)).await;
        sqlx::query(
            "UPDATE run SET record_identity = 'F-REC', record_session = 'sess-f' \
             WHERE run_id IN ('f-parent', 'f-child')",
        )
        .execute(&s.pool)
        .await
        .unwrap();

        let by_parent = s
            .runs(Some(&Chip::Branches(RunId("f-parent".into()))), 10, None)
            .await
            .unwrap();
        assert_eq!(
            by_parent
                .runs
                .iter()
                .map(|r| r.run.0.as_str())
                .collect::<Vec<_>>(),
            ["f-child"],
            "the lineage chip returns a parent's branches and not the parent"
        );

        let by_session = s
            .runs(Some(&Chip::Session("sess-f".into())), 10, None)
            .await
            .unwrap();
        assert_eq!(by_session.runs.len(), 2, "the family chip returns both");

        let by_identity = s
            .runs(Some(&Chip::RecordIdentity("F-REC".into())), 10, None)
            .await
            .unwrap();
        assert_eq!(by_identity.runs.len(), 2, "the artifact chip returns both");

        let none = s
            .runs(Some(&Chip::Session("sess-absent".into())), 10, None)
            .await
            .unwrap();
        assert!(
            none.runs.is_empty() && none.next.is_none(),
            "a chip that admits nothing"
        );

        let unfiltered = s.runs(None, 100, None).await.unwrap();
        assert!(
            unfiltered.runs.len() >= 2,
            "no chip admits every run the store holds"
        );
    }

    #[tokio::test]
    async fn a_point_arm_returns_its_run_with_no_value() {
        let Some(s) = store().await else { return };
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO staged_experiment (state, question) VALUES ('draft', 'a point, not a sweep') \
             RETURNING experiment_id",
        )
        .fetch_one(&s.pool)
        .await
        .unwrap();

        // Before it runs there is nothing to return, which is not the same
        // answer as the one below and is why both are watched.
        // **Before it runs the arm still keeps its place.** A point arm
        // produces one run, so an arm with neither a value nor a run is the
        // honest answer and an empty list would hide a registered arm.
        let sweep = s.sweep(id).await.unwrap().unwrap();
        assert_eq!(
            sweep.arms.len(),
            1,
            "the arm keeps its place before it runs"
        );
        assert!(sweep.arms[0].run.is_none(), "and it has not run");
        assert!(sweep.arms[0].value.is_none(), "and was produced under none");
        assert!(
            sweep.member.is_none(),
            "the swept member is absent rather than an empty string"
        );
        assert!(matches!(sweep.experiment, Experiment::Draft(_)));
        assert_eq!(sweep.experiment.row().state, ExperimentState::Draft);

        // **Now it runs.** Section 2.9 has an arm that frees nothing produce
        // one run, and read four returned no arms for it until 2026-09-11,
        // which left that run reachable only by a reader who already had its
        // identity.
        //
        // Perturbation: return `Vec::new()` for an experiment with no swept
        // member and this run is unreachable again.
        let run = format!(
            "point-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        seed_run(&s, &run, None, None).await;
        sqlx::query("INSERT INTO staged_experiment_run (experiment_id, run_id) VALUES ($1, $2)")
            .bind(id)
            .bind(&run)
            .execute(&s.pool)
            .await
            .unwrap();

        let sweep = s.sweep(id).await.unwrap().unwrap();
        assert_eq!(sweep.arms.len(), 1, "the point arm's one run");
        let arm = &sweep.arms[0];
        assert_eq!(
            arm.run.as_ref().expect("the run is returned").run.0,
            run,
            "and it is the run this experiment produced"
        );
        // **Absent and not null.** A point arm was produced under no value,
        // which is a different fact from a sweep whose value is JSON null,
        // and section 6 has the view name the absence rather than draw it.
        assert!(
            arm.value.is_none(),
            "the value is absent rather than empty: {:?}",
            arm.value
        );

        // **A value the store holds is reported and not overwritten.**
        // Migration 0002 constrains the experiment's member against its set
        // and says nothing about a produced run's value, so a row carrying
        // one is reachable. Perturbation: bind the value to `_` in the point
        // branch and this reads absent while the store holds a number.
        sqlx::query("UPDATE staged_experiment_run SET swept_value = '42'::jsonb WHERE run_id = $1")
            .bind(&run)
            .execute(&s.pool)
            .await
            .unwrap();
        let sweep = s.sweep(id).await.unwrap().unwrap();
        assert_eq!(
            sweep.arms[0].value.as_ref().and_then(|v| v.as_i64()),
            Some(42),
            "the value the store holds is the value the read returns"
        );

        assert!(s.sweep(i64::MAX).await.unwrap().is_none());
    }
}
