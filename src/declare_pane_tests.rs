//! conforms: web-the-agent-page-sets-what-the-agent-reports-beside-the-declaration
//!
//! **The agent page's Declare pane** (Spec 2.13; the operator's word of
//! 2026-11-13): the declaration saved under the agent's name, the truth line
//! in each of its three states, each reported member's verdict, the parts
//! the agent never reports, the prompt by its length (its digest to an
//! admin alone), the other live rows of the name as far as the reader may
//! see, the declaration screen's link an admin's alone, and the pane failing by
//! itself where its head does not read or a read inside it fails. Each test runs on a
//! store of its own. No name, artifact or digest here is a real deployment's.

use axum::http::StatusCode;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::agents_tests::rig;
use crate::declarations_tests::{posted, save};
use crate::host;
use crate::passkeys_tests::authenticator;
use crate::store::Store;
use crate::surfaces::declare_pane::{DeclarePane, Head, PaneView};

/// The text of `html` with its tags dropped and its spaces collapsed.
fn text(html: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in html.chars() {
        match c {
            '<' => tag = true,
            '>' => {
                tag = false;
                out.push(' ');
            }
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("&#39;", "'")
        .replace("&#34;", "\"")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// The Declare pane of an agent page.
fn pane(page: &str) -> String {
    let at = page
        .find(r#"<section class="pane" id="declaration">"#)
        .expect("a Declare pane");
    let rest = &page[at..];
    rest[..rest.find("</section>").unwrap()].to_owned()
}

/// A paragraph of the pane by its class, its text, or none.
fn said(pane: &str, class: &str) -> Option<String> {
    let at = pane.find(&format!(r#"<p class="{class}">"#))?;
    let p = &pane[at..];
    Some(text(&p[..p.find("</p>").unwrap()]))
}

/// **The pane's parts**: each row's part, what is declared and how it
/// stands against the agent's report, as text.
fn parts(pane: &str) -> Vec<(String, String, String)> {
    let Some(at) = pane.find("<tbody>") else {
        return Vec::new();
    };
    let body = &pane[at..pane.find("</tbody>").unwrap()];
    body.split("<tr>")
        .skip(1)
        .map(|row| {
            let cell = |open: &str, close: &str, from: usize| -> (String, usize) {
                let a = row[from..].find(open).unwrap() + from;
                let b = row[a..].find(close).unwrap() + a;
                (text(&row[a..b]), b + close.len())
            };
            let (part, at) = cell("<th", "</th>", 0);
            let (declared, at) = cell("<td", "</td>", at);
            let (against, _) = cell("<td", "</td>", at);
            (part, declared, against)
        })
        .collect()
}

/// The row of one part: what is declared and how it stands.
fn part(parts: &[(String, String, String)], name: &str) -> (String, String) {
    parts
        .iter()
        .find(|(p, _, _)| p == name)
        .map(|(_, d, a)| (d.clone(), a.clone()))
        .unwrap_or_else(|| panic!("no part {name} in {parts:?}"))
}

/// The page of an agent under a session.
async fn page_of(app: &axum::Router, agent: &str, session: &str) -> String {
    let (status, _, page) = send_as(app, "GET", &format!("/agents/{agent}"), None, session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    page
}

/// **A load the agent reports**, as the listener lands it: the row's
/// tuple, its source and its date.
async fn load(s: &Store, agent: &str, tuple: Value, source: &str) {
    sqlx::query(
        "UPDATE agent SET tuple = $1, tuple_source = $2, tuple_at = now() WHERE agent_id = $3",
    )
    .bind(tuple)
    .bind(source)
    .bind(agent)
    .execute(&s.pool)
    .await
    .unwrap();
}

/// An admin, a person, and one agent named karl that both hold a grant on.
async fn setting() -> Option<(crate::agents_tests::Rig, String, String, String)> {
    let rig = rig().await?;
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (karl, _) = rig.register(&ada, "box-a", "karl").await;
    let (bea_id, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    for person in [&ada_id, &bea_id] {
        let granted = host::grant_add(s, person, "observer", Some(&karl), None).await;
        assert!(granted.ok, "{}", granted.value);
    }
    Some((rig, ada, bea, karl))
}

/// **With no declaration saved under the name, the pane says so**, for
/// every reader, with no parts; an admin alone is offered a new one.
#[tokio::test]
async fn with_no_declaration_saved_the_pane_says_so() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    let p = pane(&page_of(&rig.app, &karl, &bea).await);
    assert_eq!(
        said(&p, "truth").as_deref(),
        Some("No declaration is saved under the name karl."),
        "{p}"
    );
    assert!(parts(&p).is_empty() && said(&p, "saved").is_none(), "{p}");
    assert!(
        !p.contains("/declarations"),
        "a person is offered no screen: {p}"
    );
    let p = pane(&page_of(&rig.app, &karl, &ada).await);
    assert!(
        p.contains(r#"<a href="/declarations/new">Declare one</a>"#),
        "{p}"
    );
}

/// **A saved declaration with no load shows its parts as declared**: the
/// head named by its version, each part the agent reports marked as having
/// no load beside it and each it never reports marked so; the screen's link
/// an admin's alone.
#[tokio::test]
async fn a_saved_declaration_with_no_load_shows_its_parts() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    let first = save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    let head = save(
        &rig.app,
        &ada,
        &format!("/declarations/{first}"),
        posted(&[("sampler.seed", "7")], &[]),
    )
    .await;
    let p = pane(&page_of(&rig.app, &karl, &bea).await);
    let saved = said(&p, "saved").expect("the head named");
    assert!(
        saved.starts_with("The declaration saved under the name karl: version 2, the head"),
        "{saved}"
    );
    assert!(!p.contains("/declarations/"), "a person has no link: {p}");
    assert!(
        said(&p, "truth").is_some_and(|t| t.contains("holds no load")),
        "{p}"
    );
    let rows = parts(&p);
    assert_eq!(
        part(&rows, "SPU box 1, the decoder"),
        (
            "decoder-a".to_owned(),
            "no load to set it beside".to_owned()
        )
    );
    assert_eq!(
        part(&rows, "the store"),
        ("none".to_owned(), "no load to set it beside".to_owned())
    );
    for (name, declared) in [
        ("placement", "a host agent"),
        ("the tunable value seed", "7"),
        ("the gate's tools", "none"),
        ("the prompt", "none"),
    ] {
        assert_eq!(
            part(&rows, name),
            (declared.to_owned(), "not reported by the agent".to_owned()),
            "{name}"
        );
    }
    let (gpu, against) = part(&rows, "SPU box 1, its GPU requirement");
    assert!(gpu.contains("card-0") && against == "not reported by the agent");
    let p = pane(&page_of(&rig.app, &karl, &ada).await);
    assert!(
        p.contains(&format!(r#"href="/declarations/{head}""#)),
        "{p}"
    );
}

/// **A load whose source the row does not name is set beside nothing**: a
/// row written before the tuple named its own source (migration `0012`)
/// holds a tuple and no source, and the pane reads no shape it cannot name,
/// so it says so and compares nothing.
#[tokio::test]
async fn a_load_whose_source_the_row_does_not_name_is_set_beside_nothing() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    sqlx::query(
        "UPDATE agent SET tuple = $1, tuple_source = NULL, tuple_at = now() WHERE agent_id = $2",
    )
    .bind(show_of("decoder-a"))
    .bind(&karl)
    .execute(&rig.store().pool)
    .await
    .unwrap();
    let p = pane(&page_of(&rig.app, &karl, &bea).await);
    assert_eq!(
        said(&p, "truth").as_deref(),
        Some(
            "The register's row holds a load from a source this page does not read, so \
             nothing the agent reports stands beside the declaration."
        ),
        "{p}"
    );
    let rows = parts(&p);
    for name in ["SPU box 1, the decoder", "the store", "the state election"] {
        assert_eq!(part(&rows, name).1, "no load to set it beside", "{name}");
    }
}

/// The base form's document as `show` would report it loaded.
fn show_of(artifact: &str) -> Value {
    json!({
        "session": "s-1", "run": "r-1", "declaration": "d0d0",
        "artifact": artifact,
        "residual_readout": false, "surprisal": false,
        "state_election": {"all-kinds": true, "keys": []},
        "state_store": {"engine": "none"},
        "state_member": false, "composer": {"binary": "w"}
    })
}

/// **A load the agent reports is set beside the head, member by member**:
/// equal, differs or not in this report, by the shape its source names,
/// with the sentence that a difference may only mean an older version was
/// the source; the page never says what the agent was loaded from.
#[tokio::test]
async fn a_load_the_agent_reports_is_set_beside_the_head() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    load(rig.store(), &karl, show_of("decoder-b"), "show").await;
    let page = page_of(&rig.app, &karl, &bea).await;
    let p = pane(&page);
    let truth = said(&p, "truth").unwrap();
    assert!(
        truth.contains("by its answer to show")
            && truth.contains("digest d0d0")
            && truth.contains("a difference may only mean an older version was the source"),
        "{truth}"
    );
    assert!(!page.to_lowercase().contains("loaded from"), "{page}");
    let rows = parts(&p);
    for (name, against) in [
        (
            "SPU box 1, the decoder",
            "differs (declared decoder-a, reported decoder-b)",
        ),
        ("the residual readout election", "equal"),
        ("the surprisal election", "equal"),
        ("the turn context", "not in this report"),
        ("the state election", "equal"),
        ("the store", "equal"),
        ("placement", "not reported by the agent"),
    ] {
        assert_eq!(part(&rows, name).1, against, "{name}");
    }

    // The harness's load event: the turn context is in it, the artifact not.
    load(
        rig.store(),
        &karl,
        json!({
            "residual_readout": false, "surprisal": false, "state_member": false,
            "state_store": {"engine": "none"}, "declaration": "e1e1",
            "composer": {"binary": "w"},
            "tee": {"all_kinds": true, "keys": []},
            "stack": {}, "boundary": "b0", "cause": {"uid": 0},
            "turn_context": "independent"
        }),
        "event",
    )
    .await;
    let p = pane(&page_of(&rig.app, &karl, &bea).await);
    let truth = said(&p, "truth").unwrap();
    assert!(
        truth.contains("by its trace's load event") && truth.contains("digest e1e1"),
        "{truth}"
    );
    let rows = parts(&p);
    assert_eq!(
        part(&rows, "the turn context").1,
        "differs (declared continuous, reported independent)"
    );
    assert_eq!(
        part(&rows, "SPU box 1, the decoder").1,
        "not in this report"
    );
    assert_eq!(part(&rows, "the state election").1, "equal");
}

/// **The prompt is shown by its length, and its digest to an admin alone**:
/// its text is on no agent page, and its digest, which compares with
/// nothing here, would let a reader who may not read the text confirm a
/// guess at it (the Planner's correction of 2026-11-13, on a security
/// review's finding).
#[tokio::test]
async fn the_prompt_is_shown_by_its_length_and_its_digest_to_an_admin_alone() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    // One letter of two bytes, so a length in bytes would read 16.
    let prompt = "Answer as K\u{e4}rl.";
    save(
        &rig.app,
        &ada,
        "/declarations",
        posted(&[("prompt", prompt)], &[]),
    )
    .await;
    let digest: String = Sha256::digest(prompt.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let page = page_of(&rig.app, &karl, &bea).await;
    assert!(!page.contains(prompt), "the prompt's text: {page}");
    assert!(!page.contains(&digest), "the prompt's digest: {page}");
    assert_eq!(
        part(&parts(&pane(&page)), "the prompt"),
        (
            "15 characters".to_owned(),
            "not reported by the agent".to_owned()
        )
    );
    let page = page_of(&rig.app, &karl, &ada).await;
    assert!(!page.contains(prompt), "the prompt's text: {page}");
    assert_eq!(
        part(&parts(&pane(&page)), "the prompt"),
        (
            format!("15 characters, sha256 {digest}"),
            "not reported by the agent".to_owned()
        )
    );
}

/// **The other live rows of the name are listed as far as the reader may
/// see**: every one to an admin, the person's own grants' to a person, a
/// row of another name never, and a retired row to nobody.
#[tokio::test]
async fn the_other_rows_of_the_name_are_listed_as_far_as_the_reader_may_see() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    let s = rig.store();
    let (karl_b, _) = rig.register(&ada, "box-b", "karl").await;
    rig.register(&ada, "box-c", "jane").await;
    let p = pane(&page_of(&rig.app, &karl, &bea).await);
    assert!(said(&p, "others").is_none(), "no grant on box-b: {p}");
    let others = said(&pane(&page_of(&rig.app, &karl, &ada).await), "others")
        .expect("an admin sees the other row");
    assert!(
        others.contains(&format!("box-b/karl ({karl_b})")) && !others.contains("jane"),
        "{others}"
    );
    let bea_id: String = sqlx::query_scalar("SELECT person_id FROM person WHERE name = 'bea'")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    let granted = host::grant_add(s, &bea_id, "observer", Some(&karl_b), None).await;
    assert!(granted.ok, "{}", granted.value);
    assert!(
        said(&pane(&page_of(&rig.app, &karl, &bea).await), "others")
            .is_some_and(|o| o.contains("box-b/karl")),
        "a grant on box-b"
    );
    let (status, _, answer) = send_as(
        &rig.app,
        "POST",
        "/admin/agents/retire",
        Some(form(&[("agent", &karl_b)])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    for session in [&ada, &bea] {
        let p = pane(&page_of(&rig.app, &karl, session).await);
        assert!(said(&p, "others").is_none(), "a retired row: {p}");
    }
}

/// The Declare pane alone, rendered from what its reads returned.
fn render(declaration: &DeclarePane) -> String {
    use askama::Template;
    PaneView { declaration }.render().unwrap()
}

/// **A head that does not read in this build fails in the pane alone**
/// (the Planner's read of 2026-11-13): a row saved by a newer build and read
/// by an older one leaves the agent page standing, Run's panes with it. The
/// pane says the head does not read, names its version, shows no parts, and
/// offers the screen's link to an admin alone.
#[tokio::test]
async fn a_head_that_does_not_read_fails_in_the_pane_alone() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    let s = rig.store();
    sqlx::query(
        "INSERT INTO declaration (declaration_ref, name, body, corpus_commit, \
         parent_declaration_id, lineage_root_id, version, what_moved, \
         the_one_thing_that_moved, author) \
         VALUES ('de-00000000000000aa', 'karl', $1, $2, NULL, NULL, 1, '{}', NULL, NULL)",
    )
    .bind(json!({"a_member_this_build_does_not_read": true}))
    .bind(crate::declaration::render::CORPUS_COMMIT)
    .execute(&s.pool)
    .await
    .unwrap();
    for (session, admin) in [(&bea, false), (&ada, true)] {
        let page = page_of(&rig.app, &karl, session).await;
        assert!(page.contains(r#"id="run""#), "Run's panes stand: {page}");
        let p = pane(&page);
        assert_eq!(
            said(&p, "truth").as_deref(),
            Some(
                "The declaration saved under the name karl, version 1, does not read in this \
                 build, so its parts are not shown."
            ),
            "{p}"
        );
        assert!(parts(&p).is_empty(), "{p}");
        assert_eq!(
            p.contains(r#"href="/declarations/de-00000000000000aa""#),
            admin,
            "the screen's link is an admin's alone: {p}"
        );
    }
}

/// **A read that fails inside the pane is stated there**, never an error
/// for the page: the head lookup, the other rows of the name and the head's
/// author each read as unread and said so in the pane, which shows nothing
/// it did not read; an admin's way back then leads to the declarations,
/// never to a new one.
#[tokio::test]
async fn a_read_that_fails_inside_the_pane_is_stated_there() {
    let Some((rig, ada, _, karl)) = setting().await else {
        return;
    };
    let s = rig.store();
    save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    let row = s.agent(&karl.parse().unwrap()).await.unwrap().unwrap();
    let saved = s.declaration_named("karl").await.unwrap().unwrap();

    let p = render(&DeclarePane::compose(&row, Head::Unread, None, None, true));
    assert_eq!(
        said(&p, "truth").as_deref(),
        Some("Whether a declaration is saved under the name karl could not be read just now."),
        "{p}"
    );
    assert!(
        parts(&p).is_empty() && said(&p, "saved").is_none() && !p.contains("Declare one"),
        "an unread head shows nothing and offers no new one: {p}"
    );
    assert_eq!(
        said(&p, "others").as_deref(),
        Some("The other live register rows of this name could not be read just now."),
        "{p}"
    );

    let p = render(&DeclarePane::compose(
        &row,
        Head::Saved(&saved),
        Some(Vec::new()),
        None,
        true,
    ));
    assert!(
        said(&p, "saved")
            .is_some_and(|t| t.contains("by an author that could not be read just now")),
        "{p}"
    );
    assert!(!parts(&p).is_empty() && said(&p, "others").is_none(), "{p}");

    let back = crate::surfaces::reading::DeclaredLink::of("karl", Head::Unread);
    assert_eq!(back.href, "/declarations");
    assert_eq!(
        back.words,
        "whether a declaration is saved under the name karl could not be read just now: the declarations"
    );
}

/// **A head lookup that fails leaves the agent page standing**: the page
/// answers with Run's panes, the pane says whether a declaration is saved
/// could not be read, and an admin's way back and the rail's Declare lead
/// to the declarations, never to a new one. The lookup is made to fail on
/// this test's own store by renaming a column it reads, which no other
/// read of the page touches.
#[tokio::test]
async fn a_head_lookup_that_fails_leaves_the_agent_page_standing() {
    let Some((rig, ada, _, karl)) = setting().await else {
        return;
    };
    let s = rig.store();
    save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    sqlx::query("ALTER TABLE declaration RENAME COLUMN corpus_commit TO corpus_commit_away")
        .execute(&s.pool)
        .await
        .unwrap();
    let page = page_of(&rig.app, &karl, &ada).await;
    assert!(page.contains(r#"id="run""#), "Run's panes stand: {page}");
    let p = pane(&page);
    assert_eq!(
        said(&p, "truth").as_deref(),
        Some("Whether a declaration is saved under the name karl could not be read just now."),
        "{p}"
    );
    let back = &page[page.find(r#"<p class="back">"#).expect("a way back")..];
    assert!(
        back.starts_with(r#"<p class="back">Back, <span class="stage"><b>1</b> Declare</span>: <a href="/declarations">"#),
        "{back}"
    );
    assert!(
        !page.contains(r#"href="/declarations/new""#),
        "nothing offers a new declaration on a lookup that failed: {page}"
    );
}

/// **A virtual-machine placement names its template, and nothing cloned**:
/// the console is pointed at a machine that exists (the operator's ruling of
/// 2026-11-13) and clones none, so the pane says where the template is
/// named and never that the agent was cloned from it.
#[tokio::test]
async fn a_virtual_machine_placement_names_its_template_and_nothing_cloned() {
    let Some((rig, ada, bea, karl)) = setting().await else {
        return;
    };
    save(
        &rig.app,
        &ada,
        "/declarations",
        posted(
            &[("placement", "virtual-machine"), ("template", "tpl-a")],
            &[],
        ),
    )
    .await;
    let p = pane(&page_of(&rig.app, &karl, &bea).await);
    assert_eq!(
        part(&parts(&p), "placement"),
        (
            "a virtual-machine agent, from the template tpl-a".to_owned(),
            "not reported by the agent".to_owned()
        ),
        "{p}"
    );
}
