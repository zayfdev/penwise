//! Field-level item patches with absolute values (the `UpdateItem` payload; LWW per [`Field`]).
//!
//! Owner: slice a.

use crate::brush::{BrushParams, Color};
use crate::item::{Affine, Item, ItemKindTag, Layer, Paint, Polygon, ShapeKind};

/// Mutable item fields. Each is one LWW register in `penwise-proto`. Codes are frozen (stored in the edit
/// codec as bit positions of the field mask; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Field {
    /// Tombstone flag (all kinds). Code 0.
    Deleted,
    /// All kinds. Code 1.
    Layer,
    /// All kinds. Code 2.
    Z,
    /// All kinds except erase. Code 3.
    Transform,
    /// Stroke `params` / shape `brush` (recolor, restyle). Code 4.
    Brush,
    /// Shape `fill`. Code 5.
    ShapeFill,
    /// Fill and text `paint`. Code 6.
    Paint,
    /// Fill `polygons` (precise erase cuts). Code 7.
    Polygons,
    /// Shape `kind` (e.g. corner radius). Code 8.
    ShapeKind,
    /// Shape `size` (handle resize). Code 9.
    ShapeSize,
    /// Text `content`. Code 10.
    Text,
    /// Text `size` (box). Code 11.
    TextBox,
    /// Text `font_size`. Code 12.
    FontSize,
}

impl Field {
    /// Every field in code order.
    pub const ALL: [Field; 13] = [
        Field::Deleted,
        Field::Layer,
        Field::Z,
        Field::Transform,
        Field::Brush,
        Field::ShapeFill,
        Field::Paint,
        Field::Polygons,
        Field::ShapeKind,
        Field::ShapeSize,
        Field::Text,
        Field::TextBox,
        Field::FontSize,
    ];

    /// Frozen code (see variant docs).
    #[must_use]
    pub fn code(self) -> u8 {
        todo!()
    }

    /// Inverse of [`Field::code`].
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        todo!()
    }

    /// True when items of kind `tag` have this field (see variant docs; erase items have only `Deleted`,
    /// `Layer`, `Z`).
    #[must_use]
    pub fn applies_to(self, tag: ItemKindTag) -> bool {
        todo!()
    }
}

/// Absolute new values for a subset of [`Field`]s (`None` = untouched). An empty patch is a valid no-op.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemPatch {
    pub deleted: Option<bool>,
    pub layer: Option<Layer>,
    pub z: Option<i64>,
    pub transform: Option<Affine>,
    pub brush: Option<BrushParams>,
    /// `Some(None)` removes a shape's fill.
    pub shape_fill: Option<Option<Color>>,
    pub paint: Option<Paint>,
    pub polygons: Option<Vec<Polygon>>,
    pub shape_kind: Option<ShapeKind>,
    pub shape_size: Option<[f32; 2]>,
    pub text: Option<String>,
    pub text_box: Option<[f32; 2]>,
    pub font_size: Option<f32>,
}

impl ItemPatch {
    /// Set fields in code order.
    #[must_use]
    pub fn fields(&self) -> Vec<Field> {
        todo!()
    }

    /// True when no field is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// The current values of `fields` on `item` (with tombstone state `deleted`), as a patch. Fields that do
    /// not apply to the item's kind are left `None`. Used by [`crate::item::Scene::invert`] and undo.
    #[must_use]
    pub fn capture(item: &Item, deleted: bool, fields: &[Field]) -> ItemPatch {
        todo!()
    }

    /// Overlays `later` onto `self`: every field set in `later` replaces this patch's value.
    pub fn merge(&mut self, later: &ItemPatch) {
        todo!()
    }

    /// Copy restricted to `fields` (others `None`).
    #[must_use]
    pub fn restrict(&self, fields: &[Field]) -> ItemPatch {
        todo!()
    }

    /// True when `field` is set in both patches with **bitwise** equal values (floats by `to_bits`, so `-0.0`
    /// ≠ `0.0` and equal NaN bits match), or unset in both. The undo staleness test.
    #[must_use]
    pub fn field_eq(&self, other: &ItemPatch, field: Field) -> bool {
        todo!()
    }
}
