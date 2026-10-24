//! Shared application state, router assembly and the serve loop.
//!
//! Owner: slice c.

use std::sync::Arc;

use axum::Router;
use penwise_proto::ServerId;
use tokio::sync::watch;

use crate::config::Config;
use crate::pairing::{Endpoints, MdnsAdvertiser, Pairing};
use crate::storage::{Storage, StorageError};
use crate::sync::SyncHub;

/// Cheaply clonable handle passed as axum state to every route and session.
#[derive(Debug, Clone)]
pub struct App {
    pub config: Arc<Config>,
    pub storage: Storage,
    pub server: ServerId,
    pub hub: SyncHub,
    pub pairing: Pairing,
    /// Current LAN / Tailscale endpoints (QR payload, Host/Origin allow-lists).
    pub endpoints: watch::Receiver<Endpoints>,
}

/// Start-up and serve failures.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("bind {addr}: {source}")]
    Bind {
        addr: std::net::SocketAddr,
        source: std::io::Error,
    },
    #[error("serve: {0}")]
    Serve(std::io::Error),
    #[error("randomness unavailable")]
    Random,
}

/// Wall clock, Unix ms (the only clock source; pass its value into proto/storage functions).
#[must_use]
pub fn now_ms() -> u64 {
    todo!()
}

impl App {
    /// Builds the state: `storage.ensure_server_id(random)` (random via `pairing::token::random_server_id`),
    /// [`SyncHub::new`], [`Pairing::new`], and discovery ([`crate::pairing::spawn_discovery`]: LAN address
    /// now, Tailscale polling if `config.tailscale`). Returns the app and the mDNS advertiser (when
    /// `config.mdns`; failure to advertise is logged, not fatal → `None`). Must run inside a tokio runtime.
    ///
    /// # Errors
    /// [`AppError::Storage`], [`AppError::Random`].
    pub async fn start(
        config: Config,
        storage: Storage,
    ) -> Result<(Self, Option<MdnsAdvertiser>), AppError> {
        todo!()
    }

    /// Full router: `sync::routes` + `pairing::routes` + `api::routes` + `web::routes` (fallback), with
    /// `tower_http` `TraceLayer` and a `RequestBodyLimitLayer` of 64 KiB (no route takes a large body;
    /// WebSocket frames are limited separately to `MAX_FRAME_BYTES`).
    #[must_use]
    pub fn router(&self) -> Router {
        todo!()
    }

    /// Binds `config.listen` and serves `router()` with
    /// `into_make_service_with_connect_info::<SocketAddr>()` (pairing and sync need the peer address) until
    /// `shutdown` resolves; then closes sessions (`SyncHub::shutdown`) and returns.
    ///
    /// # Errors
    /// [`AppError::Bind`], [`AppError::Serve`].
    pub async fn serve(self, shutdown: impl Future<Output = ()> + Send + 'static) -> Result<(), AppError> {
        todo!()
    }
}
