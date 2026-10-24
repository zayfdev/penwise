//! Precise erase of outline strokes: split in the raw-sample domain (editing-algorithms.md §1, option A).
//!
//! Owner: slice b.
//!
//! Coverage rule: origin point `u` (centre `c(u)`, half-width `h(u)`, both lerped) is erased when
//! `dist(c(u), E) < r_e + h(u)`, `E` the eraser centreline. Cut params come from 12 fixed bisection steps
//! on each inside/outside boundary segment (deterministic; 1/4096 segment = one [`SampleParam`] frac step).
//! Segments longer than `r_e / 2` are tested at sub-steps so a thin eraser cannot pass between samples.

use super::Edit;
use crate::item::{Affine, IdSource, Item, SampleParam, SampleRange, StrokeItem};

/// Number of bisection steps per cut (frozen with [`SampleParam::FRAC_BITS`]).
pub const CUT_BISECTION_STEPS: u32 = 12;

/// Pieces shorter than this fraction of their mean half-width (visible arc length) are dropped.
pub const MIN_PIECE_LENGTH_FACTOR: f32 = 0.5;

/// `visible` minus the eraser coverage, in origin space. `visible` is the stroke's effective visible set
/// (sorted, disjoint); coverage is evaluated on the stroke's page centreline (`stroke.centreline` under
/// `transform`, all stored samples). Ends created by the eraser are marked cut; surviving ends keep their
/// flags. Returns `None` when nothing visible is covered (the stroke is untouched).
#[must_use]
pub fn uncovered_ranges(
    stroke: &StrokeItem,
    transform: &Affine,
    visible: &[SampleRange],
    eraser: &[[f32; 2]],
    radius: f32,
) -> Option<Vec<SampleRange>> {
    todo!()
}

/// Edits replacing live stroke `item` (`id = item.id`, `stroke` = its data) by pieces over `kept` (output of [`uncovered_ranges`]):
/// `Delete(id)` then one `Add` per kept range whose visible arc length ≥ [`MIN_PIECE_LENGTH_FACTOR`] ·
/// mean half-width, in range order, each with `ids.next_id()`. A piece copies the original's layer, z,
/// transform, params and seed; its samples are the origin window `[start.index − CONTEXT_SAMPLES,
/// end.index + 1 + CONTEXT_SAMPLES]` clipped to what `id` stores; its [`crate::item::Trim`] has
/// `origin = stroke.origin(id)`, `visible` = the member's family set with its own range replaced by the kept
/// pieces (other members' ranges untouched), and `own` = the piece's index in that set. Only `Delete(id)` when
/// nothing survives.
#[must_use]
pub fn split_edits(
    item: &Item,
    stroke: &StrokeItem,
    kept: &[SampleRange],
    ids: &mut dyn IdSource,
) -> Vec<Edit> {
    todo!()
}

/// Bisection of one boundary: the param in `[a, b]` (one origin segment) where coverage flips, after
/// [`CUT_BISECTION_STEPS`] halvings, rounded toward the uncovered side.
#[must_use]
pub fn bisect_cut(
    stroke: &StrokeItem,
    transform: &Affine,
    a: SampleParam,
    b: SampleParam,
    eraser: &[[f32; 2]],
    radius: f32,
) -> SampleParam {
    todo!()
}
