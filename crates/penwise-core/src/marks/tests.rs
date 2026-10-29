//! Marks: placement math, visibility, grouping, status, codec versions and effective-position tools.
#![allow(
    clippy::float_cmp,
    reason = "the placement math is exact f32 arithmetic and the tests pin the exact values"
)]

use super::*;
use crate::brush::Preset;
use crate::codec::edit::{decode_edit, encode_edit};
use crate::codec::item::{decode_item, decode_item_body, encode_item, encode_item_body};
use crate::codec::varint::Reader;
use crate::edit::{self, Edit, EditError, Field, ItemPatch, LassoParams};
use crate::item::{
    Affine, FrameItem, FramePreset, Item, ItemId, ItemKind, Layer, LayerSet, PageKind, Scene,
    StrokeItem,
};
use crate::sample::Sample;

/// Fixed id (not time ordered).
fn id(n: u8) -> ItemId {
    let mut bytes = [0_u8; 16];
    bytes[15] = n;
    ItemId(bytes)
}

/// ULID-shaped id at `ms`, so grouping can read its creation time.
fn ulid(ms: u64, n: u8) -> ItemId {
    let mut bytes = [0_u8; 16];
    bytes[..6].copy_from_slice(&ms.to_be_bytes()[2..]);
    bytes[15] = n;
    ItemId(bytes)
}

fn frame_item(item_id: ItemId, z: i64, at: [f32; 2], preset: FramePreset) -> Item {
    let (w, h) = preset.size().unwrap_or((390, 844));
    Item {
        id: item_id,
        layer: Layer::Ink,
        z,
        transform: Affine::translate(at[0], at[1]),
        kind: ItemKind::Frame(FrameItem::new("site".into(), "/".into(), preset, w, h)),
    }
}

fn samples(points: &[[f32; 2]]) -> Vec<Sample> {
    (0u32..)
        .zip(points)
        .map(|(i, p)| {
            Sample {
                x: p[0],
                y: p[1],
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 2000,
            }
            .quantize()
        })
        .collect()
}

fn binding(frame: ItemId, mode: BindingMode, key: Option<&str>, state: &str) -> Binding {
    Binding::new(
        frame,
        mode,
        FramePreset::Phone,
        key.map(ToString::to_string),
        state.to_owned(),
        [0.0, 0.0],
    )
    .unwrap()
}

fn layout() -> FrameLayout {
    let mut layout = FrameLayout::new(FramePreset::Phone);
    layout.scroll = [0.0, 120.0];
    layout.doc = [390.0, 2000.0];
    layout.viewport = [390.0, 844.0];
    layout
        .rects
        .insert("k".into(), Some(FrameRect::new(100.0, 300.0, 40.0, 20.0)));
    layout
}

fn frame_state(layout: Option<FrameLayout>) -> FrameState {
    FrameState {
        pos: [500.0, 0.0],
        size: [390.0, 844.0],
        preset: FramePreset::Phone,
        layout,
    }
}

fn query<'a>(
    binding: &'a Binding,
    frame: Option<&'a FrameState>,
    status: Option<MarkStatus>,
    show_done: bool,
) -> PlaceQuery<'a> {
    PlaceQuery {
        binding,
        frame,
        status,
        mark_state: "",
        show_done,
    }
}

#[test]
fn bound_and_area_placements_follow_the_layout() {
    let frame = frame_state(Some(layout()));
    let bound = binding(id(9), BindingMode::Bound, Some("k"), "");
    assert_eq!(
        place(&query(&bound, Some(&frame), None, true)),
        Placement {
            offset: [600.0, 300.0],
            clip: FrameRect::new(500.0, 0.0, 390.0, 844.0),
            visibility: Visibility::Visible,
        }
    );
    let area = binding(id(9), BindingMode::Area, None, "");
    assert_eq!(
        place(&query(&area, Some(&frame), None, true)).offset,
        [500.0, -120.0]
    );
    // The element moved and the page scrolled: both placements follow.
    let mut moved = layout();
    moved.scroll = [0.0, 300.0];
    moved
        .rects
        .insert("k".into(), Some(FrameRect::new(150.0, 40.0, 40.0, 20.0)));
    let frame = frame_state(Some(moved));
    assert_eq!(
        place(&query(&bound, Some(&frame), None, true)).offset,
        [650.0, 40.0]
    );
    assert_eq!(
        place(&query(&area, Some(&frame), None, true)).offset,
        [500.0, -300.0]
    );
}

#[test]
fn no_layout_falls_back_to_the_binding_anchor() {
    let frame = frame_state(None);
    let mut bound = binding(id(9), BindingMode::Bound, Some("k"), "click:menu");
    bound.anchor = [628.0, 94.0];
    let placement = place(&query(&bound, Some(&frame), None, true));
    assert_eq!(placement.offset, [628.0, 94.0]);
    assert_eq!(placement.visibility, Visibility::Visible, "state ignored");
}

#[test]
fn hidden_rules() {
    let frame = frame_state(Some(layout()));
    // Preset mismatch.
    let mut other = binding(id(9), BindingMode::Bound, Some("k"), "");
    other.preset = FramePreset::Desktop;
    assert_eq!(
        place(&query(&other, Some(&frame), None, true)).visibility,
        Visibility::Hidden
    );
    // Gone element (null) and a key that is not watched at all.
    let mut gone = layout();
    gone.rects.insert("k".into(), None);
    let gone_frame = frame_state(Some(gone));
    let bound = binding(id(9), BindingMode::Bound, Some("k"), "");
    assert_eq!(
        place(&query(&bound, Some(&gone_frame), None, true)).visibility,
        Visibility::Hidden
    );
    assert_eq!(
        place(&query(
            &bound,
            Some(&frame_state(Some(FrameLayout::new(FramePreset::Phone)))),
            None,
            true
        ))
        .visibility,
        Visibility::Hidden
    );
    // Done + show-done off.
    assert_eq!(
        place(&query(&bound, Some(&frame), Some(MarkStatus::Done), false)).visibility,
        Visibility::Hidden
    );
    assert_eq!(
        place(&query(&bound, Some(&frame), Some(MarkStatus::Done), true)).visibility,
        Visibility::Visible
    );
    // Missing frame item.
    assert_eq!(
        place(&query(&bound, None, None, true)).visibility,
        Visibility::Hidden
    );
}

#[test]
fn state_mismatch_dims_and_mark_state_applies() {
    let mut frame = frame_state(Some(layout()));
    let recorded = binding(id(9), BindingMode::Bound, Some("k"), "click:menu");
    assert_eq!(
        place(&query(&recorded, Some(&frame), None, true)).visibility,
        Visibility::Dimmed
    );
    if let Some(layout) = &mut frame.layout {
        layout.state = "click:menu".into();
    }
    assert_eq!(
        place(&query(&recorded, Some(&frame), None, true)).visibility,
        Visibility::Visible
    );
    // The stroke has no state of its own: the mark's state decides.
    let plain = binding(id(9), BindingMode::Bound, Some("k"), "");
    let query_with_mark = PlaceQuery {
        binding: &plain,
        frame: Some(&frame),
        status: None,
        mark_state: "",
        show_done: true,
    };
    assert_eq!(place(&query_with_mark).visibility, Visibility::Visible);
}

#[test]
fn layouts_merge_per_key_and_keep_unmentioned_rects() {
    let mut layouts = FrameLayouts::new();
    let update = FrameLayoutUpdate {
        preset: None,
        scroll: Some([0.0, 10.0]),
        doc: None,
        viewport: None,
        state: Some("click:menu".into()),
        rects: vec![("a".into(), Some(FrameRect::new(1.0, 2.0, 3.0, 4.0)))],
    };
    assert!(layouts.merge(id(1), FramePreset::Phone, &update));
    let again = FrameLayoutUpdate {
        scroll: Some([0.0, 10.0]),
        rects: vec![],
        ..FrameLayoutUpdate::default()
    };
    assert!(!layouts.merge(id(1), FramePreset::Phone, &again));
    let gone = FrameLayoutUpdate {
        rects: vec![("a".into(), None)],
        ..FrameLayoutUpdate::default()
    };
    assert!(layouts.merge(id(1), FramePreset::Phone, &gone));
    let stored = layouts.get(id(1)).unwrap();
    assert_eq!(stored.rect("a"), Some(None));
    assert_eq!(stored.state, "click:menu");
    assert_eq!(layouts.get(id(2)), None);
}

/// A scene with one frame at (500, 0) and one bound stroke local to (10, 10)…(20, 20).
fn bound_scene() -> (Scene, FrameLayouts, ItemId, ItemId) {
    let frame = id(1);
    let stroke = id(2);
    let mut scene = Scene::new(PageKind::Infinite);
    scene
        .apply(&Edit::Add(frame_item(
            frame,
            1,
            [500.0, 0.0],
            FramePreset::Phone,
        )))
        .unwrap();
    let item = Item {
        id: stroke,
        layer: Layer::Ink,
        z: 2,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Ballpoint.params(),
            seed: 7,
            samples: samples(&[[10.0, 10.0], [20.0, 20.0]]),
            trim: None,
            binding: Some(binding(frame, BindingMode::Bound, Some("k"), "")),
        }),
    };
    scene.apply(&Edit::Add(item)).unwrap();
    let mut layouts = FrameLayouts::new();
    layouts.merge(
        frame,
        FramePreset::Phone,
        &FrameLayoutUpdate {
            rects: vec![("k".into(), Some(FrameRect::new(100.0, 300.0, 40.0, 20.0)))],
            ..FrameLayoutUpdate::default()
        },
    );
    (scene, layouts, frame, stroke)
}

#[test]
fn presentation_places_strokes_at_their_effective_position() {
    let (scene, layouts, frame, stroke) = bound_scene();
    let view = MarkView::new(&scene, &layouts, true);
    assert!(view.has_marks());
    assert_eq!(view.keys(frame), vec!["k".to_owned()]);
    let presentation = view.presentation();
    let bounds = presentation.scene.bounds(stroke).unwrap();
    // The box includes the stroke's half-width (~2 px), the centreline starts at frame.pos + rect.xy + local.
    assert!(
        (bounds.min_x - 610.0).abs() < 3.0 && (bounds.min_y - 310.0).abs() < 3.0,
        "{bounds:?}"
    );
    // The committed scene is untouched (still binding-local, half-width included).
    let local = scene.bounds(stroke).unwrap();
    assert!((local.min_x - 10.0).abs() < 3.0, "{local:?}");
}

#[test]
fn localize_turns_moves_into_local_offsets() {
    let (mut scene, layouts, _, stroke) = bound_scene();
    let presentation = MarkView::new(&scene, &layouts, true).presentation();
    let edits = edit::transform::move_items(&presentation.scene, &[stroke], 5.0, -3.0);
    let edits = presentation.localize(edits);
    scene.apply_all(&edits).unwrap();
    let item = scene.get(stroke).unwrap();
    assert_eq!(item.transform, Affine::translate(5.0, -3.0));
    assert!(matches!(
        &item.kind,
        ItemKind::Stroke(StrokeItem {
            binding: Some(binding),
            ..
        }) if binding.mode == BindingMode::Bound && binding.key.as_deref() == Some("k")
    ));
}

#[test]
fn hit_lasso_and_erase_use_effective_positions() {
    let (scene, layouts, _, stroke) = bound_scene();
    let presentation = MarkView::new(&scene, &layouts, true).presentation();
    let effective = [615.0, 315.0];
    assert!(edit::hit_test(&presentation.scene, effective, 4.0, LayerSet::ALL).contains(&stroke));
    assert!(edit::hit_test(&presentation.scene, [15.0, 15.0], 4.0, LayerSet::ALL).is_empty());
    let polygon = [
        [600.0, 300.0],
        [640.0, 300.0],
        [640.0, 340.0],
        [600.0, 340.0],
    ];
    assert_eq!(
        edit::lasso(
            &presentation.scene,
            &polygon,
            LayerSet::ALL,
            &LassoParams::DEFAULT
        ),
        vec![stroke]
    );
    let erased = edit::erase_strokes(&presentation.scene, &[effective], 6.0, LayerSet::ALL);
    assert!(matches!(erased.as_slice(), [Edit::Delete(found)] if *found == stroke));
    assert_eq!(presentation.localize(erased), vec![Edit::Delete(stroke)]);
}

#[test]
fn marks_hide_when_done_and_show_with_show_done() {
    let (scene, layouts, _, stroke) = bound_scene();
    let mark = MarkItem {
        frame: id(1),
        preset: FramePreset::Phone,
        status: MarkStatus::Done,
        state: String::new(),
        strokes: vec![stroke],
    };
    let mut scene = scene;
    scene.apply(&Edit::Add(mark.item(id(3), 3))).unwrap();
    let hidden = MarkView::new(&scene, &layouts, false).mark_entries();
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].rect, None, "done marks hide");
    let shown = MarkView::new(&scene, &layouts, true).mark_entries();
    let rect = shown[0].rect.unwrap();
    assert!(
        (rect[0] - 610.0).abs() < 3.0 && (rect[1] - 310.0).abs() < 3.0 && rect[2] < 20.0,
        "{rect:?}"
    );
    assert!(
        MarkView::new(&scene, &layouts, false)
            .presentation()
            .scene
            .get(stroke)
            .is_none()
    );
}

fn add_spec(
    stroke_id: ItemId,
    mark_id: ItemId,
    frame: ItemId,
    key: &str,
    points: &[[f32; 2]],
    now_ms: u64,
) -> BoundStroke {
    BoundStroke {
        id: stroke_id,
        mark_id,
        frame,
        mode: BindingMode::Bound,
        key: Some(key.to_owned()),
        z: 10,
        mark_z: 11,
        layer: Layer::Ink,
        params: Preset::Ballpoint.params(),
        seed: 3,
        samples: samples(points),
        now_ms,
    }
}

/// A frame at (0, 0) with one laid-out element at viewport (100, 100).
fn grouping_scene() -> (Scene, FrameLayouts, ItemId) {
    let frame = id(1);
    let mut scene = Scene::new(PageKind::Infinite);
    scene
        .apply(&Edit::Add(frame_item(
            frame,
            1,
            [0.0, 0.0],
            FramePreset::Phone,
        )))
        .unwrap();
    let mut layouts = FrameLayouts::new();
    layouts.merge(
        frame,
        FramePreset::Phone,
        &FrameLayoutUpdate {
            rects: vec![("k".into(), Some(FrameRect::new(100.0, 100.0, 40.0, 40.0)))],
            ..FrameLayoutUpdate::default()
        },
    );
    (scene, layouts, frame)
}

fn marks_of(scene: &Scene) -> Vec<(ItemId, Vec<ItemId>)> {
    scene
        .live_ids()
        .into_iter()
        .filter_map(|mark_id| mark_of(scene, mark_id).map(|m| (mark_id, m.strokes.clone())))
        .collect()
}

#[test]
fn grouping_joins_nearby_strokes_within_the_gap() {
    let (mut scene, layouts, frame) = grouping_scene();
    let runs = [(1000_u64, [110.0_f32, 110.0]), (1500, [118.0, 118.0])];
    for (n, (ms, at)) in runs.into_iter().enumerate() {
        let spec = add_spec(
            ulid(ms, u8::try_from(n).unwrap() + 2),
            ulid(ms, 200),
            frame,
            "k",
            &[at, [at[0] + 4.0, at[1] + 4.0]],
            ms,
        );
        let edits = add_bound_stroke(&scene, &layouts, &spec).unwrap();
        scene.apply_all(&edits).unwrap();
    }
    let marks = marks_of(&scene);
    assert_eq!(marks.len(), 1, "same frame, preset, key and time");
    assert_eq!(marks[0].1, vec![ulid(1000, 2), ulid(1500, 3)]);
    let mark = mark_of(&scene, marks[0].0).unwrap();
    assert_eq!(mark.status, MarkStatus::New);
    assert_eq!(mark.frame, frame);
}

#[test]
fn grouping_does_not_join_late_or_distant_strokes() {
    let (mut scene, layouts, frame) = grouping_scene();
    let first = add_spec(
        ulid(1000, 2),
        ulid(1000, 200),
        frame,
        "k",
        &[[110.0, 110.0]],
        1000,
    );
    scene
        .apply_all(&add_bound_stroke(&scene, &layouts, &first).unwrap())
        .unwrap();
    // Too late by the clock.
    let late = add_spec(
        ulid(4000, 3),
        ulid(4000, 200),
        frame,
        "k",
        &[[110.0, 110.0]],
        4000,
    );
    assert!(matches!(
        add_bound_stroke(&scene, &layouts, &late)
            .unwrap()
            .as_slice(),
        [Edit::Add(_), Edit::Add(_)]
    ));
    // Too far away.
    let far = add_spec(
        ulid(1100, 4),
        ulid(1100, 200),
        frame,
        "k",
        &[[900.0, 900.0]],
        1100,
    );
    assert!(matches!(
        add_bound_stroke(&scene, &layouts, &far).unwrap().as_slice(),
        [Edit::Add(_), Edit::Add(_)]
    ));
}

#[test]
fn grouping_skips_done_marks_and_other_frames() {
    let (mut scene, mut layouts, frame) = grouping_scene();
    let first = add_spec(
        ulid(1000, 2),
        ulid(1000, 200),
        frame,
        "k",
        &[[110.0, 110.0]],
        1000,
    );
    scene
        .apply_all(&add_bound_stroke(&scene, &layouts, &first).unwrap())
        .unwrap();
    // Once done, a mark takes no new strokes.
    let mark_id = marks_of(&scene)[0].0;
    let done = set_mark_status(&scene, mark_id, MarkStatus::Done).unwrap();
    scene.apply_all(&done).unwrap();
    let after_done = add_spec(
        ulid(1200, 5),
        ulid(1200, 200),
        frame,
        "k",
        &[[112.0, 112.0]],
        1200,
    );
    assert!(matches!(
        add_bound_stroke(&scene, &layouts, &after_done)
            .unwrap()
            .as_slice(),
        [Edit::Add(_), Edit::Add(_)]
    ));

    // Another frame: its own mark, whatever the distance.
    let other_frame = id(8);
    scene
        .apply(&Edit::Add(frame_item(
            other_frame,
            8,
            [0.0, 900.0],
            FramePreset::Phone,
        )))
        .unwrap();
    layouts.merge(
        other_frame,
        FramePreset::Phone,
        &FrameLayoutUpdate {
            rects: vec![("k".into(), Some(FrameRect::new(100.0, 100.0, 40.0, 40.0)))],
            ..FrameLayoutUpdate::default()
        },
    );
    let other = add_spec(
        ulid(1300, 6),
        ulid(1300, 200),
        other_frame,
        "k",
        &[[112.0, 1012.0]],
        1300,
    );
    assert!(matches!(
        add_bound_stroke(&scene, &layouts, &other)
            .unwrap()
            .as_slice(),
        [Edit::Add(_), Edit::Add(_)]
    ));

    // An unknown frame is rejected.
    let ghost = add_spec(
        ulid(1400, 7),
        ulid(1400, 200),
        id(99),
        "k",
        &[[112.0, 112.0]],
        1400,
    );
    assert_eq!(
        add_bound_stroke(&scene, &layouts, &ghost).unwrap_err(),
        EditError::InvalidValue { field: "frame" }
    );
}

#[test]
fn set_status_is_lww_and_undoable() {
    let (mut scene, _, _) = grouping_scene();
    let mark = MarkItem {
        frame: id(1),
        preset: FramePreset::Phone,
        status: MarkStatus::New,
        state: String::new(),
        strokes: Vec::new(),
    };
    scene.apply(&Edit::Add(mark.item(id(4), 4))).unwrap();
    let edits = set_mark_status(&scene, id(4), MarkStatus::Seen).unwrap();
    let inverse = scene.invert(&edits[0]).unwrap();
    assert!(matches!(
        inverse,
        Edit::Update {
            patch: ItemPatch {
                mark_status: Some(MarkStatus::New),
                ..
            },
            ..
        }
    ));
    scene.apply_all(&edits).unwrap();
    assert_eq!(mark_of(&scene, id(4)).unwrap().status, MarkStatus::Seen);
    scene.apply(&inverse).unwrap();
    assert_eq!(mark_of(&scene, id(4)).unwrap().status, MarkStatus::New);
    assert!(
        set_mark_status(&scene, id(4), MarkStatus::New)
            .unwrap()
            .is_empty()
    );
    assert!(
        set_mark_status(&scene, id(9), MarkStatus::Done)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn prune_deletes_emptied_marks_and_shortens_the_rest() {
    let (mut scene, _, _) = grouping_scene();
    for (n, mark) in [
        (
            5_u8,
            MarkItem {
                frame: id(1),
                preset: FramePreset::Phone,
                status: MarkStatus::New,
                state: String::new(),
                strokes: vec![id(2)],
            },
        ),
        (
            6,
            MarkItem {
                frame: id(1),
                preset: FramePreset::Phone,
                status: MarkStatus::New,
                state: String::new(),
                strokes: vec![id(2), id(3)],
            },
        ),
    ] {
        scene
            .apply(&Edit::Add(mark.item(id(n), i64::from(n))))
            .unwrap();
    }
    let prune = prune_marks(&scene, &[Edit::Delete(id(2))]);
    assert_eq!(prune.len(), 2);
    assert!(matches!(prune[0], Edit::Delete(found) if found == id(5)));
    assert!(matches!(
        &prune[1],
        Edit::Update {
            id: found,
            patch: ItemPatch {
                mark_strokes: Some(strokes),
                ..
            },
        } if *found == id(6) && strokes == &vec![id(3)]
    ));
    assert!(
        prune_marks(
            &scene,
            &[Edit::Update {
                id: id(9),
                patch: ItemPatch {
                    deleted: Some(true),
                    ..ItemPatch::default()
                }
            }]
        )
        .is_empty()
    );
}

#[test]
fn codec_round_trips_binding_and_mark_in_items_and_edits() {
    let frame = id(1);
    let binding = Binding::new(
        frame,
        BindingMode::Bound,
        FramePreset::Phone,
        Some("v1||div:nth-of-type(2)".into()),
        "click:nav > button".into(),
        [12.5, -34.25],
    )
    .unwrap();
    let stroke = Item {
        id: id(2),
        layer: Layer::Ink,
        z: 9,
        transform: Affine::translate(1.5, 2.5),
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Pencil.params(),
            seed: 11,
            samples: samples(&[[1.0, 2.0], [3.0, 4.0]]),
            trim: None,
            binding: Some(binding.clone()),
        }),
    };
    let mut blob = Vec::new();
    encode_item(&stroke, &mut blob);
    assert_eq!(decode_item(&blob).unwrap(), stroke);
    let mut body = Vec::new();
    encode_item_body(&stroke, &mut body);
    assert_eq!(decode_item_body(&mut Reader::new(&body)).unwrap(), stroke);

    let mark = MarkItem {
        frame,
        preset: FramePreset::Desktop,
        status: MarkStatus::Reopened,
        state: "click:menu".into(),
        strokes: vec![id(2), id(3)],
    };
    let mark_item = mark.clone().item(id(4), 10);
    let mut blob = Vec::new();
    encode_item(&mark_item, &mut blob);
    assert_eq!(decode_item(&blob).unwrap(), mark_item);

    for edit in [
        Edit::Add(stroke),
        Edit::Add(mark_item),
        Edit::Update {
            id: id(4),
            patch: ItemPatch {
                mark_status: Some(MarkStatus::Seen),
                mark_strokes: Some(vec![id(2)]),
                ..ItemPatch::default()
            },
        },
    ] {
        let mut blob = Vec::new();
        encode_edit(&edit, &mut blob);
        assert_eq!(decode_edit(&blob).unwrap(), edit);
    }
}

#[test]
fn v1_item_blob_still_decodes_without_a_binding() {
    let stroke = Item {
        id: id(2),
        layer: Layer::Ink,
        z: 3,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Ballpoint.params(),
            seed: 5,
            samples: samples(&[[1.0, 1.0], [2.0, 2.0]]),
            trim: None,
            binding: None,
        }),
    };
    let mut v2 = Vec::new();
    encode_item(&stroke, &mut v2);
    // v1 wrote the same body without the trailing binding flag.
    let mut v1 = Vec::new();
    v1.extend_from_slice(&crate::codec::item::ITEM_MAGIC);
    v1.extend_from_slice(&1_u16.to_le_bytes());
    v1.extend_from_slice(&v2[6..v2.len() - 9]);
    let seal = crate::hash::fnv1a64(&v1);
    v1.extend_from_slice(&seal.to_le_bytes());
    assert_eq!(decode_item(&v1).unwrap(), stroke);
}

#[test]
fn edit_codec_versions_follow_the_fields_they_carry() {
    let versions = [
        (
            Edit::Update {
                id: id(1),
                patch: ItemPatch {
                    font_size: Some(11.0),
                    ..ItemPatch::default()
                },
            },
            1_u16,
        ),
        (
            Edit::Update {
                id: id(1),
                patch: ItemPatch {
                    frame_route: Some("/a".into()),
                    ..ItemPatch::default()
                },
            },
            2,
        ),
        (
            Edit::Update {
                id: id(1),
                patch: ItemPatch {
                    mark_status: Some(MarkStatus::New),
                    ..ItemPatch::default()
                },
            },
            3,
        ),
        (Edit::Delete(id(1)), 1),
    ];
    for (edit, version) in versions {
        let mut blob = Vec::new();
        encode_edit(&edit, &mut blob);
        assert_eq!(u16::from_le_bytes([blob[4], blob[5]]), version, "{edit:?}");
        assert_eq!(decode_edit(&blob).unwrap(), edit);
    }
    // An unbound stroke is representable at item layout v1, so it stays a v1 `Add` (byte-compatible blobs).
    let plain = Item {
        id: id(2),
        layer: Layer::Ink,
        z: 1,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Ballpoint.params(),
            seed: 1,
            samples: samples(&[[0.0, 0.0]]),
            trim: None,
            binding: None,
        }),
    };
    let mut bound = plain.clone();
    if let ItemKind::Stroke(stroke) = &mut bound.kind {
        stroke.binding = Some(binding(id(1), BindingMode::Area, None, ""));
    }
    for (item, expected) in [(plain, 1_u16), (bound, 3)] {
        let mut blob = Vec::new();
        encode_edit(&Edit::Add(item), &mut blob);
        assert_eq!(u16::from_le_bytes([blob[4], blob[5]]), expected);
    }
}

#[test]
fn field_codes_pin_the_new_mark_registers() {
    assert_eq!(Field::MarkStatus.code(), 15);
    assert_eq!(Field::MarkStrokes.code(), 16);
    assert!(Field::MarkStatus.applies_to(crate::item::ItemKindTag::Mark));
    assert!(!Field::MarkStatus.applies_to(crate::item::ItemKindTag::Stroke));
}
