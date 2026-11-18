//! conforms: web-a-replacements-drive-setup-mounts-the-state-drive-it-carries-and-never-formats-it
//! conforms: web-the-setup-keeps-the-latest-report-of-each-stage-verbatim
//! conforms: web-describes-laying-names-an-adapter-only-where-the-tuple-holds-one
//!
//! **The setup connection's two asks at the store** (Spec 8.3): what `admin-con
//! setup describe` is answered, and where `admin-con setup report` lands. Each
//! serves the setup the row's pairing linked, the latest where a replacement
//! linked a second, and each re-reads the asking credential as live inside its
//! transaction, so a credential revoked since the connection's lookup is
//! refused at the act as at the door.
//!
//! **A report is a connector's report landing**, as an observation does, and
//! not among section 2.13's audited writes (the brief's readings).

use std::collections::{BTreeMap, BTreeSet};

use sqlx::Row;

use crate::hugging_face::cache::snapshot_path;
use crate::hugging_face::{RepoFile, RepoId};
use crate::link::frames::{
    Described, DescribedAdapter, DescribedFile, DescribedKey, DescribedLaying, DescribedModel,
    REPORT_BOUND, Refusal, Stage,
};
use crate::store::Store;
use crate::store::key::AgentId;
use crate::store::tuple::Laying;

/// The setup a row's pairing linked, the latest of them.
const LINKED_SETUP: &str = "SELECT setup_id, state, paired_as FROM agent_setup WHERE agent_id = $1 \
     ORDER BY paired_at DESC, setup_id LIMIT 1";

/// The same, its row held shared.
const LINKED_SETUP_SHARED: &str = "SELECT setup_id, state, paired_as FROM agent_setup \
     WHERE agent_id = $1 ORDER BY paired_at DESC, setup_id LIMIT 1 FOR SHARE";

/// Whether the row's admin credential is the one asking and live, read under
/// the row's share lock.
async fn asking_is_live(
    conn: &mut sqlx::PgConnection,
    agent_id: &AgentId,
    fingerprint: &str,
) -> anyhow::Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT name FROM agent WHERE agent_id = $1 AND admin_fingerprint = $2 \
         AND admin_state = 'live' FOR SHARE",
    )
    .bind(agent_id.as_str())
    .bind(fingerprint)
    .fetch_optional(conn)
    .await?)
}

impl Store {
    /// **`setup describe`'s answer** (Spec 8.3) for the row whose live admin
    /// credential asks: refused `not_live` where the credential no longer is,
    /// and `no_setup` where no setup's pairing links the row. **Every member
    /// is read in one snapshot**, the picks, the need, the confirmed tuple's
    /// laying and the SSH keys, so a pick changed and confirmed while the
    /// answer is read never gives the box one model's sizes with another's
    /// laying.
    pub async fn described(
        &self,
        agent_id: &AgentId,
        fingerprint: &str,
    ) -> anyhow::Result<Result<Described, Refusal>> {
        self.described_held(agent_id, fingerprint, None).await
    }

    /// `described`, with a test's hold between the picks' read and the
    /// tuple's: `reached` is sent there, and the read waits on `go`.
    pub(crate) async fn described_held(
        &self,
        agent_id: &AgentId,
        fingerprint: &str,
        hold: Option<(
            tokio::sync::oneshot::Sender<()>,
            tokio::sync::oneshot::Receiver<()>,
        )>,
    ) -> anyhow::Result<Result<Described, Refusal>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ")
            .execute(&mut *tx)
            .await?;
        let Some(agent) = asking_is_live(&mut tx, agent_id, fingerprint).await? else {
            return Ok(Err(Refusal::NotLive));
        };
        let Some(setup) = sqlx::query(LINKED_SETUP)
            .bind(agent_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(Err(Refusal::NoSetup));
        };
        let setup_id: String = setup.get("setup_id");
        let replacement =
            setup.get::<Option<String>, _>("paired_as").as_deref() == Some("replace box");

        let picks = sqlx::query(
            "SELECT m.role, m.repository, m.commit_id, m.choice, m.pull_id, mp.state \
             FROM agent_setup_model m JOIN model_pull mp ON mp.pull_id = m.pull_id \
             WHERE m.setup_id = $1 ORDER BY m.role",
        )
        .bind(&setup_id)
        .fetch_all(&mut *tx)
        .await?;
        let mut models = BTreeMap::new();
        // **The models drive's need counts a blob once** (Spec 8.3): a blob
        // two paths share, in one repository's cache, takes its bytes once.
        let mut blobs: BTreeSet<(String, String)> = BTreeSet::new();
        let mut bytes: u64 = 0;
        for pick in &picks {
            let repository: String = pick.get("repository");
            let commit: String = pick.get("commit_id");
            let Some(repo) = RepoId::parse(&repository) else {
                anyhow::bail!("the pick's repository {repository} is out of its shape");
            };
            let files = sqlx::query(
                "SELECT path, size, blob_id, lfs, sha256 FROM model_pull_file \
                 WHERE pull_id = $1 ORDER BY ordinal",
            )
            .bind(pick.get::<String, _>("pull_id"))
            .fetch_all(&mut *tx)
            .await?;
            let mut described = Vec::with_capacity(files.len());
            for f in &files {
                let file = RepoFile {
                    path: f.get("path"),
                    size: u64::try_from(f.get::<i64, _>("size"))?,
                    blob_id: f.get("blob_id"),
                    sha256: f.get("sha256"),
                    lfs: f.get("lfs"),
                };
                // The blob's name, Hugging Face's rule from the `lfs` flag
                // (#138), as the tuple names a part's.
                let blob = file.blob().to_owned();
                if blobs.insert((repository.clone(), blob.clone())) {
                    bytes += file.size;
                }
                described.push(DescribedFile {
                    path: snapshot_path(&repo, &commit, &file.path)?,
                    blob,
                    bytes: file.size,
                    sha256: file.sha256.clone(),
                });
            }
            models.insert(
                pick.get::<String, _>("role"),
                DescribedModel {
                    artifact: snapshot_path(&repo, &commit, &pick.get::<String, _>("choice"))?,
                    repository,
                    commit,
                    files: described,
                    pull: pick.get("state"),
                },
            );
        }
        if let Some((reached, go)) = hold {
            let _ = reached.send(());
            let _ = go.await;
        }
        let confirmed = Self::deployment_tuple_on(&mut tx, &setup_id).await?;
        let held_keys = Self::ssh_keys_on(&mut tx).await?;
        tx.commit().await?;

        let laying = match confirmed {
            None => None,
            Some(confirmed) => {
                let laying = Laying::of(&confirmed)?;
                Some(DescribedLaying {
                    tuple: confirmed.tuple_id,
                    artifact: laying.decoder_artifact,
                    adapter: laying.adapter.map(|adapter| DescribedAdapter {
                        artifact: adapter.artifact,
                        sha256: adapter.sha256,
                        base_blake3: adapter.base_blake3,
                        scale: adapter.scale,
                    }),
                    devices: laying.devices,
                    context_capacity: laying.context_length,
                    turn_context: laying.turn_context,
                })
            }
        };
        let ssh_keys = held_keys
            .into_iter()
            .map(|k| DescribedKey {
                key: k.line,
                fingerprint: k.fingerprint,
            })
            .collect();
        Ok(Ok(Described {
            agent,
            agent_id: agent_id.as_str().to_owned(),
            replacement,
            models: (!models.is_empty()).then_some(models),
            bytes,
            laying,
            ssh_keys,
        }))
    }

    /// **A setup report landing** (Spec 8.3): kept verbatim on the setup the
    /// row's pairing linked, the latest of its stage, with the time it
    /// arrived. Refused `too_large` past `REPORT_BOUND` before the store is
    /// read, `not_live` where the asking credential no longer is, `no_setup`
    /// where no pairing links the row, and `setup_closed` for a confirmed
    /// setup, which never changes. The setup's row is held shared, so a
    /// Confirm lands wholly before the report or wholly after it. **A
    /// report the survey's reader refuses is logged once, at its arrival**,
    /// by the setup, the stage, the cause's number and the refusal's detail
    /// (Spec 8.3). The reader runs after the commit, so neither its result
    /// nor the line can change the write or its answer.
    pub async fn land_report(
        &self,
        agent_id: &AgentId,
        fingerprint: &str,
        stage: Stage,
        report: &str,
    ) -> anyhow::Result<Result<(), Refusal>> {
        if report.len() > REPORT_BOUND {
            return Ok(Err(Refusal::TooLarge));
        }
        let mut tx = self.pool.begin().await?;
        if asking_is_live(&mut tx, agent_id, fingerprint)
            .await?
            .is_none()
        {
            return Ok(Err(Refusal::NotLive));
        }
        let Some(setup) = sqlx::query(LINKED_SETUP_SHARED)
            .bind(agent_id.as_str())
            .fetch_optional(&mut *tx)
            .await?
        else {
            return Ok(Err(Refusal::NoSetup));
        };
        if setup.get::<String, _>("state") == "confirmed" {
            return Ok(Err(Refusal::SetupClosed));
        }
        sqlx::query(
            "INSERT INTO agent_setup_report (setup_id, stage, report, arrived_at) \
             VALUES ($1, $2, $3, now()) \
             ON CONFLICT (setup_id, stage) \
             DO UPDATE SET report = EXCLUDED.report, arrived_at = EXCLUDED.arrived_at",
        )
        .bind(setup.get::<String, _>("setup_id"))
        .bind(stage.as_str())
        .bind(report)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        if let Err(refused) = crate::survey::read(report.as_bytes()) {
            tracing::warn!(
                setup = %setup.get::<String, _>("setup_id"),
                stage = stage.as_str(),
                cause = refused.cause.number(),
                detail = %refused.detail,
                "the survey's reader refused a setup report"
            );
        }
        Ok(Ok(()))
    }

    /// **The setup's reports**, each stage's latest, verbatim, with the time it
    /// arrived, the pairing's first.
    pub async fn setup_reports(
        &self,
        setup_id: &str,
    ) -> anyhow::Result<Vec<(String, String, chrono::DateTime<chrono::Utc>)>> {
        let mut conn = self.pool.acquire().await?;
        setup_reports_on(&mut conn, setup_id).await
    }

    /// **What the box reports for the setup, read through `boxes`** (Spec
    /// 6.2), on a connection of its own: the setup pages' read.
    pub async fn reports(
        &self,
        boxes: &dyn crate::box_report::BoxReports,
        setup_id: &str,
    ) -> anyhow::Result<crate::box_report::Reports> {
        let mut conn = self.pool.acquire().await?;
        boxes.reports(&mut conn, setup_id).await
    }
}

/// **The setup's reports on `conn`**, each stage's latest, verbatim, with
/// the time it arrived, the pairing's first: inside Confirm's transaction,
/// once its update holds the setup's row, every report written before it.
pub async fn setup_reports_on(
    conn: &mut sqlx::PgConnection,
    setup_id: &str,
) -> anyhow::Result<Vec<(String, String, chrono::DateTime<chrono::Utc>)>> {
    let rows = sqlx::query(
        "SELECT stage, report, arrived_at FROM agent_setup_report WHERE setup_id = $1 \
         ORDER BY stage = 'drives', stage",
    )
    .bind(setup_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get("stage"), r.get("report"), r.get("arrived_at")))
        .collect())
}
