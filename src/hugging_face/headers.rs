//! **The headers read, kept in memory** (Spec 6.2). A choice's header is
//! read by a range read of its first part through the commit, and since a
//! file at a commit never changes, what the read answered is kept by
//! repository, commit and path for the process's life: `MOST_KEPT` entries
//! at most, the oldest dropped first. An answer of the file's own, a header
//! read or one not read for a reason of the file's, is kept. A fault of the
//! read, a gated file or a refused token, is not, since it holds for the
//! read and not for the file. **At most `READS_IN_FLIGHT` reads run at once
//! across the console**, a read past them waiting for a slot.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;

use crate::gguf::BOUND;
use crate::hugging_face::{Client, HeaderRead, RepoId, Token};

/// The most answers kept.
pub const MOST_KEPT: usize = 1024;

/// The most header reads in flight across the console.
pub const READS_IN_FLIGHT: usize = 4;

type Key = (String, String, String);

#[derive(Default)]
struct Kept {
    answers: HashMap<Key, HeaderRead>,
    order: VecDeque<Key>,
}

impl Kept {
    fn keep(&mut self, key: Key, read: HeaderRead) {
        if self.answers.insert(key.clone(), read).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > MOST_KEPT {
            if let Some(oldest) = self.order.pop_front() {
                self.answers.remove(&oldest);
            }
        }
    }
}

/// The headers kept and the slots of the reads in flight, shared by every
/// surface that reads a header.
#[derive(Clone)]
pub struct Headers {
    kept: Arc<Mutex<Kept>>,
    reads: Arc<Semaphore>,
}

impl Default for Headers {
    fn default() -> Headers {
        Headers::new()
    }
}

impl Headers {
    pub fn new() -> Headers {
        Headers {
            kept: Arc::new(Mutex::new(Kept::default())),
            reads: Arc::new(Semaphore::new(READS_IN_FLIGHT)),
        }
    }

    fn key(repo: &RepoId, commit: &str, path: &str) -> Key {
        (repo.to_string(), commit.to_owned(), path.to_owned())
    }

    /// What a read of the file at the commit answered, where it is kept.
    pub fn kept(&self, repo: &RepoId, commit: &str, path: &str) -> Option<HeaderRead> {
        let key = Headers::key(repo, commit, path);
        self.kept.lock().unwrap().answers.get(&key).cloned()
    }

    /// **The file's header**: the answer kept, else a read under one of the
    /// slots, bounded at 16 MiB, its answer kept where it is the file's.
    pub async fn read(
        &self,
        client: &Client,
        token: Option<&Token>,
        repo: &RepoId,
        commit: &str,
        path: &str,
    ) -> anyhow::Result<HeaderRead> {
        if let Some(read) = self.kept(repo, commit, path) {
            return Ok(read);
        }
        let _slot = self.reads.acquire().await?;
        // A read that waited for its slot may find the answer another kept.
        if let Some(read) = self.kept(repo, commit, path) {
            return Ok(read);
        }
        let read = client.header(token, repo, commit, path, BOUND).await?;
        self.kept
            .lock()
            .unwrap()
            .keep(Headers::key(repo, commit, path), read.clone());
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **At most 1024 answers are kept, the oldest dropped first**, and an
    /// answer kept again keeps its place.
    #[test]
    fn at_most_1024_answers_are_kept_the_oldest_dropped_first() {
        let mut kept = Kept::default();
        let key = |n: usize| ("org/m".to_owned(), "c".to_owned(), format!("{n}.gguf"));
        for n in 0..MOST_KEPT {
            kept.keep(key(n), HeaderRead::NotRead(n.to_string()));
        }
        kept.keep(key(0), HeaderRead::NotRead("again".into()));
        assert_eq!(kept.answers.len(), MOST_KEPT);
        kept.keep(key(MOST_KEPT), HeaderRead::NotRead("last".into()));
        assert_eq!(kept.answers.len(), MOST_KEPT);
        assert_eq!(kept.order.len(), MOST_KEPT);
        assert!(!kept.answers.contains_key(&key(0)), "the oldest is dropped");
        assert!(kept.answers.contains_key(&key(1)));
        assert!(kept.answers.contains_key(&key(MOST_KEPT)));
    }
}
