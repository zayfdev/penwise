//! Field-level item patches with absolute values (the `UpdateItem` payload; LWW per [`Field`]).
//!
//! Owner: slice a.

use super::EditError;
use crate::brush::{BrushParams, Color};
use crate::item::{Affine, Item, ItemKind, ItemKindTag, Layer, Paint, Polygon, ShapeKind};

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
        // Declaration order is code order (see variant docs and `Field::ALL`).
        self as u8
    }

    /// Inverse of [`Field::code`].
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.get(usize::from(code)).copied()
    }

    /// True when items of kind `tag` have this field (see variant docs; erase items have only `Deleted`,
    /// `Layer`, `Z`).
    #[must_use]
    pub fn applies_to(self, tag: ItemKindTag) -> bool {
        match self {
            Self::Deleted | Self::Layer | Self::Z => true,
            Self::Transform => tag != ItemKindTag::Erase,
            Self::Brush => matches!(tag, ItemKindTag::Stroke | ItemKindTag::Shape),
            Self::ShapeFill | Self::ShapeKind | Self::ShapeSize => tag == ItemKindTag::Shape,
            Self::Paint => matches!(tag, ItemKindTag::Fill | ItemKindTag::Text),
            Self::Polygons => tag == ItemKindTag::Fill,
            Self::Text | Self::TextBox | Self::FontSize => tag == ItemKindTag::Text,
        }
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
        Field::ALL.into_iter().filter(|f| self.is_set(*f)).collect()
    }

    /// True when no field is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !Field::ALL.into_iter().any(|f| self.is_set(f))
    }

    /// The current values of `fields` on `item` (with tombstone state `deleted`), as a patch. Fields that do
    /// not apply to the item's kind are left `None`. Used by [`crate::item::Scene::invert`] and undo.
    #[must_use]
    pub fn capture(item: &Item, deleted: bool, fields: &[Field]) -> ItemPatch {
        let mut out = ItemPatch::default();
        let tag = item.kind.tag();
        for &field in fields.iter().filter(|f| f.applies_to(tag)) {
            out.capture_field(item, deleted, field);
        }
        out
    }

    /// Overlays `later` onto `self`: every field set in `later` replaces this patch's value.
    pub fn merge(&mut self, later: &ItemPatch) {
        for field in Field::ALL {
            if later.is_set(field) {
                self.copy_field(later, field);
            }
        }
    }

    /// Copy restricted to `fields` (others `None`).
    #[must_use]
    pub fn restrict(&self, fields: &[Field]) -> ItemPatch {
        let mut out = ItemPatch::default();
        for &field in fields {
            out.copy_field(self, field);
        }
        out
    }

    /// True when `field` is set in both patches with **bitwise** equal values (floats by `to_bits`, so `-0.0`
    /// ≠ `0.0` and equal NaN bits match), or unset in both. The undo staleness test.
    #[must_use]
    pub fn field_eq(&self, other: &ItemPatch, field: Field) -> bool {
        match field {
            Field::Deleted => self.deleted == other.deleted,
            Field::Layer => self.layer == other.layer,
            Field::Z => self.z == other.z,
            Field::Transform => opt_eq(
                self.transform.as_ref(),
                other.transform.as_ref(),
                Affine::bit_eq,
            ),
            // Debug output of f32 is the shortest round-trip form (distinct for every finite value, `-0.0`
            // included), so it compares the ~40 nested brush floats bitwise without listing them by hand.
            Field::Brush => opt_eq(self.brush.as_ref(), other.brush.as_ref(), debug_eq),
            Field::ShapeFill => self.shape_fill == other.shape_fill,
            Field::Paint => opt_eq(self.paint.as_ref(), other.paint.as_ref(), |a, b| {
                a.color == b.color && a.opacity.to_bits() == b.opacity.to_bits()
            }),
            Field::Polygons => opt_eq(self.polygons.as_ref(), other.polygons.as_ref(), |a, b| {
                a.len() == b.len() && a.iter().zip(b).all(|(p, q)| polygon_bit_eq(p, q))
            }),
            Field::ShapeKind => opt_eq(
                self.shape_kind.as_ref(),
                other.shape_kind.as_ref(),
                debug_eq,
            ),
            Field::ShapeSize => opt_eq(
                self.shape_size.as_ref(),
                other.shape_size.as_ref(),
                |a, b| pair_bit_eq(*a, *b),
            ),
            Field::Text => self.text == other.text,
            Field::TextBox => opt_eq(self.text_box.as_ref(), other.text_box.as_ref(), |a, b| {
                pair_bit_eq(*a, *b)
            }),
            Field::FontSize => opt_eq(self.font_size.as_ref(), other.font_size.as_ref(), |a, b| {
                a.to_bits() == b.to_bits()
            }),
        }
    }

    /// True when `field` holds a value.
    fn is_set(&self, field: Field) -> bool {
        match field {
            Field::Deleted => self.deleted.is_some(),
            Field::Layer => self.layer.is_some(),
            Field::Z => self.z.is_some(),
            Field::Transform => self.transform.is_some(),
            Field::Brush => self.brush.is_some(),
            Field::ShapeFill => self.shape_fill.is_some(),
            Field::Paint => self.paint.is_some(),
            Field::Polygons => self.polygons.is_some(),
            Field::ShapeKind => self.shape_kind.is_some(),
            Field::ShapeSize => self.shape_size.is_some(),
            Field::Text => self.text.is_some(),
            Field::TextBox => self.text_box.is_some(),
            Field::FontSize => self.font_size.is_some(),
        }
    }

    /// Copies `from`'s value of `field` (set or unset) into `self`.
    fn copy_field(&mut self, from: &ItemPatch, field: Field) {
        match field {
            Field::Deleted => self.deleted = from.deleted,
            Field::Layer => self.layer = from.layer,
            Field::Z => self.z = from.z,
            Field::Transform => self.transform = from.transform,
            Field::Brush => self.brush = from.brush,
            Field::ShapeFill => self.shape_fill = from.shape_fill,
            Field::Paint => self.paint = from.paint,
            Field::Polygons => self.polygons.clone_from(&from.polygons),
            Field::ShapeKind => self.shape_kind.clone_from(&from.shape_kind),
            Field::ShapeSize => self.shape_size = from.shape_size,
            Field::Text => self.text.clone_from(&from.text),
            Field::TextBox => self.text_box = from.text_box,
            Field::FontSize => self.font_size = from.font_size,
        }
    }

    /// Sets `field` to `item`'s current value (caller checked [`Field::applies_to`]).
    fn capture_field(&mut self, item: &Item, deleted: bool, field: Field) {
        match (field, &item.kind) {
            (Field::Deleted, _) => self.deleted = Some(deleted),
            (Field::Layer, _) => self.layer = Some(item.layer),
            (Field::Z, _) => self.z = Some(item.z),
            (Field::Transform, _) => self.transform = Some(item.transform),
            (Field::Brush, ItemKind::Stroke(s)) => self.brush = Some(s.params),
            (Field::Brush, ItemKind::Shape(s)) => self.brush = Some(s.brush),
            (Field::ShapeFill, ItemKind::Shape(s)) => self.shape_fill = Some(s.fill),
            (Field::ShapeKind, ItemKind::Shape(s)) => self.shape_kind = Some(s.kind.clone()),
            (Field::ShapeSize, ItemKind::Shape(s)) => self.shape_size = Some(s.size),
            (Field::Paint, ItemKind::Fill(f)) => self.paint = Some(f.paint),
            (Field::Paint, ItemKind::Text(t)) => self.paint = Some(t.paint),
            (Field::Polygons, ItemKind::Fill(f)) => self.polygons = Some(f.polygons.clone()),
            (Field::Text, ItemKind::Text(t)) => self.text = Some(t.content.clone()),
            (Field::TextBox, ItemKind::Text(t)) => self.text_box = Some(t.size),
            (Field::FontSize, ItemKind::Text(t)) => self.font_size = Some(t.font_size),
            _ => {}
        }
    }

    /// `item` (with tombstone state `deleted`) after this patch, validated; the scene calls this before
    /// committing an `Update`.
    ///
    /// # Errors
    /// `FieldNotApplicable` for a field the item's kind lacks; `InvalidValue` from [`Item::validate`].
    pub(crate) fn applied(&self, item: &Item, deleted: bool) -> Result<(Item, bool), EditError> {
        let tag = item.kind.tag();
        if let Some(field) = self.fields().into_iter().find(|f| !f.applies_to(tag)) {
            return Err(EditError::FieldNotApplicable { id: item.id, field });
        }
        let mut item = item.clone();
        let mut deleted = deleted;
        self.write_to(&mut item, &mut deleted);
        item.validate()?;
        Ok((item, deleted))
    }

    /// Writes every set field into `item` / `deleted` (fields that do not apply to the kind are ignored;
    /// [`crate::item::Scene::apply`] rejects them before calling this).
    pub(crate) fn write_to(&self, item: &mut Item, deleted: &mut bool) {
        if let Some(v) = self.deleted {
            *deleted = v;
        }
        if let Some(v) = self.layer {
            item.layer = v;
        }
        if let Some(v) = self.z {
            item.z = v;
        }
        if let (Some(v), false) = (self.transform, item.kind.tag() == ItemKindTag::Erase) {
            item.transform = v;
        }
        match &mut item.kind {
            ItemKind::Stroke(s) => set(&mut s.params, self.brush.as_ref()),
            ItemKind::Shape(s) => {
                set(&mut s.brush, self.brush.as_ref());
                set(&mut s.fill, self.shape_fill.as_ref());
                set(&mut s.kind, self.shape_kind.as_ref());
                set(&mut s.size, self.shape_size.as_ref());
            }
            ItemKind::Fill(f) => {
                set(&mut f.paint, self.paint.as_ref());
                set(&mut f.polygons, self.polygons.as_ref());
            }
            ItemKind::Text(t) => {
                set(&mut t.paint, self.paint.as_ref());
                set(&mut t.content, self.text.as_ref());
                set(&mut t.size, self.text_box.as_ref());
                set(&mut t.font_size, self.font_size.as_ref());
            }
            ItemKind::Erase(_) => {}
        }
    }
}

fn set<T: Clone>(slot: &mut T, value: Option<&T>) {
    if let Some(v) = value {
        slot.clone_from(v);
    }
}

fn opt_eq<T>(a: Option<&T>, b: Option<&T>, eq: impl Fn(&T, &T) -> bool) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

fn debug_eq<T: core::fmt::Debug>(a: &T, b: &T) -> bool {
    format!("{a:?}") == format!("{b:?}")
}

fn pair_bit_eq(a: [f32; 2], b: [f32; 2]) -> bool {
    a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
}

fn ring_bit_eq(a: &[[f32; 2]], b: &[[f32; 2]]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(p, q)| pair_bit_eq(*p, *q))
}

fn polygon_bit_eq(a: &Polygon, b: &Polygon) -> bool {
    ring_bit_eq(&a.outer, &b.outer)
        && a.holes.len() == b.holes.len()
        && a.holes.iter().zip(&b.holes).all(|(p, q)| ring_bit_eq(p, q))
}

#[cfg(test)]
mod tests;
