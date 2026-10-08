//! **The host's identity commands** (Spec 2.13, `docs/project/design-2026-10-07-iam.md`
//! sections 7 and 8), subcommands of the `weaver-web` binary beside the
//! register verbs: `person bootstrap`, `person token`, `person reset`,
//! `grant add`, `grant remove` and `role set`. The host is the principal
//! that is not a person, authorized by access to the server's host and the
//! store rather than by grants, which is how the first admin comes to exist
//! and how a lone admin comes to hold a role on an agent.
//!
//! **Each command follows the register verbs' shape**: it reads and decides
//! what it can without writing, writes its audit record as the host's with
//! its `--author` claim **before any mutation**, does nothing where that
//! record cannot be written, then acts in one transaction **under the
//! identity exclusion**, re-checking there what a concurrent write could
//! change, and writes the outcome naming the first record. Each answers one
//! JSON object with the exit status agreeing.
//!
//! **A token is printed once**, in the answer of the command that issued it,
//! and kept by the store only as its digest.

use crate::config::ServerConfig;
use crate::link::verbs::{Answer, first_record, refused, with_outcome};
use crate::store::Store;
use crate::store::audit::Target;
use crate::store::identity::{
    self, IssuedToken, Person, Supersedes, TOKEN_LIFETIME_MAX_HOURS, VOCABULARY,
};
use serde_json::json;

/// The lifetime a command issues its token with: the one given, or the
/// config's; refused at 0 or past seven days before anything is written.
fn lifetime(cfg: &ServerConfig, hours: Option<u32>) -> Result<u32, String> {
    let hours = hours.unwrap_or(cfg.enrollment_token_hours);
    if hours == 0 || hours > TOKEN_LIFETIME_MAX_HOURS {
        return Err(format!(
            "an enrollment token lives between 1 and {TOKEN_LIFETIME_MAX_HOURS} hours (seven days), not {hours}"
        ));
    }
    Ok(hours)
}

fn token_answer(token: &IssuedToken) -> serde_json::Value {
    json!({ "token": token.value, "expires_at": token.expires_at.to_rfc3339() })
}

/// The person `spec` names, or the command's refusal.
async fn person(store: &Store, verb: &str, spec: &str) -> Result<Person, Answer> {
    match store.person(spec).await {
        Ok(Some(person)) => Ok(person),
        Ok(None) => Err(refused(verb, format!("no person {spec}"))),
        Err(e) => Err(refused(verb, format!("{e:#}"))),
    }
}

/// `person bootstrap <name>`: **a person, their server-wide admin grant and
/// an enrollment token, in one transaction** (Spec 2.13). The host can
/// always bootstrap an admin, so a store that already has admins takes one
/// more. The token is printed once.
pub async fn bootstrap(
    store: &Store,
    cfg: &ServerConfig,
    name: &str,
    hours: Option<u32>,
    author: Option<&str>,
) -> Answer {
    const VERB: &str = "person bootstrap";
    let name = match identity::given_name(name) {
        Ok(name) => name,
        Err(why) => return refused(VERB, why),
    };
    let hours = match lifetime(cfg, hours) {
        Ok(hours) => hours,
        Err(why) => return refused(VERB, why),
    };
    let (person_id, grant_id) = match (store.mint_key("pe").await, store.mint_key("gr").await) {
        (Ok(p), Ok(g)) => (p, g),
        (Err(e), _) | (_, Err(e)) => return refused(VERB, format!("{e:#}")),
    };
    let first = match first_record(store, VERB, author, Target::Person(&person_id)).await {
        Ok(first) => first,
        Err(refusal) => return refusal,
    };
    let answer = async {
        let mut tx = store.identity_transaction().await?;
        if identity::name_taken(&mut tx, &identity::name_key(&name)).await? {
            anyhow::bail!(
                "a person whose name is {name} in its canonical form stands; names are one where they differ only by case, width or composition"
            );
        }
        identity::insert_person(&mut tx, &person_id, &name, author).await?;
        identity::insert_grant(&mut tx, &grant_id, &person_id, "admin", None, author).await?;
        let token = identity::issue_token(&mut tx, &person_id, hours, Supersedes::Issue).await?;
        tx.commit().await?;
        anyhow::Ok(token)
    }
    .await;
    let answer = match answer {
        Ok(token) => {
            let mut value = json!({
                "verb": VERB, "ok": true, "person": person_id, "name": name, "grant": grant_id,
            });
            value
                .as_object_mut()
                .unwrap()
                .extend(token_answer(&token).as_object().unwrap().clone());
            Answer { value, ok: true }
        }
        Err(e) => refused(VERB, format!("{e:#}")),
    };
    with_outcome(store, first, answer).await
}

/// `person token <person>`: **an enrollment token for a person holding no
/// passkey**, refused otherwise, since a token never replaces a credential
/// (Spec 2.13). The person's earlier live token ends.
pub async fn token(
    store: &Store,
    cfg: &ServerConfig,
    spec: &str,
    hours: Option<u32>,
    author: Option<&str>,
) -> Answer {
    const VERB: &str = "person token";
    let hours = match lifetime(cfg, hours) {
        Ok(hours) => hours,
        Err(why) => return refused(VERB, why),
    };
    let person = match person(store, VERB, spec).await {
        Ok(person) => person,
        Err(refusal) => return refusal,
    };
    let first = match first_record(store, VERB, author, Target::Person(&person.person_id)).await {
        Ok(first) => first,
        Err(refusal) => return refusal,
    };
    let answer = async {
        let mut tx = store.identity_transaction().await?;
        let enabled: bool = sqlx::query_scalar("SELECT enabled FROM person WHERE person_id = $1")
            .bind(&person.person_id)
            .fetch_one(&mut *tx)
            .await?;
        if !enabled {
            anyhow::bail!("{} is disabled, and a disabled person is issued nothing", person.person_id);
        }
        if identity::holds_passkey(&mut tx, &person.person_id).await? {
            anyhow::bail!(
                "{} holds a passkey, and a token never replaces a credential; the host reset clears them first",
                person.person_id
            );
        }
        let token =
            identity::issue_token(&mut tx, &person.person_id, hours, Supersedes::Issue).await?;
        tx.commit().await?;
        anyhow::Ok(token)
    }
    .await;
    let answer = match answer {
        Ok(token) => {
            let mut value = json!({ "verb": VERB, "ok": true, "person": person.person_id });
            value
                .as_object_mut()
                .unwrap()
                .extend(token_answer(&token).as_object().unwrap().clone());
            Answer { value, ok: true }
        }
        Err(e) => refused(VERB, format!("{e:#}")),
    };
    with_outcome(store, first, answer).await
}

/// `person reset <person>`: **the person's passkeys cleared and a token
/// issued, in one write** (design section 7). It writes no session: each
/// session opened with a cleared passkey ends at its next use.
pub async fn reset(
    store: &Store,
    cfg: &ServerConfig,
    spec: &str,
    hours: Option<u32>,
    author: Option<&str>,
) -> Answer {
    const VERB: &str = "person reset";
    let hours = match lifetime(cfg, hours) {
        Ok(hours) => hours,
        Err(why) => return refused(VERB, why),
    };
    let person = match person(store, VERB, spec).await {
        Ok(person) => person,
        Err(refusal) => return refusal,
    };
    let first = match first_record(store, VERB, author, Target::Person(&person.person_id)).await {
        Ok(first) => first,
        Err(refusal) => return refusal,
    };
    let answer = async {
        let mut tx = store.identity_transaction().await?;
        let cleared = identity::clear_passkeys(&mut tx, &person.person_id).await?;
        let token =
            identity::issue_token(&mut tx, &person.person_id, hours, Supersedes::Reset).await?;
        tx.commit().await?;
        anyhow::Ok((cleared, token))
    }
    .await;
    let answer = match answer {
        Ok((cleared, token)) => {
            let mut value = json!({
                "verb": VERB, "ok": true, "person": person.person_id, "passkeys_cleared": cleared,
            });
            value
                .as_object_mut()
                .unwrap()
                .extend(token_answer(&token).as_object().unwrap().clone());
            Answer { value, ok: true }
        }
        Err(e) => refused(VERB, format!("{e:#}")),
    };
    with_outcome(store, first, answer).await
}

/// The role and agent a grant names, checked against each other: `admin`
/// server-wide and with no agent, every other role on one agent of the
/// register.
async fn grant_scope(
    store: &Store,
    verb: &str,
    role: &str,
    agent: Option<&str>,
) -> Result<Option<String>, Answer> {
    let role = match store.role(role).await {
        Ok(Some(role)) => role,
        Ok(None) => return Err(refused(verb, format!("no role {role}"))),
        Err(e) => return Err(refused(verb, format!("{e:#}"))),
    };
    match (role.scope.as_str(), agent) {
        ("server", None) => Ok(None),
        ("server", Some(_)) => Err(refused(
            verb,
            format!("{} is server-wide and names no agent", role.name),
        )),
        (_, None) => Err(refused(
            verb,
            format!(
                "{} is a role on an agent; name the agent with --agent",
                role.name
            ),
        )),
        (_, Some(spec)) => match store.resolve_agent(spec).await {
            Ok(agent) => Ok(Some(agent.agent_id.as_str().to_owned())),
            Err(e) => Err(refused(verb, format!("{e:#}"))),
        },
    }
}

/// `grant add <person> <role> [--agent <agent>]`: **a grant written by the
/// host** (design section 8), which is how a server's only admin comes to
/// hold a role on an agent: no person writes a grant on themselves, and the
/// host is not a person.
pub async fn grant_add(
    store: &Store,
    spec: &str,
    role: &str,
    agent: Option<&str>,
    author: Option<&str>,
) -> Answer {
    const VERB: &str = "grant add";
    let person = match person(store, VERB, spec).await {
        Ok(person) => person,
        Err(refusal) => return refusal,
    };
    let agent = match grant_scope(store, VERB, role, agent).await {
        Ok(agent) => agent,
        Err(refusal) => return refusal,
    };
    let grant_id = match store.mint_key("gr").await {
        Ok(id) => id,
        Err(e) => return refused(VERB, format!("{e:#}")),
    };
    let first = match first_record(store, VERB, author, Target::Grant(&grant_id)).await {
        Ok(first) => first,
        Err(refusal) => return refusal,
    };
    let answer = async {
        let mut tx = store.identity_transaction().await?;
        let held: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM role_grant WHERE person_id = $1 AND role = $2 \
             AND agent_id IS NOT DISTINCT FROM $3 AND revoked_at IS NULL)",
        )
        .bind(&person.person_id)
        .bind(role)
        .bind(agent.as_deref())
        .fetch_one(&mut *tx)
        .await?;
        if held {
            anyhow::bail!("{} already holds {role} there", person.person_id);
        }
        identity::insert_grant(
            &mut tx,
            &grant_id,
            &person.person_id,
            role,
            agent.as_deref(),
            author,
        )
        .await?;
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;
    let answer = match answer {
        Ok(()) => Answer {
            value: json!({
                "verb": VERB, "ok": true, "grant": grant_id, "person": person.person_id,
                "role": role, "agent": agent,
            }),
            ok: true,
        },
        Err(e) => refused(VERB, format!("{e:#}")),
    };
    with_outcome(store, first, answer).await
}

// A test's hold on `grant remove` of one named grant, between the
// last-admin count and the revocation, so two removals can be staged against
// each other; keyed by the grant so no other test's removal takes it.
#[cfg(test)]
pub(crate) type Hold = (
    String,
    std::sync::Arc<tokio::sync::Notify>,
    std::sync::Arc<tokio::sync::Notify>,
);
#[cfg(test)]
pub(crate) static REMOVE_HOLD: std::sync::Mutex<Option<Hold>> = std::sync::Mutex::new(None);

/// `grant remove <person> <role> [--agent <agent>]`: **a grant revoked by the
/// host**. **The last-admin rule binds the host too**: revoking the grant
/// that would leave no enabled person holding a live admin grant is
/// refused, counted under the exclusion so two removals cannot both pass.
pub async fn grant_remove(
    store: &Store,
    spec: &str,
    role: &str,
    agent: Option<&str>,
    author: Option<&str>,
) -> Answer {
    const VERB: &str = "grant remove";
    let person = match person(store, VERB, spec).await {
        Ok(person) => person,
        Err(refusal) => return refusal,
    };
    let agent = match grant_scope(store, VERB, role, agent).await {
        Ok(agent) => agent,
        Err(refusal) => return refusal,
    };
    let grant_id = match store
        .live_grant(&person.person_id, role, agent.as_deref())
        .await
    {
        Ok(Some(id)) => id,
        Ok(None) => {
            return refused(
                VERB,
                format!("{} holds no live {role} there", person.person_id),
            );
        }
        Err(e) => return refused(VERB, format!("{e:#}")),
    };
    let first = match first_record(store, VERB, author, Target::Grant(&grant_id)).await {
        Ok(first) => first,
        Err(refusal) => return refusal,
    };
    let answer = async {
        let mut tx = store.identity_transaction().await?;
        if !identity::grant_is_live(&mut tx, &grant_id).await? {
            anyhow::bail!("{grant_id} was revoked meanwhile");
        }
        if role == "admin" && identity::admins_remaining_without(&mut tx, &grant_id).await? == 0 {
            anyhow::bail!(
                "{grant_id} is the last enabled admin's grant, and at least one enabled admin remains"
            );
        }
        #[cfg(test)]
        {
            let hold = {
                let mut slot = REMOVE_HOLD.lock().unwrap();
                match slot.as_ref() {
                    Some((grant, ..)) if *grant == grant_id => slot.take(),
                    _ => None,
                }
            };
            if let Some((_, locked, release)) = hold {
                locked.notify_one();
                release.notified().await;
            }
        }
        identity::revoke_grant(&mut tx, &grant_id, author).await?;
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;
    let answer = match answer {
        Ok(()) => Answer {
            value: json!({ "verb": VERB, "ok": true, "grant": grant_id, "person": person.person_id }),
            ok: true,
        },
        Err(e) => refused(VERB, format!("{e:#}")),
    };
    with_outcome(store, first, answer).await
}

/// `role set <role> [<verb>...]`: **a per-agent role's verbs, written by the
/// host** (design section 8), within the vocabulary of Spec 7.2 plus
/// `turn`. `admin` is fixed by the store and refused.
pub async fn role_set(store: &Store, role: &str, verbs: &[String], author: Option<&str>) -> Answer {
    const VERB: &str = "role set";
    if role == "admin" {
        return refused(
            VERB,
            "admin is fixed by the store and carries no agent verb; nothing writes it",
        );
    }
    let mut set: Vec<String> = Vec::new();
    for verb in verbs {
        if !VOCABULARY.contains(&verb.as_str()) {
            return refused(
                VERB,
                format!(
                    "{verb} is not in the vocabulary ({})",
                    VOCABULARY.join(", ")
                ),
            );
        }
        if !set.contains(verb) {
            set.push(verb.clone());
        }
    }
    match store.role(role).await {
        Ok(Some(_)) => {}
        Ok(None) => return refused(VERB, format!("no role {role}")),
        Err(e) => return refused(VERB, format!("{e:#}")),
    }
    let first = match first_record(store, VERB, author, Target::Role(role)).await {
        Ok(first) => first,
        Err(refusal) => return refusal,
    };
    let answer = async {
        let mut tx = store.identity_transaction().await?;
        identity::set_role_verbs(&mut tx, role, &set, author).await?;
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;
    let answer = match answer {
        Ok(()) => Answer {
            value: json!({ "verb": VERB, "ok": true, "role": role, "verbs": set }),
            ok: true,
        },
        Err(e) => refused(VERB, format!("{e:#}")),
    };
    with_outcome(store, first, answer).await
}
