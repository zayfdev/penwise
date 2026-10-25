//! Picture routes: upload a PNG as an asset, place it on a page, list it, remove it.
//!
//! Pictures are what make the canvas a design surface — a webpage screenshot to annotate, a component cut out of
//! a running app. They are content-addressed files (`assets/<sha256>.png`); page objects only reference them.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::AppState;
use crate::AuthQuery;
use crate::auth;
use crate::library;

/// `GET /page.png`'s pictures, with their bytes loaded once.
pub(crate) async fn load_images(app: &AppState, page: &str) -> Vec<(library::ImageMsg, Vec<u8>)> {
    let mut loaded = Vec::new();
    for image in app.library.images(page).await {
        if let Some(bytes) = app.assets.bytes(&image.asset) {
            loaded.push((image, bytes));
        } else {
            tracing::warn!(asset = %image.asset, page, "picture asset missing");
        }
    }
    loaded
}

/// `POST /assets` — body is a PNG. Returns `{"asset","width","height"}`; the digest is the picture's identity.
pub(crate) async fn asset_upload(
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.assets.put(&body) {
        Ok(stored) => axum::Json(stored).into_response(),
        Err(error) => (axum::http::StatusCode::BAD_REQUEST, error).into_response(),
    }
}

/// `POST /pages/<id>/images` — body `{"asset","x","y","w","h","label"?,"id"?,"layer"?}`.
///
/// The picture is placed in page px: the extension sends a screenshot sized to the page, and components are
/// laid out where the caller wants them.
pub(crate) async fn image_place(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<library::ImageMsg>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    if app.library.page(&page_id).await.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "unknown page").into_response();
    }
    if app.assets.path(&request.asset).is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "unknown asset").into_response();
    }
    let mut image = request;
    if image.id.is_empty() {
        image.id = library::new_image_id();
    }
    if !app.library.add_image(&page_id, image.clone()).await {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "picture has no size or no asset",
        )
            .into_response();
    }
    axum::Json(image).into_response()
}

/// `GET /pages/<id>/images` — the page's pictures.
pub(crate) async fn image_list(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    if app.library.page(&page_id).await.is_none() {
        return (axum::http::StatusCode::NOT_FOUND, "unknown page").into_response();
    }
    let images = app.library.images(&page_id).await;
    axum::Json(serde_json::json!({ "page": page_id, "images": images })).into_response()
}

/// `DELETE /pages/<id>/images` — body `{"ids":[…]}`.
pub(crate) async fn image_remove(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<IdsRequest>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let removed = app.library.remove_images(&page_id, &request.ids).await;
    axum::Json(serde_json::json!({ "removed": removed })).into_response()
}

/// `GET /assets/<id>` — the picture itself.
pub(crate) async fn asset_get(
    Path(id): Path<String>,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.assets.bytes(&id) {
        Some(bytes) => (
            [
                (axum::http::header::CONTENT_TYPE, "image/png"),
                (
                    axum::http::header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable",
                ),
            ],
            bytes,
        )
            .into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "unknown asset").into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct IdsRequest {
    #[serde(default)]
    pub ids: Vec<String>,
}

/// The picture routes.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route("/assets", post(asset_upload))
        .route("/assets/{id}", get(asset_get))
        .route(
            "/pages/{id}/images",
            get(image_list).post(image_place).delete(image_remove),
        )
}
