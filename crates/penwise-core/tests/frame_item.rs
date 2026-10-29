//! Design-harness frames as first-class page items (C1): geometry, hit/lasso, erasers, preset/route edits with
//! undo, delete, and clear page.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::Preset;
use penwise_core::edit::{
    Edit, EditError, Field, ItemPatch, LassoParams, UndoStack, erase_precise, erase_strokes,
    hit_test, lasso, topmost,
};
use penwise_core::item::{
    Affine, FrameItem, FramePreset, FrameSize, FrameSpec, IdSource, Item, ItemGeometry, ItemId,
    ItemKind, Layer, LayerSet, PageKind, Scene, add_frame,
};

struct Counter(u128);

impl IdSource for Counter {
    fn next_id(&mut self) -> ItemId {
        self.0 += 1;
        ItemId(self.0.to_be_bytes())
    }
}

fn id(n: u128) -> ItemId {
    ItemId(n.to_be_bytes())
}

/// A phone frame at `(10, 20)`; its page box is `[10, 400] × [20, 864]`.
fn frame_at(x: f32, y: f32, preset: FramePreset) -> Item {
    let spec = FrameSpec::new(
        "portfolio".to_owned(),
        "/".to_owned(),
        preset,
        None,
        None,
        x,
        y,
    )
    .unwrap();
    spec.item(id(1), Layer::Ink, 0)
}

fn scene_with_frame() -> Scene {
    let mut scene = Scene::new(PageKind::Infinite);
    scene
        .apply(&Edit::Add(frame_at(10.0, 20.0, FramePreset::Phone)))
        .unwrap();
    scene
}

fn preset(size: FrameSize) -> ItemPatch {
    ItemPatch {
        frame_preset: Some(size),
        ..ItemPatch::default()
    }
}

#[test]
fn geometry_is_the_css_box_at_the_transform_position() {
    let scene = scene_with_frame();
    let Some(ItemGeometry::Frame { corners }) = scene.geometry(id(1)) else {
        panic!("frame geometry expected")
    };
    assert_eq!(
        *corners,
        [[10.0, 20.0], [400.0, 20.0], [400.0, 864.0], [10.0, 864.0]]
    );
    let b = scene.bounds(id(1)).unwrap();
    assert_eq!(
        (b.min_x, b.min_y, b.max_x, b.max_y),
        (10.0, 20.0, 400.0, 864.0)
    );

    let moved = penwise_core::edit::transform::move_items(&scene, &[id(1)], 5.0, -5.0);
    let mut scene = scene;
    scene.apply_all(&moved).unwrap();
    let b = scene.bounds(id(1)).unwrap();
    assert_eq!(
        (b.min_x, b.min_y, b.max_x, b.max_y),
        (15.0, 15.0, 405.0, 859.0)
    );
    assert_eq!(
        scene.get(id(1)).unwrap().transform,
        Affine::translate(15.0, 15.0)
    );
}

#[test]
fn taps_inside_and_near_the_edge_hit_the_frame() {
    let scene = scene_with_frame();
    assert_eq!(
        hit_test(&scene, [100.0, 100.0], 0.0, LayerSet::ALL),
        vec![id(1)]
    );
    assert_eq!(
        topmost(
            &scene,
            &hit_test(&scene, [100.0, 100.0], 0.0, LayerSet::ALL)
        ),
        Some(id(1))
    );
    assert!(hit_test(&scene, [5.0, 100.0], 2.0, LayerSet::ALL).is_empty());
    assert_eq!(
        hit_test(&scene, [8.0, 100.0], 3.0, LayerSet::ALL),
        vec![id(1)]
    );
    assert!(hit_test(&scene, [100.0, 100.0], 0.0, LayerSet::NONE).is_empty());
}

#[test]
fn a_lasso_around_the_frame_picks_it() {
    let scene = scene_with_frame();
    let polygon = [[0.0, 0.0], [420.0, 0.0], [420.0, 900.0], [0.0, 900.0]];
    assert_eq!(
        lasso(&scene, &polygon, LayerSet::ALL, &LassoParams::DEFAULT),
        vec![id(1)]
    );
    let outside = [[500.0, 500.0], [600.0, 500.0], [600.0, 600.0]];
    assert!(lasso(&scene, &outside, LayerSet::ALL, &LassoParams::DEFAULT).is_empty());
}

#[test]
fn no_eraser_touches_a_frame_now_that_it_hosts_ink() {
    let scene = scene_with_frame();
    let mut ids = Counter(100);
    let scribble = [[60.0, 100.0], [200.0, 300.0], [350.0, 700.0]];
    assert!(
        erase_precise(&scene, &scribble, 24.0, LayerSet::ALL, &mut ids).is_empty(),
        "a frame is not ink"
    );
    // Erasing over a frame's area means erasing the ink drawn on it, never deleting the live page.
    assert!(
        erase_strokes(&scene, &scribble, 24.0, LayerSet::ALL).is_empty(),
        "the item eraser leaves frames alone too"
    );
    assert!(scene.get(id(1)).is_some());
}

#[test]
fn precise_erase_still_cuts_ink_drawn_on_the_frame() {
    let mut scene = scene_with_frame();
    let stroke = Item {
        id: id(2),
        layer: Layer::Ink,
        z: 1 << 16,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(pen_line()),
    };
    scene.apply(&Edit::Add(stroke)).unwrap();
    let mut ids = Counter(100);
    let edits = erase_precise(&scene, &[[120.0, 150.0]], 8.0, LayerSet::ALL, &mut ids);
    assert!(!edits.is_empty());
    assert!(
        edits.iter().all(|e| e.target() != id(1)),
        "the frame is never a cut target"
    );
    scene.apply_all(&edits).unwrap();
    assert!(scene.get(id(1)).is_some(), "the frame survives");
}

fn pen_line() -> penwise_core::item::StrokeItem {
    use penwise_core::Sample;
    let samples = (0..20_u16)
        .map(|i| {
            Sample {
                x: 40.0 + f32::from(i) * 20.0,
                y: 150.0,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: u32::from(i) * 2000,
            }
            .quantize()
        })
        .collect();
    penwise_core::item::StrokeItem {
        params: Preset::Ballpoint.params(),
        seed: 3,
        samples,
        trim: None,
        binding: None,
    }
}

#[test]
fn preset_and_route_switch_then_undo_and_redo() {
    let mut scene = scene_with_frame();
    let mut undo = UndoStack::new(16);
    let edit = Edit::Update {
        id: id(1),
        patch: ItemPatch {
            frame_preset: Some(FrameSize {
                preset: FramePreset::Desktop,
                css_w: 1440,
                css_h: 900,
            }),
            frame_route: Some("/work".to_owned()),
            ..ItemPatch::default()
        },
    };
    undo.apply_local(&mut scene, &[edit]).unwrap();
    let Some(ItemKind::Frame(frame)) = scene.get(id(1)).map(|i| &i.kind) else {
        panic!("frame expected")
    };
    assert_eq!(frame.preset, FramePreset::Desktop);
    assert_eq!((frame.css_w, frame.css_h), (1440, 900));
    assert_eq!(frame.route, "/work");
    let b = scene.bounds(id(1)).unwrap();
    assert_eq!((b.max_x, b.max_y), (1450.0, 920.0));

    let undone = undo.undo(&scene);
    scene.apply_all(&undone).unwrap();
    let Some(ItemKind::Frame(frame)) = scene.get(id(1)).map(|i| &i.kind) else {
        panic!("frame expected")
    };
    assert_eq!(frame.preset, FramePreset::Phone);
    assert_eq!(frame.route, "/");

    let redone = undo.redo(&scene);
    scene.apply_all(&redone).unwrap();
    let Some(ItemKind::Frame(frame)) = scene.get(id(1)).map(|i| &i.kind) else {
        panic!("frame expected")
    };
    assert_eq!(frame.preset, FramePreset::Desktop);
    assert_eq!(frame.route, "/work");
}

#[test]
fn delete_and_add_are_undoable() {
    let mut scene = scene_with_frame();
    let mut undo = UndoStack::new(16);
    undo.apply_local(&mut scene, &[Edit::Delete(id(1))])
        .unwrap();
    assert!(scene.get(id(1)).is_none());
    let undone = undo.undo(&scene);
    scene.apply_all(&undone).unwrap();
    assert!(scene.get(id(1)).is_some());

    let mut fresh = Scene::new(PageKind::Infinite);
    let mut undo = UndoStack::new(16);
    let spec = FrameSpec::new(
        "site".to_owned(),
        "/".to_owned(),
        FramePreset::Tablet,
        None,
        None,
        0.0,
        0.0,
    )
    .unwrap();
    undo.apply_local(&mut fresh, &[add_frame(&spec, id(2), Layer::Ink, 0)])
        .unwrap();
    assert!(fresh.get(id(2)).is_some());
    let undone = undo.undo(&fresh);
    assert_eq!(undone, vec![Edit::Delete(id(2))]);
    fresh.apply_all(&undone).unwrap();
    assert!(fresh.get(id(2)).is_none());
}

#[test]
fn clear_page_removes_frames_because_they_are_in_the_render_order() {
    let mut scene = scene_with_frame();
    // What the tablet's Clear page does: delete every id in the render order of every layer.
    let ids = scene.render_order(LayerSet::ALL);
    assert_eq!(ids, vec![id(1)], "a frame is an ordinary page item");
    let edits: Vec<Edit> = ids.into_iter().map(Edit::Delete).collect();
    scene.apply_all(&edits).unwrap();
    assert!(scene.is_empty());
}

#[test]
fn invalid_frame_values_and_fields_are_rejected() {
    let mut scene = scene_with_frame();
    let custom_empty = preset(FrameSize {
        preset: FramePreset::Custom,
        css_w: 0,
        css_h: 100,
    });
    assert_eq!(
        scene.apply(&Edit::Update {
            id: id(1),
            patch: custom_empty
        }),
        Err(EditError::InvalidValue {
            field: "frame_size"
        })
    );
    assert_eq!(
        scene.apply(&Edit::Update {
            id: id(1),
            patch: ItemPatch {
                frame_route: Some("no-slash".to_owned()),
                ..ItemPatch::default()
            }
        }),
        Err(EditError::InvalidValue {
            field: "frame_route"
        })
    );

    let mut bad = frame_at(0.0, 0.0, FramePreset::Phone);
    bad.id = id(4);
    let ItemKind::Frame(data) = &mut bad.kind else {
        panic!("frame expected")
    };
    data.project.clear();
    assert_eq!(
        scene.apply(&Edit::Add(bad)),
        Err(EditError::InvalidValue {
            field: "frame_project"
        })
    );

    let text = Item {
        id: id(3),
        layer: Layer::Ink,
        z: 0,
        transform: Affine::IDENTITY,
        kind: ItemKind::Text(penwise_core::item::TextItem {
            content: "hi".to_owned(),
            size: [10.0, 10.0],
            font_size: 12.0,
            paint: penwise_core::item::Paint {
                color: penwise_core::Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    };
    scene.apply(&Edit::Add(text)).unwrap();
    assert_eq!(
        scene.apply(&Edit::Update {
            id: id(3),
            patch: preset(FrameSize {
                preset: FramePreset::Phone,
                css_w: 390,
                css_h: 844
            })
        }),
        Err(EditError::FieldNotApplicable {
            id: id(3),
            field: Field::FramePreset
        })
    );
}

#[test]
fn the_frame_spec_is_the_one_call_constructor() {
    let spec = FrameSpec::new(
        "portfolio".to_owned(),
        "/about?tab=1".to_owned(),
        FramePreset::Custom,
        Some(1024),
        Some(768),
        32.0,
        64.0,
    )
    .unwrap();
    let edit = add_frame(&spec, id(9), Layer::Agent, 7);
    let Edit::Add(item) = &edit else {
        panic!("add expected")
    };
    assert_eq!(item.transform, Affine::translate(32.0, 64.0));
    assert_eq!(item.layer, Layer::Agent);
    assert_eq!(item.z, 7);
    let ItemKind::Frame(frame) = &item.kind else {
        panic!("frame expected")
    };
    assert_eq!(
        frame,
        &FrameItem {
            project: "portfolio".to_owned(),
            route: "/about?tab=1".to_owned(),
            preset: FramePreset::Custom,
            css_w: 1024,
            css_h: 768,
        }
    );
}
