//! `penwise-server` binary: parse config, open storage, start discovery, serve until Ctrl-C / SIGTERM.
//!
//! Owner: slice c.
//!
//! The only place `anyhow` is used (AGENTS.md "Rust"). Steps: `tracing_subscriber` with `RUST_LOG`
//! (`EnvFilter`, default `info`) → `penwise_server::Config::from_args` over `std::env::args().skip(1)`,
//! `std::env::var` and the host name (`/etc/hostname`, trimmed; fallback `penwise`) → create
//! `config.data_dir` recursively with mode `0o700` (`std::fs::DirBuilder` + `DirBuilderExt::mode`) →
//! `penwise_server::storage::Storage::open(&config.db_path())` → `penwise_server::App::start` →
//! `App::serve` with a shutdown future resolving on `tokio::signal::ctrl_c()` or SIGTERM
//! (`tokio::signal::unix`) → `MdnsAdvertiser::shutdown` if one was started.
#![allow(clippy::todo, reason = "M3 contract stubs")]

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    todo!()
}
