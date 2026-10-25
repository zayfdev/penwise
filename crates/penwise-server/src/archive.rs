//! Server-side archives: the undo a client cannot reach once its own file has moved on.

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};

use crate::AppState;
use crate::AuthQuery;
use crate::auth;
use crate::library::Anchor;

/// `GET /pages/<id>/history` — the server's archives of that page, newest first.
pub(crate) async fn page_history(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.history(&page_id) {
        Ok(entries) => axum::Json(serde_json::json!({
            "page": page_id,
            "archives": entries
                .into_iter()
                .map(|(stamp, objects)| serde_json::json!({ "at": stamp, "objects": objects }))
                .collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(error) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

/// `POST /pages/<id>/restore` — puts an archive back; body `{"at"?}` (default: the newest).
///
/// The server's undo for anything a client did destructively: a clear, an erase, a page replace that dropped
/// ink. The tablet's own undo stack cannot help once its file has moved on.
pub(crate) async fn page_restore(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    body: Option<axum::Json<RestoreRequest>>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let at = body.and_then(|axum::Json(request)| request.at);
    match app.library.restore(&page_id, at).await {
        Ok(Some(objects)) => axum::Json(serde_json::json!({ "restored": objects })).into_response(),
        Ok(None) => (
            axum::http::StatusCode::NOT_FOUND,
            "no archive for that page",
        )
            .into_response(),
        Err(error) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct RestoreRequest {
    /// Archive timestamp from `GET /pages/<id>/history`; the newest archive when absent.
    #[serde(default)]
    pub at: Option<i64>,
}

/// `GET /pages/<id>/anchors` — the element map of a captured page.
pub(crate) async fn anchor_list(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.anchors(&page_id) {
        Ok(anchors) => {
            axum::Json(serde_json::json!({ "page": page_id, "anchors": anchors })).into_response()
        }
        Err(error) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

/// `POST /pages/<id>/anchors` — replaces the element map: `{"anchors":[{selector, rect, role?, text?, url?}]}`.
pub(crate) async fn anchor_put(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<AnchorRequest>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.put_anchors(&page_id, request.anchors).await {
        Ok(stored) => axum::Json(serde_json::json!({ "stored": stored })).into_response(),
        Err(error) if error.starts_with("unknown page") => {
            (axum::http::StatusCode::NOT_FOUND, error).into_response()
        }
        Err(error) => (axum::http::StatusCode::BAD_REQUEST, error).into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct AnchorRequest {
    #[serde(default)]
    pub anchors: Vec<Anchor>,
}
