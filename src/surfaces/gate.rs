//! conforms: web-session-carries-a-claim-and-never-a-proof
//!
//! The seat a surface's gate sits in, per `weaver-web-Spec` section 2.8 and
//! the charter's section 6.
//!
//! **A surface reads the store and nothing else, and a surface that holds a
//! seam takes it as its own argument.** That is `surfaces/mod.rs`'s rule,
//! and a gate is a seam: implementing the first half as a router carrying
//! the store alone would leave no seat for the second, which is how a
//! surface comes to serve every run to anyone who reaches the listener.
//!
//! **This is a claim and never a proof**, per section 2.8. A session holds
//! the name its opener typed and the role the deployer configured, and
//! nothing here tests that anyone is anyone. The identity act of the
//! charter's section 6 is what changes that, per issue #336, and until it
//! lands **this gate is the shape standing rather than access control**:
//! what is missing is the proof, not the structure.

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};

use crate::store::Store;

/// Who is asking, as the session says it. **The name is claimed and the
/// role is configured**, and neither is proved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub name: String,
    pub role: String,
}

impl Claim {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

/// The bearer a browser sent, if it sent one. The instrument uses
/// `weaver_session`; the retired conversation routes used `ww_session`.
fn bearer(headers: &HeaderMap) -> Option<String> {
    let cookies = headers.get(axum::http::header::COOKIE)?.to_str().ok()?;
    cookies.split(';').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name.trim() == "weaver_session").then(|| value.trim().to_string())
    })
}

/// **The bearer is hashed before it is looked up**, per section 2.8, so a
/// read of the session table is not a set of live bearers and a leaked
/// dump is not a set of usable cookies.
pub fn digest(bearer: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bearer.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// The claim a request carries, or `None` where it carries none.
///
/// **An absent session is absent and never a default**, per Spec section 6
/// read at the gate: a request with no bearer is one this crate cannot name
/// rather than one it names as the operator.
pub async fn claim(store: &Store, headers: &HeaderMap) -> anyhow::Result<Option<Claim>> {
    let Some(bearer) = bearer(headers) else {
        return Ok(None);
    };
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT claimed_name, role FROM session \
         WHERE bearer_digest = $1 AND closed_at IS NULL",
    )
    .bind(digest(&bearer))
    .fetch_optional(&store.pool)
    .await?;
    Ok(row.map(|(name, role)| Claim { name, role }))
}
