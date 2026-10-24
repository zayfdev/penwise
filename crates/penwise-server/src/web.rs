//! Embedded browser viewer (`web/dist`, built by `just web`) served as the router fallback
//! (server-stack.md §6).
//!
//! Owner: slice d.
//!
//! - Path `/` → `index.html`; a known asset → its bytes with `Content-Type` from `metadata.mimetype()`
//!   (`.wasm` → `application/wasm` for `instantiateStreaming`), `ETag` = hex SHA-256 from metadata,
//!   `If-None-Match` → 304; unknown paths without an extension → `index.html` (client routing); unknown
//!   paths with an extension → 404.
//! - Hashed assets (`/assets/*`) get `Cache-Control: public, max-age=31536000, immutable`; `index.html`
//!   gets `no-cache`.
//! - Every HTML response carries `CONTENT_SECURITY_POLICY` plus `X-Content-Type-Options: nosniff` and
//!   `Referrer-Policy: no-referrer`.
//! - `#[allow_missing = true]` lets the crate build before `web/dist` exists (every request then 404s except
//!   a plain-text hint at `/`). In debug builds rust-embed reads from disk (live viewer edits); release builds
//!   embed. [verify against rust-embed 8.13 when implementing; fallback: a `build.rs` that creates an empty
//!   `web/dist`.]

use axum::Router;

use crate::app::App;

/// CSP for the viewer: same-origin everything; wasm compilation allowed; QR SVG/PNG as images.
pub const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
     connect-src 'self'; img-src 'self' data: blob:; style-src 'self'; object-src 'none'; \
     base-uri 'none'; frame-ancestors 'none'";

/// The built viewer.
#[derive(Debug, rust_embed::Embed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
pub struct Assets;

/// Fallback router serving [`Assets`] (module rules).
pub fn routes() -> Router<App> {
    todo!()
}
