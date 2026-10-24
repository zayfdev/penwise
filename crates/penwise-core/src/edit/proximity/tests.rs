#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::*;

const SQUARE: [[f32; 2]; 4] = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];

#[test]
fn point_segment_projection_and_clamping() {
    assert_eq!(
        point_segment_dist2([5.0, 3.0], [0.0, 0.0], [10.0, 0.0]),
        (9.0, 0.5)
    );
    assert_eq!(
        point_segment_dist2([-3.0, 4.0], [0.0, 0.0], [10.0, 0.0]),
        (25.0, 0.0)
    );
    assert_eq!(
        point_segment_dist2([13.0, 4.0], [0.0, 0.0], [10.0, 0.0]),
        (25.0, 1.0)
    );
    assert_eq!(
        point_segment_dist2([3.0, 4.0], [0.0, 0.0], [0.0, 0.0]),
        (25.0, 0.0)
    );
}

#[test]
fn segment_distances() {
    assert_eq!(
        segment_segment_dist([0.0, 0.0], [10.0, 10.0], [0.0, 10.0], [10.0, 0.0]),
        0.0
    );
    // Touching at an endpoint and collinear overlap count as intersecting.
    assert_eq!(
        segment_segment_dist([0.0, 0.0], [5.0, 0.0], [5.0, 0.0], [5.0, 5.0]),
        0.0
    );
    assert_eq!(
        segment_segment_dist([0.0, 0.0], [5.0, 0.0], [3.0, 0.0], [9.0, 0.0]),
        0.0
    );
    assert_eq!(
        segment_segment_dist([0.0, 0.0], [10.0, 0.0], [0.0, 3.0], [10.0, 3.0]),
        3.0
    );
    assert_eq!(
        segment_segment_dist([0.0, 0.0], [1.0, 0.0], [4.0, 4.0], [4.0, 9.0]),
        5.0
    );
}

#[test]
fn polyline_distance() {
    assert_eq!(point_polyline_dist([0.0, 0.0], &[]), f32::INFINITY);
    assert_eq!(point_polyline_dist([3.0, 4.0], &[[0.0, 0.0]]), 5.0);
    assert_eq!(
        point_polyline_dist([12.0, 5.0], &[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]),
        2.0
    );
}

#[test]
fn winding_numbers_and_orientation() {
    assert_eq!(winding_number([5.0, 5.0], &SQUARE).abs(), 1);
    assert_eq!(winding_number([15.0, 5.0], &SQUARE), 0);
    let mut reversed = SQUARE;
    reversed.reverse();
    assert_eq!(
        winding_number([5.0, 5.0], &reversed),
        -winding_number([5.0, 5.0], &SQUARE)
    );
    assert!(ring_contains(&SQUARE, [0.5, 9.5]));
    assert!(!ring_contains(&SQUARE[..2], [5.0, 0.0]));
}

#[test]
fn shared_edge_points_counted_once() {
    // Two squares sharing the edge x = 10; points on it belong to exactly one of them.
    let right = [[10.0, 0.0], [20.0, 0.0], [20.0, 10.0], [10.0, 10.0]];
    for y in [0.5, 3.0, 7.25, 9.5] {
        let p = [10.0, y];
        let hits = i32::from(ring_contains(&SQUARE, p)) + i32::from(ring_contains(&right, p));
        assert_eq!(hits, 1, "y = {y}");
    }
}

#[test]
fn polygons_with_holes_use_nonzero() {
    let poly = Polygon {
        outer: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        holes: vec![vec![[3.0, 3.0], [3.0, 7.0], [7.0, 7.0], [7.0, 3.0]]],
    };
    assert!(signed_area(&poly.outer) > 0.0);
    assert!(signed_area(&poly.holes[0]) < 0.0);
    assert!(polygons_contain(std::slice::from_ref(&poly), [1.0, 1.0]));
    assert!(!polygons_contain(std::slice::from_ref(&poly), [5.0, 5.0]));
    assert!(!polygons_contain(std::slice::from_ref(&poly), [11.0, 5.0]));
    assert!(!polygons_contain(&[], [1.0, 1.0]));
}

#[test]
fn signed_area_and_ring_distance() {
    assert_eq!(signed_area(&SQUARE), 100.0);
    assert_eq!(signed_area(&[[0.0, 0.0], [1.0, 1.0]]), 0.0);
    assert_eq!(point_ring_dist([5.0, 4.0], &SQUARE), 4.0);
    assert_eq!(point_ring_dist([-2.0, 5.0], &SQUARE), 2.0);
    // The closing edge (0,10) → (0,0) counts.
    assert_eq!(point_ring_dist([1.0, 5.0], &SQUARE), 1.0);
}

#[test]
fn arc_length_open_and_closed() {
    assert_eq!(arc_length(&SQUARE, false), 30.0);
    assert_eq!(arc_length(&SQUARE, true), 40.0);
    assert_eq!(arc_length(&[[1.0, 1.0]], true), 0.0);
    assert_eq!(arc_length(&[], false), 0.0);
}

#[test]
fn resample_open_line_is_uniform() {
    let mut out = Vec::new();
    resample(
        &[[0.0, 0.0], [10.0, 0.0], [10.0, 5.0]],
        false,
        4.0,
        &mut out,
    );
    assert_eq!(
        out,
        vec![[0.0, 0.0], [4.0, 0.0], [8.0, 0.0], [10.0, 2.0], [10.0, 5.0]]
    );
}

#[test]
fn resample_closed_ring_does_not_repeat_start() {
    let mut out = Vec::new();
    resample(&SQUARE, true, 5.0, &mut out);
    assert_eq!(out.len(), 8);
    assert_eq!(out[0], [0.0, 0.0]);
    assert_eq!(out[7], [0.0, 5.0]);
}

#[test]
fn resample_invalid_step_copies_vertices_and_appends() {
    let mut out = vec![[9.0, 9.0]];
    resample(&SQUARE, false, 0.0, &mut out);
    assert_eq!(out.len(), 5);
    resample(&SQUARE, false, f32::NAN, &mut out);
    assert_eq!(out.len(), 9);
    resample(&[], false, 1.0, &mut out);
    assert_eq!(out.len(), 9);
}

#[test]
fn resample_caps_huge_paths() {
    // Uncapped this would emit ~1e30 points (hang); the step grows to arc length / MAX_RESAMPLE_POINTS.
    let line = [[0.0, 0.0], [1.0e30, 0.0], [1.0e30, 1.0e30]];
    let cap = MAX_RESAMPLE_POINTS as usize + 2;
    for closed in [false, true] {
        let mut out = Vec::new();
        resample(&line, closed, 2.0, &mut out);
        assert!(out.len() <= cap, "{} points > cap {cap}", out.len());
        assert!(out.len() > 3, "still resampled");
        assert_eq!(out[0], [0.0, 0.0]);
        assert!(out.iter().flatten().all(|v| v.is_finite()));
    }
}
