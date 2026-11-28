//! conforms: web-the-bar-stands-in-two-rows
//! conforms: web-every-page-declares-its-tab-and-sub-tab
//! conforms: web-a-pages-way-forward-stands-where-its-condition-holds
//! conforms: web-the-agent-page-leads-on-to-record-and-links-no-retired-screen
//! conforms: web-the-agents-list-is-the-agents-tabs-one-page
//! conforms: web-the-retired-screens-answer-not-found
//!
//! The bar and the path (Spec 2.8, 2.13 and 6.1; the operator's words of
//! 2026-11-13): the bar stands in two rows, three tabs on top (Agents,
//! Agent Operations, IAM) with the status at the right and the page's tab's
//! sub-tabs beneath, the Agents tab's one, an admin's sub-tabs absent for
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
            ("/agents", "Agents"),
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
            ("/agents", "Agents"),
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
/// one marked and the row named by its tab: under Agents, its one sub-tab,
/// the Agents list, marked on every page of the tab, New agent and the
/// declaration among them (the operator's words of 2026-11-27); under
/// Agent Operations, The Record alone, nothing being built there yet; under
/// IAM, Persons, Grants, Register, Hugging Face and Passkeys. A
/// person who is not an admin sees the sub-tabs they may open: Agents, The
/// Record and Passkeys.
#[tokio::test]
async fn the_second_row_holds_the_sub_tabs_of_the_pages_tab() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let agents = pairs(&[("/agents", "Agents")]);
    let operations = pairs(&[("/record", "The Record")]);
    let iam = pairs(&[
        ("/admin/persons", "Persons"),
        ("/admin/grants", "Grants"),
        ("/admin/agents", "Register"),
        ("/admin/hugging-face", "Hugging Face"),
        ("/passkeys", "Passkeys"),
    ]);
    for (uri, session, tab, want, here) in [
        (
            "/record",
            &ada,
            "Agent Operations",
            operations.clone(),
            "The Record",
        ),
        ("/admin/grants", &ada, "IAM", iam.clone(), "Grants"),
        ("/agents", &bea, "Agents", agents.clone(), "Agents"),
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
        ("/agents", &ada, "Agents", agents.clone(), "Agents"),
        ("/pair", &ada, "Agents", agents.clone(), "Agents"),
        ("/admin/agents", &ada, "IAM", iam.clone(), "Register"),
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
        page(&rig.app, "/pair", &ada)
            .await
            .contains("<h1>New agent</h1>")
    );
    assert!(
        page(&rig.app, "/record", &bea)
            .await
            .contains("<h1>The Record</h1>")
    );
    assert!(
        page(&rig.app, "/agents", &bea)
            .await
            .contains("<h1>Agents</h1>")
    );
}

/// **The Agents list is the Agents tab's one page** (Spec 2.8; the
/// operator's words of 2026-11-27: "on the first tab we should have a list
/// of agents and teh create new agent button that is all"). An admin's
/// names every agent and every open setup, by name, each with one plain
/// state word, sorted by name, and holds one button, Create new agent,
/// which opens New agent, and nothing else. A setup's row opens its
/// declaration: its Pair page while it waits for its appliance, its
/// directories page once paired, confirmed or not. An agent no setup names
/// opens its own page under a grant and its row on the register without
/// one. **A load state is said only under a grant.** A person who is not an
/// admin sees the agents they hold a grant on, each opening its own page,
/// and no button. `GET /setup`, Declarations, leads to the list.
///
/// Perturbations: say the load state without a grant, and jane reads
/// loaded to the admin; drop the button's admin condition, and bea's page
/// holds it; open an admin's setup row at its agent page, and mara's row
/// fails; leave the setups in their pairing step out, and otto is missing;
/// list every open setup as a row of its own, paired ones too, and the
/// count of rows fails, since a paired agent is one row; sort the retired
/// ivan before the live one, and the order fails; drop the retired row's
/// note, and it is missing.
#[tokio::test]
async fn the_agents_list_names_each_agent_by_one_word_and_opens_its_declaration() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (ada_id, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (bea_id, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let mut setups = std::collections::HashMap::new();
    for name in ["otto", "karl", "lena", "mara"] {
        let (_, _, setup) = crate::setup_tests::begin(s, &ada, name).await;
        let setup = setup.expect("a setup opens");
        if name != "otto" {
            crate::setup_tests::paired(s, &setup).await;
        }
        setups.insert(name, setup);
    }
    let agent_of = async |name: &str| -> String {
        sqlx::query_scalar("SELECT agent_id FROM agent_setup WHERE setup_id = $1")
            .bind(&setups[name])
            .fetch_one(&s.pool)
            .await
            .unwrap()
    };
    let mara = agent_of("mara").await;
    sqlx::query("UPDATE agent_setup SET turn_context = 'continuous' WHERE setup_id = $1")
        .bind(&setups["lena"])
        .execute(&s.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE agent_setup SET state = 'confirmed' WHERE setup_id = $1")
        .bind(&setups["mara"])
        .execute(&s.pool)
        .await
        .unwrap();
    let (jane, _) = rig.register(&ada, "box-c", "jane").await;
    let (ivan, _) = rig.register(&ada, "box-d", "ivan").await;
    let (status, _, answer) = send_as(
        &rig.app,
        "POST",
        "/admin/agents/retire",
        Some(form(&[("agent", &ivan)])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    // **A name retired and registered again** is two rows of one box and
    // name: the live row first, the retired one saying so and naming its
    // identity beside its label.
    let (ivan_again, _) = rig.register(&ada, "box-d", "ivan").await;
    assert_ne!(ivan_again, ivan);
    for agent in [&jane, &mara] {
        sqlx::query(
            "UPDATE agent SET load_state = 'loaded', load_state_at = now() WHERE agent_id = $1",
        )
        .bind(agent)
        .execute(&s.pool)
        .await
        .unwrap();
    }
    for (person, agent) in [(&ada_id, &mara), (&bea_id, &mara)] {
        let granted = host::grant_add(s, person, "observer", Some(agent), None).await;
        assert!(granted.ok, "{}", granted.value);
    }

    let row = |href: &str, label: &str, state: &str| {
        format!("<td><a href=\"{href}\">{label}</a></td>\n      <td>{state}</td>")
    };
    let retired = format!(
        "<td><a href=\"/admin/agents\">box-d/ivan</a> <span class=\"mono\">retired, {ivan}</span></td>\n      <td>retired</td>"
    );
    let in_order = |page: &str, rows: &[String]| {
        let mut from = 0;
        for want in rows {
            let at = page[from..]
                .find(want.as_str())
                .unwrap_or_else(|| panic!("{want} after {from}: {page}"));
            from += at + want.len();
        }
    };
    let main = |page: &str| page[page.find("<main").unwrap()..].to_owned();

    let list = page(&rig.app, "/agents", &ada).await;
    crate::surfaces::assert_framed(&list);
    in_order(
        &list,
        &[
            row("/admin/agents", "box-d/ivan", "registered"),
            retired.clone(),
            row("/admin/agents", "box-c/jane", "registered"),
            row(
                &format!("/setup/{}/directories", setups["karl"]),
                "box-karl/karl",
                "paired",
            ),
            row(
                &format!("/setup/{}/directories", setups["lena"]),
                "box-lena/lena",
                "declaring",
            ),
            row(
                &format!("/setup/{}/directories", setups["mara"]),
                "box-mara/mara",
                "loaded",
            ),
            row(
                &format!("/pair/{}", setups["otto"]),
                "otto",
                "waiting for its appliance",
            ),
        ],
    );
    let body = main(&list);
    assert_eq!(
        body.matches("<tr>").count(),
        8,
        "seven rows and the head: {body}"
    );
    assert_eq!(
        body.matches(r#"<section class="pane">"#).count(),
        1,
        "{body}"
    );
    assert_eq!(body.matches("<form").count(), 1, "one button: {body}");
    assert!(
        body.contains(r#"<form method="get" action="/pair"><button type="submit">Create new agent</button></form>"#),
        "{body}"
    );

    // Without a grant mara's word comes from its setup, confirmed; with
    // one on jane, jane's row opens its page and says its load state.
    let revoked = host::grant_remove(s, &ada_id, "observer", Some(&mara), None).await;
    assert!(revoked.ok, "{}", revoked.value);
    let granted = host::grant_add(s, &ada_id, "observer", Some(&jane), None).await;
    assert!(granted.ok, "{}", granted.value);
    let list = page(&rig.app, "/agents", &ada).await;
    in_order(
        &list,
        &[
            row(&format!("/agents/{jane}"), "box-c/jane", "loaded"),
            row(
                &format!("/setup/{}/directories", setups["mara"]),
                "box-mara/mara",
                "confirmed",
            ),
        ],
    );

    // A person without the admin grant: the agents they hold a grant on,
    // each opening its own page, and no button.
    let list = page(&rig.app, "/agents", &bea).await;
    let body = main(&list);
    assert!(
        body.contains(&row(&format!("/agents/{mara}"), "box-mara/mara", "loaded")),
        "{body}"
    );
    assert_eq!(
        body.matches("<tr>").count(),
        2,
        "one row and the head: {body}"
    );
    assert!(!body.contains("<form"), "{body}");
    assert!(!body.contains("Create new agent"), "{body}");

    let (status, headers, _) = send_as(&rig.app, "GET", "/setup", None, &ada).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(headers[axum::http::header::LOCATION], "/agents");
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
    let (_, _, setup) = crate::setup_tests::begin(rig.store(), &ada, "otto").await;
    let setup = setup.expect("a setup opens");
    crate::setup_tests::paired(s, &setup).await;

    let mut pages = Vec::new();
    for uri in [
        "/admin/agents",
        "/agents",
        &format!("/agents/{karl}"),
        "/record",
        "/admin/persons",
        "/admin/grants",
        "/admin/hugging-face",
        "/passkeys",
        "/pair",
        &format!("/pair/{setup}"),
        &format!("/setup/{setup}/models"),
        &format!("/setup/{setup}/adapter"),
        &format!("/setup/{setup}/directories"),
        &format!("/setup/{setup}/placement"),
        &format!("/setup/{setup}/confirm"),
        &format!("/setup/{setup}/build"),
    ] {
        pages.push((uri.to_owned(), page(&rig.app, uri, &ada).await));
    }
    pages.insert(1, ("the hand-over".to_owned(), handover));
    let want = [
        ("IAM", Some("Register")),
        ("IAM", Some("Register")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agent Operations", Some("The Record")),
        ("IAM", Some("Persons")),
        ("IAM", Some("Grants")),
        ("IAM", Some("Hugging Face")),
        ("IAM", Some("Passkeys")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
        ("Agents", Some("Agents")),
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
                !crate::surfaces::tab_of(here).is_empty()
                    || ["sign-in", "enroll", "refused"].contains(&here),
                "{}: the page `{here}` names no tab",
                path.display()
            );
        }
    }
    assert!(seen >= 16, "the scan read the surfaces' pages ({seen})");
}

/// **The register's form fills a name of the register's grammar alone**,
/// from its `name` query, and leaves the field empty otherwise.
#[tokio::test]
async fn the_registers_form_fills_a_name_of_its_grammar_alone() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;

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

/// **The retired screens answer not found** (the Planner's brief of
/// 2026-11-28, part 5): the declaration screen's routes and the SSH keys
/// page's, to an admin, each 404 as any path no surface serves.
///
/// Perturbation: serve `/declarations` again, or `/admin/ssh-key`, and its
/// case answers.
#[tokio::test]
async fn the_retired_screens_answer_not_found() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    for (method, uri) in [
        ("GET", "/declarations"),
        ("GET", "/declarations/new"),
        ("GET", "/declarations/de-00000000000000aa"),
        ("POST", "/declarations"),
        ("GET", "/admin/ssh-key"),
        ("POST", "/admin/ssh-key"),
        ("POST", "/admin/ssh-key/sk-00000000000000aa/remove"),
    ] {
        let body = (method == "POST").then(|| form(&[("line", "x")]));
        let (status, _, page) = send_as(&rig.app, method, uri, body, &ada).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}: {page}");
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

/// **The agent page leads on to The Record, and links no retired screen**:
/// The Record for every reader, saying it is the whole record and not this
/// agent's; for an admin, the rail's Provision as a link. No screen writes
/// a declaration now (the Planner's brief of 2026-11-28, part 5), so no way
/// back to one stands and the rail's Declare is never a link, whether a
/// declaration is saved under the name or not. A person who is not an
/// admin sees no linked mark.
#[tokio::test]
async fn the_agent_page_leads_on_to_record_and_links_no_retired_screen() {
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

    let admins = async || {
        let page = page_of(&ada).await;
        assert!(paragraph(&page, "next").is_some(), "{page}");
        assert!(paragraph(&page, "back").is_none(), "{page}");
        assert_eq!(
            rail_links(&page),
            [("2 Provision".to_owned(), "/admin/agents".to_owned())],
            "{page}"
        );
        assert_eq!(rail(&page).len(), 8, "every stage, linked or not: {page}");
        assert!(!page.contains("/declarations"), "{page}");
        page
    };
    admins().await;

    // Two versions of karl's lineage and another lineage: the pane names
    // the head, and still nothing links a retired screen.
    let first = save(rig.store(), &ada, None, posted(&[], &[])).await;
    save(
        rig.store(),
        &ada,
        Some(&first),
        posted(&[("sampler.seed", "7")], &[]),
    )
    .await;
    save(rig.store(), &ada, None, posted(&[("name", "jane")], &[])).await;
    let page = admins().await;
    assert!(
        page.contains("The declaration saved under the name karl: version 2"),
        "{page}"
    );
}
