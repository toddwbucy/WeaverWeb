//! conforms: web-a-class-spanning-capabilities-is-warned-of
//!
//! **The compute-capability table** (Spec 6.1): each card model with its
//! compute capability and, where a source ties it to one, its family, every
//! value taken from the vendor's published documentation with the source
//! cited beside it. A model whose capability no citable source confirms is
//! left out, since a model the table does not know is admitted anyway and
//! named "capability unknown" (Spec 6.1). The table is read for the one
//! warning the declaration screen gives, a class spanning more than one
//! compute capability, and for nothing else: it never refuses.
//!
//! **Read on 2026-11-11.** The capabilities are NVIDIA's CUDA GPUs page,
//! every model it lists at the capabilities below, spelled as the page
//! spells them. A family is NVIDIA's architecture name, in its one word, and
//! comes from one of two sources, each cited on the row:
//!
//! - **a compatibility guide that ties the capability to the architecture**,
//!   for every model at that capability: Turing at 7.5, Ampere at 8.0 and
//!   8.6, Ada at 8.9, Hopper at 9.0 and Blackwell at 10.0;
//! - **the vendor's own model name**, where it carries the architecture's
//!   word, at a capability no fetched guide ties to a name: the RTX PRO
//!   Blackwell cards at 12.0.
//!
//! A model at a capability no guide ties, whose name carries no
//! architecture's word, has no family: it is reached by naming it in a
//! class's list and never through a family name.

/// NVIDIA's CUDA GPUs page, the source of every capability below.
pub const CUDA_GPUS: &str = "https://developer.nvidia.com/cuda-gpus";

/// The Turing compatibility guide: "the Turing architecture (compute
/// capability 7.5)".
pub const TURING: &str = "https://docs.nvidia.com/cuda/turing-compatibility-guide/index.html#building-turing-compatible-apps-using-cuda-10-0";

/// The Ampere compatibility guide: "the NVIDIA Ampere GPU architecture
/// (compute capability 8.0)".
pub const AMPERE_80: &str = "https://docs.nvidia.com/cuda/ampere-compatibility-guide/index.html#building-applications-using-cuda-toolkit-11-0";

/// The Ada compatibility guide: "the Ampere architecture (compute capability
/// 8.0 and 8.6)".
pub const AMPERE_86: &str = "https://docs.nvidia.com/cuda/ada-compatibility-guide/index.html#building-applications-using-cuda-toolkit-11-0-through-11-7";

/// The Ada compatibility guide: "the NVIDIA Ada GPU architecture (compute
/// capability 8.9)".
pub const ADA: &str = "https://docs.nvidia.com/cuda/ada-compatibility-guide/index.html#building-applications-using-cuda-toolkit-11-8";

/// The Hopper compatibility guide: "the Hopper architecture (compute
/// capability 9.0)".
pub const HOPPER: &str = "https://docs.nvidia.com/cuda/hopper-compatibility-guide/index.html#building-applications-using-cuda-toolkit-11-8";

/// The Blackwell compatibility guide: "the Blackwell architecture (compute
/// capability 10.0)".
pub const BLACKWELL: &str = "https://docs.nvidia.com/cuda/blackwell-compatibility-guide/index.html#building-applications-using-cuda-toolkit-12-8";

/// **Which of the two rules gave a row its family**, so the table can be
/// audited and a wrong inference is one row's fault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// A vendor's guide ties the row's capability to the architecture.
    Statement,
    /// The vendor's own model name carries the architecture's word.
    Name,
}

/// A row's family: the architecture's name, the rule that gave it, and
/// where that is stated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Family {
    pub name: &'static str,
    pub rule: Rule,
    pub source: &'static str,
}

/// One card model as the table holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    /// The model, spelled as the vendor's page spells it.
    pub model: &'static str,
    /// Its compute capability, as the page writes it.
    pub capability: &'static str,
    /// Where the capability is stated.
    pub capability_source: &'static str,
    /// Its family, or none where neither rule ties the model to one.
    pub family: Option<Family>,
}

const fn card(model: &'static str, capability: &'static str, family: Option<Family>) -> Card {
    Card {
        model,
        capability,
        capability_source: CUDA_GPUS,
        family,
    }
}

const fn stated(name: &'static str, source: &'static str) -> Option<Family> {
    Some(Family {
        name,
        rule: Rule::Statement,
        source,
    })
}

const TU: Option<Family> = stated("Turing", TURING);
const AM80: Option<Family> = stated("Ampere", AMPERE_80);
const AM86: Option<Family> = stated("Ampere", AMPERE_86);
const AD: Option<Family> = stated("Ada", ADA);
const HO: Option<Family> = stated("Hopper", HOPPER);
const BW: Option<Family> = stated("Blackwell", BLACKWELL);
/// Blackwell by the vendor's model name, at 12.0, which no fetched guide
/// ties to a name.
const BW_NAMED: Option<Family> = Some(Family {
    name: "Blackwell",
    rule: Rule::Name,
    source: CUDA_GPUS,
});

/// **The vendor's page as read on 2026-11-11**, its table's lines alone:
/// each capability, then the models the page lists under it. The table
/// below is checked against it line for line (`declarations_tests`), since
/// the vendor's page changes.
pub const READ: &str = include_str!("cuda-gpus-2026-11-11.txt");

/// **Every model the table knows**, in the page's order.
pub const TABLE: &[Card] = &[
    card("NVIDIA GB10 (DGX Spark)", "12.1", None),
    card("NVIDIA RTX Spark N1X", "12.1", None),
    card(
        "NVIDIA RTX PRO 6000 Blackwell Server Edition",
        "12.0",
        BW_NAMED,
    ),
    card(
        "NVIDIA RTX PRO 4500 Blackwell Server Edition",
        "12.0",
        BW_NAMED,
    ),
    card(
        "NVIDIA RTX PRO 6000 Blackwell Workstation Edition",
        "12.0",
        BW_NAMED,
    ),
    card(
        "NVIDIA RTX PRO 6000 Blackwell Max-Q Workstation Edition",
        "12.0",
        BW_NAMED,
    ),
    card(
        "NVIDIA RTX PRO 5500 Blackwell Workstation Edition",
        "12.0",
        BW_NAMED,
    ),
    card("NVIDIA RTX PRO 5000 Blackwell", "12.0", BW_NAMED),
    card(
        "NVIDIA RTX PRO 4500 Blackwell Workstation Edition",
        "12.0",
        BW_NAMED,
    ),
    card("NVIDIA RTX PRO 4000 Blackwell", "12.0", BW_NAMED),
    card(
        "NVIDIA RTX PRO 4000 Blackwell SFF Edition",
        "12.0",
        BW_NAMED,
    ),
    card("NVIDIA RTX PRO 2000 Blackwell", "12.0", BW_NAMED),
    card("GeForce RTX 5090", "12.0", None),
    card("GeForce RTX 5080", "12.0", None),
    card("GeForce RTX 5070 Ti", "12.0", None),
    card("GeForce RTX 5070", "12.0", None),
    card("GeForce RTX 5060 Ti", "12.0", None),
    card("GeForce RTX 5060", "12.0", None),
    card("GeForce RTX 5050", "12.0", None),
    card("Jetson T5000", "11.0", None),
    card("Jetson T4000", "11.0", None),
    card("NVIDIA GB300", "10.3", None),
    card("NVIDIA B300", "10.3", None),
    card("NVIDIA GB300 (DGX Station)", "10.3", None),
    card("NVIDIA GB200", "10.0", BW),
    card("NVIDIA B200", "10.0", BW),
    card("NVIDIA GH200", "9.0", HO),
    card("NVIDIA H200", "9.0", HO),
    card("NVIDIA H100", "9.0", HO),
    card("NVIDIA L4", "8.9", AD),
    card("NVIDIA L40", "8.9", AD),
    card("NVIDIA L40S", "8.9", AD),
    card("NVIDIA RTX 6000 Ada", "8.9", AD),
    card("NVIDIA RTX 5000 Ada", "8.9", AD),
    card("NVIDIA RTX 4500 Ada", "8.9", AD),
    card("NVIDIA RTX 4000 Ada", "8.9", AD),
    card("NVIDIA RTX 4000 SFF Ada", "8.9", AD),
    card("NVIDIA RTX 2000 Ada", "8.9", AD),
    card("GeForce RTX 4090", "8.9", AD),
    card("GeForce RTX 4080", "8.9", AD),
    card("GeForce RTX 4070 Ti", "8.9", AD),
    card("GeForce RTX 4070", "8.9", AD),
    card("GeForce RTX 4060 Ti", "8.9", AD),
    card("GeForce RTX 4060", "8.9", AD),
    card("GeForce RTX 4050", "8.9", AD),
    card("Jetson AGX Orin", "8.7", None),
    card("Jetson Orin NX", "8.7", None),
    card("Jetson Orin Nano", "8.7", None),
    card("NVIDIA A40", "8.6", AM86),
    card("NVIDIA A10", "8.6", AM86),
    card("NVIDIA A16", "8.6", AM86),
    card("NVIDIA A2", "8.6", AM86),
    card("NVIDIA RTX A6000", "8.6", AM86),
    card("NVIDIA RTX A5000", "8.6", AM86),
    card("NVIDIA RTX A4000", "8.6", AM86),
    card("NVIDIA RTX A3000", "8.6", AM86),
    card("NVIDIA RTX A2000", "8.6", AM86),
    card("GeForce RTX 3090 Ti", "8.6", AM86),
    card("GeForce RTX 3090", "8.6", AM86),
    card("GeForce RTX 3080 Ti", "8.6", AM86),
    card("GeForce RTX 3080", "8.6", AM86),
    card("GeForce RTX 3070 Ti", "8.6", AM86),
    card("GeForce RTX 3070", "8.6", AM86),
    card("GeForce RTX 3060 Ti", "8.6", AM86),
    card("GeForce RTX 3060", "8.6", AM86),
    card("GeForce RTX 3050 Ti", "8.6", AM86),
    card("GeForce RTX 3050", "8.6", AM86),
    card("NVIDIA A100", "8.0", AM80),
    card("NVIDIA A30", "8.0", AM80),
    card("NVIDIA T4", "7.5", TU),
    card("QUADRO RTX 8000", "7.5", TU),
    card("QUADRO RTX 6000", "7.5", TU),
    card("QUADRO RTX 5000", "7.5", TU),
    card("QUADRO RTX 4000", "7.5", TU),
    card("QUADRO RTX 3000", "7.5", TU),
    card("QUADRO T2000", "7.5", TU),
    card("NVIDIA T1200", "7.5", TU),
    card("NVIDIA T1000", "7.5", TU),
    card("NVIDIA T600", "7.5", TU),
    card("NVIDIA T500", "7.5", TU),
    card("NVIDIA T400", "7.5", TU),
    card("GeForce GTX 1650 Ti", "7.5", TU),
    card("NVIDIA TITAN RTX", "7.5", TU),
    card("GeForce RTX 2080 Ti", "7.5", TU),
    card("GeForce RTX 2080", "7.5", TU),
    card("GeForce RTX 2070", "7.5", TU),
    card("GeForce RTX 2060", "7.5", TU),
];

/// **A typed name as the table matches it**: ASCII case folded and every
/// run of whitespace one space, so "nvidia rtx 4000  ada" finds the row.
/// The saved document keeps what was typed.
fn folded(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// The table's row for a model, if it knows it.
pub fn model(name: &str) -> Option<&'static Card> {
    let key = folded(name);
    TABLE.iter().find(|card| folded(card.model) == key)
}

/// **The table's rows a family stands for**, every model it lists under
/// that family, or none where it knows no such family.
pub fn family(name: &str) -> Vec<&'static Card> {
    let key = folded(name);
    TABLE
        .iter()
        .filter(|card| card.family.is_some_and(|f| folded(f.name) == key))
        .collect()
}

/// Every family the table knows, once each, in the table's order.
pub fn families() -> Vec<&'static str> {
    let mut seen: Vec<&'static str> = Vec::new();
    for f in TABLE.iter().filter_map(|card| card.family) {
        if !seen.contains(&f.name) {
            seen.push(f.name);
        }
    }
    seen
}
