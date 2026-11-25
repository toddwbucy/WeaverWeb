//! **The panes a box's report feeds** (Spec 6.2): the report on the Pair
//! page, the directories page's directories as mounted with their
//! judgments, and the placement page's devices, each served read, refused
//! and waiting, with a box whose report each test sets through the `boxes`
//! seam. A refused report is drawn by its arrival and cause, and never its
//! text or the refusal's detail. A row kept under version 1's stage
//! `drives` is named on the Pair page and feeds no pane. A value a report
//! chose is drawn through the templates' escaping.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::box_report::{STATE_DRIVE_FLOOR, gib};
use crate::setup_models_tests::{rig_boxed, setup_of};
use crate::setup_place_tests::{
    Reported, VERSION_1, decoder_picked, laid_pairing_report, pairing_report, post, read,
};
use crate::setup_tests::{between, panes, walk};
use crate::survey::Cause;

const PAIRING_PANE: &str = "The box's report at the pairing stage";
const DRIVES_PANE: &str = "The directories as mounted";
const DEVICES_PANE: &str = "The devices";

/// A refused report: a format the reader does not know, cause 4.
/// A mebibyte, the margin case's shortfall.
const MIB: u64 = 1024 * 1024;

const REFUSED: &str = r#"{"format":"weaver-survey/9","stage":"pairing"}"#;

/// The test's laid report with one edit made.
fn edited(edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(&laid_pairing_report()).unwrap();
    edit(&mut value);
    value.to_string()
}

/// The walk of the Pair page: its panes carry `1 Declare`, a page under
/// New agent, the first page of stage 1 (the operator's ruling of
/// 2026-11-25).
fn pair_walk(page: &str) -> Vec<String> {
    crate::setup_tests::walk(page)
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

/// **The pairing stage's report is served in each status on the agent's
/// Pair page, read only, once the box is paired** (Spec 6.2, 8.2; the
/// operator's ruling of 2026-11-24: "After a pairing succeeds, the Pair
/// page shows the hardware report, read only"): an unpaired setup's Pair
/// page draws no report pane, and the directories page none of its own;
/// with no report the pane waits on it, saying what sends it; a refused report is drawn by its stage,
/// arrival and cause by number and words, with neither its text nor the
/// refusal's detail; a row kept under version 1's stage `drives` is named
/// by its arrival and feeds nothing; and a read report is drawn by its arrival and
/// `taken`, then its members grouped, the CPU's flags as a count with the
/// list folded, and each card by its ordinal, model, PCI address and memory.
/// A box with no card says so as a reading. Every status passes the walk,
/// and no state of the pane holds a form.
///
/// Perturbations: draw the pane whether or not the box is paired, and the
/// unpaired setup's Pair page draws it; read no report for the Pair page,
/// and the paired one draws none.
#[tokio::test]
async fn the_pairing_pages_report_is_served_waiting_refused_and_read() {
    let reported = Arc::new(Reported::default());
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, reported.clone()).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/pair/{setup}");
    let (_, _, unpaired) = crate::setup_tests::begin(rig.store(), &ada, "otto").await;
    *reported.pairing.lock().unwrap() = Some(pairing_report());
    let page = read(&rig.app, &format!("/pair/{}", unpaired.unwrap()), &ada).await;
    assert!(!page.contains(PAIRING_PANE), "unpaired: {page}");
    let page = read(&rig.app, &format!("/setup/{setup}/directories"), &ada).await;
    assert!(!page.contains(PAIRING_PANE), "the directories page: {page}");
    *reported.pairing.lock().unwrap() = None;

    let page = read(&rig.app, &uri, &ada).await;
    let waiting = pane(&page, PAIRING_PANE);
    assert_eq!(
        status(waiting),
        r#"<span class="status hold">waiting</span>"#
    );
    assert!(
        waiting.contains("No report has arrived at the pairing stage. A box brings its report with the pairing; this row was paired before the report crossed with it, and the appliance's setup script sends one with admin-con setup report."),
        "{waiting}"
    );
    assert!(
        waiting.contains("Waits on the box's report at the pairing stage"),
        "{waiting}"
    );
    assert!(!waiting.contains("<form"), "{waiting}");
    assert!(pair_walk(&page).is_empty(), "{:?}", pair_walk(&page));

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
        !page.contains("weaver-survey/9"),
        "neither the text nor the detail quoting it is drawn: {page}"
    );
    assert!(pair_walk(&page).is_empty(), "{:?}", pair_walk(&page));

    // A row kept under version 1's stage `drives` is named, and the
    // refused report stays refused.
    *reported.drives.lock().unwrap() = Some(VERSION_1.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let named = pane(&page, PAIRING_PANE);
    assert_eq!(status(named), r#"<span class="status fail">refused</span>"#);
    assert!(
        named.contains(r#"<span class="status hold">not read</span> A report kept under the stage drives, which <code>weaver-survey/2</code> does not have, arrived "#)
            && named.contains("It feeds nothing."),
        "{named}"
    );
    assert!(pair_walk(&page).is_empty(), "{:?}", pair_walk(&page));
    *reported.drives.lock().unwrap() = None;

    *reported.pairing.lock().unwrap() = Some(pairing_report());
    let page = read(&rig.app, &uri, &ada).await;
    let shown = pane(&page, PAIRING_PANE);
    assert_eq!(status(shown), r#"<span class="status ok">read</span>"#);
    assert!(!shown.contains("<form"), "read only: {shown}");
    // The survey's `image` members are the appliance's, as on Confirm.
    assert!(
        shown.contains("<caption>The appliance</caption>"),
        "{shown}"
    );
    assert!(!shown.contains("<caption>The image</caption>"), "{shown}");
    for row in [
        "taken 2026-11-17 09:00:00 UTC",
        "<tr><th>Version</th><td class=\"mono\">2.0</td></tr>",
        "<td class=\"mono\">03fcfb2e3fffc40f40d87b49d7f086d1c45c0c394a6794fbfdb1ce855502c1c8</td>",
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
    assert!(pair_walk(&page).is_empty(), "{:?}", pair_walk(&page));

    *reported.pairing.lock().unwrap() = Some(edited(|v| {
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
    *reported.pairing.lock().unwrap() = Some(edited(|v| {
        v["machine"]["product"] = json!(script);
        v["cards"][0]["name"] = json!(script);
    }));
    for uri in [
        format!("/pair/{setup}"),
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
    let page = read(&rig.app, &format!("/pair/{setup}"), &ada).await;
    let refused = pane(&page, PAIRING_PANE);
    assert!(refused.contains("Refused for cause 4: "), "{refused}");
    assert!(!page.contains("<script>alert"), "{page}");
    for escaped in ["&#60;script&#62;", "&lt;script&gt;"] {
        assert!(!page.contains(escaped), "the detail is not drawn: {page}");
    }
}

/// **The directories page draws the directories as mounted, each with its
/// judgments** (Spec 6.2), from the report: waiting where none has
/// arrived, refused by its cause, and read (the operator lays the disks
/// before pairing), each directory at the place the operator chose,
/// with its role, path, label, filesystem, size, free space and project
/// quota. One not mounted holds Confirm, naming it; a state directory one
/// byte under 9 GiB of filesystem holds Confirm by its exact size, and one
/// at the figure stands; a models directory whose free space is under the
/// picks' need is a warning, not a hold, on this page and the models page,
/// and one with room says so. A models directory whose filesystem is short
/// of the 10 GB margin and holds the need draws no warning, and neither
/// page names the margin (the operator's ruling of 2026-11-24: "No page
/// warns").
///
/// Perturbation: judge the margin again, and the short filesystem warns.
#[tokio::test]
async fn the_directories_page_draws_the_directories_as_mounted_with_their_judgments() {
    let reported = Arc::new(Reported::default());
    let Some((rig, _fake, _, _cache)) = rig_boxed(true, reported.clone()).await else {
        return;
    };
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/directories");

    let page = read(&rig.app, &uri, &ada).await;
    let waiting = pane(&page, DRIVES_PANE);
    assert_eq!(
        status(waiting),
        r#"<span class="status hold">waiting</span>"#
    );
    assert!(waiting.contains("Waits on the box's report."), "{waiting}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    // A row kept under version 1's stage `drives` alone leaves it waiting.
    *reported.drives.lock().unwrap() = Some(VERSION_1.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let still = pane(&page, DRIVES_PANE);
    assert_eq!(status(still), r#"<span class="status hold">waiting</span>"#);
    assert!(still.contains("<p>No report has arrived.</p>"), "{still}");

    *reported.pairing.lock().unwrap() = Some(REFUSED.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let refused = pane(&page, DRIVES_PANE);
    assert_eq!(
        status(refused),
        r#"<span class="status fail">refused</span>"#
    );
    assert!(refused.contains("Refused for cause 4: "), "{refused}");
    assert!(refused.contains("Confirm holds"), "{refused}");
    assert!(!page.contains("weaver-survey/9"), "{page}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(laid_pairing_report());
    let page = read(&rig.app, &uri, &ada).await;
    let shown = pane(&page, DRIVES_PANE);
    assert_eq!(status(shown), r#"<span class="status ok">read</span>"#);
    assert!(shown.contains("<p>The report, arrived "), "{shown}");
    let state = format!(
        "<tr><td>state</td><td class=\"mono\">/mnt/weaver/state</td><td class=\"mono\">weaver-state</td><td>ext4</td><td>{}</td><td>{}</td><td>on</td>",
        gib(25_191_575_552),
        gib(23_863_877_632)
    );
    let models = format!(
        "<tr><td>models</td><td class=\"mono\">/srv/weaver-models</td><td class=\"mono\">weaver-models</td><td>ext4</td><td>{}</td><td>{}</td><td>off</td>",
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

    *reported.pairing.lock().unwrap() = Some(edited(|v| {
        v["drives"][1] =
            json!({"role": "models", "mount_point": "/srv/weaver-models", "mounted": false});
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
        *reported.pairing.lock().unwrap() = Some(edited(|v| {
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
    let models_page = format!("/setup/{setup}/models");
    let need = need_shown(&read(&rig.app, &models_page, &ada).await);
    let margin = 10 * 1024 * 1024 * 1024u64;
    // The need in bytes, as the pages count it: the pick's one part here.
    let mut need_bytes = 0u64;
    for row in rig.store().setup_models(&setup).await.unwrap() {
        for file in rig.store().pull_files(&row.pull_id).await.unwrap() {
            need_bytes += file.file.size;
        }
    }
    for (bytes, free, judged_as) in [
        (135_089_524_736u64, 1u64, "warning"),
        // Short of the margin, and holding the need: no page warns (the
        // operator's ruling of 2026-11-24).
        (margin + need_bytes - MIB, margin + need_bytes - MIB, "room"),
        (135_089_524_736, 72_464_760_832, "room"),
    ] {
        *reported.pairing.lock().unwrap() = Some(edited(|v| {
            v["drives"][1]["bytes"] = json!(bytes);
            v["drives"][1]["free_bytes"] = json!(free);
        }));
        let warning = format!(
            r#"<span class="status hold">warning</span> Its free space is under the picks' need of {need}: a warning and not a hold, since the box's pull is an act of its own."#
        );
        let room = format!(
            r#"<span class="status ok">room</span> Its free space holds the picks' need of {need}."#
        );
        for (uri, heading) in [
            (&uri, DRIVES_PANE),
            (&models_page, "The models directory's need"),
        ] {
            let page = read(&rig.app, uri, &ada).await;
            let judged = pane(&page, heading);
            for (said, word) in [(&warning, "warning"), (&room, "room")] {
                assert_eq!(
                    judged.contains(said.as_str()),
                    word == judged_as,
                    "{uri}, {bytes} and {free}, {word}: {judged}"
                );
            }
            let lowered = page.to_lowercase();
            for never in ["margin", "10 gib", "10 gb"] {
                assert!(!lowered.contains(never), "{uri}, {bytes}: {never}: {page}");
            }
            assert!(walk(&page).is_empty(), "{:?}", walk(&page));
        }
    }

    // **A second run's report replaces the row** (the operator's ruling of
    // 2026-11-25): its models directory short of the need, so both pages
    // warn where the last report left room. A row kept under `drives`
    // changes neither.
    *reported.pairing.lock().unwrap() = Some(edited(|v| {
        v["drives"][1]["free_bytes"] = json!(1);
    }));
    *reported.drives.lock().unwrap() = Some(VERSION_1.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let later = pane(&page, DRIVES_PANE);
    assert!(
        later.contains(r#"<span class="status hold">warning</span>"#),
        "{later}"
    );
    let models = read(&rig.app, &models_page, &ada).await;
    let need_pane = pane(&models, "The models directory's need");
    assert!(
        need_pane.contains(r#"<span class="status hold">warning</span>"#),
        "{need_pane}"
    );

    // **Where the report does not read, the models page says which**: a
    // refused report by its cause, and no report though a `drives` row
    // stands.
    *reported.pairing.lock().unwrap() = Some(REFUSED.to_owned());
    let models = read(&rig.app, &models_page, &ada).await;
    let need_pane = pane(&models, "The models directory's need");
    assert!(
        need_pane.contains("<p>The report is refused for cause 4: ")
            && need_pane.contains(", so the need is not judged against the models directory.</p>"),
        "{need_pane}"
    );
    *reported.pairing.lock().unwrap() = None;
    let models = read(&rig.app, &models_page, &ada).await;
    let need_pane = pane(&models, "The models directory's need");
    assert!(
        need_pane.contains(
            "<p>No report has arrived, so the need is not judged against the models directory.</p>"
        ),
        "{need_pane}"
    );
}

/// The need as the models page's need pane states it.
fn need_shown(page: &str) -> String {
    let need = pane(page, "The models directory's need");
    let body = between(need, "<p>", ":").expect("the need is stated");
    body.to_owned()
}

/// **The placement page names a kept device a later report lacks** (Spec
/// 6.2), and its devices pane carries its report's status: waiting, refused
/// by its cause, and read. With cards 0 and 1 kept and a second run's
/// report carrying card 0 alone, card 1 is named, status fail, and Confirm
/// refuses it; a row kept under `drives` with two cards masks nothing.
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

    *reported.pairing.lock().unwrap() = Some(REFUSED.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let refused = pane(&page, DEVICES_PANE);
    assert_eq!(
        status(refused),
        r#"<span class="status fail">refused</span>"#
    );
    assert!(
        refused.contains("The report is refused for cause 4: ")
            && refused.contains(", so no card is offered."),
        "{refused}"
    );
    assert!(!refused.contains("name=\"device\""), "{refused}");
    assert!(walk(&page).is_empty(), "{:?}", walk(&page));

    *reported.pairing.lock().unwrap() = Some(pairing_report());
    let page = read(&rig.app, &uri, &ada).await;
    let from_report = pane(&page, DEVICES_PANE);
    assert_eq!(
        status(from_report),
        r#"<span class="status ok">read</span>"#
    );
    assert!(
        from_report.contains(r#"<span class="source">the report</span>"#),
        "{from_report}"
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

    // A second run's report with one card replaces the row.
    *reported.pairing.lock().unwrap() = Some(edited(|v| {
        v["cards"] = json!([v["cards"][0].clone()]);
    }));
    *reported.drives.lock().unwrap() = Some(VERSION_1.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let lacking = pane(&page, DEVICES_PANE);
    assert!(
        lacking.contains(r#"<span class="source">the report</span>"#),
        "{lacking}"
    );
    assert!(
        lacking.contains(r#"<span class="status fail">lacking</span> Card 1 is kept, and the report lacks it: Confirm refuses the placement until it is saved on the reported cards."#),
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

    // **A placement saved while the latest report does not read keeps the
    // devices stored before it**: a refused report arrives, no card is
    // offered, the pane says the kept devices stand, and a save of the turn
    // context leaves them. Confirm's devices line names the report's
    // state.
    *reported.pairing.lock().unwrap() = Some(REFUSED.to_owned());
    let page = read(&rig.app, &uri, &ada).await;
    let none_offered = pane(&page, DEVICES_PANE);
    assert!(!none_offered.contains("name=\"device\""), "{none_offered}");
    assert!(
        none_offered.contains("The kept devices, cards 0, 1, stand as stored, and a save here leaves them; Confirm checks them once a report reads."),
        "{none_offered}"
    );
    let version = rig.store().setup(&setup).await.unwrap().unwrap().version;
    let (status_code, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", &version.to_string()),
            ("turn_context", "independent"),
        ],
    )
    .await;
    assert_eq!(status_code, StatusCode::SEE_OTHER, "{body}");
    let kept = rig.store().setup(&setup).await.unwrap().unwrap();
    assert_eq!(kept.turn_context.as_deref(), Some("independent"));
    assert_eq!(kept.devices, Some(vec![0, 1]), "the kept devices stand");
    let confirm = read(&rig.app, &format!("/setup/{setup}/confirm"), &ada).await;
    assert!(
        confirm.contains("the report is refused for cause 4"),
        "{confirm}"
    );
    assert!(confirm.contains("so no card is read"), "{confirm}");
    assert!(
        !confirm.contains("the survey has reported no cards"),
        "{confirm}"
    );
}
