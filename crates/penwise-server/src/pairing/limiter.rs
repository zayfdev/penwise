//! Per-IP failed-handshake limiter (bad token, bad pairing code, malformed first frame).
//!
//! Owner: slice c.
//!
//! Sliding window: more than `MAX_FAILURES` failures from one IP within `WINDOW_MS` → further handshakes from
//! that IP get `Reject(RateLimited)` until the window clears. Loopback is never limited. Memory bounded by
//! `MAX_TRACKED_IPS` (oldest entries evicted). Secrets are 128/256-bit, so this is DoS hygiene, not the
//! brute-force defence.

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

/// Failures allowed per window.
pub const MAX_FAILURES: u32 = 10;
/// Window length.
pub const WINDOW_MS: u64 = 60_000;
/// Most IPs tracked.
pub const MAX_TRACKED_IPS: usize = 1024;

/// Shared limiter (clone = same state).
#[derive(Debug, Clone, Default)]
pub struct FailureLimiter {
    /// IP → failure timestamps (ms) inside the window.
    failures: Arc<Mutex<BTreeMap<IpAddr, Vec<u64>>>>,
}

impl FailureLimiter {
    /// Empty limiter.
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    /// True when `ip` may attempt a handshake now.
    #[must_use]
    pub fn allowed(&self, ip: IpAddr, now_ms: u64) -> bool {
        todo!()
    }

    /// Records one failed handshake from `ip`.
    pub fn record_failure(&self, ip: IpAddr, now_ms: u64) {
        todo!()
    }
}
