//! State shared by every `/sync` session: storage, token, clock, live channel, limiter.

use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use penwise_proto::PageId;
use penwise_proto::message::{HANDSHAKE_TIMEOUT_MS, IDLE_TIMEOUT_MS, PING_INTERVAL_MS};
use tokio::sync::{broadcast, watch};

use super::limiter::HandshakeLimiter;
use super::store::SyncStore;

/// Knobs of the sync layer (tests shrink them).
#[allow(missing_debug_implementations, reason = "holds the clock closure")]
#[derive(Clone)]
pub struct SyncConfig {
    /// The shared access token (`auth.rs`); empty accepts any `Hello` token.
    pub token: String,
    /// Wall clock in Unix ms (calibrates client clocks; injectable).
    pub now_ms: Arc<dyn Fn() -> u64 + Send + Sync>,
    /// Capacity of the live-delta broadcast (a lagging session skips deltas).
    pub live_capacity: usize,
    /// Frames queued towards one socket before reliable sends wait.
    pub outbound_capacity: usize,
    pub handshake_timeout: Duration,
    pub ping_interval: Duration,
    /// How often a connected tablet or agent has its delivery cursor re-recorded as its ack, so a long-lived
    /// connection does not pin the retention floor at its `Hello`.
    pub ack_interval: Duration,
    pub idle_timeout: Duration,
}

impl SyncConfig {
    /// Defaults from the protocol constants, `now_ms` = system clock.
    #[must_use]
    pub fn new(token: String) -> Self {
        Self {
            token,
            now_ms: Arc::new(system_now_ms),
            live_capacity: 256,
            outbound_capacity: 64,
            handshake_timeout: Duration::from_millis(HANDSHAKE_TIMEOUT_MS),
            ping_interval: Duration::from_millis(PING_INTERVAL_MS),
            ack_interval: Duration::from_secs(60),
            idle_timeout: Duration::from_millis(IDLE_TIMEOUT_MS),
        }
    }
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// One live (droppable) frame fanned out to sessions.
#[derive(Debug, Clone)]
pub struct LiveEvent {
    /// Producing session (never receives its own event).
    pub origin: u64,
    /// `Some`: only sessions watching this page; `None`: everyone.
    pub page: Option<PageId>,
    /// Encoded `ServerMsg` frame.
    pub frame: Arc<[u8]>,
}

#[allow(
    missing_debug_implementations,
    reason = "holds channels and the clock closure"
)]
pub struct Hub<S: SyncStore> {
    pub store: S,
    pub config: SyncConfig,
    live: broadcast::Sender<LiveEvent>,
    limiter: Mutex<HandshakeLimiter>,
    next_session: AtomicU64,
    shutdown: watch::Sender<bool>,
}

impl<S: SyncStore> Hub<S> {
    #[must_use]
    pub fn new(store: S, config: SyncConfig) -> Arc<Self> {
        let (live, _) = broadcast::channel(config.live_capacity.max(1));
        Arc::new(Self {
            store,
            config,
            live,
            limiter: Mutex::new(HandshakeLimiter::default()),
            next_session: AtomicU64::new(1),
            shutdown: watch::channel(false).0,
        })
    }

    /// Tells every session to send `Error{ShuttingDown}` and close.
    pub fn shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    pub(super) fn shutdown_signal(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    pub(super) fn now_ms(&self) -> u64 {
        (self.config.now_ms)()
    }

    pub(super) fn next_session_id(&self) -> u64 {
        self.next_session.fetch_add(1, Ordering::Relaxed)
    }

    pub(super) fn subscribe_live(&self) -> broadcast::Receiver<LiveEvent> {
        self.live.subscribe()
    }

    pub(super) fn publish_live(&self, event: LiveEvent) {
        // No receivers is normal (nobody else connected).
        let _ = self.live.send(event);
    }

    pub(super) fn handshake_allowed(&self, peer: Option<IpAddr>) -> bool {
        let now = self.now_ms();
        self.limiter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .allowed(peer, now)
    }

    pub(super) fn handshake_failed(&self, peer: Option<IpAddr>) {
        let now = self.now_ms();
        self.limiter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failed(peer, now);
    }

    pub(super) fn handshake_succeeded(&self, peer: Option<IpAddr>) {
        self.limiter
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .succeeded(peer);
    }
}
