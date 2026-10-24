//! Marching squares on the region mask (editing-algorithms.md §3.6).
//!
//! Owner: slice c (slice-internal).

use super::grid::Window;

/// Closed rings (page px, cell-corner vertices) bounding the set cells of `region`. Saddle cases always
/// separate diagonal cells (consistent with the 4-connected flood). Rings are emitted in scan order of their
/// first boundary edge, each starting at its minimum `(y, x)` vertex, collinear vertices removed. Outer rings
/// have positive shoelace sums, holes negative ([`crate::item::Polygon`] convention).
pub(crate) fn trace_rings(window: &Window, region: &[u64]) -> Vec<Vec<[f32; 2]>> {
    todo!()
}
