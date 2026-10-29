//! Field-level item patches with absolute values (the `UpdateItem` payload; LWW per [`Field`]).
//!
//! Owner: slice a.

use super::EditError;
use crate::brush::{BrushParams, Color};
use crate::item::{Affine, FrameSize, Item, ItemId, ItemKindTag, Layer, Paint, Polygon, ShapeKind};

mod fields;

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
    /// Frame `preset` + CSS size (they change together). Code 13.
    FramePreset,
    /// Frame `route`. Code 14.
    FrameRoute,
    /// Mark `status` (LWW). Code 15.
    MarkStatus,
    /// Mark `strokes` (its member list). Code 16.
    MarkStrokes,
    /// Frame `patch` (the ghost/variant P1 JSON; LWW, `Some(None)` clears). Code 17.
    FramePatch,
    /// Frame `label`. Code 18.
    FrameLabel,
    /// Frame `group` (variant group id; `Some(None)` clears). Code 19.
    FrameGroup,
    /// Frame `chosen` (the owner's pick; choosing one un-chooses its group's others). Code 20.
    FrameChosen,
}

impl Field {
    /// Every field in code order.
    pub const ALL: [Field; 21] = [
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
        Field::FramePreset,
        Field::FrameRoute,
        Field::MarkStatus,
        Field::MarkStrokes,
        Field::FramePatch,
        Field::FrameLabel,
        Field::FrameGroup,
        Field::FrameChosen,
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
            Self::FramePreset
            | Self::FrameRoute
            | Self::FramePatch
            | Self::FrameLabel
            | Self::FrameGroup
            | Self::FrameChosen => tag == ItemKindTag::Frame,
            Self::MarkStatus | Self::MarkStrokes => tag == ItemKindTag::Mark,
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
    /// `Some(size)`: the frame's preset and CSS size (one register).
    pub frame_preset: Option<FrameSize>,
    pub frame_route: Option<String>,
    /// The mark's status (LWW register).
    pub mark_status: Option<crate::marks::MarkStatus>,
    /// The mark's member strokes, in addition order.
    pub mark_strokes: Option<Vec<ItemId>>,
    /// The frame's ghost/variant patch (`Some(None)` clears it).
    pub frame_patch: Option<Option<String>>,
    /// The frame's label.
    pub frame_label: Option<String>,
    /// The frame's variant group id (`Some(None)` clears it).
    pub frame_group: Option<Option<String>>,
    /// The frame's chosen flag.
    pub frame_chosen: Option<bool>,
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
            Field::FramePreset => self.frame_preset.is_some(),
            Field::FrameRoute => self.frame_route.is_some(),
            Field::MarkStatus => self.mark_status.is_some(),
            Field::MarkStrokes => self.mark_strokes.is_some(),
            Field::FramePatch => self.frame_patch.is_some(),
            Field::FrameLabel => self.frame_label.is_some(),
            Field::FrameGroup => self.frame_group.is_some(),
            Field::FrameChosen => self.frame_chosen.is_some(),
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
            Field::FramePreset => self.frame_preset = from.frame_preset,
            Field::FrameRoute => self.frame_route.clone_from(&from.frame_route),
            Field::MarkStatus => self.mark_status = from.mark_status,
            Field::MarkStrokes => self.mark_strokes.clone_from(&from.mark_strokes),
            Field::FramePatch => self.frame_patch.clone_from(&from.frame_patch),
            Field::FrameLabel => self.frame_label.clone_from(&from.frame_label),
            Field::FrameGroup => self.frame_group.clone_from(&from.frame_group),
            Field::FrameChosen => self.frame_chosen = from.frame_chosen,
        }
    }

    /// `item` (with tombstone state `deleted`) after this patch, validated; the scene calls this before
    /// committing an `Update`.
    ///
    /// # Errors
    /// `FieldNotApplicable` for a field the item's kind lacks; `InvalidValue` from [`Item::validate`].
    pub fn applied(&self, item: &Item, deleted: bool) -> Result<(Item, bool), EditError> {
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
}

#[cfg(test)]
mod tests;
