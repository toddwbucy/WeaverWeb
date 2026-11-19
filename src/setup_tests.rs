//! conforms: web-every-setup-pane-carries-its-stage-backing-and-status
//! conforms: web-the-start-page-refuses-every-name-the-box-refuses
//! conforms: web-the-pairing-page-warns-where-the-box-cannot-dial-the-link
//!
//! The start page and the setup pages (Spec 6.2, `src/surfaces/setup.rs`,
//! `src/store/setup.rs`). A name outside the start page's rule opens no
//! setup, one case per clause. A person without an admin grant is refused
//! before any setup is looked up. One open setup holds a name. Every pane of
//! every page carries its stage, its backing and its status, a held pane
//! naming what it waits on and holding no input with a value. The pairing
//! page shows the console's link address and port, and warns where that
//! address is loopback and the box a virtual machine, or unspecified. A
//! stale write answers 409, writing nothing. Each DB-backed test runs on a
//! store of its own. No name or address is a real one.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderMap, StatusCode, header};

use crate::admin_tests::{admin, form, send_as, signed_in};
use crate::box_report::{BoxReports, Kept};
use crate::passkeys_tests::{ORIGIN, app, authenticator, passkeys};
use crate::store::Store;
use crate::store::read::tests::{Fresh, fresh_store};
use crate::surfaces::gate;
use crate::surfaces::setup::{Seams, routes};

pub(crate) struct Rig {
    fresh: Fresh,
    _dir: tempfile::TempDir,
    pub(crate) app: axum::Router,
    /// The puller the models page asks, whose cache is `model_cache`.
    pub(crate) puller: crate::hugging_face::pull::Puller,
    /// The real listener and its authority, where the rig serves the
    /// enrollment (Spec 8.2), for a box's commands end to end.
    pub(crate) enrollment: Option<(
        crate::link::listener::Listener,
        Arc<crate::link::authority::Authority>,
    )>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        if let Some((listener, _)) = &self.enrollment {
            listener.stop_now();
        }
    }
}

impl Rig {
    pub(crate) fn store(&self) -> &Store {
        &self.fresh.store
    }
}

/// A rig whose config names the link and the model cache given, its
/// Hugging Face a port nothing answers on.
pub(crate) async fn rig_with(
    link_listen: &str,
    link_address: Option<&str>,
    model_cache: Option<PathBuf>,
) -> Option<Rig> {
    rig_full(
        link_listen,
        link_address,
        model_cache,
        "http://127.0.0.1:9/",
        Arc::new(Kept),
    )
    .await
}

/// A rig whose config names the link and the model cache given, its
/// Hugging Face client at `hugging_face`, its puller over the cache, no
/// account connected, and what the box reports answered by `boxes`.
pub(crate) async fn rig_full(
    link_listen: &str,
    link_address: Option<&str>,
    model_cache: Option<PathBuf>,
    hugging_face: &str,
    boxes: Arc<dyn BoxReports>,
) -> Option<Rig> {
    rig_full_with(
        Some((link_listen, link_address)),
        model_cache,
        hugging_face,
        boxes,
    )
    .await
}

/// **The rig, with the real listener where `link` is none**: an authority
/// in the rig's directory and the listener on a port of its own, which the
/// setup pages' enrollment seam carries, so a box's commands run against it.
pub(crate) async fn rig_full_with(
    link: Option<(&str, Option<&str>)>,
    model_cache: Option<PathBuf>,
    hugging_face: &str,
    boxes: Arc<dyn BoxReports>,
) -> Option<Rig> {
    let fresh = fresh_store().await?;
    let dir = crate::test_dir::tempdir().unwrap();
    let enrollment = match link {
        Some(_) => None,
        None => {
            let authority = Arc::new(
                crate::link::authority::Authority::init(
                    &dir.path().join("authority"),
                    "weaver-web",
                    &[],
                )
                .unwrap(),
            );
            let listener = crate::link::listener::Listener::start_with(
                fresh.store.clone(),
                &authority,
                "127.0.0.1:0",
                Duration::from_secs(4),
                Some(dir.path().join("operations.log")),
            )
            .await
            .unwrap();
            Some((listener, authority))
        }
    };
    let mut cfg = crate::agents_tests::server_config(
        dir.path(),
        match &enrollment {
            Some((listener, _)) => listener.address(),
            None => "127.0.0.1:1".parse().unwrap(),
        },
    );
    if let Some((link_listen, link_address)) = link {
        cfg.link_listen = link_listen.to_owned();
        cfg.link_address = link_address.map(str::to_owned);
    }
    cfg.model_cache = model_cache.clone();
    let client = crate::hugging_face::Client::with_base(hugging_face).unwrap();
    let cache = model_cache.map(|root| crate::hugging_face::cache::Cache::at(&root).unwrap());
    let puller = crate::hugging_face::pull::Puller::start(
        fresh.store.clone(),
        client.clone(),
        cache,
        dir.path().join("authority"),
        crate::hugging_face::pull::Settings::default(),
    );
    let policy = gate::Policy {
        origin: Some(ORIGIN.to_owned()),
        idle: Duration::from_secs(3600),
        absolute: Duration::from_secs(12 * 3600),
        write_bound: Duration::from_secs(60),
    };
    let surface = gate::guard(
        crate::surfaces::script_policy(routes(Seams {
            policy: policy.clone(),
            cfg: Arc::new(cfg),
            client,
            puller: puller.clone(),
            authority_dir: dir.path().join("authority"),
            headers: crate::hugging_face::headers::Headers::new(),
            boxes,
            enrollment: enrollment.as_ref().map(|(listener, authority)| {
                crate::surfaces::setup::Enrollment {
                    listener: listener.clone(),
                    authority: authority.clone(),
                }
            }),
        }))
        .with_state(fresh.store.clone()),
        policy,
    );
    let app = app(&fresh.store, Some(passkeys())).merge(surface);
    Some(Rig {
        fresh,
        _dir: dir,
        app,
        puller,
        enrollment,
    })
}

pub(crate) async fn rig() -> Option<Rig> {
    rig_with("127.0.0.1:8081", Some("console.test:7443"), None).await
}

/// A setup begun under `session`: the answer, and the setup's identity
/// where it landed.
pub(crate) async fn begin(
    app: &axum::Router,
    session: &str,
    name: &str,
) -> (StatusCode, String, Option<String>) {
    let (status, headers, body) = send_as(
        app,
        "POST",
        "/setup",
        Some(form(&[("name", name)])),
        session,
    )
    .await;
    (status, body, setup_of(&headers))
}

/// The setup a write's redirect names.
fn setup_of(headers: &HeaderMap) -> Option<String> {
    let location = headers.get(header::LOCATION)?.to_str().ok()?;
    let rest = location.strip_prefix("/setup/")?;
    Some(rest[..rest.find('/')?].to_owned())
}

pub(crate) async fn setups(s: &Store) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM agent_setup")
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The audit's records about setups for `action`: first records, `ok`
/// outcomes, `failed` outcomes and refusals.
pub(crate) async fn audited(s: &Store, action: &str) -> (i64, i64, i64, i64) {
    let count = |clause: &'static str| {
        let query = format!(
            "SELECT count(*) FROM audit WHERE target_kind = 'setup' \
             AND action = $1 AND {clause}"
        );
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
                .bind(action)
                .fetch_one(&s.pool)
                .await
                .unwrap()
        }
    };
    (
        count("answers IS NULL AND refusal IS NULL").await,
        count("outcome = 'ok'").await,
        count("outcome = 'failed'").await,
        count("refusal IS NOT NULL").await,
    )
}

/// A page answered 200 under a session, and framed.
pub(crate) async fn page(app: &axum::Router, uri: &str, session: &str) -> String {
    let (status, _, page) = send_as(app, "GET", uri, None, session).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {page}");
    crate::surfaces::assert_framed(&page);
    page
}

/// Each `<section class="pane">` of a page, whole.
pub(crate) fn panes(page: &str) -> Vec<&str> {
    page.match_indices(r#"<section class="pane">"#)
        .map(|(at, _)| {
            let rest = &page[at..];
            &rest[..rest.find("</section>").expect("a pane closes") + "</section>".len()]
        })
        .collect()
}

/// The text between `open` and `close` in `s`, the first such.
pub(crate) fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = s.find(open)? + open.len();
    Some(&s[start..start + s[start..].find(close)?])
}

/// Every tag of `kind` in `html`, whole.
fn tags<'a>(html: &'a str, kind: &str) -> Vec<&'a str> {
    html.match_indices(&format!("<{kind}"))
        .map(|(at, _)| &html[at..at + html[at..].find('>').unwrap() + 1])
        .collect()
}

/// **The walk**: every pane of `page` carries the stage mark `1 Declare`,
/// a backing (`built`, `partly` or `unbuilt`) and a status word in a status
/// mark (`ok`, `hold` or `fail`) in its header, and a held pane says in its
/// footer what it waits on and holds no input with a value, no checked box
/// and no selected option. Each pane that fails, named by its heading and
/// its fault.
pub(crate) fn walk(page: &str) -> Vec<String> {
    let mut faults = Vec::new();
    let all = panes(page);
    if all.is_empty() {
        faults.push("the page holds no pane".to_owned());
    }
    for pane in all {
        let header = between(pane, "<header>", "</header>").unwrap_or("");
        let name = between(header, "<h2>", "</h2>").unwrap_or("a pane without a heading");
        let mut fault = |why: &str| faults.push(format!("{name}: {why}"));
        if !header.contains(r#"<span class="stage"><b>1</b> Declare</span>"#) {
            fault("no stage mark 1 Declare");
        }
        if !["built", "partly", "unbuilt"]
            .iter()
            .any(|b| header.contains(&format!(r#"<span class="backing {b}">{b}</span>"#)))
        {
            fault("no backing");
        }
        let status = ["ok", "hold", "fail"].into_iter().find(|s| {
            between(header, &format!(r#"<span class="status {s}">"#), "</span>")
                .is_some_and(|word| !word.trim().is_empty())
        });
        match status {
            None => fault("no status word"),
            Some("hold") => {
                if !between(pane, "<footer>", "</footer>").is_some_and(|f| f.contains("Waits on")) {
                    fault("held, and its footer names nothing it waits on");
                }
                for input in tags(pane, "input") {
                    if between(input, r#"value=""#, "\"").is_some_and(|v| !v.is_empty()) {
                        fault("held, and an input holds a value");
                    }
                    if input.contains(" checked") {
                        fault("held, and a box is checked");
                    }
                }
                if tags(pane, "option").iter().any(|o| o.contains(" selected")) {
                    fault("held, and an option is selected");
                }
                // Each text area on its own: its closing tag right after
                // its opening tag, so an empty one masks no other.
                let filled = pane.match_indices("<textarea").any(|(at, _)| {
                    let rest = &pane[at..];
                    let open = rest.find('>').map_or(rest.len(), |end| end + 1);
                    !rest[open..].starts_with("</textarea>")
                });
                if filled {
                    fault("held, and a text area holds a value");
                }
            }
            Some(_) => {}
        }
    }
    faults
}

/// **The walk names what it is shown**: a pane whose status mark is
/// dropped, a held pane whose input or any one text area holds a value, and
/// a held pane whose footer names nothing it waits on, each named by its
/// heading. The
/// perturbations of the claim, run on every build.
#[test]
fn the_walk_names_a_pane_without_a_status_or_with_a_filled_input() {
    let held = r#"<section class="pane"><header><span class="stage"><b>1</b> Declare</span><h2>Held</h2><span class="backing unbuilt">unbuilt</span><span class="status hold">hold</span></header><div class="body"><p>x</p></div><footer>Waits on the box.</footer></section>"#;
    let live = r#"<section class="pane"><header><span class="stage"><b>1</b> Declare</span><h2>Live</h2><span class="backing built">built</span><span class="status ok">ok</span></header><div class="body"><input name="a" value="b"></div></section>"#;
    assert!(walk(&format!("{held}{live}")).is_empty());
    let unmarked = live.replace(r#"<span class="status ok">ok</span>"#, "");
    assert_eq!(walk(&unmarked), ["Live: no status word"]);
    let filled = held.replace("<p>x</p>", r#"<input name="n" value="ada">"#);
    assert_eq!(walk(&filled), ["Held: held, and an input holds a value"]);
    let checked = held.replace("<p>x</p>", r#"<input type="checkbox" name="n" checked>"#);
    assert_eq!(walk(&checked), ["Held: held, and a box is checked"]);
    // Each text area on its own: an empty one masks no other that holds a
    // value (CodeRabbit on #129).
    let empty = held.replace("<p>x</p>", r#"<textarea name="a"></textarea>"#);
    assert!(walk(&empty).is_empty(), "{:?}", walk(&empty));
    let mixed = held.replace(
        "<p>x</p>",
        r#"<textarea name="a"></textarea><textarea name="b">ada</textarea>"#,
    );
    assert_eq!(walk(&mixed), ["Held: held, and a text area holds a value"]);
    let silent = held.replace("Waits on the box.", "Drawn.");
    assert_eq!(
        walk(&silent),
        ["Held: held, and its footer names nothing it waits on"]
    );
    let unstaged = live.replace(r#"<span class="stage"><b>1</b> Declare</span>"#, "");
    assert_eq!(walk(&unstaged), ["Live: no stage mark 1 Declare"]);
    let unbacked = live.replace(r#"<span class="backing built">built</span>"#, "");
    assert_eq!(walk(&unbacked), ["Live: no backing"]);
}

/// **A name outside the start page's rule opens no setup**, one case per
/// clause, each refused at 400 before any record: an uppercase letter, a
/// `-`, a `_`, a leading digit, one character, seventeen characters, and
/// each name `weaver-admin-Spec` section 4 reserves whole. A name of
/// sixteen characters opens one.
#[tokio::test]
async fn a_name_outside_the_rule_is_refused_opening_no_setup() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    for (name, why) in [
        ("Karl", "begins with a lowercase letter"),
        ("ka-rl", "lowercase letters and digits alone"),
        ("ka_rl", "lowercase letters and digits alone"),
        ("1karl", "begins with a lowercase letter"),
        ("k", "2 characters at least"),
        ("abcdefghijklmnopq", "16 characters at most"),
        ("gatecon", "a box's own accounts"),
        ("admincon", "a box's own accounts"),
    ] {
        let (status, body, setup) = begin(&rig.app, &ada, name).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{name}: {body}");
        assert!(body.contains(why), "{name}: {body}");
        assert_eq!(setup, None, "{name}");
        assert_eq!(setups(s).await, 0, "{name} opens no setup");
        assert_eq!(
            audited(s, "setup begin").await,
            (0, 0, 0, 0),
            "{name} is recorded nowhere"
        );
    }
    let (status, body, setup) = begin(&rig.app, &ada, "abcdefghijklmnop").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let setup = setup.expect("the redirect names the setup");
    assert!(crate::store::identity::is_setup_id(&setup), "{setup}");
    assert_eq!(setups(s).await, 1);
    assert_eq!(audited(s, "setup begin").await, (1, 1, 0, 0));
    let landed = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!(
        (landed.name.as_str(), landed.state.as_str(), landed.version),
        ("abcdefghijklmnop", "open", 1)
    );
    assert!(!landed.virtual_machine, "an unchecked box says host");
}

/// **A console that serves no pairing says so past the admin gate** (Spec
/// 8.2): at the code's entry a person without the admin grant meets the
/// gate's refusal, and an admin the 503.
///
/// Perturbation: answer the 503 before the gate, and a person without the
/// grant learns what the console serves.
#[cfg(feature = "enrollment")]
#[tokio::test]
async fn a_console_serving_no_pairing_says_so_past_the_admin_gate() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let entry = form(&[("compared", "on"), ("code", "HWX5R 0F74A")]);
    let uri = format!("/setup/{setup}/pair");
    let (status, _, body) = send_as(&rig.app, "POST", &uri, Some(entry.clone()), &bea).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body, crate::surfaces::admin::NOT_AN_ADMIN);
    let (status, _, body) = send_as(&rig.app, "POST", &uri, Some(entry), &ada).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body, "this console serves no pairing");
}

/// **A person without an admin grant is refused before any setup is looked
/// up**: a page of a setup that stands and of one that does not answer
/// alike, and a write to either is one refusal record, writing nothing.
#[tokio::test]
async fn a_person_without_an_admin_grant_is_refused_before_any_setup_is_looked_up() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, bea) = signed_in(&rig.app, s, &mut authenticator(), "bea").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let unknown = "st-0123456789abcdef";

    for uri in [
        "/setup".to_owned(),
        format!("/setup/{setup}"),
        format!("/setup/{setup}/pairing"),
        format!("/setup/{setup}/models"),
        format!("/setup/{setup}/drives"),
        format!("/setup/{setup}/placement"),
        format!("/setup/{setup}/confirm"),
        format!("/setup/{setup}/build"),
        format!("/setup/{unknown}/pairing"),
        format!("/setup/{unknown}/models"),
        format!("/setup/{unknown}/drives"),
        format!("/setup/{unknown}/placement"),
        format!("/setup/{unknown}/confirm"),
    ] {
        let (status, _, body) = send_as(&rig.app, "GET", &uri, None, &bea).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {body}");
        assert_eq!(body, crate::surfaces::admin::NOT_AN_ADMIN, "{uri}");
    }
    for target in [setup.as_str(), unknown] {
        for (page, fields) in [
            ("pairing", vec![("version", "1"), ("virtual_machine", "on")]),
            (
                "placement",
                vec![("version", "1"), ("turn_context", "continuous")],
            ),
            ("confirm", vec![("version", "1")]),
            ("models/drop", vec![("version", "1")]),
        ] {
            let (status, _, body) = send_as(
                &rig.app,
                "POST",
                &format!("/setup/{target}/{page}"),
                Some(form(&fields)),
                &bea,
            )
            .await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{target} {page}: {body}");
            assert_eq!(body, crate::surfaces::admin::NOT_AN_ADMIN, "{target}");
        }
    }
    let (status, body, made) = begin(&rig.app, &bea, "otto").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(made, None);
    assert_eq!(audited(s, "setup pairing").await, (0, 0, 0, 2));
    assert_eq!(audited(s, "setup placement").await, (0, 0, 0, 2));
    assert_eq!(audited(s, "setup confirm").await, (0, 0, 0, 2));
    assert_eq!(audited(s, "setup drop").await, (0, 0, 0, 2));
    assert_eq!(audited(s, "setup begin").await, (1, 1, 0, 1));
    assert_eq!(setups(s).await, 1, "bea's begin opens no setup");
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert!(
        !kept.virtual_machine && kept.version == 1,
        "nothing written"
    );
}

/// **One open setup holds a name at a time**: a second begin of the name
/// is refused at 409, naming it, its first record's outcome failed, and no
/// second row stands.
#[tokio::test]
async fn a_second_open_setup_of_one_name_is_refused() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (status, _, first) = begin(&rig.app, &ada, "karl").await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, body, second) = begin(&rig.app, &ada, "karl").await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("an open setup holds the name karl"), "{body}");
    assert_eq!(second, None);
    assert_eq!(setups(s).await, 1);
    assert_eq!(audited(s, "setup begin").await, (2, 1, 1, 0));
    let (status, _, other) = begin(&rig.app, &ada, "otto").await;
    assert_eq!(status, StatusCode::SEE_OTHER, "another name opens its own");
    assert_ne!(other, first);
    // The start page lists both, each a link to its next page.
    let start = page(&rig.app, "/setup", &ada).await;
    for (name, setup) in [("karl", first.unwrap()), ("otto", other.unwrap())] {
        assert!(
            start.contains(&format!(r#"<a href="/setup/{setup}/pairing">{name}</a>"#)),
            "{start}"
        );
    }
}

/// **Every pane of every page carries its stage, its backing and its
/// status**, and a held pane names what it waits on with no input holding
/// a value, over every page this tree serves in each state it draws: the
/// start page, the pairing page with the box a host and a virtual machine
/// at a loopback address and with the console at an unspecified one, the
/// drives page with no key and then with one, the placement and confirm
/// pages with nothing read, and the build page.
#[tokio::test]
async fn every_pane_carries_its_stage_backing_and_status() {
    let cache = crate::test_dir::tempdir().unwrap();
    let Some(rig) = rig_with("127.0.0.1:8081", None, Some(cache.path().to_path_buf())).await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let mut walked = vec![("/setup".to_owned(), page(&rig.app, "/setup", &ada).await)];
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    for p in [
        "",
        "/pairing",
        "/models",
        "/drives",
        "/placement",
        "/confirm",
        "/build",
    ] {
        let uri = format!("/setup/{setup}{p}");
        walked.push((uri.clone(), page(&rig.app, &uri, &ada).await));
    }
    let (status, _, _) = send_as(
        &rig.app,
        "POST",
        &format!("/setup/{setup}/pairing"),
        Some(form(&[("version", "1"), ("virtual_machine", "on")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let warned = page(&rig.app, &format!("/setup/{setup}/pairing"), &ada).await;
    assert!(warned.contains("a loopback address"), "{warned}");
    walked.push(("the pairing page, warned".to_owned(), warned));
    let Some(every) = rig_with("0.0.0.0:8081", None, None).await else {
        return;
    };
    let (_, bea) = admin(&every.app, every.store(), &mut authenticator(), "bea").await;
    let (_, _, other) = begin(&every.app, &bea, "otto").await;
    let warned = page(
        &every.app,
        &format!("/setup/{}/pairing", other.unwrap()),
        &bea,
    )
    .await;
    assert!(warned.contains("an unspecified address"), "{warned}");
    walked.push(("the pairing page, unspecified".to_owned(), warned));
    let key = crate::ssh_key_tests::line("ssh-ed25519", 1);
    let (status, _, body) = send_as(
        &rig.app,
        "POST",
        "/admin/ssh-key",
        Some(form(&[("line", &key)])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    walked.push((
        "the drives page, a key held".to_owned(),
        page(&rig.app, &format!("/setup/{setup}/drives"), &ada).await,
    ));

    for (uri, page) in &walked {
        let faults = walk(page);
        assert!(faults.is_empty(), "{uri}: {faults:?}\n{page}");
    }
    // Every page of a setup carries the rail of its six pages in order,
    // the current one marked, each served.
    let pairing = &walked[2].1;
    let rail = between(pairing, r#"<nav class="rail""#, "</nav>").unwrap();
    let order: Vec<usize> = [
        "Pairing",
        "Models",
        "Drives",
        "Placement",
        "Confirm",
        "Build",
    ]
    .iter()
    .map(|t| {
        rail.find(&format!("</span> {t}"))
            .unwrap_or_else(|| panic!("{t}: {rail}"))
    })
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{rail}");
    assert!(
        rail.contains(&format!(
            r#"<a class="step here" href="/setup/{setup}/pairing">"#
        )),
        "{rail}"
    );
    assert_eq!(
        rail.matches(r#"<span class="backing unbuilt">"#).count(),
        0,
        "{rail}"
    );
    for (at, next) in [
        (2, "models\">Models"),
        (3, "drives\">Drives"),
        (4, "placement\">Placement"),
        (5, "confirm\">Confirm"),
    ] {
        assert!(
            walked[at]
                .1
                .contains(&format!(r#"Next: <a href="/setup/{setup}/{next}</a>"#)),
            "{}: {}",
            walked[at].0,
            walked[at].1
        );
    }
    // Confirm's way forward stands only once Confirm does.
    assert!(!walked[6].1.contains("Next:"), "{}", walked[6].1);
    // The drives page names the model cache where it is set, and the key
    // by its fingerprint, with the way to change it.
    let drives = &walked[walked.len() - 1].1;
    assert!(
        drives.contains(&cache.path().display().to_string()),
        "{drives}"
    );
    assert!(drives.contains(
        r#"<a href="/admin/ssh-key" target="_blank" rel="noopener">Change the keys</a>, in a new tab"#
    ));
    // The build page links the declaration screen as built.
    assert!(
        walked[7]
            .1
            .contains(r#"<a href="/declarations">the declaration screen</a>"#)
    );
}

/// **The pairing page shows the console's link address and its port**:
/// `link_address` where it is set, else `link_listen`'s, in the pane titled
/// Weaver Web's address, which states that the address is Weaver Web's own,
/// for the box to dial, and never the box's (the operator's walk of
/// 2026-11-19).
#[tokio::test]
async fn the_pairing_page_shows_link_address_and_its_port_else_link_listens() {
    for (link_listen, link_address, address, port, not) in [
        (
            "127.0.0.1:8081",
            Some("console.test:7443"),
            "console.test",
            "7443",
            "8081",
        ),
        (
            "192.0.2.10:9443",
            None,
            "192.0.2.10",
            "9443",
            "console.test",
        ),
    ] {
        let Some(rig) = rig_with(link_listen, link_address, None).await else {
            return;
        };
        let s = rig.store();
        let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
        let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
        let pairing = page(
            &rig.app,
            &format!("/setup/{}/pairing", setup.unwrap()),
            &ada,
        )
        .await;
        assert!(
            pairing.contains(&format!(
                r#"<tr><th>Address</th><td class="mono">{address}</td></tr>"#
            )),
            "{pairing}"
        );
        assert!(
            pairing.contains(&format!(
                r#"<tr><th>Port</th><td class="mono">{port}</td></tr>"#
            )),
            "{pairing}"
        );
        assert!(!pairing.contains(not), "{not}: {pairing}");
        assert!(
            pairing.contains("<h2>Weaver Web's address</h2>"),
            "{pairing}"
        );
        assert!(
            pairing.contains("This is Weaver Web's own address, for the box to dial. It is never the box's address"),
            "{pairing}"
        );
        assert!(
            pairing.contains("<footer>Read from Weaver Web's own config. The box's setup asks for this address and port, and the box must reach them.</footer>"),
            "{pairing}"
        );
    }
}

/// **The loopback warning shows where the console's link address is a
/// loopback address and the checkbox says virtual machine**, and not
/// otherwise: not for a host at loopback, and not for a virtual machine at
/// an address it reaches.
#[tokio::test]
async fn the_loopback_warning_shows_for_a_virtual_machine_alone() {
    for (link_address, loopback) in [
        ("127.0.0.1:8081", true),
        ("localhost:8081", true),
        ("[::1]:8081", true),
        ("console.test:7443", false),
    ] {
        let Some(rig) = rig_with("127.0.0.1:8081", Some(link_address), None).await else {
            return;
        };
        let s = rig.store();
        let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
        let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
        let setup = setup.unwrap();
        let uri = format!("/setup/{setup}/pairing");
        let host = page(&rig.app, &uri, &ada).await;
        assert!(
            !host.contains("a loopback address"),
            "{link_address}, host: {host}"
        );
        assert!(
            !host.contains(" checked"),
            "a new setup's box is drawn unchecked"
        );
        let (status, _, _) = send_as(
            &rig.app,
            "POST",
            &uri,
            Some(form(&[("version", "1"), ("virtual_machine", "on")])),
            &ada,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let vm = page(&rig.app, &uri, &ada).await;
        assert!(vm.contains(r#"value="on" checked"#), "{vm}");
        assert_eq!(
            vm.contains("a loopback address"),
            loopback,
            "{link_address}, virtual machine: {vm}"
        );
        // Unchecked again, the box is a host and the warning goes.
        let (status, _, _) = send_as(
            &rig.app,
            "POST",
            &uri,
            Some(form(&[("version", "2")])),
            &ada,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let again = page(&rig.app, &uri, &ada).await;
        assert!(
            !again.contains("a loopback address"),
            "{link_address}: {again}"
        );
        assert!(!s.setup(&setup).await.unwrap().unwrap().virtual_machine);
    }
}

/// **The unspecified warning shows where the console's link address is an
/// unspecified address**, `0.0.0.0` or `::`, which a listener binds for
/// every interface and which names no machine to dial: `link_listen` with
/// no `link_address`, for a host and a virtual machine alike. It does not
/// show where `link_address` names an address beside such a `link_listen`,
/// or where `link_listen` names an address of its own.
#[tokio::test]
async fn the_unspecified_warning_shows_for_a_host_and_a_virtual_machine() {
    for (link_listen, link_address, unspecified) in [
        ("0.0.0.0:8081", None, true),
        ("[::]:8081", None, true),
        ("0.0.0.0:8081", Some("console.test:7443"), false),
        ("192.0.2.10:8081", None, false),
    ] {
        let Some(rig) = rig_with(link_listen, link_address, None).await else {
            return;
        };
        let s = rig.store();
        let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
        let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
        let uri = format!("/setup/{}/pairing", setup.unwrap());
        let host = page(&rig.app, &uri, &ada).await;
        let (status, _, _) = send_as(
            &rig.app,
            "POST",
            &uri,
            Some(form(&[("version", "1"), ("virtual_machine", "on")])),
            &ada,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let vm = page(&rig.app, &uri, &ada).await;
        for (shown, box_is) in [(&host, "host"), (&vm, "virtual machine")] {
            let case = format!("{link_listen}, {link_address:?}, {box_is}");
            assert_eq!(
                shown.contains("an unspecified address"),
                unspecified,
                "{case}: {shown}"
            );
            assert_eq!(
                shown.contains(r#"<span class="status fail">fail</span>"#),
                unspecified,
                "{case}: the link's pane is failed where it warns: {shown}"
            );
            assert!(!shown.contains("a loopback address"), "{case}: {shown}");
        }
    }
}

/// **A stale version answers 409 and writes nothing**: a write carrying a
/// version the row has moved past is refused, the row as the first write
/// left it, and its first record's outcome failed.
#[tokio::test]
async fn a_stale_version_answers_409_and_writes_nothing() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    let uri = format!("/setup/{setup}/pairing");
    let (status, _, _) = send_as(
        &rig.app,
        "POST",
        &uri,
        Some(form(&[("version", "1"), ("virtual_machine", "on")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let (status, _, body) = send_as(
        &rig.app,
        "POST",
        &uri,
        Some(form(&[("version", "1")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("reload it"), "{body}");
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert!(kept.virtual_machine, "the stale write changed nothing");
    assert_eq!(kept.version, 2);
    assert_eq!(audited(s, "setup pairing").await, (2, 1, 1, 0));
    // A checkbox posted with another value than `on` is the ask's fault.
    let (status, _, body) = send_as(
        &rig.app,
        "POST",
        &uri,
        Some(form(&[("version", "2"), ("virtual_machine", "yes")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(audited(s, "setup pairing").await, (2, 1, 1, 0));
}

/// **An unknown setup is not found and a malformed one refused**, each
/// recorded nowhere, and a page this tree does not serve answers as no
/// route does.
#[tokio::test]
async fn an_unknown_setup_is_not_found_and_a_malformed_one_refused() {
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let (_, _, setup) = begin(&rig.app, &ada, "karl").await;
    let setup = setup.unwrap();
    for (method, uri, want) in [
        (
            "GET",
            "/setup/st-0123456789abcdef/pairing",
            StatusCode::NOT_FOUND,
        ),
        ("GET", "/setup/st-0123456789abcdef", StatusCode::NOT_FOUND),
        ("GET", "/setup/st-0123/pairing", StatusCode::BAD_REQUEST),
        ("GET", "/setup/karl/drives", StatusCode::BAD_REQUEST),
        (
            "POST",
            "/setup/st-0123456789abcdef/pairing",
            StatusCode::NOT_FOUND,
        ),
        ("POST", "/setup/st-0123/pairing", StatusCode::BAD_REQUEST),
        (
            "GET",
            "/setup/st-0123456789abcdef/models",
            StatusCode::NOT_FOUND,
        ),
        ("GET", "/setup/st-0123/models", StatusCode::BAD_REQUEST),
        (
            "GET",
            "/setup/st-0123456789abcdef/models/pull",
            StatusCode::NOT_FOUND,
        ),
        ("GET", "/setup/st-0123/models/pull", StatusCode::BAD_REQUEST),
        ("POST", "/setup/st-0123/models", StatusCode::BAD_REQUEST),
        (
            "GET",
            "/setup/st-0123456789abcdef/placement",
            StatusCode::NOT_FOUND,
        ),
        ("GET", "/setup/st-0123/placement", StatusCode::BAD_REQUEST),
        (
            "POST",
            "/setup/st-0123456789abcdef/placement",
            StatusCode::NOT_FOUND,
        ),
        ("POST", "/setup/st-0123/placement", StatusCode::BAD_REQUEST),
        (
            "GET",
            "/setup/st-0123456789abcdef/confirm",
            StatusCode::NOT_FOUND,
        ),
        ("GET", "/setup/st-0123/confirm", StatusCode::BAD_REQUEST),
        (
            "POST",
            "/setup/st-0123456789abcdef/confirm",
            StatusCode::NOT_FOUND,
        ),
        ("POST", "/setup/st-0123/confirm", StatusCode::BAD_REQUEST),
        (
            "POST",
            "/setup/st-0123456789abcdef/models/drop",
            StatusCode::NOT_FOUND,
        ),
        (
            "POST",
            "/setup/st-0123/models/drop",
            StatusCode::BAD_REQUEST,
        ),
        (
            "GET",
            &format!("/setup/{setup}/elsewhere"),
            StatusCode::NOT_FOUND,
        ),
    ] {
        // Each post well shaped, so the setup's answer is what is judged.
        let body = (method == "POST").then(|| {
            if uri.ends_with("/placement") {
                form(&[("version", "1"), ("turn_context", "continuous")])
            } else {
                form(&[("version", "1")])
            }
        });
        let (status, _, answer) = send_as(&rig.app, method, uri, body, &ada).await;
        assert_eq!(status, want, "{method} {uri}: {answer}");
    }
    assert_eq!(audited(s, "setup pairing").await, (0, 0, 0, 0));
    assert_eq!(audited(s, "setup drop").await, (0, 0, 0, 0));
}

/// **The start page draws New agent once an agent stands in the
/// register**, the name's field opened from it, and the field itself
/// before.
#[tokio::test]
async fn the_start_page_draws_new_agent_once_an_agent_stands() {
    let Some(rig) = crate::agents_tests::rig().await else {
        return;
    };
    let s = rig.store();
    let (_, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;
    let before = page(&rig.app, "/setup", &ada).await;
    assert!(!before.contains("<summary>New agent</summary>"), "{before}");
    assert!(
        before.contains(r#"<input type="text" name="name""#),
        "{before}"
    );
    let (status, _, handover) = send_as(
        &rig.app,
        "POST",
        "/admin/agents/register",
        Some(form(&[("box", "box-a"), ("name", "karl")])),
        &ada,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{handover}");
    let after = page(&rig.app, "/setup", &ada).await;
    let opened = between(&after, "<details>", "</details>").expect("New agent stands");
    assert!(
        opened.starts_with("\n  <summary>New agent</summary>"),
        "{opened}"
    );
    assert!(
        opened.contains(r#"<input type="text" name="name""#),
        "{opened}"
    );
}

/// **A begin and the pairing page's write run from their first records to
/// their outcomes in a task the request cannot cancel** (Spec 2.13, #150):
/// each request dropped while its first record is held, the write still
/// lands, its first record answered by its `ok` outcome, the start page
/// links the setup the dropped begin opened, and the pairing page then
/// reads the setup as it stands.
///
/// Perturbation: run step four in the request's own future
/// (`admin::step_four` awaiting it in place), and the first write's
/// outcome never comes.
#[tokio::test]
async fn a_begin_and_a_pairing_dropped_at_their_first_records_still_land() {
    use crate::admin_tests::{dropped_at_the_hold, outcome_of};
    let Some(rig) = rig().await else {
        return;
    };
    let s = rig.store();
    let (person, ada) = admin(&rig.app, s, &mut authenticator(), "ada").await;

    dropped_at_the_hold(&rig.app, &person, &ada, "/setup", form(&[("name", "karl")])).await;
    assert_eq!(outcome_of(s, &person, "setup begin").await, "ok");
    let setup: String = sqlx::query_scalar("SELECT setup_id FROM agent_setup WHERE name = 'karl'")
        .fetch_one(&s.pool)
        .await
        .expect("the begin landed");
    let uri = format!("/setup/{setup}/pairing");
    let start = page(&rig.app, "/setup", &ada).await;
    assert!(
        start.contains(&uri),
        "the start page links the setup the dropped begin opened: {start}"
    );
    let shown = page(&rig.app, &uri, &ada).await;
    assert!(
        !shown.contains(" checked> The box is a virtual machine"),
        "{shown}"
    );

    dropped_at_the_hold(
        &rig.app,
        &person,
        &ada,
        &uri,
        form(&[("version", "1"), ("virtual_machine", "on")]),
    )
    .await;
    assert_eq!(outcome_of(s, &person, "setup pairing").await, "ok");
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert!(kept.virtual_machine, "the pairing's write landed");
    assert_eq!(kept.version, 2);
    let shown = page(&rig.app, &uri, &ada).await;
    assert!(
        shown.contains(" checked> The box is a virtual machine"),
        "{shown}"
    );
}
