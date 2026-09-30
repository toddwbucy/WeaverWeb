//! conforms: web-a-chip-filters-only-on-an-indexed-column
//!
//! Record, per the charter's section 3.6: **every run and branch with the
//! tuple that produced it.** A reading without its tuple is a reading of an
//! unnamed compound, and a registry that cannot hold a failure is a
//! marketing surface, so this surface shows what ran and says what it does
//! not know rather than omitting it.
//!
//! **It reads the store and nothing else**, per `weaver-web-Spec` section 6,
//! through the fifth read of section 4 and no other query. It writes
//! nothing: a run lands by the ingest of section 3.1 alone.
//!
//! **A chip is a query rather than a location.** Clearing one widens the
//! list where the operator stands and nothing navigates, so what is not
//! being seen is always visible, per the charter's section 3.6. Every chip
//! this surface offers names a column section 2.7 indexes, which is the
//! assertion this file's header cites: a chip on an unindexed column would
//! be a sequential scan offered as though it were cheap.

use askama::Template;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use super::gate;
use crate::store::{Chip, Cursor, RunId, RunTuple, Store};

/// How many runs a page holds. **A list is walked rather than quoted**, per
/// section 4's fifth read, so this is a bound on the page and not on what
/// the filter admits.
const PAGE: u32 = 50;

pub fn routes() -> Router<Store> {
    Router::new().route("/record", get(record))
}

/// The query this surface takes, which is its whole state: a chip and a
/// place in the walk. **There is no state that is a location**, per section
/// 6, so a link carrying no chip is the widest list rather than a different
/// page.
#[derive(Debug, Default, Deserialize)]
pub struct Ask {
    /// The chip's kind, one of `artifact`, `session` or `branches`.
    chip: Option<String>,
    /// The chip's value.
    of: Option<String>,
    /// The page's key, the ingest's order and the run's identity together,
    /// spelled as this surface last handed it out.
    after_at: Option<String>,
    after_run: Option<String>,
}

impl Ask {
    /// The chip the ask carries, or a refusal.
    ///
    /// **A chip that does not resolve refuses rather than widening**, for
    /// the same reason the cursor below does. A kind with no value, a value
    /// with no kind, or a kind this surface does not offer would otherwise
    /// return the widest list under a URL that says it is narrowed - and
    /// widening is the one failure an operator reading section 3.6's chip
    /// row cannot see, because the row honestly reports the chip that is in
    /// force rather than the chip that was asked for.
    fn chip(&self) -> Result<Option<Chip>, Refusal> {
        let (kind, of) = match (self.chip.as_deref(), self.of.as_deref()) {
            (None, None) => return Ok(None),
            (Some(kind), Some(of)) => (kind, of),
            _ => {
                return Err(Refusal(
                    "a chip is a kind and a value together, and one came alone",
                ));
            }
        };
        match kind {
            "artifact" => Ok(Some(Chip::RecordIdentity(of.to_string()))),
            "session" => Ok(Some(Chip::Session(of.to_string()))),
            "branches" => Ok(Some(Chip::Branches(RunId(of.to_string())))),
            _ => Err(Refusal(
                "this surface chips on a record identity, a session or a parent's branches",
            )),
        }
    }

    /// The cursor the ask carries, or a refusal.
    ///
    /// **A cursor that does not resolve refuses rather than resetting.** A
    /// silent fall back to the first page renders the newest runs under a
    /// URL that says otherwise, and a reader cannot tell a reset from an
    /// end, which is how a broken walk presents as a working one.
    fn cursor(&self) -> Result<Option<Cursor>, Refusal> {
        match (self.after_at.as_deref(), self.after_run.as_deref()) {
            (None, None) => Ok(None),
            (Some(at), Some(run)) => match at.parse() {
                Ok(ingested_at) => Ok(Some(Cursor {
                    ingested_at,
                    run: RunId(run.to_string()),
                })),
                Err(_) => Err(Refusal("the cursor's time does not read as a timestamp")),
            },
            _ => Err(Refusal(
                "a cursor is a time and a run together, and one came alone",
            )),
        }
    }
}

/// One row as the surface draws it. **Every member says what it is or says
/// it is absent**, per Spec section 6: a member the record did not carry
/// renders as absent and never as a zero or an empty string, which is the
/// absent-not-empty rule at the view.
pub struct Row {
    pub run: String,
    /// What a reader sees for the record identity. **The sentinel is a fact
    /// and is named**, per Spec section 2.3: an empty identity is a hash the
    /// SPU could not compute.
    pub record_identity: String,
    /// The value a chip on this row would carry, which is the identity as
    /// the record spells it. **It is not the display string**: a chip
    /// carrying the words a surface chose would match no row at all.
    pub record_identity_key: Option<String>,
    pub device: String,
    /// The engine's libraries by name, as the tuple carries them. **A column
    /// headed `engine` that rendered a constant would say neither what the
    /// member is nor that it is absent**, which is the whole of what a
    /// tuple is for.
    pub engine: String,
    pub seed: String,
    /// The record's session, kept as an option so the view decides absence
    /// from the member rather than from a word it compares against.
    pub session: Option<String>,
    pub parent: Option<String>,
    pub branch_position: Option<i32>,
    pub parting_position: Option<i32>,
    pub ingested_at: String,
}

/// The word a surface uses where the record carried nothing. **It is a word
/// and not a blank**, so a reader can tell a member nobody recorded from one
/// this surface failed to draw.
const ABSENT: &str = "absent";

/// **An empty string is an absence too, and the schema does not forbid it.**
/// `run.device` and `run.seed` are nullable text with no `<> ''` check, so a
/// member the record sent empty reaches here and would draw a blank cell -
/// the one rendering section 6 forbids, arriving through the member the rule
/// was broadened to cover.
fn absent_or(value: Option<&str>) -> String {
    match value {
        Some(v) if !v.trim().is_empty() => v.to_owned(),
        _ => ABSENT.to_string(),
    }
}

/// The engine's libraries, drawn by name. **The member is a JSON object and
/// a table cell is a line**, so the names are joined and the digests are
/// left to the run's own surface, which is a rendering choice rather than a
/// member discarded: a reader sees which libraries a run went through and
/// two runs through different ones read differently.
fn engine_libraries(engine: Option<&serde_json::Value>) -> String {
    // **JSON null is not a member the record carried.** The column is
    // nullable JSONB and nothing forbids the value `null` in it, which is
    // the gap between two meanings of the word that migration 0009 spells
    // out for the plan's entries. Read as carried, it would draw "recorded,
    // unnamed" for a member nobody sent.
    let Some(engine) = engine.filter(|e| !e.is_null()) else {
        return ABSENT.to_string();
    };
    let names: Vec<&str> = engine
        .as_object()
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    if names.is_empty() {
        // A member the record carried and this surface cannot name is not
        // an absent member, so it says which it is.
        "recorded, unnamed".to_string()
    } else {
        names.join(", ")
    }
}

impl From<RunTuple> for Row {
    fn from(t: RunTuple) -> Self {
        let sentinel = t.record_identity.is_empty();
        Row {
            run: t.run.0,
            record_identity: if sentinel {
                // Spec 2.3: the sentinel is the empty string and means a
                // hash the SPU could not compute. It is a fact of the
                // record rather than a missing member, so it is named.
                "the hash failed".to_string()
            } else {
                t.record_identity.clone()
            },
            // A chip on the sentinel would ask for runs whose identity is
            // the empty string, which is a real question, but the link is
            // suppressed rather than carrying a word no row holds.
            record_identity_key: (!sentinel).then_some(t.record_identity),
            device: absent_or(t.device.as_deref()),
            engine: engine_libraries(t.engine.as_ref()),
            seed: absent_or(t.seed.as_deref()),
            session: t.record_session,
            parent: t.parent_run.map(|p| p.0),
            branch_position: t.branch_position,
            parting_position: t.parting_position,
            // **Not `to_rfc3339`**: that spells the offset `+00:00`, and a
            // query string decodes `+` as a space, so the cursor this
            // surface hands out would never parse when it came back. `Z` is
            // the same instant and survives the round trip.
            ingested_at: t
                .ingested_at
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        }
    }
}

#[derive(Template)]
#[template(path = "record.html")]
struct RecordPage {
    here: &'static str,
    /// The name the session claimed, shown so a reader knows which claim
    /// this page was drawn under. **It is not a proof of anything.**
    who: String,
    rows: Vec<Row>,
    /// The chip in force, as a word a reader can see and a link can clear.
    chip_kind: Option<String>,
    chip_of: Option<String>,
    /// Where the next page resumes, absent at the end of what the filter
    /// admits.
    next_at: Option<String>,
    next_run: Option<String>,
}

/// **The answer is a page, a refusal, or a fault**, and the three are
/// different things: a refusal says the ask was malformed and a fault says
/// this surface could not serve a well-formed one.
async fn record(
    State(store): State<Store>,
    headers: HeaderMap,
    Query(ask): Query<Ask>,
) -> Result<Response, Response> {
    // **The gate is the surface's own argument**, per `surfaces/mod.rs`: a
    // surface reads the store and nothing else, and a surface holding a
    // seam takes it rather than widening the state. A router carrying the
    // store alone would leave nowhere for this to stand, which is how every
    // run's tuple came to be served to anyone who reached the listener.
    //
    // **The claim is a claim**, per Spec section 2.8, so what this refuses
    // is a request that named nobody. Until the identity act of the
    // charter's section 6 lands, per issue #336, that is the shape standing
    // and not access control.
    let who = gate::claim(&store, &headers)
        .await
        .map_err(|e| Failure::from(e).into_response())?;
    let Some(who) = who else {
        return Err(NoSession.into_response());
    };
    let chip = ask.chip().map_err(IntoResponse::into_response)?;
    let cursor = ask.cursor().map_err(IntoResponse::into_response)?;
    let page = store
        .runs(chip.as_ref(), PAGE, cursor.as_ref())
        .await
        .map_err(|e| Failure::from(e).into_response())?;
    let next = page.next;
    let html = RecordPage {
        here: "record",
        who: who.name,
        rows: page.runs.into_iter().map(Row::from).collect(),
        // Both halves are present or the ask refused above, so these
        // carry the chip in force rather than the chip that was asked for.
        chip_kind: ask.chip.clone(),
        chip_of: ask.of.clone(),
        next_at: next.as_ref().map(|c| {
            c.ingested_at
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
        }),
        next_run: next.map(|c| c.run.0),
    }
    .render()
    .map_err(|e| Failure::from(e).into_response())?;
    Ok(Html(html).into_response())
}

/// A request that named nobody. **The answer says what is missing rather
/// than what is forbidden**, because nothing here is access control: a
/// session is opened by claiming a name, and this request claimed none.
pub struct NoSession;

impl IntoResponse for NoSession {
    fn into_response(self) -> Response {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            "this surface is read under a session. Open one and ask again.",
        )
            .into_response()
    }
}

/// What this surface refuses, and why, in words a reader can act on. **A
/// refusal is not a fault**: the ask was malformed, so the answer names
/// what was wrong with it rather than logging a chain nobody sees.
pub struct Refusal(&'static str);

impl IntoResponse for Refusal {
    fn into_response(self) -> Response {
        (axum::http::StatusCode::BAD_REQUEST, self.0).into_response()
    }
}

/// A fault this surface met, as opposed to an ask it refused. **The chain
/// goes to the log and the response stays generic**, so a query's text, a
/// path, or an upstream's detail never reaches a browser.
///
/// **It is the crate's one error type and not a second copy of it.** The
/// posture is `crate::fault::Fault`'s, which outlives both the conversation
/// half and this one, so a change to the logging or the body text reaches
/// every surface rather than one of two copies.
pub type Failure = crate::fault::Fault;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    /// Open a session and hand back the bearer a browser would hold.
    ///
    /// **The row is brought to this seed rather than left as it was found.**
    /// A watch below asserts the page names the claim it was drawn under, so
    /// a session a previous run opened under another name would be the row
    /// the assertion measured.
    async fn a_session(store: &Store, name: &str, role: &str) -> String {
        let bearer = format!("bearer-{name}-{role}");
        sqlx::query(
            "INSERT INTO session (bearer_digest, claimed_name, role) VALUES ($1, $2, $3) \
             ON CONFLICT (bearer_digest) DO UPDATE SET claimed_name = EXCLUDED.claimed_name, \
             role = EXCLUDED.role, closed_at = NULL",
        )
        .bind(gate::digest(&bearer))
        .bind(name)
        .bind(role)
        .execute(&store.pool)
        .await
        .unwrap();
        bearer
    }

    async fn ask(store: &Store, uri: &str, bearer: Option<&str>) -> (StatusCode, String) {
        let mut request = Request::builder().uri(uri);
        if let Some(bearer) = bearer {
            request = request.header("cookie", format!("weaver_session={bearer}"));
        }
        let response = routes()
            .with_state(store.clone())
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    /// **A request that names nobody is not served**, per Spec section 2.8
    /// read at the gate, and one that names a claim is.
    ///
    /// conforms: web-session-carries-a-claim-and-never-a-proof
    #[tokio::test]
    async fn record_is_read_under_a_session_and_never_without_one() {
        let Some(store) = crate::store::read::tests::store().await else {
            return;
        };
        let (status, body) = ask(&store, "/record", None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "no session, no page");
        assert!(!body.contains("<table"), "and no run's tuple in the body");

        let bearer = a_session(&store, "todd", "user").await;
        let (status, body) = ask(&store, "/record", Some(&bearer)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(body.contains("todd"), "the claim the page was drawn under");
    }

    /// The surface draws a run's tuple, **names an absent member rather
    /// than drawing a blank**, and narrows on a chip.
    ///
    /// The absence is asserted on a row this watch can isolate: the run is
    /// reached under a chip that admits only it, so a regression in one
    /// member's handling cannot be carried by another row on the page.
    ///
    /// **It cites the claim it pins and not the file's own header.** The
    /// header's claim is tagged `review` at `weaver-web-Spec` section 9,
    /// because the half of it that matters - *indexed* - is a property of
    /// the statement and the schema rather than of a response, and nothing
    /// asserted here would fail if a chip filtered on an unindexed column.
    /// What this watch pins is absent-not-empty at the view, which section 6
    /// now records rather than only stating.
    ///
    /// conforms: web-the-view-names-an-absent-member
    #[tokio::test]
    async fn record_draws_the_tuple_and_names_what_is_absent() {
        let Some(store) = crate::store::read::tests::store().await else {
            return;
        };
        let bearer = a_session(&store, "todd", "user").await;
        // **The row and the session it is chipped by are this run's alone.**
        // The assertion below counts the rows on the page, so anything else
        // carrying this session - a row a previous run retained, or one a
        // concurrent run is writing - is counted as though the surface drew
        // it. A fresh tag is what makes "one header row and one run" a
        // statement about this watch rather than about the database.
        let tag = format!(
            "surf-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let run = format!("{tag}-a");
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, sampler, boundary_set, \
             record_session, device, engine) \
             VALUES ($1, 'SURF-REC', '{}', '[]', $2, 'rtx-a6000', \
             '{\"cutlass\": \"3.5\"}')",
        )
        .bind(&run)
        .bind(&tag)
        .execute(&store.pool)
        .await
        .unwrap();

        // The chip admits this row and no other, so every assertion below
        // is about this run.
        let (status, html) = ask(
            &store,
            &format!("/record?chip=session&of={tag}"),
            Some(&bearer),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            html.matches("<tr>").count(),
            2,
            "one header row and one run"
        );
        assert!(html.contains(&run), "the run is drawn");
        assert!(html.contains("rtx-a6000"), "its device is drawn");
        // **The device is the member section 9's row names**, and the watch
        // pinned only the seed until the review of PR #556: the seed was the
        // page's one null, so blanking the device changed nothing here.
        // A second run, alike but for the members it did not carry.
        sqlx::query(
            "INSERT INTO run (run_id, record_identity, sampler, boundary_set, record_session) \
             VALUES ($1, 'SURF-REC', '{}', '[]', $2)",
        )
        .bind(format!("{tag}-b"))
        .bind(&tag)
        .execute(&store.pool)
        .await
        .unwrap();
        let (_, html) = ask(
            &store,
            &format!("/record?chip=session&of={tag}"),
            Some(&bearer),
        )
        .await;
        assert_eq!(
            html.matches(ABSENT).count(),
            // The second run carries neither device, engine nor seed, and
            // the first carries no seed: four absences and not three.
            4,
            "every member neither run carried is named: {html:.900}"
        );
        assert!(
            html.contains("cutlass"),
            "the engine names its libraries rather than a constant: {html:.600}"
        );
        // This run's seed was never recorded, and it is the only run on the
        // page, so the word can only have come from its cell.
        assert!(html.contains("absent"), "an absent member is named");

        // A session no run carries, for the same reason: a fixed name could
        // be one some other row happens to hold.
        let (_, html) = ask(
            &store,
            &format!("/record?chip=session&of={tag}-nobody"),
            Some(&bearer),
        )
        .await;
        assert!(!html.contains(&run), "the chip narrowed the list");
        assert!(
            html.contains("No run answers this"),
            "and says nothing answers"
        );
    }

    /// **A chip that does not resolve refuses rather than widening.**
    ///
    /// Perturbation: return `None` for a half pair or an unknown kind and
    /// each of these three returns 200 with the unfiltered list, which is
    /// the widest answer served under a URL that asks for a narrow one.
    #[tokio::test]
    async fn a_chip_that_does_not_resolve_refuses_rather_than_widening() {
        let Some(store) = crate::store::read::tests::store().await else {
            return;
        };
        let bearer = a_session(&store, "todd", "user").await;
        for uri in [
            "/record?chip=session",
            "/record?of=a-value-with-no-kind",
            "/record?chip=nonesuch&of=x",
        ] {
            let (status, body) = ask(&store, uri, Some(&bearer)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{uri} is refused");
            assert!(!body.contains("<table"), "{uri} draws no list");
        }
        // And the shape it refuses for is the only one it refuses for: the
        // widest list carries no chip at all and is not an error.
        let (status, _) = ask(&store, "/record", Some(&bearer)).await;
        assert_eq!(status, StatusCode::OK, "no chip is the widest list");
    }

    /// **A cursor that does not resolve refuses rather than resetting**, so
    /// a broken walk cannot present as a working page.
    #[tokio::test]
    async fn a_half_cursor_refuses_and_a_whole_one_survives_the_round_trip() {
        let Some(store) = crate::store::read::tests::store().await else {
            return;
        };
        let bearer = a_session(&store, "todd", "user").await;

        let (status, _) = ask(
            &store,
            "/record?after_at=2026-09-11T00:00:00Z",
            Some(&bearer),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "a time with no run refuses"
        );
        let (status, _) = ask(
            &store,
            "/record?after_at=nonsense&after_run=x",
            Some(&bearer),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "a time that does not read refuses"
        );

        // The cursor this surface hands out has to survive being handed
        // back. `to_rfc3339` spells the offset `+00:00`, and a query string
        // decodes `+` as a space, so the round trip is the watch.
        let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
        let spelled = now.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);
        assert!(
            !spelled.contains('+'),
            "the cursor carries no plus: {spelled}"
        );
        let uri = format!("/record?after_at={spelled}&after_run=any");
        let (status, _) = ask(&store, &uri, Some(&bearer)).await;
        assert_eq!(status, StatusCode::OK, "and reads back as a cursor: {uri}");
    }
}
