//! conforms: web-an-acknowledgement-names-the-version-it-read
//! conforms: web-unload-follows-only-a-confirmed-save-point
//!
//! **Alerts on the agent page, their acknowledgement, and the operator's
//! decisions** (Spec 2.14; act 12, PR 1; the operator's rulings of
//! 2026-10-14). Everyone holding a grant on an agent sees its alerts; a
//! person holding `operator` on it acknowledges one, naming the version
//! they read, in the admin writes' order: parse, the session, the grant
//! (no grant answered as an unknown agent, no `operator` 403, each one
//! refusal record), resolve, then one transaction under the identity
//! exclusion that re-checks both and writes the acknowledgement with its
//! audit record.
//!
//! **A decision prompt's options are chosen here**, and choosing never
//! acknowledges. "Save point, then unload" is one person's choice run as two
//! of that person's asks, each admitted, held, recorded and audited, in a
//! task the request cannot cut; the `unload` is asked only on the save
//! point's hash-confirmed answer, and any other answer ends the sequence
//! with no `unload` and an alert of its own. "Force-unload" is one ask.

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};

use askama::Template;

use crate::link::frames::VerbOutcome;
use crate::link::listener::{
    Act, Asker, FORCE_UNLOAD, Listener, SAVE_POINT_THEN_UNLOAD, VerbError,
};
use crate::store::alerts::{AlertKind, Raise, acknowledge_in};
use crate::store::audit::{PersonMethod, Principal, Target};
use crate::store::identity::{is_agent_id, session_stands};
use crate::store::{AgentId, Store};
use crate::surfaces::reading::{Seams, fault, not_found, session_of};

/// An alert's identity in its shape: `al-` and sixteen hex digits.
pub fn is_alert_id(id: &str) -> bool {
    id.len() == 19
        && id.starts_with("al-")
        && id[3..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// An acknowledgement's form: the version the person read.
#[derive(serde::Deserialize)]
pub(super) struct AckForm {
    pub(super) version: String,
}

/// A decision's form: the option chosen.
#[derive(serde::Deserialize)]
pub(super) struct ChoiceForm {
    pub(super) option: String,
}

/// **Acknowledge an alert** (Spec 2.14), in the admin writes' order.
pub(super) async fn acknowledge(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    agent: String,
    alert: String,
    version: String,
) -> Response {
    if !is_agent_id(&agent) || !is_alert_id(&alert) {
        return (
            StatusCode::BAD_REQUEST,
            "the agent or the alert is not of its identity's shape",
        )
            .into_response();
    }
    let Ok(version) = version.parse::<i64>() else {
        return (StatusCode::BAD_REQUEST, "the version is not a number").into_response();
    };
    let id: AgentId = match agent.parse() {
        Ok(id) => id,
        Err(e) => return fault(anyhow::Error::msg(e)),
    };
    let session = match session_of(&store, &seams.policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let principal = Principal::Person {
        person_id: &session.person_id,
        method: PersonMethod::Session,
    };
    // **The grant, before the alert is looked up**: no grant on the agent
    // answers as an unknown agent, a grant without `operator` is 403, each
    // one refusal record, so the page is no oracle.
    let roles = match store.roles_on(&session.person_id, &id).await {
        Ok(roles) => roles,
        Err(e) => return fault(e),
    };
    let refused = if roles.is_empty() {
        Some((StatusCode::NOT_FOUND, "no grant on the agent"))
    } else if !roles.iter().any(|r| r == "operator") {
        Some((
            StatusCode::FORBIDDEN,
            "only a person holding operator on the agent acknowledges its alerts",
        ))
    } else {
        None
    };
    if let Some((status, why)) = refused {
        return match store
            .audit_refusal(principal, Target::Alert(&alert), "acknowledge", why)
            .await
        {
            Ok(_) if status == StatusCode::NOT_FOUND => not_found(),
            Ok(_) => (status, why).into_response(),
            Err(e) => fault(e.context("the refusal could not be recorded")),
        };
    }
    // **Resolved**: an alert that is not this agent's is unknown, nothing
    // recorded of it.
    match store.alert(&alert).await {
        Ok(Some(found)) if found.agent_id == id.as_str() => {}
        Ok(_) => return not_found(),
        Err(e) => return fault(e),
    }
    let first = match store
        .audit_first(principal, Target::Alert(&alert), "acknowledge")
        .await
    {
        Ok(first) => first,
        Err(e) => return fault(e.context("the first record could not be written")),
    };
    let landed = acknowledged(
        &store,
        session.session_id,
        &session.person_id,
        &id,
        &alert,
        version,
        &first,
    )
    .await;
    // **A commit whose fate is unknown writes no outcome** (Spec 2.13), so
    // a later read of the audit can still settle it.
    let unknown = crate::store::commit::outcome_unknown(&landed);
    match landed {
        Ok(Landed::Done) => (
            StatusCode::SEE_OTHER,
            [(header::LOCATION, format!("/agents/{agent}"))],
        )
            .into_response(),
        Ok(Landed::AuthorityGone) => {
            let _ = store.audit_outcome(&first, false).await;
            (
                StatusCode::FORBIDDEN,
                "your session or your operator grant ended before the acknowledgement",
            )
                .into_response()
        }
        Ok(Landed::Stale) => {
            let _ = store.audit_outcome(&first, false).await;
            (
                StatusCode::CONFLICT,
                "the alert changed or was acknowledged since you read it; read it again",
            )
                .into_response()
        }
        Err(e) => {
            if !unknown {
                let _ = store.audit_outcome(&first, false).await;
            }
            fault(e)
        }
    }
}

enum Landed {
    Done,
    AuthorityGone,
    Stale,
}

/// **The acknowledgement's transaction**, holding the identity exclusion
/// shared: the session and the `operator` grant re-checked, then the write
/// at the version read, its `ok` outcome committed with it.
async fn acknowledged(
    store: &Store,
    session_id: i64,
    person: &str,
    agent: &AgentId,
    alert: &str,
    version: i64,
    first: &str,
) -> anyhow::Result<Landed> {
    let mut tx = crate::store::access::shared_hold(store).await?;
    let operator: bool = if session_stands(&mut tx, session_id, person).await? {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM role_grant WHERE person_id = $1 AND agent_id = $2 \
             AND role = 'operator' AND revoked_at IS NULL)",
        )
        .bind(person)
        .bind(agent.as_str())
        .fetch_one(&mut *tx)
        .await?
    } else {
        false
    };
    if !operator {
        tx.rollback().await?;
        return Ok(Landed::AuthorityGone);
    }
    if !acknowledge_in(&mut tx, alert, version, person).await? {
        tx.rollback().await?;
        return Ok(Landed::Stale);
    }
    crate::store::commit::commit_with_outcome(tx, "alert acknowledge", store, first).await?;
    Ok(Landed::Done)
}

/// **A save point confirmed by its hash** (`weaver-admin-operator-contract`
/// section 5 at `toddwbucy/WeaverAgent` `55fbc66`): exit 0, an answer of
/// kind `save_point_taken`, and a report naming its save point's digest, a
/// string of at most 256 bytes and no space or control. **Nothing else
/// confirms**, and the prompt never promises more: the read-back that makes
/// the answer is served from the page cache, so durability on the device
/// rests on admin's syncs before it.
pub fn confirmed(outcome: &VerbOutcome) -> Option<String> {
    if outcome.exit_code != Some(0) {
        return None;
    }
    let answer = outcome.answer.as_ref()?;
    if answer.get("kind")?.as_str()? != "save_point_taken" {
        return None;
    }
    let digest = answer.get("report")?.get("save_point")?.as_str()?;
    let shaped =
        !digest.is_empty() && digest.len() <= 256 && digest.chars().all(|c| c.is_ascii_graphic());
    shaped.then(|| digest.to_owned())
}

/// One step of a decision's sequence, as the page shows it.
#[derive(Debug, Clone)]
pub struct Step {
    pub verb: String,
    pub first: String,
    pub status: String,
}

/// **"Save point, then unload"** (ruling 2), run in the task the request
/// detaches: the `save-point` asked as the person's, then the `unload` only
/// on its hash-confirmed answer. Any other answer stops the sequence with no
/// `unload`; a confirmation missing from an answer that was otherwise
/// admin's raises its own alert, a refusal or a fault having raised theirs
/// at the door.
pub async fn save_point_then_unload(listener: Listener, asker: Asker, agent: AgentId) -> Vec<Step> {
    let mut steps = Vec::new();
    let Some(admission) = listener.admit_ask(&asker.person_id) else {
        steps.push(Step {
            verb: "save-point".into(),
            first: String::new(),
            status: VerbError::Shed.to_string(),
        });
        return steps;
    };
    let saved = match listener
        .ask_as_person(admission, &asker, &agent, Act::Verb("save-point".into()))
        .await
    {
        Ok(enqueued) => {
            let first = enqueued.first.clone();
            (first, enqueued.answer().await)
        }
        Err(refused) => {
            steps.push(Step {
                verb: "save-point".into(),
                first: String::new(),
                status: format!("not asked: {refused}"),
            });
            return steps;
        }
    };
    let (first, answered) = saved;
    let digest = match &answered {
        Ok(outcome) => confirmed(outcome),
        Err(_) => None,
    };
    let Some(digest) = digest else {
        let status = match &answered {
            Ok(outcome) => format!(
                "answered without a hash-confirmed save point (exit {:?}); no unload sent",
                outcome.exit_code
            ),
            Err(e) => format!("{e}; no unload sent"),
        };
        if let Ok(outcome) = &answered
            && outcome.exit_code == Some(0)
        {
            listener.alerts().raise(crate::alerting::Failure {
                raise: Raise {
                    agent: agent.clone(),
                    kind: AlertKind::Failed,
                    subject: "save-point".into(),
                    summary: "the save point answered without a hash-confirmed report; the unload was not sent".into(),
                    detail: outcome.answer.as_ref().map(|a| a.to_string()).unwrap_or_default(),
                    first: Some(first.clone()),
                    options: Vec::new(),
                },
                plane: "admin",
                principal: asker.person_id.clone(),
                outcome: "failed",
                reason: serde_json::json!({ "unconfirmed": outcome.answer }),
            });
        }
        steps.push(Step {
            verb: "save-point".into(),
            first,
            status,
        });
        return steps;
    };
    steps.push(Step {
        verb: "save-point".into(),
        first,
        status: format!("taken and read back to its manifest line, {digest}"),
    });
    let Some(admission) = listener.admit_ask(&asker.person_id) else {
        steps.push(Step {
            verb: "unload".into(),
            first: String::new(),
            status: VerbError::Shed.to_string(),
        });
        return steps;
    };
    match listener
        .ask_as_person(admission, &asker, &agent, Act::Verb("unload".into()))
        .await
    {
        Ok(enqueued) => {
            let first = enqueued.first.clone();
            let status = match enqueued.answer().await {
                Ok(outcome) if outcome.exit_code == Some(0) => "admin's answer".to_owned(),
                Ok(outcome) => format!(
                    "admin's refusal: {}",
                    outcome
                        .answer
                        .as_ref()
                        .and_then(|a| a.get("kind"))
                        .and_then(|k| k.as_str())
                        .unwrap_or("unnamed")
                ),
                Err(e) => e.to_string(),
            };
            steps.push(Step {
                verb: "unload".into(),
                first,
                status,
            });
        }
        Err(refused) => steps.push(Step {
            verb: "unload".into(),
            first: String::new(),
            status: format!("not asked: {refused}"),
        }),
    }
    steps
}

#[derive(Template)]
#[template(path = "decision.html")]
struct DecisionPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: super::Chrome,
    agent_id: String,
    option: String,
    steps: Vec<Step>,
    pending: bool,
}

/// **Choose a decision prompt's option** (ruling 2): parse, the session,
/// the admission, resolve the open prompt on the agent naming the option;
/// then the choice runs as the person's own asks, each checked as any ask,
/// and the page shows each step, or that it runs on past the bound.
/// Choosing never acknowledges the prompt.
pub(super) async fn choose(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    agent: String,
    alert: String,
    option: String,
) -> Response {
    if !is_agent_id(&agent) || !is_alert_id(&alert) {
        return (
            StatusCode::BAD_REQUEST,
            "the agent or the alert is not of its identity's shape",
        )
            .into_response();
    }
    if option != SAVE_POINT_THEN_UNLOAD && option != FORCE_UNLOAD {
        return (StatusCode::BAD_REQUEST, "not an option").into_response();
    }
    let id: AgentId = match agent.parse() {
        Ok(id) => id,
        Err(e) => return fault(anyhow::Error::msg(e)),
    };
    let session = match session_of(&store, &seams.policy, &headers).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    let Some(admission) = seams.asker.admit_ask(&session.person_id) else {
        return (StatusCode::SERVICE_UNAVAILABLE, VerbError::Shed.to_string()).into_response();
    };
    // **No grant answers as an unknown agent before the alert is looked
    // up**, so the page is no oracle for alert identities; the grants' verbs
    // are each ask's own check, under its hold, with its refusal record.
    match store.roles_on(&session.person_id, &id).await {
        Ok(roles) if roles.is_empty() => return not_found(),
        Ok(_) => {}
        Err(e) => return fault(e),
    }
    // Resolved: the open prompt on this agent that offers the option.
    match store.alert(&alert).await {
        Ok(Some(found))
            if found.agent_id == id.as_str()
                && found.acknowledged_at.is_none()
                && found.options.iter().any(|o| o == &option) => {}
        Ok(_) => return not_found(),
        Err(e) => return fault(e),
    }
    let asker = Asker {
        session_id: session.session_id,
        person_id: session.person_id.clone(),
    };
    let listener = seams.asker.clone();
    let sequence = if option == SAVE_POINT_THEN_UNLOAD {
        let (listener, asker, agent) = (listener.clone(), asker.clone(), id.clone());
        tokio::spawn(async move {
            let _admission = admission;
            save_point_then_unload(listener, asker, agent).await
        })
    } else {
        let (listener, asker, agent) = (listener.clone(), asker.clone(), id.clone());
        tokio::spawn(async move {
            let mut steps = Vec::new();
            match listener
                .ask_as_person(admission, &asker, &agent, Act::Verb("force-unload".into()))
                .await
            {
                Ok(enqueued) => {
                    let first = enqueued.first.clone();
                    let status = match enqueued.answer().await {
                        Ok(outcome) if outcome.exit_code == Some(0) => {
                            "admin's answer: forced; the state since the last published save point is lost".to_owned()
                        }
                        Ok(_) => "admin's refusal".to_owned(),
                        Err(e) => e.to_string(),
                    };
                    steps.push(Step {
                        verb: "force-unload".into(),
                        first,
                        status,
                    });
                }
                Err(refused) => steps.push(Step {
                    verb: "force-unload".into(),
                    first: String::new(),
                    status: format!("not asked: {refused}"),
                }),
            }
            steps
        })
    };
    let (status, steps, pending) = match tokio::time::timeout(seams.asker.silence(), sequence).await
    {
        Ok(Ok(steps)) => (StatusCode::OK, steps, false),
        Ok(Err(e)) => return fault(anyhow::anyhow!("the decision's task ended: {e}")),
        Err(_) => (StatusCode::ACCEPTED, Vec::new(), true),
    };
    let (admin, chrome) = match (
        store.is_admin(&session.person_id).await,
        super::Chrome::of(&store, &session.person_id).await,
    ) {
        (Ok(admin), Ok(chrome)) => (admin, chrome),
        (Err(e), _) | (_, Err(e)) => return fault(e),
    };
    match (DecisionPage {
        here: "agents",
        who: session.name,
        admin,
        chrome,
        agent_id: agent,
        option,
        steps,
        pending,
    })
    .render()
    {
        Ok(html) => (status, Html(html)).into_response(),
        Err(e) => fault(e),
    }
}
