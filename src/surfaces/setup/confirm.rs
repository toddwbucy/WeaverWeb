//! conforms: web-the-deployment-tuple-refuses-any-update-or-delete-and-a-setup-is-confirmed-once
//! conforms: web-the-members-the-box-is-laid-from-are-the-confirmed-tuples-and-none-stand-before-confirm
//! conforms: web-an-admin-write-runs-from-its-first-record-to-its-outcome-in-a-task-the-request-cannot-cancel
//!
//! **The confirm page and Confirm** (Spec 6.2), a child of the setup pages'
//! module. `GET /setup/{setup}/confirm` draws every member of the
//! deployment tuple with its source, read or held, each held one saying
//! what it waits on, and Confirm, which stands only where every member is
//! read. Once the setup is confirmed the page draws the tuple from its row
//! alone and offers the build page.
//!
//! **Confirm** (`POST /setup/{setup}/confirm`) takes the admin writes'
//! order (Spec 2.13):
//!
//! 1. The form is parsed to its shape, the setup's version alone.
//! 2. The tuple's identity is minted.
//! 3. The admin gate writes its one refusal record.
//! 4. The setup must stand, open, and every member must be read. A member
//!    unread is refused, naming it and recording nothing.
//! 5. The audited write reads every member again inside its transaction and
//!    writes the tuple from what it read.
//!
//! It lands on the build page.

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use super::{Frame, MALFORMED_SETUP, Page, Seams, admin_read, frame, landed, when};
use crate::box_report::{BoxMembers, gib};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::identity::is_setup_id;
use crate::store::tuple::{Members, Model};
use crate::surfaces::admin::{fault, first_of, refused, settled, step_four, writer};

/// Confirm's action, as its audit records name it.
const ACTION: &str = "setup confirm";

pub(super) fn routes(seams: &Seams) -> Router<Store> {
    let page_seams = seams.clone();
    let confirm_seams = seams.clone();
    Router::new().route(
        "/setup/{setup}/confirm",
        get(
            move |State(store): State<Store>, headers: HeaderMap, Path(setup): Path<String>| {
                let seams = page_seams.clone();
                async move { page(store, &seams, headers, &setup).await }
            },
        )
        .post(
            move |State(store): State<Store>,
                  headers: HeaderMap,
                  Path(setup): Path<String>,
                  Form(fields): Form<Vec<(String, String)>>| {
                let seams = confirm_seams.clone();
                async move { confirm(store, &seams, headers, &setup, fields).await }
            },
        ),
    )
}

// ---- the page ----

/// **A member's pane**: its title, its source, its rows where it is read,
/// and where it is not, why and what it waits on.
struct MemberView {
    title: &'static str,
    source: &'static str,
    rows: Vec<(String, String)>,
    why: Option<String>,
    waits: &'static str,
}

fn member<T>(
    title: &'static str,
    source: &'static str,
    waits: &'static str,
    read: &Result<T, String>,
    rows: impl FnOnce(&T) -> Vec<(String, String)>,
) -> MemberView {
    match read {
        Ok(value) => MemberView {
            title,
            source,
            rows: rows(value),
            why: None,
            waits,
        },
        Err(why) => MemberView {
            title,
            source,
            rows: Vec::new(),
            why: Some(why.clone()),
            waits,
        },
    }
}

fn model_rows(model: &Model) -> Vec<(String, String)> {
    let mut rows = vec![
        ("Repository".to_owned(), model.repository.clone()),
        ("Commit".to_owned(), model.commit.clone()),
        ("Pull".to_owned(), model.pull_id.clone()),
    ];
    for (at, part) in model.parts.iter().enumerate() {
        let n = at + 1;
        rows.push((
            format!("Part {n}, under the models mount"),
            part.path.clone(),
        ));
        rows.push((format!("Part {n}, blob"), part.blob.clone()));
        rows.push((format!("Part {n}, SHA-256"), part.sha256.clone()));
        rows.push((format!("Part {n}, bytes"), part.bytes.to_string()));
    }
    rows
}

/// The box's machine as the tuple keeps it (Spec 6.2): the report read, the
/// image, the driver and CUDA runtime, each card, the guest CPU and the
/// machine.
fn machine_rows(b: &BoxMembers) -> Vec<(String, String)> {
    let mut rows = vec![
        (
            "Report".to_owned(),
            format!(
                "the {} stage's, taken {}",
                b.report.stage,
                b.report.taken.format("%Y-%m-%dT%H:%M:%SZ")
            ),
        ),
        ("Image version".to_owned(), b.image.version.clone()),
        (
            "Image manifest SHA-256".to_owned(),
            b.image.manifest_sha256.clone(),
        ),
        ("Driver".to_owned(), b.driver_version.clone()),
        ("CUDA runtime".to_owned(), b.cuda_runtime_version.clone()),
    ];
    for card in &b.cards {
        rows.push((
            format!("Card {}", card.ordinal),
            format!(
                "{}, {}, {}",
                card.model,
                card.pci_address,
                gib(card.memory_bytes)
            ),
        ));
    }
    rows.extend([
        (
            "Guest CPU".to_owned(),
            format!("{}, {}", b.cpu.vendor, b.cpu.model_name),
        ),
        (
            "CPU family, model and stepping".to_owned(),
            format!("{}, {}, {}", b.cpu.family, b.cpu.model, b.cpu.stepping),
        ),
        ("Logical CPUs".to_owned(), b.cpu.logical_cpus.to_string()),
        (
            "CPU flags".to_owned(),
            format!("{}: {}", b.cpu.flags.len(), b.cpu.flags.join(" ")),
        ),
        (
            "Virtualization".to_owned(),
            b.machine.virtualization.clone(),
        ),
        (
            "Machine".to_owned(),
            format!(
                "{}, {}, {}",
                b.machine.vendor, b.machine.product, b.machine.version
            ),
        ),
        ("Memory".to_owned(), gib(b.machine.memory_total_bytes)),
        (
            "Kernel release".to_owned(),
            b.machine.kernel_release.clone(),
        ),
    ]);
    rows
}

/// **Every member of the tuple as a pane**, in the Spec's order.
fn members_view(m: &Members) -> Vec<MemberView> {
    let one = |label: &str, value: String| vec![(label.to_owned(), value)];
    let box_waits = "the box's members, from the drives stage's report";
    vec![
        member(
            "The decoder",
            "the pull's record, verified",
            "the decoder's pull, verified",
            &m.decoder,
            model_rows,
        ),
        member(
            "The adapter",
            "the pull's record, verified",
            "the adapter's pull, verified",
            &m.adapter,
            model_rows,
        ),
        member(
            "The adapter's scale",
            "the setup, the adapter's pick",
            "the adapter's pick on the models page",
            &m.adapter_scale,
            |s| one("Scale", s.to_string()),
        ),
        member(
            "The base's identity",
            "the console's hash of the cached parts",
            "the console's hash of the decoder's cached parts",
            &m.base_blake3,
            |b| one("BLAKE3", b.clone()),
        ),
        member(
            "The context length",
            "the setup, bounded by the header",
            "a context length kept on the placement page",
            &m.context_length,
            |n| {
                vec![
                    ("Context length".to_owned(), n.to_string()),
                    (
                        "At least the box's max-tokens-per-turn".to_owned(),
                        m.max_tokens_per_turn.to_string(),
                    ),
                ]
            },
        ),
        member(
            "The turn context",
            "the setup",
            "a turn context kept on the placement page",
            &m.turn_context,
            |t| one("Turn context", t.clone()),
        ),
        member(
            "Virtual machine or host",
            "the setup's checkbox",
            "",
            &Ok::<_, String>(m.virtual_machine),
            |vm| {
                one(
                    "The box",
                    if *vm { "a virtual machine" } else { "a host" }.to_owned(),
                )
            },
        ),
        member(
            "The box's machine",
            "the drives stage's report",
            box_waits,
            &m.box_members,
            machine_rows,
        ),
        member(
            "The drives",
            "the drives stage's report",
            box_waits,
            &m.box_members,
            |b| {
                b.drives
                    .iter()
                    .map(|d| {
                        (
                            format!("The {} drive", d.role),
                            format!(
                                "{}, {}, {}, project quota {}",
                                d.label,
                                d.filesystem,
                                gib(d.bytes),
                                if d.project_quota { "on" } else { "off" }
                            ),
                        )
                    })
                    .collect()
            },
        ),
        member(
            "The devices",
            "the placement page, from the survey's report",
            "the devices chosen on the placement page, from the survey's report",
            &m.devices,
            |d| {
                one(
                    "Cards by ordinal, the shard order",
                    d.iter()
                        .map(|o| o.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            },
        ),
    ]
}

/// The tuple as confirmed, for the Confirm pane.
struct ConfirmedView {
    tuple_id: String,
    confirmed_by: String,
    confirmed_at: String,
}

#[derive(Template)]
#[template(path = "setup_confirm.html")]
struct ConfirmPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: crate::surfaces::Chrome,
    frame: Frame,
    version: i64,
    members: Vec<MemberView>,
    confirmed: Option<ConfirmedView>,
    /// Why Confirm holds, each member unread, where it does.
    unread: Vec<String>,
}

async fn page(store: Store, seams: &Seams, headers: HeaderMap, setup_id: &str) -> Response {
    let (session, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let chrome = match crate::surfaces::Chrome::of(&store, &session.person_id).await {
        Ok(chrome) => chrome,
        Err(e) => return fault(e),
    };
    // A confirmed setup is drawn from its tuple's row alone.
    let (members, confirmed) = if setup.state == "confirmed" {
        match store.deployment_tuple(setup_id).await {
            Ok(Some(c)) => {
                let t = c.tuple;
                (
                    Members {
                        decoder: Ok(t.decoder),
                        adapter: Ok(t.adapter),
                        adapter_scale: Ok(t.adapter_scale),
                        base_blake3: Ok(t.base_blake3),
                        context_length: Ok(t.context_length),
                        max_tokens_per_turn: t.max_tokens_per_turn,
                        turn_context: Ok(t.turn_context),
                        virtual_machine: t.virtual_machine,
                        devices: Ok(t.devices),
                        box_members: Ok(t.box_members),
                    },
                    Some(ConfirmedView {
                        tuple_id: c.tuple_id,
                        confirmed_by: c.confirmed_by,
                        confirmed_at: when(c.confirmed_at),
                    }),
                )
            }
            Ok(None) => {
                return fault(anyhow::anyhow!(
                    "the setup {setup_id} is confirmed and holds no tuple"
                ));
            }
            Err(e) => return fault(e),
        }
    } else {
        match store.members(&setup, seams.boxes.as_ref()).await {
            Ok(members) => (members, None),
            Err(e) => return fault(e),
        }
    };
    let unread = match members.clone().tuple() {
        Ok(_) => Vec::new(),
        Err(unread) => unread,
    };
    let rendered = ConfirmPage {
        here: "declarations",
        who: session.name.clone(),
        admin: true,
        chrome,
        frame: frame(&setup, Page::Confirm),
        version: setup.version,
        members: members_view(&members),
        confirmed,
        unread,
    }
    .render();
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

// ---- Confirm ----

/// **Confirm's form parsed to its shape**: the setup's version, alone.
fn confirm_ask(fields: &[(String, String)]) -> Result<i64, String> {
    match fields {
        [(name, version)] if name == "version" => version
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|v| *v >= 1)
            .ok_or_else(|| "a setup's version is 1 at least".to_owned()),
        _ => Err(
            "Confirm takes the setup's version alone: the tuple is read from the members' sources, never from the form"
                .to_owned(),
        ),
    }
}

async fn confirm(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    fields: Vec<(String, String)>,
) -> Response {
    if !is_setup_id(setup_id) {
        return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
    }
    let version = match confirm_ask(&fields) {
        Ok(version) => version,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let tuple_id = match store.mint_key("dt").await {
        Ok(tuple_id) => tuple_id,
        Err(e) => return fault(e),
    };
    let target = Target::Setup(setup_id);
    let session = match writer(&store, &seams.policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let setup = match store.setup(setup_id).await {
        Ok(Some(setup)) if setup.state == "open" => setup,
        Ok(Some(_)) => return refused(&Refusal::SetupConfirmed),
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    };
    match store.members(&setup, seams.boxes.as_ref()).await {
        Ok(members) => {
            if let Err(unread) = members.tuple() {
                return refused(&Refusal::Unconfirmable(unread.join(", ")));
            }
        }
        Err(e) => return fault(e),
    }
    let (setup_id, boxes) = (setup_id.to_owned(), seams.boxes.clone());
    step_four(&seams.policy, async move {
        let first = match first_of(&store, &session, Target::Setup(&setup_id), ACTION).await {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        let written = store
            .confirm(
                &session.person_id,
                session.session_id,
                &first,
                &setup_id,
                version,
                &tuple_id,
                boxes.as_ref(),
            )
            .await;
        settled(&store, &first, &written).await;
        landed(written, &setup_id, Page::Build)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::confirm_ask;

    /// **Confirm takes the version alone**: any member posted beside it, a
    /// version given twice and a version out of its shape are refused.
    #[test]
    fn confirm_takes_the_version_alone() {
        let f = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<Vec<_>>()
        };
        assert_eq!(confirm_ask(&f(&[("version", "4")])), Ok(4));
        for refused in [
            f(&[]),
            f(&[("version", "0")]),
            f(&[("version", "x")]),
            f(&[("version", "4"), ("version", "4")]),
            f(&[("version", "4"), ("devices", "0")]),
            f(&[("context_length", "1"), ("version", "4")]),
        ] {
            assert!(confirm_ask(&refused).is_err(), "{refused:?}");
        }
    }
}
