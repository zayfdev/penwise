//! Static files for the browser viewer. Serves `web/` from disk; paths never escape it.

use std::path::{Path, PathBuf};

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};

/// Serves the viewer. `/` maps to `index.html`; unknown paths get a 404 body.
pub async fn serve(root: PathBuf, uri: Uri) -> Response {
    let relative = uri.path().trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    let Some(path) = resolve(&root, relative) else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    match tokio::fs::read(&path).await {
        Ok(body) => (
            [
                (header::CONTENT_TYPE, content_type(&path)),
                (header::CACHE_CONTROL, "no-store"),
            ],
            body,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Joins `relative` onto `root`, rejecting `..`, absolute paths and symlink escapes.
fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.contains("..") {
        return None;
    }
    let candidate = root.join(relative);
    if !candidate.starts_with(root) {
        return None;
    }
    Some(candidate)
}

/// Media type for a served file (project ports share this table).
pub(crate) fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "text/xml; charset=utf-8",
        Some("webmanifest") => "application/manifest+json",
        Some("pdf") => "application/pdf",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}
