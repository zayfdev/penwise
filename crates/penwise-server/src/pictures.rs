//! Picture routes: upload a PNG as an asset, place it on a page, list it, remove it.
//!
//! Pictures are what make the canvas a design surface — a webpage screenshot to annotate, a component cut out of
//! a running app. The bytes are content-addressed files (`assets/<sha256>.png`, see [`crate::assets`]); a placed
//! picture is an `Add(Image)` item from the server device, so it reaches every device like any other mark and is
//! listed and removed through the page's items. Its label lives in `image_labels`, keyed by the item id.

mod labels;

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use penwise_core::edit::Edit;
use penwise_core::item::{
    Affine, ImageItem, Item, ItemId, ItemKind, Layer, LayerSet, Scene, Z_STEP,
};
use penwise_proto::{ItemRef, PageId};
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, AppState, Author, parse_page};
use crate::assets;
use crate::storage::PageView;

/// Largest picture side, local px (the item's own limit is 2²⁰).
const MAX_PICTURE_SIDE: f32 = 1_048_576.0;

/// A placed picture as the API shows it. `x, y, w, h` are page px.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct Picture {
    /// The image item's id (ULID text).
    pub id: String,
    pub asset: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub layer: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaceRequest {
    asset: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: Option<String>,
    /// Re-placing the same id is a no-op that returns the existing picture.
    id: Option<String>,
    /// `ink` (default) or `agent`.
    layer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdsRequest {
    #[serde(default)]
    ids: Vec<String>,
}

/// `POST /assets` — body is a PNG. Returns `{"asset","width","height"}`; the digest is the picture's identity.
async fn asset_upload(
    State(app): State<AppState>,
    body: axum::body::Bytes,
) -> Result<Response, ApiError> {
    let assets = Arc::clone(&app.assets);
    let stored = tokio::task::spawn_blocking(move || assets.put(&body))
        .await
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(ApiError::bad_request)?;
    Ok(axum::Json(stored).into_response())
}

/// `GET /assets/<id>` — the picture itself.
async fn asset_get(Path(id): Path<String>, State(app): State<AppState>) -> Response {
    match app.assets.bytes(&id) {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            ],
            bytes,
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "unknown asset").into_response(),
    }
}

async fn live_view(app: &AppState, page: &str) -> Result<(PageId, PageView), ApiError> {
    let page = parse_page(app, page).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    Ok((page, view))
}

fn layer_of(name: Option<&str>) -> Result<Layer, ApiError> {
    match name.unwrap_or("ink") {
        "ink" => Ok(Layer::Ink),
        "agent" => Ok(Layer::Agent),
        _ => Err(ApiError::bad_request("layer is `ink` or `agent`")),
    }
}

fn picture_of(item: &Item, image: &ImageItem, label: Option<String>) -> Picture {
    Picture {
        id: ItemRef::from(item.id).to_string(),
        asset: assets::id_of(&image.asset),
        x: item.transform.e,
        y: item.transform.f,
        w: image.w * item.transform.a,
        h: image.h * item.transform.d,
        layer: if item.layer == Layer::Agent {
            "agent"
        } else {
            "ink"
        },
        label,
    }
}

/// The page's live pictures, in paint order.
fn pictures_of(scene: &Scene) -> Vec<(&Item, &ImageItem)> {
    scene
        .render_order(LayerSet::ALL)
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter_map(|item| match &item.kind {
            ItemKind::Image(image) => Some((item, image)),
            _ => None,
        })
        .collect()
}

/// `POST /pages/<id>/images` — body `{"asset","x","y","w","h","label"?,"id"?,"layer"?}`.
///
/// The picture is placed in page px: the extension sends a screenshot sized to the page, and components are
/// laid out where the caller wants them. It goes beneath everything already on the page, so ink draws over it.
async fn image_place(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    axum::Json(request): axum::Json<PlaceRequest>,
) -> Result<Response, ApiError> {
    let (page, view) = live_view(&app, &page_id).await?;
    let digest = assets::digest_of(&request.asset)
        .filter(|_| app.assets.path(&request.asset).is_some())
        .ok_or_else(|| ApiError::not_found("unknown asset"))?;
    let sizes = [request.w, request.h];
    if !(sizes
        .iter()
        .all(|s| s.is_finite() && *s > 0.0 && *s <= MAX_PICTURE_SIDE)
        && request.x.is_finite()
        && request.y.is_finite())
    {
        return Err(ApiError::bad_request("picture has no size or no asset"));
    }
    if request
        .label
        .as_ref()
        .is_some_and(|l| l.len() > labels::MAX_LABEL_BYTES)
    {
        return Err(ApiError::bad_request("label is too long"));
    }
    let layer = layer_of(request.layer.as_deref())?;
    let scene = view.replica.scene();
    let id = match request.id.as_deref() {
        Some(text) => {
            let id: ItemId = text
                .parse::<ItemRef>()
                .map_err(|_| ApiError::bad_request("malformed picture id"))?
                .into();
            if let Some((item, image)) = pictures_of(scene)
                .into_iter()
                .find(|(item, _)| item.id == id)
            {
                let label = labels::of_page(&app.data_dir, page).await?.remove(&id);
                return Ok(axum::Json(picture_of(item, image, label)).into_response());
            }
            id
        }
        None => {
            ItemRef::from_parts(app.storage.now_ms(), crate::storage::random::random_u128()).into()
        }
    };
    let z = view
        .replica
        .items()
        .filter(|state| !state.deleted)
        .map(|state| state.item.z)
        .min()
        .map_or(0, |lowest| lowest.saturating_sub(Z_STEP));
    let item = Item {
        id,
        layer,
        z,
        transform: Affine::translate(request.x, request.y),
        kind: ItemKind::Image(ImageItem {
            asset: digest,
            w: request.w,
            h: request.h,
        }),
    };
    let picture = picture_of(
        &item,
        &ImageItem {
            asset: digest,
            w: request.w,
            h: request.h,
        },
        request.label.clone(),
    );
    let mut author = Author::new(&app.storage);
    author.item(page, Edit::Add(item)).await?;
    author.commit().await?;
    if let Some(label) = request.label {
        labels::put(&app.data_dir, id, label).await?;
    }
    Ok(axum::Json(picture).into_response())
}

/// `GET /pages/<id>/images` — the page's pictures.
async fn image_list(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
) -> Result<Response, ApiError> {
    let (page, view) = live_view(&app, &page_id).await?;
    let labels = labels::of_page(&app.data_dir, page).await?;
    let images: Vec<Picture> = pictures_of(view.replica.scene())
        .into_iter()
        .map(|(item, image)| picture_of(item, image, labels.get(&item.id).cloned()))
        .collect();
    Ok(
        axum::Json(serde_json::json!({ "page": page.to_string(), "images": images }))
            .into_response(),
    )
}

/// `DELETE /pages/<id>/images` — body `{"ids":[…]}`; tombstones the pictures (undoable through restore).
async fn image_remove(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    axum::Json(request): axum::Json<IdsRequest>,
) -> Result<Response, ApiError> {
    let (page, view) = live_view(&app, &page_id).await?;
    let present: BTreeSet<ItemId> = pictures_of(view.replica.scene())
        .into_iter()
        .map(|(item, _)| item.id)
        .collect();
    let mut doomed = BTreeSet::new();
    for text in &request.ids {
        let id: ItemId = text
            .parse::<ItemRef>()
            .map_err(|_| ApiError::bad_request("malformed picture id"))?
            .into();
        if present.contains(&id) {
            doomed.insert(id);
        }
    }
    let mut author = Author::new(&app.storage);
    for id in &doomed {
        author.item(page, Edit::Delete(*id)).await?;
    }
    author.commit().await?;
    Ok(axum::Json(serde_json::json!({ "removed": doomed.len() })).into_response())
}

/// The picture routes. Only the upload takes a body this large; every other route keeps axum's small default.
pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/assets",
            post(asset_upload).layer(DefaultBodyLimit::max(assets::MAX_BYTES)),
        )
        .route("/assets/{id}", get(asset_get))
        .route(
            "/pages/{id}/images",
            get(image_list).post(image_place).delete(image_remove),
        )
}

#[cfg(test)]
mod tests;
