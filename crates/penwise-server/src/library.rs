//! Notebooks, pages and their stroke lists: the on-disk `meta.json` + `pages/<id>.json` layout, the frame fan-out,
//! and every mutation the HTTP routes and WebSocket frames can make.
//!
//! See `docs/pages.md` for the wire and disk contract this implements.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, broadcast};

/// A4 portrait at the current render scale, in page px.
pub const PAGE_WIDTH: f32 = 1600.0;
/// A4 portrait at the current render scale, in page px.
pub const PAGE_HEIGHT: f32 = 2560.0;
/// Ruling / grid / dot pitch of every non-blank background (7 mm at 340 dpi).
pub const BACKGROUND_SPACING_PX: f32 = 93.7;
/// Marks a stroke as assistant-authored; cleared as a set.
pub const AGENT_LAYER: &str = "agent";
/// Longest page or notebook title, in bytes.
pub const MAX_TITLE_BYTES: usize = 256;

/// One finished stroke as the tablet sends it. The seed is a decimal string because JS numbers cannot hold a
/// 64-bit value exactly; the server treats it as opaque.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrokeMsg {
    /// `ink` for the user's own strokes, `agent` for marks an assistant added (cleared as a set).
    #[serde(default = "default_layer")]
    pub layer: String,
    pub preset: u32,
    pub argb: i32,
    pub scale: f32,
    pub seed: String,
    /// Five floats per sample: x, y, pressure, tilt, azimuth.
    pub samples: Vec<f32>,
    /// Microseconds since the stroke started, one per sample.
    pub times: Vec<i32>,
}

fn default_layer() -> String {
    "ink".to_owned()
}

/// Page extent. Immutable after creation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum PageKind {
    /// `[0, width] × [0, height]` in page px.
    Fixed { width: f32, height: f32 },
    /// Unbounded; the PNG uses content bounds.
    Infinite,
}

impl PageKind {
    /// A4 portrait at the current scale.
    #[must_use]
    pub fn a4() -> Self {
        Self::Fixed {
            width: PAGE_WIDTH,
            height: PAGE_HEIGHT,
        }
    }
}

/// Paper pattern drawn under every layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    /// Plain paper.
    #[default]
    Blank,
    /// Horizontal rules every [`BACKGROUND_SPACING_PX`].
    Ruled,
    /// Square grid.
    Grid,
    /// Dot grid.
    Dots,
}

/// One page's settings (its strokes live in `pages/<id>.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageMeta {
    /// `pg_` plus 16 hex characters.
    pub id: String,
    pub title: String,
    pub kind: PageKind,
    pub background: Background,
}

/// A named group of pages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notebook {
    /// `nb_` plus 16 hex characters.
    pub id: String,
    pub title: String,
    pub pages: Vec<PageMeta>,
}

/// The whole library index, persisted as `meta.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    /// Schema version; currently 1.
    pub version: u32,
    /// Page agents address when they do not name one.
    pub active: String,
    pub notebooks: Vec<Notebook>,
}

impl Meta {
    /// The page with `id`, anywhere in the library.
    #[must_use]
    pub fn page(&self, id: &str) -> Option<&PageMeta> {
        self.notebooks
            .iter()
            .flat_map(|notebook| &notebook.pages)
            .find(|page| page.id == id)
    }

    /// The notebook holding page `id`.
    #[must_use]
    pub fn notebook_of(&self, id: &str) -> Option<&Notebook> {
        self.notebooks
            .iter()
            .find(|notebook| notebook.pages.iter().any(|page| page.id == id))
    }

    fn page_mut(&mut self, id: &str) -> Option<&mut PageMeta> {
        self.notebooks
            .iter_mut()
            .flat_map(|notebook| &mut notebook.pages)
            .find(|page| page.id == id)
    }

    fn notebook_mut(&mut self, id: &str) -> Option<&mut Notebook> {
        self.notebooks.iter_mut().find(|notebook| notebook.id == id)
    }

    /// Every page, in reading order.
    fn all_pages(&self) -> impl Iterator<Item = &PageMeta> {
        self.notebooks.iter().flat_map(|notebook| &notebook.pages)
    }
}

/// A fan-out frame: `page` is the page it belongs to, `None` for library-wide frames.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Page the frame is about, when it is page-scoped.
    pub page: Option<String>,
    /// The JSON text sent to clients.
    pub text: String,
}

/// Request body of `POST /pages`.
#[derive(Debug, Default, Deserialize)]
pub struct NewPage {
    /// Client-chosen page id (offline-first tablets create pages before they reach the server); ignored when it
    /// already exists.
    pub id: Option<String>,
    /// Defaults to `Page N`.
    pub title: Option<String>,
    /// Defaults to A4 portrait.
    pub kind: Option<PageKind>,
    /// Defaults to blank.
    pub background: Option<Background>,
    /// Target notebook; defaults to the active page's notebook, else the first.
    pub notebook: Option<String>,
}

/// Register update for `PATCH /pages/<id>`.
#[derive(Debug, Default, Deserialize)]
pub struct PagePatch {
    /// New title.
    pub title: Option<String>,
    /// New paper pattern.
    pub background: Option<Background>,
}

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
        let body = match serde_json::to_vec(&PageFile {
            version: 1,
            strokes: strokes.to_vec(),
        }) {
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

#[derive(serde::Serialize, serde::Deserialize)]
struct PageFile {
    version: u32,
    strokes: Vec<StrokeMsg>,
}

/// Minimum replayable payload for a stroke: one sample of geometry.
fn is_well_formed(stroke: &StrokeMsg) -> bool {
    stroke.samples.len() == stroke.times.len() * 5 && !stroke.times.is_empty()
}

fn stroke_frame(page: &str, stroke: &StrokeMsg) -> String {
    serde_json::json!({ "t": "stroke", "page": page, "stroke": stroke }).to_string()
}

fn page_frame(page: &str, strokes: &[StrokeMsg]) -> String {
    serde_json::json!({ "t": "page", "page": page, "strokes": strokes }).to_string()
}

fn meta_frame_of(meta: &Meta) -> String {
    serde_json::json!({ "t": "meta", "meta": meta }).to_string()
}

fn active_frame(page: &str) -> String {
    serde_json::json!({ "t": "active", "page": page }).to_string()
}

/// Random id: prefix, `_`, 16 lowercase hex characters.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the low 64 bits of the clock are plenty to keep ids apart"
)]
fn new_id(prefix: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    // Mix the clock with a per-process counter so two pages created in the same nanosecond differ.
    let bump = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mixed = nanos as u64 ^ bump.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{prefix}_{mixed:016x}")
}

/// `pg_` plus 16 lowercase hex characters, the id shape both the server and the tablet generate.
fn is_page_id(id: &str) -> bool {
    id.len() == 19
        && id.starts_with("pg_")
        && id[3..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn check_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("title is empty".to_owned());
    }
    if title.len() > MAX_TITLE_BYTES {
        return Err(format!("title longer than {MAX_TITLE_BYTES} bytes"));
    }
    Ok(())
}

fn title_or(title: Option<String>, fallback: String) -> Result<String, String> {
    match title {
        Some(title) => {
            check_title(&title)?;
            Ok(title)
        }
        None => Ok(fallback),
    }
}

async fn load_or_create_meta(dir: &Path) -> Meta {
    let path = dir.join("meta.json");
    if let Ok(body) = tokio::fs::read(&path).await
        && let Ok(meta) = serde_json::from_slice::<Meta>(&body)
        && meta.page(&meta.active).is_some()
    {
        return meta;
    }
    let meta = migrate_or_fresh(dir).await;
    if let Ok(body) = serde_json::to_vec_pretty(&meta) {
        let _ = tokio::fs::create_dir_all(dir).await;
        let _ = tokio::fs::write(&path, body).await;
    }
    meta
}

/// First run: one notebook with one A4 page, inheriting any legacy `page.json` strokes.
async fn migrate_or_fresh(dir: &Path) -> Meta {
    let page = PageMeta {
        id: new_id("pg"),
        title: "Page 1".to_owned(),
        kind: PageKind::a4(),
        background: Background::Blank,
    };
    let legacy = read_legacy(dir).await;
    let meta = Meta {
        version: 1,
        active: page.id.clone(),
        notebooks: vec![Notebook {
            id: new_id("nb"),
            title: "Notebook 1".to_owned(),
            pages: vec![page.clone()],
        }],
    };
    if let Some(strokes) = legacy {
        let pages_dir = dir.join("pages");
        let _ = tokio::fs::create_dir_all(&pages_dir).await;
        if let Ok(body) = serde_json::to_vec(&PageFile {
            version: 1,
            strokes,
        }) {
            let _ = tokio::fs::write(pages_dir.join(format!("{}.json", page.id)), body).await;
        }
        tracing::info!(page = %page.id, "migrated legacy page.json");
    }
    meta
}

async fn read_legacy(dir: &Path) -> Option<Vec<StrokeMsg>> {
    let body = tokio::fs::read(dir.join("page.json")).await.ok()?;
    let page: PageFile = serde_json::from_slice(&body).ok()?;
    Some(
        page.strokes
            .into_iter()
            .filter(is_well_formed)
            .collect::<Vec<_>>(),
    )
}

async fn read_strokes(dir: &Path, page: &str) -> Result<Vec<StrokeMsg>, String> {
    let path = dir.join("pages").join(format!("{page}.json"));
    match tokio::fs::read(&path).await {
        Ok(body) => {
            let file: PageFile = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
            Ok(file
                .strokes
                .into_iter()
                .filter(is_well_formed)
                .collect::<Vec<_>>())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

async fn write_atomic(path: &Path, body: Vec<u8>, label: &'static str) {
    let path = path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        write_atomic_blocking(&path, &body).map_err(|e| e.to_string())
    })
    .await;
    match result {
        Ok(Ok(())) => tracing::debug!(label, "saved"),
        Ok(Err(error)) => tracing::error!(label, %error, "save failed"),
        Err(error) => tracing::error!(label, %error, "save task failed"),
    }
}

fn write_atomic_blocking(path: &Path, body: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let staging = path.with_extension("json.tmp");
    std::fs::write(&staging, body)?;
    std::fs::rename(&staging, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke() -> StrokeMsg {
        StrokeMsg {
            layer: "ink".to_owned(),
            preset: 0,
            argb: -16_777_216,
            scale: 1.0,
            seed: "42".to_owned(),
            samples: vec![0.0, 0.0, 0.5, 0.0, 0.0, 10.0, 10.0, 0.5, 0.0, 0.0],
            times: vec![0, 8_000],
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("penwise-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[tokio::test]
    async fn first_run_creates_one_notebook_and_one_page() {
        let dir = temp_dir("fresh");
        let library = Library::load(dir.clone()).await;
        let meta = library.meta().await;
        assert_eq!(1, meta.notebooks.len());
        assert_eq!(1, meta.notebooks[0].pages.len());
        assert_eq!(meta.notebooks[0].pages[0].id, meta.active);
        assert!(dir.join("meta.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_legacy_page_json_migrates_into_page_one() {
        let dir = temp_dir("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = serde_json::json!({ "version": 1, "strokes": [stroke()] });
        std::fs::write(dir.join("page.json"), legacy.to_string()).unwrap();
        let library = Library::load(dir.clone()).await;
        let active = library.active().await;
        assert_eq!(1, library.strokes(&active).await.len(), "strokes moved");
        assert!(dir.join("page.json").is_file(), "legacy file left in place");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn pages_created_after_the_first_are_numbered_and_persisted() {
        let dir = temp_dir("create");
        let library = Library::load(dir.clone()).await;
        let page = library
            .create_page(NewPage {
                background: Some(Background::Grid),
                ..NewPage::default()
            })
            .await
            .unwrap();
        assert_eq!("Page 2", page.title);
        assert_eq!(Background::Grid, page.background);
        assert_eq!(PageKind::a4(), page.kind);
        assert!(library.strokes(&page.id).await.is_empty());
        // A fresh load sees both pages and the stored background.
        drop(library);
        let reloaded = Library::load(dir.clone()).await;
        let meta = reloaded.meta().await;
        assert_eq!(2, meta.notebooks[0].pages.len());
        assert_eq!(
            Some(&Background::Grid),
            meta.page(&page.id).map(|p| &p.background)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn strokes_survive_a_reload_per_page() {
        let dir = temp_dir("strokes");
        let library = Library::load(dir.clone()).await;
        let first = library.active().await;
        let second = library.create_page(NewPage::default()).await.unwrap();
        assert!(library.append(&first, stroke()).await);
        assert!(library.append(&second.id, stroke()).await);
        assert!(library.append(&second.id, stroke()).await);
        drop(library);
        let reloaded = Library::load(dir.clone()).await;
        assert_eq!(1, reloaded.strokes(&first).await.len());
        assert_eq!(2, reloaded.strokes(&second.id).await.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_last_page_cannot_be_deleted() {
        let dir = temp_dir("delete");
        let library = Library::load(dir.clone()).await;
        let only = library.active().await;
        assert!(library.delete_page(&only).await.is_err());
        let second = library.create_page(NewPage::default()).await.unwrap();
        let meta = library.delete_page(&second.id).await.unwrap();
        assert_eq!(1, meta.notebooks[0].pages.len());
        assert_eq!(only, meta.active, "active page follows a deletion");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn deleting_the_active_page_moves_active_to_a_survivor() {
        let dir = temp_dir("delete-active");
        let library = Library::load(dir.clone()).await;
        let second = library.create_page(NewPage::default()).await.unwrap();
        assert!(library.select(&second.id).await);
        let meta = library.delete_page(&second.id).await.unwrap();
        assert_ne!(second.id, meta.active);
        assert!(meta.page(&meta.active).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_patch_updates_title_and_background() {
        let dir = temp_dir("patch");
        let library = Library::load(dir.clone()).await;
        let active = library.active().await;
        let page = library
            .patch_page(
                &active,
                PagePatch {
                    title: Some("Groceries".to_owned()),
                    background: Some(Background::Ruled),
                },
            )
            .await
            .unwrap();
        assert_eq!("Groceries", page.title);
        assert_eq!(Background::Ruled, page.background);
        assert!(
            library
                .patch_page("pg_missing", PagePatch::default())
                .await
                .is_err()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn agent_marks_are_cleared_without_touching_ink() {
        let dir = temp_dir("agent");
        let library = Library::load(dir.clone()).await;
        let page = library.active().await;
        library.append(&page, stroke()).await;
        let mut mark = stroke();
        mark.layer = AGENT_LAYER.to_owned();
        library.append_many(&page, vec![mark]).await;
        assert_eq!(2, library.strokes(&page).await.len());
        assert_eq!(1, library.clear_agent(&page).await);
        assert_eq!(1, library.strokes(&page).await.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn frames_carry_the_page_they_belong_to() {
        let dir = temp_dir("frames");
        let library = Library::load(dir.clone()).await;
        let page = library.active().await;
        let mut events = library.subscribe();
        library.append(&page, stroke()).await;
        let frame = events.recv().await.unwrap();
        assert_eq!(Some(page.clone()), frame.page);
        assert!(frame.text.contains("\"t\":\"stroke\""));
        let mut events = library.subscribe();
        library.create_page(NewPage::default()).await.unwrap();
        let frame = events.recv().await.unwrap();
        assert_eq!(None, frame.page, "library changes are global");
        assert!(frame.text.contains("\"t\":\"meta\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn malformed_strokes_and_unknown_pages_are_refused() {
        let dir = temp_dir("refuse");
        let library = Library::load(dir.clone()).await;
        let page = library.active().await;
        let mut broken = stroke();
        broken.times.clear();
        assert!(!library.append(&page, broken).await);
        assert!(!library.append("pg_missing", stroke()).await);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ids_are_prefixed_and_unique() {
        let a = new_id("pg");
        let b = new_id("pg");
        assert!(a.starts_with("pg_"));
        assert_eq!(19, a.len());
        assert_ne!(a, b);
    }

    #[test]
    fn client_ids_are_accepted_only_in_the_expected_shape() {
        assert!(is_page_id("pg_0123456789abcdef"));
        assert!(!is_page_id("pg_short"));
        assert!(!is_page_id("nb_0123456789abcdef"));
        assert!(!is_page_id("pg_0123456789ABCDEF"));
    }

    #[tokio::test]
    async fn a_client_supplied_id_is_kept_and_is_idempotent() {
        let dir = temp_dir("client-id");
        let library = Library::load(dir.clone()).await;
        let request = || NewPage {
            id: Some("pg_00000000deadbeef".to_owned()),
            ..NewPage::default()
        };
        let page = library.create_page(request()).await.unwrap();
        assert_eq!("pg_00000000deadbeef", page.id);
        let again = library.create_page(request()).await.unwrap();
        assert_eq!(page.id, again.id);
        assert_eq!(2, library.meta().await.notebooks[0].pages.len());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn titles_are_bounded() {
        assert!(check_title("ok").is_ok());
        assert!(check_title("   ").is_err());
        assert!(check_title(&"x".repeat(MAX_TITLE_BYTES + 1)).is_err());
    }
}
