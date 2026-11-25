//! The instrument's surfaces, one module per surface of the charter's
//! section 3, per `weaver-web-Spec` section 1.
//!
//! Every route and asset a browser reaches is one of this tree's modules;
//! the server's binary merges them into one router and lays the `Origin`
//! check over it.
//!
//! **None of them writes the recorded half**, per Spec section 6: a position
//! and a run land by the ingest of section 3.1 alone.

pub mod admin;
pub mod admin_table;
pub mod agents;
pub mod alerts;
pub mod declarations;
pub mod declare_pane;
#[cfg(feature = "passkeys")]
pub mod enroll;
pub mod gate;
#[cfg(test)]
pub(crate) mod gate_tests;
pub mod grants;
pub mod hugging_face;
#[cfg(feature = "passkeys")]
pub mod keys;
pub mod reading;
pub mod record;
pub mod setup;
#[cfg(feature = "passkeys")]
pub mod sign_in;
pub mod ssh_key;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};

use crate::store::Store;

/// **What every signed-in page carries above its body** (Spec 2.14): the
/// person's open alerts across the agents they hold a grant on, for the
/// nav, and a banner for each plane of those agents the server cannot
/// reach, both read from the store at the request, so a banner shows while
/// its plane is down and is gone at the next page after it reconnects. A
/// page without a session carries none.
#[derive(Debug, Default)]
pub(crate) struct Chrome {
    pub(crate) open_alerts: i64,
    pub(crate) banners: Vec<Banner>,
}

/// **The tab a page belongs to** (Spec 2.8; the operator's words of
/// 2026-11-13), by the sub-tab the page declares as its `here`: `agents`
/// for New agent, Declarations, Register and Your agents, `operations` for The Record,
/// `iam` for Persons, Grants, Hugging Face, SSH keys and Passkeys. A sub-tab belongs to one tab, so
/// this one table names it and no page names both. A page of no tab, the
/// sign-in and enrollment pages, has none.
pub(crate) fn tab_of(here: &str) -> &'static str {
    match here {
        "declarations" | "pair" | "register" | "agents" => "agents",
        "record" => "operations",
        "admin" | "grants" | "hugging-face" | "ssh-key" | "passkeys" => "iam",
        _ => "",
    }
}

/// One unavailable plane, as its banner says it.
#[derive(Debug)]
pub(crate) struct Banner {
    pub(crate) agent_id: String,
    pub(crate) label: String,
    pub(crate) says: &'static str,
    pub(crate) since: String,
}

impl Chrome {
    /// The person's chrome, scoped by their grants as the alerts are.
    pub(crate) async fn of(store: &Store, person: &str) -> anyhow::Result<Self> {
        let open_alerts = store
            .open_alerts_by_agent(person)
            .await?
            .iter()
            .map(|(_, n)| n)
            .sum();
        let banners = store
            .unavailable_for(person)
            .await?
            .into_iter()
            .map(|u| Banner {
                says: if u.plane == "admin" {
                    "agent unavailable, admin-con down"
                } else {
                    "turns unavailable, gate-con down"
                },
                since: u
                    .since
                    .map(|at| format!("since {}", at.format("%Y-%m-%d %H:%M:%S UTC")))
                    .unwrap_or_else(|| "never connected".to_owned()),
                agent_id: u.agent_id,
                label: u.label,
            })
            .collect();
        Ok(Self {
            open_alerts,
            banners,
        })
    }
}

/// Every surface this tree serves, mounted on the store and nothing else.
///
/// **The state is the store because a surface that renders what is kept
/// reads the store and nothing else**, per Spec section 6. A surface that
/// holds a seam takes it as its own argument rather than widening this:
/// the session's policy is that argument here.
pub fn routes(policy: gate::Policy) -> Router<Store> {
    let landing_policy = policy.clone();
    let router = Router::new()
        .route(
            "/",
            get(move |State(store): State<Store>, headers: HeaderMap| {
                let policy = landing_policy.clone();
                async move { landing(store, policy, headers).await }
            }),
        )
        .merge(record::routes(policy))
        .route("/sign-out", post(gate::sign_out))
        .route("/assets/surfaces/instrument.css", get(stylesheet))
        .route("/assets/surfaces/htmx.min.js", get(htmx))
        .route("/assets/surfaces/trace.js", get(trace_module));
    #[cfg(feature = "passkeys")]
    let router = router.route("/assets/surfaces/passkeys.js", get(passkeys_module));
    script_policy(router)
}

/// **`GET /` sends the browser on** (Spec 2.8): to sign-in where the request
/// names no session that serves, and to The Record, where every sign-in lands,
/// where it names one. It reads the session as any use does, and is a
/// redirect (`303 See Other`), never a page of its own.
async fn landing(store: Store, policy: gate::Policy, headers: HeaderMap) -> Response {
    match gate::session(&store, &policy, &headers).await {
        Ok(Some(_)) => Redirect::to("/record").into_response(),
        Ok(None) => Redirect::to("/sign-in").into_response(),
        Err(e) => crate::fault::Fault::from(e).into_response(),
    }
}

/// **A script runs from this server's own files alone**: the surfaces answer
/// with `Content-Security-Policy: script-src 'self'`, which they can carry
/// since none has an inline script, an inline handler or an `eval`, and the
/// passkey module and the trace module are files of this binary's. **htmx
/// holds under it too**, measured on 2026-10-08 in Chromium 153 and Firefox
/// 156: the vendored htmx's `hx-get` and `hx-post` swaps ran with no
/// violation reported, while a control page's inline script was blocked and
/// reported. What would not hold is htmx's evaluating attributes (`hx-on`,
/// `js:` values), which no surface may use while this header stands.
pub fn script_policy<S: Clone + Send + Sync + 'static>(router: Router<S>) -> Router<S> {
    router.layer(axum::middleware::map_response(
        |mut response: Response| async move {
            response.headers_mut().insert(
                header::CONTENT_SECURITY_POLICY,
                HeaderValue::from_static("script-src 'self'"),
            );
            response
        },
    ))
}

/// **The passkey ceremonies' browser module** (design section 4), one
/// hand-written file vendored into the binary.
#[cfg(feature = "passkeys")]
async fn passkeys_module() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../assets/surfaces/passkeys.js"),
    )
}

/// **The Live trace pane's browser module** (Spec 2.13), one hand-written
/// file vendored into the binary: it reads the stream the agent page's
/// `data-stream` names and places each event's words in the pane.
async fn trace_module() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../../assets/surfaces/trace.js"),
    )
}

/// **The vendored htmx, served from the surfaces' own route** (Spec 2.14):
/// the availability fragments are re-fetched by its `hx-trigger`, no
/// inline script, under the script policy above.
async fn htmx() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_bytes!("../../assets/htmx.min.js").as_slice(),
    )
}

/// The instrument's stylesheet, served from this binary.
async fn stylesheet() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../assets/surfaces/instrument.css"),
    )
}

/// **What every served page carries** (the pane grammar,
/// `docs/design/system/components/Workspace`): the masthead, the workspace
/// and the stylesheet. Each test that renders a page asserts it of that page.
#[cfg(test)]
pub(crate) fn assert_framed(page: &str) {
    for mark in [
        r#"<nav class="masthead">"#,
        r#"<main class="workspace"#,
        r#"<link rel="stylesheet" href="/assets/surfaces/instrument.css">"#,
    ] {
        assert!(page.contains(mark), "a served page carries {mark}: {page}");
    }
}

/// **The input types the stylesheet draws** (the operator's walk of
/// 2026-11-19). A field is `text` or `password`, the two types the field
/// rules draw on `inset`; every other input is `hidden`, `checkbox` or
/// `radio`, which carry no typed text.
#[cfg(test)]
pub(crate) const FIELD_TYPES: [&str; 2] = ["text", "password"];
#[cfg(test)]
pub(crate) const OTHER_TYPES: [&str; 3] = ["hidden", "checkbox", "radio"];

/// The inputs in `text` whose type the stylesheet does not draw: no
/// `type`, or one outside [`FIELD_TYPES`] and [`OTHER_TYPES`].
#[cfg(test)]
pub(crate) fn undrawn_inputs(text: &str) -> Vec<&str> {
    input_tags(text)
        .into_iter()
        .filter(|tag| {
            attribute(tag, "type")
                .is_none_or(|t| !FIELD_TYPES.contains(&t) && !OTHER_TYPES.contains(&t))
        })
        .collect()
}

/// **Every input a served page carries is one the stylesheet draws.** Each
/// end-to-end test that reads a setup page asserts it of that page. Those
/// tests stand under the `passkeys` and `enrollment` features, and so does
/// this.
#[cfg(all(test, feature = "passkeys", feature = "enrollment"))]
pub(crate) fn assert_inputs_drawn(page: &str) {
    let undrawn = undrawn_inputs(page);
    assert!(
        undrawn.is_empty(),
        "inputs the stylesheet does not draw: {undrawn:?}"
    );
}

/// The `<input ...>` tags of a template, each from `<input` to the `>`
/// that closes it outside an askama block, expression or comment.
#[cfg(test)]
pub(crate) fn input_tags(text: &str) -> Vec<&str> {
    let mut tags = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("<input") {
        let tag = &rest[at..];
        if !tag["<input".len()..].starts_with(|c: char| c.is_whitespace() || c == '>') {
            rest = &tag["<input".len()..];
            continue;
        }
        let bytes = tag.as_bytes();
        let mut i = 0;
        let mut end = None;
        while i < bytes.len() {
            if bytes[i] == b'{' && matches!(bytes.get(i + 1), Some(b'%' | b'{' | b'#')) {
                let close = match bytes[i + 1] {
                    b'%' => "%}",
                    b'{' => "}}",
                    _ => "#}",
                };
                let Some(j) = tag[i + 2..].find(close) else {
                    break;
                };
                i += 2 + j + 2;
                continue;
            }
            if bytes[i] == b'>' {
                end = Some(i);
                break;
            }
            i += 1;
        }
        let end = end.expect("an input tag closes");
        tags.push(&tag[..=end]);
        rest = &tag[end + 1..];
    }
    tags
}

/// An attribute's value in a tag, where its name follows whitespace, so
/// `type` is never read out of `inputmode`.
#[cfg(test)]
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let mut from = 0;
    while let Some(at) = tag[from..].find(&key) {
        let at = from + at;
        if tag[..at].ends_with(char::is_whitespace) {
            let value = &tag[at + key.len()..];
            return value.find('"').map(|end| &value[..end]);
        }
        from = at + key.len();
    }
    None
}

#[cfg(test)]
mod tests {
    /// **The served stylesheet is the design system's, byte for byte**:
    /// `tokens.css`, then `bundle.css` without its import line, and nothing
    /// else, so the copy served cannot drift from the copy that is the
    /// record.
    #[test]
    fn the_stylesheet_is_the_design_systems() {
        let tokens = include_str!("../../docs/design/system/tokens.css");
        let bundle = include_str!("../../docs/design/system/components/bundle.css");
        let derived: String = std::iter::once(tokens)
            .chain(
                bundle
                    .split_inclusive('\n')
                    .filter(|l| !l.starts_with("@import ")),
            )
            .collect();
        assert!(
            derived == include_str!("../../assets/surfaces/instrument.css"),
            "assets/surfaces/instrument.css has drifted from the design system; regenerate it \
             from the repository's root with: {{ cat docs/design/system/tokens.css; sed \
             '/^@import /d' docs/design/system/components/bundle.css; }} > \
             assets/surfaces/instrument.css"
        );
    }

    /// **Every input a template draws carries a type the stylesheet draws**
    /// (the operator's walk of 2026-11-19: inputs with no `type`, and one of
    /// `type="number"`, missed the stylesheet's field rule, so the browser's
    /// white field met Slate's near-white text and nothing typed could be
    /// read). The stylesheet is the design system's and does not change, so
    /// the rule is held in the templates. A field is `text` or `password`, the
    /// two types the field rules draw on `inset`. Every other input is
    /// `hidden`, `checkbox` or `radio`, which carry no typed text. An input
    /// with no type, or of any other type, fails here.
    #[test]
    fn every_input_carries_a_type_the_stylesheet_draws() {
        use super::{FIELD_TYPES, input_tags, undrawn_inputs};
        let css = include_str!("../../assets/surfaces/instrument.css");
        for field in FIELD_TYPES {
            let selector = format!("input[type={field}]");
            assert!(
                css.lines().any(|rule| {
                    rule.split_once('{').is_some_and(|(selectors, body)| {
                        selectors.split(',').any(|s| s.trim() == selector)
                            && body.contains("background: var(--inset)")
                    })
                }),
                "the stylesheet's field rule names {selector} on the inset background"
            );
        }
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/surfaces/templates");
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .expect("the templates' directory")
            .map(|entry| entry.expect("a template's entry").path())
            .filter(|path| path.extension().is_some_and(|x| x == "html"))
            .collect();
        paths.sort();
        let mut inputs = 0;
        let mut missed = Vec::new();
        for path in &paths {
            let text = std::fs::read_to_string(path).expect("a template");
            inputs += input_tags(&text).len();
            for tag in undrawn_inputs(&text) {
                missed.push(format!(
                    "{}: {tag}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        assert!(inputs > 0, "the templates draw inputs");
        assert!(
            missed.is_empty(),
            "inputs the stylesheet does not draw:\n{}",
            missed.join("\n")
        );
    }
}
