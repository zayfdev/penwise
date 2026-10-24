//! Owner: slice b.
//!
//! Eraser behaviour (M2 contract "b — erasers and trims"): untrimmed geometry equals M1, precise splits keep
//! the ink outside the cut and add no taper at cut ends, chunked and concurrent erases converge, stamp strokes
//! get masks in their local frame, fills are cut, the stroke eraser deletes whole items, undo restores.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test harness: failures are reported by panicking"
)]

use penwise_core::brush::Color;
use penwise_core::edit::erase_fill::cut_polygons;
use penwise_core::edit::erase_split::{bisect_cut, split_edits, uncovered_ranges};
use penwise_core::edit::proximity::point_ring_dist;
use penwise_core::edit::{Edit, UndoStack, erase_precise, erase_strokes};
use penwise_core::item::{
    Affine, CentrePoint, Centreline, FillItem, IdSource, Item, ItemId, ItemKind, Layer, LayerSet,
    PageKind, Paint, Polygon, SampleParam, SampleRange, Scene, StrokeItem,
};
use penwise_core::{BrushParams, Preset, Sample, StrokeBuilder, StrokeGeometry};

/// Counter id source (ids above every fixture id).
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

/// Ballpoint with long tapers, so a re-tapered cut end would be obvious.
fn tapered_ballpoint() -> BrushParams {
    let mut p = Preset::Ballpoint.params();
    p.outline.taper_start = 20.0;
    p.outline.taper_end = 20.0;
    p
}

/// A slow, nearly horizontal stroke at constant pressure from `(100, 200)`, 0.1 px per sample at 482 Hz,
/// `n` samples (slow so One Euro warm-up differences in trimmed pieces die out quickly).
fn slow_line(n: u32) -> Vec<Sample> {
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss, reason = "small test indices")]
            let k = i as f32;
            Sample {
                x: 100.0 + 0.1 * k,
                y: 200.0 + 0.02 * k,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: i * 2075,
            }
            .quantize()
        })
        .collect()
}

fn stroke_item(n: u128, params: BrushParams, samples: Vec<Sample>, transform: Affine) -> Item {
    Item {
        id: id(n),
        layer: Layer::Ink,
        z: i64::try_from(n).unwrap() << 16,
        transform,
        kind: ItemKind::Stroke(StrokeItem {
            params,
            seed: 7,
            samples,
            trim: None,
        }),
    }
}

fn stroke_of(item: &Item) -> &StrokeItem {
    match &item.kind {
        ItemKind::Stroke(s) => s,
        _ => panic!("not a stroke"),
    }
}

fn scene_with(items: &[Item]) -> Scene {
    let mut scene = Scene::new(PageKind::Infinite);
    for item in items {
        scene.apply(&Edit::Add(item.clone())).unwrap();
    }
    scene
}

/// Vertical eraser crossing the slow line at `x`.
fn vertical(x: f32) -> Vec<[f32; 2]> {
    vec![[x, 180.0], [x, 200.0], [x, 220.0], [x, 240.0]]
}

fn outline_of(g: &StrokeGeometry) -> &penwise_core::Outline {
    match g {
        StrokeGeometry::Outline { outline, .. } => outline,
        StrokeGeometry::Stamps { .. } => panic!("outline expected"),
    }
}

fn centre(stroke: &StrokeItem, transform: &Affine) -> Vec<CentrePoint> {
    let mut line = Centreline::default();
    stroke.centreline(transform, &[stroke.own_range()], &mut line);
    line.points
}

fn contains(ranges: &[SampleRange], u: SampleParam) -> bool {
    ranges.iter().any(|r| r.start <= u && u <= r.end)
}

/// Visible ranges of every live member of the family rooted at `origin`, sorted.
fn family_union(scene: &Scene, origin: ItemId) -> Vec<SampleRange> {
    let mut all: Vec<SampleRange> = scene
        .family(origin)
        .into_iter()
        .flat_map(|m| scene.visible_ranges(m))
        .collect();
    all.sort_by_key(|r| (r.start, r.end));
    all.dedup();
    all
}

#[test]
fn untrimmed_identity_geometry_equals_builder_bytes() {
    for preset in [
        Preset::Ballpoint,
        Preset::Fountain,
        Preset::Pencil,
        Preset::Spray,
    ] {
        let samples = slow_line(300);
        let item = stroke_item(1, preset.params(), samples.clone(), Affine::IDENTITY);
        let stroke = stroke_of(&item);
        let ours = stroke.build_geometry(&Affine::IDENTITY, &[stroke.own_range()]);
        let m1 = StrokeBuilder::build(&preset.params(), 7, &samples);
        assert_eq!(
            ours.canonical_bytes().unwrap(),
            m1.canonical_bytes().unwrap(),
            "{preset:?}"
        );
    }
}

#[test]
fn middle_split_keeps_ink_outside_the_cut_without_taper() {
    // 200 px long: the right piece has > 60 px of ink well past its warm-up window.
    let item = stroke_item(1, tapered_ballpoint(), slow_line(2000), Affine::IDENTITY);
    let stroke = stroke_of(&item);
    let eraser = vertical(150.0);
    let radius = 3.0;
    let kept = uncovered_ranges(
        stroke,
        &Affine::IDENTITY,
        &[stroke.own_range()],
        &eraser,
        radius,
    )
    .expect("eraser crosses the stroke");
    assert_eq!(kept.len(), 2);
    assert!(!kept[0].start_cut && kept[0].end_cut && kept[1].start_cut && !kept[1].end_cut);

    // Coverage rule on the original centreline: erased samples are hidden, clearly free ones are kept.
    for c in centre(stroke, &Affine::IDENTITY) {
        let d = (c.x - 150.0).abs();
        if d < radius + c.half_width {
            assert!(
                !contains(&kept, c.param),
                "erased sample {:?} visible",
                c.param
            );
        } else if d > radius + c.half_width + 0.5 {
            assert!(contains(&kept, c.param), "free sample {:?} hidden", c.param);
        }
    }

    let edits = split_edits(&item, stroke, &kept, &mut Counter(100));
    assert_eq!(edits.len(), 3);
    assert_eq!(edits[0], Edit::Delete(item.id));
    let original = stroke.build_geometry(&Affine::IDENTITY, &[stroke.own_range()]);
    let ring = &outline_of(&original).points;
    for (k, edit) in edits[1..].iter().enumerate() {
        let Edit::Add(piece_item) = edit else {
            panic!("piece add expected")
        };
        let piece = stroke_of(piece_item);
        let trim = piece.trim.as_ref().unwrap();
        assert_eq!(trim.origin, item.id);
        assert_eq!(trim.visible, kept);
        assert_eq!(trim.own, u32::try_from(k).unwrap());
        let geometry = piece.build_geometry(&Affine::IDENTITY, &[piece.own_range()]);
        let outline = outline_of(&geometry);
        let line = centre(piece, &Affine::IDENTITY);
        let cut = (if k == 0 { line.last() } else { line.first() }).unwrap();

        // No taper at the cut: the outline is still full width right at the cut point.
        let (lo, hi) = outline
            .points
            .iter()
            .filter(|p| (p[0] - cut.x).abs() <= 1.0)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                (
                    if p[1] < lo { p[1] } else { lo },
                    if p[1] > hi { p[1] } else { hi },
                )
            });
        assert!(
            hi - lo >= 1.6 * cut.half_width,
            "piece {k}: width {} at cut",
            hi - lo
        );

        // Away from the cut the ink is unchanged (within 1/16 px of the original outline).
        for p in &outline.points {
            let far = if k == 0 {
                p[0] < cut.x - 15.0
            } else {
                p[0] > cut.x + 60.0
            };
            if far {
                let d = point_ring_dist(*p, ring);
                assert!(d <= 1.0 / 16.0, "piece {k}: point {p:?} moved by {d}");
            }
        }
    }
}

#[test]
fn bisect_cut_with_reversed_or_empty_bounds_returns_a() {
    let item = stroke_item(1, tapered_ballpoint(), slow_line(400), Affine::IDENTITY);
    let stroke = stroke_of(&item);
    // Sample 300 sits at x = 130 (under the eraser), sample 100 at x = 110 (clear): coverage flips between
    // them, so a reversed pair used to underflow `hi - lo`.
    let (covered, clear) = (SampleParam::at_index(300), SampleParam::at_index(100));
    let eraser = vertical(130.0);
    let cut = |a, b| bisect_cut(stroke, &Affine::IDENTITY, a, b, &eraser, 3.0);
    assert_eq!(cut(covered, clear), covered);
    assert_eq!(cut(clear, clear), clear);
    // The forward pair still bisects to a point strictly inside the segment.
    let forward = cut(clear, covered);
    assert!(clear < forward && forward < covered, "{forward:?}");
}

#[test]
fn chunked_erase_matches_single_erase() {
    let item = stroke_item(1, tapered_ballpoint(), slow_line(1000), Affine::IDENTITY);
    // Down through x = 170, along below the stroke, up through x = 130.
    let path = vec![
        [170.0, 180.0],
        [170.0, 200.0],
        [170.0, 240.0],
        [150.0, 240.0],
        [130.0, 240.0],
        [130.0, 200.0],
        [130.0, 180.0],
    ];
    let layers = LayerSet::INK;

    let mut single = scene_with(std::slice::from_ref(&item));
    let edits = erase_precise(&single, &path, 3.0, layers, &mut Counter(100));
    single.apply_all(&edits).unwrap();

    let mut chunked = scene_with(std::slice::from_ref(&item));
    let mut ids = Counter(200);
    for chunk in [&path[..4], &path[3..]] {
        let edits = erase_precise(&chunked, chunk, 3.0, layers, &mut ids);
        chunked.apply_all(&edits).unwrap();
    }

    let a = family_union(&single, item.id);
    assert_eq!(a.len(), 3);
    assert_eq!(a, family_union(&chunked, item.id));
}

#[test]
fn concurrent_splits_converge_and_never_resurrect_ink() {
    let item = stroke_item(1, tapered_ballpoint(), slow_line(1000), Affine::IDENTITY);
    let stroke = stroke_of(&item).clone();
    let base = scene_with(std::slice::from_ref(&item));
    let layers = LayerSet::INK;
    let ea = erase_precise(&base, &vertical(130.0), 3.0, layers, &mut Counter(100));
    let eb = erase_precise(&base, &vertical(170.0), 3.0, layers, &mut Counter(200));
    assert!(!ea.is_empty() && !eb.is_empty());

    let mut ab = base.clone();
    ab.apply_all(&ea).unwrap();
    ab.apply_all(&eb).unwrap();
    let mut ba = base.clone();
    ba.apply_all(&eb).unwrap();
    ba.apply_all(&ea).unwrap();

    let live = ab.render_order(LayerSet::ALL);
    assert_eq!(live, ba.render_order(LayerSet::ALL));
    assert_eq!(live.len(), 4);
    for &m in &live {
        assert_eq!(ab.visible_ranges(m), ba.visible_ranges(m));
        assert_eq!(ab.geometry(m), ba.geometry(m));
    }

    let union = family_union(&ab, item.id);
    for c in centre(&stroke, &Affine::IDENTITY) {
        let hit = [130.0f32, 170.0]
            .iter()
            .any(|x| (c.x - x).abs() < 3.0 + c.half_width);
        if hit {
            assert!(
                !contains(&union, c.param),
                "erased ink at {:?} resurrected",
                c.param
            );
        }
    }
}

#[test]
fn stamp_stroke_precise_erase_adds_local_frame_mask() {
    let shift = Affine::translate(10.0, 5.0);
    let item = stroke_item(1, Preset::Pencil.params(), slow_line(600), shift);
    let scene = scene_with(std::slice::from_ref(&item));
    let path = vertical(140.0);
    let edits = erase_precise(&scene, &path, 4.0, LayerSet::INK, &mut Counter(100));
    assert_eq!(edits.len(), 1);
    let Edit::Add(mask_item) = &edits[0] else {
        panic!("mask add expected")
    };
    assert_eq!(mask_item.id, id(101));
    assert_eq!((mask_item.layer, mask_item.z), (item.layer, item.z));
    assert!(mask_item.transform.bit_eq(&Affine::IDENTITY));
    let ItemKind::Erase(mask) = &mask_item.kind else {
        panic!("erase item expected")
    };
    assert_eq!(mask.target, item.id);
    assert_eq!(mask.radius.to_bits(), 4.0f32.to_bits());
    assert!(!mask.path.is_empty());
    for p in &mask.path {
        assert!(
            path.contains(&[p[0] + 10.0, p[1] + 5.0]),
            "{p:?} not in the local frame"
        );
    }

    let mut scene = scene;
    scene.apply_all(&edits).unwrap();
    assert_eq!(scene.masks(item.id), vec![mask_item.id]);
}

#[test]
fn fully_masked_stamp_stroke_is_deleted() {
    let item = stroke_item(1, Preset::Pencil.params(), slow_line(50), Affine::IDENTITY);
    let scene = scene_with(std::slice::from_ref(&item));
    let edits = erase_precise(
        &scene,
        &[[102.5, 200.5]],
        60.0,
        LayerSet::INK,
        &mut Counter(100),
    );
    assert_eq!(edits, vec![Edit::Delete(item.id)]);
}

fn square(x0: f32, y0: f32, side: f32) -> Polygon {
    Polygon {
        outer: vec![
            [x0, y0],
            [x0 + side, y0],
            [x0 + side, y0 + side],
            [x0, y0 + side],
        ],
        holes: Vec::new(),
    }
}

fn fill_item(n: u128, polygon: Polygon) -> Item {
    Item {
        id: id(n),
        layer: Layer::Ink,
        z: 0,
        transform: Affine::IDENTITY,
        kind: ItemKind::Fill(FillItem {
            polygons: vec![polygon],
            paint: Paint {
                color: Color::INK,
                opacity: 1.0,
            },
        }),
    }
}

#[test]
fn fill_cut_leaves_a_hole() {
    let cut = cut_polygons(&[square(0.0, 0.0, 100.0)], &[[50.0, 50.0]], 10.0).unwrap();
    assert_eq!(cut.len(), 1);
    assert_eq!(cut[0].holes.len(), 1);
    let area = cut[0].area();
    assert!((area - (10_000.0 - 314.16)).abs() < 8.0, "area {area}");
    assert!(cut_polygons(&[square(0.0, 0.0, 100.0)], &[[300.0, 50.0]], 10.0).is_none());
}

#[test]
fn fill_cut_splits_a_fill_in_two() {
    let fill = fill_item(1, square(0.0, 0.0, 100.0));
    let scene = scene_with(std::slice::from_ref(&fill));
    let path = [[-10.0, 50.0], [50.0, 50.0], [110.0, 50.0]];
    let edits = erase_precise(&scene, &path, 5.0, LayerSet::INK, &mut Counter(100));
    assert_eq!(edits.len(), 1);
    let Edit::Update { id: target, patch } = &edits[0] else {
        panic!("update expected")
    };
    assert_eq!(*target, fill.id);
    assert!(patch.transform.unwrap().bit_eq(&Affine::IDENTITY));
    let polygons = patch.polygons.as_ref().unwrap();
    assert_eq!(polygons.len(), 2);
    let area: f32 = polygons.iter().map(Polygon::area).sum();
    assert!((area - 9_000.0).abs() < 1.0, "area {area}");
    // Sorted by the minimum vertex: the top half first.
    assert!(polygons[0].outer[0][1] < polygons[1].outer[0][1]);
}

#[test]
fn stroke_eraser_deletes_only_whole_touched_items() {
    let touched = stroke_item(1, tapered_ballpoint(), slow_line(600), Affine::IDENTITY);
    let far = stroke_item(
        2,
        Preset::Pencil.params(),
        slow_line(600),
        Affine::translate(0.0, 300.0),
    );
    // A fill whose ring is far from the eraser: scribbling over its interior must not delete it.
    let fill = fill_item(3, square(0.0, 0.0, 1_000.0));
    let scene = scene_with(&[touched.clone(), far, fill]);
    let edits = erase_strokes(&scene, &vertical(130.0), 3.0, LayerSet::INK);
    assert_eq!(edits, vec![Edit::Delete(touched.id)]);
    assert!(erase_strokes(&scene, &[], 3.0, LayerSet::INK).is_empty());
    assert!(erase_strokes(&scene, &vertical(130.0), 0.0, LayerSet::INK).is_empty());
}

#[test]
fn undo_of_an_erase_gesture_restores_the_stroke() {
    let item = stroke_item(1, tapered_ballpoint(), slow_line(1000), Affine::IDENTITY);
    let mut scene = scene_with(std::slice::from_ref(&item));
    let before = scene.geometry(item.id).cloned();
    let mut undo = UndoStack::new(16);
    let edits = erase_precise(
        &scene,
        &vertical(150.0),
        3.0,
        LayerSet::INK,
        &mut Counter(100),
    );
    undo.apply_local(&mut scene, &edits).unwrap();
    assert!(scene.get(item.id).is_none());
    assert_eq!(scene.family(item.id).len(), 2);

    let back = undo.undo(&scene);
    scene.apply_all(&back).unwrap();
    assert_eq!(scene.family(item.id), vec![item.id]);
    assert_eq!(
        scene.visible_ranges(item.id),
        vec![stroke_of(&item).own_range()]
    );
    assert_eq!(scene.geometry(item.id).cloned(), before);
}
