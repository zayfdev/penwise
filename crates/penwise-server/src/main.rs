//! Penwise PC server: mirrors the tablet page to a browser tab and keeps it on disk.
//!
//! Thin vertical slice of the M3 contract: JSON over WebSocket instead of the postcard/HLC op log, and no pairing
//! or mDNS yet (see `docs/contracts/m3-server.md` for the full design).

mod annotate;
mod auth;
mod http;
mod library;
mod ocr;
mod pairing;
mod render;
mod urls;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};

use library::Library;

/// Port from the contract; sync, viewer and (later) the API share it.
const DEFAULT_PORT: u16 = 7420;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    let port = std::env::var("PENWISE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    let web = PathBuf::from(std::env::var("PENWISE_WEB").unwrap_or_else(|_| "web".to_owned()));
    let data =
        PathBuf::from(std::env::var("PENWISE_DATA").unwrap_or_else(|_| "penwise-data".to_owned()));
    if !web.join("index.html").is_file() {
        tracing::warn!(dir = %web.display(), "no index.html (set PENWISE_WEB)");
    }

    let token = auth::load_or_create(&data);
    let library = Library::load(data.clone()).await;
    let pairing = Arc::new(pairing::Pairing::new());
    let urls = urls::platform_urls(port);
    let router = Router::new()
        .route("/ws", get(upgrade))
        .route("/pages", get(pages_list).post(page_create))
        .route("/pages/{id}", patch(page_update).delete(page_remove))
        .route("/pages/{id}/select", post(page_select))
        .route("/notebooks", post(notebook_create))
        .route("/pair", post(pairing::exchange_route))
        .route(
            "/pair/code",
            get(pairing::code_route).post(pairing::refresh_route),
        )
        .route(
            "/notebooks/{id}",
            patch(notebook_update).delete(notebook_remove),
        )
        .route("/page", get(page_strokes))
        .route("/text", get(ocr::page_text))
        .route("/page.png", get(page_png))
        .route("/health", get(health))
        .route("/annotate", post(annotate))
        .route("/annotate/clear", post(clear_annotations))
        .fallback(get(file))
        .with_state(AppState {
            library,
            web: Arc::new(web.clone()),
            token: Arc::new(token.clone()),
            pairing: Arc::clone(&pairing),
            urls: Arc::new(urls),
        });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "penwise-server listening");
    for url in urls::platform_urls(port) {
        tracing::info!("viewer: {url}/?token={token}");
    }
    tracing::info!("token: {token}  (also in {}/token)", data.display());
    tracing::info!(
        "pairing code: {}  (single use, {} minutes; show it in the viewer)",
        pairing.current(),
        pairing::CODE_TTL.as_secs() / 60
    );
    axum::serve(listener, router).await?;
    Ok(())
}

#[allow(
    missing_debug_implementations,
    reason = "holding the token makes a derived Debug a leak risk; nothing logs AppState"
)]
#[derive(Clone)]
pub struct AppState {
    pub library: Arc<Library>,
    pub web: Arc<PathBuf>,
    pub token: Arc<String>,
    pub pairing: Arc<pairing::Pairing>,
    pub urls: Arc<Vec<String>>,
}

async fn upgrade(
    ws: WebSocketUpgrade,
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    ws.on_upgrade(move |socket| ws::handle(socket, app.library))
}

/// `GET /page.png?page=<id>&scale=1` — one page as PNG (scale is pixels per page px; `page` defaults to active).
async fn page_png(
    State(app): State<AppState>,
    Query(query): Query<ImageQuery>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let page = match resolve_page(&app, query.page.as_deref()).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let strokes = app.library.strokes(&page.id).await;
    let scale = query.scale.unwrap_or(1.0);
    match render::page_png(&strokes, scale, &page) {
        Ok(png) => (
            [
                (axum::http::header::CONTENT_TYPE, "image/png"),
                (axum::http::header::CACHE_CONTROL, "no-store"),
            ],
            png,
        )
            .into_response(),
        Err(error) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, error).into_response(),
    }
}

/// The addressed page, or the active one; `Err` is the ready-to-return 404.
pub(crate) async fn resolve_page(
    app: &AppState,
    requested: Option<&str>,
) -> Result<library::PageMeta, Response> {
    let id = match requested {
        Some(id) => id.to_owned(),
        None => app.library.active().await,
    };
    app.library
        .page(&id)
        .await
        .ok_or_else(|| (axum::http::StatusCode::NOT_FOUND, "unknown page").into_response())
}

/// `GET /pages` — the whole index.
async fn pages_list(
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    axum::Json(app.library.meta().await).into_response()
}

/// `POST /pages` — a new page; see [`library::NewPage`] for the optional fields.
async fn page_create(
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<library::NewPage>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.create_page(request).await {
        Ok(page) => axum::Json(serde_json::json!({
            "page": page,
            "meta": app.library.meta().await,
        }))
        .into_response(),
        Err(message) => (axum::http::StatusCode::BAD_REQUEST, message).into_response(),
    }
}

/// `PATCH /pages/<id>` — title and background.
async fn page_update(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(patch_body): axum::Json<library::PagePatch>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.patch_page(&id, patch_body).await {
        Ok(page) => axum::Json(serde_json::json!({
            "page": page,
            "meta": app.library.meta().await,
        }))
        .into_response(),
        Err(message) => (axum::http::StatusCode::BAD_REQUEST, message).into_response(),
    }
}

/// `DELETE /pages/<id>` — removes a page (never the last one).
async fn page_remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.delete_page(&id).await {
        Ok(meta) => axum::Json(serde_json::json!({ "ok": true, "meta": meta })).into_response(),
        Err(message) => (axum::http::StatusCode::BAD_REQUEST, message).into_response(),
    }
}

/// `POST /pages/<id>/select` — sets the library's active page.
async fn page_select(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    if app.library.select(&id).await {
        axum::Json(serde_json::json!({ "active": id })).into_response()
    } else {
        (axum::http::StatusCode::NOT_FOUND, "unknown page").into_response()
    }
}

/// `POST /notebooks` — a new (empty) notebook.
async fn notebook_create(
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let title = body
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match app.library.create_notebook(title).await {
        Ok((notebook, meta)) => {
            axum::Json(serde_json::json!({ "notebook": notebook, "meta": meta })).into_response()
        }
        Err(message) => (axum::http::StatusCode::BAD_REQUEST, message).into_response(),
    }
}

/// `PATCH /notebooks/<id>` — rename.
async fn notebook_update(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let title = body
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    match app.library.rename_notebook(&id, title).await {
        Ok(meta) => axum::Json(serde_json::json!({ "ok": true, "meta": meta })).into_response(),
        Err(message) => (axum::http::StatusCode::BAD_REQUEST, message).into_response(),
    }
}

/// `DELETE /notebooks/<id>` — only when it is empty.
async fn notebook_remove(
    State(app): State<AppState>,
    Path(id): Path<String>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    match app.library.delete_notebook(&id).await {
        Ok(meta) => axum::Json(serde_json::json!({ "ok": true, "meta": meta })).into_response(),
        Err(message) => (axum::http::StatusCode::BAD_REQUEST, message).into_response(),
    }
}

/// `GET /page?page=<id>` — one page's strokes, for clients bootstrapping a specific page.
async fn page_strokes(
    State(app): State<AppState>,
    Query(query): Query<ImageQuery>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let page = match resolve_page(&app, query.page.as_deref()).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let strokes = app.library.strokes(&page.id).await;
    axum::Json(serde_json::json!({ "version": 1, "page": page.id, "strokes": strokes }))
        .into_response()
}

/// `POST /annotate` — draws polylines on the agent layer. Body: `{"strokes":[{"points":[[x,y],…],
/// "color":"#RRGGBB","width":3}]}`. Returns how many strokes were added.
async fn annotate(
    State(app): State<AppState>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
    axum::Json(request): axum::Json<annotate::AnnotateRequest>,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let strokes = annotate::to_strokes(&request);
    let page = match resolve_page(&app, request.page.as_deref()).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let added = app.library.append_many(&page.id, strokes).await;
    axum::Json(serde_json::json!({ "added": added })).into_response()
}

/// `POST /annotate/clear` — removes every agent-authored stroke.
async fn clear_annotations(
    State(app): State<AppState>,
    Query(query): Query<ImageQuery>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let page = match resolve_page(&app, query.page.as_deref()).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let removed = app.library.clear_agent(&page.id).await;
    axum::Json(serde_json::json!({ "removed": removed })).into_response()
}

/// `GET /health` — stroke count, for scripts and agents.
async fn health(
    State(app): State<AppState>,
    Query(query): Query<ImageQuery>,
    Query(auth_query): Query<AuthQuery>,
    headers: axum::http::HeaderMap,
) -> Response {
    if !auth::authorized(&app.token, auth_query.token.as_deref(), &headers) {
        return auth::deny();
    }
    let page = match resolve_page(&app, query.page.as_deref()).await {
        Ok(page) => page,
        Err(response) => return response,
    };
    let strokes = app.library.strokes(&page.id).await;
    let pages = app
        .library
        .meta()
        .await
        .notebooks
        .iter()
        .map(|n| n.pages.len())
        .sum::<usize>();
    axum::Json(serde_json::json!({
        "ok": true,
        "page": page.id,
        "strokes": strokes.len(),
        "pages": pages,
    }))
    .into_response()
}

#[derive(serde::Deserialize)]
struct ImageQuery {
    scale: Option<f32>,
    page: Option<String>,
}

/// Both token-bearing routes take the token in the query; `ImageQuery` stays separate so `scale` parsing is unaffected.
#[derive(Debug, serde::Deserialize)]
pub struct AuthQuery {
    pub token: Option<String>,
}

async fn file(State(app): State<AppState>, uri: axum::http::Uri) -> Response {
    http::serve((*app.web).clone(), uri).await
}
