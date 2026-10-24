//! Trimmed-stroke rendering pipeline: warm smoothing over stored samples, per-range point extraction with
//! interpolated ends, and per-range outlines with untapered cut ends.
//!
//! Owner: slice b.

use super::{CentrePoint, SampleParam, SampleRange, StrokeItem};
use crate::brush::{BrushFamily, BrushParams, CapStyle, OutlineKind};
use crate::dynamics::InkPoint;
use crate::float::{clamp01, lerp, max};
use crate::geometry::{Outline, StrokeGeometry, StrokeStyle};
use crate::item::Affine;
use crate::outline::OutlineEngine;
use crate::sample::Sample;
use crate::smoothing::Smoother;

/// `params` with `size_min`/`size_max` × [`Affine::width_scale`] (width applied after the transform).
pub(super) fn scaled_params(params: &BrushParams, transform: &Affine) -> BrushParams {
    let w = transform.width_scale();
    let mut p = *params;
    p.size_min *= w;
    p.size_max *= w;
    p
}

/// Empty geometry of the brush's family.
pub(super) fn empty_geometry(params: &BrushParams, style: StrokeStyle) -> StrokeGeometry {
    match params.family {
        BrushFamily::Outline(_) => StrokeGeometry::Outline {
            style,
            outline: Outline::default(),
        },
        BrushFamily::Stamp(_) => StrokeGeometry::Stamps {
            style,
            grain: params.stamp.grain,
            dabs: Vec::new(),
        },
    }
}

/// One ink point per stored sample, exactly as `StrokeBuilder` produces them: position mapped through
/// `transform`, quantized, One Euro smoothed, dynamics applied. The origin's last sample (pen-up) takes its
/// raw quantized position, like the builder's `finish`.
pub(super) fn ink_points(
    stroke: &StrokeItem,
    transform: &Affine,
    params: &BrushParams,
) -> Vec<InkPoint> {
    let mut smoother = Smoother::new(&params.smoothing);
    let dynamics = params.dynamics;
    let mut out = Vec::with_capacity(stroke.samples.len());
    let mut last_raw = None;
    for s in &stroke.samples {
        let [x, y] = transform.apply([s.x, s.y]);
        let filtered = smoother.filter(&Sample { x, y, ..*s }.quantize());
        out.push(dynamics.apply(&filtered));
        last_raw = Some([filtered.raw_x, filtered.raw_y]);
    }
    if stroke.ends_at_origin_end()
        && let (Some(last), Some([x, y])) = (out.last_mut(), last_raw)
    {
        last.x = x;
        last.y = y;
    }
    out
}

/// Half-width at `size_pressure` from the brush's size response: outline brushes are linear in pressure
/// (perfect-freehand thinning), stamp brushes follow `size_min + (size_max − size_min)·P^size_gamma`.
pub(super) fn half_width(params: &BrushParams, size_pressure: f32) -> f32 {
    let d_min = non_negative(params.size_min);
    let d_max = non_negative(params.size_max);
    let p = clamp01(size_pressure);
    let d = match params.family {
        BrushFamily::Outline(_) => d_min + (d_max - d_min) * p,
        BrushFamily::Stamp(_) => {
            if p <= 0.0 {
                d_min
            } else if p >= 1.0 {
                d_max
            } else {
                d_min + (d_max - d_min) * libm::powf(p, params.size_gamma)
            }
        }
    };
    max(d_min, d) * 0.5
}

fn non_negative(x: f32) -> f32 {
    if x.is_finite() && x > 0.0 { x } else { 0.0 }
}

/// Params of the points of `range` (already clipped to the `n` stored samples starting at origin index
/// `offset`): the start, every stored sample strictly inside, and the end when it differs from the start.
/// Writes into `out` (cleared first).
pub(super) fn range_params(offset: u32, n: usize, range: &SampleRange, out: &mut Vec<SampleParam>) {
    out.clear();
    if n == 0 {
        return;
    }
    out.push(range.start);
    let first = range.start.index().saturating_add(1);
    let last = range.end.index();
    let stored_end = offset.saturating_add(u32::try_from(n - 1).unwrap_or(u32::MAX));
    for index in first.max(offset)..=last.min(stored_end) {
        let u = SampleParam::at_index(index);
        if u > range.start && u < range.end {
            out.push(u);
        }
    }
    if range.end > range.start {
        out.push(range.end);
    }
}

/// Stored-slice index of param `u` and its fraction (`None` when `u` sits exactly on that sample or on the
/// last stored one).
fn locate(len: usize, offset: u32, u: SampleParam) -> Option<(usize, Option<f32>)> {
    let i = usize::try_from(u.index().checked_sub(offset)?).ok()?;
    let exact = (u.0 & super::FRAC_MASK) == 0 || i + 1 >= len;
    (i < len).then(|| (i, (!exact).then(|| u.frac())))
}

/// Centre point at `u`, all channels lerped inside its segment (clamped to the stored window).
pub(super) fn centre_at(points: &[CentrePoint], offset: u32, u: SampleParam) -> CentrePoint {
    let Some((i, t)) =
        locate(points.len(), offset, u).or_else(|| clamp_locate(points.len(), offset, u))
    else {
        return CentrePoint {
            x: 0.0,
            y: 0.0,
            half_width: 0.0,
            param: u,
        };
    };
    let a = points[i];
    let Some(t) = t else {
        return CentrePoint { param: u, ..a };
    };
    let b = points[i + 1];
    CentrePoint {
        x: lerp(a.x, b.x, t),
        y: lerp(a.y, b.y, t),
        half_width: lerp(a.half_width, b.half_width, t),
        param: u,
    }
}

/// Fallback for params outside the stored window: the nearest stored sample.
fn clamp_locate(len: usize, offset: u32, u: SampleParam) -> Option<(usize, Option<f32>)> {
    let last = len.checked_sub(1)?;
    Some((if u.index() < offset { 0 } else { last }, None))
}

/// Ink point at `u` (see [`centre_at`]); positions, pressures, speed and angles lerped, time and azimuth
/// taken from the nearer sample (azimuth wraps at ±π, so it is not lerped).
pub(super) fn ink_at(points: &[InkPoint], offset: u32, u: SampleParam) -> InkPoint {
    let Some((i, t)) =
        locate(points.len(), offset, u).or_else(|| clamp_locate(points.len(), offset, u))
    else {
        return InkPoint {
            x: 0.0,
            y: 0.0,
            t_us: 0,
            pressure: 0.0,
            size_pressure: 0.0,
            alpha_factor: 1.0,
            speed: 0.0,
            tilt: 0.0,
            azimuth: 0.0,
        };
    };
    let a = points[i];
    let Some(t) = t else {
        return a;
    };
    let b = points[i + 1];
    let near = if t < 0.5 { a } else { b };
    InkPoint {
        x: lerp(a.x, b.x, t),
        y: lerp(a.y, b.y, t),
        t_us: near.t_us,
        pressure: lerp(a.pressure, b.pressure, t),
        size_pressure: lerp(a.size_pressure, b.size_pressure, t),
        alpha_factor: lerp(a.alpha_factor, b.alpha_factor, t),
        speed: lerp(a.speed, b.speed, t),
        tilt: lerp(a.tilt, b.tilt, t),
        azimuth: near.azimuth,
    }
}

/// Outline of one visible range: cut ends get no taper and a round cap, natural ends keep the brush's.
/// The last point is committed through `finish` (as the builder commits its pen-up point).
pub(super) fn range_outline(
    params: &BrushParams,
    kind: OutlineKind,
    points: &[InkPoint],
    range: &SampleRange,
) -> Outline {
    let mut p = *params;
    if range.start_cut {
        p.outline.taper_start = 0.0;
        p.outline.cap_start = CapStyle::Round;
    }
    if range.end_cut {
        p.outline.taper_end = 0.0;
        p.outline.cap_end = CapStyle::Round;
    }
    let Some((last, body)) = points.split_last() else {
        return Outline::default();
    };
    let mut engine = OutlineEngine::new(&p, kind);
    engine.extend(body);
    engine.finish(Some(last))
}

/// Appends `piece`'s contours to `outline`.
pub(super) fn append_outline(outline: &mut Outline, piece: &Outline) {
    let base = u32::try_from(outline.points.len()).unwrap_or(u32::MAX);
    outline.points.extend_from_slice(&piece.points);
    outline.contour_ends.extend(
        piece
            .contour_ends
            .iter()
            .map(|&end| base.saturating_add(end)),
    );
}
