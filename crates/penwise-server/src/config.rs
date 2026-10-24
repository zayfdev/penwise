//! Server configuration from command-line flags and environment (no config file in M3).
//!
//! Owner: slice c.
//!
//! Ports (documented in `docs/contracts/m3-server.md` "Network"):
//! - `7420` — sync WebSocket + HTTP API + web viewer + loopback-only pairing page, bound `0.0.0.0` so it
//!   survives DHCP changes and Tailscale coming up late; everything on it except static assets and
//!   `/api/health` is token-gated.
//! - `7421` — reserved for the MCP listener (M5; loopback + Tailscale IP only). Not bound in M3.
//!
//! Flags (each at most once; unknown flags are errors):
//! `--listen <ip:port>` (default `0.0.0.0:7420`), `--data-dir <path>`, `--name <mDNS instance label>`
//! (default `Penwise on <hostname>`), `--no-mdns`, `--no-tailscale`.
//! Data dir precedence: `--data-dir` > `PENWISE_DATA_DIR` > `$XDG_DATA_HOME/penwise` > `$HOME/.local/share/penwise`.

use std::net::SocketAddr;
use std::path::PathBuf;

/// Default sync/web port.
pub const DEFAULT_PORT: u16 = 7420;
/// Port reserved for the M5 MCP listener (never bound by M3).
pub const RESERVED_MCP_PORT: u16 = 7421;
/// SQLite file name inside the data dir.
pub const DB_FILE: &str = "penwise.sqlite3";
/// Longest mDNS instance label (DNS label limit).
pub const MAX_INSTANCE_NAME_BYTES: usize = 63;

/// Resolved configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Sync/web/pairing listener.
    pub listen: SocketAddr,
    /// Database and server state directory (created on start).
    pub data_dir: PathBuf,
    /// Host name (mDNS `<hostname>.local.`, default instance label); the `hostname` argument of `from_args`.
    pub hostname: String,
    /// mDNS instance label (`≤ MAX_INSTANCE_NAME_BYTES`).
    pub instance_name: String,
    /// Advertise `_penwise._tcp` on the LAN interface.
    pub mdns: bool,
    /// Poll `tailscale status --json` for the MagicDNS name and IPs (QR payload, allowed hosts).
    pub tailscale: bool,
}

/// Bad flags or environment.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("unknown flag {0}")]
    UnknownFlag(String),
    #[error("flag {0} needs a value")]
    MissingValue(&'static str),
    #[error("flag {0} given twice")]
    Repeated(&'static str),
    #[error("invalid listen address {0}")]
    BadListen(String),
    #[error("instance name must be 1..=63 bytes")]
    BadName,
    /// No `--data-dir`, `PENWISE_DATA_DIR`, `XDG_DATA_HOME` or `HOME`.
    #[error("cannot determine a data directory")]
    NoDataDir,
}

impl Config {
    /// Parses `args` (program name already skipped). `env` looks up environment variables (injected so
    /// tests stay hermetic); `hostname` feeds the default instance name.
    ///
    /// # Errors
    /// [`ConfigError`] for any invalid flag or a missing data dir.
    pub fn from_args(
        args: impl IntoIterator<Item = String>,
        env: &dyn Fn(&str) -> Option<String>,
        hostname: &str,
    ) -> Result<Self, ConfigError> {
        todo!()
    }

    /// `data_dir/DB_FILE`.
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        todo!()
    }

    /// Listener port (QR payload, mDNS, allowed origins).
    #[must_use]
    pub fn port(&self) -> u16 {
        todo!()
    }
}
