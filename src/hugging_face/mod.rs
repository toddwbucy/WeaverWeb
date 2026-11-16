//! **The Hugging Face client** (Spec 2.15): the one outward request the
//! server makes. It speaks to `https://huggingface.co` alone, fixed, with no
//! mirror key (this document's election of 2026-11-16, the Spec's): its base
//! is a constructor argument so a test can stand a fake in its place, and no
//! configuration reaches it.
//!
//! **`Authorization` goes to the base's origin alone.** The client follows no
//! redirect by itself, so no request leaves with the token unless this module
//! put it there, and it puts it only on a request to the base's own scheme,
//! host and port (`Client::authorized`). A download's redirect to a CDN host
//! (sub-act 3) is followed by the puller, without the header.
//!
//! **Every name that reaches a URL is checked first**: a repository's
//! identity (`RepoId`), a revision (`is_revision`), a commit (`is_commit`) and
//! a file's path (`is_repo_path`), each pushed as path segments, so none adds
//! a segment, a query or a fragment of its own.

pub mod seal;

use std::time::Duration;

use reqwest::{StatusCode, Url};
use serde::Deserialize;

/// The one endpoint, fixed.
pub const ENDPOINT: &str = "https://huggingface.co";

/// The bound on one API request, from its send to its whole body read.
pub const API_BOUND: Duration = Duration::from_secs(30);

/// The bound on a connection's opening, for every request.
const CONNECT_BOUND: Duration = Duration::from_secs(10);

/// The most an API answer is read: a repository's listing of every file runs
/// to a few hundred kilobytes for the largest model repositories.
const API_BODY_LIMIT: usize = 16 << 20;

/// **A Hugging Face token**, as an admin pasted it. It renders as `Token(..)`
/// and never as its bytes, and nothing but this module and the seal reads
/// them.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

impl Token {
    /// A token of a token's shape: 1 to 256 bytes, every one a visible ASCII
    /// character. The console reads no prefix, so a token of any issuer's
    /// form is taken, and Hugging Face says whether it is one.
    pub fn parse(text: &str) -> Option<Token> {
        ((1..=256).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_graphic()))
            .then(|| Token(text.to_owned()))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

/// The role of a token, as `whoami-v2` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Read,
    Write,
    FineGrained,
}

impl Role {
    /// The role as the store and the page name it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Read => "read",
            Role::Write => "write",
            Role::FineGrained => "fine-grained",
        }
    }

    fn of_hugging_face(role: &str) -> Option<Role> {
        match role {
            "read" => Some(Role::Read),
            "write" => Some(Role::Write),
            "fineGrained" => Some(Role::FineGrained),
            _ => None,
        }
    }
}

/// The account a token belongs to and the token's role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub name: String,
    pub role: Role,
}

/// What `whoami-v2` answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Whoami {
    Account(Account),
    /// Hugging Face refused the token (401).
    Refused,
}

/// **A repository's identity**, `<owner>/<name>`. Each half is 1 to 96
/// characters of letters, digits, `-`, `_` and `.`, neither starts nor ends
/// with `-` or `.`, and neither holds `--` or `..`, Hugging Face's own rule,
/// which also keeps the cache's `models--<owner>--<name>` one name per
/// repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoId {
    owner: String,
    name: String,
}

fn is_repo_half(half: &str) -> bool {
    (1..=96).contains(&half.len())
        && half
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !half.starts_with(['-', '.'])
        && !half.ends_with(['-', '.'])
        && !half.contains("--")
        && !half.contains("..")
}

impl RepoId {
    pub fn parse(text: &str) -> Option<RepoId> {
        let (owner, name) = text.split_once('/')?;
        (is_repo_half(owner) && is_repo_half(name)).then(|| RepoId {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Display for RepoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

/// A commit: forty lowercase hex digits.
pub fn is_commit(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A revision a person may name: a commit, a branch or a tag. 1 to 255
/// characters of letters, digits, `-`, `_`, `.` and `/`, with no empty
/// component and no `.` or `..` component. It is sent as one path segment,
/// its `/` escaped.
pub fn is_revision(text: &str) -> bool {
    (1..=255).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'/'))
        && text
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// **A file's path inside a repository**, the first of a pull's six steps
/// (Spec 2.15): 1 to 1024 bytes, `/`-separated, no empty component, no `.`
/// or `..` component, no absolute path, no backslash and no control
/// character, NUL among them. A path that passes names a place under its
/// snapshot and nowhere else.
pub fn is_repo_path(text: &str) -> bool {
    (1..=1024).contains(&text.len())
        && !text.bytes().any(|b| b.is_ascii_control() || b == b'\\')
        && text
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Whether a repository is gated, as its listing says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gated {
    No,
    Auto,
    Manual,
}

/// One file of a repository at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFile {
    pub path: String,
    pub size: u64,
    /// The git blob id, forty lowercase hex digits.
    pub blob_id: String,
    /// The SHA-256 Hugging Face publishes, for a file kept in LFS alone.
    pub sha256: Option<String>,
}

/// A repository at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub commit: String,
    pub gated: Gated,
    pub private: bool,
    pub files: Vec<RepoFile>,
}

/// What a repository's listing answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelAt {
    Model(Model),
    /// No such repository or revision, or none the account may see.
    Missing,
    /// Hugging Face refused the token.
    Refused,
}

/// Whether the account may fetch a repository's files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    Granted,
    /// Gated, and the account has not been granted it: the page a person
    /// accepts the repository's terms on.
    Gated {
        accept: String,
    },
    Missing,
    Refused,
}

#[derive(Deserialize)]
struct WhoamiBody {
    name: Option<String>,
    auth: Option<WhoamiAuth>,
}

#[derive(Deserialize)]
struct WhoamiAuth {
    #[serde(rename = "accessToken")]
    access_token: Option<WhoamiToken>,
}

#[derive(Deserialize)]
struct WhoamiToken {
    role: Option<String>,
}

#[derive(Deserialize)]
struct ListingBody {
    sha: Option<String>,
    #[serde(default)]
    gated: serde_json::Value,
    #[serde(default)]
    private: bool,
    siblings: Option<Vec<Sibling>>,
}

#[derive(Deserialize)]
struct Sibling {
    rfilename: Option<String>,
    size: Option<u64>,
    #[serde(rename = "blobId")]
    blob_id: Option<String>,
    lfs: Option<Lfs>,
}

#[derive(Deserialize)]
struct Lfs {
    sha256: Option<String>,
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The client: one connection pool, its base, and no redirect followed.
#[derive(Clone, Debug)]
pub struct Client {
    http: reqwest::Client,
    base: Url,
}

impl Client {
    /// The client of `https://huggingface.co`.
    pub fn new() -> anyhow::Result<Client> {
        Client::with_base(ENDPOINT)
    }

    /// **The test seam**: a client of `base`, an origin with no path. Only
    /// the server's start names `ENDPOINT`, and a test names its fake.
    pub fn with_base(base: &str) -> anyhow::Result<Client> {
        let base = Url::parse(base)?;
        anyhow::ensure!(
            matches!(base.scheme(), "https" | "http")
                && base.host_str().is_some()
                && base.path() == "/"
                && base.query().is_none()
                && base.fragment().is_none()
                && base.username().is_empty()
                && base.password().is_none(),
            "a Hugging Face base is an origin and nothing more"
        );
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_BOUND)
            .user_agent(concat!("weaver-web/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Client { http, base })
    }

    /// The base's origin, for a link the page shows.
    pub fn base(&self) -> &Url {
        &self.base
    }

    /// `base` joined with `segments`, each escaped as one segment.
    fn url(&self, segments: &[&str]) -> Url {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .expect("an origin has a path")
            .pop_if_empty()
            .extend(segments);
        url
    }

    /// **The one place a token is put on a request**: on a request to the
    /// base's own origin, and on no other.
    pub(crate) fn authorized(
        &self,
        request: reqwest::RequestBuilder,
        to: &Url,
        token: Option<&Token>,
    ) -> reqwest::RequestBuilder {
        match token {
            Some(token) if to.origin() == self.base.origin() => request.bearer_auth(token.expose()),
            _ => request,
        }
    }

    /// A GET of an API path, its whole body read under the bound.
    async fn api_get(
        &self,
        url: Url,
        token: Option<&Token>,
    ) -> anyhow::Result<(StatusCode, reqwest::header::HeaderMap, Vec<u8>)> {
        let request = self.authorized(self.http.get(url.clone()), &url, token);
        tokio::time::timeout(API_BOUND, async {
            let mut response = request.send().await.map_err(scrubbed)?;
            let (status, headers) = (response.status(), response.headers().clone());
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(scrubbed)? {
                anyhow::ensure!(
                    body.len() + chunk.len() <= API_BODY_LIMIT,
                    "Hugging Face's answer ran past {API_BODY_LIMIT} bytes"
                );
                body.extend_from_slice(&chunk);
            }
            Ok((status, headers, body))
        })
        .await
        .map_err(|_| anyhow::anyhow!("Hugging Face did not answer within {API_BOUND:?}"))?
    }

    /// **The account a token belongs to**, from `GET /api/whoami-v2`.
    pub async fn whoami(&self, token: &Token) -> anyhow::Result<Whoami> {
        let (status, _, body) = self
            .api_get(self.url(&["api", "whoami-v2"]), Some(token))
            .await?;
        if status == StatusCode::UNAUTHORIZED {
            return Ok(Whoami::Refused);
        }
        anyhow::ensure!(
            status.is_success(),
            "Hugging Face answered {status} to whoami"
        );
        let body: WhoamiBody = serde_json::from_slice(&body)
            .map_err(|_| anyhow::anyhow!("Hugging Face's whoami answer is not its shape"))?;
        let name = body
            .name
            .filter(|name| is_repo_half(name))
            .ok_or_else(|| anyhow::anyhow!("Hugging Face's whoami answer names no account"))?;
        let role = body
            .auth
            .and_then(|auth| auth.access_token)
            .and_then(|token| token.role)
            .and_then(|role| Role::of_hugging_face(&role))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Hugging Face's whoami answer names no token role the console knows"
                )
            })?;
        Ok(Whoami::Account(Account { name, role }))
    }

    /// **A repository at a revision**, from
    /// `GET /api/models/<repo>/revision/<revision>?blobs=true`: the commit
    /// the revision names, whether it is gated or private, and every file
    /// with its size, git blob id and, for a file kept in LFS, its SHA-256.
    pub async fn model_at(
        &self,
        token: Option<&Token>,
        repo: &RepoId,
        revision: &str,
    ) -> anyhow::Result<ModelAt> {
        anyhow::ensure!(is_revision(revision), "a revision is not of its shape");
        let mut url = self.url(&[
            "api",
            "models",
            repo.owner(),
            repo.name(),
            "revision",
            revision,
        ]);
        url.set_query(Some("blobs=true"));
        let (status, headers, body) = self.api_get(url, token).await?;
        match status {
            StatusCode::NOT_FOUND => return Ok(ModelAt::Missing),
            StatusCode::UNAUTHORIZED if error_code(&headers) == Some("RepoNotFound") => {
                return Ok(ModelAt::Missing);
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => return Ok(ModelAt::Refused),
            status if !status.is_success() => {
                anyhow::bail!("Hugging Face answered {status} to a repository's listing")
            }
            _ => {}
        }
        let body: ListingBody = serde_json::from_slice(&body)
            .map_err(|_| anyhow::anyhow!("Hugging Face's listing is not its shape"))?;
        let commit = body
            .sha
            .filter(|sha| is_commit(sha))
            .ok_or_else(|| anyhow::anyhow!("Hugging Face's listing names no commit"))?;
        let gated = match &body.gated {
            serde_json::Value::Bool(false) | serde_json::Value::Null => Gated::No,
            serde_json::Value::String(s) if s == "auto" => Gated::Auto,
            serde_json::Value::String(s) if s == "manual" => Gated::Manual,
            _ => anyhow::bail!("Hugging Face's listing names a gate the console does not know"),
        };
        let mut files = Vec::new();
        for sibling in body.siblings.unwrap_or_default() {
            let (Some(path), Some(size), Some(blob_id)) =
                (sibling.rfilename, sibling.size, sibling.blob_id)
            else {
                anyhow::bail!("Hugging Face's listing holds a file without its path, size or id");
            };
            anyhow::ensure!(
                is_commit(&blob_id),
                "Hugging Face's listing holds a git blob id not of its shape"
            );
            let sha256 = match sibling.lfs {
                Some(lfs) => Some(lfs.sha256.filter(|s| is_sha256(s)).ok_or_else(|| {
                    anyhow::anyhow!("Hugging Face's listing holds an LFS file without its SHA-256")
                })?),
                None => None,
            };
            files.push(RepoFile {
                path,
                size,
                blob_id,
                sha256,
            });
        }
        Ok(ModelAt::Model(Model {
            commit,
            gated,
            private: body.private,
            files,
        }))
    }

    /// **Whether the account may fetch a repository's files**, read from a
    /// `HEAD` of one file through the commit, its redirect not followed:
    /// any 2xx or 3xx is granted, a 401 or 403 carrying
    /// `x-error-code: GatedRepo` is gated, with the repository's page to
    /// accept its terms on, a 404 is missing, and any other 401 or 403 a
    /// refused token.
    pub async fn access(
        &self,
        token: Option<&Token>,
        repo: &RepoId,
        commit: &str,
        file: &str,
    ) -> anyhow::Result<Access> {
        anyhow::ensure!(
            is_commit(commit),
            "a fetch names a commit and never a branch"
        );
        anyhow::ensure!(is_repo_path(file), "a file's path is not of its shape");
        let mut segments = vec![repo.owner(), repo.name(), "resolve", commit];
        segments.extend(file.split('/'));
        let url = self.url(&segments);
        let request = self.authorized(self.http.head(url.clone()), &url, token);
        let response = tokio::time::timeout(API_BOUND, request.send())
            .await
            .map_err(|_| anyhow::anyhow!("Hugging Face did not answer within {API_BOUND:?}"))?
            .map_err(scrubbed)?;
        let status = response.status();
        Ok(match status {
            s if s.is_success() || s.is_redirection() => Access::Granted,
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
                if error_code(response.headers()) == Some("GatedRepo") =>
            {
                Access::Gated {
                    accept: self.url(&[repo.owner(), repo.name()]).to_string(),
                }
            }
            StatusCode::NOT_FOUND => Access::Missing,
            StatusCode::UNAUTHORIZED if error_code(response.headers()) == Some("RepoNotFound") => {
                Access::Missing
            }
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Access::Refused,
            status => anyhow::bail!("Hugging Face answered {status} to a file's fetch"),
        })
    }
}

fn error_code(headers: &reqwest::header::HeaderMap) -> Option<&str> {
    headers.get("x-error-code").and_then(|v| v.to_str().ok())
}

/// **An error of the client without its request**: reqwest's error names the
/// URL, which names no token here, but the text is cut to the kind of
/// failure so no error a page or log carries holds more than it needs.
fn scrubbed(e: reqwest::Error) -> anyhow::Error {
    let e = e.without_url();
    let kind = if e.is_timeout() {
        "timed out"
    } else if e.is_connect() {
        "could not connect"
    } else if e.is_body() || e.is_decode() {
        "broke off its answer"
    } else {
        "failed"
    };
    anyhow::anyhow!("the request to Hugging Face {kind}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **`Authorization` goes to the base's origin alone**: a request to the
    /// base carries the bearer, and one to another host, another port or
    /// another scheme carries none.
    #[test]
    fn the_token_goes_to_the_bases_origin_alone() {
        let client = Client::with_base("https://huggingface.co").unwrap();
        let token = Token::parse("test-token-0001").unwrap();
        let carried = |to: &str| {
            let to = Url::parse(to).unwrap();
            client
                .authorized(client.http.get(to.clone()), &to, Some(&token))
                .build()
                .unwrap()
                .headers()
                .contains_key(reqwest::header::AUTHORIZATION)
        };
        assert!(carried("https://huggingface.co/api/whoami-v2"));
        assert!(!carried("https://cdn-lfs.hf.co/repos/x"));
        assert!(!carried("https://huggingface.co.example/api/whoami-v2"));
        assert!(!carried("http://huggingface.co/api/whoami-v2"));
        assert!(!carried("https://huggingface.co:8443/api/whoami-v2"));
    }

    #[test]
    fn a_base_is_an_origin_and_nothing_more() {
        assert!(Client::with_base("https://huggingface.co/").is_ok());
        assert!(Client::with_base("http://127.0.0.1:9/").is_ok());
        for base in [
            "https://huggingface.co/mirror",
            "https://huggingface.co/?x=1",
            "https://user@huggingface.co/",
            "ftp://huggingface.co/",
            "huggingface.co",
        ] {
            assert!(Client::with_base(base).is_err(), "{base}");
        }
    }

    #[test]
    fn names_are_checked_to_their_shapes() {
        assert!(RepoId::parse("ibm-granite/granite-4.2-30b-GGUF").is_some());
        assert_eq!(
            RepoId::parse("org/model_1").unwrap().to_string(),
            "org/model_1"
        );
        for repo in [
            "org",
            "org/",
            "/model",
            "org/model/x",
            "org/..",
            "org/.model",
            "org/model-",
            "org/a--b",
            "org/a..b",
            "or g/model",
            "org/mod%2Fel",
        ] {
            assert!(RepoId::parse(repo).is_none(), "{repo}");
        }
        assert!(is_commit("27b350a791e81d9a4d1ddca1c49282e9ec533768"));
        assert!(!is_commit("27B350A791E81D9A4D1DDCA1C49282E9EC533768"));
        assert!(!is_commit("main"));
        assert!(is_revision("main"));
        assert!(is_revision("refs/pr/1"));
        for revision in ["", "a//b", "../main", "main/..", "/main", "ma in", "a?b"] {
            assert!(!is_revision(revision), "{revision}");
        }
        assert!(is_repo_path("model.gguf"));
        assert!(is_repo_path("BF16/part-00001-of-00002.gguf"));
        for path in [
            "",
            "/etc/passwd",
            "a/../b",
            "..",
            "./a",
            "a//b",
            "a/",
            "a\\b",
            "a\0b",
            "a\nb",
        ] {
            assert!(!is_repo_path(path), "{path:?}");
        }
        assert!(Token::parse("test-token-0001").is_some());
        for token in ["", "with space", "tab\there", &"x".repeat(257)] {
            assert!(Token::parse(token).is_none(), "{token:?}");
        }
        assert_eq!(
            format!("{:?}", Token::parse("test-token-0001").unwrap()),
            "Token(..)"
        );
    }
}
