//! Valid items and small helpers for the slice a scene, hit, lasso and selection tests.

use penwise_core::codec::decode_pwrec;
use penwise_core::codec::pwrec::RunKind;
use penwise_core::edit::Edit;
use penwise_core::item::{
    Affine, EraseItem, FillItem, IdSource, Item, ItemId, ItemKind, Layer, LayerSet, Paint, Polygon,
    Scene, StrokeItem, TextItem,
};
use penwise_core::{Bounds, Color, Preset, Sample};

pub const RED: Color = Color {
    r: 200,
    g: 10,
    b: 20,
    a: 255,
};

pub fn id(n: u16) -> ItemId {
    let mut bytes = [0_u8; 16];
    bytes[0] = 0x0A;
    bytes[14..].copy_from_slice(&n.to_be_bytes());
    ItemId(bytes)
}

/// Hands out `id(start)`, `id(start + 1)`, … .
pub struct Counter(pub u16);

impl IdSource for Counter {
    fn next_id(&mut self) -> ItemId {
        self.0 += 1;
        id(self.0 - 1)
    }
}

pub fn item(id: ItemId, z: i64, kind: ItemKind) -> Item {
    Item {
        id,
        layer: Layer::Ink,
        z,
        transform: Affine::IDENTITY,
        kind,
    }
}

/// Quantized samples through `points`, constant pressure, 2 ms apart.
pub fn samples(points: &[[f32; 2]]) -> Vec<Sample> {
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

/// Points every `step` px from `from` to `to` (both included).
pub fn line(from: [f32; 2], to: [f32; 2], count: u16) -> Vec<[f32; 2]> {
    let n = f32::from(count - 1);
    (0..count)
        .map(|i| {
            let t = f32::from(i) / n;
            [
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
            ]
        })
        .collect()
}

pub fn stroke_with(id: ItemId, z: i64, preset: Preset, points: &[[f32; 2]]) -> Item {
    item(
        id,
        z,
        ItemKind::Stroke(StrokeItem {
            params: preset.params(),
            seed: 0x5EED,
            samples: samples(points),
            trim: None,
            binding: None,
        }),
    )
}

/// Straight ballpoint stroke.
pub fn stroke(id: ItemId, z: i64, from: [f32; 2], to: [f32; 2]) -> Item {
    stroke_with(id, z, Preset::Ballpoint, &line(from, to, 41))
}

pub fn text(id: ItemId, z: i64, at: [f32; 2], size: [f32; 2]) -> Item {
    let mut it = item(
        id,
        z,
        ItemKind::Text(TextItem {
            content: "hello".into(),
            size,
            font_size: 16.0,
            paint: Paint {
                color: Color {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                },
                opacity: 1.0,
            },
            sources: Vec::new(),
        }),
    );
    it.transform = Affine::translate(at[0], at[1]);
    it
}

pub fn fill(id: ItemId, z: i64, outer: Vec<[f32; 2]>, holes: Vec<Vec<[f32; 2]>>) -> Item {
    item(
        id,
        z,
        ItemKind::Fill(FillItem {
            polygons: vec![Polygon { outer, holes }],
            paint: Paint {
                color: RED,
                opacity: 0.5,
            },
        }),
    )
}

pub fn erase(id: ItemId, target: &Item, path: Vec<[f32; 2]>, radius: f32) -> Item {
    Item {
        id,
        layer: target.layer,
        z: target.z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Erase(EraseItem {
            target: target.id,
            path,
            radius,
        }),
    }
}

pub fn add_all(scene: &mut Scene, items: &[Item]) {
    for it in items {
        scene.apply(&Edit::Add(it.clone())).unwrap();
    }
}

/// Deterministic permutation of `0..n` (xorshift-driven Fisher–Yates).
pub fn permutation(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    let mut state = seed | 1;
    for i in (1..n).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = usize::try_from(state % (u64::try_from(i).unwrap() + 1)).unwrap();
        order.swap(i, j);
    }
    order
}

/// Brute-force version of `Scene::query` over every id in `ids`.
pub fn brute_query(scene: &Scene, ids: &[ItemId], area: &Bounds, layers: LayerSet) -> Vec<ItemId> {
    let mut hits: Vec<(i64, ItemId)> = ids
        .iter()
        .filter_map(|&i| scene.get(i).map(|it| (i, it)))
        .filter(|(_, it)| !matches!(it.kind, ItemKind::Erase(_)) && layers.contains(it.layer))
        .filter(|(i, _)| scene.bounds(*i).is_some_and(|b| b.intersects(area)))
        .map(|(i, it)| (it.z, i))
        .collect();
    hits.sort_unstable();
    hits.into_iter().map(|(_, i)| i).collect()
}

/// Contact strokes of a recorded fixture, quantized.
pub fn fixture_strokes(bytes: &[u8]) -> Vec<Vec<Sample>> {
    let rec = decode_pwrec(bytes).unwrap();
    let ranges = rec.digitizer_ranges().unwrap();
    rec.strokes
        .iter()
        .filter(|s| s.kind == RunKind::Contact)
        .map(|s| {
            let mut out = Vec::new();
            s.to_samples(&ranges, &mut out);
            out.into_iter().map(Sample::quantize).collect()
        })
        .collect()
}

pub fn rect_ring(min: [f32; 2], max: [f32; 2]) -> Vec<[f32; 2]> {
    vec![min, [max[0], min[1]], max, [min[0], max[1]]]
}
