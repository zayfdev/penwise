//! Penwise server library: op-log storage, `/sync` sessions, the JSON HTTP API and the pieces the binary and
//! the in-process test harness share (`serve::serve` builds the one router both use).

mod agent;
mod anchors;
mod annotate;
pub mod api;
mod assets;
pub mod auth;
pub mod frames;
mod http;
pub mod inspect;
mod ocr;
pub mod pairing;
mod pictures;
mod qr;
mod render;
mod search;
pub mod serve;
mod sidecar;
pub mod storage;
pub mod sync;
mod textlayer;
pub mod urls;

#[cfg(test)]
mod testing;
