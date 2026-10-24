//! Penwise PC server: mirrors the tablet page to a browser tab and keeps it on disk.
//!
//! Thin vertical slice of the M3 contract: JSON over WebSocket instead of the postcard/HLC op log, and no pairing
//! or mDNS yet (see `docs/contracts/m3-server.md` for the full design).

mod http;
mod render;
mod state;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use state::ServerState;

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

    let state = ServerState::load(data).await;
    let router = Router::new()
        .route("/ws", get(upgrade))
        .route("/page.png", get(page_png))
        .route("/health", get(health))
        .fallback(get(file))
        .with_state(AppState {
            state,
            web: Arc::new(web.clone()),
        });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "penwise-server listening");
    for url in advertised_urls(port) {
        tracing::info!("viewer: {url}");
    }
    axum::serve(listener, router).await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    state: Arc<ServerState>,
    web: Arc<PathBuf>,
}

async fn upgrade(ws: WebSocketUpgrade, State(app): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| ws::handle(socket, app.state))
}

/// `GET /page.png?scale=1` — the current page as PNG (scale is pixels per page px).
async fn page_png(State(app): State<AppState>, Query(query): Query<ImageQuery>) -> Response {
    let strokes = app.state.snapshot_strokes().await;
    let scale = query.scale.unwrap_or(1.0);
    match render::page_png(&strokes, scale) {
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

/// `GET /health` — stroke count, for scripts and agents.
async fn health(State(app): State<AppState>) -> Response {
    let strokes = app.state.snapshot_strokes().await;
    axum::Json(serde_json::json!({ "ok": true, "strokes": strokes.len() })).into_response()
}

#[derive(serde::Deserialize)]
struct ImageQuery {
    scale: Option<f32>,
}

async fn file(State(app): State<AppState>, uri: axum::http::Uri) -> Response {
    http::serve((*app.web).clone(), uri).await
}

/// URLs worth printing: this machine's LAN address and its Tailscale name when present.
fn advertised_urls(port: u16) -> Vec<String> {
    let mut urls = vec![format!("http://127.0.0.1:{port}")];
    if let Ok(output) = std::process::Command::new("tailscale")
        .args(["status", "--json"])
        .output()
        && output.status.success()
        && let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        && let Some(name) = json
            .get("Self")
            .and_then(|s| s.get("DNSName"))
            .and_then(|d| d.as_str())
    {
        let name = name.trim_end_matches('.');
        urls.push(format!("http://{name}:{port}"));
    }
    urls
}
