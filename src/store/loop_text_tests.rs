//! An agent's loop, versioned (Spec 6.3, migration `0035`,
//! `src/store/loop_text.rs`): a save at the head, a save against a stale
//! or an unknown head, the audit holding the digest and the byte count and
//! never the text, the version never changed or removed, and the authority
//! re-checked inside the write. Each test runs on a store of its own with
//! the verbs' rig: an admin, the agent `karl`, and `bea` holding `operator`
//! on it. No name is a real one.

use crate::host;
use crate::store::loop_text::{LoopVersion, Saved, WIRE_FILE_MOST, action, digest};
use crate::store::{AgentId, Store};
use crate::verbs_tests::operated;

/// A text no other test holds, so a search of the audit for it finds it
/// only where a write put it there.
const MARK: &str = "def on_turn(board, text):\n    board.turn([{\"role\": \"user\", \"content\": [{\"type\": \"text\", \"text\": \"marked-9f2c\"}]}])\n";

/// The open session of `person`, by its row, as the page's own session
/// would name it.
async fn session_of(s: &Store, person: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT session_id FROM session WHERE person_id = $1 AND closed_at IS NULL \
         ORDER BY session_id DESC LIMIT 1",
    )
    .bind(person)
    .fetch_one(&s.pool)
    .await
    .expect("the person's session")
}

/// An audit record as the test reads it: target kind, target, action,
/// outcome and the first record it answers.
type Record = (
    String,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
);

/// One save as the page makes it: the identity minted, the first record
/// naming it with the digest and the byte count, then the write.
async fn save(
    s: &Store,
    person: &str,
    agent: &AgentId,
    read: i32,
    text: &str,
) -> (String, String, Saved) {
    let loop_id = s.mint_key("lp").await.unwrap();
    let first = s.loop_save_first(person, &loop_id, text).await.unwrap();
    let session = session_of(s, person).await;
    let saved = s
        .save_loop(session, person, agent, read, &loop_id, text, &first)
        .await
        .unwrap();
    if !matches!(saved, Saved::Done { .. }) {
        let _ = s.audit_outcome(&first, false).await;
    }
    (loop_id, first, saved)
}

/// **A save lands at the head's number plus one, and the head reads it
/// back** with its digest, its byte count and its author's name.
#[tokio::test]
async fn a_save_lands_at_the_head_and_reads_back() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    assert!(s.loop_head(&karl).await.unwrap().is_none());
    let (one, _, saved) = save(&s, &o.bea, &karl, 0, MARK).await;
    assert_eq!(saved, Saved::Done { version: 1 });
    let second = format!("{MARK}# two\n");
    let (two, _, saved) = save(&s, &o.bea, &karl, 1, &second).await;
    assert_eq!(saved, Saved::Done { version: 2 });
    let LoopVersion {
        loop_id,
        version,
        text,
        bytes,
        sha256,
        saved_by,
        ..
    } = s.loop_head(&karl).await.unwrap().expect("the head");
    assert_eq!((loop_id.as_str(), version), (two.as_str(), 2));
    assert_eq!(text, second);
    assert_eq!(bytes as usize, second.len());
    assert_eq!(sha256, digest(&second));
    assert_eq!(saved_by, "bea");
    assert_eq!(
        s.loop_version_of(&karl, &digest(MARK)).await.unwrap(),
        Some(1),
        "{one}"
    );
    assert_eq!(
        s.loop_version_of(&karl, &"0".repeat(64)).await.unwrap(),
        None
    );
}

/// **A save against a stale or an unknown head refuses**: naming the
/// version the next save replaced, or one past the head, saves nothing and
/// leaves the head as it stood.
///
/// Perturbation: drop the head check from the insert, and a save naming a
/// version past the head lands, leaving a gap (measured 2026-11-26).
#[tokio::test]
async fn a_save_against_a_stale_head_refuses() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    assert_eq!(
        save(&s, &o.bea, &karl, 0, MARK).await.2,
        Saved::Done { version: 1 }
    );
    assert_eq!(
        save(&s, &o.bea, &karl, 0, "# stale\n").await.2,
        Saved::Stale,
        "the head moved past the version read"
    );
    assert_eq!(
        save(&s, &o.bea, &karl, 5, "# ahead\n").await.2,
        Saved::Stale,
        "a version past the head"
    );
    let head = s.loop_head(&karl).await.unwrap().unwrap();
    assert_eq!((head.version, head.text.as_str()), (1, MARK));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_loop")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

/// **A loop's text enters no audit record**: the save's first record and
/// its outcome name the version and carry the digest and the byte count in
/// their action, and no record holds the text or any line of it.
///
/// Perturbation: write the text into the action, and the search finds it
/// (measured 2026-11-26).
#[tokio::test]
async fn a_loops_text_enters_no_audit_record() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let (loop_id, first, saved) = save(&s, &o.bea, &karl, 0, MARK).await;
    assert_eq!(saved, Saved::Done { version: 1 });
    let records: Vec<Record> = sqlx::query_as(
        "SELECT target_kind, target_id, action, outcome, answers FROM audit \
             WHERE target_kind = 'loop' ORDER BY at, audit_id",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    let said = action(&digest(MARK), MARK.len());
    assert_eq!(
        records,
        vec![
            (
                "loop".to_owned(),
                Some(loop_id.clone()),
                said.clone(),
                None,
                None
            ),
            (
                "loop".to_owned(),
                Some(loop_id),
                said.clone(),
                Some("ok".to_owned()),
                Some(first)
            ),
        ]
    );
    assert_eq!(
        said,
        format!("loop save sha256 {} bytes {}", digest(MARK), MARK.len())
    );
    let holding: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit WHERE position('marked-9f2c' IN \
         coalesce(action, '') || coalesce(refusal, '') || coalesce(target_id, '')) > 0",
    )
    .fetch_one(&s.pool)
    .await
    .unwrap();
    assert_eq!(holding, 0, "no record holds the text");
}

/// **A version is never changed or removed** (migration `0035`): an
/// update, a delete and a truncate are refused, each in a transaction
/// rolled back whatever happens; and **the schema holds the digest to the
/// text** and the bound to 256 KiB.
///
/// Perturbation: drop the row trigger, and the update of a version's text
/// lands (measured 2026-11-26).
#[tokio::test]
async fn a_loop_version_is_never_changed_or_removed() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let (loop_id, _, _) = save(&s, &o.bea, &karl, 0, MARK).await;
    for statement in [
        "UPDATE agent_loop SET text = 'x', bytes = 1, sha256 = encode(sha256('x'::bytea), 'hex') \
         WHERE loop_id = $1",
        "DELETE FROM agent_loop WHERE loop_id = $1",
        "TRUNCATE agent_loop",
    ] {
        let mut tx = s.pool.begin().await.unwrap();
        let mut query = sqlx::query(statement);
        if statement.contains("$1") {
            query = query.bind(loop_id.clone());
        }
        let result = query.execute(&mut *tx).await;
        tx.rollback().await.unwrap();
        let e = result.expect_err(statement).to_string();
        assert!(e.contains("append-only"), "{statement}: {e}");
    }
    let refused = |text: String, sha256: String| {
        let (s, karl, bea) = (s.clone(), karl.clone(), o.bea.clone());
        async move {
            let id = s.mint_key("lp").await.unwrap();
            sqlx::query(
                "INSERT INTO agent_loop (loop_id, agent_id, version, text, bytes, sha256, saved_by) \
                 VALUES ($1, $2, 9, $3, octet_length($3), $4, $5)",
            )
            .bind(id)
            .bind(karl.as_str())
            .bind(text)
            .bind(sha256)
            .bind(bea)
            .execute(&s.pool)
            .await
            .is_err()
        }
    };
    assert!(
        refused("# a\n".into(), digest("# b\n")).await,
        "a digest not the text's"
    );
    let past = "#".repeat(WIRE_FILE_MOST + 1);
    assert!(
        refused(past.clone(), digest(&past)).await,
        "one byte past the bound"
    );
}

/// **The authority is re-checked inside the write**: a save by a person
/// whose `operator` grant was removed after the page's read lands nothing.
#[tokio::test]
async fn a_save_after_the_grant_ends_lands_nothing() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let removed = host::grant_remove(&s, &o.bea, "operator", Some(&o.karl), None).await;
    assert!(removed.ok, "{}", removed.value);
    assert_eq!(
        save(&s, &o.bea, &karl, 0, MARK).await.2,
        Saved::AuthorityGone
    );
    assert!(s.loop_head(&karl).await.unwrap().is_none());
}
