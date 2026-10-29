//! `/projects` (the linked-projects registry) and `POST /pages/<id>/frames` (design-harness spec §6.1, §8, C2).
//!
//! A project is a folder registered with the server; its port, notebook and lifecycle come from
//! [`crate::frames::registry`]. Linking is idempotent by root, and the first link creates the project's notebook.
//! Frames are ordinary page items added by the server device, so they sync, move, delete and undo like every
//! other item.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, post};
use penwise_core::edit::EditError;
use penwise_core::item::{FramePreset, FrameSpec, ItemId, Layer, Z_STEP, add_frame};
use penwise_proto::ItemRef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::AppState;
use super::author::Author;
use super::error::ApiError;
use super::resolve::parse_page;
use crate::frames::registry::{LinkRequest, Project};
use crate::storage::random::random_u128;

/// The project routes. Unlike the rest of the API they accept a same-machine request without the token, so
/// `penwise link` works from a local terminal without copying the token around (the rule of `/pair/local`).
pub(crate) fn routes(token: Arc<String>) -> Router<AppState> {
    Router::new()
        .route("/projects", get(list).post(link))
        .route("/projects/{id}", delete(remove))
        .route("/pages/{id}/frames", post(frame_create))
        .merge(super::project_marks::routes())
        .merge(crate::agent::routes())
        .layer(middleware::from_fn_with_state(token, require))
}

/// Token, or a loopback peer with a loopback `Host`.
async fn require(State(token): State<Arc<String>>, request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if super::same_machine(peer, host) {
        next.run(request).await
    } else {
        crate::auth::require(State(token), request, next).await
    }
}

/// A project as `GET /projects` and `POST /projects` show it.
#[derive(Debug, Serialize)]
struct ProjectJson {
    id: String,
    name: String,
    root: String,
    kind: &'static str,
    #[serde(rename = "static", skip_serializing_if = "Option::is_none")]
    static_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
    port: u16,
    notebook: Option<String>,
}

impl From<&Project> for ProjectJson {
    fn from(project: &Project) -> Self {
        Self {
            id: project.id.clone(),
            name: project.name.clone(),
            root: project.root.to_string_lossy().into_owned(),
            kind: project.kind(),
            static_dir: project
                .static_dir()
                .map(|folder| folder.to_string_lossy().into_owned()),
            origin: project.origin().map(str::to_owned),
            port: project.port,
            notebook: project.notebook.clone(),
        }
    }
}

async fn list(State(app): State<AppState>) -> Result<Json<Vec<ProjectJson>>, ApiError> {
    let projects = app.projects.list().await?;
    Ok(Json(projects.iter().map(ProjectJson::from).collect()))
}

/// `POST /projects` — body `{"name"?, "root", "static"? | "origin"?, "id"?}`. Idempotent by `root`.
async fn link(
    State(app): State<AppState>,
    Json(request): Json<LinkRequest>,
) -> Result<Json<ProjectJson>, ApiError> {
    let project = app.projects.link(request).await?;
    Ok(Json(ProjectJson::from(&project)))
}

/// `DELETE /projects/<id>` — unregisters and stops the port.
async fn remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !app.projects.remove(&id).await? {
        return Err(ApiError::not_found("unknown project"));
    }
    Ok(Json(json!({ "ok": true })))
}

/// `POST /pages/<id>/frames` — body `{"project","route","preset","cssW"?,"cssH"?,"x"?,"y"?}`.
#[derive(Debug, Deserialize)]
struct FrameBody {
    project: String,
    route: String,
    preset: String,
    #[serde(rename = "cssW")]
    css_w: Option<u32>,
    #[serde(rename = "cssH")]
    css_h: Option<u32>,
    x: Option<f32>,
    y: Option<f32>,
}

async fn frame_create(
    Path(page_id): Path<String>,
    State(app): State<AppState>,
    Json(body): Json<FrameBody>,
) -> Result<Json<Value>, ApiError> {
    let project = app
        .projects
        .get(&body.project)
        .await?
        .ok_or_else(|| ApiError::not_found("unknown project"))?;
    let preset = FramePreset::parse(&body.preset).ok_or_else(|| {
        ApiError::bad_request(format!(
            "preset is one of phone, tablet, desktop, custom (got {:?})",
            body.preset
        ))
    })?;
    let spec = FrameSpec::new(
        project.id,
        body.route,
        preset,
        body.css_w,
        body.css_h,
        body.x.unwrap_or(0.0),
        body.y.unwrap_or(0.0),
    )
    .map_err(|error| invalid_frame(&error))?;
    let page = parse_page(&app, &page_id).await?;
    let view = app
        .storage
        .page_view(page)
        .await?
        .filter(|view| !view.page.deleted)
        .ok_or_else(|| ApiError::not_found("unknown page"))?;
    let z = view.replica.scene().next_z().unwrap_or(Z_STEP);
    let id: ItemId = ItemRef::from_parts(app.storage.now_ms(), random_u128()).into();
    let edit = add_frame(&spec, id, Layer::Ink, z);
    let mut author = Author::new(&app.storage);
    author.item(page, edit).await?;
    author.commit().await?;
    Ok(Json(json!({ "id": ItemRef::from(id).to_string() })))
}

fn invalid_frame(error: &EditError) -> ApiError {
    ApiError::new(StatusCode::BAD_REQUEST, error.to_string())
}

#[cfg(test)]
mod tests;
