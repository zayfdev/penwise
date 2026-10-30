//! Stills: what a non-live frame shows (design harness v2, contract §4).
//!
//! A still is the frame's whole document captured by the server's headless Chromium at the preset width, cut into
//! strips (content-addressed PNGs), plus the document rects of the elements the frame's bound marks watch and a
//! hit map of every visible element. Clients draw the strips under their own ink and feed the rects to core as the
//! frame's layout while no live view runs.
//!
//! - [`routes`]: `GET /frames/<id>/still` (JSON, `ETag` = version, `304` on `If-None-Match`),
//!   `GET /frames/<id>/strip/<hash>.png[?w=]` (immutable strips, optionally downscaled) and
//!   `GET /frames/<id>/hit?x&y` (answered from the still's hit map).
//! - [`capture`]: the pipeline — open the frame ([`crate::agent::FrameView`]: route, [`preroll`], patch), measure,
//!   collect the hit map, cut strips, then write `FrameDocHeight` (and the infinite-feed fallback to `viewport`)
//!   back as server-device ops.
//! - [`triggers`]: when a still is re-made, debounced 500 ms per frame, and eagerly only for frames that were
//!   polled in the last 10 s.
//! - [`slots`]: one capture at a time per frame, coalescing, staleness and the negative cache of failures.
//! - [`cache`] and [`blobs`]: still metadata for every frame in memory; strip bytes in a bounded LRU that spills
//!   to disk.

mod blobs;
mod cache;
mod capture;
mod hit;
mod hitmap;
mod model;
mod preroll;
mod routes;
mod scale;
mod slots;
mod strips;
#[cfg(test)]
mod tests;
mod triggers;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use axum::body::Bytes;
use penwise_core::item::ItemId;

use crate::frames::registry::Registry;
use crate::inspect::Inspect;
use crate::storage::Storage;

use blobs::{BlobStore, DISK_BUDGET, MEMORY_BUDGET};
use cache::StillCache;
pub(crate) use model::Still;
pub(crate) use preroll::{freeze_animations, preroll};
pub(crate) use routes::routes;
pub(crate) use strips::{Strip, capture as capture_strips};

/// Why a still or a hit could not be made.
#[derive(Debug, thiserror::Error)]
pub(crate) enum StillError {
    #[error("unknown frame")]
    UnknownFrame,
    #[error(transparent)]
    Agent(#[from] crate::agent::AgentError),
    #[error(transparent)]
    Inspect(#[from] crate::inspect::Error),
    #[error("storage: {0}")]
    Storage(String),
    #[error("write: {0}")]
    Write(String),
    /// A capture failed a moment ago and is not retried until `retry_after` seconds have passed.
    #[error("{message} (not retried for {retry_after} s)")]
    Unavailable { message: String, retry_after: u64 },
}

impl From<crate::storage::StorageError> for StillError {
    fn from(error: crate::storage::StorageError) -> Self {
        Self::Storage(error.to_string())
    }
}

/// The stills service. Cheap to clone through [`Arc`].
pub(crate) struct Stills {
    inner: Arc<Inner>,
}

/// Everything the capture pipeline and the trigger tasks share.
pub(super) struct Inner {
    pub(super) storage: Storage,
    pub(super) inspect: Arc<Inspect>,
    pub(super) projects: Arc<Registry>,
    cache: Mutex<StillCache>,
    blobs: Mutex<BlobStore>,
    /// Where evicted strips go (`<data>/stills`).
    dir: PathBuf,
    slots: Mutex<HashMap<ItemId, slots::Slot>>,
    /// Frames whose switch to `viewport` the server just wrote, so its own op does not re-trigger a still.
    flips: Mutex<HashSet<ItemId>>,
    started: AtomicBool,
    /// Captures begun, for the coalescing tests.
    #[cfg(test)]
    captures: std::sync::atomic::AtomicU64,
}

impl Inner {
    pub(super) fn cache(&self) -> MutexGuard<'_, StillCache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn flips(&self) -> MutexGuard<'_, HashSet<ItemId>> {
        self.flips.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn blobs(&self) -> MutexGuard<'_, BlobStore> {
        self.blobs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Keeps strip bytes under their hash; whatever that pushes out of memory is spilled to disk.
    pub(super) async fn keep_blob(&self, hash: &str, blob: Bytes) {
        let evicted = self.blobs().put(hash, blob);
        blobs::spill(self.dir.clone(), evicted, DISK_BUDGET).await;
    }

    /// The bytes of a strip (or a derived variant of it): memory, else the spill directory.
    pub(super) async fn blob(&self, name: &str) -> Option<Bytes> {
        if let Some(blob) = self.blobs().get(name) {
            return Some(blob);
        }
        let blob = blobs::load(&self.dir, name).await?;
        self.keep_blob(name, blob.clone()).await;
        Some(blob)
    }
}

impl Stills {
    /// A service over `storage`, shooting with `inspect`, resolving projects through `projects` and spilling
    /// evicted strips to `dir`. Nothing runs until [`Stills::start`]; requests capture on demand either way.
    pub(crate) fn new(
        storage: Storage,
        inspect: Arc<Inspect>,
        projects: Arc<Registry>,
        dir: PathBuf,
    ) -> Self {
        Self::with_memory(storage, inspect, projects, dir, MEMORY_BUDGET)
    }

    pub(super) fn with_memory(
        storage: Storage,
        inspect: Arc<Inspect>,
        projects: Arc<Registry>,
        dir: PathBuf,
        memory: usize,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                storage,
                inspect,
                projects,
                cache: Mutex::new(StillCache::default()),
                blobs: Mutex::new(BlobStore::new(memory)),
                dir,
                slots: Mutex::new(HashMap::new()),
                flips: Mutex::new(HashSet::new()),
                started: AtomicBool::new(false),
                #[cfg(test)]
                captures: std::sync::atomic::AtomicU64::new(0),
            }),
        }
    }

    /// The frame's latest still for a client poll: the cached one, unless the frame went stale while nobody
    /// looked (then it is captured first); captured now when there is none yet.
    ///
    /// # Errors
    /// The frame is unknown, its page could not be opened or shot, or a capture failed moments ago.
    pub(crate) async fn current(&self, frame: ItemId) -> Result<Arc<Still>, StillError> {
        let asked = Instant::now();
        let stale = self.inner.note_poll(frame);
        if !stale && let Some(still) = self.inner.cache().get(frame) {
            return Ok(still);
        }
        self.capture_after(frame, asked).await
    }

    /// Captures the frame now unless a capture that began after this call already did.
    ///
    /// # Errors
    /// As [`Stills::current`].
    pub(crate) async fn refresh(&self, frame: ItemId) -> Result<Arc<Still>, StillError> {
        self.capture_after(frame, Instant::now()).await
    }

    /// The core of every capture: wait for the frame's turn, take a capture that began after `asked` as the
    /// answer, answer a recent failure from memory, else capture.
    async fn capture_after(&self, frame: ItemId, asked: Instant) -> Result<Arc<Still>, StillError> {
        let inner = &self.inner;
        let gate = inner.gate(frame);
        let _turn = gate.lock().await;
        if let Some(still) = inner.covered(frame, asked) {
            return Ok(still);
        }
        if let Some((message, retry_after)) = inner.recent_failure(frame) {
            return Err(StillError::Unavailable {
                message,
                retry_after,
            });
        }
        inner.begin(frame);
        #[cfg(test)]
        inner
            .captures
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match capture::run(inner, frame).await {
            Ok(still) => Ok(still),
            Err(StillError::UnknownFrame) => {
                inner.forget(frame);
                Err(StillError::UnknownFrame)
            }
            Err(error) => {
                inner.fail(frame, error.to_string());
                Err(error)
            }
        }
    }

    /// The bytes of the strip named `hash`, optionally downscaled to the bucket of `width` px. A strip that is
    /// in neither memory nor the spill directory but belongs to the frame's current still is re-shot once.
    ///
    /// # Errors
    /// The frame is unknown or its page could not be shot.
    pub(crate) async fn strip(
        &self,
        frame: ItemId,
        hash: &str,
        width: Option<u32>,
    ) -> Result<Option<Bytes>, StillError> {
        let mut bytes = self.inner.blob(hash).await;
        let current = self.inner.cache().get(frame);
        if bytes.is_none() && current.is_some_and(|still| still.has_strip(hash)) {
            self.refresh(frame).await?;
            bytes = self.inner.blob(hash).await;
        }
        match (bytes, width) {
            (Some(bytes), Some(width)) => scale::variant(&self.inner, hash, bytes, width).await,
            (bytes, _) => Ok(bytes),
        }
    }

    /// The element at document point `point` from the frame's latest still, without a browser; `None` when the
    /// frame has no still.
    pub(crate) fn answer_from_still(
        &self,
        frame: ItemId,
        point: [f32; 2],
    ) -> Option<serde_json::Value> {
        let still = self.inner.cache().get(frame)?;
        Some(still.hit_at(point))
    }

    /// The element at document point `point`: from the still's hit map when there is one, else by opening the
    /// frame's page in the browser.
    ///
    /// # Errors
    /// The frame has no still, is unknown, or its page could not be opened.
    pub(crate) async fn hit(
        &self,
        frame: ItemId,
        point: [f32; 2],
    ) -> Result<serde_json::Value, StillError> {
        self.inner.note_poll(frame);
        if let Some(answer) = self.answer_from_still(frame, point) {
            return Ok(answer);
        }
        hit::at(&self.inner, frame, point).await
    }
}

impl std::fmt::Debug for Stills {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Stills").finish_non_exhaustive()
    }
}
