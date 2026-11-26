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
