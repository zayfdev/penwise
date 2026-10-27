//! A piece cut from a stroke keeps the smoothing state of its origin (`Trim::warm`): its centreline is the
//! origin's centreline over the piece's range, not a re-smoothed approximation (≤ 1/16 px at 480 px/s).
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::edit::erase_split::{split_edits, uncovered_ranges};
use penwise_core::edit::{Edit, erase_precise};
use penwise_core::item::{
    Affine, Centreline, IdSource, Item, ItemId, ItemKind, Layer, LayerSet, PageKind, Scene,
    StrokeItem,
};
use penwise_core::{Preset, Sample};

const TOLERANCE_PX: f32 = 1.0 / 16.0;

struct Counter(u128);

impl IdSource for Counter {
    fn next_id(&mut self) -> ItemId {
        self.0 += 1;
        ItemId(self.0.to_be_bytes())
    }
}

/// A circular arc (radius 150, 5.7 rad, so a radial eraser crosses it once) drawn at 480 px/s on a 482 Hz digitizer, pressure swelling and fading.
fn arc(n: u32) -> Vec<Sample> {
    let r = 150.0_f32;
    let omega = 480.0 / r;
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "small test indices")]
            let k = i as f32;
            let t = k * 2075.0e-6;
            let a = omega * t;
            Sample {
                x: 300.0 + r * libm::cosf(a),
                y: 300.0 + r * libm::sinf(a),
                pressure: 0.5 + 0.4 * libm::sinf(k * 0.05),
                tilt: 0.3,
                azimuth: 1.0,
                t_us: i * 2075,
            }
            .quantize()
        })
        .collect()
}

fn stroke_item(n: u128, transform: Affine) -> Item {
    Item {
        id: ItemId(n.to_be_bytes()),
        layer: Layer::Ink,
        z: 1 << 16,
        transform,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Fountain.params(),
            seed: 5,
            samples: arc(880),
            trim: None,
        }),
    }
}

fn stroke_of(item: &Item) -> &StrokeItem {
    match &item.kind {
        ItemKind::Stroke(s) => s,
        _ => panic!("stroke expected"),
    }
}

/// Eraser crossing the arc radially at angle `a`.
fn radial(a: f32) -> Vec<[f32; 2]> {
    let (c, s) = (libm::cosf(a), libm::sinf(a));
    vec![
        [300.0 + 130.0 * c, 300.0 + 130.0 * s],
        [300.0 + 170.0 * c, 300.0 + 170.0 * s],
    ]
}

fn pieces(item: &Item, eraser: &[[f32; 2]], ids: &mut Counter) -> Vec<Item> {
    let stroke = stroke_of(item);
    let kept =
        uncovered_ranges(stroke, &item.transform, &[stroke.own_range()], eraser, 4.0).unwrap();
    split_edits(item, stroke, &kept, ids)
        .into_iter()
        .filter_map(|e| match e {
            Edit::Add(piece) => Some(piece),
            _ => None,
        })
        .collect()
}

/// Largest distance between a piece's centreline and the original stroke's over the same range, and the
/// largest half-width difference.
fn deviation(original: &Item, piece: &Item) -> (f32, f32) {
    let (whole, part) = (stroke_of(original), stroke_of(piece));
    let range = part.own_range();
    let mut a = Centreline::default();
    let mut b = Centreline::default();
    whole.centreline(&original.transform, &[range], &mut a);
    part.centreline(&piece.transform, &[range], &mut b);
    assert_eq!(
        a.points.len(),
        b.points.len(),
        "same points over the same range"
    );
    assert!(b.points.len() > 20, "a real stretch of ink");
    let mut pos = 0.0_f32;
    let mut width = 0.0_f32;
    for (p, q) in a.points.iter().zip(&b.points) {
        assert_eq!(p.param, q.param);
        pos = penwise_core::float::max(pos, libm::hypotf(p.x - q.x, p.y - q.y));
        width = penwise_core::float::max(width, (p.half_width - q.half_width).abs());
    }
    (pos, width)
}

#[test]
fn piece_centreline_matches_the_origin_within_a_sixteenth_of_a_pixel() {
    for transform in [Affine::IDENTITY, Affine::translate(40.0, -12.0)] {
        let item = stroke_item(1, transform);
        let mut ids = Counter(100);
        let made = pieces(&item, &radial(1.4), &mut ids);
        assert_eq!(made.len(), 2);
        for piece in &made {
            let trim = stroke_of(piece).trim.as_ref().unwrap();
            assert_eq!(trim.warm.is_some(), trim.sample_offset > 0);
            let (pos, width) = deviation(&item, piece);
            assert!(pos <= TOLERANCE_PX, "centreline moved {pos} px");
            assert!(width <= TOLERANCE_PX, "half-width changed by {width}");
        }
    }
}

#[test]
fn a_piece_of_a_piece_still_matches_the_origin() {
    let item = stroke_item(1, Affine::IDENTITY);
    let mut ids = Counter(100);
    let first = pieces(&item, &radial(1.0), &mut ids);
    // The second cut lands inside the right-hand piece.
    let right = first.last().unwrap();
    let second = pieces(right, &radial(2.6), &mut ids);
    assert_eq!(second.len(), 2);
    for piece in &second {
        let (pos, width) = deviation(&item, piece);
        assert!(pos <= TOLERANCE_PX, "centreline moved {pos} px");
        assert!(width <= TOLERANCE_PX, "half-width changed by {width}");
    }
    let trim = stroke_of(&second[1]).trim.as_ref().unwrap();
    assert!(trim.sample_offset > stroke_of(right).trim.as_ref().unwrap().sample_offset);
}

#[test]
fn piece_outline_matches_the_origin_away_from_the_cut() {
    let item = stroke_item(1, Affine::IDENTITY);
    let whole = {
        let s = stroke_of(&item);
        s.build_geometry(&item.transform, &[s.own_range()])
    };
    let penwise_core::StrokeGeometry::Outline { outline: ring, .. } = &whole else {
        panic!("outline expected")
    };
    let made = pieces(&item, &radial(1.4), &mut Counter(100));
    for piece in &made {
        let s = stroke_of(piece);
        let g = s.build_geometry(&piece.transform, &[s.own_range()]);
        let penwise_core::StrokeGeometry::Outline { outline, .. } = &g else {
            panic!("outline expected")
        };
        let cut = radial(1.4)[0];
        let first = stroke_of(&item).samples[0];
        let last = *stroke_of(&item).samples.last().unwrap();
        let ends = [cut, [first.x, first.y], [last.x, last.y]];
        let mut checked = 0;
        for p in &outline.points {
            // Skip the caps: the cut end and the origin's natural ends, whose outline vertices are placed by
            // the engine's decimation from wherever the run starts.
            if ends
                .iter()
                .any(|e| libm::hypotf(p[0] - e[0], p[1] - e[1]) < 60.0)
            {
                continue;
            }
            let d = penwise_core::edit::proximity::point_ring_dist(*p, &ring.points);
            assert!(
                d <= TOLERANCE_PX,
                "outline point {p:?} off the origin by {d}"
            );
            checked += 1;
        }
        assert!(checked > 20, "only {checked} outline points compared");
    }
}

#[test]
fn scene_erase_gesture_gives_the_same_ink_as_the_uncut_stroke() {
    let item = stroke_item(1, Affine::IDENTITY);
    let mut scene = Scene::new(PageKind::Infinite);
    scene.apply(&Edit::Add(item.clone())).unwrap();
    let edits = erase_precise(&scene, &radial(2.0), 4.0, LayerSet::ALL, &mut Counter(100));
    scene.apply_all(&edits).unwrap();
    let live = scene.render_order(LayerSet::ALL);
    assert_eq!(live.len(), 2);
    for id in live {
        let piece = scene.get(id).unwrap();
        let (pos, _) = deviation(&item, piece);
        assert!(pos <= TOLERANCE_PX);
    }
}

/// FNV-1a 64 of the canonical bytes of both pieces' geometry from one cut (and of a second-generation cut).
/// Runs natively and on wasm32 (`wasm-pack test --node`), so the warm-state path is pinned bit-identical on both.
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
fn piece_geometry_is_bit_identical_on_every_target() {
    let item = stroke_item(1, Affine::translate(3.0, 4.0));
    let mut ids = Counter(100);
    let first = pieces(&item, &radial(1.0), &mut ids);
    let second = pieces(first.last().unwrap(), &radial(2.6), &mut ids);
    let mut bytes = Vec::new();
    for piece in first.iter().chain(&second) {
        let s = stroke_of(piece);
        let g = s.build_geometry(&piece.transform, &[s.own_range()]);
        bytes.extend(g.canonical_bytes().unwrap());
    }
    assert_eq!(
        penwise_core::hash::fnv1a64(&bytes),
        PINNED_HASH,
        "piece geometry changed (native and wasm must agree)"
    );
}

const PINNED_HASH: u64 = 16_213_410_617_085_589_966;
