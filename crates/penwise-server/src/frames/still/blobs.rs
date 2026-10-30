//! Strip PNGs by content address: a bounded in-memory LRU over a bounded, LRU-pruned spill directory.
//!
//! The still metadata never leaves memory, so evicting bytes never costs a recapture: an evicted strip is
//! written to `<data>/stills/<hash>.png` and read back on the next request. Same hash, same bytes, so the files
//! are immutable and shared by every frame that shows them.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use axum::body::Bytes;

/// PNG bytes kept in memory.
pub(crate) const MEMORY_BUDGET: usize = 256 * 1024 * 1024;
/// PNG bytes kept on disk.
pub(crate) const DISK_BUDGET: u64 = 2 * 1024 * 1024 * 1024;

/// The in-memory part: most recently used last.
#[derive(Debug)]
pub(crate) struct BlobStore {
    budget: usize,
    bytes: usize,
    blobs: HashMap<String, Bytes>,
    order: VecDeque<String>,
}

impl BlobStore {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            blobs: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub(crate) fn get(&mut self, hash: &str) -> Option<Bytes> {
        let blob = self.blobs.get(hash).cloned()?;
        self.touch(hash);
        Some(blob)
    }

    /// Stores `blob` and returns what had to make room (to be spilled to disk). The new blob always stays.
    pub(crate) fn put(&mut self, hash: &str, blob: Bytes) -> Vec<(String, Bytes)> {
        if self.blobs.contains_key(hash) {
            self.touch(hash);
            return Vec::new();
        }
        self.bytes += blob.len();
        self.blobs.insert(hash.to_owned(), blob);
        self.order.push_back(hash.to_owned());
        let mut evicted = Vec::new();
        while self.bytes > self.budget && self.order.len() > 1 {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(gone) = self.blobs.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(gone.len());
                evicted.push((oldest, gone));
            }
        }
        evicted
    }

    /// Bytes held in memory.
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }

    fn touch(&mut self, hash: &str) {
        self.order.retain(|known| known != hash);
        self.order.push_back(hash.to_owned());
    }
}

fn file(dir: &Path, hash: &str) -> PathBuf {
    dir.join(format!("{hash}.png"))
}

/// Writes evicted blobs to `dir` and prunes the directory to `budget` bytes, oldest first. Failures are logged:
/// a strip that did not reach the disk is simply gone, and its frame is re-shot when someone asks for it.
pub(crate) async fn spill(dir: PathBuf, evicted: Vec<(String, Bytes)>, budget: u64) {
    if evicted.is_empty() {
        return;
    }
    let result = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        for (hash, blob) in evicted {
            let target = file(&dir, &hash);
            if !target.exists() {
                let partial = dir.join(format!("{hash}.part"));
                std::fs::write(&partial, &blob)?;
                std::fs::rename(&partial, &target)?;
            }
        }
        prune(&dir, budget)
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(%error, "still strips not spilled to disk"),
        Err(error) => tracing::warn!(%error, "still spill task failed"),
    }
}

/// Reads a spilled blob back.
pub(crate) async fn load(dir: &Path, hash: &str) -> Option<Bytes> {
    tokio::fs::read(file(dir, hash)).await.ok().map(Bytes::from)
}

/// Deletes the oldest files until `dir` holds at most `budget` bytes.
pub(crate) fn prune(dir: &Path, budget: u64) -> std::io::Result<()> {
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    files.sort();
    for (_, size, path) in files {
        if total <= budget {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}
