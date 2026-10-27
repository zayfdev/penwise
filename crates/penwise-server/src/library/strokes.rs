//! Stroke mutations: append, batch append, remove, replace, clear.

use std::collections::HashSet;

use super::legacy::accepted;
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
        let Some(stroke) = accepted(stroke) else {
            return false;
        };
        if !self.has_page(page).await {
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
        let strokes: Vec<StrokeMsg> = strokes.into_iter().filter_map(accepted).collect();
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

    // The destructive edits below share one order: archive, commit to the database, only then change the
    // cache and fan out. A failed write changes nothing and tells nobody, so clients never see an edit the
    // database refused (it would come back on the next restart). They return 0 in that case.

    /// Removes every agent-authored stroke from `page`; returns how many were removed.
    pub async fn clear_agent(&self, page: &str) -> usize {
        let (all, removed) = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let removed = current
                .strokes
                .iter()
                .filter(|stroke| stroke.layer == AGENT_LAYER)
                .count();
            if removed == 0 {
                return 0;
            }
            self.archived(page);
            if let Err(error) = self.store.remove_agent_strokes(page) {
                tracing::error!(page, %error, "agent clear failed");
                return 0;
            }
            current.strokes.retain(|stroke| stroke.layer != AGENT_LAYER);
            (current.clone(), removed)
        };
        self.fan_out(Some(page.to_owned()), page_frame(page, &all));
        removed
    }

    /// Removes the strokes whose seeds are listed; broadcasts the removal, not the whole page.
    pub async fn remove_strokes(&self, page: &str, seeds: &[String]) -> usize {
        if seeds.is_empty() {
            return 0;
        }
        let doomed: Vec<String> = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            // Only seeds that name strokes here: an id of another object kind must not be deleted through
            // a stroke `remove`.
            let doomed: Vec<String> = current
                .strokes
                .iter()
                .filter(|stroke| seeds.contains(&stroke.seed))
                .map(|stroke| stroke.seed.clone())
                .collect();
            if doomed.is_empty() {
                return 0;
            }
            self.archived(page);
            if let Err(error) = self.store.remove(page, &doomed) {
                tracing::error!(page, %error, "stroke delete failed");
                return 0;
            }
            current
                .strokes
                .retain(|stroke| !doomed.contains(&stroke.seed));
            doomed
        };
        self.fan_out(Some(page.to_owned()), op_frame(page, &remove_op(&doomed)));
        self.resolve_anchors(page).await;
        doomed.len()
    }

    /// Replaces the given strokes with `strokes` (a split erase); broadcasts the replacement.
    ///
    /// Replacements whose seed the page already holds (or that repeat inside the batch) are dropped, so the
    /// cache and the database always list the same strokes. Returns the page's stroke count afterwards.
    pub async fn replace_strokes(
        &self,
        page: &str,
        seeds: &[String],
        strokes: Vec<StrokeMsg>,
    ) -> usize {
        let incoming: Vec<StrokeMsg> = strokes.into_iter().filter_map(accepted).collect();
        let (total, added) = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let mut next: Vec<StrokeMsg> = current
                .strokes
                .iter()
                .filter(|stroke| !seeds.contains(&stroke.seed))
                .cloned()
                .collect();
            let mut seen: HashSet<String> = next.iter().map(|stroke| stroke.seed.clone()).collect();
            let added: Vec<StrokeMsg> = incoming
                .into_iter()
                .filter(|stroke| seen.insert(stroke.seed.clone()))
                .collect();
            next.extend(added.iter().cloned());
            self.archived(page);
            if let Err(error) = self.store.replace_strokes(page, &next) {
                tracing::error!(page, %error, "stroke replace failed");
                return 0;
            }
            let total = next.len();
            current.strokes = next;
            (total, added)
        };
        self.fan_out(
            Some(page.to_owned()),
            op_frame(page, &replace_op(seeds, &added)),
        );
        self.resolve_anchors(page).await;
        total
    }

    /// Empties one page (the tablet's clear).
    pub async fn clear_page(&self, page: &str) -> usize {
        let removed = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.strokes.len() + current.images.len();
            if before == 0 {
                return 0;
            }
            self.archived(page);
            if let Err(error) = self.store.clear(page) {
                tracing::error!(page, %error, "page clear failed");
                return 0;
            }
            current.strokes.clear();
            current.images.clear();
            before
        };
        self.fan_out(
            Some(page.to_owned()),
            op_frame(page, &serde_json::json!({ "k": "clear" })),
        );
        removed
    }

    /// Replaces every stroke on `page` (tablet undo, clear or erase) and fans the snapshot out.
    ///
    /// Strokes repeating a seed are dropped after the first. Returns the stroke count now on the page, 0 when
    /// the page is unknown or the database refused.
    pub async fn replace(&self, page: &str, strokes: Vec<StrokeMsg>) -> usize {
        let mut seen = HashSet::new();
        let strokes: Vec<StrokeMsg> = strokes
            .into_iter()
            .filter_map(accepted)
            .filter(|stroke| seen.insert(stroke.seed.clone()))
            .collect();
        let count = strokes.len();
        let content = {
            let mut pages = self.pages.write().await;
            let images = pages
                .get(page)
                .map(|content| content.images.clone())
                .unwrap_or_default();
            let content = PageContent { strokes, images };
            self.archived(page);
            if let Err(error) = self.store.replace_strokes(page, &content.strokes) {
                tracing::error!(page, %error, "page replace failed");
                return 0;
            }
            pages.insert(page.to_owned(), content.clone());
            content
        };
        self.fan_out(Some(page.to_owned()), page_frame(page, &content));
        self.resolve_anchors(page).await;
        count
    }
}
