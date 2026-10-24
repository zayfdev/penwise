//! Paper grain: deterministic **procedural** tileable value noise in canvas space (ink-rendering.md §3.7).
//!
//! Owner: slice c.
//!
//! Decision: procedural, not asset-based. The texture is a pure function of `(x, y, scale)`, so core needs no
//! bytes, no I/O and no license for a texture file, and every surface gets the same paper. Surfaces that want a
//! bitmap (Android shader, `Canvas2D` pattern) call [`bake_tile`] once (through FFI/wasm) and upload the result;
//! `assets/brushes/` may hold baked previews but they are generated from this code, never hand-edited.
//! Grain is applied at raster level (perceptual equality), but the function itself is bit-identical.

use super::prng::mix64;
use crate::float::{clamp01, lerp, smoothstep};

/// Lattice cells per tile edge; the noise repeats every `PAPER_PERIOD · PAPER_CELL_PX · scale` px.
pub const PAPER_PERIOD: u32 = 256;
/// Lattice cell size at `scale = 1`, px.
pub const PAPER_CELL_PX: f32 = 2.0;
/// Fixed paper seed (canvas-wide, not per stroke).
pub const PAPER_SEED: u64 = 0x7061_7065_725F_7631;

/// Paper height `h ∈ [0, 1]` at page position `(x, y)`:
/// two octaves of value noise, octave `o ∈ {0, 1}` with cell `PAPER_CELL_PX·scale / 2^o`, weight `1, 0.5`,
/// normalized by `1.5`. Lattice value at integer cell `(i, j)` (wrapped `mod PAPER_PERIOD·2^o`) is
/// `(prng::mix64(PAPER_SEED ^ o<<62 ^ i<<31 ^ j) >> 40) · 2⁻²⁴`; bilinear interpolation with smoothstep weights
/// `t²(3 − 2t)`. Cell coordinates use `floor` (IEEE-exact). `scale ≤ 0` or non-finite input → 0.5.
#[must_use]
pub fn paper_height(x: f32, y: f32, scale: f32) -> f32 {
    if !(x.is_finite() && y.is_finite() && scale.is_finite()) || scale <= 0.0 {
        return 0.5;
    }
    let cell = PAPER_CELL_PX * scale;
    let (Some(coarse), Some(fine)) = (octave(x, y, cell, 0), octave(x, y, cell * 0.5, 1)) else {
        return 0.5;
    };
    clamp01((coarse + 0.5 * fine) / 1.5)
}

/// One value-noise octave with lattice period `PAPER_PERIOD·2^o` cells; `None` when the cell coordinates
/// overflow (degenerate `scale`).
fn octave(x: f32, y: f32, cell: f32, o: u32) -> Option<f32> {
    let gx = x / cell;
    let gy = y / cell;
    if !(gx.is_finite() && gy.is_finite()) {
        return None;
    }
    let fx = gx.floor();
    let fy = gy.floor();
    let tx = smooth_weight(gx - fx);
    let ty = smooth_weight(gy - fy);
    let period = i64::from(PAPER_PERIOD) << o;
    let i0 = wrap_cell(fx, period);
    let j0 = wrap_cell(fy, period);
    let i1 = (i0 + 1) % period.cast_unsigned();
    let j1 = (j0 + 1) % period.cast_unsigned();
    let top = lerp(lattice(o, i0, j0), lattice(o, i1, j0), tx);
    let bottom = lerp(lattice(o, i0, j1), lattice(o, i1, j1), tx);
    Some(lerp(top, bottom, ty))
}

/// `floor`ed cell coordinate wrapped into `0..period`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "integral f32, saturating cast; wrapped right after"
)]
fn wrap_cell(floored: f32, period: i64) -> u64 {
    (floored as i64).rem_euclid(period).cast_unsigned()
}

/// Smoothstep weight `t²(3 − 2t)` for `t ∈ [0, 1)`.
fn smooth_weight(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Lattice value in `[0, 1)` at wrapped cell `(i, j)` of octave `o`.
#[allow(clippy::cast_precision_loss, reason = "24-bit integer, exact in f32")]
fn lattice(o: u32, i: u64, j: u64) -> f32 {
    let key = PAPER_SEED ^ (u64::from(o) << 62) ^ (i << 31) ^ j;
    (mix64(key) >> 40) as f32 * (1.0 / 16_777_216.0)
}

/// Grain coverage multiplier (Krita "subtract"-style tooth fill):
/// `lerp(1, smoothstep(1 − level − w, 1 − level + w, h), strength)` with `w = softness`.
/// Peaks catch first; pressing harder (higher `level`) fills valleys.
#[must_use]
pub fn grain_coverage(height: f32, level: f32, softness: f32, strength: f32) -> f32 {
    let edge = 1.0 - level;
    lerp(
        1.0,
        smoothstep(edge - softness, edge + softness, height),
        strength,
    )
}

/// Largest tile edge [`bake_tile`] accepts, px (64 MiB of output); one full paper period
/// (`PAPER_PERIOD · PAPER_CELL_PX · scale` px) fits up to `scale = 16`.
pub const MAX_TILE_SIZE: u32 = 8192;

/// Bakes one `size × size` tile of [`paper_height`] at `scale` sampled at pixel centers `(x + 0.5, y + 0.5)`,
/// row-major, `round(h·255)` per byte. Clears `out` first, then writes exactly `size²` bytes; leaves `out` empty
/// when `size > MAX_TILE_SIZE` or the buffer cannot be allocated (never panics or aborts on hostile sizes).
#[allow(
    clippy::cast_precision_loss,
    reason = "pixel indices ≤ 8192, exact in f32"
)]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped to 0..=255 before the cast"
)]
pub fn bake_tile(scale: f32, size: u32, out: &mut Vec<u8>) {
    out.clear();
    if size > MAX_TILE_SIZE {
        return;
    }
    let Ok(n) = usize::try_from(u64::from(size) * u64::from(size)) else {
        return;
    };
    if out.try_reserve_exact(n).is_err() {
        return;
    }
    for py in 0..size {
        let y = py as f32 + 0.5;
        for px in 0..size {
            let h = clamp01(paper_height(px as f32 + 0.5, y, scale));
            out.push((h * 255.0).round() as u8);
        }
    }
}

#[cfg(test)]
mod tests;
