//! The `/inspect/*` debug routes, mounted behind the server token by `api::router`.

use std::path::PathBuf;

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;

use super::{ElementInfo, Error};
use crate::api::{ApiError, AppState};

/// Viewport used by the debug routes when `w`/`h` are absent.
const DEFAULT_WIDTH: u32 = 1440;
const DEFAULT_HEIGHT: u32 = 900;

/// `GET /inspect/element` and `GET /inspect/snapshot`, behind the server token.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/inspect/element", get(element_route))
        .route("/inspect/snapshot", get(snapshot_route))
}

#[derive(Debug, Deserialize)]
struct ElementQuery {
    url: String,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    /// Debug convenience: inspect by selector instead of a point.
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    w: Option<u32>,
    #[serde(default)]
    h: Option<u32>,
    /// Project root for `file:line` mapping; without it, source entries carry URLs only.
    #[serde(default)]
    root: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct SnapshotQuery {
    url: String,
    #[serde(default)]
    w: Option<u32>,
    #[serde(default)]
    h: Option<u32>,
}

async fn element_route(
    State(app): State<AppState>,
    Query(query): Query<ElementQuery>,
) -> Result<Json<ElementInfo>, ApiError> {
    let mut page = app
        .inspect
        .open(
            &query.url,
            query.w.unwrap_or(DEFAULT_WIDTH),
            query.h.unwrap_or(DEFAULT_HEIGHT),
        )
        .await
        .map_err(api_error)?;
    page.set_root(query.root);
    let outcome = if let Some(selector) = query.selector.as_deref() {
        app.inspect.element(&page, selector).await
    } else {
        match (query.x, query.y) {
            (Some(x), Some(y)) => app.inspect.element_at(&page, x, y).await,
            _ => Err(Error::Invalid("pass x and y, or selector".to_owned())),
        }
    };
    page.close().await;
    outcome.map(Json).map_err(api_error)
}

async fn snapshot_route(
    State(app): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> Response {
    let outcome = snapshot_page(&app, &query).await;
    match outcome {
        Ok(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            png,
        )
            .into_response(),
        Err(error) => api_error(error).into_response(),
    }
}

async fn snapshot_page(app: &AppState, query: &SnapshotQuery) -> Result<Vec<u8>, Error> {
    let page = app
        .inspect
        .open(
            &query.url,
            query.w.unwrap_or(DEFAULT_WIDTH),
            query.h.unwrap_or(DEFAULT_HEIGHT),
        )
        .await?;
    let png = app.inspect.snapshot(&page, None).await;
    page.close().await;
    png
}

fn api_error(error: Error) -> ApiError {
    match error {
        Error::Invalid(message) => ApiError::bad_request(message),
        Error::NoElement { x, y } => ApiError::not_found(format!("no element at ({x}, {y})")),
        Error::NotFound(selector) => ApiError::not_found(format!("no element matches {selector}")),
        other => ApiError::new(StatusCode::BAD_GATEWAY, other.to_string()),
    }
}
