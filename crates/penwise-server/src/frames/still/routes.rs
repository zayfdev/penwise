//! The still routes. They sit in the project group (token, or a same-machine request), beside the agent routes.

use axum::Json;
use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use penwise_core::item::ItemId;
use serde::Deserialize;

use crate::api::AppState;

use super::StillError;
use super::model::{etag_matches, is_hash};

/// Strips are content-addressed: the same URL is the same bytes forever.
const STRIP_CACHE: &str = "private, max-age=31536000, immutable";
/// Furthest a hit point may be from the origin, in CSS px (the frame side limit).
const MAX_HIT: f32 = 1_048_576.0;

/// `GET /frames/<id>/still`, `GET /frames/<id>/strip/<hash>.png`, `GET /frames/<id>/hit`.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/frames/{id}/still", get(still))
        .route("/frames/{id}/strip/{file}", get(strip))
        .route("/frames/{id}/hit", get(hit))
}

/// A failed still request: the status of its cause, and `Retry-After` when trying again later can help.
struct Failure(StatusCode, String, Option<u64>);

impl From<StillError> for Failure {
    fn from(error: StillError) -> Self {
        let (status, retry) = match &error {
            StillError::UnknownFrame => (StatusCode::NOT_FOUND, None),
            StillError::Storage(_) | StillError::Write(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, None)
            }
            StillError::Unavailable { retry_after, .. } => {
                (StatusCode::SERVICE_UNAVAILABLE, Some(*retry_after))
            }
            StillError::Agent(_) | StillError::Inspect(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Some(super::slots::FAILURE_TTL.as_secs()),
            ),
        };
        Self(status, error.to_string(), retry)
    }
}

impl Failure {
    fn bad_request(message: &str) -> Self {
        Self(StatusCode::BAD_REQUEST, message.to_owned(), None)
    }
}

impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        let mut response = (self.0, self.1).into_response();
        if let Some(seconds) = self.2
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

fn frame_id(text: &str) -> Result<ItemId, Failure> {
    ItemId::from_hex(text).ok_or_else(|| Failure::bad_request("frame id is 32 hex characters"))
}

/// The still's JSON, versioned by `ETag`. Clients poll it every 3 s for every visible non-live frame; an
/// unchanged frame answers `304` with no body. A frame whose capture failed moments ago answers `503` with
/// `Retry-After` and is not retried before that.
async fn still(
    State(app): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, Failure> {
    let frame = frame_id(&id)?;
    let still = app.stills.current(frame).await?;
    let etag = still.etag();
    let mut response = if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| etag_matches(value, &etag))
    {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        Json(still.json(&frame.to_hex())).into_response()
    };
    let out = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&etag) {
        out.insert(header::ETAG, value);
    }
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    Ok(response)
}

/// `GET /frames/<id>/strip/<hash>.png` query.
#[derive(Debug, Deserialize)]
struct StripQuery {
    /// Target width in px; snapped up to 360, 720 or 1440, and never above the strip's own width.
    w: Option<u32>,
}

/// One strip by content address, optionally downscaled. The hash is the whole identity: a changed page has new
/// hashes, an unchanged strip keeps its URL across versions.
async fn strip(
    State(app): State<AppState>,
    Path((id, file)): Path<(String, String)>,
    Query(query): Query<StripQuery>,
) -> Result<Response, Failure> {
    let frame = frame_id(&id)?;
    let hash = file
        .strip_suffix(".png")
        .filter(|hash| is_hash(hash))
        .ok_or_else(|| Failure::bad_request("a strip is <32 hex characters>.png"))?;
    let png = app
        .stills
        .strip(frame, hash, query.w)
        .await?
        .ok_or_else(|| Failure(StatusCode::NOT_FOUND, "no such strip".to_owned(), None))?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, STRIP_CACHE),
        ],
        png,
    )
        .into_response())
}

/// `GET /frames/<id>/hit?x&y` query: a point in document coordinates.
#[derive(Debug, Deserialize)]
struct HitQuery {
    x: f32,
    y: f32,
}

/// The element under document point `(x, y)`: the bridge's `pw:hit` payload `{key, selector, rect}`, with `rect`
/// in **document** coordinates (clients convert to what they need). All three are `null` when nothing is there.
/// Answered from the still's hit map; only a frame with no still yet opens a browser page.
async fn hit(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<HitQuery>,
) -> Result<Json<serde_json::Value>, Failure> {
    let frame = frame_id(&id)?;
    if !(query.x.is_finite() && query.y.is_finite())
        || query.x.abs() > MAX_HIT
        || query.y.abs() > MAX_HIT
    {
        return Err(Failure::bad_request("x and y are finite document px"));
    }
    Ok(Json(app.stills.hit(frame, [query.x, query.y]).await?))
}
