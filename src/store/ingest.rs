//! conforms: web-ingest-is-idempotent-on-the-key
//! conforms: web-generation-summary-lands
//! conforms: web-position-is-stored-at-ingest
//! conforms: web-record-members-are-absent-where-unsent
//!
//! **The ingest of `weaver-web-Spec` section 3.1**: one `weaver-analysis
//! signals` emission landed in the recorded half, under the rulings of
//! 2026-10-06 in `docs/project/brief-2026-10-06-act-10-the-replay-surface.md`.
//! `emission.rs` reads and plans; this file writes, in three passes:
//!
//! 1. **Each run lands**, in the emission's order: its row first, `writing`,
//!    then its generations and points, one transaction per generation. A run
//!    that is not a branch closes right after, `whole` or `short`. A run the
//!    store already holds is a replay: compared key by key before anything is
//!    written, equal a no-op, different refused in the answer alone with the
//!    store untouched; a replayed `writing` row whose every key is equal is
//!    completed.
//! 2. **Cycles among the emission's branches are refused**, one transaction
//!    per cycle, persisted only on the rows this ingest created.
//! 3. **Each other branch resolves and closes in one transaction**, parents
//!    before children: the link where the parent is held, the walk where it
//!    is also whole, and the close. An ingest that dies before leaves the
//!    branch `writing`, never `whole` with its parent unresolved.

use std::collections::{BTreeMap, HashMap, HashSet};

use sqlx::{PgConnection, Postgres, Row, Transaction};

use super::Store;
use super::emission::{Emission, GenerationRow, PositionRow, RunMembers, RunPlan, Unreadable};

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
    /// After a run's generations are filled, before a run that is not a
    /// branch closes: an ingest that has written and not yet closed.
    AfterFill,
}

/// The ingest's options: a test's stop and a test's race, and nothing in a
/// release build.
#[derive(Debug, Default, Clone)]
pub struct Options {
    #[cfg(test)]
    pub stop_at: Option<Step>,
    /// **A second ingest landed whole at a named point of this one**, as a
    /// concurrent ingest would land there: the races of two ingests,
    /// staged where no clock could stage them.
    #[cfg(test)]
    pub race: Option<(Race, Emission)>,
    /// Where the race's ingest stops, as one still running would stand.
    #[cfg(test)]
    pub race_stop: Option<Step>,
}

/// Where a test's race lands its second ingest.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Race {
    /// Between this ingest's read of a run and its insert of the row.
    BeforeCreate,
    /// After the first generation of a run this ingest created is filled,
    /// before the next.
    AfterFirstFill,
    /// After the cycle scan, before the branches resolve.
    BeforeResolve,
    /// After a run's generations are filled, before it closes.
    BeforeClose,
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
        // **The first pass**: each run lands, branches left open.
        let mut open: Vec<Open> = Vec::new();
        // Every run the emission names whose row the store holds after the
        // first pass, with its parent reference: the open ones and the
        // closed ones replayed equal, which a cycle may run through.
        let mut named: HashMap<String, Named> = HashMap::new();
        for (run, planned) in emission.plan() {
            let plan = match planned {
                Ok(plan) => plan,
                Err(why) => {
                    outcomes.push(RunOutcome::refused(&run, why));
                    continue;
                }
            };
            let mut stored = self.stored(&run).await.map_err(store_error)?;
            let mut created = false;
            if stored.is_none() {
                #[cfg(test)]
                self.race(options, Race::BeforeCreate).await;
                match self.create(&run, &plan).await {
                    Ok(()) => created = true,
                    // **Another ingest created the row since it was read**:
                    // this one is a replay of it, compared and completed
                    // like any other, and never a refusal for having lost
                    // a race it did not know it ran.
                    Err(e) if unique_violation(&e) => {
                        stored = self.stored(&run).await.map_err(store_error)?;
                    }
                    Err(e) => return Err(store_error(e)),
                }
            }
            let mut outcome = RunOutcome::landed(&run, &plan, !created);
            if !created {
                let Some(stored) = stored else {
                    outcome.status = "refused".into();
                    outcome.reason = Some(
                        "the run's row was created and removed while this ingest read it".into(),
                    );
                    outcomes.push(outcome);
                    continue;
                };
                if let Err(why) = compare(&plan, &stored, false) {
                    // **A conflicting replay changes nothing stored**
                    // (ruling 17): the refusal is the answer's alone.
                    outcome.status = "refused".into();
                    outcome.reason = Some(why);
                    outcome.stored = Some(stored.status);
                    outcomes.push(outcome);
                    continue;
                }
                if stored.status != "writing" {
                    // **An equal replay of a closed run is a no-op that
                    // counts as written**, and answers with what the
                    // store holds for it, the reason, the link and the
                    // parting included.
                    answer_stored(&mut outcome, &stored);
                    outcomes.push(outcome);
                    named.insert(
                        run.clone(),
                        Named {
                            parent: plan.members.parent_reference.clone(),
                            status: stored.status,
                            outcome: Some(outcomes.len() - 1),
                        },
                    );
                    continue;
                }
            }
            // **The generations and points, filled under the row's lock**:
            // one transaction per generation for a row this ingest created,
            // one for a replayed `writing` row, each comparing what another
            // ingest may have written meanwhile and inserting only what is
            // missing.
            let scopes: Vec<Option<i32>> = if created {
                plan.generations.iter().map(|g| Some(g.seq)).collect()
            } else {
                vec![None]
            };
            let mut settled = None;
            let (mut written_generations, mut written_positions) = (0, 0);
            for (index, scope) in scopes.into_iter().enumerate() {
                #[cfg(test)]
                if index == 1 {
                    self.race(options, Race::AfterFirstFill).await;
                }
                let _ = index;
                match self.fill(&run, &plan, scope).await.map_err(store_error)? {
                    Fill::Done(generations, positions) => {
                        written_generations += generations;
                        written_positions += positions;
                    }
                    Fill::Closed => {
                        settled = Some(None);
                        break;
                    }
                    Fill::Differs(why) => {
                        settled = Some(Some(why));
                        break;
                    }
                }
            }
            outcome.generations = written_generations;
            outcome.positions = written_positions;
            match settled {
                None => {}
                // **Another ingest closed the row meanwhile**: compared whole
                // before this one answers, so a difference in a generation
                // this one had not reached is refused rather than reported
                // as the other's success.
                Some(None) => {
                    self.settle_closed(&run, &plan, &mut outcome)
                        .await
                        .map_err(store_error)?;
                    outcomes.push(outcome);
                    continue;
                }
                Some(Some(why)) => {
                    outcome.status = "refused".into();
                    outcome.reason = Some(why);
                    outcome.stored = Some("writing".into());
                    outcomes.push(outcome);
                    continue;
                }
            }
            #[cfg(test)]
            if options.stop_at == Some(Step::AfterFill) {
                return Err("stopped by the test's hook after the fill".into());
            }
            if plan.members.parent_reference.is_none() {
                // **A run that is not a branch closes as soon as its points
                // are written** (ruling 27), and only where what is stored
                // is what it planned.
                #[cfg(test)]
                self.race(options, Race::BeforeClose).await;
                match self
                    .close_compared(&run, &plan)
                    .await
                    .map_err(store_error)?
                {
                    Close::Closed(status, reason) => {
                        outcome.status = status.into();
                        outcome.reason = reason;
                        // Closed over exactly the plan, so the plan's counts
                        // are what the store holds.
                        outcome.generations = plan.generations.len();
                        outcome.positions = plan.points.len();
                    }
                    // Another ingest of the run closed it first.
                    Close::NotOpen => {
                        self.settle_closed(&run, &plan, &mut outcome)
                            .await
                            .map_err(store_error)?;
                    }
                    Close::Differs(why) => {
                        outcome.status = "refused".into();
                        outcome.reason = Some(why);
                        outcome.stored = Some("writing".into());
                    }
                }
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
                    outcome: None,
                });
                references.insert(run, parent);
            }
            frontier = next.into_iter().collect();
        }
        let open_runs: Vec<String> = open.iter().map(|o| o.run.clone()).collect();
        let (cycles, order) = plan_resolution(&open_runs, &references);
        let in_cycle: HashSet<&str> = cycles.iter().flatten().map(String::as_str).collect();
        for cycle in &cycles {
            let named_cycle = format!("a reference cycle: {} -> {}", cycle.join(" -> "), cycle[0]);
            let on_cycle: HashSet<&str> = cycle.iter().map(String::as_str).collect();
            let created: Vec<&Open> = open
                .iter()
                .filter(|o| o.created && on_cycle.contains(o.run.as_str()))
                .collect();
            let created_runs: HashSet<&str> = created.iter().map(|o| o.run.as_str()).collect();
            // **One transaction per cycle** (ruling 30), persisted only on the
            // rows this ingest created (ruling 29).
            let mut tx = self.pool.begin().await.map_err(store_error)?;
            for row in &created {
                sqlx::query(
                    "UPDATE run SET ingest_status = 'refused', ingest_reason = $2 \
                     WHERE run_id = $1 AND ingest_status = 'writing'",
                )
                .bind(&row.run)
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
            tx.commit().await.map_err(store_error)?;
            // Every other run on the cycle is reported refused in the answer
            // with the status the store keeps for it.
            for run in cycle {
                let created = created_runs.contains(run.as_str());
                let held = &named[run];
                let index = match held.outcome {
                    Some(index) => index,
                    None => {
                        let mut outcome = RunOutcome::refused(run, String::new());
                        outcome.replayed = true;
                        outcome.parent_reference = held.parent.clone();
                        outcomes.push(outcome);
                        outcomes.len() - 1
                    }
                };
                let outcome = &mut outcomes[index];
                outcome.status = "refused".into();
                outcome.reason = Some(named_cycle.clone());
                if !created {
                    outcome.stored = Some(held.status.clone());
                }
            }
        }
        // **The third pass**: every other branch, parents first, resolved
        // and closed in one transaction each (rulings 23, 27 and 28), in the
        // order `plan_resolution` gave. A chain that leads into a cycle
        // stops there: the cycle's rows are refused or left `writing`, never
        // whole, so its children resolve after it with their parting unknown.
        let by_run: HashMap<&str, &Open> = open.iter().map(|o| (o.run.as_str(), o)).collect();
        let order: Vec<&Open> = order
            .iter()
            .filter(|run| !in_cycle.contains(run.as_str()))
            .map(|run| by_run[run.as_str()])
            .collect();
        // The ends of chains this ingest's resolutions have followed, so a
        // long chain is followed once and not once per branch on it.
        let mut tails: HashMap<String, Option<String>> = HashMap::new();
        #[cfg(test)]
        self.race(options, Race::BeforeResolve).await;
        for o in order {
            let Some(resolved) = self
                .resolve_retrying(o, &mut tails)
                .await
                .map_err(store_error)?
            else {
                // Another ingest of the run resolved and closed it first.
                self.settle_closed(&o.run, &o.plan, &mut outcomes[o.outcome])
                    .await
                    .map_err(store_error)?;
                continue;
            };
            let outcome = &mut outcomes[o.outcome];
            if let Some(stored) = resolved.stored.clone() {
                outcome.stored = Some(stored);
            }
            outcome.status = resolved.status.into();
            if resolved.stored.is_none() && resolved.status != "refused" {
                // Closed over exactly the plan.
                outcome.generations = o.plan.generations.len();
                outcome.positions = o.plan.points.len();
            }
            outcome.reason = resolved.reason;
            outcome.parent_linked = resolved.linked;
            outcome.parting_known = resolved.parting.is_known();
            outcome.parting_position = resolved.parting.position();
        }
        Ok(())
    }

    /// What the store holds for `run`, or `None` where it holds no row.
    async fn stored(&self, run: &str) -> Result<Option<Stored>, sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        stored_on(&mut conn, run).await
    }

    /// **A run that is not a branch, closed under its row's lock and only
    /// where the stored run is exactly what this ingest planned.** The
    /// comparison and the status move under one lock, so an ingest whose
    /// plan holds less than another wrote meanwhile never closes the run
    /// over generations its emission never carried: it is refused in the
    /// answer and the row is left `writing` for the ingest whose plan
    /// matches it.
    async fn close_compared(&self, run: &str, plan: &RunPlan) -> Result<Close, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        if !lock(&mut tx, run).await? {
            return Ok(Close::NotOpen);
        }
        let Some(stored) = stored_on(&mut tx, run).await? else {
            return Ok(Close::NotOpen);
        };
        if let Err(why) = compare(plan, &stored, true) {
            return Ok(Close::Differs(why));
        }
        let (status, reason) = plan.closing();
        sqlx::query(
            "UPDATE run SET ingest_status = $2, ingest_reason = $3 \
             WHERE run_id = $1 AND ingest_status = 'writing'",
        )
        .bind(run)
        .bind(status)
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Close::Closed(status, reason))
    }
}

/// What the store holds for `run` over one connection, a transaction's
/// included, or `None` where it holds no row.
async fn stored_on(conn: &mut PgConnection, run: &str) -> Result<Option<Stored>, sqlx::Error> {
    {
        let Some(row) = sqlx::query(
            "SELECT record_identity, seed::text AS seed, sampler, device, engine, field_depth, \
             record_session, record_digest, prefix_length, parent_reference, ingest_status, \
             ingest_reason, parent_run_id IS NOT NULL AS linked, parting_known, parting_position \
             FROM run WHERE run_id = $1",
        )
        .bind(run)
        .fetch_optional(&mut *conn)
        .await?
        else {
            return Ok(None);
        };
        let members = RunMembers {
            record_identity: row.get("record_identity"),
            seed: row.get("seed"),
            sampler: row.get("sampler"),
            device: row.get("device"),
            engine: row.get("engine"),
            field_depth: row.get("field_depth"),
            record_session: row.get("record_session"),
            record_digest: row.get("record_digest"),
            prefix_length: row.get("prefix_length"),
            parent_reference: row.get("parent_reference"),
        };
        let status: String = row.get("ingest_status");
        let reason: Option<String> = row.get("ingest_reason");
        let linked: bool = row.get("linked");
        let parting_known: bool = row.get("parting_known");
        let parting_position: Option<i32> = row.get("parting_position");
        let generations = sqlx::query(
            "SELECT seq, turn, perplexity, resident, output_count, generation_seed::text AS generation_seed \
             FROM generation WHERE run_id = $1 ORDER BY seq",
        )
        .bind(run)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| GenerationRow {
            seq: r.get("seq"),
            turn: r.get("turn"),
            perplexity: r.get("perplexity"),
            resident: r.get("resident"),
            output_count: r.get("output_count"),
            generation_seed: r.get("generation_seed"),
        })
        .collect();
        let positions = sqlx::query(
            "SELECT turn, position, token_id, entropy, surprisal, \
             (token_text IS NULL AND alternatives IS NULL AND realized IS NULL AND residual IS NULL) AS unfilled \
             FROM position WHERE run_id = $1",
        )
        .bind(run)
        .fetch_all(&mut *conn)
        .await?
        .into_iter()
        .map(|r| {
            let row = PositionRow {
                turn: r.get("turn"),
                position: r.get("position"),
                token_id: r.get("token_id"),
                entropy: r.get("entropy"),
                surprisal: r.get("surprisal"),
            };
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
            members,
            status,
            reason,
            linked,
            parting_known,
            parting_position,
            generations,
            positions,
        }))
    }
}

impl Store {
    /// **The run's row, first and `writing`** (Spec 3.1). The boundary set
    /// is written empty, a fact about every run this ingest can meet (Spec
    /// 2.2).
    async fn create(&self, run: &str, plan: &RunPlan) -> Result<(), sqlx::Error> {
        let m = &plan.members;
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, seed, sampler, device, engine, field_depth, \
             boundary_set, record_session, record_digest, prefix_length, parent_reference, ingest_status) \
             VALUES ($1, $2, $3::numeric, $4, $5, $6, $7, '[]'::jsonb, $8, $9, $10, $11, 'writing')",
        )
        .bind(run)
        .bind(&m.record_identity)
        .bind(&m.seed)
        .bind(&m.sampler)
        .bind(&m.device)
        .bind(&m.engine)
        .bind(m.field_depth)
        .bind(&m.record_session)
        .bind(&m.record_digest)
        .bind(m.prefix_length)
        .bind(&m.parent_reference)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// **Generations and points filled under the row's lock**, in one
    /// transaction: one generation where `scope` names it, every one the
    /// plan holds otherwise. The row must still read `writing`, so nothing
    /// lands on a closed run. What is already stored, which another ingest
    /// of the run may have written since this one read it, is compared and
    /// never rewritten; only what is missing is inserted, bulk per
    /// generation (Spec 3.1).
    async fn fill(
        &self,
        run: &str,
        plan: &RunPlan,
        scope: Option<i32>,
    ) -> Result<Fill, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        if !lock(&mut tx, run).await? {
            return Ok(Fill::Closed);
        }
        let generations: Vec<&GenerationRow> = plan
            .generations
            .iter()
            .filter(|g| scope.is_none_or(|seq| g.seq == seq))
            .collect();
        let held: HashMap<i32, GenerationRow> = sqlx::query(
            "SELECT seq, turn, perplexity, resident, output_count, generation_seed::text AS generation_seed \
             FROM generation WHERE run_id = $1",
        )
        .bind(run)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(|r| {
            let g = GenerationRow {
                seq: r.get("seq"),
                turn: r.get("turn"),
                perplexity: r.get("perplexity"),
                resident: r.get("resident"),
                output_count: r.get("output_count"),
                generation_seed: r.get("generation_seed"),
            };
            (g.seq, g)
        })
        .collect();
        let mut inserted = 0;
        for g in &generations {
            match held.get(&g.seq) {
                Some(stored) if stored == *g => {}
                Some(_) => {
                    return Ok(Fill::Differs(format!(
                        "generation {} was written meanwhile with a different summary",
                        g.seq
                    )));
                }
                None => {
                    insert_generation(&mut tx, run, g).await?;
                    inserted += 1;
                }
            }
        }
        let points: Vec<&PositionRow> = plan
            .points
            .iter()
            .filter(|(seq, _)| scope.is_none_or(|s| *seq == s))
            .map(|(_, p)| p)
            .collect();
        let stored = stored_positions(&mut tx, run, &points).await?;
        let mut missing = Vec::new();
        for p in points {
            match stored.get(&(p.turn.clone(), p.position)) {
                Some(held) if held.row == *p && held.unfilled => {}
                Some(_) => {
                    return Ok(Fill::Differs(format!(
                        "turn {} position {} was written meanwhile with a different payload",
                        p.turn, p.position
                    )));
                }
                None => missing.push(p),
            }
        }
        insert_points(&mut tx, run, &missing).await?;
        tx.commit().await?;
        Ok(Fill::Done(inserted, missing.len()))
    }

    /// **The outcome of a run another ingest closed**: the stored run
    /// compared whole against this one's plan, as any replay is. Equal
    /// answers with what the store holds; different is refused in the
    /// answer with the first difference named, the stored row untouched
    /// (ruling 17).
    async fn settle_closed(
        &self,
        run: &str,
        plan: &RunPlan,
        outcome: &mut RunOutcome,
    ) -> Result<(), sqlx::Error> {
        let Some(stored) = self.stored(run).await? else {
            return Ok(());
        };
        match compare(plan, &stored, false) {
            Ok(()) => answer_stored(outcome, &stored),
            Err(why) => {
                outcome.status = "refused".into();
                outcome.reason = Some(why);
                outcome.stored = Some(stored.status);
            }
        }
        Ok(())
    }

    /// The test's race, where it names `at`.
    #[cfg(test)]
    async fn race(&self, options: &Options, at: Race) {
        if let Some((race, emission)) = &options.race
            && *race == at
        {
            let options = Options {
                stop_at: options.race_stop,
                ..Options::default()
            };
            Box::pin(self.ingest_with(Ok(emission.clone()), &options)).await;
        }
    }

    /// `resolve`, retried where the store broke a deadlock against another
    /// ingest's resolution: two branches naming each other lock each other's
    /// rows in opposite orders, and the one the store aborts resolves again
    /// once the other has committed.
    async fn resolve_retrying(
        &self,
        open: &Open,
        tails: &mut HashMap<String, Option<String>>,
    ) -> Result<Option<Resolved>, sqlx::Error> {
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
    ) -> Result<Option<Resolved>, sqlx::Error> {
        let parent = open
            .plan
            .members
            .parent_reference
            .as_deref()
            .expect("only branches resolve");
        let mut tx = self.pool.begin().await?;
        if !lock(&mut tx, &open.run).await? {
            return Ok(None);
        }
        // **The branch closes only over exactly what it planned**, compared
        // under the same lock as its close, as `close_compared` does for a
        // run that is not a branch.
        if let Some(stored) = stored_on(&mut tx, &open.run).await?
            && let Err(why) = compare(&open.plan, &stored, true)
        {
            return Ok(Some(Resolved {
                status: "refused",
                reason: Some(why),
                linked: false,
                parting: Parting::Unknown,
                stored: Some("writing".into()),
            }));
        }
        let parent_status: Option<String> =
            sqlx::query_scalar("SELECT ingest_status FROM run WHERE run_id = $1 FOR SHARE")
                .bind(parent)
                .fetch_optional(&mut *tx)
                .await?;
        let linked = parent_status.is_some();
        // **The cycle, rechecked under this transaction's locks**: a
        // concurrent ingest may have created the parent, or a run on its
        // chain, naming this branch after this ingest's cycle scan ran. The
        // chain is followed from the parent through held rows, each locked
        // for share as it is read; where it returns to this branch, the
        // branch is refused by name rather than closed on a cycle.
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
                            "SELECT parent_reference FROM run WHERE run_id = $1 FOR SHARE",
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
                        return Ok(Some(Resolved {
                            status: "refused",
                            reason: Some(reason),
                            linked: false,
                            parting: Parting::Unknown,
                            stored: Some("writing".into()),
                        }));
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
                    return Ok(Some(Resolved {
                        status: "refused",
                        reason: Some(reason),
                        linked: false,
                        parting: Parting::Unknown,
                        stored: None,
                    }));
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
        Ok(Some(Resolved {
            status,
            reason,
            linked,
            parting,
            stored: None,
        }))
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

/// Lock the run's row inside `tx`: whether it still reads `writing`. Two
/// ingests of one run serialize here, the second reading what the first
/// wrote once the first commits.
async fn lock(tx: &mut Transaction<'_, Postgres>, run: &str) -> Result<bool, sqlx::Error> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT ingest_status FROM run WHERE run_id = $1 FOR UPDATE")
            .bind(run)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(status.as_deref() == Some("writing"))
}

/// How a compared close ended.
enum Close {
    Closed(&'static str, Option<String>),
    /// The row no longer reads `writing`: another ingest closed it.
    NotOpen,
    /// What is stored is not exactly what this ingest planned.
    Differs(String),
}

/// How a fill ended.
enum Fill {
    /// Filled, with the generations and positions this fill inserted.
    Done(usize, usize),
    /// The row no longer reads `writing`: another ingest closed it.
    Closed,
    /// Another ingest wrote a key with a different payload meanwhile.
    Differs(String),
}

/// Whether a store error is a unique-key conflict.
fn unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(d) if d.code().as_deref() == Some("23505"))
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

/// The stored positions among `points`' keys: one generation's turn and
/// span where they share one, the run's every position otherwise.
async fn stored_positions(
    tx: &mut Transaction<'_, Postgres>,
    run: &str,
    points: &[&PositionRow],
) -> Result<HashMap<(String, i32), StoredPosition>, sqlx::Error> {
    let one_turn = points
        .first()
        .map(|p| p.turn.as_str())
        .filter(|t| points.iter().all(|p| p.turn == *t));
    let rows = match one_turn {
        Some(turn) => {
            let low = points.iter().map(|p| p.position).min().unwrap_or(0);
            let high = points.iter().map(|p| p.position).max().unwrap_or(0);
            sqlx::query(
                "SELECT turn, position, token_id, entropy, surprisal, \
                 (token_text IS NULL AND alternatives IS NULL AND realized IS NULL AND residual IS NULL) AS unfilled \
                 FROM position WHERE run_id = $1 AND turn = $2 AND position BETWEEN $3 AND $4",
            )
            .bind(run)
            .bind(turn)
            .bind(low)
            .bind(high)
            .fetch_all(&mut **tx)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT turn, position, token_id, entropy, surprisal, \
                 (token_text IS NULL AND alternatives IS NULL AND realized IS NULL AND residual IS NULL) AS unfilled \
                 FROM position WHERE run_id = $1",
            )
            .bind(run)
            .fetch_all(&mut **tx)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|r| {
            let row = PositionRow {
                turn: r.get("turn"),
                position: r.get("position"),
                token_id: r.get("token_id"),
                entropy: r.get("entropy"),
                surprisal: r.get("surprisal"),
            };
            (
                (row.turn.clone(), row.position),
                StoredPosition {
                    row,
                    unfilled: r.get("unfilled"),
                },
            )
        })
        .collect())
}

async fn insert_generation(
    tx: &mut Transaction<'_, Postgres>,
    run: &str,
    g: &GenerationRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO generation (run_id, seq, turn, perplexity, resident, output_count, generation_seed) \
         VALUES ($1, $2, $3, $4, $5, $6, $7::numeric)",
    )
    .bind(run)
    .bind(g.seq)
    .bind(&g.turn)
    .bind(g.perplexity)
    .bind(g.resident)
    .bind(g.output_count)
    .bind(&g.generation_seed)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The points, in one statement over arrays. The text, the alternatives,
/// the rank and the residual are never written: this seam carries none.
async fn insert_points(
    tx: &mut Transaction<'_, Postgres>,
    run: &str,
    points: &[&PositionRow],
) -> Result<(), sqlx::Error> {
    if points.is_empty() {
        return Ok(());
    }
    let turns: Vec<&str> = points.iter().map(|p| p.turn.as_str()).collect();
    let positions: Vec<i32> = points.iter().map(|p| p.position).collect();
    let tokens: Vec<i64> = points.iter().map(|p| p.token_id).collect();
    let entropies: Vec<Option<f64>> = points.iter().map(|p| p.entropy).collect();
    let surprisals: Vec<Option<f64>> = points.iter().map(|p| p.surprisal).collect();
    sqlx::query(
        "INSERT INTO position (run_id, turn, position, token_id, entropy, surprisal) \
         SELECT $1, * FROM UNNEST($2::text[], $3::int4[], $4::int8[], $5::float8[], $6::float8[])",
    )
    .bind(run)
    .bind(&turns)
    .bind(&positions)
    .bind(&tokens)
    .bind(&entropies)
    .bind(&surprisals)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// **A replay compared against what is stored**, key by key, before
/// anything is written (rulings 14 and 17). Every stored key must be one the
/// emission names with an equal payload; a closed run must hold exactly
/// what the emission names, and a `writing` one may hold less. The first
/// difference is named.
fn compare(plan: &RunPlan, stored: &Stored, whole: bool) -> Result<(), String> {
    let refused = |what: String| {
        format!("a replay that differs from the stored run, which stands unchanged: {what}")
    };
    if plan.members != stored.members {
        return Err(refused(member_difference(&plan.members, &stored.members)));
    }
    for g in &stored.generations {
        match plan.generations.get(g.seq as usize) {
            Some(p) if p == g => {}
            _ => return Err(refused(format!("generation {} differs", g.seq))),
        }
    }
    let mut planned: BTreeMap<(&str, i32), &PositionRow> = BTreeMap::new();
    for (_, p) in &plan.points {
        planned.insert((p.turn.as_str(), p.position), p);
    }
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
    if (whole || stored.status != "writing")
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

fn member_difference(a: &RunMembers, b: &RunMembers) -> String {
    let names = [
        ("record identity", a.record_identity != b.record_identity),
        ("seed", a.seed != b.seed),
        ("sampler", a.sampler != b.sampler),
        ("device", a.device != b.device),
        ("engine", a.engine != b.engine),
        ("field depth", a.field_depth != b.field_depth),
        ("session", a.record_session != b.record_session),
        ("digest", a.record_digest != b.record_digest),
        ("seated prefix's length", a.prefix_length != b.prefix_length),
        ("parent reference", a.parent_reference != b.parent_reference),
    ];
    let differ: Vec<&str> = names.iter().filter(|(_, d)| *d).map(|(n, _)| *n).collect();
    format!("the run's {} differs", differ.join(", "))
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
        let (Some(turn), Some(resident)) = (
            g.get::<Option<String>, _>("turn"),
            g.get::<Option<i32>, _>("resident"),
        ) else {
            return Ok(None);
        };
        let count: i32 = g.get("output_count");
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
