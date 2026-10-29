//! Per-project ports (design-harness spec §6.1).
//!
//! Every linked project gets its own port so absolute URLs (`/main.js`, `/@vite/client`) resolve unchanged:
//!
//! - **dev origin** — every request is reverse-proxied ([`proxy`]), WebSocket upgrades included, so Vite/Next HMR
//!   keeps working;
//! - **static folder** — served from disk with an `index.html` fallback ([`static_site`]), file watching and a
//!   reload socket at [`RELOAD_PATH`].
//!
//! Both modes inject [`BRIDGE_PATH`] into `text/html` ([`inject`]) and require the Penwise token: the
//! [`COOKIE`] set by [`ENTER_PATH`], the `X-Penwise-Token` header, or — for same-machine tools — a loopback peer
//! with a loopback `Host` (the rule of `GET /pair/local`). Nothing else is served without it.

mod bridge;
mod inject;
pub(crate) mod patch;
mod proxy;
pub mod registry;
mod static_site;

#[cfg(test)]
mod tests;

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use proxy::Proxy;
use static_site::Site;

/// The injected script, served on every project port.
pub const BRIDGE_PATH: &str = "/__penwise/bridge.js";
/// Sets the token cookie and redirects to the requested route.
pub const ENTER_PATH: &str = "/__penwise/enter";
/// The static-mode live-reload socket the bridge listens to.
pub const RELOAD_PATH: &str = "/__penwise/reload";
/// Cookie carrying the access token (`GET /__penwise/enter` sets it).
pub const COOKIE: &str = "penwise_token";
/// What the reload socket sends; the bridge answers with `location.reload()`.
const RELOAD_MESSAGE: &str = "{\"t\":\"pw:reload\"}";
/// One month, so a tablet keeps working across launches.
const COOKIE_MAX_AGE: u32 = 30 * 24 * 60 * 60;

use bridge::BRIDGE_JS;

/// What a project port serves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A folder served by Penwise itself, watched for live reload.
    Static(PathBuf),
    /// A dev server origin (`http://127.0.0.1:5173`), reverse-proxied.
    Origin(String),
}

/// One project port.
#[derive(Debug, Clone)]
pub struct ProjectConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub target: Target,
}

impl ProjectConfig {
    /// A static project on `host:port`.
    #[must_use]
    pub fn static_site(
        id: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        folder: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: id.into(),
            host: host.into(),
            port,
            target: Target::Static(folder.into()),
        }
    }

    /// A project proxied to a dev origin.
    #[must_use]
    pub fn dev_origin(
        id: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        origin: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            host: host.into(),
            port,
            target: Target::Origin(origin.into()),
        }
    }
}

/// One `PENWISE_PROJECTS` element. The registry ([`registry`]) is the real one; this env override stays for
/// development and tests.
#[derive(Debug, Deserialize)]
struct ProjectEntry {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    host: Option<String>,
    port: u16,
    #[serde(default, rename = "static")]
    folder: Option<PathBuf>,
    #[serde(default)]
    origin: Option<String>,
}

impl ProjectEntry {
    fn into_config(self, default_host: &str) -> Result<ProjectConfig, FrameError> {
        let target = match (self.folder, self.origin) {
            (Some(folder), None) => Target::Static(folder),
            (None, Some(origin)) => Target::Origin(origin),
            _ => {
                return Err(FrameError::Config(format!(
                    "project {} needs exactly one of \"static\" or \"origin\"",
                    self.id.as_deref().unwrap_or("<unnamed>")
                )));
            }
        };
        Ok(ProjectConfig {
            id: self.id.unwrap_or_else(|| format!("p{}", self.port)),
            host: self.host.unwrap_or_else(|| default_host.to_owned()),
            port: self.port,
            target,
        })
    }
}

/// Parses `PENWISE_PROJECTS`: a JSON array of `{"id", "host"?, "port", "static"|"origin"}`.
///
/// # Errors
/// Invalid JSON, an entry with both or neither target, or a missing `port`.
pub fn parse_projects(json: &str) -> Result<Vec<ProjectConfig>, FrameError> {
    let entries: Vec<ProjectEntry> = serde_json::from_str(json)
        .map_err(|error| FrameError::Config(format!("PENWISE_PROJECTS: {error}")))?;
    let host = std::env::var("PENWISE_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
    entries
        .into_iter()
        .map(|entry| entry.into_config(&host))
        .collect()
}

/// Why a project port could not be set up.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("project config: {0}")]
    Config(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// A listening project port.
#[allow(missing_debug_implementations, reason = "holds the listener task")]
pub struct ProjectServer {
    pub id: String,
    pub addr: SocketAddr,
    pub target: Target,
    task: JoinHandle<()>,
    site: Option<Arc<Site>>,
}

impl ProjectServer {
    /// Binds and serves one project. `port` 0 binds an ephemeral port (tests).
    ///
    /// # Errors
    /// A missing static folder, an unparsable origin, or a port already in use.
    pub async fn start(config: ProjectConfig, token: String) -> Result<Self, FrameError> {
        let (site, proxy) = match &config.target {
            Target::Static(folder) => (Some(Arc::new(Site::new(folder.clone())?)), None),
            Target::Origin(origin) => (None, Some(Arc::new(Proxy::new(origin)?))),
        };
        let state = Arc::new(App {
            token: Arc::new(token),
            site: site.clone(),
            proxy,
        });
        let listener = TcpListener::bind((config.host.as_str(), config.port)).await?;
        let addr = listener.local_addr()?;
        let router = router(Arc::clone(&state));
        let id = config.id.clone();
        let task = tokio::spawn(async move {
            let service = router.into_make_service_with_connect_info::<SocketAddr>();
            if let Err(error) = axum::serve(listener, service).await {
                tracing::warn!(%error, project = %id, "project port stopped");
            }
        });
        Ok(Self {
            id: config.id,
            addr,
            target: config.target,
            task,
            site,
        })
    }

    /// How many pages currently listen for live reloads (`0` for proxied projects).
    #[must_use]
    pub fn reload_listeners(&self) -> usize {
        self.site.as_ref().map_or(0, |site| site.listeners())
    }

    /// Stops accepting and drops the router (which also stops a static project's watcher).
    pub async fn shutdown(self) {
        self.task.abort();
        let _ = self.task.await;
        drop(self.site);
    }

    /// Stops the listener without awaiting (server `Drop` and registry teardown).
    pub fn abort(mut self) {
        self.task.abort();
        self.site = None;
    }
}

/// Starts every project in `PENWISE_PROJECTS`; a bad entry is logged and skipped, so the main server keeps
/// running.
///
/// # Errors
/// The JSON itself is unparsable.
pub async fn start_all(json: &str, token: &str) -> Result<Vec<ProjectServer>, FrameError> {
    let mut servers = Vec::new();
    for config in parse_projects(json)? {
        let id = config.id.clone();
        match ProjectServer::start(config, token.to_owned()).await {
            Ok(server) => {
                tracing::info!(
                    project = %server.id,
                    addr = %server.addr,
                    target = ?server.target,
                    "project port listening"
                );
                servers.push(server);
            }
            Err(error) => tracing::warn!(project = %id, %error, "project port not started"),
        }
    }
    Ok(servers)
}

/// Everything a project port's handlers need.
struct App {
    token: Arc<String>,
    site: Option<Arc<Site>>,
    proxy: Option<Arc<Proxy>>,
}

fn router(state: Arc<App>) -> Router {
    let guarded = Router::new()
        .route(RELOAD_PATH, get(reload))
        .fallback(any(forward))
        .layer(middleware::from_fn_with_state(
            Arc::clone(&state.token),
            require_token,
        ));
    Router::new()
        .route(BRIDGE_PATH, get(bridge))
        .route(ENTER_PATH, get(enter))
        .merge(guarded)
        .with_state(state)
}

/// The bridge source. Public: it is the same code for every project and reveals nothing.
async fn bridge() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        BRIDGE_JS,
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct EnterQuery {
    token: Option<String>,
    to: Option<String>,
}

/// `GET /__penwise/enter?token=…&to=/path` — sets the cookie and redirects, so no token stays in the URL.
async fn enter(State(state): State<Arc<App>>, Query(query): Query<EnterQuery>) -> Response {
    if !crate::auth::authorized(&state.token, query.token.as_deref(), &HeaderMap::new()) {
        return deny();
    }
    let cookie = format!(
        "{COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={COOKIE_MAX_AGE}",
        encode_cookie(&state.token)
    );
    let mut response = StatusCode::SEE_OTHER.into_response();
    let headers = response.headers_mut();
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        headers.insert(header::SET_COOKIE, value);
    }
    if let Ok(value) = HeaderValue::from_str(redirect_target(query.to.as_deref())) {
        headers.insert(header::LOCATION, value);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Where `enter` may send the browser: a local path, never another origin.
fn redirect_target(to: Option<&str>) -> &str {
    match to {
        Some(path)
            if path.starts_with('/')
                && !path.starts_with("//")
                && !path.chars().any(|c| matches!(c, '\r' | '\n' | '\\')) =>
        {
            path
        }
        _ => "/",
    }
}

/// `GET /__penwise/reload` — the bridge's live-reload socket (static projects only).
async fn reload(State(state): State<Arc<App>>, upgrade: WebSocketUpgrade) -> Response {
    let Some(site) = state.site.as_ref() else {
        return (
            StatusCode::NOT_FOUND,
            "live reload is only for static projects",
        )
            .into_response();
    };
    let reloads = site.subscribe();
    upgrade.on_upgrade(move |socket| pump(socket, reloads))
}

async fn pump(mut socket: WebSocket, mut reloads: broadcast::Receiver<()>) {
    loop {
        tokio::select! {
            event = reloads.recv() => match event {
                Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => {
                    // A lagged listener is still owed a reload; send one and move on.
                    if socket.send(Message::Text(RELOAD_MESSAGE.to_owned().into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            incoming = socket.recv() => match incoming {
                None | Some(Err(_) | Ok(Message::Close(_))) => return,
                Some(Ok(_)) => {}
            },
        }
    }
}

/// Everything else: proxy it or serve it.
async fn forward(State(state): State<Arc<App>>, request: Request) -> Response {
    if let Some(proxy) = &state.proxy {
        return proxy.forward(request).await;
    }
    let Some(site) = &state.site else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "project has no target").into_response();
    };
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return (StatusCode::METHOD_NOT_ALLOWED, "only GET and HEAD").into_response();
    }
    site.serve(request.uri()).await
}

/// Middleware: the token cookie, the header, or a loopback peer with a loopback `Host`.
async fn require_token(State(token): State<Arc<String>>, request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0.ip());
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if allowed(&token, peer, host, request.headers()) {
        next.run(request).await
    } else {
        deny()
    }
}

/// The access rule shared by every guarded project route.
fn allowed(token: &str, peer: Option<IpAddr>, host: Option<&str>, headers: &HeaderMap) -> bool {
    if crate::api::same_machine(peer, host) {
        return true;
    }
    let cookie = cookie_value(headers, COOKIE);
    crate::auth::authorized(token, cookie.as_deref(), headers)
}

fn deny() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "PenwiseCookie")],
        "missing or wrong Penwise token (open /__penwise/enter?token=… or send X-Penwise-Token)",
    )
        .into_response()
}

/// Reads one cookie from any `Cookie` header.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            if key.trim().eq_ignore_ascii_case(name) {
                percent_decode(value.trim())
            } else {
                None
            }
        })
}

/// Percent-encodes what a cookie value may not contain (and `%`, so decoding is unambiguous).
fn encode_cookie(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'!' | b'#'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b'-'
                    | b'.'
                    | b'^'
                    | b'_'
                    | b'`'
                    | b'|'
                    | b'~'
            );
        if safe {
            out.push(char::from(byte));
        } else {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("%{byte:02X}"));
        }
    }
    out
}

/// Percent-decodes a URL path or cookie value; invalid escapes or non-UTF-8 are a hard `None`.
pub(crate) fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = value.get(index + 1..index + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}
