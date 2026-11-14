//! conforms: web-the-agent-page-sets-what-the-agent-reports-beside-the-declaration
//!
//! **The agent page's Declare pane** (Spec 2.13; the operator's word of
//! 2026-11-13, "yes the agent page should show its declaration"): the
//! declaration saved under the agent's name, its parts, and each member the
//! agent reports set beside the same member of it.
//!
//! **By name, never by digest.** A declaration names no box, so one lineage
//! stands behind every register row of the name, and the pane names it as
//! "the declaration saved under this name", the head of that lineage. The
//! console writes no loadable file and records no digest, so the pane never
//! says which version an agent was loaded from; that waits on Provision.
//!
//! **The truth line, state by state**: no lineage of the name; a lineage
//! and no load on the row; a lineage and a load the agent reports, each
//! reported member set beside the head (`declaration::reported`), with the
//! sentence that a difference may only mean an older version was the
//! source. A part the agent never reports is shown as declared and marked
//! so.
//!
//! **Access** (the Planner's line, which the operator rules by the merge):
//! every grant holder of the agent sees the parts; the link to the
//! declaration screen stays an admin's, as the screen does; the prompt is
//! shown by its length in characters, its text never on this page, and its
//! digest to an admin alone: a security review found on 2026-11-13 that an
//! unkeyed digest handed to a reader who may not read the text lets them
//! confirm a guess at it, and on this page it compares with nothing. The
//! other live register rows of the name are listed to an admin in full and
//! to a person as far as their own grants reach, since an agent without a
//! grant answers to them exactly as one that does not exist.
//!
//! **The pane fails by itself** (the Planner's read of 2026-11-13): a head
//! this build does not read (a row saved by a newer build, read after a
//! rollback) and a store read inside the pane that fails (the head lookup,
//! the other rows of the name, the head's author) are logged and said in
//! the pane, and never an error for the page, so the page that unloads an
//! agent stands whatever the declaration's state. The reads are taken first
//! and handed to [`DeclarePane::compose`], which shows nothing it did not
//! read.

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::declaration::render::{real, requirement_words};
use crate::declaration::reported::{self, Member, Source, Verdict};
use crate::declaration::{Document, Function, Value64};
use crate::link::register::{Agent, CredentialState};
use crate::store::Store;
use crate::store::declarations::Saved;
use crate::store::identity::hex;

/// **What the head lookup read** (Spec 2.13): the head of the lineage saved
/// under the agent's name, no lineage of the name, or a read that failed.
#[derive(Clone, Copy)]
pub enum Head<'a> {
    Saved(&'a Saved),
    NoneSaved,
    Unread,
}

impl<'a> Head<'a> {
    /// The lookup's result as the pane and the way back read it.
    pub fn of(read: &'a anyhow::Result<Option<Saved>>) -> Head<'a> {
        match read {
            Ok(Some(saved)) => Head::Saved(saved),
            Ok(None) => Head::NoneSaved,
            Err(_) => Head::Unread,
        }
    }
}

/// **What the pane shows**.
pub struct DeclarePane {
    /// The agent's name, which the lineage holds.
    pub name: String,
    /// The head of the lineage saved under the name, none where none is.
    pub saved: Option<SavedView>,
    /// The other live register rows of the name the reader may see.
    pub others: Vec<String>,
    /// Whether the read of the other rows failed, which the pane says.
    pub others_unread: bool,
    /// The truth line.
    pub truth: String,
    /// The parts, each with how it stands against the agent's report.
    pub parts: Vec<PartRow>,
    /// The way to a new declaration, an admin's alone, where none is saved.
    pub declare: bool,
}

/// The head, as the pane names it.
pub struct SavedView {
    pub version: i64,
    pub saved_at: String,
    pub author: String,
    /// The declaration screen's page of the head, an admin's alone.
    pub href: Option<String>,
}

/// One part of the declaration.
pub struct PartRow {
    pub part: String,
    pub declared: String,
    /// How it stands against the agent's report, in words.
    pub against: String,
    /// The status the words carry: `ok`, `fail`, `hold`, or none.
    pub status: &'static str,
}

/// The words of a part the agent never reports.
pub const NOT_REPORTED: &str = "not reported by the agent";
/// The words of a reported part where the row holds no load to read.
pub const NO_LOAD: &str = "no load to set it beside";
/// The words of an author whose read failed.
pub const AUTHOR_UNREAD: &str = "an author that could not be read just now";

/// **The pane alone, rendered**: the fragment `agent_page.html` includes,
/// for the tests that render a pane from what its reads returned.
#[cfg(test)]
#[derive(askama::Template)]
#[template(path = "declare_pane.html")]
pub struct PaneView<'a> {
    pub declaration: &'a DeclarePane,
}

/// **Where a load stands to compare against**: its tuple and its source.
struct Report<'a> {
    tuple: &'a Value,
    source: Source,
}

impl DeclarePane {
    /// **The pane for `row`**, read for a person who is or is not an admin:
    /// the other rows of the name and the head's author read here, each read
    /// that fails logged and handed on as unread, so the pane never fails
    /// the page.
    pub async fn of(
        store: &Store,
        row: &Agent,
        head: Head<'_>,
        admin: bool,
        person: &str,
    ) -> DeclarePane {
        let others = match others(store, row, admin, person).await {
            Ok(others) => Some(others),
            Err(e) => {
                tracing::error!(agent = %row.agent_id.as_str(), "the Declare pane could not read the other rows of the name: {e:#}");
                None
            }
        };
        let author = match head {
            Head::Saved(saved) => match store.author(saved.author.as_deref()).await {
                Ok(author) => Some(super::declarations::author_words(author)),
                Err(e) => {
                    tracing::error!(agent = %row.agent_id.as_str(), "the Declare pane could not read the head's author: {e:#}");
                    None
                }
            },
            Head::NoneSaved | Head::Unread => None,
        };
        DeclarePane::compose(row, head, others, author, admin)
    }

    /// **The pane from what its reads returned**, each `None` a read that
    /// failed: an unread head says so and shows nothing; a head this build
    /// does not read is named by its version, its screen linked for an
    /// admin, and shows no parts; unread others and an unread author are
    /// said so.
    pub fn compose(
        row: &Agent,
        head: Head<'_>,
        others: Option<Vec<String>>,
        author: Option<String>,
        admin: bool,
    ) -> DeclarePane {
        let others_unread = others.is_none();
        let others = others.unwrap_or_default();
        let saved = match head {
            Head::Saved(saved) => saved,
            Head::NoneSaved => {
                return DeclarePane {
                    name: row.name.clone(),
                    saved: None,
                    others,
                    others_unread,
                    truth: format!("No declaration is saved under the name {}.", row.name),
                    parts: Vec::new(),
                    declare: admin,
                };
            }
            Head::Unread => {
                return DeclarePane {
                    name: row.name.clone(),
                    saved: None,
                    others,
                    others_unread,
                    truth: format!(
                        "Whether a declaration is saved under the name {} could not be read just now.",
                        row.name
                    ),
                    parts: Vec::new(),
                    declare: false,
                };
            }
        };
        let view = SavedView {
            version: saved.version,
            saved_at: saved.saved_at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
            author: author.unwrap_or_else(|| AUTHOR_UNREAD.to_owned()),
            href: admin.then(|| format!("/declarations/{}", saved.de)),
        };
        // The head is read as the declaration screen reads a saved row, a
        // member a row keeps from an earlier grammar discarded (Spec 6.1's
        // withdrawn placement), so a row saved before 2026-11-14 reads here.
        let document = match Document::read_saved(&saved.body) {
            Ok(document) => document,
            Err(e) => {
                tracing::warn!(agent = %row.agent_id.as_str(), declaration = %saved.de, "the declaration saved under the name does not read in this build: {e}");
                return DeclarePane {
                    name: row.name.clone(),
                    saved: Some(view),
                    others,
                    others_unread,
                    truth: format!(
                        "The declaration saved under the name {}, version {}, does not read in this \
                         build, so its parts are not shown.",
                        row.name, saved.version
                    ),
                    parts: Vec::new(),
                    declare: false,
                };
            }
        };
        let report = match (&row.tuple, row.tuple_source.as_deref().and_then(Source::of)) {
            (Some(tuple), Some(source)) => Some(Report { tuple, source }),
            _ => None,
        };
        let truth = match (&report, &row.tuple) {
            (Some(report), _) => {
                let by = match report.source {
                    Source::Show => "its answer to show",
                    Source::Event => "its trace's load event",
                };
                let at = row
                    .tuple_at
                    .map(|at| format!(", dated {}", at.format("%Y-%m-%d %H:%M:%S UTC")))
                    .unwrap_or_default();
                format!(
                    "The agent reports, by {by}{at}, that it loaded a declaration file of digest {}. \
                     The console wrote no file for this agent, so which saved version, if any, that \
                     file was derived from is not known. Each member the agent reports is set beside \
                     version {}, the head: a difference may only mean an older version was the source.",
                    reported::digest(report.tuple).unwrap_or("it does not name"),
                    saved.version
                )
            }
            (None, Some(_)) => "The register's row holds a load from a source this page does not \
                                read, so nothing the agent reports stands beside the declaration."
                .to_owned(),
            (None, None) => "The register's row holds no load, so nothing the agent reports \
                             stands beside the declaration."
                .to_owned(),
        };
        DeclarePane {
            name: row.name.clone(),
            saved: Some(view),
            others,
            others_unread,
            truth,
            parts: parts(&document, report.as_ref(), admin),
            declare: false,
        }
    }
}

/// **The other live register rows of the name** the reader may see: every
/// one for an admin, those the person holds a grant on otherwise.
async fn others(
    store: &Store,
    row: &Agent,
    admin: bool,
    person: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = if admin {
        store.agents().await?
    } else {
        store.readable_agents(person).await?
    };
    Ok(rows
        .into_iter()
        .filter(|a| {
            a.name == row.name
                && a.agent_id != row.agent_id
                && (a.gate.state == CredentialState::Live || a.admin.state == CredentialState::Live)
        })
        .map(|a| format!("{}/{} ({})", a.r#box, a.name, a.agent_id.as_str()))
        .collect())
}

/// A part the agent never reports.
fn unreported(part: String, declared: String) -> PartRow {
    PartRow {
        part,
        declared,
        against: NOT_REPORTED.to_owned(),
        status: "",
    }
}

/// A part the agent reports, against the report where one stands.
fn compared(
    document: &Document,
    member: Member,
    part: String,
    report: Option<&Report<'_>>,
) -> PartRow {
    let declared = reported::declared(document, member).shown();
    let Some(report) = report else {
        return PartRow {
            part,
            declared,
            against: NO_LOAD.to_owned(),
            status: "",
        };
    };
    let verdict = reported::verdict(document, member, report.tuple, report.source);
    let status = match verdict {
        Verdict::Equal | Verdict::EqualAsReported(_) => "ok",
        Verdict::Differs { .. } => "fail",
        Verdict::NotInReport | Verdict::Unreadable(_) => "hold",
    };
    PartRow {
        part,
        declared,
        against: verdict.words(),
        status,
    }
}

/// **The declaration's parts, in the screen's order**, each a part the agent
/// reports set beside its report or one it never reports marked so; the
/// prompt's digest an admin's alone.
fn parts(document: &Document, report: Option<&Report<'_>>, admin: bool) -> Vec<PartRow> {
    let mut rows = Vec::new();
    for (i, spu) in document.boxes.iter().enumerate() {
        let n = i + 1;
        for model in &spu.models {
            let part = format!("SPU box {n}, the {}", model.function.word());
            match model.function {
                Function::Decoder => rows.push(compared(document, Member::Artifact, part, report)),
                Function::Classifier => rows.push(unreported(part, model.artifact.clone())),
            }
            if let Some(adapter) = &model.adapter {
                rows.push(unreported(
                    format!("SPU box {n}, the {}'s adapter", model.function.word()),
                    format!(
                        "{}, sha256 {}, scale {}",
                        adapter.artifact,
                        adapter.sha256,
                        real(adapter.scale)
                    ),
                ));
            }
        }
        rows.push(unreported(
            format!("SPU box {n}, its GPU requirement"),
            requirement_words(&spu.gpu),
        ));
    }
    let present = document.sampler.present();
    for (member, _) in crate::declaration::SAMPLER {
        let value = present
            .iter()
            .find(|(n, _)| *n == member)
            .map(|(_, v)| match v {
                Value64::Real(x) => real(*x),
                Value64::Count(c) => c.to_string(),
            })
            .unwrap_or_else(|| "not declared on this row".to_owned());
        rows.push(unreported(format!("the tunable value {member}"), value));
    }
    for member in [
        Member::ResidualReadout,
        Member::Surprisal,
        Member::TurnContext,
        Member::StateElection,
        Member::Store,
    ] {
        rows.push(compared(
            document,
            member,
            member.label().to_owned(),
            report,
        ));
    }
    rows.push(unreported(
        "the gate's tools".to_owned(),
        if document.tools.is_empty() {
            "none".to_owned()
        } else {
            document.tools.join(", ")
        },
    ));
    rows.push(unreported(
        "the prompt".to_owned(),
        match &document.prompt {
            Some(prompt) => prompt_words(prompt, admin),
            None => "none".to_owned(),
        },
    ));
    rows
}

/// **The prompt by its length**, never its text: its length in characters,
/// and to an admin alone the sha256 of its UTF-8 bytes.
pub fn prompt_words(prompt: &str, admin: bool) -> String {
    let length = format!("{} characters", prompt.chars().count());
    if admin {
        format!(
            "{length}, sha256 {}",
            hex(&Sha256::digest(prompt.as_bytes()))
        )
    } else {
        length
    }
}
