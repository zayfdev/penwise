//! The frame edit helpers: patch limits, exclusive choice, variant placement and grouping.

use super::*;
use crate::edit::{Edit, EditError};
use crate::item::{Affine, FramePreset, Item, ItemKind, Layer, MAX_LABEL_LEN, PageKind, Scene};

fn id(n: u8) -> ItemId {
    ItemId([n; 16])
}

fn base_scene() -> (Scene, ItemId) {
    let mut scene = Scene::new(PageKind::Infinite);
    let base = ItemId([1; 16]);
    let item = FrameItem::new(
        "portfolio".to_owned(),
        "/".to_owned(),
        FramePreset::Phone,
        390,
        844,
    );
    scene
        .apply(&Edit::Add(Item {
            id: base,
            layer: Layer::Ink,
            z: 1 << 16,
            transform: Affine::translate(100.0, 20.0),
            kind: ItemKind::Frame(item),
        }))
        .expect("base frame");
    (scene, base)
}

fn variant_specs() -> Vec<VariantSpec> {
    vec![
        VariantSpec {
            id: id(2),
            label: "A".to_owned(),
            patch: r#"{"css":"h1{font-size:40px}"}"#.to_owned(),
        },
        VariantSpec {
            id: id(3),
            label: "B".to_owned(),
            patch: r#"{"css":"h1{font-size:32px}"}"#.to_owned(),
        },
    ]
}

#[test]
fn variant_frames_place_and_fill_the_group() {
    let (mut scene, base) = base_scene();
    let edits = variant_frames(&scene, base, &variant_specs(), "grp", 2 << 16).expect("variants");
    assert_eq!(edits.len(), 2);
    scene.apply_all(&edits).expect("apply");
    let frame = |n: u8| {
        let ItemKind::Frame(frame) = &scene.get(id(n)).expect("frame").kind else {
            panic!("frame expected");
        };
        frame.clone()
    };
    // 100 + 390 + 40 = 530, then + 390 + 40.
    let (a, b) = (
        scene.get(id(2)).expect("a").transform,
        scene.get(id(3)).expect("b").transform,
    );
    assert!((a.e - 530.0).abs() < f32::EPSILON && (a.f - 20.0).abs() < f32::EPSILON);
    assert!((b.e - 960.0).abs() < f32::EPSILON && (b.f - 20.0).abs() < f32::EPSILON);
    for (n, label) in [(2, "A"), (3, "B")] {
        let variant = frame(n);
        assert_eq!(variant.label, label);
        assert_eq!(variant.group.as_deref(), Some("grp"));
        assert!(!variant.chosen);
        assert!(variant.patch.is_some());
        assert_eq!(variant.route, "/");
        assert_eq!(variant.project, "portfolio");
        assert_eq!(variant.css_w, 390);
    }
    assert_eq!(group_frames(&scene, "grp"), vec![id(2), id(3)]);
    assert_eq!(group_frames(&scene, "other"), Vec::<ItemId>::new());
}

#[test]
fn variant_frames_reject_bad_input() {
    let (scene, base) = base_scene();
    let long_patch = "x".repeat(MAX_PATCH_LEN + 1);
    let mut one = variant_specs();
    one[0].patch = long_patch;
    assert_eq!(
        variant_frames(&scene, base, &one, "grp", 0),
        Err(EditError::InvalidValue {
            field: "frame_patch"
        })
    );
    let mut long_label = variant_specs();
    long_label[0].label = "x".repeat(MAX_LABEL_LEN + 1);
    assert_eq!(
        variant_frames(&scene, base, &long_label, "grp", 0),
        Err(EditError::InvalidValue {
            field: "frame_label"
        })
    );
    assert_eq!(
        variant_frames(&scene, base, &variant_specs(), "", 0),
        Err(EditError::InvalidValue {
            field: "frame_group"
        })
    );
    assert_eq!(
        variant_frames(&scene, base, &[], "grp", 0),
        Err(EditError::InvalidValue {
            field: "frame_variant"
        })
    );
    let five: Vec<VariantSpec> = (0..=4)
        .map(|n| VariantSpec {
            id: id(n + 10),
            label: "x".to_owned(),
            patch: "{}".to_owned(),
        })
        .collect();
    assert_eq!(
        variant_frames(&scene, base, &five, "grp", 0),
        Err(EditError::InvalidValue {
            field: "frame_variant"
        })
    );
    assert_eq!(
        variant_frames(&scene, id(99), &variant_specs(), "grp", 0),
        Err(EditError::InvalidValue { field: "frame" })
    );
}

#[test]
fn choosing_one_frame_un_chooses_its_group() {
    let (mut scene, base) = base_scene();
    scene
        .apply_all(&variant_frames(&scene, base, &variant_specs(), "grp", 0).expect("variants"))
        .expect("apply");
    let chosen = |scene: &Scene, n: u8| frame_of(scene, id(n)).expect("frame").chosen;

    let edits = set_chosen(&scene, id(2), true).expect("choose");
    assert_eq!(edits.len(), 1, "nobody else was chosen yet");
    scene.apply_all(&edits).expect("apply");
    assert!(chosen(&scene, 2));
    assert!(!chosen(&scene, 3));
    assert_eq!(set_chosen(&scene, id(2), true).expect("no-op"), Vec::new());

    let edits = set_chosen(&scene, id(3), true).expect("choose b");
    assert_eq!(edits.len(), 2, "A is un-chosen in the same batch");
    let before: Vec<Edit> = edits
        .iter()
        .rev()
        .map(|edit| scene.invert(edit).expect("invert"))
        .collect();
    scene.apply_all(&edits).expect("apply");
    assert!(chosen(&scene, 3));
    assert!(!chosen(&scene, 2));
    scene.apply_all(&before).expect("undo");
    assert!(chosen(&scene, 2) && !chosen(&scene, 3), "one undo step");

    assert_eq!(
        set_chosen(&scene, id(1), true).expect("no group"),
        Vec::new()
    );
    assert_eq!(
        set_chosen(&scene, id(99), true).expect("unknown"),
        Vec::new()
    );
}

#[test]
fn patch_set_clear_and_limits() {
    let (mut scene, base) = base_scene();
    assert!(frame_of(&scene, base).expect("frame").patch.is_none());
    let patch = r#"{"css":"h1{color:red}"}"#.to_owned();
    let edits = set_patch(&scene, base, Some(patch.clone())).expect("set");
    assert_eq!(edits.len(), 1);
    scene.apply_all(&edits).expect("apply");
    assert_eq!(frame_of(&scene, base).expect("frame").patch, Some(patch));
    assert_eq!(
        set_patch(&scene, base, Some(r#"{"css":"h1{color:red}"}"#.to_owned())).expect("no-op"),
        Vec::new()
    );

    let cleared = set_patch(&scene, base, None).expect("clear");
    assert_eq!(cleared.len(), 1);
    scene.apply_all(&cleared).expect("apply");
    assert!(frame_of(&scene, base).expect("frame").patch.is_none());

    assert_eq!(
        set_patch(&scene, base, Some("x".repeat(MAX_PATCH_LEN + 1))),
        Err(EditError::InvalidValue {
            field: "frame_patch"
        })
    );
    let exact = "x".repeat(MAX_PATCH_LEN);
    assert!(
        set_patch(&scene, base, Some(exact)).is_ok(),
        "64 KiB is fine"
    );
    assert_eq!(
        set_patch(&scene, id(99), None).expect("unknown"),
        Vec::new()
    );
}
