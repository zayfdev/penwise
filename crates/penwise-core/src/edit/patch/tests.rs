use super::*;
use crate::brush::Preset;
use crate::item::{ItemId, ItemKind, TextItem};

fn text_item() -> Item {
    Item {
        id: ItemId([7; 16]),
        layer: Layer::Text,
        z: 42,
        transform: Affine::translate(3.0, 4.0),
        kind: ItemKind::Text(TextItem {
            content: "abc".into(),
            size: [10.0, 5.0],
            font_size: 12.0,
            paint: Paint {
                color: Color {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 4,
                },
                opacity: 0.5,
            },
            sources: Vec::new(),
        }),
    }
}

#[test]
fn field_codes_are_dense_and_round_trip() {
    for (i, field) in Field::ALL.into_iter().enumerate() {
        assert_eq!(usize::from(field.code()), i);
        assert_eq!(Field::from_code(field.code()), Some(field));
    }
    assert_eq!(Field::from_code(15), None);
    assert_eq!(Field::FontSize.code(), 12);
    assert_eq!(Field::FramePreset.code(), 13);
    assert_eq!(Field::FrameRoute.code(), 14);
    assert!(
        Field::FramePreset.applies_to(ItemKindTag::Frame)
            && Field::FrameRoute.applies_to(ItemKindTag::Frame)
            && !Field::FrameRoute.applies_to(ItemKindTag::Text)
    );
}

#[test]
fn applicability_matches_variant_docs() {
    let erase: Vec<Field> = Field::ALL
        .into_iter()
        .filter(|f| f.applies_to(ItemKindTag::Erase))
        .collect();
    assert_eq!(erase, vec![Field::Deleted, Field::Layer, Field::Z]);
    assert!(Field::Transform.applies_to(ItemKindTag::Fill));
    assert!(
        Field::Brush.applies_to(ItemKindTag::Stroke) && Field::Brush.applies_to(ItemKindTag::Shape)
    );
    assert!(!Field::Brush.applies_to(ItemKindTag::Text));
    assert!(
        Field::Paint.applies_to(ItemKindTag::Fill) && Field::Paint.applies_to(ItemKindTag::Text)
    );
    assert!(!Field::Polygons.applies_to(ItemKindTag::Shape));
    assert!(Field::ShapeSize.applies_to(ItemKindTag::Shape));
    assert!(
        Field::TextBox.applies_to(ItemKindTag::Text)
            && !Field::TextBox.applies_to(ItemKindTag::Shape)
    );
}

#[test]
fn fields_merge_restrict() {
    let mut a = ItemPatch {
        z: Some(1),
        text: Some("x".into()),
        ..ItemPatch::default()
    };
    assert!(ItemPatch::default().is_empty() && !a.is_empty());
    assert_eq!(a.fields(), vec![Field::Z, Field::Text]);
    let later = ItemPatch {
        z: Some(2),
        layer: Some(Layer::Agent),
        ..ItemPatch::default()
    };
    a.merge(&later);
    assert_eq!(a.z, Some(2));
    assert_eq!(a.layer, Some(Layer::Agent));
    assert_eq!(a.text.as_deref(), Some("x"));
    let only = a.restrict(&[Field::Text, Field::FontSize]);
    assert_eq!(only.fields(), vec![Field::Text]);
}

#[test]
fn capture_reads_current_values_of_applicable_fields() {
    let item = text_item();
    let patch = ItemPatch::capture(&item, true, &Field::ALL);
    assert_eq!(
        patch.fields(),
        vec![
            Field::Deleted,
            Field::Layer,
            Field::Z,
            Field::Transform,
            Field::Paint,
            Field::Text,
            Field::TextBox,
            Field::FontSize
        ]
    );
    assert_eq!(patch.deleted, Some(true));
    assert_eq!(patch.text.as_deref(), Some("abc"));
    // Writing a captured patch back is a no-op.
    let mut copy = item.clone();
    let mut deleted = true;
    patch.write_to(&mut copy, &mut deleted);
    assert_eq!(copy, item);
}

#[test]
fn field_eq_is_bitwise() {
    let a = ItemPatch {
        font_size: Some(0.0),
        transform: Some(Affine::IDENTITY),
        ..ItemPatch::default()
    };
    let b = ItemPatch {
        font_size: Some(-0.0),
        transform: Some(Affine::translate(-0.0, 0.0)),
        ..ItemPatch::default()
    };
    assert!(!a.field_eq(&b, Field::FontSize));
    assert!(!a.field_eq(&b, Field::Transform));
    assert!(a.field_eq(&a.clone(), Field::FontSize));
    assert!(a.field_eq(&b, Field::Text), "unset in both");
    assert!(
        !a.field_eq(&ItemPatch::default(), Field::FontSize),
        "set in one only"
    );

    let mut brush = Preset::Ballpoint.params();
    brush.outline.taper_start = 0.0;
    let mut signed = brush;
    signed.outline.taper_start = -0.0;
    let p = ItemPatch {
        brush: Some(brush),
        ..ItemPatch::default()
    };
    let q = ItemPatch {
        brush: Some(signed),
        ..ItemPatch::default()
    };
    assert!(!p.field_eq(&q, Field::Brush), "-0.0 differs from 0.0");
    assert!(p.field_eq(&p.clone(), Field::Brush));
    let r = ItemPatch {
        brush: Some(Preset::Marker.params()),
        ..ItemPatch::default()
    };
    assert!(!p.field_eq(&r, Field::Brush));

    let poly = |x: f32| Polygon {
        outer: vec![[x, 0.0], [1.0, 0.0], [0.0, 1.0]],
        holes: Vec::new(),
    };
    let s = ItemPatch {
        polygons: Some(vec![poly(0.0)]),
        ..ItemPatch::default()
    };
    let t = ItemPatch {
        polygons: Some(vec![poly(-0.0)]),
        ..ItemPatch::default()
    };
    assert!(!s.field_eq(&t, Field::Polygons));
    assert!(s.field_eq(&s.clone(), Field::Polygons));
}
