//! The server: storage, `/sync` and the HTTP API on a caller-supplied listener. The binary and the in-process
//! test harness both call [`serve`]; the clock and a fault plan are injectable, everything else is the
//! production wiring.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::api::{self, AppState};
use crate::assets::Assets;
use crate::pairing::Pairing;
use crate::storage::{FaultPlan, NowMs, Storage, StorageConfig, StorageError, system_now_ms};
use crate::sync::{Hub, SyncConfig};
use crate::urls::Urls;

/// File name of the database inside the data directory.
pub const DB_FILE: &str = "penwise.db";

/// Where a local Ollama listens unless `PENWISE_OCR_URL` says otherwise.
pub const DEFAULT_OCR_URL: &str = "http://127.0.0.1:11434";

/// How to build a server.
#[allow(missing_debug_implementations, reason = "holds the clock closure")]
pub struct ServerConfig {
    pub data_dir: PathBuf,
    /// Shared access token (empty accepts any).
    pub token: String,
    /// Wall clock (Unix ms) for storage and sync.
    pub clock: NowMs,
    /// Capacity of the committed-ops broadcast (`None` = storage default); tiny values force lag.
    pub broadcast_capacity: Option<usize>,
    /// Fault injection into the writer.
    pub fault: Option<FaultPlan>,
    /// Overrides for the sync layer (timeouts, queue sizes); the token and clock fields are replaced.
    pub sync: Option<SyncConfig>,
    /// Directory of the static viewer (missing files answer 404).
    pub web_dir: PathBuf,
    /// URLs handed out when pairing; empty means discover them per request (LAN, Tailscale, loopback).
    pub urls: Vec<String>,
    /// Base URL of the local vision model (Ollama) that reads handwriting.
    pub ocr_url: String,
}

impl ServerConfig {
    /// Defaults: system clock, default capacities, no faults.
    #[must_use]
    pub fn new(data_dir: impl Into<PathBuf>, token: impl Into<String>) -> Self {
        Self {
            data_dir: data_dir.into(),
            token: token.into(),
            clock: Arc::new(system_now_ms),
            broadcast_capacity: None,
            fault: None,
            sync: None,
            web_dir: PathBuf::from("web"),
            urls: Vec::new(),
            ocr_url: std::env::var("PENWISE_OCR_URL")
                .unwrap_or_else(|_| DEFAULT_OCR_URL.to_owned()),
        }
    }
}

/// A running server.
#[allow(
    missing_debug_implementations,
    reason = "holds the storage handle and a task"
)]
pub struct ServerHandle {
    pub addr: SocketAddr,
    pub storage: Storage,
    pub hub: Arc<Hub<Storage>>,
    pub pairing: Arc<Pairing>,
    task: JoinHandle<()>,
}

impl ServerHandle {
    /// Closes every session (`Error{ShuttingDown}`), stops accepting and waits for the listener to finish.
    pub async fn shutdown(self) {
        self.hub.shutdown();
        self.task.abort();
        let _ = self.task.await;
    }

    /// Resolves when the listener task ends (it only does on an accept error or a panic).
    pub async fn wait(self) {
        let _ = self.task.await;
    }
}

/// Opens storage and serves `/sync`, the HTTP API and the viewer on `listener`.
///
/// # Errors
/// Storage open failures (`NeedsMigration`, …) and a listener without a local address.
pub async fn serve(
    listener: TcpListener,
    config: ServerConfig,
) -> Result<ServerHandle, ServeError> {
    let mut storage_config = StorageConfig::new(config.data_dir.join(DB_FILE));
    storage_config.now_ms = Arc::clone(&config.clock);
    if let Some(capacity) = config.broadcast_capacity {
        storage_config.broadcast_capacity = capacity;
    }
    storage_config.fault = config.fault;
    std::fs::create_dir_all(&config.data_dir).map_err(|e| ServeError::Io(e.to_string()))?;
    let addr = listener
        .local_addr()
        .map_err(|e| ServeError::Io(e.to_string()))?;
    let addr_port = addr.port();
    let storage = tokio::task::spawn_blocking(move || Storage::open(storage_config))
        .await
        .map_err(|e| ServeError::Io(e.to_string()))??;
    let mut sync_config = config
        .sync
        .unwrap_or_else(|| SyncConfig::new(String::new()));
    let sync_config_token = config.token.clone();
    sync_config.token = config.token;
    sync_config.now_ms = config.clock;
    let hub = Hub::new(storage.clone(), sync_config);
    let pairing = Arc::new(Pairing::new());
    let app = AppState {
        storage: storage.clone(),
        token: Arc::new(sync_config_token),
        assets: Assets::load(config.data_dir.join("assets")),
        data_dir: Arc::new(config.data_dir),
        web: Arc::new(config.web_dir),
        pairing: Arc::clone(&pairing),
        urls: Arc::new(if config.urls.is_empty() {
            Urls::discovered(addr_port)
        } else {
            Urls::fixed(config.urls)
        }),
        ocr_url: Arc::new(config.ocr_url),
    };
    let router = Router::new()
        .merge(api::router(app))
        .merge(crate::sync::routes(Arc::clone(&hub)));
    let task = tokio::spawn(async move {
        let app = router.into_make_service_with_connect_info::<SocketAddr>();
        if let Err(error) = axum::serve(listener, app).await {
            tracing::warn!(%error, "server stopped");
        }
    });
    Ok(ServerHandle {
        addr,
        storage,
        hub,
        pairing,
        task,
    })
}

/// Why [`serve`] failed.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("io: {0}")]
    Io(String),
}
