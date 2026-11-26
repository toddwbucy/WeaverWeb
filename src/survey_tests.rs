//! **The survey's reader against the format** (Spec 8.3): the format's
//! worked example of `weaver-survey/2` read whole, its file checked by its
//! sha256; then one test per refusal of the format's section 3, each case
//! refused naming its cause; a case at each bound the format states and
//! one past it; and each string that may be empty and each that may not.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::survey::{Cause, Mount, Planes, QUOTE_BOUND, REPORT_BOUND, Role, format_taken, read};

/// The format's worked example, carried byte for byte (its section 6).
const EXAMPLE: &str = include_str!("../tests/fixtures/weaver-survey-2-worked-example.json");

/// The example's sha256, as the suite's pull request 57 carries it.
const EXAMPLE_SHA256: &str = "fb6801694af2ebccedf3a2a52915f2610205fa20ac7defc16ada88a58ab59d95";

/// Version 1's worked example, which this reader refuses by its name.
const VERSION_1: &str = include_str!("../tests/fixtures/weaver-survey-1-worked-example.json");

/// A named edit of the example.
type Case = (&'static str, fn(&mut Value));

/// A named edit of the example and the cause it is refused for.
type Refusing = (&'static str, fn(&mut Value), Cause);

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

/// The example with one unknown member `later` after `stage`, as text.
fn later(member: &str) -> String {
    replaced(
        "\"stage\": \"pairing\",",
        &format!("\"stage\": \"pairing\", \"later\": {member},"),
    )
}

/// The cause a report is refused for, or a panic naming what read.
fn cause(report: impl AsRef<[u8]>) -> Cause {
    match read(report.as_ref()) {
        Ok(survey) => panic!("read: {survey:?}"),
        Err(refused) => refused.cause,
    }
}

/// Whether a report reads, with the refusal where it does not.
fn reads(report: impl AsRef<[u8]>) -> bool {
    match read(report.as_ref()) {
        Ok(_) => true,
        Err(refused) => panic!("refused: {refused}"),
    }
}

/// The example with the two drives' places set, each drive not mounted.
fn places(state: &str, models: &str) -> String {
    let (state, models) = (state.to_owned(), models.to_owned());
    edited(move |v| {
        v["drives"] = json!([
            {"role": "state", "mount_point": state, "mounted": false},
            {"role": "models", "mount_point": models, "mounted": false},
        ]);
    })
}

/// The example with the two planes and `planes` set.
fn network(management: (&str, u64), data: (&str, u64), planes: &str) -> String {
    let (m, d, planes) = (
        json!({"address": management.0, "prefix_length": management.1}),
        json!({"address": data.0, "prefix_length": data.1}),
        planes.to_owned(),
    );
    edited(move |v| {
        v["network"] = json!({"management": m, "data": d, "planes": planes});
    })
}

/// The example with both planes on `address` under `shared`.
fn one_address(address: &str, prefix_length: u64) -> String {
    network((address, prefix_length), (address, prefix_length), "shared")
}

#[test]
fn the_worked_example_reads_whole() {
    let digest = Sha256::digest(EXAMPLE.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, EXAMPLE_SHA256, "the fixture is the format's example");
    let survey = read(EXAMPLE.as_bytes()).unwrap();
    assert_eq!(survey.taken.to_rfc3339(), "2026-11-17T09:00:00+00:00");
    assert_eq!(survey.image.version, "2.0");
    assert_eq!(
        survey.image.manifest_sha256,
        "03fcfb2e3fffc40f40d87b49d7f086d1c45c0c394a6794fbfdb1ce855502c1c8"
    );
    assert_eq!(survey.kernel_release, "7.3.4-arch1-2");
    assert_eq!(survey.machine.virtualization, "kvm");
    assert_eq!(survey.machine.vendor, "QEMU");
    assert_eq!(survey.machine.product, "Standard PC (Q35 + ICH9, 2009)");
    assert_eq!(survey.machine.version, "pc-q35-9.2");
    assert_eq!(survey.cpu.vendor, "AuthenticAMD");
    assert_eq!(survey.cpu.model_name, "AMD EPYC 9354P 32-Core Processor");
    assert_eq!(
        (survey.cpu.family, survey.cpu.model, survey.cpu.stepping),
        (25, 17, 1)
    );
    assert_eq!(survey.cpu.logical_cpus, 16);
    assert_eq!(
        survey.cpu.flags,
        [
            "aes",
            "avx",
            "avx2",
            "avx512_bf16",
            "avx512_vnni",
            "avx512bw",
            "avx512cd",
            "avx512dq",
            "avx512f",
            "avx512vl",
            "bmi1",
            "bmi2",
            "f16c",
            "fma",
            "fpu",
            "sse",
            "sse2",
            "sse4_1",
            "sse4_2",
            "ssse3",
        ]
    );
    assert_eq!(survey.memory_total_bytes, 33071620096);
    assert_eq!(survey.driver.version, "620.70.06");
    assert!(survey.driver.loaded);
    assert_eq!(survey.cuda_runtime_version, "13.4.92");
    assert_eq!(survey.cards.len(), 2);
    assert_eq!(survey.cards[0].ordinal, 0);
    assert_eq!(survey.cards[0].model, "NVIDIA RTX A5000");
    assert_eq!(survey.cards[0].pci_address, "0000:06:00.0");
    assert_eq!(survey.cards[0].memory_bytes, 25757220864);
    assert_eq!(survey.cards[1].ordinal, 1);
    assert_eq!(survey.cards[1].model, "NVIDIA RTX A5000");
    assert_eq!(survey.cards[1].pci_address, "0000:07:00.0");
    assert_eq!(survey.cards[1].memory_bytes, 25757220864);
    assert_eq!(survey.drives[0].role, Role::State);
    assert_eq!(survey.drives[0].mount_point, "/mnt/weaver/state");
    assert_eq!(survey.drives[1].role, Role::Models);
    assert_eq!(
        survey.drives[1].mount_point, "/srv/weaver-models",
        "the place the example's operator chose"
    );
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
    match &survey.drives[1].mount {
        Mount::Mounted {
            label,
            filesystem,
            bytes,
            free_bytes,
            project_quota,
        } => {
            assert_eq!(
                (label.as_str(), filesystem.as_str()),
                ("weaver-models", "ext4")
            );
            assert_eq!((*bytes, *free_bytes), (135089524736, 72464760832));
            assert!(!project_quota);
        }
        Mount::NotMounted => panic!("the models drive is mounted"),
    }
    let network = survey.network;
    assert_eq!(network.management.address.to_string(), "192.0.2.10");
    assert_eq!(network.management.prefix_length, 24);
    assert_eq!(network.data.address.to_string(), "198.51.100.10");
    assert_eq!(network.data.prefix_length, 24);
    assert_eq!(network.planes, Planes::Separate);
}

/// **The image proof's shape** (the format's section 5): both drives not
/// mounted, at the two places the proof gives, and the guest's one address
/// as both planes under `shared`.
#[test]
fn the_image_proofs_shape_reads() {
    let report = edited(|v| {
        v["drives"] = json!([
            {"role": "state", "mount_point": "/mnt/weaver/state", "mounted": false},
            {"role": "models", "mount_point": "/mnt/weaver/models", "mounted": false},
        ]);
        let plane = json!({"address": "192.0.2.10", "prefix_length": 24});
        v["network"] = json!({"management": plane, "data": plane, "planes": "shared"});
    });
    let survey = read(report.as_bytes()).unwrap();
    assert!(survey.drives.iter().all(|d| d.mount == Mount::NotMounted));
    assert_eq!(survey.network.management, survey.network.data);
    assert_eq!(survey.network.planes, Planes::Shared);
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
        v["network"]["later"] = json!([]);
        v["network"]["data"]["later"] = json!({"zone": "eth0"});
    });
    assert_eq!(
        read(report.as_bytes()).unwrap(),
        read(EXAMPLE.as_bytes()).unwrap()
    );
    let unmounted =
        places("/a", "/b").replacen("\"mounted\":false", "\"mounted\":false,\"later\":1", 1);
    assert!(reads(unmounted));
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
    assert!(reads(EXAMPLE.trim_end()), "no newline after the object");
    assert!(reads(format!("\n  {EXAMPLE}")), "whitespace before is free");
}

/// **The 64 KiB bound counts the newline** (the format's section 2): a
/// report of 65,536 bytes ending in its newline reads; one byte more is
/// refused, a space before it or the object filling 65,536 bytes with its
/// newline after.
#[test]
fn refusal_2_more_than_64_kib() {
    assert!(EXAMPLE.ends_with("}\n"));
    let at_bound = format!("{}{EXAMPLE}", " ".repeat(65536 - EXAMPLE.len()));
    assert_eq!(at_bound.len(), 65536);
    assert!(reads(&at_bound));
    assert_eq!(cause(format!(" {at_bound}")), Cause::TooLarge);
    let object = EXAMPLE.trim_end();
    let full = format!("{}{object}", " ".repeat(65536 - object.len()));
    assert_eq!(full.len(), 65536);
    assert!(reads(&full), "the object alone at the bound");
    let with_newline = format!("{full}\n");
    assert_eq!(with_newline.len(), 65537);
    assert_eq!(cause(with_newline), Cause::TooLarge, "the newline counts");
}

#[test]
fn refusal_3_a_member_name_given_twice_in_one_object_at_any_depth() {
    let top = replaced(
        "\"stage\": \"pairing\",",
        "\"stage\": \"pairing\", \"stage\": \"pairing\",",
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
    let in_a_plane = replaced(
        "{\"address\": \"192.0.2.10\", \"prefix_length\": 24}",
        "{\"address\": \"192.0.2.10\", \"prefix_length\": 24, \"prefix_length\": 24}",
    );
    assert_eq!(cause(in_a_plane), Cause::NameTwice);
    assert_eq!(cause(later("[{\"a\": 1, \"a\": 2}]")), Cause::NameTwice);
}

/// **A format the reader does not know**, and version 1 by its name (the
/// format's section 7): the detail names the version read and the version
/// taken.
#[test]
fn refusal_4_a_format_missing_or_not_one_the_reader_knows() {
    assert_eq!(
        cause(edited(|v| drop(
            v.as_object_mut().unwrap().remove("format")
        ))),
        Cause::Format
    );
    for other in [
        json!("weaver-survey/1"),
        json!("weaver-survey/3"),
        json!("weaver-survey/2 "),
        json!("Weaver-survey/2"),
        json!(""),
        json!(null),
        json!(2),
    ] {
        assert_eq!(
            cause(edited(|v| v["format"] = other.clone())),
            Cause::Format,
            "{other}"
        );
    }
    let refused = read(VERSION_1.as_bytes()).unwrap_err();
    assert_eq!(refused.cause, Cause::Format);
    assert_eq!(
        refused.detail,
        "format \"weaver-survey/1\" read, the earlier version, which this reader does not \
         read; weaver-survey/2 taken"
    );
    let third = read(edited(|v| v["format"] = json!("weaver-survey/3")).as_bytes()).unwrap_err();
    assert_eq!(
        third.detail,
        "format \"weaver-survey/3\" read; weaver-survey/2 taken"
    );
}

#[test]
fn refusal_5_a_member_missing_null_or_of_the_wrong_type() {
    let cases: [Case; 16] = [
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
        ("network missing", |v| {
            drop(v.as_object_mut().unwrap().remove("network"))
        }),
        ("the management plane missing", |v| {
            drop(v["network"].as_object_mut().unwrap().remove("management"))
        }),
        ("the data plane a string", |v| {
            v["network"]["data"] = json!("198.51.100.10/24")
        }),
        ("an address missing", |v| {
            drop(
                v["network"]["data"]
                    .as_object_mut()
                    .unwrap()
                    .remove("address"),
            )
        }),
        ("an address a number", |v| {
            v["network"]["management"]["address"] = json!(3221225994u64)
        }),
        ("a prefix length a string", |v| {
            v["network"]["management"]["prefix_length"] = json!("24")
        }),
        ("the data plane's prefix length missing", |v| {
            drop(
                v["network"]["data"]
                    .as_object_mut()
                    .unwrap()
                    .remove("prefix_length"),
            )
        }),
        ("the data plane's prefix length null", |v| {
            v["network"]["data"]["prefix_length"] = json!(null)
        }),
        ("planes null", |v| v["network"]["planes"] = json!(null)),
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
    let signed_prefix = replaced(
        "{\"address\": \"192.0.2.10\", \"prefix_length\": 24}",
        "{\"address\": \"192.0.2.10\", \"prefix_length\": -0}",
    );
    assert_eq!(
        cause(signed_prefix),
        Cause::Integer,
        "-0 in a prefix length"
    );
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
                json!("03FCFB2E3FFFC40F40D87B49D7F086D1C45C0C394A6794FBFDB1CE855502C1C8")
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
    let escaped = replaced("\"QEMU\"", "\"QEM\\u0055\"");
    assert_eq!(read(escaped.as_bytes()).unwrap().machine.vendor, "QEMU");
}

#[test]
fn refusal_8_a_stage_or_a_taken_out_of_form() {
    let cases = [
        ("stage", json!("drives")),
        ("stage", json!("Pairing")),
        ("stage", json!("setup")),
        ("stage", json!("")),
        ("taken", json!("2026-12-31T23:59:60Z")),
        ("taken", json!("2026-02-30T00:00:00Z")),
        ("taken", json!("2026-11-17T09:00:00+00:00")),
        ("taken", json!("2026-11-17T09:00:00.5Z")),
        ("taken", json!("2026-11-17 09:00:00Z")),
        ("taken", json!("2026-13-17T09:00:00Z")),
        ("taken", json!("2026-11-17t09:00:00Z")),
        ("taken", json!("2026-11-17T09:00:00z")),
        ("taken", json!("")),
    ];
    for (member, value) in cases {
        assert_eq!(
            cause(edited(|v| v[member] = value.clone())),
            Cause::StageOrTaken,
            "{member} {value}"
        );
    }
    let last_second = edited(|v| v["taken"] = json!("2026-12-31T23:59:59Z"));
    assert_eq!(
        read(last_second.as_bytes()).unwrap().taken.to_rfc3339(),
        "2026-12-31T23:59:59+00:00"
    );
}

/// **The addresses ascend by number** (the format's section 1): a domain of
/// four hex digits before one of five ascends, though its bytes do not.
#[test]
fn refusal_9_an_ordinal_not_its_index_or_addresses_that_do_not_ascend() {
    let pair = |first: &'static str, second: &'static str| {
        edited(move |v| {
            v["cards"][0]["pci_address"] = json!(first);
            v["cards"][1]["pci_address"] = json!(second);
        })
    };
    assert_eq!(
        cause(edited(|v| v["cards"][1]["ordinal"] = json!(2))),
        Cause::Cards
    );
    for (first, second) in [
        ("0000:06:00.0", "0000:06:00.0"),
        ("0000:07:00.0", "0000:06:00.0"),
        ("10000:06:00.0", "ffff:07:00.0"),
        ("0000:06:01.0", "0000:06:00.7"),
    ] {
        assert_eq!(cause(pair(first, second)), Cause::Cards, "{first} {second}");
    }
    for (first, second) in [
        ("ffff:06:00.0", "10000:07:00.0"),
        ("0000:06:00.7", "0000:06:01.0"),
        ("0000:06:1f.0", "0000:07:00.0"),
    ] {
        assert!(reads(pair(first, second)), "{first} {second}");
    }
}

/// **The drives and their places** (the format's sections 1 and 1.1):
/// state then models, each place in the one spelling, the two different
/// and neither inside the other, judged part by part.
#[test]
fn refusal_10_drives_their_order_and_their_places() {
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
        ("another role", |v| {
            v["drives"][1]["role"] = json!("scratch")
        }),
        ("an empty role", |v| v["drives"][0]["role"] = json!("")),
        (
            "a drive not mounted carrying a mounted drive's member",
            |v| v["drives"][0]["mounted"] = json!(false),
        ),
    ];
    for (name, edit) in cases {
        assert_eq!(cause(edited(edit)), Cause::Drives, "{name}");
    }
    let part = |n: usize| format!("/{}", "p".repeat(n));
    for place in [
        "".to_owned(),
        "/".to_owned(),
        "a".to_owned(),
        "//a".to_owned(),
        "/a/".to_owned(),
        "/a//b".to_owned(),
        "/.".to_owned(),
        "/..".to_owned(),
        "/a/./b".to_owned(),
        "/a/../b".to_owned(),
        "/a b".to_owned(),
        "/a+b".to_owned(),
        "/a\\b".to_owned(),
        part(256),
        format!("/{}/{}", "p".repeat(200), "q".repeat(55)),
    ] {
        assert_eq!(cause(places(&place, "/b")), Cause::Drives, "{place:?}");
        assert_eq!(cause(places("/b", &place)), Cause::Drives, "{place:?}");
    }
    for (state, models) in [
        ("/a", "/a"),
        ("/a", "/a/b"),
        ("/a/b", "/a"),
        ("/mnt/weaver", "/mnt/weaver/models"),
    ] {
        assert_eq!(
            cause(places(state, models)),
            Cause::Drives,
            "{state} {models}"
        );
    }
    for (state, models) in [
        ("/a", "/ab"),
        ("/mnt/weaverx", "/mnt/weaver"),
        ("/A.b_c-d", "/srv/0"),
        ("/...", "/a..b"),
    ] {
        assert!(reads(places(state, models)), "{state} {models}");
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
        ("17 cards", |v| v["cards"] = cards(17)),
        ("no flag", |v| v["cpu"]["flags"] = json!([])),
        ("513 flags", |v| v["cpu"]["flags"] = flags(513)),
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
}

/// `n` cards in bus order.
fn cards(n: usize) -> Value {
    Value::Array(
        (0..n)
            .map(|i| {
                json!({"ordinal": i, "name": "c", "pci_address": format!("0000:{i:02x}:00.0"), "memory_bytes": 1})
            })
            .collect(),
    )
}

/// `n` flags in byte order.
fn flags(n: usize) -> Value {
    Value::Array((0..n).map(|i| json!(format!("f{i:03}"))).collect())
}

/// **A plane's address, its prefix length and `planes`** (the format's
/// sections 1 and 1.1, refusal 12).
#[test]
fn refusal_12_a_planes_address_its_prefix_or_planes() {
    for address in [
        "192.0.2.010",
        "192.0.2",
        "192.0.2.256",
        " 192.0.2.10",
        "2001:DB8::1",
        "2001:db8:0:0:0:0:0:1",
        "2001:0db8::1",
        "2001:db8::1:1:1:1:1",
        "::ffff:c000:201",
        "::ffff:192.0.2.1",
        "fe80::1%eth0",
        "192.0.2.10%eth0",
        "0.0.0.0",
        "::",
        "169.254.1.1",
        "fe80::1",
        "febf::1",
        "224.0.0.1",
        "239.255.255.255",
        "ff02::1",
        "255.255.255.255",
        "",
    ] {
        assert_eq!(
            cause(one_address(address, 24)),
            Cause::Network,
            "{address:?}"
        );
    }
    // Just outside each refused kind: IPv6's link-local is fe80::/10 alone,
    // IPv4's 169.254.0.0/16, multicast 224.0.0.0/4, and the limited
    // broadcast one address.
    for address in [
        "fec0::1",
        "fe7f::1",
        "fe00::1",
        "169.255.0.1",
        "169.253.255.255",
        "223.255.255.255",
        "240.0.0.1",
        "255.255.255.254",
        "fe::1",
        "feff::1",
    ] {
        assert!(reads(one_address(address, 24)), "{address}");
    }
    for (address, prefix_length) in [
        ("192.0.2.10", 0),
        ("192.0.2.10", 33),
        ("2001:db8::10", 0),
        ("2001:db8::10", 129),
    ] {
        assert_eq!(
            cause(one_address(address, prefix_length)),
            Cause::Network,
            "{address}/{prefix_length}"
        );
    }
    for planes in ["both", "Separate", ""] {
        assert_eq!(
            cause(network(("192.0.2.10", 24), ("198.51.100.10", 24), planes)),
            Cause::Network,
            "{planes:?}"
        );
    }
    for (management, data) in [
        (("192.0.2.10", 24), ("192.0.2.10", 24)),
        (("192.0.2.10", 24), ("192.0.2.20", 24)),
        (("192.0.2.10", 16), ("192.0.3.10", 24)),
        (("192.0.3.10", 24), ("192.0.2.10", 16)),
        (("2001:db8::10", 64), ("2001:db8::20", 64)),
        (("192.0.2.10", 24), ("192.0.2.130", 24)),
        (("192.0.2.10", 31), ("192.0.2.11", 31)),
        (("2001:db8::10", 127), ("2001:db8::11", 127)),
        (("2001:db8:1::1", 48), ("2001:db8:1:ffff::1", 64)),
    ] {
        assert_eq!(
            cause(network(management, data, "separate")),
            Cause::Network,
            "{management:?} {data:?}"
        );
    }
    for (management, data, planes) in [
        (("192.0.2.10", 24), ("198.51.100.10", 24), "separate"),
        (("192.0.2.10", 24), ("2001:db8::10", 64), "separate"),
        (("192.0.2.10", 24), ("192.0.2.10", 24), "shared"),
        (("192.0.2.10", 24), ("192.0.2.20", 24), "shared"),
        (("192.0.2.10", 25), ("192.0.2.200", 25), "separate"),
        (("2001:db8:1::1", 64), ("2001:db8:2::1", 64), "separate"),
        (("192.0.2.10", 32), ("192.0.2.11", 32), "separate"),
        (("2001:db8::10", 128), ("2001:db8::11", 128), "separate"),
        (("2001:db8::10", 64), ("192.0.2.10", 32), "separate"),
        (("127.0.0.1", 8), ("127.0.0.1", 8), "shared"),
        (("::1", 128), ("::1", 128), "shared"),
        (("192.0.2.10", 1), ("192.0.2.10", 32), "shared"),
        (("2001:db8::10", 1), ("2001:db8::10", 128), "shared"),
    ] {
        assert!(
            reads(network(management, data, planes)),
            "{management:?} {data:?} {planes}"
        );
    }
}

/// **IPv6's one spelling is RFC 5952's**, pinned on its own examples: each
/// reads as written, and another spelling of the same address is refused.
#[test]
fn an_ipv6_address_reads_in_rfc_5952s_spelling_alone() {
    for (spelling, others) in [
        (
            "2001:db8::1",
            &["2001:db8:0:0:0:0:0:1", "2001:DB8::1", "2001:db8::0:1"][..],
        ),
        (
            "2001:db8:0:1:1:1:1:1",
            &["2001:db8::1:1:1:1:1", "2001:0db8:0:1:1:1:1:1"][..],
        ),
        (
            "2001:0:0:1::1",
            &["2001:0:0:1:0:0:0:1", "2001:0:0:1:0::1"][..],
        ),
        (
            "2001::1:0:0:1",
            &["2001:0:0:0:1::1", "2001:0:0:0:1:0:0:1"][..],
        ),
        (
            "2001:db8::1:0:0:1",
            &["2001:db8:0:0:1::1", "2001:db8:0:0:1:0:0:1"][..],
        ),
        (
            "2001:db80:1234:5678:9abc:def0:1234:5678",
            &["2001:db80:1234:5678:9abc:def0:1234:56789"][..],
        ),
        ("::102:304", &["::1.2.3.4"][..]),
    ] {
        assert!(reads(one_address(spelling, 64)), "{spelling}");
        for other in others {
            assert_eq!(
                cause(one_address(other, 64)),
                Cause::Network,
                "{other} for {spelling}"
            );
        }
    }
    assert_eq!("2001:db80:1234:5678:9abc:def0:1234:5678".len(), 39);
}

/// **A case at each bound the format states, and one past it** (the
/// format's section 1).
#[test]
fn each_bound_reads_at_it_and_refuses_one_past_it() {
    let string_bounds: [(&str, &str, usize); 8] = [
        ("kernel", "release", 64),
        ("machine", "virtualization", 32),
        ("machine", "vendor", 128),
        ("machine", "product", 128),
        ("machine", "version", 128),
        ("cpu", "vendor", 32),
        ("cpu", "model_name", 128),
        ("cpu", "flags", 32),
    ];
    for (object, member, bound) in string_bounds {
        let set = |n: usize| {
            edited(move |v| {
                let value = "a".repeat(n);
                if member == "flags" {
                    v[object][member] = json!([value]);
                } else {
                    v[object][member] = json!(value);
                }
            })
        };
        assert!(reads(set(bound)), "{object}.{member} at {bound}");
        assert_eq!(cause(set(bound + 1)), Cause::String, "{object}.{member}");
    }
    for (member, bound) in [("name", 96), ("label", 16), ("filesystem", 16)] {
        let set = |n: usize| {
            edited(move |v| {
                let at = if member == "name" { "cards" } else { "drives" };
                v[at][0][member] = json!("a".repeat(n));
            })
        };
        assert!(reads(set(bound)), "{member} at {bound}");
        assert_eq!(cause(set(bound + 1)), Cause::String, "{member}");
    }
    for member in ["family", "model", "stepping"] {
        for (n, read) in [(0, true), (65535, true), (65536, false)] {
            let report = edited(|v| v["cpu"][member] = json!(n));
            if read {
                assert!(reads(&report), "{member} {n}");
            } else {
                assert_eq!(cause(report), Cause::Bound, "{member} {n}");
            }
        }
    }
    for (n, read) in [(1, true), (4096, true), (0, false), (4097, false)] {
        let report = edited(|v| v["cpu"]["logical_cpus"] = json!(n));
        if read {
            assert!(reads(&report), "logical_cpus {n}");
        } else {
            assert_eq!(cause(report), Cause::Bound, "logical_cpus {n}");
        }
    }
    for (n, read) in [(1, true), (512, true), (0, false), (513, false)] {
        let report = edited(|v| v["cpu"]["flags"] = flags(n));
        if read {
            assert!(reads(&report), "{n} flags");
        } else {
            assert_eq!(cause(report), Cause::Bound, "{n} flags");
        }
    }
    for (n, read) in [(0, true), (16, true), (17, false)] {
        let report = edited(|v| {
            v["cards"] = cards(n);
            v["driver"]["loaded"] = json!(n > 0);
        });
        if read {
            assert!(reads(&report), "{n} cards");
        } else {
            assert_eq!(cause(report), Cause::Bound, "{n} cards");
        }
    }
    for (domain, read) in [
        ("0000", true),
        ("00000000", true),
        ("000", false),
        ("000000000", false),
    ] {
        let report = edited(|v| {
            v["cards"] = json!([{"ordinal": 0, "name": "c", "pci_address": format!("{domain}:06:00.0"), "memory_bytes": 1}]);
        });
        if read {
            assert!(reads(&report), "domain {domain}");
        } else {
            assert_eq!(cause(report), Cause::String, "domain {domain}");
        }
    }
    // A place: the whole at most 255 bytes, so a part reaches 254 within
    // it, and the whole's bound is the one a case can pass.
    let whole = |n: usize| format!("/{}", "p".repeat(n - 1));
    assert!(reads(places(&whole(255), "/b")), "a place of 255 bytes");
    assert_eq!(cause(places(&whole(256), "/b")), Cause::Drives);
    let two_parts = format!("/{}/{}", "p".repeat(127), "q".repeat(126));
    assert_eq!(two_parts.len(), 255);
    assert!(reads(places(&two_parts, "/b")));
    for (n, read) in [("9007199254740991", true), ("9007199254740992", false)] {
        let report = replaced(
            "\"total_bytes\": 33071620096",
            &format!("\"total_bytes\": {n}"),
        );
        if read {
            assert!(reads(&report), "{n}");
        } else {
            assert_eq!(cause(report), Cause::Integer, "{n}");
        }
    }
    // The three versions have no byte bound of their own: past 64 bytes
    // within the report's bound, each reads.
    let long = "1".repeat(80);
    let versions = edited(|v| {
        v["image"]["version"] = json!(format!("2.{long}"));
        v["driver"]["version"] = json!(format!("620.70.{long}"));
        v["cuda_runtime"]["version"] = json!(format!("13.{long}"));
    });
    assert!(reads(versions));
}

/// **Which strings may be empty** (the format's section 1): a string whose
/// bound names no pattern may be empty; one with a pattern matches it
/// whole, so empty is refused, each under its refusal.
#[test]
fn a_string_without_a_pattern_may_be_empty_and_one_with_a_pattern_may_not() {
    let may: [Case; 10] = [
        ("kernel.release", |v| v["kernel"]["release"] = json!("")),
        ("machine.virtualization", |v| {
            v["machine"]["virtualization"] = json!("")
        }),
        ("machine.vendor", |v| v["machine"]["vendor"] = json!("")),
        ("machine.product", |v| v["machine"]["product"] = json!("")),
        ("machine.version", |v| v["machine"]["version"] = json!("")),
        ("cpu.vendor", |v| v["cpu"]["vendor"] = json!("")),
        ("cpu.model_name", |v| v["cpu"]["model_name"] = json!("")),
        ("cards[].name", |v| v["cards"][0]["name"] = json!("")),
        ("drives[].label", |v| v["drives"][0]["label"] = json!("")),
        ("drives[].filesystem", |v| {
            v["drives"][1]["filesystem"] = json!("")
        }),
    ];
    for (name, edit) in may {
        assert!(reads(edited(edit)), "{name} empty");
    }
    let may_not: [Refusing; 15] = [
        ("format", |v| v["format"] = json!(""), Cause::Format),
        ("stage", |v| v["stage"] = json!(""), Cause::StageOrTaken),
        ("taken", |v| v["taken"] = json!(""), Cause::StageOrTaken),
        (
            "image.version",
            |v| v["image"]["version"] = json!(""),
            Cause::String,
        ),
        (
            "image.manifest_sha256",
            |v| v["image"]["manifest_sha256"] = json!(""),
            Cause::String,
        ),
        (
            "driver.version",
            |v| v["driver"]["version"] = json!(""),
            Cause::String,
        ),
        (
            "cuda_runtime.version",
            |v| v["cuda_runtime"]["version"] = json!(""),
            Cause::String,
        ),
        (
            "a cpu.flags entry",
            |v| v["cpu"]["flags"][0] = json!(""),
            Cause::String,
        ),
        (
            "cards[].pci_address",
            |v| v["cards"][0]["pci_address"] = json!(""),
            Cause::String,
        ),
        (
            "drives[].role",
            |v| v["drives"][0]["role"] = json!(""),
            Cause::Drives,
        ),
        (
            "drives[].mount_point",
            |v| v["drives"][1]["mount_point"] = json!(""),
            Cause::Drives,
        ),
        (
            "network.management.address",
            |v| v["network"]["management"]["address"] = json!(""),
            Cause::Network,
        ),
        (
            "network.data.address",
            |v| v["network"]["data"]["address"] = json!(""),
            Cause::Network,
        ),
        (
            "network.planes",
            |v| v["network"]["planes"] = json!(""),
            Cause::Network,
        ),
        (
            "a drive not mounted, its place empty",
            |v| v["drives"][0] = json!({"role": "state", "mount_point": "", "mounted": false}),
            Cause::Drives,
        ),
    ];
    for (name, edit, refused) in may_not {
        assert_eq!(cause(edited(edit)), refused, "{name} empty");
    }
}

#[test]
fn a_refusal_names_its_cause_in_the_formats_words() {
    let refused = read(b"[]").unwrap_err();
    assert_eq!(refused.cause.number(), 1);
    let text = refused.to_string();
    assert!(
        text.starts_with("refused by weaver-survey/2's cause 1:"),
        "{text}"
    );
    assert!(text.contains("not one JSON object"), "{text}");
    let network = cause(one_address("0.0.0.0", 24));
    assert_eq!(network.number(), 12);
    assert!(network.words().contains("a plane's address"));
}

#[test]
fn an_unknown_member_with_a_number_past_any_float_is_ignored() {
    assert!(reads(later("1e400")));
    let known = replaced("\"family\": 25,", "\"family\": 1e400,");
    assert_eq!(cause(known), Cause::Integer);
}

#[test]
fn nesting_is_read_to_the_parsers_own_bound_and_refused_past_it() {
    let nested = |levels: usize| later(&format!("{}{}", "[".repeat(levels), "]".repeat(levels)));
    assert!(reads(nested(126)), "the report and 126 arrays");
    assert_eq!(cause(nested(127)), Cause::Encoding);
    assert_eq!(cause(nested(20_000)), Cause::Encoding);
}

#[test]
fn an_unknown_member_with_a_lone_surrogate_is_ignored_and_names_compare_by_code_units() {
    assert!(reads(later(
        r#"{"\ud800": "\udfff", "\ud801": 1, "\ud83d\ude00": "\ud83d"}"#
    )));
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

/// A refusal's detail is for the console's log, never a page: printable
/// ASCII alone, whatever the report holds.
fn printable(detail: &str) -> bool {
    detail.bytes().all(|b| (0x20..0x7f).contains(&b))
}

#[test]
fn a_refusals_detail_quotes_a_value_cut_to_its_bound() {
    let long = "x".repeat(1000);
    let report = replaced(
        "\"format\": \"weaver-survey/2\"",
        &format!("\"format\": \"{long}\""),
    );
    let refused = read(report.as_bytes()).unwrap_err();
    assert_eq!(refused.cause, Cause::Format);
    assert!(
        refused.detail.contains(&"x".repeat(QUOTE_BOUND)),
        "{}",
        refused.detail
    );
    assert!(
        !refused.detail.contains(&"x".repeat(QUOTE_BOUND + 1)),
        "{}",
        refused.detail
    );
    assert!(refused.detail.contains("1000 bytes"), "{}", refused.detail);
    let name = "y".repeat(5000);
    let twice = later(&format!("{{\"{name}\": {{\"{name}\": 1, \"{name}\": 2}}}}"));
    let refused = read(twice.as_bytes()).unwrap_err();
    assert_eq!(refused.cause, Cause::NameTwice);
    assert!(
        refused.detail.len() < 4 * 3 * QUOTE_BOUND,
        "{}",
        refused.detail.len()
    );
}

#[test]
fn a_refusals_detail_escapes_a_control_or_non_ascii_byte() {
    let report = replaced(
        "\"format\": \"weaver-survey/2\"",
        "\"format\": \"\\u001b[2J\\u00e9\"",
    );
    let refused = read(report.as_bytes()).unwrap_err();
    assert_eq!(refused.cause, Cause::Format);
    assert!(
        refused.detail.contains("\\x1b[2J\\xc3\\xa9"),
        "{}",
        refused.detail
    );
    assert!(printable(&refused.detail), "{:?}", refused.detail);
    let twice = later("{\"\\u001b\": 1, \"\\u001b\": 2}");
    let refused = read(twice.as_bytes()).unwrap_err();
    assert_eq!(refused.cause, Cause::NameTwice);
    assert!(refused.detail.contains("\\x1b"), "{}", refused.detail);
    assert!(printable(&refused.detail), "{:?}", refused.detail);
    let address = read(one_address("\u{1b}[2J", 24).as_bytes()).unwrap_err();
    assert_eq!(address.cause, Cause::Network);
    assert!(printable(&address.detail), "{:?}", address.detail);
}

/// **What the pairing's first frame reads: the format alone** (Spec 8.2),
/// answered yes or no. Version 2's example, and reports of version 2 the
/// reader refuses for another cause, read yes. Another version, version 1
/// among them; a `format` missing, of another type or given twice at the
/// top; and a report from which the reader reads no member read no. The
/// reader's own detail for version 1 stays where it is read, on a stored
/// report (`refusal_4_...`).
#[test]
fn the_first_frame_reads_the_format_alone() {
    assert!(format_taken(EXAMPLE.as_bytes()));
    for other_cause in [
        r#"{"format":"weaver-survey/2"}"#.to_owned(),
        later(r#"{"a": 1, "a": 2}"#),
        replaced("\"family\": 25,", "\"family\": -1,"),
        edited(|v| v["stage"] = json!("drives")),
        r#"{"format":"weaver-survey/2","x":{"format":1,"format":2}}"#.to_owned(),
    ] {
        assert!(read(other_cause.as_bytes()).is_err());
        assert!(format_taken(other_cause.as_bytes()), "{other_cause}");
    }
    let mut latin = br#"{"format":"weaver-survey/2","x":"e"}"#.to_vec();
    let at = latin.len() - 3;
    latin[at] = 0xE9;
    let past = format!("{}{EXAMPLE}", " ".repeat(65537 - EXAMPLE.len()));
    for report in [
        VERSION_1.as_bytes(),
        br#"{"format":"weaver-survey/3"}"#,
        br#"{"format":"weaver-survey/2 "}"#,
        b"{}",
        br#"{"format":null}"#,
        br#"{"format":["weaver-survey/2"]}"#,
        br#"{"format":"weaver-survey/2","format":"weaver-survey/2"}"#,
        b"[]",
        b"{\"format\":\"weaver-survey/2\"}  \n\n",
        "\u{feff}{\"format\":\"weaver-survey/2\"}".as_bytes(),
        &latin,
        past.as_bytes(),
    ] {
        assert!(
            !format_taken(report),
            "{:?}",
            String::from_utf8_lossy(report)
        );
    }
}

/// **The walk for a name given twice is linear in the report** (the
/// Planner's fix round of 2026-11-26): a report within the bound whose one
/// unknown member has a name of 30,000 bytes and holds an array filling the
/// rest with zeros, some 16,800, reads, and a name given twice in an object
/// at that array's end is refused naming its whole path by its length. The
/// walk builds a path only where a name is found twice; before, it built
/// one of 30 KB for each item, some 500 MB written for one read. Measured
/// 2026-11-26 by the read's time in a debug build: 15.6 ms before and
/// 6.7 ms after on this report, and after, 1.30, 2.44 and 4.71 ms on
/// reports each twice the last (1.75, 5.00 and 11.08 ms before).
#[test]
fn a_long_name_over_a_long_array_reads_and_the_path_is_built_once() {
    let mut v: Value = serde_json::from_str(EXAMPLE).unwrap();
    let name = "n".repeat(30_000);
    let room = REPORT_BOUND - EXAMPLE.len() - name.len() - 16;
    let zeros = room / 2;
    v[name.as_str()] = Value::Array(vec![json!(0); zeros]);
    let report = v.to_string();
    assert!(report.len() <= REPORT_BOUND, "{}", report.len());
    assert!(zeros > 12_000, "{zeros}");
    assert!(reads(report));

    v[name.as_str()][zeros - 1] = json!({});
    let twice = v.to_string().replacen("{}", r#"{"a":1,"a":2}"#, 1);
    let refused = read(twice.as_bytes()).unwrap_err();
    assert_eq!(refused.cause, Cause::NameTwice);
    // The path, the name then the item's index, is cut for the detail and
    // named by its whole length.
    let path = format!("{name}[{}]", zeros - 1);
    assert!(
        refused
            .detail
            .ends_with(&format!("(cut, {} bytes) gives \"a\" twice", path.len())),
        "{}",
        &refused.detail[refused.detail.len().saturating_sub(80)..]
    );
    assert!(
        refused.detail.starts_with("nnnn"),
        "{}",
        &refused.detail[..40]
    );
}
