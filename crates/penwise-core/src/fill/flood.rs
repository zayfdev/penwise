//! Seeded flood fill with gap closing (editing-algorithms.md §3.4–3.5).
//!
//! Owner: slice c (slice-internal).

use super::distance::squared_edt;
use super::grid::{Window, get_bit, get_index, index, reset, set_bit, set_run};

/// Outcome of one flood in one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloodOutcome {
    /// Region written to `region`; it never touched a non-page window edge.
    Closed,
    /// The flood reached a window edge that is not a page edge: retry with a larger window.
    Leaked,
    /// The seed is a wall cell, or sits on a dilated wall with no free cell within `g`.
    SeedOnWall,
}

/// Inputs of [`close_and_flood`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FloodInput {
    /// Seed cell.
    pub(crate) seed: [u32; 2],
    /// Gap tolerance, cells (`0..=MAX_GAP_CELLS`).
    pub(crate) gap: u32,
}

/// Morphological closing flood:
/// 1. a seed on a wall cell is `SeedOnWall` (a tap on ink never picks a side);
/// 2. blocked = `dist ≤ g²` (dilated walls, fixed-page edges included; `g = 0` → walls only);
/// 3. if the seed is blocked, move it to the nearest unblocked cell within `g` (ties: smaller `(y, x)`),
///    else `SeedOnWall`;
/// 4. 4-connected scanline flood over unblocked cells with the explicit `stack`; reaching a non-page window
///    edge is `Leaked`;
/// 5. grow back: the 4-connected component (containing the seed) of non-wall cells with a step-4 region cell
///    within `√2·g` (via a second EDT on the region; `√2·g` recovers a 90° corner of the dilated walls
///    completely), so the region never grows across a wall into a neighbouring area and a narrow corridor
///    is entered by at most `√2·g`; reaching a non-page window edge here is `Leaked` too (the window would
///    otherwise clip the region); then tuck under ink: every wall cell 4-adjacent to the region joins it.
pub(crate) fn close_and_flood(
    window: &Window,
    input: FloodInput,
    walls: &[u64],
    dist: &mut Vec<u16>,
    stack: &mut Vec<[u32; 2]>,
    region: &mut Vec<u64>,
) -> FloodOutcome {
    let (w, h) = (window.width, window.height);
    let [sx, sy] = input.seed;
    if sx >= w || sy >= h || get_bit(walls, w, sx, sy) {
        return FloodOutcome::SeedOnWall;
    }
    let g2 = u16::try_from(input.gap * input.gap).unwrap_or(u16::MAX);
    squared_edt(walls, w, h, window.page_edges, dist);
    let Some(seed) = free_seed(dist, w, h, input.seed, input.gap) else {
        return FloodOutcome::SeedOnWall;
    };
    let open = window.page_edges.map(|page| !page);
    let grid = Grid { w, h };
    let leaked = grid.flood(seed, &|i: usize| dist[i] > g2, open, stack, region);
    if leaked {
        return FloodOutcome::Leaked;
    }
    if input.gap > 0 {
        let grow2 = g2.saturating_mul(2);
        squared_edt(region, w, h, [false; 4], dist);
        let free = |i: usize| dist[i] <= grow2 && !get_index(walls, i);
        if grid.flood(seed, &free, open, stack, region) {
            return FloodOutcome::Leaked;
        }
    }
    grid.tuck(walls, region, stack);
    FloodOutcome::Closed
}

/// The seed if unblocked, else the nearest unblocked cell within `gap` cells (ties: smaller `(y, x)`).
fn free_seed(dist: &[u16], w: u32, h: u32, seed: [u32; 2], gap: u32) -> Option<[u32; 2]> {
    let g2 = gap * gap;
    let blocked = |x: u32, y: u32| dist.get(index(w, x, y)).is_none_or(|&d| u32::from(d) <= g2);
    if !blocked(seed[0], seed[1]) {
        return Some(seed);
    }
    let mut best: Option<(u32, [u32; 2])> = None;
    for y in seed[1].saturating_sub(gap)..=seed[1].saturating_add(gap).min(h - 1) {
        for x in seed[0].saturating_sub(gap)..=seed[0].saturating_add(gap).min(w - 1) {
            let d2 = x.abs_diff(seed[0]).pow(2) + y.abs_diff(seed[1]).pow(2);
            // Scan order is (y, x) ascending, so strict `<` keeps the smaller (y, x) on ties.
            if d2 <= g2 && !blocked(x, y) && best.is_none_or(|(b, _)| d2 < b) {
                best = Some((d2, [x, y]));
            }
        }
    }
    best.map(|(_, cell)| cell)
}

/// Window extent for the flood helpers.
#[derive(Debug, Clone, Copy)]
struct Grid {
    w: u32,
    h: u32,
}

impl Grid {
    /// Scanline 4-connected flood of `free` cells from `seed` into `region` (reset first). Returns `true` as
    /// soon as the region touches a window side marked in `open` (the flood stops there).
    fn flood(
        self,
        seed: [u32; 2],
        free: &impl Fn(usize) -> bool,
        open: [bool; 4],
        stack: &mut Vec<[u32; 2]>,
        region: &mut Vec<u64>,
    ) -> bool {
        let Grid { w, h } = self;
        reset(region, w, h);
        stack.clear();
        stack.push(seed);
        let fresh = |region: &[u64], x: u32, y: u32| {
            let i = index(w, x, y);
            free(i) && !get_index(region, i)
        };
        while let Some([x, y]) = stack.pop() {
            if !fresh(region.as_slice(), x, y) {
                continue;
            }
            let mut l = x;
            while l > 0 && fresh(region.as_slice(), l - 1, y) {
                l -= 1;
            }
            let mut r = x;
            while r + 1 < w && fresh(region.as_slice(), r + 1, y) {
                r += 1;
            }
            set_run(region, w, y, l, r + 1);
            let touched = [l == 0, y == 0, r + 1 == w, y + 1 == h];
            if touched.iter().zip(open).any(|(&t, o)| t && o) {
                return true;
            }
            for ny in [y.checked_sub(1), Some(y + 1).filter(|&n| n < h)]
                .into_iter()
                .flatten()
            {
                let mut inside = false;
                for nx in l..=r {
                    let ok = fresh(region.as_slice(), nx, ny);
                    if ok && !inside {
                        stack.push([nx, ny]);
                    }
                    inside = ok;
                }
            }
        }
        false
    }

    /// Adds every wall cell 4-adjacent to the region (tuck the fill one cell under the ink).
    #[allow(
        clippy::cast_possible_truncation,
        reason = "cell indices are below width · height ≤ 4096², far inside u32"
    )]
    fn tuck(self, walls: &[u64], region: &mut [u64], list: &mut Vec<[u32; 2]>) {
        let Grid { w, h } = self;
        list.clear();
        let cells = w as usize * h as usize;
        for (wi, &word) in region.iter().enumerate() {
            let mut rest = word;
            while rest != 0 {
                let i = wi * 64 + rest.trailing_zeros() as usize;
                rest &= rest - 1;
                if i >= cells {
                    break;
                }
                let (x, y) = ((i % w as usize) as u32, (i / w as usize) as u32);
                let neighbours = [
                    x.checked_sub(1).map(|nx| [nx, y]),
                    y.checked_sub(1).map(|ny| [x, ny]),
                    Some([x + 1, y]).filter(|c| c[0] < w),
                    Some([x, y + 1]).filter(|c| c[1] < h),
                ];
                for [nx, ny] in neighbours.into_iter().flatten() {
                    if get_bit(walls, w, nx, ny) {
                        list.push([nx, ny]);
                    }
                }
            }
        }
        for [x, y] in list.drain(..) {
            set_bit(region, w, x, y);
        }
    }
}

#[cfg(test)]
mod tests;
