//! Stroke items, visible-range trims for precise-erase pieces, and stroke centrelines for hit testing.
//!
//! Owner: slice b.
//!
//! **Sample-param space.** A stroke family is rooted at its *origin* stroke (an untrimmed stroke is its own
//! origin). Every piece cut from it (and every piece cut from a piece) keeps the origin's sample indexing:
//! `SampleParam` `u` addresses the point `lerp(s[i], s[i+1], frac)` of the origin's samples, where
//! `i = u >> 12`. A piece stores only the window of origin samples it needs (its visible range plus
//! [`CUT_LOOKAHEAD_SAMPLES`] after it, clipped), starting at origin index [`Trim::sample_offset`]; the smoother
//! memory at that index is stored in [`Trim::warm`].
//!
//! **Concurrent splits (decided).** Each piece carries the whole-origin visible set produced by the split
//! that created it ([`Trim::visible`]). The scene renders a live member `X` of a family over
//! `own(X) ∩ ⋂ visible(Y)` for every other live member `Y` (an untrimmed origin's sets are "everything"),
//! so two devices splitting the same stroke concurrently intersect their results and erased ink never
//! reappears. Pieces of one split share the same set, so intersecting with siblings is idempotent.

mod param;
mod ranges;
mod render;

pub use param::{MAX_STROKE_SAMPLES, SampleParam, SampleRange, Trim};

use super::affine::Affine;
use super::id::ItemId;
use crate::brush::{BrushFamily, BrushParams};
use crate::builder::StrokeBuilder;
use crate::geometry::{Outline, StrokeGeometry, StrokeStyle};
use crate::sample::Sample;
use crate::smoothing::SmootherState;

use render::{centre_at, range_params};

/// Origin samples kept after a piece's visible range: the end point is interpolated between its segment's
/// samples. Nothing is kept before it: smoothing resumes from [`Trim::warm`] and everything downstream of the
/// smoother is causal. Not a stored format.
pub const CUT_LOOKAHEAD_SAMPLES: u32 = 1;

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
        self.trim.as_ref().map_or(id, |t| t.origin)
    }

    /// This item's own visible range in origin space: `trim.visible[own]`, or for an untrimmed stroke
    /// `[0, (n−1) << 12]` with both ends natural (`n` = sample count; a 1-sample stroke is `[0, 0]`).
    #[must_use]
    pub fn own_range(&self) -> SampleRange {
        if let Some(range) = self.trim.as_ref().and_then(|t| {
            usize::try_from(t.own)
                .ok()
                .and_then(|own| t.visible.get(own))
        }) {
            return *range;
        }
        let last = u32::try_from(self.samples.len().saturating_sub(1)).unwrap_or(u32::MAX);
        SampleRange {
            start: SampleParam::at_index(0),
            end: SampleParam::at_index(last),
            start_cut: false,
            end_cut: false,
        }
    }

    /// Whole-origin visible set this member contributes to the family intersection: `trim.visible`, or
    /// `[own_range()]` for an untrimmed stroke (= "everything", since the origin spans all its samples).
    #[must_use]
    pub fn family_visible(&self) -> Vec<SampleRange> {
        match &self.trim {
            Some(t) => t.visible.clone(),
            None => vec![self.own_range()],
        }
    }

    /// Visible centreline in page px over `visible` (origin-space ranges; normally
    /// [`crate::item::Scene::visible_ranges`]). Runs the brush's own `Smoother` + `Dynamics` over **all**
    /// stored samples (warm context), maps positions through `transform`, scales half-widths by
    /// [`Affine::width_scale`], and emits per range the points strictly inside it plus points interpolated
    /// (all channels lerped at `frac`) at both ends. Ranges outside the stored window are clipped; empty
    /// `visible` → empty centreline. Writes into `out` (cleared first) to reuse its buffers.
    pub fn centreline(&self, transform: &Affine, visible: &[SampleRange], out: &mut Centreline) {
        out.points.clear();
        out.run_ends.clear();
        if visible.is_empty() || self.samples.is_empty() {
            return;
        }
        let samples = self.centre_samples(transform);
        let mut params = Vec::new();
        for range in visible {
            let Some(clipped) = self.clip(range) else {
                continue;
            };
            range_params(self.sample_offset(), samples.len(), &clipped, &mut params);
            let before = out.points.len();
            out.points.extend(
                params
                    .iter()
                    .map(|&u| centre_at(&samples, self.sample_offset(), u)),
            );
            if out.points.len() > before {
                out.run_ends
                    .push(u32::try_from(out.points.len()).unwrap_or(u32::MAX));
            }
        }
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
        let params = render::scaled_params(&self.params, transform);
        let style = StrokeStyle {
            color: params.color,
            opacity: params.opacity,
            blend: params.blend,
        };
        if visible.is_empty() {
            return render::empty_geometry(&params, style);
        }
        let kind = match params.family {
            BrushFamily::Stamp(_) => return self.build_whole(&params, transform),
            BrushFamily::Outline(kind) => kind,
        };
        if self.trim.is_none() && visible.len() == 1 && visible[0] == self.own_range() {
            return self.build_whole(&params, transform);
        }
        let ink = render::ink_points(self, transform, &params);
        let mut outline = Outline::default();
        let mut params_buf = Vec::new();
        let mut points = Vec::new();
        for range in visible {
            let Some(clipped) = self.clip(range) else {
                continue;
            };
            range_params(self.sample_offset(), ink.len(), &clipped, &mut params_buf);
            points.clear();
            points.extend(
                params_buf
                    .iter()
                    .map(|&u| render::ink_at(&ink, self.sample_offset(), u)),
            );
            let piece = render::range_outline(&params, kind, &points, &clipped);
            render::append_outline(&mut outline, &piece);
        }
        StrokeGeometry::Outline { style, outline }
    }

    /// `StrokeBuilder::build` over the samples mapped through `transform`.
    fn build_whole(&self, params: &BrushParams, transform: &Affine) -> StrokeGeometry {
        let mapped: Vec<Sample> = self
            .samples
            .iter()
            .map(|s| {
                let [x, y] = transform.apply([s.x, s.y]);
                Sample { x, y, ..*s }
            })
            .collect();
        StrokeBuilder::build(params, self.seed, &mapped)
    }

    /// Smoother memory before the stored sample at origin index `index` (see [`Trim::warm`]), for a piece cut
    /// from this stroke under `transform`; `None` when nothing precedes it or it cannot be represented.
    pub(crate) fn warm_state(&self, transform: &Affine, index: u32) -> Option<SmootherState> {
        let upto = usize::try_from(index.checked_sub(self.sample_offset())?).ok()?;
        render::warm_before(self, transform, upto)
    }

    /// Origin index of `samples[0]`.
    pub(crate) fn sample_offset(&self) -> u32 {
        self.trim.as_ref().map_or(0, |t| t.sample_offset)
    }

    /// Origin index of the last stored sample (`sample_offset` for an empty stroke).
    pub(crate) fn last_index(&self) -> u32 {
        let n = u32::try_from(self.samples.len()).unwrap_or(u32::MAX);
        self.sample_offset().saturating_add(n.saturating_sub(1))
    }

    /// True when the last stored sample is the origin's last sample (its natural pen-up point), so the
    /// builder's raw-position `last` rule applies to it.
    pub(crate) fn ends_at_origin_end(&self) -> bool {
        match &self.trim {
            None => true,
            Some(t) => t
                .visible
                .last()
                .is_some_and(|r| !r.end_cut && r.end == SampleParam::at_index(self.last_index())),
        }
    }

    /// `range` clipped to the stored window (`None` when disjoint). A clipped end becomes a cut end.
    pub(crate) fn clip(&self, range: &SampleRange) -> Option<SampleRange> {
        if self.samples.is_empty() {
            return None;
        }
        let lo = SampleParam::at_index(self.sample_offset());
        let hi = SampleParam::at_index(self.last_index());
        let (start, start_cut) = if range.start < lo {
            (lo, true)
        } else {
            (range.start, range.start_cut)
        };
        let (end, end_cut) = if range.end > hi {
            (hi, true)
        } else {
            (range.end, range.end_cut)
        };
        (start <= end).then_some(SampleRange {
            start,
            end,
            start_cut,
            end_cut,
        })
    }

    /// Params of the centreline points of `range` (clipped with [`StrokeItem::clip`]): its start, every stored
    /// sample strictly inside, and its end. Writes into `out` (cleared first).
    pub(crate) fn range_params(&self, range: &SampleRange, out: &mut Vec<SampleParam>) {
        range_params(self.sample_offset(), self.samples.len(), range, out);
    }

    /// Centre point at `u` interpolated from `points` (output of [`StrokeItem::centre_samples`]).
    pub(crate) fn centre_at(&self, points: &[CentrePoint], u: SampleParam) -> CentrePoint {
        centre_at(points, self.sample_offset(), u)
    }

    /// One page-space centre point per stored sample (smoothing, dynamics and transform applied).
    pub(crate) fn centre_samples(&self, transform: &Affine) -> Vec<CentrePoint> {
        let params = render::scaled_params(&self.params, transform);
        let offset = self.sample_offset();
        render::ink_points(self, transform, &params)
            .iter()
            .zip(offset..)
            .map(|(p, index)| CentrePoint {
                x: p.x,
                y: p.y,
                half_width: render::half_width(&params, p.size_pressure),
                param: SampleParam::at_index(index),
            })
            .collect()
    }
}

/// Intersection of two sorted, disjoint range sets (result sorted, disjoint). Empty intersections
/// (`start > end`) are dropped; a degenerate `start == end` result is kept only when one operand range is
/// itself degenerate (a dot), never when two ranges merely touch. An end of a result range is a cut when it
/// is a cut in the operand that bounds it there (equal bounds: cut wins).
#[must_use]
pub fn intersect_ranges(a: &[SampleRange], b: &[SampleRange]) -> Vec<SampleRange> {
    ranges::intersect(a, b)
}

#[cfg(test)]
mod tests;
