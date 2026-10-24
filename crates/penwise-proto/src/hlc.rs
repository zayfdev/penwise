//! Hand-rolled hybrid logical clock (Kulkarni et al., "Logical Physical Clocks"; server-stack.md §4).
//!
//! Owner: slice b.
//!
//! An [`Hlc`] is `(wall_ms, counter, device)`. The derived `Ord` compares exactly in that field order, which
//! is the protocol's total order and the LWW rule: of two writes to the same register the larger `Hlc` wins;
//! equal `(wall_ms, counter)` is broken by [`DeviceId`] bytes, so two distinct devices never tie and two
//! writes from one device are strictly ordered by its clock.
//!
//! Rules (`l` = the clock's last value, `pt` = injected physical time `now_ms`):
//! - **send** (local event, new op): `w = max(l.wall_ms, pt)`; `c = l.counter + 1` if `w == l.wall_ms` else `0`.
//! - **recv** (remote op with stamp `m`): reject when `m.wall_ms > pt + MAX_SKEW_MS`; else
//!   `w = max(l.wall_ms, m.wall_ms, pt)`; `c = max(l.counter, m.counter) + 1` if `w == l.wall_ms == m.wall_ms`,
//!   `l.counter + 1` if only `w == l.wall_ms`, `m.counter + 1` if only `w == m.wall_ms`, else `0`.
//! - **overflow**: a counter that would pass `u16::MAX` instead moves `wall_ms` forward by 1 ms with counter 0
//!   (never wraps, never errors).
//! - `wall_ms` saturates at [`MAX_WALL_MS`] (year ≈ 6429), keeping [`Hlc::pack`] below 2⁶³ (SQLite `INTEGER`).
//!
//! The result of both operations is always strictly greater than `l` and than `m`, so every op a device
//! creates after seeing an op is ordered after it (causality ⇒ LWW order).

use serde::{Deserialize, Serialize};

use crate::ids::DeviceId;

/// Largest accepted lead of a remote stamp over local physical time (5 minutes).
pub const MAX_SKEW_MS: u64 = 5 * 60 * 1000;
/// Largest `wall_ms` (47 bits) so `pack()` fits a non-negative `i64`.
pub const MAX_WALL_MS: u64 = (1 << 47) - 1;
/// Size of [`Hlc::to_bytes`].
pub const HLC_BYTES: usize = 24;

/// One hybrid-logical-clock stamp. Field order is the comparison order (do not reorder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hlc {
    /// Physical component, Unix ms (`≤ MAX_WALL_MS`).
    pub wall_ms: u64,
    /// Logical component within one `wall_ms`.
    pub counter: u16,
    /// Writer; final tie-breaker.
    pub device: DeviceId,
}

/// Clock failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HlcError {
    /// Remote stamp is more than [`MAX_SKEW_MS`] ahead of local physical time (a device with a wrong clock;
    /// accepting it would let its writes win every later LWW comparison).
    #[error("remote clock {ahead_ms} ms ahead")]
    ClockSkew { ahead_ms: u64 },
}

impl Hlc {
    /// The smallest stamp of `device` (`wall_ms = 0, counter = 0`): initial clock state.
    #[must_use]
    pub fn zero(device: DeviceId) -> Self {
        todo!()
    }

    /// `wall_ms << 16 | counter`; order-preserving for equal devices. Diagnostics and `received` metrics only:
    /// storage keeps the full [`Hlc::to_bytes`].
    #[must_use]
    pub fn pack(self) -> u64 {
        todo!()
    }

    /// 24 bytes: `pack()` big-endian (8) then device bytes (16). Lexicographic byte order equals `Ord`, so
    /// SQLite `BLOB` comparison (`memcmp`) and `MAX(hlc)` follow the LWW order.
    #[must_use]
    pub fn to_bytes(self) -> [u8; HLC_BYTES] {
        todo!()
    }

    /// Inverse of [`Hlc::to_bytes`]; `None` for a wrong length or `wall_ms > MAX_WALL_MS`.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        todo!()
    }
}

/// One device's clock. Persist [`HlcClock::last`] (or recompute it as the max stamp in the local log) and
/// [`HlcClock::resume`] after a restart so stamps never go backwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HlcClock {
    last: Hlc,
}

impl HlcClock {
    /// Fresh clock at [`Hlc::zero`].
    #[must_use]
    pub fn new(device: DeviceId) -> Self {
        todo!()
    }

    /// Clock continuing after `last` (its device is the clock's device).
    #[must_use]
    pub fn resume(last: Hlc) -> Self {
        todo!()
    }

    /// Last issued or observed stamp.
    #[must_use]
    pub fn last(&self) -> Hlc {
        todo!()
    }

    /// Stamp for a new local op (module rule **send**). Never fails; strictly increasing.
    pub fn send(&mut self, now_ms: u64) -> Hlc {
        todo!()
    }

    /// Observes a remote stamp (module rule **recv**) and returns the updated local stamp.
    ///
    /// # Errors
    /// [`HlcError::ClockSkew`] when `remote.wall_ms > now_ms + MAX_SKEW_MS`; the clock is unchanged.
    pub fn recv(&mut self, remote: Hlc, now_ms: u64) -> Result<Hlc, HlcError> {
        todo!()
    }
}
