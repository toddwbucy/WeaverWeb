//! The ingest of `weaver-web-Spec` section 3.1 against a live PostgreSQL
//! named by `DATABASE_URL`, over the vendored emissions under
//! `tests/fixtures/signals/` (their provenance is that directory's README).
//! Without the variable they pass by not running and say so.
//!
//! **No test runs the WeaverAnalysis binary.** Shapes the emitter's
//! fixtures do not carry (a second run, a disagreement, a cycle, a chain, a
//! shorter child) are built here by editing a vendored emission, and each
//! test that does so says what it changed. Every run identity is tagged
//! per test, so a test never meets another's rows or a previous run's.

use serde_json::{Value, json};
use sqlx::Row;

use super::Store;
use super::emission::Emission;
use super::ingest::{Hold, HoldAt, Options, Step};
use super::key::{RunId, TurnId};
use super::read::tests::store;

const CERTIFIED: &str = include_str!("../../tests/fixtures/signals/diagnostic-certified.ndjson");
const SERVING: &str = include_str!("../../tests/fixtures/signals/serving-source.ndjson");
const COLUMNS_A: &str = include_str!("../../tests/fixtures/signals/columns-a.ndjson");
/// **Hand-made, not an emitter's output**: see the fixtures' README.
const HAND_MADE_BRANCH: &str = include_str!("../../tests/fixtures/signals/hand-made-branch.ndjson");

/// An emission held as values, for a test to edit before it lands.
#[derive(Clone)]
struct Wire {
    summary: Value,
    points: Vec<Value>,
}

fn tag(name: &str) -> String {
    format!(
        "{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )
}

impl Wire {
    fn of(text: &str) -> Self {
        let mut lines = text.lines();
        let summary = serde_json::from_str(lines.next().unwrap()).unwrap();
        let points = lines
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        Self { summary, points }
    }

    /// Every run identity, the lineage's included, suffixed with `tag`.
    fn tagged(mut self, tag: &str) -> Self {
        for g in self.generations_mut() {
            let run = g["run"].as_str().unwrap().to_owned();
            g["run"] = json!(format!("{run}#{tag}"));
            if let Some(parent) = g
                .pointer("/lineage/built_from/run")
                .and_then(Value::as_str)
                .map(str::to_owned)
            {
                g["lineage"]["built_from"]["run"] = json!(format!("{parent}#{tag}"));
                g["lineage"]["run"] = json!(format!("{parent}#{tag}"));
            }
        }
        self
    }

    /// Every generation's run renamed to `run`.
    fn renamed(mut self, run: &str) -> Self {
        for g in self.generations_mut() {
            g["run"] = json!(run);
        }
        self
    }

    /// Every generation's lineage built from `parent`, in the hand-made
    /// fixture's shape.
    fn branch_of(mut self, parent: &str) -> Self {
        for g in self.generations_mut() {
            g["lineage"] = json!({
                "save_point": format!("{}1", "0".repeat(63)),
                "run": parent, "sequence": 2, "turn": 2, "operator_supplied": false,
                "built_from": {"parent": "s-karl-1", "run": parent, "through": 2},
            });
        }
        self
    }

    fn run(&self) -> String {
        self.summary["generations"][0]["run"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn generations_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.summary["generations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
    }

    /// Two emissions as one, the first's runs landing first.
    fn then(mut self, other: Wire) -> Self {
        let more = other.summary["generations"].as_array().unwrap().clone();
        self.summary["generations"]
            .as_array_mut()
            .unwrap()
            .extend(more);
        self.points.extend(other.points);
        self
    }

    /// The emission's text, its counts taken over its points as the emitter
    /// takes them.
    fn text(&self) -> String {
        let mut summary = self.summary.clone();
        summary["positions"] = json!(self.points.len());
        summary["with_entropy"] = json!(
            self.points
                .iter()
                .filter(|p| p.get("entropy").is_some())
                .count()
        );
        summary["with_surprisal"] = json!(
            self.points
                .iter()
                .filter(|p| p.get("surprisal").is_some())
                .count()
        );
        let mut text = serde_json::to_string(&summary).unwrap();
        for p in &self.points {
            text.push('\n');
            text.push_str(&serde_json::to_string(p).unwrap());
        }
        text.push('\n');
        text
    }
}

async fn ingest(s: &Store, w: &Wire) -> Value {
    s.ingest(w.text().as_bytes()).await.value
}

async fn ingest_stopping(s: &Store, w: &Wire, step: Step) -> Value {
    s.ingest_with(
        Emission::read(w.text().as_bytes()),
        &Options {
            stop_at: Some(step),
            ..Options::default()
        },
    )
    .await
    .value
}

/// The run's row as the ingest left it, or `None`.
#[derive(Debug, PartialEq)]
struct Landed {
    status: String,
    reason: Option<String>,
    parent_run_id: Option<String>,
    parent_reference: Option<String>,
    parting_position: Option<i32>,
    parting_known: bool,
}

async fn landed(s: &Store, run: &str) -> Option<Landed> {
    sqlx::query(
        "SELECT ingest_status, ingest_reason, parent_run_id, parent_reference, \
         parting_position, parting_known FROM run WHERE run_id = $1",
    )
    .bind(run)
    .fetch_optional(&s.pool)
    .await
    .unwrap()
    .map(|r| Landed {
        status: r.get("ingest_status"),
        reason: r.get("ingest_reason"),
        parent_run_id: r.get("parent_run_id"),
        parent_reference: r.get("parent_reference"),
        parting_position: r.get("parting_position"),
        parting_known: r.get("parting_known"),
    })
}

async fn count(s: &Store, table: &str, run: &str) -> i64 {
    let sql = match table {
        "position" => "SELECT count(*) FROM position WHERE run_id = $1",
        "generation" => "SELECT count(*) FROM generation WHERE run_id = $1",
        other => panic!("no count for {other}"),
    };
    sqlx::query_scalar(sql)
        .bind(run)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// **A run lands whole, its positions derived at ingest and stored**
/// (Spec 3.1): `(R - O - 1) + j` from each generation's two counts, the
/// first of the first generation at 60 (73 - 12 - 1), the first of the
/// second at 116 (560 - 443 - 1). The summary's counts altered after the
/// ingest move no stored position, since read two reads the stored one.
/// The members the record did not carry are absent; the sampler is the
/// declared sampling without its per-generation seed, which lands on the
/// generation.
///
/// conforms: web-position-is-stored-at-ingest
#[tokio::test]
async fn a_run_lands_whole_with_its_positions_derived_and_stored() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("whole"));
    let run = w.run();
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let out = &answer["runs"][0];
    assert_eq!(out["status"], json!("whole"), "{answer}");
    assert_eq!(
        (out["positions"].clone(), out["generations"].clone()),
        (json!(455), json!(2))
    );
    assert_eq!(count(&s, "position", &run).await, 455);
    assert_eq!(count(&s, "generation", &run).await, 2);

    let first = s
        .range(&RunId(run.clone()), &TurnId("t-1".into()), 0, 1000)
        .await
        .unwrap();
    assert_eq!(first.first().map(|p| p.position), Some(60));
    assert_eq!(first.last().map(|p| p.position), Some(71));
    assert_eq!(first[0].token_id, 9707);
    assert_eq!(first[0].token_text, None, "the text does not cross");
    assert!(first[0].entropy.is_some());
    assert_eq!(first[0].surprisal, None, "no surprisal crossed");
    let second = s
        .range(&RunId(run.clone()), &TurnId("t-2".into()), 0, 1000)
        .await
        .unwrap();
    assert_eq!(second.first().map(|p| p.position), Some(116));
    assert_eq!(second[0].token_id, w.points[12]["token"].as_i64().unwrap());
    let alt = s
        .alternatives_at(&super::PositionKey {
            run: RunId(run.clone()),
            turn: TurnId("t-1".into()),
            position: 60,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!((alt.alternatives, alt.realized), (None, None));

    let tuple = s.tuple(&RunId(run.clone())).await.unwrap().unwrap();
    assert_eq!(tuple.ingest_status, "whole");
    assert_eq!(tuple.ingest_reason, None);
    assert_eq!(tuple.seed.as_deref(), Some("451234785645"));
    assert!(
        tuple.sampler.get("generation_seed").is_none(),
        "{}",
        tuple.sampler
    );
    assert_eq!(tuple.sampler["seed"], json!(451234785645u64));
    assert_eq!((tuple.record_digest, tuple.prefix_length), (None, None));
    assert_eq!(tuple.parent_reference, None);
    assert_eq!(tuple.boundary_set, json!([]));
    let seed: String = sqlx::query_scalar(
        "SELECT generation_seed::text FROM generation WHERE run_id = $1 AND seq = 0",
    )
    .bind(&run)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(seed, "14458752852352082704");

    // The summary's counts move after the ingest; the stored positions do not.
    sqlx::query("UPDATE generation SET resident = resident + 7 WHERE run_id = $1")
        .bind(&run)
        .execute(&s.pool)
        .await
        .unwrap();
    let again = s
        .range(&RunId(run.clone()), &TurnId("t-1".into()), 0, 1000)
        .await
        .unwrap();
    assert_eq!(again.first().map(|p| p.position), Some(60));
}

/// **A replay is compared, never skipped or rewritten** (rulings 14 and
/// 17): replayed equal, the run reads `whole` and nothing doubles; replayed
/// with one point's token changed, the replay is refused in the answer
/// naming the key, and the stored row, its status and its reason stand.
///
/// conforms: web-ingest-is-idempotent-on-the-key
#[tokio::test]
async fn a_replay_is_compared_and_a_differing_one_changes_nothing() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("replay"));
    let run = w.run();
    assert_eq!(ingest(&s, &w).await["ok"], json!(true));
    let again = ingest(&s, &w).await;
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["runs"][0]["status"], json!("whole"));
    assert_eq!(again["runs"][0]["replayed"], json!(true));
    assert_eq!(
        (
            again["runs"][0]["positions"].clone(),
            again["runs"][0]["generations"].clone()
        ),
        (json!(455), json!(2)),
        "an equal replay answers what the store holds"
    );
    assert_eq!(count(&s, "position", &run).await, 455, "nothing doubled");

    let mut changed = w.clone();
    let held = changed.points[3]["token"].as_i64().unwrap();
    changed.points[3]["token"] = json!(held + 1);
    let refused = ingest(&s, &changed).await;
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(refused["runs"][0]["status"], json!("refused"));
    assert_eq!(refused["runs"][0]["stored"], json!("whole"));
    assert_eq!(
        (
            refused["runs"][0]["positions"].clone(),
            refused["runs"][0]["generations"].clone()
        ),
        (json!(0), json!(0)),
        "a refused replay wrote nothing and says so"
    );
    let why = refused["runs"][0]["reason"].as_str().unwrap();
    assert!(why.contains("turn t-1 position 63"), "{why}");
    let token: i64 = sqlx::query_scalar(
        "SELECT token_id FROM position WHERE run_id = $1 AND turn = 't-1' AND position = 63",
    )
    .bind(&run)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(token, held, "the stored row stands");
    let row = landed(&s, &run).await.unwrap();
    assert_eq!((row.status.as_str(), row.reason), ("whole", None));
}

/// **A generation with no resident count lands its summary and not its
/// points, and the run reads `short` naming it** (Spec 3.1, ruling 3):
/// `columns-a` carries no resident count on either generation. It carries
/// no effective sampling either, which alone would refuse it before a row,
/// so this test gives each generation `diagnostic-certified`'s.
#[tokio::test]
async fn a_run_whose_points_cannot_be_addressed_lands_short() {
    let Some(s) = store().await else { return };
    let sampling = Wire::of(CERTIFIED).summary["generations"][0]["effective_sampling"].clone();
    let mut w = Wire::of(COLUMNS_A).tagged(&tag("short"));
    for g in w.generations_mut() {
        g["effective_sampling"] = sampling.clone();
    }
    let run = w.run();
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["ok"], json!(true), "short is a landed run: {answer}");
    assert_eq!(answer["runs"][0]["status"], json!("short"));
    let row = landed(&s, &run).await.unwrap();
    assert_eq!(row.status, "short");
    let why = row.reason.unwrap();
    assert!(
        why.contains("generation 0 (turn t-1): no resident count")
            && why.contains("generation 1 (turn t-2): no resident count"),
        "{why}"
    );
    assert_eq!(count(&s, "generation", &run).await, 2, "the summaries land");
    assert_eq!(
        count(&s, "position", &run).await,
        0,
        "no point is addressed"
    );
}

/// **A point with no turn key does not land; nothing invents a turn**
/// (ruling 16). Built from `diagnostic-certified` with the first
/// generation's turn removed from its entry and its points.
#[tokio::test]
async fn a_point_with_no_turn_key_does_not_land() {
    let Some(s) = store().await else { return };
    let mut w = Wire::of(CERTIFIED).tagged(&tag("turnless"));
    w.summary["generations"][0]
        .as_object_mut()
        .unwrap()
        .remove("turn");
    for p in w.points.iter_mut().take(12) {
        p.as_object_mut().unwrap().remove("turn");
    }
    let run = w.run();
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["runs"][0]["status"], json!("short"), "{answer}");
    let row = landed(&s, &run).await.unwrap();
    assert!(row.reason.unwrap().contains("generation 0: no turn key"));
    assert_eq!(count(&s, "position", &run).await, 443);
    let turn: Option<String> =
        sqlx::query_scalar("SELECT turn FROM generation WHERE run_id = $1 AND seq = 0")
            .bind(&run)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(turn, None);
}

/// **Generations that disagree are a defect refused by name, before any
/// row** (contract 2.2, the rulings' pre-row refusals): a session that
/// differs between the two generations, and two lineages within one run.
///
/// conforms: web-record-members-agree-across-a-run
#[tokio::test]
async fn generations_that_disagree_are_refused_before_a_row() {
    let Some(s) = store().await else { return };
    let mut sessions = Wire::of(CERTIFIED).tagged(&tag("two-sessions"));
    sessions.summary["generations"][1]["session"] = json!("s-other");
    let answer = ingest(&s, &sessions).await;
    assert_eq!(answer["ok"], json!(false));
    assert_eq!(answer["runs"][0]["status"], json!("refused"));
    assert!(
        answer["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("session")
    );
    assert_eq!(landed(&s, &sessions.run()).await, None, "no row");

    let mut lineages = Wire::of(CERTIFIED)
        .tagged(&tag("two-lineages"))
        .branch_of("p-one");
    lineages.summary["generations"][1]["lineage"]["built_from"]["run"] = json!("p-two");
    let answer = ingest(&s, &lineages).await;
    assert!(
        answer["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("lineage"),
        "{answer}"
    );
    assert_eq!(landed(&s, &lineages.run()).await, None);

    let mut digests = Wire::of(SERVING).tagged(&tag("two-digests"));
    digests.summary["generations"][1]["digest"] = json!("0".repeat(64));
    let answer = ingest(&s, &digests).await;
    assert!(
        answer["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("digest"),
        "{answer}"
    );
    assert_eq!(
        landed(&s, &digests.run()).await,
        None,
        "never one run with two"
    );
}

/// **An entropy the generation did not measure lands absent, never zero**
/// (contract 7, Spec 6). Built from `diagnostic-certified` with the first
/// point's entropy removed.
#[tokio::test]
async fn an_absent_entropy_lands_absent() {
    let Some(s) = store().await else { return };
    let mut w = Wire::of(CERTIFIED).tagged(&tag("no-entropy"));
    w.points[0].as_object_mut().unwrap().remove("entropy");
    assert_eq!(ingest(&s, &w).await["ok"], json!(true));
    let points = s
        .range(&RunId(w.run()), &TurnId("t-1".into()), 60, 61)
        .await
        .unwrap();
    assert_eq!(points[0].entropy, None, "absent, not zero");
    assert!(points[1].entropy.is_some());
}

/// **An absent effective sampling or weights hash writes no row**: the
/// sampler is a condition the row is not a row without (contract 2.2), and
/// an absent hash is not the sentinel.
#[tokio::test]
async fn an_absent_sampling_or_weights_hash_writes_no_row() {
    let Some(s) = store().await else { return };
    for member in ["effective_sampling", "weights_hash"] {
        let mut w = Wire::of(CERTIFIED).tagged(&tag(member));
        for g in w.generations_mut() {
            g.as_object_mut().unwrap().remove(member);
        }
        let answer = ingest(&s, &w).await;
        assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
        assert_eq!(landed(&s, &w.run()).await, None, "{member}: no row");
    }
}

/// **Several runs in one emission land each as its own row** (ruling 4),
/// and the answer lists every one. `serving-source`'s digest and seated
/// prefix land where it carries them.
#[tokio::test]
async fn several_runs_in_one_emission_land_each_as_its_own_row() {
    let Some(s) = store().await else { return };
    let t = tag("several");
    let first = Wire::of(CERTIFIED).tagged(&t);
    let second = Wire::of(SERVING).tagged(&t);
    let (a, b) = (first.run(), second.run());
    let answer = ingest(&s, &first.then(second)).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    assert_eq!(answer["runs"].as_array().unwrap().len(), 2);
    assert_eq!(landed(&s, &a).await.unwrap().status, "whole");
    let tuple = s.tuple(&RunId(b)).await.unwrap().unwrap();
    assert_eq!(tuple.ingest_status, "whole");
    assert_eq!(tuple.prefix_length, Some(43));
    assert_eq!(
        tuple.record_digest.as_deref(),
        Some("d64605b28e7b66a40ffab4422dae8cdd07b36ae7061c85fc8b5bc052c1129532")
    );
}

/// **A branch of a held, whole parent is linked and walked** (rulings 8,
/// 9 and 18), over the hand-made branch fixture, which is not an emitter's
/// output: its points are the parent's, so both whole and equal, the paths
/// never part, a known null.
#[tokio::test]
async fn a_branch_of_a_held_whole_parent_is_linked_and_walked() {
    let Some(s) = store().await else { return };
    let t = tag("held");
    let parent = Wire::of(SERVING).tagged(&t);
    let child = Wire::of(HAND_MADE_BRANCH).tagged(&t);
    assert_eq!(ingest(&s, &parent).await["ok"], json!(true));
    let answer = ingest(&s, &child).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(
        row,
        Landed {
            status: "whole".into(),
            reason: None,
            parent_run_id: Some(parent.run()),
            parent_reference: Some(parent.run()),
            parting_position: None,
            parting_known: true,
        }
    );
}

/// **A branch whose parent is not held keeps its reference and no link**,
/// its parting unknown (ruling 8).
#[tokio::test]
async fn a_branch_whose_parent_is_not_held_keeps_its_reference() {
    let Some(s) = store().await else { return };
    let child = Wire::of(HAND_MADE_BRANCH).tagged(&tag("unheld"));
    let answer = ingest(&s, &child).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(row.status, "whole");
    assert!(row.parent_reference.is_some());
    assert_eq!((row.parent_run_id, row.parting_known), (None, false));
}

/// **The walk finds where the paths part** (rulings 21, 22 and 31): a
/// child one position shorter than its whole parent parts at the parent's
/// last position, which only the parent holds; a child with one token
/// changed parts there. Both children are built from the hand-made branch
/// fixture: the first with its last point dropped and its second
/// generation's counts lowered by one so every other position stands, the
/// second with point 100 (the second generation's ordinal 88, position 204)
/// changed.
#[tokio::test]
async fn the_walk_finds_where_the_paths_part() {
    let Some(s) = store().await else { return };
    let t = tag("parts");
    let parent = Wire::of(SERVING).tagged(&t);
    assert_eq!(ingest(&s, &parent).await["ok"], json!(true));

    let mut shorter = Wire::of(HAND_MADE_BRANCH)
        .tagged(&t)
        .renamed(&format!("shorter#{t}"));
    shorter.points.pop();
    shorter.summary["generations"][1]["output_count"] = json!(442);
    shorter.summary["generations"][1]["resident"] = json!(559);
    assert_eq!(ingest(&s, &shorter).await["ok"], json!(true));
    let row = landed(&s, &shorter.run()).await.unwrap();
    assert_eq!((row.parting_known, row.parting_position), (true, Some(558)));

    let mut changed = Wire::of(HAND_MADE_BRANCH)
        .tagged(&t)
        .renamed(&format!("changed#{t}"));
    let held = changed.points[100]["token"].as_i64().unwrap();
    changed.points[100]["token"] = json!(held + 1);
    assert_eq!(ingest(&s, &changed).await["ok"], json!(true));
    let row = landed(&s, &changed.run()).await.unwrap();
    assert_eq!((row.parting_known, row.parting_position), (true, Some(204)));
}

/// **A short child's walk knows only what its retained positions prove**
/// (rulings 21 and 22): a child whose first generation is skipped (its
/// resident count removed) and whose second matches its whole parent is not
/// "never parted", since the skipped span may differ, and a difference it
/// does find in the second generation is not known either, since an
/// earlier, skipped position may hold the first. Both children are built
/// from the hand-made branch fixture.
#[tokio::test]
async fn a_short_childs_walk_knows_only_what_it_retained() {
    let Some(s) = store().await else { return };
    let t = tag("short-child");
    let parent = Wire::of(SERVING).tagged(&t);
    assert_eq!(ingest(&s, &parent).await["ok"], json!(true));
    let skipped = |name: &str| {
        let mut w = Wire::of(HAND_MADE_BRANCH)
            .tagged(&t)
            .renamed(&format!("{name}#{t}"));
        w.summary["generations"][0]
            .as_object_mut()
            .unwrap()
            .remove("resident");
        w
    };
    let matching = skipped("matching");
    assert_eq!(ingest(&s, &matching).await["ok"], json!(true));
    let row = landed(&s, &matching.run()).await.unwrap();
    assert_eq!(row.status, "short");
    assert!(!row.parting_known, "never parted needs both whole: {row:?}");

    let mut differing = skipped("differing");
    let held = differing.points[100]["token"].as_i64().unwrap();
    differing.points[100]["token"] = json!(held + 1);
    assert_eq!(ingest(&s, &differing).await["ok"], json!(true));
    let row = landed(&s, &differing.run()).await.unwrap();
    assert_eq!(row.parent_run_id, Some(parent.run()));
    assert!(!row.parting_known, "a skipped span precedes it: {row:?}");
}

/// **A branch of a parent that is held but not whole is linked and not
/// walked** (rulings 9 and 18): the parent, itself a branch of a run the
/// store does not hold, is left `writing` by the test's hook; its child is
/// linked to it and its parting stays unknown.
#[tokio::test]
async fn a_branch_of_an_incomplete_parent_is_linked_and_not_walked() {
    let Some(s) = store().await else { return };
    let t = tag("incomplete-parent");
    let parent = Wire::of(SERVING)
        .renamed(&format!("parent#{t}"))
        .branch_of(&format!("elsewhere#{t}"));
    ingest_stopping(&s, &parent, Step::AfterPoints).await;
    assert_eq!(landed(&s, &parent.run()).await.unwrap().status, "writing");
    let child = Wire::of(CERTIFIED)
        .renamed(&format!("child#{t}"))
        .branch_of(&parent.run());
    assert_eq!(ingest(&s, &child).await["ok"], json!(true));
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(
        row.parent_run_id,
        Some(parent.run()),
        "the link follows presence"
    );
    assert!(!row.parting_known, "the walk follows completeness: {row:?}");
}

/// **A tape with no coordinate derives no parting** (Spec 3.1's monotone
/// guard): a parent whose positions do not increase in generation order,
/// built from `diagnostic-certified` with its two generations swapped, its
/// points with them, gives the walk nothing to compare on, so the branch is
/// linked and its parting stays unknown.
#[tokio::test]
async fn a_tape_with_no_coordinate_derives_no_parting() {
    let Some(s) = store().await else { return };
    let t = tag("no-coordinate");
    let mut parent = Wire::of(CERTIFIED)
        .tagged(&t)
        .renamed(&format!("swapped#{t}"));
    parent.summary["generations"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    let (first, second) = parent.points.split_at(12);
    parent.points = second.iter().chain(first).cloned().collect();
    assert_eq!(ingest(&s, &parent).await["ok"], json!(true));
    assert_eq!(landed(&s, &parent.run()).await.unwrap().status, "whole");

    let child = Wire::of(CERTIFIED)
        .tagged(&t)
        .renamed(&format!("child#{t}"))
        .branch_of(&parent.run());
    assert_eq!(ingest(&s, &child).await["ok"], json!(true));
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(row.parent_run_id, Some(parent.run()), "linked");
    assert!(!row.parting_known, "and not walked: {row:?}");
}

/// **A parent later in the same emission resolves in the same ingest**
/// (ruling 23): the child listed first is still linked and walked.
#[tokio::test]
async fn a_parent_later_in_the_emission_resolves_in_the_same_ingest() {
    let Some(s) = store().await else { return };
    let t = tag("child-first");
    let parent = Wire::of(SERVING).tagged(&t);
    let child = Wire::of(HAND_MADE_BRANCH).tagged(&t);
    let (p, c) = (parent.run(), child.run());
    let answer = ingest(&s, &child.then(parent)).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let row = landed(&s, &c).await.unwrap();
    assert_eq!(row.parent_run_id, Some(p));
    assert_eq!((row.status.as_str(), row.parting_known), ("whole", true));
}

/// **A chain resolves parents first** (ruling 28): child, mid and root
/// listed in that order, the mid a branch of the root and the child of the
/// mid, all built from the vendored emissions. Every link is set and every
/// walk runs.
#[tokio::test]
async fn a_chain_resolves_parents_first() {
    let Some(s) = store().await else { return };
    let t = tag("chain");
    let root = Wire::of(SERVING).tagged(&t);
    let mid = Wire::of(CERTIFIED)
        .renamed(&format!("mid#{t}"))
        .branch_of(&root.run());
    let child = Wire::of(CERTIFIED)
        .renamed(&format!("child#{t}"))
        .branch_of(&mid.run());
    let (r, m, c) = (root.run(), mid.run(), child.run());
    let answer = ingest(&s, &child.then(mid).then(root)).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let mid = landed(&s, &m).await.unwrap();
    assert_eq!((mid.parent_run_id, mid.parting_known), (Some(r), true));
    let child = landed(&s, &c).await.unwrap();
    assert_eq!((child.parent_run_id, child.parting_known), (Some(m), true));
}

/// **A branch closes only after its resolution** (ruling 27): an ingest
/// stopped by the test's hook after the points and before the resolution
/// leaves the branch `writing`, unlinked, and an equal replay completes it,
/// resolution included.
#[tokio::test]
async fn an_ingest_stopped_before_the_resolution_leaves_the_branch_writing() {
    let Some(s) = store().await else { return };
    let t = tag("stopped");
    let parent = Wire::of(SERVING).tagged(&t);
    let child = Wire::of(HAND_MADE_BRANCH).tagged(&t);
    assert_eq!(ingest(&s, &parent).await["ok"], json!(true));
    let stopped = ingest_stopping(&s, &child, Step::AfterPoints).await;
    assert_eq!(stopped["ok"], json!(false));
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!((row.status.as_str(), row.parent_run_id), ("writing", None));
    assert_eq!(count(&s, "position", &child.run()).await, 455);

    let completed = ingest(&s, &child).await;
    assert_eq!(completed["ok"], json!(true), "{completed}");
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(row.status, "whole");
    assert_eq!(
        (row.parent_run_id, row.parting_known),
        (Some(parent.run()), true)
    );
}

/// **A reference cycle is refused in one transaction** (rulings 28 and 30):
/// two disjoint cycles in one emission, each refused with its own reason
/// naming it; and a cycle whose ingest the test's hook stops inside the
/// cycle's transaction leaves every row `writing`, which the next ingest
/// reports refused in its answer and leaves so (ruling 29).
#[tokio::test]
async fn a_reference_cycle_is_refused_in_one_transaction() {
    let Some(s) = store().await else { return };
    let t = tag("cycles");
    let run = |name: &str, parent: &str| {
        Wire::of(CERTIFIED)
            .renamed(&format!("{name}#{t}"))
            .branch_of(&format!("{parent}#{t}"))
    };
    let emission = run("a", "b")
        .then(run("b", "a"))
        .then(run("c", "d"))
        .then(run("d", "c"));
    let answer = ingest(&s, &emission).await;
    assert_eq!(answer["ok"], json!(false));
    for (name, cycle) in [("a", "a#"), ("b", "a#"), ("c", "c#"), ("d", "c#")] {
        let row = landed(&s, &format!("{name}#{t}")).await.unwrap();
        assert_eq!(row.status, "refused", "{name}");
        let why = row.reason.unwrap();
        assert!(
            why.contains("reference cycle") && why.contains(cycle),
            "{name}: {why}"
        );
    }

    let t = tag("cycle-stopped");
    let run = |name: &str, parent: &str| {
        Wire::of(CERTIFIED)
            .renamed(&format!("{name}#{t}"))
            .branch_of(&format!("{parent}#{t}"))
    };
    let emission = run("a", "b").then(run("b", "a"));
    let stopped = ingest_stopping(&s, &emission, Step::AfterCycleRow).await;
    assert_eq!(stopped["ok"], json!(false));
    for name in ["a", "b"] {
        let row = landed(&s, &format!("{name}#{t}")).await.unwrap();
        assert_eq!(row.status, "writing", "{name}: both or neither");
    }
    let next = ingest(&s, &emission).await;
    for (i, name) in ["a", "b"].iter().enumerate() {
        assert_eq!(next["runs"][i]["status"], json!("refused"), "{next}");
        assert_eq!(next["runs"][i]["stored"], json!("writing"));
        let row = landed(&s, &format!("{name}#{t}")).await.unwrap();
        assert_eq!(row.status, "writing", "{name}: left so");
    }
}

/// **A cycle's refusal is persisted only on rows this ingest created**
/// (ruling 29): A, a branch of B, is left `writing` by the hook; A replayed
/// equal beside a new B naming A closes a cycle, and A stays `writing` with
/// its refusal in the answer alone while B is persisted `refused`.
#[tokio::test]
async fn a_cycle_never_changes_a_row_this_ingest_did_not_create() {
    let Some(s) = store().await else { return };
    let t = tag("cycle-replayed");
    let a = Wire::of(CERTIFIED)
        .renamed(&format!("a#{t}"))
        .branch_of(&format!("b#{t}"));
    let b = Wire::of(CERTIFIED)
        .renamed(&format!("b#{t}"))
        .branch_of(&format!("a#{t}"));
    ingest_stopping(&s, &a, Step::AfterPoints).await;
    assert_eq!(landed(&s, &a.run()).await.unwrap().status, "writing");
    let answer = ingest(&s, &a.clone().then(b.clone())).await;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    assert_eq!(answer["runs"][0]["stored"], json!("writing"));
    let row = landed(&s, &a.run()).await.unwrap();
    assert_eq!((row.status.as_str(), row.reason), ("writing", None));
    assert_eq!(landed(&s, &b.run()).await.unwrap().status, "refused");
}

/// **An emission the reader cannot address lands nothing**: counts that
/// disagree with the points, and an empty input.
#[tokio::test]
async fn an_unreadable_emission_lands_nothing() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("unreadable"));
    let mut text = w.text();
    text = text.replacen("\"positions\":455", "\"positions\":456", 1);
    let answer = s.ingest(text.as_bytes()).await.value;
    assert_eq!(answer["ok"], json!(false));
    assert!(
        answer["error"]
            .as_str()
            .unwrap()
            .contains("counts 456 positions"),
        "{answer}"
    );
    assert_eq!(landed(&s, &w.run()).await, None);
    let empty = s.ingest(&b""[..]).await.value;
    assert!(empty["error"].as_str().unwrap().contains("empty"));
}

/// **A cycle closed through a stored, closed run is found** (Codex pass
/// one on PR #23): A, a branch of a B the store does not hold, lands and
/// closes `whole`; replayed equal beside a new B naming A, it closes a
/// cycle through itself, and B is refused while A, which this ingest did
/// not create, is reported refused in the answer and stays `whole`.
#[tokio::test]
async fn a_cycle_through_a_replayed_closed_run_is_refused() {
    let Some(s) = store().await else { return };
    let t = tag("cycle-closed");
    let a = Wire::of(CERTIFIED)
        .renamed(&format!("a#{t}"))
        .branch_of(&format!("b#{t}"));
    let b = Wire::of(CERTIFIED)
        .renamed(&format!("b#{t}"))
        .branch_of(&format!("a#{t}"));
    assert_eq!(ingest(&s, &a).await["ok"], json!(true));
    assert_eq!(landed(&s, &a.run()).await.unwrap().status, "whole");
    let answer = ingest(&s, &a.clone().then(b.clone())).await;
    assert_eq!(answer["ok"], json!(false), "{answer}");
    assert_eq!(answer["runs"][0]["status"], json!("refused"));
    assert_eq!(answer["runs"][0]["stored"], json!("whole"));
    let row = landed(&s, &b.run()).await.unwrap();
    assert_eq!(row.status, "refused", "B closes the cycle: {row:?}");
    assert!(row.reason.unwrap().contains("reference cycle"));
    assert_eq!(landed(&s, &a.run()).await.unwrap().status, "whole");
}

/// **A cycle closed through a stored row the emission does not name is
/// found too**: A, stored `whole` and naming a B the store does not hold;
/// an emission carrying only B, naming A. B is refused, and A is reported
/// refused in the answer with its stored status, its row untouched.
#[tokio::test]
async fn a_cycle_through_a_stored_row_alone_is_refused() {
    let Some(s) = store().await else { return };
    let t = tag("cycle-stored");
    let a = Wire::of(CERTIFIED)
        .renamed(&format!("a#{t}"))
        .branch_of(&format!("b#{t}"));
    let b = Wire::of(CERTIFIED)
        .renamed(&format!("b#{t}"))
        .branch_of(&format!("a#{t}"));
    assert_eq!(ingest(&s, &a).await["ok"], json!(true));
    let answer = ingest(&s, &b).await;
    assert_eq!(answer["ok"], json!(false), "{answer}");
    assert_eq!(landed(&s, &b.run()).await.unwrap().status, "refused");
    let reported = answer["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["run"] == json!(a.run()))
        .unwrap_or_else(|| panic!("A is reported: {answer}"));
    assert_eq!(reported["status"], json!("refused"));
    assert_eq!(reported["stored"], json!("whole"));
    assert_eq!(landed(&s, &a.run()).await.unwrap().status, "whole");
}

/// **An equal replay of a closed run answers with what the store holds**
/// (Codex pass one on PR #23): a replayed `short` run carries its reason,
/// and a replayed resolved branch its link and its parting.
#[tokio::test]
async fn an_equal_replay_answers_with_what_the_store_holds() {
    let Some(s) = store().await else { return };
    let t = tag("replay-answer");
    let sampling = Wire::of(CERTIFIED).summary["generations"][0]["effective_sampling"].clone();
    let mut short = Wire::of(COLUMNS_A).tagged(&t);
    for g in short.generations_mut() {
        g["effective_sampling"] = sampling.clone();
    }
    ingest(&s, &short).await;
    let again = ingest(&s, &short).await;
    assert_eq!(again["runs"][0]["status"], json!("short"), "{again}");
    assert!(
        again["runs"][0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("no resident count")),
        "{again}"
    );

    let parent = Wire::of(SERVING).tagged(&t);
    ingest(&s, &parent).await;
    let mut child = Wire::of(HAND_MADE_BRANCH)
        .tagged(&t)
        .renamed(&format!("child#{t}"));
    let held = child.points[100]["token"].as_i64().unwrap();
    child.points[100]["token"] = json!(held + 1);
    ingest(&s, &child).await;
    let again = ingest(&s, &child).await;
    let out = &again["runs"][0];
    assert_eq!(out["replayed"], json!(true), "{again}");
    assert_eq!(out["parent_linked"], json!(true), "{again}");
    assert_eq!(out["parting_known"], json!(true), "{again}");
    assert_eq!(out["parting_position"], json!(204), "{again}");
}

/// One defect, planted in an emission by editing it.
type Plant = Box<dyn Fn(&mut Wire)>;

/// **Every constraint the schema holds a row to is met while planning**
/// (Codex pass five on PR #23): each defect below is planted in one run of a
/// two-run emission, before a good run. The planted run is refused by name
/// before a row, and the good run after it lands `whole`: a store refusal
/// at the insert would have failed the whole ingest instead.
#[tokio::test]
async fn a_member_the_schema_refuses_is_refused_while_planning() {
    let Some(s) = store().await else { return };
    let past_i32 = json!(i32::MAX as u64 + 1);
    let plants: Vec<(&str, Plant)> = vec![
        (
            "a digest that is not sha256 hex",
            Box::new(|w: &mut Wire| {
                for g in w.generations_mut() {
                    g["digest"] = json!("XYZ");
                }
            }),
        ),
        (
            "a NUL in the session",
            Box::new(|w: &mut Wire| {
                for g in w.generations_mut() {
                    g["session"] = json!("s\u{0}x");
                }
            }),
        ),
        (
            "a NUL in the run identity",
            Box::new(|w: &mut Wire| {
                let run = format!("{}\u{0}", w.run());
                for g in w.generations_mut() {
                    g["run"] = json!(run);
                }
            }),
        ),
        (
            "a NUL in a turn key",
            Box::new(|w: &mut Wire| {
                w.summary["generations"][0]["turn"] = json!("t\u{0}1");
                for p in w.points.iter_mut().take(12) {
                    p["turn"] = json!("t\u{0}1");
                }
            }),
        ),
        (
            "a NUL in the effective sampling",
            Box::new(|w: &mut Wire| {
                for g in w.generations_mut() {
                    g["effective_sampling"]["sampler"] = json!("top\u{0}k");
                }
            }),
        ),
        (
            "a seated prefix past INTEGER",
            Box::new({
                let past = past_i32.clone();
                move |w: &mut Wire| {
                    for g in w.generations_mut() {
                        g["prefix_length"] = past.clone();
                    }
                }
            }),
        ),
        (
            "a field depth past INTEGER",
            Box::new({
                let past = past_i32.clone();
                move |w: &mut Wire| {
                    for g in w.generations_mut() {
                        g["field_depth"] = past.clone();
                    }
                }
            }),
        ),
        (
            "a resident count past INTEGER",
            Box::new({
                let past = past_i32.clone();
                move |w: &mut Wire| {
                    w.summary["generations"][0]["resident"] = past.clone();
                }
            }),
        ),
        (
            "a run identity past KEY_BOUND",
            Box::new(|w: &mut Wire| {
                // Characters that do not repeat, so the store cannot compress
                // the key under its btree limit and the bound is what is tested.
                let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
                let run: String = (0..4096)
                    .map(|_| {
                        x ^= x << 13;
                        x ^= x >> 7;
                        x ^= x << 17;
                        char::from(b'a' + (x % 26) as u8)
                    })
                    .collect();
                for g in w.generations_mut() {
                    g["run"] = json!(run);
                }
            }),
        ),
        (
            "a token past BIGINT",
            Box::new(|w: &mut Wire| {
                w.points[0]["token"] = json!(i64::MAX as u64 + 1);
            }),
        ),
    ];
    for (i, (what, plant)) in plants.iter().enumerate() {
        let t = tag(&format!("plant-{i}"));
        let mut bad = Wire::of(CERTIFIED).renamed(&format!("bad#{t}"));
        plant(&mut bad);
        let good = Wire::of(SERVING).tagged(&t);
        let answer = ingest(&s, &bad.clone().then(good.clone())).await;
        assert!(
            answer.get("error").is_none(),
            "{what}: the store refused rather than the planner: {answer}"
        );
        assert_eq!(
            answer["runs"][0]["status"],
            json!("refused"),
            "{what}: {answer}"
        );
        assert_eq!(
            answer["runs"][1]["status"],
            json!("whole"),
            "{what}: {answer}"
        );
        assert_eq!(
            landed(&s, &good.run()).await.unwrap().status,
            "whole",
            "{what}"
        );
    }
}

/// **The cycle scan and the resolution order are linear in the runs and
/// references** (Codex pass six on PR #23): a bound test and not a failing
/// one. A chain of fifty thousand branches, each naming the one before and
/// listed child first, and the same chain closed into one cycle, are each
/// scanned and ordered within two seconds in a debug build; the quadratic
/// forms, a linear search of the path for the return and a depth re-walked
/// per branch, take far longer at this size. Fifty thousand and not ten,
/// so the two forms are apart by more than a slow machine's margin.
#[test]
fn the_cycle_scan_and_the_order_are_linear() {
    use super::ingest::plan_resolution;
    use std::collections::HashMap;
    let n = 50_000;
    let started = std::time::Instant::now();
    let open: Vec<String> = (0..n).rev().map(|i| format!("r{i}")).collect();
    let mut references: HashMap<String, Option<String>> = (0..n)
        .map(|i| (format!("r{i}"), (i > 0).then(|| format!("r{}", i - 1))))
        .collect();
    let (cycles, order) = plan_resolution(&open, &references);
    assert!(cycles.is_empty());
    assert_eq!(
        order.first().map(String::as_str),
        Some("r0"),
        "parents first"
    );
    assert_eq!(order.last(), Some(&format!("r{}", n - 1)));

    references.insert("r0".into(), Some(format!("r{}", n - 1)));
    let (cycles, order) = plan_resolution(&open, &references);
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].len(), n);
    assert!(order.is_empty());
    let took = started.elapsed();
    assert!(took < std::time::Duration::from_secs(2), "took {took:?}");
}

/// **A tape that repeats a coordinate gives the walk nothing to compare
/// on** (Codex pass seven on PR #23): a branch whose second generation,
/// moved onto the first's turn, starts at the first's last position with
/// the same token, the repeat folded to one stored row. Against a whole
/// parent the walk would otherwise find a parting; the plan records the
/// repeat and the parting stays unknown. Built from the hand-made branch
/// fixture.
#[tokio::test]
async fn a_tape_that_repeats_a_coordinate_derives_no_parting() {
    let Some(s) = store().await else { return };
    let t = tag("repeat");
    let parent = Wire::of(SERVING).tagged(&t);
    assert_eq!(ingest(&s, &parent).await["ok"], json!(true));
    let mut child = Wire::of(HAND_MADE_BRANCH)
        .tagged(&t)
        .renamed(&format!("child#{t}"));
    // The second generation onto the first's turn, its floor at 71, the
    // first generation's last position.
    child.summary["generations"][1]["turn"] = json!("t-1");
    child.summary["generations"][1]["resident"] = json!(71 + 443 + 1);
    for p in child.points.iter_mut().skip(12) {
        p["turn"] = json!("t-1");
    }
    let last = child.points[11].clone();
    child.points[12]["token"] = last["token"].clone();
    child.points[12]["entropy"] = last["entropy"].clone();
    let answer = ingest(&s, &child).await;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(
        (row.status.as_str(), row.parent_run_id.clone()),
        ("whole", Some(parent.run()))
    );
    assert!(
        !row.parting_known,
        "a repeated coordinate derives nothing: {row:?}"
    );
    assert_eq!(
        count(&s, "position", &child.run()).await,
        454,
        "one row per key"
    );
}

/// **A replay compares every run column the ingest writes, the constant
/// ones included** (Codex pass nine on PR #23): a stored run whose boundary
/// set is not the empty set the ingest writes, replayed equal in everything
/// else, is refused naming the member, the stored row untouched.
#[tokio::test]
async fn a_replay_compares_the_boundary_set_it_writes() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("boundary"));
    assert_eq!(ingest(&s, &w).await["ok"], json!(true));
    sqlx::query("UPDATE run SET boundary_set = '[\"loopback\"]'::jsonb WHERE run_id = $1")
        .bind(w.run())
        .execute(&s.pool)
        .await
        .unwrap();
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    assert!(
        answer["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("boundary_set"),
        "{answer}"
    );
    let held: Value = sqlx::query_scalar("SELECT boundary_set FROM run WHERE run_id = $1")
        .bind(w.run())
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert_eq!(held, json!(["loopback"]), "the stored row stands");
}

/// Two `Notify`s for a test's hold on an ingest.
fn hold() -> Hold {
    Hold {
        at: HoldAt::AfterLocks,
        locked: std::sync::Arc::new(tokio::sync::Notify::new()),
        release: std::sync::Arc::new(tokio::sync::Notify::new()),
    }
}

/// An ingest of `w` spawned on its own task, under `options`.
fn spawn_ingest(s: &Store, w: &Wire, options: Options) -> tokio::task::JoinHandle<Value> {
    let (s, text) = (s.clone(), w.text());
    tokio::spawn(async move {
        s.ingest_with(Emission::read(text.as_bytes()), &options)
            .await
            .value
    })
}

/// **One ingest writes a run at a time; the rest wait and replay** (Spec
/// 3.1, the operator's ruling of 2026-10-06 on PR #23): an ingest holds the
/// run's lock and is held there by the test; a second ingest of the same run
/// waits for it, writing nothing, and once the first finishes meets the run
/// whole and replays it.
#[tokio::test]
async fn a_second_ingest_of_a_run_waits_and_replays() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("waits"));
    let first_hold = hold();
    let first = spawn_ingest(
        &s,
        &w,
        Options {
            hold: Some(first_hold.clone()),
            ..Options::default()
        },
    );
    first_hold.locked.notified().await;
    let second = spawn_ingest(&s, &w, Options::default());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !second.is_finished(),
        "the second ingest waits for the run's lock"
    );
    assert_eq!(
        landed(&s, &w.run()).await,
        None,
        "nothing is written meanwhile"
    );
    first_hold.release.notify_one();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(first["runs"][0]["status"], json!("whole"), "{first}");
    assert_eq!(first["runs"][0]["replayed"], json!(false));
    assert_eq!(second["runs"][0]["status"], json!("whole"), "{second}");
    assert_eq!(
        second["runs"][0]["replayed"],
        json!(true),
        "the second replays"
    );
    assert_eq!(count(&s, "position", &w.run()).await, 455);
}

/// **Two concurrent ingests creating opposite references never both land
/// whole**: one lands A naming B and is held on its locks; the other, of B
/// naming A, shares them and waits; the first lands A with B absent, and the
/// second then finds the cycle through A and refuses B.
#[tokio::test]
async fn concurrent_opposite_references_never_both_land_whole() {
    let Some(s) = store().await else { return };
    let t = tag("opposite");
    let a = Wire::of(CERTIFIED)
        .renamed(&format!("a#{t}"))
        .branch_of(&format!("b#{t}"));
    let b = Wire::of(CERTIFIED)
        .renamed(&format!("b#{t}"))
        .branch_of(&format!("a#{t}"));
    let first_hold = hold();
    let first = spawn_ingest(
        &s,
        &a,
        Options {
            hold: Some(first_hold.clone()),
            ..Options::default()
        },
    );
    first_hold.locked.notified().await;
    let second = spawn_ingest(&s, &b, Options::default());
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    first_hold.release.notify_one();
    let (_, second) = (first.await.unwrap(), second.await.unwrap());
    let (a, b) = (
        landed(&s, &a.run()).await.unwrap(),
        landed(&s, &b.run()).await.unwrap(),
    );
    assert!(
        !(a.status == "whole" && b.status == "whole"),
        "both landed whole on a cycle: {a:?} {b:?}"
    );
    assert_eq!(b.status, "refused", "{second}");
}

/// **The replay compares every column the insert writes**, the three rows
/// each defined once in `rows.rs`: the insert's column list is the row's
/// declaration (beside the identity and the status), the loader selects
/// every declared member, and the comparison names every declared member on
/// which two rows differ.
#[test]
fn the_replay_compares_every_column_the_insert_writes() {
    use super::rows::{
        Column, GenerationRow, INSERT_GENERATIONS, INSERT_POSITIONS, INSERT_RUN, PositionRow,
        RunMembers, SELECT_GENERATIONS, SELECT_POSITIONS, SELECT_RUN,
    };
    let inserted = |sql: &str| -> Vec<String> {
        let open = sql.find('(').unwrap();
        let close = sql[open..].find(')').unwrap() + open;
        sql[open + 1..close]
            .split(", ")
            .map(str::to_owned)
            .collect()
    };
    let declared =
        |columns: &[Column]| -> Vec<String> { columns.iter().map(|c| c.name.to_owned()).collect() };
    let with = |head: &[&str], middle: Vec<String>, tail: &[&str]| -> Vec<String> {
        head.iter()
            .map(|s| s.to_string())
            .chain(middle)
            .chain(tail.iter().map(|s| s.to_string()))
            .collect()
    };
    assert_eq!(
        inserted(&INSERT_RUN),
        with(
            &["run_id"],
            declared(RunMembers::COLUMNS),
            &["ingest_status"]
        )
    );
    assert_eq!(
        inserted(&INSERT_GENERATIONS),
        with(&["run_id"], declared(GenerationRow::COLUMNS), &[])
    );
    assert_eq!(
        inserted(&INSERT_POSITIONS),
        with(&["run_id"], declared(PositionRow::COLUMNS), &[])
    );
    for (select, columns) in [
        (&*SELECT_RUN, RunMembers::COLUMNS),
        (&*SELECT_GENERATIONS, GenerationRow::COLUMNS),
        (&*SELECT_POSITIONS, PositionRow::COLUMNS),
    ] {
        for c in columns {
            assert!(select.contains(c.select), "{} unread: {select}", c.name);
        }
    }
    let a = RunMembers {
        record_identity: "a".into(),
        seed: Some("1".into()),
        sampler: json!({"a": 1}),
        device: Some("a".into()),
        engine: Some(json!({"a": 1})),
        field_depth: Some(1),
        record_session: Some("a".into()),
        record_digest: Some("a".into()),
        prefix_length: Some(1),
        parent_reference: Some("a".into()),
        boundary_set: json!([]),
        task_source: None,
        task_identity: None,
        forced_position: None,
        forced_token: None,
        branch_position: None,
        signature: None,
    };
    let b = RunMembers {
        record_identity: "b".into(),
        seed: Some("2".into()),
        sampler: json!({"b": 2}),
        device: Some("b".into()),
        engine: Some(json!({"b": 2})),
        field_depth: Some(2),
        record_session: Some("b".into()),
        record_digest: Some("b".into()),
        prefix_length: Some(2),
        parent_reference: Some("b".into()),
        boundary_set: json!(["b"]),
        task_source: Some("b".into()),
        task_identity: Some("b".into()),
        forced_position: Some(1),
        forced_token: Some("b".into()),
        branch_position: Some(1),
        signature: Some(json!({"b": 1})),
    };
    assert_eq!(
        a.differing(&b),
        declared(RunMembers::COLUMNS),
        "every declared member is compared"
    );
}

/// **No column of a landed run is written outside the declaration**: after
/// an ingest, every column the run row holds a value in is a declared member
/// or the ingest's bookkeeping, so the replay's comparison, which covers the
/// members, covers every column the ingest writes.
#[tokio::test]
async fn every_column_an_ingest_writes_is_a_declared_member() {
    let Some(s) = store().await else { return };
    let w = Wire::of(SERVING).tagged(&tag("columns"));
    assert_eq!(ingest(&s, &w).await["ok"], json!(true));
    let row: Value = sqlx::query_scalar("SELECT to_jsonb(run) FROM run WHERE run_id = $1")
        .bind(w.run())
        .fetch_one(&s.pool)
        .await
        .unwrap();
    let members: Vec<&str> = super::rows::RunMembers::COLUMNS
        .iter()
        .map(|c| c.name)
        .collect();
    let bookkeeping = [
        "run_id",
        "ingest_status",
        "ingest_reason",
        "parent_run_id",
        "parting_position",
        "parting_known",
        "ingested_at",
    ];
    for (column, value) in row.as_object().unwrap() {
        if value.is_null() || (column == "parting_known" && *value == json!(false)) {
            continue;
        }
        assert!(
            members.contains(&column.as_str()) || bookkeeping.contains(&column.as_str()),
            "the ingest wrote {column} outside the declaration"
        );
    }
}

/// One synthetic run: its identity, its parent, and its generations'
/// output counts.
type SyntheticRun = (String, Option<String>, Vec<usize>);

/// An emission of synthetic runs, each `(run, parent, output counts)`, its
/// positions rising from zero in each run and its tokens the positions: no
/// record carries one, and the sweep below needs sizes no fixture has.
fn synthetic(runs: &[SyntheticRun]) -> Wire {
    let sampling = Wire::of(CERTIFIED).summary["generations"][0]["effective_sampling"].clone();
    let mut generations = Vec::new();
    let mut points = Vec::new();
    for (run, parent, counts) in runs {
        let mut floor = 0usize;
        for &count in counts {
            let mut entry = json!({
                "turn": "t-1", "resident": floor + count + 1, "output_count": count,
                "weights_hash": "ab", "run": run, "session": "s-sweep",
                "effective_sampling": sampling,
            });
            if let Some(parent) = parent {
                entry["lineage"] = json!({
                    "save_point": "0", "run": parent, "sequence": 1, "turn": 1,
                    "operator_supplied": false,
                    "built_from": {"parent": "s-sweep", "run": parent, "through": 1},
                });
            }
            generations.push(entry);
            for j in 0..count {
                points
                    .push(json!({"turn": "t-1", "ordinal": j, "token": floor + j, "entropy": 1.0}));
            }
            floor += count;
        }
    }
    Wire {
        summary: json!({"positions": points.len(), "with_entropy": points.len(),
            "with_surprisal": 0, "generations": generations}),
        points,
    }
}

/// **Every loop that reads or writes the store per item stays within one
/// bound** (the sweep the operator asked for on PR #23): a bound test and
/// not a failing one. Each row is a large synthetic input to one such loop,
/// and each lands within sixty seconds in a debug build; the last four
/// passes' quadratic forms each took minutes on its row. The pure scan and
/// order are held apart by `the_cycle_scan_and_the_order_are_linear`.
#[tokio::test]
async fn every_per_item_loop_lands_within_one_bound() {
    let Some(s) = store().await else { return };
    // A bound asserted against the shared scratch server, so held alone
    // against the link's and the other heavy tests.
    let _exclusive = super::read::tests::exclusive().lock().await;
    let t = tag("sweep");
    let id = |name: &str, i: usize| format!("{name}-{i}#{t}");
    let chain = 1_000;
    let cases: Vec<(&str, Vec<SyntheticRun>, &str)> = vec![
        (
            "many runs",
            (0..2_000).map(|i| (id("runs", i), None, vec![1])).collect(),
            "whole",
        ),
        (
            "a long chain, child first",
            (0..chain)
                .rev()
                .map(|i| (id("chain", i), (i > 0).then(|| id("chain", i - 1)), vec![1]))
                .collect(),
            "whole",
        ),
        (
            "many generations",
            vec![(id("generations", 0), None, vec![1; 20_000])],
            "whole",
        ),
        (
            "many points in one generation",
            vec![(id("points", 0), None, vec![100_000])],
            "whole",
        ),
        (
            "many empty generations behind a large one",
            vec![(id("empty", 0), None, {
                let mut counts = vec![20_000];
                counts.extend(std::iter::repeat_n(0, 5_000));
                counts
            })],
            "whole",
        ),
        (
            "many branches on one parent",
            std::iter::once((id("fan", 0), None, vec![1]))
                .chain((1..2_000).map(|i| (id("fan", i), Some(id("fan", 0)), vec![1])))
                .collect(),
            "whole",
        ),
        (
            "a long cycle",
            (0..chain)
                .map(|i| (id("cycle", i), Some(id("cycle", (i + 1) % chain)), vec![1]))
                .collect(),
            "refused",
        ),
    ];
    for (name, runs, expected) in cases {
        let w = synthetic(&runs);
        let started = std::time::Instant::now();
        let answer = ingest(&s, &w).await;
        let took = started.elapsed();
        let landed = answer["runs"].as_array().unwrap();
        assert_eq!(landed.len(), runs.len(), "{name}: {}", answer["error"]);
        assert!(
            landed.iter().all(|r| r["status"] == json!(expected)),
            "{name}: not every run {expected}: {}",
            answer["error"]
        );
        assert!(
            took < std::time::Duration::from_secs(60),
            "{name} took {took:?}"
        );
        // **A reason's size never grows with the input** (Codex pass
        // fifteen): a cycle's reason names two identities and a count, so
        // every reason here, the long cycle's included, stays under one
        // bound however many runs the cycle has.
        let bound = 2 * super::rows::KEY_BOUND + 100;
        for r in landed {
            if let Some(reason) = r["reason"].as_str() {
                assert!(
                    reason.len() <= bound,
                    "{name}: a reason of {} bytes, past {bound}",
                    reason.len()
                );
            }
        }
    }
}

/// **A cycle closed by ingests that share no lock is refused at the
/// resolution**: the recheck under the run locks, which the cycle scan cannot
/// stand in for where two ingests' runs are disjoint. B naming C and D naming
/// A land first, each `whole` with its parent absent. An ingest of A naming B
/// is held after its scan, which found C absent; an ingest of C naming D,
/// sharing none of its locks, lands meanwhile and refuses C on the cycle its
/// own scan finds. A's resolution then follows B, C and D back to A and
/// refuses A rather than closing it `whole` on the cycle. The run identities
/// are tagged until A's and B's locks share no bucket with C's and D's, so
/// the two ingests cannot wait on each other.
#[tokio::test]
async fn a_cycle_closed_by_ingests_sharing_no_lock_is_refused_at_resolution() {
    let Some(s) = store().await else { return };
    let ids = loop {
        let t = tag("disjoint");
        let ids: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .map(|n| format!("{n}#{t}"))
            .collect();
        let buckets: Vec<i32> =
            sqlx::query_scalar("SELECT hashtext(r) & 1023 FROM unnest($1::text[]) AS r")
                .bind(&ids)
                .fetch_all(&s.pool)
                .await
                .unwrap();
        if buckets[0] != buckets[2]
            && buckets[0] != buckets[3]
            && buckets[1] != buckets[2]
            && buckets[1] != buckets[3]
        {
            break ids;
        }
    };
    let run =
        |i: usize, parent: usize| Wire::of(CERTIFIED).renamed(&ids[i]).branch_of(&ids[parent]);
    assert_eq!(ingest(&s, &run(1, 2)).await["ok"], json!(true));
    assert_eq!(ingest(&s, &run(3, 0)).await["ok"], json!(true));
    let held = Hold {
        at: HoldAt::BeforeResolve,
        ..hold()
    };
    let first = spawn_ingest(
        &s,
        &run(0, 1),
        Options {
            hold: Some(held.clone()),
            ..Options::default()
        },
    );
    held.locked.notified().await;
    let c = tokio::time::timeout(std::time::Duration::from_secs(30), ingest(&s, &run(2, 3)))
        .await
        .expect("the two ingests share no lock");
    assert_eq!(c["runs"][0]["status"], json!("refused"), "{c}");
    held.release.notify_one();
    let a = first.await.unwrap();
    assert_eq!(a["runs"][0]["status"], json!("refused"), "{a}");
    let row = landed(&s, &ids[0]).await.unwrap();
    assert_eq!(row.status, "refused", "A is not closed whole on the cycle");
    assert!(row.reason.unwrap().contains("reference cycle"));
}

/// **A waiter holds no work capacity** (Codex pass ten on PR #23): more
/// concurrent ingests of one run than the store's pool holds connections,
/// all on one store, each waiting on the run's lock in turn, and every one
/// answers within the sweep's bound. With the lock's session taken from the
/// pool, the waiters would hold every connection and the lock's owner could
/// never get one to write with.
#[tokio::test]
async fn more_ingests_of_a_run_than_the_pool_holds_all_answer() {
    let Some(s) = store().await else { return };
    // A bound asserted against the shared scratch server, so held alone
    // against the link's and the other heavy tests.
    let _exclusive = super::read::tests::exclusive().lock().await;
    let w = Wire::of(CERTIFIED).tagged(&tag("many-waiters"));
    let waiters = s.pool.options().get_max_connections() as usize + 1;
    let ingests: Vec<_> = (0..waiters)
        .map(|_| spawn_ingest(&s, &w, Options::default()))
        .collect();
    let answers = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        let mut answers = Vec::new();
        for ingest in ingests {
            answers.push(ingest.await.unwrap());
        }
        answers
    })
    .await
    .expect("every ingest answers; none waits forever on the pool");
    for answer in &answers {
        assert_eq!(answer["runs"][0]["status"], json!("whole"), "{answer}");
    }
    let created = answers
        .iter()
        .filter(|a| a["runs"][0]["replayed"] == json!(false))
        .count();
    assert_eq!(created, 1, "one wrote the run, the rest replayed it");
}

/// **A run the first pass refused answers once, though it lies on a
/// cycle** (Codex pass eleven on PR #23): X is stored naming a Y the store
/// does not hold; an emission replays X with one token changed, which is
/// refused, beside a new Y naming X, which closes a cycle through X's stored
/// row. The answer holds one object for X, refused with the replay's own
/// reason and `stored` set, and Y is refused on the cycle.
#[tokio::test]
async fn a_run_refused_in_the_first_pass_answers_once_on_a_cycle() {
    let Some(s) = store().await else { return };
    let t = tag("refused-on-cycle");
    let x = Wire::of(CERTIFIED)
        .renamed(&format!("x#{t}"))
        .branch_of(&format!("y#{t}"));
    let y = Wire::of(CERTIFIED)
        .renamed(&format!("y#{t}"))
        .branch_of(&format!("x#{t}"));
    assert_eq!(ingest(&s, &x).await["ok"], json!(true));
    let mut changed = x.clone();
    let held = changed.points[3]["token"].as_i64().unwrap();
    changed.points[3]["token"] = json!(held + 1);
    let answer = ingest(&s, &changed.then(y.clone())).await;
    let for_x: Vec<&Value> = answer["runs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["run"] == json!(x.run()))
        .collect();
    assert_eq!(for_x.len(), 1, "one object for X: {answer}");
    assert_eq!(for_x[0]["status"], json!("refused"));
    assert!(
        for_x[0]["reason"].as_str().unwrap().contains("differs"),
        "X keeps its own reason: {answer}"
    );
    assert_eq!(for_x[0]["stored"], json!("whole"));
    assert_eq!(landed(&s, &y.run()).await.unwrap().status, "refused");
}

/// **A cycle row this ingest did not create is reported with its status at
/// the refusal**, not at the earlier read of the references (Codex pass
/// eleven on PR #23): a cycle X, Y, Z, W, with Y and W landed first. An
/// ingest of Z, whose locks share none with this one's, is held before its
/// resolution with Z `writing`; this ingest of X reads the references,
/// finds the cycle through Z and is held before its refusal; the first is
/// released, refuses Z on the cycle its own recheck finds, and ends; this
/// one is released and reports Z `refused`, as the store holds it at the
/// refusal, never the `writing` it read before.
#[tokio::test]
async fn a_cycle_row_owned_elsewhere_is_reported_as_of_the_refusal() {
    let Some(s) = store().await else { return };
    let ids = loop {
        let t = tag("moved-status");
        let ids: Vec<String> = ["x", "y", "z", "w"]
            .iter()
            .map(|n| format!("{n}#{t}"))
            .collect();
        let buckets: Vec<i32> =
            sqlx::query_scalar("SELECT hashtext(r) & 1023 FROM unnest($1::text[]) AS r")
                .bind(&ids)
                .fetch_all(&s.pool)
                .await
                .unwrap();
        if [buckets[0], buckets[1]]
            .iter()
            .all(|b| *b != buckets[2] && *b != buckets[3])
        {
            break ids;
        }
    };
    let run =
        |i: usize, parent: usize| Wire::of(CERTIFIED).renamed(&ids[i]).branch_of(&ids[parent]);
    // X names Y, Y names Z, Z names W, W names X.
    assert_eq!(ingest(&s, &run(3, 0)).await["ok"], json!(true));
    assert_eq!(ingest(&s, &run(1, 2)).await["ok"], json!(true));
    let z_hold = Hold {
        at: HoldAt::BeforeResolve,
        ..hold()
    };
    let z = spawn_ingest(
        &s,
        &run(2, 3),
        Options {
            hold: Some(z_hold.clone()),
            ..Options::default()
        },
    );
    z_hold.locked.notified().await;
    let x_hold = Hold {
        at: HoldAt::BeforeCycles,
        ..hold()
    };
    let x = spawn_ingest(
        &s,
        &run(0, 1),
        Options {
            hold: Some(x_hold.clone()),
            ..Options::default()
        },
    );
    x_hold.locked.notified().await;
    z_hold.release.notify_one();
    let z = z.await.unwrap();
    assert_eq!(z["runs"][0]["status"], json!("refused"), "{z}");
    x_hold.release.notify_one();
    let x = x.await.unwrap();
    let reported = x["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["run"] == json!(ids[2]))
        .unwrap_or_else(|| panic!("Z is reported on the cycle: {x}"));
    assert_eq!(reported["stored"], json!("refused"), "{x}");
}

/// **A generation with no drawn tokens owes no points and leaves the run
/// whole** (Codex pass twelve on PR #23): `diagnostic-certified` with a third
/// generation that drew nothing and carries neither a turn key nor a resident
/// count. Its summary lands; nothing it owed failed to, so the run is
/// `whole` with no reason.
#[tokio::test]
async fn an_empty_generation_leaves_the_run_whole() {
    let Some(s) = store().await else { return };
    let mut w = Wire::of(CERTIFIED).tagged(&tag("empty-generation"));
    let mut empty = w.summary["generations"][1].clone();
    let object = empty.as_object_mut().unwrap();
    object.remove("turn");
    object.remove("resident");
    object.remove("perplexity");
    empty["output_count"] = json!(0);
    w.summary["generations"].as_array_mut().unwrap().push(empty);
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["runs"][0]["status"], json!("whole"), "{answer}");
    let row = landed(&s, &w.run()).await.unwrap();
    assert_eq!((row.status.as_str(), row.reason), ("whole", None));
    assert_eq!(count(&s, "generation", &w.run()).await, 3);
}

/// **A generation with no drawn tokens adds no position to a parent's
/// tape** (Codex pass thirteen on PR #23): a whole parent, `serving-source`
/// with a third generation that drew nothing and carries neither a turn key
/// nor a resident count, and a child branching from it, the hand-made branch
/// fixture, whose points are the parent's. The walk rebuilds the parent's
/// tape past the empty generation, and the parting is known: never parted.
#[tokio::test]
async fn an_empty_generation_in_a_parent_withholds_no_tape() {
    let Some(s) = store().await else { return };
    let t = tag("empty-in-parent");
    let mut parent = Wire::of(SERVING).tagged(&t);
    let mut empty = parent.summary["generations"][1].clone();
    let object = empty.as_object_mut().unwrap();
    object.remove("turn");
    object.remove("resident");
    object.remove("perplexity");
    empty["output_count"] = json!(0);
    parent.summary["generations"]
        .as_array_mut()
        .unwrap()
        .push(empty);
    assert_eq!(
        ingest(&s, &parent).await["runs"][0]["status"],
        json!("whole")
    );
    let child = Wire::of(HAND_MADE_BRANCH).tagged(&t);
    assert_eq!(ingest(&s, &child).await["ok"], json!(true));
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(row.parent_run_id, Some(parent.run()));
    assert!(row.parting_known, "the parent's tape is whole: {row:?}");
    assert_eq!(row.parting_position, None, "never parted");
}

/// **A cycle member the emission did not name is answered with nothing a
/// plan would have said** (Codex pass fourteen on PR #23): A is stored
/// naming an absent B, and an emission carrying only B, naming A, closes
/// the cycle through A's row. A's answer is refused with its reason, its
/// stored status and its parent, says it was not named, and carries no
/// positions, generations or other member of a plan, none having touched
/// it; it was not replayed.
#[tokio::test]
async fn an_unnamed_cycle_member_carries_no_plans_members() {
    let Some(s) = store().await else { return };
    let t = tag("unnamed");
    let a = Wire::of(CERTIFIED)
        .renamed(&format!("a#{t}"))
        .branch_of(&format!("b#{t}"));
    let b = Wire::of(CERTIFIED)
        .renamed(&format!("b#{t}"))
        .branch_of(&format!("a#{t}"));
    assert_eq!(ingest(&s, &a).await["ok"], json!(true));
    let answer = ingest(&s, &b).await;
    let reported = answer["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["run"] == json!(a.run()))
        .unwrap_or_else(|| panic!("A is reported: {answer}"))
        .clone();
    assert_eq!(reported["status"], json!("refused"), "{reported}");
    assert_eq!(reported["named"], json!(false));
    assert_eq!(reported["replayed"], json!(false));
    assert_eq!(reported["stored"], json!("whole"));
    assert_eq!(reported["parent_reference"], json!(b.run()));
    for member in [
        "positions",
        "generations",
        "absent",
        "not_stored",
        "parent_linked",
        "parting_known",
    ] {
        assert!(
            reported.get(member).is_none(),
            "{member} on an unnamed run: {reported}"
        );
    }
    let named = answer["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["run"] == json!(b.run()))
        .unwrap();
    assert!(
        named.get("named").is_none(),
        "a named run's answer is as before"
    );
    assert!(named.get("positions").is_some());
}

/// **A parent's tape is loaded once for all its branches** (Codex pass
/// sixteen on PR #23): two whole parents, P and Q, and five branches listed
/// interleaved, P's and Q's alternating. The order puts each parent's
/// branches together and the resolution holds one parent's tape at a time,
/// so the store is asked for a tape twice, not five times, and every walk
/// still runs.
#[tokio::test]
async fn a_parents_tape_is_loaded_once_for_its_branches() {
    let Some(s) = store().await else { return };
    let t = tag("one-tape");
    let p = Wire::of(SERVING).tagged(&t);
    let q = Wire::of(CERTIFIED).renamed(&format!("q#{t}"));
    assert_eq!(ingest(&s, &p).await["ok"], json!(true));
    assert_eq!(ingest(&s, &q).await["ok"], json!(true));
    let parents = [p.run(), q.run()];
    let mut branches = Wire::of(CERTIFIED)
        .renamed(&format!("b0#{t}"))
        .branch_of(&parents[0]);
    for i in 1..5 {
        branches = branches.then(
            Wire::of(CERTIFIED)
                .renamed(&format!("b{i}#{t}"))
                .branch_of(&parents[i % 2]),
        );
    }
    let loads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let answer = s
        .ingest_with(
            Emission::read(branches.text().as_bytes()),
            &Options {
                parent_loads: Some(loads.clone()),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    for r in answer["runs"].as_array().unwrap() {
        assert_eq!(r["parting_known"], json!(true), "every walk runs: {r}");
    }
    assert_eq!(
        loads.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "one load per parent"
    );
}

/// **A `writing` row is completed only by an emission of its own shape**
/// (Codex pass seventeen on PR #23): the row lands with every generation's
/// summary, so a run the test's step hook stops after generation 0's points
/// holds its whole skeleton, and a replay naming a different generation 1,
/// fewer generations or more is refused with the row left `writing` and
/// its one generation's points.
#[tokio::test]
async fn a_writing_row_is_completed_only_by_its_own_shape() {
    let Some(s) = store().await else { return };
    for (case, replay) in [
        ("different", vec![3, 5]),
        ("fewer", vec![3]),
        ("more", vec![3, 4, 5]),
    ] {
        let run = tag(&format!("shape-{case}"));
        let stopped = ingest_stopping(
            &s,
            &synthetic(&[(run.clone(), None, vec![3, 4])]),
            Step::AfterGeneration(0),
        )
        .await;
        assert_eq!(stopped["ok"], json!(false), "{stopped}");
        assert_eq!(count(&s, "position", &run).await, 3);
        let answer = ingest(&s, &synthetic(&[(run.clone(), None, replay)])).await;
        assert_eq!(
            answer["runs"][0]["status"],
            json!("refused"),
            "{case}: {answer}"
        );
        assert_eq!(answer["runs"][0]["stored"], json!("writing"), "{case}");
        let row = landed(&s, &run).await.unwrap();
        assert_eq!(
            (row.status.as_str(), row.reason),
            ("writing", None),
            "{case}"
        );
        assert_eq!(count(&s, "position", &run).await, 3, "{case}");
    }
}

/// **A completion writes one generation per transaction, as creation does**
/// (Codex pass seventeen on PR #23): a run of three generations stopped
/// after generation 0's points, then completed by an ingest the hook stops
/// after generation 1's, holds generations 0 and 1's points and not 2's;
/// the next ingest completes it whole.
#[tokio::test]
async fn a_completion_lands_one_generation_at_a_time() {
    let Some(s) = store().await else { return };
    let run = tag("completion");
    let w = synthetic(&[(run.clone(), None, vec![3, 4, 5])]);
    ingest_stopping(&s, &w, Step::AfterGeneration(0)).await;
    assert_eq!(count(&s, "position", &run).await, 3);
    let stopped = ingest_stopping(&s, &w, Step::AfterGeneration(1)).await;
    assert_eq!(stopped["ok"], json!(false), "{stopped}");
    assert_eq!(
        count(&s, "position", &run).await,
        7,
        "generation 1's points stand, generation 2's do not"
    );
    assert_eq!(landed(&s, &run).await.unwrap().status, "writing");
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["runs"][0]["status"], json!("whole"), "{answer}");
    assert_eq!(count(&s, "position", &run).await, 12);
}

/// **The lock's session outlives a server's idle timeout** (Codex pass
/// eighteen on PR #23): the store's lock options carry
/// `idle_session_timeout = 1s`, standing in for a server that sets it. An
/// ingest held on its locks for three seconds still holds them, so a second
/// ingest of the run waits and writes nothing, then replays.
#[tokio::test]
async fn the_lock_session_outlives_an_idle_timeout() {
    let Some(s) = store().await else { return };
    let mut idle = s.clone();
    idle.connect = s.connect.clone().options([("idle_session_timeout", "1s")]);
    let w = Wire::of(CERTIFIED).tagged(&tag("idle-timeout"));
    let first_hold = hold();
    let first = spawn_ingest(
        &idle,
        &w,
        Options {
            hold: Some(first_hold.clone()),
            ..Options::default()
        },
    );
    first_hold.locked.notified().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let second = spawn_ingest(&idle, &w, Options::default());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !second.is_finished(),
        "the second ingest still waits for the run's lock"
    );
    assert_eq!(
        landed(&s, &w.run()).await,
        None,
        "nothing is written meanwhile"
    );
    first_hold.release.notify_one();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(first["runs"][0]["status"], json!("whole"), "{first}");
    assert_eq!(second["runs"][0]["replayed"], json!(true), "{second}");
}

/// **An ingest whose locks are lost writes nothing further** (Codex pass
/// eighteen on PR #23): the lock's session is terminated while the ingest is
/// held on its locks, and the ingest answers the lost locks without writing
/// the run.
#[tokio::test]
async fn an_ingest_whose_locks_are_lost_writes_nothing_further() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("locks-lost"));
    let first_hold = hold();
    let first = spawn_ingest(
        &s,
        &w,
        Options {
            hold: Some(first_hold.clone()),
            ..Options::default()
        },
    );
    first_hold.locked.notified().await;
    let terminated: bool = sqlx::query_scalar(
        "SELECT pg_terminate_backend(pid) FROM pg_locks WHERE locktype = 'advisory' \
         AND classid::bigint = $1 AND objid::bigint = (hashtext($2) & 1023) \
         AND objsubid = 2 AND granted",
    )
    .bind(i64::from(super::ingest::LOCK_CLASS))
    .bind(w.run())
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert!(terminated);
    first_hold.release.notify_one();
    let answer = first.await.unwrap();
    assert_eq!(answer["ok"], json!(false), "{answer}");
    assert!(
        answer["error"]
            .as_str()
            .is_some_and(|e| e.starts_with("the run locks were lost")),
        "{answer}"
    );
    assert_eq!(landed(&s, &w.run()).await, None, "nothing written after");
}

/// **A stored chain is read in one statement per walk** (the planner's
/// ruling on PR #23, after Codex pass eighteen): a chain of a thousand runs
/// stored by one ingest, its root naming a run never held, and a branch of
/// its last run landed by another. The scan reads the chain in one
/// statement and the resolution's recheck the absent root in one more; a
/// walk reading one link per statement reads a thousand.
#[tokio::test]
async fn a_stored_chain_is_read_in_one_statement_per_walk() {
    let Some(s) = store().await else { return };
    let t = tag("stored-chain");
    let name = |i: usize| format!("c{i}#{t}");
    let chain: Vec<SyntheticRun> = (0..1000)
        .map(|i| {
            let parent = if i == 0 {
                format!("root#{t}")
            } else {
                name(i - 1)
            };
            (name(i), Some(parent), vec![1])
        })
        .collect();
    let stored = ingest(&s, &synthetic(&chain)).await;
    assert_eq!(stored["ok"], json!(true));
    let reads = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let branch = synthetic(&[(format!("b#{t}"), Some(name(999)), vec![1])]);
    let answer = s
        .ingest_with(
            Emission::read(branch.text().as_bytes()),
            &Options {
                walk_reads: Some(reads.clone()),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["runs"][0]["status"], json!("whole"), "{answer}");
    assert_eq!(answer["runs"][0]["parent_linked"], json!(true));
    assert_eq!(
        reads.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the scan's one statement and the recheck's one"
    );
}

/// Whether `held`'s lock buckets and `other`'s share none, so an ingest of
/// `other` never waits on one of `held` the test is holding.
async fn buckets_apart(s: &Store, held: &[String], other: &[String]) -> bool {
    let buckets = |names: &[String]| {
        sqlx::query_scalar::<_, i32>("SELECT hashtext(r) & 1023 FROM unnest($1::text[]) AS r")
            .bind(names.to_vec())
            .fetch_all(&s.pool)
    };
    let held: std::collections::HashSet<i32> = buckets(held).await.unwrap().into_iter().collect();
    buckets(other)
        .await
        .unwrap()
        .into_iter()
        .all(|b| !held.contains(&b))
}

/// Run identities `names` under a tag whose lock buckets keep the runs at
/// `held` apart from the rest.
async fn apart_names<const N: usize>(
    s: &Store,
    base: &str,
    names: [&str; N],
    held: &[usize],
) -> [String; N] {
    for k in 0.. {
        let t = tag(&format!("{base}-{k}"));
        let runs = names.map(|n| format!("{n}#{t}"));
        let side = |inside: bool| -> Vec<String> {
            runs.iter()
                .enumerate()
                .filter(|(i, _)| held.contains(i) == inside)
                .map(|(_, r)| r.clone())
                .collect()
        };
        let (inside, rest) = (side(true), side(false));
        if buckets_apart(s, &inside, &rest).await {
            return runs;
        }
    }
    unreachable!()
}

/// The outcome the answer gives `run`.
fn outcome<'a>(answer: &'a Value, run: &str) -> &'a Value {
    answer["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["run"] == json!(run))
        .unwrap_or_else(|| panic!("no outcome for {run}: {answer}"))
}

/// **A walk reads again an absent end it reaches through what an earlier
/// walk kept** (Codex pass nineteen on PR #23): X and A both name B, stored
/// naming C, which is absent, and D is stored naming A. X's walk ends at C;
/// held after X's resolution, a disjoint ingest lands C naming D. A's walk
/// reaches C through what X's walk kept, reads it again, and A is refused
/// on A, B, C, D.
#[tokio::test]
async fn a_walk_reads_again_an_absent_end_it_jumped_to() {
    let Some(s) = store().await else { return };
    let [x, a, b, c, d] =
        apart_names(&s, "absent-end", ["x", "a", "b", "c", "d"], &[0, 1, 2]).await;
    assert_eq!(
        ingest(&s, &synthetic(&[(b.clone(), Some(c.clone()), vec![1])])).await["ok"],
        json!(true)
    );
    assert_eq!(
        ingest(&s, &synthetic(&[(d.clone(), Some(a.clone()), vec![1])])).await["ok"],
        json!(true)
    );
    let held = Hold {
        at: HoldAt::AfterFirstResolution,
        ..hold()
    };
    let first = spawn_ingest(
        &s,
        &synthetic(&[
            (x.clone(), Some(b.clone()), vec![1]),
            (a.clone(), Some(b.clone()), vec![1]),
        ]),
        Options {
            hold: Some(held.clone()),
            ..Options::default()
        },
    );
    held.locked.notified().await;
    ingest(&s, &synthetic(&[(c.clone(), Some(d.clone()), vec![1])])).await;
    held.release.notify_one();
    let answer = first.await.unwrap();
    assert_eq!(outcome(&answer, &x)["status"], json!("whole"), "{answer}");
    let least = [&a, &b, &c, &d].into_iter().min().unwrap();
    assert_eq!(
        outcome(&answer, &a)["reason"],
        json!(format!(
            "a reference cycle of 4 runs through {least}, this run naming {b}"
        )),
        "{answer}"
    );
    assert_eq!(landed(&s, &a).await.unwrap().status, "refused");
}

/// The stored chain the two tests below share: Z names W and W names A,
/// whose parent P names Q, absent; and an emission of Y naming Z, then A
/// naming P, Y resolving first.
async fn chain_behind_a_sibling(s: &Store, base: &str) -> [String; 6] {
    // Z and P sorted so, Y's parent before A's.
    let runs = apart_names(s, base, ["y", "k1", "w", "a", "k2", "q"], &[0, 1, 3, 4]).await;
    let [_, z, w, a, p, q] = runs.clone();
    let stored = synthetic(&[
        (z, Some(w.clone()), vec![1]),
        (w, Some(a), vec![1]),
        (p, Some(q), vec![1]),
    ]);
    assert_eq!(ingest(s, &stored).await["ok"], json!(true));
    runs
}

/// **A cycle closed behind a jump is refused with its count** (Codex pass
/// nineteen on PR #23): Y's walk steps Z, W, A and P to Q, absent, and keeps
/// them as reaching Q. Held after Y's resolution, a disjoint ingest lands Q
/// naming W, closing A, P, Q, W. A's walk jumps from P to Q, reads it grown,
/// and begins again on a cleared cache, so A is refused as a cycle of four
/// rather than jumping over itself from W.
#[tokio::test]
async fn a_cycle_closed_behind_a_jump_is_refused_with_its_count() {
    let Some(s) = store().await else { return };
    let [y, z, w, a, p, q] = chain_behind_a_sibling(&s, "behind-jump").await;
    let held = Hold {
        at: HoldAt::AfterFirstResolution,
        ..hold()
    };
    let first = spawn_ingest(
        &s,
        &synthetic(&[
            (y.clone(), Some(z.clone()), vec![1]),
            (a.clone(), Some(p.clone()), vec![1]),
        ]),
        Options {
            hold: Some(held.clone()),
            ..Options::default()
        },
    );
    held.locked.notified().await;
    ingest(&s, &synthetic(&[(q.clone(), Some(w.clone()), vec![1])])).await;
    held.release.notify_one();
    let answer = first.await.unwrap();
    assert_eq!(outcome(&answer, &y)["status"], json!("whole"), "{answer}");
    let least = [&a, &p, &q, &w].into_iter().min().unwrap();
    assert_eq!(
        outcome(&answer, &a)["reason"],
        json!(format!(
            "a reference cycle of 4 runs through {least}, this run naming {p}"
        )),
        "{answer}"
    );
}

/// **A cycle a sibling walked round refuses its member** (Codex pass
/// nineteen on PR #23): the same chain, Q landing naming W before the
/// resolution. Y's walk steps round A, P, Q, W and keeps it as a cycle,
/// with Z leading into it; A's walk meets the kept cycle at P, finds itself
/// on it, and is refused with its count.
#[tokio::test]
async fn a_cycle_a_sibling_walked_round_refuses_its_member() {
    let Some(s) = store().await else { return };
    let [y, z, w, a, p, q] = chain_behind_a_sibling(&s, "walked-round").await;
    let held = Hold {
        at: HoldAt::BeforeResolve,
        ..hold()
    };
    let first = spawn_ingest(
        &s,
        &synthetic(&[
            (y.clone(), Some(z.clone()), vec![1]),
            (a.clone(), Some(p.clone()), vec![1]),
        ]),
        Options {
            hold: Some(held.clone()),
            ..Options::default()
        },
    );
    held.locked.notified().await;
    ingest(&s, &synthetic(&[(q.clone(), Some(w.clone()), vec![1])])).await;
    held.release.notify_one();
    let answer = first.await.unwrap();
    assert_eq!(outcome(&answer, &y)["status"], json!("whole"), "{answer}");
    let least = [&a, &p, &q, &w].into_iter().min().unwrap();
    assert_eq!(
        outcome(&answer, &a)["reason"],
        json!(format!(
            "a reference cycle of 4 runs through {least}, this run naming {p}"
        )),
        "{answer}"
    );
}

/// **Every column of `run` is a declared member or the ingest's
/// bookkeeping** (Codex pass twenty on PR #23), read from the schema, so a
/// column added to the table and not to `RunMembers` fails here rather than
/// passing every replay unread.
#[tokio::test]
async fn every_run_column_is_declared_or_bookkeeping() {
    let Some(s) = store().await else { return };
    let bookkeeping = [
        "run_id",
        "ingest_status",
        "ingest_reason",
        "parent_run_id",
        "parting_position",
        "parting_known",
        "ingested_at",
    ];
    let mut columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name::text FROM information_schema.columns \
         WHERE table_schema = current_schema() AND table_name = 'run'",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    columns.retain(|c| !bookkeeping.contains(&c.as_str()));
    columns.sort();
    let mut declared: Vec<String> = super::rows::RunMembers::COLUMNS
        .iter()
        .map(|c| c.name.to_owned())
        .collect();
    declared.sort();
    assert_eq!(columns, declared);
}

/// **A stored member this seam never fills differs from every emission**
/// (Codex pass twenty on PR #23): a run landed whole and then given a
/// signature, as another writer would, is refused as a differing replay
/// naming the signature, and stands unchanged.
#[tokio::test]
async fn a_member_another_writer_filled_refuses_the_replay() {
    let Some(s) = store().await else { return };
    let w = Wire::of(SERVING).tagged(&tag("filled-member"));
    assert_eq!(ingest(&s, &w).await["ok"], json!(true));
    sqlx::query("UPDATE run SET signature = '{\"shingles\": [1]}'::jsonb WHERE run_id = $1")
        .bind(w.run())
        .execute(&s.pool)
        .await
        .unwrap();
    let answer = ingest(&s, &w).await;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    assert!(
        answer["runs"][0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("signature")),
        "{answer}"
    );
    assert_eq!(answer["runs"][0]["stored"], json!("whole"));
}

/// Terminate the session holding `run`'s lock, as a server or a dropped
/// socket would.
async fn terminate_lock_session(s: &Store, run: &str) {
    let terminated: bool = sqlx::query_scalar(
        "SELECT pg_terminate_backend(pid) FROM pg_locks WHERE locktype = 'advisory' \
         AND classid::bigint = $1 AND objid::bigint = (hashtext($2) & 1023) \
         AND objsubid = 2 AND granted",
    )
    .bind(i64::from(super::ingest::LOCK_CLASS))
    .bind(run)
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert!(terminated);
}

/// **A write whose session lock was lost inside it never interleaves with
/// another's** (Codex pass twenty on PR #23): the first ingest of a run is
/// held inside a write transaction at `at`, written and locked, and its
/// lock session is terminated there. A second ingest of the run takes the
/// session lock, plans the same write and waits on the transaction's own
/// lock; released, the first commits and stops at its next check, and the
/// second finds its target written, answering the run refused by name with
/// no store failure. A third completes it.
async fn a_lost_lock_inside_a_write(base: &str, at: HoldAt) {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag(base));
    let held = Hold { at, ..hold() };
    let first = spawn_ingest(
        &s,
        &w,
        Options {
            hold: Some(held.clone()),
            ..Options::default()
        },
    );
    held.locked.notified().await;
    terminate_lock_session(&s, &w.run()).await;
    let second = spawn_ingest(&s, &w, Options::default());
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(
        !second.is_finished(),
        "the second waits on the write's lock"
    );
    held.release.notify_one();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert!(
        first["error"]
            .as_str()
            .is_some_and(|e| e.starts_with("the run locks were lost")),
        "{first}"
    );
    assert_eq!(second.get("error"), None, "no store failure: {second}");
    assert_eq!(second["runs"][0]["status"], json!("refused"), "{second}");
    assert_eq!(
        second["runs"][0]["reason"],
        json!(
            "another ingest wrote this run while this one's locks were lost; a replay compares it"
        )
    );
    assert_eq!(second["runs"][0]["stored"], json!("writing"));
    let third = ingest(&s, &w).await;
    assert_eq!(third["runs"][0]["status"], json!("whole"), "{third}");
}

#[tokio::test]
async fn a_lost_lock_inside_the_creation_is_answered() {
    a_lost_lock_inside_a_write("lost-in-create", HoldAt::BeforeCreateCommit).await;
}

#[tokio::test]
async fn a_lost_lock_inside_a_generations_points_is_answered() {
    a_lost_lock_inside_a_write("lost-in-points", HoldAt::BeforeFirstPointsCommit).await;
}

/// **A close that finds its row moved is answered, not claimed** (Codex
/// pass twenty on PR #23): the first ingest is held after its lock check,
/// before the run's close or resolution, and its lock session is
/// terminated there. A second ingest of the run completes and closes it;
/// released, the first's close finds no `writing` row and answers the run
/// refused by name with the stored status.
async fn a_lost_lock_before_the_close(w: Wire) {
    let Some(s) = store().await else { return };
    let held = Hold {
        at: HoldAt::BeforeClosing,
        ..hold()
    };
    let first = spawn_ingest(
        &s,
        &w,
        Options {
            hold: Some(held.clone()),
            ..Options::default()
        },
    );
    held.locked.notified().await;
    terminate_lock_session(&s, &w.run()).await;
    let second = ingest(&s, &w).await;
    assert_eq!(second["runs"][0]["status"], json!("whole"), "{second}");
    held.release.notify_one();
    let first = first.await.unwrap();
    assert_eq!(first["runs"][0]["status"], json!("refused"), "{first}");
    assert_eq!(
        first["runs"][0]["reason"],
        json!(
            "another ingest wrote this run while this one's locks were lost; a replay compares it"
        )
    );
    assert_eq!(first["runs"][0]["stored"], json!("whole"));
    assert_eq!(
        (
            first["runs"][0]["positions"].clone(),
            first["runs"][0]["generations"].clone()
        ),
        (
            json!(count(&s, "position", &w.run()).await),
            json!(count(&s, "generation", &w.run()).await)
        ),
        "the counts are the store's: {first}"
    );
    let row = landed(&s, &w.run()).await.unwrap();
    assert_eq!(
        (
            first["runs"][0]["parent_linked"].clone(),
            first["runs"][0]["parting_known"].clone(),
            first["runs"][0].get("parting_position").cloned(),
        ),
        (
            json!(row.parent_run_id.is_some()),
            json!(row.parting_known),
            row.parting_position.map(|p| json!(p)),
        ),
        "the link and the parting are the store's: {first}"
    );
}

#[tokio::test]
async fn a_close_that_finds_its_row_moved_is_answered() {
    a_lost_lock_before_the_close(Wire::of(CERTIFIED).tagged(&tag("moved-close"))).await;
}

/// The branch's parent is held and whole, so the competitor's resolution
/// links it and walks it, and the answer must carry that link and parting.
#[tokio::test]
async fn a_resolution_that_finds_its_row_moved_is_answered() {
    let Some(s) = store().await else { return };
    let t = tag("moved-resolution");
    let parent = Wire::of(SERVING).tagged(&t);
    assert_eq!(
        ingest(&s, &parent).await["runs"][0]["status"],
        json!("whole")
    );
    let child = Wire::of(HAND_MADE_BRANCH).tagged(&t);
    a_lost_lock_before_the_close(child.clone()).await;
    let row = landed(&s, &child.run()).await.unwrap();
    assert_eq!(
        row.parent_run_id,
        Some(parent.run()),
        "the competitor linked it"
    );
    assert!(row.parting_known, "and walked it: {row:?}");
}
