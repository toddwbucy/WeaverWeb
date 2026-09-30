//! conforms: web-nothing-is-computed-at-read-time-unless-the-query-is-recorded
//! conforms: web-an-authored-identity-says-what-it-addresses
//!
//! **The sixth read of `weaver-web-Spec` section 4: one plan whole.**
//!
//! Section 4 named this read owed rather than discovered - "section 2.9's
//! plan is rendered by the matrix with its arms and their entries, and
//! none of these five returns it, so that read is owed at the act that
//! gives the plan its schema." This is that act, and the read lands with
//! the schema rather than after the surface that wants it, which is the
//! rule section 4 states about itself working a third time.
//!
//! **It derives nothing.** An arm's status is not computed here: section
//! 5.1 has the arm take the five states of the staged experiment it
//! became, so the status is that row's `state` read across the reference,
//! and an arm whose reference is null has not been registered - which the
//! null itself records rather than a sixth word this document would have to
//! name.
//!
//! **An arm is not a run.** Section 2.9 has an arm reach its runs
//! through its staged experiment and never directly, so nothing here joins
//! an arm to `run`.

use std::collections::HashMap;

use serde::Serialize;
use sqlx::Row as _;

use super::{ArmId, ExperimentState, PlanId, Store};

/// What an operator is still composing: one parent run, and the arms
/// each of which becomes at most one staged experiment.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    /// Spelled `pl-` and sixteen hex, per migration 0009.
    pub plan: PlanId,
    /// The run every arm branches from, per section 2.9.
    pub parent_run: String,
    /// Per section 3.2, and null where the store could not name one. **It
    /// never means the operator.**
    pub author: Option<String>,
    pub version: i64,
    pub arms: Vec<Arm>,
}

/// One arm of a plan, with the disposition of every member it names: a
/// candidate experiment, which the matrix of the sketch draws as one
/// column of its grid.
///
/// **The same arm as `experiment::Arm`, at the resolution the plan holds it
/// at.** This one is the arm as composed, before registration freezes it;
/// that one is the arm as fanned, one row per value of the frozen set with
/// the run it produced. The module path is the resolution, which is why
/// neither type is renamed to keep them apart.
///
/// **An arm that frees nothing does not fan**, per section 2.9: it registers
/// a staged experiment with no swept member and produces one run. So the
/// finer resolution is empty there rather than singular, and the arm is
/// simply this row. That is what a resolution does at its limit, and not a
/// second kind of thing.
#[derive(Debug, Clone, Serialize)]
pub struct Arm {
    /// **The arm's identity, which its name is not.** The name is the
    /// operator's label and changes when they relabel it; this does not.
    /// Spelled `ar-` and sixteen hex, which says both what it addresses and
    /// that this crate authored the row rather than receiving it.
    pub arm: ArmId,
    /// The operator's word for this arm, unique within its plan.
    pub name: String,
    /// **The registration is one fact and not two nullable members.** A
    /// arm that became a staged experiment has that row's identity and
    /// that row's state together, and an arm that did not has neither, so
    /// an experiment without a status and a status without an experiment are
    /// states section 5.1 does not admit and this type cannot hold. It is
    /// the argument `Disposition` below makes, applied to the arm.
    pub registration: Option<Registration>,
    pub entries: Vec<Entry>,
}

/// What an arm became, where it has been registered.
#[derive(Debug, Clone, Serialize)]
pub struct Registration {
    pub experiment: i64,
    /// **The arm's status is the staged experiment's state**, per section
    /// 5.1, which is why this document names no sixth vocabulary for the
    /// matrix to render.
    pub status: ExperimentState,
}

/// One member of the tuple, as this arm disposes of it.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub member: String,
    pub disposition: Disposition,
}

/// **Held or freed, and the value travels with the disposition.** Section
/// 2.9 has the value where a member is held and the value set where it is
/// freed, and the two are never both present, so they are arms of one thing
/// rather than two nullable members a reader has to correlate.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "disposition")]
pub enum Disposition {
    Held { value: serde_json::Value },
    Freed { values: Vec<serde_json::Value> },
}

/// The plan, its arms, and each arm's entries. `None` where no plan
/// carries this identity.
///
/// **The arms come back by key in byte order, which is not the order they
/// were authored.** Section 2.9 gives a plan its arms and says nothing
/// about their order, so the store records none and this read cannot return
/// one; what it can do is return the same order on every box, which an ICU
/// collation does not - `col-10` sorts before `col-2` and punctuation is
/// ignored, so two boxes with different collations render one plan two ways.
/// The `COLLATE "C"` is what makes the order a fact rather than a setting.
/// **The store holds no order to return**, section 2.9 recording none.
/// Whether it should - and whether the order a batch executes in is the
/// order the matrix shows - is open at issue #549. What this read owes
/// either way is that every box return the same order, which is what the
/// collation buys and nothing more.
///
/// **Three statements under one snapshot, each an index hit**: the plan by
/// its key, the arms by the plan's, and the entries by the plan's.
///
/// **The snapshot and not the transaction is what makes it one read.** At
/// read committed - this server's default - every statement takes a fresh
/// snapshot, so wrapping the three in a transaction leaves them three reads
/// and an arm committed between the second and the third is visible to one
/// statement and not the other. The isolation is raised to repeatable read,
/// which takes the snapshot at the first statement and holds it.
///
/// It is stated as three rather than one join because the join returns the
/// plan's row once per entry and the caller rebuilds this shape anyway.
const SELECT_PLAN: &str =
    "SELECT plan_id, parent_run_id, author, version FROM plan WHERE plan_id = $1";

const SELECT_ARMS: &str = "SELECT a.arm_id, a.name, a.experiment_id, e.state \
     FROM plan_arm a LEFT JOIN staged_experiment e ON e.experiment_id = a.experiment_id \
     WHERE a.plan_id = $1 ORDER BY a.name COLLATE \"C\"";

const SELECT_ENTRIES: &str = "SELECT e.arm_id, e.member, e.disposition, e.held_value, \
     e.freed_values FROM plan_entry e JOIN plan_arm a ON a.arm_id = e.arm_id \
     WHERE a.plan_id = $1 ORDER BY a.name COLLATE \"C\", e.member COLLATE \"C\"";

impl Store {
    /// Read one plan whole, per section 4's sixth read.
    pub async fn plan(&self, plan: &PlanId) -> anyhow::Result<Option<Plan>> {
        let mut tx = self.pool.begin().await?;
        // **A transaction is not a snapshot at read committed**, which is
        // this server's default: there, every statement takes a new one, so
        // three statements in one transaction are still three snapshots and
        // an arm committed between the second and the third is visible to
        // one and not the other. Repeatable read takes the snapshot once, at
        // the first statement, and the three become one read.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await?;

        let Some(row) = sqlx::query(SELECT_PLAN)
            .bind(plan.as_str())
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(None);
        };

        let mut arms: Vec<Arm> = sqlx::query(SELECT_ARMS)
            .bind(plan.as_str())
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| {
                // The two arrive together or not at all: the join is on the
                // arm's reference, so a state with no experiment is not a
                // row this query can produce.
                let status: Option<String> = r.get("state");
                let registration = match (r.get::<Option<i64>, _>("experiment_id"), status) {
                    (Some(experiment), Some(state)) => Some(Registration {
                        experiment,
                        // Section 5.1's five and no sixth. Read four refuses
                        // a state the five do not name, and a read that
                        // passed one through would render to the matrix what
                        // the other read would not return.
                        status: ExperimentState::parse(&state).ok_or_else(|| {
                            anyhow::anyhow!(
                                "staged_experiment {experiment} holds a state the five do not \
                                 name: {state}"
                            )
                        })?,
                    }),
                    (None, None) => None,
                    (experiment, state) => anyhow::bail!(
                        "an arm is registered or it is not: experiment {experiment:?} with \
                         state {state:?}"
                    ),
                };
                Ok(Arm {
                    arm: r.get::<String, _>("arm_id").parse().map_err(|e| {
                        anyhow::anyhow!("the store holds an arm identity it cannot spell: {e}")
                    })?,
                    name: r.get("name"),
                    registration,
                    entries: Vec::new(),
                })
            })
            .collect::<anyhow::Result<_>>()?;

        // Owned keys: the map outlives the loop that pushes into `arms`,
        // and a borrowed key would hold the vector immutably while it does.
        let at: HashMap<ArmId, usize> = arms
            .iter()
            .enumerate()
            .map(|(i, a)| (a.arm.clone(), i))
            .collect();

        for r in sqlx::query(SELECT_ENTRIES)
            .bind(plan.as_str())
            .fetch_all(&mut *tx)
            .await?
        {
            let arm: ArmId = r.get::<String, _>("arm_id").parse().map_err(|e| {
                anyhow::anyhow!("the store holds an arm identity it cannot spell: {e}")
            })?;
            let disposition: String = r.get("disposition");
            // The schema's own check holds that a held entry carries a value
            // and a freed one carries a set, so an arm that found neither
            // would be a row the store should not be able to hold. It is an
            // error here rather than a default, a default being the
            // absent-not-empty failure moved into the reader.
            let disposition = match disposition.as_str() {
                "held" => Disposition::Held {
                    value: r
                        .try_get::<Option<serde_json::Value>, _>("held_value")?
                        .filter(|v| !v.is_null())
                        .ok_or_else(|| anyhow::anyhow!("a held entry with no value: {arm}"))?,
                },
                "freed" => {
                    let values = r
                        .try_get::<Option<serde_json::Value>, _>("freed_values")?
                        .ok_or_else(|| anyhow::anyhow!("a freed entry with no set: {arm}"))?;
                    Disposition::Freed {
                        values: match values {
                            serde_json::Value::Array(v) if !v.is_empty() => v,
                            other => anyhow::bail!("a freed entry's set is not a set: {other}"),
                        },
                    }
                }
                other => anyhow::bail!("a disposition the schema does not admit: {other}"),
            };
            // **An entry whose arm is absent is an error and not a
            // silence.** Inside one transaction it cannot happen, which is
            // the point: if it ever does, the transaction is not holding and
            // a plan would come back quietly missing entries.
            let Some(&i) = at.get(&arm) else {
                anyhow::bail!("an entry names arm {arm}, which this plan's read did not return");
            };
            arms[i].entries.push(Entry {
                member: r.get("member"),
                disposition,
            });
        }

        Ok(Some(Plan {
            plan: row.get::<String, _>("plan_id").parse().map_err(|e| {
                anyhow::anyhow!("the store holds a plan identity it cannot spell: {e}")
            })?,
            parent_run: row.get("parent_run_id"),
            author: row.get("author"),
            version: row.get("version"),
            arms,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::read::tests::store;

    /// Seed a plan whose parent is a run of this invocation's own, and hand
    /// back the plan's key. **The rows are this run's alone**: the watches
    /// below count arms and entries, so anything else carrying the same
    /// plan would be counted as though this read returned it.
    async fn a_plan(s: &Store, tag: &str) -> PlanId {
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, sampler, boundary_set) \
             VALUES ($1, 'PLAN-REC', '{}', '[]')",
        )
        .bind(tag)
        .execute(&s.pool)
        .await
        .unwrap();
        sqlx::query_scalar::<_, String>(
            "INSERT INTO plan (parent_run_id, author) VALUES ($1, $2) RETURNING plan_id",
        )
        .bind(tag)
        .bind("todd")
        .fetch_one(&s.pool)
        .await
        .unwrap()
        .parse()
        .unwrap()
    }

    fn tag(prefix: &str) -> String {
        format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    /// The sixth read returns a plan with its arms and their entries, and
    /// **an arm's status is the staged experiment's state or is absent**,
    /// per section 5.1.
    ///
    /// conforms: web-nothing-is-computed-at-read-time-unless-the-query-is-recorded
    #[tokio::test]
    async fn read_six_returns_the_plan_with_its_arms_and_their_entries() {
        let Some(s) = store().await else { return };
        let tag = tag("plan");
        let plan = a_plan(&s, &tag).await;
        let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recorded_query")
            .fetch_one(&s.pool)
            .await
            .unwrap();

        // One arm registered, one not. The registered one is what makes
        // the status a read across the reference rather than a member.
        let experiment: i64 = sqlx::query_scalar(
            "INSERT INTO staged_experiment (state, question, parent_run_id) \
             VALUES ('registered', 'does the sampler carry it?', $1) RETURNING experiment_id",
        )
        .bind(&tag)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let arm_a: String = sqlx::query_scalar(
            "INSERT INTO plan_arm (plan_id, name, experiment_id) VALUES ($1, 'arm-a', $2) \
             RETURNING arm_id",
        )
        .bind(plan.as_str())
        .bind(experiment)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let arm_b: String = sqlx::query_scalar(
            "INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'arm-b') RETURNING arm_id",
        )
        .bind(plan.as_str())
        .fetch_one(&s.pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO plan_entry (arm_id, member, disposition, held_value, freed_values) \
             VALUES ($1, 'sampler', 'freed', NULL, '[{\"top_p\": 0.9}, {\"top_p\": 0.95}]'), \
                    ($1, 'device', 'held', '\"cuda:0\"', NULL), \
                    ($2, 'device', 'held', '\"cuda:1\"', NULL)",
        )
        .bind(arm_a)
        .bind(arm_b)
        .execute(&s.pool)
        .await
        .unwrap();

        let read = s.plan(&plan).await.unwrap().expect("the plan is returned");
        assert_eq!(read.parent_run, tag, "the run every arm branches from");
        assert_eq!(read.author.as_deref(), Some("todd"));
        assert_eq!(read.arms.len(), 2, "both arms, registered or not");

        let a = &read.arms[0];
        assert_eq!(a.name, "arm-a");
        let registered = a.registration.as_ref().expect("arm-a is registered");
        assert_eq!(registered.experiment, experiment);
        // Perturbation: read the status from the arm's own row and there
        // is nothing to read, the arm having no state of its own. Drop
        // the join and the registration is absent while the arm is
        // registered.
        assert_eq!(
            registered.status,
            ExperimentState::Registered,
            "the arm's status is the experiment's state"
        );
        assert_eq!(a.entries.len(), 2, "both members this arm names");

        let b = &read.arms[1];
        assert_eq!(b.name, "arm-b");
        // **A null reference records that the arm was never registered**,
        // which is why no sixth word is named for it - and the identity and
        // the status go absent together because they are one member.
        assert!(
            b.registration.is_none(),
            "an unregistered arm carries no registration"
        );

        // The value travels with the disposition rather than beside it.
        let freed = a
            .entries
            .iter()
            .find(|e| e.member == "sampler")
            .expect("the freed member");
        match &freed.disposition {
            Disposition::Freed { values } => {
                assert_eq!(values.len(), 2, "the set registration would freeze")
            }
            other => panic!("the sampler is freed, not {other:?}"),
        }
        let held = a
            .entries
            .iter()
            .find(|e| e.member == "device")
            .expect("the held member");
        match &held.disposition {
            Disposition::Held { value } => assert_eq!(value, "cuda:0"),
            other => panic!("the device is held, not {other:?}"),
        }

        assert!(
            s.plan(&"pl-0000000000000000".parse().unwrap())
                .await
                .unwrap()
                .is_none(),
            "a plan nobody authored is absent rather than empty"
        );

        // **It records nothing**, watched as read five's claim is: a delta
        // across the read and not a count of the table, section 2.6 being a
        // table other writers may legitimately fill. Perturbation: record a
        // section 2.6 row per plan read and this rises.
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recorded_query")
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(after, recorded, "a plan is read rather than quoted");
    }

    /// **The three statements are one snapshot**, so an arm committed
    /// while the read is in flight is invisible to all three rather than to
    /// some of them.
    ///
    /// This replays the read's own statements at its own isolation with a
    /// concurrent writer committing in between, which is the interleaving
    /// `plan()` cannot be made to take on command. **Perturbation: drop the
    /// `SET TRANSACTION ISOLATION LEVEL` and this fails** - at read
    /// committed the second statement sees the writer's row and the first
    /// did not, which is the torn read the transaction alone does not
    /// prevent.
    #[tokio::test]
    async fn the_plan_is_read_under_one_snapshot() {
        let Some(s) = store().await else { return };
        let tag = tag("snap");
        let plan = a_plan(&s, &tag).await;
        sqlx::query("INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'arm-a')")
            .bind(plan.as_str())
            .execute(&s.pool)
            .await
            .unwrap();

        let mut tx = s.pool.begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await
            .unwrap();
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plan_arm WHERE plan_id = $1")
            .bind(plan.as_str())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(before, 1, "one arm when the snapshot was taken");

        // Another connection entirely, committing between the statements.
        sqlx::query("INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'arm-b')")
            .bind(plan.as_str())
            .execute(&s.pool)
            .await
            .expect("the writer commits");

        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plan_arm WHERE plan_id = $1")
            .bind(plan.as_str())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            after, before,
            "the snapshot holds: the read sees the plan as it was, not half of two"
        );
        tx.rollback().await.unwrap();

        // And the writer's arm is really there, so the watch is about
        // the snapshot rather than about a write that never landed.
        let now: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM plan_arm WHERE plan_id = $1")
            .bind(plan.as_str())
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(now, 2, "outside the snapshot both arms stand");
    }

    /// **An identity says what it addresses and that this crate authored
    /// it**, per migration 0009: two letters and sixteen hex, generated by
    /// the store and held to shape.
    ///
    /// Perturbation: drop a `*_is_shaped` check and a plan, an arm or a ref
    /// takes an identity of any spelling, including one belonging to another
    /// kind of row - which is the confusion the prefix exists to refuse and
    /// which a bare integer key could not refuse at all.
    #[tokio::test]
    async fn an_identity_says_what_it_addresses() {
        let Some(s) = store().await else { return };
        let tag = tag("shape");
        let plan = a_plan(&s, &tag).await;
        assert!(
            plan.as_str().starts_with("pl-") && plan.as_str().len() == 19,
            "the plan's identity is spelled: {plan}"
        );

        let arm: String = sqlx::query_scalar(
            "INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'arm') RETURNING arm_id",
        )
        .bind(plan.as_str())
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert!(
            arm.starts_with("ar-") && arm.len() == 19,
            "the arm's: {arm}"
        );

        let reference: String = sqlx::query_scalar(
            "INSERT INTO ref (name, run_id, author) VALUES ('kept', $1, NULL) RETURNING ref_id",
        )
        .bind(&tag)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        assert!(
            reference.starts_with("rf-") && reference.len() == 19,
            "the ref's: {reference}"
        );

        // **Each kind's shape is held, and each is watched.** One negative
        // case per domain: a spelling belonging to another kind offered
        // where this one is owed. A watch that exercised only the plan's
        // would stay green with the other two domains dropped, which is a
        // watch for one check standing in for three.
        //
        // **What this holds is the kind and not the provenance.** A text key
        // with a default is one a writer may supply - these inserts supply
        // one - where the sequence it replaced could be overridden only
        // explicitly. The domain refuses the wrong kind, not a hand-written
        // key of the right one.
        for (what, sql, wrong) in [
            (
                "a plan",
                "INSERT INTO plan (plan_id, parent_run_id, author) VALUES ($1, $2, NULL)",
                arm.clone(),
            ),
            (
                "an arm",
                "INSERT INTO plan_arm (arm_id, plan_id, name) VALUES ($1, $2, 'x')",
                reference.clone(),
            ),
            (
                "a ref",
                "INSERT INTO ref (ref_id, name, run_id, author) VALUES ($1, 'x', $2, NULL)",
                plan.as_str().to_owned(),
            ),
        ] {
            let refused = sqlx::query(sql)
                .bind(&wrong)
                .bind(if what == "an arm" {
                    plan.as_str()
                } else {
                    tag.as_str()
                })
                .execute(&s.pool)
                .await
                .expect_err("the wrong kind is refused");
            assert!(
                refused.to_string().contains("violates check constraint"),
                "{what} took {wrong}: {refused}"
            );
        }

        // And the run it names keeps the identity the record spelled, which
        // is the other half of the convention: a bare spelling is a row this
        // crate received rather than authored.
        let named: String = sqlx::query_scalar("SELECT run_id FROM ref WHERE ref_id = $1")
            .bind(&reference)
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(
            named, tag,
            "the run's identity is the record's and not ours"
        );
    }

    /// **A ref names a run and keeps it**, per section 2.10, and this is the
    /// half of the act that had no watch at all until the review of PR #547.
    ///
    /// It also pins that `ref` survives unquoted. The word is reserved in
    /// the SQL standard and not in Postgres, which is a fact about this
    /// engine that a test should hold rather than a fact a reader should
    /// have to know.
    #[tokio::test]
    async fn a_ref_names_a_run_and_carries_its_author() {
        let Some(s) = store().await else { return };
        let tag = tag("ref");
        a_plan(&s, &tag).await;
        sqlx::query("INSERT INTO ref (name, run_id, author) VALUES ($1, $2, $3)")
            .bind("the bound cites this one")
            .bind(&tag)
            .bind("todd")
            .execute(&s.pool)
            .await
            .expect("a ref lands");
        let (name, author, made): (String, Option<String>, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT name, author, made_at FROM ref WHERE run_id = $1")
                .bind(&tag)
                .fetch_one(&s.pool)
                .await
                .unwrap();
        assert_eq!(name, "the bound cites this one");
        assert_eq!(author.as_deref(), Some("todd"));
        assert!(made.timestamp() > 0, "a ref records when it was made");

        // **A ref pins a run that exists.** Section 2.10 has a ref be how a
        // run says it must survive, and a name pointing at no run would make
        // reachability answer for a root that is not there.
        let dangling = sqlx::query("INSERT INTO ref (name, run_id, author) VALUES ($1, $2, NULL)")
            .bind("nothing")
            .bind("no-such-run")
            .execute(&s.pool)
            .await
            .expect_err("a ref to no run is refused");
        assert!(
            dangling.to_string().contains("ref_run_id_fkey"),
            "refused by the wrong rule: {dangling}"
        );
    }

    /// **An arm frees at most one member**, per section 2.9, section 5.4
    /// having a sweep name one member and its value set.
    ///
    /// Perturbation: drop `plan_arm_frees_at_most_one_member` and a
    /// arm frees two, which is a sweep of a set the sweep's own row
    /// cannot carry.
    ///
    /// conforms: web-an-arm-frees-at-most-one-member
    #[tokio::test]
    async fn an_arm_frees_at_most_one_member() {
        let Some(s) = store().await else { return };
        let tag = tag("free");
        let plan = a_plan(&s, &tag).await;
        let arm: String = sqlx::query_scalar(
            "INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'arm') RETURNING arm_id",
        )
        .bind(plan.as_str())
        .fetch_one(&s.pool)
        .await
        .unwrap();
        let free = |member: &'static str| {
            let pool = s.pool.clone();
            let arm = arm.clone();
            async move {
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition, freed_values) \
                     VALUES ($1, $2, 'freed', '[1, 2]')",
                )
                .bind(arm)
                .bind(member)
                .execute(&pool)
                .await
            }
        };
        free("sampler").await.expect("the first freed member lands");
        let second = free("device").await.expect_err("the second is refused");
        // **Named, not merely refused.** A watch that accepts any error
        // passes when a typo, a NOT NULL or the composite foreign key
        // refuses instead, which is a watch for the database being reachable
        // rather than for the bound.
        assert!(
            second
                .to_string()
                .contains("plan_arm_frees_at_most_one_member"),
            "refused by the wrong rule: {second}"
        );

        // An arm holding the rest is the ordinary case and is not touched
        // by the bound: the index is partial on the freed disposition.
        sqlx::query(
            "INSERT INTO plan_entry (arm_id, member, disposition, held_value) \
             VALUES ($1, 'device', 'held', '\"cuda:0\"'), \
                    ($1, 'engine', 'held', '{}')",
        )
        .bind(arm)
        .execute(&s.pool)
        .await
        .expect("held members are unbounded");
    }

    /// **An arm registers at most once**, per section 2.9, the reference
    /// being what holds the claim: two arms cannot reach one staged
    /// experiment.
    ///
    /// This is the schema's half of `web-a-arm-registers-at-most-once`.
    /// **The whole of that assertion is the registration write**, which
    /// section 5.1 has set the reference only where it was null and in the
    /// same transaction as the staged experiment, and which lands with
    /// Stage. Section 9's perturbation - register a plan twice - needs that
    /// write to exist before it can be run, so what is watched here is the
    /// constraint the write will lean on and not the write.
    #[tokio::test]
    async fn two_arms_cannot_claim_one_staged_experiment() {
        let Some(s) = store().await else { return };
        let tag = tag("claim");
        let plan = a_plan(&s, &tag).await;
        let experiment: i64 = sqlx::query_scalar(
            "INSERT INTO staged_experiment (state, question, parent_run_id) \
             VALUES ('registered', 'twice?', $1) RETURNING experiment_id",
        )
        .bind(&tag)
        .fetch_one(&s.pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO plan_arm (plan_id, name, experiment_id) VALUES ($1, 'a', $2)")
            .bind(plan.as_str())
            .bind(experiment)
            .execute(&s.pool)
            .await
            .expect("the first arm claims it");
        let second =
            sqlx::query("INSERT INTO plan_arm (plan_id, name, experiment_id) VALUES ($1, 'b', $2)")
                .bind(plan.as_str())
                .bind(experiment)
                .execute(&s.pool)
                .await
                .expect_err("the second is refused");
        assert!(
            second.to_string().contains("plan_arm_experiment_id_key"),
            "refused by the wrong rule: {second}"
        );

        // Two unregistered arms are the ordinary case: the uniqueness is
        // over the reference and nulls do not collide.
        sqlx::query("INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'c'), ($1, 'd')")
            .bind(plan.as_str())
            .execute(&s.pool)
            .await
            .expect("unregistered arms do not collide");
    }

    /// **An entry states the value its disposition names**, per section 2.9,
    /// so a held entry with no value and a freed entry with no set are rows
    /// the store cannot hold.
    ///
    /// Perturbation: drop the check and the read above meets an entry whose
    /// disposition says held and whose value is absent, which is the
    /// absent-not-empty failure moved into the store.
    ///
    /// conforms: web-entry-states-the-value-its-disposition-names
    #[tokio::test]
    async fn an_entry_states_the_value_its_disposition_names() {
        let Some(s) = store().await else { return };
        let tag = tag("state");
        let plan = a_plan(&s, &tag).await;
        let arm: String = sqlx::query_scalar(
            "INSERT INTO plan_arm (plan_id, name) VALUES ($1, 'arm') RETURNING arm_id",
        )
        .bind(plan.as_str())
        .fetch_one(&s.pool)
        .await
        .unwrap();
        // **Five statements and not one built from a string.** sqlx
        // refuses a dynamic query for the reason this crate agrees with,
        // and a watch that reached for `AssertSqlSafe` to say five things
        // would be spending that refusal on its own convenience.
        // Each case names the rule that must refuse it. Three different
        // constraints back these five, so an assertion that only asked for
        // an error could not tell the value check from the disposition
        // check, nor either from an arm name typed wrong.
        for (member, rule, refused) in [
            (
                "held-with-no-value",
                "plan_entry_states_the_value_its_disposition_names",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition) \
                     VALUES ($1, $2, 'held')",
                ),
            ),
            (
                "freed-with-no-set",
                "plan_entry_states_the_value_its_disposition_names",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition) \
                     VALUES ($1, $2, 'freed')",
                ),
            ),
            (
                "held-carrying-a-set",
                "plan_entry_states_the_value_its_disposition_names",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition, held_value, \
                     freed_values) VALUES ($1, $2, 'held', '1', '[1]')",
                ),
            ),
            (
                "freed-whose-set-is-not-an-array",
                "plan_entry_freed_values_is_an_array",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition, freed_values) \
                     VALUES ($1, $2, 'freed', '1')",
                ),
            ),
            (
                "a-disposition-of-its-own",
                "plan_entry_disposition_is_held_or_freed",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition, held_value) \
                     VALUES ($1, $2, 'moved', '1')",
                ),
            ),
            // **JSON null is not SQL NULL**, and a check written as IS NOT
            // NULL admits the one spelling of no-value it exists to refuse.
            (
                "held-whose-value-is-json-null",
                "plan_entry_states_the_value_its_disposition_names",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition, held_value) \
                     VALUES ($1, $2, 'held', 'null')",
                ),
            ),
            // A sweep is one member **and its value set**, so a freed member
            // over no values would register an arm with no arms.
            (
                "freed-over-no-values",
                "plan_entry_freed_values_is_an_array",
                sqlx::query(
                    "INSERT INTO plan_entry (arm_id, member, disposition, freed_values) \
                     VALUES ($1, $2, 'freed', '[]')",
                ),
            ),
        ] {
            let outcome = refused
                .bind(&arm)
                .bind(member)
                .execute(&s.pool)
                .await
                .expect_err("the store refuses it");
            assert!(
                outcome.to_string().contains(rule),
                "{member} was refused by the wrong rule: {outcome}"
            );
        }
    }
}
