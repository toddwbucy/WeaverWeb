//! conforms: web-a-context-length-above-the-headers-maximum-is-refused
//! conforms: web-the-turn-context-is-required-and-never-defaulted
//! conforms: web-the-devices-are-a-non-empty-set-of-the-surveys-cards-by-ordinal
//!
//! **The placement page** (Spec 6.2), a child of the setup pages' module.
//! `GET /setup/{setup}/placement` draws five things:
//!
//! - the context length, bounded below by the box's max-tokens-per-turn and
//!   above by the decoder's header, held where no maximum was read, and
//!   drawn as no fit where the maximum is below the floor
//! - the turn context, neither word checked until one is kept
//! - the devices, the survey's cards by ordinal, held until the survey's
//!   report brings them
//! - the model's layers, from the decoder's header and pick
//! - another shard order and the fit, unbuilt
//!
//! **The placement's save** (`POST /setup/{setup}/placement`) takes the
//! admin writes' order (Spec 2.13):
//!
//! 1. The form is parsed to its shape. A field the page does not take, a
//!    field given twice, a context length out of its shape, a missing or
//!    unknown turn context and a device out of its shape or given twice are
//!    each refused, saving nothing.
//! 2. The admin gate writes its one refusal record.
//! 3. The setup must stand, open, and **each field is held to its source**:
//!    the context length to the box's max-tokens-per-turn and the decoder's
//!    maximum, and the devices to the survey's cards. A field the page holds is refused where it is posted.
//!    Each refusal records nothing.
//! 4. The audited write of the three, a held field none.

use askama::Template;
use axum::Router;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;

use super::{Frame, MALFORMED_SETUP, Page, Seams, admin_read, frame, landed};
use crate::gguf::Header;
use crate::store::Store;
use crate::store::admin::Refusal;
use crate::store::audit::Target;
use crate::store::identity::is_setup_id;
use crate::store::setup::{MAX_TOKENS_PER_TURN, Placement, SetupModel};
use crate::surfaces::admin::{fault, first_of, refused, settled, writer};

/// The save's action, as its audit records name it.
const ACTION: &str = "setup placement";

/// The fields the placement's form carries: each once, but the devices,
/// one field per card checked.
const FIELDS: [&str; 4] = ["version", "context_length", "turn_context", "device"];

/// The two turn contexts.
const TURN_CONTEXTS: [&str; 2] = ["continuous", "independent"];

pub(super) fn routes(seams: &Seams) -> Router<Store> {
    let page_seams = seams.clone();
    let save_seams = seams.clone();
    Router::new().route(
        "/setup/{setup}/placement",
        get(
            move |State(store): State<Store>, headers: HeaderMap, Path(setup): Path<String>| {
                let seams = page_seams.clone();
                async move { page(store, &seams, headers, &setup).await }
            },
        )
        .post(
            move |State(store): State<Store>,
                  headers: HeaderMap,
                  Path(setup): Path<String>,
                  Form(fields): Form<Vec<(String, String)>>| {
                let seams = save_seams.clone();
                async move { save(store, &seams, headers, &setup, fields).await }
            },
        ),
    )
}

/// **The decoder's maximum context**: none where no decoder is picked,
/// and where one is, the header's maximum, or none where it was not read.
fn maximum(rows: &[SetupModel]) -> Option<Option<u64>> {
    rows.iter().find(|r| r.role == "decoder").map(|r| {
        serde_json::from_value::<Header>(r.header.clone())
            .ok()
            .and_then(|h| h.context_length)
    })
}

/// **Why no context length fits**, where none does: the decoder's maximum
/// is below the box's max-tokens-per-turn, the field's floor.
fn none_fits(max: u64) -> Option<String> {
    let floor = MAX_TOKENS_PER_TURN;
    u64::try_from(floor).is_ok_and(|f| max < f).then(|| {
        format!(
            "no context length fits: the decoder's maximum context of {max} is below the box's max-tokens-per-turn of {floor}"
        )
    })
}

// ---- the page ----

/// The context length's pane.
struct ContextView {
    /// The field's floor, the box's max-tokens-per-turn.
    min: i64,
    /// The decoder's maximum, where the field is live.
    max: Option<u64>,
    /// Why no length fits, where the maximum is below the floor.
    none_fits: Option<String>,
    kept: Option<i64>,
    /// The kept length is above the decoder's maximum, after a pick.
    above: bool,
    /// What the held field waits on.
    waits: &'static str,
    status: &'static str,
}

/// A card of the survey's report, as the devices pane offers it.
struct CardView {
    ordinal: i32,
    identity: String,
    checked: bool,
}

/// The model's layers, from the decoder's header and pick.
struct LayersView {
    blocks: String,
    parts: Vec<String>,
}

#[derive(Template)]
#[template(path = "setup_placement.html")]
struct PlacementPage {
    here: &'static str,
    who: String,
    admin: bool,
    chrome: crate::surfaces::Chrome,
    frame: Frame,
    version: i64,
    open: bool,
    context: ContextView,
    /// The turn context kept, and empty where none is.
    turn: String,
    turn_contexts: [&'static str; 2],
    /// The survey's cards, where the survey has reported.
    cards: Option<Vec<CardView>>,
    layers: Option<LayersView>,
}

async fn page(store: Store, seams: &Seams, headers: HeaderMap, setup_id: &str) -> Response {
    let (session, setup) = match admin_read(&store, seams, &headers, setup_id).await {
        Ok(read) => read,
        Err(answer) => return answer,
    };
    let (chrome, rows) = match async {
        Ok::<_, anyhow::Error>((
            crate::surfaces::Chrome::of(&store, &session.person_id).await?,
            store.setup_models(setup_id).await?,
        ))
    }
    .await
    {
        Ok(read) => read,
        Err(e) => return fault(e),
    };
    let layers = match rows.iter().find(|r| r.role == "decoder") {
        None => None,
        Some(decoder) => {
            let files = match store.pull_files(&decoder.pull_id).await {
                Ok(files) => files,
                Err(e) => return fault(e),
            };
            Some(LayersView {
                blocks: serde_json::from_value::<Header>(decoder.header.clone())
                    .ok()
                    .and_then(|h| h.block_count)
                    .map_or_else(|| "not read".to_owned(), |n| n.to_string()),
                parts: files.into_iter().map(|f| f.file.path).collect(),
            })
        }
    };
    let context = match maximum(&rows) {
        None => ContextView {
            min: MAX_TOKENS_PER_TURN,
            max: None,
            none_fits: None,
            kept: None,
            above: false,
            waits: "a decoder picked on the models page",
            status: "hold",
        },
        Some(None) => ContextView {
            min: MAX_TOKENS_PER_TURN,
            max: None,
            none_fits: None,
            kept: None,
            above: false,
            waits: "the decoder's maximum context, which its header does not carry",
            status: "hold",
        },
        Some(Some(max)) if none_fits(max).is_some() => ContextView {
            min: MAX_TOKENS_PER_TURN,
            max: None,
            none_fits: none_fits(max),
            kept: None,
            above: false,
            waits: "",
            status: "fail",
        },
        Some(Some(max)) => {
            let above = setup
                .context_length
                .is_some_and(|n| u64::try_from(n).is_ok_and(|n| n > max));
            ContextView {
                min: MAX_TOKENS_PER_TURN,
                max: Some(max),
                none_fits: None,
                kept: setup.context_length,
                above,
                waits: "",
                status: if above { "fail" } else { "ok" },
            }
        }
    };
    let kept = setup.devices.clone().unwrap_or_default();
    let cards = seams.boxes.survey(setup_id).map(|cards| {
        cards
            .into_iter()
            .map(|c| CardView {
                checked: kept.contains(&c.ordinal),
                ordinal: c.ordinal,
                identity: c.identity,
            })
            .collect()
    });
    let rendered = PlacementPage {
        here: "declarations",
        who: session.name.clone(),
        admin: true,
        chrome,
        frame: frame(&setup, Page::Placement),
        version: setup.version,
        open: setup.state == "open",
        context,
        turn: setup.turn_context.clone().unwrap_or_default(),
        turn_contexts: TURN_CONTEXTS,
        cards,
        layers,
    }
    .render();
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => fault(e),
    }
}

// ---- the save ----

/// The placement's form, parsed to its shape.
#[derive(Debug, PartialEq)]
struct PlaceAsk {
    version: i64,
    context_length: Option<i64>,
    turn_context: &'static str,
    /// The cards checked, by ordinal, ascending: the shard order.
    devices: Vec<i32>,
}

/// **The placement's form parsed to its shape**, before anything is looked
/// up: each field once but the devices, the context length a positive whole
/// number where it is given, the turn context one of the two words, and
/// each device a card's ordinal, none twice.
fn place_ask(fields: &[(String, String)]) -> Result<PlaceAsk, String> {
    for (at, (name, _)) in fields.iter().enumerate() {
        if !FIELDS.contains(&name.as_str()) {
            return Err(format!(
                "the placement takes no {name:?}: its fields are the context length, the turn context and the devices, the survey's cards by ordinal"
            ));
        }
        if name != "device" && fields[..at].iter().any(|(earlier, _)| earlier == name) {
            return Err(format!("the placement names its {name} once"));
        }
    }
    let field = |name: &str| {
        fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.trim())
    };
    let version: i64 = field("version")
        .and_then(|v| v.parse().ok())
        .filter(|v| *v >= 1)
        .ok_or("a setup's version is 1 at least")?;
    let context_length = match field("context_length") {
        None | Some("") => None,
        Some(text) => Some(
            text.parse::<i64>()
                .ok()
                .filter(|n| *n >= 1)
                .ok_or("the context length is a positive whole number")?,
        ),
    };
    let turn_context = TURN_CONTEXTS
        .into_iter()
        .find(|word| field("turn_context") == Some(*word))
        .ok_or("the turn context is continuous or independent, and the post names neither")?;
    let mut devices: Vec<i32> = Vec::new();
    for (_, text) in fields.iter().filter(|(n, _)| n == "device") {
        let ordinal = text
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|n| *n >= 0)
            .ok_or_else(|| {
                format!("a device is a card's ordinal, a whole number from 0, and {text:?} is not")
            })?;
        if devices.contains(&ordinal) {
            return Err(format!("the placement names card {ordinal} once"));
        }
        devices.push(ordinal);
    }
    devices.sort_unstable();
    Ok(PlaceAsk {
        version,
        context_length,
        turn_context,
        devices,
    })
}

/// **Each field held to its source**, or why the placement is not kept:
/// the context length to the decoder's maximum, live only where one was
/// read, and the devices to the survey's cards, live only where the survey
/// has reported.
fn held_to_sources(
    ask: &PlaceAsk,
    maximum: Option<Option<u64>>,
    survey: Option<&[crate::box_report::Card]>,
) -> Result<(), String> {
    let floor = MAX_TOKENS_PER_TURN;
    match (ask.context_length, maximum) {
        (Some(_), None) => {
            return Err("the context length waits on a decoder picked on the models page".into());
        }
        (Some(_), Some(None)) => {
            return Err(
                "the context length waits on the decoder's maximum context, which its header does not carry"
                    .into(),
            );
        }
        (_, Some(Some(max))) if none_fits(max).is_some() => {
            return Err(none_fits(max).unwrap_or_default());
        }
        (None, Some(Some(max))) => {
            return Err(format!(
                "the context length is required: a whole number from {floor}, the box's max-tokens-per-turn, to the decoder's maximum of {max}"
            ));
        }
        (Some(n), Some(Some(_))) if n < floor => {
            return Err(format!(
                "the context length is at least the box's max-tokens-per-turn of {floor}, and {n} is below it"
            ));
        }
        (Some(n), Some(Some(max))) if u64::try_from(n).is_ok_and(|n| n > max) => {
            return Err(format!(
                "the context length is at most the decoder's maximum of {max}, and {n} is above it"
            ));
        }
        _ => {}
    }
    match survey {
        None if !ask.devices.is_empty() => {
            Err("the devices wait on the survey's report, which brings the cards".into())
        }
        None => Ok(()),
        Some(_) if ask.devices.is_empty() => Err(
            "the devices are a set of one card at least, since the agent refuses an empty set rather than default to a device"
                .into(),
        ),
        Some(cards) => match ask
            .devices
            .iter()
            .find(|d| !cards.iter().any(|c| c.ordinal == **d))
        {
            Some(lacking) => Err(format!("card {lacking} is not among the survey's cards")),
            None => Ok(()),
        },
    }
}

async fn save(
    store: Store,
    seams: &Seams,
    headers: HeaderMap,
    setup_id: &str,
    fields: Vec<(String, String)>,
) -> Response {
    if !is_setup_id(setup_id) {
        return (StatusCode::BAD_REQUEST, MALFORMED_SETUP).into_response();
    }
    let ask = match place_ask(&fields) {
        Ok(ask) => ask,
        Err(why) => return (StatusCode::BAD_REQUEST, why).into_response(),
    };
    let target = Target::Setup(setup_id);
    let session = match writer(&store, &seams.policy, &headers, target, ACTION).await {
        Ok(session) => session,
        Err(answer) => return answer,
    };
    match store.setup(setup_id).await {
        Ok(Some(setup)) if setup.state == "open" => {}
        Ok(Some(_)) => return refused(&Refusal::SetupConfirmed),
        Ok(None) => return refused(&Refusal::NoSuchSetup),
        Err(e) => return fault(e),
    }
    let rows = match store.setup_models(setup_id).await {
        Ok(rows) => rows,
        Err(e) => return fault(e),
    };
    let survey = seams.boxes.survey(setup_id);
    if let Err(why) = held_to_sources(&ask, maximum(&rows), survey.as_deref()) {
        return refused(&Refusal::Unplaceable(why));
    }
    let first = match first_of(&store, &session, target, ACTION).await {
        Ok(first) => first,
        Err(answer) => return answer,
    };
    let placement = Placement {
        context_length: ask.context_length,
        turn_context: ask.turn_context,
        devices: (!ask.devices.is_empty()).then_some(ask.devices.as_slice()),
    };
    let written = store
        .setup_placement(
            &session.person_id,
            session.session_id,
            &first,
            setup_id,
            ask.version,
            &placement,
        )
        .await;
    settled(&store, &first, &written).await;
    landed(written, setup_id, Page::Placement)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::box_report::Card;

    fn fields(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// **The devices are kept ascending**, the shard order, whatever order
    /// the form posts them in, and an empty context length is none.
    #[test]
    fn the_devices_are_kept_ascending() {
        let ask = place_ask(&fields(&[
            ("version", "3"),
            ("context_length", ""),
            ("turn_context", "independent"),
            ("device", "2"),
            ("device", "0"),
            ("device", "1"),
        ]))
        .unwrap();
        assert_eq!(
            ask,
            PlaceAsk {
                version: 3,
                context_length: None,
                turn_context: "independent",
                devices: vec![0, 1, 2],
            }
        );
    }

    /// **Each field is held to its source**: a context length live only
    /// where a maximum was read, from the box's max-tokens-per-turn to the
    /// maximum, and none fitting a maximum below it, and the devices live only
    /// where the survey reported, a non-empty set of its cards.
    #[test]
    fn each_field_is_held_to_its_source() {
        let ask = |context_length, devices: &[i32]| PlaceAsk {
            version: 1,
            context_length,
            turn_context: "continuous",
            devices: devices.to_vec(),
        };
        let cards = [Card {
            ordinal: 3,
            identity: "card".into(),
        }];
        let held = |context_length, maximum| {
            held_to_sources(&ask(context_length, &[]), maximum, None).is_ok()
        };
        assert!(held(None, None));
        assert!(!held(Some(4096), None));
        assert!(!held(Some(4096), Some(None)));
        assert!(!held(None, Some(Some(8192))));
        assert!(!held(Some(8193), Some(Some(8192))));
        assert!(held(Some(8192), Some(Some(8192))));
        assert!(!held(Some(4095), Some(Some(8192))));
        assert!(held(Some(4096), Some(Some(8192))));
        // A maximum below the floor: no length fits, none posted included.
        assert!(!held(None, Some(Some(2048))));
        assert!(!held(Some(2048), Some(Some(2048))));
        assert!(held_to_sources(&ask(None, &[3]), None, None).is_err());
        assert!(held_to_sources(&ask(None, &[]), None, Some(&cards)).is_err());
        assert!(held_to_sources(&ask(None, &[2]), None, Some(&cards)).is_err());
        assert!(held_to_sources(&ask(None, &[3]), None, Some(&cards)).is_ok());
    }
}
