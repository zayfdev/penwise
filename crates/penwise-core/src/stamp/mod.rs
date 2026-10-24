//! Stamp brushes: deterministic dab placement along the path (ink-rendering.md §3).
//!
//! Owner: slice c.
//!
//! Dabs are geometry (bit-identical across targets); dab masks, coverage buffers and grain sampling are raster
//! (`raster::stamps`). Randomness comes only from [`prng`], keyed by `(seed, dab_index, channel)`, so batch and
//! streaming give identical dabs regardless of push chunking. Paper grain is the procedural [`texture`].
//!
//! Only [`StampEngine`]'s pub API, [`prng`] and [`texture`] are cross-slice contract; `spacing`/`dab` items are
//! `pub(super)` and slice c may reshape them.

mod dab;
pub mod prng;
mod spacing;
pub mod texture;

use crate::brush::{BrushParams, StampKind};
use crate::dynamics::InkPoint;
use crate::geometry::Dab;
use spacing::{DabSite, SpacingRule, SpacingState};

/// Safety cap on dabs (spray: particles) per stroke, and on dab sites (spacing crossings, including spray sites
/// that emit no particles); emission stops silently beyond it, so the walk terminates for any params.
pub const MAX_DABS_PER_STROKE: u32 = 200_000;

/// Incremental stamp engine for one stroke.
///
/// Stable vs tentative (§4.3): dabs on segments between committed points are final and appended to the stable
/// list in order; dabs on the segment from the last committed point to the pending tail are tentative and
/// rebuilt by [`StampEngine::update_tail`] from a copy of the spacing state (phase and dab index are not
/// advanced by tentative work).
///
/// Slice c may add private fields. The stamp kind is not stored: dab geometry is fully determined by `params`
/// (spray bursts by `stamp.spray`, temporal emission by `stamp.dabs_per_second`).
#[derive(Debug, Clone, PartialEq)]
pub struct StampEngine {
    params: BrushParams,
    seed: u64,
    /// Last committed point (segment start for the next commit).
    prev: Option<InkPoint>,
    /// Spacing reference diameter at `prev`.
    prev_diameter: f32,
    spacing: SpacingState,
    stable: Vec<Dab>,
    tentative: Vec<Dab>,
}

impl StampEngine {
    /// New engine for `kind` with `params` (copied) and the stroke `seed`.
    #[must_use]
    pub fn new(params: &BrushParams, kind: StampKind, seed: u64) -> Self {
        // `kind` mirrors `params.family`; every behaviour is driven by the params themselves.
        let _ = kind;
        Self {
            params: *params,
            seed,
            prev: None,
            prev_diameter: 0.0,
            spacing: SpacingState::START,
            stable: Vec::new(),
            tentative: Vec::new(),
        }
    }

    /// Commits final points: the first point ever emits the first dab at arc position 0; each following point
    /// walks the straight segment from the previous point with the spacing accumulator
    /// `dφ = Δs / max(spacing·D, 0.5 px) + Δt·dabs_per_second` (Δt clamped to ≤ 50 ms, from sample timestamps
    /// only), emitting a dab (or a spray burst) at every integer crossing of φ. Inputs lerp linearly along the
    /// segment. Exact-duplicate positions still advance the temporal term. Stops emitting at
    /// [`MAX_DABS_PER_STROKE`]. Chunk-invariant: `extend(a ++ b)` ≡ `extend(a); extend(b)`.
    pub fn extend(&mut self, committed: &[InkPoint]) {
        for point in committed {
            self.commit(point);
        }
    }

    /// Rebuilds the tentative dabs for the segment from the last committed point to `tail` (cleared when
    /// `None`) without changing committed state.
    pub fn update_tail(&mut self, tail: Option<&InkPoint>) {
        self.tentative.clear();
        let Some(tail) = tail else {
            return;
        };
        let rule = spacing_rule(&self.params);
        let d = dab::reference_diameter(&self.params, tail);
        let (params, seed, base) = (&self.params, self.seed, self.stable.len());
        let out = &mut self.tentative;
        // Work on a copy: tentative dabs never advance the committed phase or dab index.
        let mut state = self.spacing;
        match self.prev {
            None => {
                let site = DabSite::new(*tail, state.next_index, rule, d);
                emit(params, seed, &site, out, base);
            }
            Some(a) => spacing::walk_segment(
                &mut state,
                &a,
                tail,
                rule,
                [self.prev_diameter, d],
                &mut |site| emit(params, seed, &site, out, base),
            ),
        }
    }

    /// Final dabs so far, in emission order. Never shrinks or changes.
    #[must_use]
    pub fn stable(&self) -> &[Dab] {
        &self.stable
    }

    /// Current tentative dabs (replaced on every `update_tail`).
    #[must_use]
    pub fn tentative(&self) -> &[Dab] {
        &self.tentative
    }

    /// Commits `last` (if any) exactly as `extend(&[last])` would, and returns all dabs in emission order.
    /// No points ever → empty `Vec`; one point → exactly the `s = 0` dab (or burst).
    #[must_use]
    pub fn finish(mut self, last: Option<&InkPoint>) -> Vec<Dab> {
        if let Some(last) = last {
            self.commit(last);
        }
        self.stable
    }

    /// Commits one point: the very first emits the `s = 0` dab; later ones walk the segment from `prev`.
    fn commit(&mut self, point: &InkPoint) {
        let rule = spacing_rule(&self.params);
        let d = dab::reference_diameter(&self.params, point);
        let (params, seed) = (&self.params, self.seed);
        let out = &mut self.stable;
        match self.prev {
            None => {
                let site = DabSite::new(*point, self.spacing.next_index, rule, d);
                self.spacing = SpacingState {
                    phase: 0.0,
                    next_index: self.spacing.next_index.wrapping_add(1),
                };
                emit(params, seed, &site, out, 0);
            }
            Some(a) => spacing::walk_segment(
                &mut self.spacing,
                &a,
                point,
                rule,
                [self.prev_diameter, d],
                &mut |site| emit(params, seed, &site, out, 0),
            ),
        }
        self.prev = Some(*point);
        self.prev_diameter = d;
    }
}

/// Spacing rule of a brush.
fn spacing_rule(params: &BrushParams) -> SpacingRule {
    SpacingRule {
        spacing: params.stamp.spacing,
        dabs_per_second: params.stamp.dabs_per_second,
    }
}

/// Appends the dab (or spray burst) for `site` to `out`, where `base` dabs of the stroke precede `out`; never lets
/// the stroke exceed [`MAX_DABS_PER_STROKE`]. Returns whether more dabs may follow.
fn emit(params: &BrushParams, seed: u64, site: &DabSite, out: &mut Vec<Dab>, base: usize) -> bool {
    let cap = usize::try_from(MAX_DABS_PER_STROKE).unwrap_or(usize::MAX);
    let room = cap.saturating_sub(base);
    if out.len() >= room {
        return false;
    }
    if params.stamp.spray.is_some() {
        dab::spray_burst(params, site, seed, out);
    } else {
        out.push(dab::make_dab(params, site, seed));
    }
    out.truncate(room);
    out.len() < room
}

#[cfg(test)]
mod tests;
