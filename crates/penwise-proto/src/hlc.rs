//! Hand-rolled hybrid logical clock (Kulkarni et al., "Logical Physical Clocks"; server-stack.md §4).
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
//! - `wall_ms` saturates at [`MAX_WALL_MS`] (year ≈ 6429), keeping [`Hlc::pack`] below 2⁶³ (`SQLite` `INTEGER`).
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
        Self {
            wall_ms: 0,
            counter: 0,
            device,
        }
    }

    /// `wall_ms << 16 | counter`; order-preserving for equal devices. Diagnostics and `received` metrics only:
    /// storage keeps the full [`Hlc::to_bytes`].
    #[must_use]
    pub fn pack(self) -> u64 {
        (self.wall_ms.min(MAX_WALL_MS) << 16) | u64::from(self.counter)
    }

    /// 24 bytes: `pack()` big-endian (8) then device bytes (16). Lexicographic byte order equals `Ord`, so
    /// `SQLite` `BLOB` comparison (`memcmp`) and `MAX(hlc)` follow the LWW order.
    #[must_use]
    pub fn to_bytes(self) -> [u8; HLC_BYTES] {
        let mut out = [0_u8; HLC_BYTES];
        let (head, tail) = out.split_at_mut(8);
        head.copy_from_slice(&self.pack().to_be_bytes());
        tail.copy_from_slice(&self.device.0);
        out
    }

    /// Inverse of [`Hlc::to_bytes`]; `None` for a wrong length or `wall_ms > MAX_WALL_MS`.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let bytes: &[u8; HLC_BYTES] = bytes.try_into().ok()?;
        let (head, tail) = bytes.split_at(8);
        let packed = u64::from_be_bytes(head.try_into().ok()?);
        let wall_ms = packed >> 16;
        if wall_ms > MAX_WALL_MS {
            return None;
        }
        Some(Self {
            wall_ms,
            counter: u16::try_from(packed & 0xFFFF).ok()?,
            device: DeviceId::from_slice(tail)?,
        })
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
        Self::resume(Hlc::zero(device))
    }

    /// Clock continuing after `last` (its device is the clock's device).
    #[must_use]
    pub fn resume(last: Hlc) -> Self {
        Self { last }
    }

    /// Last issued or observed stamp.
    #[must_use]
    pub fn last(&self) -> Hlc {
        self.last
    }

    /// Stamp for a new local op (module rule **send**). Never fails; strictly increasing.
    pub fn send(&mut self, now_ms: u64) -> Hlc {
        let l = self.last;
        let wall = l.wall_ms.max(now_ms.min(MAX_WALL_MS));
        let counter = if wall == l.wall_ms {
            Some(l.counter)
        } else {
            None
        };
        self.advance(wall, counter)
    }

    /// Observes a remote stamp (module rule **recv**) and returns the updated local stamp.
    ///
    /// # Errors
    /// [`HlcError::ClockSkew`] when `remote.wall_ms > now_ms + MAX_SKEW_MS`; the clock is unchanged.
    pub fn recv(&mut self, remote: Hlc, now_ms: u64) -> Result<Hlc, HlcError> {
        if remote.wall_ms > now_ms.saturating_add(MAX_SKEW_MS) {
            return Err(HlcError::ClockSkew {
                ahead_ms: remote.wall_ms - now_ms,
            });
        }
        let l = self.last;
        let wall = l
            .wall_ms
            .max(remote.wall_ms.min(MAX_WALL_MS))
            .max(now_ms.min(MAX_WALL_MS));
        let counter = [(l.wall_ms, l.counter), (remote.wall_ms, remote.counter)]
            .into_iter()
            .filter(|(w, _)| *w == wall)
            .map(|(_, c)| c)
            .max();
        Ok(self.advance(wall, counter))
    }

    /// Observes a stamp the server already logged: [`HlcClock::recv`], but a stamp too far ahead is still
    /// adopted (the log holds it, so every later local stamp must order after it or lose LWW against it).
    ///
    /// # Errors
    /// [`HlcError::ClockSkew`] after adopting the stamp, so the caller can report the skew.
    pub fn observe_logged(&mut self, remote: Hlc, now_ms: u64) -> Result<Hlc, HlcError> {
        match self.recv(remote, now_ms) {
            Ok(stamp) => Ok(stamp),
            Err(error) => {
                let adopted = Hlc {
                    device: self.last.device,
                    ..remote
                };
                self.last = self.last.max(adopted);
                Err(error)
            }
        }
    }

    /// Moves the clock back to `max(floor, now_ms)` (device kept). For a device whose own stamps ran ahead of
    /// the server clock (refused `ClockSkew`): `floor` is the newest remote stamp seen, so new stamps still
    /// order after everything the device has observed. Local stamps issued before are not ordered after the
    /// new ones; the caller re-stamps them.
    pub fn rebase(&mut self, floor: Hlc, now_ms: u64) {
        let device = self.last.device;
        let base = Hlc {
            wall_ms: now_ms.min(MAX_WALL_MS),
            counter: 0,
            device,
        };
        self.last = base.max(Hlc { device, ..floor });
    }

    /// Stores and returns the stamp after `(wall, counter)`: `counter` is the largest counter already seen at
    /// `wall` (`None` when `wall` is fresh), so the new counter is one above it; an exhausted counter moves
    /// `wall` forward by 1 ms.
    fn advance(&mut self, wall: u64, seen: Option<u16>) -> Hlc {
        let (wall, counter) = match seen {
            None => (wall, 0),
            Some(c) => match c.checked_add(1) {
                Some(next) => (wall, next),
                // Ceiling: the stamp cannot grow any further; keep the maximum instead of wrapping.
                None if wall >= MAX_WALL_MS => (MAX_WALL_MS, u16::MAX),
                None => (wall + 1, 0),
            },
        };
        self.last = Hlc {
            wall_ms: wall,
            counter,
            device: self.last.device,
        };
        self.last
    }
}

/// Client-side clock calibration from `Welcome.server_now_ms` (design §2.2). A device whose clock is far off
/// would otherwise be rejected `ClockSkew` (too far ahead) or lose every LWW race (too far behind): it feeds
/// `adjust(local_now)` instead of its raw wall clock to [`HlcClock::send`] / [`HlcClock::recv`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClockOffset {
    offset_ms: i64,
}

impl ClockOffset {
    /// No correction.
    pub const ZERO: Self = Self { offset_ms: 0 };

    /// Offset implied by one `Welcome`: `server_now_ms − local_now_ms`.
    #[must_use]
    pub fn from_welcome(server_now_ms: u64, local_now_ms: u64) -> Self {
        let diff = i128::from(server_now_ms) - i128::from(local_now_ms);
        Self {
            offset_ms: i64::try_from(diff).unwrap_or(if diff < 0 { i64::MIN } else { i64::MAX }),
        }
    }

    /// Persisted form (the host stores it next to the cursor).
    #[must_use]
    pub fn from_millis(offset_ms: i64) -> Self {
        Self { offset_ms }
    }

    /// The stored offset, ms.
    #[must_use]
    pub fn millis(self) -> i64 {
        self.offset_ms
    }

    /// Corrected time for `local_now_ms`, clamped to `0..=MAX_WALL_MS`.
    #[must_use]
    pub fn adjust(self, local_now_ms: u64) -> u64 {
        let corrected = i128::from(local_now_ms) + i128::from(self.offset_ms);
        u64::try_from(corrected.clamp(0, i128::from(MAX_WALL_MS))).unwrap_or(0)
    }
}
