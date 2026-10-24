#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    reason = "test grids are small; exact outputs are the contract under test"
)]

use super::clean::normalize;
use super::contour::trace_rings;
use super::distance::squared_edt;
use super::flood::{FloodInput, FloodOutcome, close_and_flood};
use super::grid::{Window, get_bit, reset, set_bit, set_run};
use super::scan::{clear_capsule, fill_capsule, fill_contours};
use super::simplify::simplify_ring;
use crate::item::{PageKind, Polygon};

fn window(width: u32, height: u32, page_edges: [bool; 4]) -> Window {
    Window {
        origin: [0, 0],
        width,
        height,
        page_edges,
    }
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

fn count(bits: &[u64], w: u32, h: u32) -> usize {
    (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| get_bit(bits, w, x, y))
        .count()
}

/// Squared distance to the nearest wall or virtual page-edge wall, by exhaustive search.
fn brute_edt(walls: &[u64], w: u32, h: u32, edges: [bool; 4], x: u32, y: u32) -> u16 {
    let (x, y) = (i64::from(x), i64::from(y));
    let mut best = i64::MAX;
    let mut consider = |wx: i64, wy: i64| best = best.min((wx - x).pow(2) + (wy - y).pow(2));
    for wy in 0..h {
        for wx in 0..w {
            if get_bit(walls, w, wx, wy) {
                consider(i64::from(wx), i64::from(wy));
            }
        }
    }
    if edges[0] {
        consider(-1, y);
    }
    if edges[1] {
        consider(x, -1);
    }
    if edges[2] {
        consider(i64::from(w), y);
    }
    if edges[3] {
        consider(x, i64::from(h));
    }
    u16::try_from(best).unwrap_or(u16::MAX)
}

#[test]
fn edt_matches_brute_force_on_random_grids() {
    let mut state = 7;
    for case in 0..40u32 {
        let w = 1 + (lcg(&mut state) % 37) as u32;
        let h = 1 + (lcg(&mut state) % 29) as u32;
        let density = lcg(&mut state) % 25;
        let mut walls = Vec::new();
        reset(&mut walls, w, h);
        for y in 0..h {
            for x in 0..w {
                if lcg(&mut state) % 100 < density {
                    set_bit(&mut walls, w, x, y);
                }
            }
        }
        let edges = [case % 2 == 0, case % 3 == 0, case % 5 == 0, case % 7 == 0];
        let mut dist = Vec::new();
        squared_edt(&walls, w, h, edges, &mut dist);
        for y in 0..h {
            for x in 0..w {
                let got = dist[(y * w + x) as usize];
                let want = brute_edt(&walls, w, h, edges, x, y);
                assert_eq!(got, want, "case {case} ({w}×{h}) cell ({x}, {y})");
            }
        }
    }
}

#[test]
fn edt_without_walls_is_saturated() {
    let mut walls = Vec::new();
    reset(&mut walls, 9, 5);
    let mut dist = Vec::new();
    squared_edt(&walls, 9, 5, [false; 4], &mut dist);
    assert!(dist.iter().all(|&d| d == u16::MAX));
}

#[test]
fn contours_fill_cells_whose_centres_are_inside_nonzero() {
    let win = window(16, 16, [false; 4]);
    let mut bits = Vec::new();
    reset(&mut bits, 16, 16);
    let a: &[[f32; 2]] = &[[2.0, 2.0], [6.0, 2.0], [6.0, 6.0], [2.0, 6.0]];
    // Same orientation, overlapping: nonzero union, not even-odd.
    let b: &[[f32; 2]] = &[[4.0, 4.0], [8.0, 4.0], [8.0, 8.0], [4.0, 8.0]];
    fill_contours(&win, &[a, b], &mut bits);
    assert_eq!(count(&bits, 16, 16), 16 + 16 - 4);
    assert!(get_bit(&bits, 16, 5, 5));
    assert!(!get_bit(&bits, 16, 6, 3));
}

#[test]
fn thin_diagonal_capsule_blocks_a_four_connected_flood() {
    let (w, h) = (64, 64);
    let win = window(w, h, [true; 4]);
    let mut walls = Vec::new();
    reset(&mut walls, w, h);
    fill_capsule(&win, [-3.0, 70.3], [70.0, -2.7], 0.0, &mut walls);
    let (mut dist, mut stack, mut region) = (Vec::new(), Vec::new(), Vec::new());
    let input = FloodInput {
        seed: [5, 5],
        gap: 0,
    };
    let outcome = close_and_flood(&win, input, &walls, &mut dist, &mut stack, &mut region);
    assert_eq!(outcome, FloodOutcome::Closed);
    assert!(get_bit(&region, w, 10, 10));
    assert!(!get_bit(&region, w, 50, 50));
}

#[test]
fn mask_capsule_clears_walls() {
    let win = window(32, 32, [false; 4]);
    let mut bits = Vec::new();
    reset(&mut bits, 32, 32);
    fill_capsule(&win, [4.0, 16.0], [28.0, 16.0], 2.0, &mut bits);
    assert!(get_bit(&bits, 32, 16, 16));
    clear_capsule(&win, [16.0, 10.0], [16.0, 22.0], 3.0, &mut bits);
    assert!(!get_bit(&bits, 32, 16, 16));
    assert!(get_bit(&bits, 32, 6, 16));
}

/// A 40-cell box of 2-cell-wide walls with a 4-cell gap in its top side, in a 64² open window.
fn box_with_gap() -> (Window, Vec<u64>) {
    let win = window(64, 64, [false; 4]);
    let mut walls = Vec::new();
    reset(&mut walls, 64, 64);
    for (a, b) in [
        ([10.0, 10.0], [28.0, 10.0]),
        ([34.0, 10.0], [50.0, 10.0]),
        ([50.0, 10.0], [50.0, 50.0]),
        ([50.0, 50.0], [10.0, 50.0]),
        ([10.0, 50.0], [10.0, 10.0]),
    ] {
        fill_capsule(&win, a, b, 1.0, &mut walls);
    }
    (win, walls)
}

fn flood_box(gap: u32) -> (FloodOutcome, Vec<u64>) {
    let (win, walls) = box_with_gap();
    let (mut dist, mut stack, mut region) = (Vec::new(), Vec::new(), Vec::new());
    let input = FloodInput {
        seed: [30, 30],
        gap,
    };
    let outcome = close_and_flood(&win, input, &walls, &mut dist, &mut stack, &mut region);
    (outcome, region)
}

#[test]
fn gap_wider_than_tolerance_leaks() {
    assert_eq!(flood_box(0).0, FloodOutcome::Leaked);
    assert_eq!(flood_box(1).0, FloodOutcome::Leaked);
}

#[test]
fn gap_within_tolerance_closes_and_grows_back_to_the_walls() {
    let (outcome, region) = flood_box(3);
    assert_eq!(outcome, FloodOutcome::Closed);
    // Interior reaches the walls (cells next to the wall cells, plus the tucked wall cells).
    assert!(get_bit(&region, 64, 11, 30));
    assert!(get_bit(&region, 64, 48, 48));
    assert!(get_bit(&region, 64, 10, 30), "tucked under the left wall");
    // Only a small bulge passes the closed gap.
    for y in 0..7 {
        for x in 0..64 {
            assert!(!get_bit(&region, 64, x, y), "leak at ({x}, {y})");
        }
    }
    assert!(!get_bit(&region, 64, 5, 30));
}

#[test]
fn tap_on_a_wall_is_rejected() {
    let (win, walls) = box_with_gap();
    let (mut dist, mut stack, mut region) = (Vec::new(), Vec::new(), Vec::new());
    let input = FloodInput {
        seed: [10, 30],
        gap: 3,
    };
    let outcome = close_and_flood(&win, input, &walls, &mut dist, &mut stack, &mut region);
    assert_eq!(outcome, FloodOutcome::SeedOnWall);
}

#[test]
fn seed_near_a_wall_moves_into_the_free_area() {
    let (win, walls) = box_with_gap();
    let (mut dist, mut stack, mut region) = (Vec::new(), Vec::new(), Vec::new());
    let input = FloodInput {
        seed: [12, 30],
        gap: 3,
    };
    let outcome = close_and_flood(&win, input, &walls, &mut dist, &mut stack, &mut region);
    assert_eq!(outcome, FloodOutcome::Closed);
    assert!(get_bit(&region, 64, 30, 30));
}

#[test]
fn page_edges_are_walls_and_closing_recovers_the_border() {
    let win = window(40, 30, [true; 4]);
    let mut walls = Vec::new();
    reset(&mut walls, 40, 30);
    let (mut dist, mut stack, mut region) = (Vec::new(), Vec::new(), Vec::new());
    let input = FloodInput {
        seed: [20, 15],
        gap: 2,
    };
    let outcome = close_and_flood(&win, input, &walls, &mut dist, &mut stack, &mut region);
    assert_eq!(outcome, FloodOutcome::Closed);
    assert_eq!(count(&region, 40, 30), 40 * 30);
}

#[test]
fn fixed_page_window_is_clipped_and_marks_edges() {
    let page = PageKind::Fixed {
        width: 300.0,
        height: 200.0,
    };
    let win = Window::around([100.0, 50.0], 512, page).unwrap();
    assert_eq!(win.origin, [0, 0]);
    assert_eq!((win.width, win.height), (300, 200));
    assert_eq!(win.page_edges, [true; 4]);
    assert!(Window::around([301.0, 50.0], 512, page).is_none());
    let open = Window::around([100.5, -3.2], 512, PageKind::Infinite).unwrap();
    assert_eq!(open.origin, [100 - 256, -4 - 256]);
    assert_eq!(open.cell_of([100.5, -3.2]), Some([256, 256]));
}

fn region_of(cells: &[(u32, u32, u32, u32)], w: u32, h: u32) -> Vec<u64> {
    let mut bits = Vec::new();
    reset(&mut bits, w, h);
    for &(x0, x1, y0, y1) in cells {
        for y in y0..y1 {
            set_run(&mut bits, w, y, x0, x1);
        }
    }
    bits
}

#[test]
fn donut_traces_to_one_polygon_with_one_hole() {
    let win = window(24, 24, [false; 4]);
    let mut region = region_of(&[(4, 20, 4, 20)], 24, 24);
    // Punch the hole: rebuild without cells 9..14 × 9..14.
    for y in 9..14 {
        super::grid::clear_run(&mut region, 24, y, 9, 14);
    }
    let rings = trace_rings(&win, &region);
    assert_eq!(rings.len(), 2);
    assert_eq!(
        rings[0],
        vec![[4.0, 4.0], [20.0, 4.0], [20.0, 20.0], [4.0, 20.0]]
    );
    assert!(crate::edit::proximity::signed_area(&rings[0]) > 0.0);
    assert!(crate::edit::proximity::signed_area(&rings[1]) < 0.0);
    let polygons = normalize(&rings);
    assert_eq!(polygons.len(), 1);
    assert_eq!(polygons[0].holes.len(), 1);
    assert_eq!(polygons[0].holes[0][0], [9.0, 9.0]);
    assert_eq!(polygons[0].area(), 16.0 * 16.0 - 25.0);
}

#[test]
fn saddle_cells_trace_as_separate_rings() {
    let win = window(8, 8, [false; 4]);
    let region = region_of(&[(2, 3, 2, 3), (3, 4, 3, 4)], 8, 8);
    let rings = trace_rings(&win, &region);
    assert_eq!(rings.len(), 2);
    assert!(rings.iter().all(|r| r.len() == 4));
}

#[test]
fn rdp_removes_the_staircase_and_keeps_tiny_rings() {
    let mut ring = vec![[0.0, 0.0], [10.0, 0.0]];
    for k in 0..10u8 {
        let k = f32::from(k);
        ring.push([10.0 - k, k + 1.0]);
        ring.push([9.0 - k, k + 1.0]);
    }
    let before = ring.len();
    simplify_ring(&mut ring, 0.75);
    assert!(
        ring.len() >= 3 && ring.len() <= 6,
        "{} of {before} kept",
        ring.len()
    );
    assert_eq!(ring[0], [0.0, 0.0]);

    let mut triangle = vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    simplify_ring(&mut triangle, 5.0);
    assert_eq!(triangle.len(), 3);
}

#[test]
fn normalize_resolves_crossings_and_orders_output() {
    // A bow tie (self-crossing: two lobes of opposite winding) and a separate square listed first but lying
    // lower on the page.
    let rings = vec![
        vec![[0.0, 20.0], [4.0, 20.0], [4.0, 24.0], [0.0, 24.0]],
        vec![[0.0, 0.0], [8.0, 8.0], [8.0, 0.0], [0.0, 8.0]],
    ];
    let polygons = normalize(&rings);
    assert!(polygons.len() >= 2);
    assert_eq!(polygons.last().map(|p| p.outer[0]), Some([0.0, 20.0]));
    let firsts: Vec<[f32; 2]> = polygons.iter().map(|p| p.outer[0]).collect();
    let mut sorted = firsts.clone();
    sorted.sort_by(|p, q| p[1].total_cmp(&q[1]).then(p[0].total_cmp(&q[0])));
    assert_eq!(firsts, sorted);
    for p in &polygons {
        assert!(crate::edit::proximity::signed_area(&p.outer) > 0.0);
    }
    let total: f32 = polygons.iter().map(Polygon::area).sum();
    assert_eq!(total, 16.0 + 16.0 + 16.0);
}
