use penwise_core::brush::Preset;
use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, ItemKind, Layer, LayerSet, ShapeKind};
use penwise_proto::{Background, DeviceId, PageId, PageKind};

use super::{
    SAMPLE_FLOATS, SceneHandle, ShapeSpec, StrokeSpec, decode_edits, encode_edits,
    handle_from_code, shape_kind,
};
use crate::render_floats::{item_floats, kind};
use crate::sync_core::{NewPage, SyncCore};

fn setup() -> (SyncCore, SceneHandle, PageId) {
    let mut core = SyncCore::new(DeviceId::from_parts(1_000, 1), 3);
    let notebook = core
        .create_notebook(None, "nb", 0, 5_000)
        .expect("notebook");
    let page = core
        .create_page(
            None,
            NewPage {
                notebook,
                title: "p".to_owned(),
                position: 0,
                kind: PageKind::Fixed {
                    width: 1000.0,
                    height: 1000.0,
                },
                background: Background::Blank,
            },
            5_000,
        )
        .expect("page");
    let scene = SceneHandle::open(&core, page, 9).expect("open");
    (core, scene, page)
}

/// Commits `edits` to the core and checks the scene still mirrors the replica.
fn commit(core: &mut SyncCore, scene: &SceneHandle, edits: &[Edit]) {
    core.commit_edits(scene.page(), edits, 6_000)
        .expect("commit");
    let replica = core.replica().page(scene.page()).expect("page").scene();
    let ids = replica.render_order(LayerSet::ALL);
    assert_eq!(scene.render_order(LayerSet::ALL), ids);
    for id in ids {
        assert_eq!(item_floats(scene.scene(), id), item_floats(replica, id));
    }
}

fn stroke(scene: &mut SceneHandle, preset: usize, y: f32) -> Vec<Edit> {
    let id = scene.new_item_id(5_500);
    let (mut samples, mut times) = (Vec::new(), Vec::new());
    for i in 0..12_i32 {
        let x = 20.0 + f32::from(u8::try_from(i).expect("small")) * 8.0;
        samples.extend([x, y, 0.6, 0.0, 0.0]);
        times.push(i * 8_000);
    }
    scene
        .add_stroke(&StrokeSpec {
            id,
            preset,
            argb: 0xFF10_2030,
            scale: 1.0,
            seed: 11,
            samples: &samples,
            times: &times,
        })
        .expect("stroke")
}

#[test]
fn every_preset_commits_and_renders() {
    let (mut core, mut scene, _) = setup();
    for preset in 0..Preset::ALL.len() {
        let edits = stroke(
            &mut scene,
            preset,
            50.0 + 20.0 * f32::from(u8::try_from(preset).expect("small")),
        );
        commit(&mut core, &scene, &edits);
    }
    for id in scene.render_order(LayerSet::ALL) {
        let floats = item_floats(scene.scene(), id).expect("drawable");
        assert_eq!(
            part_kinds(&floats).len(),
            1,
            "one part per untouched stroke"
        );
    }
}

#[test]
fn stroke_arrays_must_agree() {
    let (_, mut scene, _) = setup();
    let id = scene.new_item_id(1);
    let spec = StrokeSpec {
        id,
        preset: 0,
        argb: 0xFF00_0000,
        scale: 1.0,
        seed: 1,
        samples: &[0.0; SAMPLE_FLOATS * 2],
        times: &[0],
    };
    assert!(scene.add_stroke(&spec).is_err());
    assert!(
        scene
            .add_stroke(&StrokeSpec {
                samples: &[],
                times: &[],
                ..spec
            })
            .is_err()
    );
}

#[test]
fn undo_and_redo_are_ordinary_edits_the_mirror_follows() {
    let (mut core, mut scene, _) = setup();
    scene.begin_gesture();
    let edits = stroke(&mut scene, 0, 60.0);
    scene.end_gesture();
    commit(&mut core, &scene, &edits);
    assert!(scene.can_undo() && !scene.can_redo());
    let undo = scene.undo().expect("undo");
    assert!(matches!(undo.as_slice(), [Edit::Delete(_)]));
    commit(&mut core, &scene, &undo);
    assert!(scene.render_order(LayerSet::ALL).is_empty());
    assert!(scene.can_redo());
    let redo = scene.redo().expect("redo");
    commit(&mut core, &scene, &redo);
    assert_eq!(scene.render_order(LayerSet::ALL).len(), 1);
    assert!(scene.undo().is_ok());
    let none = SceneHandle::open(&core, scene.page(), 1)
        .expect("open")
        .undo()
        .expect("empty");
    assert!(none.is_empty(), "a fresh handle has no history");
}

#[test]
fn precise_erase_is_one_undo_step_with_adds_before_deletes() {
    let (mut core, mut scene, _) = setup();
    let first = stroke(&mut scene, 0, 100.0);
    commit(&mut core, &scene, &first);
    scene.begin_gesture();
    let edits = scene
        .erase(&[[60.0, 80.0], [60.0, 120.0]], 5.0, true, LayerSet::ALL)
        .expect("erase");
    scene.end_gesture();
    assert!(edits.len() > 1);
    let first_delete = edits
        .iter()
        .position(|e| matches!(e, Edit::Delete(_)))
        .expect("deletes the original");
    assert!(
        edits[first_delete..]
            .iter()
            .all(|e| matches!(e, Edit::Delete(_))),
        "deletes last"
    );
    commit(&mut core, &scene, &edits);
    let undo = scene.undo().expect("undo");
    commit(&mut core, &scene, &undo);
    assert_eq!(
        scene.render_order(LayerSet::ALL).len(),
        1,
        "one step restores the original"
    );
}

#[test]
fn shapes_resize_and_report_masks_as_parts() {
    let (mut core, mut scene, _) = setup();
    let id = scene.new_item_id(1);
    let edits = scene
        .add_shape(&ShapeSpec {
            id,
            kind: ShapeKind::Rect,
            start: [100.0, 100.0],
            end: [200.0, 180.0],
            constrain: false,
            brush: penwise_core::BrushParams::for_preset(0, 0xFF00_0000, 1.0),
            fill: Some(penwise_core::Color::rgb(200, 30, 30)),
        })
        .expect("shape");
    commit(&mut core, &scene, &edits);
    let floats = item_floats(scene.scene(), id).expect("drawable");
    assert_eq!(
        part_kinds(&floats),
        [kind::OUTLINE, kind::OUTLINE],
        "fill then outline"
    );
    let resize = scene
        .resize_shape(
            id,
            handle_from_code(4).expect("bottom right"),
            [260.0, 240.0],
            false,
        )
        .expect("resize");
    assert_eq!(resize.len(), 1);
    commit(&mut core, &scene, &resize);
    // A precise erase across a shape adds a mask item; the shape's array then carries a MASK part.
    let erase = scene
        .erase(&[[150.0, 90.0], [150.0, 260.0]], 6.0, true, LayerSet::ALL)
        .expect("erase");
    assert!(
        erase
            .iter()
            .any(|e| matches!(e, Edit::Add(item) if matches!(item.kind, ItemKind::Erase(_))))
    );
    commit(&mut core, &scene, &erase);
    let floats = item_floats(scene.scene(), id).expect("drawable");
    assert!(part_kinds(&floats).contains(&kind::MASK));
}

/// Kinds of the parts in an `item_floats` array.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "counts stored as exact small floats"
)]
fn part_kinds(floats: &[f32]) -> Vec<f32> {
    let mut kinds = Vec::new();
    let mut at = 2;
    for _ in 0..floats[1] as usize {
        kinds.push(floats[at]);
        at += 2 + floats[at + 1] as usize;
    }
    assert_eq!(at, floats.len(), "parts tile the array exactly");
    kinds
}

#[test]
fn lasso_move_layer_and_delete_work_on_selections() {
    let (mut core, mut scene, _) = setup();
    for y in [100.0, 300.0] {
        let edits = stroke(&mut scene, 0, y);
        commit(&mut core, &scene, &edits);
    }
    let polygon = [[0.0, 50.0], [400.0, 50.0], [400.0, 150.0], [0.0, 150.0]];
    let picked = scene.lasso(&polygon, LayerSet::ALL);
    assert_eq!(picked.len(), 1);
    let moved = scene.move_items(&picked, 10.0, 0.0).expect("move");
    commit(&mut core, &scene, &moved);
    let layered = scene.set_layer(&picked, Layer::Agent).expect("layer");
    commit(&mut core, &scene, &layered);
    assert!(scene.lasso(&polygon, LayerSet::INK).is_empty());
    assert_eq!(scene.hit_test([60.0, 100.0], 6.0, LayerSet::ALL).len(), 1);
    let gone = scene.delete_items(&picked).expect("delete");
    commit(&mut core, &scene, &gone);
    assert_eq!(scene.render_order(LayerSet::ALL).len(), 1);
}

#[test]
fn edit_lists_round_trip_and_reject_garbage() {
    let (_, mut scene, _) = setup();
    let edits = stroke(&mut scene, 0, 10.0);
    let blob = encode_edits(&edits);
    assert_eq!(decode_edits(&blob).expect("decodes"), edits);
    assert_eq!(decode_edits(&encode_edits(&[])).expect("empty"), Vec::new());
    assert!(decode_edits(&blob[..blob.len() - 1]).is_err());
    let mut trailing = blob;
    trailing.push(0);
    assert!(decode_edits(&trailing).is_err());
    assert!(decode_edits(&[]).is_err());
}

#[test]
fn code_helpers_reject_unknown_values() {
    assert!(shape_kind(99, 0.0).is_err());
    assert!(shape_kind(4, 7.0).is_err());
    assert!(matches!(shape_kind(5, 6.0), Ok(ShapeKind::Polygon { points }) if points.len() == 6));
    assert!(handle_from_code(12).is_err());
    assert!(handle_from_code(-1).is_err());
}

#[test]
fn opening_an_unknown_page_fails_and_reload_empties_a_vanished_page() {
    let (core, mut scene, _) = setup();
    assert!(SceneHandle::open(&core, PageId::from_parts(1, 1), 0).is_err());
    let other = SyncCore::new(DeviceId::from_parts(1_000, 2), 1);
    scene.reload(&other).expect("tolerates a missing page");
    assert!(scene.render_order(LayerSet::ALL).is_empty());
}

#[test]
fn item_floats_skip_unknown_items() {
    let (_, scene, _) = setup();
    assert!(item_floats(scene.scene(), ItemId([1; 16])).is_none());
}

fn dirty_ids(blob: &[u8]) -> (bool, Vec<ItemId>) {
    let ids = blob[1..]
        .chunks_exact(16)
        .map(|c| ItemId(c.try_into().expect("16 bytes")))
        .collect();
    (blob[0] == 1, ids)
}

#[test]
fn dirty_ids_track_what_the_renderer_must_repaint() {
    let (mut core, mut scene, _) = setup();
    assert!(
        dirty_ids(&scene.take_dirty()).0,
        "a fresh scene is all dirty"
    );
    assert_eq!(dirty_ids(&scene.take_dirty()), (false, Vec::new()));
    let id = scene.new_item_id(1);
    let edits = scene
        .add_shape(&ShapeSpec {
            id,
            kind: ShapeKind::Rect,
            start: [100.0, 100.0],
            end: [200.0, 180.0],
            constrain: false,
            brush: penwise_core::BrushParams::for_preset(0, 0xFF00_0000, 1.0),
            fill: None,
        })
        .expect("shape");
    commit(&mut core, &scene, &edits);
    assert_eq!(dirty_ids(&scene.take_dirty()), (false, vec![id]));
    // Erasing a shape adds a mask item; the shape is what changes visually.
    let erase = scene
        .erase(&[[150.0, 90.0], [150.0, 260.0]], 6.0, true, LayerSet::ALL)
        .expect("erase");
    let (all, ids) = dirty_ids(&scene.take_dirty());
    assert!(!all && ids.contains(&id), "the erased target is dirty");
    // Undoing the mask deletes the erase item; its target is dirty again.
    commit(&mut core, &scene, &erase);
    let undo = scene.undo().expect("undo");
    commit(&mut core, &scene, &undo);
    assert!(dirty_ids(&scene.take_dirty()).1.contains(&id));
    scene.reload(&core).expect("reload");
    assert!(dirty_ids(&scene.take_dirty()).0);
}

#[test]
fn a_stamp_brush_shape_falls_back_to_an_outline_brush() {
    let (mut core, mut scene, _) = setup();
    let stamp = Preset::ALL
        .iter()
        .position(|p| matches!(p, Preset::Pencil))
        .expect("pencil");
    let id = scene.new_item_id(1);
    let edits = scene
        .add_shape(&ShapeSpec {
            id,
            kind: ShapeKind::Ellipse,
            start: [100.0, 100.0],
            end: [220.0, 180.0],
            constrain: false,
            brush: super::shape_brush(stamp, 0xFF30_4050, 1.0),
            fill: None,
        })
        .expect("a pencil shape is drawn with an outline brush");
    commit(&mut core, &scene, &edits);
    assert!(item_floats(scene.scene(), id).is_some());
}
