//! conforms: web-the-masthead-stands-in-the-paths-order
//! conforms: web-a-pages-way-forward-stands-where-its-condition-holds
//! conforms: web-the-agent-page-leads-on-to-record-and-an-admin-back-to-its-declaration
//!
//! The path in the masthead (Spec 2.8, 2.13 and 6.1; the operator's word of
//! 2026-11-13): the masthead stands in the path's order, each work page
//! with its stage marks and Record last and unmarked, an admin's links
//! absent for a person; each page's way forward stands where its condition
//! holds and nowhere else; and the agent page's rail links Declare and
//! Provision for an admin alone. Each test runs on a store of its own. No
//! name is a real one.

use axum::http::StatusCode;

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::agents_tests::rig;
use crate::declarations_tests::{posted, save};
use crate::host;
use crate::passkeys_tests::authenticator;
use crate::reading_tests::rail;

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
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// **The masthead as a person reads it**, in its order: each link as its
/// address and its text, and who is signed in as `who`.
fn masthead(page: &str) -> Vec<(String, String)> {
    let start = page.find(r#"<nav class="masthead">"#).expect("a masthead");
    let nav = &page[start..start + page[start..].find("</nav>").unwrap()];
    let mut items = Vec::new();
    let mut rest = nav;
    loop {
        let link = rest.find("<a href=\"");
        let who = rest.find(r#"<span class="who">"#);
        match (link, who) {
            (Some(a), w) if w.is_none_or(|w| a < w) => {
                let at = &rest[a + "<a href=\"".len()..];
                let href = at[..at.find('"').unwrap()].to_owned();
                let body = &at[at.find('>').unwrap() + 1..];
                let end = body.find("</a>").unwrap();
                items.push((href, text(&body[..end])));
                rest = &body[end..];
            }
            (_, Some(w)) => {
                items.push(("who".to_owned(), String::new()));
                rest = &rest[w + r#"<span class="who">"#.len()..];
            }
            _ => break,
        }
    }
    items
}

/// The addresses alone, in the masthead's order.
fn order(page: &str) -> Vec<String> {
    masthead(page).into_iter().map(|(href, _)| href).collect()
}

/// **The masthead stands in the path's order**: the work on the left in
/// the order a person does it, each page carrying the stage marks it
/// serves, Record last and unmarked; on the right, after who is signed in,
/// the person and the house. A person who is not an admin sees the same
/// order less the four links an admin alone opens, and a page without a
/// session names Record and sign-in.
#[tokio::test]
async fn the_masthead_stands_in_the_paths_order() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;

    let (status, _, page) = send_as(&rig.app, "GET", "/passkeys", None, &ada).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        masthead(&page),
        [
            ("/declarations", "1 Declare Declarations"),
            ("/admin/agents", "2 Provision Register"),
            ("/agents", "3 Run 4 Observe Agents"),
            ("/record", "Record"),
            ("who", ""),
            ("/admin/persons", "Persons"),
            ("/admin/grants", "Grants"),
            ("/passkeys", "Passkeys"),
        ]
        .map(|(h, t)| (h.to_owned(), t.to_owned())),
        "the admin's masthead: {page}"
    );

    let (status, _, page) = send_as(&rig.app, "GET", "/passkeys", None, &bea).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(
        order(&page),
        ["/agents", "/record", "who", "/passkeys"],
        "a person's masthead is the admin's less what they may not open: {page}"
    );

    let (status, _, page) = send_as(&rig.app, "GET", "/sign-in", None, "no-session").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(order(&page), ["/record", "/sign-in"], "{page}");
}

/// **A saved declaration leads on to registering its name**, and the
/// register's form fills a name of the register's grammar alone. The empty
/// screen leads nowhere, having no name.
#[tokio::test]
async fn a_saved_declaration_leads_on_to_registering_its_name() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let de = save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;

    let (status, _, page) =
        send_as(&rig.app, "GET", &format!("/declarations/{de}"), None, &ada).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let next = &page[page.find(r#"<p class="next">"#).expect("a way forward")..];
    let next = &next[..next.find("</p>").unwrap()];
    assert!(
        next.contains(r#"href="/admin/agents?name=karl""#)
            && text(next).contains("2 Provision")
            && text(next).contains("register an agent named karl"),
        "{next}"
    );
    let (_, _, empty) = send_as(&rig.app, "GET", "/declarations/new", None, &ada).await;
    assert!(!empty.contains(r#"class="next""#), "{empty}");

    for (uri, filled) in [
        ("/admin/agents?name=karl", "karl"),
        ("/admin/agents?name=box-a%2Fkarl", ""),
        ("/admin/agents?name=", ""),
        ("/admin/agents", ""),
    ] {
        let (status, _, page) = send_as(&rig.app, "GET", uri, None, &ada).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {page}");
        assert!(
            page.contains(&format!(
                r#"<input id="name" type="text" name="name" value="{filled}""#
            )),
            "{uri}: the name field holds {filled:?}: {page}"
        );
    }
}

/// The hand-over page's way forward, its text.
fn way_forward(page: &str) -> String {
    let next = &page[page.find(r#"<p class="next">"#).expect("a way forward")..];
    next[..next.find("</p>").unwrap()].to_owned()
}

/// **The hand-over leads on to the agent's page where the admin holds a
/// grant on it, and otherwise says why not**, linking to Grants; both say
/// what is done on the box between the two steps.
#[tokio::test]
async fn the_hand_over_leads_on_to_the_agents_page_or_says_why_not() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (status, _, page) = send_as(
        &rig.app,
        "POST",
        "/admin/agents/register",
        Some(form(&[("box", "box-a"), ("name", "karl")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let karl: String = sqlx::query_scalar("SELECT agent_id FROM agent WHERE name = 'karl'")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    let next = way_forward(&page);
    assert!(
        next.contains(r#"href="/admin/grants""#)
            && text(&next).contains("you hold none")
            && !next.contains("/agents/ag-"),
        "no grant: Grants, and why: {next}"
    );
    assert!(text(&next).contains("install both configs"), "{next}");

    let granted = host::grant_add(s, &ada_id, "observer", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (status, _, page) = send_as(
        &rig.app,
        "POST",
        "/admin/agents/rotate",
        Some(form(&[("agent", &karl)])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let next = way_forward(&page);
    assert!(
        next.contains(&format!(r#"href="/agents/{karl}""#))
            && !next.contains("/admin/grants")
            && text(&next).contains("3 Run"),
        "a grant: the agent's page: {next}"
    );
    assert!(text(&next).contains("install both configs"), "{next}");
}

/// The rail's links: each linked mark as its numeral and name, with its
/// address.
fn rail_links(page: &str) -> Vec<(String, String)> {
    let start = page.find(r#"<nav class="rail""#).expect("a rail");
    let nav = &page[start..start + page[start..].find("</nav>").unwrap()];
    nav.split(r#"<a class="step" href=""#)
        .skip(1)
        .map(|link| {
            let href = link[..link.find('"').unwrap()].to_owned();
            let mark = link.split("<b>").nth(1).unwrap();
            let (number, rest) = mark.split_once("</b> ").unwrap();
            let name = rest.split('<').next().unwrap();
            (format!("{number} {name}"), href)
        })
        .collect()
}

/// A paragraph of the page by its class, its text and its markup, or none.
fn paragraph(page: &str, class: &str) -> Option<String> {
    let at = page.find(&format!(r#"<p class="{class}">"#))?;
    let p = &page[at..];
    Some(p[..p.find("</p>").unwrap()].to_owned())
}

/// **The agent page leads on to Record and, for an admin, back to its
/// declaration**: Record for every reader, saying it is the whole record
/// and not this agent's; for an admin, the declaration saved under the
/// agent's name, its head, or a new one where none is saved, and the
/// rail's Declare and Provision as links. A person who is not an admin
/// sees neither the way back nor a linked mark.
#[tokio::test]
async fn the_agent_page_leads_on_to_record_and_back_to_its_declaration() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (karl, _) = rig.register(&ada, "box-a", "karl").await;
    let (bea_id, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    for person in [&ada_id, &bea_id] {
        let granted = host::grant_add(s, person, "observer", Some(&karl), None).await;
        assert!(granted.ok, "{}", granted.value);
    }
    let page_of = async |session: &str| {
        let (status, _, page) =
            send_as(&rig.app, "GET", &format!("/agents/{karl}"), None, session).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        page
    };

    let page = page_of(&bea).await;
    let onward = paragraph(&page, "next").expect("the way on");
    assert!(
        onward.contains(r#"href="/record""#)
            && text(&onward).contains("not filtered by this agent"),
        "{onward}"
    );
    assert!(paragraph(&page, "back").is_none(), "{page}");
    assert!(rail_links(&page).is_empty(), "{page}");
    assert_eq!(rail(&page).len(), 8, "{page}");

    let page = page_of(&ada).await;
    assert!(paragraph(&page, "next").is_some(), "{page}");
    let back = paragraph(&page, "back").expect("the way back");
    assert!(
        back.contains(r#"href="/declarations/new""#)
            && text(&back).contains("no declaration is saved under the name karl"),
        "{back}"
    );
    assert_eq!(
        rail_links(&page),
        [
            ("1 Declare".to_owned(), "/declarations/new".to_owned()),
            ("2 Provision".to_owned(), "/admin/agents".to_owned()),
        ],
        "{page}"
    );
    assert_eq!(rail(&page).len(), 8, "every stage, linked or not: {page}");

    // Two versions of karl's lineage and another lineage: the head of
    // karl's is the one named.
    let first = save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    let second = save(
        &rig.app,
        &ada,
        &format!("/declarations/{first}"),
        posted(&[("sampler.seed", "7")], &[]),
    )
    .await;
    let _other = save(
        &rig.app,
        &ada,
        "/declarations",
        posted(&[("name", "jane")], &[]),
    )
    .await;
    let page = page_of(&ada).await;
    let back = paragraph(&page, "back").expect("the way back");
    assert!(
        back.contains(&format!(r#"href="/declarations/{second}""#))
            && text(&back).contains("the declaration saved under the name karl, version 2"),
        "{back}"
    );
    assert_eq!(
        rail_links(&page)[0],
        ("1 Declare".to_owned(), format!("/declarations/{second}")),
        "{page}"
    );
}
