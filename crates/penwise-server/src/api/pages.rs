//! `/pages`: list, create, patch, delete, select.

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use penwise_core::item::{ItemId, ItemKind};
use penwise_proto::notebook::{MAX_TITLE_BYTES, POSITION_STEP};
use penwise_proto::{
    Background, Catalog, ItemRef, NotebookChange, NotebookId, Page, PageChange, PageId, PageKind,
    PagePatch,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::AppState;
use super::author::Author;
use super::error::ApiError;
use super::resolve::{current, page_of, parse_notebook};
use super::views::{BackgroundJson, NewPage, PagePatchBody, meta, page_json};
use crate::storage::random::random_u128;

/// New pages are A4-landscape-ish like the tablet: the pre-rewrite default.
const DEFAULT_WIDTH: f32 = 2560.0;
const DEFAULT_HEIGHT: f32 = 1600.0;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/pages", get(list).post(create))
        .route("/pages/{id}", patch(update).delete(remove))
        .route("/pages/{id}/select", post(select))
        .route("/pages/{id}/focus", post(focus))
}

async fn list(State(app): State<AppState>) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!(meta(&app).await?)))
}

pub(super) fn check_title(title: &str) -> Result<(), ApiError> {
    if title.trim().is_empty() {
        return Err(ApiError::bad_request("title is empty"));
    }
    if title.len() > MAX_TITLE_BYTES {
        return Err(ApiError::bad_request(format!(
            "title longer than {MAX_TITLE_BYTES} bytes"
        )));
    }
    Ok(())
}

async fn create(
    State(app): State<AppState>,
    Json(request): Json<NewPage>,
) -> Result<Json<Value>, ApiError> {
    if let Some(title) = &request.title {
        check_title(title)?;
    }
    let catalog = app.storage.catalog().await?;
    let mut author = Author::new(&app.storage);
    let notebook = pick_notebook(&app, &catalog, request.notebook.as_deref(), &mut author).await?;
    let siblings = catalog.pages(notebook);
    let page = Page {
        id: PageId::from_parts(app.storage.now_ms(), random_u128()),
        notebook,
        title: request
            .title
            .unwrap_or_else(|| format!("Page {}", siblings.len() + 1)),
        position: siblings
            .last()
            .map_or(POSITION_STEP, |p| p.position + POSITION_STEP),
        kind: request.kind.map_or(
            PageKind::Fixed {
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
            },
            Into::into,
        ),
        background: request.background.map_or(Background::Blank, Into::into),
        deleted: false,
    };
    let change = PageChange::Create {
        notebook,
        title: page.title.clone(),
        position: page.position,
        kind: page.kind,
        background: page.background,
    };
    author.page(page.id, change).await?;
    author.commit().await?;
    Ok(Json(
        json!({ "page": page_json(&page), "meta": meta(&app).await? }),
    ))
}

/// The requested notebook, else the current page's, else the first live one; a library without any notebook
/// gets a default one (created in `author`'s batch, ahead of the page).
pub(crate) async fn pick_notebook(
    app: &AppState,
    catalog: &Catalog,
    requested: Option<&str>,
    author: &mut Author<'_>,
) -> Result<NotebookId, ApiError> {
    if let Some(text) = requested {
        let id = parse_notebook(app, text).await?;
        if catalog.notebook(id).is_none_or(|nb| nb.deleted) {
            return Err(ApiError::not_found("unknown notebook"));
        }
        return Ok(id);
    }
    if let Some(id) = default_notebook(app, catalog).await? {
        return Ok(id);
    }
    let id = NotebookId::from_parts(app.storage.now_ms(), random_u128());
    let change = NotebookChange::Create {
        title: "Notebook".to_owned(),
        position: POSITION_STEP,
    };
    author.notebook(id, change).await?;
    Ok(id)
}

/// The notebook of the current page, else the first live notebook.
async fn default_notebook(
    app: &AppState,
    catalog: &Catalog,
) -> Result<Option<NotebookId>, ApiError> {
    if let Some(id) = current(app, catalog).await?
        && let Some(page) = catalog.page(id)
    {
        return Ok(Some(page.notebook));
    }
    Ok(catalog.notebooks().first().map(|nb| nb.id))
}

async fn update(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PagePatchBody>,
) -> Result<Json<Value>, ApiError> {
    let page = page_of(&app, Some(&id)).await?;
    if let Some(title) = &body.title {
        check_title(title)?;
    }
    if body.title.is_none() && body.background.is_none() {
        return Err(ApiError::bad_request("nothing to change"));
    }
    let patch = PagePatch {
        title: body.title,
        background: body.background.map(BackgroundJson::into),
        ..PagePatch::default()
    };
    let mut author = Author::new(&app.storage);
    author.page(page, PageChange::Update(patch)).await?;
    author.commit().await?;
    let catalog = app.storage.catalog().await?;
    let updated = catalog
        .page(page)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    Ok(Json(
        json!({ "page": page_json(&updated), "meta": meta(&app).await? }),
    ))
}

async fn remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let page = page_of(&app, Some(&id)).await?;
    let catalog = app.storage.catalog().await?;
    let live: usize = catalog
        .notebooks()
        .iter()
        .map(|nb| catalog.pages(nb.id).len())
        .sum();
    if live <= 1 {
        return Err(ApiError::bad_request("cannot delete the last page"));
    }
    let patch = PagePatch {
        deleted: Some(true),
        ..PagePatch::default()
    };
    let mut author = Author::new(&app.storage);
    author.page(page, PageChange::Update(patch)).await?;
    author.commit().await?;
    Ok(Json(json!({ "ok": true, "meta": meta(&app).await? })))
}

async fn select(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let page = page_of(&app, Some(&id)).await?;
    app.storage.set_current_page(page).await?;
    // Design harness M3 (hunt #17): the tablet and PC follow a page switch made over HTTP, like the one a
    // client makes with `ClientMsg::SwitchPage`.
    (app.announce_page)(page);
    Ok(Json(json!({ "active": page.to_string() })))
}

/// `POST /pages/<id>/focus {"item"?}`: every connected device switches to the page and fits the frame `item`
/// (hex or ULID text); no item fits the page's content. Mirrors `select`, for the agent's camera.
#[derive(Debug, Default, Deserialize)]
struct FocusBody {
    #[serde(default)]
    item: Option<String>,
}

async fn focus(
    State(app): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<FocusBody>>,
) -> Result<Json<Value>, ApiError> {
    let page = page_of(&app, Some(&id)).await?;
    let item = match body.and_then(|Json(body)| body.item) {
        Some(text) => Some(focus_target(&app, page, &text).await?),
        None => None,
    };
    (app.announce_focus)(page, item);
    Ok(Json(json!({
        "page": page.to_string(),
        "item": item.map(ItemId::to_hex),
    })))
}

/// The frame a focus names: it must be a live frame of `page`.
async fn focus_target(app: &AppState, page: PageId, text: &str) -> Result<ItemId, ApiError> {
    let id = ItemId::from_hex(text)
        .or_else(|| text.parse::<ItemRef>().ok().map(ItemId::from))
        .ok_or_else(|| ApiError::bad_request("item is a frame id"))?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    match view.replica.scene().get(id).map(|item| &item.kind) {
        Some(ItemKind::Frame(_)) => Ok(id),
        _ => Err(ApiError::not_found("unknown frame")),
    }
}

#[cfg(test)]
mod tests;
