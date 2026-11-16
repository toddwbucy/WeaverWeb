# Brief: the Hugging Face connection and the model cache

**Date:** 2026-11-16. **From:** the Planner. **To:** the WeaverWeb Executor.
**Authority:** the operator's rulings of 2026-11-16 (setup pages), section 2, and items
11 and 13 of their addendum, with the Planner's decisions of 2026-11-16 on the
Executor's plan. Act 2 of the three the rulings lift the hold for. Act 1 (#120) carries
the rulings into the standing texts, and Act 3 builds the start page and the setup
pages.

## Goal

The console connects the operator's Hugging Face account once, as a console setting,
reads a repository at a commit, says whether the account may fetch it, pulls the chosen
files in the background into a cache in Hugging Face's own layout, hashes each file and
compares it before the file takes its name, and keeps the pull record the rulings name.
The models page that drives a pull is Act 3's. The box's size query and its pull from
the cache over TLS wait on the other side's interface.

## What the rulings fix

1. Hugging Face is the one model source, and its layout is the declared standard for an
   agent's models.
2. The account is a console setting. It reaches public models, the account's private
   repositories and the gated models it has been granted. A gated model the account
   lacks is shown with the link to accept it and a button to check again.
3. The setting takes a read-only or a read-write token. The console never writes to
   Hugging Face except on an explicit push in Train, and no write path is built here.
   The token lives in the console's store.
4. A pull records the repository, the commit, every file's SHA-256 and size, the account
   that pulled and the time. Every download goes through the commit and never a branch
   name. The console hashes after download and compares.
5. The console keeps the model in its cache and reuses it from there. On a host the
   models directory defaults to the user's Hugging Face cache, the console is the only
   account that talks to Hugging Face and the only writer of that directory, and agents
   read the model in place, read-only, through a group.

## The decisions of 2026-11-16

1. **The token is sealed**, with ChaCha20-Poly1305 from `ring`, under a key in
   `authority_dir`. A token the console must present outward cannot be held as a digest,
   so Spec 2.8 and 2.13 gain one stated exception: the tokens the console issues are
   kept as digests, and the one credential it presents to Hugging Face is sealed. A lost
   key means the operator enters the token again, and the texts say so.
2. **The HTTP client is `reqwest`** without default features, on rustls with `ring` and
   the operating system's roots. `Authorization` goes to `huggingface.co` alone, and a
   test shows a redirect to the CDN host carries no token.
3. **`model_cache` is optional.** A pull is refused where it is unset, and the page says
   why.
4. **A pull interrupted by a stop resumes at the next start.**
5. **One pull runs at a time.**
6. **An admin connects the account and pulls.**
7. **The endpoint is `https://huggingface.co`**, fixed, with no mirror key.

## The cache

`model_cache` names the directory that holds the `models--<owner>--<name>` entries. The
console writes Hugging Face's layout itself, so no Hugging Face tool is needed on the
machine: a blob under its SHA-256 for an LFS file and under its git blob id for a file
kept in git, a snapshot under the commit as a relative symbolic link to its blob, and a
download in flight as `<id>.incomplete`. It writes no `refs/`, takes no lock file and
sets no file mode. A host's group read comes from the directory's group and the
service's umask, set outside this repository.

## The sub-acts, in order

1. `act/the-hugging-face-connection-1-spec`, docs only: a new section 2.15 for the
   account and the pulls, the stated exception in 2.8 and 2.13, the claims in section 9
   and the cells in section 10.
2. `act/the-hugging-face-connection-2-the-account`, code: the client, the seal, the
   account's row (migration `0024`), the admin page and `authority seal-key`.
3. `act/the-hugging-face-connection-3-the-cache-and-the-pull`, code: `model_cache`, the
   pulls (migration `0025`), the cache in Hugging Face's layout, the puller and the
   cache's section of the page.

## Acceptance

- Each sub-act passes `cargo fmt --all -- --check`, `cargo build --locked`, `cargo
  clippy --all-targets --locked -- -D warnings` with and without
  `--no-default-features`, and `cargo test --locked` with `DATABASE_URL` and `PGUSER`
  set, the report naming the count passed and that nothing was skipped.
- The tests run against a fake Hugging Face and a fake CDN served in-process. No test
  reaches Hugging Face, and none downloads the first agent's parts. The first fresh
  download is the rulings' first test, after Act 3.
- The connectors' dependency graph stays free of `openssl-sys`
  (`tests/connectors_link.rs`).
- No credential enters the repository. Test tokens never take Hugging Face's `hf_`
  prefix.

## Scope boundary

No models page, no start page, no setup page (Act 3). No frame on the link for a model's
size or a pull (the box's side). No write to Hugging Face. No path, account, group or
mode of any box. The sub-acts open as drafts against the stack and are pushed on the
Planner's word.
