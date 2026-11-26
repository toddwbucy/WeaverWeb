//! The loop page (Spec 6.3, `src/surfaces/loop_page.rs`): the first draft
//! pinned byte for byte to today's compiled loop as wires, the sentence
//! that a saved loop is not on the appliance, which loop ran as the row
//! says it, and the page read by every grant holder and by no one else.
//! Each test runs on a store of its own with the verbs' rig: an admin
//! `ada`, the agent `karl`, and `bea` holding `operator` on it. No name is
//! a real one.

use axum::http::StatusCode;
use sha2::{Digest, Sha256};

use crate::admin_tests::{send_as, signed_in};
use crate::host;
use crate::passkeys_tests::authenticator;
use crate::store::loop_text::{Saved, digest};
use crate::store::{AgentId, Store};
use crate::surfaces::loop_page::{
    FIRST_DRAFT, FIRST_DRAFT_SHA256, NO_LOOP_SAVED, NOT_ON_THE_APPLIANCE, ran_said,
};
use crate::verbs_tests::operated;

/// The page's text with every character reference decoded, as a browser
/// reads it: askama writes `<`, `>`, `&`, `"` and `'` as references.
pub(crate) fn decoded(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest.find(';').filter(|e| *e <= 8);
        let named = end.and_then(|e| match &rest[1..e] {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                .ok()
                .and_then(char::from_u32),
            n if n.starts_with('#') => n[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        });
        match (end, named) {
            (Some(e), Some(c)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// **The wire file's text as the browser reads it**: what stands between
/// the textarea's tag and its end, less the one newline a browser drops
/// right after the tag, decoded.
pub(crate) fn textarea(page: &str) -> String {
    let at = page
        .find("<textarea id=\"loop-text\"")
        .expect("the wire file");
    let rest = &page[at..];
    let open = rest.find('>').expect("the tag's end") + 1;
    let close = rest.find("</textarea>").expect("the textarea's end");
    let inner = &rest[open..close];
    decoded(inner.strip_prefix('\n').unwrap_or(inner))
}

/// The open session of `person`, by its row.
pub(crate) async fn session_of(s: &Store, person: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT session_id FROM session WHERE person_id = $1 AND closed_at IS NULL \
         ORDER BY session_id DESC LIMIT 1",
    )
    .bind(person)
    .fetch_one(&s.pool)
    .await
    .expect("the person's session")
}

/// A save through the store, as the page's own write makes it.
pub(crate) async fn saved(s: &Store, person: &str, agent: &AgentId, read: i32, text: &str) {
    let loop_id = s.mint_key("lp").await.unwrap();
    let first = s.loop_save_first(person, &loop_id, text).await.unwrap();
    let session = session_of(s, person).await;
    let done = s
        .save_loop(session, person, agent, read, &loop_id, text, &first)
        .await
        .unwrap();
    assert!(matches!(done, Saved::Done { .. }), "{done:?}");
}

/// **The first draft is today's compiled loop written as wires, byte for
/// byte**: the fixture's sha256 is the figure measured on the agent's
/// side's file, the binary carries the fixture, and a page with no saved
/// loop draws it as the wire file, beside the sentence that it is not
/// saved.
///
/// Perturbation: change one byte of the fixture, and the pin fails
/// (measured 2026-11-26).
#[tokio::test]
async fn the_first_draft_is_todays_loop_as_wires() {
    let on_disk = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/loop-wires-today.py"
    ))
    .expect("the fixture");
    assert_eq!(
        format!("{:x}", Sha256::digest(&on_disk)),
        FIRST_DRAFT_SHA256,
        "the fixture is the agent's side's file"
    );
    assert_eq!(on_disk, FIRST_DRAFT.as_bytes());
    assert_eq!(digest(FIRST_DRAFT), FIRST_DRAFT_SHA256);
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let (status, _, page) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}/loop", o.karl),
        None,
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(textarea(&page), FIRST_DRAFT);
    assert!(decoded(&page).contains(NO_LOOP_SAVED), "{page}");
    assert!(decoded(&page).contains(NOT_ON_THE_APPLIANCE), "{page}");
}

/// **The loop page says a saved loop is not on the appliance, beside the
/// stored digest**: once a version is saved the pane draws it, its byte
/// count and its digest, with the sentence, and the first draft's
/// sentence no more.
///
/// Perturbation: drop the sentence from the saved head's branch of the
/// template, and this test fails (measured 2026-11-26).
#[tokio::test]
async fn the_loop_page_says_a_saved_loop_is_not_on_the_appliance() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let text = format!("{FIRST_DRAFT}# one line more, <kept> & \"quoted\"\n");
    saved(&s, &o.bea, &karl, 0, &text).await;
    let (status, _, page) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}/loop", o.karl),
        None,
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let read = decoded(&page);
    assert_eq!(textarea(&page), text);
    assert!(read.contains("Version 1, saved "), "{read}");
    assert!(
        read.contains(&format!(
            "by bea: {} bytes, sha256 <span class=\"mono\">{}</span>.</p>",
            text.len(),
            digest(&text)
        )),
        "{read}"
    );
    let line = read
        .lines()
        .skip_while(|l| !l.contains("Version 1, saved"))
        .nth(1)
        .unwrap_or_default();
    assert!(
        line.contains(NOT_ON_THE_APPLIANCE),
        "beside the digest: {read}"
    );
    assert!(!read.contains(NO_LOOP_SAVED), "{read}");
}

/// **Every grant holder reads the page, and no one else**: an observer
/// reads it; a person holding no grant on the agent is answered exactly as
/// for an unknown agent, 404; an identity not of its shape is 400.
#[tokio::test]
async fn the_loop_page_is_read_by_grant_holders_alone() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (cay, cay_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cay").await;
    let uri = format!("/agents/{}/loop", o.karl);
    let (none, _, unheld) = send_as(&o.rig.app, "GET", &uri, None, &cay_session).await;
    let (unknown_status, _, unknown) = send_as(
        &o.rig.app,
        "GET",
        "/agents/ag-0000000000000000/loop",
        None,
        &cay_session,
    )
    .await;
    assert_eq!(
        (none, unheld.as_str()),
        (unknown_status, unknown.as_str()),
        "no grant answers as an unknown agent"
    );
    assert_eq!(none, StatusCode::NOT_FOUND);
    let (bad, _, _) = send_as(&o.rig.app, "GET", "/agents/karl/loop", None, &cay_session).await;
    assert_eq!(bad, StatusCode::BAD_REQUEST);
    let granted = host::grant_add(&s, &cay, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, _, page) = send_as(&o.rig.app, "GET", &uri, None, &cay_session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(textarea(&page), FIRST_DRAFT);
}

/// **Which loop ran, said from the row** (Spec 6.3): no load event, a load
/// that named no loop the page reads, the compiled loop, and the wires by
/// digest with the saved version that has it or without one.
#[tokio::test]
async fn which_loop_ran_is_said_from_the_row() {
    assert_eq!(
        ran_said(None, false, None),
        "No load event is recorded for this agent."
    );
    assert_eq!(
        ran_said(None, true, None),
        "The last load named no loop this page reads."
    );
    assert_eq!(
        ran_said(Some("compiled"), true, None),
        "The compiled loop ran at the last load."
    );
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    saved(&s, &o.bea, &karl, 0, FIRST_DRAFT).await;
    let uri = format!("/agents/{}/loop", o.karl);
    let page = |digest: String| {
        let (s, app, session, karl, uri) = (
            s.clone(),
            o.rig.app.clone(),
            o.bea_session.clone(),
            o.karl.clone(),
            uri.clone(),
        );
        async move {
            sqlx::query("UPDATE agent SET loop_ran = $1, loop_ran_at = now() WHERE agent_id = $2")
                .bind(digest)
                .bind(karl)
                .execute(&s.pool)
                .await
                .unwrap();
            decoded(&send_as(&app, "GET", &uri, None, &session).await.2)
        }
    };
    let read = decoded(
        &send_as(&o.rig.app, "GET", &uri, None, &o.bea_session)
            .await
            .2,
    );
    assert!(
        read.contains("No load event is recorded for this agent."),
        "{read}"
    );
    assert!(page(FIRST_DRAFT_SHA256.to_owned()).await.contains(&format!(
        "The wires ran at the last load, sha256 {FIRST_DRAFT_SHA256}, the saved version 1."
    )));
    let other = "0".repeat(64);
    assert!(page(other.clone()).await.contains(&format!(
        "The wires ran at the last load, sha256 {other}, a digest no saved version here has."
    )));
    assert!(
        page("compiled".to_owned())
            .await
            .contains("The compiled loop ran at the last load.")
    );
}

/// **The agent page links its loop page**, under the Declare pane, for
/// every grant holder.
#[tokio::test]
async fn the_agent_page_links_its_loop_page() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let (status, _, page) = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}", o.karl),
        None,
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let declare = page.find("id=\"declaration\"").expect("the Declare pane");
    let link = page
        .find(&format!(
            "<a href=\"/agents/{}/loop\">its loop page</a>",
            o.karl
        ))
        .expect("the link");
    assert!(link > declare, "under the Declare pane");
}

/// A form's value percent-encoded as a browser sends it: every byte but
/// an unreserved one as `%XX`.
pub(crate) fn encoded(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for b in bytes {
        if b.is_ascii_alphanumeric() || b"-._~".contains(b) {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A save's form as the page posts it, its text's line breaks sent as
/// CRLF, as a browser sends a textarea's.
pub(crate) fn save_form(text: &[u8], version: &str) -> String {
    let mut sent = Vec::with_capacity(text.len());
    for b in text {
        if *b == b'\n' {
            sent.extend_from_slice(b"\r\n");
        } else {
            sent.push(*b);
        }
    }
    format!("text={}&version={version}", encoded(&sent))
}

/// The loop records the audit holds: target, action, refusal and outcome.
async fn loop_records(s: &Store) -> Vec<(Option<String>, String, Option<String>, Option<String>)> {
    sqlx::query_as(
        "SELECT target_id, action, refusal, outcome FROM audit WHERE target_kind = 'loop' \
         ORDER BY at, audit_id",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap()
}

/// **A wire file is refused one byte past 262,144, empty, holding a NUL,
/// a carriage return left after the fold (a lone one, or the first of
/// `\r\r\n`), or not UTF-8**, each with its one sentence and no record; a
/// `%` not followed by two hex digits (`%+5`) is not the form; 262,144 bytes
/// save; and **the fixture saved through the form keeps its digest**,
/// though the browser sends its line breaks as CRLF.
///
/// Perturbations: move the bound one byte, and 262,145 bytes save; decode
/// the form lossily, and a byte sequence that is not UTF-8 saves with
/// U+FFFD; drop the hex check, and `%+5` decodes; drop the carriage
/// return's refusal, and a lone one saves (each measured 2026-11-26).
#[tokio::test]
async fn a_wire_file_is_refused_past_its_bound_empty_with_a_nul_or_not_utf8() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let uri = format!("/agents/{}/loop", o.karl);
    let post = |form: String| {
        let (app, uri, session) = (o.rig.app.clone(), uri.clone(), o.bea_session.clone());
        async move { send_as(&app, "POST", &uri, Some(form), &session).await }
    };
    let past = vec![b'#'; 262_145];
    for (form, said) in [
        (save_form(b"", "0"), "The file is empty."),
        (
            save_form(&past, "0"),
            "The file is 262,145 bytes; a wire file is at most 262,144.",
        ),
        (save_form(b"a\0b\n", "0"), "The file holds a NUL byte."),
        (
            save_form(b"a\rb\n", "0"),
            "The file holds a carriage return that is not part of a line break.",
        ),
        (
            save_form(b"a\r\nb\n", "0"),
            "The file holds a carriage return that is not part of a line break.",
        ),
        (
            "text=%+5&version=0".to_owned(),
            "The form is not the loop page's: it carries the text and the version once each.",
        ),
        (save_form(b"a\xffb\n", "0"), "The file is not UTF-8."),
        (
            format!("{}\r", save_form(b"x", "0")),
            "The version is not a whole number of at most 2,147,483,647.",
        ),
        (
            save_form(b"x", "2147483648"),
            "The version is not a whole number of at most 2,147,483,647.",
        ),
        (
            "text=x".to_owned(),
            "The form is not the loop page's: it carries the text and the version once each.",
        ),
    ] {
        let (status, _, answer) = post(form).await;
        assert_eq!((status, answer.as_str()), (StatusCode::BAD_REQUEST, said));
    }
    assert!(
        loop_records(&s).await.is_empty(),
        "a parse refusal writes no record"
    );
    assert!(s.loop_head(&karl).await.unwrap().is_none());
    let (status, headers, answer) = post(save_form(FIRST_DRAFT.as_bytes(), "0")).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(headers[axum::http::header::LOCATION], uri.as_str());
    let head = s.loop_head(&karl).await.unwrap().unwrap();
    assert_eq!(
        head.sha256, FIRST_DRAFT_SHA256,
        "the fixture keeps its digest"
    );
    assert_eq!(head.text, FIRST_DRAFT);
    let most = vec![b'#'; 262_144];
    let (status, _, answer) = post(save_form(&most, "1")).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    let head = s.loop_head(&karl).await.unwrap().unwrap();
    assert_eq!((head.version, head.bytes), (2, 262_144));
}

/// **The request's bound admits the largest file the contract admits, in
/// its longest form**: 262,144 line breaks, each sent as `%0D%0A`, 1,572,864
/// bytes of value, saves; a body past the route's bound is refused 413.
#[tokio::test]
async fn the_largest_file_crosses_the_requests_bound() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let uri = format!("/agents/{}/loop", o.karl);
    let form = save_form(&[b'\n'; 262_144], "0");
    assert_eq!(form.len(), "text=&version=0".len() + 1_572_864);
    let (status, _, answer) = send_as(&o.rig.app, "POST", &uri, Some(form), &o.bea_session).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    let head = s.loop_head(&karl).await.unwrap().unwrap();
    assert_eq!(head.bytes, 262_144);
    assert_eq!(head.text, "\n".repeat(262_144));
    let past = format!(
        "text={}&version=1",
        "a".repeat(crate::surfaces::loop_page::LOOP_FORM_MOST)
    );
    let (status, _, _) = send_as(&o.rig.app, "POST", &uri, Some(past), &o.bea_session).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

/// **Only a person holding `operator` on the agent saves its loop**: an
/// observer is refused 403 and a person holding no grant is answered
/// exactly as for an unknown agent, each with one refusal record naming
/// the digest and the byte count and no first record; the operator's form
/// is on the page and the observer's page has none; a save against a
/// stale version is 409 with its outcome failed.
///
/// Perturbation: drop the page's `operator` check, and the observer's
/// save reaches the store, which refuses it at its own re-check, writing a
/// first record and a failed outcome in place of the one refusal (measured
/// 2026-11-26); the store's re-check is `a_save_after_the_grant_ends_lands_nothing`.
#[tokio::test]
async fn only_an_operator_holder_saves_a_loop() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let (cay, cay_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cay").await;
    let uri = format!("/agents/{}/loop", o.karl);
    let form = save_form(b"# cay's\n", "0");
    let said = crate::store::loop_text::action(&digest("# cay's\n"), 8);
    let (none, _, unheld) =
        send_as(&o.rig.app, "POST", &uri, Some(form.clone()), &cay_session).await;
    let (unknown, _, of_unknown) = send_as(
        &o.rig.app,
        "POST",
        "/agents/ag-0000000000000000/loop",
        Some(form.clone()),
        &cay_session,
    )
    .await;
    assert_eq!((none, unheld.as_str()), (unknown, of_unknown.as_str()));
    assert_eq!(none, StatusCode::NOT_FOUND);
    let granted = host::grant_add(&s, &cay, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, _, answer) =
        send_as(&o.rig.app, "POST", &uri, Some(form.clone()), &cay_session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    let records = loop_records(&s).await;
    assert_eq!(records.len(), 3, "{records:?}");
    for (target, action, refusal, outcome) in &records {
        assert!(target.as_deref().is_some_and(|t| t.starts_with("lp-")));
        assert_eq!(action, &said);
        assert!(outcome.is_none());
        assert!(refusal.is_some(), "{records:?}");
    }
    assert_eq!(records[0].2.as_deref(), Some("no grant on the agent"));
    assert_eq!(
        records[2].2.as_deref(),
        Some("only a person holding operator on the agent saves its loop")
    );
    assert!(s.loop_head(&karl).await.unwrap().is_none());
    let observer = send_as(&o.rig.app, "GET", &uri, None, &cay_session).await.2;
    assert!(
        !observer.contains(&format!("action=\"{uri}\"")),
        "{observer}"
    );
    assert!(observer.contains("readonly"));
    let operator = send_as(&o.rig.app, "GET", &uri, None, &o.bea_session)
        .await
        .2;
    assert!(
        operator.contains(&format!("<form method=\"post\" action=\"{uri}\">")),
        "{operator}"
    );
    assert!(operator.contains("name=\"version\" value=\"0\""));
    let (status, _, _) = send_as(
        &o.rig.app,
        "POST",
        &uri,
        Some(save_form(b"# one\n", "0")),
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, answer) = send_as(
        &o.rig.app,
        "POST",
        &uri,
        Some(save_form(b"# two\n", "0")),
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    let last = loop_records(&s).await.pop().unwrap();
    assert_eq!(last.3.as_deref(), Some("failed"));
    let head = s.loop_head(&karl).await.unwrap().unwrap();
    assert_eq!((head.version, head.text.as_str()), (1, "# one\n"));
}

/// admin's word on `verb` for karl: `exit` 0 an answer, 1 a refusal of
/// `kind`.
fn outcome(verb: &str, exit: i32, kind: &str) -> crate::link::frames::VerbOutcome {
    crate::link::frames::VerbOutcome {
        verb: verb.into(),
        agent: "karl".into(),
        exit_code: Some(exit),
        answer: Some(serde_json::json!({ "kind": kind })),
        raw_stdout: None,
        stderr: None,
    }
}

/// bea's reload, run as its own task.
fn reloaded(
    o: &crate::verbs_tests::Operated,
) -> tokio::task::JoinHandle<(StatusCode, axum::http::HeaderMap, String)> {
    let (app, session, uri) = (
        o.rig.app.clone(),
        o.bea_session.clone(),
        format!("/agents/{}/loop/reload", o.karl),
    );
    tokio::spawn(async move { send_as(&app, "POST", &uri, None, &session).await })
}

/// **A reload asks `load` only after `unload` exits 0** (Spec 6.3): an
/// `unload` answered leads to the `load`, both drawn as admin answered; an
/// `unload` refused ends the reload with no `load` sent, its refusal drawn;
/// a person holding no grant is answered as for an unknown agent and no
/// frame leaves the server.
///
/// Perturbation: ask `load` after any `unload` answer, and the refused
/// `unload` is followed by a `load` frame (measured 2026-11-26).
#[tokio::test]
async fn a_reload_asks_load_only_after_unload_exits_0() {
    use crate::link::frames::FromClient;
    use crate::verbs_tests::{no_frame, take_verb};
    let Some(mut o) = operated(&["show", "unload", "load"]).await else {
        return;
    };
    let page = send_as(
        &o.rig.app,
        "GET",
        &format!("/agents/{}/loop", o.karl),
        None,
        &o.bea_session,
    )
    .await
    .2;
    assert!(
        page.contains(&format!(
            "<form method=\"post\" action=\"/agents/{}/loop/reload\"><button type=\"submit\" class=\"verb\">Reload</button></form>",
            o.karl
        )),
        "offered: {page}"
    );

    let asked = reloaded(&o);
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "unload");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome("unload", 0, "state")),
            error: None,
        })
        .await;
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "load", "the load follows the unload's exit 0");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome("load", 0, "state")),
            error: None,
        })
        .await;
    let (status, _, page) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    crate::surfaces::assert_framed(&page);
    let read = decoded(&page);
    assert!(read.contains("<td>unload</td>"), "{read}");
    assert!(read.contains("<td>load</td>"), "{read}");
    assert!(!read.contains("no load sent"), "{read}");

    let asked = reloaded(&o);
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "unload");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome("unload", 1, "not_loaded")),
            error: None,
        })
        .await;
    no_frame(&mut o.admin_con).await;
    let (status, _, page) = asked.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{page}");
    let read = decoded(&page);
    assert!(
        read.contains("admin's refusal: not_loaded; no load sent"),
        "{read}"
    );
    assert!(!read.contains("<td>load</td>"), "{read}");

    let s = o.rig.store().clone();
    let (_, cay_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cay").await;
    let (status, _, answer) = send_as(
        &o.rig.app,
        "POST",
        &format!("/agents/{}/loop/reload", o.karl),
        None,
        &cay_session,
    )
    .await;
    let (unknown, _, of_unknown) = send_as(
        &o.rig.app,
        "POST",
        "/agents/ag-0000000000000000/loop/reload",
        None,
        &cay_session,
    )
    .await;
    assert_eq!((status, answer), (unknown, of_unknown));
    assert_eq!(status, StatusCode::NOT_FOUND);
    no_frame(&mut o.admin_con).await;
}

/// **The reload pane says why it is not offered**, drawn disabled and
/// never hidden: to an observer, that it is an operator's; to an operator
/// whose connection's ceiling lacks `load`, the ceiling.
#[tokio::test]
async fn the_reload_pane_says_why_it_is_not_offered() {
    use crate::surfaces::loop_page::{RELOAD_NOT_CEILING, RELOAD_NOT_OPERATOR};
    let Some(o) = operated(&["show", "unload"]).await else {
        return;
    };
    let uri = format!("/agents/{}/loop", o.karl);
    let disabled = "<button type=\"submit\" class=\"verb\" disabled>Reload</button>";
    let page = decoded(
        &send_as(&o.rig.app, "GET", &uri, None, &o.bea_session)
            .await
            .2,
    );
    assert!(page.contains(disabled), "{page}");
    assert!(page.contains(RELOAD_NOT_CEILING), "{page}");
    let s = o.rig.store().clone();
    let (cay, cay_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cay").await;
    let granted = host::grant_add(&s, &cay, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let page = decoded(&send_as(&o.rig.app, "GET", &uri, None, &cay_session).await.2);
    assert!(page.contains(disabled), "{page}");
    assert!(page.contains(RELOAD_NOT_OPERATOR), "{page}");
}

/// The agent's alerts as the store holds them, polled until `found` holds
/// of them or the bound passes.
async fn alerts_until(
    s: &Store,
    agent: &AgentId,
    found: impl Fn(&[crate::store::alerts::Alert]) -> bool,
) -> Vec<crate::store::alerts::Alert> {
    let until = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let alerts = s.alerts_on(agent, 50).await.unwrap();
        if found(&alerts) || tokio::time::Instant::now() >= until {
            return alerts;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// **A refused second step never hides the unload that ran** (point 1 of
/// the Planner's round of 2026-11-26): the `unload` answered 0, the
/// `load`'s first record failing in the audit (a trigger in this test's
/// own store), the page draws both steps at the refusal's status, 500; the
/// door raised its alert for the `load`, and **the reload raises no second
/// one**.
///
/// Perturbations: answer a 500 with nothing drawn, and the page is the
/// plain fault; raise the reload's alert whether or not the door raised
/// one, and a second alert stands (each measured 2026-11-26).
#[tokio::test]
async fn a_refused_load_draws_the_unload_that_ran() {
    use crate::link::frames::FromClient;
    use crate::verbs_tests::{no_frame, take_verb};
    let Some(mut o) = operated(&["show", "unload", "load"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    sqlx::raw_sql(
        "CREATE FUNCTION test_the_load_first_fails() RETURNS trigger AS $$ BEGIN \
           RAISE EXCEPTION 'the load''s first record fails, by this test'; END; $$ LANGUAGE plpgsql; \
         CREATE TRIGGER test_the_load_first_fails BEFORE INSERT ON audit FOR EACH ROW \
           WHEN (NEW.action = 'load' AND NEW.refusal IS NULL AND NEW.answers IS NULL) \
           EXECUTE FUNCTION test_the_load_first_fails();",
    )
    .execute(&s.pool)
    .await
    .unwrap();
    let asked = reloaded(&o);
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "unload");
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome("unload", 0, "state")),
            error: None,
        })
        .await;
    no_frame(&mut o.admin_con).await;
    let (status, _, page) = asked.await.unwrap();
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{page}");
    crate::surfaces::assert_framed(&page);
    let read = decoded(&page);
    assert!(
        read.contains("<td>unload</td>") && read.contains("<td>admin's answer</td>"),
        "{read}"
    );
    assert!(read.contains("<td>load</td>"), "{read}");
    assert!(read.contains("not asked:"), "{read}");
    let alerts = alerts_until(&s, &karl, |a| a.iter().any(|a| a.subject == "load")).await;
    assert!(alerts.iter().any(|a| a.subject == "load"), "{alerts:#?}");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let alerts = s.alerts_on(&karl, 50).await.unwrap();
    assert!(
        !alerts.iter().any(|a| a.subject == "reload"),
        "the door raised one, so the reload raises none: {alerts:#?}"
    );
}

/// **A reload left half done raises an alert** (point 2): the `unload`
/// exited 0, then the person's grant ended before the `load` was asked;
/// no `load` frame leaves, the door raises nothing (no authorization
/// stood), and the reload raises its own, naming the reason.
///
/// Perturbation: raise nothing from the reload, and no alert stands
/// (measured 2026-11-26).
#[tokio::test]
async fn a_reload_left_half_done_raises_an_alert() {
    use crate::link::frames::FromClient;
    use crate::verbs_tests::{no_frame, take_verb};
    let Some(mut o) = operated(&["show", "unload", "load"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let karl: AgentId = o.karl.parse().unwrap();
    let asked = reloaded(&o);
    let (id, verb, _) = take_verb(&mut o.admin_con).await;
    assert_eq!(verb, "unload");
    let removed = host::grant_remove(&s, &o.bea, "operator", Some(&o.karl), None).await;
    assert!(removed.ok, "{}", removed.value);
    o.admin_con
        .send(FromClient::Verb {
            id,
            outcome: Some(outcome("unload", 0, "state")),
            error: None,
        })
        .await;
    no_frame(&mut o.admin_con).await;
    let (status, _, _) = asked.await.unwrap();
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "no grant answers as an unknown agent"
    );
    let alerts = alerts_until(&s, &karl, |a| a.iter().any(|a| a.subject == "reload")).await;
    let raised = alerts
        .iter()
        .find(|a| a.subject == "reload")
        .unwrap_or_else(|| panic!("the reload raised no alert: {alerts:#?}"));
    assert!(
        raised
            .summary
            .starts_with("the reload unloaded the agent and the load was not sent: "),
        "{}",
        raised.summary
    );
    assert!(!alerts.iter().any(|a| a.subject == "load"), "{alerts:#?}");
}

/// A reload's audit record as the test reads it: target kind, target,
/// refusal and outcome.
type ReloadRecord = (String, Option<String>, Option<String>, Option<String>);

/// **Only a person holding `operator` reloads** (point 3; decision 6 of
/// the loop page's brief): an observer whose role is authored to carry
/// `unload` and `load` is refused 403 before any frame leaves, with one
/// refusal record naming the agent with the action `loop reload`; a person
/// holding no grant is answered as for an unknown agent, with one record.
///
/// Perturbation: drop the handler's `operator` check, and the observer's
/// reload sends an `unload` frame (measured 2026-11-26).
#[tokio::test]
async fn only_an_operator_holder_reloads() {
    use crate::verbs_tests::no_frame;
    let Some(mut o) = operated(&["show", "unload", "load"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let set = host::role_set(
        &s,
        "observer",
        &["show".to_owned(), "unload".to_owned(), "load".to_owned()],
        None,
    )
    .await;
    assert!(set.ok, "{}", set.value);
    let (cay, cay_session) = signed_in(&o.rig.app, &s, &mut authenticator(), "cay").await;
    let uri = format!("/agents/{}/loop/reload", o.karl);
    let (status, _, _) = send_as(&o.rig.app, "POST", &uri, None, &cay_session).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let granted = host::grant_add(&s, &cay, "observer", Some(&o.karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, _, answer) = send_as(&o.rig.app, "POST", &uri, None, &cay_session).await;
    assert_eq!(
        (status, answer.as_str()),
        (
            StatusCode::FORBIDDEN,
            "only a person holding operator on the agent reloads it"
        )
    );
    no_frame(&mut o.admin_con).await;
    let records: Vec<ReloadRecord> = sqlx::query_as(
        "SELECT target_kind, target_id, refusal, outcome FROM audit WHERE action = 'loop reload' \
         ORDER BY at, audit_id",
    )
    .fetch_all(&s.pool)
    .await
    .unwrap();
    let karl = Some(o.karl.clone());
    assert_eq!(
        records,
        vec![
            (
                "agent".to_owned(),
                karl.clone(),
                Some("no grant on the agent".to_owned()),
                None
            ),
            (
                "agent".to_owned(),
                karl,
                Some("only a person holding operator on the agent reloads it".to_owned()),
                None
            ),
        ]
    );
}

/// **A save naming the integer's top version is stale** (point 8): 409
/// with its outcome failed, never a 500.
#[tokio::test]
async fn a_save_at_the_top_version_is_stale() {
    let Some(o) = operated(&["show"]).await else {
        return;
    };
    let s = o.rig.store().clone();
    let (status, _, answer) = send_as(
        &o.rig.app,
        "POST",
        &format!("/agents/{}/loop", o.karl),
        Some(save_form(b"# top\n", "2147483647")),
        &o.bea_session,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    let last = loop_records(&s).await.pop().unwrap();
    assert_eq!(last.3.as_deref(), Some("failed"));
}
