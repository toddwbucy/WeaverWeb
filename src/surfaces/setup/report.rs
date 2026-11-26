//! conforms: web-no-page-judges-the-models-disks-margin
//!
//! **The panes a box's report feeds** (Spec 6.2): the report on the Pair
//! page and the directories page's directories as mounted, each the report
//! as the source reads it (`crate::box_report`), and the status each
//! carries: read, refused or waiting. A refused report is drawn by its
//! arrival and its cause, by number and in the format's words, and never by
//! its text or the refusal's detail, which is the console's log's (Spec
//! 8.3). Every value is drawn through the templates' escaping.

use crate::box_report::{Report, STATE_DRIVE_FLOOR, gib, role_name};
use crate::survey::{Mount, Role, Survey};

use super::when;

/// **The report as a pane draws it**: its status, and in that status what
/// it shows.
pub(crate) enum Shown<T> {
    /// No report has arrived.
    Waiting,
    Refused {
        arrived: String,
        number: u8,
        words: &'static str,
    },
    Read {
        arrived: String,
        body: T,
    },
}

impl<T> Shown<T> {
    /// The pane's view of `report`, a read survey drawn by `body`.
    pub(crate) fn of(report: &Report, body: impl FnOnce(&Survey) -> T) -> Shown<T> {
        match report {
            Report::Waiting => Shown::Waiting,
            Report::Refused {
                arrived_at,
                refused,
            } => Shown::Refused {
                arrived: when(*arrived_at),
                number: refused.cause.number(),
                words: refused.cause.words(),
            },
            Report::Read { arrived_at, survey } => Shown::Read {
                arrived: when(*arrived_at),
                body: body(survey),
            },
        }
    }

    /// The class of the pane's status mark: `ok` read, `hold` waiting, and
    /// `fail` refused.
    pub(crate) fn class(&self) -> &'static str {
        match self {
            Shown::Waiting => "hold",
            Shown::Refused { .. } => "fail",
            Shown::Read { .. } => "ok",
        }
    }

    /// The status mark's word.
    pub(crate) fn word(&self) -> &'static str {
        match self {
            Shown::Waiting => "waiting",
            Shown::Refused { .. } => "refused",
            Shown::Read { .. } => "read",
        }
    }
}

/// **The report, read** (Spec 6.2): its `taken`, what a blank appliance
/// lacks, then its members grouped, the CPU's flags as a count with the
/// list folded, and the network.
pub(crate) struct PairingBody {
    pub(crate) taken: String,
    pub(crate) image_version: String,
    pub(crate) manifest_sha256: String,
    pub(crate) kernel_release: String,
    pub(crate) virtualization: String,
    pub(crate) machine_vendor: String,
    pub(crate) machine_product: String,
    pub(crate) machine_version: String,
    pub(crate) cpu_vendor: String,
    pub(crate) cpu_model_name: String,
    pub(crate) cpu_family_model_stepping: String,
    pub(crate) logical_cpus: u32,
    pub(crate) flags: String,
    pub(crate) flag_count: usize,
    pub(crate) memory: String,
    pub(crate) driver_version: String,
    pub(crate) driver_loaded: bool,
    pub(crate) cuda_runtime_version: String,
    pub(crate) cards: Vec<CardRow>,
    pub(crate) network: NetworkRows,
    pub(crate) lacks: Option<Lacks>,
}

/// **The network as the report gives it** (the brief of 2026-11-25): each
/// plane's address in the report's one spelling and its prefix length, and
/// whether the planes are separate or shared. Read only, judging nothing.
pub(crate) struct NetworkRows {
    pub(crate) management: String,
    pub(crate) management_prefix: u8,
    pub(crate) data: String,
    pub(crate) data_prefix: u8,
    pub(crate) planes: &'static str,
}

/// **What a blank appliance lacks** (the brief of 2026-11-25: "A blank
/// appliance pairs"): the drives the report gives not mounted, each by the
/// place the report names, one sentence per case.
pub(crate) enum Lacks {
    State(String),
    Models(String),
    Both { state: String, models: String },
}

impl Lacks {
    /// The drives `survey` reports not mounted, or none.
    fn of(survey: &Survey) -> Option<Lacks> {
        let not_mounted = |role: Role| {
            survey
                .drives
                .iter()
                .find(|d| d.role == role && d.mount == Mount::NotMounted)
                .map(|d| d.mount_point.clone())
        };
        match (not_mounted(Role::State), not_mounted(Role::Models)) {
            (None, None) => None,
            (Some(state), None) => Some(Lacks::State(state)),
            (None, Some(models)) => Some(Lacks::Models(models)),
            (Some(state), Some(models)) => Some(Lacks::Both { state, models }),
        }
    }
}

/// A card by its ordinal, model, PCI address and memory.
pub(crate) struct CardRow {
    pub(crate) ordinal: u32,
    pub(crate) model: String,
    pub(crate) pci_address: String,
    pub(crate) memory: String,
}

impl PairingBody {
    pub(crate) fn of(survey: &Survey) -> PairingBody {
        PairingBody {
            taken: when(survey.taken),
            image_version: survey.image.version.clone(),
            manifest_sha256: survey.image.manifest_sha256.clone(),
            kernel_release: survey.kernel_release.clone(),
            virtualization: survey.machine.virtualization.clone(),
            machine_vendor: survey.machine.vendor.clone(),
            machine_product: survey.machine.product.clone(),
            machine_version: survey.machine.version.clone(),
            cpu_vendor: survey.cpu.vendor.clone(),
            cpu_model_name: survey.cpu.model_name.clone(),
            cpu_family_model_stepping: format!(
                "{}, {}, {}",
                survey.cpu.family, survey.cpu.model, survey.cpu.stepping
            ),
            logical_cpus: survey.cpu.logical_cpus,
            flags: survey.cpu.flags.join(" "),
            flag_count: survey.cpu.flags.len(),
            memory: gib(survey.memory_total_bytes),
            driver_version: survey.driver.version.clone(),
            driver_loaded: survey.driver.loaded,
            cuda_runtime_version: survey.cuda_runtime_version.clone(),
            cards: survey
                .cards
                .iter()
                .map(|card| CardRow {
                    ordinal: card.ordinal,
                    model: card.model.clone(),
                    pci_address: card.pci_address.clone(),
                    memory: gib(card.memory_bytes),
                })
                .collect(),
            network: NetworkRows {
                management: survey.network.management.address.to_string(),
                management_prefix: survey.network.management.prefix_length,
                data: survey.network.data.address.to_string(),
                data_prefix: survey.network.data.prefix_length,
                planes: survey.network.planes.word(),
            },
            lacks: Lacks::of(survey),
        }
    }
}

/// **The report, read** (Spec 6.2): its `taken` and
/// each directory as mounted, with its judgments.
pub(crate) struct DrivesBody {
    pub(crate) taken: String,
    pub(crate) drives: Vec<DriveRow>,
}

/// A drive by its role and mount point, what it reports mounted, and its
/// judgments.
pub(crate) struct DriveRow {
    pub(crate) role: &'static str,
    pub(crate) mount_point: String,
    pub(crate) mounted: Option<Mounted>,
    pub(crate) judgments: Vec<Judgment>,
}

/// A mounted drive's label, filesystem, size, free space and project quota.
pub(crate) struct Mounted {
    pub(crate) label: String,
    pub(crate) filesystem: String,
    pub(crate) size: String,
    pub(crate) free: String,
    pub(crate) project_quota: &'static str,
}

/// **A judgment shown on a drive**: a status mark and, where it says more,
/// its sentence.
pub(crate) struct Judgment {
    pub(crate) class: &'static str,
    pub(crate) word: &'static str,
    pub(crate) words: String,
}

impl Judgment {
    fn of(class: &'static str, word: &'static str, words: impl Into<String>) -> Judgment {
        Judgment {
            class,
            word,
            words: words.into(),
        }
    }
}

/// The models directory's need as the pages judge a directory by it: its
/// bytes, and as the page states it.
pub(crate) struct NeedOf<'a> {
    pub(crate) bytes: u64,
    pub(crate) shown: &'a str,
}

impl DrivesBody {
    /// **The directories as mounted, judged** (Spec 6.2, the operator's
    /// rulings of 2026-11-17, 2026-11-23 and 2026-11-24): one not mounted
    /// holds Confirm; a state directory whose filesystem is under 9 GiB
    /// holds Confirm by its exact size; a models directory whose free space
    /// is under the picks' need is a warning and not a hold, and one with
    /// room says so. No judgment names the models disk's margin, which the
    /// New agent states as guidance (the operator's ruling of
    /// 2026-11-24: "No page warns").
    pub(crate) fn of(survey: &Survey, need: Option<NeedOf<'_>>) -> DrivesBody {
        let drives = survey
            .drives
            .iter()
            .map(|drive| {
                let (mounted, judgments) = match &drive.mount {
                    Mount::NotMounted => (
                        None,
                        vec![Judgment::of(
                            "fail",
                            "not mounted",
                            "Confirm holds until the box mounts it.",
                        )],
                    ),
                    Mount::Mounted {
                        label,
                        filesystem,
                        bytes,
                        free_bytes,
                        project_quota,
                    } => {
                        let judgment = match drive.role {
                            Role::State if *bytes < STATE_DRIVE_FLOOR => Judgment::of(
                                "fail",
                                "under 9 GiB",
                                format!(
                                    "Its filesystem is {bytes} bytes, under 9 GiB \
                                     ({STATE_DRIVE_FLOOR} bytes): Confirm holds."
                                ),
                            ),
                            Role::State => Judgment::of("ok", "at least 9 GiB", ""),
                            Role::Models => match &need {
                                None => Judgment::of(
                                    "ok",
                                    "mounted",
                                    "No model is picked, so its free space is not judged.",
                                ),
                                Some(need) if *free_bytes < need.bytes => Judgment::of(
                                    "hold",
                                    "warning",
                                    format!(
                                        "Its free space is under the picks' need of {}: a \
                                         warning and not a hold, since the box's pull is an \
                                         act of its own.",
                                        need.shown
                                    ),
                                ),
                                Some(need) => Judgment::of(
                                    "ok",
                                    "room",
                                    format!(
                                        "Its free space holds the picks' need of {}.",
                                        need.shown
                                    ),
                                ),
                            },
                        };
                        (
                            Some(Mounted {
                                label: label.clone(),
                                filesystem: filesystem.clone(),
                                size: gib(*bytes),
                                free: gib(*free_bytes),
                                project_quota: if *project_quota { "on" } else { "off" },
                            }),
                            vec![judgment],
                        )
                    }
                };
                DriveRow {
                    role: role_name(drive.role),
                    mount_point: drive.mount_point.clone(),
                    mounted,
                    judgments,
                }
            })
            .collect();
        DrivesBody {
            taken: when(survey.taken),
            drives,
        }
    }
}
