//! Polygon validity and canonical form via `i_overlay` (editing-algorithms.md §3.8).
//!
//! Owner: slice c (slice-internal).
//!
//! Integer API only: points snapped to the 1/16 px grid (`i32 = round(v · 16)`) before `i_overlay`
//! (`OverlayRule::Union`/self-union, `FillRule::NonZero`), converted back by `/ 16`. No float overlay API.

use crate::item::Polygon;

/// Canonical, valid polygons from possibly self-crossing rings (outer and hole rings mixed, any orientation;
/// classified by the nonzero union itself). Output: outer rings positive shoelace, holes negative, each ring
/// rotated to start at its minimum `(y, x)` vertex, holes sorted by that vertex, polygons sorted by their
/// outer ring's minimum vertex; rings with < 3 points or zero area dropped.
pub(crate) fn normalize(rings: &[Vec<[f32; 2]>]) -> Vec<Polygon> {
    todo!()
}
