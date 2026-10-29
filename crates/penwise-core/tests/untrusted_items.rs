//! `Item::validate` as the gate for untrusted ingest: brush parameters, picture sizes and stored smoother
//! state must be rejected when out of range, never merely clamped.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::edit::{Edit, EditError, ItemPatch};
use penwise_core::item::{
    Affine, ImageItem, Item, ItemId, ItemKind, Layer, PageKind, Scene, ShapeItem, ShapeKind,
    StrokeItem,
};
use penwise_core::{BrushParams, Preset, Sample};

fn id(n: u8) -> ItemId {
    let mut bytes = [0_u8; 16];
    bytes[15] = n;
    ItemId(bytes)
}

fn samples() -> Vec<Sample> {
    (0..20_u32)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "small test indices")]
            let k = i as f32;
            Sample {
                x: 10.0 + 2.0 * k,
                y: 30.0 + k,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 2075,
            }
            .quantize()
        })
        .collect()
}

fn item(kind: ItemKind) -> Item {
    Item {
        id: id(1),
        layer: Layer::Ink,
        z: 0,
        transform: Affine::IDENTITY,
        kind,
    }
}

fn stroke_with(params: BrushParams) -> Item {
    item(ItemKind::Stroke(StrokeItem {
        params,
        seed: 1,
        samples: samples(),
        trim: None,
        binding: None,
    }))
}

fn brush_error(item: &Item) -> Option<&'static str> {
    match item.validate() {
        Err(EditError::InvalidValue { field }) => Some(field),
        Ok(()) => None,
        Err(other) => panic!("unexpected error {other:?}"),
    }
}

/// Applies `f` to a ballpoint and a pencil (outline and stamp family) and asserts both are rejected.
fn rejects(f: impl Fn(&mut BrushParams)) {
    for preset in [Preset::Ballpoint, Preset::Pencil, Preset::Spray] {
        let mut p = preset.params();
        f(&mut p);
        assert_eq!(brush_error(&stroke_with(p)), Some("brush"), "{preset:?}");
    }
}

#[test]
fn every_preset_is_valid_as_stroke_and_scaled() {
    for preset in Preset::ALL {
        assert!(preset.params().is_valid(), "{preset:?}");
        assert_eq!(
            brush_error(&stroke_with(preset.params())),
            None,
            "{preset:?}"
        );
    }
    for index in 0..Preset::ALL.len() {
        for scale in [0.25, 1.0, 3.0] {
            assert!(BrushParams::for_preset(index, 0xFF12_3456, scale).is_valid());
        }
    }
}

#[test]
fn opacity_outside_unit_range_is_rejected() {
    for opacity in [5.0, 1.000_1, -0.1, f32::NAN, f32::INFINITY] {
        rejects(|p| p.opacity = opacity);
    }
}

#[test]
fn non_finite_or_degenerate_gamma_is_rejected() {
    rejects(|p| p.size_gamma = f32::NAN);
    rejects(|p| p.size_gamma = 0.0);
    rejects(|p| p.size_gamma = -1.0);
    rejects(|p| p.dynamics.pressure.gamma = f32::NAN);
    rejects(|p| p.dynamics.pressure.gamma = f32::INFINITY);
}

#[test]
fn inverted_zero_negative_or_huge_sizes_are_rejected() {
    rejects(|p| {
        p.size_min = 20.0;
        p.size_max = 10.0;
    });
    rejects(|p| p.size_min = 0.0);
    rejects(|p| p.size_min = -3.0);
    rejects(|p| p.size_max = -3.0);
    rejects(|p| p.size_max = f32::INFINITY);
    rejects(|p| p.size_max = 2.0e6);
    rejects(|p| p.size_max = f32::NAN);
}

#[test]
fn every_other_float_of_the_brush_is_checked_too() {
    rejects(|p| p.smoothing.position.min_cutoff_hz = 0.0);
    rejects(|p| p.smoothing.pressure.beta = f32::NAN);
    rejects(|p| p.dynamics.velocity.size = 2.0);
    rejects(|p| p.dynamics.tilt.max_aspect = 0.5);
    rejects(|p| p.dynamics.tilt.side_alpha = f32::NAN);
    rejects(|p| p.outline.taper_start = -1.0);
    rejects(|p| p.outline.nib.thickness = f32::INFINITY);
    rejects(|p| p.outline.nib.fixation = 3.0);
    rejects(|p| p.stamp.hardness = 0.0);
    rejects(|p| p.stamp.spacing = 0.0);
    rejects(|p| p.stamp.jitter.angle = -0.5);
    rejects(|p| p.stamp.dabs_per_second = 1.0e9);
}

#[test]
fn spray_particle_sizes_and_count_are_checked() {
    let mut p = Preset::Spray.params();
    let spray = p.stamp.spray.as_mut().unwrap();
    spray.particles_max = 0;
    assert_eq!(brush_error(&stroke_with(p)), Some("brush"));
    let mut p = Preset::Spray.params();
    p.stamp.spray.as_mut().unwrap().particles_max = u32::MAX;
    assert_eq!(brush_error(&stroke_with(p)), Some("brush"));
    let mut p = Preset::Spray.params();
    let spray = p.stamp.spray.as_mut().unwrap();
    spray.particle_size_min = spray.particle_size_max + 1.0;
    assert_eq!(brush_error(&stroke_with(p)), Some("brush"));
}

#[test]
fn shape_brushes_are_checked_like_stroke_brushes() {
    let mut brush = Preset::Fineliner.params();
    brush.opacity = 5.0;
    let shape = item(ItemKind::Shape(ShapeItem {
        kind: ShapeKind::Rect,
        size: [40.0, 20.0],
        brush,
        fill: None,
    }));
    assert_eq!(brush_error(&shape), Some("brush"));
}

#[test]
fn bad_brush_cannot_enter_a_scene_by_add_or_update() {
    let mut scene = Scene::new(PageKind::Infinite);
    let mut bad = Preset::Ballpoint.params();
    bad.opacity = 5.0;
    assert_eq!(
        scene.apply(&Edit::Add(stroke_with(bad))),
        Err(EditError::InvalidValue { field: "brush" })
    );
    scene
        .apply(&Edit::Add(stroke_with(Preset::Ballpoint.params())))
        .unwrap();
    let patch = ItemPatch {
        brush: Some(bad),
        ..ItemPatch::default()
    };
    assert_eq!(
        scene.apply(&Edit::Update { id: id(1), patch }),
        Err(EditError::InvalidValue { field: "brush" })
    );
    assert_eq!(
        scene.get(id(1)).map(|i| i.kind.clone()),
        Some(stroke_with(Preset::Ballpoint.params()).kind),
        "a rejected update leaves the item untouched"
    );
}

#[test]
fn pictures_need_a_positive_finite_box() {
    let picture = |w: f32, h: f32| {
        item(ItemKind::Image(ImageItem {
            asset: [3; 32],
            w,
            h,
        }))
    };
    assert_eq!(brush_error(&picture(640.0, 480.0)), None);
    for (w, h) in [
        (0.0, 1.0),
        (1.0, -1.0),
        (f32::NAN, 1.0),
        (1.0, f32::INFINITY),
        (3.0e6, 1.0),
    ] {
        assert_eq!(brush_error(&picture(w, h)), Some("image_size"), "{w}x{h}");
    }
}
