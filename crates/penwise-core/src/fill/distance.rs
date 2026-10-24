//! Exact squared Euclidean distance transform of the wall grid (Felzenszwalb–Huttenlocher, linear time,
//! integer-only; the row pass is Meijster et al.'s integer formulation of the lower envelope).
//!
//! Owner: slice c (slice-internal).

use super::grid::{BOTTOM, LEFT, RIGHT, TOP, get_index};

/// Column distance marker: no wall in this column.
const NONE: u16 = u16::MAX;

/// Writes into `dist` (resized to `width · height`, row-major) the squared distance in cells² from each cell
/// to the nearest wall cell (`0` on walls), saturating at `u16::MAX`; no wall → all `u16::MAX`. `edges`
/// (indexed like [`super::grid::Window::page_edges`]) adds a virtual wall row/column just outside that side
/// of the grid (fixed-page edges). Two 1-D passes (columns, then rows) with `i64` intermediates.
pub(crate) fn squared_edt(
    walls: &[u64],
    width: u32,
    height: u32,
    edges: [bool; 4],
    dist: &mut Vec<u16>,
) {
    let (w, h) = (width as usize, height as usize);
    dist.clear();
    dist.resize(w * h, NONE);
    let any_column_wall = column_pass(walls, w, h, edges, dist);
    let mut envelope = Envelope::default();
    for row in dist.chunks_exact_mut(w.max(1)) {
        if any_column_wall {
            envelope.row_pass(row, i64::from(width) + i64::from(height));
        } else {
            row.fill(NONE);
        }
        apply_side_walls(row, edges);
    }
}

/// Vertical distance (cells) to the nearest wall in the same column, `NONE` when the column has none.
/// Returns whether any column has a finite distance.
fn column_pass(walls: &[u64], w: usize, h: usize, edges: [bool; 4], dist: &mut [u16]) -> bool {
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let d = if get_index(walls, i) {
                0
            } else if y == 0 {
                if edges[TOP] { 1 } else { NONE }
            } else {
                step(dist[i - w])
            };
            dist[i] = d;
        }
    }
    for y in (0..h).rev() {
        for x in 0..w {
            let i = y * w + x;
            let below = if y + 1 == h {
                if edges[BOTTOM] { 1 } else { NONE }
            } else {
                step(dist[i + w])
            };
            if below < dist[i] {
                dist[i] = below;
            }
            any |= dist[i] != NONE;
        }
    }
    any
}

/// One more cell away (`NONE` stays `NONE`; real distances stay far below it).
fn step(d: u16) -> u16 {
    if d == NONE { NONE } else { d + 1 }
}

/// Virtual wall columns at `x = −1` / `x = width` (whole columns, so the distance is horizontal).
fn apply_side_walls(row: &mut [u16], edges: [bool; 4]) {
    let n = row.len();
    for (x, d) in row.iter_mut().enumerate() {
        if edges[LEFT] {
            *d = (*d).min(saturate_sq(x + 1));
        }
        if edges[RIGHT] {
            *d = (*d).min(saturate_sq(n - x));
        }
    }
}

fn saturate_sq(d: usize) -> u16 {
    u16::try_from(d.saturating_mul(d)).unwrap_or(NONE)
}

/// Reusable buffers of the row pass.
#[derive(Debug, Default)]
struct Envelope {
    /// Column distances of the current row (`inf` for `NONE`).
    g: Vec<i64>,
    /// Parabola apexes of the lower envelope.
    s: Vec<i64>,
    /// First x where each envelope parabola is minimal.
    t: Vec<i64>,
}

impl Envelope {
    /// Replaces the row's column distances by squared 2-D distances (Meijster phase 2). `inf` exceeds every
    /// real distance, so a column without walls never wins while any column has one.
    #[allow(
        clippy::cast_possible_wrap,
        reason = "row widths are at most 4096 cells"
    )]
    fn row_pass(&mut self, row: &mut [u16], inf: i64) {
        let n = row.len() as i64;
        self.g.clear();
        self.g.extend(
            row.iter()
                .map(|&d| if d == NONE { inf } else { i64::from(d) }),
        );
        self.s.clear();
        self.t.clear();
        self.s.push(0);
        self.t.push(0);
        for u in 1..n {
            while let (Some(&s), Some(&t)) = (self.s.last(), self.t.last()) {
                if self.f(t, s) > self.f(t, u) {
                    self.s.pop();
                    self.t.pop();
                } else {
                    break;
                }
            }
            if let Some(&s) = self.s.last() {
                let first = 1 + self.sep(s, u);
                if first < n {
                    self.s.push(u);
                    self.t.push(first);
                }
            } else {
                self.s.push(u);
                self.t.push(0);
            }
        }
        for u in (0..n).rev() {
            let (Some(&s), Some(&t)) = (self.s.last(), self.t.last()) else {
                break;
            };
            if let Some(cell) = usize::try_from(u).ok().and_then(|i| row.get_mut(i)) {
                *cell = u16::try_from(self.f(u, s)).unwrap_or(NONE);
            }
            if u == t {
                self.s.pop();
                self.t.pop();
            }
        }
    }

    fn gsq(&self, i: i64) -> i64 {
        let g = usize::try_from(i)
            .ok()
            .and_then(|i| self.g.get(i))
            .copied()
            .unwrap_or(0);
        g * g
    }

    /// Squared distance from `x` to the nearest wall of column `i`.
    fn f(&self, x: i64, i: i64) -> i64 {
        (x - i) * (x - i) + self.gsq(i)
    }

    /// Last x where parabola `i` is ≤ parabola `u` (`i < u`), floor division.
    fn sep(&self, i: i64, u: i64) -> i64 {
        (u * u - i * i + self.gsq(u) - self.gsq(i)).div_euclid(2 * (u - i))
    }
}
