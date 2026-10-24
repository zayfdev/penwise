//! Notebooks, pages and their stroke lists: the on-disk layout, the frame fan-out, and every mutation the HTTP
//! routes and WebSocket frames can make.
//!
//! See `docs/pages.md` for the wire and disk contract this implements.

mod disk;
mod model;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

use disk::{
    is_well_formed, load_or_create_meta, read_strokes, write_atomic, write_atomic_blocking,
};
pub use model::{
    AGENT_LAYER, BACKGROUND_SPACING_PX, Background, Frame, Meta, NewPage, Notebook, PageKind,
    PageMeta, PagePatch, StrokeMsg,
};
use model::{
    active_frame, check_title, is_page_id, meta_frame_of, new_id, op_frame, page_frame, remove_op,
    replace_op, stroke_frame, title_or,
};

/// The library: index plus stroke lists, all shared behind one `Arc`.
pub struct Library {
    meta: RwLock<Meta>,
    pages: RwLock<HashMap<String, Vec<StrokeMsg>>>,
    tx: broadcast::Sender<Frame>,
    data_dir: PathBuf,
}

impl Library {
    /// Loads `meta.json` (migrating a legacy `page.json` on first run) and every page's strokes.
    pub async fn load(data_dir: PathBuf) -> Arc<Self> {
        let meta = load_or_create_meta(&data_dir).await;
        let mut pages = HashMap::new();
        for page in meta.all_pages() {
            match read_strokes(&data_dir, &page.id).await {
                Ok(strokes) => {
                    pages.insert(page.id.clone(), strokes);
                }
                Err(error) => tracing::warn!(page = %page.id, %error, "page load failed"),
            }
        }
        tracing::info!(
            notebooks = meta.notebooks.len(),
            pages = pages.len(),
            dir = %data_dir.display(),
            "library loaded"
        );
        let (tx, _) = broadcast::channel(256);
        Arc::new(Self {
            meta: RwLock::new(meta),
            pages: RwLock::new(pages),
            tx,
            data_dir,
        })
    }

    /// Subscribes to every frame; the caller filters by page.
    pub fn subscribe(&self) -> broadcast::Receiver<Frame> {
        self.tx.subscribe()
    }

    /// The materialized index.
    pub async fn meta(&self) -> Meta {
        self.meta.read().await.clone()
    }

    /// `{"t":"meta",…}`: the index as the viewer and tablet understand it.
    pub async fn meta_frame(&self) -> String {
        let meta = self.meta.read().await;
        serde_json::json!({ "t": "meta", "meta": *meta }).to_string()
    }

    /// The active page id.
    pub async fn active(&self) -> String {
        self.meta.read().await.active.clone()
    }

    /// `{"t":"active","page":…}`.
    pub async fn active_frame(&self) -> String {
        let page = self.active().await;
        active_frame(&page)
    }

    /// One page's settings.
    pub async fn page(&self, id: &str) -> Option<PageMeta> {
        self.meta.read().await.page(id).cloned()
    }

    /// Makes `page` the library's active page; false when it does not exist.
    pub async fn select(&self, page: &str) -> bool {
        let mut meta = self.meta.write().await;
        if meta.page(page).is_none() {
            return false;
        }
        if meta.active == page {
            return true;
        }
        page.clone_into(&mut meta.active);
        let index = meta.clone();
        drop(meta);
        self.save_meta(&index).await;
        self.fan_out(None, active_frame(page));
        true
    }

    /// A copy of one page's strokes (empty for an unknown page).
    pub async fn strokes(&self, page: &str) -> Vec<StrokeMsg> {
        self.pages
            .read()
            .await
            .get(page)
            .cloned()
            .unwrap_or_default()
    }

    /// `{"t":"page","page":…,"strokes":[…]}`.
    pub async fn snapshot_frame(&self, page: &str) -> String {
        let strokes = self.strokes(page).await;
        page_frame(page, &strokes)
    }

    /// Appends a stroke to `page`, persists, and fans it out. False for a malformed stroke or unknown page.
    pub async fn append(&self, page: &str, stroke: StrokeMsg) -> bool {
        if !is_well_formed(&stroke) || !self.has_page(page).await {
            return false;
        }
        let all = {
            let mut pages = self.pages.write().await;
            let strokes = pages.entry(page.to_owned()).or_default();
            strokes.push(stroke.clone());
            let all = strokes.clone();
            drop(pages);
            self.save_page(page, &all).await;
            all
        };
        let _ = all;
        self.fan_out(Some(page.to_owned()), stroke_frame(page, &stroke));
        true
    }

    /// Appends several strokes with one save and one snapshot broadcast (agent annotations).
    pub async fn append_many(&self, page: &str, strokes: Vec<StrokeMsg>) -> usize {
        let strokes: Vec<StrokeMsg> = strokes.into_iter().filter(is_well_formed).collect();
        if strokes.is_empty() || !self.has_page(page).await {
            return 0;
        }
        let added = strokes.len();
        let all = {
            let mut pages = self.pages.write().await;
            let current = pages.entry(page.to_owned()).or_default();
            current.extend(strokes);
            let all = current.clone();
            drop(pages);
            self.save_page(page, &all).await;
            all
        };
        self.fan_out(Some(page.to_owned()), page_frame(page, &all));
        added
    }

    /// Removes every agent-authored stroke from `page`; returns how many were removed.
    pub async fn clear_agent(&self, page: &str) -> usize {
        let (all, removed) = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.len();
            current.retain(|stroke| stroke.layer != AGENT_LAYER);
            (current.clone(), before - current.len())
        };
        if removed > 0 {
            self.save_page(page, &all).await;
            self.fan_out(Some(page.to_owned()), page_frame(page, &all));
        }
        removed
    }

    /// Removes the strokes whose seeds are listed; broadcasts the removal, not the whole page.
    pub async fn remove_strokes(&self, page: &str, seeds: &[String]) -> usize {
        if seeds.is_empty() {
            return 0;
        }
        let (all, removed) = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.len();
            current.retain(|stroke| !seeds.iter().any(|seed| seed == &stroke.seed));
            (current.clone(), before - current.len())
        };
        if removed == 0 {
            return 0;
        }
        self.save_page(page, &all).await;
        self.fan_out(Some(page.to_owned()), op_frame(page, &remove_op(seeds)));
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
                current.retain(|stroke| !seeds.iter().any(|seed| seed == &stroke.seed));
            }
            current.extend(strokes.clone());
            current.clone()
        };
        self.save_page(page, &all).await;
        self.fan_out(
            Some(page.to_owned()),
            op_frame(page, &replace_op(seeds, &strokes)),
        );
        all.len()
    }

    /// Empties one page (the tablet's clear).
    pub async fn clear_page(&self, page: &str) -> usize {
        let removed = {
            let mut pages = self.pages.write().await;
            let Some(current) = pages.get_mut(page) else {
                return 0;
            };
            let before = current.len();
            current.clear();
            before
        };
        if removed > 0 {
            self.save_page(page, &[]).await;
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
        {
            let mut pages = self.pages.write().await;
            pages.insert(page.to_owned(), strokes.clone());
        }
        self.save_page(page, &strokes).await;
        self.fan_out(Some(page.to_owned()), page_frame(page, &strokes));
        count
    }

    /// Creates a page from `request`; `Err` carries a message for the client.
    pub async fn create_page(&self, request: NewPage) -> Result<PageMeta, String> {
        let mut meta = self.meta.write().await;
        let notebook_id = request
            .notebook
            .clone()
            .or_else(|| {
                meta.notebook_of(&meta.active.clone())
                    .map(|notebook| notebook.id.clone())
            })
            .or_else(|| meta.notebooks.first().map(|notebook| notebook.id.clone()))
            .ok_or("no notebook")?;
        let requested = request.id.filter(|id| is_page_id(id));
        if let Some(id) = requested.as_deref()
            && let Some(existing) = meta.page(id).cloned()
        {
            return Ok(existing);
        }
        let notebook = meta
            .notebook_mut(&notebook_id)
            .ok_or_else(|| format!("unknown notebook {notebook_id}"))?;
        let number = notebook.pages.len() + 1;
        let page = PageMeta {
            id: requested.unwrap_or_else(|| new_id("pg")),
            title: title_or(request.title, format!("Page {number}"))?,
            kind: request.kind.unwrap_or_else(PageKind::a4),
            background: request.background.unwrap_or_default(),
        };
        notebook.pages.push(page.clone());
        let index = meta.clone();
        drop(meta);
        self.pages.write().await.insert(page.id.clone(), Vec::new());
        self.save_meta(&index).await;
        self.save_page(&page.id, &[]).await;
        self.fan_out(None, meta_frame_of(&index));
        Ok(page)
    }

    /// Applies a register update to one page.
    pub async fn patch_page(&self, id: &str, patch: PagePatch) -> Result<PageMeta, String> {
        if let Some(title) = patch.title.as_deref() {
            check_title(title)?;
        }
        let mut meta = self.meta.write().await;
        let page = meta
            .page_mut(id)
            .ok_or_else(|| format!("unknown page {id}"))?;
        if let Some(title) = patch.title {
            page.title = title;
        }
        if let Some(background) = patch.background {
            page.background = background;
        }
        let page = page.clone();
        let index = meta.clone();
        drop(meta);
        self.save_meta(&index).await;
        self.fan_out(None, meta_frame_of(&index));
        self.fan_out(Some(page.id.clone()), page_frame(&page.id, &[]));
        Ok(page)
    }

    /// Deletes a page; refuses the last one. Returns the new index.
    pub async fn delete_page(&self, id: &str) -> Result<Meta, String> {
        let mut meta = self.meta.write().await;
        let total: usize = meta.notebooks.iter().map(|n| n.pages.len()).sum();
        if total <= 1 {
            return Err("cannot delete the last page".to_owned());
        }
        let mut found = false;
        for notebook in &mut meta.notebooks {
            let before = notebook.pages.len();
            notebook.pages.retain(|page| page.id != id);
            found |= notebook.pages.len() != before;
        }
        if !found {
            return Err(format!("unknown page {id}"));
        }
        if meta.active == id {
            let survivor = meta
                .all_pages()
                .next()
                .map(|page| page.id.clone())
                .unwrap_or_default();
            meta.active = survivor;
        }
        let index = meta.clone();
        drop(meta);
        self.pages.write().await.remove(id);
        let _ = tokio::fs::remove_file(self.page_path(id)).await;
        self.save_meta(&index).await;
        self.fan_out(None, meta_frame_of(&index));
        self.fan_out(None, active_frame(&index.active));
        Ok(index)
    }

    /// Creates an empty notebook and returns it.
    pub async fn create_notebook(&self, title: String) -> Result<(Notebook, Meta), String> {
        check_title(&title)?;
        let mut meta = self.meta.write().await;
        let notebook = Notebook {
            id: new_id("nb"),
            title,
            pages: Vec::new(),
        };
        meta.notebooks.push(notebook.clone());
        let index = meta.clone();
        drop(meta);
        self.save_meta(&index).await;
        self.fan_out(None, meta_frame_of(&index));
        Ok((notebook, index))
    }

    async fn has_page(&self, id: &str) -> bool {
        self.meta.read().await.page(id).is_some()
    }

    fn page_path(&self, id: &str) -> PathBuf {
        self.data_dir.join("pages").join(format!("{id}.json"))
    }

    /// Fans one frame out; `page` decides which sessions receive it.
    pub fn fan_out(&self, page: Option<String>, text: String) {
        let _ = self.tx.send(Frame { page, text });
    }

    async fn save_meta(&self, meta: &Meta) {
        let body = match serde_json::to_vec_pretty(meta) {
            Ok(body) => body,
            Err(error) => {
                tracing::error!(%error, "meta serialize failed");
                return;
            }
        };
        write_atomic(&self.data_dir.join("meta.json"), body, "meta").await;
    }

    async fn save_page(&self, page: &str, strokes: &[StrokeMsg]) {
        let body = match disk::page_body(strokes) {
            Ok(body) => body,
            Err(error) => {
                tracing::error!(%error, "page serialize failed");
                return;
            }
        };
        let path = self.page_path(page);
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let body_for_write = body.clone();
        let result = tokio::task::spawn_blocking(move || {
            write_atomic_blocking(&path, &body_for_write).map_err(|error| error.to_string())
        })
        .await;
        match result {
            Ok(Ok(())) => tracing::debug!(page, strokes = strokes.len(), "page saved"),
            Ok(Err(error)) => tracing::error!(page, %error, "page save failed"),
            Err(error) => tracing::error!(page, %error, "page save task failed"),
        }
    }
}

#[cfg(test)]
mod tests;
