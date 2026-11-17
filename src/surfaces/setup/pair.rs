//! conforms: web-the-pairing-page-reads-an-entry-in-crockfords-form-and-a-malformed-entry-is-no-miss
//! conforms: web-on-unknown-the-pairing-page-shows-what-the-write-made
//!
//! **The pairing page's waiting box and the code's entry** (Spec 8.2, 6.2).
//! The pane lists the boxes waiting, each by the name it claims, its peer
//! address and its wait, marked as the box's claim, beside the console's
//! fingerprint, the fingerprint's confirmation and the code's field, and
//! the register's live rows holding the setup's name, each by its box and
//! its live credentials' fingerprints as the register keeps them. **Those
//! rows are what a pairing's write made**, whether its answer was lost or
//! not, so the page never shows the answer's fingerprints in their place.
//!
//! **The entry takes the admin writes' order** (Spec 2.13): the setup's
//! identity and the code read to their shapes, an entry that is not ten
//! characters of the code's alphabet answered as malformed with no record
//! and no miss, and the confirmation required; then the admin gate with
//! its one refusal record; the setup and a replaced row resolved, unknown
//! answered as not found with no record; then the setup in its pairing
//! step and its field open, each refusal one record; then the code
//! compared with each waiting connection's, **a miss one refusal record
//! naming the setup and never the code**, counted on the setup row; and on
//! a match the first record of `pair` or `replace box`, whose target is the
//! agent row, and the exchange in a task a dropped request cannot cut.

use std::sync::Arc;

use chrono::Utc;

use crate::link::authority::Authority;
use crate::link::listener::Listener;
use crate::link::pair_code::grouped;
use crate::link::register::Agent;
use crate::store::Store;
use crate::store::setup::Setup;

use super::Seams;

/// **The enrollment's seams** (Spec 8.2): the link's listener, whose boxes
/// wait and whose waiting connections an entry is compared with, and the
/// authority the server loaded, whose fingerprint the page shows and which
/// signs at an exchange.
#[derive(Clone)]
pub struct Enrollment {
    pub listener: Listener,
    pub authority: Arc<Authority>,
}

/// A waiting box as the page lists it, marked as the box's claim.
pub(super) struct WaitingView {
    pub claim: String,
    pub peer: String,
    pub waited: String,
    /// The box dialed from a loopback address, the console's own machine,
    /// and the checkbox says virtual machine: the page warns and never
    /// refuses (Spec 8.2).
    pub loopback: bool,
}

/// A live register row holding the setup's name, as the page draws it.
pub(super) struct RowView {
    pub agent_id: String,
    pub r#box: String,
    pub gate: String,
    pub admin: String,
    pub label: &'static str,
}

/// The answer to a code's entry, drawn above the page.
pub(super) struct Entered {
    pub status: &'static str,
    pub words: String,
    /// The box was refused `box_and_name_live`, so the register's retire
    /// is offered (Spec 8.2, step 1).
    pub retire: bool,
}

/// **The waiting box's pane** as drawn.
pub(super) struct PairPane {
    /// The enrollment is served: false draws the pane held.
    pub live: bool,
    pub status: &'static str,
    /// The console's fingerprint, the authority's, in sixteen groups of
    /// four.
    pub fingerprint: String,
    pub waiting: Vec<WaitingView>,
    /// The setup is in its pairing step, so the field is drawn.
    pub in_step: bool,
    /// The row the pairing linked and which write it was.
    pub paired: Option<(String, String)>,
    /// Until when five misses close the field.
    pub closed_until: Option<String>,
    pub rows: Vec<RowView>,
    pub entered: Option<Entered>,
}

/// **A row's label** (Spec 2.12): "paired, setup running" until either
/// connector's service is first admitted, then the row's presence.
pub(super) fn label(row: &Agent) -> &'static str {
    if row.setup_running {
        "paired, setup running"
    } else if row.present() {
        "present"
    } else {
        "not present"
    }
}

fn credential(c: &crate::link::register::Credential) -> String {
    match c.state {
        crate::link::register::CredentialState::Live => grouped(&c.fingerprint),
        crate::link::register::CredentialState::Revoked => "revoked".to_owned(),
    }
}

/// The register's live rows holding `name`, as the page draws them.
pub(super) fn rows_of(rows: &[Agent]) -> Vec<RowView> {
    rows.iter()
        .map(|row| RowView {
            agent_id: row.agent_id.as_str().to_owned(),
            r#box: row.r#box.clone(),
            gate: credential(&row.gate),
            admin: credential(&row.admin),
            label: label(row),
        })
        .collect()
}

fn waited(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    format!("{} min {} s", secs / 60, secs % 60)
}

/// **The pane for `setup`**, read from the listener, the authority and the
/// store.
pub(super) async fn pane(
    store: &Store,
    seams: &Seams,
    setup: &Setup,
    entered: Option<Entered>,
) -> anyhow::Result<PairPane> {
    let rows = rows_of(&store.live_rows_named(&setup.name).await?);
    let now = Utc::now();
    let closed_until = setup
        .code_closed_until
        .filter(|_| setup.code_closed(now))
        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string());
    let Some(enrollment) = seams.enrollment.as_ref() else {
        return Ok(PairPane {
            live: false,
            status: "hold",
            fingerprint: String::new(),
            waiting: Vec::new(),
            in_step: setup.in_pairing_step(),
            paired: setup.paired.clone(),
            closed_until,
            rows,
            entered,
        });
    };
    let waiting: Vec<WaitingView> = enrollment
        .listener
        .waiting()
        .into_iter()
        .map(|w| WaitingView {
            claim: w.r#box,
            peer: w.peer.to_string(),
            waited: waited(w.waited),
            loopback: w.peer.is_loopback() && setup.virtual_machine,
        })
        .collect();
    let status = if waiting.iter().any(|w| w.loopback) {
        "fail"
    } else {
        "ok"
    };
    Ok(PairPane {
        live: cfg!(feature = "enrollment"),
        status,
        fingerprint: grouped(&enrollment.authority.fingerprint()),
        waiting,
        in_step: setup.in_pairing_step(),
        paired: setup.paired.clone(),
        closed_until,
        rows,
        entered,
    })
}

#[cfg(feature = "enrollment")]
pub(super) use entry::route;

#[cfg(feature = "enrollment")]
mod entry {
    use axum::extract::{Form, Path, State};
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{Html, IntoResponse, Response};
    use serde::Deserialize;

    use super::{Entered, pane};
    use crate::link::exchange::{self, Ended, Exchange, Outcome};
    use crate::link::frames::{PairToBox, pair_reason};
    use crate::link::pair_code::Code;
    use crate::link::pairing::close_with;
    use crate::link::register::{AdminWrite, PairingAct, PairingRefusal};
    use crate::store::Store;
    use crate::store::admin::Refusal;
    use crate::store::audit::{PersonMethod, Principal, Target};
    use crate::store::identity::is_setup_id;
    use crate::store::key::AgentId;
    use crate::store::setup::Field;
    use crate::surfaces::admin::{fault, first_of, refused, settled, writer};
    use crate::surfaces::gate::Session;

    use super::super::{MALFORMED_SETUP, Page, Seams, frame, pairing_page};

    /// What an entry posts: the code as typed, the confirmation that the
    /// fingerprints were compared, and, for a setup whose name a live row
    /// holds, the row whose box the pairing replaces.
    #[derive(Deserialize)]
    pub struct PairAsk {
        #[serde(default)]
        code: String,
        #[serde(default)]
        compared: Option<String>,
        #[serde(default)]
        replace: Option<String>,
    }

    const ACTION: &str = "pair";

    /// **What the comparison found**, under the setup row's lock.
    enum Compared {
        NotInStep,
        Closed,
        Missed { closed: bool },
        Matched(Box<crate::link::pairing::Taken>),
    }

    /// The answer to an entry that is not a code.
    const MALFORMED_CODE: &str = "the code is ten characters of 0-9 and A-Z without I, L, O and U, as the box shows it: check it and type it again";

    pub(in super::super) fn route(seams: Seams) -> axum::routing::MethodRouter<Store> {
        axum::routing::post(
            move |State(store): State<Store>,
                  headers: HeaderMap,
                  Path(setup): Path<String>,
                  Form(ask): Form<PairAsk>| {
                let seams = seams.clone();
                async move { enter(store, &seams, headers, &setup, ask).await }
            },
        )
    }

    async fn record_refusal(
        store: &Store,
        session: &Session,
        setup_id: &str,
        why: &str,
    ) -> Result<(), Response> {
        let principal = Principal::Person {
            person_id: &session.person_id,
            method: PersonMethod::Session,
        };
        store
            .audit_refusal(principal, Target::Setup(setup_id), ACTION, why)
            .await
            .map(|_| ())
            .map_err(fault)
    }

    /// **A code's entry** (Spec 8.2), in the admin writes' order.
    async fn enter(
        store: Store,
        seams: &Seams,
        headers: HeaderMap,
        setup_id: &str,
        ask: PairAsk,
    ) -> Response {
        if !is_setup_id(setup_id) {
            return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
        }
        // **Read in Crockford's form, a malformed entry no miss**: no
        // record, no count.
        let Ok(code) = Code::read(&ask.code) else {
            return (StatusCode::BAD_REQUEST, MALFORMED_CODE).into_response();
        };
        // **The comparison is never skipped** (Spec 8.2): the code alone
        // does not stop a party between the box and the console.
        if ask.compared.as_deref() != Some("on") {
            return (
                StatusCode::BAD_REQUEST,
                "compare the fingerprint the box shows with the console's, group for group, and confirm it before the code",
            )
                .into_response();
        }
        let replace = match ask.replace.as_deref() {
            None | Some("") => None,
            Some(spec) => match spec.parse::<AgentId>() {
                Ok(id) => Some(id),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        "an agent's identity is ag- and sixteen lowercase hex",
                    )
                        .into_response();
                }
            },
        };
        let session = match writer(
            &store,
            &seams.policy,
            &headers,
            Target::Setup(setup_id),
            ACTION,
        )
        .await
        {
            Ok(session) => session,
            Err(answer) => return answer,
        };
        // **A console that serves no pairing says so past the admin gate**,
        // as every system refusal does once the person's grant stands, so
        // the gate is the first thing a person without it meets.
        let Some(enrollment) = seams.enrollment.clone() else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "this console serves no pairing",
            )
                .into_response();
        };
        let setup = match store.setup(setup_id).await {
            Ok(Some(setup)) => setup,
            Ok(None) => return refused(&Refusal::NoSuchSetup),
            Err(e) => return fault(e),
        };
        if let Some(id) = &replace {
            match store.agent(id).await {
                Ok(Some(row))
                    if row.name == setup.name
                        && (row.gate.state == crate::link::register::CredentialState::Live
                            || row.admin.state == crate::link::register::CredentialState::Live) => {
                }
                Ok(_) => return refused(&Refusal::NoSuchAgent),
                Err(e) => return fault(e),
            }
        }
        // **One exclusion per setup spans the field's check, the comparison
        // and the miss** (Spec 8.2): the setup row's lock, taken in a task a
        // dropped request cannot cut, so entries for one setup compare one
        // at a time against the field as the last left it, none compares
        // once five misses have closed it, and a miss is counted and
        // recorded whatever the request does.
        let compared = {
            let store = store.clone();
            let listener = enrollment.listener.clone();
            let setup_id = setup_id.to_owned();
            let person = session.person_id.clone();
            tokio::spawn(async move {
                let (field, state) = store.code_field(&setup_id).await?;
                Ok::<Compared, anyhow::Error>(match state {
                    Field::NotInStep => {
                        field.release().await?;
                        Compared::NotInStep
                    }
                    Field::Closed => {
                        field.release().await?;
                        Compared::Closed
                    }
                    Field::Open => match listener.take_pairing(&code).await {
                        Some(taken) => {
                            field.release().await?;
                            Compared::Matched(Box::new(taken))
                        }
                        // **A miss**: counted on the setup row under the
                        // lock, then one refusal record naming the setup
                        // and never the code.
                        None => {
                            let closed = field.missed().await?;
                            let principal = Principal::Person {
                                person_id: &person,
                                method: PersonMethod::Session,
                            };
                            store
                                .audit_refusal(
                                    principal,
                                    Target::Setup(&setup_id),
                                    ACTION,
                                    "the code matches no waiting box",
                                )
                                .await?;
                            Compared::Missed { closed }
                        }
                    },
                })
            })
        };
        let taken = match compared.await {
            Ok(Ok(Compared::Matched(taken))) => *taken,
            Ok(Ok(Compared::NotInStep)) => {
                if let Err(answer) =
                    record_refusal(&store, &session, setup_id, "setup_closed").await
                {
                    return answer;
                }
                return (
                    StatusCode::CONFLICT,
                    PairingRefusal::SetupClosed.to_string(),
                )
                    .into_response();
            }
            Ok(Ok(Compared::Closed)) => {
                if let Err(answer) =
                    record_refusal(&store, &session, setup_id, "the code field is closed").await
                {
                    return answer;
                }
                return (
                    StatusCode::CONFLICT,
                    "five misses closed the code field for ten minutes from the fifth",
                )
                    .into_response();
            }
            Ok(Ok(Compared::Missed { closed })) => {
                let words = if closed {
                    "the code matches no waiting box, and five misses close the code field for ten minutes"
                } else {
                    "the code matches no waiting box: check it against the box and enter it again"
                };
                return (StatusCode::CONFLICT, words).into_response();
            }
            Ok(Err(e)) => return fault(e),
            Err(e) => return fault(anyhow::Error::new(e)),
        };
        let act = match replace {
            Some(id) => PairingAct::Replace(id),
            None => match store
                .mint_key("ag")
                .await
                .and_then(|id| id.parse().map_err(anyhow::Error::msg))
            {
                Ok(id) => PairingAct::Pair(id),
                Err(e) => return fault(e),
            },
        };
        let first = match first_of(
            &store,
            &session,
            Target::Agent(act.agent_id().as_str()),
            act.action(),
        )
        .await
        {
            Ok(first) => first,
            Err(answer) => return answer,
        };
        // **The exchange in a task of its own**, so a dropped request
        // cannot cut it while a box waits on its answer.
        let task = {
            let store = store.clone();
            let cfg = seams.cfg.clone();
            let setup_id = setup_id.to_owned();
            let name = setup.name.clone();
            let person = session.person_id.clone();
            let session_id = session.session_id;
            let first = first.clone();
            let act = act.clone();
            let mut taken = taken;
            tokio::spawn(async move {
                // **The authority lock, waited for within the box's life**
                // (Spec 8.2), the box hearing heartbeats meanwhile: a box
                // gone ends the wait with nothing written, and a lock held
                // past the box's life by another write refuses it `busy`.
                let lock = tokio::select! {
                    lock = store.authority_lock_by(taken.ends) => lock?,
                    _ = taken.connection.lost() => return Ok(Ended::from(Outcome::BoxGone)),
                };
                let Some(mut lock) = lock else {
                    let write_bound = taken.connection.write_bound();
                    if let Some(mut connection) = taken.connection.resume().await {
                        let busy = PairToBox::Refused {
                            reason: pair_reason::BUSY.to_owned(),
                        };
                        close_with(&mut connection, &busy, write_bound).await;
                    }
                    return Ok(Ended::from(Outcome::Busy));
                };
                // The server signs under the authority it loaded, so a
                // rotation on disk since the start is refused, as every
                // register write through the server refuses it.
                crate::link::verbs::name_agrees(&cfg, &enrollment.authority)
                    .and_then(|_| {
                        crate::link::verbs::authority_still_stands(&cfg, &enrollment.authority)
                    })
                    .map_err(|e| e.context("restart the server after a rotation"))?;
                Ok::<Ended, anyhow::Error>(
                    exchange::run(
                        &mut lock,
                        Exchange {
                            authority: &enrollment.authority,
                            setup_id: &setup_id,
                            name: &name,
                            act: &act,
                            admin: AdminWrite {
                                session_id,
                                person: &person,
                                first: &first,
                                store: &store,
                            },
                        },
                        taken,
                    )
                    .await,
                )
            })
        };
        let ended = match task.await {
            Ok(Ok(ended)) => ended,
            Ok(Err(e)) => Ended::from(Outcome::Unknown(e)),
            Err(e) => Ended::from(Outcome::Unknown(e.into())),
        };
        let written: anyhow::Result<Result<(), Refusal>> = match ended.outcome {
            Outcome::Paired { .. } => Ok(Ok(())),
            Outcome::Unknown(ref e) if e.is::<crate::store::commit::OutcomeUnknown>() => {
                Err(anyhow::Error::new(crate::store::commit::OutcomeUnknown))
            }
            _ => Ok(Err(Refusal::Stale)),
        };
        settled(&store, &first, &written).await;
        answer(&store, seams, &session, setup_id, &setup.name, ended).await
    }

    /// **The entry's answer**: back to the page where the row stands, else
    /// the pairing page with what happened above it and, below, the rows
    /// the register holds for the name, which is what the write made.
    async fn answer(
        store: &Store,
        seams: &Seams,
        session: &Session,
        setup_id: &str,
        name: &str,
        ended: Ended,
    ) -> Response {
        let (status, entered_status, words) = match &ended.outcome {
            Outcome::Paired { .. } => {
                return (StatusCode::SEE_OTHER, [(header::LOCATION, format!("/setup/{setup_id}"))])
                    .into_response();
            }
            Outcome::Refused(refusal) => (
                StatusCode::CONFLICT,
                "fail",
                format!("refused, the box told {}: {refusal}", refusal.reason()),
            ),
            Outcome::Request(why) => (
                StatusCode::CONFLICT,
                "fail",
                format!("refused, the box told request_refused: {why}"),
            ),
            Outcome::BoxGone => (
                StatusCode::CONFLICT,
                "fail",
                "the box's connection ended before its answer, so nothing was written: pair the box again".to_owned(),
            ),
            Outcome::Busy => (
                StatusCode::SERVICE_UNAVAILABLE,
                "fail",
                "another register write held the console past the box's ten minutes, so the box was refused busy and nothing was written: pair the box again".to_owned(),
            ),
            Outcome::LostWord => (
                StatusCode::CONFLICT,
                "fail",
                format!(
                    "the box did not say it wrote the answer within {} s, so nothing was written: pair the box again",
                    crate::link::listener::HANDSHAKE_SECS
                ),
            ),
            Outcome::AuthorityGone => (
                StatusCode::FORBIDDEN,
                "fail",
                "the session or the admin grant that entered the code ended before the write, so nothing was written: pair the box again".to_owned(),
            ),
            Outcome::Unknown(e) => {
                tracing::error!("the pairing of {setup_id} ended unknown: {e:#}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "fail",
                    format!(
                        "unknown: whether the pairing's write landed cannot be said. The rows below are what the register holds for {name}: compare their fingerprints with the two the box's unknown line shows, and adopt only where both match"
                    ),
                )
            }
        };
        if let Some([gate, admin]) = &ended.answered {
            tracing::warn!(
                "the pairing of {setup_id} ended with no row confirmed: the box was answered gate {gate} and admin {admin}"
            );
        }
        let setup = match store.setup(setup_id).await {
            Ok(Some(setup)) => setup,
            Ok(None) => return refused(&Refusal::NoSuchSetup),
            Err(e) => return fault(e),
        };
        let chrome = match crate::surfaces::Chrome::of(store, &session.person_id).await {
            Ok(chrome) => chrome,
            Err(e) => return fault(e),
        };
        let entered = Entered {
            status: entered_status,
            words,
            retire: matches!(
                ended.outcome,
                Outcome::Refused(PairingRefusal::BoxAndNameLive)
            ),
        };
        let pair = match pane(store, seams, &setup, Some(entered)).await {
            Ok(pair) => pair,
            Err(e) => return fault(e),
        };
        let page = pairing_page(
            seams,
            session.name.clone(),
            chrome,
            frame(&setup, Page::Pairing),
            &setup,
            pair,
        );
        match askama::Template::render(&page) {
            Ok(html) => (status, Html(html)).into_response(),
            Err(e) => fault(e),
        }
    }
}
