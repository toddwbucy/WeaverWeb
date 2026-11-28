//! conforms: web-the-theme-is-chosen-in-the-masthead-and-drawn-by-the-server
//!
//! **The theme selector** (Spec 2.8; the operator's words of 2026-11-27,
//! "can you add a theme selector, perhaps a dropdown on the upper right
//! corner"; the Planner's brief of 2026-11-28, part 6). The masthead names
//! the design system's six themes and auto in one drop-down at its right.
//! The choice is kept per browser, in a cookie of its own, and **the server
//! draws it**: every page this tree serves leaves the server with
//! `data-theme` already on its root and the choice already selected, so no
//! page is ever drawn in one theme and then repainted in another. Auto is
//! no attribute at all, so the stylesheet's own rule holds: Slate, or Mist
//! where the browser prefers light (the design system's README). The
//! stylesheet and its tokens are untouched.
//!
//! **No script of its own.** The drop-down posts through htmx on change,
//! and the answer asks htmx to reload the page, which the server draws in
//! the new theme. Without htmx the form's button, drawn in `noscript`,
//! posts the same choice.
//!
//! **One seam over every page** rather than a field on each page's
//! struct: [`script_policy`](super::script_policy), which every surface's
//! router passes through, reads the cookie from the request and sets the
//! choice on an HTML answer's text. Only `instrument.html` opens `<html>`,
//! and only it draws the drop-down, so the two places the choice is set
//! are each found once.

use axum::Form;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;

/// The design system's six themes, Slate first as its default, then the
/// other darks and the lights in the README's order.
pub const THEMES: [&str; 6] = ["slate", "graphite", "umber", "chalk", "mist", "paper"];

/// The cookie that keeps a browser's choice. Like the session's, it is
/// `__Host-` and `Secure`, `HttpOnly` (no script reads it) and
/// `SameSite=Strict`; unlike the session's it names no person and carries
/// nothing but one of [`THEMES`].
pub const COOKIE: &str = "__Host-weaver_theme";

/// The cookie's attributes; a year, since a theme is a standing preference.
const ATTRIBUTES: &str = "Secure; HttpOnly; SameSite=Strict; Path=/";
const KEPT_SECS: u64 = 365 * 24 * 60 * 60;

/// The largest page this seam reads to set the choice on it. A page past
/// it is answered as a fault rather than cut.
const PAGE_BOUND: usize = 16 << 20;

/// **The browser's choice**, one of [`THEMES`], or none for auto: a
/// missing cookie, or one holding anything else, reads as auto.
pub fn chosen(headers: &HeaderMap) -> Option<&'static str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().strip_prefix(COOKIE)?.strip_prefix('='))
        .find_map(|value| THEMES.iter().copied().find(|theme| *theme == value))
}

/// **The choice set on a page's text**: `data-theme` on the root where a
/// theme is chosen, and the drop-down's option of the choice, auto where
/// none is, marked selected. A text without either is left as it is.
pub fn apply(page: &str, theme: Option<&str>) -> String {
    const ROOT: &str = r#"<html lang="en">"#;
    const SELECT: &str = r#"<select name="theme""#;
    let mut out = match theme {
        Some(theme) => page.replacen(
            ROOT,
            &format!(r#"<html lang="en" data-theme="{theme}">"#),
            1,
        ),
        None => page.to_owned(),
    };
    if let Some(start) = out.find(SELECT)
        && let Some(len) = out[start..].find("</select>")
    {
        let option = format!(r#"<option value="{}">"#, theme.unwrap_or("auto"));
        if let Some(at) = out[start..start + len].find(&option) {
            out.insert_str(start + at + option.len() - 1, " selected");
        }
    }
    out
}

/// **The layer's half of the seam**: the request's choice set on the
/// answer where it is a page. A fragment carries no root and no
/// drop-down, so it leaves as it came; a stream is not HTML.
pub(crate) async fn on_page(request: Request, next: Next) -> Response {
    let theme = chosen(request.headers());
    let response = next.run(request).await;
    let html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("text/html"));
    if !html {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let bytes = match axum::body::to_bytes(body, PAGE_BOUND).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("a page could not be read to set its theme: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let body = match std::str::from_utf8(&bytes) {
        Ok(page) => Body::from(apply(page, theme)),
        Err(_) => Body::from(bytes),
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    Response::from_parts(parts, body)
}

/// The drop-down's post.
#[derive(Deserialize)]
pub struct ThemeAsk {
    theme: String,
}

/// **`POST /theme`**: one of [`THEMES`] kept, or `auto` cleared, for
/// whoever asks, with a session or without, since it writes nothing but
/// the browser's own cookie. It passes the `Origin` check as every post
/// does. From htmx it answers 204 with `HX-Refresh`, so the page reloads
/// drawn in the choice; from the form alone, a redirect to `/`. Anything
/// else is refused, 400, and nothing is kept.
pub async fn set(headers: HeaderMap, Form(ask): Form<ThemeAsk>) -> Response {
    let cookie = if ask.theme == "auto" {
        format!("{COOKIE}=; {ATTRIBUTES}; Max-Age=0")
    } else if THEMES.contains(&ask.theme.as_str()) {
        format!("{COOKIE}={}; {ATTRIBUTES}; Max-Age={KEPT_SECS}", ask.theme)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            "a theme is auto or one of slate, graphite, umber, chalk, mist and paper",
        )
            .into_response();
    };
    let cookie = HeaderValue::from_str(&cookie).expect("a theme's cookie is ASCII");
    let mut answer = if headers.contains_key("hx-request") {
        let mut answer = StatusCode::NO_CONTENT.into_response();
        answer
            .headers_mut()
            .insert("hx-refresh", HeaderValue::from_static("true"));
        answer
    } else {
        Redirect::to("/").into_response()
    };
    answer.headers_mut().insert(header::SET_COOKIE, cookie);
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!doctype html>
<html lang="en">
<body><select name="other"><option value="auto">elsewhere</option><option value="graphite">elsewhere</option></select>
<select name="theme" hx-post="/theme"><option value="auto">Auto</option><option value="slate">Slate</option><option value="graphite">Graphite</option></select></body>"#;

    fn headers(cookie: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(cookie).unwrap());
        headers
    }

    /// **A browser's choice is read from its cookie alone**, among the
    /// others, and anything not one of the six reads as auto.
    #[test]
    fn the_choice_is_read_from_its_cookie_and_anything_else_is_auto() {
        assert_eq!(chosen(&HeaderMap::new()), None);
        assert_eq!(
            chosen(&headers(
                "__Host-weaver_session=abc; __Host-weaver_theme=umber"
            )),
            Some("umber")
        );
        assert_eq!(chosen(&headers("__Host-weaver_theme=paper")), Some("paper"));
        for odd in [
            "__Host-weaver_theme=Paper",
            "__Host-weaver_theme=auto",
            "__Host-weaver_theme=",
            "__Host-weaver_theme=\"><script>",
            "__Host-weaver_themex=paper",
            "weaver_theme=paper",
        ] {
            assert_eq!(chosen(&headers(odd)), None, "{odd}");
        }
    }

    /// **The choice is set on the page's text**: `data-theme` on the root
    /// and the choice's option selected in the theme's drop-down alone;
    /// auto sets no attribute and selects auto.
    ///
    /// Perturbation: select the option of that value in the page's first
    /// drop-down whatever its name, or set `data-theme` for auto, and the
    /// case fails.
    #[test]
    fn the_choice_is_set_on_the_root_and_the_drop_down() {
        let page = apply(PAGE, Some("graphite"));
        assert!(
            page.contains(r#"<html lang="en" data-theme="graphite">"#),
            "{page}"
        );
        assert!(
            page.contains(r#"<option value="graphite" selected>Graphite</option>"#),
            "{page}"
        );
        assert!(
            page.contains(r#"<option value="auto">Auto</option>"#),
            "{page}"
        );
        assert!(
            page.contains(r#"<option value="graphite">elsewhere</option>"#),
            "{page}"
        );
        assert_eq!(page.matches(" selected").count(), 1, "{page}");

        let page = apply(PAGE, None);
        assert!(
            page.contains(r#"<html lang="en">"#) && !page.contains("data-theme"),
            "{page}"
        );
        assert!(
            page.contains(r#"<option value="auto" selected>Auto</option>"#),
            "{page}"
        );
        assert_eq!(page.matches(" selected").count(), 1, "{page}");

        let fragment = "<p>no root and no drop-down</p>";
        assert_eq!(apply(fragment, Some("mist")), fragment);
    }
}
