//! Stroke items, visible-range trims for precise-erase pieces, and stroke centrelines for hit testing.
//!
//! Owner: slice b.
//!
//! **Sample-param space.** A stroke family is rooted at its *origin* stroke (an untrimmed stroke is its own
//! origin). Every piece cut from it (and every piece cut from a piece) keeps the origin's sample indexing:
//! `SampleParam` `u` addresses the point `lerp(s[i], s[i+1], frac)` of the origin's samples, where
//! `i = u >> 12`. A piece stores only the window of origin samples it needs (its visible range plus
//! [`CONTEXT_SAMPLES`] on each side, clipped), starting at origin index [`Trim::sample_offset`].
//!
//! **Concurrent splits (decided).** Each piece carries the whole-origin visible set produced by the split
//! that created it ([`Trim::visible`]). The scene renders a live member `X` of a family over
//! `own(X) ∩ ⋂ visible(Y)` for every other live member `Y` (an untrimmed origin's sets are "everything"),
//! so two devices splitting the same stroke concurrently intersect their results and erased ink never
//! reappears. Pieces of one split share the same set, so intersecting with siblings is idempotent.

use super::affine::Affine;
use super::id::ItemId;
use crate::brush::BrushParams;
use crate::geometry::StrokeGeometry;
use crate::sample::Sample;

/// Origin samples kept on each side of a piece's visible range so smoothing and velocity dynamics are warm
/// at the cut (editing-algorithms.md §1: ~66 ms at 482 Hz). Not a stored format: older pieces keep whatever
/// window they were cut with.
pub const CONTEXT_SAMPLES: u32 = 32;

/// Fixed-point position along the origin's samples: `index << 12 | frac` (`frac` in 1/4096 of a segment).
/// Integer so ranges compare, intersect and encode exactly; spans strokes up to 2²⁰ samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SampleParam(pub u32);

impl SampleParam {
    /// Fractional bits.
    pub const FRAC_BITS: u32 = 12;

    /// The param exactly at origin sample `index` (saturates at the largest representable index).
    #[must_use]
    pub fn at_index(index: u32) -> Self {
        todo!()
    }

    /// Origin sample index of the segment start.
    #[must_use]
    pub fn index(self) -> u32 {
        todo!()
    }

    /// Fraction along the segment, `0.0..1.0` (`frac / 4096`).
    #[must_use]
    pub fn frac(self) -> f32 {
        todo!()
    }
}

/// A visible interval `[start, end]` of the origin. `start_cut`/`end_cut` mark ends produced by an eraser:
/// rendered with a round cap at full width and no taper; natural ends keep the brush's tapers and caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SampleRange {
    pub start: SampleParam,
    pub end: SampleParam,
    pub start_cut: bool,
    pub end_cut: bool,
}

/// Visible-range data of a precise-erase piece (outline-family strokes only; stamp strokes are masked).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trim {
    /// Root stroke of the family (never a piece id).
    pub origin: ItemId,
    /// Origin index of `StrokeItem::samples[0]`.
    pub sample_offset: u32,
    /// Whole-origin visible set after the split that created this piece: sorted, disjoint, non-empty.
    pub visible: Vec<SampleRange>,
    /// Index into `visible` of the range this piece renders.
    pub own: u32,
}

/// One centreline point in page px with its half-width (from the brush dynamics, after transform).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CentrePoint {
    pub x: f32,
    pub y: f32,
    pub half_width: f32,
    /// Origin sample-param of this point.
    pub param: SampleParam,
}

/// Visible centreline runs (one per visible range). Run `k` is `points[run_ends[k-1] .. run_ends[k]]`.
/// Hit testing treats consecutive points of a run as tapered capsules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Centreline {
    pub points: Vec<CentrePoint>,
    /// Exclusive end index of each run, strictly increasing, last == `points.len()`.
    pub run_ends: Vec<u32>,
}

/// A pen stroke (PLAN "Data model": brush params + seed + raw samples are the source of truth).
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeItem {
    /// Brush (mutable as a whole: [`crate::edit::Field::Brush`], e.g. recolor).
    pub params: BrushParams,
    /// Per-stroke PRNG seed (immutable).
    pub seed: u64,
    /// Local-frame samples on the quantization grid (immutable).
    pub samples: Vec<Sample>,
    /// `None` = whole stroke (its own origin). Immutable.
    pub trim: Option<Trim>,
}

impl StrokeItem {
    /// Root of this stroke's family: `trim.origin`, or `id` (this item's own id) when untrimmed.
    #[must_use]
    pub fn origin(&self, id: ItemId) -> ItemId {
        todo!()
    }

    /// This item's own visible range in origin space: `trim.visible[own]`, or for an untrimmed stroke
    /// `[0, (n−1) << 12]` with both ends natural (`n` = sample count; a 1-sample stroke is `[0, 0]`).
    #[must_use]
    pub fn own_range(&self) -> SampleRange {
        todo!()
    }

    /// Whole-origin visible set this member contributes to the family intersection: `trim.visible`, or
    /// `[own_range()]` for an untrimmed stroke (= "everything", since the origin spans all its samples).
    #[must_use]
    pub fn family_visible(&self) -> Vec<SampleRange> {
        todo!()
    }

    /// Visible centreline in page px over `visible` (origin-space ranges; normally
    /// [`crate::item::Scene::visible_ranges`]). Runs the brush's own `Smoother` + `Dynamics` over **all**
    /// stored samples (warm context), maps positions through `transform`, scales half-widths by
    /// [`Affine::width_scale`], and emits per range the points strictly inside it plus points interpolated
    /// (all channels lerped at `frac`) at both ends. Ranges outside the stored window are clipped; empty
    /// `visible` → empty centreline. Writes into `out` (cleared first) to reuse its buffers.
    pub fn centreline(&self, transform: &Affine, visible: &[SampleRange], out: &mut Centreline) {
        todo!()
    }

    /// Render geometry over `visible`.
    /// - Untrimmed stroke whose `visible` is its whole own range: exactly
    ///   `StrokeBuilder::build(scaled_params, seed, mapped_samples)` where samples are mapped through
    ///   `transform` and `scaled_params` has `size_min`/`size_max` × [`Affine::width_scale`]
    ///   (stroke width applied after transform). Identity transform ⇒ bit-identical to M1 geometry.
    /// - Otherwise (outline family only): smoothing/dynamics run over every stored sample; for each range the
    ///   points inside it (ends interpolated) go through `OutlineEngine` with taper `0` and `CapStyle::Round`
    ///   on cut ends and the brush's taper/caps on natural ends; contours are concatenated into one
    ///   [`crate::geometry::Outline`] (nonzero). Stamp-family strokes ignore `visible` (they are never trimmed).
    /// - Empty `visible` → empty geometry with the brush style.
    #[must_use]
    pub fn build_geometry(&self, transform: &Affine, visible: &[SampleRange]) -> StrokeGeometry {
        todo!()
    }
}

/// Intersection of two sorted, disjoint range sets (result sorted, disjoint). Empty intersections
/// (`start > end`) are dropped; a degenerate `start == end` result is kept only when one operand range is
/// itself degenerate (a dot), never when two ranges merely touch. An end of a result range is a cut when it
/// is a cut in the operand that bounds it there (equal bounds: cut wins).
#[must_use]
pub fn intersect_ranges(a: &[SampleRange], b: &[SampleRange]) -> Vec<SampleRange> {
    todo!()
}
