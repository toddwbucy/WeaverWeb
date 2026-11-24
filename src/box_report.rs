//! **What the box reports to the setup pages** (Spec 6.2): the cards the
//! placement page offers as the devices, and the box's members of the
//! deployment tuple, which Confirm reads. Both come from the reports the
//! setup keeps (`agent_setup_report`, each stage's latest, verbatim), read
//! by the survey's reader (`crate::survey`, Spec 8.3).
//!
//! **Which report feeds what** is 6.2's: the cards, every box member, and
//! the directories page's directories, from the latest report of either
//! stage, so one tuple stands on one report (the Planner's rulings of
//! 2026-11-24 on the plan and on its review: the
//! operator lays the disks before pairing, so a drives-stage report may
//! never come, and one that is sent is still read). A report absent,
//! refused, or kept under one stage while naming the other feeds nothing,
//! and what it would feed holds, naming it. A store that fails to answer is
//! a fault, never an absent report.
//!
//! **The seam** (`BoxReports`) is how the setup pages' tests supply a
//! report. It reads on the connection it is handed, so Confirm reads the
//! reports inside its own transaction, after its update has locked the
//! setup's row: a report's write holds that row shared, so Confirm reads
//! every report written before it and none written after.

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;

use crate::survey::{self, Mount, Refused, Role, Stage, Survey};

/// **The state drive's figure** (Spec 6.2, the operator's ruling of
/// 2026-11-17): a state drive whose filesystem's size is under 9 GiB,
/// 9663676416 bytes, holds Confirm. The state disk is required at 10 GiB at
/// least, and the judgment is made on the filesystem's size because a
/// filesystem reports less than its disk: a 10 GiB disk formatted ext4 with
/// its defaults reports about 9.75 GiB.
pub const STATE_DRIVE_FLOOR: u64 = 9 * GIB;

/// **The models disk's margin** (the operator's ruling of 2026-11-23: "a
/// models disk at least 10 GB larger than the model"): a models directory
/// whose filesystem is under the picks' need and this margin is a warning
/// on the models and directories pages, and nothing is refused for the
/// margin alone. Taken in GiB, as the state disk's 10 is. A placeholder of
/// the Spec until the operator rules how the margin is held.
pub const MODELS_MARGIN: u64 = 10 * GIB;

const GIB: u64 = 1024 * 1024 * 1024;

/// A card the placement page offers: its ordinal in PCI bus order, the
/// order the agent numbers its devices in, and its model as the report
/// names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub ordinal: i32,
    pub model: String,
}

/// **One stage's kept report, as the source reads it.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Report {
    /// None has arrived.
    Waiting,
    /// Refused by the reader, for a cause of the format's section 3.
    Refused {
        arrived_at: DateTime<Utc>,
        refused: Refused,
    },
    /// Read whole, but its `stage` names the other stage than the one it
    /// was kept under.
    Misfiled {
        arrived_at: DateTime<Utc>,
        names: Stage,
    },
    /// Read whole, under its own stage.
    Read {
        arrived_at: DateTime<Utc>,
        survey: Box<Survey>,
    },
}

/// **The setup's two reports**, each stage's latest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reports {
    pub pairing: Report,
    pub drives: Report,
}

/// **The box's members of the deployment tuple** (Spec 6.2, the operator's
/// rulings of 2026-11-17), from the latest report of either stage: the machine's
/// facts and none of the moment's, so no drive's free space and not
/// whether the driver is loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoxMembers {
    pub report: ReportMember,
    pub image: ImageMember,
    pub driver_version: String,
    pub cuda_runtime_version: String,
    pub cards: Vec<CardMember>,
    pub cpu: CpuMember,
    pub machine: MachineMember,
    pub drives: Vec<DriveMember>,
}

/// The report the members were read from, by its stage and `taken`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportMember {
    pub stage: String,
    pub taken: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageMember {
    pub version: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardMember {
    pub ordinal: u32,
    pub model: String,
    pub pci_address: String,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuMember {
    pub vendor: String,
    pub model_name: String,
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub logical_cpus: u32,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineMember {
    pub virtualization: String,
    pub vendor: String,
    pub product: String,
    pub version: String,
    pub memory_total_bytes: u64,
    pub kernel_release: String,
}

/// A drive by its role, label, filesystem, size and project quota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveMember {
    pub role: String,
    pub label: String,
    pub filesystem: String,
    pub bytes: u64,
    pub project_quota: bool,
}

/// A stage by the name the format gives it.
pub fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Pairing => "pairing",
        Stage::Drives => "drives",
    }
}

/// A drive's role by the name the format gives it.
pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::State => "state",
        Role::Models => "models",
    }
}

/// A size in GiB, as the flow states every size (Spec 6.2).
pub fn gib(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / GIB as f64)
}

impl Report {
    /// One kept report read: refused, misfiled, or read under `stage`.
    fn of(stage: Stage, report: &str, arrived_at: DateTime<Utc>) -> Report {
        match survey::read(report.as_bytes()) {
            Err(refused) => Report::Refused {
                arrived_at,
                refused,
            },
            Ok(survey) if survey.stage != stage => Report::Misfiled {
                arrived_at,
                names: survey.stage,
            },
            Ok(survey) => Report::Read {
                arrived_at,
                survey: Box::new(survey),
            },
        }
    }

    /// The survey, where the report is read under its own stage.
    pub fn survey(&self) -> Option<&Survey> {
        match self {
            Report::Read { survey, .. } => Some(survey.as_ref()),
            _ => None,
        }
    }

    /// When the report arrived, where one has.
    pub fn arrived_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Report::Waiting => None,
            Report::Refused { arrived_at, .. }
            | Report::Misfiled { arrived_at, .. }
            | Report::Read { arrived_at, .. } => Some(*arrived_at),
        }
    }
}

impl Reports {
    /// **The setup's reports read**, from the rows the store hands over:
    /// stage, the verbatim report and its arrival. A stage with no row is
    /// waiting.
    pub fn of(rows: &[(String, String, DateTime<Utc>)]) -> Reports {
        let mut reports = Reports {
            pairing: Report::Waiting,
            drives: Report::Waiting,
        };
        for (stage, report, arrived_at) in rows {
            match stage.as_str() {
                "pairing" => reports.pairing = Report::of(Stage::Pairing, report, *arrived_at),
                "drives" => reports.drives = Report::of(Stage::Drives, report, *arrived_at),
                _ => {}
            }
        }
        reports
    }

    /// **The cards the placement page offers and Confirm checks**: the
    /// latest report's of either stage, and none where the latest does not
    /// read, so the devices and the box's members stand on one report.
    pub fn cards(&self) -> Option<Vec<Card>> {
        self.cards_from().map(|(_, cards)| cards)
    }

    /// The cards the placement page offers, with the stage whose report
    /// brings them: the latest report's, where it reads.
    pub fn cards_from(&self) -> Option<(Stage, Vec<Card>)> {
        let (stage, latest) = self.latest();
        let survey = latest.survey()?;
        Some((
            stage,
            survey
                .cards
                .iter()
                .filter_map(|card| {
                    Some(Card {
                        ordinal: i32::try_from(card.ordinal).ok()?,
                        model: card.model.clone(),
                    })
                })
                .collect(),
        ))
    }

    /// **The latest report of either stage**, by its arrival, and its
    /// stage: the drives stage's on a tie, and the drives stage's waiting
    /// where neither has arrived.
    pub fn latest(&self) -> (Stage, &Report) {
        match (self.pairing.arrived_at(), self.drives.arrived_at()) {
            (Some(pairing), Some(drives)) if pairing > drives => (Stage::Pairing, &self.pairing),
            (Some(_), None) => (Stage::Pairing, &self.pairing),
            _ => (Stage::Drives, &self.drives),
        }
    }

    /// **The box's members Confirm reads**, from the latest report of
    /// either stage, or why Confirm holds: no report arrived, the latest
    /// refused or misfiled, a drive not mounted, or the state drive's
    /// filesystem under the figure, each named.
    pub fn members(&self) -> Result<BoxMembers, String> {
        let (stage, latest) = self.latest();
        let stage = stage_name(stage);
        let survey = match latest {
            Report::Waiting => {
                return Err("no report of either stage has arrived".to_owned());
            }
            Report::Refused { refused, .. } => {
                return Err(format!(
                    "the {stage} stage's report, the latest, is refused for cause {}: {}",
                    refused.cause.number(),
                    refused.cause.words()
                ));
            }
            Report::Misfiled { names, .. } => {
                return Err(format!(
                    "the report kept as the {stage} stage's, the latest, names its stage {}",
                    stage_name(*names)
                ));
            }
            Report::Read { survey, .. } => survey,
        };
        let mut holds = Vec::new();
        let mut drives = Vec::new();
        for drive in &survey.drives {
            let role = role_name(drive.role);
            match &drive.mount {
                Mount::NotMounted => holds.push(format!(
                    "the {role} drive at {} is not mounted",
                    drive.mount_point
                )),
                Mount::Mounted {
                    label,
                    filesystem,
                    bytes,
                    project_quota,
                    ..
                } => {
                    if drive.role == Role::State && *bytes < STATE_DRIVE_FLOOR {
                        holds.push(format!(
                            "the state drive's filesystem is {bytes} bytes, under 9 GiB \
                             ({STATE_DRIVE_FLOOR} bytes)"
                        ));
                    }
                    drives.push(DriveMember {
                        role: role.to_owned(),
                        label: label.clone(),
                        filesystem: filesystem.clone(),
                        bytes: *bytes,
                        project_quota: *project_quota,
                    });
                }
            }
        }
        if !holds.is_empty() {
            return Err(holds.join(", and "));
        }
        Ok(BoxMembers {
            report: ReportMember {
                stage: stage_name(survey.stage).to_owned(),
                taken: survey.taken,
            },
            image: ImageMember {
                version: survey.image.version.clone(),
                manifest_sha256: survey.image.manifest_sha256.clone(),
            },
            driver_version: survey.driver.version.clone(),
            cuda_runtime_version: survey.cuda_runtime_version.clone(),
            cards: survey
                .cards
                .iter()
                .map(|card| CardMember {
                    ordinal: card.ordinal,
                    model: card.model.clone(),
                    pci_address: card.pci_address.clone(),
                    memory_bytes: card.memory_bytes,
                })
                .collect(),
            cpu: CpuMember {
                vendor: survey.cpu.vendor.clone(),
                model_name: survey.cpu.model_name.clone(),
                family: survey.cpu.family,
                model: survey.cpu.model,
                stepping: survey.cpu.stepping,
                logical_cpus: survey.cpu.logical_cpus,
                flags: survey.cpu.flags.clone(),
            },
            machine: MachineMember {
                virtualization: survey.machine.virtualization.clone(),
                vendor: survey.machine.vendor.clone(),
                product: survey.machine.product.clone(),
                version: survey.machine.version.clone(),
                memory_total_bytes: survey.memory_total_bytes,
                kernel_release: survey.kernel_release.clone(),
            },
            drives,
        })
    }
}

/// **The source of what the box reports for a setup**, read on the
/// connection handed to it.
pub trait BoxReports: Send + Sync {
    /// The setup's reports, each stage's latest, read. A store that fails
    /// to answer is an error, never an absent report.
    fn reports<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        setup_id: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Reports>>;
}

/// **The server's source**: the reports the setup keeps.
pub struct Kept;

impl BoxReports for Kept {
    fn reports<'a>(
        &'a self,
        conn: &'a mut PgConnection,
        setup_id: &'a str,
    ) -> BoxFuture<'a, anyhow::Result<Reports>> {
        Box::pin(async move {
            let rows = crate::store::setup_ask::setup_reports_on(conn, setup_id).await?;
            Ok(Reports::of(&rows))
        })
    }
}
