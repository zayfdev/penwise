//! Failed-handshake limiter: too many failures from one peer inside a window → `RateLimited`.

use std::collections::HashMap;
use std::net::IpAddr;

/// Failures allowed per window before a peer is refused.
pub const MAX_FAILURES: u32 = 10;
/// Window length.
pub const WINDOW_MS: u64 = 60_000;
/// Peers remembered at most (oldest windows are dropped first when full).
const MAX_PEERS: usize = 1024;

#[derive(Debug, Default)]
pub struct HandshakeLimiter {
    peers: HashMap<Option<IpAddr>, (u64, u32)>,
}

impl HandshakeLimiter {
    /// True when `peer` may attempt a handshake at `now_ms`.
    pub fn allowed(&mut self, peer: Option<IpAddr>, now_ms: u64) -> bool {
        match self.peers.get(&peer) {
            Some(&(start, count)) if now_ms.saturating_sub(start) < WINDOW_MS => {
                count < MAX_FAILURES
            }
            Some(_) => {
                self.peers.remove(&peer);
                true
            }
            None => true,
        }
    }

    /// Records one failed handshake.
    pub fn failed(&mut self, peer: Option<IpAddr>, now_ms: u64) {
        if self.peers.len() >= MAX_PEERS && !self.peers.contains_key(&peer) {
            self.peers
                .retain(|_, &mut (start, _)| now_ms.saturating_sub(start) < WINDOW_MS);
            if self.peers.len() >= MAX_PEERS {
                return;
            }
        }
        let entry = self.peers.entry(peer).or_insert((now_ms, 0));
        if now_ms.saturating_sub(entry.0) >= WINDOW_MS {
            *entry = (now_ms, 0);
        }
        entry.1 = entry.1.saturating_add(1);
    }

    /// A successful handshake clears the peer's failures.
    pub fn succeeded(&mut self, peer: Option<IpAddr>) {
        self.peers.remove(&peer);
    }
}
