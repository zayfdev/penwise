//! Origin sample-param space: [`SampleParam`], [`SampleRange`] and the piece [`Trim`].
//!
//! Owner: slice b.

use crate::item::id::ItemId;
use crate::smoothing::SmootherState;

/// Largest origin sample index a [`SampleParam`] can address.
const MAX_INDEX: u32 = u32::MAX >> SampleParam::FRAC_BITS;

/// Most samples a stroke family can address (`MAX_INDEX + 1` = 2²⁰); [`crate::item::Item::validate`]
/// rejects strokes whose stored window reaches past it.
pub const MAX_STROKE_SAMPLES: u32 = MAX_INDEX + 1;

/// Mask of the fractional bits.
pub(super) const FRAC_MASK: u32 = (1 << SampleParam::FRAC_BITS) - 1;

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
        let index = if index > MAX_INDEX { MAX_INDEX } else { index };
        Self(index << Self::FRAC_BITS)
    }

    /// Origin sample index of the segment start.
    #[must_use]
    pub fn index(self) -> u32 {
        self.0 >> Self::FRAC_BITS
    }

    /// Fraction along the segment, `0.0..1.0` (`frac / 4096`).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "frac < 4096 is exactly representable in f32"
    )]
    pub fn frac(self) -> f32 {
        (self.0 & FRAC_MASK) as f32 / 4096.0
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
#[derive(Debug, Clone, PartialEq)]
pub struct Trim {
    /// Root stroke of the family (never a piece id).
    pub origin: ItemId,
    /// Origin index of `StrokeItem::samples[0]`.
    pub sample_offset: u32,
    /// Whole-origin visible set after the split that created this piece: sorted, disjoint, non-empty.
    pub visible: Vec<SampleRange>,
    /// Index into `visible` of the range this piece renders.
    pub own: u32,
    /// Smoother memory after origin sample `sample_offset − 1`, in the **local** frame (positions and velocities
    /// before the item transform), so the piece's first sample is filtered exactly as in the origin. `None` when
    /// `sample_offset == 0` (nothing precedes it).
    pub warm: Option<SmootherState>,
}
