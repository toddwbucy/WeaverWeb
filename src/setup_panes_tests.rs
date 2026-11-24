//! **The panes a box's report feeds** (Spec 6.2): the pairing page's report,
//! the drives page's drives as mounted with their three judgments, and the
//! placement page's devices, each served read, refused, misfiled and waiting,
//! with a box whose reports each test sets through the `boxes` seam. A
//! refused report is drawn by its stage, arrival and cause, and never its
//! text or the refusal's detail. A value a report chose is drawn through the
//! templates' escaping.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::box_report::{STATE_DRIVE_FLOOR, gib};
use crate::setup_models_tests::{rig_boxed, setup_of};
use crate::setup_place_tests::{Reported, decoder_picked, post, read, report};
use crate::setup_tests::{between, panes, walk};
use crate::survey::{Cause, MODELS_MOUNT};

const PAIRING_PANE: &str = "The box's report at the pairing stage";
const DRIVES_PANE: &str = "The drives as mounted";
const DEVICES_PANE: &str = "The devices";

/// A refused report: a format the reader does not know, cause 4.
const REFUSED: &str = r#"{"format":"weaver-survey/2","stage":"pairing"}"#;

/// The test's report at `stage` with one edit made.
fn edited(stage: &str, edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(&report(stage)).unwrap();
    edit(&mut value);
    value.to_string()
}

/// The pane of `page` headed `heading`, whole.
pub(crate) fn pane<'a>(page: &'a str, heading: &str) -> &'a str {
    panes(page)
        .into_iter()
        .find(|p| between(p, "<h2>", "</h2>") == Some(heading))
        .unwrap_or_else(|| panic!("no pane {heading}: {page}"))
}

/// The status mark in a pane's header.
pub(crate) fn status(pane: &str) -> String {
    let header = between(pane, "<header>", "</header>").unwrap();
    let at = header
        .find(r#"<span class="status "#)
        .expect("a status mark");
    let rest = &header[at..];
    rest[..rest.find("</span>").unwrap() + "</span>".len()].to_owned()
}

/// **The pairing stage's report is served in each status** (Spec 6.2), on
/// the drives page since the report left the pairing: with
/// no report the pane waits on it; a refused report is drawn by its stage,
/// arrival and cause by number and words, with neither its text nor the
/// refusal's detail; a report kept as the pairing stage's that names the
/// drives stage is misfiled; and a read report is drawn by its arrival and
/// `taken`, then its members grouped, the CPU's flags as a count with the
/// list folded, and each card by its ordinal, model, PCI address and memory.
/// A box with no card says so as a reading. Every status passes the walk.
#[tokio::test]
async fn the_pairing_pages_report_is_served_waiting_refused_misfiled_and_read() {
    let reported = Arc::new(Reported::default());
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, reported.clone()).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/drives");

    let page = read(&rig.app, &uri, &ada).await;
    let waiting = pane(&page, PAIRING_PANE);
    assert_eq!(
        status(waiting),
        r#"<span class="status hold">waiting</span>"#
    );
    assert!(
        waiting.contains("Waits on the box's report at the pairing stage"),
        "{waiting}"
    );
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(REFUSED.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let refused = pane(&page, PAIRING_PANE);
    assert_eq!(
        status(refused),
        r#"<span class="status fail">refused</span>"#
    );
    assert!(refused.contains("Refused for cause 4: "), "{refused}");
    assert!(refused.contains(Cause::Format.words()), "{refused}");
    assert!(refused.contains("Arrived "), "{refused}");
    assert!(
        !page.contains("weaver-survey/2"),
        "neither the text nor the detail quoting it is drawn: {page}"
    );
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(report("drives"));
    let page = read(&rig.app, &uri, &ada).await;
    let misfiled = pane(&page, PAIRING_PANE);
    assert_eq!(
        status(misfiled),
        r#"<span class="status fail">misfiled</span>"#
    );
    assert!(
        misfiled.contains("The report kept as the pairing stage's names its stage drives."),
        "{misfiled}"
    );
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(report("pairing"));
    let page = read(&rig.app, &uri, &ada).await;
    let shown = pane(&page, PAIRING_PANE);
    assert_eq!(status(shown), r#"<span class="status ok">read</span>"#);
    for row in [
        "taken 2026-11-17 09:00:00 UTC",
        "<tr><th>Version</th><td class=\"mono\">2.0</td></tr>",
        "<td class=\"mono\">5b1f0c9e7a2d4486b3e0f19c2d7a65e8c41f03b9d2a7e6158c0b94f3a1d2e7c6</td>",
        "<tr><th>Release</th><td class=\"mono\">7.3.4-arch1-2</td></tr>",
        "<tr><th>Virtualization</th><td>kvm</td></tr>",
        "<tr><th>Vendor</th><td>machine-vendor</td></tr>",
        "<tr><th>Product</th><td>machine-1</td></tr>",
        "<tr><th>Version</th><td>pc-q35-9.2</td></tr>",
        "<tr><th>Vendor</th><td>cpu-vendor</td></tr>",
        "<tr><th>Model name</th><td>cpu-1</td></tr>",
        "<tr><th>Family, model, stepping</th><td>25, 17, 1</td></tr>",
        "<tr><th>Logical CPUs</th><td>16</td></tr>",
        "<summary>20 flags</summary>",
        "avx512f",
        "<tr><th>Version</th><td class=\"mono\">620.70.06</td></tr>",
        "<tr><th>Loaded</th><td>loaded</td></tr>",
        "<tr><th>Version</th><td class=\"mono\">13.4.92</td></tr>",
    ] {
        assert!(shown.contains(row), "{row}: {shown}");
    }
    assert!(
        shown.contains(&format!(
            "<tr><th>Total</th><td>{}</td></tr>",
            gib(33_071_620_096)
        )),
        "{shown}"
    );
    for (ordinal, model, address) in [(0, "card-a", "0000:06:00.0"), (1, "card-b", "0000:07:00.0")]
    {
        let row = format!(
            "<tr><td>{ordinal}</td><td>{model}</td><td class=\"mono\">{address}</td><td>{}</td></tr>",
            gib(25_757_220_864)
        );
        assert!(shown.contains(&row), "{row}: {shown}");
    }
    assert!(shown.contains("Arrived "), "{shown}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(edited("pairing", |v| {
        v["cards"] = json!([]);
        v["driver"]["loaded"] = json!(false);
    }));
    let page = read(&rig.app, &uri, &ada).await;
    let none = pane(&page, PAIRING_PANE);
    assert_eq!(status(none), r#"<span class="status ok">read</span>"#);
    assert!(
        none.contains("The box reported no card: a reading, not a fault."),
        "{none}"
    );
    assert!(
        none.contains("<tr><th>Loaded</th><td>not loaded</td></tr>"),
        "{none}"
    );
}

/// **A value a report chose is drawn through the templates' escaping**
/// (Spec 6.2): a read report whose machine's product and a card's model
/// hold a script element draws them escaped, as askama escapes `<` and
/// `>` (`&#60;` and `&#62;`), and no script element stands on the page. A report whose `format` holds one is refused for cause 4,
/// and neither the value nor the refusal's detail quoting it is drawn,
/// escaped or not.
#[tokio::test]
async fn a_value_a_report_chose_is_drawn_escaped() {
    let reported = Arc::new(Reported::default());
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, reported.clone()).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let script = "<script>alert(1)</script>";
    *reported.pairing.lock().unwrap() = Some(edited("pairing", |v| {
        v["machine"]["product"] = json!(script);
        v["cards"][0]["name"] = json!(script);
    }));
    for uri in [
        format!("/setup/{setup}/drives"),
        format!("/setup/{setup}/placement"),
    ] {
        let page = read(&rig.app, &uri, &ada).await;
        assert!(
            page.contains("&#60;script&#62;alert(1)&#60;"),
            "{uri}: {page}"
        );
        assert!(!page.contains("<script>alert"), "{uri}: {page}");
    }

    *reported.pairing.lock().unwrap() =
        Some(r#"{"format":"<script>alert(1)</script>"}"#.to_owned());
    let page = read(&rig.app, &format!("/setup/{setup}/drives"), &ada).await;
    let refused = pane(&page, PAIRING_PANE);
    assert!(refused.contains("Refused for cause 4: "), "{refused}");
    assert!(!page.contains("<script>alert"), "{page}");
    for escaped in ["&#60;script&#62;", "&lt;script&gt;"] {
        assert!(!page.contains(escaped), "the detail is not drawn: {page}");
    }
}

/// **The drives page draws the drives as mounted, each with its judgments**
/// (Spec 6.2): waiting on the drives stage's report, refused by its cause,
/// and read, each drive with its role, mount point, label, filesystem, size,
/// free space and project quota. A drive not mounted holds Confirm, naming
/// it; a state drive one byte under 9 GiB of filesystem holds Confirm by its
/// exact size, and one at the figure stands; a models drive whose free space
/// is under the picks' need is a warning, not a hold, and one with room says
/// so.
#[tokio::test]
async fn the_drives_page_draws_the_drives_as_mounted_with_their_three_judgments() {
    let reported = Arc::new(Reported::default());
    let Some((rig, _fake, _, _cache)) = rig_boxed(true, reported.clone()).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/drives");

    let page = read(&rig.app, &uri, &ada).await;
    let waiting = pane(&page, DRIVES_PANE);
    assert_eq!(
        status(waiting),
        r#"<span class="status hold">waiting</span>"#
    );
    assert!(
        waiting.contains("Waits on the box's report at the drives stage"),
        "{waiting}"
    );
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.drives.lock().unwrap() = Some(REFUSED.replace("pairing", "drives"));
    let page = read(&rig.app, &uri, &ada).await;
    let refused = pane(&page, DRIVES_PANE);
    assert_eq!(
        status(refused),
        r#"<span class="status fail">refused</span>"#
    );
    assert!(refused.contains("Refused for cause 4: "), "{refused}");
    assert!(refused.contains("Confirm holds"), "{refused}");
    assert!(!page.contains("weaver-survey/2"), "{page}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.drives.lock().unwrap() = Some(report("drives"));
    let page = read(&rig.app, &uri, &ada).await;
    let shown = pane(&page, DRIVES_PANE);
    assert_eq!(status(shown), r#"<span class="status ok">read</span>"#);
    let state = format!(
        "<tr><td>state</td><td class=\"mono\">/mnt/weaver/state</td><td class=\"mono\">weaver-state</td><td>ext4</td><td>{}</td><td>{}</td><td>on</td>",
        gib(25_191_575_552),
        gib(23_863_877_632)
    );
    let models = format!(
        "<tr><td>models</td><td class=\"mono\">/mnt/weaver/models</td><td class=\"mono\">weaver-models</td><td>ext4</td><td>{}</td><td>{}</td><td>off</td>",
        gib(135_089_524_736),
        gib(72_464_760_832)
    );
    assert!(shown.contains(&state), "{state}: {shown}");
    assert!(shown.contains(&models), "{models}: {shown}");
    assert!(
        shown.contains(r#"<span class="status ok">at least 9 GiB</span>"#),
        "{shown}"
    );
    assert!(
        shown.contains("No model is picked, so its free space is not judged."),
        "{shown}"
    );
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.drives.lock().unwrap() = Some(edited("drives", |v| {
        v["drives"][1] = json!({"role": "models", "mount_point": MODELS_MOUNT, "mounted": false});
    }));
    let page = read(&rig.app, &uri, &ada).await;
    let unmounted = pane(&page, DRIVES_PANE);
    assert!(
        unmounted.contains(
            r#"<span class="status fail">not mounted</span> Confirm holds until the box mounts it."#
        ),
        "{unmounted}"
    );

    for (bytes, holds) in [(STATE_DRIVE_FLOOR - 1, true), (STATE_DRIVE_FLOOR, false)] {
        *reported.drives.lock().unwrap() = Some(edited("drives", |v| {
            v["drives"][0]["bytes"] = json!(bytes);
            v["drives"][0]["free_bytes"] = json!(bytes / 2);
        }));
        let page = read(&rig.app, &uri, &ada).await;
        let judged = pane(&page, DRIVES_PANE);
        let under = format!(
            r#"<span class="status fail">under 9 GiB</span> Its filesystem is {bytes} bytes, under 9 GiB (9663676416 bytes): Confirm holds."#
        );
        assert_eq!(judged.contains(&under), holds, "{bytes}: {judged}");
        assert_eq!(
            judged.contains(r#"<span class="status ok">at least 9 GiB</span>"#),
            !holds,
            "{bytes}: {judged}"
        );
    }

    decoder_picked(&rig, &ada, &setup, "1").await;
    let need = need_shown(&read(&rig.app, &uri, &ada).await);
    for (free, warned) in [(1u64, true), (72_464_760_832, false)] {
        *reported.drives.lock().unwrap() = Some(edited("drives", |v| {
            v["drives"][1]["free_bytes"] = json!(free);
        }));
        let page = read(&rig.app, &uri, &ada).await;
        let judged = pane(&page, DRIVES_PANE);
        let warning = format!(
            r#"<span class="status hold">warning</span> Its free space is under the picks' need of {need}: a warning and not a hold, since the box's pull is an act of its own."#
        );
        let room = format!(
            r#"<span class="status ok">room</span> Its free space holds the picks' need of {need}."#
        );
        assert_eq!(judged.contains(&warning), warned, "{free}: {judged}");
        assert_eq!(judged.contains(&room), !warned, "{free}: {judged}");
        assert!(walk(&page).is_empty(), "{:?}", walk(&page));
    }
}

/// The need as the drives page's need pane states it.
fn need_shown(page: &str) -> String {
    let need = pane(page, "The models drive's need");
    let body = between(need, "<p>", ":").expect("the need is stated");
    body.to_owned()
}

/// **The placement page names a kept device a later report lacks** (Spec
/// 6.2), and its devices pane carries its report's status: read from the
/// pairing stage's report, then the drives stage's, refused naming the
/// drives stage's report where it is refused and the pairing stage's has
/// not arrived, and waiting where neither has. With cards 0 and 1 kept and
/// a drives report carrying card 0 alone, card 1 is named, status fail, and
/// Confirm refuses it.
#[tokio::test]
async fn the_placement_page_names_a_kept_device_a_later_report_lacks() {
    let reported = Arc::new(Reported::default());
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, reported.clone()).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/placement");

    let page = read(&rig.app, &uri, &ada).await;
    let waiting = pane(&page, DEVICES_PANE);
    assert_eq!(
        status(waiting),
        r#"<span class="status hold">waiting</span>"#
    );
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.drives.lock().unwrap() = Some(REFUSED.replace("pairing", "drives"));
    let page = read(&rig.app, &uri, &ada).await;
    let refused = pane(&page, DEVICES_PANE);
    assert_eq!(
        status(refused),
        r#"<span class="status fail">refused</span>"#
    );
    assert!(
        refused.contains("The drives stage's report is refused for cause 4: "),
        "{refused}"
    );
    assert!(!refused.contains("name=\"device\""), "{refused}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(report("pairing"));
    let page = read(&rig.app, &uri, &ada).await;
    let from_pairing = pane(&page, DEVICES_PANE);
    assert_eq!(
        status(from_pairing),
        r#"<span class="status ok">read</span>"#
    );
    assert!(
        from_pairing.contains(r#"<span class="source">the pairing stage's report</span>"#),
        "{from_pairing}"
    );
    let (status_code, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", "1"),
            ("turn_context", "continuous"),
            ("device", "0"),
            ("device", "1"),
        ],
    )
    .await;
    assert_eq!(status_code, StatusCode::SEE_OTHER, "{body}");

    *reported.drives.lock().unwrap() = Some(edited("drives", |v| {
        v["cards"] = json!([v["cards"][0].clone()]);
    }));
    let page = read(&rig.app, &uri, &ada).await;
    let lacking = pane(&page, DEVICES_PANE);
    assert!(
        lacking.contains(r#"<span class="source">the drives stage's report</span>"#),
        "{lacking}"
    );
    assert!(
        lacking.contains(r#"<span class="status fail">lacking</span> Card 1 is kept, and the drives stage's report lacks it: Confirm refuses the placement until it is saved on the reported cards."#),
        "{lacking}"
    );
    assert!(
        lacking.contains(r#"name="device" value="0" checked>"#),
        "{lacking}"
    );
    assert!(!lacking.contains(r#"value="1""#), "{lacking}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));
    let confirm = read(&rig.app, &format!("/setup/{setup}/confirm"), &ada).await;
    assert!(
        confirm.contains("card 1 is not among the survey's cards"),
        "{confirm}"
    );
}
