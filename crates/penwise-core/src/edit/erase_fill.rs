//! Precise erase of fills: polygon difference, the fill stays vector (decided; editing-algorithms.md §2).
//!
//! Owner: slice b.
//!
//! Booleans use `i_overlay`'s **integer** API only (`i_overlay::core`, `OverlayRule::Difference`,
//! `FillRule::NonZero`) on coordinates snapped to the 1/16 px grid (`i32 = round(v · 16)`), so the result is
//! bit-identical everywhere. The eraser buffer is built here, not with `i_overlay`'s float stroke API (which
//! uses std trig): each eraser point becomes a [`CAPSULE_SEGMENTS`]-gon from a constant unit-circle table and
//! each segment a quad joining the two discs' tangent points.

use super::Edit;
use crate::item::{Item, Polygon};

/// Vertices of the regular polygon approximating each eraser disc (circumscribed: vertex radius
/// `r / cos(π / N)`, so the polygon covers the disc).
pub const CAPSULE_SEGMENTS: u32 = 32;

/// Fills with less remaining area (page px²) are deleted instead of updated.
pub const MIN_FILL_AREA: f32 = 1.0;

/// `polygons − buffer(path, radius)`, all page px. Output polygons are canonical (`i_overlay` orientation,
/// see [`Polygon`]), sorted by the `(y, x)` of each outer ring's minimum vertex, holes sorted the same way,
/// rings rotated to start at their minimum `(y, x)` vertex. `None` when the buffer does not intersect the
/// polygons (fill untouched).
#[must_use]
pub fn cut_polygons(polygons: &[Polygon], path: &[[f32; 2]], radius: f32) -> Option<Vec<Polygon>> {
    todo!()
}

/// Edits for one live fill item `item` (with page polygons `page_polygons` from its cached geometry):
/// - untouched → empty;
/// - remaining area < [`MIN_FILL_AREA`] → `Delete(item.id)`;
/// - otherwise one `Update { polygons: Some(cut in page space), transform: Some(Affine::IDENTITY) }` (the cut
///   is done in page space so non-uniform transforms stay exact; resetting the transform keeps local = page).
///
/// Known limitation (LWW): two concurrent cuts of one fill keep only the later `Polygons` value.
#[must_use]
pub fn fill_cut_edits(
    item: &Item,
    page_polygons: &[Polygon],
    path: &[[f32; 2]],
    radius: f32,
) -> Vec<Edit> {
    todo!()
}
