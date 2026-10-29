//! Owner: slice b.
//!
//! Eraser index (Gate B item 4): same answers as a full scan; long gestures over many long strokes cost
//! ~linear work, measured by the timing-free segment-visit counter.

use super::index::{EraserIndex, take_test_count};
use super::{erase_strokes, path_touches_in};
use crate::edit::erase_precise;
use crate::edit::erase_split::uncovered_in;
use crate::item::{
    Affine, IdSource, Item, ItemId, ItemKind, Layer, LayerSet, PageKind, Scene, StrokeItem,
};
use crate::{Preset, Sample};

struct Counter(u128);

impl IdSource for Counter {
    fn next_id(&mut self) -> ItemId {
        self.0 += 1;
        ItemId(self.0.to_be_bytes())
    }
}

#[allow(clippy::cast_precision_loss, reason = "small test indices")]
fn fl(i: u32) -> f32 {
    i as f32
}

/// Ballpoint stroke `n` along `y` from x = 50, 0.8 px per sample, `len` samples.
fn line(n: u32, y: f32, len: u32) -> Item {
    let samples = (0..len)
        .map(|i| {
            Sample {
                x: 50.0 + 0.8 * fl(i),
                y,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 2075,
            }
            .quantize()
        })
        .collect();
    Item {
        id: ItemId(u128::from(n).to_be_bytes()),
        layer: Layer::Ink,
        z: i64::from(n) << 16,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Ballpoint.params(),
            seed: 7,
            samples,
            trim: None,
            binding: None,
        }),
    }
}

/// `count` strokes of `len` samples at y = 20k + 20.
fn lines(count: u32, len: u32) -> (Scene, Vec<ItemId>) {
    let mut scene = Scene::new(PageKind::Infinite);
    let mut ids = Vec::new();
    for k in 0..count {
        let item = line(k + 1, 20.0 * fl(k) + 20.0, len);
        ids.push(item.id);
        scene.apply(&crate::edit::Edit::Add(item)).unwrap();
    }
    (scene, ids)
}

#[test]
fn long_gesture_over_many_strokes_does_near_linear_work() {
    let (scene, _) = lines(200, 500);
    // Boustrophedon scribble: 200 rows × 10 points across x ∈ [0, 500], each row midway between two
    // strokes; the row-to-row connectors run at x = 0 / 500, past the stroke ends (x ∈ [50, 450)).
    let mut path = Vec::new();
    for r in 0..200u32 {
        let y = 20.0 * fl(r) + 30.0;
        for j in 0..10u32 {
            let t = 500.0 * fl(j) / 9.0;
            path.push([if r % 2 == 0 { t } else { 500.0 - t }, y]);
        }
    }
    assert_eq!(path.len(), 2000);

    // A full scan visits 200 strokes × 499 centreline segments × 1999 eraser segments ≈ 2·10^8.
    let bound = 200 * 500 * 32;
    take_test_count();
    assert!(erase_strokes(&scene, &path, 1.0, LayerSet::INK).is_empty());
    let visits = take_test_count();
    assert!(visits <= bound, "stroke eraser visited {visits} segments");
    assert!(erase_precise(&scene, &path, 1.0, LayerSet::INK, &mut Counter(1000)).is_empty());
    let visits = take_test_count();
    assert!(visits <= bound, "precise eraser visited {visits} segments");

    // A lead-in from (250, 10) to the first row crosses only the stroke at y = 20; still near-linear.
    path.insert(0, [250.0, 10.0]);
    assert_eq!(
        erase_strokes(&scene, &path, 1.0, LayerSet::INK),
        vec![crate::edit::Edit::Delete(ItemId(1u128.to_be_bytes()))]
    );
    assert!(take_test_count() <= bound);
}

#[test]
fn bucketed_index_matches_full_scan() {
    let (scene, ids) = lines(20, 300);
    // Deterministic jittered diagonal crossing most strokes.
    let mut s = 0x2545_f491u32;
    let mut path = Vec::new();
    for i in 0..400u32 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let jitter = fl(s >> 24) / 255.0 * 30.0 - 15.0;
        path.push([40.0 + 0.7 * fl(i) + jitter, 20.0 + fl(i) + jitter]);
    }
    let grid = EraserIndex::new(&path);
    let full = EraserIndex::linear(&path);
    for radius in [0.5f32, 1.0, 4.0] {
        let mut touched = 0;
        for &id in &ids {
            let hit = path_touches_in(&scene, id, &path, radius, &grid);
            assert_eq!(hit, path_touches_in(&scene, id, &path, radius, &full));
            let Some(ItemKind::Stroke(stroke)) = scene.get(id).map(|item| &item.kind) else {
                panic!("stroke expected");
            };
            let visible = scene.visible_ranges(id);
            let kept = uncovered_in(stroke, &Affine::IDENTITY, &visible, &grid, radius);
            assert_eq!(
                kept,
                uncovered_in(stroke, &Affine::IDENTITY, &visible, &full, radius)
            );
            touched += usize::from(hit);
        }
        assert!(touched > 5, "{touched} touched");
    }
}
