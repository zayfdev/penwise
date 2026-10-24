//! Precise erase of fills: polygon difference, the fill stays vector (decided; editing-algorithms.md §2).
//!
//! Owner: slice b.
//!
//! Booleans use `i_overlay`'s **integer** API only (`i_overlay::core`, `OverlayRule::Difference`,
//! `FillRule::NonZero`) on coordinates snapped to the 1/16 px grid (`i32 = round(v · 16)`), so the result is
//! bit-identical everywhere. The eraser buffer is built here, not with `i_overlay`'s float stroke API (which
//! uses std trig): each eraser point becomes a [`CAPSULE_SEGMENTS`]-gon from a constant unit-circle table and
//! each segment a quad joining the two discs' tangent points.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::{Overlay, ShapeType};
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::point::IntPoint;

use super::erase_stroke::eraser_bounds;
use super::{Edit, ItemPatch};
use crate::item::{Affine, Item, Polygon};
use crate::sample::POSITION_STEPS_PER_PX;

/// Vertices of the regular polygon approximating each eraser disc (circumscribed: vertex radius
/// `r / cos(π / N)`, so the polygon covers the disc).
pub const CAPSULE_SEGMENTS: u32 = 32;

/// Fills with less remaining area (page px²) are deleted instead of updated.
pub const MIN_FILL_AREA: f32 = 1.0;

/// `polygons − buffer(path, radius)`, all page px. Output polygons are canonical (`i_overlay` orientation,
/// see [`Polygon`]), sorted by the `(y, x)` of each outer ring's minimum vertex, holes sorted the same way,
/// rings rotated to start at their minimum `(y, x)` vertex. `None` when the buffer does not intersect the
/// polygons (fill untouched).
#[must_use]
pub fn cut_polygons(polygons: &[Polygon], path: &[[f32; 2]], radius: f32) -> Option<Vec<Polygon>> {
    eraser_bounds(path, radius)?;
    let mut subject = Vec::new();
    let mut subject_area2 = 0i128;
    for poly in polygons {
        let first = subject.len();
        subject.extend(
            std::iter::once(&poly.outer)
                .chain(&poly.holes)
                .map(|r| snap_ring(r)),
        );
        subject_area2 += subject[first..]
            .iter()
            .map(|r| shoelace2(r))
            .sum::<i128>()
            .abs();
    }
    let clip = buffer(path, radius);
    let edges = subject.iter().chain(&clip).map(Vec::len).sum();
    let mut overlay = Overlay::<i32>::new(edges);
    overlay.add_source(&subject, ShapeType::Subject);
    overlay.add_source(&clip, ShapeType::Clip);
    let shapes = overlay.overlay(OverlayRule::Difference, FillRule::NonZero);
    let mut out = Vec::new();
    let mut out_area2 = 0i128;
    for shape in shapes {
        let mut rings = shape.into_iter();
        let Some(outer) = rings.next().and_then(|r| canonical_ring(r, true)) else {
            continue;
        };
        let mut holes: Vec<Vec<IntPoint>> =
            rings.filter_map(|r| canonical_ring(r, false)).collect();
        holes.sort_by_key(|h| key(h));
        out_area2 += shoelace2(&outer) + holes.iter().map(|h| shoelace2(h)).sum::<i128>();
        out.push((outer, holes));
    }
    if out_area2 == subject_area2 {
        return None;
    }
    out.sort_by_key(|(outer, _)| key(outer));
    Some(
        out.iter()
            .map(|(outer, holes)| Polygon {
                outer: to_page(outer),
                holes: holes.iter().map(|h| to_page(h)).collect(),
            })
            .collect(),
    )
}

/// Edits for one live fill item `item` (with page polygons `page_polygons` from its cached geometry):
/// - untouched → empty;
/// - remaining area < [`MIN_FILL_AREA`] → `Delete(item.id)`;
/// - otherwise one `Update { polygons: Some(cut in page space), transform: Some(Affine::IDENTITY) }` (the cut
///   is done in page space so non-uniform transforms stay exact; resetting the transform keeps local = page).
///
/// Known limitation (LWW): two concurrent cuts of one fill keep only the later `Polygons` value.
#[must_use]
pub fn fill_cut_edits(
    item: &Item,
    page_polygons: &[Polygon],
    path: &[[f32; 2]],
    radius: f32,
) -> Vec<Edit> {
    let Some(cut) = cut_polygons(page_polygons, path, radius) else {
        return Vec::new();
    };
    let area: f32 = cut.iter().map(Polygon::area).sum();
    if area < MIN_FILL_AREA {
        return vec![Edit::Delete(item.id)];
    }
    vec![Edit::Update {
        id: item.id,
        patch: ItemPatch {
            polygons: Some(cut),
            transform: Some(Affine::IDENTITY),
            ..ItemPatch::default()
        },
    }]
}

/// `cos(m·π/16)` for `m = 0..=8`: one quadrant of the [`CAPSULE_SEGMENTS`]-gon unit circle.
const QUADRANT: [f32; 9] = [
    1.0,
    0.980_785_25,
    0.923_879_5,
    0.831_469_6,
    0.707_106_77,
    0.555_570_24,
    0.382_683_43,
    0.195_090_32,
    0.0,
];

/// `1 / cos(π / 32)`: vertex radius factor of the circumscribed disc polygon.
const CIRCUMSCRIBE: f32 = 1.004_838_6;

/// Grid coordinate limit kept well inside `i_overlay`'s `i32` range (`±2^30`).
const GRID_LIMIT: f32 = 536_870_912.0;

/// `(cos, sin)` of `2π·k/32`, from the quadrant table by symmetry (counter-clockwise in y-up maths).
fn unit(k: usize) -> [f32; 2] {
    let q = |m: usize| QUADRANT[m];
    match k % 32 {
        k @ 0..=8 => [q(k), q(8 - k)],
        k @ 9..=16 => [-q(16 - k), q(k - 8)],
        k @ 17..=24 => [-q(k - 16), -q(24 - k)],
        k => [q(32 - k), -q(k - 24)],
    }
}

/// Eraser buffer: one circumscribed disc polygon per point and one tangent quad per segment, all with
/// positive orientation so the nonzero clip is their union.
fn buffer(path: &[[f32; 2]], radius: f32) -> Vec<Vec<IntPoint>> {
    let vertex_r = radius * CIRCUMSCRIBE;
    let mut out = Vec::with_capacity(path.len() * 2);
    for &[x, y] in path {
        out.push(
            (0..32)
                .map(|k| {
                    let [c, s] = unit(k);
                    grid_point([x + c * vertex_r, y + s * vertex_r])
                })
                .collect(),
        );
    }
    for w in path.windows(2) {
        let (p, q) = (w[0], w[1]);
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len.is_nan() || len <= 0.0 {
            continue;
        }
        let (nx, ny) = (-dy / len * radius, dx / len * radius);
        let mut quad = vec![
            grid_point([p[0] + nx, p[1] + ny]),
            grid_point([q[0] + nx, q[1] + ny]),
            grid_point([q[0] - nx, q[1] - ny]),
            grid_point([p[0] - nx, p[1] - ny]),
        ];
        match shoelace2(&quad) {
            0 => continue,
            a if a < 0 => quad.reverse(),
            _ => {}
        }
        out.push(quad);
    }
    out
}

/// Page point → 1/16 px grid (`round(v · 16)`, clamped into the engine's range).
#[allow(
    clippy::cast_possible_truncation,
    reason = "rounded and clamped to ±2^29 before the cast"
)]
fn grid_point(p: [f32; 2]) -> IntPoint {
    let g = |v: f32| {
        let v = (v * POSITION_STEPS_PER_PX).round();
        let v = if v > GRID_LIMIT {
            GRID_LIMIT
        } else if v < -GRID_LIMIT {
            -GRID_LIMIT
        } else if v.is_nan() {
            0.0
        } else {
            v
        };
        v as i32
    };
    IntPoint::new(g(p[0]), g(p[1]))
}

fn snap_ring(ring: &[[f32; 2]]) -> Vec<IntPoint> {
    ring.iter().map(|&p| grid_point(p)).collect()
}

/// Twice the signed shoelace area of an implicitly closed ring (exact).
fn shoelace2(ring: &[IntPoint]) -> i128 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            i128::from(a.x) * i128::from(b.y) - i128::from(b.x) * i128::from(a.y)
        })
        .sum()
}

/// Ring with ≥ 3 points and non-zero area, oriented positive (`outer`) or negative (hole), rotated to start at
/// its minimum `(y, x)` vertex.
fn canonical_ring(mut ring: Vec<IntPoint>, outer: bool) -> Option<Vec<IntPoint>> {
    let area = shoelace2(&ring);
    if ring.len() < 3 || area == 0 {
        return None;
    }
    if (area > 0) != outer {
        ring.reverse();
    }
    let start = ring
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| (p.y, p.x))
        .map_or(0, |(i, _)| i);
    ring.rotate_left(start);
    Some(ring)
}

/// Sort key of a canonical ring: its first (minimum) vertex.
fn key(ring: &[IntPoint]) -> (i32, i32) {
    ring.first().map_or((i32::MAX, i32::MAX), |p| (p.y, p.x))
}

#[allow(
    clippy::cast_precision_loss,
    reason = "grid coordinates below 2^24 (1 M page px) convert exactly"
)]
fn to_page(ring: &[IntPoint]) -> Vec<[f32; 2]> {
    ring.iter()
        .map(|p| {
            [
                p.x as f32 / POSITION_STEPS_PER_PX,
                p.y as f32 / POSITION_STEPS_PER_PX,
            ]
        })
        .collect()
}
