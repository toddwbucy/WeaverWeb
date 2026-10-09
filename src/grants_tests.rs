//! An admin's writes to roles and grants (`src/surfaces/grants.rs`,
//! `src/store/grants.rs`; Spec 2.13): the page and every write refused
//! without a live admin grant, each write end to end with its audit pair,
//! stale versions, the self-change rules before any record and under the
//! exclusion, the last-admin rule, the authority re-checked inside the
//! write's transaction, malformed identities, and the shared-conversation
//! note. Each test runs on a database of its own. No name is a real one.

use std::time::Duration;

use axum::http::StatusCode;

use crate::admin_tests::{admin, form, hold, send_as, signed_in};
use crate::host;
use crate::passkeys_tests::{app, authenticator, passkeys, person_with_token};
use crate::sign_in_tests::rows;
use crate::store::Store;
use crate::store::admin::{INSIDE_HOLD, Refusal};
use crate::store::read::tests::fresh_store;
use crate::surfaces::admin::READ_HOLD;

/// An agent in the register, its fingerprints derived from its name.
async fn agent(s: &Store, name: &str) -> String {
    let fp =
        |plane: &str| crate::link::authority::fingerprint(format!("{plane}-{name}").as_bytes());
    let (id, _) = s
        .register_agent(
            "box",
            name,
            Some("lab"),
            &fp("gate"),
            &fp("admin"),
            &fp("authority"),
        )
        .await
        .unwrap();
    id.as_str().to_owned()
}

/// The host's grant, its identity.
async fn host_grant(s: &Store, person: &str, role: &str, agent: Option<&str>) -> String {
    let granted = host::grant_add(s, person, role, agent, None).await;
    assert!(granted.ok, "{}", granted.value);
    granted.value["grant"].as_str().unwrap().to_owned()
}

async fn grant_version(s: &Store, grant: &str) -> String {
    let version: i64 = sqlx::query_scalar("SELECT version FROM role_grant WHERE grant_id = $1")
        .bind(grant)
        .fetch_one(&s.pool)
        .await
        .unwrap();
    version.to_string()
}

async fn role_version(s: &Store, role: &str) -> String {
    s.role(role).await.unwrap().unwrap().version.to_string()
}

async fn live(s: &Store, grant: &str) -> bool {
    sqlx::query_scalar("SELECT revoked_at IS NULL FROM role_grant WHERE grant_id = $1")
        .bind(grant)
        .fetch_one(&s.pool)
        .await
        .unwrap()
}

/// The admin's audit records of `action` on `target`: the first records,
/// the `ok` outcomes and the `failed` ones, each by `session`.
async fn audited(
    s: &Store,
    admin: &str,
    kind: &str,
    target: &str,
    action: &str,
) -> (i64, i64, i64) {
    let count = |clause: &'static str| {
        let query = format!(
            "SELECT count(*) FROM audit WHERE principal = 'person' AND person_id = $1 \
             AND method = 'session' AND target_kind = $2 AND target_id = $3 \
             AND action = $4 AND {clause}"
        );
        async move {
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(query))
                .bind(admin)
                .bind(kind)
                .bind(target)
                .bind(action)
                .fetch_one(&s.pool)
                .await
                .unwrap()
        }
    };
    (
        count("answers IS NULL AND refusal IS NULL").await,
        count("outcome = 'ok'").await,
        count("outcome = 'failed'").await,
    )
}

async fn post(app: &axum::Router, uri: &str, body: String, session: &str) -> (StatusCode, String) {
    let (status, _, answer) = send_as(app, "POST", uri, Some(body), session).await;
    (status, answer)
}

/// **A session whose person holds no live admin grant is refused at the
/// page and at every write**, each write's refusal one record carrying it,
/// nothing written.
#[tokio::test]
async fn a_session_without_the_admin_grant_is_refused_at_the_page_and_every_write() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, _) = admin(&app, s, &mut authenticator(), "ada").await;
    let (_, session) = signed_in(&app, s, &mut authenticator(), "bea").await;
    let k = agent(s, "karl").await;
    let held = host_grant(s, &ada, "observer", Some(&k)).await;

    let (status, _, page) = send_as(&app, "GET", "/admin/grants", None, &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{page}");
    let v = grant_version(s, &held).await;
    let r = role_version(s, "operator").await;
    for (uri, body) in [
        (
            "/admin/grants/grant",
            form(&[("person", &ada), ("role", "operator"), ("agent", &k)]),
        ),
        (
            "/admin/grants/revoke",
            form(&[("grant", &held), ("version", &v)]),
        ),
        (
            "/admin/roles/set",
            form(&[("role", "operator"), ("version", &r), ("verbs", "show")]),
        ),
    ] {
        let (status, answer) = post(&app, uri, body, &session).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}: {answer}");
    }
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE refusal = 'not an admin'"
        )
        .await,
        3
    );
    assert!(live(s, &held).await);
    assert_eq!(
        rows(s, "SELECT count(*) FROM role_grant WHERE role = 'operator'").await,
        0
    );
    assert_eq!(
        s.role("operator")
            .await
            .unwrap()
            .unwrap()
            .version
            .to_string(),
        r
    );
}

/// **The page lists every live grant, every role and the register's
/// agents**, offers no write on the admin's own grants or a role they hold,
/// and the navigation reaches it.
#[tokio::test]
async fn the_page_lists_grants_roles_and_agents() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, _) = person_with_token(s, "bea").await;
    let k = agent(s, "karl").await;
    let own = host_grant(s, &ada, "observer", Some(&k)).await;
    let theirs = host_grant(s, &bea, "operator", Some(&k)).await;

    let (status, _, page) = send_as(&app, "GET", "/admin/grants", None, &session).await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert!(page.contains("box/karl") && page.contains(&k), "{page}");
    assert!(
        page.contains(&format!("value=\"{theirs}\"")),
        "bea's grant revocable"
    );
    assert!(
        !page.contains(&format!("value=\"{own}\"")),
        "no write on one's own grant"
    );
    assert!(
        page.contains("you hold this role"),
        "observer, held, not editable"
    );
    assert!(
        page.contains("name=\"role\" value=\"operator\""),
        "operator editable: {page}"
    );
    assert!(
        !page.contains("name=\"role\" value=\"admin\""),
        "admin fixed"
    );
    assert!(
        page.contains("href=\"/admin/grants\""),
        "the navigation reaches it"
    );
}

/// **Each write lands with its audit pair**: a grant, a role's verbs, and
/// a revocation, each a first record and an `ok` outcome with the admin by
/// `session` as the principal; a grant's scope and a held grant refused.
#[tokio::test]
async fn each_write_lands_with_its_audit_pair() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, _) = person_with_token(s, "bea").await;
    let k = agent(s, "karl").await;

    let (status, answer) = post(
        &app,
        "/admin/grants/grant",
        form(&[("person", &bea), ("role", "observer"), ("agent", &k)]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    let (granted, _) = s
        .live_grant(&bea, "observer", Some(&k))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        audited(s, &ada, "grant", &granted, "grant add").await,
        (1, 1, 0)
    );
    let author: Option<String> =
        sqlx::query_scalar("SELECT author FROM role_grant WHERE grant_id = $1")
            .bind(&granted)
            .fetch_one(&s.pool)
            .await
            .unwrap();
    assert_eq!(author.as_deref(), Some(ada.as_str()));

    let (status, _) = post(
        &app,
        "/admin/grants/grant",
        form(&[("person", &bea), ("role", "observer"), ("agent", &k)]),
        &session,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "one live grant per person, role and agent"
    );
    let (status, _) = post(
        &app,
        "/admin/grants/grant",
        form(&[("person", &bea), ("role", "observer"), ("agent", "")]),
        &session,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "a per-agent role names its agent"
    );
    let (status, _) = post(
        &app,
        "/admin/grants/grant",
        form(&[("person", &bea), ("role", "admin"), ("agent", &k)]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "admin names no agent");
    let (status, answer) = post(
        &app,
        "/admin/grants/grant",
        form(&[("person", &bea), ("role", "admin"), ("agent", "")]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(s.is_admin(&bea).await.unwrap());

    let r = role_version(s, "operator").await;
    let (status, answer) = post(
        &app,
        "/admin/roles/set",
        form(&[
            ("role", "operator"),
            ("version", &r),
            ("verbs", "show"),
            ("verbs", "turn"),
            ("verbs", "show"),
        ]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert_eq!(
        s.role("operator").await.unwrap().unwrap().verbs,
        vec!["show".to_owned(), "turn".to_owned()]
    );
    assert_eq!(
        audited(s, &ada, "role", "operator", "role set").await,
        (1, 1, 0)
    );

    let v = grant_version(s, &granted).await;
    let (status, answer) = post(
        &app,
        "/admin/grants/revoke",
        form(&[("grant", &granted), ("version", &v)]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    assert!(!live(s, &granted).await);
    assert_eq!(
        audited(s, &ada, "grant", &granted, "grant remove").await,
        (1, 1, 0)
    );
}

/// **A stale version is refused**: two edits of one role from one page,
/// the first held after the surface's read while the second lands, and two
/// revocations of one grant likewise; and a revocation carrying a version
/// the grant no longer has.
#[tokio::test]
async fn a_stale_version_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, ada_session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (_, cara_session) = admin(&app, s, &mut authenticator(), "cara").await;
    let (bea, _) = person_with_token(s, "bea").await;
    let k = agent(s, "karl").await;

    let r = role_version(s, "operator").await;
    let (read, release) = hold(&READ_HOLD, &ada);
    let first = tokio::spawn({
        let (app, session, body) = (
            app.clone(),
            ada_session.clone(),
            form(&[("role", "operator"), ("version", &r), ("verbs", "show")]),
        );
        async move { post(&app, "/admin/roles/set", body, &session).await }
    });
    read.notified().await;
    let (status, answer) = post(
        &app,
        "/admin/roles/set",
        form(&[("role", "operator"), ("version", &r), ("verbs", "turn")]),
        &cara_session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{answer}");
    release.notify_one();
    let (status, answer) = first.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
    assert_eq!(
        s.role("operator").await.unwrap().unwrap().verbs,
        vec!["turn".to_owned()]
    );
    assert_eq!(
        audited(s, &ada, "role", "operator", "role set").await,
        (1, 0, 1)
    );

    let granted = host_grant(s, &bea, "observer", Some(&k)).await;
    let v = grant_version(s, &granted).await;
    let (status, _) = post(
        &app,
        "/admin/grants/revoke",
        form(&[("grant", &granted), ("version", "99")]),
        &ada_session,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a version the grant does not have"
    );
    assert!(live(s, &granted).await);
    let (read, release) = hold(&READ_HOLD, &ada);
    let first = tokio::spawn({
        let (app, session, body) = (
            app.clone(),
            ada_session.clone(),
            form(&[("grant", &granted), ("version", &v)]),
        );
        async move { post(&app, "/admin/grants/revoke", body, &session).await }
    });
    read.notified().await;
    let (status, _) = post(
        &app,
        "/admin/grants/revoke",
        form(&[("grant", &granted), ("version", &v)]),
        &cara_session,
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    release.notify_one();
    let (status, answer) = first.await.unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{answer}");
}

/// **The self-change rules, refused before any record**: a grant to
/// oneself, a revocation of one's own grant (an agent's role and the admin
/// grant), and an edit of a role one holds.
#[tokio::test]
async fn the_self_change_rules_are_refused_before_any_record() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (_, _) = admin(&app, s, &mut authenticator(), "cara").await;
    let k = agent(s, "karl").await;
    let own = host_grant(s, &ada, "observer", Some(&k)).await;
    let (own_admin, _) = s.live_grant(&ada, "admin", None).await.unwrap().unwrap();
    let before = rows(s, "SELECT count(*) FROM audit").await;

    let (status, answer) = post(
        &app,
        "/admin/grants/grant",
        form(&[("person", &ada), ("role", "operator"), ("agent", &k)]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "self-grant: {answer}");
    for grant in [&own, &own_admin] {
        let v = grant_version(s, grant).await;
        let (status, answer) = post(
            &app,
            "/admin/grants/revoke",
            form(&[("grant", grant), ("version", &v)]),
            &session,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "self-revoke: {answer}");
    }
    let r = role_version(s, "observer").await;
    let (status, answer) = post(
        &app,
        "/admin/roles/set",
        form(&[
            ("role", "observer"),
            ("version", &r),
            ("verbs", "show"),
            ("verbs", "stop"),
        ]),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "a role one holds: {answer}");

    assert_eq!(
        rows(s, "SELECT count(*) FROM audit").await,
        before,
        "before any record"
    );
    assert!(live(s, &own).await && live(s, &own_admin).await);
    assert!(
        s.live_grant(&ada, "operator", Some(&k))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        s.role("observer").await.unwrap().unwrap().verbs,
        vec!["show".to_owned()]
    );
}

/// **A role's edit is checked against its editor's grants under the
/// exclusion**: the host grants the role to the editing admin between the
/// surface's read and the edit, and the edit is refused inside the
/// transaction, audited as failed.
#[tokio::test]
async fn a_role_granted_to_its_editor_mid_edit_refuses_the_edit() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let k = agent(s, "karl").await;
    let r = role_version(s, "operator").await;

    let (read, release) = hold(&READ_HOLD, &ada);
    let edit = tokio::spawn({
        let (app, session, body) = (
            app.clone(),
            session.clone(),
            form(&[("role", "operator"), ("version", &r), ("verbs", "show")]),
        );
        async move { post(&app, "/admin/roles/set", body, &session).await }
    });
    read.notified().await;
    host_grant(s, &ada, "operator", Some(&k)).await;
    release.notify_one();
    let (status, answer) = edit.await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(answer.contains("a role they hold"), "{answer}");
    assert_eq!(
        audited(s, &ada, "role", "operator", "role set").await,
        (1, 0, 1)
    );
    assert_eq!(
        s.role("operator")
            .await
            .unwrap()
            .unwrap()
            .version
            .to_string(),
        r
    );
}

/// **The last enabled admin's grant is never revoked**: the store's count
/// answers before its own-grant rule. Through the surface the write cannot
/// be formed, the revoker being an enabled admin who is not the grantee, so
/// this asks the store directly with the admin's own grant.
#[tokio::test]
async fn the_last_enabled_admins_grant_is_never_revoked() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let session_id: i64 =
        sqlx::query_scalar("SELECT session_id FROM session WHERE bearer_digest = $1")
            .bind(crate::surfaces::gate::digest(&session))
            .fetch_one(&s.pool)
            .await
            .unwrap();
    let (grant, version) = s.live_grant(&ada, "admin", None).await.unwrap().unwrap();
    assert_eq!(
        s.admin_revoke(&ada, session_id, &grant, version)
            .await
            .unwrap(),
        Err(Refusal::LastAdmin)
    );
    assert!(live(s, &grant).await);
}

/// **Two admins revoking each other's admin grant at once leave one**: the
/// exclusion serializes the two, and the second re-checks its authority
/// after the first commits, finding its own grant revoked.
#[tokio::test]
async fn two_admins_revoking_each_other_leave_one() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, ada_session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, bea_session) = admin(&app, s, &mut authenticator(), "bea").await;
    let (ada_grant, va) = s.live_grant(&ada, "admin", None).await.unwrap().unwrap();
    let (bea_grant, vb) = s.live_grant(&bea, "admin", None).await.unwrap().unwrap();

    let (inside, release) = hold(&INSIDE_HOLD, &ada);
    let first = tokio::spawn({
        let (app, body) = (
            app.clone(),
            form(&[("grant", &bea_grant), ("version", &vb.to_string())]),
        );
        async move { post(&app, "/admin/grants/revoke", body, &ada_session).await }
    });
    inside.notified().await;
    let second = tokio::spawn({
        let (app, body) = (
            app.clone(),
            form(&[("grant", &ada_grant), ("version", &va.to_string())]),
        );
        async move { post(&app, "/admin/grants/revoke", body, &bea_session).await }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    release.notify_one();
    let (first, second) = (first.await.unwrap(), second.await.unwrap());
    assert_eq!(first.0, StatusCode::SEE_OTHER, "{}", first.1);
    assert_eq!(second.0, StatusCode::FORBIDDEN, "{}", second.1);
    assert!(live(s, &ada_grant).await, "one admin remains");
    assert!(!live(s, &bea_grant).await);
}

/// **The authority is re-checked inside the write's transaction**: an
/// admin whose grant the host revokes between the surface's read and the
/// write is refused as its authority gone, audited as failed.
#[tokio::test]
async fn a_write_whose_admin_grant_was_revoked_is_refused() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (ada, session) = admin(&app, s, &mut authenticator(), "ada").await;
    admin(&app, s, &mut authenticator(), "cara").await;
    let (bea, _) = person_with_token(s, "bea").await;
    let k = agent(s, "karl").await;

    let (read, release) = hold(&READ_HOLD, &ada);
    let write = tokio::spawn({
        let (app, session, body) = (
            app.clone(),
            session.clone(),
            form(&[("person", &bea), ("role", "observer"), ("agent", &k)]),
        );
        async move { post(&app, "/admin/grants/grant", body, &session).await }
    });
    read.notified().await;
    let removed = host::grant_remove(s, &ada, "admin", None, None).await;
    assert!(removed.ok, "{}", removed.value);
    release.notify_one();
    let (status, answer) = write.await.unwrap();
    assert_eq!(status, StatusCode::FORBIDDEN, "{answer}");
    assert!(answer.contains("no longer stands"), "{answer}");
    assert!(
        s.live_grant(&bea, "observer", Some(&k))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        rows(
            s,
            "SELECT count(*) FROM audit WHERE action = 'grant add' AND outcome = 'failed'"
        )
        .await,
        1
    );
}

/// **Every name or identity an ask refers to is resolved before any
/// record**: a malformed identity, an unknown role at a grant or a role's
/// edit, a role whose scope disagrees with the agent named or not, the
/// admin role's edit, or a verb outside the vocabulary each answers as the
/// ask's fault, and no audit row is written.
#[tokio::test]
async fn a_malformed_ask_is_refused_before_any_record() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (_, other) = signed_in(&app, s, &mut authenticator(), "dot").await;
    let (bea, _) = person_with_token(s, "bea").await;
    let k = agent(s, "karl").await;
    let r = role_version(s, "operator").await;
    let before = rows(s, "SELECT count(*) FROM audit").await;
    let cases = [
        (
            "/admin/grants/grant",
            form(&[("person", &bea), ("role", "watcher"), ("agent", &k)]),
            StatusCode::NOT_FOUND,
        ),
        (
            "/admin/grants/grant",
            form(&[("person", &bea), ("role", "observer"), ("agent", "")]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/grant",
            form(&[("person", &bea), ("role", "admin"), ("agent", &k)]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/grant",
            form(&[("person", "bea"), ("role", "observer"), ("agent", &k)]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/grant",
            form(&[("person", "pe-0123"), ("role", "observer"), ("agent", &k)]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/grant",
            form(&[
                ("person", &bea),
                ("role", "observer"),
                ("agent", "box/karl"),
            ]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/grant",
            form(&[("person", &bea), ("role", "observer"), ("agent", "ag-0123")]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/revoke",
            form(&[("grant", "gr-0123"), ("version", "1")]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/grants/revoke",
            form(&[("grant", "pe-0123456789abcdef"), ("version", "1")]),
            StatusCode::BAD_REQUEST,
        ),
        (
            "/admin/roles/set",
            form(&[("role", "watcher"), ("version", &r), ("verbs", "show")]),
            StatusCode::NOT_FOUND,
        ),
        (
            "/admin/roles/set",
            form(&[("role", "admin"), ("version", "1"), ("verbs", "show")]),
            StatusCode::FORBIDDEN,
        ),
        (
            "/admin/roles/set",
            form(&[("role", "operator"), ("version", &r), ("verbs", "fly")]),
            StatusCode::BAD_REQUEST,
        ),
    ];
    // Each ask's records are counted before its status, so a guard dropped
    // fails where the records land: an admin's begun and failed pair, or a
    // non-admin's refusal.
    for (uri, body, expected) in cases {
        for (who, asking) in [("an admin", &session), ("a non-admin", &other)] {
            let (status, answer) = post(&app, uri, body.clone(), asking).await;
            assert_eq!(
                rows(s, "SELECT count(*) FROM audit").await,
                before,
                "{uri} {body} from {who}: no record"
            );
            assert_eq!(status, expected, "{uri} {body} from {who}: {answer}");
        }
    }
}

/// **The page names a shared conversation** beside an agent on which more
/// than one person holds a role carrying `turn`, and not otherwise.
#[tokio::test]
async fn the_page_names_a_shared_conversation() {
    let Some(fresh) = fresh_store().await else {
        return;
    };
    let s = &fresh.store;
    let app = app(s, Some(passkeys()));
    let (_, session) = admin(&app, s, &mut authenticator(), "ada").await;
    let (bea, _) = person_with_token(s, "bea").await;
    let (dot, _) = person_with_token(s, "dot").await;
    let k = agent(s, "karl").await;
    let j = agent(s, "jane").await;
    host_grant(s, &bea, "operator", Some(&k)).await;
    host_grant(s, &dot, "observer", Some(&k)).await;
    host_grant(s, &dot, "operator", Some(&j)).await;

    let note = |page: &str, agent: &str| {
        let at = page.find(&format!("<td>{agent}</td>")).unwrap();
        let row_end = at + page[at..].find("</tr>").unwrap();
        page[at..row_end].contains("shared-turn")
    };
    let (_, _, page) = send_as(&app, "GET", "/admin/grants", None, &session).await;
    assert!(!note(&page, &k), "one holder of turn on karl: {page}");
    assert!(!note(&page, &j));

    host_grant(s, &dot, "operator", Some(&k)).await;
    let (_, _, page) = send_as(&app, "GET", "/admin/grants", None, &session).await;
    assert!(note(&page, &k), "two holders of turn on karl: {page}");
    assert!(!note(&page, &j), "one on jane");
}
