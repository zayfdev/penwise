//! Page items: the M2 scene model (PLAN "Data model" → Item) and the in-memory page [`Scene`].
//!
//! Owner: slice a (this file, `id.rs`, `layer.rs`, `affine.rs`, `paint.rs`, `text.rs`, `geometry.rs`,
//! `scene.rs`, `index.rs`, `validate.rs`); slice b owns `stroke.rs` and `erase.rs`; slice c owns `fill.rs`;
//! slice d owns `shape.rs`. Contract: `docs/contracts/m2-editing.md`.
//!
//! Every item kind lives in its own **local frame** mapped to page px by [`Item::transform`] (a 2×3 affine).
//! New items are created with [`Affine::IDENTITY`] and page-px data, so local = page until a transform edit.
//! Data is the source of truth; geometry ([`ItemGeometry`]) is always re-derived (PLAN "Data model").
mod affine;
mod erase;
mod fill;
mod frame;
mod geometry;
mod id;
mod image;
mod index;
mod layer;
mod paint;
mod scene;
mod shape;
mod stroke;
mod text;
mod validate;

pub use affine::Affine;
pub use erase::EraseItem;
pub use fill::{FillItem, Polygon};
pub use frame::{
    FrameItem, FramePreset, FrameSize, FrameSpec, MAX_FRAME_SIDE, MAX_PROJECT_LEN, MAX_ROUTE_LEN,
    add_frame,
};
pub use geometry::{DeriveContext, ItemGeometry, RenderBand, RenderKey};
pub use id::{IdSource, ItemId};
pub use image::ImageItem;
pub use layer::{Layer, LayerSet};
pub use paint::Paint;
pub use scene::{PageKind, Scene};
pub use shape::{ArrowHead, ShapeItem, ShapeKind};
pub use stroke::{
    CUT_LOOKAHEAD_SAMPLES, CentrePoint, Centreline, MAX_STROKE_SAMPLES, SampleParam, SampleRange,
    StrokeItem, Trim, intersect_ranges,
};
pub use text::TextItem;
pub use validate::MAX_POLYGON_POINTS;

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

/// Kind-specific data (PLAN "Data model": stroke, erase, shape, fill, text, image; design harness: frame).
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
    /// Placed picture (asset hash + box).
    Image(ImageItem),
    /// Live web page of a linked project at a size preset (design harness).
    Frame(FrameItem),
    /// A group of strokes drawn on one frame in one go, with an LWW status (design harness M2).
    Mark(crate::marks::MarkItem),
}

/// Stable kind tags (frozen: stored in the item codec; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKindTag {
    Stroke,
    Erase,
    Shape,
    Fill,
    Text,
    Image,
    Frame,
    Mark,
}

impl ItemKindTag {
    /// Byte tag: `Stroke = 0, Erase = 1, Shape = 2, Fill = 3, Text = 4, Image = 5, Frame = 6, Mark = 7`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Stroke => 0,
            Self::Erase => 1,
            Self::Shape => 2,
            Self::Fill => 3,
            Self::Text => 4,
            Self::Image => 5,
            Self::Frame => 6,
            Self::Mark => 7,
        }
    }

    /// Inverse of [`ItemKindTag::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Stroke),
            1 => Some(Self::Erase),
            2 => Some(Self::Shape),
            3 => Some(Self::Fill),
            4 => Some(Self::Text),
            5 => Some(Self::Image),
            6 => Some(Self::Frame),
            7 => Some(Self::Mark),
            _ => None,
        }
    }
}

impl ItemKind {
    /// The kind's tag.
    #[must_use]
    pub fn tag(&self) -> ItemKindTag {
        match self {
            Self::Stroke(_) => ItemKindTag::Stroke,
            Self::Erase(_) => ItemKindTag::Erase,
            Self::Shape(_) => ItemKindTag::Shape,
            Self::Fill(_) => ItemKindTag::Fill,
            Self::Text(_) => ItemKindTag::Text,
            Self::Image(_) => ItemKindTag::Image,
            Self::Frame(_) => ItemKindTag::Frame,
            Self::Mark(_) => ItemKindTag::Mark,
        }
    }
}

#[cfg(test)]
mod tests;
