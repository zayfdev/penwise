//! Stroke mutations: append, batch append, remove, replace, clear.

use std::collections::HashSet;

use super::legacy::is_well_formed;
use super::model::{AGENT_LAYER, op_frame, page_frame, remove_op, replace_op, stroke_frame};
use super::{Library, PageContent, StrokeMsg};

/// Keeps a copy of a page before a destructive edit; shared by stroke and picture edits.
pub(super) trait LibraryArchives {
    /// Archives the page, logging (not failing) on error.
    fn archived(&self, page: &str);
}

impl LibraryArchives for Library {
    fn archived(&self, page: &str) {
        if let Err(error) = self.store.archive(page) {
            tracing::warn!(page, %error, "page archive failed");
        }
    }
}

impl Library {
    /// A copy of one page's strokes (empty for an unknown page).
    pub async fn strokes(&self, page: &str) -> Vec<StrokeMsg> {
        self.content(page)
            .await
            .map(|content| content.strokes)
            .unwrap_or_default()
    }

    /// Re-reads a page from the store and replaces the cached copy; returns the fresh content.
    ///
    /// Batch writes go through here because the store has the final say on what a page holds (it ignores a
    /// stroke whose seed is already there); extending the cache blindly would list duplicates to clients.
    pub(super) async fn refresh(&self, page: &str) -> PageContent {
        let fresh = self.store.content(page).unwrap_or_default();
        self.pages
            .write()
            .await
            .insert(page.to_owned(), fresh.clone());
        fresh
    }

    /// A copy of everything on one page.
    pub async fn content(&self, page: &str) -> Option<PageContent> {
        self.pages.read().await.get(page).cloned()
    }

    /// `{"t":"page","page":…,"strokes":[…],"images":[…]}`.
    pub async fn snapshot_frame(&self, page: &str) -> String {
        let content = self.content(page).await.unwrap_or_default();
        page_frame(page, &content)
    }

    /// Appends a stroke to `page`, persists, and fans it out. False for a malformed stroke or unknown page.
    pub async fn append(&self, page: &str, stroke: StrokeMsg) -> bool {
        if !is_well_formed(&stroke) || !self.has_page(page).await {
            return false;
        }
        {
            let mut pages = self.pages.write().await;
            let content = pages.entry(page.to_owned()).or_default();
            // Seeds are stroke identity: a client resending its page (reconnect, offline queue) must not
            // duplicate ink.
            if content
                .strokes
                .iter()
                .any(|existing| existing.seed == stroke.seed)
            {
                return true;
            }
            if let Err(error) = self.store.add_stroke(page, &stroke) {
                tracing::error!(page, %error, "stroke insert failed");
                return false;
            }
            content.strokes.push(stroke.clone());
        }
        self.fan_out(Some(page.to_owned()), stroke_frame(page, &stroke));
        self.resolve_anchors(page).await;
        true
    }

    /// Appends several strokes with one save and one snapshot broadcast (agent annotations).
    pub async fn append_many(&self, page: &str, strokes: Vec<StrokeMsg>) -> usize {
        let strokes: Vec<StrokeMsg> = strokes.into_iter().filter(is_well_formed).collect();
        if strokes.is_empty() || !self.has_page(page).await {
            return 0;
        }
        // Skip strokes the page already has (and repeats inside the batch), so an annotation sent twice
        // does not stack copies.
        let strokes: Vec<StrokeMsg> = {
            let pages = self.pages.read().await;
            let mut seen: HashSet<String> = pages
                .get(page)
                .map(|current| {
                    current
                        .strokes
                        .iter()
                        .map(|stroke| stroke.seed.clone())
                        .collect()
                })
                .unwrap_or_default();
            strokes
                .into_iter()
                .filter(|stroke| seen.insert(stroke.seed.clone()))
                .collect()
        };
        if strokes.is_empty() {
            return 0;
        }
        let added = match self.store.add_strokes(page, &strokes) {
            Ok(added) => added,
            Err(error) => {
                tracing::error!(page, %error, "stroke batch insert failed");
                return 0;
            }
        };
        // The database is authoritative: it ignored strokes the page already had, so the cache is rebuilt from
        // it rather than extended blindly (which would list duplicates to every client).
        let all = self.refresh(page).await;
        self.fan_out(Some(page.to_owned()), page_frame(page, &all));
        self.resolve_anchors(page).await;
        added
    }

    /// Removes every agent-authored stroke from `page`; returns how many were removed.
    pub async fn clear_agent(&self, page: &str) -> usize {
        let (all, removed) = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.strokes.len();
            current.strokes.retain(|stroke| stroke.layer != AGENT_LAYER);
            (current.clone(), before - current.strokes.len())
        };
        if removed > 0 {
            self.archived(page);
            if let Err(error) = self.store.remove_agent_strokes(page) {
                tracing::error!(page, %error, "agent clear failed");
            }
            self.fan_out(Some(page.to_owned()), page_frame(page, &all));
        }
        removed
    }

    /// Removes the strokes whose seeds are listed; broadcasts the removal, not the whole page.
    pub async fn remove_strokes(&self, page: &str, seeds: &[String]) -> usize {
        if seeds.is_empty() {
            return 0;
        }
        let removed = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.strokes.len();
            current
                .strokes
                .retain(|stroke| !seeds.iter().any(|seed| seed == &stroke.seed));
            before - current.strokes.len()
        };
        if removed == 0 {
            return 0;
        }
        self.archived(page);
        if let Err(error) = self.store.remove(page, seeds) {
            tracing::error!(page, %error, "stroke delete failed");
        }
        self.fan_out(Some(page.to_owned()), op_frame(page, &remove_op(seeds)));
        self.resolve_anchors(page).await;
        removed
    }

    /// Replaces the given strokes with `strokes` (a split erase); broadcasts the replacement.
    pub async fn replace_strokes(
        &self,
        page: &str,
        seeds: &[String],
        strokes: Vec<StrokeMsg>,
    ) -> usize {
        let strokes: Vec<StrokeMsg> = strokes.into_iter().filter(is_well_formed).collect();
        let all = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            if !seeds.is_empty() {
                current
                    .strokes
                    .retain(|stroke| !seeds.iter().any(|seed| seed == &stroke.seed));
            }
            current.strokes.extend(strokes.clone());
            current.clone()
        };
        self.archived(page);
        if let Err(error) = self.store.replace_strokes(page, &all.strokes) {
            tracing::error!(page, %error, "stroke replace failed");
        }
        self.fan_out(
            Some(page.to_owned()),
            op_frame(page, &replace_op(seeds, &strokes)),
        );
        self.resolve_anchors(page).await;
        all.strokes.len()
    }

    /// Empties one page (the tablet's clear).
    pub async fn clear_page(&self, page: &str) -> usize {
        let removed = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.strokes.len() + current.images.len();
            current.strokes.clear();
            current.images.clear();
            before
        };
        if removed > 0 {
            self.archived(page);
            if let Err(error) = self.store.clear(page) {
                tracing::error!(page, %error, "page clear failed");
            }
            self.fan_out(
                Some(page.to_owned()),
                op_frame(page, &serde_json::json!({ "k": "clear" })),
            );
        }
        removed
    }

    /// Replaces every stroke on `page` (tablet undo, clear or erase) and fans the snapshot out.
    pub async fn replace(&self, page: &str, strokes: Vec<StrokeMsg>) -> usize {
        let strokes: Vec<StrokeMsg> = strokes.into_iter().filter(is_well_formed).collect();
        let count = strokes.len();
        let content = {
            let mut pages = self.pages.write().await;
            let images = pages
                .get(page)
                .map(|content| content.images.clone())
                .unwrap_or_default();
            let content = PageContent { strokes, images };
            pages.insert(page.to_owned(), content.clone());
            content
        };
        self.archived(page);
        if let Err(error) = self.store.replace_strokes(page, &content.strokes) {
            tracing::error!(page, %error, "page replace failed");
        }
        self.fan_out(Some(page.to_owned()), page_frame(page, &content));
        self.resolve_anchors(page).await;
        count
    }
}
