//! Fill bucket on a vector scene (PLAN "Fill bucket"; editing-algorithms.md §3).
//!
//! Owner: slice c (all of `fill/`). Contract: `docs/contracts/m2-editing.md`.
//!
//! Pipeline, one file per step: `grid` (fixed page-unit cells, globally snapped windows growing 512² → 4096²
//! cells) → `walls` + `scan` (binary nonzero scanline rasterization of ink walls) → `distance`
//! (Felzenszwalb–Huttenlocher squared EDT) → `flood` (gap closing: flood over non-dilated cells, grow back by
//! `g`, tuck 1 cell under ink) → `contour` (marching squares, 4-connected saddles) → `simplify` (iterative
//! RDP) → `clean` (`i_overlay` integer self-union, canonical rings).
//!
//! Determinism: cell size and windows never depend on zoom or device; only `+ − × ÷` and integer maths;
//! every output float is on the 1/16 px grid. The polygon is stored in the item, so replay never re-runs
//! the fill; determinism still matters for tests and tablet/agent agreement.
//!
//! The submodules are slice-internal (`pub(crate)`): slice c may reshape them freely.
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::needless_pass_by_value,
    unused_variables,
    dead_code,
    reason = "M2 contract stubs; slices remove this"
)]

pub(crate) mod clean;
pub(crate) mod contour;
pub(crate) mod distance;
pub(crate) mod flood;
pub(crate) mod grid;
pub(crate) mod scan;
pub(crate) mod simplify;
pub(crate) mod walls;

use crate::edit::Edit;
use crate::item::{FillItem, ItemId, Paint, Scene};

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
    todo!()
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
    todo!()
}
