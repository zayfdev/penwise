//! Penwise PC server: mirrors the tablet page to a browser tab and keeps it on disk.
//!
//! Thin vertical slice of the M3 contract: JSON over WebSocket instead of the postcard/HLC op log, and no pairing
//! or mDNS yet (see `docs/contracts/m3-server.md` for the full design).

mod http;
mod state;
mod ws;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
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
