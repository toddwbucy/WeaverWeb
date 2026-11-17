//! conforms: web-a-pairing-naming-a-live-box-and-name-is-refused-and-retires-nothing
//! conforms: web-a-lost-word-writes-no-row
//! conforms: web-a-rows-box-changes-only-through-the-replace-box-write
//!
//! **The exchange** (Spec 8.2): once a code matched one waiting connection,
//! the console takes six steps under the authority lock, held from the first
//! to the last, the box's word among them:
//! 1. `setup_closed` or `box_and_name_live`, by a register write of the
//!    exchange's own (`Store::pairing_check_on`), which retires nothing;
//! 2. each certificate request verified, ECDSA P-256 with SHA-256 alone
//!    (`link::request`), else `request_refused`;
//! 3. each public half signed as its plane, as `mint_client` signs;
//! 4. the answer sent;
//! 5. the box's word awaited within `HANDSHAKE_SECS`, and **a lost word
//!    writes nothing**;
//! 6. the row written, or for `replace box` the row's box set beside the
//!    box it held, with the setup's link, in one identity transaction that
//!    re-checks the session and the admin grant and commits with the
//!    audit's outcome (`Store::pairing_write_on`), and then the last word.
//!
//! The caller takes the authority lock and checks the authority it loaded
//! before step 1, as every register write through the server does. **The box
//! hears a heartbeat at the link's cadence until step 4 and while step 6
//! runs** (`pairing::Kept`), so neither the lock's wait nor the store's leaves
//! it silent.

use crate::link::authority::{Authority, SignedClient};
use crate::link::frames::{Line, PairFromBox, PairToBox, Plane, pair_reason};
use crate::link::listener::HANDSHAKE_SECS;
use crate::link::pairing::{Kept, Taken, close_with};
use crate::link::register::{AdminWrite, AuthorityLock, Pairing, PairingAct, PairingRefusal};
use crate::link::request;
use crate::store::Store;
use crate::store::key::AgentId;
use std::time::Duration;

/// What the exchange needs beyond the connection.
pub struct Exchange<'a> {
    pub authority: &'a Authority,
    pub setup_id: &'a str,
    /// The setup's name, the agent's.
    pub name: &'a str,
    pub act: &'a PairingAct,
    pub admin: AdminWrite<'a>,
}

/// **How an exchange ended.**
#[derive(Debug)]
pub enum Outcome {
    /// The row stands with the answer's two certificates live, and the
    /// box was sent the last word. A replacement's old fingerprints are
    /// revoked, their connections closed by the store's notification.
    Paired { agent_id: AgentId },
    /// Refused at step 1 or 6, the box told why.
    Refused(PairingRefusal),
    /// A request refused at step 2, the box told `request_refused`.
    Request(request::Refused),
    /// No word from the box within the bound, or a connection that ended
    /// before it: nothing was written.
    LostWord,
    /// The box went before its answer, a heartbeat not sent within the
    /// write bound: nothing was written.
    BoxGone,
    /// The authority lock was held past the box's life by another write, so
    /// the box was refused `busy`: nothing was written.
    Busy,
    /// The admin's session or grant went before step 6: nothing was
    /// written, and the box hears no last word.
    AuthorityGone,
    /// A step failed, the store's or the authority's fault, or step 6's
    /// commit answered unknown: the box hears no last word, and the pairing
    /// page says what the write made.
    Unknown(anyhow::Error),
}

/// **An exchange's end**: how it ended, and the fingerprints of the two
/// certificates the box was answered, once step 4 sent them, which the box
/// prints on `unknown` and the console logs beside any end but `paired`.
#[derive(Debug)]
pub struct Ended {
    pub outcome: Outcome,
    pub answered: Option<[String; 2]>,
}

impl From<Outcome> for Ended {
    fn from(outcome: Outcome) -> Self {
        Ended {
            outcome,
            answered: None,
        }
    }
}

/// **Run the exchange** over a taken connection, under `lock`.
pub async fn run(lock: &mut AuthorityLock, x: Exchange<'_>, taken: Taken) -> Ended {
    let Taken {
        r#box,
        gate_request,
        admin_request,
        connection: kept,
        ..
    } = taken;
    let (cadence, write_bound) = (kept.cadence(), kept.write_bound());
    let pairing = Pairing {
        setup_id: x.setup_id,
        act: x.act,
        r#box: &r#box,
        name: x.name,
    };
    let refuse = |reason: &str| PairToBox::Refused {
        reason: reason.to_owned(),
    };

    // 1. The setup in its pairing step, and no live row of the claimed
    // box and the setup's name.
    match Store::pairing_check_on(lock.connection(), &pairing).await {
        Ok(Ok(())) => {}
        Ok(Err(refusal)) => {
            if let Some(mut connection) = kept.resume().await {
                close_with(&mut connection, &refuse(refusal.reason()), write_bound).await;
            }
            return Outcome::Refused(refusal).into();
        }
        Err(error) => return Outcome::Unknown(error).into(),
    }

    // 2. Each request's own signature, ECDSA P-256 with SHA-256 alone.
    let verified = request::verify(&gate_request)
        .and_then(|gate| request::verify(&admin_request).map(|admin| (gate, admin)));
    let (gate_key, admin_key) = match verified {
        Ok(keys) => keys,
        Err(why) => {
            if let Some(mut connection) = kept.resume().await {
                let refused = refuse(pair_reason::REQUEST_REFUSED);
                close_with(&mut connection, &refused, write_bound).await;
            }
            return Outcome::Request(why).into();
        }
    };

    // 3. Each half signed as its plane, the console's own members.
    let signed = x
        .authority
        .sign_client(x.name, Plane::Gate, &gate_key)
        .and_then(|gate| {
            x.authority
                .sign_client(x.name, Plane::Admin, &admin_key)
                .map(|admin| (gate, admin))
        });
    let (gate, admin): (SignedClient, SignedClient) = match signed {
        Ok(pair) => pair,
        Err(error) => return Outcome::Unknown(error).into(),
    };
    let answered = [gate.fingerprint.clone(), admin.fingerprint.clone()];
    let ended = |outcome| Ended {
        outcome,
        answered: Some(answered.clone()),
    };

    // 4. The answer, the connection taken back from its heartbeats.
    let Some(mut connection) = kept.resume().await else {
        return Outcome::BoxGone.into();
    };
    let answer = PairToBox::Answer {
        agent: x.name.to_owned(),
        agent_id: x.act.agent_id().as_str().to_owned(),
        r#box: r#box.clone(),
        server_name: x.authority.server_name().to_owned(),
        authority: x.authority.certificate_pem().to_owned(),
        gate: gate.certificate_pem,
        admin: admin.certificate_pem,
    };
    if !crate::link::pairing::send(&mut connection, &answer, write_bound).await {
        return ended(Outcome::LostWord);
    }

    // 5. The box's word, bounded, the lock still held: a lost word writes
    // nothing.
    let word = tokio::time::timeout(Duration::from_secs(HANDSHAKE_SECS), connection.next()).await;
    let wrote = match word {
        Ok(Line::Frame(line)) => matches!(
            serde_json::from_str::<PairFromBox>(&line),
            Ok(PairFromBox::Written)
        ),
        _ => false,
    };
    if !wrote {
        return ended(Outcome::LostWord);
    }

    // 6. The row and the setup's link, in one identity transaction, the box
    // hearing heartbeats while it runs, then the last word. A box gone
    // meanwhile hears none, and prints unknown.
    let kept = Kept::new(connection, cadence, write_bound);
    let written = Store::pairing_write_on(
        lock.connection(),
        x.admin,
        &pairing,
        &answered[0],
        &answered[1],
        &x.authority.fingerprint(),
    )
    .await;
    let mut connection = kept.resume().await;
    ended(match written {
        Ok(Ok(_revoked)) => {
            if let Some(connection) = connection.as_mut() {
                close_with(connection, &PairToBox::Paired, write_bound).await;
            }
            Outcome::Paired {
                agent_id: x.act.agent_id().clone(),
            }
        }
        Ok(Err(refusal)) => {
            if let Some(connection) = connection.as_mut() {
                close_with(connection, &refuse(refusal.reason()), write_bound).await;
            }
            Outcome::Refused(refusal)
        }
        Err(error) if error.is::<crate::link::register::AuthorityGone>() => Outcome::AuthorityGone,
        Err(error) => Outcome::Unknown(error),
    })
}
