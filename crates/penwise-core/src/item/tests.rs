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

fn shape_item(kind: ShapeKind, size: [f32; 2]) -> Item {
    base(ItemKind::Shape(ShapeItem {
        kind,
        size,
        brush: Preset::Fineliner.params(),
        fill: None,
    }))
}

#[test]
fn hostile_transforms_and_extents_are_rejected() {
    let mut s = stroke(Preset::Ballpoint, None);
    for t in [
        Affine::scale(3.0e38, 1.0),
        Affine::translate(0.0, -2.0e6),
        Affine::rotate(0.5).then(&Affine::scale(2.0e6, 2.0e6)),
    ] {
        s.transform = t;
        assert_eq!(s.validate(), invalid("transform"), "{t:?}");
    }
    // Coefficients in range, but the 27 px stroke maps to ~1.8e6 px.
    s.transform = Affine::scale(65_536.0, 1.0);
    assert_eq!(s.validate(), invalid("extent"));
    // Near the limit but inside it: accepted.
    s.transform = Affine::translate(1.0e6, -1.0e6);
    assert_eq!(s.validate(), Ok(()));
    // A single sample at the origin has no extent; its page half-width (1e6 × 5 px) must still be bounded.
    if let ItemKind::Stroke(st) = &mut s.kind {
        st.samples.truncate(1);
        st.samples[0].y = 0.0;
    }
    s.transform = Affine::scale(1.0e6, 1.0e6);
    assert_eq!(s.validate(), invalid("extent"));
    s.transform = Affine::scale(1000.0, 1000.0);
    assert_eq!(s.validate(), Ok(()));

    let mut fat = stroke(Preset::Ballpoint, None);
    if let ItemKind::Stroke(st) = &mut fat.kind {
        st.params.size_max = 3.0e38;
    }
    assert_eq!(fat.validate(), invalid("brush"));

    let far_fill = base(ItemKind::Fill(FillItem {
        polygons: vec![Polygon {
            outer: vec![[0.0, 0.0], [3.0e38, 0.0], [0.0, 4.0]],
            holes: Vec::new(),
        }],
        paint: Paint {
            color: Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            opacity: 1.0,
        },
    }));
    assert_eq!(far_fill.validate(), invalid("extent"));
    let far_erase = base(ItemKind::Erase(EraseItem {
        target: id(2),
        path: vec![[2.0e6, 0.0]],
        radius: 1.0,
    }));
    assert_eq!(far_erase.validate(), invalid("extent"));
}

#[test]
fn polygon_vertex_count_and_shape_size_are_capped() {
    let ngon = |n: usize| ShapeKind::Polygon {
        points: (0..n)
            .map(|k| {
                let k = f32::from(u16::try_from(k % 2).unwrap());
                [k - 0.5, 0.0]
            })
            .collect(),
    };
    assert_eq!(
        shape_item(ngon(MAX_POLYGON_POINTS), [10.0, 10.0]).validate(),
        Ok(())
    );
    assert_eq!(
        shape_item(ngon(MAX_POLYGON_POINTS + 1), [10.0, 10.0]).validate(),
        invalid("shape_kind")
    );
    assert_eq!(
        shape_item(ngon(131_072), [10.0, 10.0]).validate(),
        invalid("shape_kind")
    );
    assert_eq!(
        shape_item(ShapeKind::Rect, [3.0e38, 1.0]).validate(),
        invalid("shape_size")
    );
    assert_eq!(
        shape_item(ShapeKind::Rect, [2.0e6, 1.0]).validate(),
        invalid("shape_size")
    );
    // Vertices far outside the unit box blow the local extent up even at a legal size.
    let wild = ShapeKind::Polygon {
        points: vec![[0.0, 0.0], [1.0e6, 0.0], [0.0, 1.0]],
    };
    assert_eq!(shape_item(wild, [10.0, 10.0]).validate(), invalid("extent"));
}

#[test]
fn strokes_past_the_addressable_sample_window_are_rejected() {
    let sample = Sample {
        x: 1.0,
        y: 2.0,
        pressure: 0.5,
        tilt: 0.0,
        azimuth: 0.0,
        t_us: 0,
    }
    .quantize();
    let n = usize::try_from(MAX_STROKE_SAMPLES).unwrap();
    let mut whole = stroke(Preset::Ballpoint, None);
    if let ItemKind::Stroke(st) = &mut whole.kind {
        st.samples = vec![sample; n + 1];
    }
    assert_eq!(whole.validate(), invalid("samples"));
    if let ItemKind::Stroke(st) = &mut whole.kind {
        st.samples.pop();
    }
    assert_eq!(whole.validate(), Ok(()));

    // A piece whose stored window ends past the last addressable origin index.
    let last = MAX_STROKE_SAMPLES - 1;
    let piece = |offset: u32| {
        let own = range(offset, last);
        stroke(Preset::Ballpoint, Some(trim(offset, vec![own], 0)))
    };
    assert_eq!(piece(last - 9).validate(), Ok(())); // 10 samples ending exactly at `last`
    assert_eq!(piece(last - 8).validate(), invalid("samples"));
}
