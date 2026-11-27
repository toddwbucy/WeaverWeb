//! **The source's reading of the kept report** (Spec 6.2): the one report
//! of version 2 feeds the cards and the box's members, the holds Confirm
//! takes from it, a row kept under version 1's stage `drives` named and
//! feeding nothing, and the state drive's figure. No store: `Reports::of`
//! reads rows as the store hands them over.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::{Value, json};

use crate::box_report::{Card, Report, Reports, STATE_DRIVE_FLOOR};
use crate::survey::Cause;

const EXAMPLE: &str = include_str!("../tests/fixtures/weaver-survey-2-worked-example.json");

/// Version 1's worked example, as a row kept under the stage `drives`.
const VERSION_1: &str = include_str!("../tests/fixtures/weaver-survey-1-worked-example.json");

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

/// The worked example with both drives not mounted, at the two places the
/// image proof gives, and one card.
fn unlaid() -> String {
    edited(|v| {
        v["cards"] = json!([v["cards"][0].clone()]);
        v["drives"] = json!([
            {"role": "state", "mount_point": "/mnt/weaver/state", "mounted": false},
            {"role": "models", "mount_point": "/mnt/weaver/models", "mounted": false},
        ]);
    })
}

/// The setup's kept rows as the store hands them over: the report under
/// `pairing`, and a row under `drives` where one was kept before version 2.
fn kept(pairing: Option<String>, drives: Option<String>) -> Reports {
    let mut rows = Vec::new();
    if let Some(report) = pairing {
        rows.push(("pairing".to_owned(), report, arrived()));
    }
    if let Some(report) = drives {
        rows.push((
            "drives".to_owned(),
            report,
            arrived() + chrono::Duration::minutes(5),
        ));
    }
    Reports::of(&rows)
}

fn ordinals(cards: &[Card]) -> Vec<i32> {
    cards.iter().map(|c| c.ordinal).collect()
}

/// **The cards are the report's**, as the box's members are, so one tuple
/// stands on one report; none where it does not read. A row kept under
/// `drives`, though it arrived later and reads under version 1, feeds no
/// card.
#[test]
fn the_cards_are_the_reports_and_a_drives_row_feeds_none() {
    let both = kept(Some(EXAMPLE.to_owned()), Some(VERSION_1.to_owned()));
    let cards = both.cards().unwrap();
    assert_eq!(ordinals(&cards), vec![0, 1]);
    assert_eq!(cards[1].model, "NVIDIA RTX A5000");
    assert_eq!(
        ordinals(&kept(Some(unlaid()), None).cards().unwrap()),
        vec![0]
    );
    let drives_alone = kept(None, Some(EXAMPLE.to_owned()));
    assert_eq!(drives_alone.cards(), None, "a drives row is never read");
    assert_eq!(
        drives_alone.cards_or_why().unwrap_err(),
        "no report has arrived, so no card is read"
    );
    assert_eq!(kept(None, None).cards(), None);
    let refused = kept(Some("not json".to_owned()), None);
    assert_eq!(refused.cards(), None);
    assert!(
        refused
            .cards_or_why()
            .unwrap_err()
            .starts_with("the report is refused for cause 1:"),
    );
}

/// **A row kept under `drives` is named by its arrival alone** (the
/// format's section 7: version 2 writes no such stage).
#[test]
fn a_drives_row_is_named_by_its_arrival_and_read_by_nothing() {
    let reports = kept(Some(EXAMPLE.to_owned()), Some("{}".to_owned()));
    assert_eq!(
        reports.drives_kept,
        Some(arrived() + chrono::Duration::minutes(5))
    );
    assert!(matches!(reports.report, Report::Read { .. }));
    assert_eq!(kept(Some(EXAMPLE.to_owned()), None).drives_kept, None);
}

#[test]
fn the_box_members_are_the_reports_typed_as_ruled() {
    let members = kept(Some(EXAMPLE.to_owned()), None).members().unwrap();
    assert_eq!(members.report.stage, "pairing");
    assert_eq!(
        members.report.taken.to_rfc3339(),
        "2026-11-17T09:00:00+00:00"
    );
    assert_eq!(members.image.version, "2.0");
    assert_eq!(
        members.image.manifest_sha256,
        "03fcfb2e3fffc40f40d87b49d7f086d1c45c0c394a6794fbfdb1ce855502c1c8"
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
    for moment in [
        "free_bytes",
        "loaded",
        "mount_point",
        "network",
        "prefix_length",
    ] {
        assert!(
            !stored.to_string().contains(moment),
            "the tuple takes no {moment}: {stored}"
        );
    }
}

/// **Confirm reads the report, and holds naming why** where none has
/// arrived, the report is refused, or a drive is not mounted. A row kept
/// under `drives` neither stands in for a missing report nor masks a
/// refused one.
#[test]
fn confirm_reads_the_report_and_holds_naming_why() {
    let members = kept(Some(EXAMPLE.to_owned()), None).members().unwrap();
    assert_eq!(members.report.stage, "pairing");
    let waiting = kept(None, Some(EXAMPLE.to_owned())).members().unwrap_err();
    assert_eq!(waiting, "no report has arrived");
    let unlaid = kept(Some(unlaid()), None).members().unwrap_err();
    assert!(
        unlaid.contains("the state drive at /mnt/weaver/state is not mounted"),
        "{unlaid}"
    );
    let refused = kept(Some("{}".to_owned()), Some(VERSION_1.to_owned()))
        .members()
        .unwrap_err();
    assert!(
        refused.starts_with("the report is refused for cause 4"),
        "{refused}"
    );
    assert!(matches!(
        kept(Some(VERSION_1.to_owned()), None).report,
        Report::Refused { ref refused, .. } if refused.cause == Cause::Format
    ));
}

#[test]
fn confirm_holds_for_a_drive_not_mounted_naming_it() {
    let models_unmounted = edited(|v| {
        v["drives"][1] =
            json!({"role": "models", "mount_point": "/srv/weaver-models", "mounted": false});
    });
    let held = kept(Some(models_unmounted), None).members().unwrap_err();
    assert!(held.contains("the models drive"), "{held}");
    assert!(held.contains("not mounted"), "{held}");
    assert!(!held.contains("state"), "{held}");
}

#[test]
fn the_state_drive_holds_confirm_under_9_gib_of_filesystem_and_not_at_it() {
    assert_eq!(STATE_DRIVE_FLOOR, 9_663_676_416);
    let state_of = |bytes: u64| {
        let report = edited(|v| v["drives"][0]["bytes"] = json!(bytes));
        kept(Some(report), None).members()
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
        kept(Some(small_models), None).members().is_ok(),
        "the figure is the state drive's"
    );
}
