//! Seeded flood fill with gap closing (editing-algorithms.md §3.4–3.5).
//!
//! Owner: slice c (slice-internal).

use super::grid::Window;

/// Outcome of one flood in one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FloodOutcome {
    /// Region written to `region`; it never touched a non-page window edge.
    Closed,
    /// The flood reached a window edge that is not a page edge: retry with a larger window.
    Leaked,
    /// The seed (after moving to the nearest free cell within `g`) sits on a dilated wall.
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
/// 1. blocked = `dist ≤ g²` (dilated walls; `g = 0` → walls only);
/// 2. if the seed is blocked, move it to the nearest unblocked cell within `g` (ties: smaller `(y, x)`),
///    else `SeedOnWall`;
/// 3. 4-connected scanline flood over unblocked cells with the explicit `stack`;
/// 4. grow back: region' = cells with a region cell within `g` (via a second EDT on the region) that are not
///    walls; then tuck under ink: dilate by 1 cell into wall cells.
///
/// Leak detection uses step 3's region touching a non-page window edge.
pub(crate) fn close_and_flood(
    window: &Window,
    input: FloodInput,
    walls: &[u64],
    dist: &mut Vec<u16>,
    stack: &mut Vec<[u32; 2]>,
    region: &mut Vec<u64>,
) -> FloodOutcome {
    todo!()
}
