//! Pairing and device auth: one-time pairing codes → long-lived device tokens, the loopback-only pairing page,
//! LAN/Tailscale endpoint discovery and mDNS advertisement (PLAN "Networking & pairing").
//!
//! Owner: slice c.
//!
//! Flow:
//! 1. The PC browser (viewer `ui/pairing.ts`) opens `GET /pair` **from loopback** ([`local::is_local_request`]):
//!    the server issues a fresh code (`PAIRING_CODE_TTL_MS`, single use, kept only in memory as a hash) and
//!    returns `{ uri, code, svg, expires_ms }` where `uri = penwise://pair?…` ([`qr::PairingPayload`]) and
//!    `svg` its QR. `GET /pair/qr.svg` returns just the SVG of a fresh code.
//! 2. The tablet scans the QR (or a remote browser types the code), opens `/sync` on an endpoint from the
//!    payload and sends `ClientMsg::Pair { code, device, name, client }`.
//! 3. [`Pairing::redeem`] consumes the code, generates a 32-byte token, stores `SHA-256(token)` with the
//!    device (`Storage::pair_device`) and returns it once in `ServerMsg::Paired`.
//! 4. Every later connection authenticates with `Hello { device, token }` ([`Pairing::authenticate`]); HTTP API
//!    calls use `Authorization: Bearer <token>` ([`token::require_bearer`]).
//!
//! Loopback-only device management: `GET /pair/devices` (JSON list), `DELETE /pair/devices/{id}` (revoke all
//! tokens + `SyncHub::disconnect_device`).

pub mod lan;
pub mod limiter;
pub mod local;
pub mod mdns;
pub mod qr;
pub mod tailscale;
pub mod token;

use std::collections::BTreeMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use axum::Router;
use penwise_proto::{AuthToken, ClientKind, DeviceId, PairingCode, ServerId};
use tokio::sync::watch;

use crate::app::App;
use crate::config::Config;
use crate::storage::{Storage, StorageError};

pub use mdns::MdnsAdvertiser;
pub use tailscale::TailscaleInfo;

/// Lifetime of a pairing code.
pub const PAIRING_CODE_TTL_MS: u64 = 120_000;
/// Most unexpired codes held at once; issuing another evicts the oldest.
pub const MAX_LIVE_CODES: usize = 8;
/// Longest device name (UTF-8 bytes).
pub const MAX_DEVICE_NAME_BYTES: usize = 64;

/// Addresses clients can reach this server on (QR payload, allowed Host/Origin values).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Endpoints {
    /// Private IPv4 of the physical LAN interface ([`lan::lan_ipv4`]).
    pub lan: Option<Ipv4Addr>,
    /// Tailscale identity while the backend is running.
    pub tailscale: Option<TailscaleInfo>,
}

/// Pairing failures (handshake rejections map them to `RejectReason`).
#[derive(Debug, thiserror::Error)]
pub enum PairingError {
    /// Unknown, used or expired code.
    #[error("bad pairing code")]
    BadCode,
    /// Unknown or revoked token, or a token of another device.
    #[error("bad token")]
    BadToken,
    /// Empty or over-long device name.
    #[error("bad device name")]
    BadName,
    #[error("randomness unavailable")]
    Random,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

/// Code store + token checks (clone = same state).
#[derive(Debug, Clone)]
pub struct Pairing {
    storage: Storage,
    server: ServerId,
    /// SHA-256(code) → expiry ms.
    codes: Arc<Mutex<BTreeMap<[u8; 32], u64>>>,
    limiter: limiter::FailureLimiter,
}

impl Pairing {
    /// Empty code store.
    #[must_use]
    pub fn new(storage: Storage, server: ServerId) -> Self {
        todo!()
    }

    /// Failed-handshake limiter shared by `/sync` sessions.
    #[must_use]
    pub fn limiter(&self) -> &limiter::FailureLimiter {
        todo!()
    }

    /// Issues a fresh code (drops expired ones; evicts the oldest beyond `MAX_LIVE_CODES`). Returns the code
    /// and its expiry (Unix ms).
    ///
    /// # Errors
    /// [`PairingError::Random`].
    pub fn issue_code(&self, now_ms: u64) -> Result<(PairingCode, u64), PairingError> {
        todo!()
    }

    /// Consumes `code` (single use even on later failure) and pairs `device`: validates `name`
    /// (`1..=MAX_DEVICE_NAME_BYTES` after trimming), generates a token, `Storage::pair_device` (blocking:
    /// call from `spawn_blocking`). Returns the token (the only time it exists in clear).
    ///
    /// # Errors
    /// `BadCode`, `BadName`, `Random`, `Storage`.
    pub fn redeem(
        &self,
        code: &PairingCode,
        device: DeviceId,
        name: &str,
        kind: ClientKind,
        now_ms: u64,
    ) -> Result<AuthToken, PairingError> {
        todo!()
    }

    /// Checks `token` belongs to `device` and is not revoked: hash → `Storage::token_device` → constant-time
    /// compare of the stored hash and device match → `Storage::touch_token` (blocking).
    ///
    /// # Errors
    /// `BadToken`, `Storage`.
    pub fn authenticate(
        &self,
        device: DeviceId,
        token: &AuthToken,
        now_ms: u64,
    ) -> Result<(), PairingError> {
        todo!()
    }

    /// Bearer check for HTTP routes: the device owning a valid token (any device).
    ///
    /// # Errors
    /// `BadToken`, `Storage`.
    pub fn device_for_bearer(&self, token: &str, now_ms: u64) -> Result<DeviceId, PairingError> {
        todo!()
    }
}

/// Starts discovery: computes `lan_ipv4()` now (and every `tailscale::POLL_INTERVAL_MS`), polls Tailscale
/// when `config.tailscale`, and publishes changes on the returned channel. Runs until the runtime stops.
#[must_use]
pub fn spawn_discovery(config: &Config) -> watch::Receiver<Endpoints> {
    todo!()
}

/// `/pair`, `/pair/qr.svg`, `/pair/devices`, `/pair/devices/{id}`: all behind [`local::require_local`]; JSON
/// responses carry `Cache-Control: no-store`.
pub fn routes() -> Router<App> {
    todo!()
}
