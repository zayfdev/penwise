#![allow(
    clippy::float_cmp,
    reason = "hull points are copied, not recomputed: bit-exact equality is the property under test"
)]

use super::*;
use crate::outline::nib::nib_corners;

fn area(pts: &[[f32; 2]]) -> f32 {
    let mut a = 0.0;
    for (i, p) in pts.iter().enumerate() {
        let q = pts[(i + 1) % pts.len()];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a * 0.5
}

fn hull_of(a: &NibQuad, b: &NibQuad) -> Vec<[f32; 2]> {
    let mut pts = [[0.0_f32; 2]; 8];
    pts[..4].copy_from_slice(a);
    pts[4..].copy_from_slice(b);
    let mut hull = [[0.0_f32; 2]; 16];
    let m = convex_hull(&mut pts, &mut hull);
    hull[..m].to_vec()
}

#[test]
fn hull_of_two_translated_squares_is_a_positive_hexagon() {
    let a = nib_corners([0.0, 0.0], 2.0, 2.0, 0.0);
    let b = nib_corners([3.0, 3.0], 2.0, 2.0, 0.0);
    let h = hull_of(&a, &b);
    assert_eq!(h.len(), 6, "{h:?}");
    // (−1,−1) (1,−1) (4,2) (4,4) (2,4) (−1,1): shoelace area 16.
    assert!((area(&h) - 16.0).abs() < 1e-4, "area {}", area(&h));
}

#[test]
fn hull_drops_duplicates_and_degenerates_to_nothing() {
    let a = nib_corners([5.0, 5.0], 0.0, 0.0, 0.3);
    let h = hull_of(&a, &a);
    assert!(h.len() < 3, "{h:?}");
    let mut left = Vec::new();
    let mut right = Vec::new();
    push_segment(
        &a,
        &a,
        &mut Sides {
            left: &mut left,
            right: &mut right,
        },
    );
    assert!(left.is_empty() && right.is_empty());
}

#[test]
fn segments_are_closed_loops_bridged_through_their_anchor() {
    let a = nib_corners([0.0, 0.0], 10.0, 1.0, 0.7);
    let b = nib_corners([4.0, 1.0], 10.0, 1.0, 0.7);
    let c = nib_corners([8.0, -2.0], 10.0, 1.0, 0.7);
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut sides = Sides {
        left: &mut left,
        right: &mut right,
    };
    push_segment(&a, &b, &mut sides);
    push_segment(&b, &c, &mut sides);
    assert_eq!(right.len(), 2);
    let first = hull_of(&a, &b);
    let second = hull_of(&b, &c);
    assert_eq!(left.len(), first.len() + second.len() + 2);
    assert_eq!(right[0], first[0]);
    assert_eq!(left[first.len()], first[0]);
    // The single contour's signed area is the sum of the hulls' (the bridges cancel).
    let mut polygon = left.clone();
    polygon.extend(right.iter().rev());
    let expected = area(&first) + area(&second);
    assert!(area(&first) > 0.0 && area(&second) > 0.0);
    assert!((area(&polygon) - expected).abs() < 1e-3);
}
