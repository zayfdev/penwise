//! Fill windows on the global cell grid and bit-grid helpers.
//!
//! Owner: slice c (slice-internal).
//!
//! Bit grids are row-major over the window's cells with a flat index `y · width + x`, 64 cells per word
//! (rows are not word-aligned).

use super::FILL_CELL;
use crate::bounds::Bounds;
use crate::item::PageKind;

/// Largest accepted tap coordinate magnitude, page px (the sample grid's `2²⁴` limit). Keeps cell arithmetic
/// far from `i64` overflow.
const MAX_TAP: f32 = 16_777_216.0;

/// Edge indices into [`Window::page_edges`].
pub(crate) const LEFT: usize = 0;
pub(crate) const TOP: usize = 1;
pub(crate) const RIGHT: usize = 2;
pub(crate) const BOTTOM: usize = 3;

/// A rectangle of cells on the global grid: cell `(i, j)` covers page px
/// `[(origin[0] + i)·FILL_CELL, (origin[0] + i + 1)·FILL_CELL) × …`; samples are taken at cell centres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    /// Global cell coordinates of cell `(0, 0)`.
    pub(crate) origin: [i64; 2],
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// True when an edge of the window is the fixed page's edge (a wall, not "open"), indexed by
    /// [`LEFT`], [`TOP`], [`RIGHT`], [`BOTTOM`].
    pub(crate) page_edges: [bool; 4],
}

impl Window {
    /// Square window of `side` cells centred on the tap's cell: `origin = floor(tap / FILL_CELL) − side / 2`
    /// (globally snapped, so every device gets the same cells). Fixed pages clip it to the page's cells and
    /// mark clipped edges in `page_edges`. `None` when the tap is outside a fixed page (or not finite, or
    /// beyond `2²⁴` px).
    pub(crate) fn around(tap: [f32; 2], side: u32, page: PageKind) -> Option<Self> {
        if !tap.iter().all(|v| v.is_finite() && v.abs() <= MAX_TAP) {
            return None;
        }
        let half = i64::from(side / 2);
        let mut lo = [cell_index(tap[0]) - half, cell_index(tap[1]) - half];
        let mut hi = [lo[0] + i64::from(side), lo[1] + i64::from(side)];
        let mut page_edges = [false; 4];
        if let PageKind::Fixed { width, height } = page {
            for (k, &extent) in [width, height].iter().enumerate() {
                if !extent.is_finite() || extent <= 0.0 || tap[k] < 0.0 || tap[k] > extent {
                    return None;
                }
                let cells = page_cells(extent);
                if lo[k] <= 0 {
                    lo[k] = 0;
                    page_edges[k] = true;
                }
                if hi[k] >= cells {
                    hi[k] = cells;
                    page_edges[k + 2] = true;
                }
            }
        }
        Some(Self {
            origin: lo,
            width: u32::try_from(hi[0] - lo[0]).ok().filter(|&w| w > 0)?,
            height: u32::try_from(hi[1] - lo[1]).ok().filter(|&h| h > 0)?,
            page_edges,
        })
    }

    /// Page-px position of cell corner `(x, y)` (`0..=width`, `0..=height`; contour vertices).
    pub(crate) fn corner(&self, x: i64, y: i64) -> [f32; 2] {
        [to_page(self.origin[0], x), to_page(self.origin[1], y)]
    }

    /// Cell containing page point `p`, if inside the window.
    pub(crate) fn cell_of(&self, p: [f32; 2]) -> Option<[u32; 2]> {
        if !p.iter().all(|v| v.is_finite() && v.abs() <= MAX_TAP) {
            return None;
        }
        let x = u32::try_from(cell_index(p[0]) - self.origin[0]).ok()?;
        let y = u32::try_from(cell_index(p[1]) - self.origin[1]).ok()?;
        (x < self.width && y < self.height).then_some([x, y])
    }

    /// Page-px rectangle covered by the window's cells.
    pub(crate) fn page_bounds(&self) -> Bounds {
        let lo = self.corner(0, 0);
        let hi = self.corner(i64::from(self.width), i64::from(self.height));
        Bounds {
            min_x: lo[0],
            min_y: lo[1],
            max_x: hi[0],
            max_y: hi[1],
        }
    }
}

/// Global cell index of page coordinate `v` (`|v| ≤ 2²⁴`, so the cast is exact).
#[allow(
    clippy::cast_possible_truncation,
    reason = "callers bound |v| ≤ 2^24 px, far inside i64"
)]
fn cell_index(v: f32) -> i64 {
    (f64::from(v) / f64::from(FILL_CELL)).floor() as i64
}

/// Cells needed to cover a fixed page extent (at least one).
#[allow(
    clippy::cast_possible_truncation,
    reason = "page extents are finite page-px values far inside i64"
)]
fn page_cells(extent: f32) -> i64 {
    let cells = (f64::from(extent) / f64::from(FILL_CELL)).ceil() as i64;
    if cells < 1 { 1 } else { cells }
}

/// Point `p` in global cell units (`p / FILL_CELL`; cell `g`'s centre is `g + 0.5`), in `f64`. The same for
/// every window, so rasterized walls never depend on the window's placement.
pub(crate) fn global(p: [f32; 2]) -> [f64; 2] {
    let cell = f64::from(FILL_CELL);
    [f64::from(p[0]) / cell, f64::from(p[1]) / cell]
}

/// Page px of global cell coordinate `origin + i`, computed in `f64` and rounded once.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "cell coordinates are bounded by the 2^24 px tap limit; one rounding to f32"
)]
fn to_page(origin: i64, i: i64) -> f32 {
    ((origin + i) as f64 * f64::from(FILL_CELL)) as f32
}

/// Words needed for a `width × height` bit grid.
pub(crate) fn words_for(width: u32, height: u32) -> usize {
    (width as usize * height as usize).div_ceil(64)
}

/// Clears `bits` and sizes it for a `width × height` grid (all zero).
pub(crate) fn reset(bits: &mut Vec<u64>, width: u32, height: u32) {
    bits.clear();
    bits.resize(words_for(width, height), 0);
}

/// Flat cell index of `(x, y)`.
pub(crate) fn index(width: u32, x: u32, y: u32) -> usize {
    y as usize * width as usize + x as usize
}

/// Reads bit `(x, y)` of a row-major bit grid `width` cells wide (64 cells per word).
pub(crate) fn get_bit(bits: &[u64], width: u32, x: u32, y: u32) -> bool {
    get_index(bits, index(width, x, y))
}

/// Reads the bit at flat index `i` (`false` past the end).
pub(crate) fn get_index(bits: &[u64], i: usize) -> bool {
    bits.get(i >> 6).is_some_and(|w| (w >> (i & 63)) & 1 == 1)
}

/// Sets bit `(x, y)`.
pub(crate) fn set_bit(bits: &mut [u64], width: u32, x: u32, y: u32) {
    let i = index(width, x, y);
    if let Some(w) = bits.get_mut(i >> 6) {
        *w |= 1 << (i & 63);
    }
}

/// Sets cells `x0..x1` of row `y`.
pub(crate) fn set_run(bits: &mut [u64], width: u32, y: u32, x0: u32, x1: u32) {
    apply_run(bits, index(width, x0, y), index(width, x1, y), true);
}

/// Clears cells `x0..x1` of row `y`.
pub(crate) fn clear_run(bits: &mut [u64], width: u32, y: u32, x0: u32, x1: u32) {
    apply_run(bits, index(width, x0, y), index(width, x1, y), false);
}

/// Sets or clears flat indices `start..end`, a word at a time.
fn apply_run(bits: &mut [u64], start: usize, end: usize, set: bool) {
    let mut i = start;
    while i < end {
        let bit = i & 63;
        let n = (64 - bit).min(end - i);
        let mask = if n == 64 {
            u64::MAX
        } else {
            ((1u64 << n) - 1) << bit
        };
        if let Some(w) = bits.get_mut(i >> 6) {
            if set {
                *w |= mask;
            } else {
                *w &= !mask;
            }
        }
        i += n;
    }
}
