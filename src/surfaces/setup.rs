//! conforms: web-every-setup-pane-carries-its-stage-backing-and-status
//! conforms: web-the-start-page-refuses-every-name-the-box-refuses
//! conforms: web-the-pairing-page-warns-where-the-box-cannot-dial-the-link
//! conforms: web-a-pages-way-forward-stands-where-its-condition-holds
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//! conforms: web-the-pair-page-shows-the-pairing-stages-report-read-only-once-paired
//! conforms: web-new-agent-holds-what-an-agent-needs-the-steps-at-the-appliance-and-one-form-with-no-click-before-them
//! conforms: web-new-agent-gives-weaver-webs-management-address-and-never-the-data-doors-and-says-so-where-one-door-serves-both-planes
//! conforms: web-new-agent-lists-each-open-setup-not-paired-with-a-link-to-its-own-pair-page
//!
//! **New agent, and Declare's path** (Spec 6.2, 8.2), an admin's, the
//! server-wide admin grant. **New agent is the first page of stage 1**
//! (the operator's ruling of 2026-11-25; and of 2026-11-23: "Pairing is its
//! own page", and "the agent is named at pairing"): `GET /pair` holds, with
//! no click before them, what an agent needs, the steps at the appliance,
//! Weaver Web's management address and fingerprint, and the appliances
//! awaiting final initialization; `POST /pair` names the agent and enters
//! the chosen appliance's code, and the exchange makes the setup with the
//! row, so nothing is written before the code matches. `GET /pair/{setup}`
//! is that agent's Pair page, which shows the pairing stage's report and
//! leads on to the declaration, and `POST /pair/{setup}` keeps its
//! checkbox. A setup begun before New agent named agents in one form keeps
//! its Pair page as built, its code field among it. **The declaration is Declare's**: the Agents list
//! (`GET /agents`) opens it, `GET /setup` leads there, and `GET /setup/{setup}/{page}`
//! draws one page of the setup. The models page and its fragments are the child
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
use axum::response::{Html, IntoResponse, Redirect, Response};
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
use crate::store::setup::Setup;
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

/// **The appliance's setup command** (the WeaverAgent seat's script at its
/// head of 2026-11-25, not final until the script lands in a repository):
/// written here alone, so a change of the script's name changes one line.
/// The page says it runs with root's authority and names no program that
/// grants it, since `tests/no_privilege.rs` admits that program's name in
/// admin-con's invoker alone (Spec 7.2).
pub(crate) const SETUP_COMMAND: &str = "weaver-setup";

/// **The appliance's starting user and its first password, as New agent
/// names them**: neither is printed until the operator rules on 72 (v),
/// since a password in a template enters a public repository (`CLAUDE.md`,
/// hard boundaries). Written here alone, so his answer changes one line
/// each.
pub(crate) const STARTING_USER: &str = "the appliance's starting user";
pub(crate) const FIRST_PASSWORD: &str = "Its first password";

/// **The setup's pages, in the path's order** (Spec 6.2). Pairing is no
/// page of the setup: it is New agent's, before the declaration.
/// **The declaration starts at directories** (the operator's ruling of
/// 2026-11-23: "The declaration starts at directories, not drives").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Directories,
    Models,
    Placement,
    Confirm,
    Build,
}

impl Page {
    const ALL: [Page; 5] = [
        Page::Directories,
        Page::Models,
        Page::Placement,
        Page::Confirm,
        Page::Build,
    ];

    fn slug(self) -> &'static str {
        match self {
            Page::Models => "models",
            Page::Directories => "directories",
            Page::Placement => "placement",
            Page::Confirm => "confirm",
            Page::Build => "build",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Page::Models => "Models",
            Page::Directories => "Directories",
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
    let first_seams = seams.clone();
    let pair_seams = seams.clone();
    let pairing_seams = seams.clone();
    let mut router = Router::new()
        // **Declarations, the start page, is folded into the Agents list**
        // (Spec 2.8; the operator's words of 2026-11-27), which names every
        // setup; its address leads there.
        .route("/setup", get(|| async { Redirect::to("/agents") }))
        .route(
            "/pair",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let seams = first_seams.clone();
                async move { first_page(store, &seams, headers).await }
            }),
        )
        .route(
            "/pair/{setup}",
            get(
                move |State(store): State<Store>, headers: HeaderMap, Path(setup): Path<String>| {
                    let seams = pair_seams.clone();
                    async move { pair_box(store, &seams, headers, &setup).await }
                },
            )
            .post(
                move |State(store): State<Store>,
                      headers: HeaderMap,
                      Path(setup): Path<String>,
                      Form(ask): Form<PairingAsk>| {
                    let seams = pairing_seams.clone();
                    async move { pairing(store, &seams, headers, &setup, ask).await }
                },
            ),
        );
    // The code's entries, which the server's binary carries (Spec 8.2):
    // New agent's, and a setup's begun before it.
    #[cfg(feature = "enrollment")]
    {
        router = router
            .route("/pair", pair::finalize_route(seams.clone()))
            .route("/pair/{setup}/code", pair::route(seams.clone()));
    }
    // The pages drawn here alone. The rest are their child modules'.
    for p in [Page::Directories, Page::Build] {
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

/// **A declaration page's read** (Spec 6.2): `admin_read`, and **a setup
/// whose box is not paired leads to its Pair page**, since a declaration
/// opens once its box is paired (the Planner's ruling of 2026-11-24 on the
/// review). A pairing is never undone, so a setup read paired stays so.
async fn declaration_read(
    store: &Store,
    seams: &Seams,
    headers: &HeaderMap,
    setup_id: &str,
) -> Result<(Session, Setup), Response> {
    let (session, setup) = admin_read(store, seams, headers, setup_id).await?;
    if setup.paired.is_none() {
        return Err((
            StatusCode::SEE_OTHER,
            [(header::LOCATION, pair_href(&setup.setup_id))],
        )
            .into_response());
    }
    Ok((session, setup))
}

/// **A declaration write's state** (Spec 6.2): refused where the setup is
/// confirmed, which never changes, or its box is not paired, whose
/// declaration is not open, each as the wrong-state writes are, with no
/// record and before step four.
fn declaration_state(setup: &Setup) -> Result<(), Refusal> {
    if setup.state != "open" {
        Err(Refusal::SetupConfirmed)
    } else if setup.paired.is_none() {
        Err(Refusal::SetupUnpaired)
    } else {
        Ok(())
    }
}

/// The page of a pairing: New agent's page for `setup_id`.
fn pair_href(setup_id: &str) -> String {
    format!("/pair/{setup_id}")
}

/// The declaration's first page of `setup_id`, the way on from a pairing.
fn declare_next(setup_id: &str) -> Next {
    Next {
        href: format!("/setup/{setup_id}/{}", Page::ALL[0].slug()),
        title: Page::ALL[0].title(),
    }
}

/// **The way into a setup's declaration**, as the Agents list links it
/// (Spec 2.8): its Pair page while it waits for its appliance, and the
/// declaration's first page once paired.
pub(crate) fn declaration_href(setup: &Setup) -> String {
    if setup.in_pairing_step() {
        pair_href(&setup.setup_id)
    } else {
        declare_next(&setup.setup_id).href
    }
}

/// An appliance awaiting final initialization as New agent lists it.
struct FirstWaiting {
    /// The handle the form chooses it by, never its code.
    handle: String,
    claim: String,
    peer: String,
    waited: String,
    /// Dialed from a loopback address, Weaver Web's own machine, which a
    /// virtual machine never is: the page warns and never refuses.
    loopback: bool,
    /// The appliance the post being answered chose.
    chosen: bool,
}

/// **An open setup whose box is not paired, as New agent lists it**: begun
/// the old way, it holds its name, and its own Pair page is the one way to
/// pair it (the review of pull request 214, findings 5 and 6).
struct FirstUnpaired {
    name: String,
    href: String,
}

/// **What a post to New agent keeps**, drawn back into the form: the name
/// as typed, the appliance chosen, and what happened.
#[derive(Default)]
struct Kept {
    name: String,
    appliance: String,
    entered: Option<pair::Entered>,
    /// On an unknown outcome, the register's live rows holding the name,
    /// drawn below the answer; `None` draws no table.
    rows: Option<Vec<pair::RowView>>,
}

#[derive(Template)]
#[template(path = "pair_start.html")]
struct FirstPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    command: &'static str,
    starting_user: &'static str,
    first_password: &'static str,
    /// Weaver Web's management door, which the script asks for.
    link: Link,
    /// One door serves both planes (Spec 8).
    one_door: bool,
    loopback_warning: bool,
    unspecified_warning: bool,
    /// The enrollment is served: false draws the appliances' pane held.
    live: bool,
    fingerprint: Vec<String>,
    waiting: Vec<FirstWaiting>,
    /// An appliance listed dialed from a loopback address.
    any_loopback: bool,
    name_most: usize,
    name: String,
    entered: Option<pair::Entered>,
    /// On an unknown outcome, the register's live rows holding the name.
    rows: Option<Vec<pair::RowView>>,
    /// The open setups not paired, each with a link to its own Pair page;
    /// their pane is drawn only where one exists.
    unpaired: Vec<FirstUnpaired>,
}

/// **New agent, as drawn** for `session`, with what a post kept.
async fn first_view(
    store: &Store,
    seams: &Seams,
    session: &Session,
    kept: Kept,
) -> anyhow::Result<FirstPage> {
    let chrome = super::Chrome::of(store, &session.person_id).await?;
    // **The management door's address** (Spec 8): the script dials it for
    // the pairing and admin-con, never the data door's.
    let link = link_of(seams.cfg.link_address());
    let (live, fingerprint, waiting) = match seams.enrollment.as_ref() {
        Some(enrollment) => (
            cfg!(feature = "enrollment"),
            pair::fingerprint_rows(&enrollment.authority.fingerprint()),
            enrollment
                .listener
                .waiting()
                .into_iter()
                .map(|w| FirstWaiting {
                    chosen: w.handle == kept.appliance,
                    handle: w.handle,
                    claim: w.r#box,
                    peer: w.peer.to_string(),
                    waited: pair::waited(w.waited),
                    loopback: w.peer.is_loopback(),
                })
                .collect(),
        ),
        None => (false, Vec::new(), Vec::new()),
    };
    let unpaired = store
        .open_setups()
        .await?
        .into_iter()
        .filter(|s| s.in_pairing_step())
        .map(|s| FirstUnpaired {
            href: pair_href(&s.setup_id),
            name: s.name,
        })
        .collect();
    Ok(FirstPage {
        here: "pair",
        who: session.name.clone(),
        admin: true,
        chrome,
        command: SETUP_COMMAND,
        starting_user: STARTING_USER,
        first_password: FIRST_PASSWORD,
        one_door: seams.cfg.data_door().is_none(),
        loopback_warning: link.loopback,
        unspecified_warning: link.unspecified,
        link,
        live,
        fingerprint,
        any_loopback: waiting.iter().any(|w| w.loopback),
        waiting,
        name_most: crate::store::setup::NAME_MOST,
        name: kept.name,
        entered: kept.entered,
        rows: kept.rows,
        unpaired,
    })
}

/// **New agent** (Spec 8.2; the operator's rulings of 2026-11-25): what an
/// agent needs, the steps at the appliance, Weaver Web's management address
/// and fingerprint, the appliances awaiting final initialization, and the
/// one form that names the agent and finalizes the pairing, with no click
/// before them.
async fn first_page(store: Store, seams: &Seams, headers: HeaderMap) -> Response {
    let session = match admin_session(&store, &seams.policy, &headers).await {
        Ok(Ok(session)) => session,
        Ok(Err(_)) => return (StatusCode::FORBIDDEN, NOT_AN_ADMIN).into_response(),
        Err(answer) => return answer,
    };
    first_answer(&store, seams, &session, StatusCode::OK, Kept::default()).await
}

/// New agent drawn at `status`, with what a post kept: the answer to every
/// post that passed the admin gate and did not pair.
async fn first_answer(
    store: &Store,
    seams: &Seams,
    session: &Session,
    status: StatusCode,
    kept: Kept,
) -> Response {
    match first_view(store, seams, session, kept)
        .await
        .map(|p| p.render())
    {
        Ok(Ok(html)) => (status, Html(html)).into_response(),
        Ok(Err(e)) => fault(e),
        Err(e) => fault(e),
    }
}

/// Back to the setup's page, the write landed.
fn landed(written: anyhow::Result<Result<(), Refusal>>, setup_id: &str, page: Page) -> Response {
    landed_at(written, &format!("/setup/{setup_id}/{}", page.slug()))
}

/// Back to the page at `location`, the write landed.
fn landed_at(written: anyhow::Result<Result<(), Refusal>>, location: &str) -> Response {
    match written {
        Ok(Ok(())) => (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response(),
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

/// **The Pair page's checkbox**, over the setup read at the page's
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
    // **A confirmed setup's checkbox is refused as its other writes are**,
    // with no record (the Planner's ruling of 2026-11-24 on the second
    // review).
    match store.setup(setup_id).await {
        Ok(Some(setup)) if setup.state != "open" => return refused(&Refusal::SetupConfirmed),
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
        landed_at(written, &pair_href(&setup_id))
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
    setup_id: String,
    name: String,
    begun_by: String,
    begun_at: String,
    /// The setup is confirmed, so the machine's kind is drawn as text.
    confirmed: bool,
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
    /// **The report, read only, once the box is paired** (Spec 8.2, the
    /// operator's ruling of 2026-11-24): none before.
    report: Option<report::Shown<report::PairingBody>>,
    /// When a row kept under version 1's stage `drives` arrived, named and
    /// feeding nothing.
    drives_kept: Option<String>,
    /// The way on once the box is paired: the declaration's first page.
    declare: Option<Next>,
}

/// The report as the Pair page draws it, and when a row kept under the
/// stage `drives` arrived.
type PairingReport = (Option<report::Shown<report::PairingBody>>, Option<String>);

/// **The report for the Pair page**: read once the setup's box is paired,
/// and none before, since no report arrives before it.
async fn pairing_report(
    store: &Store,
    seams: &Seams,
    setup: &Setup,
) -> anyhow::Result<PairingReport> {
    if setup.paired.is_none() {
        return Ok((None, None));
    }
    let reports = store.reports(seams.boxes.as_ref(), &setup.setup_id).await?;
    Ok((
        Some(report::Shown::of(&reports.report, report::PairingBody::of)),
        reports.drives_kept.map(when),
    ))
}

/// The Pair page as drawn, the waiting box's pane given.
fn pairing_page(
    seams: &Seams,
    who: String,
    chrome: super::Chrome,
    setup: &Setup,
    pair: pair::PairPane,
    (report, drives_kept): PairingReport,
) -> PairingPage {
    let link = link_of(seams.cfg.link_address());
    PairingPage {
        here: "pair",
        who,
        admin: true,
        chrome,
        setup_id: setup.setup_id.clone(),
        name: setup.name.clone(),
        begun_by: setup.begun_by.clone(),
        begun_at: when(setup.begun_at),
        confirmed: setup.state != "open",
        version: setup.version,
        virtual_machine: setup.virtual_machine,
        loopback_warning: link.loopback && setup.virtual_machine,
        unspecified_warning: link.unspecified,
        link,
        pair,
        report,
        drives_kept,
        declare: setup
            .paired
            .is_some()
            .then(|| declare_next(&setup.setup_id)),
    }
}

/// **An agent's pairing**, New agent's page for `setup_id`: what to do at
/// the appliance, Weaver Web's address, the checkbox and the waiting box.
async fn pair_box(store: Store, seams: &Seams, headers: HeaderMap, setup_id: &str) -> Response {
    let (session, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let (chrome, pair, report) = match async {
        Ok::<_, anyhow::Error>((
            super::Chrome::of(&store, &session.person_id).await?,
            pair::pane(&store, seams, &setup, None).await?,
            pairing_report(&store, seams, &setup).await?,
        ))
    }
    .await
    {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    match pairing_page(seams, session.name.clone(), chrome, &setup, pair, report).render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

#[derive(Template)]
#[template(path = "setup_directories.html")]
struct DirectoriesPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    frame: Frame,
    keys: Vec<crate::surfaces::ssh_key::KeyView>,
    /// The console's `model_cache`, where set.
    model_cache: Option<String>,
    /// The directories as mounted, from the report, judged (Spec 6.2).
    directories: report::Shown<report::DrivesBody>,
}

/// **The directories as mounted**, from the report, judged against `need`.
fn directories(
    reports: &crate::box_report::Reports,
    need: Option<&Need>,
) -> report::Shown<report::DrivesBody> {
    report::Shown::of(&reports.report, |survey| {
        report::DrivesBody::of(
            survey,
            need.map(|need| report::NeedOf {
                bytes: need.total,
                shown: &need.bytes,
            }),
        )
    })
}

/// **The models directory's need** (Spec 6.2): the bytes of every pick's
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
    let (session, setup) = match declaration_read(&store, seams, &headers, setup_id).await {
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
        Page::Directories => {
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
            let directories = directories(&reports, need.as_ref());
            DirectoriesPage {
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
                directories,
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
