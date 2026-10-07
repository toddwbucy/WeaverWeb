//! **The rows the ingest writes, each defined once.** A run's members, a
//! generation and a position are each declared here by one macro, field by
//! field with its column, and everything that touches the row derives from
//! that one declaration: the insert's columns and placeholders, the loader's
//! select list, the bindings, the read back, and the replay's comparison,
//! which is the struct's equality over every field. A member added later is
//! added in one place, and cannot be written by the insert and left
//! uncompared by the replay. Each row's validation, every rule the schema
//! holds it to that the planner must meet before a row exists, sits beside
//! it.

use std::sync::LazyLock;

use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::Query;
use sqlx::{Postgres, Row};

/// The longest key a run may carry, in bytes: its identity, its parent
/// reference, its record identity, its session and every turn key are keyed
/// or indexed in the store, and PostgreSQL refuses a btree entry past about a
/// third of a page, some 2.7 KB, at the insert, which would fail the whole
/// ingest. This bound sits well under that limit and far over any key a
/// record carries.
pub const KEY_BOUND: usize = 1024;

/// One column of a row: its name, how the insert writes its placeholder
/// (`$` standing for the parameter), how the loader selects it, and its
/// array type for a bulk insert.
#[derive(Debug, Clone, Copy)]
pub struct Column {
    pub name: &'static str,
    pub insert: &'static str,
    pub select: &'static str,
    pub array: &'static str,
}

macro_rules! row {
    (
        $(#[$meta:meta])*
        pub struct $name:ident {
            $(
                $(#[$fmeta:meta])*
                $field:ident : $ty:ty => $col:literal, insert $ins:literal, select $sel:literal, array $arr:literal;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $name {
            $( $(#[$fmeta])* pub $field: $ty, )*
        }

        impl $name {
            /// Every column of the row, in declaration order.
            pub const COLUMNS: &'static [Column] = &[
                $( Column { name: $col, insert: $ins, select: $sel, array: $arr }, )*
            ];

            /// Bind every member, in `COLUMNS`' order.
            pub fn bind<'q>(
                &'q self,
                mut query: Query<'q, Postgres, PgArguments>,
            ) -> Query<'q, Postgres, PgArguments> {
                $( query = query.bind(&self.$field); )*
                query
            }

            /// Bind every member of `rows` as one array per column, in
            /// `COLUMNS`' order, for a bulk insert over `UNNEST`.
            #[allow(dead_code)]
            pub fn bind_arrays<'q>(
                rows: &[&Self],
                mut query: Query<'q, Postgres, PgArguments>,
            ) -> Query<'q, Postgres, PgArguments> {
                $( query = query.bind(rows.iter().map(|r| r.$field.clone()).collect::<Vec<$ty>>()); )*
                query
            }

            /// Read every member back from a row selected by `select_list`.
            pub fn read(row: &PgRow) -> Self {
                Self { $( $field: row.get($col), )* }
            }

            /// The members on which `self` and `other` differ, by column.
            pub fn differing(&self, other: &Self) -> Vec<&'static str> {
                let mut differ = Vec::new();
                $( if self.$field != other.$field { differ.push($col); } )*
                differ
            }
        }
    };
}

/// The insert's placeholders for `columns`, numbered from `first`.
pub fn placeholders(columns: &[Column], first: usize) -> String {
    columns
        .iter()
        .enumerate()
        .map(|(i, c)| c.insert.replace('$', &format!("${}", first + i)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The columns' names, comma-joined.
pub fn names(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|c| c.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The loader's select list for `columns`.
pub fn select_list(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|c| c.select)
        .collect::<Vec<_>>()
        .join(", ")
}

row! {
    /// **The run row's members, the whole row the ingest writes**, constants
    /// included: as the planner forms them, the insert writes them, the
    /// loader reads them and a replay compares them. Each `None` is a member
    /// the record did not carry, or one this seam never fills and writes
    /// empty. The row's bookkeeping, its status and reason, its link, its
    /// parting and its landing time, is the ingest's own and not a member:
    /// written by the store, the close and the resolution, never compared.
    /// Every other column of `run` is a member, which a test reads from the
    /// schema.
    pub struct RunMembers {
        record_identity: String => "record_identity", insert "$", select "record_identity", array "text[]";
        /// The declared seed, as text, since the record spells it unsigned.
        seed: Option<String> => "seed", insert "$::numeric", select "seed::text AS seed", array "text[]";
        /// The effective sampling's declared members, `generation_seed` removed.
        sampler: serde_json::Value => "sampler", insert "$", select "sampler", array "jsonb[]";
        device: Option<String> => "device", insert "$", select "device", array "text[]";
        engine: Option<serde_json::Value> => "engine", insert "$", select "engine", array "jsonb[]";
        field_depth: Option<i32> => "field_depth", insert "$", select "field_depth", array "int4[]";
        record_session: Option<String> => "record_session", insert "$", select "record_session", array "text[]";
        record_digest: Option<String> => "record_digest", insert "$", select "record_digest", array "text[]";
        prefix_length: Option<i32> => "prefix_length", insert "$", select "prefix_length", array "int4[]";
        /// The lineage's `built_from.run`, the parent as the record names it.
        parent_reference: Option<String> => "parent_reference", insert "$", select "parent_reference", array "text[]";
        /// The declared boundary set, written empty: a fact about every run
        /// this ingest can meet (Spec 2.2), and a member a replay compares
        /// like any other.
        boundary_set: serde_json::Value => "boundary_set", insert "$", select "boundary_set", array "jsonb[]";
        /// **The members this seam never fills, written empty** (Spec 2.2):
        /// the task's source and identity, the forced position and token, the
        /// branch position the authoring path writes, and the signature,
        /// shingles over a text that does not cross. A stored row holding
        /// one is another writer's, and differs from every emission.
        task_source: Option<String> => "task_source", insert "$", select "task_source", array "text[]";
        task_identity: Option<String> => "task_identity", insert "$", select "task_identity", array "text[]";
        forced_position: Option<i32> => "forced_position", insert "$", select "forced_position", array "int4[]";
        forced_token: Option<String> => "forced_token", insert "$", select "forced_token", array "text[]";
        branch_position: Option<i32> => "branch_position", insert "$", select "branch_position", array "int4[]";
        signature: Option<serde_json::Value> => "signature", insert "$", select "signature", array "jsonb[]";
    }
}

row! {
    /// One generation's row, keyed by the run and `seq`, its landing order.
    pub struct GenerationRow {
        seq: i32 => "seq", insert "$", select "seq", array "int4[]";
        turn: Option<String> => "turn", insert "$", select "turn", array "text[]";
        perplexity: Option<f64> => "perplexity", insert "$", select "perplexity", array "float8[]";
        resident: Option<i32> => "resident", insert "$", select "resident", array "int4[]";
        output_count: i32 => "output_count", insert "$", select "output_count", array "int4[]";
        generation_seed: Option<String> => "generation_seed", insert "$::numeric", select "generation_seed::text AS generation_seed", array "text[]";
    }
}

row! {
    /// One position's row as this seam fills it: the text, the alternatives,
    /// the rank and the residual never cross it, so it leaves them absent.
    pub struct PositionRow {
        turn: String => "turn", insert "$", select "turn", array "text[]";
        position: i32 => "position", insert "$", select "position", array "int4[]";
        token_id: i64 => "token_id", insert "$", select "token_id", array "int8[]";
        entropy: Option<f64> => "entropy", insert "$", select "entropy", array "float8[]";
        surprisal: Option<f64> => "surprisal", insert "$", select "surprisal", array "float8[]";
    }
}

/// The run's insert: the identity, every member, and the status `writing`.
pub static INSERT_RUN: LazyLock<String> = LazyLock::new(|| {
    format!(
        "INSERT INTO run (run_id, {}, ingest_status) VALUES ($1, {}, 'writing')",
        names(RunMembers::COLUMNS),
        placeholders(RunMembers::COLUMNS, 2)
    )
});

/// The run's loader: every member, then the bookkeeping the answer reads.
pub static SELECT_RUN: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {}, ingest_status, ingest_reason, parent_run_id IS NOT NULL AS linked, \
         parting_known, parting_position FROM run WHERE run_id = $1",
        select_list(RunMembers::COLUMNS)
    )
});

/// **Many rows in one statement**, one array per column over `UNNEST`, each
/// column written through its own insert form.
fn bulk_insert(table: &str, columns: &[Column]) -> String {
    let arrays: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("${}::{}", i + 2, c.array))
        .collect();
    let values: Vec<String> = columns
        .iter()
        .map(|c| c.insert.replace('$', &format!("u.{}", c.name)))
        .collect();
    format!(
        "INSERT INTO {table} (run_id, {}) SELECT $1, {} FROM UNNEST({}) AS u({})",
        names(columns),
        values.join(", "),
        arrays.join(", "),
        names(columns)
    )
}

/// A run's generations, every one in one statement.
pub static INSERT_GENERATIONS: LazyLock<String> =
    LazyLock::new(|| bulk_insert("generation", GenerationRow::COLUMNS));

/// A run's generations, in order.
pub static SELECT_GENERATIONS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {} FROM generation WHERE run_id = $1 ORDER BY seq",
        select_list(GenerationRow::COLUMNS)
    )
});

/// Many positions in one statement.
pub static INSERT_POSITIONS: LazyLock<String> =
    LazyLock::new(|| bulk_insert("position", PositionRow::COLUMNS));

/// A run's positions, with whether the members this seam never fills are in
/// fact empty, so a row another writer filled reads as different.
pub static SELECT_POSITIONS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "SELECT {}, (token_text IS NULL AND alternatives IS NULL AND realized IS NULL \
         AND residual IS NULL) AS unfilled FROM position WHERE run_id = $1",
        select_list(PositionRow::COLUMNS)
    )
});

/// A text member refused where it holds a NUL byte, which PostgreSQL
/// refuses in `TEXT`.
pub fn no_nul(name: &str, text: &str) -> Result<(), String> {
    if text.contains('\0') {
        return Err(format!(
            "the run's {name} holds a NUL byte, which the store refuses"
        ));
    }
    Ok(())
}

/// A keyed or indexed text member refused past `KEY_BOUND` bytes, or
/// holding a NUL byte.
pub fn key(name: &str, key: &str) -> Result<(), String> {
    no_nul(name, key)?;
    if key.len() > KEY_BOUND {
        return Err(format!(
            "the run's {name} runs {} bytes, past the {KEY_BOUND} a key may carry",
            key.len()
        ));
    }
    Ok(())
}

/// A JSON member refused where any key or string in it holds a NUL byte,
/// which PostgreSQL refuses in `JSONB`.
pub fn no_nul_in(name: &str, json: &serde_json::Value) -> Result<(), String> {
    match json {
        serde_json::Value::String(s) => no_nul(name, s),
        serde_json::Value::Array(items) => items.iter().try_for_each(|v| no_nul_in(name, v)),
        serde_json::Value::Object(map) => map.iter().try_for_each(|(k, v)| {
            no_nul(name, k)?;
            no_nul_in(name, v)
        }),
        _ => Ok(()),
    }
}

impl RunMembers {
    /// **Every rule the schema holds the run row to, met before it exists**,
    /// so the store refusing an insert is never how a malformed run is
    /// found: that would fail the whole ingest where one run should be
    /// refused by name. The rules, and where each is met:
    /// - no text or JSON member holds a NUL byte (`TEXT`, `JSONB`): here;
    /// - every keyed or indexed text member fits a btree entry
    ///   (`KEY_BOUND`): the identity, the record identity, the session and
    ///   the parent reference, here;
    /// - the digest is 64 lowercase hex characters (migration 0003): here;
    /// - the record identity and the sampler are present (0001), the seed
    ///   fits `NUMERIC(20,0)` and the integers fit `INTEGER` unsigned: by
    ///   the planner, which forms them from unsigned values converted
    ///   checked and refuses a run without either;
    /// - the boundary set is an array (0006): it is the constant `[]`.
    pub fn validate(&self, run: &str) -> Result<(), String> {
        key("run identity", run)?;
        key("record identity", &self.record_identity)?;
        for (name, text) in [
            ("session", self.record_session.as_deref()),
            ("parent reference", self.parent_reference.as_deref()),
        ] {
            if let Some(text) = text {
                key(name, text)?;
            }
        }
        for (name, text) in [
            ("digest", self.record_digest.as_deref()),
            ("device model", self.device.as_deref()),
            ("seed", self.seed.as_deref()),
        ] {
            if let Some(text) = text {
                no_nul(name, text)?;
            }
        }
        no_nul_in("effective sampling", &self.sampler)?;
        no_nul_in("boundary set", &self.boundary_set)?;
        if let Some(engine) = &self.engine {
            no_nul_in("code identity", engine)?;
        }
        if let Some(digest) = &self.record_digest
            && !(digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        {
            return Err(format!(
                "the run's digest is not sha256 as lowercase hex (64 characters): {digest:?}"
            ));
        }
        Ok(())
    }
}

impl GenerationRow {
    /// The generation's rules: its turn key fits a key and holds no NUL; its
    /// counts and order are formed checked by the planner (0014's checks).
    pub fn validate(&self) -> Result<(), String> {
        if let Some(turn) = &self.turn {
            key("turn key", turn)?;
        }
        Ok(())
    }
}
