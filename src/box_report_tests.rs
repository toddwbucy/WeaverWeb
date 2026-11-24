//! **The source's reading of the kept reports** (Spec 6.2): which report
//! feeds the cards and which the box's members, the holds Confirm takes
//! from the latest report of either stage, and the state drive's figure. No store:
//! `Reports::of` reads rows as the store hands them over.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};

use crate::box_report::{Card, Report, Reports, STATE_DRIVE_FLOOR};
use crate::survey::{Cause, MODELS_MOUNT, STATE_MOUNT, Stage};

const EXAMPLE: &str = include_str!("../tests/fixtures/weaver-survey-1-worked-example.json");

/// What a 10 GiB disk formatted ext4 with its defaults reports as its
/// filesystem's size, measured on 2026-11-17: 2,554,693 blocks of 4 KiB,
/// the 2,621,440 less 66,747 of overhead, about 9.75 GiB.
const TEN_GIB_DISK_EXT4: u64 = 2_554_693 * 4096;

fn arrived() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 17, 9, 5, 0).unwrap()
}

fn edited(edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(EXAMPLE).unwrap();
    edit(&mut value);
    serde_json::to_string(&value).unwrap()
}

/// The worked example at the pairing stage: both drives not mounted, and one
/// card, so a test can tell its cards from the drives report's two.
fn pairing() -> String {
    edited(|v| {
        v["stage"] = json!("pairing");
        v["cards"] = json!([v["cards"][0].clone()]);
        v["drives"] = json!([
            {"role": "state", "mount_point": STATE_MOUNT, "mounted": false},
            {"role": "models", "mount_point": MODELS_MOUNT, "mounted": false},
        ]);
    })
}

/// The setup's kept reports as the store hands them over: stage, the
/// verbatim report, its arrival.
fn kept(pairing: Option<String>, drives: Option<String>) -> Reports {
    let mut rows = Vec::new();
    if let Some(report) = pairing {
        rows.push(("pairing".to_owned(), report, arrived()));
    }
    if let Some(report) = drives {
        rows.push(("drives".to_owned(), report, arrived()));
    }
    Reports::of(&rows)
}

fn ordinals(cards: &[Card]) -> Vec<i32> {
    cards.iter().map(|c| c.ordinal).collect()
}

#[test]
fn the_cards_are_the_drives_reports_where_it_reads_else_the_pairing_reports() {
    let both = kept(Some(pairing()), Some(EXAMPLE.to_owned()));
    let cards = both.cards().unwrap();
    assert_eq!(ordinals(&cards), vec![0, 1]);
    assert_eq!(cards[1].model, "NVIDIA RTX A5000");
    let pairing_alone = kept(Some(pairing()), None);
    assert_eq!(ordinals(&pairing_alone.cards().unwrap()), vec![0]);
    let refused_drives = kept(Some(pairing()), Some("{}".to_owned()));
    assert_eq!(ordinals(&refused_drives.cards().unwrap()), vec![0]);
    assert_eq!(kept(None, None).cards(), None);
    assert_eq!(kept(Some("not json".to_owned()), None).cards(), None);
}

#[test]
fn the_box_members_are_the_drives_reports_typed_as_ruled() {
    let members = kept(Some(pairing()), Some(EXAMPLE.to_owned()))
        .members()
        .unwrap();
    assert_eq!(members.report.stage, "drives");
    assert_eq!(
        members.report.taken.to_rfc3339(),
        "2026-11-17T09:00:00+00:00"
    );
    assert_eq!(members.image.version, "2.0");
    assert_eq!(
        members.image.manifest_sha256,
        "5b1f0c9e7a2d4486b3e0f19c2d7a65e8c41f03b9d2a7e6158c0b94f3a1d2e7c6"
    );
    assert_eq!(members.driver_version, "620.70.06");
    assert_eq!(members.cuda_runtime_version, "13.4.92");
    assert_eq!(members.cards.len(), 2);
    assert_eq!(members.cards[1].ordinal, 1);
    assert_eq!(members.cards[1].model, "NVIDIA RTX A5000");
    assert_eq!(members.cards[1].pci_address, "0000:07:00.0");
    assert_eq!(members.cards[1].memory_bytes, 25_757_220_864);
    assert_eq!(members.cpu.vendor, "AuthenticAMD");
    assert_eq!(members.cpu.model_name, "AMD EPYC 9354P 32-Core Processor");
    assert_eq!(
        (members.cpu.family, members.cpu.model, members.cpu.stepping),
        (25, 17, 1)
    );
    assert_eq!(members.cpu.logical_cpus, 16);
    assert_eq!(members.cpu.flags.len(), 20);
    assert_eq!(members.machine.virtualization, "kvm");
    assert_eq!(members.machine.vendor, "QEMU");
    assert_eq!(members.machine.product, "Standard PC (Q35 + ICH9, 2009)");
    assert_eq!(members.machine.version, "pc-q35-9.2");
    assert_eq!(members.machine.memory_total_bytes, 33_071_620_096);
    assert_eq!(members.machine.kernel_release, "7.3.4-arch1-2");
    let drives: Vec<_> = members
        .drives
        .iter()
        .map(|d| {
            (
                d.role.as_str(),
                d.label.as_str(),
                d.filesystem.as_str(),
                d.bytes,
                d.project_quota,
            )
        })
        .collect();
    assert_eq!(
        drives,
        vec![
            ("state", "weaver-state", "ext4", 25_191_575_552, true),
            ("models", "weaver-models", "ext4", 135_089_524_736, false),
        ]
    );
    let stored = serde_json::to_value(&members).unwrap();
    for moment in ["free_bytes", "loaded", "mount_point"] {
        assert!(
            !stored.to_string().contains(moment),
            "the tuple takes no {moment}: {stored}"
        );
    }
}

/// The setup's kept reports, each with its own arrival.
fn kept_at(rows: &[(&str, String, chrono::DateTime<Utc>)]) -> Reports {
    let rows: Vec<_> = rows
        .iter()
        .map(|(stage, report, at)| ((*stage).to_owned(), report.clone(), *at))
        .collect();
    Reports::of(&rows)
}

/// The pairing stage's report of a box whose disks were laid before
/// pairing (the operator's ruling of 2026-11-23): both drives mounted.
fn laid_pairing() -> String {
    edited(|v| v["stage"] = json!("pairing"))
}

/// **Confirm reads the latest report of either stage, and holds naming
/// it** (the Planner's ruling of 2026-11-24 on the plan): a pairing-stage
/// report alone gives the box's members, a drives-stage report arriving
/// later is read in its place, a pairing-stage report arriving after a
/// drives-stage one is read in its, and a tie goes to the drives stage.
/// Where no report has arrived, or the latest is refused or misfiled,
/// Confirm holds naming it, a refused latest never masked by an earlier
/// report that reads.
#[test]
fn confirm_reads_the_latest_report_of_either_stage_and_holds_naming_it() {
    let (early, late) = (arrived(), arrived() + chrono::Duration::minutes(5));
    let stage = |reports: Reports| reports.members().unwrap().report.stage;
    assert_eq!(
        stage(kept_at(&[("pairing", laid_pairing(), early)])),
        "pairing"
    );
    assert_eq!(
        stage(kept_at(&[
            ("pairing", laid_pairing(), early),
            ("drives", EXAMPLE.to_owned(), late),
        ])),
        "drives"
    );
    assert_eq!(
        stage(kept_at(&[
            ("drives", EXAMPLE.to_owned(), early),
            ("pairing", laid_pairing(), late),
        ])),
        "pairing"
    );
    assert_eq!(
        stage(kept_at(&[
            ("pairing", laid_pairing(), early),
            ("drives", EXAMPLE.to_owned(), early),
        ])),
        "drives"
    );
    let waiting = kept(None, None).members().unwrap_err();
    assert_eq!(waiting, "no report of either stage has arrived");
    // A pairing-stage report with its drives not mounted holds, naming them.
    let unlaid = kept(Some(pairing()), None).members().unwrap_err();
    assert!(unlaid.contains("is not mounted"), "{unlaid}");
    let refused = kept_at(&[
        ("pairing", laid_pairing(), early),
        ("drives", "{}".to_owned(), late),
    ])
    .members()
    .unwrap_err();
    assert!(
        refused.contains("the drives stage's report, the latest, is refused for cause 4"),
        "{refused}"
    );
    let misfiled = kept(None, Some(pairing())).members().unwrap_err();
    assert!(
        misfiled
            .contains("the report kept as the drives stage's, the latest, names its stage pairing"),
        "{misfiled}"
    );
    let reports = kept(None, Some(pairing()));
    assert!(matches!(
        reports.drives,
        Report::Misfiled {
            names: Stage::Pairing,
            ..
        }
    ));
    assert!(matches!(
        kept(None, Some("{}".to_owned())).drives,
        Report::Refused { ref refused, .. } if refused.cause == Cause::Format
    ));
}

#[test]
fn confirm_holds_for_a_drive_not_mounted_naming_it() {
    let models_unmounted = edited(|v| {
        v["drives"][1] = json!({"role": "models", "mount_point": MODELS_MOUNT, "mounted": false});
    });
    let held = kept(None, Some(models_unmounted)).members().unwrap_err();
    assert!(held.contains("the models drive"), "{held}");
    assert!(held.contains("not mounted"), "{held}");
    assert!(!held.contains("state"), "{held}");
}

#[test]
fn the_state_drive_holds_confirm_under_9_gib_of_filesystem_and_not_at_it() {
    assert_eq!(STATE_DRIVE_FLOOR, 9_663_676_416);
    let state_of = |bytes: u64| {
        let report = edited(|v| v["drives"][0]["bytes"] = json!(bytes));
        kept(None, Some(report)).members()
    };
    let held = state_of(STATE_DRIVE_FLOOR - 1).unwrap_err();
    assert!(held.contains("the state drive"), "{held}");
    assert!(held.contains("under 9 GiB"), "{held}");
    assert!(state_of(STATE_DRIVE_FLOOR).is_ok());
    assert!(
        state_of(TEN_GIB_DISK_EXT4).is_ok(),
        "a 10 GiB disk's ext4 size"
    );
    let small_models = edited(|v| v["drives"][1]["bytes"] = json!(STATE_DRIVE_FLOOR - 1));
    assert!(
        kept(None, Some(small_models)).members().is_ok(),
        "the figure is the state drive's"
    );
}
