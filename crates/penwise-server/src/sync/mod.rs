//! `/sync`: binary postcard WebSocket sessions (design §4).
//!
//! - [`routes`] mounts the endpoint for a [`Hub`]; [`Hub::new`] takes the storage as a [`SyncStore`].
//! - A session authenticates with the shared token (`auth.rs`), gets `Welcome`, then a snapshot or an
//!   `OpsSince` catch-up, then live traffic. Pushed ops go through `SyncStore::append_ops` and are answered
//!   by one `Ack`.
//! - No-dup/no-gap: the session subscribes to committed batches *before* reading; each batch is classified
//!   against the session cursor ([`cursor::classify`]); a gap or broadcast lag is repaired from storage.
//! - Live strokes ride a separate droppable channel and never touch storage.

mod cursor;
mod delivery;
mod handshake;
mod hub;
mod limiter;
mod live;
mod outbound;
mod push;
mod session;
mod storage_adapter;
mod store;
mod transport;

#[cfg(test)]
mod tests;

pub use hub::{Hub, SyncConfig};
pub use store::{
    AppendFailure, LogStatus, OpFilter, OpsPage, OpsRead, SnapshotRead, StoreError, SyncStore,
};
pub use transport::routes;
