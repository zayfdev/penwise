//! Server-local metadata that sits beside the op log in `penwise.db` and is never synced: picture labels and
//! the element maps of captured pages.
//!
//! Each call opens its own short-lived connection on the blocking pool (WAL, busy timeout), so these writes
//! never touch the storage writer thread: they carry no ops and nothing waits on them.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use rusqlite::Connection;

use crate::api::ApiError;
use crate::serve::DB_FILE;

fn open(data_dir: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(data_dir.join(DB_FILE))?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}

/// Runs `work` with a connection on the blocking pool.
pub(crate) async fn with_connection<T: Send + 'static>(
    data_dir: &Arc<PathBuf>,
    work: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    let data_dir = Arc::clone(data_dir);
    tokio::task::spawn_blocking(move || work(&open(&data_dir)?))
        .await
        .map_err(|error| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map_err(|error| {
            tracing::error!(%error, "sidecar metadata failure");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "metadata failure")
        })
}
