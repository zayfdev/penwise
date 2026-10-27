//! One-time import of the tablet's pre-rewrite page files (design §3.7).
//!
//! The old `StrokeFile` (`pages/<pageId>.bin`): magic `PWST`, `u8` version 1, then records, all big-endian
//! (`DataOutputStream`): `u8 tag = 1, i32 preset, i32 argb, f32 sizeScale, i64 seed, i32 count,
//! count × (5 × f32 x y pressure tilt azimuth, i32 timeUs)`. A torn or corrupt tail keeps the strokes before it,
//! like the Kotlin reader. The old undo files are **not** imported: undo is memory-only now (owner decision 2).
//!
//! Every stroke becomes `Add` with the deterministic ids of `penwise_proto::legacy` (item id from the page text
//! and the stroke's identity text, op id from the item id). The identity is the seed text, except that a seed
//! an earlier stroke of the same file already used (the oldest files had no seeds, so many carry `0`) gets
//! `"<seed>#k"`, `k` counting the repeats in file order: unique per file and still deterministic. The brush
//! seed stays the stroke's own, so the server's own migration of the same row and every other
//! device's import produce the same id/op id and union by dedupe. Strokes the replica already holds (the
//! server's migrated copy arrived first) are skipped: import once [`SyncCore::is_caught_up`]. An import made
//! earlier (offline) is reconciled when the server's creates arrive: the server's version of a legacy row
//! replaces the local one (`SyncCore::on_ops`).

use penwise_core::item::Z_STEP;
use std::collections::BTreeMap;

use penwise_proto::legacy::{
    LegacyStroke, SAMPLE_STRIDE, legacy_add_op, legacy_item_id, legacy_page_id, stroke_to_item_as,
};

use super::SyncCore;
use crate::error::SyncError;

const MAGIC: &[u8; 4] = b"PWST";
const VERSION: u8 = 1;
const TAG_STROKE: u8 = 1;
const MAX_SAMPLES: i32 = 1 << 20;

/// What an import did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Strokes queued as new `Add` ops.
    pub added: u32,
    /// Strokes the replica already had (same deterministic id).
    pub already_present: u32,
    /// Strokes that did not convert (invalid values) and were dropped.
    pub skipped: u32,
    /// Strokes whose seed an earlier stroke of the same file already used: they got the identity
    /// `"<seed>#k"` instead of being merged into it (counted in `added`/`already_present` as well).
    pub seed_collisions: u32,
}

/// Cursor over big-endian `DataInputStream` data; every read fails softly with `None` at the end.
struct Be<'a>(&'a [u8]);

impl Be<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, tail) = self.0.split_first_chunk::<N>()?;
        self.0 = tail;
        Some(*head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take::<1>().map(|b| b[0])
    }

    fn i32(&mut self) -> Option<i32> {
        self.take().map(i32::from_be_bytes)
    }

    fn f32(&mut self) -> Option<f32> {
        self.take().map(f32::from_be_bytes)
    }

    fn i64(&mut self) -> Option<i64> {
        self.take().map(i64::from_be_bytes)
    }
}

/// Parses a `PWST` file; empty for a wrong header, strokes before a torn/corrupt record otherwise.
#[must_use]
pub fn parse_page_file(bytes: &[u8]) -> Vec<LegacyStroke> {
    let mut input = Be(bytes);
    if input.take::<4>().as_ref() != Some(MAGIC) || input.u8() != Some(VERSION) {
        return Vec::new();
    }
    let mut strokes = Vec::new();
    while let Some(stroke) = read_stroke(&mut input) {
        strokes.push(stroke);
    }
    strokes
}

fn read_stroke(input: &mut Be<'_>) -> Option<LegacyStroke> {
    if input.u8()? != TAG_STROKE {
        return None;
    }
    let preset = input.i32()?;
    let argb = input.i32()?;
    let scale = input.f32()?;
    let seed = input.i64()?;
    let count = input.i32()?;
    if !(0..=MAX_SAMPLES).contains(&count) {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    let mut samples = Vec::with_capacity(count.min(4096) * SAMPLE_STRIDE);
    let mut times = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        for _ in 0..SAMPLE_STRIDE {
            samples.push(input.f32()?);
        }
        times.push(input.i32()?);
    }
    Some(LegacyStroke {
        layer: "ink".to_owned(),
        preset: u32::try_from(preset).unwrap_or(u32::MAX),
        argb,
        scale,
        seed: seed.to_string(),
        samples,
        times,
    })
}

impl SyncCore {
    /// Imports `file` (a `PWST` page file) into the page whose legacy id text is `page_text` (the file name
    /// stem). The page must already exist in the replica (`legacy_page_id(page_text)`: the server's migrated
    /// catalog, or [`SyncCore::create_page`] with that id).
    ///
    /// # Errors
    /// `UnknownPage` when the page is missing; replica errors from the first failing op.
    pub fn import_legacy_page(
        &mut self,
        page_text: &str,
        file: &[u8],
        now_ms: u64,
    ) -> Result<ImportReport, SyncError> {
        let page = legacy_page_id(page_text);
        if self.replica.page(page).is_none() {
            return Err(SyncError::UnknownPage);
        }
        let mut report = ImportReport::default();
        let mut z = self.replica.page(page).map_or(0, |p| p.scene().top_z());
        let mut seen: BTreeMap<String, u32> = BTreeMap::new();
        for stroke in parse_page_file(file) {
            let k = seen.entry(stroke.seed.clone()).or_insert(0);
            let row_id = if *k == 0 {
                stroke.seed.clone()
            } else {
                report.seed_collisions += 1;
                format!("{}#{k}", stroke.seed)
            };
            *k += 1;
            let id = legacy_item_id(page_text, &row_id);
            if self
                .replica
                .page(page)
                .is_some_and(|p| p.item(id).is_some())
            {
                report.already_present += 1;
                continue;
            }
            z = z.saturating_add(Z_STEP);
            let Ok(item) = stroke_to_item_as(page_text, &row_id, &stroke, z) else {
                report.skipped += 1;
                continue;
            };
            let hlc = self.stamp(now_ms);
            let op = legacy_add_op(page, self.device, hlc, item);
            self.commit(&op)?;
            report.added += 1;
        }
        Ok(report)
    }
}
