//! `/notebooks`: create, rename, delete (only when empty).

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{patch, post};
use penwise_proto::notebook::POSITION_STEP;
use penwise_proto::{Notebook, NotebookChange, NotebookId, NotebookPatch};
use serde_json::{Value, json};

use super::AppState;
use super::author::Author;
use super::error::ApiError;
use super::pages::check_title;
use super::resolve::parse_notebook;
use super::views::{TitleBody, meta, notebook_json};
use crate::storage::random::random_u128;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/notebooks", post(create))
        .route("/notebooks/{id}", patch(rename).delete(remove))
}

async fn create(
    State(app): State<AppState>,
    Json(body): Json<TitleBody>,
) -> Result<Json<Value>, ApiError> {
    check_title(&body.title)?;
    let catalog = app.storage.catalog().await?;
    let position = catalog
        .notebooks()
        .last()
        .map_or(POSITION_STEP, |nb| nb.position + POSITION_STEP);
    let notebook = Notebook {
        id: NotebookId::from_parts(app.storage.now_ms(), random_u128()),
        title: body.title,
        position,
        deleted: false,
    };
    let mut author = Author::new(&app.storage);
    let change = NotebookChange::Create {
        title: notebook.title.clone(),
        position,
    };
    author.notebook(notebook.id, change).await?;
    author.commit().await?;
    let created = notebook_json(&catalog, &notebook);
    Ok(Json(
        json!({ "notebook": created, "meta": meta(&app).await? }),
    ))
}

/// The notebook if it exists and is live.
async fn live_notebook(
    app: &AppState,
    id: &str,
) -> Result<(NotebookId, penwise_proto::Catalog), ApiError> {
    let id = parse_notebook(app, id).await?;
    let catalog = app.storage.catalog().await?;
    if catalog.notebook(id).is_none_or(|nb| nb.deleted) {
        return Err(ApiError::not_found("unknown notebook"));
    }
    Ok((id, catalog))
}

async fn rename(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<TitleBody>,
) -> Result<Json<Value>, ApiError> {
    check_title(&body.title)?;
    let (id, _) = live_notebook(&app, &id).await?;
    let patch = NotebookPatch {
        title: Some(body.title),
        ..NotebookPatch::default()
    };
    let mut author = Author::new(&app.storage);
    author.notebook(id, NotebookChange::Update(patch)).await?;
    author.commit().await?;
    Ok(Json(json!({ "ok": true, "meta": meta(&app).await? })))
}

async fn remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let (id, catalog) = live_notebook(&app, &id).await?;
    if catalog.notebooks().len() <= 1 {
        return Err(ApiError::bad_request("cannot delete the last notebook"));
    }
    if !catalog.pages(id).is_empty() {
        return Err(ApiError::bad_request(
            "move or delete the notebook's pages first",
        ));
    }
    let patch = NotebookPatch {
        deleted: Some(true),
        ..NotebookPatch::default()
    };
    let mut author = Author::new(&app.storage);
    author.notebook(id, NotebookChange::Update(patch)).await?;
    author.commit().await?;
    Ok(Json(json!({ "ok": true, "meta": meta(&app).await? })))
}
