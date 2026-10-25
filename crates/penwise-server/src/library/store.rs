//! Durable state: `SQLite`, one row per object.
//!
//! Pages used to be one JSON file each, rewritten in full on every append, with strokes as raw float arrays
//! (~11 bytes per float). That is quadratic over a session and cannot hold pictures. Here every stroke or
//! picture is a row, so an append writes one row, and stroke payloads are compact blobs
//! ([`super::stroke_codec`]).
//!
//! The connection is a single `SQLite` handle behind a mutex: every statement is a microsecond-scale
//! single-row write for one user, so a pool would buy nothing but risk.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension, params};

use super::model::{
    AGENT_LAYER, Background, ImageMsg, Meta, Notebook, PageContent, PageKind, PageMeta, StrokeMsg,
};
use super::stroke_codec;

/// How many archives per page survive; older ones are dropped.
pub(super) const HISTORY_KEEP: i64 = 10;

/// Schema version written to `PRAGMA user_version`.
const SCHEMA_VERSION: i64 = 3;

/// One page's objects, as stored. Cheap to clone: the handle is shared.
#[derive(Clone)]
pub(crate) struct Store {
    pub(super) conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Store")
    }
}

impl Store {
    /// Opens (creating and migrating) the database at `path`.
    pub(crate) fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let conn = Connection::open(path).map_err(|error| error.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| error.to_string())?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(|error| error.to_string())?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|error| error.to_string())?;
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if version < SCHEMA_VERSION {
            create_schema(&conn)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)
                .map_err(|error| error.to_string())?;
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// The whole library: index plus every page's objects.
    pub(crate) fn load(&self) -> Result<(Meta, HashMap<String, PageContent>), String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let mut meta = Meta {
            version: 1,
            active: String::new(),
            notebooks: Vec::new(),
        };
        let mut notebooks: Vec<Notebook> = Vec::new();
        let mut stmt = conn
            .prepare("SELECT id, title FROM notebooks WHERE deleted = 0 ORDER BY position, id")
            .map_err(|error| error.to_string())?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Notebook {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    pages: Vec::new(),
                })
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            notebooks.push(row.map_err(|error| error.to_string())?);
        }
        let mut pages: HashMap<String, PageContent> = HashMap::new();
        for notebook in &mut notebooks {
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, kind, background FROM pages
                     WHERE notebook = ?1 AND deleted = 0 ORDER BY position, id",
                )
                .map_err(|error| error.to_string())?;
            let rows = stmt
                .query_map(params![notebook.id], |row| {
                    let kind: String = row.get(2)?;
                    let background: String = row.get(3)?;
                    Ok(PageMeta {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        kind: page_kind_from(&kind),
                        background: background_from(&background),
                    })
                })
                .map_err(|error| error.to_string())?;
            for row in rows {
                let page = row.map_err(|error| error.to_string())?;
                pages.insert(page.id.clone(), PageContent::default());
                notebook.pages.push(page);
            }
        }
        meta.notebooks = notebooks;
        meta.active = conn
            .query_row("SELECT value FROM state WHERE key = 'active'", [], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|error| error.to_string())?
            .unwrap_or_default();
        for (page_id, content) in &mut pages {
            *content = read_objects(&conn, page_id)?;
        }
        Ok((meta, pages))
    }

    /// Writes the index: upserts every notebook and page, drops the ones that are gone, remembers the active
    /// page. Upsert rather than replace-all because deleting a `pages` row cascades to its objects.
    pub(crate) fn write_meta(&self, meta: &Meta) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for (notebook_index, notebook) in meta.notebooks.iter().enumerate() {
            tx.execute(
                "INSERT INTO notebooks (id, title, position, deleted) VALUES (?1, ?2, ?3, 0)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title, position = excluded.position,
                     deleted = 0",
                params![
                    notebook.id,
                    notebook.title,
                    i64::try_from(notebook_index).unwrap_or(i64::MAX)
                ],
            )
            .map_err(|error| error.to_string())?;
            for (page_index, page) in notebook.pages.iter().enumerate() {
                tx.execute(
                    "INSERT INTO pages (id, notebook, title, kind, background, position, deleted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)
                     ON CONFLICT(id) DO UPDATE SET notebook = excluded.notebook,
                         title = excluded.title, kind = excluded.kind,
                         background = excluded.background, position = excluded.position, deleted = 0",
                    params![
                        page.id,
                        notebook.id,
                        page.title,
                        page_kind_to(page.kind),
                        background_to(page.background),
                        i64::try_from(page_index).unwrap_or(i64::MAX)
                    ],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        let page_ids: Vec<String> = meta.all_pages().map(|page| page.id.clone()).collect();
        let notebook_ids: Vec<String> = meta
            .notebooks
            .iter()
            .map(|notebook| notebook.id.clone())
            .collect();
        tx.execute(
            "DELETE FROM pages WHERE id NOT IN (SELECT value FROM json_each(?1))",
            params![serde_json::to_string(&page_ids).map_err(|error| error.to_string())?],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "DELETE FROM notebooks WHERE id NOT IN (SELECT value FROM json_each(?1))",
            params![serde_json::to_string(&notebook_ids).map_err(|error| error.to_string())?],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO state (key, value) VALUES ('active', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![meta.active],
        )
        .map_err(|error| error.to_string())?;
        tx.commit().map_err(|error| error.to_string())
    }

    /// True when the database holds no pages yet (first run or an un-migrated JSON library).
    pub(crate) fn is_empty(&self) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM pages", [], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        Ok(count == 0)
    }

    /// Inserts one stroke unless the page already has that seed.
    pub(crate) fn add_stroke(&self, page: &str, stroke: &StrokeMsg) -> Result<bool, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO objects (page, id, kind, layer, z, codec, payload)
                 SELECT ?1, ?2, 'stroke', ?3,
                        COALESCE((SELECT MAX(z) + 65536 FROM objects WHERE page = ?1), 0),
                        'stroke_v1', ?4",
                params![
                    page,
                    stroke.seed,
                    stroke.layer,
                    stroke_codec::encode(stroke)
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(changed > 0)
    }

    /// Inserts several strokes in one transaction; returns how many were new.
    pub(crate) fn add_strokes(&self, page: &str, strokes: &[StrokeMsg]) -> Result<usize, String> {
        if strokes.is_empty() {
            return Ok(0);
        }
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let mut added = 0;
        let mut z: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(z) + 65536, 0) FROM objects WHERE page = ?1",
                params![page],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        for stroke in strokes {
            let changed = tx
                .execute(
                    "INSERT OR IGNORE INTO objects (page, id, kind, layer, z, codec, payload)
                     VALUES (?1, ?2, 'stroke', ?3, ?4, 'stroke_v1', ?5)",
                    params![
                        page,
                        stroke.seed,
                        stroke.layer,
                        z,
                        stroke_codec::encode(stroke)
                    ],
                )
                .map_err(|error| error.to_string())?;
            if changed > 0 {
                added += 1;
                z += 65536;
            }
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(added)
    }

    /// Inserts one picture unless the page already has that id.
    pub(crate) fn add_image(&self, page: &str, image: &ImageMsg) -> Result<bool, String> {
        let payload = serde_json::to_vec(image).map_err(|error| error.to_string())?;
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO objects (page, id, kind, layer, z, codec, payload)
                 SELECT ?1, ?2, 'image', ?3,
                        COALESCE((SELECT MIN(z) - 65536 FROM objects WHERE page = ?1), 0),
                        'image_v1', ?4",
                params![page, image.id, image.layer, payload],
            )
            .map_err(|error| error.to_string())?;
        Ok(changed > 0)
    }

    /// Deletes objects by id; returns how many rows went.
    pub(crate) fn remove(&self, page: &str, ids: &[String]) -> Result<usize, String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        let mut removed = 0;
        for id in ids {
            removed += tx
                .execute(
                    "DELETE FROM objects WHERE page = ?1 AND id = ?2",
                    params![page, id],
                )
                .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(removed)
    }

    /// Deletes every agent-authored stroke on the page.
    pub(crate) fn remove_agent_strokes(&self, page: &str) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute(
            "DELETE FROM objects WHERE page = ?1 AND kind = 'stroke' AND layer = ?2",
            params![page, AGENT_LAYER],
        )
        .map_err(|error| error.to_string())
    }

    /// Empties one page, pictures included.
    pub(crate) fn clear(&self, page: &str) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|error| error.to_string())?;
        conn.execute("DELETE FROM objects WHERE page = ?1", params![page])
            .map_err(|error| error.to_string())
    }

    /// Replaces every stroke on a page and keeps its pictures (tablet undo, erase, clear).
    pub(crate) fn replace_strokes(&self, page: &str, strokes: &[StrokeMsg]) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|error| error.to_string())?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        tx.execute(
            "DELETE FROM objects WHERE page = ?1 AND kind = 'stroke'",
            params![page],
        )
        .map_err(|error| error.to_string())?;
        for (index, stroke) in strokes.iter().enumerate() {
            tx.execute(
                "INSERT OR REPLACE INTO objects (page, id, kind, layer, z, codec, payload)
                 VALUES (?1, ?2, 'stroke', ?3, ?4, 'stroke_v1', ?5)",
                params![
                    page,
                    stroke.seed,
                    stroke.layer,
                    i64::try_from(index).unwrap_or(i64::MAX) * 65_536,
                    stroke_codec::encode(stroke)
                ],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())
    }
}

pub(super) fn read_objects(conn: &Connection, page: &str) -> Result<PageContent, String> {
    let mut content = PageContent::default();
    let mut stmt = conn
        .prepare(
            "SELECT id, kind, layer, codec, payload FROM objects WHERE page = ?1 ORDER BY z, id",
        )
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![page], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let (_, kind, layer, codec, payload) = row.map_err(|error| error.to_string())?;
        match (kind.as_str(), codec.as_str()) {
            ("stroke", "stroke_v1") => {
                if let Some(stroke) = stroke_codec::decode(&payload, &layer) {
                    content.strokes.push(stroke);
                } else {
                    tracing::warn!(page = %page, "stroke blob undecodable");
                }
            }
            ("image", "image_v1") => match serde_json::from_slice::<ImageMsg>(&payload) {
                Ok(image) => content.images.push(image),
                Err(error) => tracing::warn!(page = %page, %error, "image row unreadable"),
            },
            _ => tracing::warn!(page = %page, %kind, %codec, "unknown object"),
        }
    }
    Ok(content)
}

fn create_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS notebooks (
             id TEXT PRIMARY KEY,
             title TEXT NOT NULL,
             position INTEGER NOT NULL,
             deleted INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS pages (
             id TEXT PRIMARY KEY,
             notebook TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
             title TEXT NOT NULL,
             kind TEXT NOT NULL,
             background TEXT NOT NULL,
             position INTEGER NOT NULL,
             deleted INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS objects (
             page TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE,
             id TEXT NOT NULL,
             kind TEXT NOT NULL,
             layer TEXT NOT NULL,
             z INTEGER NOT NULL,
             codec TEXT NOT NULL,
             payload BLOB NOT NULL,
             PRIMARY KEY (page, id)
         );
         CREATE INDEX IF NOT EXISTS objects_order ON objects(page, z, id);
         CREATE TABLE IF NOT EXISTS anchors (
             page TEXT NOT NULL,
             selector TEXT NOT NULL,
             rect TEXT NOT NULL,
             role TEXT,
             text TEXT,
             url TEXT,
             stale INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (page, selector)
         );
         CREATE TABLE IF NOT EXISTS state (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS ink_anchors (
             page TEXT NOT NULL,
             object TEXT NOT NULL,
             selector TEXT NOT NULL,
             PRIMARY KEY (page, object)
         );
         CREATE TABLE IF NOT EXISTS object_history (
             page TEXT NOT NULL,
             stamp INTEGER NOT NULL,
             id TEXT NOT NULL,
             kind TEXT NOT NULL,
             layer TEXT NOT NULL,
             z INTEGER NOT NULL,
             codec TEXT NOT NULL,
             payload BLOB NOT NULL
         );
         CREATE INDEX IF NOT EXISTS object_history_order ON object_history(page, stamp);",
    )
    .map_err(|error| error.to_string())
}

fn page_kind_to(kind: PageKind) -> String {
    match kind {
        PageKind::Fixed { width, height } => format!("fixed:{width}:{height}"),
        PageKind::Infinite => "infinite".to_owned(),
    }
}

fn page_kind_from(text: &str) -> PageKind {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() == 3
        && parts[0] == "fixed"
        && let (Ok(width), Ok(height)) = (parts[1].parse(), parts[2].parse())
    {
        return PageKind::Fixed { width, height };
    }
    PageKind::Infinite
}

fn background_to(background: Background) -> String {
    match background {
        Background::Blank => "blank",
        Background::Ruled => "ruled",
        Background::Grid => "grid",
        Background::Dots => "dots",
    }
    .to_owned()
}

fn background_from(text: &str) -> Background {
    match text {
        "ruled" => Background::Ruled,
        "grid" => Background::Grid,
        "dots" => Background::Dots,
        _ => Background::Blank,
    }
}
