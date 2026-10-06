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
use super::ingest::{Options, Race, Step};
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
    assert_eq!(count(&s, "position", &run).await, 455, "nothing doubled");

    let mut changed = w.clone();
    let held = changed.points[3]["token"].as_i64().unwrap();
    changed.points[3]["token"] = json!(held + 1);
    let refused = ingest(&s, &changed).await;
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(refused["runs"][0]["status"], json!("refused"));
    assert_eq!(refused["runs"][0]["stored"], json!("whole"));
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

/// **An ingest that loses the race to create a run replays it** (Codex pass
/// two on PR #23): a second ingest of the same emission lands between this
/// one's read of the run and its insert of the row, by the test-only race
/// option. This one meets the row on its insert, compares and answers the
/// run's stored outcome, rather than refusing for having lost a race.
#[tokio::test]
async fn an_ingest_that_loses_the_race_to_create_a_run_replays_it() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("race"));
    let answer = s
        .ingest_with(
            Emission::read(w.text().as_bytes()),
            &Options {
                race: Some((
                    Race::BeforeCreate,
                    Emission::read(w.text().as_bytes()).unwrap(),
                )),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["ok"], json!(true), "{answer}");
    assert_eq!(answer["runs"][0]["status"], json!("whole"));
    assert_eq!(answer["runs"][0]["replayed"], json!(true));
    assert_eq!(landed(&s, &w.run()).await.unwrap().status, "whole");
    assert_eq!(count(&s, "position", &w.run()).await, 455);
}

/// **A run another ingest closed is compared whole before this one
/// answers** (Codex pass three on PR #23): two ingests of one unseen run
/// whose payloads differ in the second generation. This one creates the row
/// and fills the first generation; the test-only race lands the other, which
/// replays the `writing` row, fills the second generation with its own
/// payload and closes it. This one then meets the closed row and is refused
/// naming the first difference, the stored row untouched.
#[tokio::test]
async fn a_run_closed_by_another_ingest_is_compared_before_its_answer() {
    let Some(s) = store().await else { return };
    let w = Wire::of(CERTIFIED).tagged(&tag("closed-under"));
    let mut other = w.clone();
    let held = other.points[100]["token"].as_i64().unwrap();
    other.points[100]["token"] = json!(held + 1);
    let answer = s
        .ingest_with(
            Emission::read(w.text().as_bytes()),
            &Options {
                race: Some((
                    Race::AfterFirstFill,
                    Emission::read(other.text().as_bytes()).unwrap(),
                )),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    assert_eq!(answer["runs"][0]["stored"], json!("whole"));
    let why = answer["runs"][0]["reason"].as_str().unwrap();
    assert!(why.contains("turn t-2 position 204"), "{why}");
    assert_eq!(landed(&s, &w.run()).await.unwrap().status, "whole");
}

/// **A cycle two concurrent ingests close is refused on both sides**
/// (Codex pass three on PR #23): this ingest creates A naming B and scans
/// for cycles while B is absent; the test-only race lands an ingest of B
/// naming A, which refuses B. This one's resolution of A rechecks the chain
/// under its locks, meets B naming A, and refuses A rather than closing it
/// on a cycle.
#[tokio::test]
async fn a_cycle_two_concurrent_ingests_close_is_refused_on_both_sides() {
    let Some(s) = store().await else { return };
    let t = tag("cycle-concurrent");
    let a = Wire::of(CERTIFIED)
        .renamed(&format!("a#{t}"))
        .branch_of(&format!("b#{t}"));
    let b = Wire::of(CERTIFIED)
        .renamed(&format!("b#{t}"))
        .branch_of(&format!("a#{t}"));
    let answer = s
        .ingest_with(
            Emission::read(a.text().as_bytes()),
            &Options {
                race: Some((
                    Race::BeforeResolve,
                    Emission::read(b.text().as_bytes()).unwrap(),
                )),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    for run in [a.run(), b.run()] {
        let row = landed(&s, &run).await.unwrap();
        assert_eq!(row.status, "refused", "{run}: {row:?}");
        assert!(row.reason.unwrap().contains("reference cycle"));
    }
}

/// The certified emission cut to its first generation and its points: an
/// emission whose plan is a strict prefix of the whole one.
fn first_generation_of(w: &Wire) -> Wire {
    let mut short = w.clone();
    short.summary["generations"]
        .as_array_mut()
        .unwrap()
        .truncate(1);
    short.points.truncate(12);
    short
}

/// **An ingest closes a run only over exactly what it planned** (Codex
/// pass four on PR #23): this ingest carries the run's first generation
/// only; the test-only race lands an ingest of the whole run, which writes
/// the second generation and stops before its own close, between this
/// one's fill and its close. This one is refused naming the difference and
/// leaves the row `writing`, and the whole ingest, replayed, closes it.
#[tokio::test]
async fn a_shorter_ingest_never_closes_a_run_holding_more() {
    let Some(s) = store().await else { return };
    let whole = Wire::of(CERTIFIED).tagged(&tag("prefix"));
    let short = first_generation_of(&whole);
    let answer = s
        .ingest_with(
            Emission::read(short.text().as_bytes()),
            &Options {
                race: Some((
                    Race::BeforeClose,
                    Emission::read(whole.text().as_bytes()).unwrap(),
                )),
                race_stop: Some(Step::AfterFill),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    assert_eq!(answer["runs"][0]["stored"], json!("writing"));
    assert!(
        answer["runs"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("generation 1 differs"),
        "{answer}"
    );
    assert_eq!(landed(&s, &whole.run()).await.unwrap().status, "writing");
    assert_eq!(ingest(&s, &whole).await["ok"], json!(true));
    assert_eq!(landed(&s, &whole.run()).await.unwrap().status, "whole");
}

/// **A branch closes only over exactly what it planned**, the same race
/// against a branch's close inside its resolution: both emissions name a
/// parent the store does not hold, so each stays open until it resolves.
#[tokio::test]
async fn a_shorter_branch_never_closes_a_run_holding_more() {
    let Some(s) = store().await else { return };
    let t = tag("prefix-branch");
    let whole = Wire::of(CERTIFIED)
        .renamed(&format!("child#{t}"))
        .branch_of(&format!("elsewhere#{t}"));
    let short = first_generation_of(&whole);
    let answer = s
        .ingest_with(
            Emission::read(short.text().as_bytes()),
            &Options {
                race: Some((
                    Race::BeforeResolve,
                    Emission::read(whole.text().as_bytes()).unwrap(),
                )),
                race_stop: Some(Step::AfterPoints),
                ..Options::default()
            },
        )
        .await
        .value;
    assert_eq!(answer["runs"][0]["status"], json!("refused"), "{answer}");
    assert_eq!(landed(&s, &whole.run()).await.unwrap().status, "writing");
    assert_eq!(ingest(&s, &whole).await["ok"], json!(true));
    assert_eq!(landed(&s, &whole.run()).await.unwrap().status, "whole");
}
