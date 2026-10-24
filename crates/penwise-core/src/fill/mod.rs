//! Fill bucket on a vector scene (PLAN "Fill bucket"; editing-algorithms.md §3).
//!
//! Owner: slice c (all of `fill/`). Contract: `docs/contracts/m2-editing.md`.
//!
//! Pipeline, one file per step: `grid` (fixed page-unit cells, globally snapped windows growing 512² → 4096²
//! cells) → `walls` + `scan` (binary nonzero scanline rasterization of ink walls) → `distance`
//! (Felzenszwalb–Huttenlocher squared EDT) → `flood` (gap closing: flood over non-dilated cells, grow back by
//! `√2·g` inside the walls, tuck 1 cell under ink) → `contour` (marching squares, 4-connected saddles) →
//! `simplify` (iterative RDP) → `clean` (`i_overlay` integer self-union, canonical rings).
//!
//! Determinism: cell size and windows never depend on zoom or device; only `+ − × ÷` and integer maths;
//! every output float is on the 1/16 px grid. The polygon is stored in the item, so replay never re-runs
//! the fill; determinism still matters for tests and tablet/agent agreement.
//!
//! The submodules are slice-internal (`pub(crate)`): slice c may reshape them freely.
pub(crate) mod clean;
pub(crate) mod contour;
pub(crate) mod distance;
pub(crate) mod flood;
pub(crate) mod grid;
pub(crate) mod scan;
pub(crate) mod simplify;
pub(crate) mod walls;

use crate::bounds::Bounds;
use crate::edit::proximity::polygons_contain;
use crate::edit::{Edit, ItemPatch};
use crate::item::{
    Affine, FillItem, Item, ItemGeometry, ItemId, ItemKind, Layer, LayerSet, Paint, Scene,
};
use flood::{FloodInput, FloodOutcome};
use grid::Window;

/// Cell edge, page px (fixed; never zoom-dependent). Tuned at the M4 feel check.
pub const FILL_CELL: f32 = 1.0;
/// Window sides in cells, tried in order until the region does not touch the window border.
pub const WINDOW_SIDES: [u32; 4] = [512, 1024, 2048, 4096];
/// Largest accepted gap tolerance, cells (Krita caps at 32 px; editing-algorithms.md §3.4).
pub const MAX_GAP_CELLS: u32 = 16;

/// A fill-bucket tap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillRequest {
    /// Tap point, page px.
    pub tap: [f32; 2],
    /// Gaps narrower than this (page px) are closed; converted to `g = round(gap / FILL_CELL)` cells and
    /// clamped to `0..=MAX_GAP_CELLS`.
    pub gap: f32,
    /// Largest window side to try, cells (rounded down to an entry of [`WINDOW_SIDES`], at least 512).
    pub max_window: u32,
    /// Paint of the new fill (or the recolor target).
    pub paint: Paint,
}

/// Why no fill was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FillError {
    /// The region still touches the border of the largest window (open area on an infinite page).
    #[error("region is not closed")]
    OpenRegion,
    /// The tap lies on a wall (inside a line, or within the gap tolerance of one with no free cell nearby).
    #[error("tap is on a line")]
    TapOnInk,
    /// The tap lies outside a fixed page.
    #[error("tap is outside the page")]
    OutsidePage,
    /// The traced region is empty after cleanup (degenerate area).
    #[error("region is empty")]
    EmptyRegion,
}

/// Reusable buffers (caller-owned; one per page or tool) so repeated fills do not reallocate 16 M-cell
/// grids. Contents between calls are unspecified.
#[derive(Debug, Clone, Default)]
pub struct FillScratch {
    /// Wall bits, one per cell (row-major, 64 cells per word).
    pub(crate) walls: Vec<u64>,
    /// Region bits.
    pub(crate) region: Vec<u64>,
    /// Squared distance to the nearest wall in cells², saturating at `u16::MAX` (only `≤ g²` matters).
    pub(crate) dist: Vec<u16>,
    /// Flood stack of `(x, y)` cell seeds.
    pub(crate) stack: Vec<[u32; 2]>,
}

/// Traces the closed region around `req.tap` into a fill (page-px polygons on the 1/16 px grid, identity
/// transform implied). Walls: live ink-layer strokes (visible outline polygons; stamp strokes as capsule
/// chains of their dab radii where `opacity · hardness ≥ 0.25`) and shape strokes, each at least one cell
/// wide, minus their masks; shape fills, existing fills, highlighter (`BlendMode::Multiply`) strokes, agent
/// and text layers are not walls. Fixed pages: the window is clipped to the page and the page edge is a wall.
///
/// # Errors
/// [`FillError`] as documented per variant.
pub fn flood_fill(
    scene: &Scene,
    req: &FillRequest,
    scratch: &mut FillScratch,
) -> Result<FillItem, FillError> {
    let gap = gap_cells(req.gap);
    let cap = req.max_window.max(WINDOW_SIDES[0]);
    for side in WINDOW_SIDES.into_iter().filter(|&s| s <= cap) {
        let window = Window::around(req.tap, side, scene.kind()).ok_or(FillError::OutsidePage)?;
        let seed = window.cell_of(req.tap).ok_or(FillError::OutsidePage)?;
        walls::rasterize_walls(scene, &window, &mut scratch.walls);
        let outcome = flood::close_and_flood(
            &window,
            FloodInput { seed, gap },
            &scratch.walls,
            &mut scratch.dist,
            &mut scratch.stack,
            &mut scratch.region,
        );
        match outcome {
            FloodOutcome::Closed => return trace_fill(&window, &scratch.region, req.paint),
            FloodOutcome::SeedOnWall => return Err(FillError::TapOnInk),
            FloodOutcome::Leaked => {}
        }
    }
    Err(FillError::OpenRegion)
}

/// `g = round(gap / FILL_CELL)` clamped to `0..=MAX_GAP_CELLS` (NaN and negatives → 0).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "small integers; rounded and clamped to 0..=MAX_GAP_CELLS first"
)]
fn gap_cells(gap: f32) -> u32 {
    let g = (gap / FILL_CELL).round();
    if g.is_nan() || g <= 0.0 {
        0
    } else if g >= MAX_GAP_CELLS as f32 {
        MAX_GAP_CELLS
    } else {
        g as u32
    }
}

/// Region mask → rings (marching squares) → RDP → `i_overlay` cleanup.
fn trace_fill(window: &Window, region: &[u64], paint: Paint) -> Result<FillItem, FillError> {
    let mut rings = contour::trace_rings(window, region);
    let epsilon = simplify::RDP_EPSILON_CELLS * FILL_CELL;
    for ring in &mut rings {
        simplify::simplify_ring(ring, epsilon);
    }
    let polygons = clean::normalize(&rings);
    if polygons.is_empty() {
        return Err(FillError::EmptyRegion);
    }
    Ok(FillItem { polygons, paint })
}

/// The fill tool's tap: when `req.tap` lies inside a live fill on the ink layer (topmost by render order),
/// `Update { paint }` recolors it (empty `Vec` if the paint is unchanged); otherwise [`flood_fill`] and
/// `Add(Item { id, layer: Ink, z: scene.next_z(), transform: IDENTITY, kind: Fill })`.
///
/// # Errors
/// The [`flood_fill`] error.
pub fn fill_at(
    scene: &Scene,
    req: &FillRequest,
    id: ItemId,
    scratch: &mut FillScratch,
) -> Result<Vec<Edit>, FillError> {
    if let Some((target, paint)) = fill_under(scene, req.tap) {
        if paint == req.paint {
            return Ok(Vec::new());
        }
        let patch = ItemPatch {
            paint: Some(req.paint),
            ..ItemPatch::default()
        };
        return Ok(vec![Edit::Update { id: target, patch }]);
    }
    let fill = flood_fill(scene, req, scratch)?;
    Ok(vec![Edit::Add(Item {
        id,
        layer: Layer::Ink,
        z: scene.next_z(),
        transform: Affine::IDENTITY,
        kind: ItemKind::Fill(fill),
    })])
}

/// Topmost live ink-layer fill containing `tap` (query order is `(z, id)` ascending and every fill is in the
/// same band), with its paint.
fn fill_under(scene: &Scene, tap: [f32; 2]) -> Option<(ItemId, Paint)> {
    if !(tap[0].is_finite() && tap[1].is_finite()) {
        return None;
    }
    let ids = scene.query(&Bounds::from_point(tap[0], tap[1]), LayerSet::INK);
    ids.into_iter()
        .rev()
        .find_map(|id| match scene.geometry(id) {
            Some(ItemGeometry::Fill { polygons, paint }) if polygons_contain(polygons, tap) => {
                Some((id, *paint))
            }
            _ => None,
        })
}

#[cfg(test)]
mod tests;
