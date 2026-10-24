//! Lasso selection (editing-algorithms.md §4).
//!
//! Owner: slice a.

use crate::item::{ItemId, LayerSet, Scene};

/// Lasso tuning (constants re-tuned at the M4 feel check).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LassoParams {
    /// Fraction of an item's resampled points that must lie inside (`0..=1`).
    pub threshold: f32,
    /// Arc-length resample step, page px.
    pub step: f32,
    /// Items with total arc length below this (page px) are selected iff their centroid is inside.
    pub tiny_length: f32,
}

impl LassoParams {
    /// Research defaults: 75%, 2 px step, 4 px tiny length.
    pub const DEFAULT: Self = Self {
        threshold: 0.75,
        step: 2.0,
        tiny_length: 4.0,
    };
}

/// Items selected by the lasso `polygon` (page px; auto-closed back to its first point; nonzero rule via
/// [`crate::edit::proximity::ring_contains`]), sorted by `(z, id)` ascending.
///
/// 1. Prefilter: [`Scene::query`] on the lasso's bounds, `layers` only; erase items never selected.
/// 2. Per candidate, sample points:
///    - stroke: each visible centreline run resampled every `step` (arc length, so slow pen segments do not
///      bias the count), dropping points inside the item's masks;
///    - shape: flattened page path resampled, plus every path vertex, dropping masked points;
///    - fill: outer rings resampled plus their vertices;
///    - text: the 4 corners and the centre.
/// 3. Select when `inside / total ≥ threshold`. Items whose total arc length is `< tiny_length` (dots) are
///    selected iff the mean of their samples is inside. An item with zero unmasked samples is never selected.
///
/// Fewer than 3 lasso points → empty.
#[must_use]
pub fn lasso(
    scene: &Scene,
    polygon: &[[f32; 2]],
    layers: LayerSet,
    params: &LassoParams,
) -> Vec<ItemId> {
    todo!()
}
