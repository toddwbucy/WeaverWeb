//! conforms: web-ingest-is-idempotent-on-the-key
//! conforms: web-generation-summary-lands
//! conforms: web-position-is-stored-at-ingest
//! conforms: web-record-members-are-absent-where-unsent
//!
//! **The ingest of `weaver-web-Spec` section 3.1**: one `weaver-analysis
//! signals` emission landed in the recorded half, under the rulings of
//! 2026-10-06 in `docs/project/brief-2026-10-06-act-10-the-replay-surface.md`.
//! `emission.rs` reads and plans, `rows.rs` defines the rows, and this file
//! writes.
//!
//! **One ingest writes a run at a time; the rest wait and replay.** The
//! ingest first takes a lock for every run it will write or resolve against,
//! and holds them to its end, so every step below runs with no other writer
//! on its runs: a second ingest of one run waits, then meets a finished row
//! and replays it. Then three passes:
//!
//! 1. **Each run lands**, in the emission's order: its row first, `writing`,
//!    then its generations and points, one transaction per generation. A run
//!    that is not a branch closes right after, `whole` or `short`. A run the
//!    store already holds is a replay: compared whole, equal a no-op,
//!    different refused in the answer alone with the store untouched; a
//!    replayed `writing` row whose every key is equal is completed.
//! 2. **Cycles among the emission's branches are refused**, one transaction
//!    per cycle, persisted only on the rows this ingest created.
//! 3. **Each other branch resolves and closes in one transaction**, parents
//!    before children: the link where the parent is held, the walk where it
//!    is also whole, and the close. An ingest that dies before leaves the
//!    branch `writing`, never `whole` with its parent unresolved.

use std::collections::{HashMap, HashSet};

use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection, Postgres, Row, Transaction};

use super::Store;
use super::emission::{Emission, RunPlan, Unreadable};
use super::rows::{
    GenerationRow, INSERT_GENERATION, INSERT_POSITIONS, INSERT_RUN, PositionRow, RunMembers,
    SELECT_GENERATIONS, SELECT_POSITIONS, SELECT_RUN,
};

/// **Where a test may stop the ingest**, as a process dying there would:
/// the step hook of the brief's rulings 27 and 30. A real kill has no
/// deterministic place to land; this does, and it exists only in a test
/// build.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// After every run's points are written and the non-branches closed,
    /// before the cycles and the resolution.
    AfterPoints,
    /// After one cycle row's refusal is written inside the cycle's
    /// transaction, before the next row's and before the commit.
    AfterCycleRow,
}

/// **A test's hold on an ingest at a named point**: it says so on `locked`
/// and waits on `release`, so a test can run a second ingest beside it.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct Hold {
    pub at: HoldAt,
    pub locked: std::sync::Arc<tokio::sync::Notify>,
    pub release: std::sync::Arc<tokio::sync::Notify>,
}

/// Where a test's hold stops its ingest, its locks held throughout.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldAt {
    /// Once the locks are taken, before anything is read or written.
    AfterLocks,
    /// After the cycle scan, before the branches resolve.
    BeforeResolve,
    /// After the references are read and the cycles found, before the
    /// cycles' refusals.
    BeforeCycles,
}

#[cfg(test)]
impl Options {
    async fn hold(&self, at: HoldAt) {
        if let Some(hold) = &self.hold
            && hold.at == at
        {
            hold.locked.notify_one();
            hold.release.notified().await;
        }
    }
}

/// The ingest's options: a test's stop and hold, and nothing in a release
/// build.
#[derive(Debug, Default, Clone)]
pub struct Options {
    #[cfg(test)]
    pub stop_at: Option<Step>,
    #[cfg(test)]
    pub hold: Option<Hold>,
}

/// How one run came out, which the answer reports.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunOutcome {
    pub run: String,
    /// `whole`, `short` or `refused`, or the stored status where a replay
    /// changed nothing.
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Whether the store held the run before this ingest.
    pub replayed: bool,
    /// The status the store holds after this ingest, where it differs from
    /// `status`: a replay refused in the answer alone leaves it as it was.
    /// **For a row this ingest did not create and refused on a cycle, it is
    /// the status read inside the refusal's own transaction**, which that
    /// row's own writer may move afterwards: a row owned elsewhere has no
    /// other truth this ingest can give.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored: Option<String>,
    /// The positions and generations the store holds for the run as this
    /// ingest left it: what it wrote, or, for a run it found already
    /// written equal, what is stored. Never the plan's counts where they
    /// did not land.
    pub positions: usize,
    pub generations: usize,
    pub absent: Vec<String>,
    /// Members that crossed and have no column to land in.
    pub not_stored: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_reference: Option<String>,
    pub parent_linked: bool,
    pub parting_known: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parting_position: Option<i32>,
}

impl RunOutcome {
    fn refused(run: &str, reason: String) -> Self {
        Self {
            run: run.to_owned(),
            status: "refused".into(),
            reason: Some(reason),
            replayed: false,
            stored: None,
            positions: 0,
            generations: 0,
            absent: Vec::new(),
            not_stored: Vec::new(),
            parent_reference: None,
            parent_linked: false,
            parting_known: false,
            parting_position: None,
        }
    }

    fn landed(run: &str, plan: &RunPlan, replayed: bool) -> Self {
        Self {
            run: run.to_owned(),
            status: "writing".into(),
            reason: None,
            replayed,
            stored: None,
            // What this ingest wrote, counted as it writes; a refusal that
            // wrote nothing reports none.
            positions: 0,
            generations: 0,
            absent: plan.absent(),
            not_stored: if plan.verdict_crossed {
                vec!["verdict (no column until its kind lands, Spec 2.2)"]
            } else {
                Vec::new()
            },
            parent_reference: plan.members.parent_reference.clone(),
            parent_linked: false,
            parting_known: false,
            parting_position: None,
        }
    }
}

/// **The ingest's answer**: one object, the exit status agreeing, as the
/// register verbs answer. `ok` where every run is `whole` or `short`.
#[derive(Debug, Clone)]
pub struct IngestAnswer {
    pub ok: bool,
    pub value: serde_json::Value,
}

/// What the store held for a run before this ingest.
struct Stored {
    members: RunMembers,
    status: String,
    /// What the store says of the run beyond its members, which an equal
    /// replay of a closed run answers with.
    reason: Option<String>,
    linked: bool,
    parting_known: bool,
    parting_position: Option<i32>,
    generations: Vec<GenerationRow>,
    positions: HashMap<(String, i32), StoredPosition>,
}

/// A stored position, with whether the members this seam never fills are
/// in fact empty, so a row some other writer filled reads as different.
#[derive(Debug, PartialEq)]
struct StoredPosition {
    row: PositionRow,
    unfilled: bool,
}

/// A run a cycle may run through: its parent reference, the status the
/// store holds for it, and its entry in the answer where it has one.
struct Named {
    parent: Option<String>,
    status: String,
    outcome: Option<usize>,
}

/// One run this ingest holds open as `writing`, after the first pass.
struct Open {
    run: String,
    plan: RunPlan,
    /// Whether this ingest created the row, which decides whether a refusal
    /// may be persisted on it (ruling 29).
    created: bool,
    outcome: usize,
}

/// The advisory locks' class, the first of the two keys, so they share no
/// key with any other advisory lock in the store (the listener's single
/// lock takes the one-key form, a separate space).
const LOCK_CLASS: i32 = 0x5754_4931;

/// The number of buckets a run's identity hashes into, the second key.
const LOCK_BUCKETS: i32 = 1024;

/// **The runs an ingest writes, locked for its whole length.**
///
/// The lock is **session-level, on a connection of its own that this guard
/// holds**, and not transaction-level: a run's writes are several
/// transactions, one per generation as Spec 3.1 has them, and a lock taken
/// in a transaction would end at its commit and let a second ingest in
/// between two generations. The connection is closed when the guard drops,
/// which ends the session and so releases every lock on every path out of
/// the ingest, an error or a test's stop included.
///
/// **The session is outside the store's pool**, opened from the options
/// the store was connected with. The pool is the ingest's work capacity,
/// and a session taken from it would be held by every ingest waiting on a
/// lock: as many waiters as the pool holds connections, and the lock's
/// owner could never get one to write with. Outside it, a waiter holds no
/// work capacity. **The only bound on waiters is then the box's own**, the
/// connections the database server admits, which the box owns and this
/// crate does not set.
///
/// **Keying.** Each run's identity is hashed by the store's own `hashtext`
/// into one of `LOCK_BUCKETS` buckets, under `LOCK_CLASS`; two runs sharing a
/// bucket serialize when they need not, which costs time and never
/// correctness, and an ingest holds at most `LOCK_BUCKETS` locks however many
/// runs it carries, inside PostgreSQL's shared lock table. **The buckets are
/// taken in ascending order**, every ingest taking all of its own before
/// writing anything, so two ingests over overlapping runs never deadlock:
/// the later waits for the earlier whole.
struct RunLocks {
    _held: PgConnection,
}

impl RunLocks {
    async fn take(connect: &PgConnectOptions, runs: &[&str]) -> Result<Self, sqlx::Error> {
        let mut held = PgConnection::connect_with(connect).await?;
        let buckets: Vec<i32> = sqlx::query_scalar(
            "SELECT DISTINCT hashtext(r) & $2 FROM unnest($1::text[]) AS r ORDER BY 1",
        )
        .bind(runs)
        .bind(LOCK_BUCKETS - 1)
        .fetch_all(&mut held)
        .await?;
        for bucket in buckets {
            sqlx::query("SELECT pg_advisory_lock($1, $2)")
                .bind(LOCK_CLASS)
                .bind(bucket)
                .execute(&mut held)
                .await?;
        }
        Ok(Self { _held: held })
    }
}

impl Store {
    /// **Ingest one emission**, read from `input`, and answer.
    pub async fn ingest(&self, input: impl std::io::BufRead) -> IngestAnswer {
        self.ingest_with(Emission::read(input), &Options::default())
            .await
    }

    /// As `ingest`, over an emission already read, under `options`.
    pub async fn ingest_with(
        &self,
        emission: Result<Emission, Unreadable>,
        options: &Options,
    ) -> IngestAnswer {
        let emission = match emission {
            Ok(emission) => emission,
            Err(Unreadable(why)) => {
                return IngestAnswer {
                    ok: false,
                    value: serde_json::json!({
                        "verb": "ingest", "ok": false,
                        "error": format!("the emission is refused: {why}"),
                        "runs": [],
                    }),
                };
            }
        };
        let mut outcomes = Vec::new();
        let failed = self.land(&emission, options, &mut outcomes).await.err();
        let ok = failed.is_none()
            && outcomes
                .iter()
                .all(|o| o.status == "whole" || o.status == "short");
        let mut value = serde_json::json!({
            "verb": "ingest",
            "ok": ok,
            "runs": outcomes,
        });
        if let Some(why) = failed {
            value["error"] = serde_json::Value::String(why);
        }
        IngestAnswer { ok, value }
    }

    async fn land(
        &self,
        emission: &Emission,
        options: &Options,
        outcomes: &mut Vec<RunOutcome>,
    ) -> Result<(), String> {
        let _ = options;
        let store_error = |e: sqlx::Error| format!("the store failed: {e}");
        let planned = emission.plan();
        // **The locks first**: every run this ingest will write, and every
        // parent a branch will resolve against, taken before anything is
        // read or written and held to the end (`RunLocks`).
        let mut touched: Vec<&str> = Vec::new();
        for (run, plan) in &planned {
            if let Ok(plan) = plan {
                touched.push(run);
                if let Some(parent) = &plan.members.parent_reference {
                    touched.push(parent);
                }
            }
        }
        let _locks = RunLocks::take(&self.connect, &touched)
            .await
            .map_err(store_error)?;
        #[cfg(test)]
        options.hold(HoldAt::AfterLocks).await;

        // **The first pass**: each run lands, branches left open.
        let mut open: Vec<Open> = Vec::new();
        // Every run the emission names whose row the store holds after the
        // first pass, with its parent reference: the open ones and the
        // closed ones replayed equal, which a cycle may run through.
        let mut named: HashMap<String, Named> = HashMap::new();
        for (run, planned) in planned {
            let plan = match planned {
                Ok(plan) => plan,
                Err(why) => {
                    outcomes.push(RunOutcome::refused(&run, why));
                    continue;
                }
            };
            let stored = self.stored(&run).await.map_err(store_error)?;
            let created = stored.is_none();
            let mut outcome = RunOutcome::landed(&run, &plan, !created);
            match &stored {
                None => {
                    self.create(&run, &plan).await.map_err(store_error)?;
                    for generation in &plan.generations {
                        self.write_generation(&run, &plan, generation.seq)
                            .await
                            .map_err(store_error)?;
                    }
                }
                Some(stored) => {
                    if let Err(why) = compare(&plan, stored) {
                        // **A conflicting replay changes nothing stored**
                        // (ruling 17): the refusal is the answer's alone.
                        outcome.status = "refused".into();
                        outcome.reason = Some(why);
                        outcome.stored = Some(stored.status.clone());
                        outcomes.push(outcome);
                        continue;
                    }
                    if stored.status != "writing" {
                        // **An equal replay of a closed run is a no-op that
                        // counts as written**, and answers with what the
                        // store holds for it.
                        answer_stored(&mut outcome, stored);
                        outcomes.push(outcome);
                        named.insert(
                            run.clone(),
                            Named {
                                parent: plan.members.parent_reference.clone(),
                                status: stored.status.clone(),
                                outcome: Some(outcomes.len() - 1),
                            },
                        );
                        continue;
                    }
                    // A `writing` row whose every key is equal is completed.
                    self.complete(&run, &plan, stored)
                        .await
                        .map_err(store_error)?;
                }
            }
            outcome.generations = plan.generations.len();
            outcome.positions = plan.points.len();
            if plan.members.parent_reference.is_none() {
                // **A run that is not a branch closes as soon as its points
                // are written** (ruling 27).
                let (status, reason) = plan.closing();
                sqlx::query(
                    "UPDATE run SET ingest_status = $2, ingest_reason = $3 \
                     WHERE run_id = $1 AND ingest_status = 'writing'",
                )
                .bind(&run)
                .bind(status)
                .bind(&reason)
                .execute(&self.pool)
                .await
                .map_err(store_error)?;
                outcome.status = status.into();
                outcome.reason = reason;
                outcomes.push(outcome);
            } else {
                outcomes.push(outcome);
                named.insert(
                    run.clone(),
                    Named {
                        parent: plan.members.parent_reference.clone(),
                        status: "writing".into(),
                        outcome: Some(outcomes.len() - 1),
                    },
                );
                open.push(Open {
                    run,
                    plan,
                    created,
                    outcome: outcomes.len() - 1,
                });
            }
        }
        #[cfg(test)]
        if options.stop_at == Some(Step::AfterPoints) {
            return Err("stopped by the test's hook after the points".into());
        }

        // **The second pass**: cycles. Each run names at most one parent, so
        // the graph is functional and its cycles are vertex-disjoint, one
        // cycle being one strongly connected component (ruling 30). **A cycle
        // this ingest would close runs through an open branch**, and may
        // close through any run the store holds: a closed run the emission
        // replayed, or a row the emission does not name at all. So the held
        // references reachable from the emission's are read first, one batch
        // per step of the chains, and the scan then runs over them in memory,
        // linear in the runs and references (Spec 3.1).
        // **One outcome per run**: every run the first pass answered, by its
        // identity, so a stored row the walk below reaches for a run the
        // emission named answers on that run's one outcome and never on a
        // second.
        let answered: HashMap<String, usize> = outcomes
            .iter()
            .enumerate()
            .map(|(index, o)| (o.run.clone(), index))
            .collect();
        let mut references: HashMap<String, Option<String>> = named
            .iter()
            .map(|(run, n)| (run.clone(), n.parent.clone()))
            .collect();
        let mut asked: HashSet<String> = HashSet::new();
        let mut frontier: Vec<String> = references
            .values()
            .flatten()
            .filter(|r| !references.contains_key(*r))
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        while !frontier.is_empty() {
            asked.extend(frontier.iter().cloned());
            let held = sqlx::query(
                "SELECT run_id, parent_reference, ingest_status FROM run WHERE run_id = ANY($1)",
            )
            .bind(&frontier)
            .fetch_all(&self.pool)
            .await
            .map_err(store_error)?;
            let mut next = HashSet::new();
            for row in held {
                let run: String = row.get("run_id");
                let parent: Option<String> = row.get("parent_reference");
                if let Some(parent) = &parent
                    && !references.contains_key(parent)
                    && !asked.contains(parent)
                {
                    next.insert(parent.clone());
                }
                named.entry(run.clone()).or_insert(Named {
                    parent: parent.clone(),
                    status: row.get("ingest_status"),
                    outcome: answered.get(&run).copied(),
                });
                references.insert(run, parent);
            }
            frontier = next.into_iter().collect();
        }
        let open_runs: Vec<String> = open.iter().map(|o| o.run.clone()).collect();
        let (cycles, order) = plan_resolution(&open_runs, &references);
        let in_cycle: HashSet<&str> = cycles.iter().flatten().map(String::as_str).collect();
        #[cfg(test)]
        options.hold(HoldAt::BeforeCycles).await;
        for cycle in &cycles {
            let named_cycle = format!("a reference cycle: {} -> {}", cycle.join(" -> "), cycle[0]);
            let on_cycle: HashSet<&str> = cycle.iter().map(String::as_str).collect();
            let created: HashSet<&str> = open
                .iter()
                .filter(|o| o.created && on_cycle.contains(o.run.as_str()))
                .map(|o| o.run.as_str())
                .collect();
            // **One transaction per cycle** (ruling 30), persisted only on the
            // rows this ingest created (ruling 29).
            let mut tx = self.pool.begin().await.map_err(store_error)?;
            for run in cycle.iter().filter(|r| created.contains(r.as_str())) {
                sqlx::query(
                    "UPDATE run SET ingest_status = 'refused', ingest_reason = $2 \
                     WHERE run_id = $1 AND ingest_status = 'writing'",
                )
                .bind(run)
                .bind(&named_cycle)
                .execute(&mut *tx)
                .await
                .map_err(store_error)?;
                #[cfg(test)]
                if options.stop_at == Some(Step::AfterCycleRow) {
                    drop(tx);
                    return Err("stopped by the test's hook inside a cycle's refusal".into());
                }
            }
            // **The other rows' statuses, read in the refusal's own
            // transaction**: a row this ingest did not create may lie outside
            // its locks, and its owner may have moved it since the references
            // were read. The cycle itself cannot move, a reference never
            // changing once written; only the status can, and it is read here.
            let others: Vec<&String> = cycle
                .iter()
                .filter(|r| !created.contains(r.as_str()))
                .collect();
            let statuses: HashMap<String, String> =
                sqlx::query("SELECT run_id, ingest_status FROM run WHERE run_id = ANY($1)")
                    .bind(&others)
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(store_error)?
                    .into_iter()
                    .map(|r| (r.get("run_id"), r.get("ingest_status")))
                    .collect();
            tx.commit().await.map_err(store_error)?;
            // Every other run on the cycle is reported refused in the answer
            // with the status the store keeps for it; a run the first pass
            // already refused keeps its own reason.
            for run in cycle {
                let held = &named[run];
                let index = match held.outcome {
                    Some(index) => index,
                    None => {
                        let mut outcome = RunOutcome::refused(run, named_cycle.clone());
                        outcome.replayed = true;
                        outcome.parent_reference = held.parent.clone();
                        outcomes.push(outcome);
                        outcomes.len() - 1
                    }
                };
                let outcome = &mut outcomes[index];
                if outcome.status != "refused" {
                    outcome.status = "refused".into();
                    outcome.reason = Some(named_cycle.clone());
                }
                if !created.contains(run.as_str()) {
                    outcome.stored = Some(
                        statuses
                            .get(run)
                            .cloned()
                            .unwrap_or_else(|| held.status.clone()),
                    );
                }
            }
        }

        // **The third pass**: every other branch, parents first, resolved
        // and closed in one transaction each (rulings 23, 27 and 28), in the
        // order `plan_resolution` gave. A chain that leads into a cycle
        // stops there: the cycle's rows are refused or left `writing`, never
        // whole, so its children resolve after it with their parting unknown.
        #[cfg(test)]
        options.hold(HoldAt::BeforeResolve).await;
        let by_run: HashMap<&str, &Open> = open.iter().map(|o| (o.run.as_str(), o)).collect();
        // The ends of chains this ingest's resolutions have followed, so a
        // long chain is followed once and not once per branch on it.
        let mut tails: HashMap<String, Option<String>> = HashMap::new();
        for run in order.iter().filter(|r| !in_cycle.contains(r.as_str())) {
            let o = by_run[run.as_str()];
            let resolved = self
                .resolve_retrying(o, &mut tails)
                .await
                .map_err(store_error)?;
            let outcome = &mut outcomes[o.outcome];
            outcome.status = resolved.status.into();
            outcome.reason = resolved.reason;
            outcome.stored = resolved.stored;
            outcome.parent_linked = resolved.linked;
            outcome.parting_known = resolved.parting.is_known();
            outcome.parting_position = resolved.parting.position();
        }
        Ok(())
    }

    /// What the store holds for `run`, or `None` where it holds no row.
    async fn stored(&self, run: &str) -> Result<Option<Stored>, sqlx::Error> {
        let Some(row) = sqlx::query(SELECT_RUN.as_str())
            .bind(run)
            .fetch_optional(&self.pool)
            .await?
        else {
            return Ok(None);
        };
        let generations = sqlx::query(SELECT_GENERATIONS.as_str())
            .bind(run)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(GenerationRow::read)
            .collect();
        let positions = sqlx::query(SELECT_POSITIONS.as_str())
            .bind(run)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|r| {
                let row = PositionRow::read(r);
                (
                    (row.turn.clone(), row.position),
                    StoredPosition {
                        row,
                        unfilled: r.get("unfilled"),
                    },
                )
            })
            .collect();
        Ok(Some(Stored {
            members: RunMembers::read(&row),
            status: row.get("ingest_status"),
            reason: row.get("ingest_reason"),
            linked: row.get("linked"),
            parting_known: row.get("parting_known"),
            parting_position: row.get("parting_position"),
            generations,
            positions,
        }))
    }

    /// **The run's row, first and `writing`** (Spec 3.1), every member as
    /// `RunMembers` declares it.
    async fn create(&self, run: &str, plan: &RunPlan) -> Result<(), sqlx::Error> {
        plan.members
            .bind(sqlx::query(INSERT_RUN.as_str()).bind(run))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// **One generation and its points, in one transaction**: bulk per
    /// generation and never per point (Spec 3.1), reading nothing, since no
    /// other ingest writes the run while this one holds its lock.
    async fn write_generation(
        &self,
        run: &str,
        plan: &RunPlan,
        seq: i32,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        plan.generations[seq as usize]
            .bind(sqlx::query(INSERT_GENERATION.as_str()).bind(run))
            .execute(&mut *tx)
            .await?;
        let points: Vec<&PositionRow> = plan.points[plan.spans[seq as usize].clone()]
            .iter()
            .map(|(_, p)| p)
            .collect();
        insert_points(&mut tx, run, &points).await?;
        tx.commit().await
    }

    /// **A replayed `writing` row, completed**: the generations and points
    /// it lacks, in one transaction, every stored key having compared equal.
    async fn complete(
        &self,
        run: &str,
        plan: &RunPlan,
        stored: &Stored,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let held: HashSet<i32> = stored.generations.iter().map(|g| g.seq).collect();
        for generation in plan.generations.iter().filter(|g| !held.contains(&g.seq)) {
            generation
                .bind(sqlx::query(INSERT_GENERATION.as_str()).bind(run))
                .execute(&mut *tx)
                .await?;
        }
        let missing: Vec<&PositionRow> = plan
            .points
            .iter()
            .map(|(_, p)| p)
            .filter(|p| !stored.positions.contains_key(&(p.turn.clone(), p.position)))
            .collect();
        insert_points(&mut tx, run, &missing).await?;
        tx.commit().await
    }

    /// `resolve`, retried where the store broke a deadlock: a net, since the
    /// run locks order every ingest's writes and leave none to break.
    async fn resolve_retrying(
        &self,
        open: &Open,
        tails: &mut HashMap<String, Option<String>>,
    ) -> Result<Resolved, sqlx::Error> {
        let mut tries = 0;
        loop {
            match self.resolve(open, tails).await {
                Err(sqlx::Error::Database(d))
                    if tries < 3 && matches!(d.code().as_deref(), Some("40P01" | "40001")) =>
                {
                    tries += 1;
                }
                other => return other,
            }
        }
    }

    /// **One branch, resolved and closed in one transaction**: the link
    /// where the parent is held whatever its status (ruling 18), the walk
    /// only where the parent is also whole, and the close.
    async fn resolve(
        &self,
        open: &Open,
        tails: &mut HashMap<String, Option<String>>,
    ) -> Result<Resolved, sqlx::Error> {
        let parent = open
            .plan
            .members
            .parent_reference
            .as_deref()
            .expect("only branches resolve");
        let mut tx = self.pool.begin().await?;
        let parent_status: Option<String> =
            sqlx::query_scalar("SELECT ingest_status FROM run WHERE run_id = $1 FOR SHARE")
                .bind(parent)
                .fetch_optional(&mut *tx)
                .await?;
        let linked = parent_status.is_some();
        // **The cycle, rechecked at the resolution**: an ingest whose runs
        // share no lock with this one's may have created a run on the
        // parent's chain naming this branch after this ingest's scan. The
        // chain is followed from the parent through held rows; where it
        // returns to this branch, the branch is refused by name rather than
        // closed on a cycle. Of two such ingests the later to resolve sees
        // every row the cycle needs, each having been created before its own
        // ingest resolved.
        //
        // **References never change once written**, so where a resolution
        // earlier in this ingest followed a chain, its end is remembered:
        // `None` where it ended at a run naming no parent or joined a cycle
        // elsewhere, which no new row can change, and the run it ended at,
        // not then held, otherwise, which is where this walk goes on. A long
        // chain is so followed once and not once per branch on it.
        if linked {
            let mut chain = vec![open.run.clone(), parent.to_owned()];
            let mut seen: HashSet<String> = chain.iter().cloned().collect();
            let mut visited: Vec<String> = Vec::new();
            let mut at = parent.to_owned();
            let end: Option<String> = loop {
                let next: Option<Option<String>> = match tails.get(&at) {
                    // Followed before: go on from where that walk ended.
                    Some(None) => break None,
                    Some(Some(absent)) => Some(Some(absent.clone())),
                    None => {
                        let next = sqlx::query_scalar(
                            "SELECT parent_reference FROM run WHERE run_id = $1",
                        )
                        .bind(&at)
                        .fetch_optional(&mut *tx)
                        .await?;
                        visited.push(at.clone());
                        next
                    }
                };
                let next = match next {
                    // Not held: the chain ends at `at`, which may be created
                    // later naming something.
                    None => break Some(at.clone()),
                    // Names no parent: the chain ends for good.
                    Some(None) => break None,
                    Some(Some(next)) => next,
                };
                if next == open.run {
                    let reason =
                        format!("a reference cycle: {} -> {}", chain.join(" -> "), open.run);
                    if !open.created {
                        // Not this ingest's row: the refusal is the answer's
                        // alone (ruling 29).
                        return Ok(Resolved {
                            status: "refused",
                            reason: Some(reason),
                            linked: false,
                            parting: Parting::Unknown,
                            stored: Some("writing".into()),
                        });
                    }
                    sqlx::query(
                        "UPDATE run SET ingest_status = 'refused', ingest_reason = $2 \
                         WHERE run_id = $1 AND ingest_status = 'writing'",
                    )
                    .bind(&open.run)
                    .bind(&reason)
                    .execute(&mut *tx)
                    .await?;
                    tx.commit().await?;
                    return Ok(Resolved {
                        status: "refused",
                        reason: Some(reason),
                        linked: false,
                        parting: Parting::Unknown,
                        stored: None,
                    });
                }
                if !seen.insert(next.clone()) {
                    // Joined a cycle this branch is not on.
                    break None;
                }
                chain.push(next.clone());
                at = next;
            };
            for run in visited {
                tails.insert(run, end.clone());
            }
        }
        let parting = if parent_status.as_deref() == Some("whole") {
            match parent_path(&mut tx, parent).await? {
                Some(parent_path) => walk(&open.plan, &parent_path),
                None => Parting::Unknown,
            }
        } else {
            Parting::Unknown
        };
        let (status, reason) = open.plan.closing();
        sqlx::query(
            "UPDATE run SET parent_run_id = $2, parting_position = $3, parting_known = $4, \
             ingest_status = $5, ingest_reason = $6 WHERE run_id = $1 AND ingest_status = 'writing'",
        )
        .bind(&open.run)
        .bind(linked.then_some(parent))
        .bind(parting.position())
        .bind(parting.is_known())
        .bind(status)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Resolved {
            status,
            reason,
            linked,
            parting,
            stored: None,
        })
    }
}

struct Resolved {
    status: &'static str,
    reason: Option<String>,
    linked: bool,
    parting: Parting,
    /// The stored status, where the answer's status is not the store's.
    stored: Option<String>,
}

/// An outcome answering with what the store holds for the run.
fn answer_stored(outcome: &mut RunOutcome, stored: &Stored) {
    outcome.generations = stored.generations.len();
    outcome.positions = stored.positions.len();
    outcome.status = stored.status.clone();
    outcome.reason = stored.reason.clone();
    outcome.parent_linked = stored.linked;
    outcome.parting_known = stored.parting_known;
    outcome.parting_position = stored.parting_position;
}

/// The points, in one statement over arrays, one per column as
/// `PositionRow` declares it. The text, the alternatives, the rank and the
/// residual are never written: this seam carries none.
async fn insert_points(
    tx: &mut Transaction<'_, Postgres>,
    run: &str,
    points: &[&PositionRow],
) -> Result<(), sqlx::Error> {
    if points.is_empty() {
        return Ok(());
    }
    PositionRow::bind_arrays(points, sqlx::query(INSERT_POSITIONS.as_str()).bind(run))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// **A replay compared against what is stored**, whole, before anything is
/// written (rulings 14 and 17): every member `RunMembers` declares, which is
/// every column the ingest writes; every stored generation against the
/// plan's of its order; every stored position against the plan's of its key,
/// the members this seam never fills required empty. A closed run must hold
/// exactly what the emission names, and a `writing` one may hold less. The
/// first difference is named.
fn compare(plan: &RunPlan, stored: &Stored) -> Result<(), String> {
    let refused = |what: String| {
        format!("a replay that differs from the stored run, which stands unchanged: {what}")
    };
    let differ = plan.members.differing(&stored.members);
    if !differ.is_empty() {
        return Err(refused(format!("the run's {} differs", differ.join(", "))));
    }
    for g in &stored.generations {
        match plan.generations.get(g.seq as usize) {
            Some(p) if p == g => {}
            _ => return Err(refused(format!("generation {} differs", g.seq))),
        }
    }
    let planned: HashMap<(&str, i32), &PositionRow> = plan
        .points
        .iter()
        .map(|(_, p)| ((p.turn.as_str(), p.position), p))
        .collect();
    let mut keys: Vec<&(String, i32)> = stored.positions.keys().collect();
    keys.sort();
    for key in keys {
        let held = &stored.positions[key];
        match planned.get(&(key.0.as_str(), key.1)) {
            Some(p) if **p == held.row && held.unfilled => {}
            _ => {
                return Err(refused(format!(
                    "turn {} position {} differs",
                    key.0, key.1
                )));
            }
        }
    }
    if stored.status != "writing"
        && (stored.generations.len() != plan.generations.len()
            || stored.positions.len() != planned.len())
    {
        return Err(refused(format!(
            "the stored run holds {} generations and {} positions, the emission {} and {}",
            stored.generations.len(),
            stored.positions.len(),
            plan.generations.len(),
            planned.len()
        )));
    }
    Ok(())
}

/// **The cycles the open branches are on, and the order the others resolve
/// in**, over every held run's parent reference (`references`, a run absent
/// from it not being held). Linear in the runs and references: the return
/// check is a lookup in the path's index, a walk stops where it meets a run
/// an earlier walk settled, and each depth is computed once.
///
/// The cycles are the ones an open branch is on; a cycle no open branch is
/// on was in the store before the ingest and is not its to refuse. The order
/// holds every open branch not on a cycle, parents before children over the
/// references among them, the emission's order kept between equals.
pub(crate) fn plan_resolution(
    open: &[String],
    references: &HashMap<String, Option<String>>,
) -> (Vec<Vec<String>>, Vec<String>) {
    let open_set: HashSet<&str> = open.iter().map(String::as_str).collect();
    let mut in_cycle: HashSet<&str> = HashSet::new();
    let mut settled: HashSet<&str> = HashSet::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    for start in open {
        let mut path: Vec<&str> = Vec::new();
        let mut index: HashMap<&str, usize> = HashMap::new();
        let mut at: &str = start;
        loop {
            if in_cycle.contains(at) || settled.contains(at) {
                break;
            }
            if let Some(&from) = index.get(at) {
                let cycle = &path[from..];
                if cycle.iter().any(|n| open_set.contains(n)) {
                    in_cycle.extend(cycle.iter().copied());
                    cycles.push(cycle.iter().map(|n| n.to_string()).collect());
                }
                break;
            }
            // Not held: the chain ends.
            let Some((held, parent)) = references.get_key_value(at) else {
                break;
            };
            index.insert(held, path.len());
            path.push(held);
            match parent {
                Some(parent) => at = parent,
                None => break,
            }
        }
        settled.extend(path.into_iter().filter(|n| !in_cycle.contains(n)));
    }
    // Depths among the open branches not on a cycle, each computed once.
    let parent_of = |run: &str| -> Option<&str> {
        references
            .get(run)
            .and_then(|p| p.as_deref())
            .filter(|p| open_set.contains(p) && !in_cycle.contains(p))
    };
    let mut depth: HashMap<&str, usize> = HashMap::new();
    for start in open
        .iter()
        .map(String::as_str)
        .filter(|r| !in_cycle.contains(r))
    {
        let mut chain: Vec<&str> = Vec::new();
        let mut at = start;
        let mut base = None;
        loop {
            if let Some(&d) = depth.get(at) {
                base = Some(d);
                break;
            }
            chain.push(at);
            match parent_of(at) {
                Some(parent) => at = parent,
                None => break,
            }
        }
        for run in chain.into_iter().rev() {
            let d = base.map_or(0, |d| d + 1);
            depth.insert(run, d);
            base = Some(d);
        }
    }
    let mut order: Vec<String> = open
        .iter()
        .filter(|r| !in_cycle.contains(r.as_str()))
        .cloned()
        .collect();
    order.sort_by_key(|r| depth[r.as_str()]);
    (cycles, order)
}

/// The parting position as the walk found it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parting {
    /// The paths provably part here.
    At(i32),
    /// Both paths are whole and equal through the window: never parted.
    Never,
    /// The walk could not say: not derived, a member not yet derivable.
    Unknown,
}

impl Parting {
    fn is_known(self) -> bool {
        !matches!(self, Parting::Unknown)
    }
    fn position(self) -> Option<i32> {
        match self {
            Parting::At(p) => Some(p),
            _ => None,
        }
    }
}

/// A whole parent's token path in generation order, `(position, token)`, or
/// `None` where the store cannot rebuild it: a generation row missing, or a
/// point its generation's counts address absent. A run the schema held
/// before 0014 has no generation rows and so no path.
async fn parent_path(
    tx: &mut Transaction<'_, Postgres>,
    parent: &str,
) -> Result<Option<Vec<(i32, i64)>>, sqlx::Error> {
    let generations = sqlx::query(
        "SELECT turn, resident, output_count FROM generation WHERE run_id = $1 ORDER BY seq",
    )
    .bind(parent)
    .fetch_all(&mut **tx)
    .await?;
    if generations.is_empty() {
        return Ok(None);
    }
    let tokens: HashMap<(String, i32), i64> =
        sqlx::query("SELECT turn, position, token_id FROM position WHERE run_id = $1")
            .bind(parent)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .map(|r| ((r.get("turn"), r.get("position")), r.get("token_id")))
            .collect();
    let mut path = Vec::new();
    for g in generations {
        // **A generation with no drawn tokens adds no position to a tape**,
        // so it is passed over before its addressing is asked for: it may
        // lawfully lack both, and leaves its run whole (Spec 3.1).
        let count: i32 = g.get("output_count");
        if count == 0 {
            continue;
        }
        let (Some(turn), Some(resident)) = (
            g.get::<Option<String>, _>("turn"),
            g.get::<Option<i32>, _>("resident"),
        ) else {
            return Ok(None);
        };
        let floor = resident - count - 1;
        for j in 0..count {
            match tokens.get(&(turn.clone(), floor + j)) {
                Some(token) => path.push((floor + j, *token)),
                None => return Ok(None),
            }
        }
    }
    Ok(Some(path))
}

/// **The walk of Spec 3.1**, against a parent whose status is `whole`.
///
/// The two paths share the position coordinate and nothing else: a branch
/// is a new run whose turn keys restart, its first point sitting at its
/// seated prefix, which holds the parent's tape through the cut. So the
/// walk compares on position alone, turn keys ignored, **from the child's
/// first landed position upward**, the parent's positions below it being the
/// restored prefix and outside the window. A tape whose positions do not
/// strictly increase in generation order gives no coordinate, and the walk
/// derives nothing.
///
/// Within the window, a position both hold with different tokens is a
/// parting, known where no child generation before it was skipped (ruling
/// 22). A position only one holds is a parting only where the child is
/// whole too, whichever path is longer (ruling 31). No difference through
/// both is a known null only where the child is whole (ruling 21).
pub fn walk(child: &RunPlan, parent: &[(i32, i64)]) -> Parting {
    // A tape that revisited a coordinate, folded to one row per key, has
    // none to compare on, as a tape whose positions fall does not.
    if child.repeated {
        return Parting::Unknown;
    }
    if !increasing(child.points.iter().map(|(_, p)| p.position))
        || !increasing(parent.iter().map(|(p, _)| *p))
    {
        return Parting::Unknown;
    }
    let Some((_, first)) = child.points.first() else {
        return Parting::Unknown;
    };
    let start = first.position;
    let whole = child.skipped.is_empty();
    let first_skip = child.skipped.iter().map(|s| s.seq).min();
    let unskipped_before = |seq: i32| first_skip.is_none_or(|skip| seq < skip);
    let mut theirs = parent.iter().filter(|(p, _)| *p >= start).peekable();
    let mut ours = child.points.iter().peekable();
    let only_one = |at: i32| {
        if whole {
            Parting::At(at)
        } else {
            Parting::Unknown
        }
    };
    loop {
        match (ours.peek(), theirs.peek()) {
            (Some((seq, c)), Some((p, token))) if c.position == *p => {
                if c.token_id != *token {
                    return if unskipped_before(*seq) {
                        Parting::At(*p)
                    } else {
                        Parting::Unknown
                    };
                }
                ours.next();
                theirs.next();
            }
            (Some((_, c)), Some((p, _))) => return only_one(c.position.min(*p)),
            (Some((_, c)), None) => return only_one(c.position),
            (None, Some((p, _))) => return only_one(*p),
            (None, None) => {
                return if whole {
                    Parting::Never
                } else {
                    Parting::Unknown
                };
            }
        }
    }
}

/// Whether positions strictly increase in the order given.
fn increasing(mut positions: impl Iterator<Item = i32>) -> bool {
    let mut last = None;
    positions.all(|p| {
        let up = last.is_none_or(|l| p > l);
        last = Some(p);
        up
    })
}
