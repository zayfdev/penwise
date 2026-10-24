//! Outline brushes: centerline + widths → one closed nonzero polygon (ink-rendering.md §1–§2).
//!
//! Owner: slice b.
//!
//! Kind → algorithm:
//! - `Ballpoint`, `Fineliner`, `Marker`: perfect-freehand (`freehand.rs`) with the §1.4 penwise deviations.
//! - `Fountain`: perfect-freehand centerline, radius scaled by the elliptic nib support (`nib.rs`, §2.2).
//! - `Calligraphy`, `Highlighter`: flat nib (`nib.rs`, §2.1) swept as the union of per-segment convex hulls of
//!   the nib rectangle (`sweep.rs`), so crossing the nib axis never twists or cancels the fill; always flat ends
//!   (the nib rectangle at each end is the cap, also when tapered).
//! - Strokes of 2+ points shorter than `END_NOISE_THRESHOLD` (3 px) with round tips render the round dot at the
//!   first point (upstream `isVeryShort`); flat nibs sweep them like any stroke.
//!
//! Marker/highlighter need no separate module: flat opacity is a style property (one nonzero polygon never
//! double-covers), the round marker is perfect-freehand, and the highlighter is a flat nib.
//!
//! Initial-pressure window (§1.4): see `window.rs`.
//!
//! Only [`OutlineEngine`]'s pub API is cross-slice contract (used by `builder`). Items in `freehand`/`nib`/
//! `walker`/`window` are `pub(super)` and slice b may reshape them.

mod freehand;
mod nib;
mod sweep;
mod walker;
mod window;

use crate::brush::{BrushParams, OutlineKind};
use crate::dynamics::InkPoint;
use crate::geometry::{Outline, OutlineView};
use freehand::{END_NOISE_THRESHOLD, FreehandOptions, StrokePoint, arc_f32, non_negative};
use walker::{Ends, Shape, Sides, Step, Tip, Walker};
use window::PressureWindow;

/// Incremental outline engine for one stroke.
///
/// Stable vs tentative (§4.3): left/right points up to the last centerline point with
/// `L_i ≤ L_total − taper_end − END_NOISE_THRESHOLD` and `i ≤ n − 2` never change once emitted; everything
/// after it, both caps, the initial-pressure window and the min-length gate are tentative and recomputed from
/// state on each [`OutlineEngine::update_tail`] (bounded work: only the tentative window is recomputed).
///
/// Slice b may add private fields.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEngine {
    shape: Shape,
    /// Centerline points committed so far (after dedupe/min-length gate).
    points: Vec<StrokePoint>,
    window: PressureWindow,
    /// Walk state after the last stable point; cloned to walk the tentative tail.
    walker: Walker,
    /// Index of the first centerline point not yet walked into the stable buffers.
    next_stable: usize,
    /// Stable left/right offset points.
    left: Vec<[f32; 2]>,
    right: Vec<[f32; 2]>,
    /// Tentative parts, rebuilt by `update_tail`.
    tentative_left: Vec<[f32; 2]>,
    tentative_right: Vec<[f32; 2]>,
    start_cap: Vec<[f32; 2]>,
    end_cap: Vec<[f32; 2]>,
}

impl OutlineEngine {
    /// New engine for `kind` with `params` (copied). No allocation beyond empty `Vec`s.
    #[must_use]
    pub fn new(params: &BrushParams, kind: OutlineKind) -> Self {
        let opts = FreehandOptions::from_params(params);
        let nib = params.outline.nib;
        let tip = match kind {
            OutlineKind::Ballpoint | OutlineKind::Fineliner | OutlineKind::Marker => Tip::Round,
            OutlineKind::Fountain => Tip::Fountain(nib),
            OutlineKind::Calligraphy | OutlineKind::Highlighter => Tip::Flat(nib),
        };
        Self {
            shape: Shape {
                opts,
                tip,
                size_min: non_negative(params.size_min),
                size_max: non_negative(params.size_max),
            },
            points: Vec::new(),
            window: PressureWindow::new(opts.size),
            walker: Walker::new(),
            next_stable: 0,
            left: Vec::new(),
            right: Vec::new(),
            tentative_left: Vec::new(),
            tentative_right: Vec::new(),
            start_cap: Vec::new(),
            end_cap: Vec::new(),
        }
    }

    /// Commits points (all final: they will never be replaced). Advances centerline state (exact-equality
    /// dedupe, min-length gate at `min(size, 1 px)`, running arc length in `f64`), and moves left/right points
    /// that became stable into the stable buffers. Calling it once with `a ++ b` or twice with `a` then `b`
    /// must leave identical state.
    ///
    /// Clears the tentative parts (they may overlap the newly stable points); the next
    /// [`OutlineEngine::update_tail`] rebuilds them.
    pub fn extend(&mut self, committed: &[InkPoint]) {
        self.clear_tentative();
        for ink in committed {
            self.commit(ink, true);
        }
        self.stabilize();
    }

    /// Recomputes the tentative tail, start cap and end cap from committed state plus the optional pending
    /// `tail` point, as if the stroke ended at `tail` (or at the last committed point when `None`). Does not
    /// change committed state, so `finish` output never depends on how often this was called.
    pub fn update_tail(&mut self, tail: Option<&InkPoint>) {
        let tail = tail.and_then(|ink| self.candidate(ink));
        self.rebuild(tail.as_ref());
    }

    /// Live view borrowing the engine's buffers (as of the last `extend`/`update_tail`).
    #[must_use]
    pub fn view(&self) -> OutlineView<'_> {
        OutlineView {
            stable_left: &self.left,
            stable_right: &self.right,
            tentative_left: &self.tentative_left,
            tentative_right: &self.tentative_right,
            start_cap: &self.start_cap,
            end_cap: &self.end_cap,
        }
    }

    /// Commits `last` (if any) and returns the final single-contour polygon
    /// `left ++ end_cap ++ reverse(right) ++ start_cap` with `contour_ends = [points.len()]`.
    /// Edge cases: no points at all → empty `Outline` (no points, no contours); exactly one centerline point →
    /// a dot (13-point round fan for round brushes, the nib rectangle for flat nibs); a round-tip stroke shorter
    /// than 3 px → the round dot at its first point.
    #[must_use]
    pub fn finish(mut self, last: Option<&InkPoint>) -> Outline {
        if let Some(ink) = last {
            // The pen-up point is the stroke's last point: it bypasses the min-length gate (upstream `last`).
            self.commit(ink, false);
        }
        self.rebuild(None);
        let mut points = Vec::new();
        self.view().write_polygon(&mut points);
        let contour_ends = if points.is_empty() {
            Vec::new()
        } else {
            vec![u32::try_from(points.len()).unwrap_or(u32::MAX)]
        };
        Outline {
            points,
            contour_ends,
        }
    }

    fn clear_tentative(&mut self) {
        self.tentative_left.clear();
        self.tentative_right.clear();
        self.start_cap.clear();
        self.end_cap.clear();
    }

    /// The centerline point `ink` would add after the committed points (`None`: non-finite or duplicate).
    fn candidate(&self, ink: &InkPoint) -> Option<StrokePoint> {
        let xy = [ink.x, ink.y];
        if !xy[0].is_finite() || !xy[1].is_finite() {
            return None;
        }
        match self.points.last() {
            None => Some(StrokePoint::first(xy, ink.size_pressure)),
            Some(prev) => prev.next(xy, ink.size_pressure),
        }
    }

    /// Appends `ink` to the centerline; `gate` applies the min-length gate to the second point.
    fn commit(&mut self, ink: &InkPoint, gate: bool) {
        let Some(p) = self.candidate(ink) else {
            return;
        };
        let index = self.points.len();
        if gate && index == 1 && p.distance < self.shape.opts.min_length {
            return;
        }
        self.window.add(index, &p);
        self.points.push(p);
    }

    /// Walks every point that can no longer change into the stable buffers.
    fn stabilize(&mut self) {
        let n = self.points.len();
        let Some(last) = self.points.last() else {
            return;
        };
        if !self.window.closed {
            return;
        }
        let total = last.running_length;
        let limit = total - f64::from(self.shape.opts.taper_end + END_NOISE_THRESHOLD);
        while self.next_stable + 1 < n {
            let Some(step) = self.step_at(self.next_stable, n, None, total, &self.window) else {
                return;
            };
            if step.point.running_length > limit {
                return;
            }
            let mut sides = Sides {
                left: &mut self.left,
                right: &mut self.right,
            };
            self.walker.step(&self.shape, &step, &mut sides);
            self.next_stable += 1;
        }
    }

    /// Rebuilds tentative sides and caps for a stroke ending at `tail` (or the last committed point).
    fn rebuild(&mut self, tail: Option<&StrokePoint>) {
        self.clear_tentative();
        let n = self.points.len() + usize::from(tail.is_some());
        let Some(last) = n.checked_sub(1).and_then(|i| self.point_at(i, tail)) else {
            return;
        };
        let mut window = self.window;
        if let Some(t) = tail {
            window.add(n - 1, t);
        }
        let total = last.running_length;
        let very_short =
            !matches!(self.shape.tip, Tip::Flat(_)) && total < f64::from(END_NOISE_THRESHOLD);
        if n == 1 || very_short {
            let first = self.point_at(0, tail).unwrap_or(last);
            let pressure = window.pressure(0, &first);
            self.shape.dot(first.point, pressure, &mut self.start_cap);
            return;
        }
        let mut walker = self.walker;
        for i in self.next_stable..n {
            if let Some(step) = self.step_at(i, n, tail, total, &window) {
                let mut sides = Sides {
                    left: &mut self.tentative_left,
                    right: &mut self.tentative_right,
                };
                walker.step(&self.shape, &step, &mut sides);
            }
        }
        let ends = Ends {
            first: self.points.first().map_or(last.point, |p| p.point),
            last,
            radius: walker.radius,
            first_sides: first_pair(&self.left, &self.right)
                .or_else(|| first_pair(&self.tentative_left, &self.tentative_right)),
        };
        self.shape
            .caps(&ends, &mut self.start_cap, &mut self.end_cap);
    }

    /// Committed point `i`, or `tail` at index `points.len()`.
    fn point_at(&self, i: usize, tail: Option<&StrokePoint>) -> Option<StrokePoint> {
        match self.points.get(i) {
            Some(p) => Some(*p),
            None if i == self.points.len() => tail.copied(),
            None => None,
        }
    }

    /// Walk input for point `i` of an `n`-point centerline of length `total` with pressure window `window`.
    fn step_at(
        &self,
        i: usize,
        n: usize,
        tail: Option<&StrokePoint>,
        total: f64,
        window: &PressureWindow,
    ) -> Option<Step> {
        let mut point = self.point_at(i, tail)?;
        let next = if i + 1 < n {
            self.point_at(i + 1, tail)
        } else {
            None
        };
        if i == 0 {
            // Upstream: `points[0].vector = points[1].vector`.
            point.vector = next.map_or([0.0, 0.0], |p| p.vector);
        }
        Some(Step {
            index: i,
            point,
            next_vector: next.map(|p| p.vector),
            pressure: window.pressure(i, &point),
            remaining: arc_f32(total - point.running_length),
        })
    }
}

/// First `(left, right)` pair of two side buffers.
fn first_pair(left: &[[f32; 2]], right: &[[f32; 2]]) -> Option<([f32; 2], [f32; 2])> {
    left.first().copied().zip(right.first().copied())
}

#[cfg(test)]
mod tests;
