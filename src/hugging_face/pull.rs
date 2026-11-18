//! conforms: web-authorization-goes-to-hugging-face-alone
//! conforms: web-every-download-names-the-commit
//! conforms: web-no-file-takes-its-blobs-name-before-its-digest-matches
//! conforms: web-a-pull-asked-through-an-account-never-runs-without-its-token
//! conforms: web-the-free-space-is-checked-against-what-the-run-has-yet-to-write
//! conforms: web-a-server-that-answers-a-range-with-the-whole-body-restarts-the-file
//! conforms: web-an-admins-cancel-stops-the-running-pull-and-keeps-its-partial
//!
//! **The puller** (Spec 2.15): one task that runs the pull the store names
//! `running`, file after file, and waits for the next ask. One pull runs at
//! a time, so a pull of one model fills the link alone and the free-space
//! check stays true. **A pull running when the server stops resumes at the
//! next start**: the task's first act is to read the running pull, and a
//! file's partial resumes by range after its bytes are hashed again. **The
//! account is read again before each file and each retry**, so a pull
//! whose account is disconnected or replaced stops there. **An admin's
//! cancel stops the running pull** at its next read or wait, its partial
//! kept for a later pull to resume.
//!
//! **A file's pull takes six steps.** Its names are checked. A blob that
//! stands and hashes right is recorded as found. The free space is checked
//! against what remains. The file is fetched through the commit, with
//! `Authorization` on Hugging Face's own origin alone (`Client::authorized`),
//! every redirect followed by this module and none by the client. The
//! partial is written while it is hashed. The digest is compared, and only
//! a match renames the partial to its blob and links the snapshot.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use reqwest::header::{CONTENT_RANGE, LOCATION, RANGE};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Notify, watch};

use super::cache::{Cache, Hasher, hash_from_start};
use super::seal::SealKey;
use super::{Client, RepoFile, RepoId, Token, is_commit, is_repo_path};
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::model_pull::{Pull, PullFile, Source};

/// The bound on a fetch's answer and on each read of its body: past it the
/// read is a stall, retried by range.
pub const STALL: Duration = Duration::from_secs(60);

/// How many stalls or cuts a file survives before its pull fails.
pub const RETRIES: u32 = 5;

/// The pause before a file's next fetch after a stall, a cut or an answer
/// asking for one later, times the setbacks so far.
pub const BACKOFF: Duration = Duration::from_secs(5);

/// **How long an idle puller waits before it reads the store again**,
/// woken or not: a pull whose ask's commit answer was lost may land with
/// nothing to wake the puller for it.
pub const IDLE_RECHECK: Duration = Duration::from_secs(15);

/// How many redirects a fetch follows.
const REDIRECTS: usize = 5;

/// How often a file's progress is written: every this many bytes.
const PROGRESS_EVERY: u64 = 8 << 20;

/// The puller's bounds and its free-space seam.
#[derive(Clone)]
pub struct Settings {
    pub stall: Duration,
    pub retries: u32,
    pub backoff: Duration,
    /// The bytes free where a repository's blobs are written.
    pub free_space: fn(&Path) -> io::Result<u64>,
    /// How long an idle puller waits before it reads the store again.
    pub idle_recheck: Duration,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            stall: STALL,
            retries: RETRIES,
            backoff: BACKOFF,
            free_space: Cache::free_space,
            idle_recheck: IDLE_RECHECK,
        }
    }
}

struct Inner {
    store: Store,
    client: Client,
    cache: Option<Cache>,
    authority_dir: PathBuf,
    settings: Settings,
    wake: Notify,
    /// **The word that a pull was verified**, which the base's identity
    /// waits on (Spec 6.2): a word left with no waiter is kept for the
    /// next wait.
    verified: Arc<Notify>,
    /// **Every pull an admin's cancel ended, or may have ended where its
    /// answer was lost**, that the puller may still be in, which the running
    /// pull watches. A set, so a later cancel never undoes an earlier one.
    /// An id is removed when the puller leaves its pull, and every id when
    /// the store shows nothing running: the page signals a cancel only for a
    /// pull it read running, and a pull that no longer runs never runs
    /// again, so the clear drops no id of a pull the puller may yet take.
    cancel: watch::Sender<HashSet<String>>,
    task: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// How many cancels were signalled, for the tests: a signal the worker
    /// has already acted on leaves no id in `cancel`, and this count stays.
    #[cfg(test)]
    cancels_signalled: std::sync::atomic::AtomicUsize,
}

/// The puller: its task and the seam an ask wakes it through.
#[derive(Clone)]
pub struct Puller {
    inner: Arc<Inner>,
}

/// Why a pull ended before its last file, or why its run paused.
enum Ended {
    /// The pull failed, for the reason given, written on its row.
    Failed(String),
    /// The store could not be reached: the pull stays `running`, and the
    /// task tries again after a pause.
    Store(anyhow::Error),
    /// An admin's cancel ended the pull, whose row says so already.
    Cancelled,
}

impl From<anyhow::Error> for Ended {
    fn from(e: anyhow::Error) -> Ended {
        Ended::Store(e)
    }
}

fn failed(reason: impl Into<String>) -> Ended {
    Ended::Failed(reason.into())
}

/// Why a fetch did not answer with its body.
enum Fetch {
    Failed(String),
    Retry,
}

impl Puller {
    /// **The puller started**: its task reads the running pull first, so
    /// one the server's stop interrupted resumes here.
    pub fn start(
        store: Store,
        client: Client,
        cache: Option<Cache>,
        authority_dir: PathBuf,
        settings: Settings,
    ) -> Puller {
        let inner = Arc::new(Inner {
            store,
            client,
            cache,
            authority_dir,
            settings,
            wake: Notify::new(),
            verified: Arc::new(Notify::new()),
            cancel: watch::Sender::new(HashSet::new()),
            task: std::sync::Mutex::new(None),
            #[cfg(test)]
            cancels_signalled: std::sync::atomic::AtomicUsize::new(0),
        });
        let task = tokio::spawn(work(inner.clone()));
        *inner.task.lock().unwrap() = Some(task);
        Puller { inner }
    }

    /// The cache, where `model_cache` is set.
    pub fn cache(&self) -> Option<&Cache> {
        self.inner.cache.as_ref()
    }

    /// **The word each verified pull leaves**, for the base's identity to
    /// wait on (Spec 6.2).
    pub fn verified(&self) -> Arc<Notify> {
        self.inner.verified.clone()
    }

    /// The task stopped where it stands, as the server's stop stops it. A
    /// partial stays on disk for the next start.
    pub fn stop(&self) {
        if let Some(task) = self.inner.task.lock().unwrap().take() {
            task.abort();
        }
    }

    /// **The running pull told of its cancel**, once the cancel's row is
    /// written or where the write's answer was lost, since it may have
    /// landed: it stops at its next read or wait, and its partial stays. A
    /// pull the store still runs is taken again from its partial.
    pub fn cancel(&self, pull_id: &str) {
        #[cfg(test)]
        self.inner
            .cancels_signalled
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.inner.cancel.send_modify(|ids| {
            ids.insert(pull_id.to_owned());
        });
    }

    /// How many cancelled pulls the puller holds, for the pull's tests,
    /// which build with the `passkeys` feature alone.
    #[cfg(all(test, feature = "passkeys"))]
    pub(crate) fn cancels_held(&self) -> usize {
        self.inner.cancel.borrow().len()
    }

    /// How many cancels were ever signalled to the puller, for the pull's
    /// tests: unlike [`Puller::cancels_held`], a signal the worker already
    /// acted on and cleared still counts.
    #[cfg(all(test, feature = "passkeys"))]
    pub(crate) fn cancels_signalled(&self) -> usize {
        self.inner
            .cancels_signalled
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// **An admin's ask of a pull**, after its first record: the rows
    /// written in the admin's transaction with the `ok` outcome, then the
    /// task woken. Refused where no cache is set, where a file's path is out
    /// of its shape or no file is asked, and where another pull runs.
    #[allow(clippy::too_many_arguments)]
    pub async fn ask(
        &self,
        admin: &str,
        session_id: i64,
        first: &str,
        pull_id: &str,
        repo: &RepoId,
        commit: &str,
        account: Option<&str>,
        files: &[RepoFile],
    ) -> anyhow::Result<Result<(), Refusal>> {
        self.ask_with(commit, files, || {
            self.inner.store.ask_pull(
                admin, session_id, first, pull_id, repo, commit, account, files,
            )
        })
        .await
    }

    /// **An ask of a pull whose rows `write` lands**, after its first
    /// record: the setup's pick writes its pull with its role's row in one
    /// transaction (Spec 6.2), and the cache's page writes it alone.
    /// Refused before `write` where no cache is set, where a file's path is
    /// out of its shape or no file is asked, and by `write` where another
    /// pull runs. Every answer of `write` wakes the puller.
    pub async fn ask_with<F, Fut>(
        &self,
        commit: &str,
        files: &[RepoFile],
        write: F,
    ) -> anyhow::Result<Result<(), Refusal>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<Result<(), Refusal>>>,
    {
        if self.inner.cache.is_none() {
            return Ok(Err(Refusal::NoModelCache));
        }
        if !is_commit(commit) {
            return Ok(Err(Refusal::Name(
                "a pull names a 40-hex commit and never a branch".into(),
            )));
        }
        if files.is_empty() {
            return Ok(Err(Refusal::Name("a pull names at least one file".into())));
        }
        if let Some(file) = files.iter().find(|f| !is_repo_path(&f.path)) {
            return Ok(Err(Refusal::Name(format!(
                "{:?} is not a file's path in a repository",
                file.path
            ))));
        }
        let written = write().await;
        // Every answer wakes the puller, which acts on the store's rows
        // alone: a commit whose answer was lost may have landed.
        self.inner.wake.notify_one();
        written
    }
}

async fn work(inner: Arc<Inner>) {
    loop {
        match inner.store.running_pull().await {
            Ok(Some(pull)) => {
                match run(&inner, &pull).await {
                    Ok(()) => finish(&inner, &pull, None).await,
                    Err(Ended::Failed(reason)) => {
                        tracing::warn!("the pull {} failed: {reason}", pull.pull_id);
                        finish(&inner, &pull, Some(&reason)).await;
                    }
                    Err(Ended::Store(e)) => {
                        tracing::error!("the pull {} paused on the store: {e:#}", pull.pull_id);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                    Err(Ended::Cancelled) => {
                        tracing::info!("the pull {} stopped at an admin's cancel", pull.pull_id);
                    }
                }
                inner
                    .cancel
                    .send_if_modified(|ids| ids.remove(&pull.pull_id));
            }
            Ok(None) => {
                // Nothing runs, so every cancelled id is of a pull that has
                // ended and never runs again: a cancel is signalled only for
                // a pull the page read running, and the next read follows
                // this.
                inner.cancel.send_if_modified(|ids| {
                    let held = !ids.is_empty();
                    ids.clear();
                    held
                });
                // Woken, or the store read again within `idle_recheck`: a
                // pull whose ask's commit outlasted its read-back lands after
                // the ask's wake, with nothing else to wake the puller.
                let _ =
                    tokio::time::timeout(inner.settings.idle_recheck, inner.wake.notified()).await;
            }
            Err(e) => {
                tracing::error!("the puller could not read the running pull: {e:#}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

async fn finish(inner: &Inner, pull: &Pull, reason: Option<&str>) {
    while let Err(e) = inner.store.pull_finished(&pull.pull_id, reason).await {
        tracing::error!(
            "the end of the pull {} was not written: {e:#}",
            pull.pull_id
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    if reason.is_none() {
        inner.verified.notify_one();
    }
}

/// The account's token, where one is connected and its seal opens.
/// **The token the pull runs with**: none for a pull asked without an
/// account, and for one asked with an account, that account's token, or the
/// pull fails, since a pull whose record names an account never runs
/// without its token.
async fn token(inner: &Inner, pull: &Pull) -> Result<Option<Token>, Ended> {
    let Some(asked) = &pull.account else {
        return Ok(None);
    };
    let Some(row) = inner.store.hugging_face_account().await? else {
        return Err(failed(format!(
            "the pull was asked through the account {asked}, which is disconnected"
        )));
    };
    if row.account != *asked {
        return Err(failed(format!(
            "the pull was asked through the account {asked}, and {} is connected",
            row.account
        )));
    }
    let key = SealKey::load(&inner.authority_dir)
        .map_err(|e| failed(format!("the seal key could not be read: {e:#}")))?
        .ok_or_else(|| failed("the console holds no seal key, so the token cannot be opened"))?;
    key.open(&row.sealed).map(Some).ok_or_else(|| {
        failed("the account's token does not open under the seal key, so connect it again")
    })
}

async fn run(inner: &Inner, pull: &Pull) -> Result<(), Ended> {
    let cache = inner
        .cache
        .as_ref()
        .ok_or_else(|| failed("no model_cache is set, so the pull cannot run"))?;
    let repo = RepoId::parse(&pull.repository)
        .ok_or_else(|| failed("the pull's repository is not of its shape"))?;
    if !is_commit(&pull.commit) {
        return Err(failed("the pull names no commit"));
    }
    let mut cancel = inner.cancel.subscribe();
    let files = inner.store.pull_files(&pull.pull_id).await?;
    let pending: Vec<&PullFile> = files.iter().filter(|f| f.source.is_none()).collect();
    for (at, done) in pending.iter().enumerate() {
        let file = &done.file;
        if is_cancelled(&cancel, pull) {
            return Err(Ended::Cancelled);
        }
        // The account read again before each file.
        let token = token(inner, pull).await?;
        // Step 1: every name checked before any write.
        if !is_repo_path(&file.path) {
            return Err(failed(format!(
                "{:?} is not a file's path in a repository",
                file.path
            )));
        }
        let blobs = cache.blobs(&repo);
        cache
            .ensure_dir(&blobs)
            .map_err(|e| failed(format!("{}: {e:#}", file.path)))?;
        // Step 2: a blob that stands and hashes right is found.
        if let Some(sha256) = found(cache, &repo, pull, file, &cancel).await? {
            inner
                .store
                .pull_file_done(&pull.pull_id, &file.path, &sha256, Source::Found)
                .await?;
            continue;
        }
        // Step 3: the free space against every byte that remains.
        let remaining = remaining(cache, &repo, &pending[at..]);
        let free = (inner.settings.free_space)(&blobs)
            .map_err(|e| failed(format!("the cache's free space could not be read: {e}")))?;
        if free < remaining {
            return Err(failed(format!(
                "the cache needs {remaining} bytes free for what remains and has {free}"
            )));
        }
        // Steps 4 to 6.
        let sha256 = download(inner, cache, &repo, pull, file, token, &mut cancel).await?;
        inner
            .store
            .pull_file_done(&pull.pull_id, &file.path, &sha256, Source::Downloaded)
            .await?;
    }
    Ok(())
}

/// **The bytes that remain**, over the file at hand and the files after
/// it, the ones this run has yet to verify: each less what its partial
/// holds, and a later file whose blob stands at its size counting nothing,
/// since step 2 finds it. The file at hand counts whole less its partial,
/// since step 2 has just found its blob missing or not hashing right. **A
/// blob two paths share counts once**, since the blob and its partial are
/// named by its identity and step 2 finds it for the second path.
fn remaining(cache: &Cache, repo: &RepoId, rest: &[&PullFile]) -> u64 {
    let regular = |path: anyhow::Result<PathBuf>| {
        path.ok()
            .and_then(|p| std::fs::symlink_metadata(p).ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
    };
    let mut counted = HashSet::new();
    rest.iter()
        .enumerate()
        .map(|(at, f)| {
            let id = f.file.blob();
            if !counted.insert(id) {
                return 0;
            }
            if at > 0 && regular(cache.blob(repo, id)) == Some(f.file.size) {
                return 0;
            }
            let held = regular(cache.incomplete(repo, id)).unwrap_or(0);
            f.file.size.saturating_sub(held)
        })
        .sum()
}

/// Whether an admin's cancel has ended `pull`.
fn is_cancelled(cancel: &watch::Receiver<HashSet<String>>, pull: &Pull) -> bool {
    cancel.borrow().contains(&pull.pull_id)
}

/// **Answers once an admin's cancel ends `pull`**, and never where the
/// puller is gone.
async fn cancelled(cancel: &mut watch::Receiver<HashSet<String>>, pull: &Pull) {
    let ended = cancel
        .wait_for(|ids| ids.contains(&pull.pull_id))
        .await
        .is_ok();
    if !ended {
        std::future::pending::<()>().await;
    }
}

/// **Step 2**: the file's blob, where it stands as a regular file of the
/// file's size and hashes to its identity, linked from the snapshot and
/// answered with its SHA-256. A blob that does not hash right is not
/// trusted, and the file is fetched again. A cancel that lands during the
/// check stops the file once the check finishes, before the link.
async fn found(
    cache: &Cache,
    repo: &RepoId,
    pull: &Pull,
    file: &RepoFile,
    cancel: &watch::Receiver<HashSet<String>>,
) -> Result<Option<String>, Ended> {
    let id = file.blob().to_owned();
    let mut blob = match cache.open(repo, &id) {
        Ok(blob) => blob,
        Err(_) => return Ok(None),
    };
    match blob.metadata() {
        Ok(meta) if meta.len() == file.size => {}
        _ => return Ok(None),
    }
    let (size, in_git) = (file.size, file.in_git());
    #[cfg(test)]
    let checked = cache
        .blob(repo, &id)
        .map_err(|e| failed(format!("{e:#}")))?;
    let hashed = tokio::task::spawn_blocking(move || {
        let digest = hash_from_start(&mut blob, size, in_git);
        #[cfg(test)]
        hold_after_hash(&checked);
        digest
    })
    .await
    .map_err(|e| failed(format!("a blob's check stopped: {e}")))?;
    // A cancel that landed while the check ran stops the file before its
    // link is written or it is recorded (Spec 2.15).
    if is_cancelled(cancel, pull) {
        return Err(Ended::Cancelled);
    }
    let Ok((hasher, _)) = hashed else {
        return Ok(None);
    };
    match hasher.verify(file) {
        Ok(sha256) => {
            cache
                .link(repo, &pull.commit, &file.path, &id)
                .map_err(|e| failed(format!("{}: {e:#}", file.path)))?;
            Ok(Some(sha256))
        }
        Err(_) => Ok(None),
    }
}

/// **The partials this process re-hashed**, for the tests alone, as
/// `link::authority`'s faults are: a fixture hashes too fast for a test to
/// time a cancel into it, so a test reads whether a partial was hashed after
/// a cancel. Each test's cache is a directory of its own, so a test reads
/// its own paths whatever runs beside it.
#[cfg(test)]
pub(crate) static HASHED_PARTIALS: std::sync::Mutex<Vec<PathBuf>> =
    std::sync::Mutex::new(Vec::new());

/// **The hashes a test holds once they finish**, by the path hashed:
/// whether the hold was reached, and whether the test has released it. A
/// fixture hashes too fast for a test to time a cancel into it, so a test
/// holds the hash's end as a long hash would, on the blocking thread the
/// hash runs on, within ten seconds.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(crate) static HOLD_AFTER_HASH: std::sync::Mutex<
    Vec<(
        PathBuf,
        Arc<(std::sync::atomic::AtomicBool, std::sync::atomic::AtomicBool)>,
    )>,
> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn hold_after_hash(path: &Path) {
    use std::sync::atomic::Ordering;
    let held = HOLD_AFTER_HASH
        .lock()
        .unwrap()
        .iter()
        .find(|(hashed, _)| hashed.as_path() == path)
        .map(|(_, held)| held.clone());
    if let Some(held) = held {
        held.0.store(true, Ordering::SeqCst);
        for _ in 0..1000 {
            if held.1.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// **Steps 4 to 6 for one file**: the partial opened without following a
/// symbolic link and hashed again, fetched by range from what it holds,
/// written while it is hashed, and renamed to its blob only on a match.
/// **Before each retry the account is read again**, and **a cancel stops
/// the file at its next read or wait**, or once a hash under way finishes,
/// before its blob takes its name, the partial flushed and kept.
async fn download(
    inner: &Inner,
    cache: &Cache,
    repo: &RepoId,
    pull: &Pull,
    file: &RepoFile,
    mut token: Option<Token>,
    cancel: &mut watch::Receiver<HashSet<String>>,
) -> Result<String, Ended> {
    // A cancel that landed during step 2's blob check, or since the loop's
    // own check, starts no partial's hash: only a hash already under way
    // finishes after a cancel (Spec 2.15).
    if is_cancelled(cancel, pull) {
        return Err(Ended::Cancelled);
    }
    let id = file.blob().to_owned();
    let named = |e: &dyn std::fmt::Display| failed(format!("{}: {e}", file.path));
    let partial = cache.incomplete(repo, &id).map_err(|e| named(&e))?;
    let blob = cache.blob(repo, &id).map_err(|e| named(&e))?;
    let out = open_partial(&partial).map_err(|e| named(&e))?;
    let (size, in_git) = (file.size, file.in_git());
    let mut reading = out.try_clone().map_err(|e| named(&e))?;
    #[cfg(test)]
    HASHED_PARTIALS.lock().unwrap().push(partial.clone());
    #[cfg(test)]
    let hashed = partial.clone();
    let (mut hasher, mut have) = tokio::task::spawn_blocking(move || {
        let digest = hash_from_start(&mut reading, size, in_git);
        #[cfg(test)]
        hold_after_hash(&hashed);
        digest
    })
    .await
    .map_err(|e| named(&e))?
    .map_err(|e| named(&e))?;
    let mut out = tokio::fs::File::from_std(out);
    if have > size {
        out.set_len(0).await.map_err(|e| named(&e))?;
        (hasher, have) = (Hasher::new(size, in_git), 0);
    }
    out.seek(io::SeekFrom::Start(have))
        .await
        .map_err(|e| named(&e))?;
    let mut setbacks = 0u32;
    let mut written_at = have;
    let mut first = true;
    while have < size {
        if !first {
            // A retry: the cancel and the account read again first.
            if is_cancelled(cancel, pull) {
                return Err(Ended::Cancelled);
            }
            token = self::token(inner, pull).await?;
        }
        first = false;
        // The wait for the response's headers, every redirect's included,
        // ends at a cancel as the reads do.
        let fetched = tokio::select! {
            fetched = fetch(inner, repo, &pull.commit, &file.path, have, token.as_ref()) => fetched,
            () = cancelled(cancel, pull) => {
                out.flush().await.map_err(|e| named(&e))?;
                inner.store.pull_progress(&pull.pull_id, &file.path, have).await?;
                return Err(Ended::Cancelled);
            }
        };
        match fetched {
            Ok((mut response, start)) => {
                if start == 0 && have > 0 {
                    out.set_len(0).await.map_err(|e| named(&e))?;
                    out.seek(io::SeekFrom::Start(0))
                        .await
                        .map_err(|e| named(&e))?;
                    (hasher, have, written_at) = (Hasher::new(size, in_git), 0, 0);
                }
                // The body's end, a cut or a stall ends the reads, and what
                // is short of the size is fetched again by range. A cancel
                // ends them too, the partial flushed and kept.
                loop {
                    let read = tokio::select! {
                        read = tokio::time::timeout(inner.settings.stall, response.chunk()) => read,
                        () = cancelled(cancel, pull) => {
                            out.flush().await.map_err(|e| named(&e))?;
                            inner.store.pull_progress(&pull.pull_id, &file.path, have).await?;
                            return Err(Ended::Cancelled);
                        }
                    };
                    let Ok(Ok(Some(chunk))) = read else { break };
                    if have + chunk.len() as u64 > size {
                        drop(out);
                        let _ = std::fs::remove_file(&partial);
                        return Err(failed(format!(
                            "{}: Hugging Face sent more than its {size} bytes",
                            file.path
                        )));
                    }
                    out.write_all(&chunk).await.map_err(|e| named(&e))?;
                    hasher.update(&chunk);
                    have += chunk.len() as u64;
                    if have - written_at >= PROGRESS_EVERY {
                        inner
                            .store
                            .pull_progress(&pull.pull_id, &file.path, have)
                            .await?;
                        written_at = have;
                    }
                }
                if have < size {
                    setbacks += 1;
                }
            }
            Err(Fetch::Failed(reason)) => {
                return Err(failed(format!("{}: {reason}", file.path)));
            }
            Err(Fetch::Retry) => setbacks += 1,
        }
        out.flush().await.map_err(|e| named(&e))?;
        inner
            .store
            .pull_progress(&pull.pull_id, &file.path, have)
            .await?;
        written_at = have;
        if have < size && setbacks > inner.settings.retries {
            return Err(failed(format!(
                "{}: the download stalled or was cut {setbacks} times, at {have} of {size} bytes",
                file.path
            )));
        }
        if have < size && setbacks > 0 {
            tokio::select! {
                () = tokio::time::sleep(inner.settings.backoff * setbacks) => {}
                () = cancelled(cancel, pull) => return Err(Ended::Cancelled),
            }
        }
    }
    // A cancel that landed while the partial's hash ran, which a partial
    // holding every byte never follows with a read, or after the last read,
    // stops the file before its blob takes its name: the partial stays
    // (Spec 2.15).
    if is_cancelled(cancel, pull) {
        out.flush().await.map_err(|e| named(&e))?;
        inner
            .store
            .pull_progress(&pull.pull_id, &file.path, have)
            .await?;
        return Err(Ended::Cancelled);
    }
    out.sync_all().await.map_err(|e| named(&e))?;
    drop(out);
    // Step 6: the digest compared, and only a match takes the blob's name.
    let sha256 = match hasher.verify(file) {
        Ok(sha256) => sha256,
        Err(reason) => {
            let _ = std::fs::remove_file(&partial);
            return Err(failed(reason));
        }
    };
    std::fs::rename(&partial, &blob).map_err(|e| named(&e))?;
    if let Some(blobs) = blob.parent() {
        // Opened as a directory and never through a link, so a FIFO at the
        // name is refused at once.
        use std::os::unix::fs::OpenOptionsExt;
        let _ = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(blobs)
            .and_then(|d| d.sync_all());
    }
    cache
        .link(repo, &pull.commit, &file.path, &id)
        .map_err(|e| named(&e))?;
    Ok(sha256)
}

/// **The partial opened to read and write**, created where it is missing,
/// never truncated, and never through a symbolic link: a link planted at
/// its name refuses the file.
fn open_partial(path: &Path) -> anyhow::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ELOOP) => {
                anyhow::anyhow!(
                    "a symbolic link stands at the partial's name, so nothing is written"
                )
            }
            _ => anyhow::anyhow!("the partial could not be opened: {e}"),
        })?;
    anyhow::ensure!(
        file.metadata()?.is_file(),
        "the partial's name is not a regular file"
    );
    Ok(file)
}

/// **Step 4: the fetch through the commit**, from byte `from`, following
/// each redirect itself: the token on Hugging Face's own origin alone, and
/// a redirect to another scheme than the base's refused.
async fn fetch(
    inner: &Inner,
    repo: &RepoId,
    commit: &str,
    path: &str,
    from: u64,
    token: Option<&Token>,
) -> Result<(reqwest::Response, u64), Fetch> {
    let client = &inner.client;
    let mut url = client
        .resolve(repo, commit, path)
        .map_err(|e| Fetch::Failed(format!("{e:#}")))?;
    for _ in 0..=REDIRECTS {
        let mut request = client.authorized(client.http().get(url.clone()), &url, token);
        if from > 0 {
            request = request.header(RANGE, format!("bytes={from}-"));
        }
        let response = match tokio::time::timeout(inner.settings.stall, request.send()).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) | Err(_) => return Err(Fetch::Retry),
        };
        let status = response.status();
        if status.is_redirection() {
            let next = response
                .headers()
                .get(LOCATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|location| url.join(location).ok())
                .ok_or_else(|| Fetch::Failed("a redirect named no place".into()))?;
            if next.scheme() != client.base().scheme() {
                return Err(Fetch::Failed(format!(
                    "a redirect to {} is refused",
                    next.scheme()
                )));
            }
            url = next;
            continue;
        }
        let code = response
            .headers()
            .get("x-error-code")
            .and_then(|v| v.to_str().ok());
        return match status {
            StatusCode::OK => Ok((response, 0)),
            StatusCode::PARTIAL_CONTENT => {
                let starts = response
                    .headers()
                    .get(CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|range| range.starts_with(&format!("bytes {from}-")));
                if starts {
                    Ok((response, from))
                } else {
                    Err(Fetch::Failed(
                        "a range answered from another byte than asked".into(),
                    ))
                }
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN if code == Some("GatedRepo") => {
                Err(Fetch::Failed(format!(
                    "{repo} is gated, and the account has not been granted it"
                )))
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                Err(Fetch::Failed("Hugging Face refused the fetch".into()))
            }
            StatusCode::NOT_FOUND => Err(Fetch::Failed(format!(
                "the file does not stand at {commit}"
            ))),
            StatusCode::TOO_MANY_REQUESTS => Err(Fetch::Retry),
            status if status.is_server_error() => Err(Fetch::Retry),
            status => Err(Fetch::Failed(format!("Hugging Face answered {status}"))),
        };
    }
    Err(Fetch::Failed(format!("more than {REDIRECTS} redirects")))
}
