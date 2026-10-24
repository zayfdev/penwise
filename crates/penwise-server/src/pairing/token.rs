//! Secrets: token/code generation, hashing, constant-time comparison, random ids, and the HTTP bearer
//! middleware (server-stack.md §2 "Auth").
//!
//! Owner: slice c.
//!
//! - Token: 32 bytes from `getrandom::fill`, base64url no padding (43 chars).
//! - Pairing code: 16 bytes, same encoding (22 chars).
//! - At rest: `SHA-256(UTF-8 text)` only. Compare with `subtle::ConstantTimeEq`.
//! - Secrets never reach logs: proto `AuthToken`/`PairingCode` `Debug` redact; never log request headers.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use penwise_proto::{AuthToken, PairingCode, ServerId};

use crate::app::App;

/// Random bytes in a device token.
pub const TOKEN_BYTES: usize = 32;
/// Random bytes in a pairing code.
pub const CODE_BYTES: usize = 16;

/// Randomness source failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("randomness unavailable")]
pub struct RandomError;

/// Fresh device token.
///
/// # Errors
/// [`RandomError`].
pub fn generate_token() -> Result<AuthToken, RandomError> {
    todo!()
}

/// Fresh pairing code.
///
/// # Errors
/// [`RandomError`].
pub fn generate_code() -> Result<PairingCode, RandomError> {
    todo!()
}

/// Fresh server id: `ServerId::from_parts(now_ms, 80 random bits)`.
///
/// # Errors
/// [`RandomError`].
pub fn random_server_id(now_ms: u64) -> Result<ServerId, RandomError> {
    todo!()
}

/// `SHA-256(text)` (token and code storage form).
#[must_use]
pub fn hash_secret(text: &str) -> [u8; 32] {
    todo!()
}

/// Constant-time equality of two hashes.
#[must_use]
pub fn secrets_match(a: &[u8; 32], b: &[u8; 32]) -> bool {
    todo!()
}

/// axum middleware (`from_fn_with_state`) for `/api/*`: requires `Authorization: Bearer <token>` valid per
/// `Pairing::device_for_bearer` (in `spawn_blocking`); 401 with `WWW-Authenticate: Bearer` otherwise. Inserts
/// the authenticated `DeviceId` into request extensions.
///
/// # Errors
/// `StatusCode::UNAUTHORIZED`; `INTERNAL_SERVER_ERROR` on storage failure.
pub async fn require_bearer(
    State(app): State<App>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    todo!()
}
