use super::super::grid::{RIGHT, Window, get_bit, reset, set_bit, set_run};
use super::{FloodInput, FloodOutcome, close_and_flood};

const W: u32 = 40;
const H: u32 = 20;

/// A box of 1-cell walls (columns 2 and 39, rows 2 and 17) whose right wall is the window's last column and
/// has a 3-cell gap (rows 9..=11). With `g = 4` the dilated walls close the gap, so the step-4 region stays
/// inside (x ≤ 35), but the `√2·g` grow-back passes the gap and reaches column 39.
fn box_open_at_right_edge() -> Vec<u64> {
    let mut walls = Vec::new();
    reset(&mut walls, W, H);
    set_run(&mut walls, W, 2, 2, W);
    set_run(&mut walls, W, 17, 2, W);
    for y in 2..=17 {
        set_bit(&mut walls, W, 2, y);
        if !(9..=11).contains(&y) {
            set_bit(&mut walls, W, W - 1, y);
        }
    }
    walls
}

fn flood(page_edges: [bool; 4]) -> (FloodOutcome, Vec<u64>) {
    let window = Window {
        origin: [0, 0],
        width: W,
        height: H,
        page_edges,
    };
    let walls = box_open_at_right_edge();
    let (mut dist, mut stack, mut region) = (Vec::new(), Vec::new(), Vec::new());
    let input = FloodInput {
        seed: [20, 10],
        gap: 4,
    };
    let outcome = close_and_flood(&window, input, &walls, &mut dist, &mut stack, &mut region);
    (outcome, region)
}

#[test]
fn grow_back_reaching_an_open_window_edge_leaks() {
    // The closing step alone stays inside the box; only the grow-back touches the open right edge.
    assert_eq!(flood([false; 4]).0, FloodOutcome::Leaked);
    // Deterministic: the same input leaks again.
    assert_eq!(flood([false; 4]).0, FloodOutcome::Leaked);
}

#[test]
fn grow_back_reaching_a_page_edge_stays_closed() {
    let mut edges = [false; 4];
    edges[RIGHT] = true;
    let (outcome, region) = flood(edges);
    assert_eq!(outcome, FloodOutcome::Closed);
    // The grow-back passed the gap up to the page edge, and nothing escaped the box elsewhere.
    assert!(get_bit(&region, W, W - 1, 10));
    assert!(!get_bit(&region, W, 0, 0));
    assert!(!get_bit(&region, W, 1, 10));
}
