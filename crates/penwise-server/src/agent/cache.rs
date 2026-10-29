//! The crops a mark bundle names, remembered between `GET /marks` and the image URLs it hands out. An entry is
//! only valid for the page `updated_seq` it was taken at, so an edited page is rebuilt on the next read.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use penwise_core::item::ItemId;

use super::images::{BuiltImages, ImageKind};

/// How many crops the cache keeps before it drops everything (the bundle rebuilds what it needs).
const MAX_CACHED: usize = 512;

#[derive(Debug, Clone)]
struct Cached {
    /// The page's `updated_seq` when the crop was taken.
    version: u64,
    png: Arc<Vec<u8>>,
}

/// Crops built by the last `GET /marks` (and by image requests that missed), keyed by mark and kind.
#[derive(Debug, Default)]
pub(crate) struct ImageCache {
    entries: Mutex<BTreeMap<(ItemId, ImageKind), Cached>>,
}

impl ImageCache {
    /// The cached crop, when it was built for `version`.
    pub(crate) fn get(&self, mark: ItemId, kind: ImageKind, version: u64) -> Option<Arc<Vec<u8>>> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(&(mark, kind))
            .filter(|entry| entry.version == version)
            .map(|entry| Arc::clone(&entry.png))
    }

    /// Stores all three crops of `mark` at `version`.
    pub(crate) fn put(&self, mark: ItemId, version: u64, images: &BuiltImages) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() >= MAX_CACHED {
            entries.clear();
        }
        for kind in ImageKind::ALL {
            entries.insert(
                (mark, kind),
                Cached {
                    version,
                    png: Arc::new(images.get(kind).to_vec()),
                },
            );
        }
    }
}
