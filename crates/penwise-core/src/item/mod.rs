//! Page items: the M2 scene model (PLAN "Data model" → Item) and the in-memory page [`Scene`].
//!
//! Owner: slice a (this file, `id.rs`, `layer.rs`, `affine.rs`, `paint.rs`, `text.rs`, `geometry.rs`,
//! `scene.rs`, `index.rs`); slice b owns `stroke.rs` and `erase.rs`; slice c owns `fill.rs`; slice d owns
//! `shape.rs`. Contract: `docs/contracts/m2-editing.md`.
//!
//! Every item kind lives in its own **local frame** mapped to page px by [`Item::transform`] (a 2×3 affine).
//! New items are created with [`Affine::IDENTITY`] and page-px data, so local = page until a transform edit.
//! Data is the source of truth; geometry ([`ItemGeometry`]) is always re-derived (PLAN "Data model").
#![allow(
    clippy::todo,
    clippy::missing_panics_doc,
    clippy::unused_self,
    clippy::needless_pass_by_value,
    unused_variables,
    dead_code,
    reason = "M2 contract stubs; slices remove this"
)]

mod affine;
mod erase;
mod fill;
mod geometry;
mod id;
mod index;
mod layer;
mod paint;
mod scene;
mod shape;
mod stroke;
mod text;

pub use affine::Affine;
pub use erase::EraseItem;
pub use fill::{FillItem, Polygon};
pub use geometry::{DeriveContext, ItemGeometry, RenderBand, RenderKey};
pub use id::{IdSource, ItemId};
pub use layer::{Layer, LayerSet};
pub use paint::Paint;
pub use scene::{PageKind, Scene};
pub use shape::{ArrowHead, ShapeItem, ShapeKind};
pub use stroke::{
    CONTEXT_SAMPLES, CentrePoint, Centreline, SampleParam, SampleRange, StrokeItem, Trim,
    intersect_ranges,
};
pub use text::TextItem;

/// Gap between consecutive z values handed out by [`Scene::next_z`], so a later "insert between" can pick a
/// midpoint without renumbering. z is a plain `i64` (not a fractional index): concurrent adds that pick the same
/// z are ordered by [`ItemId`] (ULID, so roughly by creation time), which is deterministic everywhere.
pub const Z_STEP: i64 = 1 << 16;

/// One page item. `id`, `kind` tag and immutable kind data never change after `Add`; `layer`, `z`,
/// `transform` and the mutable kind fields listed in [`crate::edit::Field`] change through `Update` edits
/// carrying absolute values (LWW per field in `penwise-proto`).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub layer: Layer,
    /// Global paint/hit order key together with `id`: items sort by `(z, id)` ascending (bottom → top)
    /// within each [`RenderBand`].
    pub z: i64,
    /// Local → page. Stroke widths are applied **after** the transform (see [`StrokeItem::build_geometry`],
    /// [`crate::shape::shape_geometry`]). Ignored for [`ItemKind::Erase`] (masks use their target's transform);
    /// stored as identity there.
    pub transform: Affine,
    pub kind: ItemKind,
}

/// Kind-specific data (PLAN "Data model": stroke, erase, shape, fill, text; image is later).
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// Pen stroke: brush params + seed + raw samples (+ visible-range [`Trim`] for precise-erase pieces).
    Stroke(StrokeItem),
    /// Precise-eraser mask attached to one stamp stroke or shape (destination-out over its target only).
    Erase(EraseItem),
    /// Rectangle, rounded rectangle, ellipse, line, arrow or polygon.
    Shape(ShapeItem),
    /// Fill-bucket region: polygons with holes.
    Fill(FillItem),
    /// Recognized or typed text.
    Text(TextItem),
}

/// Stable kind tags (frozen: stored in the item codec; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKindTag {
    Stroke,
    Erase,
    Shape,
    Fill,
    Text,
}

impl ItemKindTag {
    /// Byte tag: `Stroke = 0, Erase = 1, Shape = 2, Fill = 3, Text = 4`.
    #[must_use]
    pub fn code(self) -> u8 {
        todo!()
    }

    /// Inverse of [`ItemKindTag::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        todo!()
    }
}

impl ItemKind {
    /// The kind's tag.
    #[must_use]
    pub fn tag(&self) -> ItemKindTag {
        todo!()
    }
}

impl Item {
    /// Checks the invariants [`Scene::apply`] relies on; never panics. Rules:
    /// - every float finite (transform, sizes, radii, polygon points, font size, paint opacity in `0..=1`);
    /// - stroke samples on the quantization grid (`s.quantize() == s`, bitwise) and at least one sample;
    ///   [`Trim`] only on outline-family strokes, `own < visible.len()`, ranges sorted, disjoint,
    ///   `start ≤ end`, and the own range inside the stored samples (`sample_offset ..` window);
    /// - erase path non-empty, radius `> 0`, transform identity;
    /// - shape brush of the outline family; `size` components `≥ 0`; `RoundedRect` radius `≥ 0`; `Polygon`
    ///   ≥ 3 points;
    /// - fill: ≥ 1 polygon, every ring ≥ 3 points;
    /// - text: `font_size > 0`, size components `≥ 0`.
    ///
    /// # Errors
    /// [`crate::edit::EditError::InvalidValue`] naming the first offending field (fields checked in the
    /// order above).
    pub fn validate(&self) -> Result<(), crate::edit::EditError> {
        todo!()
    }
}
