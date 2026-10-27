//! Paper grain for `Canvas2D` dab rendering (core `stamp::texture`).
//!
//! The viewer bakes one height tile per grain scale (`bake_grain_tile(scale, grain_period_px(scale))`, one full
//! period so the pattern repeats seamlessly), maps heights to alpha with `grain_coverage_lut` for a stroke's
//! level/softness/strength, and uses the result as a `CanvasPattern` (`destination-in` over the stroke's
//! coverage canvas) anchored at page origin under the current view transform. Canvas grain is a perceptual
//! approximation (one level per stroke: the mean `grain_level` of its dabs); exact per-dab grain is
//! server/native raster only.

use penwise_core::stamp::texture::{
    MAX_TILE_SIZE, PAPER_CELL_PX, PAPER_PERIOD, bake_tile, grain_coverage,
};
use wasm_bindgen::prelude::wasm_bindgen;

/// Pixel period of the paper at `scale`: `PAPER_PERIOD · PAPER_CELL_PX · scale`, rounded, clamped to
/// `1..=MAX_TILE_SIZE` (a NaN scale gives 1).
#[wasm_bindgen]
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 1..=MAX_TILE_SIZE before the cast"
)]
pub fn grain_period_px(scale: f32) -> u32 {
    let period = f64::from(PAPER_PERIOD) * f64::from(PAPER_CELL_PX) * f64::from(scale);
    if period.is_nan() {
        return 1;
    }
    period.round().clamp(1.0, f64::from(MAX_TILE_SIZE)) as u32
}

/// `size × size` heights `0..=255`, row-major (core `bake_tile`). Empty when `size > MAX_TILE_SIZE`.
#[wasm_bindgen]
#[must_use]
pub fn bake_grain_tile(scale: f32, size: u32) -> Vec<u8> {
    let mut tile = Vec::new();
    bake_tile(scale, size, &mut tile);
    tile
}

/// 256 alpha bytes: `round(255 · grain_coverage(h / 255, level, softness, strength))` for `h = 0..=255`.
#[wasm_bindgen]
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 before the cast"
)]
pub fn grain_coverage_lut(level: f32, softness: f32, strength: f32) -> Vec<u8> {
    (0..=255_u8)
        .map(|h| {
            let coverage = grain_coverage(f32::from(h) / 255.0, level, softness, strength);
            (255.0 * coverage).round().clamp(0.0, 255.0) as u8
        })
        .collect()
}
