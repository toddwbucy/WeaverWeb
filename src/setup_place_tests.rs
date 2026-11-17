//! conforms: web-a-context-length-above-the-headers-maximum-is-refused
//! conforms: web-a-context-length-below-the-boxs-max-tokens-per-turn-is-refused-and-the-tuple-carries-it
//! conforms: web-the-turn-context-is-required-and-never-defaulted
//! conforms: web-the-devices-are-a-non-empty-set-of-the-surveys-cards-by-ordinal
//! conforms: web-the-deployment-tuple-refuses-any-update-or-delete-and-a-setup-is-confirmed-once
//! conforms: web-the-members-the-box-is-laid-from-are-the-confirmed-tuples-and-none-stand-before-confirm
//! conforms: web-a-pages-way-forward-stands-where-its-condition-holds
//! conforms: web-every-setup-pane-carries-its-stage-backing-and-status
//!
//! The placement and confirm pages (Spec 6.2, `src/surfaces/setup/placement.rs`,
//! `src/surfaces/setup/confirm.rs`, `src/store/tuple.rs`), against the models
//! page's fake Hugging Face and a box whose reports each test sets through
//! the seam the enrollment's act fills. A context length above the
//! decoder's maximum or below the box's max-tokens-per-turn is refused, and
//! one at either bound kept. The turn context is
//! required and never drawn checked. The devices are held until the
//! survey's report brings the cards, then a non-empty set of them by
//! ordinal. Confirm stands only where every member is read, writes one
//! immutable tuple whose members are their sources', and the laying members
//! stand only once it does. Each DB-backed test runs on a store of its own.
//! No byte, name or address here is a real model's, card's or host's.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::http::{StatusCode, header};

use crate::admin_tests::{form, send_as};
use crate::box_report::{BoxMembers, BoxReports, Card, NoBox};
use crate::hugging_face::base_identity::BaseIdentities;
use crate::setup_models_tests::{
    COMMIT, count, ended, pick, post_pick, pull_of, rig_boxed, setup_of,
};
use crate::setup_tests::{Rig, audited, begin, page, walk};
use crate::store::setup::MAX_TOKENS_PER_TURN;
use crate::store::tuple::{Laying, Part};

/// What the test's box reports, set as each test goes.
#[derive(Default)]
struct Reported {
    survey: Mutex<Option<Vec<Card>>>,
    members: Mutex<Option<BoxMembers>>,
}

impl BoxReports for Reported {
    fn survey(&self, _setup_id: &str) -> Option<Vec<Card>> {
        self.survey.lock().unwrap().clone()
    }

    fn members(&self, _setup_id: &str) -> Option<BoxMembers> {
        self.members.lock().unwrap().clone()
    }
}

fn two_cards() -> Vec<Card> {
    vec![
        Card {
            ordinal: 0,
            identity: "card-a".into(),
        },
        Card {
            ordinal: 1,
            identity: "card-b".into(),
        },
    ]
}

fn box_members() -> BoxMembers {
    BoxMembers {
        image_version: "image-1".into(),
        driver: "driver-1".into(),
        cards: vec!["card-a".into(), "card-b".into()],
        guest_cpu: "cpu-1".into(),
        machine: "machine-1".into(),
        drives: vec!["the state drive".into(), "the models drive".into()],
    }
}

/// A page answered 200 under a session, framed, the apostrophe as askama
/// escapes it read back for the assertions.
async fn read(app: &axum::Router, uri: &str, session: &str) -> String {
    page(app, uri, session).await.replace("&#39;", "'")
}

/// A post to one of the setup's pages: the answer, its body and where it
/// leads.
async fn post(
    rig: &Rig,
    session: &str,
    setup: &str,
    page: &str,
    fields: &[(&str, &str)],
) -> (StatusCode, String, Option<String>) {
    let (status, headers, body) = send_as(
        &rig.app,
        "POST",
        &format!("/setup/{setup}/{page}"),
        Some(form(fields)),
        session,
    )
    .await;
    let location = headers
        .get(header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    (status, body, location)
}

/// The split model picked as the decoder at `version`.
async fn decoder_picked(rig: &Rig, session: &str, setup: &str, version: &str) {
    let (status, body) = post_pick(
        rig,
        session,
        setup,
        pick(
            version,
            "decoder",
            "org/split",
            "m-00001-of-00002.gguf",
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
}

/// **A context length above the decoder's maximum is refused, and one at
/// it is kept** (Spec 6.2): before a decoder is picked the field is held
/// and a context length posted is refused. Once one is, the field is
/// bounded by its header's maximum, 131072: a post without it, above it or
/// out of its shape is refused, saving nothing and recording nothing, and
/// one at it is kept.
#[tokio::test]
async fn a_context_length_above_the_headers_maximum_is_refused_and_one_at_it_kept() {
    let Some((rig, _fake, _, _cache)) = rig_boxed(true, Arc::new(NoBox)).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/placement");
    let before = read(&rig.app, &uri, &ada).await;
    assert!(
        before.contains("Waits on a decoder picked on the models page"),
        "{before}"
    );
    assert!(!before.contains(r#"name="context_length""#), "{before}");
    assert!(walk(&before).is_empty(), "{:?}", walk(&before));
    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", "1"),
            ("context_length", "4096"),
            ("turn_context", "continuous"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("waits on a decoder"), "{body}");

    decoder_picked(&rig, &ada, &setup, "1").await;
    let picked = read(&rig.app, &uri, &ada).await;
    assert!(
        picked.contains(
            r#"<input name="context_length" type="number" min="4096" max="131072" required>"#
        ),
        "{picked}"
    );
    assert!(walk(&picked).is_empty(), "{:?}", walk(&picked));
    for (fields, want, why) in [
        (
            vec![("version", "2"), ("turn_context", "continuous")],
            StatusCode::CONFLICT,
            "the context length is required",
        ),
        (
            vec![
                ("version", "2"),
                ("context_length", "131073"),
                ("turn_context", "continuous"),
            ],
            StatusCode::CONFLICT,
            "at most the decoder's maximum of 131072",
        ),
        (
            vec![
                ("version", "2"),
                ("context_length", "0"),
                ("turn_context", "continuous"),
            ],
            StatusCode::BAD_REQUEST,
            "a positive whole number",
        ),
        (
            vec![
                ("version", "2"),
                ("context_length", "4k"),
                ("turn_context", "continuous"),
            ],
            StatusCode::BAD_REQUEST,
            "a positive whole number",
        ),
    ] {
        let (status, body, _) = post(&rig, &ada, &setup, "placement", &fields).await;
        assert_eq!(status, want, "{fields:?}: {body}");
        assert!(body.contains(why), "{fields:?}: {body}");
    }
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!((kept.context_length, kept.version), (None, 2));
    assert_eq!(audited(s, "setup placement").await, (0, 0, 0, 0));

    let (status, body, location) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", "2"),
            ("context_length", "131072"),
            ("turn_context", "continuous"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(location.as_deref(), Some(uri.as_str()));
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!(
        (
            kept.context_length,
            kept.turn_context.as_deref(),
            kept.version
        ),
        (Some(131_072), Some("continuous"), 3)
    );
    assert_eq!(audited(s, "setup placement").await, (1, 1, 0, 0));
    let shown = read(&rig.app, &uri, &ada).await;
    assert!(shown.contains(r#"max="131072" value="131072""#), "{shown}");
    assert!(walk(&shown).is_empty(), "{:?}", walk(&shown));
}

/// **A context length below the box's max-tokens-per-turn is refused, and
/// the tuple carries that bound** (Spec 6.2, `toddwbucy/WeaverAgent#580`):
/// a capacity under the per-turn ceiling makes an agent whose turn can
/// exceed its whole context. The field's floor is the bound, 4096. One
/// below it is refused, saving nothing and recording nothing, and one at it
/// is kept. Where the decoder's maximum is below the bound, no length fits:
/// the page draws no field and says why, and every post is refused. A
/// length below the bound that reached the setup by another way is read as
/// no member on the confirm page. The tuple's own carrying of the bound is
/// in the confirm test.
#[tokio::test]
async fn a_context_length_below_the_boxs_max_tokens_per_turn_is_refused() {
    assert_eq!(MAX_TOKENS_PER_TURN, 4096, "create-agent.sh's value");
    let Some((rig, _fake, _, _cache)) = rig_boxed(true, Arc::new(NoBox)).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    decoder_picked(&rig, &ada, &setup, "1").await;
    let uri = format!("/setup/{setup}/placement");
    let picked = read(&rig.app, &uri, &ada).await;
    assert!(picked.contains(r#"min="4096" max="131072""#), "{picked}");
    assert!(picked.contains("the box's max-tokens-per-turn"), "{picked}");
    let at = |n: &'static str| {
        vec![
            ("version", "2"),
            ("context_length", n),
            ("turn_context", "continuous"),
        ]
    };
    let (status, body, _) = post(&rig, &ada, &setup, "placement", &at("4095")).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("at least the box's max-tokens-per-turn of 4096"),
        "{body}"
    );
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!((kept.context_length, kept.version), (None, 2));
    assert_eq!(audited(s, "setup placement").await, (0, 0, 0, 0));
    let (status, body, _) = post(&rig, &ada, &setup, "placement", &at("4096")).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(
        s.setup(&setup).await.unwrap().unwrap().context_length,
        Some(4096)
    );

    // A length below the bound kept by another way than the page is no
    // member on the confirm page.
    sqlx::query("UPDATE agent_setup SET context_length = 2048 WHERE setup_id = $1")
        .bind(&setup)
        .execute(&s.pool)
        .await
        .unwrap();
    let confirm = read(&rig.app, &format!("/setup/{setup}/confirm"), &ada).await;
    assert!(
        confirm.contains("the context length 2048 is below the box's max-tokens-per-turn of 4096"),
        "{confirm}"
    );

    // A decoder whose maximum is below the bound: no length fits.
    let pull = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &pull).await, "verified");
    let (_, _, short) = begin(&rig.app, &ada, "otto").await;
    let short = short.expect("a second setup");
    let (status, body) = post_pick(
        &rig,
        &ada,
        &short,
        pick("1", "decoder", "org/short", "s.gguf", None),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let none_fits = read(&rig.app, &format!("/setup/{short}/placement"), &ada).await;
    assert!(
        !none_fits.contains(r#"name="context_length""#),
        "{none_fits}"
    );
    assert!(none_fits.contains("no context length fits"), "{none_fits}");
    assert!(walk(&none_fits).is_empty(), "{:?}", walk(&none_fits));
    for fields in [
        vec![("version", "2"), ("turn_context", "continuous")],
        vec![
            ("version", "2"),
            ("context_length", "2048"),
            ("turn_context", "continuous"),
        ],
    ] {
        let (status, body, _) = post(&rig, &ada, &short, "placement", &fields).await;
        assert_eq!(status, StatusCode::CONFLICT, "{fields:?}: {body}");
        assert!(
            body.contains("no context length fits"),
            "{fields:?}: {body}"
        );
    }
    assert_eq!(audited(s, "setup placement").await, (1, 1, 0, 0));
}

/// **The turn context is required and never defaulted** (Spec 6.2).
/// Neither word is drawn checked on the first draw. A post without one,
/// with another word, with two, or with a field the page does not take is
/// refused, saving nothing and recording nothing. Either word is kept and
/// drawn checked, and a stale version answers 409.
#[tokio::test]
async fn the_turn_context_is_required_and_never_defaulted() {
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, Arc::new(NoBox)).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/placement");
    let first = read(&rig.app, &uri, &ada).await;
    for word in ["continuous", "independent"] {
        assert!(
            first.contains(&format!(
                r#"<input type="radio" name="turn_context" value="{word}" required>"#
            )),
            "{word}: {first}"
        );
    }
    for (fields, why) in [
        (
            vec![("version", "1")],
            "the turn context is continuous or independent",
        ),
        (
            vec![("version", "1"), ("turn_context", "")],
            "the turn context is continuous or independent",
        ),
        (
            vec![("version", "1"), ("turn_context", "both")],
            "the turn context is continuous or independent",
        ),
        (
            vec![
                ("version", "1"),
                ("turn_context", "continuous"),
                ("turn_context", "independent"),
            ],
            "once",
        ),
        (
            vec![
                ("version", "1"),
                ("turn_context", "continuous"),
                ("cpu", "on"),
            ],
            "takes no",
        ),
    ] {
        let (status, body, _) = post(&rig, &ada, &setup, "placement", &fields).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{fields:?}: {body}");
        assert!(body.contains(why), "{fields:?}: {body}");
    }
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!((kept.turn_context, kept.version), (None, 1));
    assert_eq!(audited(s, "setup placement").await, (0, 0, 0, 0));

    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[("version", "1"), ("turn_context", "independent")],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let shown = read(&rig.app, &uri, &ada).await;
    assert!(
        shown.contains(r#"name="turn_context" value="independent" required checked>"#),
        "{shown}"
    );
    assert!(
        shown.contains(r#"name="turn_context" value="continuous" required>"#),
        "{shown}"
    );
    assert!(walk(&shown).is_empty(), "{:?}", walk(&shown));
    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[("version", "1"), ("turn_context", "continuous")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("reload it"), "{body}");
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!(
        (kept.turn_context.as_deref(), kept.version),
        (Some("independent"), 2)
    );
    assert_eq!(audited(s, "setup placement").await, (2, 1, 1, 0));
}

/// **The devices and the fit are held until the survey reports** (Spec
/// 6.2): with no survey's report, the devices pane offers no card and says
/// what it waits on, the fit is drawn planned and held, and a device posted
/// is refused, saving nothing.
#[tokio::test]
async fn the_devices_and_the_fit_are_held_until_the_survey_reports() {
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, Arc::new(NoBox)).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let shown = read(&rig.app, &format!("/setup/{setup}/placement"), &ada).await;
    assert!(
        shown.contains("Waits on the survey's report, which brings the cards."),
        "{shown}"
    );
    assert!(!shown.contains(r#"name="device""#), "{shown}");
    assert!(
        shown.contains("planned, from the box's room plan"),
        "{shown}"
    );
    assert!(
        shown.contains("Waits on the room plan's answer over TLS"),
        "{shown}"
    );
    assert!(walk(&shown).is_empty(), "{:?}", walk(&shown));
    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", "1"),
            ("turn_context", "continuous"),
            ("device", "0"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("the devices wait on the survey's report"),
        "{body}"
    );
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!((kept.devices, kept.version), (None, 1));
    assert_eq!(audited(s, "setup placement").await, (0, 0, 0, 0));
}

/// **The devices are a non-empty set of the survey's cards, by ordinal**
/// (Spec 6.2), with a survey of two cards supplied through the seam: both
/// are offered by ordinal and identity, none checked. An empty set, an
/// ordinal the survey lacks, one given twice and one out of its shape (the
/// CPU among them) are each refused, saving nothing. The kept devices are
/// the checked ordinals, ascending.
#[tokio::test]
async fn the_devices_are_a_non_empty_set_of_the_surveys_cards_by_ordinal() {
    let reported = Arc::new(Reported::default());
    *reported.survey.lock().unwrap() = Some(two_cards());
    let Some((rig, _fake, _, _cache)) = rig_boxed(false, reported.clone()).await else {
        return;
    };
    let s = rig.store();
    let (ada, setup) = setup_of(&rig, "karl").await;
    let uri = format!("/setup/{setup}/placement");
    let offered = read(&rig.app, &uri, &ada).await;
    for (ordinal, identity) in [(0, "card-a"), (1, "card-b")] {
        assert!(
            offered.contains(&format!(
                r#"<label><input type="checkbox" name="device" value="{ordinal}"> Card {ordinal}: {identity}</label>"#
            )),
            "{ordinal}: {offered}"
        );
    }
    assert!(walk(&offered).is_empty(), "{:?}", walk(&offered));
    let turn = ("turn_context", "continuous");
    for (fields, want, why) in [
        (
            vec![("version", "1"), turn],
            StatusCode::CONFLICT,
            "a set of one card at least",
        ),
        (
            vec![("version", "1"), turn, ("device", "2")],
            StatusCode::CONFLICT,
            "card 2 is not among the survey's cards",
        ),
        (
            vec![("version", "1"), turn, ("device", "1"), ("device", "1")],
            StatusCode::BAD_REQUEST,
            "names card 1 once",
        ),
        (
            vec![("version", "1"), turn, ("device", "cpu")],
            StatusCode::BAD_REQUEST,
            "a card's ordinal",
        ),
        (
            vec![("version", "1"), turn, ("device", "-1")],
            StatusCode::BAD_REQUEST,
            "a card's ordinal",
        ),
    ] {
        let (status, body, _) = post(&rig, &ada, &setup, "placement", &fields).await;
        assert_eq!(status, want, "{fields:?}: {body}");
        assert!(body.contains(why), "{fields:?}: {body}");
    }
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!((kept.devices, kept.version), (None, 1));
    assert_eq!(audited(s, "setup placement").await, (0, 0, 0, 0));

    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[("version", "1"), turn, ("device", "1"), ("device", "0")],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let kept = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!(kept.devices, Some(vec![0, 1]), "ascending, the shard order");
    let shown = read(&rig.app, &uri, &ada).await;
    for ordinal in [0, 1] {
        assert!(
            shown.contains(&format!(r#"name="device" value="{ordinal}" checked>"#)),
            "{ordinal}: {shown}"
        );
    }
    assert!(walk(&shown).is_empty(), "{:?}", walk(&shown));
}

/// The base's identity of the split model, once its hash is kept, within
/// ten seconds.
async fn identity_kept(s: &crate::store::Store) -> String {
    for _ in 0..200 {
        if let Some(identity) = s
            .base_identity("org/split", COMMIT, "m-00001-of-00002.gguf")
            .await
            .unwrap()
        {
            return identity.blake3;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the base's identity is not kept");
}

/// **Confirm stands only where every member is read, and writes one
/// tuple, its members their sources'** (Spec 6.2). With nothing read the
/// page holds Confirm and offers no way forward. With every member read
/// but the box's report, Confirm still holds, naming it, and a post is
/// refused writing nothing, and no laying member stands. Once the box
/// reports, Confirm stands, and its post writes one tuple whose members are the
/// pulls' records, the base's identity row, the placement and the box's
/// report, and the laying members are the tuple's own. The tuple is drawn
/// from its row after, whatever the box reports. A second Confirm and any
/// write to the confirmed setup are refused, and the schema refuses an
/// update, a delete, a truncation and a second tuple. The model rows may be
/// truncated while no setup is confirmed, and never once one is.
#[tokio::test]
async fn confirm_stands_only_where_every_member_is_read_and_writes_one_tuple() {
    let reported = Arc::new(Reported::default());
    *reported.survey.lock().unwrap() = Some(two_cards());
    let Some((rig, _fake, _, _cache)) = rig_boxed(true, reported.clone()).await else {
        return;
    };
    let s = rig.store();
    let _identities = BaseIdentities::start(
        s.clone(),
        rig.puller.cache().cloned(),
        rig.puller.verified(),
    );
    let (ada, setup) = setup_of(&rig, "karl").await;
    let confirm = format!("/setup/{setup}/confirm");
    let action = format!(r#"<form method="post" action="{confirm}">"#);

    let held = read(&rig.app, &confirm, &ada).await;
    assert!(walk(&held).is_empty(), "{:?}", walk(&held));
    assert!(!held.contains(&action), "{held}");
    assert!(!held.contains("Next:"), "{held}");
    assert!(held.contains("no decoder is picked"), "{held}");

    decoder_picked(&rig, &ada, &setup, "1").await;
    let decoder_pull = pull_of(s, &setup, "decoder").await;
    assert_eq!(ended(s, &decoder_pull).await, "verified");
    let (status, body) = post_pick(
        &rig,
        &ada,
        &setup,
        pick("2", "adapter", "org/adapter", "adapter.gguf", Some("0.5")),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let adapter_pull = pull_of(s, &setup, "adapter").await;
    assert_eq!(ended(s, &adapter_pull).await, "verified");
    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[
            ("version", "3"),
            ("context_length", "4096"),
            ("turn_context", "independent"),
            ("device", "1"),
            ("device", "0"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    let blake3 = identity_kept(s).await;

    // While no setup is confirmed, the model rows may be truncated: here in
    // a transaction rolled back, so the setup keeps them.
    let models = count(s, "agent_setup_model").await;
    assert_eq!(models, 2);
    let mut tx = s.pool.begin().await.unwrap();
    sqlx::query("TRUNCATE agent_setup_model")
        .execute(&mut *tx)
        .await
        .expect("a truncation while no setup is confirmed");
    tx.rollback().await.unwrap();
    assert_eq!(count(s, "agent_setup_model").await, models);

    let waiting = read(&rig.app, &confirm, &ada).await;
    assert!(walk(&waiting).is_empty(), "{:?}", walk(&waiting));
    assert!(!waiting.contains(&action), "{waiting}");
    assert!(waiting.contains("Waits on the box's members"), "{waiting}");
    let (status, body, _) = post(&rig, &ada, &setup, "confirm", &[("version", "4")]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("the box's members"), "{body}");
    assert_eq!(count(s, "deployment_tuple").await, 0);
    let open = s.setup(&setup).await.unwrap().unwrap();
    assert_eq!((open.state.as_str(), open.version), ("open", 4));
    assert_eq!(audited(s, "setup confirm").await, (0, 0, 0, 0));
    assert_eq!(s.laying(&setup).await.unwrap(), None);

    *reported.members.lock().unwrap() = Some(box_members());
    let ready = read(&rig.app, &confirm, &ada).await;
    assert!(walk(&ready).is_empty(), "{:?}", walk(&ready));
    assert!(ready.contains(&action), "{ready}");
    assert!(!ready.contains("Next:"), "{ready}");
    assert_eq!(
        s.laying(&setup).await.unwrap(),
        None,
        "every member read, and none stands before Confirm"
    );
    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "confirm",
        &[("version", "4"), ("devices", "0")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let (status, body, location) = post(&rig, &ada, &setup, "confirm", &[("version", "4")]).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");
    assert_eq!(
        location.as_deref(),
        Some(format!("/setup/{setup}/build").as_str())
    );
    assert_eq!(audited(s, "setup confirm").await, (1, 1, 0, 0));
    assert_eq!(count(s, "deployment_tuple").await, 1);
    assert_eq!(s.setup(&setup).await.unwrap().unwrap().state, "confirmed");

    // Each member is its source's: the pulls' records in split order, the
    // base's identity row, the placement, the checkbox and the box. Every
    // part here is kept in LFS, so its blob is named by its SHA-256.
    let confirmed = s
        .deployment_tuple(&setup)
        .await
        .unwrap()
        .expect("the tuple stands");
    let tuple = &confirmed.tuple;
    let parts_of = |files: &[crate::store::model_pull::PullFile], repo: &str, paths: &[&str]| {
        paths
            .iter()
            .map(|path| {
                let f = files.iter().find(|f| f.file.path == *path).unwrap();
                Part {
                    path: format!("models--{repo}/snapshots/{COMMIT}/{path}"),
                    blob: f.file.sha256.clone().unwrap(),
                    sha256: f.file.sha256.clone().unwrap(),
                    bytes: f.file.size as i64,
                }
            })
            .collect::<Vec<_>>()
    };
    let decoder_files = s.pull_files(&decoder_pull).await.unwrap();
    let adapter_files = s.pull_files(&adapter_pull).await.unwrap();
    assert_eq!(
        tuple.decoder.parts,
        parts_of(
            &decoder_files,
            "org--split",
            &["m-00001-of-00002.gguf", "m-00002-of-00002.gguf"]
        )
    );
    assert_eq!(
        (
            tuple.decoder.repository.as_str(),
            tuple.decoder.commit.as_str(),
            tuple.decoder.choice.as_str(),
            tuple.decoder.pull_id.as_str()
        ),
        (
            "org/split",
            COMMIT,
            "m-00001-of-00002.gguf",
            decoder_pull.as_str()
        )
    );
    assert_eq!(
        tuple.adapter.parts,
        parts_of(&adapter_files, "org--adapter", &["adapter.gguf"])
    );
    assert_eq!(tuple.adapter.pull_id, adapter_pull);
    assert_eq!(tuple.adapter_scale, 0.5);
    assert_eq!(tuple.base_blake3, blake3);
    assert_eq!(
        (
            tuple.context_length,
            tuple.turn_context.as_str(),
            tuple.virtual_machine
        ),
        (4096, "independent", false)
    );
    assert_eq!(tuple.devices, vec![0, 1]);
    assert_eq!(tuple.max_tokens_per_turn, MAX_TOKENS_PER_TURN);
    assert_eq!(tuple.box_members, box_members());
    assert_eq!(confirmed.confirmed_by, "ada");
    assert!(crate::store::identity::is_tuple_id(&confirmed.tuple_id));

    let laying = s.laying(&setup).await.unwrap().expect("the laying stands");
    assert_eq!(
        laying,
        Laying {
            decoder_artifact: tuple.decoder.parts[0].path.clone(),
            adapter_artifact: tuple.adapter.parts[0].path.clone(),
            adapter_sha256: tuple.adapter.parts[0].sha256.clone(),
            adapter_scale: 0.5,
            base_blake3: blake3.clone(),
            devices: vec![0, 1],
            context_length: 4096,
            turn_context: "independent".into(),
        }
    );

    // Drawn from the tuple's row after Confirm, whatever the box reports.
    *reported.members.lock().unwrap() = None;
    let shown = read(&rig.app, &confirm, &ada).await;
    assert!(walk(&shown).is_empty(), "{:?}", walk(&shown));
    assert!(shown.contains(&confirmed.tuple_id), "{shown}");
    assert!(shown.contains("image-1"), "{shown}");
    assert!(!shown.contains(&action), "{shown}");
    assert!(
        shown.contains(&format!(
            r#"Next: <a href="/setup/{setup}/build">Build</a>"#
        )),
        "{shown}"
    );

    // A second Confirm, and any write to the confirmed setup, is refused.
    let (status, body, _) = post(&rig, &ada, &setup, "confirm", &[("version", "5")]).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("the setup is confirmed"), "{body}");
    let (status, body, _) = post(
        &rig,
        &ada,
        &setup,
        "placement",
        &[("version", "5"), ("turn_context", "continuous")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body.contains("the setup is confirmed"), "{body}");
    assert_eq!(audited(s, "setup confirm").await, (1, 1, 0, 0));
    assert_eq!(audited(s, "setup placement").await, (1, 1, 0, 0));

    // The schema refuses a change to the tuple or to the confirmed setup,
    // and a second tuple of the setup.
    for (sql, refusal) in [
        (
            "UPDATE deployment_tuple SET context_length = 1",
            "a deployment tuple never changes",
        ),
        (
            "DELETE FROM deployment_tuple",
            "a deployment tuple never changes",
        ),
        (
            "TRUNCATE deployment_tuple",
            "a deployment tuple never changes",
        ),
        (
            "UPDATE agent_setup SET context_length = 1",
            "a confirmed setup never changes",
        ),
        (
            "UPDATE agent_setup_model SET scale = 2 WHERE role = 'adapter'",
            "a confirmed setup never changes",
        ),
        // A truncation of the model rows fires no row trigger, so a
        // statement trigger refuses it while a setup is confirmed. The
        // setup's own is refused by the references to it, and, cascaded, by
        // the tuple's guard (CodeRabbit on #129).
        (
            "TRUNCATE agent_setup_model",
            "a confirmed setup never changes",
        ),
        (
            "TRUNCATE agent_setup",
            "cannot truncate a table referenced in a foreign key constraint",
        ),
        ("TRUNCATE agent_setup CASCADE", "never changes"),
        (
            "INSERT INTO deployment_tuple SELECT (jsonb_populate_record(NULL::deployment_tuple, \
             to_jsonb(t) || '{\"tuple_id\": \"dt-0000000000000000\"}')).* FROM deployment_tuple t",
            "deployment_tuple_one_per_setup",
        ),
        (
            "INSERT INTO deployment_tuple SELECT (jsonb_populate_record(NULL::deployment_tuple, \
             to_jsonb(t) || '{\"tuple_id\": \"dt-0000000000000001\", \"context_length\": 4095}')).* \
             FROM deployment_tuple t",
            "deployment_tuple_a_context_holds_a_turn",
        ),
    ] {
        let refused = sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&s.pool)
            .await
            .expect_err(sql);
        assert!(refused.to_string().contains(refusal), "{sql}: {refused}");
    }
    assert_eq!(count(s, "deployment_tuple").await, 1);
    assert_eq!(count(s, "agent_setup_model").await, models);
}
