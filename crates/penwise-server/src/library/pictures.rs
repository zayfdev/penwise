//! Pictures on a page, and the one-time import of a pre-SQLite library.

use super::legacy::{default_meta, read_meta, read_page, read_single_page};
use super::model::{image_frame, page_frame};
use super::store::Store;
use super::strokes::LibraryArchives;
use super::{ImageMsg, Library, new_id};

impl Library {
    /// A copy of one page's pictures (empty for an unknown page).
    pub async fn images(&self, page: &str) -> Vec<ImageMsg> {
        self.content(page)
            .await
            .map(|content| content.images)
            .unwrap_or_default()
    }

    /// Places one picture on `page`; false when the page or picture is unusable.
    pub async fn add_image(&self, page: &str, image: ImageMsg) -> bool {
        if !image.is_well_formed() || !self.has_page(page).await {
            return false;
        }
        {
            let mut pages = self.pages.write().await;
            let content = pages.entry(page.to_owned()).or_default();
            if content
                .images
                .iter()
                .any(|existing| existing.id == image.id)
            {
                return true;
            }
            if let Err(error) = self.store.add_image(page, &image) {
                tracing::error!(page, %error, "image insert failed");
                return false;
            }
            content.images.push(image.clone());
        }
        self.fan_out(Some(page.to_owned()), image_frame(page, &image));
        true
    }

    /// Removes pictures by id; returns how many were there.
    pub async fn remove_images(&self, page: &str, ids: &[String]) -> usize {
        if ids.is_empty() {
            return 0;
        }
        let (content, removed) = {
            let mut pages = self.pages.write().await;
            let Some(content) = pages.get_mut(page) else {
                return 0;
            };
            let doomed: Vec<String> = content
                .images
                .iter()
                .filter(|image| ids.contains(&image.id))
                .map(|image| image.id.clone())
                .collect();
            if doomed.is_empty() {
                return 0;
            }
            self.archived(page);
            if let Err(error) = self.store.remove(page, &doomed) {
                tracing::error!(page, %error, "image delete failed");
                return 0;
            }
            content.images.retain(|image| !doomed.contains(&image.id));
            (content.clone(), doomed.len())
        };
        self.fan_out(Some(page.to_owned()), page_frame(page, &content));
        removed
    }
}

impl Library {
    /// Archive stamps for a page (newest first), each with its object count.
    pub fn history(&self, page: &str) -> Result<Vec<(i64, usize)>, String> {
        self.store.history(page)
    }

    /// Puts an archive back; returns how many objects came back, or `None` when the page or the archive does
    /// not exist (nothing changes then). The current state is archived first, so a restore can be undone.
    pub async fn restore(&self, page: &str, stamp: Option<i64>) -> Result<Option<usize>, String> {
        if !self.has_page(page).await {
            return Ok(None);
        }
        let (restored, content) = {
            let mut pages = self.pages.write().await;
            let Some(restored) = self.store.restore(page, stamp)? else {
                return Ok(None);
            };
            let content = self.store.content(page)?;
            pages.insert(page.to_owned(), content.clone());
            (restored, content)
        };
        self.fan_out(Some(page.to_owned()), page_frame(page, &content));
        Ok(Some(restored))
    }
}

/// A fresh picture id, `img_` plus 16 hex characters.
#[must_use]
pub fn new_image_id() -> String {
    new_id("img")
}

/// Brings a pre-SQLite library across once: the old index, its page files, or the oldest single page file.
pub(super) fn import_json_library(store: &Store, dir: &std::path::Path) {
    let (meta, strokes) = if let Some(meta) = read_meta(dir) {
        (meta, None)
    } else if let Some(strokes) = read_single_page(dir) {
        (default_meta(), Some(strokes))
    } else {
        (default_meta(), None)
    };
    if let Err(error) = store.write_meta(&meta) {
        tracing::error!(%error, "initial index write failed");
        return;
    }
    let mut imported = 0;
    for page in meta.all_pages() {
        let strokes = read_page(dir, &page.id);
        match store.add_strokes(&page.id, &strokes) {
            Ok(added) => imported += added,
            Err(error) => tracing::warn!(page = %page.id, %error, "legacy page import failed"),
        }
    }
    if let Some(strokes) = strokes {
        if let Err(error) = store.add_strokes(&meta.active, &strokes) {
            tracing::warn!(%error, "legacy page.json import failed");
        } else {
            imported += strokes.len();
        }
    }
    tracing::info!(
        pages = meta.notebooks.iter().map(|n| n.pages.len()).sum::<usize>(),
        imported,
        "imported the JSON library"
    );
}
