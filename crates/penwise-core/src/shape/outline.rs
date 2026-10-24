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
//!
//! Resampling subdivides every path segment separately (each into `ceil(length / step)` equal pieces), so
//! path vertices are always samples and corners stay sharp.

use super::ShapePath;
use crate::brush::{BrushFamily, BrushParams, CapStyle};
use crate::builder::StrokeBuilder;
use crate::float::max;
use crate::geometry::{Outline, StrokeGeometry, StrokeStyle};
use crate::sample::Sample;
use crate::smoothing::OneEuroParams;

/// Synthetic sample timestep, µs.
pub const SYNTHETIC_DT_US: u32 = 2_000;

/// One Euro cutoff high enough that the filter weight rounds to exactly 1 in `f32` (pass-through).
const PASS_THROUGH_CUTOFF_HZ: f32 = 1.0e12;

/// Upper bound on resampling pieces per path segment (bounds work for absurdly large shapes).
const MAX_PIECES_PER_SEGMENT: f32 = 65_536.0;

/// Outline geometry of `path` with `brush` (outline family; a stamp-family brush yields empty geometry with
/// its style). Velocity dynamics see a constant speed of `step / 2 ms`; smoothing is disabled
/// (`SmoothingParams` copied with `beta = 0` and a cutoff high enough to be a pass-through) so corners stay
/// sharp at the brush's own width.
#[must_use]
pub fn stroke_path(path: &ShapePath, brush: &BrushParams) -> StrokeGeometry {
    let style = StrokeStyle {
        color: brush.color,
        opacity: brush.opacity,
        blend: brush.blend,
    };
    if matches!(brush.family, BrushFamily::Stamp(_)) {
        return StrokeBuilder::build(brush, 0, &[]);
    }
    let params = closed_stroke_params(brush);
    let step = max(1.0, brush.size_max / 4.0);
    let mut outline = Outline::default();
    let mut positions = Vec::new();
    let mut samples = Vec::new();
    for run in runs(path) {
        run_positions(run, path.closed, step, &mut positions);
        if positions.is_empty() {
            continue;
        }
        samples.clear();
        samples.extend(positions.iter().zip(0u32..).map(|(&p, i)| synthetic(p, i)));
        if let StrokeGeometry::Outline { outline: piece, .. } =
            StrokeBuilder::build(&params, 0, &samples)
        {
            append_outline(&mut outline, &piece);
        }
    }
    StrokeGeometry::Outline { style, outline }
}

/// Interior of the first closed run of `path` as one nonzero contour (`None` for open paths or < 3 points).
#[must_use]
pub fn fill_outline(path: &ShapePath) -> Option<Outline> {
    if !path.closed {
        return None;
    }
    let ring = runs(path).next()?;
    if ring.len() < 3 {
        return None;
    }
    Some(Outline {
        points: ring.to_vec(),
        contour_ends: vec![u32::try_from(ring.len()).ok()?],
    })
}

/// The runs of `path` (see [`ShapePath`]); malformed run ends are clamped to the point count.
fn runs(path: &ShapePath) -> impl Iterator<Item = &[[f32; 2]]> {
    let len = path.points.len();
    let mut start = 0usize;
    path.run_ends.iter().map(move |&end| {
        let end = usize::try_from(end).unwrap_or(len).clamp(start, len);
        let run = &path.points[start..end];
        start = end;
        run
    })
}

/// The brush with tapers 0, round caps and pass-through smoothing.
fn closed_stroke_params(brush: &BrushParams) -> BrushParams {
    let mut params = *brush;
    params.outline.taper_start = 0.0;
    params.outline.taper_end = 0.0;
    params.outline.cap_start = CapStyle::Round;
    params.outline.cap_end = CapStyle::Round;
    let pass = |p: OneEuroParams| OneEuroParams {
        min_cutoff_hz: PASS_THROUGH_CUTOFF_HZ,
        beta: 0.0,
        d_cutoff_hz: p.d_cutoff_hz,
    };
    params.smoothing.position = pass(params.smoothing.position);
    params.smoothing.pressure = pass(params.smoothing.pressure);
    params
}

/// Clears `out` and fills it with the synthetic sample positions of one run: every segment (plus the closing
/// one for closed runs) split into `ceil(length / step)` pieces, zero-length segments skipped; closed runs
/// with at least two distinct positions end back on the first and then repeat the second, so the first two
/// samples appear again at the end.
fn run_positions(run: &[[f32; 2]], closed: bool, step: f32, out: &mut Vec<[f32; 2]>) {
    out.clear();
    let Some(&first) = run.first() else {
        return;
    };
    out.push(first);
    let mut prev = first;
    for &next in &run[1..] {
        subdivide(prev, next, step, out);
        prev = next;
    }
    if closed && let Some(&second) = out.get(1) {
        // Ends on `first` exactly (or on the last vertex when it already coincides with `first`).
        subdivide(prev, first, step, out);
        out.push(second);
    }
}

/// Appends the points after `a` up to and including `b`, `ceil(|b − a| / step)` equal pieces; nothing for a
/// zero-length (or non-finite) segment.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "piece count is a small positive integer (capped), exact in f32"
)]
fn subdivide(a: [f32; 2], b: [f32; 2], step: f32, out: &mut Vec<[f32; 2]>) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length = (dx * dx + dy * dy).sqrt();
    if !(length > 0.0 && length.is_finite()) {
        return;
    }
    let pieces = (length / step).ceil().clamp(1.0, MAX_PIECES_PER_SEGMENT) as u32;
    for i in 1..pieces {
        let t = i as f32 / pieces as f32;
        out.push([a[0] + dx * t, a[1] + dy * t]);
    }
    out.push(b);
}

/// Synthetic sample `index` at `p`: pressure 1, upright pen, `index · 2 ms`.
fn synthetic(p: [f32; 2], index: u32) -> Sample {
    Sample {
        x: p[0],
        y: p[1],
        pressure: 1.0,
        tilt: 0.0,
        azimuth: 0.0,
        t_us: index.saturating_mul(SYNTHETIC_DT_US),
    }
}

/// Appends `piece`'s contours to `into`, offsetting contour ends.
fn append_outline(into: &mut Outline, piece: &Outline) {
    let offset = u32::try_from(into.points.len()).unwrap_or(u32::MAX);
    into.points.extend_from_slice(&piece.points);
    into.contour_ends.extend(
        piece
            .contour_ends
            .iter()
            .map(|&end| end.saturating_add(offset)),
    );
}
