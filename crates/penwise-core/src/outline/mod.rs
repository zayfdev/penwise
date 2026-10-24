//! Outline brushes: centerline + widths → one closed nonzero polygon (ink-rendering.md §1–§2).
//!
//! Owner: slice b.
//!
//! Kind → algorithm:
//! - `Ballpoint`, `Fineliner`, `Marker`: perfect-freehand (`freehand.rs`) with the §1.4 penwise deviations.
//! - `Fountain`: perfect-freehand centerline, radius scaled by the elliptic nib support (`nib.rs`, §2.2).
//! - `Calligraphy`, `Highlighter`: flat nib ribbon offset along the nib axis (`nib.rs`, §2.1), corner fans off.
//!
//! Marker/highlighter need no separate module: flat opacity is a style property (one nonzero polygon never
//! double-covers), the round marker is perfect-freehand, and the highlighter is a flat nib.
//!
//! Only [`OutlineEngine`]'s pub API is cross-slice contract (used by `builder`). Items in `freehand`/`nib`
//! are `pub(super)` and slice b may reshape them.

mod freehand;
mod nib;

use crate::brush::{BrushParams, OutlineKind};
use crate::dynamics::InkPoint;
use crate::geometry::{Outline, OutlineView};

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
    kind: OutlineKind,
    params: BrushParams,
    /// Centerline points committed so far (after dedupe/min-length gate).
    points: Vec<freehand::StrokePoint>,
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
        todo!()
    }

    /// Commits points (all final: they will never be replaced). Advances centerline state (exact-equality
    /// dedupe, min-length gate at `min(size, 1 px)`, running arc length in `f64`), and moves left/right points
    /// that became stable into the stable buffers. Calling it once with `a ++ b` or twice with `a` then `b`
    /// must leave identical state.
    pub fn extend(&mut self, committed: &[InkPoint]) {
        todo!()
    }

    /// Recomputes the tentative tail, start cap and end cap from committed state plus the optional pending
    /// `tail` point, as if the stroke ended at `tail` (or at the last committed point when `None`). Does not
    /// change committed state, so `finish` output never depends on how often this was called.
    pub fn update_tail(&mut self, tail: Option<&InkPoint>) {
        todo!()
    }

    /// Live view borrowing the engine's buffers (as of the last `extend`/`update_tail`).
    #[must_use]
    pub fn view(&self) -> OutlineView<'_> {
        todo!()
    }

    /// Commits `last` (if any) and returns the final single-contour polygon
    /// `left ++ end_cap ++ reverse(right) ++ start_cap` with `contour_ends = [points.len()]`.
    /// Edge cases: no points at all → empty `Outline` (no points, no contours); exactly one centerline point →
    /// a dot (13-point round fan for round brushes, the nib rectangle for flat nibs).
    #[must_use]
    pub fn finish(self, last: Option<&InkPoint>) -> Outline {
        todo!()
    }
}
