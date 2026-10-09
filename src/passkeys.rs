//! **The passkey ceremonies' server half** (`docs/project/design-2026-10-07-iam.md`
//! sections 2 and 6): the relying party built from the server's config, and
//! the ceremony table that holds a ceremony's state between its start and its
//! finish. Carried by the `passkeys` feature, which only the server binary
//! requires, so the connectors never link the library's OpenSSL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use webauthn_rs::prelude::{PasskeyRegistration, Url, Webauthn, WebauthnBuilder};

use crate::config::ServerConfig;

/// **At most this many ceremonies in flight** (design section 6): a ceremony
/// starts before anyone is authenticated, so without a cap a client could
/// grow the table until the process fails. A handful of people with a
/// ceremony or two each, and a wide margin; a ceremony's state is under a
/// kilobyte, so the table stays within tens of kilobytes.
pub const CEREMONIES_IN_FLIGHT: usize = 64;

/// **A ceremony expires this long after it starts** (design section 6), the
/// same five minutes the library asks the browser to wait.
pub const CEREMONY_LIFETIME: Duration = Duration::from_secs(5 * 60);

/// **A ceremony's state between its start and its finish**, with what it is
/// bound to.
pub enum Ceremony {
    /// A token's redemption registering its person's first passkey: the
    /// person, and the token's digest, checked again at the finish.
    Enrollment {
        state: PasskeyRegistration,
        person_id: String,
        token_digest: String,
    },
}

/// The table is full: a ceremony is refused until one finishes or expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Full;

/// **The ceremony table**, in the server's memory alone: keyed by a ceremony
/// identity, a bearer drawn as every bearer the server issues is (32 bytes
/// from the operating system's random source); at most
/// [`CEREMONIES_IN_FLIGHT`]; each expiring after [`CEREMONY_LIFETIME`] and
/// used once. A restart drops what is in flight, and the person begins again.
#[derive(Default)]
pub struct Ceremonies {
    held: Mutex<HashMap<String, (Ceremony, Instant)>>,
}

impl Ceremonies {
    /// **A ceremony started**: its identity, or [`Full`] where the cap is
    /// reached by ceremonies that have not expired.
    pub fn start(&self, ceremony: Ceremony) -> Result<String, Full> {
        let now = Instant::now();
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        held.retain(|_, (_, started)| now.duration_since(*started) < CEREMONY_LIFETIME);
        if held.len() >= CEREMONIES_IN_FLIGHT {
            return Err(Full);
        }
        let identity = crate::store::identity::hex(&crate::store::identity::bearer());
        held.insert(identity.clone(), (ceremony, now));
        Ok(identity)
    }

    /// How many ceremonies the table holds, expired or not.
    #[cfg(test)]
    pub fn held(&self) -> usize {
        self.held.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// **A ceremony taken for its finish, once**: removed whatever it
    /// answers, and `None` where it is unknown, already taken, or expired.
    pub fn take(&self, identity: &str) -> Option<Ceremony> {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        let (ceremony, started) = held.remove(identity)?;
        (started.elapsed() < CEREMONY_LIFETIME).then_some(ceremony)
    }
}

/// **What the passkey surfaces take as their own argument**: the relying
/// party and the ceremony table.
#[derive(Clone)]
pub struct Passkeys {
    pub webauthn: Arc<Webauthn>,
    pub ceremonies: Arc<Ceremonies>,
}

impl Passkeys {
    /// **The relying party from the server's config**, where `rp_id` and
    /// `origin` are both configured, `listen::relying_party` having checked
    /// them at start; `None` where neither is, and no passkey is on.
    pub fn from_config(cfg: &ServerConfig) -> anyhow::Result<Option<Self>> {
        let (Some(rp_id), Some(origin)) = (&cfg.rp_id, &cfg.origin) else {
            return Ok(None);
        };
        let webauthn = WebauthnBuilder::new(rp_id, &Url::parse(origin)?)?
            .rp_name("weaver-web")
            .timeout(CEREMONY_LIFETIME)
            .build()?;
        Ok(Some(Self {
            webauthn: Arc::new(webauthn),
            ceremonies: Arc::new(Ceremonies::default()),
        }))
    }
}
