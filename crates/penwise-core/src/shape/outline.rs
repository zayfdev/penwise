//! Strokes a [`ShapePath`] with an outline brush in page space (stroke width applied after transform).
//!
//! Owner: slice d.
//!
//! The M1 builder has no closed mode, so closed runs are emulated with M1's public API only: synthetic
//! [`crate::sample::Sample`]s at constant pressure 1, tilt 0, azimuth 0, timestamps every 2 ms, resampled at
//! `max(1, size_max / 4)` px along the run; the run is closed by repeating the first two samples at the end,
//! and the brush is copied with tapers 0 and round caps so the overlap disappears under the nonzero fill.
//! Each run builds with `StrokeBuilder::build(params, seed = 0, samples)`; run contours are concatenated into
//! one [`Outline`] (nonzero, so overlaps never double-cover).

use super::ShapePath;
use crate::brush::BrushParams;
use crate::geometry::{Outline, StrokeGeometry};

/// Synthetic sample timestep, µs.
pub const SYNTHETIC_DT_US: u32 = 2_000;

/// Outline geometry of `path` with `brush` (outline family; a stamp-family brush yields empty geometry with
/// its style). Velocity dynamics see a constant speed of `step / 2 ms`; smoothing is disabled
/// (`SmoothingParams` copied with `beta = 0` and a cutoff high enough to be a pass-through) so corners stay
/// sharp at the brush's own width.
#[must_use]
pub fn stroke_path(path: &ShapePath, brush: &BrushParams) -> StrokeGeometry {
    todo!()
}

/// Interior of the first closed run of `path` as one nonzero contour (`None` for open paths or < 3 points).
#[must_use]
pub fn fill_outline(path: &ShapePath) -> Option<Outline> {
    todo!()
}
