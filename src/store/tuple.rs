//! conforms: web-the-deployment-tuple-refuses-any-update-or-delete-and-a-setup-is-confirmed-once
//! conforms: web-the-members-the-box-is-laid-from-are-the-confirmed-tuples-and-none-stand-before-confirm
//!
//! **The deployment tuple** (Spec 6.2, migration `0029`): every member the
//! box is laid from, each read from its source, and Confirm, which writes
//! them once. `members_on` reads each member or says why it is not read,
//! for the confirm page and again inside Confirm's transaction, so the
//! tuple holds what the sources held at its commit. The laying members are
//! read from the tuple alone, so none stands before Confirm.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};

use crate::box_report::{BoxMembers, Card};
use crate::gguf::Header;
use crate::hugging_face::RepoId;
use crate::hugging_face::cache::snapshot_path;
use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;
use crate::store::setup::MAX_TOKENS_PER_TURN;

/// **A part as the tuple names it**: its path under the models mount, in
/// Hugging Face's layout, its blob, its SHA-256 and its bytes, from the
/// pull's record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub path: String,
    pub blob: String,
    pub sha256: String,
    pub bytes: i64,
}

/// **A model as the tuple names it**: its repository, commit and choice,
/// the verified pull that fetched it, and its parts in split order.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub repository: String,
    pub commit: String,
    pub choice: String,
    pub pull_id: String,
    pub parts: Vec<Part>,
}

/// **The tuple's members** (Spec 6.2), every one read.
#[derive(Debug, Clone, PartialEq)]
pub struct Tuple {
    pub decoder: Model,
    pub adapter: Model,
    pub adapter_scale: f64,
    pub base_blake3: String,
    pub context_length: i64,
    /// The box's max-tokens-per-turn the context length was bounded by.
    pub max_tokens_per_turn: i64,
    pub turn_context: String,
    pub virtual_machine: bool,
    pub devices: Vec<i32>,
    pub box_members: BoxMembers,
}

/// **A confirmed tuple**: its identity, who confirmed it and when, and its
/// members.
#[derive(Debug, Clone, PartialEq)]
pub struct Confirmed {
    pub tuple_id: String,
    /// The name of the person who confirmed.
    pub confirmed_by: String,
    pub confirmed_at: DateTime<Utc>,
    pub tuple: Tuple,
}

/// **The members the box is laid from** (Spec 6.2): the decoder's artifact
/// (its first part), the adapter's artifact, SHA-256 and scale, the base's
/// identity, the devices, the context length and the turn context.
#[derive(Debug, Clone, PartialEq)]
pub struct Laying {
    pub decoder_artifact: String,
    pub adapter_artifact: String,
    pub adapter_sha256: String,
    pub adapter_scale: f64,
    pub base_blake3: String,
    pub devices: Vec<i32>,
    pub context_length: i64,
    pub turn_context: String,
}

/// **Each member read, or why it is not**, in words that follow "not
/// read:".
#[derive(Debug, Clone)]
pub struct Members {
    pub decoder: Result<Model, String>,
    pub adapter: Result<Model, String>,
    pub adapter_scale: Result<f64, String>,
    pub base_blake3: Result<String, String>,
    pub context_length: Result<i64, String>,
    /// The box's max-tokens-per-turn, the context length's floor.
    pub max_tokens_per_turn: i64,
    pub turn_context: Result<String, String>,
    pub virtual_machine: bool,
    pub devices: Result<Vec<i32>, String>,
    pub box_members: Result<BoxMembers, String>,
}

impl Members {
    /// The tuple, where every member is read, or why each unread one is
    /// not.
    pub fn tuple(self) -> Result<Tuple, Vec<String>> {
        let unread: Vec<String> = [
            self.decoder.as_ref().err(),
            self.adapter.as_ref().err(),
            self.adapter_scale.as_ref().err(),
            self.base_blake3.as_ref().err(),
            self.context_length.as_ref().err(),
            self.turn_context.as_ref().err(),
            self.devices.as_ref().err(),
            self.box_members.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
        match self {
            Members {
                decoder: Ok(decoder),
                adapter: Ok(adapter),
                adapter_scale: Ok(adapter_scale),
                base_blake3: Ok(base_blake3),
                context_length: Ok(context_length),
                max_tokens_per_turn,
                turn_context: Ok(turn_context),
                virtual_machine,
                devices: Ok(devices),
                box_members: Ok(box_members),
            } => Ok(Tuple {
                decoder,
                adapter,
                adapter_scale,
                base_blake3,
                context_length,
                max_tokens_per_turn,
                turn_context,
                virtual_machine,
                devices,
                box_members,
            }),
            _ => Err(unread),
        }
    }
}

/// The setup's fields the members read.
struct Fields {
    virtual_machine: bool,
    context_length: Option<i64>,
    turn_context: Option<String>,
    devices: Option<Vec<i32>>,
}

/// A role's row and its pull's state.
struct RoleRow {
    repository: String,
    commit: String,
    choice: String,
    pull_id: String,
    pull_state: String,
    scale: Option<f64>,
    header: serde_json::Value,
}

/// **A role's model as its pull recorded it**, where the pull is verified.
async fn model_of(
    conn: &mut PgConnection,
    role: &str,
    row: &RoleRow,
) -> anyhow::Result<Result<Model, String>> {
    if row.pull_state != "verified" {
        return Ok(Err(format!("the {role}'s pull is {}", row.pull_state)));
    }
    let Some(repo) = RepoId::parse(&row.repository) else {
        anyhow::bail!(
            "the {role}'s repository {} is out of its shape",
            row.repository
        );
    };
    let rows = sqlx::query(
        "SELECT path, size, blob_id, lfs, sha256 FROM model_pull_file WHERE pull_id = $1 ORDER BY ordinal",
    )
    .bind(&row.pull_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut parts = Vec::with_capacity(rows.len());
    for r in &rows {
        let path: String = r.get("path");
        let file = crate::hugging_face::RepoFile {
            path: path.clone(),
            size: u64::try_from(r.get::<i64, _>("size"))?,
            blob_id: r.get("blob_id"),
            sha256: r.get("sha256"),
            lfs: r.get("lfs"),
        };
        let Some(sha256) = file.sha256.clone() else {
            return Ok(Err(format!(
                "the {role}'s pull holds no SHA-256 for {path}"
            )));
        };
        parts.push(Part {
            path: snapshot_path(&repo, &row.commit, &path)?,
            // The blob's name, Hugging Face's rule, read from the file's
            // `lfs` flag (#138).
            blob: file.blob().to_owned(),
            sha256,
            bytes: r.get("size"),
        });
    }
    if parts.is_empty() {
        return Ok(Err(format!("the {role}'s pull records no part")));
    }
    Ok(Ok(Model {
        repository: row.repository.clone(),
        commit: row.commit.clone(),
        choice: row.choice.clone(),
        pull_id: row.pull_id.clone(),
        parts,
    }))
}

/// **Each member of the setup's tuple, read from its source or not**, on
/// `conn`: the role rows and their pulls' records, the base's identity, the
/// setup's fields, the survey's cards and the box's report.
async fn members_on(
    conn: &mut PgConnection,
    setup_id: &str,
    fields: &Fields,
    survey: Option<&[Card]>,
    reported: Option<BoxMembers>,
) -> anyhow::Result<Members> {
    let rows = sqlx::query(
        "SELECT m.role, m.repository, m.commit_id, m.choice, m.pull_id, mp.state AS pull_state, \
         m.scale, m.header FROM agent_setup_model m JOIN model_pull mp ON mp.pull_id = m.pull_id \
         WHERE m.setup_id = $1",
    )
    .bind(setup_id)
    .fetch_all(&mut *conn)
    .await?;
    let role = |name: &str| {
        rows.iter()
            .find(|r| r.get::<String, _>("role") == name)
            .map(|r| RoleRow {
                repository: r.get("repository"),
                commit: r.get("commit_id"),
                choice: r.get("choice"),
                pull_id: r.get("pull_id"),
                pull_state: r.get("pull_state"),
                scale: r.get("scale"),
                header: r.get("header"),
            })
    };
    let (decoder_row, adapter_row) = (role("decoder"), role("adapter"));
    let decoder = match &decoder_row {
        Some(row) => model_of(conn, "decoder", row).await?,
        None => Err("no decoder is picked".to_owned()),
    };
    let adapter = match &adapter_row {
        Some(row) => model_of(conn, "adapter", row).await?,
        None => Err("no adapter is picked".to_owned()),
    };
    let adapter_scale = match &adapter_row {
        Some(row) => row
            .scale
            .ok_or_else(|| "the adapter's pick holds no scale".to_owned()),
        None => Err("no adapter is picked".to_owned()),
    };
    let base_blake3 = match &decoder_row {
        None => Err("no decoder is picked".to_owned()),
        Some(row) => {
            let blake3: Option<String> = sqlx::query_scalar(
                "SELECT blake3 FROM base_identity WHERE repository = $1 AND commit_id = $2 AND choice = $3",
            )
            .bind(&row.repository)
            .bind(&row.commit)
            .bind(&row.choice)
            .fetch_optional(&mut *conn)
            .await?;
            blake3.ok_or_else(|| {
                if row.pull_state == "verified" {
                    "the base's identity is owed, and its hash runs".to_owned()
                } else {
                    format!(
                        "the base's identity waits on the decoder's pull, which is {}",
                        row.pull_state
                    )
                }
            })
        }
    };
    let maximum = decoder_row.as_ref().map(|row| {
        serde_json::from_value::<Header>(row.header.clone())
            .ok()
            .and_then(|h| h.context_length)
    });
    let context_length = match (fields.context_length, maximum) {
        (None, _) => Err("no context length is kept".to_owned()),
        (Some(_), None) => {
            Err("no decoder is picked, so the context length has no bound".to_owned())
        }
        (Some(_), Some(None)) => Err("the decoder's maximum context was not read".to_owned()),
        (Some(n), Some(Some(_))) if n < MAX_TOKENS_PER_TURN => Err(format!(
            "the context length {n} is below the box's max-tokens-per-turn of {MAX_TOKENS_PER_TURN}"
        )),
        (Some(n), Some(Some(max))) if u64::try_from(n).is_ok_and(|n| n > max) => Err(format!(
            "the context length {n} is above the decoder's maximum of {max}"
        )),
        (Some(n), Some(Some(_))) => Ok(n),
    };
    let turn_context = fields
        .turn_context
        .clone()
        .ok_or_else(|| "no turn context is kept".to_owned());
    let devices = match (&fields.devices, survey) {
        (None, _) => Err("no devices are chosen".to_owned()),
        (Some(_), None) => Err("the survey has reported no cards".to_owned()),
        (Some(devices), Some(cards)) => match devices
            .iter()
            .find(|d| !cards.iter().any(|c| c.ordinal == **d))
        {
            Some(lacking) => Err(format!("card {lacking} is not among the survey's cards")),
            None => Ok(devices.clone()),
        },
    };
    let box_members = reported.ok_or_else(|| "the box's members are not reported".to_owned());
    Ok(Members {
        decoder,
        adapter,
        adapter_scale,
        base_blake3,
        context_length,
        max_tokens_per_turn: MAX_TOKENS_PER_TURN,
        turn_context,
        virtual_machine: fields.virtual_machine,
        devices,
        box_members,
    })
}

/// The parts as the tuple's row holds them.
fn parts_json(parts: &[Part]) -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::to_value(parts)?)
}

fn parts_of(value: serde_json::Value) -> anyhow::Result<Vec<Part>> {
    Ok(serde_json::from_value(value)?)
}

impl Store {
    /// **Each member of the setup's tuple, read or not**, for the confirm
    /// page.
    pub async fn members(
        &self,
        setup: &crate::store::setup::Setup,
        survey: Option<&[Card]>,
        reported: Option<BoxMembers>,
    ) -> anyhow::Result<Members> {
        let mut conn = self.pool.acquire().await?;
        let fields = Fields {
            virtual_machine: setup.virtual_machine,
            context_length: setup.context_length,
            turn_context: setup.turn_context.clone(),
            devices: setup.devices.clone(),
        };
        members_on(&mut conn, &setup.setup_id, &fields, survey, reported).await
    }

    /// **Confirm** (Spec 6.2), over the open setup read at `version`, in one
    /// transaction under the identity exclusion: the setup moved to
    /// `confirmed`, every member read again inside it, and the tuple written
    /// with the identity `tuple_id`. Stale where the setup is not what the
    /// page read, and refused, writing nothing, where a member is not read.
    #[allow(clippy::too_many_arguments)]
    pub async fn confirm(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        setup_id: &str,
        version: i64,
        tuple_id: &str,
        survey: Option<&[Card]>,
        reported: Option<BoxMembers>,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.identity_transaction().await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let Some(row) = sqlx::query(
            "UPDATE agent_setup SET state = 'confirmed', version = version + 1 \
             WHERE setup_id = $1 AND version = $2 AND state = 'open' \
             RETURNING virtual_machine, context_length, turn_context, devices",
        )
        .bind(setup_id)
        .bind(version)
        .fetch_optional(&mut *tx)
        .await?
        else {
            return Ok(Err(Refusal::Stale));
        };
        let fields = Fields {
            virtual_machine: row.get("virtual_machine"),
            context_length: row.get("context_length"),
            turn_context: row.get("turn_context"),
            devices: row.get("devices"),
        };
        let members = members_on(&mut tx, setup_id, &fields, survey, reported).await?;
        let tuple = match members.tuple() {
            Ok(tuple) => tuple,
            Err(unread) => return Ok(Err(Refusal::Unconfirmable(unread.join(", ")))),
        };
        sqlx::query(
            "INSERT INTO deployment_tuple (tuple_id, setup_id, confirmed_by, \
             decoder_pull, decoder_repository, decoder_commit, decoder_choice, decoder_parts, \
             adapter_pull, adapter_repository, adapter_commit, adapter_choice, adapter_parts, \
             adapter_scale, base_blake3, context_length, max_tokens_per_turn, turn_context, \
             virtual_machine, devices, box_members) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, \
             $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)",
        )
        .bind(tuple_id)
        .bind(setup_id)
        .bind(admin)
        .bind(&tuple.decoder.pull_id)
        .bind(&tuple.decoder.repository)
        .bind(&tuple.decoder.commit)
        .bind(&tuple.decoder.choice)
        .bind(parts_json(&tuple.decoder.parts)?)
        .bind(&tuple.adapter.pull_id)
        .bind(&tuple.adapter.repository)
        .bind(&tuple.adapter.commit)
        .bind(&tuple.adapter.choice)
        .bind(parts_json(&tuple.adapter.parts)?)
        .bind(tuple.adapter_scale)
        .bind(&tuple.base_blake3)
        .bind(tuple.context_length)
        .bind(tuple.max_tokens_per_turn)
        .bind(&tuple.turn_context)
        .bind(tuple.virtual_machine)
        .bind(&tuple.devices)
        .bind(serde_json::to_value(&tuple.box_members)?)
        .execute(&mut *tx)
        .await?;
        commit_with_outcome(tx, "setup confirm", self, first).await?;
        Ok(Ok(()))
    }

    /// **The setup's confirmed tuple**, read from its row, or none before
    /// Confirm.
    pub async fn deployment_tuple(&self, setup_id: &str) -> anyhow::Result<Option<Confirmed>> {
        let mut conn = self.pool.acquire().await?;
        Self::deployment_tuple_on(&mut conn, setup_id).await
    }

    /// The same read on a connection the caller holds, so it can share a
    /// transaction's snapshot.
    pub(crate) async fn deployment_tuple_on(
        conn: &mut sqlx::PgConnection,
        setup_id: &str,
    ) -> anyhow::Result<Option<Confirmed>> {
        let Some(r) = sqlx::query(
            "SELECT t.*, p.name AS confirmed_by_name FROM deployment_tuple t \
             JOIN person p ON p.person_id = t.confirmed_by WHERE t.setup_id = $1",
        )
        .bind(setup_id)
        .fetch_optional(&mut *conn)
        .await?
        else {
            return Ok(None);
        };
        let model = |prefix: &str| -> anyhow::Result<Model> {
            Ok(Model {
                repository: r.get(format!("{prefix}_repository").as_str()),
                commit: r.get(format!("{prefix}_commit").as_str()),
                choice: r.get(format!("{prefix}_choice").as_str()),
                pull_id: r.get(format!("{prefix}_pull").as_str()),
                parts: parts_of(r.get(format!("{prefix}_parts").as_str()))?,
            })
        };
        Ok(Some(Confirmed {
            tuple_id: r.get("tuple_id"),
            confirmed_by: r.get("confirmed_by_name"),
            confirmed_at: r.get("confirmed_at"),
            tuple: Tuple {
                decoder: model("decoder")?,
                adapter: model("adapter")?,
                adapter_scale: r.get("adapter_scale"),
                base_blake3: r.get("base_blake3"),
                context_length: r.get("context_length"),
                max_tokens_per_turn: r.get("max_tokens_per_turn"),
                turn_context: r.get("turn_context"),
                virtual_machine: r.get("virtual_machine"),
                devices: r.get("devices"),
                box_members: serde_json::from_value(r.get("box_members"))?,
            },
        }))
    }

    /// **The members the box is laid from** (Spec 6.2), read from the
    /// confirmed tuple alone, so none stands before Confirm.
    pub async fn laying(&self, setup_id: &str) -> anyhow::Result<Option<Laying>> {
        let Some(confirmed) = self.deployment_tuple(setup_id).await? else {
            return Ok(None);
        };
        Laying::of(&confirmed).map(Some)
    }
}

impl Laying {
    /// **The members a confirmed tuple lays the box from** (Spec 6.2), one
    /// rule for the pages and for `setup describe`.
    pub fn of(confirmed: &Confirmed) -> anyhow::Result<Laying> {
        let t = &confirmed.tuple;
        let (Some(decoder), Some(adapter)) = (t.decoder.parts.first(), t.adapter.parts.first())
        else {
            anyhow::bail!(
                "the tuple {} names a model without a part",
                confirmed.tuple_id
            );
        };
        Ok(Laying {
            decoder_artifact: decoder.path.clone(),
            adapter_artifact: adapter.path.clone(),
            adapter_sha256: adapter.sha256.clone(),
            adapter_scale: t.adapter_scale,
            base_blake3: t.base_blake3.clone(),
            devices: t.devices.clone(),
            context_length: t.context_length,
            turn_context: t.turn_context.clone(),
        })
    }
}
