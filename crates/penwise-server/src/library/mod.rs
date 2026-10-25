//! Notebooks, pages and their stroke lists: the on-disk layout, the frame fan-out, and every mutation the HTTP
//! routes and WebSocket frames can make.
//!
//! See `docs/pages.md` for the wire and disk contract this implements.

mod anchors;
mod archive;
mod legacy;
mod model;
mod pictures;
mod store;
mod stroke_codec;
mod strokes;
mod text;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{RwLock, broadcast};

pub use anchors::Anchor;
pub use model::{
    AGENT_LAYER, BACKGROUND_SPACING_PX, Background, Frame, ImageMsg, Meta, NewPage, Notebook,
    PageContent, PageKind, PageMeta, PagePatch, StrokeMsg,
};
use model::{active_frame, check_title, is_page_id, meta_frame_of, new_id, page_frame, title_or};
use pictures::import_json_library;
pub use pictures::new_image_id;
use store::Store;

/// The library: index plus stroke lists, all shared behind one `Arc`.
#[allow(
    missing_debug_implementations,
    reason = "holds live locks and stroke buffers; nothing needs to print a library"
)]
pub struct Library {
    meta: RwLock<Meta>,
    /// Every page's objects, decoded. One user's library is a few MB, so it is all resident; the database is
    /// the source of truth and each mutation writes only the rows it touched.
    pages: RwLock<HashMap<String, PageContent>>,
    tx: broadcast::Sender<Frame>,
    store: Store,
    data_dir: PathBuf,
}

impl Library {
    /// Loads `meta.json` (migrating a legacy `page.json` on first run) and every page's strokes.
    pub fn load(data_dir: PathBuf) -> Arc<Self> {
        let store = match Store::open(&data_dir.join("penwise.db")) {
            Ok(store) => store,
            Err(error) => {
                tracing::error!(%error, "cannot open the library database");
                std::process::exit(1);
            }
        };
        let empty = store.is_empty().unwrap_or(false);
        if empty {
            import_json_library(&store, &data_dir);
        }
        let (meta, pages) = match store.load() {
            Ok(loaded) => loaded,
            Err(error) => {
                tracing::error!(%error, "cannot read the library database");
                std::process::exit(1);
            }
        };
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
            store,
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
            kind: request.kind.unwrap_or_else(PageKind::landscape),
            background: request.background.unwrap_or_default(),
        };
        notebook.pages.push(page.clone());
        let index = meta.clone();
        drop(meta);
        self.pages
            .write()
            .await
            .insert(page.id.clone(), PageContent::default());
        self.save_meta(&index).await;
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
        let content = self.content(&page.id).await.unwrap_or_default();
        self.fan_out(Some(page.id.clone()), page_frame(&page.id, &content));
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
        let _ = tokio::fs::remove_file(self.data_dir.join("pages").join(format!("{id}.txt"))).await;
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

    /// Renames a notebook.
    pub async fn rename_notebook(&self, id: &str, title: String) -> Result<Meta, String> {
        check_title(&title)?;
        let mut meta = self.meta.write().await;
        let notebook = meta
            .notebook_mut(id)
            .ok_or_else(|| format!("unknown notebook {id}"))?;
        notebook.title = title;
        let index = meta.clone();
        drop(meta);
        self.save_meta(&index).await;
        self.fan_out(None, meta_frame_of(&index));
        Ok(index)
    }

    /// Deletes an empty notebook; refuses the last notebook and any notebook that still holds pages.
    pub async fn delete_notebook(&self, id: &str) -> Result<Meta, String> {
        let mut meta = self.meta.write().await;
        if meta.notebooks.len() <= 1 {
            return Err("cannot delete the last notebook".to_owned());
        }
        let Some(notebook) = meta.notebooks.iter().find(|notebook| notebook.id == id) else {
            return Err(format!("unknown notebook {id}"));
        };
        if !notebook.pages.is_empty() {
            return Err("move or delete the notebook's pages first".to_owned());
        }
        meta.notebooks.retain(|notebook| notebook.id != id);
        let index = meta.clone();
        drop(meta);
        self.save_meta(&index).await;
        self.fan_out(None, meta_frame_of(&index));
        Ok(index)
    }

    /// The recognised text for `page`, when it has been read before.
    pub async fn text(&self, page: &str) -> Option<String> {
        text::read(&self.data_dir, page).await
    }

    /// Stores the recognised text beside the page.
    pub async fn put_text(&self, page: &str, text: &str) {
        text::write(&self.data_dir, page, text).await;
    }

    async fn has_page(&self, id: &str) -> bool {
        self.meta.read().await.page(id).is_some()
    }

    /// Fans one frame out; `page` decides which sessions receive it.
    pub fn fan_out(&self, page: Option<String>, text: String) {
        let _ = self.tx.send(Frame { page, text });
    }

    async fn save_meta(&self, meta: &Meta) {
        let meta = meta.clone();
        let store = self.store.clone();
        let result = tokio::task::spawn_blocking(move || store.write_meta(&meta)).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::error!(%error, "index write failed"),
            Err(error) => tracing::error!(%error, "index write task failed"),
        }
    }
}

#[cfg(test)]
mod tests;
