//! conforms: web-every-setup-pane-carries-its-stage-backing-and-status
//! conforms: web-the-start-page-refuses-every-name-the-box-refuses
//! conforms: web-the-pairing-page-warns-where-the-box-cannot-dial-the-link
//! conforms: web-a-pages-way-forward-stands-where-its-condition-holds
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//!
//! **Declare's path: the start page and the setup pages** (Spec 6.2), an
//! admin's, the server-wide admin grant, since a setup comes before
//! Provision's register. `GET /setup` is the start page: the agent's name
//! and the setups left open, New agent standing in the field's place once
//! an agent stands in the register. `POST /setup` begins a setup of the
//! name. `GET /setup/{setup}/{page}` draws one page of the setup, `GET
//! /setup/{setup}` its pairing page, and `POST /setup/{setup}/pairing` keeps the
//! pairing page's checkbox. The models page and its fragments are the child
//! module `models` (`setup/models.rs`), the placement page and its post
//! `placement` (`setup/placement.rs`), and the confirm page and Confirm
//! `confirm` (`setup/confirm.rs`). What the box reports, the survey's cards
//! and the box's members, comes through the `boxes` seam, which reads the
//! reports the setup keeps through the survey's reader (`box_report::Kept`).
//!
//! **Every pane carries its stage, its backing and its status**, and a
//! held pane says in its footer what it waits on and holds no input with a
//! value. **Each write takes the admin writes' order** (Spec 2.13): the
//! form parsed to its shape, the start page's name rule among it, the admin
//! gate with its one refusal record before anything is looked up, the
//! setup resolved, then the audited write with the admin as principal by
//! `session`. Reads are not audited. **The pages reach no agent and no
//! box**: a write lands in the store, and nothing leaves on the link.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use crate::box_report::BoxReports;
use crate::config::ServerConfig;
use crate::hugging_face::Client;
use crate::hugging_face::headers::Headers;
use crate::hugging_face::pull::Puller;
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::identity::is_setup_id;
use crate::store::setup::{Setup, agent_name};
use crate::surfaces::admin::{
    NOT_AN_ADMIN, admin_session, fault, first_of, refused, settled, step_four, writer,
};
use crate::surfaces::gate::{Policy, Session};

mod confirm;
mod models;
mod pair;
mod placement;
mod report;

pub use pair::Enrollment;

/// What the surface takes beside the store: the session policy; the
/// server's config, for the console's link address and `model_cache`; and,
/// for the models page, the Hugging Face client (whose base is the test
/// seam), the puller a pick asks, the directory the seal key stands in, and
/// the headers kept in memory. For the placement and confirm pages, it
/// takes what the box reports.
#[derive(Clone)]
pub struct Seams {
    pub policy: Policy,
    pub cfg: Arc<ServerConfig>,
    pub client: Client,
    pub puller: Puller,
    pub authority_dir: PathBuf,
    pub headers: Headers,
    pub boxes: Arc<dyn BoxReports>,
    /// **The enrollment** (Spec 8.2): the link's listener, whose boxes
    /// wait, and the authority the server loaded. None draws the waiting
    /// box held.
    pub enrollment: Option<Enrollment>,
}

/// The answer to a setup's identity of the wrong shape.
const MALFORMED_SETUP: &str = "a setup's identity is st- and sixteen lowercase hex";

/// **The setup's pages, in the path's order** (Spec 6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Pairing,
    Models,
    Drives,
    Placement,
    Confirm,
    Build,
}

impl Page {
    const ALL: [Page; 6] = [
        Page::Pairing,
        Page::Models,
        Page::Drives,
        Page::Placement,
        Page::Confirm,
        Page::Build,
    ];

    fn slug(self) -> &'static str {
        match self {
            Page::Pairing => "pairing",
            Page::Models => "models",
            Page::Drives => "drives",
            Page::Placement => "placement",
            Page::Confirm => "confirm",
            Page::Build => "build",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Pairing => "Pairing",
            Page::Models => "Models",
            Page::Drives => "Drives",
            Page::Placement => "Placement",
            Page::Confirm => "Confirm",
            Page::Build => "Build",
        }
    }
}

pub fn routes(seams: Seams) -> Router<Store> {
    macro_rules! read {
        ($page:expr) => {{
            let seams = seams.clone();
            get(
                move |State(store): State<Store>, headers: HeaderMap, Path(setup): Path<String>| {
                    let seams = seams.clone();
                    async move { page(store, &seams, headers, &setup, $page).await }
                },
            )
        }};
    }
    let start_seams = seams.clone();
    let begin_seams = seams.clone();
    let pairing_seams = seams.clone();
    let mut router = Router::new()
        .route(
            "/setup",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let seams = start_seams.clone();
                async move { start(store, &seams, headers).await }
            })
            .post(
                move |State(store): State<Store>, headers: HeaderMap, Form(ask): Form<BeginAsk>| {
                    let seams = begin_seams.clone();
                    async move { begin(store, &seams, headers, ask).await }
                },
            ),
        )
        .route("/setup/{setup}", read!(Page::Pairing))
        .route(
            "/setup/{setup}/pairing",
            read!(Page::Pairing).post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Form(ask): Form<PairingAsk>| {
                    let seams = pairing_seams.clone();
                    async move { pairing(store, &seams, headers, &setup, ask).await }
                },
            ),
        );
    // The code's entry, which the server's binary carries (Spec 8.2).
    #[cfg(feature = "enrollment")]
    {
        router = router.route("/setup/{setup}/pair", pair::route(seams.clone()));
    }
    // The pages drawn here alone. The rest are their child modules'.
    for p in [Page::Drives, Page::Build] {
        router = router.route(&format!("/setup/{{setup}}/{}", p.slug()), read!(p));
    }
    router
        .merge(models::routes(&seams))
        .merge(placement::routes(&seams))
        .merge(confirm::routes(&seams))
        .merge(crate::surfaces::ssh_key::routes(seams.policy.clone()))
}

fn when(at: chrono::DateTime<chrono::Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// A step of the setup's rail.
struct Step {
    title: &'static str,
    href: String,
    here: bool,
}

/// **What every setup page carries**: the setup, its rail, and the way on
/// to the next page.
struct Frame {
    setup_id: String,
    name: String,
    state: String,
    begun_by: String,
    begun_at: String,
    steps: Vec<Step>,
    next: Option<Next>,
}

/// The way on: the next page.
struct Next {
    href: String,
    title: &'static str,
}

/// The frame of `here`. **Confirm's way forward is Confirm itself**, so
/// the confirm page offers the next page only once the setup is confirmed
/// (Spec 6.2).
fn frame(setup: &Setup, here: Page) -> Frame {
    let at = Page::ALL.iter().position(|p| *p == here).unwrap_or(0);
    let next = Page::ALL
        .get(at + 1)
        .filter(|_| here != Page::Confirm || setup.state == "confirmed")
        .map(|p| Next {
            href: format!("/setup/{}/{}", setup.setup_id, p.slug()),
            title: p.title(),
        });
    Frame {
        setup_id: setup.setup_id.clone(),
        name: setup.name.clone(),
        state: setup.state.clone(),
        begun_by: setup.begun_by.clone(),
        begun_at: when(setup.begun_at),
        steps: Page::ALL
            .iter()
            .map(|p| Step {
                title: p.title(),
                href: format!("/setup/{}/{}", setup.setup_id, p.slug()),
                here: *p == here,
            })
            .collect(),
        next,
    }
}

/// **The console's link address and port** as the box's setup asks for
/// them: `link_address`, else `link_listen`, as a client config carries it.
struct Link {
    address: String,
    /// None where the address names no port.
    port: Option<String>,
    /// Whether the address is a loopback address, which a virtual machine
    /// cannot reach on the console's host.
    loopback: bool,
    /// Whether the address is an unspecified address, `0.0.0.0` or `::`,
    /// which a listener binds for every interface and which names no
    /// machine to dial: RFC 1122 admits `0.0.0.0` as a source alone, and
    /// RFC 4291 refuses `::` as a destination.
    unspecified: bool,
}

fn link_of(given: &str) -> Link {
    let (host, port) = match given.rsplit_once(':') {
        Some((host, port))
            if !port.is_empty()
                && port.chars().all(|c| c.is_ascii_digit())
                && (!host.contains(':') || host.ends_with(']')) =>
        {
            (host, Some(port.to_owned()))
        }
        _ => (given, None),
    };
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    let name = bare.trim_end_matches('.').to_ascii_lowercase();
    let ip = bare.parse::<IpAddr>().ok().map(|ip| match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    });
    let loopback = name == "localhost"
        || name.ends_with(".localhost")
        || ip.is_some_and(|ip| ip.is_loopback());
    Link {
        address: host.to_owned(),
        port,
        loopback,
        unspecified: ip.is_some_and(|ip| ip.is_unspecified()),
    }
}

/// The session a page or a write reads, its person an admin, or the
/// answer. A setup's identity is parsed first and the gate stands before
/// the setup is looked up, so a person without an admin grant learns
/// nothing of which setups stand.
async fn admin_read(
    store: &Store,
    seams: &Seams,
    headers: &HeaderMap,
    setup_id: &str,
) -> Result<(Session, Setup), Response> {
    if !is_setup_id(setup_id) {
        return Err((StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response());
    }
    let session = match admin_session(store, &seams.policy, headers).await? {
        Ok(session) => session,
        Err(_) => return Err((StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response()),
    };
    match store.setup(setup_id).await {
        Ok(Some(setup)) => Ok((session, setup)),
        Ok(None) => Err(refused(&Refusal::NoSuchSetup)),
        Err(e) => Err(fault(e)),
    }
}

struct OpenView {
    name: String,
    begun_by: String,
    begun_at: String,
    href: String,
}

#[derive(Template)]
#[template(path = "setup_start.html")]
struct StartPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    agent_stands: bool,
    open: Vec<OpenView>,
    name_most: usize,
}

/// **The start page**: the agent's name and the setups left open, each a
/// link to its next page.
async fn start(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, &seams.policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    let (chrome, open, agent_stands) = match async {
        Ok::<_, anyhow::Error>((
            super::Chrome::of(&store, &session.person_id).await?,
            store.open_setups().await?,
            store.an_agent_stands().await?,
        ))
    }
    .await
    {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    let page = StartPage {
        here: "declarations",
        who: session.name.clone(),
        admin: true,
        chrome,
        agent_stands,
        open: open
            .into_iter()
            .map(|s| OpenView {
                href: format!("/setup/{}/{}", s.setup_id, Page::Pairing.slug()),
                name: s.name,
                begun_by: s.begun_by,
                begun_at: when(s.begun_at),
            })
            .collect(),
        name_most: crate::store::setup::NAME_MOST,
    };
    match page.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct BeginAsk {
    name: String,
}

/// **A new setup of the name**, the name held to the start page's rule
/// before anything else, so a refused name opens no setup and is recorded
/// nowhere.
async fn begin(store: Store, seams: &Seams, headers: HeaderMap, ask: BeginAsk) -> Response {
    const ACTION: &str = "setup begin";
    let name = match agent_name(ask.name.trim()) {
        Ok(name) => name,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let setup_id = match store.mint_key("st").await {
        Ok(setup_id) => setup_id,
        Err(e) => return fault(e),
    };
    let target = Target::Setup(&setup_id);
    let session = match writer(&store, &seams.policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let name = name.to_owned();
    step_four(&seams.policy, async move {
        let first = match first_of(&store, &session, Target::Setup(&setup_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .begin_setup(
                &session.person_id,
                session.session_id,
                &first,
                &setup_id,
                &name,
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, &setup_id, Page::Pairing)
    })
    .await
}

/// Back to the setup's page, the write landed.
fn landed(written: anyhow::Result<Result<(), Refusal>>, setup_id: &str, page: Page) -> Response {
    match written {
        Ok(Ok(())) => (
            StatusCode::SEE_OTHER,
            [(
                header::LOCATION,
                format!("/setup/{setup_id}/{}", page.slug()),
            )],
        )
            .into_response(),
        Ok(Err(refusal)) => refused(&refusal),
        Err(e) => fault(e),
    }
}

#[derive(Deserialize)]
pub struct PairingAsk {
    version: i64,
    /// `on` where the checkbox is checked, absent where it is not, as a
    /// browser posts a checkbox.
    #[serde(default)]
    virtual_machine: Option<String>,
}

/// **The pairing page's checkbox**, over the setup read at the page's
/// version.
async fn pairing(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    ask: PairingAsk,
) -> Response {
    const ACTION: &str = "setup pairing";
    if !is_setup_id(setup_id) {
        return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
    }
    if ask.version < 1 {
        return (StatusCode::BAD_REQUEST, "a setup's version is 1 at least").into_response();
    }
    let virtual_machine = match ask.virtual_machine.as_deref() {
        None => false,
        Some("on") => true,
        Some(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "the checkbox is posted as on, or not at all",
            )
                .into_response();
        }
    };
    let target = Target::Setup(setup_id);
    let session = match writer(&store, &seams.policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.setup(setup_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    }
    let setup_id = setup_id.to_owned();
    step_four(&seams.policy, async move {
        let first = match first_of(&store, &session, Target::Setup(&setup_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .setup_machine(
                &session.person_id,
                session.session_id,
                &first,
                &setup_id,
                virtual_machine,
                ask.version,
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, &setup_id, Page::Pairing)
    })
    .await
}

#[derive(Template)]
#[template(path = "setup_pairing.html")]
struct PairingPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    frame: Frame,
    version: i64,
    virtual_machine: bool,
    link: Link,
    /// The address is a loopback address and the checkbox says virtual
    /// machine.
    loopback_warning: bool,
    /// The address is an unspecified address, which no box dials, a host
    /// or a virtual machine.
    unspecified_warning: bool,
    /// The waiting box's pane, the enrollment's parts (Spec 8.2).
    pair: pair::PairPane,
    /// The `pairing` stage's report (Spec 6.2).
    report: report::Shown<report::PairingBody>,
}

/// **The `pairing` stage's report as its pane draws it** (Spec 6.2), read
/// through the `boxes` seam. A store that fails to answer is an error.
async fn pairing_report(
    store: &Store,
    seams: &Seams,
    setup_id: &str,
) -> anyhow::Result<report::Shown<report::PairingBody>> {
    let reports = store.reports(seams.boxes.as_ref(), setup_id).await?;
    Ok(report::Shown::of(&reports.pairing, report::PairingBody::of))
}

/// The pairing page as drawn, the waiting box's pane and the report given.
fn pairing_page(
    seams: &Seams,
    who: String,
    chrome: super::Chrome,
    frame: Frame,
    setup: &Setup,
    pair: pair::PairPane,
    report: report::Shown<report::PairingBody>,
) -> PairingPage {
    let link = link_of(seams.cfg.link_address());
    PairingPage {
        here: "declarations",
        who,
        admin: true,
        chrome,
        frame,
        version: setup.version,
        virtual_machine: setup.virtual_machine,
        loopback_warning: link.loopback && setup.virtual_machine,
        unspecified_warning: link.unspecified,
        link,
        pair,
        report,
    }
}

#[derive(Template)]
#[template(path = "setup_drives.html")]
struct DrivesPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    frame: Frame,
    keys: Vec<crate::surfaces::ssh_key::KeyView>,
    /// The console's `model_cache`, where set.
    model_cache: Option<String>,
    /// The models drive's need, where a model is picked.
    need: Option<Need>,
    /// The drives as mounted, from the `drives` stage's report, judged
    /// (Spec 6.2).
    drives: report::Shown<report::DrivesBody>,
}

/// **The models drive's need** (Spec 6.2): the bytes of every pick's
/// distinct blobs, a blob two paths share counted once.
struct Need {
    /// The bytes, as a person reads them.
    bytes: String,
    /// The bytes, counted.
    total: u64,
    blobs: usize,
    picks: usize,
}

/// The bytes and the count of the distinct blobs among `files`, each
/// named by its repository and its blob's name, since a box keeps each
/// repository's blobs apart, as the cache does.
fn distinct_blobs<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a crate::hugging_face::RepoFile)>,
) -> (u64, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut bytes = 0u64;
    for (repository, file) in files {
        if seen.insert((repository, file.blob())) {
            bytes += file.size;
        }
    }
    (bytes, seen.len())
}

async fn need(store: &Store, setup_id: &str) -> anyhow::Result<Option<Need>> {
    let rows = store.setup_models(setup_id).await?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut files = Vec::new();
    for row in &rows {
        for file in store.pull_files(&row.pull_id).await? {
            files.push((row.repository.clone(), file.file));
        }
    }
    let (bytes, blobs) = distinct_blobs(files.iter().map(|(r, f)| (r.as_str(), f)));
    Ok(Some(Need {
        bytes: crate::surfaces::hugging_face::bytes(bytes),
        total: bytes,
        blobs,
        picks: rows.len(),
    }))
}

#[derive(Template)]
#[template(path = "setup_build.html")]
struct BuildPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    frame: Frame,
}

/// **One page of a setup**, drawn from the store and the config alone.
async fn page(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    at: Page,
) -> Response {
    let (session, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let chrome = match super::Chrome::of(&store, &session.person_id).await {
        Ok(chrome) => chrome,
        Err(e) => return fault(e),
    };
    let frame = frame(&setup, at);
    let who = session.name.clone();
    let rendered = match at {
        Page::Pairing => {
            let pair = match pair::pane(&store, seams, &setup, None).await {
                Ok(pair) => pair,
                Err(e) => return fault(e),
            };
            let report = match pairing_report(&store, seams, setup_id).await {
                Ok(report) => report,
                Err(e) => return fault(e),
            };
            pairing_page(seams, who, chrome, frame, &setup, pair, report).render()
        }
        Page::Drives => {
            let (keys, need, reports) = match async {
                Ok::<_, anyhow::Error>((
                    crate::surfaces::ssh_key::held(&store).await?,
                    need(&store, setup_id).await?,
                    store.reports(seams.boxes.as_ref(), setup_id).await?,
                ))
            }
            .await
            {
                Ok(read) => read,
                Err(e) => return fault(e),
            };
            let drives = report::Shown::of(&reports.drives, |survey| {
                report::DrivesBody::of(
                    survey,
                    need.as_ref().map(|need| report::NeedOf {
                        bytes: need.total,
                        shown: &need.bytes,
                    }),
                )
            });
            DrivesPage {
                here: "declarations",
                who,
                admin: true,
                chrome,
                frame,
                keys,
                model_cache: seams
                    .cfg
                    .model_cache
                    .as_ref()
                    .map(|p| p.display().to_string()),
                need,
                drives,
            }
            .render()
        }
        Page::Build => BuildPage {
            here: "declarations",
            who,
            admin: true,
            chrome,
            frame,
        }
        .render(),
        // Each is its child module's, never routed here.
        Page::Models | Page::Placement | Page::Confirm => {
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[cfg(test)]
mod tests {
    use super::link_of;

    /// **The console's link address read as the box asks for it**: the
    /// host and the port apart, an IPv6 host in its brackets, loopback
    /// named for `localhost`, a name under it, 127/8, `::1` and the
    /// IPv4-mapped loopback, and the unspecified address named for
    /// `0.0.0.0`, `::` and the IPv4-mapped `0.0.0.0`. No address here is a
    /// real one.
    #[test]
    fn the_link_address_is_read_for_its_port_its_loopback_and_unspecified() {
        for (given, address, port, loopback) in [
            ("console.test:7443", "console.test", Some("7443"), false),
            ("192.0.2.10:9443", "192.0.2.10", Some("9443"), false),
            ("127.0.0.1:8081", "127.0.0.1", Some("8081"), true),
            ("127.1.2.3:8081", "127.1.2.3", Some("8081"), true),
            ("localhost:8081", "localhost", Some("8081"), true),
            ("LocalHost.:8081", "LocalHost.", Some("8081"), true),
            ("box.localhost:8081", "box.localhost", Some("8081"), true),
            ("[::1]:8081", "[::1]", Some("8081"), true),
            (
                "[::ffff:127.0.0.1]:8081",
                "[::ffff:127.0.0.1]",
                Some("8081"),
                true,
            ),
            ("[2001:db8::1]:8081", "[2001:db8::1]", Some("8081"), false),
            ("console.test", "console.test", None, false),
            ("::1", "::1", None, true),
        ] {
            let link = link_of(given);
            assert_eq!(link.address, address, "{given}");
            assert_eq!(link.port.as_deref(), port, "{given}");
            assert_eq!(link.loopback, loopback, "{given}");
            assert!(!link.unspecified, "{given}");
        }
        for (given, address, port) in [
            ("0.0.0.0:8081", "0.0.0.0", Some("8081")),
            ("0.0.0.0", "0.0.0.0", None),
            ("[::]:8081", "[::]", Some("8081")),
            ("::", "::", None),
            ("[::ffff:0.0.0.0]:8081", "[::ffff:0.0.0.0]", Some("8081")),
        ] {
            let link = link_of(given);
            assert_eq!(link.address, address, "{given}");
            assert_eq!(link.port.as_deref(), port, "{given}");
            assert!(link.unspecified, "{given}");
            assert!(!link.loopback, "{given}");
        }
    }
}
