//! Pictures as first-class page items: scene geometry, paint order, hit/lasso/erase, editable fields and undo.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::edit::select::duplicate_items;
use penwise_core::edit::transform::move_items;
use penwise_core::edit::{
    Edit, EditError, Field, ItemPatch, LassoParams, UndoStack, erase_strokes, hit_test, lasso,
};
use penwise_core::item::{
    Affine, IdSource, ImageItem, Item, ItemGeometry, ItemId, ItemKind, Layer, LayerSet, PageKind,
    Scene,
};
use penwise_core::{Bounds, Preset};

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

fn picture(n: u128, z: i64, transform: Affine) -> Item {
    Item {
        id: id(n),
        layer: Layer::Ink,
        z,
        transform,
        kind: ItemKind::Image(ImageItem {
            asset: [9; 32],
            w: 100.0,
            h: 60.0,
        }),
    }
}

fn scene_with_picture() -> Scene {
    let mut scene = Scene::new(PageKind::Infinite);
    scene
        .apply(&Edit::Add(picture(1, 0, Affine::translate(200.0, 100.0))))
        .unwrap();
    scene
}

#[test]
fn geometry_is_the_transformed_box_and_bounds_cover_it() {
    let scene = scene_with_picture();
    let Some(ItemGeometry::Image { corners }) = scene.geometry(id(1)) else {
        panic!("image geometry expected")
    };
    assert_eq!(
        *corners,
        [
            [200.0, 100.0],
            [300.0, 100.0],
            [300.0, 160.0],
            [200.0, 160.0]
        ]
    );
    let b = scene.bounds(id(1)).unwrap();
    assert_eq!(
        (b.min_x, b.min_y, b.max_x, b.max_y),
        (200.0, 100.0, 300.0, 160.0)
    );
    let scale = Affine::scale(2.0, 1.0).about([200.0, 100.0]);
    let mut moved = scene.clone();
    let patch = ItemPatch {
        transform: Some(Affine::translate(200.0, 100.0).then(&scale)),
        ..ItemPatch::default()
    };
    moved.apply(&Edit::Update { id: id(1), patch }).unwrap();
    let b = moved.bounds(id(1)).unwrap();
    assert_eq!(
        (b.min_x, b.max_x),
        (200.0, 400.0),
        "transform edits re-derive the box"
    );
}

#[test]
fn paint_order_is_by_z_and_layer_filtering_applies() {
    let mut scene = scene_with_picture();
    let mut agent = picture(2, -10, Affine::IDENTITY);
    agent.layer = Layer::Agent;
    scene.apply(&Edit::Add(agent)).unwrap();
    assert_eq!(scene.render_order(LayerSet::ALL), vec![id(2), id(1)]);
    assert_eq!(scene.render_order(LayerSet::INK), vec![id(1)]);
}

#[test]
fn hit_test_and_lasso_select_the_picture_inside_its_box() {
    let scene = scene_with_picture();
    assert_eq!(
        hit_test(&scene, [250.0, 130.0], 0.0, LayerSet::ALL),
        vec![id(1)]
    );
    assert!(hit_test(&scene, [250.0, 200.0], 2.0, LayerSet::ALL).is_empty());
    let around = [[190.0, 90.0], [310.0, 90.0], [310.0, 170.0], [190.0, 170.0]];
    assert_eq!(
        lasso(&scene, &around, LayerSet::ALL, &LassoParams::DEFAULT),
        vec![id(1)]
    );
    let beside = [[0.0, 0.0], [120.0, 0.0], [120.0, 80.0], [0.0, 80.0]];
    assert!(lasso(&scene, &beside, LayerSet::ALL, &LassoParams::DEFAULT).is_empty());
}

#[test]
fn stroke_eraser_deletes_a_touched_picture() {
    let scene = scene_with_picture();
    let edits = erase_strokes(&scene, &[[250.0, 50.0], [250.0, 130.0]], 3.0, LayerSet::ALL);
    assert_eq!(edits, vec![Edit::Delete(id(1))]);
}

#[test]
fn only_layer_z_transform_and_deleted_apply_to_pictures() {
    let tag = scene_with_picture().get(id(1)).unwrap().kind.tag();
    let applicable: Vec<Field> = Field::ALL
        .into_iter()
        .filter(|f| f.applies_to(tag))
        .collect();
    assert_eq!(
        applicable,
        vec![Field::Deleted, Field::Layer, Field::Z, Field::Transform]
    );
    let mut scene = scene_with_picture();
    let patch = ItemPatch {
        brush: Some(Preset::Ballpoint.params()),
        ..ItemPatch::default()
    };
    assert_eq!(
        scene.apply(&Edit::Update { id: id(1), patch }),
        Err(EditError::FieldNotApplicable {
            id: id(1),
            field: Field::Brush
        })
    );
}

#[test]
fn move_and_undo_round_trip_and_duplicate_copies_the_asset() {
    let mut scene = scene_with_picture();
    let mut undo = UndoStack::default();
    let edits = move_items(&scene, &[id(1)], 10.0, 5.0);
    undo.apply_local(&mut scene, &edits).unwrap();
    assert!((scene.bounds(id(1)).unwrap().min_x - 210.0).abs() < 1e-3);
    let back = undo.undo(&scene);
    scene.apply_all(&back).unwrap();
    assert!((scene.bounds(id(1)).unwrap().min_x - 200.0).abs() < 1e-3);

    let copies = duplicate_items(&scene, &[id(1)], 30.0, 0.0, &mut Counter(50));
    scene.apply_all(&copies).unwrap();
    assert_eq!(scene.len(), 2);
    let copy = scene.get(id(51)).unwrap();
    assert!(matches!(&copy.kind, ItemKind::Image(i) if i.asset == [9; 32]));
    assert_eq!(
        scene.bounds(id(51)).unwrap(),
        Bounds {
            min_x: 230.0,
            min_y: 100.0,
            max_x: 330.0,
            max_y: 160.0
        }
    );
}
