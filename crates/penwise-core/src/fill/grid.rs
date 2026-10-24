//! Fill windows on the global cell grid and bit-grid helpers.
//!
//! Owner: slice c (slice-internal).

use crate::item::PageKind;

/// A rectangle of cells on the global grid: cell `(i, j)` covers page px
/// `[(origin[0] + i)·FILL_CELL, (origin[0] + i + 1)·FILL_CELL) × …`; samples are taken at cell centres.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Window {
    /// Global cell coordinates of cell `(0, 0)`.
    pub(crate) origin: [i64; 2],
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// True when an edge of the window is the fixed page's edge (a wall, not "open").
    pub(crate) page_edges: [bool; 4],
}

impl Window {
    /// Square window of `side` cells centred on the tap's cell: `origin = floor(tap / FILL_CELL) − side / 2`
    /// (globally snapped, so every device gets the same cells). Fixed pages clip it to the page's cells and
    /// mark clipped edges in `page_edges`. `None` when the tap is outside a fixed page.
    pub(crate) fn around(tap: [f32; 2], side: u32, page: PageKind) -> Option<Self> {
        todo!()
    }

    /// Page-px centre of cell `(i, j)`.
    pub(crate) fn cell_centre(&self, i: u32, j: u32) -> [f32; 2] {
        todo!()
    }

    /// Cell containing page point `p`, if inside the window.
    pub(crate) fn cell_of(&self, p: [f32; 2]) -> Option<[u32; 2]> {
        todo!()
    }
}

/// Reads bit `(x, y)` of a row-major bit grid `width` cells wide (64 cells per word).
pub(crate) fn get_bit(bits: &[u64], width: u32, x: u32, y: u32) -> bool {
    todo!()
}

/// Sets bit `(x, y)`.
pub(crate) fn set_bit(bits: &mut [u64], width: u32, x: u32, y: u32) {
    todo!()
}
