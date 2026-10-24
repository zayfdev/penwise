//! Marching squares on the region mask (editing-algorithms.md §3.6).
//!
//! Owner: slice c (slice-internal).
//!
//! Boundary edges between set and unset cells are directed so the set cell lies on the side of the left
//! normal `(−dy, dx)` (cell coordinates, y down). A cell's top edge then runs right, its right edge down, its
//! bottom edge left and its left edge up, so outer rings have positive shoelace sums and holes negative.

use super::grid::{Window, get_bit, get_index, reset, set_bit};

/// Direction of a boundary edge in cell coordinates (y down).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Right,
    Down,
    Left,
    Up,
}

impl Dir {
    fn delta(self) -> [i64; 2] {
        match self {
            Self::Right => [1, 0],
            Self::Down => [0, 1],
            Self::Left => [-1, 0],
            Self::Up => [0, -1],
        }
    }

    /// The turn towards the set cell (`(dx, dy) → (−dy, dx)`): wraps tightly around the cell the incoming
    /// edge bounds, which separates diagonal neighbours at saddles.
    fn toward_interior(self) -> Self {
        match self {
            Self::Right => Self::Down,
            Self::Down => Self::Left,
            Self::Left => Self::Up,
            Self::Up => Self::Right,
        }
    }
}

/// The region mask with out-of-window cells read as unset.
#[derive(Debug, Clone, Copy)]
struct Cells<'a> {
    bits: &'a [u64],
    w: u32,
    h: u32,
}

impl Cells<'_> {
    fn get(&self, x: i64, y: i64) -> bool {
        match (u32::try_from(x), u32::try_from(y)) {
            (Ok(x), Ok(y)) if x < self.w && y < self.h => get_bit(self.bits, self.w, x, y),
            _ => false,
        }
    }

    /// Outgoing boundary edge at corner `(x, y)` after arriving along `incoming`. Around the corner,
    /// `a`/`b` are the cells above-left/above-right and `c`/`d` below-left/below-right.
    fn outgoing(&self, x: i64, y: i64, incoming: Dir) -> Option<Dir> {
        let a = self.get(x - 1, y - 1);
        let b = self.get(x, y - 1);
        let c = self.get(x - 1, y);
        let d = self.get(x, y);
        let open = |dir: Dir| match dir {
            Dir::Right => d && !b,
            Dir::Down => c && !d,
            Dir::Left => a && !c,
            Dir::Up => b && !a,
        };
        let turn = incoming.toward_interior();
        if open(turn) {
            return Some(turn);
        }
        [Dir::Right, Dir::Down, Dir::Left, Dir::Up]
            .into_iter()
            .find(|&dir| open(dir))
    }
}

/// Closed rings (page px, cell-corner vertices) bounding the set cells of `region`. Saddle cases always
/// separate diagonal cells (consistent with the 4-connected flood). Rings are emitted in scan order of their
/// first boundary edge, each starting at its minimum `(y, x)` vertex, collinear vertices removed. Outer rings
/// have positive shoelace sums, holes negative ([`crate::item::Polygon`] convention).
pub(crate) fn trace_rings(window: &Window, region: &[u64]) -> Vec<Vec<[f32; 2]>> {
    let cells = Cells {
        bits: region,
        w: window.width,
        h: window.height,
    };
    let total = window.width as usize * window.height as usize;
    // Visited top edges ("Right" edges), indexed by the cell below them.
    let mut visited = Vec::new();
    reset(&mut visited, window.width, window.height);
    let mut rings = Vec::new();
    for (wi, &word) in region.iter().enumerate() {
        let mut rest = word;
        while rest != 0 {
            let i = wi * 64 + rest.trailing_zeros() as usize;
            rest &= rest - 1;
            if i >= total {
                break;
            }
            let (x, y) = cell_xy(i, window.width);
            if !get_index(&visited, i) && !cells.get(x, y - 1) {
                rings.push(trace(window, &cells, &mut visited, [x, y]));
            }
        }
    }
    rings
}

#[allow(
    clippy::cast_possible_wrap,
    reason = "cell indices are below width · height ≤ 4096²"
)]
fn cell_xy(i: usize, width: u32) -> (i64, i64) {
    let w = width as usize;
    ((i % w) as i64, (i / w) as i64)
}

/// Follows one ring from the top edge of cell `start` (a corner with an unvisited "Right" edge).
fn trace(
    window: &Window,
    cells: &Cells<'_>,
    visited: &mut [u64],
    start: [i64; 2],
) -> Vec<[f32; 2]> {
    let mut ring = Vec::new();
    let [mut x, mut y] = start;
    let mut dir = Dir::Right;
    // Each boundary edge is walked once; the bound only guards against a malformed mask.
    let limit = 4 * (i64::from(cells.w) + 1) * (i64::from(cells.h) + 1);
    for _ in 0..limit {
        if dir == Dir::Right
            && let (Ok(cx), Ok(cy)) = (u32::try_from(x), u32::try_from(y))
        {
            set_bit(visited, cells.w, cx, cy);
        }
        let [dx, dy] = dir.delta();
        x += dx;
        y += dy;
        let Some(next) = cells.outgoing(x, y, dir) else {
            break;
        };
        if next != dir {
            ring.push(window.corner(x, y));
        }
        if [x, y] == start && next == Dir::Right {
            break;
        }
        dir = next;
    }
    rotate_to_min(&mut ring);
    ring
}

/// Rotates a ring to start at its minimum `(y, x)` vertex.
pub(crate) fn rotate_to_min(ring: &mut [[f32; 2]]) {
    let start = ring
        .iter()
        .enumerate()
        .min_by(|(_, p), (_, q)| p[1].total_cmp(&q[1]).then(p[0].total_cmp(&q[0])))
        .map_or(0, |(i, _)| i);
    ring.rotate_left(start);
}
