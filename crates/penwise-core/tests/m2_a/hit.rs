//! Tap hit testing: radius edges, tapered capsules on a real recording, masks, topmost.

use penwise_core::Preset;
use penwise_core::edit::hit::item_hit;
use penwise_core::edit::{hit_test, topmost};
use penwise_core::item::{
    ItemGeometry, ItemKind, Layer, LayerSet, PageKind, Scene, StrokeItem, Z_STEP,
};

use crate::fixtures::{
    add_all, erase, fill, fixture_strokes, id, item, line, permutation, rect_ring, stroke,
    stroke_with, text,
};

const SENTENCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/pwrec/01-sentence.pwrec"
));

#[test]
fn stroke_hit_edges_follow_half_width_plus_tolerance() {
    let s = stroke(id(1), Z_STEP, [0.0, 100.0], [200.0, 100.0]);
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[s]);
    let Some(ItemGeometry::Stroke { centreline, .. }) = scene.geometry(id(1)) else {
        panic!("no stroke geometry")
    };
    let mid = centreline.points[centreline.points.len() / 2];
    for tol in [0.0, 3.0] {
        let reach = mid.half_width + tol;
        assert!(
            item_hit(&scene, id(1), [mid.x, mid.y + reach - 0.01], tol),
            "inside edge, tol {tol}"
        );
        assert!(
            !item_hit(&scene, id(1), [mid.x, mid.y + reach + 0.01], tol),
            "outside edge, tol {tol}"
        );
        assert_eq!(
            hit_test(&scene, [mid.x, mid.y - reach + 0.01], tol, LayerSet::ALL),
            vec![id(1)]
        );
        assert!(hit_test(&scene, [mid.x, mid.y - reach - 0.01], tol, LayerSet::ALL).is_empty());
    }
}

#[test]
fn text_and_fill_edges() {
    let t = text(id(1), Z_STEP, [100.0, 100.0], [50.0, 20.0]);
    let f = fill(
        id(2),
        Z_STEP,
        rect_ring([300.0, 300.0], [400.0, 400.0]),
        vec![vec![
            [330.0, 330.0],
            [330.0, 370.0],
            [370.0, 370.0],
            [370.0, 330.0],
        ]],
    );
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[t, f]);
    assert!(item_hit(&scene, id(1), [120.0, 110.0], 0.0));
    assert!(!item_hit(&scene, id(1), [153.0, 110.0], 2.5));
    assert!(item_hit(&scene, id(1), [153.0, 110.0], 3.0));
    assert!(item_hit(&scene, id(2), [310.0, 310.0], 0.0));
    assert!(
        !item_hit(&scene, id(2), [350.0, 350.0], 0.0),
        "hole is not filled"
    );
    assert!(
        item_hit(&scene, id(2), [350.0, 365.0], 5.0),
        "within tol of the hole ring"
    );
    assert!(!item_hit(&scene, id(2), [405.0, 350.0], 4.9));
    assert!(item_hit(&scene, id(2), [405.0, 350.0], 5.0));
}

#[test]
fn tapered_capsule_hits_a_recorded_stroke() {
    let strokes = fixture_strokes(SENTENCE);
    let samples = strokes
        .into_iter()
        .find(|s| s.len() > 20)
        .expect("a long stroke");
    let s = item(
        id(1),
        Z_STEP,
        ItemKind::Stroke(StrokeItem {
            params: Preset::Fountain.params(),
            seed: 3,
            samples,
            trim: None,
            binding: None,
        }),
    );
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[s]);
    let Some(ItemGeometry::Stroke { centreline, .. }) = scene.geometry(id(1)) else {
        panic!("no geometry")
    };
    assert!(!centreline.points.is_empty());
    for p in &centreline.points {
        assert!(
            item_hit(&scene, id(1), [p.x, p.y], 0.0),
            "centreline point {p:?}"
        );
        assert_eq!(
            hit_test(&scene, [p.x, p.y], 0.0, LayerSet::ALL),
            vec![id(1)]
        );
    }
    let b = scene.bounds(id(1)).unwrap();
    assert!(
        hit_test(
            &scene,
            [b.max_x + 20.0, b.max_y + 20.0],
            10.0,
            LayerSet::ALL
        )
        .is_empty()
    );
    assert!(hit_test(&scene, [b.min_x - 1.0, b.min_y - 1.0], 0.5, LayerSet::ALL).is_empty());
}

#[test]
fn masked_ink_is_not_hit() {
    let pencil = stroke_with(
        id(1),
        Z_STEP,
        Preset::Pencil,
        &line([0.0, 50.0], [100.0, 50.0], 26),
    );
    let mask = erase(id(2), &pencil, vec![[40.0, 50.0], [60.0, 50.0]], 8.0);
    let mut scene = Scene::new(PageKind::Infinite);
    add_all(&mut scene, &[pencil, mask]);
    assert!(item_hit(&scene, id(1), [20.0, 50.0], 0.0));
    assert!(!item_hit(&scene, id(1), [50.0, 50.0], 0.0));
    assert!(hit_test(&scene, [50.0, 50.0], 0.0, LayerSet::ALL).is_empty());
    assert!(
        !item_hit(&scene, id(2), [50.0, 50.0], 0.0),
        "erase items are never hit"
    );
}

#[test]
fn topmost_uses_band_then_z_then_id() {
    let low = text(id(1), 5 * Z_STEP, [0.0, 0.0], [50.0, 50.0]);
    let high = text(id(2), 9 * Z_STEP, [0.0, 0.0], [50.0, 50.0]);
    let tie = text(id(3), 9 * Z_STEP, [0.0, 0.0], [50.0, 50.0]);
    // A fill with the largest z still paints under everything else.
    let under = fill(
        id(4),
        99 * Z_STEP,
        rect_ring([0.0, 0.0], [50.0, 50.0]),
        Vec::new(),
    );
    let mut agent = text(id(5), 100 * Z_STEP, [0.0, 0.0], [50.0, 50.0]);
    agent.layer = Layer::Agent;
    let items = [low, high, tie, under, agent];
    let mut reference = None;
    for seed in [1, 2, 3] {
        let mut scene = Scene::new(PageKind::Infinite);
        for i in permutation(items.len(), seed) {
            add_all(&mut scene, std::slice::from_ref(&items[i]));
        }
        let hits = hit_test(&scene, [25.0, 25.0], 0.0, LayerSet::INK);
        assert_eq!(hits, vec![id(1), id(2), id(3), id(4)]);
        assert_eq!(topmost(&scene, &hits), Some(id(3)));
        assert_eq!(
            topmost(&scene, &hit_test(&scene, [25.0, 25.0], 0.0, LayerSet::ALL)),
            Some(id(5))
        );
        assert_eq!(topmost(&scene, &[]), None);
        assert_eq!(*reference.get_or_insert(hits.clone()), hits);
    }
}
