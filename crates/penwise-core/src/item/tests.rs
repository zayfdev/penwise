use super::*;
use crate::brush::{Color, Preset};
use crate::edit::EditError;
use crate::sample::Sample;

fn id(n: u8) -> ItemId {
    ItemId([n; 16])
}

fn base(kind: ItemKind) -> Item {
    Item {
        id: id(1),
        layer: Layer::Ink,
        z: 0,
        transform: Affine::IDENTITY,
        kind,
    }
}

fn stroke(preset: Preset, trim: Option<Trim>) -> Item {
    let samples = (0..10u16)
        .map(|i| {
            Sample {
                x: f32::from(i) * 3.0,
                y: 5.0,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: u32::from(i) * 2000,
            }
            .quantize()
        })
        .collect();
    base(ItemKind::Stroke(StrokeItem {
        params: preset.params(),
        seed: 1,
        samples,
        trim,
    }))
}

fn range(a: u32, b: u32) -> SampleRange {
    SampleRange {
        start: SampleParam(a << SampleParam::FRAC_BITS),
        end: SampleParam(b << SampleParam::FRAC_BITS),
        start_cut: true,
        end_cut: true,
    }
}

fn trim(sample_offset: u32, visible: Vec<SampleRange>, own: u32) -> Trim {
    Trim {
        origin: id(9),
        sample_offset,
        visible,
        own,
    }
}

fn invalid(field: &'static str) -> Result<(), EditError> {
    Err(EditError::InvalidValue { field })
}

#[test]
fn tags_round_trip() {
    for code in 0..=255u8 {
        if let Some(tag) = ItemKindTag::from_code(code) {
            assert_eq!(tag.code(), code);
        }
        if let Some(layer) = Layer::from_code(code) {
            assert_eq!(layer.code(), code);
        }
    }
    assert_eq!(ItemKindTag::from_code(4), Some(ItemKindTag::Text));
    assert_eq!(ItemKindTag::from_code(5), None);
    assert_eq!(Layer::from_code(3), None);
    assert_eq!(
        stroke(Preset::Ballpoint, None).kind.tag(),
        ItemKindTag::Stroke
    );
}

#[test]
fn layer_sets() {
    let set = LayerSet::NONE.with(Layer::Agent).with(Layer::Text);
    assert!(set.contains(Layer::Agent) && set.contains(Layer::Text) && !set.contains(Layer::Ink));
    assert_eq!(
        set.without(Layer::Agent).without(Layer::Text),
        LayerSet::NONE
    );
    assert_eq!(
        LayerSet::INK.with(Layer::Agent).with(Layer::Text),
        LayerSet::ALL
    );
}

#[test]
fn valid_strokes_pass() {
    assert_eq!(stroke(Preset::Ballpoint, None).validate(), Ok(()));
    assert_eq!(stroke(Preset::Pencil, None).validate(), Ok(()));
    let ranges = vec![range(10, 12), range(14, 19)];
    assert_eq!(
        stroke(Preset::Ballpoint, Some(trim(10, ranges, 1))).validate(),
        Ok(())
    );
}

#[test]
fn stroke_rules() {
    let mut off_grid = stroke(Preset::Ballpoint, None);
    if let ItemKind::Stroke(s) = &mut off_grid.kind {
        s.samples[3].x += 0.001;
    }
    assert_eq!(off_grid.validate(), invalid("samples"));
    let mut empty = stroke(Preset::Ballpoint, None);
    if let ItemKind::Stroke(s) = &mut empty.kind {
        s.samples.clear();
    }
    assert_eq!(empty.validate(), invalid("samples"));
    let cases = [
        (Preset::Pencil, Some(trim(0, vec![range(0, 3)], 0))),
        (Preset::Ballpoint, Some(trim(0, vec![range(0, 3)], 1))),
        (Preset::Ballpoint, Some(trim(0, vec![range(4, 3)], 0))),
        (
            Preset::Ballpoint,
            Some(trim(0, vec![range(0, 3), range(3, 5)], 0)),
        ),
        (
            Preset::Ballpoint,
            Some(trim(0, vec![range(4, 6), range(0, 2)], 0)),
        ),
        (Preset::Ballpoint, Some(trim(0, vec![range(0, 10)], 0))),
        (Preset::Ballpoint, Some(trim(5, vec![range(2, 6)], 0))),
        (Preset::Ballpoint, Some(trim(0, Vec::new(), 0))),
    ];
    for (preset, t) in cases {
        assert_eq!(
            stroke(preset, t.clone()).validate(),
            invalid("trim"),
            "{t:?}"
        );
    }
}

#[test]
fn transform_and_kind_rules() {
    let mut s = stroke(Preset::Ballpoint, None);
    s.transform = Affine::translate(f32::INFINITY, 0.0);
    assert_eq!(s.validate(), invalid("transform"));

    let erase = |path: Vec<[f32; 2]>, radius: f32| {
        base(ItemKind::Erase(EraseItem {
            target: id(2),
            path,
            radius,
        }))
    };
    assert_eq!(erase(vec![[1.0, 1.0]], 2.0).validate(), Ok(()));
    assert_eq!(erase(Vec::new(), 2.0).validate(), invalid("path"));
    assert_eq!(erase(vec![[1.0, 1.0]], 0.0).validate(), invalid("radius"));
    let mut moved = erase(vec![[1.0, 1.0]], 2.0);
    moved.transform = Affine::translate(1.0, 0.0);
    assert_eq!(moved.validate(), invalid("transform"));

    shape_fill_text_rules();
}

fn shape_fill_text_rules() {
    let shape = |kind: ShapeKind, size: [f32; 2], preset: Preset| {
        base(ItemKind::Shape(ShapeItem {
            kind,
            size,
            brush: preset.params(),
            fill: None,
        }))
    };
    assert_eq!(
        shape(ShapeKind::Rect, [3.0, 4.0], Preset::Fineliner).validate(),
        Ok(())
    );
    assert_eq!(
        shape(ShapeKind::Rect, [3.0, 4.0], Preset::Pencil).validate(),
        invalid("brush")
    );
    assert_eq!(
        shape(ShapeKind::Rect, [-3.0, 4.0], Preset::Fineliner).validate(),
        invalid("shape_size")
    );
    let two = ShapeKind::Polygon {
        points: vec![[0.0, 0.0], [1.0, 1.0]],
    };
    assert_eq!(
        shape(two, [3.0, 4.0], Preset::Fineliner).validate(),
        invalid("shape_kind")
    );
    let negative = ShapeKind::RoundedRect { radius: -1.0 };
    assert_eq!(
        shape(negative, [3.0, 4.0], Preset::Fineliner).validate(),
        invalid("shape_kind")
    );

    let paint = Paint {
        color: Color {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        },
        opacity: 1.0,
    };
    let fill =
        |polygons: Vec<Polygon>, paint: Paint| base(ItemKind::Fill(FillItem { polygons, paint }));
    let tri = Polygon {
        outer: vec![[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]],
        holes: Vec::new(),
    };
    assert_eq!(fill(vec![tri.clone()], paint).validate(), Ok(()));
    assert_eq!(fill(Vec::new(), paint).validate(), invalid("polygons"));
    let mut holed = tri.clone();
    holed.holes.push(vec![[1.0, 1.0], [2.0, 1.0]]);
    assert_eq!(fill(vec![holed], paint).validate(), invalid("polygons"));
    let faint = Paint {
        opacity: 1.5,
        ..paint
    };
    assert_eq!(fill(vec![tri], faint).validate(), invalid("paint"));

    let text = |font_size: f32, size: [f32; 2]| {
        base(ItemKind::Text(TextItem {
            content: "hi".into(),
            size,
            font_size,
            paint,
            sources: Vec::new(),
        }))
    };
    assert_eq!(text(12.0, [10.0, 0.0]).validate(), Ok(()));
    assert_eq!(text(0.0, [10.0, 0.0]).validate(), invalid("font_size"));
    assert_eq!(text(12.0, [f32::NAN, 0.0]).validate(), invalid("text_box"));
}
