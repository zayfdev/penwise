//! Static projects: serve a folder with an `index.html` fallback for directories, inject the bridge into HTML,
//! and tell connected pages to reload when a file changes.
//!
//! The watcher is inotify-driven on Linux (the kernel queue, no directory walk per tick); elsewhere it compares
//! modification times. Both feeds are debounced so one editor save yields one reload. `notify` would have been
//! the usual choice, but it is CC0, which `cargo deny` rejects here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use super::FrameError;
use super::inject;

/// How long the burst an editor produces is coalesced into one reload.
const DEBOUNCE: Duration = Duration::from_millis(100);
/// How often the watcher checks its queue (Linux) or the tree (other platforms).
const TICK: Duration = Duration::from_millis(100);
/// Directories whose churn is not a page change.
const IGNORED: [&str; 4] = ["node_modules", ".git", ".venv", "__pycache__"];
/// Reload broadcasts buffered per page.
const CAPACITY: usize = 32;

/// A served folder plus its reload feed. Dropping it stops the watcher.
#[allow(
    missing_debug_implementations,
    reason = "holds the broadcast sender and a task"
)]
pub struct Site {
    root: PathBuf,
    reload: broadcast::Sender<()>,
    task: JoinHandle<()>,
}

impl Drop for Site {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Site {
    /// Canonicalizes `root` (it must exist) and starts watching it.
    ///
    /// # Errors
    /// A missing or unreadable folder, or a watcher thread that cannot spawn.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, FrameError> {
        let root = root.into();
        let root = std::fs::canonicalize(&root).map_err(|error| {
            FrameError::Config(format!("static root {}: {error}", root.display()))
        })?;
        if !root.is_dir() {
            return Err(FrameError::Config(format!(
                "static root {} is not a directory",
                root.display()
            )));
        }
        let (changes, incoming) = mpsc::unbounded_channel();
        let watched = root.clone();
        std::thread::Builder::new()
            .name("penwise-watch".to_owned())
            .spawn(move || watch(&watched, &changes))?;
        let (reload, _) = broadcast::channel(CAPACITY);
        let task = tokio::spawn(debounce(incoming, reload.clone()));
        Ok(Self { root, reload, task })
    }

    /// A feed that fires once per settled change under the served folder.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.reload.subscribe()
    }

    /// How many pages listen for reloads right now.
    #[must_use]
    pub fn listeners(&self) -> usize {
        self.reload.receiver_count()
    }

    /// Serves one file request. Unknown paths and escapes above the root answer 404.
    pub async fn serve(&self, uri: &Uri) -> Response {
        let Some(path) = self.path_for(uri).await else {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        };
        let Ok(body) = tokio::fs::read(&path).await else {
            return (StatusCode::NOT_FOUND, "not found").into_response();
        };
        let content_type = crate::http::content_type(&path);
        let body = if inject::is_html(Some(content_type)) {
            inject::inject(&body)
        } else {
            body
        };
        (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store"),
            ],
            body,
        )
            .into_response()
    }

    /// Resolves a request path to a file inside the root; a directory maps to its `index.html`.
    async fn path_for(&self, uri: &Uri) -> Option<PathBuf> {
        let decoded = super::percent_decode(uri.path())?;
        let relative = decoded.trim_start_matches('/');
        let relative = if relative.is_empty() {
            "index.html"
        } else {
            relative
        };
        if relative
            .split('/')
            .any(|segment| segment == ".." || segment.contains('\\') || segment.contains('\0'))
        {
            return None;
        }
        let candidate = inside(&self.root, relative).await?;
        if candidate.is_dir() {
            return inside(
                &self.root,
                &format!("{}/index.html", relative.trim_end_matches('/')),
            )
            .await;
        }
        Some(candidate)
    }
}

/// Canonicalizes `root/relative` and rejects anything that leaves `root` (including through a symlink).
async fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = tokio::fs::canonicalize(root.join(relative)).await.ok()?;
    path.starts_with(root).then_some(path)
}

/// Coalesces a burst of file events into one reload.
async fn debounce(mut changes: mpsc::UnboundedReceiver<()>, reload: broadcast::Sender<()>) {
    while changes.recv().await.is_some() {
        while let Ok(Some(())) = tokio::time::timeout(DEBOUNCE, changes.recv()).await {}
        // No subscribers is normal (no page open); the next change will reload once one connects.
        let _ = reload.send(());
    }
}

/// `true` for paths inside a directory whose churn is irrelevant to the served page.
fn ignored(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| IGNORED.contains(&name))
    })
}

#[cfg(target_os = "linux")]
fn watch(root: &Path, changes: &mpsc::UnboundedSender<()>) {
    use inotify::{EventMask, Inotify, WatchMask};

    let mask = WatchMask::MODIFY
        | WatchMask::CREATE
        | WatchMask::DELETE
        | WatchMask::MOVED_TO
        | WatchMask::MOVED_FROM
        | WatchMask::ATTRIB
        | WatchMask::CLOSE_WRITE;
    let mut inotify = match Inotify::init() {
        Ok(inotify) => inotify,
        Err(error) => {
            tracing::warn!(%error, "inotify unavailable; live reload is off");
            return;
        }
    };
    let mut watches = HashMap::new();
    add_tree(&inotify, root, mask, &mut watches);

    let mut buffer = [0_u8; 16 * 1024];
    loop {
        if changes.is_closed() {
            return;
        }
        match inotify.read_events(&mut buffer) {
            Ok(events) => {
                let mut changed = false;
                for event in events {
                    let Some(base) = watches.get(&event.wd).cloned() else {
                        continue;
                    };
                    let path = match event.name {
                        Some(name) => base.join(name),
                        None => base,
                    };
                    if ignored(&path) {
                        continue;
                    }
                    if event.mask.contains(EventMask::ISDIR)
                        && event
                            .mask
                            .intersects(EventMask::CREATE | EventMask::MOVED_TO)
                    {
                        add_tree(&inotify, &path, mask, &mut watches);
                    }
                    changed = true;
                }
                if changed && changes.send(()).is_err() {
                    return;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(TICK);
            }
            Err(error) => {
                tracing::warn!(%error, "file watcher stopped");
                return;
            }
        }
    }
}

/// Watches `dir` and every subdirectory below it, skipping ignored ones and symlinked loops.
#[cfg(target_os = "linux")]
fn add_tree(
    inotify: &inotify::Inotify,
    dir: &Path,
    mask: inotify::WatchMask,
    watches: &mut HashMap<inotify::WatchDescriptor, PathBuf>,
) {
    match inotify.watches().add(dir, mask) {
        Ok(descriptor) => {
            watches.insert(descriptor, dir.to_path_buf());
        }
        Err(error) => {
            tracing::debug!(dir = %dir.display(), %error, "cannot watch directory");
            return;
        }
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() && !file_type.is_symlink() && !ignored(&path) {
            add_tree(inotify, &path, mask, watches);
        }
    }
}

/// Fallback for platforms without inotify: compare modification times and sizes.
#[cfg(not(target_os = "linux"))]
fn watch(root: &Path, changes: &mpsc::UnboundedSender<()>) {
    let mut last = snapshot(root);
    loop {
        std::thread::sleep(TICK * 3);
        if changes.is_closed() {
            return;
        }
        let now = snapshot(root);
        if now != last {
            last = now;
            if changes.send(()).is_err() {
                return;
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn snapshot(root: &Path) -> HashMap<PathBuf, (u64, u64)> {
    fn walk(dir: &Path, out: &mut HashMap<PathBuf, (u64, u64)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if ignored(&path) {
                continue;
            }
            match entry.metadata() {
                Ok(metadata) if metadata.is_dir() => walk(&path, out),
                Ok(metadata) => {
                    let modified = metadata
                        .modified()
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |age| age.as_millis() as u64);
                    out.insert(path, (modified, metadata.len()));
                }
                Err(_) => {}
            }
        }
    }
    let mut out = HashMap::new();
    walk(root, &mut out);
    out
}
