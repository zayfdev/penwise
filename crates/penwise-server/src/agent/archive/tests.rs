//! The geometry of a freeze: which items make room when a frame moves one column right, and which variants a
//! clear spares. Scenes are built in memory; the snapshots themselves need a browser and are covered end to end
//! in `penwise-mcp/tests/round.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::collections::BTreeSet;

use penwise_core::edit::Edit;
use penwise_core::item::{
    Affine, FramePreset, FrameSpec, ImageItem, Item, ItemId, ItemKind, Layer, PageKind, Scene,
    VariantSpec, Z_STEP, variant_frames,
};
use penwise_core::marks::{MarkItem, MarkStatus};

use super::ink::{Snapshot, draw_strokes};
use super::{Picture, place_pictures, shifts};
use crate::agent::variants::clear_plan;

fn id(n: u8) -> ItemId {
    ItemId([n; 16])
}

fn frame_at(n: u8, x: f32, y: f32) -> Item {
    FrameSpec::new(
        "prj".to_owned(),
        format!("/{n}"),
        FramePreset::Desktop,
        None,
        None,
        x,
        y,
    )
    .unwrap()
    .item(id(n), Layer::Ink, Z_STEP * i64::from(n))
}

fn image_at(n: u8, x: f32, y: f32) -> Item {
    Item {
        id: id(n),
        layer: Layer::Ink,
        z: Z_STEP * i64::from(n),
        transform: Affine::translate(x, y),
        kind: ItemKind::Image(ImageItem {
            asset: [0; 32],
            w: 300.0,
            h: 200.0,
        }),
    }
}

fn scene(items: Vec<Item>) -> Scene {
    Scene::from_items(
        PageKind::Infinite,
        items.into_iter().map(|item| (item, false)),
    )
    .expect("scene")
}

fn picture(frame: u8, x: f32, y: f32) -> Picture {
    Picture {
        frame: id(frame),
        id: id(100 + frame),
        asset: "0".repeat(64),
        position: [x, y],
        size: [1440.0, 900.0],
        label: format!("Round 1 \u{2014} desktop /{frame}"),
    }
}

const COLUMN: f32 = 1440.0 + 40.0;

#[test]
fn two_routes_on_one_row_the_next_one_makes_room() {
    // A at 0, B right of it with the usual gap, both on the first row.
    let scene = scene(vec![frame_at(1, 0.0, 0.0), frame_at(2, COLUMN, 0.0)]);
    let a = picture(1, 0.0, 0.0);
    let moved = shifts(&scene, &[&a], &BTreeSet::new());
    assert_eq!(moved.get(&id(1)), Some(&COLUMN), "A moves one column");
    assert_eq!(
        moved.get(&id(2)),
        Some(&COLUMN),
        "B moves with it, so A does not land on B"
    );
}

#[test]
fn when_both_routes_are_frozen_the_second_picture_makes_room_too() {
    let scene = scene(vec![frame_at(1, 0.0, 0.0), frame_at(2, COLUMN, 0.0)]);
    let (a, b) = (picture(1, 0.0, 0.0), picture(2, COLUMN, 0.0));
    let moved = shifts(&scene, &[&a, &b], &BTreeSet::new());
    assert_eq!(moved.get(&id(1)), Some(&COLUMN));
    assert_eq!(
        moved.get(&id(2)),
        Some(&(2.0 * COLUMN)),
        "B carries A's column and its own"
    );
    let placed = place_pictures(&scene, &[&a, &b], &BTreeSet::new());
    assert!(
        (placed[0].1[0]).abs() < 0.01,
        "A's picture stays where A was"
    );
    assert!(
        (placed[1].1[0] - 2.0 * COLUMN).abs() < 0.01,
        "B's picture sits where B is after A's column was made: {:?}",
        placed[1].1
    );
    // Nothing overlaps: A's new box ends where B's picture begins, with the gap kept.
    let a_new_right = moved[&id(1)] + 1440.0;
    assert!(placed[1].1[0] - a_new_right >= 39.9);
}

#[test]
fn lanes_stack_downward_and_only_the_frames_own_lane_moves() {
    let scene = scene(vec![
        frame_at(1, 0.0, 0.0),
        frame_at(2, COLUMN, 0.0),
        // The next lane, below the tallest box of the first.
        frame_at(3, 0.0, 1100.0),
        frame_at(4, COLUMN, 1100.0),
        // A history picture further right in lane 1 and one in lane 2.
        image_at(5, 4440.0, 100.0),
        image_at(6, 4440.0, 1300.0),
    ]);
    let a = picture(1, 0.0, 0.0);
    let moved = shifts(&scene, &[&a], &BTreeSet::new());
    let mut keys: Vec<ItemId> = moved.keys().copied().collect();
    keys.sort();
    assert_eq!(keys, vec![id(1), id(2), id(5)], "lane 1 only: {moved:?}");

    let c = picture(3, 0.0, 1100.0);
    let moved = shifts(&scene, &[&c], &BTreeSet::new());
    let mut keys: Vec<ItemId> = moved.keys().copied().collect();
    keys.sort();
    assert_eq!(keys, vec![id(3), id(4), id(6)], "lane 2 only: {moved:?}");
}

#[test]
fn items_on_the_frame_marks_and_frames_to_be_deleted_stay_put() {
    let mut items = vec![frame_at(1, 0.0, 0.0), frame_at(2, COLUMN, 0.0)];
    // Ink over the frame itself (starts left of its right edge) stays on the history picture.
    items.push(image_at(7, 100.0, 100.0));
    let scene = scene(items);
    let a = picture(1, 0.0, 0.0);
    let spared: BTreeSet<ItemId> = [id(2)].into_iter().collect();
    let moved = shifts(&scene, &[&a], &spared);
    let keys: Vec<ItemId> = moved.keys().copied().collect();
    assert_eq!(
        keys,
        vec![id(1)],
        "ink on the frame stays, a frame being deleted is not moved"
    );
}

#[test]
fn a_variant_carrying_a_mark_newer_than_the_round_is_not_cleared() {
    let mut items = vec![frame_at(1, 0.0, 0.0)];
    let base = scene(items.clone());
    let specs: Vec<VariantSpec> = (0..2)
        .map(|n| VariantSpec {
            id: id(10 + n),
            label: format!("v{n}"),
            patch: "{}".to_owned(),
        })
        .collect();
    for edit in variant_frames(&base, id(1), &specs, "vg_1", Z_STEP * 20).unwrap() {
        if let Edit::Add(item) = edit {
            items.push(item);
        }
    }
    let mark_on = |mark: u8, frame: u8| {
        MarkItem {
            frame: id(frame),
            preset: FramePreset::Desktop,
            status: MarkStatus::New,
            state: String::new(),
            strokes: Vec::new(),
        }
        .item(id(mark), Z_STEP * 40)
    };
    items.push(mark_on(50, 10)); // on variant 0, part of the round
    items.push(mark_on(51, 11)); // on variant 1, drawn after the send
    let scene = scene(items);

    let none = BTreeSet::new();
    let all = clear_plan(&scene, "vg_1", &none).unwrap().unwrap();
    assert_eq!(all.deleted.len(), 2, "without a protected mark both go");

    let newer: BTreeSet<ItemId> = [id(51)].into_iter().collect();
    let plan = clear_plan(&scene, "vg_1", &newer).unwrap().unwrap();
    assert_eq!(
        plan.deleted,
        vec![id(10)],
        "the variant with the newer mark stays"
    );
    assert_eq!(
        plan.marks,
        vec![id(50)],
        "only the round's own mark is done"
    );
    assert!(
        !plan
            .edits
            .iter()
            .any(|edit| matches!(edit, Edit::Delete(frame) if *frame == id(11))),
        "variant 1 is not deleted"
    );
}

fn pixel(pixmap: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * pixmap.width() + x) * 4) as usize;
    pixmap.data()[at..at + 4].try_into().unwrap()
}

#[test]
fn ink_is_composited_in_the_scaled_space_of_a_scaled_down_snapshot() {
    // A 1440 x 1000 box snapshotted at half scale (720 x 500 pixels); a stroke at page (200, 200).
    let stroke = penwise_testkit::stroke_item(
        id(9),
        Z_STEP,
        Layer::Ink,
        1,
        &[[160.0, 200.0], [240.0, 200.0], [320.0, 200.0]],
    );
    let ink = scene(vec![stroke]);
    let mut canvas = tiny_skia::Pixmap::new(720, 500).unwrap();
    canvas.fill(tiny_skia::Color::WHITE);
    let snapshot = Snapshot {
        pixmap: canvas,
        scale: 0.5,
        box_size: [1440.0, 1000.0],
    };
    let drawn = draw_strokes(snapshot, &ink, [0.0, 0.0]).unwrap();
    let white = [255, 255, 255, 255];
    assert_ne!(
        pixel(&drawn, 120, 100),
        white,
        "page (240, 200) is pixel (120, 100)"
    );
    assert_eq!(
        pixel(&drawn, 240, 200),
        white,
        "not at the unscaled position"
    );

    // The frame sits at (1000, 500) on the board: the same stroke, drawn relative to it.
    let moved = scene(vec![penwise_testkit::stroke_item(
        id(9),
        Z_STEP,
        Layer::Ink,
        1,
        &[[1160.0, 700.0], [1240.0, 700.0], [1320.0, 700.0]],
    )]);
    let mut canvas = tiny_skia::Pixmap::new(720, 500).unwrap();
    canvas.fill(tiny_skia::Color::WHITE);
    let drawn = draw_strokes(
        Snapshot {
            pixmap: canvas,
            scale: 0.5,
            box_size: [1440.0, 1000.0],
        },
        &moved,
        [1000.0, 500.0],
    )
    .unwrap();
    assert_ne!(pixel(&drawn, 120, 100), white);
}
