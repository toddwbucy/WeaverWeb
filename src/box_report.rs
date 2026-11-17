//! **What the box reports to the setup pages** (Spec 6.2): the survey's
//! cards, which the placement page offers as the devices, and the box's
//! members of the deployment tuple, which Confirm reads. Both arrive over
//! the paired link once the enrollment's act (Spec 8.2) builds their
//! source. Until it does the server answers none (`NoBox`), so the
//! devices and the box's members are held and Confirm holds with them. The
//! seam is how the setup pages' tests supply a report, and how the
//! enrollment's act will.

use serde::{Deserialize, Serialize};

/// A card of the survey's report: the ordinal the report gives it in PCI
/// bus order, the order the agent numbers its devices in, and its identity
/// as the report names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub ordinal: i32,
    pub identity: String,
}

/// **The box's members of the deployment tuple** (Spec 6.2), as the box
/// reports them: the image version, the driver, the cards by identity, the
/// guest CPU, the machine and the drives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoxMembers {
    pub image_version: String,
    pub driver: String,
    pub cards: Vec<String>,
    pub guest_cpu: String,
    pub machine: String,
    pub drives: Vec<String>,
}

/// **The source of what the box reports for a setup.**
pub trait BoxReports: Send + Sync {
    /// The survey's cards, where the survey has reported.
    fn survey(&self, setup_id: &str) -> Option<Vec<Card>>;
    /// The box's members, where the box has reported them.
    fn members(&self, setup_id: &str) -> Option<BoxMembers>;
}

/// **The server's source until the enrollment's act**: no box reports.
pub struct NoBox;

impl BoxReports for NoBox {
    fn survey(&self, _setup_id: &str) -> Option<Vec<Card>> {
        None
    }

    fn members(&self, _setup_id: &str) -> Option<BoxMembers> {
        None
    }
}
