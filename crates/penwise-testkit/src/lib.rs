//! Dev-only test harness for the Penwise server.
//!
//! - [`TestDevice`]: a real WebSocket client built from `penwise-proto` pieces (HLC clock, outbox, replica) that
//!   can go online/offline, crash and restart, and inject raw bytes.
//! - [`RawSocket`]: the bare transport, for handshake and malformed-frame scenarios.
//! - [`fixture`]: generator of historical v3 databases (migration tests).
//! - [`check`]: convergence helpers (replica fingerprints, folds of the server's `ops` and item tables).
//!
//! The in-process `TestServer` lives in `penwise-server/tests/common` (it needs the server crate; this crate must
//! not depend on it).

pub mod check;
mod device;
mod device_ops;
mod device_pump;
pub mod fixture;
pub mod ids;
mod live;
mod socket;
pub mod tempdir;

pub use device::{DeviceConfig, Events, TestDevice, Welcome};
pub use device_ops::stroke_item;
pub use ids::IdGen;
pub use live::delta as live_delta;
pub use socket::{RawSocket, Recv};
pub use tempdir::TempDir;

use std::sync::Arc;

/// Injectable wall clock, milliseconds since the Unix epoch.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The system clock.
#[must_use]
pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    })
}
