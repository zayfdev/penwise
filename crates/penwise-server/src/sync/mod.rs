//! WebSocket sync: upgrade route, the hub shared by all sessions (broadcast, current page, session registry).
//!
//! Owner: slice b.
//!
//! `GET /sync` → `WebSocketUpgrade` with `max_message_size(MAX_FRAME_BYTES)` → [`session::run`]. Before
//! upgrading, a present `Origin` header must be an `http`/`https` origin whose host (port ignored, so the
//! Vite dev server on :5173 works) is `localhost`/`127.0.0.1`/`[::1]`, the LAN address, or the Tailscale
//! name/IPs from [`crate::pairing::Endpoints`]; otherwise 403 — blocks cross-site WebSocket hijacking from a
//! browser tab. Absent `Origin` (tablet, tests) is allowed: auth is the first frame either way.

pub mod broadcast;
pub mod session;

use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::HeaderMap;
use penwise_proto::{DeviceId, PageId};

use crate::app::App;
use crate::pairing::Endpoints;
use broadcast::Broadcast;

/// Process-unique session number (logs, registry key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(pub u64);

/// Shared sync state (clone = same hub).
#[derive(Debug, Clone)]
pub struct SyncHub {
    inner: Arc<HubInner>,
}

/// Slice-internal state (reshape freely).
#[derive(Debug)]
struct HubInner {
    broadcast: Broadcast,
    current_page: Mutex<Option<PageId>>,
    sessions: Mutex<BTreeMap<SessionId, session::SessionHandle>>,
    next_session: AtomicU64,
}

impl SyncHub {
    /// Empty hub with a `BROADCAST_CAPACITY` channel.
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    /// The fan-out channel.
    #[must_use]
    pub fn broadcast(&self) -> &Broadcast {
        todo!()
    }

    /// Last page any client switched to (`Welcome::current_page`).
    #[must_use]
    pub fn current_page(&self) -> Option<PageId> {
        todo!()
    }

    /// Records `page` as current and broadcasts `SwitchPage { page, by }` to every other session.
    pub fn switch_page(&self, page: PageId, by: DeviceId) {
        todo!()
    }

    /// Registers a new authenticated session; the handle lets the hub close it.
    pub(crate) fn register(&self, device: DeviceId) -> (SessionId, session::CloseSignal) {
        todo!()
    }

    /// Removes a finished session.
    pub(crate) fn unregister(&self, id: SessionId) {
        todo!()
    }

    /// Closes every open session of `device` (after token revocation).
    pub fn disconnect_device(&self, device: DeviceId) {
        todo!()
    }

    /// Closes every session (server shutdown; close code 1001).
    pub fn shutdown(&self) {
        todo!()
    }

    /// Open authenticated sessions.
    #[must_use]
    pub fn session_count(&self) -> usize {
        todo!()
    }
}

impl Default for SyncHub {
    fn default() -> Self {
        Self::new()
    }
}

/// `/sync` route.
pub fn routes() -> Router<App> {
    todo!()
}

/// Module-doc `Origin` rule. `None` origin → true.
#[must_use]
pub fn origin_allowed(headers: &HeaderMap, endpoints: &Endpoints, port: u16) -> bool {
    todo!()
}
