//! Lasso: the 75% rule, the dot rule, masked samples, kinds, insertion-order independence.

use penwise_core::Preset;
use penwise_core::edit::{LassoParams, lasso};
use penwise_core::item::{
    Affine, ItemKind, LayerSet, PageKind, Scene, ShapeItem, ShapeKind, Z_STEP,
};

use crate::fixtures::{
    add_all, erase, fill, id, item, line, permutation, rect_ring, stroke, stroke_with, text,
};

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<[f32; 2]> {
    vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn select(scene: &Scene, polygon: &[[f32; 2]]) -> Vec<penwise_core::item::ItemId> {
    lasso(scene, polygon, LayerSet::ALL, &LassoParams::DEFAULT)
}

#[test]
fn stroke_needs_three_quarters_inside() {
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(
        &mut scene,
        &[stroke(id(1), Z_STEP, [0.0, 50.0], [100.0, 50.0])],
    );
    assert_eq!(
        select(&scene, &rect(-10.0, 0.0, 90.0, 100.0)),
        vec![id(1)],
        "≈90% inside"
    );
    assert!(
        select(&scene, &rect(-10.0, 0.0, 60.0, 100.0)).is_empty(),
        "≈60% inside"
    );
    assert!(
        select(&scene, &rect(-10.0, 0.0, 60.0, 100.0)[..2]).is_empty(),
        "fewer than 3 points"
    );
}

#[test]
fn text_counts_corners_and_centre() {
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(
        &mut scene,
        &[text(id(1), Z_STEP, [0.0, 0.0], [100.0, 100.0])],
    );
    // Excludes only the bottom-right corner: 4 of 5 samples inside.
    let cut = [
        [-10.0, -10.0],
        [110.0, -10.0],
        [110.0, 80.0],
        [80.0, 110.0],
        [-10.0, 110.0],
    ];
    assert_eq!(select(&scene, &cut), vec![id(1)]);
    let strict = LassoParams {
        threshold: 0.85,
        ..LassoParams::DEFAULT
    };
    assert!(lasso(&scene, &cut, LayerSet::ALL, &strict).is_empty());
    // Excludes the bottom edge: 3 of 5.
    assert!(select(&scene, &rect(-10.0, -10.0, 110.0, 90.0)).is_empty());
}

#[test]
fn tiny_items_use_their_centroid() {
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(
        &mut scene,
        &[stroke(id(1), Z_STEP, [0.0, 50.0], [30.0, 50.0])],
    );
    let left_part = rect(-10.0, 0.0, 18.0, 100.0);
    assert!(
        select(&scene, &left_part).is_empty(),
        "≈60% inside, not tiny with the defaults"
    );
    let tiny = LassoParams {
        tiny_length: 50.0,
        ..LassoParams::DEFAULT
    };
    assert_eq!(
        lasso(&scene, &left_part, LayerSet::ALL, &tiny),
        vec![id(1)],
        "centroid inside"
    );
    let right_part = rect(18.0, 0.0, 40.0, 100.0);
    assert!(
        lasso(&scene, &right_part, LayerSet::ALL, &tiny).is_empty(),
        "centroid outside"
    );

    // A real dot: one sample, zero arc length.
    let dot = stroke_with(id(2), Z_STEP, Preset::Ballpoint, &[[300.0, 300.0]]);
    add_all(&mut scene, &[dot]);
    assert_eq!(
        select(&scene, &rect(290.0, 290.0, 310.0, 310.0)),
        vec![id(2)]
    );
    assert!(select(&scene, &rect(300.5, 290.0, 320.0, 310.0)).is_empty());
}

#[test]
fn masked_samples_do_not_count() {
    let pencil = stroke_with(
        id(1),
        Z_STEP,
        Preset::Pencil,
        &line([0.0, 50.0], [100.0, 50.0], 26),
    );
    let left = rect(-10.0, 0.0, 45.0, 100.0);
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, std::slice::from_ref(&pencil));
    assert!(select(&scene, &left).is_empty(), "≈45% of the whole stroke");
    let mask = erase(id(2), &pencil, vec![[60.0, 50.0], [110.0, 50.0]], 10.0);
    add_all(&mut scene, &[mask]);
    assert_eq!(
        select(&scene, &left),
        vec![id(1)],
        "≈90% of the unmasked samples"
    );
    let everything = rect(-50.0, -50.0, 200.0, 200.0);
    assert_eq!(
        select(&scene, &everything),
        vec![id(1)],
        "erase items are never selected"
    );
    let full = erase(id(3), &pencil, vec![[-20.0, 50.0], [120.0, 50.0]], 30.0);
    add_all(&mut scene, &[full]);
    assert!(
        select(&scene, &everything).is_empty(),
        "no unmasked samples → never selected"
    );
}

#[test]
fn shapes_and_fills_are_lassoed_by_their_outlines() {
    let mut shape = item(
        id(1),
        Z_STEP,
        ItemKind::Shape(ShapeItem {
            kind: ShapeKind::Ellipse,
            size: [40.0, 20.0],
            brush: Preset::Fineliner.params(),
            fill: None,
        }),
    );
    shape.transform = Affine::translate(300.0, 300.0);
    let f = fill(
        id(2),
        2 * Z_STEP,
        rect_ring([0.0, 0.0], [50.0, 50.0]),
        Vec::new(),
    );
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[shape, f]);
    assert_eq!(
        select(&scene, &rect(270.0, 280.0, 330.0, 320.0)),
        vec![id(1)]
    );
    assert!(
        select(&scene, &rect(270.0, 280.0, 300.0, 320.0)).is_empty(),
        "half the ellipse"
    );
    assert_eq!(select(&scene, &rect(-5.0, -5.0, 55.0, 55.0)), vec![id(2)]);
    assert!(select(&scene, &rect(-5.0, -5.0, 55.0, 30.0)).is_empty());
}

#[test]
fn lasso_is_insertion_order_independent() {
    let mut items = Vec::new();
    for i in 0..12u16 {
        let y = f32::from(i) * 10.0;
        items.push(stroke(
            id(10 + i),
            i64::from(i % 3) * Z_STEP,
            [0.0, y],
            [60.0, y + 5.0],
        ));
    }
    let polygon = [[-10.0, 15.0], [80.0, 15.0], [80.0, 95.0], [-10.0, 95.0]];
    let mut reference = None;
    for seed in [5, 6, 77] {
        let mut scene = Scene::new(PageKind::Infinite);
        for i in permutation(items.len(), seed) {
            add_all(&mut scene, std::slice::from_ref(&items[i]));
        }
        let picked = select(&scene, &polygon);
        assert!(!picked.is_empty());
        let keys: Vec<_> = picked
            .iter()
            .map(|&p| (scene.get(p).unwrap().z, p))
            .collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(*reference.get_or_insert(picked.clone()), picked);
    }
}
