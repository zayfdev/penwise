//! Loopback-only guard for the pairing page (a pairing code is a credential: whoever sees it can pair).
//!
//! Owner: slice c.
//!
//! A request is local iff **all** hold:
//! 1. the TCP peer (`ConnectInfo<SocketAddr>`) is loopback (`127.0.0.0/8` or `::1`, including IPv4-mapped);
//! 2. the `Host` header's host is exactly `localhost`, `127.0.0.1` or `[::1]` (any port) — defeats DNS
//!    rebinding, where a hostile page resolves its own name to 127.0.0.1;
//! 3. an `Origin` header, if present, has one of those hosts too (blocks cross-site `fetch` from another
//!    local origin's page reading the response; CORS is never enabled).
//!
//! Requests with `X-Forwarded-For`/`Forwarded` headers are refused (a local reverse proxy would make every
//! remote client look local).

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::Response;

/// Module-doc rule.
#[must_use]
pub fn is_local_request(peer: SocketAddr, headers: &HeaderMap) -> bool {
    todo!()
}

/// axum middleware (`from_fn`): 404 (not 403, so the route's existence is not advertised) unless
/// [`is_local_request`].
///
/// # Errors
/// `StatusCode::NOT_FOUND`.
pub async fn require_local(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    todo!()
}
