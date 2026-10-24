//! `UndoStack` over a multi-writer scene: gesture grain, bitwise restore, staleness (value equality), redo
//! rules, depth cap.

use super::fixtures::{RED, erase, fill, id, piece, shape, stamp_stroke, stroke, text};
use penwise_core::Preset;
use penwise_core::edit::undo::{DEFAULT_DEPTH, UndoStack};
use penwise_core::edit::{Edit, EditError, ItemPatch};
use penwise_core::item::{Affine, Item, ItemId, Layer, PageKind, Paint, Scene, ShapeKind};

fn scene_with(items: &[Item]) -> Scene {
    let mut scene = Scene::new(PageKind::Infinite);
    for item in items {
        scene.apply(&Edit::Add(item.clone())).unwrap();
    }
    scene
}

/// Live state of `ids` (tombstoned and never-added both read as `None`).
fn snapshot(scene: &Scene, ids: &[ItemId]) -> Vec<Option<Item>> {
    ids.iter().map(|&i| scene.get(i).cloned()).collect()
}

fn translate(dx: f32, dy: f32) -> Affine {
    Affine {
        e: dx,
        f: dy,
        ..Affine::IDENTITY
    }
}

fn update(id: ItemId, patch: ItemPatch) -> Edit {
    Edit::Update { id, patch }
}

fn moved(id: ItemId, dx: f32, dy: f32) -> Edit {
    update(
        id,
        ItemPatch {
            transform: Some(translate(dx, dy)),
            ..ItemPatch::default()
        },
    )
}

fn restored(id: ItemId) -> Edit {
    update(
        id,
        ItemPatch {
            deleted: Some(false),
            ..ItemPatch::default()
        },
    )
}

fn red_ballpoint() -> penwise_core::BrushParams {
    let mut params = Preset::Ballpoint.params();
    params.color = RED;
    params
}

fn undo(stack: &mut UndoStack, scene: &mut Scene) -> Vec<Edit> {
    let edits = stack.undo(scene);
    scene.apply_all(&edits).unwrap();
    edits
}

fn redo(stack: &mut UndoStack, scene: &mut Scene) -> Vec<Edit> {
    let edits = stack.redo(scene);
    scene.apply_all(&edits).unwrap();
    edits
}

/// Records `edits` as one gesture (one `apply_local` per edit), then checks undo restores the pre-gesture
/// state and redo the post-gesture state, twice. Leaves the gesture applied.
fn check_gesture(scene: &mut Scene, stack: &mut UndoStack, ids: &[ItemId], edits: &[Edit]) {
    let before = snapshot(scene, ids);
    stack.begin_gesture();
    for edit in edits {
        stack
            .apply_local(scene, std::slice::from_ref(edit))
            .unwrap();
    }
    stack.end_gesture();
    let after = snapshot(scene, ids);
    assert_ne!(before, after, "gesture changes the scene: {edits:?}");
    for _ in 0..2 {
        assert!(!undo(stack, scene).is_empty());
        assert_eq!(snapshot(scene, ids), before, "undo of {edits:?}");
        assert!(!redo(stack, scene).is_empty());
        assert_eq!(snapshot(scene, ids), after, "redo of {edits:?}");
    }
}

#[test]
fn stroke_undo_deletes_and_redo_restores() {
    let mut scene = scene_with(&[]);
    let mut stack = UndoStack::default();
    let s = stroke(id(1), 1 << 16, 10.0);
    stack
        .apply_local(&mut scene, &[Edit::Add(s.clone())])
        .unwrap();
    assert!(stack.can_undo() && !stack.can_redo());

    assert_eq!(undo(&mut stack, &mut scene), vec![Edit::Delete(id(1))]);
    assert!(scene.get(id(1)).is_none());
    assert!(!stack.can_undo() && stack.can_redo());

    assert_eq!(redo(&mut stack, &mut scene), vec![restored(id(1))]);
    assert_eq!(scene.get(id(1)), Some(&s));
    assert!(stack.can_undo() && !stack.can_redo());
    assert!(stack.redo(&scene).is_empty());
}

#[test]
fn every_gesture_kind_restores_bitwise_state() {
    let (a, b, f, s, t, c) = (id(1), id(2), id(3), id(4), id(5), id(6));
    let mut scene = scene_with(&[
        stroke(a, 1 << 16, 10.0),
        stamp_stroke(b, 2 << 16, 20.0),
        fill(f, 3 << 16),
        shape(s, 4 << 16, ShapeKind::Rect),
        text(t, 5 << 16, vec![a]),
        stroke(c, 6 << 16, 200.0),
    ]);
    let mut stack = UndoStack::default();
    let ids: Vec<ItemId> = (1..=12).map(id).collect();
    let gestures = [
        vec![Edit::Add(stroke(id(7), 7 << 16, 400.0))],
        vec![Edit::Delete(a), Edit::Add(piece(id(10), a, 1 << 16, 10.0))],
        vec![Edit::Add(erase(id(11), b, 2 << 16))],
        vec![
            moved(b, 1.0, 0.0),
            moved(f, 2.0, 3.0),
            moved(b, 5.0, 2.0),
            moved(s, -4.0, 8.5),
        ],
        vec![update(
            f,
            ItemPatch {
                paint: Some(Paint {
                    color: RED,
                    opacity: 0.25,
                }),
                ..ItemPatch::default()
            },
        )],
        vec![Edit::Add(fill(id(12), 8 << 16))],
        vec![update(
            s,
            ItemPatch {
                shape_size: Some([60.0, 20.0]),
                transform: Some(translate(310.0, 205.0)),
                shape_fill: Some(None),
                ..ItemPatch::default()
            },
        )],
        vec![update(
            c,
            ItemPatch {
                brush: Some(red_ballpoint()),
                ..ItemPatch::default()
            },
        )],
        vec![update(
            t,
            ItemPatch {
                text: Some("edited".to_owned()),
                text_box: Some([90.0, 40.0]),
                font_size: Some(30.0),
                ..ItemPatch::default()
            },
        )],
        vec![Edit::Delete(b), Edit::Delete(t)],
        vec![update(
            c,
            ItemPatch {
                layer: Some(Layer::Agent),
                z: Some(-5),
                ..ItemPatch::default()
            },
        )],
    ];
    for edits in &gestures {
        check_gesture(&mut scene, &mut stack, &ids, edits);
    }
}

#[test]
fn gesture_is_one_step_with_net_writes() {
    let mut scene = scene_with(&[]);
    let mut stack = UndoStack::default();
    let (a, b) = (id(1), id(2));
    stack.begin_gesture();
    stack
        .apply_local(&mut scene, &[Edit::Add(stroke(a, 1, 0.0))])
        .unwrap();
    stack
        .apply_local(
            &mut scene,
            &[Edit::Add(stroke(b, 2, 50.0)), moved(a, 3.0, 0.0)],
        )
        .unwrap();
    stack
        .apply_local(&mut scene, &[moved(a, 9.0, 1.0)])
        .unwrap();
    stack.end_gesture();

    let edits = undo(&mut stack, &mut scene);
    assert_eq!(edits.len(), 2, "one edit per item: {edits:?}");
    assert!(scene.get(a).is_none() && scene.get(b).is_none());
    assert!(!stack.can_undo());

    redo(&mut stack, &mut scene);
    assert_eq!(scene.get(a).unwrap().transform, translate(9.0, 1.0));
    assert!(scene.get(b).is_some());
}

#[test]
fn remote_change_to_a_field_we_wrote_is_skipped() {
    let a = id(1);
    let original = stroke(a, 1, 0.0);
    let mut scene = scene_with(std::slice::from_ref(&original));
    let mut stack = UndoStack::default();
    let local = update(
        a,
        ItemPatch {
            transform: Some(translate(10.0, 0.0)),
            brush: Some(red_ballpoint()),
            ..ItemPatch::default()
        },
    );
    stack.apply_local(&mut scene, &[local]).unwrap();
    scene.apply(&moved(a, 0.0, 77.0)).unwrap(); // another writer

    let edits = undo(&mut stack, &mut scene);
    let brush_only = ItemPatch {
        brush: Some(Preset::Ballpoint.params()),
        ..ItemPatch::default()
    };
    assert_eq!(edits, vec![update(a, brush_only)]);
    let now = scene.get(a).unwrap();
    assert_eq!(now.transform, translate(0.0, 77.0), "their edit wins");
    assert_eq!(now.kind, original.kind);
}

#[test]
fn all_stale_step_is_dropped_and_the_next_one_undone() {
    let (a, b) = (id(1), id(2));
    let mut scene = scene_with(&[stroke(a, 1, 0.0), stroke(b, 2, 40.0)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[moved(a, 1.0, 1.0)])
        .unwrap();
    stack
        .apply_local(&mut scene, &[moved(b, 2.0, 2.0)])
        .unwrap();
    scene.apply(&moved(b, 3.0, 3.0)).unwrap();

    let edits = undo(&mut stack, &mut scene);
    let back = ItemPatch {
        transform: Some(Affine::IDENTITY),
        ..ItemPatch::default()
    };
    assert_eq!(edits, vec![update(a, back)]);
    assert_eq!(scene.get(b).unwrap().transform, translate(3.0, 3.0));
    assert!(!stack.can_undo(), "the stale step is gone");

    assert_eq!(redo(&mut stack, &mut scene), vec![moved(a, 1.0, 1.0)]);
    assert!(!stack.can_redo());
}

#[test]
fn remote_delete_makes_our_writes_stale() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[moved(a, 5.0, 5.0)])
        .unwrap();
    scene.apply(&Edit::Delete(a)).unwrap();
    assert!(stack.undo(&scene).is_empty());
    assert!(!stack.can_undo());

    let c = id(2);
    stack
        .apply_local(&mut scene, &[Edit::Add(stroke(c, 2, 9.0))])
        .unwrap();
    scene.apply(&Edit::Delete(c)).unwrap();
    assert!(
        stack.undo(&scene).is_empty(),
        "their delete of our add wins"
    );
}

#[test]
fn redo_uses_current_values_and_skips_fields_changed_since() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[moved(a, 4.0, 0.0)])
        .unwrap();
    undo(&mut stack, &mut scene);
    let recolor = update(
        a,
        ItemPatch {
            brush: Some(red_ballpoint()),
            ..ItemPatch::default()
        },
    );
    scene.apply(&recolor).unwrap(); // another writer, other field

    assert_eq!(redo(&mut stack, &mut scene), vec![moved(a, 4.0, 0.0)]);
    let now = scene.get(a).unwrap();
    assert_eq!(now.transform, translate(4.0, 0.0));
    let penwise_core::item::ItemKind::Stroke(data) = &now.kind else {
        panic!("stroke");
    };
    assert_eq!(data.params, red_ballpoint(), "redo does not clobber them");

    undo(&mut stack, &mut scene);
    scene.apply(&moved(a, 0.0, -6.0)).unwrap();
    assert!(stack.redo(&scene).is_empty(), "redo target changed by them");
    assert_eq!(scene.get(a).unwrap().transform, translate(0.0, -6.0));
}

#[test]
fn new_local_step_after_undo_clears_redo() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[moved(a, 1.0, 0.0)])
        .unwrap();
    undo(&mut stack, &mut scene);
    assert!(stack.can_redo());
    stack
        .apply_local(&mut scene, &[moved(a, 2.0, 0.0)])
        .unwrap();
    assert!(!stack.can_redo());
    assert!(stack.redo(&scene).is_empty());
}

#[test]
fn net_no_op_gesture_is_discarded_and_keeps_redo() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[moved(a, 1.0, 0.0)])
        .unwrap();
    undo(&mut stack, &mut scene);
    stack.begin_gesture();
    stack
        .apply_local(&mut scene, &[moved(a, 8.0, 0.0)])
        .unwrap();
    stack
        .apply_local(&mut scene, &[moved(a, 0.0, 0.0)])
        .unwrap();
    stack.end_gesture();
    assert!(
        !stack.can_undo(),
        "no step for a gesture with no net change"
    );
    assert!(stack.can_redo());
}

#[test]
fn undo_ends_an_open_gesture() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
    let mut stack = UndoStack::default();
    stack.begin_gesture();
    stack
        .apply_local(&mut scene, &[moved(a, 1.0, 0.0)])
        .unwrap();
    assert!(stack.can_undo());
    undo(&mut stack, &mut scene);
    assert_eq!(scene.get(a).unwrap().transform, Affine::IDENTITY);
}

#[test]
fn depth_cap_drops_the_oldest_steps() {
    let a = id(1);
    for (depth, kept) in [(2, 2), (0, 1)] {
        let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
        let mut stack = UndoStack::new(depth);
        for step in 1_u8..=3 {
            stack
                .apply_local(&mut scene, &[moved(a, f32::from(step), 0.0)])
                .unwrap();
        }
        let mut undone = 0;
        while !undo(&mut stack, &mut scene).is_empty() {
            undone += 1;
        }
        assert_eq!(undone, kept, "depth {depth}");
        assert_eq!(
            scene.get(a).unwrap().transform,
            translate(f32::from(u8::try_from(3 - kept).unwrap()), 0.0)
        );
    }
}

#[test]
fn default_depth_keeps_200_steps() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 0, 0.0)]);
    let mut stack = UndoStack::default();
    let steps = i64::try_from(DEFAULT_DEPTH).unwrap() + 1;
    for z in 1..=steps {
        let set_z = ItemPatch {
            z: Some(z),
            ..ItemPatch::default()
        };
        stack.apply_local(&mut scene, &[update(a, set_z)]).unwrap();
    }
    let mut undone = 0;
    while !undo(&mut stack, &mut scene).is_empty() {
        undone += 1;
    }
    assert_eq!(undone, DEFAULT_DEPTH);
    assert_eq!(scene.get(a).unwrap().z, 1);
}

#[test]
fn failed_edit_keeps_earlier_edits_recorded() {
    let (a, ghost) = (id(1), id(99));
    let mut scene = scene_with(&[]);
    let mut stack = UndoStack::default();
    let err = stack
        .apply_local(
            &mut scene,
            &[
                Edit::Add(stroke(a, 1, 0.0)),
                moved(ghost, 1.0, 1.0),
                Edit::Add(stroke(id(2), 2, 5.0)),
            ],
        )
        .unwrap_err();
    assert_eq!(err, EditError::UnknownItem(ghost));
    assert!(scene.get(id(2)).is_none(), "later edits are not applied");
    assert_eq!(undo(&mut stack, &mut scene), vec![Edit::Delete(a)]);
}

#[test]
fn delete_gesture_undo_restores_and_redo_deletes() {
    let (a, b) = (id(1), id(2));
    let mut scene = scene_with(&[stroke(a, 1, 0.0), fill(b, 2)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[Edit::Delete(b), Edit::Delete(a)])
        .unwrap();
    assert_eq!(
        undo(&mut stack, &mut scene),
        vec![restored(a), restored(b)],
        "id order"
    );
    assert_eq!(
        redo(&mut stack, &mut scene),
        vec![Edit::Delete(a), Edit::Delete(b)]
    );
}

#[test]
fn clear_drops_everything() {
    let a = id(1);
    let mut scene = scene_with(&[stroke(a, 1, 0.0)]);
    let mut stack = UndoStack::default();
    stack
        .apply_local(&mut scene, &[moved(a, 1.0, 0.0)])
        .unwrap();
    stack
        .apply_local(&mut scene, &[moved(a, 2.0, 0.0)])
        .unwrap();
    undo(&mut stack, &mut scene);
    stack.begin_gesture();
    stack.clear();
    assert!(!stack.can_undo() && !stack.can_redo());
    assert!(stack.undo(&scene).is_empty() && stack.redo(&scene).is_empty());
}
