//! The recognised-text sidecar: `pages/<id>.txt`, written by the handwriting model and read by search.

use std::path::Path;

/// The text stored for `page`, if any.
pub(crate) async fn read(dir: &Path, page: &str) -> Option<String> {
    match tokio::fs::read_to_string(path_of(dir, page)).await {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            tracing::warn!(page, %error, "text cache read failed");
            None
        }
    }
}

/// Replaces the stored text for `page`.
pub(crate) async fn write(dir: &Path, page: &str, text: &str) {
    let path = path_of(dir, page);
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    if let Err(error) = tokio::fs::write(&path, text).await {
        tracing::warn!(page, %error, "text cache write failed");
    }
}

/// Where a page's recognised text lives.
pub(crate) fn path_of(dir: &Path, page: &str) -> std::path::PathBuf {
    dir.join("pages").join(format!("{page}.txt"))
}
