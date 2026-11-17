//! conforms: web-the-base-identity-is-blake3-over-the-parts-in-split-order
//!
//! **The base's identity, computed from the cache** (Spec 6.2). An adapter
//! binds to its base by `base-blake3`, which weaver-spu takes as one BLAKE3
//! hasher over every byte of the first part, then of each later part in
//! split order (its `weights_hash` at WeaverAgent's `6b91747e`). Once a
//! decoder's pull is verified, the hasher hashes its cached parts the same
//! way and keeps the digest in one immutable row per repository, commit and
//! choice (`src/store/base_identity.rs`).
//!
//! **One hash at a time, on a thread of its own**: the task reads the owed
//! choices, hashes each on a blocking thread and waits for it before the
//! next, then waits for the puller's word that a pull was verified, or
//! `IDLE_RECHECK`, before it reads again. A start finds every verified pull
//! with no row, so a hash the server's stop cut short runs again: the stop
//! sets a flag the hash reads between its reads, so a stop never waits on a
//! hash of a large model's parts. A drift
//! between this hash and the agent's fails closed, since admission computes
//! the hash again and refuses a mismatch.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Notify;

use crate::hugging_face::cache::Cache;
use crate::hugging_face::choice::in_split_order;
use crate::hugging_face::pull::IDLE_RECHECK;
use crate::hugging_face::{RepoFile, RepoId};
use crate::store::Store;
use crate::store::base_identity::Owed;

/// **The identity of one choice's cached parts**: one BLAKE3 hasher over
/// every byte of each part's blob in split order, whatever order the files
/// are given in, each blob of its file's size. The digest as 64 lowercase
/// hex, and every byte hashed. Ended without a digest where `stop` is set
/// between two reads.
pub fn identity_of(
    cache: &Cache,
    repo: &RepoId,
    files: &[RepoFile],
    stop: &AtomicBool,
) -> anyhow::Result<(String, u64)> {
    let parts = in_split_order(files).map_err(anyhow::Error::msg)?;
    let mut hasher = blake3::Hasher::new();
    let mut hashed = 0u64;
    let mut buffer = vec![0u8; 1 << 20];
    for part in &parts {
        let mut blob = cache.open(repo, part.identity())?;
        anyhow::ensure!(
            blob.metadata()?.len() == part.size,
            "{}'s blob is not of its size",
            part.path
        );
        loop {
            anyhow::ensure!(!stop.load(Ordering::Relaxed), "the server stopped");
            let n = blob.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            hashed += n as u64;
        }
    }
    Ok((hasher.finalize().to_hex().to_string(), hashed))
}

/// **The hasher**: its task and its stop, which the last clone's drop
/// sets, so the server's return from `main` stops it on every path.
#[derive(Clone)]
pub struct BaseIdentities {
    /// Held for its drop alone.
    _stopper: Arc<Stopper>,
}

struct Stopper {
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Stopper {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.task.abort();
    }
}

impl BaseIdentities {
    /// **The hasher started**: it reads the owed choices at once, so a
    /// verified pull with no row is hashed at every start, then each time
    /// `verified` wakes it.
    pub fn start(store: Store, cache: Option<Cache>, verified: Arc<Notify>) -> BaseIdentities {
        let stop = Arc::new(AtomicBool::new(false));
        let task = tokio::spawn(work(store, cache, verified, stop.clone()));
        BaseIdentities {
            _stopper: Arc::new(Stopper { stop, task }),
        }
    }
}

async fn work(store: Store, cache: Option<Cache>, verified: Arc<Notify>, stop: Arc<AtomicBool>) {
    loop {
        match store.base_identities_owed().await {
            Ok(owed) => {
                for one in &owed {
                    if let Err(e) = hash(&store, cache.as_ref(), one, &stop).await {
                        tracing::warn!(
                            "the base identity of {} at {} ({}) was not computed: {e:#}",
                            one.repository,
                            one.commit,
                            one.choice
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("the hasher could not read the owed identities: {e:#}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
        // A pull verified while the hashes ran left its word, so this wait
        // ends at once and the owed choices are read again.
        let _ = tokio::time::timeout(IDLE_RECHECK, verified.notified()).await;
    }
}

/// One owed choice hashed on a blocking thread and its row kept.
async fn hash(
    store: &Store,
    cache: Option<&Cache>,
    owed: &Owed,
    stop: &Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let cache = cache
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no model_cache is set, so no part can be read"))?;
    let repo = RepoId::parse(&owed.repository)
        .ok_or_else(|| anyhow::anyhow!("the repository is not of its shape"))?;
    let files: Vec<RepoFile> = store
        .pull_files(&owed.pull_id)
        .await?
        .into_iter()
        .map(|f| f.file)
        .collect();
    let stop = stop.clone();
    let (digest, bytes) =
        tokio::task::spawn_blocking(move || identity_of(&cache, &repo, &files, &stop)).await??;
    store.keep_base_identity(owed, &digest, bytes).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha256(bytes: &[u8]) -> String {
        crate::store::identity::hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
    }

    /// Each part's bytes written as its blob, named by its SHA-256, and
    /// the files as a listing names them.
    fn cached(cache: &Cache, repo: &RepoId, parts: &[(&str, &[u8])]) -> Vec<RepoFile> {
        let blobs = cache.blobs(repo);
        cache.ensure_dir(&blobs).unwrap();
        parts
            .iter()
            .map(|(path, bytes)| {
                let id = sha256(bytes);
                std::fs::write(cache.blob(repo, &id).unwrap(), bytes).unwrap();
                RepoFile {
                    path: (*path).to_owned(),
                    size: bytes.len() as u64,
                    blob_id: "0123456789abcdef0123456789abcdef01234567".to_owned(),
                    sha256: Some(id),
                }
            })
            .collect()
    }

    /// **The identity of a two-part base is BLAKE3 over part 1's bytes,
    /// then part 2's**, from a listing that names part 2 first, and
    /// differs from part 1's alone and from the parts in the listing's
    /// order. No byte here is a real model's.
    #[test]
    fn the_identity_is_blake3_over_the_parts_in_split_order() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path()).unwrap();
        let repo = RepoId::parse("org/split").unwrap();
        let one: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let two: Vec<u8> = (0..200_000u32).map(|i| (i % 241) as u8).collect();
        let listed = cached(
            &cache,
            &repo,
            &[
                ("m-00002-of-00002.gguf", &two),
                ("m-00001-of-00002.gguf", &one),
            ],
        );
        let (digest, bytes) = identity_of(&cache, &repo, &listed, &AtomicBool::new(false)).unwrap();
        let mut both = blake3::Hasher::new();
        both.update(&one);
        both.update(&two);
        assert_eq!(digest, both.finalize().to_hex().to_string());
        assert_eq!(bytes, (one.len() + two.len()) as u64);
        assert_ne!(
            digest,
            blake3::hash(&one).to_hex().to_string(),
            "part 1's alone"
        );
        let mut listing = blake3::Hasher::new();
        listing.update(&two);
        listing.update(&one);
        assert_ne!(
            digest,
            listing.finalize().to_hex().to_string(),
            "the listing's order"
        );
    }

    /// **A blob not of its file's size is refused**, and so is a part
    /// missing from the split, and a hash the stop ends gives no digest.
    #[test]
    fn a_blob_not_of_its_size_or_a_part_missing_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::at(dir.path()).unwrap();
        let repo = RepoId::parse("org/split").unwrap();
        let mut listed = cached(&cache, &repo, &[("lone.gguf", b"abc")]);
        listed[0].size = 4;
        assert!(identity_of(&cache, &repo, &listed, &AtomicBool::new(false)).is_err());
        let half = cached(&cache, &repo, &[("m-00001-of-00002.gguf", b"abc")]);
        assert!(identity_of(&cache, &repo, &half, &AtomicBool::new(false)).is_err());
        let whole = cached(&cache, &repo, &[("lone.gguf", b"abc")]);
        let stopped = identity_of(&cache, &repo, &whole, &AtomicBool::new(true));
        assert!(stopped.is_err(), "a stop ends the hash");
    }
}
