//! Brush sheet: the same strokes rendered once per preset, one cell per preset in `Preset::ALL` order
//! (ballpoint … shading), laid out row-major on a white page with 1 px gray gutters. Shared by
//! `examples/brush_sheet.rs` and the `raster_brush_sheet` PNG golden.
//!
//! Owner: slice e.
//!
//! Every cell renders the same page rect (stroke bounds + margin) at one scale, is optionally turned a quarter
//! turn (handwriting recorded with the tablet sideways), then cropped to its own ink so rows stay readable.

mod pixels;

use super::stamps::CoverageBuffer;
use super::{RasterError, Viewport, render_strokes};
use crate::bounds::Bounds;
use crate::brush::Preset;
use crate::builder::StrokeBuilder;
use crate::codec::CodecError;
use crate::codec::pwrec::{Recording, RunKind};
use crate::float::min;
use crate::geometry::StrokeGeometry;
use crate::sample::Sample;
use pixels::{PixelRect, WHITE};
use tiny_skia::Pixmap;

/// Largest uncropped cell side, pixels.
const MAX_CELL_PX: u32 = 8192;
/// Largest sheet side, pixels (a 13-row sheet of long handwriting runs to ~30 000 px tall).
const MAX_SHEET_PX: u32 = 32_768;
/// Gutter color between cells (premultiplied, opaque light gray).
const GUTTER: [u8; 4] = [200, 200, 205, 255];

/// Quarter turn applied to every cell after rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Rotation {
    #[default]
    None,
    /// Clockwise: the page's left edge becomes the top.
    Cw,
    /// Counter-clockwise: the page's right edge becomes the top.
    Ccw,
}

/// Sheet layout knobs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetOptions {
    pub rotation: Rotation,
    /// Cells per row (`≥ 1`; clamped to the preset count).
    pub columns: u32,
    /// Sheets whose uncropped cells would be wider than this are scaled down (never up).
    pub max_width_px: u32,
    /// Page px kept around the stroke bounds before cropping (fits the largest brush radius).
    pub margin: f32,
    /// White pixels kept around each cell's ink after cropping.
    pub padding_px: u32,
}

impl SheetOptions {
    /// One column, no rotation, ≤ 1600 px wide, 60 page px margin, 8 px padding.
    pub const DEFAULT: Self = Self {
        rotation: Rotation::None,
        columns: 1,
        max_width_px: 1600,
        margin: 60.0,
        padding_px: 8,
    };
}

impl Default for SheetOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Quantized samples of every non-empty contact run of `recording` (hover runs skipped), in recording order.
///
/// # Errors
/// Errors from [`Recording::digitizer_ranges`].
pub fn contact_strokes(recording: &Recording) -> Result<Vec<Vec<Sample>>, CodecError> {
    let ranges = recording.digitizer_ranges()?;
    let mut strokes = Vec::new();
    for run in recording
        .strokes
        .iter()
        .filter(|r| r.kind == RunKind::Contact)
    {
        let mut samples = Vec::new();
        run.to_samples(&ranges, &mut samples);
        if !samples.is_empty() {
            strokes.push(samples);
        }
    }
    Ok(strokes)
}

/// Renders `strokes` (stroke `i` built with seed `i`) once per preset in `Preset::ALL` order into a sheet
/// (see module docs). A cell whose preset draws nothing stays a blank `2·padding` square.
///
/// # Errors
/// [`RasterError::InvalidSize`] when there are no samples (`0x0`), an uncropped cell would exceed 8192 px per
/// side or the sheet 32 768 px per side; [`RasterError::InvalidViewport`] for a non-finite derived scale.
pub fn render_brush_sheet(
    strokes: &[Vec<Sample>],
    options: &SheetOptions,
) -> Result<Pixmap, RasterError> {
    let cells = render_cells(strokes, options)?;
    let columns = options.columns.clamp(1, preset_count()) as usize;
    let mut col_w = vec![0u32; columns];
    let mut row_h = vec![0u32; cells.len().div_ceil(columns)];
    for (i, cell) in cells.iter().enumerate() {
        if let Some(w) = col_w.get_mut(i % columns) {
            *w = (*w).max(cell.width());
        }
        if let Some(h) = row_h.get_mut(i / columns) {
            *h = (*h).max(cell.height());
        }
    }
    let (width, height) = (track_total(&col_w), track_total(&row_h));
    if width > MAX_SHEET_PX || height > MAX_SHEET_PX {
        return Err(RasterError::InvalidSize { width, height });
    }
    let mut sheet = pixels::filled(width, height, WHITE)?;
    let col_x = track_starts(&col_w);
    let row_y = track_starts(&row_h);
    for &x in col_x.iter().skip(1) {
        pixels::fill(&mut sheet, rect(x.saturating_sub(1), 0, 1, height), GUTTER);
    }
    for &y in row_y.iter().skip(1) {
        pixels::fill(&mut sheet, rect(0, y.saturating_sub(1), width, 1), GUTTER);
    }
    for (i, cell) in cells.iter().enumerate() {
        if let (Some(&x), Some(&y)) = (col_x.get(i % columns), row_y.get(i / columns)) {
            pixels::blit(&mut sheet, cell, x, y);
        }
    }
    Ok(sheet)
}

/// One cropped (and turned) cell per preset.
fn render_cells(
    strokes: &[Vec<Sample>],
    options: &SheetOptions,
) -> Result<Vec<Pixmap>, RasterError> {
    let bounds = strokes.iter().flatten().fold(Bounds::EMPTY, |mut b, s| {
        b.include_point(s.x, s.y);
        b
    });
    if bounds.is_empty() {
        return Err(RasterError::InvalidSize {
            width: 0,
            height: 0,
        });
    }
    let (viewport, width, height) = cell_frame(&bounds, options)?;
    let mut scratch = CoverageBuffer::default();
    let mut cells = Vec::with_capacity(Preset::ALL.len());
    for preset in Preset::ALL {
        let params = preset.params();
        let geometry: Vec<StrokeGeometry> = (0u64..)
            .zip(strokes)
            .map(|(seed, samples)| StrokeBuilder::build(&params, seed, samples))
            .collect();
        let mut page = pixels::filled(width, height, WHITE)?;
        render_strokes(&geometry, &viewport, &mut page, &mut scratch)?;
        let turned = pixels::rotate(&page, options.rotation)?;
        let pad = options.padding_px;
        cells.push(match pixels::content_rect(&turned, WHITE, pad) {
            Some(ink) => pixels::crop(&turned, ink)?,
            None => pixels::filled(2 * pad.max(1), 2 * pad.max(1), WHITE)?,
        });
    }
    Ok(cells)
}

/// Viewport and uncropped pixel size of a cell: page rect = bounds + margin, scaled so `columns` turned cells
/// fit `max_width_px`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "sizes are checked finite, positive and ≤ 8192 before the casts; inputs are far below 2^24"
)]
fn cell_frame(
    bounds: &Bounds,
    options: &SheetOptions,
) -> Result<(Viewport, u32, u32), RasterError> {
    let margin = options.margin;
    let page_w = bounds.width() + 2.0 * margin;
    let page_h = bounds.height() + 2.0 * margin;
    let turned_w = if options.rotation == Rotation::None {
        page_w
    } else {
        page_h
    };
    let columns = options.columns.clamp(1, preset_count()) as f32;
    let scale = min(1.0, options.max_width_px as f32 / (turned_w * columns));
    let viewport = Viewport {
        origin_x: bounds.min_x - margin,
        origin_y: bounds.min_y - margin,
        scale,
    };
    viewport.check()?;
    let (w, h) = ((page_w * scale).ceil(), (page_h * scale).ceil());
    let side = MAX_CELL_PX as f32;
    if !(w >= 1.0 && h >= 1.0 && w <= side && h <= side) {
        return Err(RasterError::InvalidSize {
            width: if w.is_finite() { w as u32 } else { u32::MAX },
            height: if h.is_finite() { h as u32 } else { u32::MAX },
        });
    }
    Ok((viewport, w as u32, h as u32))
}

fn preset_count() -> u32 {
    u32::try_from(Preset::ALL.len()).unwrap_or(u32::MAX)
}

/// Sum of track sizes plus one gutter pixel between tracks (saturating).
fn track_total(tracks: &[u32]) -> u32 {
    let gutters = u32::try_from(tracks.len().saturating_sub(1)).unwrap_or(u32::MAX);
    tracks.iter().fold(gutters, |acc, &t| acc.saturating_add(t))
}

/// Start offset of each track (one gutter pixel after the previous track).
fn track_starts(tracks: &[u32]) -> Vec<u32> {
    let mut at = 0u32;
    tracks
        .iter()
        .map(|&t| {
            let start = at;
            at = at.saturating_add(t).saturating_add(1);
            start
        })
        .collect()
}

const fn rect(x: u32, y: u32, width: u32, height: u32) -> PixelRect {
    PixelRect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests;
