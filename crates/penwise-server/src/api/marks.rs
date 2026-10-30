//! `POST /annotate`, `POST /annotate/clear`: agent marks as ops from the server device (design §1.5).

use axum::Json;
use axum::Router;
use axum::extract::{Query, State};
use axum::routing::post;
use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, Layer, Z_STEP};
use penwise_proto::{OpId, PageId};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::author::Author;
use super::error::ApiError;
use super::resolve::page_of;
use crate::annotate::{AnnotateRequest, Identity, to_items};
use crate::storage::random::random_u128;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/annotate", post(annotate))
        .route("/annotate/clear", post(clear))
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    page: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClearBody {
    #[serde(default)]
    page: Option<String>,
}

/// ULID item ids and 64-bit seeds from the OS generator.
struct RandomIdentity {
    now_ms: u64,
}

impl Identity for RandomIdentity {
    fn next(&mut self) -> (ItemId, u64) {
        let id = ItemId(OpId::from_parts(self.now_ms, random_u128()).0);
        let seed = u64::try_from(random_u128() & u128::from(u64::MAX)).unwrap_or(0);
        (id, seed)
    }
}

/// Body: `{"page"?, "strokes":[{"points":[[x,y],…],"color":"#RRGGBB","width":3}]}`. Returns `{"added": n}`.
async fn annotate(
    State(app): State<AppState>,
    Query(query): Query<PageQuery>,
    Json(request): Json<AnnotateRequest>,
) -> Result<Json<Value>, ApiError> {
    let requested = request.page.as_deref().or(query.page.as_deref());
    let page = page_of(&app, requested).await?;
    let added = add_annotations(&app, page, &request).await?;
    Ok(Json(json!({ "added": added })))
}

/// Draws `request`'s strokes on `page`'s agent layer in one batch; returns how many strokes were added.
///
/// # Errors
/// 404 for an unknown page, storage failures.
pub(crate) async fn add_annotations(
    app: &AppState,
    page: PageId,
    request: &AnnotateRequest,
) -> Result<usize, ApiError> {
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let first_z = view.replica.scene().next_z().unwrap_or(Z_STEP);
    let mut identity = RandomIdentity {
        now_ms: app.storage.now_ms(),
    };
    let items = to_items(request, first_z, &mut identity);
    let added = items.len();
    let mut author = Author::new(&app.storage);
    for item in items {
        author.item(page, Edit::Add(item)).await?;
    }
    author.commit().await?;
    Ok(added)
}

/// Deletes every live agent item of the page (`?page=` or body `{"page"}`); ink and promoted marks stay.
async fn clear(
    State(app): State<AppState>,
    Query(query): Query<PageQuery>,
    body: Option<Json<ClearBody>>,
) -> Result<Json<Value>, ApiError> {
    let from_body = body.and_then(|Json(b)| b.page);
    let page = page_of(&app, from_body.as_deref().or(query.page.as_deref())).await?;
    let ids = app.storage.live_item_ids(page, Some(Layer::Agent)).await?;
    let removed = ids.len();
    let mut author = Author::new(&app.storage);
    for id in ids {
        author.item(page, Edit::Delete(id)).await?;
    }
    author.commit().await?;
    Ok(Json(json!({ "removed": removed })))
}
