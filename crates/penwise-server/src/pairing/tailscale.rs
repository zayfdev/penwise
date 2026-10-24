//! Tailscale identity via `tailscale status --json` (server-stack.md §5 "Discovering the Tailscale identity").
//!
//! Owner: slice c.
//!
//! Deserializes only `BackendState`, `Self.DNSName`, `Self.TailscaleIPs` (every field `#[serde(default)]`;
//! the CLI warns its format may change). Missing binary, non-zero exit, `BackendState != "Running"` or an
//! empty DNS name → `Ok(None)` (Tailscale simply not offered). Traffic over Tailscale is WireGuard-encrypted.

use std::net::IpAddr;

/// Re-poll period.
pub const POLL_INTERVAL_MS: u64 = 60_000;
/// Kill the CLI if it has not answered by then.
pub const COMMAND_TIMEOUT_MS: u64 = 5_000;

/// This node's tailnet identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TailscaleInfo {
    /// MagicDNS name without the trailing dot, e.g. `deskpc.tail000000.ts.net`.
    pub dns_name: String,
    /// Tailscale addresses (IPv4 `100.64.0.0/10` first, then IPv6).
    pub ips: Vec<IpAddr>,
}

/// Unparseable CLI output.
#[derive(Debug, thiserror::Error)]
pub enum TailscaleError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Pure parser of the CLI's JSON (tests feed recorded output).
///
/// # Errors
/// [`TailscaleError::Json`] for invalid JSON (missing fields are not errors).
pub fn parse_status(json: &[u8]) -> Result<Option<TailscaleInfo>, TailscaleError> {
    todo!()
}

/// Runs `tailscale status --json` (`tokio::process::Command`, `COMMAND_TIMEOUT_MS`) and parses it; any
/// process failure is `Ok(None)` (logged at `debug`).
///
/// # Errors
/// [`TailscaleError::Json`].
pub async fn query() -> Result<Option<TailscaleInfo>, TailscaleError> {
    todo!()
}
