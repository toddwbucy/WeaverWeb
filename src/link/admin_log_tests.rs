//! conforms: web-the-admin-log-comes-through-the-service
//!
//! **admin-con's reader of the admin log, against the fake service** (Spec
//! 2.14, 7.2; `weaver-admin-operator-contract` section 8.6): each message
//! becomes its entry, `own` is the service's judgment, every gap is a
//! discontinuity, and each word the log connection can meet is placed. No
//! test reads a file or reaches a service.

use super::admin_log::{self, SOURCE};
use super::admin_service::{Discontinuity, LogEvent, Purpose, Refused, word};
use super::client::Backoff;
use super::entries::Entries;
use super::fake_service::FakeService;
use serde_json::json;
use std::time::Duration;
use tokio::sync::watch;

const SOON: Duration = Duration::from_secs(10);

const SHORT: Backoff = Backoff {
    base: Duration::from_millis(20),
    cap: Duration::from_millis(40),
};

struct Reading {
    entries: Entries,
    _stop: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

fn follow(fake: &FakeService, backoff: Backoff) -> Reading {
    let entries = Entries::new("admin-con");
    let (stop, shutdown) = watch::channel(false);
    let task = admin_log::follow(
        fake.socket.clone(),
        "karl".into(),
        entries.clone(),
        backoff,
        shutdown,
    );
    Reading {
        entries,
        _stop: stop,
        task,
    }
}

/// The log connections the fake admitted.
fn logs(fake: &FakeService) -> usize {
    fake.with(|s| s.opened.iter().filter(|p| **p == Purpose::Log).count())
}

async fn wait_logs(fake: &FakeService, n: usize) {
    let until = tokio::time::Instant::now() + SOON;
    while logs(fake) < n {
        assert!(
            tokio::time::Instant::now() < until,
            "never {n} log connections"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // The connection's sender stands once its opening is answered.
    tokio::time::sleep(Duration::from_millis(30)).await;
}

async fn wait_entries(entries: &Entries, n: usize) -> Vec<serde_json::Value> {
    let until = tokio::time::Instant::now() + SOON;
    loop {
        let pending = entries.pending();
        if pending.len() >= n {
            return pending.into_iter().map(|(_, e)| e).collect();
        }
        assert!(
            tokio::time::Instant::now() < until,
            "never {n} entries: {pending:#?}"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// The source of every entry, and each one's own field or marker.
fn shapes(entries: &Entries) -> Vec<String> {
    entries
        .pending()
        .into_iter()
        .map(|(source, e)| {
            if let Some(kind) = e["kind"].as_str() {
                format!("{source} failure {kind}")
            } else if e.get("discontinuity").is_some() {
                format!("{source} discontinuity")
            } else if e.get("oversized").is_some() {
                format!("{source} oversized")
            } else if e.get("unparsed").is_some() {
                format!("{source} text own={}", e["own"])
            } else {
                format!("{source} line own={}", e["own"])
            }
        })
        .collect()
}

/// **Each message is its entry, and `own` is the service's
/// `through_service`**, whatever the line's caller uid says: a line, a text
/// line, an oversized marker, and the two discontinuities.
///
/// Perturbation (measured 2026-11-27): judge `own` by the line's `uid`
/// against admin-con's real uid again, and the service's judgment is lost.
#[tokio::test]
async fn each_message_is_an_entry_and_own_is_the_services_judgment() {
    let fake = FakeService::start(&["show"], "idle");
    let r = follow(&fake, SHORT);
    wait_logs(&fake, 1).await;
    let me = unsafe { libc::getuid() };
    fake.log(&LogEvent::Line {
        line: json!({"verb": "load", "uid": me + 1}),
        through_service: true,
    });
    fake.log(&LogEvent::Line {
        line: json!({"verb": "load", "uid": me}),
        through_service: false,
    });
    fake.log(&LogEvent::Text {
        text: "not json".into(),
        through_service: true,
    });
    fake.log(&LogEvent::Oversized(9000));
    fake.log(&LogEvent::Discontinuity(Discontinuity::Truncated));
    fake.log(&LogEvent::Discontinuity(Discontinuity::Replaced));
    let got = wait_entries(&r.entries, 6).await;
    assert_eq!(
        shapes(&r.entries),
        [
            "admin_log line own=true",
            "admin_log line own=false",
            "admin_log text own=true",
            "admin_log oversized",
            "admin_log discontinuity",
            "admin_log discontinuity",
        ]
    );
    assert_eq!(got[0]["line"]["verb"], "load", "the line as admin wrote it");
    assert_eq!(got[2]["line"], "not json");
    assert_eq!(
        got[4]["discontinuity"],
        "admin.log was truncated; read from its start"
    );
    assert_eq!(
        got[5]["discontinuity"],
        "admin.log was replaced; read from its start"
    );
    assert!(r.entries.pending().iter().all(|(s, _)| s == SOURCE));
}

/// **Every gap is a discontinuity**: the service's close is one, and the
/// connection is reopened on the backoff; an opening admitted after a
/// refusal is one more, since lines appended meanwhile were not read; and
/// `opening_late` and `full` raise no failure.
///
/// Perturbation (measured 2026-11-27): drop the discontinuity at a reopen
/// after a refusal, and the gap reads as continuous.
#[tokio::test]
async fn a_close_and_a_reopening_after_a_refusal_are_each_a_discontinuity() {
    let fake = FakeService::start(&["show"], "idle");
    let r = follow(&fake, SHORT);
    wait_logs(&fake, 1).await;
    fake.log_close();
    wait_logs(&fake, 2).await;
    fake.log(&LogEvent::Line {
        line: json!({"n": 1}),
        through_service: false,
    });
    wait_entries(&r.entries, 2).await;
    for passing in [word::OPENING_LATE, word::FULL] {
        fake.refuse(Some(Purpose::Log), passing);
        fake.log_close();
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    fake.with(|s| s.refuse = None);
    wait_logs(&fake, 3).await;
    fake.log(&LogEvent::Line {
        line: json!({"n": 2}),
        through_service: false,
    });
    wait_entries(&r.entries, 5).await;
    assert_eq!(
        shapes(&r.entries),
        [
            "admin_log discontinuity",
            "admin_log line own=false",
            "admin_log discontinuity",
            "admin_log discontinuity",
            "admin_log line own=false",
        ]
    );
    let got = r.entries.pending();
    assert!(
        got[3].1["discontinuity"]
            .as_str()
            .unwrap()
            .contains("was refused"),
        "{:?}",
        got[3].1
    );
}

/// **`log_behind` closes the connection with a discontinuity, and it is
/// reopened at once**, not on the backoff.
#[tokio::test]
async fn log_behind_is_a_discontinuity_reopened_at_once() {
    let fake = FakeService::start(&["show"], "idle");
    let slow = Backoff {
        base: Duration::from_secs(30),
        cap: Duration::from_secs(30),
    };
    let r = follow(&fake, slow);
    wait_logs(&fake, 1).await;
    fake.log_behind();
    tokio::time::timeout(Duration::from_secs(2), wait_logs(&fake, 2))
        .await
        .expect("reopened at once, not on the backoff");
    assert_eq!(shapes(&r.entries), ["admin_log discontinuity"]);
}

/// **A refusal of the opening by `log_unread`, or by a word of admin-con's
/// defect, raises one failure per change of word, not one per retry**; an
/// admission clears it.
///
/// Perturbation (measured 2026-11-27): raise the entry at every refused
/// opening, and the retries raise one each.
#[tokio::test]
async fn log_unread_raises_one_entry_per_change_not_per_retry() {
    let fake = FakeService::start(&["show"], "idle");
    fake.refuse(Some(Purpose::Log), word::LOG_UNREAD);
    let r = follow(&fake, SHORT);
    tokio::time::sleep(Duration::from_millis(400)).await;
    fake.refuse(Some(Purpose::Log), word::NAME_REFUSED);
    tokio::time::sleep(Duration::from_millis(400)).await;
    fake.with(|s| s.refuse = None);
    wait_logs(&fake, 1).await;
    wait_entries(&r.entries, 3).await;
    assert_eq!(
        shapes(&r.entries),
        [
            "admin-con failure service_log_unread",
            "admin-con failure service_name_refused",
            "admin_log discontinuity",
        ]
    );
}

/// **`ask_malformed` on the log connection closes it**: a failure entry,
/// a discontinuity, and the connection reopened.
#[tokio::test]
async fn ask_malformed_on_the_log_closes_it_with_an_entry() {
    let fake = FakeService::start(&["show"], "idle");
    let r = follow(&fake, SHORT);
    wait_logs(&fake, 1).await;
    let mut line =
        serde_json::to_vec(&json!({"refused": Refused::new(word::ASK_MALFORMED, "a byte")}))
            .unwrap();
    line.push(b'\n');
    fake.log_raw(&line);
    wait_logs(&fake, 2).await;
    assert_eq!(
        shapes(&r.entries),
        [
            "admin-con failure service_ask_malformed",
            "admin_log discontinuity"
        ]
    );
}

/// **The reader ends at the stop**, and where the stop's sender is gone.
#[tokio::test]
async fn the_reader_ends_at_the_stop() {
    let fake = FakeService::start(&["show"], "idle");
    let r = follow(&fake, SHORT);
    wait_logs(&fake, 1).await;
    r._stop.send(true).unwrap();
    tokio::time::timeout(SOON, r.task).await.unwrap().unwrap();
    let r = follow(&fake, SHORT);
    wait_logs(&fake, 2).await;
    drop(r._stop);
    tokio::time::timeout(SOON, r.task).await.unwrap().unwrap();
}
