//! The connectors' entries (Spec 2.14; act 12, PR 2; `src/link/entries.rs`,
//! `src/link/admin_log.rs`, the listener's entry door): what the server
//! takes from a connector's entries and what it raises, against the real
//! listener and fake connectors dialing it with minted configs; and the
//! connector's own half, its backlog and its reading of `admin.log`, on
//! files the tests make. No name, path or uid is a real one.

use std::io::Write;
use std::time::Duration;

use serde_json::json;

use crate::agents_tests::{Connector, Rig};
use crate::alerts_tests::{alerts, wait_alerts, wait_log};
use crate::link::admin_log;
use crate::link::entries::{BACKLOG_ENTRIES, Entries};
use crate::link::frames::{ENTRY_BOUND, FromClient, Position, Refusal, ToClient};
use crate::store::AgentId;
use crate::store::alerts::Alert;
use crate::traceview::TraceEvent;
use crate::verbs_tests::{Operated, admitted_admin, dial_gate, operated};

/// A run as a connector draws one.
const RUN: &str = "0123456789abcdef";
/// The fake box's caller uid for admin-con, and another party's.
const MINE: u64 = 4242;
const THEIRS: u64 = 4343;

/// An `admin.log` line of an invocation, as admin writes it.
fn line(verb: &str, uid: u64, outcome: serde_json::Value) -> serde_json::Value {
    json!({
        "wall_ms": 1,
        "command": format!("weaver-admin {verb} karl"),
        "uid": uid,
        "verb": verb,
        "agent": "karl",
        "outcome": outcome.to_string(),
    })
}

/// An entry of `admin.log` as admin-con sends it.
fn box_entry(line: serde_json::Value, own: bool) -> serde_json::Value {
    json!({ "line": line, "own": own, "read_at": 1 })
}

async fn send_entry(c: &mut Connector, seq: u64, source: &str, entry: serde_json::Value) {
    c.send(FromClient::Entry {
        run: RUN.into(),
        seq,
        source: source.into(),
        entry,
    })
    .await;
}

/// The server's acknowledgement of an entry, other frames passed over.
async fn entry_ack(c: &mut Connector) -> (String, u64) {
    loop {
        match c.recv().await {
            Some(ToClient::EntryAck { run, seq }) => return (run, seq),
            Some(_) => {}
            None => panic!("the connection ended before an entry's acknowledgement"),
        }
    }
}

/// Wait for a refusal, other frames passed over.
async fn refused(c: &mut Connector) -> Option<Refusal> {
    loop {
        match c.recv().await {
            Some(ToClient::Refusal { reason }) => return Some(reason),
            Some(_) => {}
            None => return None,
        }
    }
}

/// The operations log's entry lines.
fn entry_lines(rig: &Rig) -> Vec<serde_json::Value> {
    rig.operations_log()
        .into_iter()
        .filter(|l| l.get("entry").is_some())
        .collect()
}

/// Wait for the operations log to hold `n` entry lines.
async fn wait_entries(rig: &Rig, n: usize) -> Vec<serde_json::Value> {
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let lines = entry_lines(rig);
        if lines.len() >= n {
            return lines;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the log never held {n} entries: {lines:#?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn open_of(found: &[Alert], kind: &str) -> Option<Alert> {
    found
        .iter()
        .find(|a| a.kind == kind && a.acknowledged_at.is_none())
        .cloned()
}

/// **An own line is the log's alone; a line of another party is an alert**:
/// a refused `load` admin-con asked itself lands in the operations log and
/// raises nothing, its alert having been raised at the door; the same
/// refusal by another party on the box raises a `failed` alert from the
/// box, saying admin's refusal and never the line's uid or command line,
/// which the operations log keeps.
///
/// Perturbation: drop the own check at the server, and the act asked
/// through WeaverWeb raises its alert again from its line; put the whole
/// line back in the alert's detail, and the uid and command are shown.
#[tokio::test]
async fn an_own_line_is_logged_and_another_partys_raises_an_alert() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let refusal = json!({"kind": "config_invalid", "field": null});
    send_entry(
        &mut o.admin_con,
        1,
        admin_log::SOURCE,
        box_entry(line("load", MINE, refusal.clone()), true),
    )
    .await;
    assert_eq!(entry_ack(&mut o.admin_con).await, (RUN.to_owned(), 1));
    let logged = wait_entries(&o.rig, 1).await;
    assert_eq!(logged[0]["source"], "admin_log");
    assert_eq!(logged[0]["own"], true);
    assert_eq!(logged[0]["entry"]["line"]["verb"], "load");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        alerts(&o.rig, &o.karl).await.is_empty(),
        "an own line raises nothing"
    );

    send_entry(
        &mut o.admin_con,
        2,
        admin_log::SOURCE,
        box_entry(line("load", THEIRS, refusal), false),
    )
    .await;
    entry_ack(&mut o.admin_con).await;
    let found = wait_alerts(&o.rig, &o.karl, "the box's refusal", |a| !a.is_empty()).await;
    let alert = open_of(&found, "failed").expect("a failed alert");
    assert_eq!(
        (alert.subject.as_str(), alert.source.as_str()),
        ("load", "admin")
    );
    assert!(
        alert.summary.contains("not through WeaverWeb"),
        "{}",
        alert.summary
    );
    let logged = wait_entries(&o.rig, 2).await;
    assert_eq!(logged[1]["own"], false);
    assert_eq!(logged[1]["alert"], alert.alert_id.as_str());
    // **The alert says what happened, and the box's facts stay in the
    // log**: admin's refusal and the verb, never the caller's uid or the
    // command line, which the operations log's line keeps whole.
    let shown = format!("{} {}", alert.summary, alert.detail);
    assert!(shown.contains("config_invalid"), "{shown}");
    assert!(
        !shown.contains(&THEIRS.to_string()),
        "the uid is shown: {shown}"
    );
    assert!(
        !shown.contains("weaver-admin load"),
        "the command is shown: {shown}"
    );
    let line = logged[1]["entry"]["line"].to_string();
    assert!(
        line.contains(&THEIRS.to_string()) && line.contains("weaver-admin load"),
        "{line}"
    );
}

/// **An S9 entered on the box raises the save-point prompt**, closing PR 1's
/// residue: another party's `unload` refusing `save_point_not_taken` on a
/// leg at rest raises the prompt with its two options; `published` and
/// `boundary_unverified` raise `data_at_risk`; a relay's refusal of another
/// party raises an alert; an answer raises nothing.
#[tokio::test]
async fn the_boxs_refusals_are_classified_by_the_doors_rules() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let lines = [
        line(
            "unload",
            THEIRS,
            json!({"kind": "save_point_not_taken", "missed": "finished"}),
        ),
        line(
            "save-point",
            THEIRS,
            json!({"kind": "save_point_not_taken", "missed": "published"}),
        ),
        line("load", THEIRS, json!({"kind": "boundary_unverified"})),
        json!({"wall_ms": 1, "actor": "relay", "agent": "karl", "event": "refused",
            "peer_uid": THEIRS, "detail": "not the reader"}),
        line("show", THEIRS, json!({"kind": "state", "state": "idle"})),
    ];
    for (n, l) in lines.into_iter().enumerate() {
        send_entry(
            &mut o.admin_con,
            n as u64 + 1,
            admin_log::SOURCE,
            box_entry(l, false),
        )
        .await;
    }
    wait_entries(&o.rig, 5).await;
    let found = wait_alerts(&o.rig, &o.karl, "four alerts", |a| a.len() == 4).await;
    let prompt = open_of(&found, "save_point_missed").expect("the prompt");
    assert_eq!(prompt.options, ["save_point_then_unload", "force_unload"]);
    assert_eq!(prompt.source, "admin");
    let at_risk: Vec<_> = found
        .iter()
        .filter(|a| a.kind == "data_at_risk")
        .map(|a| a.subject.as_str())
        .collect();
    assert_eq!(at_risk.len(), 2, "{found:#?}");
    assert!(at_risk.contains(&"save-point") && at_risk.contains(&"load"));
    let door = open_of(&found, "failed").expect("the relay's refusal");
    assert_eq!(door.subject, "trace door");
    assert!(
        door.summary.contains("a peer other than admin-con"),
        "{}",
        door.summary
    );
    let shown = format!("{} {}", door.summary, door.detail);
    assert!(
        !shown.contains(&THEIRS.to_string()),
        "the peer's uid is shown: {shown}"
    );
}

/// **A connector's own failure is a `failed` alert from its source**, its
/// subject the failure's kind.
#[tokio::test]
async fn a_connectors_failure_raises_a_failed_alert_from_its_source() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    send_entry(
        &mut o.admin_con,
        1,
        "admin-con",
        json!({"kind": "trace_relay_refused", "message": "the relay said no"}),
    )
    .await;
    let found = wait_alerts(&o.rig, &o.karl, "the failure", |a| !a.is_empty()).await;
    let alert = open_of(&found, "failed").unwrap();
    assert_eq!(
        (alert.subject.as_str(), alert.source.as_str()),
        ("trace_relay_refused", "admin-con")
    );
    assert!(alert.summary.contains("the relay said no"));
}

/// **An entry is taken once, though it is sent again**: sent, the link cut
/// before its acknowledgement is read, and sent again on a new connection,
/// it is one line of the log; each sending is acknowledged.
///
/// Perturbation: drop the dedupe, and the resent entry is logged twice.
#[tokio::test]
async fn a_resent_entry_is_logged_once() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let entry = json!({"kind": "link_outage", "message": "down for 3 s"});
    send_entry(&mut o.admin_con, 1, "admin-con", entry.clone()).await;
    wait_entries(&o.rig, 1).await;
    let Operated {
        rig,
        admin_config,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut again = admitted_admin(&admin_config, &["show"]).await;
    send_entry(&mut again, 1, "admin-con", entry).await;
    assert_eq!(entry_ack(&mut again).await, (RUN.to_owned(), 1));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(entry_lines(&rig).len(), 1, "{:#?}", entry_lines(&rig));
}

/// **An entry the store did not take closes the connection, so nothing
/// after it moves the run's mark past it**: entry 1's take fails, the
/// connection is refused `store_unavailable` before entry 2 is taken, and
/// both, resent in order on a new connection, are each one line of the log.
///
/// Perturbation: log the failed take and read on, and entry 2 is taken past
/// entry 1, whose resend is then taken for a duplicate and never logged.
#[tokio::test]
async fn an_entry_not_taken_closes_so_its_resend_is_taken() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    o.rig.listener.fail_next_entry();
    let first = json!({"kind": "probe", "message": "one"});
    let second = json!({"kind": "probe", "message": "two"});
    send_entry(&mut o.admin_con, 1, "admin-con", first.clone()).await;
    send_entry(&mut o.admin_con, 2, "admin-con", second.clone()).await;
    assert_eq!(
        refused(&mut o.admin_con).await,
        Some(Refusal::StoreUnavailable)
    );
    let Operated {
        rig,
        admin_config,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut again = admitted_admin(&admin_config, &["show"]).await;
    send_entry(&mut again, 1, "admin-con", first).await;
    send_entry(&mut again, 2, "admin-con", second).await;
    entry_ack(&mut again).await;
    entry_ack(&mut again).await;
    let logged = wait_entries(&rig, 2).await;
    let messages: Vec<_> = logged
        .iter()
        .map(|l| l["entry"]["message"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(messages, ["one", "two"]);
}

/// **The door checks an entry once**: an unknown source, an entry past the
/// bound, and gate-con claiming the admin plane's `admin_log` are each
/// refused as malformed, the connection closed, nothing logged.
///
/// Perturbation: drop the door's source allow-list, and gate-con's entry
/// claiming `admin_log` is taken and classified.
#[tokio::test]
async fn an_entry_outside_its_shape_is_refused_at_the_door() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let mut gate = dial_gate(&o).await;
    send_entry(
        &mut gate,
        1,
        admin_log::SOURCE,
        box_entry(
            line(
                "unload",
                THEIRS,
                json!({"kind": "save_point_not_taken", "missed": "finished"}),
            ),
            false,
        ),
    )
    .await;
    assert_eq!(refused(&mut gate).await, Some(Refusal::Malformed));
    let Operated {
        rig,
        karl,
        admin_config,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    for (source, entry) in [
        ("the box", json!({"kind": "x", "message": "y"})),
        (
            "admin-con",
            json!({"kind": "x", "message": "p".repeat(ENTRY_BOUND)}),
        ),
    ] {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let mut c = admitted_admin(&admin_config, &["show"]).await;
        send_entry(&mut c, 1, source, entry).await;
        assert_eq!(refused(&mut c).await, Some(Refusal::Malformed), "{source}");
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(entry_lines(&rig).is_empty(), "{:#?}", entry_lines(&rig));
    assert!(
        alerts(&rig, &karl)
            .await
            .iter()
            .all(|a| a.kind == "lost_contact"),
        "nothing but the closes' lost contact"
    );
}

/// The trace's `load` event, carrying a reset where one is given.
fn load_event(seq: u64, reset: Option<serde_json::Value>) -> FromClient {
    let mut payload = json!({"declaration": "d", "stack": {}});
    if let Some(reset) = reset {
        payload["reset"] = reset;
    }
    FromClient::Event {
        position: Position {
            generation: "g-1".into(),
            offset: seq * 100,
            digest: format!("d{seq}"),
        },
        replayed: false,
        event: TraceEvent {
            seq,
            mark: None,
            run: Some("run-2".into()),
            turn: None,
            kind: Some("load".into()),
            raw: json!({"kind": "load", "run": "run-2", "wall_ms": 1, "payload": payload}),
        },
    }
}

/// **A load that reset the agent is a loss that already happened**: the
/// trace's `load` event carrying `reset` raises `data_lost`, with no
/// options, naming the reason, once however often the event is read; a
/// load with no reset raises nothing.
///
/// Perturbation: drop the reset check, and a reset load raises nothing.
#[tokio::test]
async fn a_reset_load_raises_data_lost_once() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    o.admin_con.send(load_event(1, None)).await;
    let reset = json!({"prior_run": "run-1", "reason": "forced-unload"});
    o.admin_con.send(load_event(2, Some(reset.clone()))).await;
    let found = wait_alerts(&o.rig, &o.karl, "the loss", |a| !a.is_empty()).await;
    let lost = open_of(&found, "data_lost").expect("data_lost");
    assert_eq!(lost.subject, "reset of run-1");
    assert_eq!(lost.source, "trace");
    assert!(lost.options.is_empty());
    assert!(lost.summary.contains("forced unload"), "{}", lost.summary);
    let id: AgentId = o.karl.parse().unwrap();
    let s = o.rig.store();
    let alert = s.alerts_on(&id, 10).await.unwrap()[0].clone();
    // Acknowledged, then read again from a replay: nothing is raised again.
    sqlx::query(
        "UPDATE alert SET acknowledged_at = now(), acknowledged_by = $2 WHERE alert_id = $1",
    )
    .bind(&alert.alert_id)
    .bind(&o.bea)
    .execute(&s.pool)
    .await
    .unwrap();
    o.admin_con.send(load_event(3, Some(reset))).await;
    wait_log(&o.rig, 1).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        alerts(&o.rig, &o.karl).await[0].acknowledged_at.is_some(),
        "the loss stands acknowledged"
    );
    let all = alerts(&o.rig, &o.karl).await;
    assert_eq!(
        all.iter().filter(|a| a.kind == "data_lost").count(),
        1,
        "{all:#?}"
    );
}

/// **The backlog is bounded, its drops counted and reported**: past
/// `BACKLOG_ENTRIES` the oldest go first; an acknowledgement reports the
/// count as one entry.
///
/// Perturbation: drop the backlog's bound, and an outage grows it without
/// limit.
#[test]
fn a_backlog_past_its_bound_drops_the_oldest_and_reports_the_count() {
    let entries = Entries::new("admin-con");
    for n in 0..BACKLOG_ENTRIES + 10 {
        entries.failure("probe", format!("{n}"));
    }
    assert_eq!(entries.held(), (BACKLOG_ENTRIES, 10));
    let pending = entries.pending();
    assert_eq!(pending[0].1["message"], "10", "the oldest went first");
    entries.ack(&entries.run(), 20);
    let pending = entries.pending();
    let report = pending.last().unwrap();
    assert_eq!(report.1["kind"], "entries_dropped");
    assert_eq!(report.1["count"], 10);
    assert_eq!(entries.held().1, 0);
}

/// Wait for the backlog to hold `n` entries.
fn wait_pending(entries: &Entries, n: usize) -> Vec<(String, serde_json::Value)> {
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let pending = entries.pending();
        if pending.len() >= n {
            return pending;
        }
        assert!(
            std::time::Instant::now() < until,
            "never {n} entries: {pending:#?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// **admin.log is read from admin-con's start, each line passed along, a
/// truncation or a replacement read from its start after a
/// discontinuity**: what stood at the start is not read; a line appended is
/// an entry, own by its caller uid; a truncation, or another file renamed
/// over the path, and a line after it are a discontinuity and that line.
#[test]
fn admin_log_is_read_from_the_start_and_a_truncation_is_marked() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("admin.log");
    std::fs::write(
        &path,
        format!("{}\n", line("show", MINE, json!({"kind": "state"}))),
    )
    .unwrap();
    let entries = Entries::new("admin-con");
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let tail = admin_log::tail(path.clone(), entries.clone(), MINE as u32, shutdown);
    std::thread::sleep(admin_log::POLL * 2);
    assert!(entries.pending().is_empty(), "no history at start");
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(
        f,
        "{}",
        line("load", THEIRS, json!({"kind": "config_invalid"}))
    )
    .unwrap();
    writeln!(f, "{}", line("show", MINE, json!({"kind": "state"}))).unwrap();
    writeln!(f, "not json").unwrap();
    let pending = wait_pending(&entries, 3);
    assert_eq!(pending[0].0, admin_log::SOURCE);
    assert_eq!(pending[0].1["own"], false);
    assert_eq!(pending[0].1["line"]["verb"], "load");
    assert_eq!(pending[1].1["own"], true);
    assert_eq!(pending[2].1["unparsed"], true);
    std::fs::write(
        &path,
        format!("{}\n", line("stop", THEIRS, json!({"kind": "at_rest"}))),
    )
    .unwrap();
    let pending = wait_pending(&entries, 5);
    assert!(
        pending[3].1["discontinuity"]
            .as_str()
            .is_some_and(|d| d.contains("truncated")),
        "{pending:#?}"
    );
    assert_eq!(pending[4].1["line"]["verb"], "stop");
    let fresh = dir.path().join("fresh.log");
    std::fs::write(
        &fresh,
        format!(
            "{}\n",
            line("validate", THEIRS, json!({"kind": "validated"}))
        ),
    )
    .unwrap();
    std::fs::rename(&fresh, &path).unwrap();
    let pending = wait_pending(&entries, 7);
    assert!(
        pending[5].1["discontinuity"]
            .as_str()
            .is_some_and(|d| d.contains("replaced")),
        "{pending:#?}"
    );
    assert_eq!(pending[6].1["line"]["verb"], "validate");
    stop.send_replace(true);
    tail.join().unwrap();
}

/// **A link or a FIFO planted at the path is refused, never read**: each
/// is the connector's own failure, once.
#[test]
fn admin_log_refuses_a_link_and_a_fifo() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real.log");
    std::fs::write(&real, "").unwrap();
    let link = dir.path().join("admin.log");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let fifo = dir.path().join("fifo.log");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: a path the test made, a mode for its own user.
    assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
    for (path, why) in [(link, "symlink"), (fifo, "not a regular file")] {
        let entries = Entries::new("admin-con");
        let (stop, shutdown) = tokio::sync::watch::channel(false);
        let tail = admin_log::tail(path, entries.clone(), MINE as u32, shutdown);
        let pending = wait_pending(&entries, 1);
        std::thread::sleep(admin_log::POLL * 2);
        stop.send_replace(true);
        tail.join().unwrap();
        let pending_after = entries.pending();
        assert_eq!(pending_after.len(), 1, "once: {pending_after:#?}");
        assert_eq!(pending[0].0, "admin-con");
        assert_eq!(pending[0].1["kind"], "admin_log_unreadable");
        assert!(
            pending[0].1["message"].as_str().unwrap().contains(why),
            "{pending:#?}"
        );
    }
}

/// **An entry is acknowledged only once durable, and applied from that
/// copy**: on a new connection, 1,000 entries are sent and acknowledged
/// while the drainer is held; once it resumes, each is one line of the log.
/// A line that fails is written on the drainer's next look.
///
/// Perturbation: acknowledge without writing the entry's row, and the
/// entries acknowledged are never logged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entries_acknowledged_are_applied_from_their_durable_copy() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let alerts = o.rig.listener.alerts().clone();
    let Operated {
        rig,
        admin_config,
        admin_con,
        ..
    } = o;
    drop(admin_con);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut again = admitted_admin(&admin_config, &["show"]).await;
    let held = alerts.stall_worker().await;
    let count = 1000u64;
    for seq in 1..=count {
        send_entry(
            &mut again,
            seq,
            "admin-con",
            json!({"kind": "probe", "message": format!("{seq}")}),
        )
        .await;
    }
    loop {
        let (_, seq) = entry_ack(&mut again).await;
        if seq == count {
            break;
        }
    }
    assert!(entry_lines(&rig).is_empty(), "nothing applied while held");
    drop(held);
    let until = tokio::time::Instant::now() + Duration::from_secs(60);
    while entry_lines(&rig).len() < count as usize {
        assert!(
            tokio::time::Instant::now() < until,
            "only {} of {count} lines",
            entry_lines(&rig).len()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(entry_lines(&rig).len(), count as usize);

    alerts.fail_next_line();
    send_entry(
        &mut again,
        count + 1,
        "admin-con",
        json!({"kind": "probe", "message": "retried"}),
    )
    .await;
    let lines = wait_entries(&rig, count as usize + 1).await;
    assert_eq!(lines.last().unwrap()["entry"]["message"], "retried");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(entry_lines(&rig).len(), count as usize + 1, "once");
}

/// **A file absent at admin-con's start is read from its start once it
/// appears**: its absence is a failure, once, and both lines written to it
/// are sent.
///
/// Perturbation: take every first open as the start's baseline, and the
/// lines written before the file was found are skipped.
#[test]
fn admin_log_absent_at_the_start_is_read_from_its_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("admin.log");
    let entries = Entries::new("admin-con");
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let tail = admin_log::tail(path.clone(), entries.clone(), MINE as u32, shutdown);
    let failed = wait_pending(&entries, 1);
    assert_eq!(failed[0].1["kind"], "admin_log_unreadable");
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n",
            line("load", THEIRS, json!({"kind": "config_invalid"})),
            line("stop", THEIRS, json!({"kind": "at_rest"}))
        ),
    )
    .unwrap();
    let pending = wait_pending(&entries, 3);
    assert_eq!(pending[1].1["line"]["verb"], "load");
    assert_eq!(pending[2].1["line"]["verb"], "stop");
    stop.send_replace(true);
    tail.join().unwrap();
}

/// **A copy-truncate regrown past the offset between two looks is seen**:
/// the digest of the last bytes read no longer matches, the discontinuity
/// is marked, and the new content is read from its start, whole, never
/// resumed mid-line.
///
/// Perturbation: drop the digest check, and the regrown file is resumed
/// at the old offset, mid-line.
#[test]
fn admin_log_truncated_and_regrown_between_looks_is_reread_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("admin.log");
    std::fs::write(&path, "").unwrap();
    let entries = Entries::new("admin-con");
    let (stop, shutdown) = tokio::sync::watch::channel(false);
    let tail = admin_log::tail(path.clone(), entries.clone(), MINE as u32, shutdown);
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{}", line("show", MINE, json!({"kind": "state"}))).unwrap();
    drop(f);
    wait_pending(&entries, 1);
    // Truncated in place and regrown past the offset in one write, well
    // inside a look's interval.
    let long = line(
        "load",
        THEIRS,
        json!({"kind": "config_invalid", "pad": "p".repeat(400)}),
    );
    std::fs::write(
        &path,
        format!(
            "{long}\n{}\n",
            line("stop", THEIRS, json!({"kind": "at_rest"}))
        ),
    )
    .unwrap();
    let pending = wait_pending(&entries, 4);
    assert!(
        pending[1].1["discontinuity"]
            .as_str()
            .is_some_and(|d| d.contains("truncated")),
        "{pending:#?}"
    );
    assert_eq!(pending[2].1["line"]["verb"], "load", "{pending:#?}");
    assert_eq!(pending[3].1["line"]["verb"], "stop");
    assert!(
        pending.iter().all(|(_, e)| e["unparsed"] != true),
        "no line read from its middle: {pending:#?}"
    );
    stop.send_replace(true);
    tail.join().unwrap();
}
