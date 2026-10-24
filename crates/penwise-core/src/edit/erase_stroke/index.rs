//! Bucketed eraser segments: proximity tests against a long eraser path visit only the segments whose boxes
//! come near the query, so erasing over many long strokes costs ~linear work instead of
//! `centreline segments × eraser segments`.
//!
//! Exactness: a segment farther than `tol` from the query cannot pass a `≤ tol` / `< tol` distance test.
//! Query boxes are inflated by `tol` plus a rounding slack, so the candidate set is a superset of the
//! segments a full scan would accept and every caller gets the same answer as a full scan.

use super::segments;
use crate::bounds::Bounds;
use crate::float::max;

/// Paths with at most this many segments are scanned linearly (a grid would not pay off).
const LINEAR_MAX: usize = 16;
/// Absolute rounding slack added to every query box (page px).
const SLACK_ABS: f32 = 1e-3;
/// Relative rounding slack (of the query's coordinate magnitude and tolerance); ≈ 8 f32 ulps.
const SLACK_REL: f32 = 1e-6;

type Segment = ([f32; 2], [f32; 2]);

/// The eraser path's segments (one degenerate segment for a single point), optionally bucketed in a grid.
#[derive(Debug)]
pub(crate) struct EraserIndex {
    segs: Vec<(Segment, Bounds)>,
    /// Bounds of the path points (not inflated).
    path_bounds: Bounds,
    grid: Option<Grid>,
}

/// Uniform grid over `path_bounds`; cell `r * cols + c` lists the segments whose box overlaps it.
#[derive(Debug)]
struct Grid {
    origin: [f32; 2],
    inv_cell: f32,
    cols: usize,
    rows: usize,
    /// CSR offsets: cell `k` holds `ids[starts[k]..starts[k + 1]]`.
    starts: Vec<usize>,
    ids: Vec<usize>,
}

impl EraserIndex {
    /// Index of `path` (finite points; callers check [`super::eraser_bounds`] first).
    pub(crate) fn new(path: &[[f32; 2]]) -> Self {
        Self::build(path, true)
    }

    /// Full-scan index (reference for tests).
    #[cfg(test)]
    pub(crate) fn linear(path: &[[f32; 2]]) -> Self {
        Self::build(path, false)
    }

    fn build(path: &[[f32; 2]], bucket: bool) -> Self {
        let segs: Vec<(Segment, Bounds)> = segments(path, false)
            .map(|(a, b)| {
                let mut bx = Bounds::from_point(a[0], a[1]);
                bx.include_point(b[0], b[1]);
                ((a, b), bx)
            })
            .collect();
        let path_bounds = segs
            .iter()
            .fold(Bounds::EMPTY, |acc, (_, bx)| acc.union(bx));
        let grid = if bucket && segs.len() > LINEAR_MAX {
            Grid::build(&segs, &path_bounds)
        } else {
            None
        };
        Self {
            segs,
            path_bounds,
            grid,
        }
    }

    /// True when `test(e0, e1)` holds for some eraser segment whose box meets `query`. `query` must contain
    /// every point within the caller's tolerance of the tested geometry (use [`query_box`]).
    pub(crate) fn any(
        &self,
        query: &Bounds,
        mut test: impl FnMut([f32; 2], [f32; 2]) -> bool,
    ) -> bool {
        if !query.intersects(&self.path_bounds) {
            return false;
        }
        let mut check = |i: usize| {
            count_test();
            self.segs
                .get(i)
                .is_some_and(|&((e0, e1), bx)| bx.intersects(query) && test(e0, e1))
        };
        let Some(g) = &self.grid else {
            return (0..self.segs.len()).any(check);
        };
        let (c0, r0) = g.cell(query.min_x, query.min_y);
        let (c1, r1) = g.cell(query.max_x, query.max_y);
        for r in r0..=r1 {
            for c in c0..=c1 {
                let k = r * g.cols + c;
                let (Some(&lo), Some(&hi)) = (g.starts.get(k), g.starts.get(k + 1)) else {
                    continue;
                };
                if g.ids.get(lo..hi).unwrap_or(&[]).iter().any(|&i| check(i)) {
                    return true;
                }
            }
        }
        false
    }
}

/// Box of segment `a–b` grown by `tol` plus rounding slack.
pub(crate) fn query_box(a: [f32; 2], b: [f32; 2], tol: f32) -> Bounds {
    let mut q = Bounds::from_point(a[0], a[1]);
    q.include_point(b[0], b[1]);
    let mag = max(
        max(q.min_x.abs(), q.max_x.abs()),
        max(q.min_y.abs(), q.max_y.abs()),
    );
    q.inflate(tol + SLACK_ABS + (mag + tol.abs()) * SLACK_REL)
}

impl Grid {
    /// Cell size `max(mean segment length, √(area / n), (w + h) / n)` keeps the grid at ≤ ~2n cells and
    /// each short query within a few cells. `None` for a degenerate extent.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        reason = "f64 length sum rounded once; segment counts are far below 2^24 in practice"
    )]
    fn build(segs: &[(Segment, Bounds)], pb: &Bounds) -> Option<Self> {
        let n = segs.len();
        let nf = n as f32;
        let total: f64 = segs
            .iter()
            .map(|&((a, b), _)| {
                let dx = f64::from(b[0]) - f64::from(a[0]);
                let dy = f64::from(b[1]) - f64::from(a[1]);
                (dx * dx + dy * dy).sqrt()
            })
            .sum();
        let (w, h) = (pb.width(), pb.height());
        let mean = (total / n as f64) as f32;
        let cell = max(max(mean, (w * h / nf).sqrt()), (w + h) / nf);
        let inv_cell = 1.0 / cell;
        if !(cell.is_finite() && cell > 0.0 && inv_cell.is_finite()) {
            return None;
        }
        let mut grid = Self {
            origin: [pb.min_x, pb.min_y],
            inv_cell,
            cols: axis_cells(w, inv_cell, n),
            rows: axis_cells(h, inv_cell, n),
            starts: Vec::new(),
            ids: Vec::new(),
        };
        let cells = grid.cols * grid.rows;
        let mut counts = vec![0usize; cells + 1];
        for (_, bx) in segs {
            grid.for_cells(bx, |k| {
                if let Some(c) = counts.get_mut(k + 1) {
                    *c += 1;
                }
            });
        }
        let mut acc = 0usize;
        for c in &mut counts {
            acc += *c;
            *c = acc;
        }
        let mut next = counts.clone();
        let mut ids = vec![0usize; counts.last().copied().unwrap_or(0)];
        for (i, (_, bx)) in segs.iter().enumerate() {
            grid.for_cells(bx, |k| {
                if let Some(slot) = next.get_mut(k) {
                    if let Some(id) = ids.get_mut(*slot) {
                        *id = i;
                    }
                    *slot += 1;
                }
            });
        }
        grid.starts = counts;
        grid.ids = ids;
        Some(grid)
    }

    /// `(column, row)` of a point, clamped to the grid (monotone in each coordinate).
    fn cell(&self, x: f32, y: f32) -> (usize, usize) {
        (
            axis_index(x - self.origin[0], self.inv_cell, self.cols),
            axis_index(y - self.origin[1], self.inv_cell, self.rows),
        )
    }

    fn for_cells(&self, bx: &Bounds, mut f: impl FnMut(usize)) {
        let (c0, r0) = self.cell(bx.min_x, bx.min_y);
        let (c1, r1) = self.cell(bx.max_x, bx.max_y);
        for r in r0..=r1 {
            for c in c0..=c1 {
                f(r * self.cols + c);
            }
        }
    }
}

/// Cells along one axis of extent `len` (at most `n + 1`).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "saturating float→int cast of a non-negative, clamped cell count"
)]
fn axis_cells(len: f32, inv_cell: f32, n: usize) -> usize {
    let k = (len * inv_cell) as usize;
    (if k > n { n } else { k }) + 1
}

/// Cell index of offset `d` (negative/NaN → 0, past the end → last).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "saturating float→int cast (negative and NaN map to 0), then clamped"
)]
fn axis_index(d: f32, inv_cell: f32, len: usize) -> usize {
    let k = (d * inv_cell) as usize;
    if k >= len { len.saturating_sub(1) } else { k }
}

#[cfg(test)]
thread_local! {
    static TESTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Eraser segments visited (box check + test) on this thread since the last call (work counter for tests).
#[cfg(test)]
pub(crate) fn take_test_count() -> u64 {
    TESTS.with(|c| c.replace(0))
}

#[inline]
fn count_test() {
    #[cfg(test)]
    TESTS.with(|c| c.set(c.get() + 1));
}
