//! conforms: web-the-service-socket-bounds-every-line
//!
//! The service's socket from the connector's side (Spec 7.2,
//! `weaver-admin-operator-contract` section 8): the client against the fake
//! service, every opening word at every purpose it can answer, the bounds on
//! every line, and each exchange the fake serves. No test reaches an agent or
//! a privileged program.

use super::admin_service::{
    self, ANSWER_WAIT, Ask, ControlLine, Discontinuity, Final, LineError, LogEvent, LogLine,
    OPENING_BOUND, OpenError, Opened, Purpose, word,
};
use super::fake_relay::FakeRelay;
use super::fake_service::{Act, Cut, FakeService, Grants, Script, raw_opening};
use super::frames::Principal;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Notify;

const AGENT: &str = "a1";
const SHORT: Duration = Duration::from_secs(5);

fn person() -> Principal {
    Principal::Person {
        person_id: "pe-0123456789abcdef".to_owned(),
    }
}

async fn open(fake: &FakeService, purpose: Purpose) -> admin_service::Connection {
    match admin_service::open(&fake.socket, purpose, AGENT, SHORT).await {
        Ok(Opened::Open(connection)) => connection,
        other => panic!("the opening was not answered open: {other:?}"),
    }
}

async fn refused(fake: &FakeService, purpose: Purpose) -> admin_service::Refused {
    match admin_service::open(&fake.socket, purpose, AGENT, SHORT).await {
        Ok(Opened::Refused(refusal)) => refusal,
        other => panic!("the opening was not refused: {other:?}"),
    }
}

async fn next(connection: &mut admin_service::Connection) -> ControlLine {
    tokio::time::timeout(SHORT, connection.next_control())
        .await
        .expect("a control line in time")
        .expect("a well-formed line")
        .expect("the connection stands")
}

/// **Every word that can answer an opening is read by the client, at each
/// purpose it can answer**: the six of any purpose at control, trace and
/// log, `relay_unreachable` at trace and `log_unread` at log.
#[tokio::test]
async fn every_opening_word_is_read_at_each_purpose_it_answers() {
    let fake = FakeService::start(&["show"], "idle");
    let mut read: Vec<(Purpose, String)> = Vec::new();
    for purpose in [Purpose::Control, Purpose::Trace, Purpose::Log] {
        let mut words: Vec<&str> = word::OPENING.to_vec();
        match purpose {
            Purpose::Trace => words.push(word::RELAY_UNREACHABLE),
            Purpose::Log => words.push(word::LOG_UNREAD),
            Purpose::Control => {}
        }
        for w in words {
            fake.refuse(Some(purpose), w);
            read.push((purpose, refused(&fake, purpose).await.word));
        }
    }
    // What the client read, against the contract's table written out.
    let opening = [
        "opening_malformed",
        "opening_late",
        "version_unknown",
        "purpose_unknown",
        "name_refused",
        "full",
    ];
    let mut want: Vec<(Purpose, String)> = Vec::new();
    for (purpose, own) in [
        (Purpose::Control, None),
        (Purpose::Trace, Some("relay_unreachable")),
        (Purpose::Log, Some("log_unread")),
    ] {
        want.extend(
            opening
                .iter()
                .chain(own.iter())
                .map(|w| (purpose, (*w).to_owned())),
        );
    }
    assert_eq!(read, want);
}

/// **The eighteen words are spelled as the contract's section 8.2 writes
/// them**, each constant checked once against the literal, so the fake and
/// the client cannot agree on a misspelling.
#[test]
fn the_eighteen_words_are_spelled_as_the_contract_writes_them() {
    let spelled = [
        (word::OPENING_MALFORMED, "opening_malformed"),
        (word::OPENING_LATE, "opening_late"),
        (word::VERSION_UNKNOWN, "version_unknown"),
        (word::PURPOSE_UNKNOWN, "purpose_unknown"),
        (word::NAME_REFUSED, "name_refused"),
        (word::FULL, "full"),
        (word::RELAY_UNREACHABLE, "relay_unreachable"),
        (word::LOG_UNREAD, "log_unread"),
        (word::ASK_UNKNOWN, "ask_unknown"),
        (word::ASK_MALFORMED, "ask_malformed"),
        (word::ASK_OUTSTANDING, "ask_outstanding"),
        (word::GRANTS_UNREAD, "grants_unread"),
        (word::VERB_UNKNOWN, "verb_unknown"),
        (word::NOT_GRANTED, "not_granted"),
        (word::GRANT_UNREAD, "grant_unread"),
        (word::BUSY, "busy"),
        (word::NOT_STARTED, "not_started"),
        (word::LOG_BEHIND, "log_behind"),
    ];
    for (constant, literal) in spelled {
        assert_eq!(constant, literal);
    }
    assert_eq!(
        word::OPENING,
        [
            "opening_malformed",
            "opening_late",
            "version_unknown",
            "purpose_unknown",
            "name_refused",
            "full"
        ]
    );
}

/// **A read the caller gives up mid-line is taken up where it stood**: a
/// line sent in two parts, the wait ending between them, is read whole by
/// the next call.
///
/// Perturbation (measured 2026-11-27): make the part line local to the
/// read again, and the next call reads from the line's middle, malformed.
#[tokio::test]
async fn a_read_dropped_mid_line_is_taken_up_where_it_stood() {
    let dir = crate::test_dir::tempdir().unwrap();
    let socket = dir.path().join("service.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let (cut, cut_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut opening = String::new();
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut opening).await.unwrap();
        stream
            .write_all(
                b"{\"open\":{\"version\":1,\"instance\":\"0123456789ABCDEF\"}}\n{\"grants\":{\"ve",
            )
            .await
            .unwrap();
        cut_rx.await.unwrap();
        stream.write_all(b"rbs\":[\"show\"]}}\n").await.unwrap();
        stream
    });
    let mut c = match admin_service::open(&socket, Purpose::Control, AGENT, SHORT).await {
        Ok(Opened::Open(c)) => c,
        other => panic!("{other:?}"),
    };
    assert_eq!(c.instance, "0123456789ABCDEF", "an instance of either case");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), c.next_control())
            .await
            .is_err(),
        "the wait ends with part of a line read"
    );
    cut.send(()).unwrap();
    assert_eq!(
        next(&mut c).await,
        ControlLine::Grants {
            verbs: vec!["show".into()]
        }
    );
    drop(server.await.unwrap());
}

/// **The fake judges an opening line as the contract's table does**: a line
/// that does not parse, a version above its own naming `versions`, an
/// unknown purpose naming `purposes`, a refused name, and no line in time.
#[tokio::test]
async fn the_fake_judges_an_opening_by_the_table() {
    let fake = FakeService::start(&["show"], "idle");
    let read = |answer: Vec<u8>| -> serde_json::Value {
        serde_json::from_slice(&answer).expect("one refused line")
    };
    let answer = read(raw_opening(&fake.socket, b"not json\n").await);
    assert_eq!(answer["refused"]["word"], word::OPENING_MALFORMED);
    let answer = read(
        raw_opening(
            &fake.socket,
            b"{\"weaver_admin\":{\"version\":2,\"purpose\":\"control\",\"agent\":\"a1\"}}\n",
        )
        .await,
    );
    assert_eq!(answer["refused"]["word"], word::VERSION_UNKNOWN);
    assert_eq!(answer["refused"]["versions"], serde_json::json!([1]));
    let answer = read(
        raw_opening(
            &fake.socket,
            b"{\"weaver_admin\":{\"version\":1,\"purpose\":\"loop\",\"agent\":\"a1\"}}\n",
        )
        .await,
    );
    assert_eq!(answer["refused"]["word"], word::PURPOSE_UNKNOWN);
    assert_eq!(
        answer["refused"]["purposes"],
        serde_json::json!(["control", "trace", "log"])
    );
    let answer = read(
        raw_opening(
            &fake.socket,
            b"{\"weaver_admin\":{\"version\":1,\"purpose\":\"control\",\"agent\":\"admin\"}}\n",
        )
        .await,
    );
    assert_eq!(answer["refused"]["word"], word::NAME_REFUSED);
    fake.with(|s| s.opening_wait = Duration::from_millis(100));
    let answer = read(raw_opening(&fake.socket, b"").await);
    assert_eq!(answer["refused"]["word"], word::OPENING_LATE);
}

/// **An answer past its 4096 bytes is malformed, never read further.**
///
/// Perturbation (measured 2026-11-27): read the opening's answer at
/// `usize::MAX` in `admin_service::open` in place of `OPENING_BOUND`, and
/// this test is red, the client answering `Open` on the padded line.
#[tokio::test]
async fn an_answer_past_its_bound_is_malformed() {
    let fake = FakeService::start(&["show"], "idle");
    let pad = "x".repeat(OPENING_BOUND);
    let mut raw = serde_json::to_vec(&serde_json::json!({
        "open": {"version": 1, "instance": "0123456789abcdef", "pad": pad}
    }))
    .unwrap();
    raw.push(b'\n');
    fake.with(|s| s.raw_answer = Some(raw));
    match admin_service::open(&fake.socket, Purpose::Control, AGENT, SHORT).await {
        Err(OpenError::Malformed(why)) => assert!(why.contains("4096"), "{why}"),
        other => panic!("an answer past its bound was taken: {other:?}"),
    }
}

/// An answer that ends without its newline, names another version, or an
/// `instance` of another shape is malformed; one past the wait is late; a
/// close with no byte is the service's close.
#[tokio::test]
async fn an_answer_of_another_shape_or_time_is_named() {
    let fake = FakeService::start(&["show"], "idle");
    for (raw, says) in [
        (
            &b"{\"open\":{\"version\":1,\"instance\":\"0123456789abcdef\"}}"[..],
            "newline",
        ),
        (
            b"{\"open\":{\"version\":2,\"instance\":\"0123456789abcdef\"}}\n",
            "version",
        ),
        (
            b"{\"open\":{\"version\":1,\"instance\":\"0123\"}}\n",
            "instance",
        ),
        (b"{\"opened\":{}}\n", "parse"),
    ] {
        fake.with(|s| s.raw_answer = Some(raw.to_vec()));
        match admin_service::open(&fake.socket, Purpose::Control, AGENT, SHORT).await {
            Err(OpenError::Malformed(why)) => assert!(why.contains(says), "{why}"),
            other => panic!("{says}: {other:?}"),
        }
    }
    fake.with(|s| s.raw_answer = Some(Vec::new()));
    assert_eq!(
        admin_service::open(&fake.socket, Purpose::Control, AGENT, SHORT)
            .await
            .unwrap_err(),
        OpenError::Closed
    );
    fake.with(|s| {
        s.raw_answer = None;
        s.answer_delay = Duration::from_millis(400);
    });
    assert_eq!(
        admin_service::open(
            &fake.socket,
            Purpose::Control,
            AGENT,
            Duration::from_millis(100)
        )
        .await
        .unwrap_err(),
        OpenError::Late
    );
    let gone = fake.socket.with_file_name("none.sock");
    assert!(matches!(
        admin_service::open(&gone, Purpose::Control, AGENT, SHORT).await,
        Err(OpenError::Unreachable(_))
    ));
    let long = "a".repeat(OPENING_BOUND);
    assert_eq!(
        admin_service::open(&fake.socket, Purpose::Control, &long, SHORT)
            .await
            .unwrap_err(),
        OpenError::TooLong
    );
    assert_eq!(ANSWER_WAIT, Duration::from_secs(11));
}

/// A line read at its bound: exactly the bound is taken, one past is not,
/// and a connection ended inside a line is named.
#[tokio::test]
async fn a_line_is_read_within_its_bound_and_never_past_it() {
    let line = |n: usize| {
        let mut bytes = vec![b'a'; n - 1];
        bytes.push(b'\n');
        bytes
    };
    let (at_bytes, past_bytes) = (line(64), line(65));
    let mut at = BufReader::new(&at_bytes[..]);
    assert_eq!(
        admin_service::read_line(&mut at, 64)
            .await
            .unwrap()
            .unwrap()
            .len(),
        64
    );
    let mut past = BufReader::new(&past_bytes[..]);
    assert_eq!(
        admin_service::read_line(&mut past, 64).await,
        Err(LineError::TooLong(64))
    );
    let mut cut = BufReader::new(&b"abc"[..]);
    assert_eq!(
        admin_service::read_line(&mut cut, 64).await,
        Err(LineError::Truncated)
    );
    let mut empty = BufReader::new(&b""[..]);
    assert_eq!(admin_service::read_line(&mut empty, 64).await, Ok(None));
}

/// **The contract's own example lines parse as the client's types**.
#[test]
fn the_contracts_example_lines_parse() {
    let ran: ControlLine = serde_json::from_str(
        r#"{"ran":{"run":"0123456789abcdef","verb":"load","exit_code":0,"answer":{"kind":"answered"},"stderr":"...","stderr_cut":false}}"#,
    )
    .unwrap();
    assert!(matches!(ran, ControlLine::Ran(r) if r.verb == "load" && !r.stderr_cut));
    let run: ControlLine = serde_json::from_str(
        r#"{"run":{"instance":"0123456789abcdef","standing":[{"run":"0123456789abcdef","verb":"unload","started_ms":1795651200000}],"ended":{"ran":{"run":"fedcba9876543210","verb":"load","exit_code":1,"answer":{}}}}}"#,
    )
    .unwrap();
    let ControlLine::Run(run) = run else {
        panic!("a run answer");
    };
    assert_eq!(run.standing[0].started_ms, Some(1795651200000));
    assert!(matches!(run.ended, Some(Final::Ran(ref r)) if r.exit_code == 1));
    let busy: ControlLine = serde_json::from_str(
        r#"{"refused":{"word":"busy","detail":"...","run":"0123456789abcdef"}}"#,
    )
    .unwrap();
    assert!(
        matches!(busy, ControlLine::Refused(r) if r.run.as_deref() == Some("0123456789abcdef"))
    );
    let unstarted: admin_service::Standing =
        serde_json::from_str(r#"{"run":"0123456789abcdef","verb":"load","started_ms":null}"#)
            .unwrap();
    assert_eq!(unstarted.started_ms, None);
    let ask = String::from_utf8(admin_service::to_line(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    }))
    .unwrap();
    assert_eq!(
        ask,
        "{\"verb\":{\"verb\":\"load\",\"principal\":{\"kind\":\"person\",\"person_id\":\"pe-0123456789abcdef\"}}}\n"
    );
    assert_eq!(
        admin_service::to_line(&Ask::Grants {}),
        b"{\"grants\":{}}\n"
    );
    assert_eq!(admin_service::to_line(&Ask::Run {}), b"{\"run\":{}}\n");
}

/// **The grants**: a list, `grants_unread`, or silence the caller bounds.
#[tokio::test]
async fn the_grants_are_a_list_a_refusal_or_silence() {
    let fake = FakeService::start(&["show", "load"], "idle");
    let mut c = open(&fake, Purpose::Control).await;
    assert!(admin_service::is_id(&c.instance));
    c.ask(&Ask::Grants {}).await.unwrap();
    assert_eq!(
        next(&mut c).await,
        ControlLine::Grants {
            verbs: vec!["show".into(), "load".into()]
        }
    );
    fake.with(|s| s.grants = Grants::Unread);
    c.ask(&Ask::Grants {}).await.unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::GRANTS_UNREAD));
    fake.with(|s| s.grants = Grants::Silent);
    c.ask(&Ask::Grants {}).await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(200), c.next_control())
            .await
            .is_err()
    );
}

/// **A verb is named, then answered**, its principal recorded and its
/// standard error's cut carried; a grant read again at the verb refuses
/// one the grants answer listed; a refusal of section 8.4 and a fault
/// arrive as scripted.
#[tokio::test]
async fn a_verb_is_named_then_answered_and_its_grant_is_read_at_the_ask() {
    let fake = FakeService::start(&["show", "load", "unload"], "idle");
    fake.script(Script::of(
        Some("load"),
        Act::Answer {
            exit_code: 1,
            answer: serde_json::json!({"kind": "refused"}),
            stderr: "x".repeat(10),
            stderr_cut: true,
        },
    ));
    let mut c = open(&fake, Purpose::Control).await;
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    let ControlLine::Started { run } = next(&mut c).await else {
        panic!("started first");
    };
    let ControlLine::Ran(ran) = next(&mut c).await else {
        panic!("ran last");
    };
    assert_eq!(
        (ran.run.as_str(), ran.exit_code, ran.stderr_cut),
        (run.as_str(), 1, true)
    );
    assert_eq!(
        fake.with(|s| s.ran.clone()),
        vec![("load".to_owned(), person())]
    );
    // The grant is read at the ask: `unload` listed, then taken away.
    fake.with(|s| s.granted = Some(vec!["show".into(), "load".into()]));
    c.ask(&Ask::Verb {
        verb: "unload".into(),
        principal: Principal::Server,
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::NOT_GRANTED));
    fake.script(Script::of(
        Some("load"),
        Act::Refuse(admin_service::Refused::new(word::GRANT_UNREAD, "no read")),
    ));
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::GRANT_UNREAD));
    fake.script(Script::of(Some("load"), Act::Fault("signal 9".into())));
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Started { .. }));
    assert!(matches!(next(&mut c).await, ControlLine::Fault(f) if f.detail == "signal 9"));
    c.ask(&Ask::Verb {
        verb: "dance".into(),
        principal: person(),
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::VERB_UNKNOWN));
    // A `show` answers from the state, `in_transition` for its count.
    fake.with(|s| s.in_transition = 1);
    for kind in ["in_transition", "state"] {
        c.ask(&Ask::Verb {
            verb: "show".into(),
            principal: Principal::Server,
        })
        .await
        .unwrap();
        assert!(matches!(next(&mut c).await, ControlLine::Started { .. }));
        assert!(matches!(next(&mut c).await, ControlLine::Ran(r) if r.answer["kind"] == kind));
    }
}

/// **One run of an agent at a time**: `busy` names the run standing; a
/// `show` starts beside it, and a `force-unload` beside an `unload`.
#[tokio::test]
async fn busy_names_the_run_and_two_overlaps_are_admitted() {
    let fake = FakeService::start(&["show", "load", "unload", "force-unload"], "idle");
    let release = Arc::new(Notify::new());
    let mut held = Script::answer(Some("unload"), serde_json::json!({"kind": "unloaded"}));
    held.release = Some(release.clone());
    fake.script(held);
    let mut first = open(&fake, Purpose::Control).await;
    first
        .ask(&Ask::Verb {
            verb: "unload".into(),
            principal: person(),
        })
        .await
        .unwrap();
    let ControlLine::Started { run } = next(&mut first).await else {
        panic!("started");
    };
    let mut second = open(&fake, Purpose::Control).await;
    second
        .ask(&Ask::Verb {
            verb: "load".into(),
            principal: person(),
        })
        .await
        .unwrap();
    assert!(
        matches!(next(&mut second).await, ControlLine::Refused(r) if r.word == word::BUSY && r.run.as_deref() == Some(run.as_str()))
    );
    second
        .ask(&Ask::Verb {
            verb: "show".into(),
            principal: Principal::Server,
        })
        .await
        .unwrap();
    assert!(matches!(
        next(&mut second).await,
        ControlLine::Started { .. }
    ));
    assert!(matches!(next(&mut second).await, ControlLine::Ran(_)));
    second
        .ask(&Ask::Verb {
            verb: "force-unload".into(),
            principal: person(),
        })
        .await
        .unwrap();
    assert!(matches!(
        next(&mut second).await,
        ControlLine::Started { .. }
    ));
    assert!(matches!(next(&mut second).await, ControlLine::Ran(_)));
    release.notify_one();
    assert!(matches!(next(&mut first).await, ControlLine::Ran(r) if r.run == run));
}

/// **A run stands from its grant and goes on when its connection closes**:
/// `started_ms` null while it waits to start, then a later `run` names it
/// standing, then ended; a run whose start fails leaves with nothing kept.
#[tokio::test]
async fn a_run_stands_from_its_grant_and_a_later_run_ask_names_it() {
    let fake = FakeService::start(&["show", "load"], "idle");
    let start = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let mut script = Script::answer(Some("load"), serde_json::json!({"kind": "loaded"}));
    script.hold_start = Some(start.clone());
    script.release = Some(release.clone());
    script.cut = Cut::Unstarted;
    fake.script(script);
    let mut c = open(&fake, Purpose::Control).await;
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    // The connection closes while the run stands unstarted.
    assert_eq!(
        tokio::time::timeout(SHORT, c.next_control()).await.unwrap(),
        Ok(None)
    );
    let ask_run = async || {
        let mut r = open(&fake, Purpose::Control).await;
        r.ask(&Ask::Run {}).await.unwrap();
        match next(&mut r).await {
            ControlLine::Run(run) => run,
            other => panic!("{other:?}"),
        }
    };
    // Standing, unstarted.
    let mut answer = ask_run().await;
    for _ in 0..50 {
        if !answer.standing.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        answer = ask_run().await;
    }
    assert_eq!(answer.standing.len(), 1);
    assert_eq!(answer.standing[0].started_ms, None);
    assert_eq!(answer.ended, None);
    let run = answer.standing[0].run.clone();
    start.notify_one();
    // The run goes on, its start now dated.
    let mut started = ask_run().await;
    for _ in 0..50 {
        if started
            .standing
            .first()
            .is_some_and(|s| s.started_ms.is_some())
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        started = ask_run().await;
    }
    assert!(started.standing[0].started_ms.is_some());
    release.notify_one();
    let mut ended = ask_run().await;
    for _ in 0..50 {
        if ended.ended.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        ended = ask_run().await;
    }
    assert!(ended.standing.is_empty());
    assert_eq!(ended.ended.as_ref().map(Final::run), Some(run.as_str()));
    // A start that fails: refused `not_started`, the last ended unchanged.
    fake.script(Script::of(Some("load"), Act::StartFails));
    let mut c = open(&fake, Purpose::Control).await;
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::NOT_STARTED));
    let after = ask_run().await;
    assert!(after.standing.is_empty());
    assert_eq!(after.ended.as_ref().map(Final::run), Some(run.as_str()));
}

/// **A restart closes every connection and forgets every run under a new
/// `instance`**, while a run it started goes on to its end, which is kept
/// nowhere and sent to no one.
#[tokio::test]
async fn a_restart_closes_every_connection_and_forgets_its_runs() {
    let dir = crate::test_dir::tempdir().unwrap();
    let file = dir.path().join("trace.ndjson");
    std::fs::write(&file, b"{\"n\":1}\n").unwrap();
    let mut relay = FakeRelay::new(&file);
    relay.start();
    let fake = FakeService::start(&["show", "load"], "idle");
    fake.with(|s| s.relay = Some(relay.socket.clone()));
    let release = Arc::new(Notify::new());
    let mut script = Script::answer(Some("load"), serde_json::json!({"kind": "loaded"}));
    script.release = Some(release.clone());
    fake.script(script);
    let mut c = open(&fake, Purpose::Control).await;
    let before = c.instance.clone();
    let mut log = open(&fake, Purpose::Log).await;
    let mut trace = open(&fake, Purpose::Trace).await.into_inner();
    trace
        .get_mut()
        .write_all(b"{\"offset\":0}\n")
        .await
        .unwrap();
    let mut header = String::new();
    trace.read_line(&mut header).await.unwrap();
    assert!(header.contains("\"header\""), "{header}");
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Started { .. }));
    fake.restart();
    assert_eq!(
        tokio::time::timeout(SHORT, c.next_control()).await.unwrap(),
        Ok(None),
        "the control connection closes"
    );
    assert_eq!(
        tokio::time::timeout(SHORT, log.next_log()).await.unwrap(),
        Ok(None),
        "the log connection closes"
    );
    let mut rest = Vec::new();
    tokio::time::timeout(SHORT, trace.read_to_end(&mut rest))
        .await
        .unwrap()
        .unwrap();
    release.notify_one();
    // The run goes on to its end.
    for _ in 0..100 {
        if fake.with(|s| s.runs.contains(&"end load".to_owned())) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(fake.with(|s| s.runs.clone()), ["start load", "end load"]);
    let mut r = open(&fake, Purpose::Control).await;
    assert_ne!(r.instance, before);
    r.ask(&Ask::Run {}).await.unwrap();
    let ControlLine::Run(answer) = next(&mut r).await else {
        panic!("a run answer");
    };
    assert_eq!(answer.instance, r.instance);
    assert!(answer.standing.is_empty());
    assert_eq!(answer.ended, None, "its end is kept nowhere");
}

/// **A cut while running closes the connection after `started`**, and a
/// cut after the start and before its line leaves the run started and
/// standing.
#[tokio::test]
async fn a_cut_after_the_start_leaves_the_run_standing() {
    let fake = FakeService::start(&["show", "load", "validate"], "idle");
    for (verb, cut) in [
        ("load", Cut::WhileRunning),
        ("validate", Cut::BeforeStarted),
    ] {
        let release = Arc::new(Notify::new());
        let mut script = Script::answer(Some(verb), serde_json::json!({"kind": "done"}));
        script.release = Some(release.clone());
        script.cut = cut;
        fake.script(script);
        let mut c = open(&fake, Purpose::Control).await;
        c.ask(&Ask::Verb {
            verb: verb.into(),
            principal: person(),
        })
        .await
        .unwrap();
        if cut == Cut::WhileRunning {
            assert!(matches!(next(&mut c).await, ControlLine::Started { .. }));
        }
        assert_eq!(
            tokio::time::timeout(SHORT, c.next_control()).await.unwrap(),
            Ok(None)
        );
        let mut r = open(&fake, Purpose::Control).await;
        r.ask(&Ask::Run {}).await.unwrap();
        let ControlLine::Run(answer) = next(&mut r).await else {
            panic!("a run answer");
        };
        assert_eq!(answer.standing.len(), 1, "{verb}");
        assert!(answer.standing[0].started_ms.is_some(), "{verb}");
        release.notify_one();
        for _ in 0..100 {
            if fake.with(|s| s.runs.contains(&format!("end {verb}"))) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// **Dropping the fake closes every connection that stands.**
#[tokio::test]
async fn dropping_the_fake_closes_every_connection() {
    let fake = FakeService::start(&["show"], "idle");
    let mut c = open(&fake, Purpose::Control).await;
    let mut log = open(&fake, Purpose::Log).await;
    drop(fake);
    assert_eq!(
        tokio::time::timeout(SHORT, c.next_control()).await.unwrap(),
        Ok(None)
    );
    assert_eq!(
        tokio::time::timeout(SHORT, log.next_log()).await.unwrap(),
        Ok(None)
    );
}

/// **The fake judges as a service does where the reading found it loose**:
/// a principal with a member past its shape, or a person's id of another
/// case, is `ask_malformed`; the grant is read before a scripted refusal,
/// so a verb not granted is `not_granted` whatever is scripted; and it
/// serves the one agent its first opening named.
#[tokio::test]
async fn the_fake_judges_the_principal_the_grant_and_the_agent_as_a_service() {
    let fake = FakeService::start(&["show"], "idle");
    for ask in [
        &b"{\"verb\":{\"verb\":\"show\",\"principal\":{\"kind\":\"server\",\"x\":1}}}\n"[..],
        b"{\"verb\":{\"verb\":\"show\",\"principal\":{\"kind\":\"person\",\"person_id\":\"pe-0123456789ABCDEF\"}}}\n",
    ] {
        let mut stream = open(&fake, Purpose::Control).await.into_inner();
        stream.get_mut().write_all(ask).await.unwrap();
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        assert!(line.contains(word::ASK_MALFORMED), "{line}");
    }
    let mut busy = admin_service::Refused::new(word::BUSY, "scripted");
    busy.run = Some("0123456789abcdef".into());
    fake.script(Script::of(Some("load"), Act::Refuse(busy)));
    let mut c = open(&fake, Purpose::Control).await;
    c.ask(&Ask::Verb {
        verb: "load".into(),
        principal: person(),
    })
    .await
    .unwrap();
    assert!(matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::NOT_GRANTED));
    match admin_service::open(&fake.socket, Purpose::Control, "a2", SHORT).await {
        Ok(Opened::Refused(r)) => assert_eq!(r.word, word::NAME_REFUSED),
        other => panic!("another agent was admitted: {other:?}"),
    }
}

/// **One ask at a time**: a line while one is outstanding closes the
/// connection; an unknown ask is refused and the connection stands; an ask
/// naming `length` is malformed and closes it.
#[tokio::test]
async fn a_control_connection_takes_one_ask_at_a_time() {
    let fake = FakeService::start(&["show"], "idle");
    let mut c = open(&fake, Purpose::Control).await;
    c.ask(&Ask::Grants {}).await.unwrap();
    let _ = next(&mut c).await;
    let mut stream = c.into_inner();
    stream
        .get_mut()
        .write_all(b"{\"later\":{}}\n")
        .await
        .unwrap();
    let mut line = String::new();
    stream.read_line(&mut line).await.unwrap();
    assert!(line.contains(word::ASK_UNKNOWN), "{line}");
    stream
        .get_mut()
        .write_all(b"{\"run\":{\"length\":3,\"sha256\":\"x\"}}\n")
        .await
        .unwrap();
    line.clear();
    stream.read_line(&mut line).await.unwrap();
    assert!(line.contains(word::ASK_MALFORMED), "{line}");
    line.clear();
    assert_eq!(stream.read_line(&mut line).await.unwrap(), 0, "closed");
    fake.with(|s| s.grants = Grants::Silent);
    let mut c = open(&fake, Purpose::Control).await;
    c.ask(&Ask::Grants {}).await.unwrap();
    c.ask(&Ask::Run {}).await.unwrap();
    assert!(
        matches!(next(&mut c).await, ControlLine::Refused(r) if r.word == word::ASK_OUTSTANDING)
    );
    assert_eq!(
        tokio::time::timeout(SHORT, c.next_control()).await.unwrap(),
        Ok(None)
    );
}

/// **A trace connection is the relay's wire after `open`**, the relay
/// dialed first; with no relay it is refused `relay_unreachable`; a relay
/// that died closes before the header and then refuses; a passed window
/// closes before the header and then admits.
#[tokio::test]
async fn a_trace_connection_is_the_relays_wire_after_open() {
    let dir = crate::test_dir::tempdir().unwrap();
    let file = dir.path().join("trace.ndjson");
    std::fs::write(&file, b"{\"n\":1}\n").unwrap();
    let mut relay = FakeRelay::new(&file);
    relay.start();
    let fake = FakeService::start(&["show"], "idle");
    assert_eq!(
        refused(&fake, Purpose::Trace).await.word,
        word::RELAY_UNREACHABLE
    );
    fake.with(|s| s.relay = Some(relay.socket.clone()));
    let header = async || {
        let mut stream = open(&fake, Purpose::Trace).await.into_inner();
        stream
            .get_mut()
            .write_all(b"{\"offset\":0}\n")
            .await
            .unwrap();
        let mut line = String::new();
        let n = tokio::time::timeout(SHORT, stream.read_line(&mut line))
            .await
            .unwrap()
            .unwrap();
        (n, line)
    };
    let (_, line) = header().await;
    assert!(line.contains("\"header\""), "{line}");
    fake.window_passes_next();
    assert_eq!(header().await.0, 0, "a close before any header byte");
    assert!(
        header().await.1.contains("\"header\""),
        "the next is admitted"
    );
    fake.relay_dies_next();
    assert_eq!(header().await.0, 0, "a close before any header byte");
    assert_eq!(
        refused(&fake, Purpose::Trace).await.word,
        word::RELAY_UNREACHABLE
    );
}

/// **A log connection carries each message as sent**, with its flag;
/// `log_behind` precedes the close; the newest replaces the older; any
/// byte from the connector is malformed.
#[tokio::test]
async fn a_log_connection_carries_its_messages_and_the_newest_replaces_the_older() {
    let fake = FakeService::start(&["show"], "idle");
    let mut older = open(&fake, Purpose::Log).await;
    let next_log = async |c: &mut admin_service::Connection| {
        tokio::time::timeout(SHORT, c.next_log()).await.unwrap()
    };
    let events = [
        LogEvent::Line {
            line: serde_json::json!({"event": "load", "uid": 990}),
            through_service: true,
        },
        LogEvent::Text {
            text: "not json".into(),
            through_service: false,
        },
        LogEvent::Oversized(9000),
        LogEvent::Discontinuity(Discontinuity::Replaced),
    ];
    for event in &events {
        fake.log(event);
        assert_eq!(
            next_log(&mut older).await,
            Ok(Some(LogLine::Log(event.clone())))
        );
    }
    fake.log_raw(b"{\"log\":{\"oversized\":3,\"text\":\"two shapes\"}}\n");
    assert!(matches!(
        next_log(&mut older).await,
        Err(LineError::Malformed(_))
    ));
    let mut newer = open(&fake, Purpose::Log).await;
    assert_eq!(next_log(&mut older).await, Ok(None), "the older is closed");
    fake.log_behind();
    assert!(
        matches!(next_log(&mut newer).await, Ok(Some(LogLine::Refused(r))) if r.word == word::LOG_BEHIND)
    );
    assert_eq!(next_log(&mut newer).await, Ok(None));
    let mut third = open(&fake, Purpose::Log).await;
    fake.log_close();
    assert_eq!(next_log(&mut third).await, Ok(None));
    let mut stream = open(&fake, Purpose::Log).await.into_inner();
    stream.get_mut().write_all(b"x").await.unwrap();
    let mut line = String::new();
    stream.read_line(&mut line).await.unwrap();
    assert!(line.contains(word::ASK_MALFORMED), "{line}");
    assert_eq!(
        fake.with(|s| s.opened.iter().filter(|p| **p == Purpose::Log).count()),
        4
    );
}
