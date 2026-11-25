//! conforms: web-the-bar-stands-in-two-rows
//! conforms: web-every-page-declares-its-tab-and-sub-tab
//! conforms: web-a-pages-way-forward-stands-where-its-condition-holds
//! conforms: web-the-agent-page-leads-on-to-record-and-an-admin-back-to-its-declaration
//!
//! The bar and the path (Spec 2.8, 2.13 and 6.1; the operator's words of
//! 2026-11-13): the bar stands in two rows, three tabs on top (Agents,
//! Agent Operations, IAM) with the status at the right and the page's tab's
//! sub-tabs beneath, the agents' in the path's order with their stage marks, an admin's sub-tabs absent for
//! a person, and every page declaring its tab and sub-tab; each page's way
//! forward stands where its condition holds and nowhere else; and the agent
//! page's rail links Declare and Provision for an admin alone. Each test runs on a store of its own. No
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

/// **One row of the bar as a person reads it**, in its order: each link as
/// its address, its text and whether it is marked current, and who is
/// signed in as `who`.
type Row = Vec<(String, String, bool)>;

fn row(nav: &str) -> Row {
    let mut items = Vec::new();
    let mut rest = nav;
    loop {
        let link = rest.find("<a href=\"");
        let who = rest.find(r#"<span class="who">"#);
        match (link, who) {
            (Some(a), w) if w.is_none_or(|w| a < w) => {
                let at = &rest[a + "<a href=\"".len()..];
                let href = at[..at.find('"').unwrap()].to_owned();
                let tag = &at[..at.find('>').unwrap()];
                let body = &at[at.find('>').unwrap() + 1..];
                let end = body.find("</a>").unwrap();
                items.push((href, text(&body[..end]), tag.contains(r#"class="here""#)));
                rest = &body[end..];
            }
            (_, Some(w)) => {
                items.push(("who".to_owned(), String::new(), false));
                rest = &rest[w + r#"<span class="who">"#.len()..];
            }
            _ => break,
        }
    }
    items
}

/// The `<nav>` that opens with `open`, none where the page has none.
fn nav<'a>(page: &'a str, open: &str) -> Option<&'a str> {
    let start = page.find(open)?;
    Some(&page[start..start + page[start..].find("</nav>").unwrap()])
}

/// **The bar's two rows**: the top row, and the second where one stands.
fn bar(page: &str) -> (Row, Option<Row>) {
    let top = nav(page, r#"<nav class="masthead">"#).expect("a top row");
    (
        row(top),
        nav(page, r#"<nav class="masthead tabs""#).map(row),
    )
}

/// A row's links, each as its address and its text.
fn links(row: &Row) -> Vec<(String, String)> {
    row.iter().map(|(h, t, _)| (h.clone(), t.clone())).collect()
}

/// The texts a row marks current.
fn current(row: &Row) -> Vec<String> {
    row.iter()
        .filter(|(_, _, here)| *here)
        .map(|(_, t, _)| t.clone())
        .collect()
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    items
        .iter()
        .map(|(h, t)| ((*h).to_owned(), (*t).to_owned()))
        .collect()
}

/// A page of the bar, answered 200, under a session.
async fn page(app: &axum::Router, uri: &str, session: &str) -> String {
    let (status, _, page) = send_as(app, "GET", uri, None, session).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {page}");
    page
}

/// **The top row holds three tabs, and the status at the right** (the
/// operator's words of 2026-11-13): Agents, Agent Operations and IAM, each
/// tab opening the first of its sub-tabs the person may open; then, at the
/// right, who is signed in and the open-alert count beside it, a status
/// and not a tab. A page without a session names Agent Operations and
/// sign-in, as it named Record and sign-in before (The Record itself
/// answers such a reader with the way to sign in), and the sign-in page,
/// of no tab, has no second row.
#[tokio::test]
async fn the_top_row_holds_three_tabs_and_the_status_at_the_right() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;

    let (top, _) = bar(&page(&rig.app, "/passkeys", &ada).await);
    assert_eq!(
        links(&top),
        pairs(&[
            ("/setup", "Agents"),
            ("/record", "Agent Operations"),
            ("/admin/persons", "IAM"),
            ("who", ""),
        ]),
        "the admin's top row"
    );
    let (top, _) = bar(&page(&rig.app, "/passkeys", &bea).await);
    assert_eq!(
        links(&top),
        pairs(&[
            ("/agents", "Agents"),
            ("/record", "Agent Operations"),
            ("/passkeys", "IAM"),
            ("who", ""),
        ]),
        "a person's tabs open the sub-tab they may open"
    );

    // An open alert on an agent the admin holds a grant on: the count
    // stands after who, never among the tabs.
    let (karl, _) = rig.register(&ada, "box-a", "karl").await;
    let granted = host::grant_add(s, &ada_id, "observer", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    s.raise_alert(&crate::store::alerts::Raise {
        agent: karl.parse::<crate::store::AgentId>().unwrap(),
        kind: crate::store::alerts::AlertKind::Failed,
        subject: "load".into(),
        summary: "load failed".into(),
        detail: String::new(),
        first: None,
        options: Vec::new(),
        source: "server",
        once: false,
    })
    .await
    .unwrap();
    let (top, _) = bar(&page(&rig.app, "/record", &ada).await);
    assert_eq!(
        links(&top),
        pairs(&[
            ("/setup", "Agents"),
            ("/record", "Agent Operations"),
            ("/admin/persons", "IAM"),
            ("who", ""),
            ("/agents", "1 open alert"),
        ]),
        "the count is a status at the right"
    );

    let signed_out = page(&rig.app, "/sign-in", "no-session").await;
    let (top, second) = bar(&signed_out);
    assert_eq!(
        links(&top),
        pairs(&[("/record", "Agent Operations"), ("/sign-in", "Sign in")]),
        "{signed_out}"
    );
    assert_eq!(current(&top), ["Sign in"]);
    assert!(
        second.is_none(),
        "the sign-in page is of no tab: {signed_out}"
    );
}

/// **The second row holds the sub-tabs of the page's tab**, the current
/// one marked and the row named by its tab: under Agents, Declarations,
/// Pair a box, Register and Your agents in the path's order with their
/// stage marks;
/// under Agent Operations, The Record alone, nothing being built there yet;
/// under IAM, Persons, Grants, Hugging Face, SSH keys and Passkeys. A person who is not an admin
/// sees the sub-tabs they may open: Your agents, The Record and Passkeys.
#[tokio::test]
async fn the_second_row_holds_the_sub_tabs_of_the_pages_tab() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let agents = pairs(&[
        ("/setup", "1 Declare Declarations"),
        ("/pair", "2 Provision Pair a box"),
        ("/admin/agents", "2 Provision Register"),
        ("/agents", "3 Run 4 Observe Your agents"),
    ]);
    let operations = pairs(&[("/record", "The Record")]);
    let iam = pairs(&[
        ("/admin/persons", "Persons"),
        ("/admin/grants", "Grants"),
        ("/admin/hugging-face", "Hugging Face"),
        ("/admin/ssh-key", "SSH keys"),
        ("/passkeys", "Passkeys"),
    ]);
    for (uri, session, tab, want, here) in [
        (
            "/declarations",
            &ada,
            "Agents",
            agents.clone(),
            "1 Declare Declarations",
        ),
        (
            "/record",
            &ada,
            "Agent Operations",
            operations.clone(),
            "The Record",
        ),
        ("/admin/grants", &ada, "IAM", iam.clone(), "Grants"),
        (
            "/agents",
            &bea,
            "Agents",
            vec![agents[3].clone()],
            "3 Run 4 Observe Your agents",
        ),
        (
            "/record",
            &bea,
            "Agent Operations",
            operations.clone(),
            "The Record",
        ),
        (
            "/admin/hugging-face",
            &ada,
            "IAM",
            iam.clone(),
            "Hugging Face",
        ),
        ("/admin/ssh-key", &ada, "IAM", iam.clone(), "SSH keys"),
        (
            "/setup",
            &ada,
            "Agents",
            agents.clone(),
            "1 Declare Declarations",
        ),
        (
            "/pair",
            &ada,
            "Agents",
            agents.clone(),
            "2 Provision Pair a box",
        ),
        ("/passkeys", &bea, "IAM", vec![iam[4].clone()], "Passkeys"),
    ] {
        let page = page(&rig.app, uri, session).await;
        let (top, second) = bar(&page);
        let second = second.unwrap_or_else(|| panic!("{uri}: a second row: {page}"));
        assert_eq!(current(&top), [tab], "{uri}: the tab");
        assert_eq!(links(&second), want, "{uri}: the sub-tabs");
        assert_eq!(current(&second), [here], "{uri}: the sub-tab");
        let label = &page[page.find(r#"<nav class="masthead tabs""#).unwrap()..];
        assert!(
            label.starts_with(&format!(
                r#"<nav class="masthead tabs" aria-label="{tab}">"#
            )),
            "{uri}: the second row is named by its tab"
        );
    }
    // A page's heading is its sub-tab's name.
    assert!(
        page(&rig.app, "/record", &bea)
            .await
            .contains("<h1>The Record</h1>")
    );
    assert!(
        page(&rig.app, "/agents", &bea)
            .await
            .contains("<h1>Your agents</h1>")
    );
}

/// **Every page declares its tab and sub-tab**: each page an admin opens
/// marks the tab and the sub-tab it belongs to, and every page a surface
/// serves names a sub-tab whose tab the table holds, or is a page without
/// a session.
#[tokio::test]
async fn every_page_declares_its_tab_and_sub_tab() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let de = save(&rig.app, &ada, "/declarations", posted(&[], &[])).await;
    let (status, _, handover) = send_as(
        &rig.app,
        "POST",
        "/admin/agents/register",
        Some(form(&[("box", "box-a"), ("name", "karl")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{handover}");
    let karl: String = sqlx::query_scalar("SELECT agent_id FROM agent WHERE name = 'karl'")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    let granted = host::grant_add(s, &ada_id, "observer", Some(&karl), None).await;
    assert!(granted.ok, "{}", granted.value);
    let (_, _, setup) = crate::setup_tests::begin(&rig.app, &ada, "otto").await;
    let setup = setup.expect("a setup opens");
    crate::setup_tests::paired(s, &setup).await;

    let mut pages = Vec::new();
    for uri in [
        "/declarations",
        "/declarations/new",
        &format!("/declarations/{de}"),
        "/admin/agents",
        "/agents",
        &format!("/agents/{karl}"),
        "/record",
        "/admin/persons",
        "/admin/grants",
        "/admin/hugging-face",
        "/admin/ssh-key",
        "/passkeys",
        "/setup",
        "/pair",
        "/pair/new",
        &format!("/pair/{setup}"),
        &format!("/setup/{setup}/models"),
        &format!("/setup/{setup}/directories"),
        &format!("/setup/{setup}/placement"),
        &format!("/setup/{setup}/confirm"),
        &format!("/setup/{setup}/build"),
    ] {
        pages.push((uri.to_owned(), page(&rig.app, uri, &ada).await));
    }
    pages.insert(4, ("the hand-over".to_owned(), handover));
    let want = [
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("2 Provision Register")),
        ("Agents", Some("2 Provision Register")),
        ("Agents", Some("3 Run 4 Observe Your agents")),
        ("Agents", Some("3 Run 4 Observe Your agents")),
        ("Agent Operations", Some("The Record")),
        ("IAM", Some("Persons")),
        ("IAM", Some("Grants")),
        ("IAM", Some("Hugging Face")),
        ("IAM", Some("SSH keys")),
        ("IAM", Some("Passkeys")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("2 Provision Pair a box")),
        ("Agents", Some("2 Provision Pair a box")),
        ("Agents", Some("2 Provision Pair a box")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("1 Declare Declarations")),
        ("Agents", Some("1 Declare Declarations")),
    ];
    assert_eq!(pages.len(), want.len());
    for ((uri, page), (tab, sub)) in pages.iter().zip(want) {
        let (top, second) = bar(page);
        assert_eq!(current(&top), [tab], "{uri}: the tab");
        assert_eq!(
            second.map(|r| current(&r)),
            sub.map(|s| vec![s.to_owned()]),
            "{uri}: the sub-tab"
        );
    }

    // Every `here` a surface declares names a sub-tab of a tab, or a page
    // without a session.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/surfaces");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        for (at, _) in source.match_indices("here: \"") {
            let rest = &source[at + "here: \"".len()..];
            let here = &rest[..rest.find('"').unwrap()];
            seen += 1;
            assert!(
                !crate::surfaces::tab_of(here).is_empty() || ["sign-in", "enroll"].contains(&here),
                "{}: the page `{here}` names no tab",
                path.display()
            );
        }
    }
    assert!(seen >= 16, "the scan read the surfaces' pages ({seen})");
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

/// **The agent page leads on to The Record and, for an admin, back to its
/// declaration**: The Record for every reader, saying it is the whole record
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
            && text(&onward)
                .replace(" ,", ",")
                .starts_with("Onward: The Record,")
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
