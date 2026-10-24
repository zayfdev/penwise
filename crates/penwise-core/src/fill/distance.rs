//! Exact squared Euclidean distance transform of the wall grid (Felzenszwalb–Huttenlocher, linear time,
//! integer-only).
//!
//! Owner: slice c (slice-internal).

/// Writes into `dist` (resized to `width · height`, row-major) the squared distance in cells² from each cell
/// to the nearest wall cell (`0` on walls), saturating at `u16::MAX`; no wall → all `u16::MAX`. Two 1-D
/// passes (columns, then rows) of the lower-envelope-of-parabolas algorithm with `i64` intermediates.
pub(crate) fn squared_edt(walls: &[u64], width: u32, height: u32, dist: &mut Vec<u16>) {
    todo!()
}
