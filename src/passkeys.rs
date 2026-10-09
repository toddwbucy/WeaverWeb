//! **The passkey ceremonies' server half** (`docs/project/design-2026-10-07-iam.md`
//! sections 2 and 6): the relying party built from the server's config, and
//! the ceremony table that holds a ceremony's state between its start and its
//! finish. Carried by the `passkeys` feature, which only the server binary
//! requires, so the connectors never link the library's OpenSSL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;
use webauthn_rs::prelude::{
    AuthenticationResult, Credential, Passkey, PasskeyAuthentication, PasskeyRegistration, Url,
    Webauthn, WebauthnBuilder,
};

use crate::config::ServerConfig;
use crate::store::Store;

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
    /// A name-first sign-in: the person the name found, and **the passkeys
    /// the challenge was for, by credential ID to their own `pk-` identity**,
    /// recorded at the start. The finish uses this mapping alone, so a
    /// credential re-enrolled meanwhile, a new row under a new identity the
    /// ceremony never challenged, is never counted or opened on.
    SignIn {
        state: PasskeyAuthentication,
        person_id: String,
        challenged: HashMap<String, String>,
    },
    /// **The fresh assertion before adding a passkey** (design section 7):
    /// an authentication ceremony for the session's person, recording the
    /// challenged passkeys as sign-in does, bound to the session that asked.
    AddAssertion {
        state: PasskeyAuthentication,
        person_id: String,
        session_id: i64,
        challenged: HashMap<String, String>,
    },
    /// **The one-time add grant** a verified fresh assertion yields: bound
    /// to that session and that person, expiring with the ceremony window
    /// and counting toward the cap like any ceremony, and consumed by the
    /// registration that starts with it.
    AddGrant { person_id: String, session_id: i64 },
    /// **The registration the grant started**, bound to the same session.
    AddRegistration {
        state: PasskeyRegistration,
        person_id: String,
        session_id: i64,
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

/// **An assertion the library verified**, as the counter rule reads it: the
/// counter it returned, and how it merges into a stored passkey (the
/// library's `update_credential`: the counter, the backup state and the
/// backup eligibility). A trait so a test can drive counters a software
/// authenticator never returns, zero above all.
pub trait Assertion {
    fn counter(&self) -> u32;
    fn merge_into(&self, passkey: &mut Passkey);
}

impl Assertion for AuthenticationResult {
    fn counter(&self) -> u32 {
        AuthenticationResult::counter(self)
    }

    fn merge_into(&self, passkey: &mut Passkey) {
        passkey.update_credential(self);
    }
}

/// **What the counter rule came to.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Counted {
    /// The assertion's counter rose or is zero, and the stored passkey is
    /// updated: the passkey's identity, for the session it opens.
    Admitted { passkey_id: String },
    /// A nonzero counter not greater than the freshly read stored one: a
    /// possible cloned credential. Nothing was written.
    PossibleClone { passkey_id: String },
    /// The challenged passkey no longer stands for this person.
    Gone,
}

/// **The counter rule** (design section 6), for every assertion the server
/// verifies: in a transaction of its own, the stored passkey is read under
/// its row's lock (`FOR UPDATE`), so this holds the freshest copy and no
/// other assertion's update interleaves; where the returned counter is
/// nonzero and not greater than that copy's, the assertion is refused as a
/// possible clone and nothing is written; otherwise the assertion is merged
/// into that fresh copy and written back, **always where the returned
/// counter is zero**, which has nothing to race. It commits before any
/// action the assertion authorizes begins, so an action that then fails
/// leaves the passkey as updated: the authenticator did advance.
pub async fn count(
    store: &Store,
    passkey_id: &str,
    person_id: &str,
    assertion: &impl Assertion,
) -> anyhow::Result<Counted> {
    let passkey_id = passkey_id.to_owned();
    let mut tx = store.pool.begin().await?;
    let stored: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT credential FROM passkey WHERE passkey_id = $1 AND person_id = $2 FOR UPDATE",
    )
    .bind(&passkey_id)
    .bind(person_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(stored) = stored else {
        return Ok(Counted::Gone);
    };
    #[cfg(test)]
    hold_after_the_read(&passkey_id).await;
    let mut passkey: Passkey = serde_json::from_value(stored)?;
    let stored_counter = Credential::from(passkey.clone()).counter;
    let returned = assertion.counter();
    if returned != 0 && returned <= stored_counter {
        return Ok(Counted::PossibleClone { passkey_id });
    }
    assertion.merge_into(&mut passkey);
    sqlx::query("UPDATE passkey SET credential = $2, last_used_at = now() WHERE passkey_id = $1")
        .bind(&passkey_id)
        .bind(serde_json::to_value(&passkey)?)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Counted::Admitted { passkey_id })
}

/// A hold after the counter rule's locked read, keyed by the passkey's
/// identity so no other test's assertion takes it, and one of a list, so
/// tests running at once each keep their own: the rule signals `read` and waits
/// on `release`, which lets a test start a second assertion meanwhile.
#[cfg(test)]
pub(crate) type CountHold = (String, Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>);

#[cfg(test)]
pub(crate) static COUNT_HOLD: Mutex<Vec<CountHold>> = Mutex::new(Vec::new());

#[cfg(test)]
async fn hold_after_the_read(passkey_id: &str) {
    let hold = {
        let mut holds = COUNT_HOLD.lock().unwrap();
        holds
            .iter()
            .position(|(key, ..)| key == passkey_id)
            .map(|at| holds.remove(at))
    };
    if let Some((_, read, release)) = hold {
        read.notify_one();
        release.notified().await;
    }
}
