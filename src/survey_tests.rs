//! **The survey's reader against the format** (Spec 8.3): the format's
//! worked example read whole, and one report per cause of its section 3,
//! each refused naming its cause.

use serde_json::{Value, json};

use crate::survey::{Cause, MODELS_MOUNT, Mount, Role, STATE_MOUNT, Stage, read};

/// The format's worked example, carried as written (its section 6).
const EXAMPLE: &str = include_str!("../tests/fixtures/weaver-survey-1-worked-example.json");

/// A named edit of the example.
type Case = (&'static str, fn(&mut Value));

/// The example with one edit made on its parsed value.
fn edited(edit: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(EXAMPLE).unwrap();
    edit(&mut value);
    serde_json::to_string(&value).unwrap()
}

/// The example's text with `from` replaced by `to`, once.
fn replaced(from: &str, to: &str) -> String {
    assert_eq!(EXAMPLE.matches(from).count(), 1, "{from}");
    EXAMPLE.replacen(from, to, 1)
}

/// The cause a report is refused for, or a panic naming what read.
fn cause(report: impl AsRef<[u8]>) -> Cause {
    match read(report.as_ref()) {
        Ok(survey) => panic!("read: {survey:?}"),
        Err(refused) => refused.cause,
    }
}

/// The example at the stage `pairing`, each drive not mounted.
fn pairing() -> String {
    edited(|v| {
        v["stage"] = json!("pairing");
        v["drives"] = json!([
            {"role": "state", "mount_point": STATE_MOUNT, "mounted": false},
            {"role": "models", "mount_point": MODELS_MOUNT, "mounted": false},
        ]);
    })
}

#[test]
fn the_worked_example_reads_whole() {
    let survey = read(EXAMPLE.as_bytes()).unwrap();
    assert_eq!(survey.stage, Stage::Drives);
    assert_eq!(survey.taken.to_rfc3339(), "2026-11-17T09:00:00+00:00");
    assert_eq!(survey.image.version, "2.0");
    assert_eq!(
        survey.image.manifest_sha256,
        "5b1f0c9e7a2d4486b3e0f19c2d7a65e8c41f03b9d2a7e6158c0b94f3a1d2e7c6"
    );
    assert_eq!(survey.kernel_release, "7.3.4-arch1-2");
    assert_eq!(survey.machine.virtualization, "kvm");
    assert_eq!(survey.machine.product, "Standard PC (Q35 + ICH9, 2009)");
    assert_eq!(survey.machine.version, "pc-q35-9.2");
    assert_eq!(survey.cpu.vendor, "AuthenticAMD");
    assert_eq!(
        (survey.cpu.family, survey.cpu.model, survey.cpu.stepping),
        (25, 17, 1)
    );
    assert_eq!(survey.cpu.logical_cpus, 16);
    assert_eq!(survey.cpu.flags.len(), 20);
    assert_eq!(survey.cpu.flags[3], "avx512_bf16");
    assert_eq!(survey.memory_total_bytes, 33071620096);
    assert_eq!(survey.driver.version, "620.70.06");
    assert!(survey.driver.loaded);
    assert_eq!(survey.cuda_runtime_version, "13.4.92");
    assert_eq!(survey.cards.len(), 2);
    assert_eq!(survey.cards[1].ordinal, 1);
    assert_eq!(survey.cards[1].model, "NVIDIA RTX A5000");
    assert_eq!(survey.cards[1].pci_address, "0000:07:00.0");
    assert_eq!(survey.cards[1].memory_bytes, 25757220864);
    assert_eq!(survey.drives[0].role, Role::State);
    assert_eq!(survey.drives[1].role, Role::Models);
    assert_eq!(survey.drives[1].mount_point, MODELS_MOUNT);
    match &survey.drives[0].mount {
        Mount::Mounted {
            label,
            filesystem,
            bytes,
            free_bytes,
            project_quota,
        } => {
            assert_eq!(
                (label.as_str(), filesystem.as_str()),
                ("weaver-state", "ext4")
            );
            assert_eq!((*bytes, *free_bytes), (25191575552, 23863877632));
            assert!(project_quota);
        }
        Mount::NotMounted => panic!("the state drive is mounted"),
    }
}

#[test]
fn a_pairing_report_whose_drives_are_both_not_mounted_reads_whole() {
    let survey = read(pairing().as_bytes()).unwrap();
    assert_eq!(survey.stage, Stage::Pairing);
    assert!(survey.drives.iter().all(|d| d.mount == Mount::NotMounted));
    assert_eq!(survey.cards.len(), 2);
}

#[test]
fn a_box_with_no_card_and_no_module_loaded_reads() {
    let survey = read(
        edited(|v| {
            v["cards"] = json!([]);
            v["driver"]["loaded"] = json!(false);
        })
        .as_bytes(),
    )
    .unwrap();
    assert!(survey.cards.is_empty());
    assert!(!survey.driver.loaded);
}

#[test]
fn a_member_the_reader_does_not_know_is_kept_and_ignored_at_any_depth() {
    let report = edited(|v| {
        v["later"] = json!({"any": [1.5, -2, "x", null]});
        v["cpu"]["later"] = json!(1e3);
        v["cards"][1]["later"] = json!(true);
        v["drives"][0]["later"] = json!("x");
    });
    assert_eq!(
        read(report.as_bytes()).unwrap(),
        read(EXAMPLE.as_bytes()).unwrap()
    );
    let unmounted = pairing().replacen("\"mounted\":false", "\"mounted\":false,\"later\":1", 1);
    assert!(read(unmounted.as_bytes()).is_ok());
}

#[test]
fn refusal_1_bytes_that_are_not_one_json_object_in_utf_8() {
    let mut latin = EXAMPLE.as_bytes().to_vec();
    let at = EXAMPLE.find("QEMU").unwrap();
    latin[at] = 0xC9;
    assert_eq!(cause(&latin), Cause::Encoding);
    assert_eq!(cause(format!("\u{feff}{EXAMPLE}")), Cause::Encoding);
    assert_eq!(cause(format!("{EXAMPLE}{EXAMPLE}")), Cause::Encoding);
    assert_eq!(cause(format!("{}x", EXAMPLE.trim_end())), Cause::Encoding);
    assert_eq!(
        cause(format!("{}\n\n", EXAMPLE.trim_end())),
        Cause::Encoding
    );
    assert_eq!(cause(format!("{} ", EXAMPLE.trim_end())), Cause::Encoding);
    assert_eq!(cause(format!("[{EXAMPLE}]")), Cause::Encoding);
    assert_eq!(cause(&EXAMPLE[..EXAMPLE.len() - 3]), Cause::Encoding);
    assert_eq!(cause(""), Cause::Encoding);
    assert!(
        read(EXAMPLE.trim_end().as_bytes()).is_ok(),
        "no newline after the object"
    );
    assert!(
        read(format!("\n  {EXAMPLE}").as_bytes()).is_ok(),
        "whitespace is free"
    );
}

#[test]
fn refusal_2_more_than_64_kib() {
    let at_bound = format!("{}{EXAMPLE}", " ".repeat(65536 - EXAMPLE.len()));
    assert_eq!(at_bound.len(), 65536);
    assert!(read(at_bound.as_bytes()).is_ok());
    assert_eq!(cause(format!(" {at_bound}")), Cause::TooLarge);
}

#[test]
fn refusal_3_a_member_name_given_twice_in_one_object_at_any_depth() {
    let top = replaced(
        "\"stage\": \"drives\",",
        "\"stage\": \"drives\", \"stage\": \"pairing\",",
    );
    assert_eq!(cause(top), Cause::NameTwice);
    let nested = replaced(
        "\"kernel\": {\"release\": \"7.3.4-arch1-2\"},",
        "\"kernel\": {\"release\": \"7.3.4-arch1-2\", \"release\": \"7.3.4-arch1-2\"},",
    );
    assert_eq!(cause(nested), Cause::NameTwice);
    let in_a_card = replaced(
        "{\"ordinal\": 1, \"name\"",
        "{\"ordinal\": 1, \"ordinal\": 1, \"name\"",
    );
    assert_eq!(cause(in_a_card), Cause::NameTwice);
    let in_a_drive = replaced(
        "\"project_quota\": false}",
        "\"project_quota\": false, \"project_quota\": true}",
    );
    assert_eq!(cause(in_a_drive), Cause::NameTwice);
    let in_an_unknown_member = replaced(
        "\"stage\": \"drives\",",
        "\"stage\": \"drives\", \"later\": [{\"a\": 1, \"a\": 2}],",
    );
    assert_eq!(cause(in_an_unknown_member), Cause::NameTwice);
}

#[test]
fn refusal_4_a_format_missing_or_not_one_the_reader_knows() {
    assert_eq!(
        cause(edited(|v| drop(
            v.as_object_mut().unwrap().remove("format")
        ))),
        Cause::Format
    );
    for other in [
        json!("weaver-survey/2"),
        json!("weaver-survey/1 "),
        json!(null),
        json!(1),
    ] {
        assert_eq!(
            cause(edited(|v| v["format"] = other.clone())),
            Cause::Format,
            "{other}"
        );
    }
}

#[test]
fn refusal_5_a_member_missing_null_or_of_the_wrong_type() {
    let cases: [Case; 7] = [
        ("missing", |v| {
            drop(v["kernel"].as_object_mut().unwrap().remove("release"))
        }),
        ("null", |v| v["cpu"]["model_name"] = json!(null)),
        ("a string for a boolean", |v| {
            v["driver"]["loaded"] = json!("true")
        }),
        ("a string for an integer", |v| {
            v["cpu"]["family"] = json!("25")
        }),
        ("an object for an array", |v| v["cards"] = json!({})),
        ("a mounted drive without its label", |v| {
            drop(v["drives"][0].as_object_mut().unwrap().remove("label"))
        }),
        ("a whole object missing", |v| {
            drop(v.as_object_mut().unwrap().remove("machine"))
        }),
    ];
    for (name, edit) in cases {
        assert_eq!(cause(edited(edit)), Cause::Member, "{name}");
    }
}

#[test]
fn refusal_6_an_integer_negative_fractional_with_an_exponent_or_past_2_53_less_1() {
    for written in [
        "1.0",
        "1e3",
        "1E3",
        "-0",
        "-1",
        "9007199254740992",
        "18446744073709551616",
    ] {
        let report = replaced("\"family\": 25,", &format!("\"family\": {written},"));
        assert_eq!(cause(report), Cause::Integer, "{written}");
    }
    let largest = replaced(
        "\"total_bytes\": 33071620096",
        "\"total_bytes\": 9007199254740991",
    );
    assert_eq!(
        read(largest.as_bytes()).unwrap().memory_total_bytes,
        9007199254740991
    );
}

#[test]
fn refusal_7_a_string_outside_printable_ascii_over_its_bound_or_off_its_pattern() {
    let cases: [Case; 11] = [
        ("a tab", |v| v["machine"]["product"] = json!("Standard\tPC")),
        ("past ASCII", |v| {
            v["machine"]["product"] = json!("Standard PC \u{e9}")
        }),
        ("a delete", |v| {
            v["machine"]["product"] = json!("Standard PC \u{7f}")
        }),
        ("65 bytes", |v| {
            v["kernel"]["release"] = json!("7".repeat(65))
        }),
        ("17 bytes", |v| {
            v["drives"][1]["label"] = json!("m".repeat(17))
        }),
        ("a version off its pattern", |v| {
            v["image"]["version"] = json!("2")
        }),
        ("an uppercase digest", |v| {
            v["image"]["manifest_sha256"] =
                json!("5B1F0C9E7A2D4486B3E0F19C2D7A65E8C41F03B9D2A7E6158C0B94F3A1D2E7C6")
        }),
        ("an uppercase address", |v| {
            v["cards"][0]["pci_address"] = json!("0000:0A:00.0")
        }),
        ("a driver of one part", |v| {
            v["driver"]["version"] = json!("620")
        }),
        ("a runtime of one part", |v| {
            v["cuda_runtime"]["version"] = json!("13")
        }),
        ("an uppercase flag", |v| v["cpu"]["flags"][0] = json!("AES")),
    ];
    for (name, edit) in cases {
        assert_eq!(cause(edited(edit)), Cause::String, "{name}");
    }
    let at_bound = edited(|v| v["kernel"]["release"] = json!("7".repeat(64)));
    assert!(read(at_bound.as_bytes()).is_ok());
    let escaped = replaced("\"QEMU\"", "\"QEM\\u0055\"");
    assert_eq!(read(escaped.as_bytes()).unwrap().machine.vendor, "QEMU");
}

#[test]
fn refusal_8_a_stage_or_a_taken_out_of_form() {
    let cases = [
        ("stage", json!("setup")),
        ("taken", json!("2026-11-17T09:00:00+00:00")),
        ("taken", json!("2026-11-17T09:00:00.5Z")),
        ("taken", json!("2026-11-17 09:00:00Z")),
        ("taken", json!("2026-13-17T09:00:00Z")),
        ("taken", json!("2026-11-17t09:00:00z")),
    ];
    for (member, value) in cases {
        assert_eq!(
            cause(edited(|v| v[member] = value.clone())),
            Cause::StageOrTaken,
            "{value}"
        );
    }
}

#[test]
fn refusal_9_an_ordinal_not_its_index_or_addresses_that_do_not_ascend() {
    assert_eq!(
        cause(edited(|v| v["cards"][1]["ordinal"] = json!(2))),
        Cause::Cards
    );
    assert_eq!(
        cause(edited(
            |v| v["cards"][1]["pci_address"] = json!("0000:06:00.0")
        )),
        Cause::Cards
    );
    assert_eq!(
        cause(edited(
            |v| v["cards"][1]["pci_address"] = json!("0000:05:00.0")
        )),
        Cause::Cards
    );
}

#[test]
fn refusal_10_drives_not_exactly_state_then_models_with_their_mount_points() {
    let cases: [Case; 6] = [
        ("reversed", |v| {
            let drives = v["drives"].as_array_mut().unwrap();
            drives.swap(0, 1);
        }),
        ("one drive", |v| {
            drop(v["drives"].as_array_mut().unwrap().pop())
        }),
        ("three drives", |v| {
            let first = v["drives"][0].clone();
            v["drives"].as_array_mut().unwrap().push(first)
        }),
        ("another mount point", |v| {
            v["drives"][0]["mount_point"] = json!("/srv/state")
        }),
        ("another role", |v| {
            v["drives"][1]["role"] = json!("scratch")
        }),
        (
            "a drive not mounted carrying a mounted drive's member",
            |v| v["drives"][0]["mounted"] = json!(false),
        ),
    ];
    for (name, edit) in cases {
        assert_eq!(cause(edited(edit)), Cause::Drives, "{name}");
    }
}

#[test]
fn refusal_11_an_integer_or_a_count_outside_its_bound() {
    let cases: [Case; 8] = [
        ("a family past 65535", |v| v["cpu"]["family"] = json!(65536)),
        ("no logical CPU", |v| v["cpu"]["logical_cpus"] = json!(0)),
        ("4097 logical CPUs", |v| {
            v["cpu"]["logical_cpus"] = json!(4097)
        }),
        ("17 cards", |v| {
            v["cards"] = Value::Array(
                (0..17)
                    .map(|i| {
                        json!({"ordinal": i, "name": "c", "pci_address": format!("0000:{i:02x}:00.0"), "memory_bytes": 1})
                    })
                    .collect(),
            )
        }),
        ("no flag", |v| v["cpu"]["flags"] = json!([])),
        ("513 flags", |v| {
            v["cpu"]["flags"] = Value::Array((0..513).map(|i| json!(format!("f{i:03}"))).collect())
        }),
        ("flags out of byte order", |v| {
            v["cpu"]["flags"].as_array_mut().unwrap().swap(0, 1)
        }),
        ("a flag given twice", |v| {
            v["cpu"]["flags"][1] = json!("aes")
        }),
    ];
    for (name, edit) in cases {
        assert_eq!(cause(edited(edit)), Cause::Bound, "{name}");
    }
    let sixteen = edited(|v| {
        v["cards"] = Value::Array(
            (0..16)
                .map(|i| json!({"ordinal": i, "name": "c", "pci_address": format!("0000:{i:02x}:00.0"), "memory_bytes": 1}))
                .collect(),
        )
    });
    assert_eq!(read(sixteen.as_bytes()).unwrap().cards.len(), 16);
}

#[test]
fn a_refusal_names_its_cause_in_the_formats_words() {
    let refused = read(b"[]").unwrap_err();
    assert_eq!(refused.cause.number(), 1);
    let text = refused.to_string();
    assert!(
        text.starts_with("refused by weaver-survey/1's cause 1:"),
        "{text}"
    );
    assert!(text.contains("not one JSON object"), "{text}");
}

#[test]
fn an_unknown_member_with_a_number_past_any_float_is_ignored() {
    let report = replaced(
        "\"stage\": \"drives\",",
        "\"stage\": \"drives\", \"later\": 1e400,",
    );
    assert!(
        read(report.as_bytes()).is_ok(),
        "{:?}",
        read(report.as_bytes())
    );
    let known = replaced("\"family\": 25,", "\"family\": 1e400,");
    assert_eq!(cause(known), Cause::Integer);
}

#[test]
fn nesting_is_read_to_the_parsers_own_bound_and_refused_past_it() {
    let nested = |levels: usize| {
        let inner = format!("{}{}", "[".repeat(levels), "]".repeat(levels));
        replaced(
            "\"stage\": \"drives\",",
            &format!("\"stage\": \"drives\", \"later\": {inner},"),
        )
    };
    assert!(
        read(nested(126).as_bytes()).is_ok(),
        "the report and 126 arrays"
    );
    assert_eq!(cause(nested(127)), Cause::Encoding);
    assert_eq!(cause(nested(20_000)), Cause::Encoding);
}

#[test]
fn an_unknown_member_with_a_lone_surrogate_is_ignored_and_names_compare_by_code_units() {
    let later = |member: &str| {
        replaced(
            "\"stage\": \"drives\",",
            &format!("\"stage\": \"drives\", \"later\": {member},"),
        )
    };
    let report = later(r#"{"\ud800": "\udfff", "\ud801": 1, "\ud83d\ude00": "\ud83d"}"#);
    assert!(
        read(report.as_bytes()).is_ok(),
        "{:?}",
        read(report.as_bytes())
    );
    assert_eq!(
        cause(later(r#"{"\ud800": 1, "\uD800": 2}"#)),
        Cause::NameTwice
    );
    assert_eq!(
        cause(later("{\"\\ud83d\\ude00\": 1, \"\u{1F600}\": 2}")),
        Cause::NameTwice
    );
    let known = replaced(
        "\"release\": \"7.3.4-arch1-2\"",
        r#""release": "7.3.4\ud800""#,
    );
    assert_eq!(cause(known), Cause::String);
}
