//! Penwise PC server: SQLite op log, WebSocket sync with live ink fan-out, QR pairing + device tokens,
//! mDNS/Tailscale discovery, page → PNG render and the embedded browser viewer (PLAN "Architecture").
//!
//! Owner: contract. Only the contract owner edits this file; the M3 contract lives in
//! `docs/contracts/m3-server.md`. `main.rs` is a thin binary over this library so integration tests
//! (`tests/`) can start a server in-process.
//!
//! One listener (`Config::listen`, default `0.0.0.0:7420`) serves everything; the MCP listener (7421) arrives
//! in M5. Routes:
//! - `GET /sync` — WebSocket; auth = first frame (`Hello`/`Pair`) ([`sync`]).
//! - `GET /pair`, `GET /pair/qr.svg` — pairing code + QR, loopback-only ([`pairing`]).
//! - `GET /api/health`, `GET /api/pages/{page}/png`, `GET /api/pages/{page}/items` — bearer token ([`api`]).
//! - everything else — embedded viewer assets ([`web`]).
//!
//! Blocking SQLite work runs through [`storage::Storage`]'s synchronous API inside
//! `tokio::task::spawn_blocking`; nothing else blocks the runtime.
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::unused_self,
    clippy::unused_async,
    unused_variables,
    dead_code,
    reason = "M3 contract stubs"
)]

pub mod api;
pub mod app;
pub mod config;
pub mod pairing;
pub mod render;
pub mod storage;
pub mod sync;
pub mod web;

pub use app::{App, AppError, now_ms};
pub use config::{Config, ConfigError};
