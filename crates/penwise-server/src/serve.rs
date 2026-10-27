//! In-process server: storage + `/sync` on a caller-supplied listener (the test harness and, from S4, the
//! binary). Injectable clock and fault plan; everything else is the production wiring.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::storage::{FaultPlan, NowMs, Storage, StorageConfig, StorageError, system_now_ms};
use crate::sync::{Hub, SyncConfig};

/// File name of the database inside the data directory.
pub const DB_FILE: &str = "penwise.db";

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
    task: JoinHandle<()>,
}

impl ServerHandle {
    /// Closes every session (`Error{ShuttingDown}`), stops accepting and waits for the listener to finish.
    pub async fn shutdown(self) {
        self.hub.shutdown();
        self.task.abort();
        let _ = self.task.await;
    }
}

/// Opens storage and serves `/sync` (and `/health`) on `listener`.
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
    let storage = tokio::task::spawn_blocking(move || Storage::open(storage_config))
        .await
        .map_err(|e| ServeError::Io(e.to_string()))??;
    let mut sync_config = config
        .sync
        .unwrap_or_else(|| SyncConfig::new(String::new()));
    sync_config.token = config.token;
    sync_config.now_ms = config.clock;
    let hub = Hub::new(storage.clone(), sync_config);
    let router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(crate::sync::routes(Arc::clone(&hub)));
    let addr = listener
        .local_addr()
        .map_err(|e| ServeError::Io(e.to_string()))?;
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
