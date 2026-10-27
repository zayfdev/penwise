//! Generator of historical v3 databases (`PRAGMA user_version = 3`, the pre-rewrite `library/store.rs` schema).
//!
//! Synthetic data only; never read the live DB. [`DbFixture::build`] writes every historical shape the migration
//! must survive and returns [`Expected`], the ground truth the migration tests assert against.

use std::path::Path;

use rusqlite::Connection;

mod seed;
mod structure;

use seed::Rows;

/// Flat-sample count per stroke row (5 floats each: x, y, pressure, tilt, azimuth).
const STRIDE: usize = 5;

/// One stroke row the migration must carry over.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeExpect {
    pub page: String,
    /// `objects.id` (the legacy seed text).
    pub row_id: String,
    pub argb: i32,
    pub z: i64,
    pub agent: bool,
    pub samples: usize,
    /// `seed` the migrated item must have (two's complement of a signed id, `u64` parse, or hash for text ids).
    /// `None` for the non-numeric id (only determinism is asserted).
    pub seed: Option<u64>,
}

/// One image row the migration must carry over.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageExpect {
    pub page: String,
    pub row_id: String,
    pub z: i64,
    pub label: Option<String>,
}

/// Per-page ground truth.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageExpect {
    pub id: String,
    pub notebook: String,
    pub deleted: bool,
    /// Decodable strokes + images.
    pub migrated: usize,
    /// Rows that must land in `legacy_unmigrated`.
    pub unmigrated: usize,
}

/// Everything a migration test asserts.
#[derive(Debug, Clone, Default)]
pub struct Expected {
    pub notebooks: Vec<(String, bool)>,
    pub pages: Vec<PageExpect>,
    pub strokes: Vec<StrokeExpect>,
    pub images: Vec<ImageExpect>,
    pub objects_rows: usize,
    pub history_rows: usize,
    pub anchors_rows: usize,
    pub ink_anchor_rows: usize,
    pub active_page: String,
}

impl Expected {
    /// `migrated + unmigrated` over all pages (must equal `objects_rows`).
    #[must_use]
    pub fn accounted_rows(&self) -> usize {
        self.pages.iter().map(|p| p.migrated + p.unmigrated).sum()
    }

    /// Expected page record by legacy id.
    #[must_use]
    pub fn page(&self, id: &str) -> Option<&PageExpect> {
        self.pages.iter().find(|p| p.id == id)
    }
}

/// Raw `stroke_v1` blob, byte-compatible with the historical `stroke_codec::encode`.
#[must_use]
#[allow(clippy::cast_possible_truncation, reason = "fixture sizes are tiny")]
pub fn stroke_blob(
    preset: u32,
    argb: i32,
    scale: f32,
    blob_seed: u64,
    samples: &[f32],
    times: &[i32],
) -> Vec<u8> {
    let count = times.len();
    let mut out = vec![1_u8];
    out.extend_from_slice(&preset.to_le_bytes());
    out.extend_from_slice(&argb.to_le_bytes());
    out.extend_from_slice(&scale.to_le_bytes());
    out.extend_from_slice(&blob_seed.to_le_bytes());
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for i in 0..count {
        out.extend_from_slice(&samples[i * STRIDE].to_le_bytes());
        out.extend_from_slice(&samples[i * STRIDE + 1].to_le_bytes());
    }
    for i in 0..count {
        for k in 2..5 {
            out.extend_from_slice(&samples[i * STRIDE + k].to_le_bytes());
        }
    }
    for t in times {
        out.extend_from_slice(&t.to_le_bytes());
    }
    out
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "fixture"
)]
pub(crate) fn wave(n: usize, phase: f32) -> (Vec<f32>, Vec<i32>) {
    let mut samples = Vec::new();
    let mut times = Vec::new();
    for i in 0..n {
        let t = i as f32;
        samples.extend_from_slice(&[
            40.0 + t * 7.5 + phase,
            100.0 + (t * 0.7 + phase).sin() * 30.0,
            0.3 + (t % 5.0) * 0.1,
            0.2,
            -0.4,
        ]);
        times.push((i * 8_333) as i32);
    }
    (samples, times)
}

/// The v3 DB generator.
#[derive(Debug, Clone, Copy)]
pub struct DbFixture;

impl DbFixture {
    /// Creates `path` as a v3 database with every historical shape and returns the ground truth.
    ///
    /// # Errors
    /// The `SQLite` error text.
    pub fn build(path: &Path) -> Result<Expected, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| e.to_string())?;
        conn.execute_batch(V3_SCHEMA).map_err(|e| e.to_string())?;
        let mut rows = Rows::new(&conn);
        rows.structure()?;
        rows.strokes()?;
        rows.images()?;
        rows.quarantined()?;
        rows.extras()?;
        conn.pragma_update(None, "user_version", 3)
            .map_err(|e| e.to_string())?;
        conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")
            .map_err(|e| e.to_string())
            .ok();
        Ok(rows.expect)
    }
}

const V3_SCHEMA: &str = "
CREATE TABLE notebooks (id TEXT PRIMARY KEY, title TEXT NOT NULL, position INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE pages (id TEXT PRIMARY KEY, notebook TEXT NOT NULL REFERENCES notebooks(id) ON DELETE CASCADE,
  title TEXT NOT NULL, kind TEXT NOT NULL, background TEXT NOT NULL, position INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0);
CREATE TABLE objects (page TEXT NOT NULL REFERENCES pages(id) ON DELETE CASCADE, id TEXT NOT NULL, kind TEXT NOT NULL,
  layer TEXT NOT NULL, z INTEGER NOT NULL, codec TEXT NOT NULL, payload BLOB NOT NULL, PRIMARY KEY (page, id));
CREATE INDEX objects_order ON objects(page, z, id);
CREATE TABLE anchors (page TEXT NOT NULL, selector TEXT NOT NULL, rect TEXT NOT NULL, role TEXT, text TEXT, url TEXT,
  stale INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (page, selector));
CREATE TABLE state (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE ink_anchors (page TEXT NOT NULL, object TEXT NOT NULL, selector TEXT NOT NULL, PRIMARY KEY (page, object));
CREATE TABLE object_history (page TEXT NOT NULL, stamp INTEGER NOT NULL, id TEXT NOT NULL, kind TEXT NOT NULL,
  layer TEXT NOT NULL, z INTEGER NOT NULL, codec TEXT NOT NULL, payload BLOB NOT NULL);
CREATE INDEX object_history_order ON object_history(page, stamp);
";
