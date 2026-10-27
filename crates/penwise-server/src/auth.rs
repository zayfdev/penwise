//! Access token for the LAN socket.
//!
//! One token per data directory, created on first run from `/dev/urandom`. Every route that reveals or changes the
//! page requires it (`?token=…` or `X-Penwise-Token`); the viewer's static assets do not, since they are public code.

use std::io::Read;
use std::path::Path;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// `PENWISE_TOKEN` wins (useful in tests); otherwise `${data}/token`, created when missing.
pub fn load_or_create(data_dir: &Path) -> String {
    if let Ok(existing) = std::env::var("PENWISE_TOKEN")
        && !existing.is_empty()
    {
        return existing;
    }
    let path = data_dir.join("token");
    if let Ok(token) = std::fs::read_to_string(&path) {
        let token = token.trim().to_owned();
        if !token.is_empty() {
            return token;
        }
    }
    let token = random_hex();
    if let Err(error) =
        std::fs::create_dir_all(data_dir).and_then(|()| std::fs::write(&path, &token))
    {
        tracing::warn!(%error, "could not persist the access token; it will change on restart");
    }
    token
}

/// 16 random bytes as hex. Falls back to a time/pid mix only if `/dev/urandom` is unreadable.
fn random_hex() -> String {
    let mut bytes = [0_u8; 16];
    let filled = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_ok();
    if !filled {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
            .unwrap_or(0)
            ^ (u64::from(std::process::id()) << 32);
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        bytes[8..].copy_from_slice(&seed.rotate_left(17).to_le_bytes());
    }
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{byte:02x}"));
    }
    out
}

/// `true` when the request carries the expected token in the query or the `X-Penwise-Token` header.
#[must_use]
pub fn authorized(expected: &str, query_token: Option<&str>, headers: &HeaderMap) -> bool {
    if expected.is_empty() {
        return true;
    }
    if query_token.is_some_and(|t| constant_time_eq(t, expected)) {
        return true;
    }
    headers
        .get("x-penwise-token")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|t| constant_time_eq(t, expected))
}

/// Length-independent-enough comparison: no early exit on content, and length alone leaks only the size.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 401 with a plain-text reason (the caller answers the request directly).
#[must_use]
pub fn deny() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "PenwiseToken")],
        "missing or wrong token (append ?token=… or set X-Penwise-Token)",
    )
        .into_response()
}
