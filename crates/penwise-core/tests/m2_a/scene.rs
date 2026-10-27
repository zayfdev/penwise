//! `Scene`: apply/invert round trips, tombstones, bulk load, index consistency, families.

use penwise_core::edit::{Edit, EditError, Field, ItemPatch};
use penwise_core::item::{
    Affine, Item, ItemId, ItemKind, Layer, LayerSet, PageKind, Polygon, SampleParam, SampleRange,
    Scene, ShapeItem, ShapeKind, StrokeItem, Trim, Z_STEP,
};
use penwise_core::{Bounds, Preset};

use crate::fixtures::{
    RED, add_all, brute_query, erase, fill, id, line, permutation, rect_ring, stroke, stroke_with,
    text,
};

fn shape(id: ItemId, z: i64) -> Item {
    let mut it = crate::fixtures::item(
        id,
        z,
        ItemKind::Shape(ShapeItem {
            kind: ShapeKind::Rect,
            size: [40.0, 20.0],
            brush: Preset::Fineliner.params(),
            fill: None,
        }),
    );
    it.transform = Affine::translate(300.0, 300.0);
    it
}

/// One item of every kind plus a mask, all live.
fn sample_scene() -> (Scene, Vec<Item>) {
    let s = stroke(id(1), Z_STEP, [10.0, 10.0], [90.0, 30.0]);
    let pencil = stroke_with(
        id(2),
        2 * Z_STEP,
        Preset::Pencil,
        &line([10.0, 60.0], [90.0, 60.0], 21),
    );
    let mask = erase(id(3), &pencil, vec![[50.0, 60.0]], 6.0);
    let items = vec![
        s,
        pencil,
        mask,
        shape(id(4), 3 * Z_STEP),
        fill(
            id(5),
            4 * Z_STEP,
            rect_ring([200.0, 0.0], [260.0, 50.0]),
            Vec::new(),
        ),
        text(id(6), 5 * Z_STEP, [0.0, 200.0], [80.0, 20.0]),
    ];
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &items);
    (scene, items)
}

/// Bitwise snapshot of one item's stored state and cached geometry.
fn snapshot(scene: &Scene, item: ItemId) -> String {
    format!(
        "{:?} {:?} {:?}",
        scene.get_any(item),
        scene.geometry(item),
        scene.bounds(item)
    )
}

fn patch_for(field: Field) -> (ItemId, ItemPatch) {
    let mut p = ItemPatch::default();
    let target = match field {
        Field::Deleted => {
            p.deleted = Some(true);
            id(1)
        }
        Field::Layer => {
            p.layer = Some(Layer::Agent);
            id(1)
        }
        Field::Z => {
            p.z = Some(-7);
            id(1)
        }
        Field::Transform => {
            p.transform = Some(Affine::rotate(0.3).then(&Affine::translate(5.0, -2.0)));
            id(2)
        }
        Field::Brush => {
            p.brush = Some(Preset::Marker.params());
            id(1)
        }
        Field::ShapeFill => {
            p.shape_fill = Some(Some(RED));
            id(4)
        }
        Field::Paint => {
            p.paint = Some(penwise_core::item::Paint {
                color: RED,
                opacity: 0.25,
            });
            id(6)
        }
        Field::Polygons => {
            p.polygons = Some(vec![Polygon {
                outer: rect_ring([0.0, 0.0], [5.0, 5.0]),
                holes: Vec::new(),
            }]);
            id(5)
        }
        Field::ShapeKind => {
            p.shape_kind = Some(ShapeKind::RoundedRect { radius: 4.0 });
            id(4)
        }
        Field::ShapeSize => {
            p.shape_size = Some([10.0, 70.0]);
            id(4)
        }
        Field::Text => {
            p.text = Some("bye".into());
            id(6)
        }
        Field::TextBox => {
            p.text_box = Some([33.0, 44.0]);
            id(6)
        }
        Field::FontSize => {
            p.font_size = Some(9.5);
            id(6)
        }
    };
    (target, p)
}

#[test]
fn apply_then_inverse_restores_every_field_bitwise() {
    for field in Field::ALL {
        let (mut scene, items) = sample_scene();
        let before: Vec<String> = items.iter().map(|it| snapshot(&scene, it.id)).collect();
        let (target, patch) = patch_for(field);
        let edit = Edit::Update { id: target, patch };
        let inverse = scene.invert(&edit).unwrap();
        scene.apply(&edit).unwrap();
        assert_ne!(
            snapshot(&scene, target),
            before[items.iter().position(|i| i.id == target).unwrap()]
        );
        scene.apply(&inverse).unwrap();
        let after: Vec<String> = items.iter().map(|it| snapshot(&scene, it.id)).collect();
        assert_eq!(before, after, "{field:?}");
    }
}

#[test]
fn applying_an_update_twice_equals_once() {
    let (mut scene, _) = sample_scene();
    let (target, patch) = patch_for(Field::Transform);
    let edit = Edit::Update { id: target, patch };
    scene.apply(&edit).unwrap();
    let once = snapshot(&scene, target);
    scene.apply(&edit).unwrap();
    assert_eq!(once, snapshot(&scene, target));
}

#[test]
fn add_and_delete_invert() {
    let (mut scene, items) = sample_scene();
    let new = text(id(50), 0, [5.0, 5.0], [10.0, 10.0]);
    let add = Edit::Add(new.clone());
    assert_eq!(scene.invert(&add).unwrap(), Edit::Delete(new.id));
    scene.apply(&add).unwrap();
    scene.apply(&Edit::Delete(new.id)).unwrap();
    assert!(scene.get(new.id).is_none());

    let before = snapshot(&scene, items[0].id);
    let delete = Edit::Delete(items[0].id);
    let inverse = scene.invert(&delete).unwrap();
    scene.apply(&delete).unwrap();
    assert!(scene.get(items[0].id).is_none());
    assert_eq!(scene.get_any(items[0].id).map(|(_, dead)| dead), Some(true));
    scene.apply(&inverse).unwrap();
    assert_eq!(before, snapshot(&scene, items[0].id));
}

#[test]
fn tombstones_restore_with_geometry_and_index() {
    let (mut scene, _) = sample_scene();
    let everything = Bounds {
        min_x: -1e6,
        min_y: -1e6,
        max_x: 1e6,
        max_y: 1e6,
    };
    let all = scene.query(&everything, LayerSet::ALL);
    scene.apply(&Edit::Delete(id(4))).unwrap();
    // Deleting a tombstone is a no-op.
    scene.apply(&Edit::Delete(id(4))).unwrap();
    assert!(!scene.query(&everything, LayerSet::ALL).contains(&id(4)));
    assert!(scene.render_key(id(4)).is_none());
    // Updates on tombstones keep LWW state.
    let z = ItemPatch {
        z: Some(Z_STEP * 3),
        ..ItemPatch::default()
    };
    scene
        .apply(&Edit::Update {
            id: id(4),
            patch: z,
        })
        .unwrap();
    let restore = ItemPatch {
        deleted: Some(false),
        ..ItemPatch::default()
    };
    scene
        .apply(&Edit::Update {
            id: id(4),
            patch: restore,
        })
        .unwrap();
    assert_eq!(scene.query(&everything, LayerSet::ALL), all);
    assert!(scene.geometry(id(4)).is_some());
}

#[test]
fn invalid_edits_leave_the_scene_unchanged() {
    let (mut scene, items) = sample_scene();
    let before: Vec<String> = items.iter().map(|it| snapshot(&scene, it.id)).collect();
    let bad = [
        (Edit::Add(items[0].clone()), EditError::DuplicateItem(id(1))),
        (Edit::Delete(id(99)), EditError::UnknownItem(id(99))),
        (
            Edit::Update {
                id: id(1),
                patch: ItemPatch {
                    font_size: Some(3.0),
                    ..ItemPatch::default()
                },
            },
            EditError::FieldNotApplicable {
                id: id(1),
                field: Field::FontSize,
            },
        ),
        (
            Edit::Update {
                id: id(3),
                patch: ItemPatch {
                    transform: Some(Affine::IDENTITY),
                    ..ItemPatch::default()
                },
            },
            EditError::FieldNotApplicable {
                id: id(3),
                field: Field::Transform,
            },
        ),
        (
            Edit::Update {
                id: id(6),
                patch: ItemPatch {
                    font_size: Some(-1.0),
                    ..ItemPatch::default()
                },
            },
            EditError::InvalidValue { field: "font_size" },
        ),
        (
            Edit::Update {
                id: id(2),
                patch: ItemPatch {
                    transform: Some(Affine::translate(f32::NAN, 0.0)),
                    ..ItemPatch::default()
                },
            },
            EditError::InvalidValue { field: "transform" },
        ),
    ];
    for (edit, error) in bad {
        assert_eq!(scene.invert(&edit), Err(error.clone()));
        assert_eq!(scene.apply(&edit), Err(error));
    }
    let after: Vec<String> = items.iter().map(|it| snapshot(&scene, it.id)).collect();
    assert_eq!(before, after);
}

#[test]
fn from_items_equals_sequential_adds() {
    let (_, items) = sample_scene();
    let tombstoned = [id(4)];
    for seed in [1, 7, 99] {
        let order = permutation(items.len(), seed);
        let shuffled: Vec<(Item, bool)> = order
            .iter()
            .map(|&i| (items[i].clone(), tombstoned.contains(&items[i].id)))
            .collect();
        let bulk = Scene::from_items(PageKind::Infinite, shuffled).unwrap();
        let mut seq = Scene::new(PageKind::Infinite);
        add_all(&mut seq, &items);
        for t in tombstoned {
            seq.apply(&Edit::Delete(t)).unwrap();
        }
        for it in &items {
            assert_eq!(snapshot(&bulk, it.id), snapshot(&seq, it.id));
            assert_eq!(bulk.masks(it.id), seq.masks(it.id));
        }
        assert_eq!(
            bulk.render_order(LayerSet::ALL),
            seq.render_order(LayerSet::ALL)
        );
        let area = Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 100.0,
            max_y: 100.0,
        };
        assert_eq!(
            bulk.query(&area, LayerSet::ALL),
            seq.query(&area, LayerSet::ALL)
        );
        assert_eq!(bulk.len(), seq.len());
    }
    let dup = vec![(items[0].clone(), false), (items[0].clone(), true)];
    assert_eq!(
        Scene::from_items(PageKind::Infinite, dup).err(),
        Some(EditError::DuplicateItem(id(1)))
    );
}

#[test]
fn query_is_sorted_by_z_then_id_and_insertion_independent() {
    let mut items = Vec::new();
    for i in 0..40u16 {
        let x = f32::from(i % 8) * 30.0;
        let y = f32::from(i / 8) * 30.0;
        // Several items share a z so the id tie-break matters.
        items.push(text(
            id(100 + i),
            i64::from(i % 5) * Z_STEP,
            [x, y],
            [25.0, 25.0],
        ));
    }
    let area = Bounds {
        min_x: 20.0,
        min_y: 20.0,
        max_x: 130.0,
        max_y: 100.0,
    };
    let mut reference = None;
    for seed in [3, 11, 12345] {
        let mut scene = Scene::new(PageKind::Infinite);
        for i in permutation(items.len(), seed) {
            scene.apply(&Edit::Add(items[i].clone())).unwrap();
        }
        let hits = scene.query(&area, LayerSet::ALL);
        let keys: Vec<(i64, ItemId)> = hits.iter().map(|&h| (scene.get(h).unwrap().z, h)).collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]), "sorted by (z, id)");
        assert!(!hits.is_empty());
        assert_eq!(*reference.get_or_insert(hits.clone()), hits);
    }
}

/// Builds the 30-item fixture (in insertion order) and returns it with the ids.
fn index_fixture() -> (Scene, Vec<ItemId>) {
    let mut scene = Scene::new(PageKind::Infinite);
    let mut ids: Vec<ItemId> = Vec::new();
    for i in 0..30u16 {
        let x = f32::from(i) * 13.0;
        let it = text(
            id(200 + i),
            i64::from(i) * Z_STEP,
            [x, f32::from(i % 4) * 20.0],
            [10.0, 8.0],
        );
        ids.push(it.id);
        scene.apply(&Edit::Add(it)).unwrap();
    }
    (scene, ids)
}

fn index_query_areas() -> [Bounds; 4] {
    [
        Bounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 60.0,
            max_y: 30.0,
        },
        Bounds {
            min_x: 100.0,
            min_y: -50.0,
            max_x: 260.0,
            max_y: 50.0,
        },
        Bounds {
            min_x: -1e5,
            min_y: -1e5,
            max_x: 1e5,
            max_y: 1e5,
        },
        Bounds {
            min_x: 500.0,
            min_y: 500.0,
            max_x: 900.0,
            max_y: 900.0,
        },
    ]
}

/// The edit applied at `step`: cycles through move, delete, restyle, and resize.
fn index_edit(step: usize, target: ItemId) -> Edit {
    match step % 4 {
        0 => Edit::Update {
            id: target,
            patch: ItemPatch {
                transform: Some(Affine::translate(
                    600.0,
                    600.0 + f32::from(u16::try_from(step).unwrap()),
                )),
                ..ItemPatch::default()
            },
        },
        1 => Edit::Delete(target),
        2 => Edit::Update {
            id: target,
            patch: ItemPatch {
                z: Some(-1),
                layer: Some(Layer::Agent),
                ..ItemPatch::default()
            },
        },
        _ => Edit::Update {
            id: target,
            patch: ItemPatch {
                text_box: Some([200.0, 3.0]),
                ..ItemPatch::default()
            },
        },
    }
}

fn assert_queries_match_brute(scene: &Scene, ids: &[ItemId], areas: &[Bounds], step: usize) {
    for area in areas {
        for layers in [LayerSet::ALL, LayerSet::INK] {
            assert_eq!(
                scene.query(area, layers),
                brute_query(scene, ids, area, layers),
                "step {step}"
            );
        }
    }
}

#[test]
fn index_stays_consistent_through_add_update_delete() {
    let (mut scene, ids) = index_fixture();
    let areas = index_query_areas();
    let order = permutation(ids.len(), 42);
    for (step, &k) in order.iter().enumerate() {
        scene.apply(&index_edit(step, ids[k])).unwrap();
        if step % 7 == 6 {
            let restore = ItemPatch {
                deleted: Some(false),
                ..ItemPatch::default()
            };
            scene
                .apply(&Edit::Update {
                    id: ids[order[step - 5]],
                    patch: restore,
                })
                .unwrap();
        }
        assert_queries_match_brute(&scene, &ids, &areas, step);
    }
}

#[test]
fn next_z_and_render_bands() {
    let (scene, _) = sample_scene();
    assert_eq!(scene.top_z(), 5 * Z_STEP);
    assert_eq!(scene.next_z(), Some(6 * Z_STEP));
    assert_eq!(Scene::new(PageKind::Infinite).top_z(), 0);
    // The fill sits in the lowest band although its z is high.
    let order = scene.render_order(LayerSet::ALL);
    assert_eq!(order.first(), Some(&id(5)));
    assert!(
        !order.contains(&id(3)),
        "erase items are not painted on their own"
    );
    assert_eq!(scene.masks(id(2)), vec![id(3)]);
    assert_eq!(scene.len(), 6);
}

#[test]
fn next_z_is_none_instead_of_tying_with_the_top() {
    let mut scene = Scene::new(PageKind::Infinite);
    let top = stroke(id(1), i64::MAX - Z_STEP, [0.0, 0.0], [10.0, 0.0]);
    scene.apply(&Edit::Add(top)).unwrap();
    assert_eq!(scene.next_z(), Some(i64::MAX));
    let patch = ItemPatch {
        z: Some(i64::MAX - Z_STEP + 1),
        ..ItemPatch::default()
    };
    scene.apply(&Edit::Update { id: id(1), patch }).unwrap();
    assert_eq!(scene.next_z(), None, "no z above the top item is left");
}

/// A transform whose page image overflows `f32` (or the codec's ±2²⁰ px) is rejected before it can reach
/// the spatial index (rstar aborts on NaN areas once nodes overflow).
#[test]
fn hostile_transforms_are_rejected_before_indexing() {
    let (mut scene, items) = sample_scene();
    let before = scene.clone();
    let hostile = [
        Affine::scale(3.0e38, 1.0),
        Affine::scale(1.0, -3.0e38),
        Affine::translate(2.0e6, 0.0),
        Affine::scale(65_536.0, 65_536.0), // in-range coefficients, but every item maps past 2²⁰ px
    ];
    for transform in hostile {
        let patch = ItemPatch {
            transform: Some(transform),
            ..ItemPatch::default()
        };
        for item in items
            .iter()
            .filter(|i| !matches!(i.kind, ItemKind::Erase(_)))
        {
            let update = Edit::Update {
                id: item.id,
                patch: patch.clone(),
            };
            assert!(
                matches!(scene.apply(&update), Err(EditError::InvalidValue { .. })),
                "{transform:?} on {:?}",
                item.id
            );
            let mut added = item.clone();
            added.id = id(200);
            added.transform = transform;
            assert!(matches!(
                scene.apply(&Edit::Add(added)),
                Err(EditError::InvalidValue { .. })
            ));
        }
    }
    for item in &items {
        assert_eq!(snapshot(&scene, item.id), snapshot(&before, item.id));
    }
    // Many near-limit items still index and query normally (no rstar reinsertion panic).
    for n in 0..64u16 {
        let mut s = stroke(id(100 + n), i64::from(n), [0.0, 0.0], [1.0, 1.0]);
        s.transform = Affine::translate(1.0e6 - f32::from(n), -1.0e6);
        scene.apply(&Edit::Add(s)).unwrap();
    }
    let far = Bounds {
        min_x: 9.0e5,
        min_y: -1.1e6,
        max_x: 1.1e6,
        max_y: -9.0e5,
    };
    assert_eq!(scene.query(&far, LayerSet::ALL).len(), 64);
}

#[test]
fn masks_follow_their_target() {
    let (mut scene, _) = sample_scene();
    let mask_at = |scene: &Scene| {
        let Some(penwise_core::item::ItemGeometry::Erase { path, .. }) = scene.geometry(id(3))
        else {
            panic!("no mask geometry");
        };
        path.clone()
    };
    assert_eq!(mask_at(&scene), vec![[50.0, 60.0]]);
    let moved = ItemPatch {
        transform: Some(Affine::translate(10.0, 0.0)),
        ..ItemPatch::default()
    };
    scene
        .apply(&Edit::Update {
            id: id(2),
            patch: moved,
        })
        .unwrap();
    assert_eq!(mask_at(&scene), vec![[60.0, 60.0]]);
    scene.apply(&Edit::Delete(id(2))).unwrap();
    assert!(
        mask_at(&scene).is_empty(),
        "a dead target makes its masks inert"
    );
}

/// Last sample index of `fixtures::stroke` (41 samples).
const LAST: u32 = 40;

fn param(i: u32) -> SampleParam {
    SampleParam(i << SampleParam::FRAC_BITS)
}

fn range(a: u32, b: u32) -> SampleRange {
    SampleRange {
        start: param(a),
        end: param(b),
        start_cut: a != 0,
        end_cut: b != LAST,
    }
}

fn piece(of: &Item, piece_id: ItemId, visible: Vec<SampleRange>, own: u32) -> Item {
    let ItemKind::Stroke(s) = &of.kind else {
        panic!("not a stroke")
    };
    let mut it = of.clone();
    it.id = piece_id;
    it.kind = ItemKind::Stroke(StrokeItem {
        trim: Some(Trim {
            origin: of.id,
            sample_offset: 0,
            visible,
            own,
            warm: None,
        }),
        ..s.clone()
    });
    it
}

#[test]
fn family_intersection_hides_ink_erased_by_two_concurrent_splits() {
    let origin = stroke(id(1), Z_STEP, [0.0, 0.0], [200.0, 0.0]);
    let split_a = vec![range(0, 10), range(20, LAST)];
    let split_b = vec![range(0, 5), range(15, LAST)];
    let edits_a = vec![
        Edit::Delete(origin.id),
        Edit::Add(piece(&origin, id(10), split_a.clone(), 0)),
        Edit::Add(piece(&origin, id(11), split_a, 1)),
    ];
    let edits_b = vec![
        Edit::Delete(origin.id),
        Edit::Add(piece(&origin, id(20), split_b.clone(), 0)),
        Edit::Add(piece(&origin, id(21), split_b, 1)),
    ];
    let mut results = Vec::new();
    for (first, second) in [(&edits_a, &edits_b), (&edits_b, &edits_a)] {
        let mut scene = Scene::new(PageKind::Infinite);
        scene.apply(&Edit::Add(origin.clone())).unwrap();
        scene.apply_all(first).unwrap();
        scene.apply_all(second).unwrap();
        let members = scene.family(origin.id);
        assert_eq!(members, vec![id(10), id(11), id(20), id(21)]);
        let mut shown = Vec::new();
        for m in &members {
            for r in scene.visible_ranges(*m) {
                // Nothing erased by either device (5..20) is visible again.
                assert!(
                    r.end <= param(5) || r.start >= param(20),
                    "{m:?} shows {r:?}"
                );
                shown.push((r.start, r.end));
            }
        }
        shown.sort();
        shown.dedup();
        assert_eq!(shown, vec![(param(0), param(5)), (param(20), param(LAST))]);
        results.push(
            members
                .iter()
                .map(|m| snapshot(&scene, *m))
                .collect::<Vec<_>>(),
        );
    }
    assert_eq!(results[0], results[1], "both delivery orders converge");
}

#[test]
fn restoring_a_family_member_re_derives_its_siblings() {
    let origin = stroke(id(1), Z_STEP, [0.0, 0.0], [200.0, 0.0]);
    let split = vec![range(0, 10), range(20, LAST)];
    let mut scene = Scene::new(PageKind::Infinite);
    scene.apply(&Edit::Add(origin.clone())).unwrap();
    scene
        .apply(&Edit::Add(piece(&origin, id(10), split, 0)))
        .unwrap();
    // The live untrimmed origin is narrowed by the piece's visible set.
    assert_eq!(scene.visible_ranges(origin.id).len(), 2);
    scene.apply(&Edit::Delete(id(10))).unwrap();
    assert_eq!(scene.visible_ranges(origin.id), vec![range(0, LAST)]);
}
