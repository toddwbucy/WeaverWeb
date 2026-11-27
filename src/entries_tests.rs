//! The connectors' entries (Spec 2.14; act 12, PR 2; `src/link/entries.rs`,
//! `src/link/admin_log.rs`, the listener's entry door): what the server
//! takes from a connector's entries and what it raises, against the real
//! listener and fake connectors dialing it with minted configs; and the
//! connector's own half, its backlog and its reading of `admin.log`, on
//! files the tests make. No name, path or uid is a real one.

use std::time::Duration;

use serde_json::json;

use crate::agents_tests::{Connector, Rig};
use crate::alerts_tests::{alerts, wait_alerts, wait_log};
use crate::link::admin_log;
use crate::link::admin_service::LogEvent;
use crate::link::entries::{BACKLOG_ENTRIES, Entries};
use crate::link::frames::{ENTRY_BOUND, FromClient, Position, Refusal, ToClient};
use crate::store::AgentId;
use crate::store::alerts::Alert;
use crate::traceview::TraceEvent;
use crate::verbs_tests::{Operated, dial_gate, operated, readmitted_admin};

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
    let mut again = readmitted_admin(&admin_config, &["show"]).await;
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
    let mut again = readmitted_admin(&admin_config, &["show"]).await;
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
        let mut c = readmitted_admin(&admin_config, &["show"]).await;
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
            replayed: false,
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
    let mut again = readmitted_admin(&admin_config, &["show"]).await;
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

/// **A row that keeps failing holds no other**: entries of one run whose
/// rows never apply are backed off and kept, and another run's entry behind
/// them is applied.
///
/// Perturbation: stop the drain at a failed row and try it first again with
/// no backoff, and the entry behind it is never applied.
#[tokio::test]
async fn a_row_that_keeps_failing_holds_no_other() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let other = "fedcba9876543210";
    o.rig.listener.alerts().poison_run(RUN);
    for seq in 1..=70 {
        send_entry(
            &mut o.admin_con,
            seq,
            "admin-con",
            json!({"kind": "probe", "message": format!("poisoned {seq}")}),
        )
        .await;
    }
    o.admin_con
        .send(FromClient::Entry {
            run: other.into(),
            seq: 1,
            source: "admin-con".into(),
            entry: json!({"kind": "probe", "message": "behind"}),
        })
        .await;
    let lines = wait_entries(&o.rig, 1).await;
    assert_eq!(lines[0]["entry"]["message"], "behind");
    let stuck: i64 =
        sqlx::query_scalar("SELECT count(*) FROM entry_inbox WHERE run_id = $1 AND attempts >= 1")
            .bind(RUN)
            .fetch_one(&o.rig.store().pool)
            .await
            .unwrap();
    assert!(stuck >= 1, "the failing rows are kept and backed off");
}

/// **An entry the door's parser took is kept whatever it holds**: a string
/// carrying a NUL, which `jsonb` refuses, is taken, acknowledged and logged,
/// and the link stands; its alert's text carries U+FFFD for the NUL, which
/// PostgreSQL's text holds none of.
///
/// Perturbation: keep the entry as `jsonb`, and its take fails, the
/// connection closing, and the connector's resend would close it again.
#[tokio::test]
async fn an_entry_holding_a_nul_is_taken_and_logged() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    send_entry(
        &mut o.admin_con,
        1,
        "admin-con",
        json!({"kind": "probe", "message": "a\u{0}b"}),
    )
    .await;
    assert_eq!(entry_ack(&mut o.admin_con).await, (RUN.to_owned(), 1));
    let lines = wait_entries(&o.rig, 1).await;
    assert_eq!(lines[0]["entry"]["message"], "a\u{0}b");
    let found = wait_alerts(&o.rig, &o.karl, "its alert", |a| !a.is_empty()).await;
    assert!(
        found[0].summary.contains("a\u{fffd}b"),
        "{}",
        found[0].summary
    );
}

/// **Every shape a connector sends is one the door takes for its source**,
/// its fallbacks included: a line of `admin.log` and its discontinuity, its
/// oversized marker, an `admin_log` entry past the bound (an 8 KiB line of
/// invalid UTF-8 grows past it through the lossy reading), a connector's
/// failure, its oversized failure, and its report of drops; and the reported
/// case is acknowledged through the real door.
///
/// Perturbation: replace an oversized `admin_log` entry with the failure
/// shape again, and the door refuses it, at every resend.
#[tokio::test]
async fn every_shape_a_connector_sends_passes_the_door() {
    use crate::link::frames::Plane;
    use crate::link::listener::entry_passes;
    let entries = Entries::new("admin-con");
    let invalid = vec![0xffu8; 8 * 1024];
    entries.push(
        admin_log::SOURCE,
        admin_log::entry_of(&LogEvent::Text {
            text: String::from_utf8_lossy(&invalid).into_owned(),
            through_service: false,
        }),
    );
    entries.push(
        admin_log::SOURCE,
        admin_log::entry_of(&LogEvent::Line {
            line: json!({"verb": "show", "uid": 1}),
            through_service: false,
        }),
    );
    entries.push(admin_log::SOURCE, admin_log::discontinuity("truncated"));
    entries.push(
        admin_log::SOURCE,
        json!({"oversized": "a line past the bound", "own": false, "read_at": 1}),
    );
    entries.failure("probe", "p".repeat(ENTRY_BOUND));
    entries.failure("link_outage", "down for 3 s");
    // Every shape so far, before the fillers below push them out.
    let mut pending = entries.pending();
    assert_eq!(pending.len(), 6, "{pending:#?}");
    for _ in 0..BACKLOG_ENTRIES + 3 {
        entries.failure("probe", "filler");
    }
    entries.ack(&entries.run(), (BACKLOG_ENTRIES + 3) as u64);
    let report = entries.pending();
    assert!(
        report.iter().any(|(_, e)| e["kind"] == "entries_dropped"),
        "{report:#?}"
    );
    pending.extend(report);
    let gate = Entries::new("gate-con");
    gate.failure("link_outage", "down for 3 s");
    gate.failure("probe", "p".repeat(ENTRY_BOUND));
    for (plane, sent) in [(Plane::Admin, pending), (Plane::Gate, gate.pending())] {
        for (source, entry) in sent {
            assert!(
                entry_passes(plane, &source, entry.clone()),
                "the {plane} door refuses {source}: {entry}"
            );
        }
    }

    // The reported case, through the real door: the invalid line is sent
    // as `admin_log`'s own oversized marker, and acknowledged.
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let fresh = Entries::new("admin-con");
    fresh.push(
        admin_log::SOURCE,
        admin_log::entry_of(&LogEvent::Text {
            text: String::from_utf8_lossy(&invalid).into_owned(),
            through_service: false,
        }),
    );
    let (source, entry) = fresh.pending().remove(0);
    assert!(entry.get("oversized").is_some(), "{entry}");
    send_entry(&mut o.admin_con, 1, &source, entry).await;
    assert_eq!(entry_ack(&mut o.admin_con).await, (RUN.to_owned(), 1));
}

/// **An alert and its record on the row are one transaction**: the row's
/// record failing after the raise rolls the raise back with it, and the
/// retry leaves one alert of one occurrence.
///
/// Perturbation: raise the alert in a transaction of its own, and the
/// retry raises it again, two occurrences.
#[tokio::test]
async fn an_alert_and_its_record_on_the_row_are_one() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    o.rig.listener.alerts().fail_next_record();
    send_entry(
        &mut o.admin_con,
        1,
        "admin-con",
        json!({"kind": "probe", "message": "once"}),
    )
    .await;
    wait_entries(&o.rig, 1).await;
    // The line is written before the claim's transaction commits, so the
    // alert is read once the row has left the inbox, which that commit does.
    let until = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM entry_inbox WHERE run_id = $1")
            .bind(RUN)
            .fetch_one(&o.rig.store().pool)
            .await
            .unwrap();
        if rows == 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < until,
            "the row never left the inbox"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let found = alerts(&o.rig, &o.karl).await;
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].occurrences, 1, "{found:#?}");
}

/// **The drain is fair**: a row that failed is never chosen ahead of a row
/// not tried since. Seventy rows whose every failure is slow, a batch of
/// them outlasting the first backoffs, sit ahead of another run's entry,
/// which is applied within a few batches.
///
/// Perturbation: choose due rows by their identity alone, and the slow
/// failures are chosen again ahead of it, batch after batch.
#[tokio::test]
async fn the_drain_is_fair_to_a_row_behind_slow_failures() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    o.rig
        .listener
        .alerts()
        .poison_run_slowly(RUN, Duration::from_millis(80));
    for seq in 1..=70 {
        send_entry(
            &mut o.admin_con,
            seq,
            "admin-con",
            json!({"kind": "probe", "message": format!("slow {seq}")}),
        )
        .await;
    }
    o.admin_con
        .send(FromClient::Entry {
            run: "fedcba9876543210".into(),
            seq: 1,
            source: "admin-con".into(),
            entry: json!({"kind": "probe", "message": "fresh"}),
        })
        .await;
    let until = tokio::time::Instant::now() + Duration::from_secs(10);
    while entry_lines(&o.rig).is_empty() {
        assert!(
            tokio::time::Instant::now() < until,
            "the fresh entry starved behind the slow failures"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(entry_lines(&o.rig)[0]["entry"]["message"], "fresh");
}

/// **The drainer lives and dies with its listener, and the next one drains
/// what stands, once**: entries taken while a listener's drainer is held
/// stay in the inbox when it stops, its drainer applying nothing after the
/// stop though released; the next listener on the store applies each, one
/// line each.
///
/// Perturbation: leave the drainer out of the listener's tasks, and the
/// stopped listener's drainer applies the rows once released.
#[tokio::test]
async fn the_drainer_stops_with_its_listener_and_the_next_drains_once() {
    let Some(mut o) = operated(&["show"]).await else {
        return;
    };
    let held = o.rig.listener.alerts().stall_worker().await;
    for seq in 1..=5 {
        send_entry(
            &mut o.admin_con,
            seq,
            "admin-con",
            json!({"kind": "probe", "message": format!("{seq}")}),
        )
        .await;
    }
    loop {
        if entry_ack(&mut o.admin_con).await.1 == 5 {
            break;
        }
    }
    let next = o.rig.next_listener().await;
    // The next listener's own drainer held too, so what is watched is the
    // stopped one's.
    let next_held = next.alerts().stall_worker().await;
    drop(held);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let standing: i64 = sqlx::query_scalar("SELECT count(*) FROM entry_inbox")
        .fetch_one(&o.rig.store().pool)
        .await
        .unwrap();
    assert_eq!(standing, 5, "the stopped listener's drainer applied rows");
    drop(next_held);
    wait_entries(&o.rig, 5).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(entry_lines(&o.rig).len(), 5, "{:#?}", entry_lines(&o.rig));
    next.stop().await;
}

/// **No two drainers ever apply one row**: two drainers on one store, the
/// listener's own held, apply two hundred rows between them, each row's
/// line once.
///
/// Perturbation: drop the claim, and both apply the same rows, lines
/// written twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_drainers_apply_disjoint_rows() {
    use crate::alerting::Alerts;
    use crate::store::alerts::TakenEntry;
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let _held = o.rig.listener.alerts().stall_worker().await;
    let store = o.rig.store().clone();
    let id: AgentId = o.karl.parse().unwrap();
    for seq in 1..=200u64 {
        let entry = json!({"kind": "probe", "message": format!("{seq}")});
        assert!(
            store
                .take_entry(&TakenEntry {
                    run: RUN,
                    seq,
                    agent: &id,
                    plane: "admin",
                    source: "admin-con",
                    entry: &entry,
                })
                .await
                .unwrap()
        );
    }
    let log = o.rig.operations_log_path();
    let (_one, one) = Alerts::start(store.clone(), Some(log.clone()));
    let (_two, two) = Alerts::start(store.clone(), Some(log));
    let until = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let standing: i64 = sqlx::query_scalar("SELECT count(*) FROM entry_inbox")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        if standing == 0 {
            break;
        }
        assert!(tokio::time::Instant::now() < until, "{standing} rows stand");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut seen: Vec<String> = entry_lines(&o.rig)
        .iter()
        .map(|l| l["entry"]["message"].as_str().unwrap().to_owned())
        .collect();
    let lines = seen.len();
    seen.sort();
    seen.dedup();
    assert_eq!((lines, seen.len()), (200, 200), "each row's line once");
    one.abort();
    two.abort();
}

/// **The stop runs in the order the work flows**: stopped with a person's
/// verb in flight, the listener's teardown raises its unknown, and the
/// alert and its operations-log line are written before the stop returns.
///
/// Perturbation: end the alerts' worker before the connections, and the
/// teardown's alert and line are lost.
#[tokio::test]
async fn a_stop_writes_its_teardowns_alerts_before_it_returns() {
    let Some(mut o) = operated(&["show", "load"]).await else {
        return;
    };
    let asked = crate::verbs_tests::ask(&o.rig, &o.bea_session, &o.karl, "load");
    let (_, verb, _) = crate::verbs_tests::take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load");
    o.rig.listener.stop().await;
    let found = alerts(&o.rig, &o.karl).await;
    assert!(
        found
            .iter()
            .any(|a| a.kind == "unknown" && a.subject == "load"),
        "{found:#?}"
    );
    let lines = o.rig.operations_log();
    assert!(
        lines
            .iter()
            .any(|l| l["act"] == "load" && l["outcome"] == "unknown"),
        "{lines:#?}"
    );
    let _ = asked.await;
}
