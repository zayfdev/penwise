//! Per-point offset walk of perfect-freehand's `getStrokeOutlinePoints` loop (ink-rendering.md §1.3 steps 1–8),
//! with the §2 nib variants. Derived from perfect-freehand v1.2.3 (MIT, © 2021 Stephen Ruiz Ltd; full notice in
//! `freehand.rs`).
//!
//! Owner: slice b.
//!
//! [`Walker`] is the loop's carried state (`prevVector`, `isPrevPointSharpCorner`, `prevLeft/Right`, the
//! direction smoother, the last radius). It is `Copy`: the engine advances one instance over stable points and
//! clones it to walk the tentative tail, so a point's offsets never depend on when it was walked.

use super::freehand::{
    self, CORNER_CAP_SEGMENTS, END_NOISE_THRESHOLD, FIXED_PI, FreehandOptions, MIN_RADIUS,
    StrokePoint, arc_f32, flat_cap, per, rot_around, round_end_cap, round_start_cap,
};
use super::nib::{
    DirectionSmoother, effective_angle, flat_offsets, fountain_scale, nib_rect, nib_width,
};
use crate::brush::{CapStyle, NibParams};
use crate::float::{lerp, max};

/// Offset model of an outline kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Tip {
    /// perfect-freehand symmetric normal offsets (ballpoint, fineliner, marker).
    Round,
    /// Round offsets scaled by the elliptic nib support (§2.2).
    Fountain(NibParams),
    /// Ribbon offset along the nib axis, no corner fans (§2.1: calligraphy, highlighter).
    Flat(NibParams),
}

/// Everything about the brush the walk needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Shape {
    pub opts: FreehandOptions,
    pub tip: Tip,
    /// Flat-nib widths (`BrushParams::size_min/size_max`).
    pub size_min: f32,
    pub size_max: f32,
}

/// One point of the walk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Step {
    /// Index in the centerline (upstream `i`, used by the `i ≤ 1` decimation rule).
    pub index: usize,
    /// The point with its final `vector` (point 0 carries point 1's vector).
    pub point: StrokePoint,
    /// Next point's vector; `None` for the last point.
    pub next_vector: Option<[f32; 2]>,
    /// Pressure after the initial-pressure window.
    pub pressure: f32,
    /// `totalLength − runningLength`, px.
    pub remaining: f32,
}

/// Output buffers of one side pair.
#[derive(Debug)]
pub(super) struct Sides<'a> {
    pub left: &'a mut Vec<[f32; 2]>,
    pub right: &'a mut Vec<[f32; 2]>,
}

/// Stroke ends for [`Shape::caps`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Ends {
    /// First centerline point.
    pub first: [f32; 2],
    pub last: StrokePoint,
    /// Radius of the last walked point.
    pub radius: f32,
    /// First emitted `(left, right)` offset points.
    pub first_sides: Option<([f32; 2], [f32; 2])>,
    /// Last emitted `(left, right)` offset points.
    pub last_sides: Option<([f32; 2], [f32; 2])>,
}

impl Shape {
    /// Single-point stroke at `center`: 13-point round fan, or the nib rectangle for flat nibs.
    pub(super) fn dot(&self, center: [f32; 2], pressure: f32, out: &mut Vec<[f32; 2]>) {
        match self.tip {
            Tip::Round | Tip::Fountain(_) => freehand::dot(center, self.opts.radius(pressure), out),
            Tip::Flat(nib) => {
                let w = nib_width(self.size_min, self.size_max, pressure);
                nib_rect(center, max(2.0 * MIN_RADIUS, w), &nib, out);
            }
        }
    }

    /// Start and end caps (§1.3 "Caps"): a tapered start gets none, a tapered end the bare end point; flat
    /// nibs always get flat caps across their ribbon ends.
    pub(super) fn caps(&self, e: &Ends, start: &mut Vec<[f32; 2]>, end: &mut Vec<[f32; 2]>) {
        let opts = &self.opts;
        let flat_tip = matches!(self.tip, Tip::Flat(_));
        if let Some((left0, right0)) = e.first_sides.filter(|_| opts.taper_start <= 0.0) {
            if flat_tip || opts.cap_start == CapStyle::Flat {
                flat_cap(e.first, half_diff(left0, right0), start);
            } else {
                round_start_cap(e.first, right0, start);
            }
        }
        let last = e.last.point;
        if opts.taper_end > 0.0 {
            end.push(last);
        } else if flat_tip {
            let (left, right) = e.last_sides.unwrap_or((last, last));
            flat_cap(last, half_diff(right, left), end);
        } else if opts.cap_end == CapStyle::Flat {
            flat_cap(last, scale(per(e.last.vector), e.radius), end);
        } else {
            round_end_cap(last, e.last.vector, e.radius, end);
        }
    }
}

/// Loop state carried from point to point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Walker {
    prev_vector: [f32; 2],
    prev_was_corner: bool,
    prev_left: [f32; 2],
    prev_right: [f32; 2],
    dir: DirectionSmoother,
    /// Radius of the last walked point (after taper): the end cap radius.
    pub radius: f32,
}

impl Walker {
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            prev_vector: [0.0, 0.0],
            prev_was_corner: false,
            prev_left: [0.0, 0.0],
            prev_right: [0.0, 0.0],
            dir: DirectionSmoother::new(),
            radius: MIN_RADIUS,
        }
    }

    /// Walks one point, appending its offset points (none when skipped as end noise).
    pub(super) fn step(&mut self, shape: &Shape, s: &Step, out: &mut Sides<'_>) {
        if s.index == 0 {
            self.prev_vector = s.point.vector;
        }
        let forward = [-s.point.vector[0], -s.point.vector[1]];
        match shape.tip {
            Tip::Round => {
                let r = shape.opts.radius(s.pressure);
                self.round_step(shape, s, r, out);
            }
            Tip::Fountain(nib) => {
                let r = shape.opts.radius(s.pressure);
                let d = self.dir.update(forward, s.point.distance, r);
                self.round_step(shape, s, r * fountain_scale(d, &nib), out);
            }
            Tip::Flat(nib) => {
                let w = nib_width(shape.size_min, shape.size_max, s.pressure);
                let d = self.dir.update(forward, s.point.distance, w * 0.5);
                self.flat_step(shape, s, w, d, &nib, out);
            }
        }
    }

    /// Upstream loop body for round tips with untapered radius `r`.
    fn round_step(&mut self, shape: &Shape, s: &Step, r: f32, out: &mut Sides<'_>) {
        let is_last = s.next_vector.is_none();
        if !is_last && s.remaining < END_NOISE_THRESHOLD {
            return;
        }
        let arc = arc_f32(s.point.running_length);
        let r = max(MIN_RADIUS, r * shape.opts.taper(arc, s.remaining));
        self.radius = r;
        let p = s.point.point;
        let vector = s.point.vector;
        let next = s.next_vector.unwrap_or(vector);
        let next_dpr = if is_last { 1.0 } else { dot(vector, next) };
        let prev_dpr = dot(vector, self.prev_vector);
        let is_corner = prev_dpr < 0.0 && !self.prev_was_corner;
        let next_corner = next_dpr < 0.0;
        if is_corner || next_corner {
            self.corner_fan(p, r, out);
            if next_corner {
                self.prev_was_corner = true;
            }
            return;
        }
        self.prev_was_corner = false;
        if is_last {
            let offset = scale(per(vector), r);
            out.left.push(sub(p, offset));
            out.right.push(add(p, offset));
            return;
        }
        let blend = [
            lerp(next[0], vector[0], next_dpr),
            lerp(next[1], vector[1], next_dpr),
        ];
        let offset = scale(per(blend), r);
        let min_dist = shape.opts.min_dist_sq();
        let tl = sub(p, offset);
        if s.index <= 1 || dist_sq(self.prev_left, tl) > min_dist {
            out.left.push(tl);
            self.prev_left = tl;
        }
        let tr = add(p, offset);
        if s.index <= 1 || dist_sq(self.prev_right, tr) > min_dist {
            out.right.push(tr);
            self.prev_right = tr;
        }
        self.prev_vector = vector;
    }

    /// Sharp-corner half-disc fans on both sides (§1.3 step 5), `t = k/13, k = 0..=13`.
    fn corner_fan(&mut self, p: [f32; 2], r: f32, out: &mut Sides<'_>) {
        let offset = scale(per(self.prev_vector), r);
        let left_start = sub(p, offset);
        let right_start = add(p, offset);
        let segments = f32::from(CORNER_CAP_SEGMENTS);
        for k in 0..=CORNER_CAP_SEGMENTS {
            let angle = FIXED_PI * (f32::from(k) / segments);
            let tl = rot_around(left_start, p, angle);
            let tr = rot_around(right_start, p, -angle);
            out.left.push(tl);
            out.right.push(tr);
            self.prev_left = tl;
            self.prev_right = tr;
        }
    }

    /// Flat-nib ribbon point: width `w` (untapered), smoothed forward direction `d`. Both sides are emitted
    /// together (decimated on either side moving more than `minDist`) so the ribbon edges stay paired.
    fn flat_step(
        &mut self,
        shape: &Shape,
        s: &Step,
        w: f32,
        d: [f32; 2],
        nib: &NibParams,
        out: &mut Sides<'_>,
    ) {
        let is_last = s.next_vector.is_none();
        if !is_last && s.remaining < END_NOISE_THRESHOLD {
            return;
        }
        let arc = arc_f32(s.point.running_length);
        let taper = shape.opts.taper(arc, s.remaining);
        let w = max(2.0 * MIN_RADIUS, w * taper);
        self.radius = w * 0.5;
        let tapered = NibParams {
            thickness: nib.thickness * taper,
            ..*nib
        };
        let phi = effective_angle(d, nib);
        let (tl, tr) = flat_offsets(s.point.point, d, w, phi, &tapered);
        let min_dist = shape.opts.min_dist_sq();
        if is_last
            || s.index <= 1
            || dist_sq(self.prev_left, tl) > min_dist
            || dist_sq(self.prev_right, tr) > min_dist
        {
            out.left.push(tl);
            out.right.push(tr);
            self.prev_left = tl;
            self.prev_right = tr;
        }
        self.prev_vector = s.point.vector;
    }
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn scale(a: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] * k, a[1] * k]
}

/// `(a − b) / 2`.
fn half_diff(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] - b[0]) * 0.5, (a[1] - b[1]) * 0.5]
}

fn dist_sq(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}
