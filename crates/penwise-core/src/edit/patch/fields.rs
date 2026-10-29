//! Per-field comparison, capture and write of [`ItemPatch`] values.
//!
//! Owner: slice a.

use super::{Field, ItemPatch};
use crate::item::{Affine, Item, ItemKind, ItemKindTag, Polygon};

impl ItemPatch {
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
            Field::FramePreset => self.frame_preset == other.frame_preset,
            Field::FrameRoute => self.frame_route == other.frame_route,
            Field::MarkStatus => self.mark_status == other.mark_status,
            Field::MarkStrokes => self.mark_strokes == other.mark_strokes,
            Field::FramePatch => self.frame_patch == other.frame_patch,
            Field::FrameLabel => self.frame_label == other.frame_label,
            Field::FrameGroup => self.frame_group == other.frame_group,
            Field::FrameChosen => self.frame_chosen == other.frame_chosen,
        }
    }

    /// Sets `field` to `item`'s current value (caller checked [`Field::applies_to`]).
    pub(super) fn capture_field(&mut self, item: &Item, deleted: bool, field: Field) {
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
            (Field::FramePreset, ItemKind::Frame(f)) => self.frame_preset = Some(f.size()),
            (Field::FrameRoute, ItemKind::Frame(f)) => self.frame_route = Some(f.route.clone()),
            (Field::MarkStatus, ItemKind::Mark(m)) => self.mark_status = Some(m.status),
            (Field::MarkStrokes, ItemKind::Mark(m)) => self.mark_strokes = Some(m.strokes.clone()),
            (Field::FramePatch, ItemKind::Frame(f)) => self.frame_patch = Some(f.patch.clone()),
            (Field::FrameLabel, ItemKind::Frame(f)) => self.frame_label = Some(f.label.clone()),
            (Field::FrameGroup, ItemKind::Frame(f)) => self.frame_group = Some(f.group.clone()),
            (Field::FrameChosen, ItemKind::Frame(f)) => self.frame_chosen = Some(f.chosen),
            _ => {}
        }
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
            ItemKind::Frame(f) => {
                if let Some(size) = self.frame_preset {
                    f.preset = size.preset;
                    f.css_w = size.css_w;
                    f.css_h = size.css_h;
                }
                set(&mut f.route, self.frame_route.as_ref());
                set(&mut f.patch, self.frame_patch.as_ref());
                set(&mut f.label, self.frame_label.as_ref());
                set(&mut f.group, self.frame_group.as_ref());
                set(&mut f.chosen, self.frame_chosen.as_ref());
            }
            ItemKind::Mark(m) => {
                set(&mut m.status, self.mark_status.as_ref());
                set(&mut m.strokes, self.mark_strokes.as_ref());
            }
            ItemKind::Erase(_) | ItemKind::Image(_) => {}
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
