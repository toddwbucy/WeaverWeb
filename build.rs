//! **The migrations are part of the build.** `sqlx::migrate!` embeds the
//! files of `migrations/` when the crate compiles, and Cargo tracks no file a
//! macro reads. Without this script a migration added since the last build
//! is not seen: the crate keeps its embedded set until a file holding the
//! macro is rebuilt, and a run against a fresh store applies the old schema
//! while its tests pass (measured 2026-11-18, migration `0032` running as
//! `0029`'s schema). Naming the directory makes Cargo rerun this script, and
//! so rebuild the crate, whenever an entry in it changes, is added or is
//! removed. A clean build was never affected.

fn main() {
    println!("cargo::rerun-if-changed=migrations");
}
