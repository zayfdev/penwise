//! Transforms, selection bounds, delete, duplicate, recolor, set layer.

use penwise_core::edit::Edit;
use penwise_core::edit::select::{delete_items, duplicate_items, recolor_items, set_layer};
use penwise_core::edit::transform::{
    move_items, rotate_items, scale_items, selection_bounds, transform_items,
};
use penwise_core::item::{
    Affine, ItemGeometry, ItemKind, Layer, LayerSet, PageKind, SampleParam, SampleRange, Scene,
    StrokeItem, Trim, Z_STEP,
};
use penwise_core::{Bounds, Preset};

use crate::fixtures::{
    Counter, RED, add_all, erase, fill, id, line, rect_ring, stroke, stroke_with, text,
};

fn scene_with_mask() -> Scene {
    let pencil = stroke_with(
        id(1),
        Z_STEP,
        Preset::Pencil,
        &line([0.0, 0.0], [50.0, 0.0], 11),
    );
    let mask = erase(id(2), &pencil, vec![[25.0, 0.0]], 4.0);
    let t = text(id(3), 2 * Z_STEP, [100.0, 100.0], [20.0, 10.0]);
    let f = fill(
        id(4),
        3 * Z_STEP,
        rect_ring([200.0, 0.0], [220.0, 20.0]),
        Vec::new(),
    );
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[pencil, mask, t, f]);
    scene
}

fn transform_of(edit: &Edit) -> Affine {
    let Edit::Update { patch, .. } = edit else {
        panic!("{edit:?}")
    };
    patch.transform.unwrap()
}

#[test]
fn transforms_emit_absolute_updates_sorted_by_id() {
    let scene = scene_with_mask();
    assert!(
        move_items(&scene, &[id(3)], 0.0, 0.0).is_empty(),
        "identity delta"
    );
    let edits = move_items(&scene, &[id(3), id(2), id(1), id(3), id(77)], 5.0, -5.0);
    let targets: Vec<_> = edits.iter().map(Edit::target).collect();
    assert_eq!(
        targets,
        vec![id(1), id(3)],
        "erase, duplicate and unknown ids skipped"
    );
    assert!(transform_of(&edits[1]).bit_eq(&Affine::translate(105.0, 95.0)));

    let scaled = scale_items(&scene, &[id(3)], [100.0, 100.0], 2.0, 3.0);
    let t = transform_of(&scaled[0]);
    assert_eq!(t.apply([0.0, 0.0]), [100.0, 100.0], "anchor stays fixed");
    assert_eq!(t.apply([20.0, 10.0]), [140.0, 130.0]);

    let pivot = [110.0, 105.0];
    let rotated = rotate_items(&scene, &[id(3)], pivot, core::f32::consts::FRAC_PI_2);
    let r = transform_of(&rotated[0]);
    let p = r.apply([10.0, 5.0]);
    assert!(
        (p[0] - pivot[0]).abs() < 1e-4 && (p[1] - pivot[1]).abs() < 1e-4,
        "pivot fixed"
    );

    let singular = Affine::scale(0.0, 1.0);
    assert!(
        transform_items(&scene, &[id(3)], &singular).is_empty(),
        "non-invertible skipped"
    );
}

#[test]
fn applied_moves_carry_masks_and_bounds() {
    let mut scene = scene_with_mask();
    let before = selection_bounds(&scene, &[id(1), id(3)]);
    let edits = move_items(&scene, &[id(1), id(3)], 10.0, 20.0);
    scene.apply_all(&edits).unwrap();
    let after = selection_bounds(&scene, &[id(1), id(3)]);
    assert!((after.min_x - before.min_x - 10.0).abs() < 0.01);
    assert!((after.max_y - before.max_y - 20.0).abs() < 0.01);
    let Some(ItemGeometry::Erase { path, .. }) = scene.geometry(id(2)) else {
        panic!("no mask")
    };
    assert_eq!(path, &vec![[35.0, 20.0]]);
    assert_eq!(selection_bounds(&scene, &[id(2), id(99)]), Bounds::EMPTY);
}

#[test]
fn delete_recolor_and_layer() {
    let scene = scene_with_mask();
    assert_eq!(
        delete_items(&scene, &[id(4), id(2), id(1)]),
        vec![Edit::Delete(id(1)), Edit::Delete(id(4))]
    );

    let edits = recolor_items(&scene, &[id(1), id(2), id(3), id(4)], RED);
    let targets: Vec<_> = edits.iter().map(Edit::target).collect();
    assert_eq!(
        targets,
        vec![id(1), id(3)],
        "fill is already red, erase skipped"
    );
    let mut recolored = scene.clone();
    recolored.apply_all(&edits).unwrap();
    match &recolored.get(id(1)).unwrap().kind {
        ItemKind::Stroke(s) => {
            assert_eq!(s.params.color, RED);
            assert_eq!(s.params.size_max, Preset::Pencil.params().size_max);
        }
        ItemKind::Erase(_) | ItemKind::Shape(_) | ItemKind::Fill(_) | ItemKind::Text(_) => {
            panic!("not a stroke")
        }
    }
    assert!(recolor_items(&recolored, &[id(1), id(3), id(4)], RED).is_empty());

    let edits = set_layer(&scene, &[id(1), id(3)], Layer::Agent);
    let targets: Vec<_> = edits.iter().map(Edit::target).collect();
    assert_eq!(
        targets,
        vec![id(1), id(2), id(3)],
        "the mask follows its target"
    );
    let mut moved = scene.clone();
    moved.apply_all(&edits).unwrap();
    assert!(set_layer(&moved, &[id(1), id(3)], Layer::Agent).is_empty());
    assert_eq!(moved.render_order(LayerSet::INK), vec![id(4)]);
}

#[test]
fn duplicate_stacks_copies_on_top_with_masks() {
    let scene = scene_with_mask();
    let mut ids = Counter(500);
    let edits = duplicate_items(&scene, &[id(3), id(1)], 30.0, 0.0, &mut ids);
    let added: Vec<_> = edits
        .iter()
        .map(|e| {
            let Edit::Add(item) = e else { panic!("{e:?}") };
            item.clone()
        })
        .collect();
    // (z, id) order: pencil (z 1) then its mask, then the text (z 2).
    assert_eq!(
        added.iter().map(|i| i.id).collect::<Vec<_>>(),
        vec![id(500), id(501), id(502)]
    );
    let top = scene.next_z().unwrap();
    assert_eq!(added[0].z, top);
    assert_eq!(added[2].z, top + Z_STEP);
    let ItemKind::Erase(mask) = &added[1].kind else {
        panic!("not a mask")
    };
    assert_eq!(mask.target, id(500));
    assert_eq!(added[1].z, added[0].z);
    assert!(added[2].transform.bit_eq(&Affine::translate(130.0, 100.0)));
    let mut copied = scene.clone();
    copied.apply_all(&edits).unwrap();
    assert_eq!(copied.masks(id(500)), vec![id(501)]);
}

/// A ballpoint stroke `id(1)` split into a remaining piece `id(2)` (samples 20..40), origin deleted.
fn piece_scene() -> Scene {
    let origin = stroke(id(1), Z_STEP, [0.0, 0.0], [200.0, 0.0]);
    let ItemKind::Stroke(s) = &origin.kind else {
        panic!()
    };
    let p = |i: u32| SampleParam(i << SampleParam::FRAC_BITS);
    let visible = vec![
        SampleRange {
            start: p(0),
            end: p(10),
            start_cut: false,
            end_cut: true,
        },
        SampleRange {
            start: p(20),
            end: p(40),
            start_cut: true,
            end_cut: false,
        },
    ];
    let mut piece = origin.clone();
    piece.id = id(2);
    piece.kind = ItemKind::Stroke(StrokeItem {
        trim: Some(Trim {
            origin: origin.id,
            sample_offset: 0,
            visible,
            own: 1,
        }),
        ..s.clone()
    });
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[origin.clone(), piece]);
    scene.apply(&Edit::Delete(origin.id)).unwrap();
    scene
}

#[test]
fn duplicated_pieces_become_their_own_family() {
    let scene = piece_scene();
    let edits = duplicate_items(&scene, &[id(2)], 0.0, 50.0, &mut Counter(900));
    let [Edit::Add(copy)] = edits.as_slice() else {
        panic!("{edits:?}")
    };
    let ItemKind::Stroke(c) = &copy.kind else {
        panic!()
    };
    let trim = c.trim.as_ref().unwrap();
    assert_eq!(trim.origin, id(900));
    assert_eq!(trim.visible, scene.visible_ranges(id(2)));
    assert_eq!(trim.own, 0);
    let mut copied = scene.clone();
    copied.apply_all(&edits).unwrap();
    assert_eq!(copied.family(id(900)), vec![id(900)]);
    assert_eq!(copied.family(id(1)), vec![id(2)]);
    assert_eq!(copied.visible_ranges(id(900)), scene.visible_ranges(id(2)));
}

#[test]
fn duplicated_piece_keeps_its_masks() {
    let mut scene = piece_scene();
    let piece = scene.get(id(2)).unwrap().clone();
    add_all(&mut scene, &[erase(id(3), &piece, vec![[150.0, 0.0]], 4.0)]);
    let edits = duplicate_items(&scene, &[id(2)], 0.0, 50.0, &mut Counter(900));
    let [Edit::Add(copy), Edit::Add(mask)] = edits.as_slice() else {
        panic!("{edits:?}")
    };
    assert_eq!(copy.id, id(900));
    assert_eq!(mask.id, id(901));
    let ItemKind::Erase(e) = &mask.kind else {
        panic!("not a mask")
    };
    assert_eq!(e.target, id(900));
    assert_eq!((mask.layer, mask.z), (copy.layer, copy.z));
    let mut copied = scene.clone();
    copied.apply_all(&edits).unwrap();
    assert_eq!(copied.masks(id(900)), vec![id(901)]);
    assert_eq!(copied.masks(id(2)), vec![id(3)]);
}
