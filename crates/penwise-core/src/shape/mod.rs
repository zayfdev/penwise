//! Shape geometry and handle math (PLAN "Brushes & tools": shapes; editing-algorithms.md §5).
//!
//! Owner: slice d (this file, `path.rs`, `outline.rs`, `handles.rs`, `resize.rs`). Contract:
//! `docs/contracts/m2-editing.md`.
//!
//! Pipeline: [`ShapeItem`] (local box) → `path` (local construction, Béziers mapped through the item transform
//! — exact for Béziers — then flattened in page space) → [`ShapePath`] → `outline` (stroked in page space with
//! the shape brush at its own width) → [`ShapeGeometry`]. No trig at render time: ellipses use the Bézier
//! constant, arrowheads a precomputed rotation, regular polygons are generated once at creation.
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::needless_pass_by_value,
    unused_variables,
    dead_code,
    reason = "M2 contract stubs; slices remove this"
)]

pub mod handles;
pub mod outline;
pub mod path;
pub mod resize;

pub use handles::{Handle, HandleFrame, HandleHit};

use crate::brush::Color;
use crate::geometry::{Outline, StrokeGeometry};
use crate::item::{Affine, ShapeItem, ShapeKind};

/// Maximum distance (page px) between a flattened polyline and its Bézier.
pub const FLATTEN_TOLERANCE: f32 = 0.25;

/// Flattened page-space path of a shape. Run `k` is `points[run_ends[k-1] .. run_ends[k]]`. Closed kinds have
/// one closed run (rect, rounded rect, ellipse, polygon); a line has one open run; an arrow has the shaft run
/// followed by one open run per head (`left barb → tip → right barb`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapePath {
    pub points: Vec<[f32; 2]>,
    pub run_ends: Vec<u32>,
    /// True when every run is implicitly closed (closed kinds); false for line and arrow.
    pub closed: bool,
}

/// Render geometry of a shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeGeometry {
    /// Page path (hit test, lasso, erasers, fill walls).
    pub path: ShapePath,
    /// Outline stroked along `path` with the shape's brush ([`outline::stroke_path`]).
    pub stroke: StrokeGeometry,
    /// Interior of closed filled shapes: the closed run as one nonzero contour, with its color. `None` for
    /// unfilled or linear shapes. Drawn before `stroke`.
    pub fill: Option<(Color, Outline)>,
}

/// Full geometry of `shape` under `transform`: [`shape_path`] → [`outline::stroke_path`] → optional fill.
/// Zero-size boxes give degenerate (but finite) paths: a zero-area rect strokes as a line, a fully zero
/// shape as a dot.
#[must_use]
pub fn shape_geometry(shape: &ShapeItem, transform: &Affine) -> ShapeGeometry {
    todo!()
}

/// Flattened page-space path for `shape` under `transform` (dispatches to [`path`] builders).
#[must_use]
pub fn shape_path(shape: &ShapeItem, transform: &Affine) -> ShapePath {
    todo!()
}

/// `ShapeKind::Polygon` for a regular `n`-gon (`n` clamped to `3..=64`) inscribed in the unit box
/// `[−0.5, 0.5]²`, first vertex at the top (`(0, −0.5)`), clockwise on screen; vertices from
/// `libm::sincosf(2π·k/n)` (once at creation; afterwards they are data).
#[must_use]
pub fn regular_polygon(n: u32) -> ShapeKind {
    todo!()
}
