//! Owner: slice c. Fill bucket on real strokes: closed rectangles fill their interior, rings leave a hole,
//! gap closing vs `OpenRegion`, taps on ink, fixed page edges as walls, recolor, and determinism (same bytes
//! twice, for any tap inside the region, any window cap and any scene insertion order).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "test harness: failures are reported by panicking; small test geometry"
)]

use penwise_core::edit::{Edit, ItemPatch};
use penwise_core::fill::{FillError, FillRequest, FillScratch, fill_at, flood_fill};
use penwise_core::item::{
    Affine, FillItem, Item, ItemId, ItemKind, Layer, PageKind, Paint, Polygon, Scene, StrokeItem,
    Z_STEP,
};
use penwise_core::{Color, Preset, Sample};

const RED: Paint = Paint {
    color: Color::rgb(200, 40, 40),
    opacity: 1.0,
};
const BLUE: Paint = Paint {
    color: Color::rgb(40, 40, 200),
    opacity: 0.8,
};

/// Fineliner stroke from `a` to `b`, one quantized sample every ~2 px.
fn stroke(id: u8, a: [f32; 2], b: [f32; 2]) -> Item {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = (dx * dx + dy * dy).sqrt();
    let n = ((len / 2.0).ceil() as u32).max(1);
    let samples = (0..=n)
        .map(|k| {
            let t = k as f32 / n as f32;
            Sample {
                x: a[0] + (b[0] - a[0]) * t,
                y: a[1] + (b[1] - a[1]) * t,
                pressure: 0.5,
                tilt: 0.0,
                azimuth: 0.0,
                t_us: k * 2000,
            }
            .quantize()
        })
        .collect();
    Item {
        id: ItemId([id; 16]),
        layer: Layer::Ink,
        z: i64::from(id) * Z_STEP,
        transform: Affine::IDENTITY,
        kind: ItemKind::Stroke(StrokeItem {
            params: Preset::Fineliner.params(),
            seed: u64::from(id),
            samples,
            trim: None,
        }),
    }
}

/// A square drawn as four strokes overshooting each corner by 10 px (how people draw boxes).
fn square(first_id: u8, lo: f32, hi: f32) -> Vec<Item> {
    let o = 10.0;
    vec![
        stroke(first_id, [lo - o, lo], [hi + o, lo]),
        stroke(first_id + 1, [hi, lo - o], [hi, hi + o]),
        stroke(first_id + 2, [hi + o, hi], [lo - o, hi]),
        stroke(first_id + 3, [lo, hi + o], [lo, lo - o]),
    ]
}

fn scene_of(page: PageKind, items: Vec<Item>) -> Scene {
    let mut scene = Scene::new(page);
    for item in items {
        scene.apply(&Edit::Add(item)).unwrap();
    }
    scene
}

fn request(tap: [f32; 2], gap: f32) -> FillRequest {
    FillRequest {
        tap,
        gap,
        max_window: 512,
        paint: RED,
    }
}

fn fill(scene: &Scene, req: &FillRequest) -> Result<FillItem, FillError> {
    flood_fill(scene, req, &mut FillScratch::default())
}

/// Canonical bytes of the polygons (bit patterns of every coordinate, ring by ring).
fn bytes(fill: &FillItem) -> Vec<u8> {
    let mut out = Vec::new();
    let mut ring = |r: &[[f32; 2]]| {
        out.extend_from_slice(&(r.len() as u32).to_le_bytes());
        for p in r {
            out.extend_from_slice(&p[0].to_bits().to_le_bytes());
            out.extend_from_slice(&p[1].to_bits().to_le_bytes());
        }
    };
    for p in &fill.polygons {
        ring(&p.outer);
        for h in &p.holes {
            ring(h);
        }
    }
    out
}

fn area(fill: &FillItem) -> f32 {
    fill.polygons.iter().map(Polygon::area).sum()
}

#[test]
fn closed_square_fills_its_interior() {
    let scene = scene_of(PageKind::Infinite, square(1, 100.0, 300.0));
    let fill = fill(&scene, &request([200.0, 200.0], 0.0)).unwrap();
    assert_eq!(fill.paint, RED);
    assert_eq!(fill.polygons.len(), 1);
    assert!(fill.polygons[0].holes.is_empty());
    // Centreline square 200², ink ≈ 4 px wide, fill tucked one cell under the ink.
    let a = area(&fill);
    assert!((190.0 * 190.0..=202.0 * 202.0).contains(&a), "area {a}");
    let b = fill.polygons[0].bounds();
    assert!(b.min_x > 96.0 && b.min_x < 104.0, "{b:?}");
    assert!(b.max_x > 296.0 && b.max_x < 304.0, "{b:?}");
    assert!(b.min_y > 96.0 && b.max_y < 304.0, "{b:?}");
    // Output is on the 1/16 px grid.
    for p in &fill.polygons[0].outer {
        assert_eq!((p[0] * 16.0).fract(), 0.0);
        assert_eq!((p[1] * 16.0).fract(), 0.0);
    }
}

#[test]
fn ring_between_two_squares_has_one_hole() {
    let mut items = square(1, 100.0, 300.0);
    items.extend(square(11, 170.0, 230.0));
    let scene = scene_of(PageKind::Infinite, items);
    let fill = fill(&scene, &request([130.0, 130.0], 0.0)).unwrap();
    assert_eq!(fill.polygons.len(), 1);
    let polygon = &fill.polygons[0];
    assert_eq!(polygon.holes.len(), 1);
    let hole = Polygon {
        outer: polygon.holes[0].clone(),
        holes: Vec::new(),
    };
    let hole_area = hole.area();
    assert!(
        (56.0 * 56.0..=66.0 * 66.0).contains(&hole_area),
        "hole {hole_area}"
    );
    let a = polygon.area();
    let expected = 198.0 * 198.0 - 62.0 * 62.0;
    assert!(
        (a - expected).abs() < 0.04 * expected,
        "area {a} vs {expected}"
    );

    // The inner square fills on its own (taps inside the hole do not see the ring).
    let inner = flood_fill(
        &scene,
        &request([200.0, 200.0], 0.0),
        &mut FillScratch::default(),
    )
    .unwrap();
    assert!(area(&inner) < 66.0 * 66.0);
}

/// The square with a gap of `gap_px` (centreline) in the middle of its top side.
fn square_with_gap(gap_px: f32) -> Scene {
    let mut items = square(1, 100.0, 300.0);
    items[0] = stroke(1, [90.0, 100.0], [200.0 - gap_px / 2.0, 100.0]);
    items.push(stroke(9, [200.0 + gap_px / 2.0, 100.0], [310.0, 100.0]));
    scene_of(PageKind::Infinite, items)
}

#[test]
fn small_gap_closes_within_tolerance() {
    let scene = square_with_gap(8.0);
    assert_eq!(
        fill(&scene, &request([200.0, 200.0], 0.0)),
        Err(FillError::OpenRegion)
    );
    let closed = fill(&scene, &request([200.0, 200.0], 6.0)).unwrap();
    let a = area(&closed);
    assert!((185.0 * 185.0..=203.0 * 203.0).contains(&a), "area {a}");
    // Only a small bulge passes the gap.
    let b = closed.polygons[0].bounds();
    assert!(b.min_y > 88.0, "{b:?}");
}

#[test]
fn gap_wider_than_tolerance_is_open() {
    let scene = square_with_gap(60.0);
    assert_eq!(
        fill(&scene, &request([200.0, 200.0], 6.0)),
        Err(FillError::OpenRegion)
    );
    // An empty infinite page is open at every window size.
    let empty = Scene::new(PageKind::Infinite);
    let req = FillRequest {
        max_window: 1024,
        ..request([10.0, 10.0], 0.0)
    };
    assert_eq!(fill(&empty, &req), Err(FillError::OpenRegion));
}

#[test]
fn tap_on_ink_is_rejected() {
    let scene = scene_of(PageKind::Infinite, square(1, 100.0, 300.0));
    assert_eq!(
        fill(&scene, &request([200.0, 100.0], 6.0)),
        Err(FillError::TapOnInk)
    );
    assert_eq!(
        fill(&scene, &request([f32::NAN, 100.0], 6.0)),
        Err(FillError::OutsidePage)
    );
}

#[test]
fn fixed_page_edge_is_a_wall() {
    let page = PageKind::Fixed {
        width: 400.0,
        height: 300.0,
    };
    let scene = scene_of(page, vec![stroke(1, [200.0, -10.0], [200.0, 310.0])]);
    let filled = fill(&scene, &request([100.0, 150.0], 0.0)).unwrap();
    let b = filled.polygons[0].bounds();
    assert_eq!((b.min_x, b.min_y, b.max_y), (0.0, 0.0, 300.0));
    assert!(b.max_x > 196.0 && b.max_x < 201.0, "{b:?}");
    let a = area(&filled);
    assert!((196.0 * 300.0..=201.0 * 300.0).contains(&a), "area {a}");
    // Same stroke on an infinite page: open.
    let open = scene_of(
        PageKind::Infinite,
        vec![stroke(1, [200.0, -10.0], [200.0, 310.0])],
    );
    assert_eq!(
        fill(&open, &request([100.0, 150.0], 0.0)),
        Err(FillError::OpenRegion)
    );
    assert_eq!(
        fill(&scene, &request([500.0, 150.0], 0.0)),
        Err(FillError::OutsidePage)
    );
}

#[test]
fn result_is_deterministic_and_independent_of_tap_window_and_insertion_order() {
    let mut items = square(1, 100.0, 300.0);
    items.extend(square(11, 170.0, 230.0));
    let scene = scene_of(PageKind::Infinite, items.clone());
    let base = fill(&scene, &request([130.0, 130.0], 4.0)).unwrap();
    let mut scratch = FillScratch::default();
    let again = flood_fill(&scene, &request([130.0, 130.0], 4.0), &mut scratch).unwrap();
    // Reused scratch buffers give the same bytes too.
    let reused = flood_fill(&scene, &request([130.0, 130.0], 4.0), &mut scratch).unwrap();
    assert_eq!(bytes(&base), bytes(&again));
    assert_eq!(bytes(&base), bytes(&reused));
    // Any tap inside the same region (different window placement on the global grid).
    let other_tap = fill(&scene, &request([270.5, 251.25], 4.0)).unwrap();
    assert_eq!(bytes(&base), bytes(&other_tap));
    // Window cap does not matter once the region is closed.
    let big = FillRequest {
        max_window: 4096,
        ..request([130.0, 130.0], 4.0)
    };
    assert_eq!(bytes(&base), bytes(&fill(&scene, &big).unwrap()));
    // Shuffled insertion order.
    items.reverse();
    items.swap(1, 5);
    let shuffled = scene_of(PageKind::Infinite, items);
    let from_shuffled = fill(&shuffled, &request([130.0, 130.0], 4.0)).unwrap();
    assert_eq!(bytes(&base), bytes(&from_shuffled));
}

#[test]
fn fill_at_adds_then_recolors() {
    let mut scene = scene_of(PageKind::Infinite, square(1, 100.0, 300.0));
    let mut scratch = FillScratch::default();
    let id = ItemId([42; 16]);
    let edits = fill_at(&scene, &request([200.0, 200.0], 0.0), id, &mut scratch).unwrap();
    let [Edit::Add(item)] = edits.as_slice() else {
        panic!("expected one Add, got {edits:?}");
    };
    assert_eq!(item.id, id);
    assert_eq!(item.layer, Layer::Ink);
    assert_eq!(item.z, scene.next_z());
    assert!(item.transform.bit_eq(&Affine::IDENTITY));
    assert!(matches!(&item.kind, ItemKind::Fill(f) if f.paint == RED));
    scene.apply(&edits[0]).unwrap();

    // Same paint: nothing to do.
    let same = fill_at(
        &scene,
        &request([200.0, 200.0], 0.0),
        ItemId([43; 16]),
        &mut scratch,
    );
    assert_eq!(same, Ok(Vec::new()));
    // New paint: recolor the existing fill.
    let recolor = FillRequest {
        paint: BLUE,
        ..request([200.0, 200.0], 0.0)
    };
    let edits = fill_at(&scene, &recolor, ItemId([44; 16]), &mut scratch).unwrap();
    let expected = Edit::Update {
        id,
        patch: ItemPatch {
            paint: Some(BLUE),
            ..ItemPatch::default()
        },
    };
    assert_eq!(edits, vec![expected]);
}

#[test]
fn polygon_area_bounds_and_mapping() {
    let polygon = Polygon {
        outer: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        holes: vec![vec![[2.0, 2.0], [2.0, 4.0], [4.0, 4.0], [4.0, 2.0]]],
    };
    assert_eq!(polygon.area(), 96.0);
    let b = polygon.bounds();
    assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (0.0, 0.0, 10.0, 10.0));
    let mapped = polygon.mapped(&Affine::scale(2.0, 3.0));
    assert_eq!(mapped.area(), 96.0 * 6.0);
    assert_eq!(mapped.outer[2], [20.0, 30.0]);
    let mirrored = polygon.mapped(&Affine::scale(-1.0, 1.0));
    assert_eq!(mirrored.area(), 96.0);
    assert_eq!(mirrored.bounds().min_x, -10.0);
}
