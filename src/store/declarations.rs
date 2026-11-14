//! conforms: web-a-save-from-a-row-not-its-lineages-head-is-refused
//! conforms: web-no-two-lineages-share-a-name
//! conforms: web-a-declaration-rows-authored-half-never-changes
//!
//! **The saved declarations** (Spec 2.4): every save appends a row whose
//! authored half never changes, the lineage identified by its root, a row's
//! version its ordinal in the lineage. **A save is an admin write** (Spec
//! 2.13, 6.1): in one transaction holding the identity exclusion shared, for
//! the grant's re-check, and the declarations' own advisory key, the admin
//! grant, the head and the name are checked again, the row is inserted and
//! the `ok` outcome is written, so the row and its outcome commit together.
//! **The head check is this module's and not the schema's**: the schema
//! admits a second child of one parent, which [`insert_row`] alone writes.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Postgres, Row as _, Transaction};

use crate::declaration::render::CORPUS_COMMIT;
use crate::declaration::{Document, moved};
use crate::store::Store;
use crate::store::admin::{Refusal, authorized};
use crate::store::commit::commit_with_outcome;
use crate::store::identity::IDENTITY_LOCK_KEY;

/// **The declarations' own advisory key**, taken exclusively by every save
/// after the identity exclusion, shared: no other act takes it.
pub const DECLARATION_LOCK_KEY: i64 = i64::from_be_bytes(*b"weaverdc");

/// **Whether `spec` is of a declaration's identity's shape**: `de-` and
/// sixteen lowercase hex. Not a lookup.
pub fn is_declaration_id(spec: &str) -> bool {
    spec.strip_prefix("de-").is_some_and(|hex| {
        hex.len() == 16
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// One saved row, as its page reads it.
#[derive(Debug, Clone)]
pub struct Saved {
    pub declaration_id: i64,
    pub de: String,
    pub name: String,
    pub body: Value,
    pub corpus_commit: String,
    /// The row it was saved from, by identity, none at a root.
    pub parent: Option<String>,
    /// The lineage's root, by identity, itself at a root.
    pub root: String,
    pub root_id: i64,
    pub version: i64,
    pub saved_at: DateTime<Utc>,
    pub what_moved: Vec<String>,
    pub the_one_thing: Option<String>,
    pub author: Option<String>,
    /// Whether no later row stands in its lineage.
    pub head: bool,
}

/// One lineage, as the list reads it: its root and name, and its head's
/// version, save and author.
#[derive(Debug, Clone)]
pub struct Lineage {
    pub name: String,
    pub head: String,
    pub version: i64,
    pub saved_at: DateTime<Utc>,
    pub author: Option<String>,
}

/// The row's lineage key: its root, or itself at a root.
const LINEAGE: &str = "coalesce(d.lineage_root_id, d.declaration_id)";

fn saved(r: &sqlx::postgres::PgRow) -> Saved {
    Saved {
        declaration_id: r.get("declaration_id"),
        de: r.get("declaration_ref"),
        name: r.get("name"),
        body: r.get("body"),
        corpus_commit: r.get("corpus_commit"),
        parent: r.get("parent_ref"),
        root: r.get("root_ref"),
        root_id: r.get("root_id"),
        version: r.get("version"),
        saved_at: r.get("saved_at"),
        what_moved: r.get("what_moved"),
        the_one_thing: r.get("the_one_thing_that_moved"),
        author: r.get("author"),
        head: r.get("head"),
    }
}

fn select_saved(clause: &str) -> String {
    format!(
        "SELECT d.declaration_id, d.declaration_ref, d.name, d.body, d.corpus_commit, \
         p.declaration_ref AS parent_ref, r.declaration_ref AS root_ref, \
         r.declaration_id AS root_id, d.version, d.saved_at, d.what_moved, \
         d.the_one_thing_that_moved, d.author, \
         NOT EXISTS (SELECT 1 FROM declaration o \
           WHERE coalesce(o.lineage_root_id, o.declaration_id) = {LINEAGE} \
           AND o.declaration_id > d.declaration_id) AS head \
         FROM declaration d \
         LEFT JOIN declaration p ON p.declaration_id = d.parent_declaration_id \
         JOIN declaration r ON r.declaration_id = {LINEAGE} \
         WHERE {clause}"
    )
}

async fn read_saved(conn: &mut sqlx::PgConnection, de: &str) -> anyhow::Result<Option<Saved>> {
    Ok(
        sqlx::query(sqlx::AssertSqlSafe(select_saved("d.declaration_ref = $1")))
            .bind(de)
            .fetch_optional(conn)
            .await?
            .as_ref()
            .map(saved),
    )
}

async fn name_held(conn: &mut sqlx::PgConnection, name: &str) -> anyhow::Result<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM declaration WHERE name = $1)")
            .bind(name)
            .fetch_one(conn)
            .await?,
    )
}

/// The parent a child row is inserted under: its key, its lineage's root
/// and its version.
#[derive(Debug, Clone, Copy)]
pub struct Under {
    pub declaration_id: i64,
    pub root_id: i64,
    pub version: i64,
}

/// **One row inserted**, a root where `under` is none and otherwise a child
/// one version past its parent: the store's one insert, which checks no
/// head, so the schema's admitting a second child of one parent is what it
/// writes (Spec 2.4).
pub(crate) async fn insert_row(
    tx: &mut Transaction<'_, Postgres>,
    de: &str,
    name: &str,
    body: &Value,
    under: Option<Under>,
    what_moved: &[String],
    author: &str,
) -> anyhow::Result<()> {
    let one = (what_moved.len() == 1).then(|| what_moved[0].clone());
    sqlx::query(
        "INSERT INTO declaration (declaration_ref, name, body, corpus_commit, \
         parent_declaration_id, lineage_root_id, version, what_moved, \
         the_one_thing_that_moved, author) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(de)
    .bind(name)
    .bind(body)
    .bind(CORPUS_COMMIT)
    .bind(under.map(|u| u.declaration_id))
    .bind(under.map(|u| u.root_id))
    .bind(under.map_or(1, |u| u.version + 1))
    .bind(what_moved)
    .bind(one)
    .bind(author)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

impl Store {
    /// **Every lineage**, by name, with its head.
    pub async fn lineages(&self) -> anyhow::Result<Vec<Lineage>> {
        let rows = sqlx::query(
            "SELECT r.name, h.declaration_ref AS head, h.version, h.saved_at, h.author \
             FROM declaration r \
             JOIN LATERAL (SELECT * FROM declaration h \
               WHERE coalesce(h.lineage_root_id, h.declaration_id) = r.declaration_id \
               ORDER BY h.declaration_id DESC LIMIT 1) h ON true \
             WHERE r.parent_declaration_id IS NULL ORDER BY r.name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| Lineage {
                name: r.get("name"),
                head: r.get("head"),
                version: r.get("version"),
                saved_at: r.get("saved_at"),
                author: r.get("author"),
            })
            .collect())
    }

    /// A row by its identity.
    pub async fn declaration(&self, de: &str) -> anyhow::Result<Option<Saved>> {
        let mut conn = self.pool.acquire().await?;
        read_saved(&mut conn, de).await
    }

    /// **A row's lineage**, oldest first.
    pub async fn lineage(&self, root_id: i64) -> anyhow::Result<Vec<Saved>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{} ORDER BY d.declaration_id",
            select_saved(&format!("{LINEAGE} = $1"))
        )))
        .bind(root_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(saved).collect())
    }

    /// **The declaration saved under a name** (Spec 2.4, 2.13): the head of
    /// the one lineage whose root holds the name, none where no lineage
    /// does. A declaration names no box, so every register row of the name
    /// finds the same lineage.
    pub async fn declaration_named(&self, name: &str) -> anyhow::Result<Option<Saved>> {
        Ok(sqlx::query(sqlx::AssertSqlSafe(select_saved(
            "d.declaration_id = (SELECT max(h.declaration_id) FROM declaration h \
             JOIN declaration n ON n.declaration_id = coalesce(h.lineage_root_id, h.declaration_id) \
             WHERE n.name = $1 AND n.parent_declaration_id IS NULL)",
        )))
        .bind(name)
        .fetch_optional(&self.pool)
        .await?
        .as_ref()
        .map(saved))
    }

    /// Whether any row holds the name.
    pub async fn declaration_name_held(&self, name: &str) -> anyhow::Result<bool> {
        let mut conn = self.pool.acquire().await?;
        name_held(&mut conn, name).await
    }

    /// **A declaration saved by an admin** (Spec 6.1, step four): a new
    /// lineage's root where `from` is none, and otherwise a row whose parent
    /// is `from`, in one transaction that re-checks the admin grant, the
    /// head and the name under the declarations' key before it inserts.
    // The admin, the session and the first record that authorize it, the
    // row's identity, the row it is saved from and the document: each an
    // argument the one insert takes.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_declaration(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        de: &str,
        from: Option<&str>,
        document: &Document,
    ) -> anyhow::Result<Result<(), Refusal>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock_shared($1)")
            .bind(IDENTITY_LOCK_KEY)
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(DECLARATION_LOCK_KEY)
            .execute(&mut *tx)
            .await?;
        if let Err(refusal) = authorized(&mut tx, session_id, admin).await? {
            return Ok(Err(refusal));
        }
        let body = document.to_value();
        match from {
            None => {
                if name_held(&mut tx, &document.name).await? {
                    return Ok(Err(Refusal::NameHeld(document.name.clone())));
                }
                insert_row(&mut tx, de, &document.name, &body, None, &[], admin).await?;
            }
            Some(from) => {
                let Some(parent) = read_saved(&mut tx, from).await? else {
                    return Ok(Err(Refusal::NoSuchDeclaration));
                };
                if let Err(refusal) = saveable(&parent, document) {
                    return Ok(Err(refusal));
                }
                let what_moved = moved(&parent.body, &body);
                let under = Under {
                    declaration_id: parent.declaration_id,
                    root_id: parent.root_id,
                    version: parent.version,
                };
                insert_row(
                    &mut tx,
                    de,
                    &document.name,
                    &body,
                    Some(under),
                    &what_moved,
                    admin,
                )
                .await?;
            }
        }
        commit_with_outcome(tx, "declaration save", self, first).await?;
        Ok(Ok(()))
    }
}

/// **Whether a document may be saved from a row** (Spec 2.4): the row is
/// its lineage's head, and the document keeps the lineage's name. The
/// surface asks it before any record and the store again under the key.
pub fn saveable(parent: &Saved, document: &Document) -> Result<(), Refusal> {
    if !parent.head {
        return Err(Refusal::NotHead);
    }
    if document.name != parent.name {
        return Err(Refusal::NameChanged(parent.name.clone()));
    }
    Ok(())
}
