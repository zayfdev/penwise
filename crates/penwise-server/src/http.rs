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

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}
