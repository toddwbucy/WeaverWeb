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

use sqlx::{Postgres, Row, Transaction};

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
}

/// The ingest's options: a test's stop, and nothing in a release build.
#[derive(Debug, Default, Clone)]
pub struct Options {
    #[cfg(test)]
    pub stop_at: Option<Step>,
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
            positions: plan.points.len(),
            generations: plan.generations.len(),
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
            let stored = self.stored(&run).await.map_err(store_error)?;
            let created = stored.is_none();
            let mut outcome = RunOutcome::landed(&run, &plan, !created);
            match stored {
                Some(stored) => {
                    if let Err(why) = compare(&plan, &stored) {
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
                        outcome.status = stored.status.clone();
                        outcome.reason = stored.reason.clone();
                        outcome.parent_linked = stored.linked;
                        outcome.parting_known = stored.parting_known;
                        outcome.parting_position = stored.parting_position;
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
                    // A `writing` row whose every key is equal is completed.
                    self.complete(&run, &plan, &stored)
                        .await
                        .map_err(store_error)?;
                }
                None => {
                    if let Err(e) = self.create(&run, &plan).await {
                        // Another ingest created the row since it was read.
                        outcome.status = "refused".into();
                        outcome.reason = Some(format!(
                            "the run's row could not be created ({e}); another ingest may hold it, and a replay compares against it"
                        ));
                        outcomes.push(outcome);
                        continue;
                    }
                    for generation in &plan.generations {
                        self.write_generation(&run, &plan, generation.seq)
                            .await
                            .map_err(store_error)?;
                    }
                }
            }
            if plan.members.parent_reference.is_none() {
                // **A run that is not a branch closes as soon as its points
                // are written** (ruling 27).
                let (status, reason) = plan.closing();
                close(&self.pool, &run, status, reason.as_deref())
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
        // replayed, or a row the emission does not name at all. So each open
        // branch's reference chain is followed through held rows until it
        // ends, joins a chain already followed, or returns.
        let mut in_cycle: HashSet<String> = HashSet::new();
        let mut explored: HashSet<String> = HashSet::new();
        let mut cycles: Vec<Vec<String>> = Vec::new();
        for start in &open {
            let mut path: Vec<String> = Vec::new();
            let mut at = start.run.clone();
            loop {
                if in_cycle.contains(&at) || explored.contains(&at) {
                    break;
                }
                if let Some(from) = path.iter().position(|n| *n == at) {
                    let cycle = path[from..].to_vec();
                    // A cycle no open branch is on was in the store before
                    // this ingest, and is not this ingest's to refuse.
                    if cycle.iter().any(|n| open.iter().any(|o| o.run == *n)) {
                        in_cycle.extend(cycle.iter().cloned());
                        cycles.push(cycle);
                    }
                    break;
                }
                let parent = match named.get(&at) {
                    Some(n) => n.parent.clone(),
                    None => match self.reference_of(&at).await.map_err(store_error)? {
                        Some((parent, status)) => {
                            named.insert(
                                at.clone(),
                                Named {
                                    parent: parent.clone(),
                                    status,
                                    outcome: None,
                                },
                            );
                            parent
                        }
                        // Not held: the chain ends.
                        None => break,
                    },
                };
                path.push(at.clone());
                match parent {
                    Some(parent) => at = parent,
                    None => break,
                }
            }
            explored.extend(path.into_iter().filter(|n| !in_cycle.contains(n)));
        }
        for cycle in &cycles {
            let named_cycle = format!("a reference cycle: {} -> {}", cycle.join(" -> "), cycle[0]);
            let created: Vec<&Open> = open
                .iter()
                .filter(|o| cycle.contains(&o.run) && o.created)
                .collect();
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
                let created = created.iter().any(|o| o.run == *run);
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
        let parent_of: HashMap<&str, &str> = open
            .iter()
            .filter_map(|o| {
                let parent = o.plan.members.parent_reference.as_deref()?;
                open.iter()
                    .any(|p| p.run == parent)
                    .then_some((o.run.as_str(), parent))
            })
            .collect();

        // **The third pass**: every other branch, parents first, resolved
        // and closed in one transaction each (rulings 23, 27 and 28).
        // A chain that leads into a cycle stops there: the cycle's rows are
        // refused or left `writing`, never whole, so its children resolve
        // after it with their parting unknown.
        let depth = |run: &str| {
            let mut depth = 0;
            let mut at = run;
            while let Some(parent) = parent_of.get(at) {
                depth += 1;
                if in_cycle.contains(*parent) {
                    break;
                }
                at = parent;
            }
            depth
        };
        let mut order: Vec<&Open> = open.iter().filter(|o| !in_cycle.contains(&o.run)).collect();
        order.sort_by_key(|o| depth(&o.run));
        for o in order {
            let resolved = self.resolve(o).await.map_err(store_error)?;
            let outcome = &mut outcomes[o.outcome];
            outcome.status = resolved.status.into();
            outcome.reason = resolved.reason;
            outcome.parent_linked = resolved.linked;
            outcome.parting_known = resolved.parting.is_known();
            outcome.parting_position = resolved.parting.position();
        }
        Ok(())
    }

    /// A held run's parent reference and status, or `None` where the store
    /// holds no row for it.
    async fn reference_of(
        &self,
        run: &str,
    ) -> Result<Option<(Option<String>, String)>, sqlx::Error> {
        Ok(
            sqlx::query("SELECT parent_reference, ingest_status FROM run WHERE run_id = $1")
                .bind(run)
                .fetch_optional(&self.pool)
                .await?
                .map(|r| (r.get("parent_reference"), r.get("ingest_status"))),
        )
    }

    /// What the store holds for `run`, or `None` where it holds no row.
    async fn stored(&self, run: &str) -> Result<Option<Stored>, sqlx::Error> {
        let Some(row) = sqlx::query(
            "SELECT record_identity, seed::text AS seed, sampler, device, engine, field_depth, \
             record_session, record_digest, prefix_length, parent_reference, ingest_status, \
             ingest_reason, parent_run_id IS NOT NULL AS linked, parting_known, parting_position \
             FROM run WHERE run_id = $1",
        )
        .bind(run)
        .fetch_optional(&self.pool)
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
        .fetch_all(&self.pool)
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
        .fetch_all(&self.pool)
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

    /// **One generation and its points, in one transaction**: bulk per
    /// generation and never per point (Spec 3.1). The run's row is locked
    /// and must still read `writing`, so nothing lands on a closed run.
    async fn write_generation(
        &self,
        run: &str,
        plan: &RunPlan,
        seq: i32,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        lock_open(&mut tx, run).await?;
        let generation = &plan.generations[seq as usize];
        insert_generation(&mut tx, run, generation).await?;
        let points: Vec<&PositionRow> = plan
            .points
            .iter()
            .filter(|(s, _)| *s == seq)
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
        lock_open(&mut tx, run).await?;
        let held: HashSet<i32> = stored.generations.iter().map(|g| g.seq).collect();
        for generation in plan.generations.iter().filter(|g| !held.contains(&g.seq)) {
            insert_generation(&mut tx, run, generation).await?;
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

    /// **One branch, resolved and closed in one transaction**: the link
    /// where the parent is held whatever its status (ruling 18), the walk
    /// only where the parent is also whole, and the close.
    async fn resolve(&self, open: &Open) -> Result<Resolved, sqlx::Error> {
        let parent = open
            .plan
            .members
            .parent_reference
            .as_deref()
            .expect("only branches resolve");
        let mut tx = self.pool.begin().await?;
        lock_open(&mut tx, &open.run).await?;
        let parent_status: Option<String> =
            sqlx::query_scalar("SELECT ingest_status FROM run WHERE run_id = $1 FOR SHARE")
                .bind(parent)
                .fetch_optional(&mut *tx)
                .await?;
        let linked = parent_status.is_some();
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
        })
    }
}

struct Resolved {
    status: &'static str,
    reason: Option<String>,
    linked: bool,
    parting: Parting,
}

/// Lock the run's row inside `tx`, refusing where it no longer reads
/// `writing`.
async fn lock_open(tx: &mut Transaction<'_, Postgres>, run: &str) -> Result<(), sqlx::Error> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT ingest_status FROM run WHERE run_id = $1 FOR UPDATE")
            .bind(run)
            .fetch_optional(&mut **tx)
            .await?;
    match status.as_deref() {
        Some("writing") => Ok(()),
        other => Err(sqlx::Error::Protocol(format!(
            "run {run} is no longer open: it reads {other:?}"
        ))),
    }
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

async fn close(
    pool: &sqlx::PgPool,
    run: &str,
    status: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE run SET ingest_status = $2, ingest_reason = $3 \
         WHERE run_id = $1 AND ingest_status = 'writing'",
    )
    .bind(run)
    .bind(status)
    .bind(reason)
    .execute(pool)
    .await?;
    Ok(())
}

/// **A replay compared against what is stored**, key by key, before
/// anything is written (rulings 14 and 17). Every stored key must be one the
/// emission names with an equal payload; a closed run must hold exactly
/// what the emission names, and a `writing` one may hold less. The first
/// difference is named.
fn compare(plan: &RunPlan, stored: &Stored) -> Result<(), String> {
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
