//! Paper grain: deterministic **procedural** tileable value noise in canvas space (ink-rendering.md §3.7).
//!
//! Owner: slice c.
//!
//! Decision: procedural, not asset-based. The texture is a pure function of `(x, y, scale)`, so core needs no
//! bytes, no I/O and no license for a texture file, and every surface gets the same paper. Surfaces that want a
//! bitmap (Android shader, Canvas2D pattern) call [`bake_tile`] once (through FFI/wasm) and upload the result;
//! `assets/brushes/` may hold baked previews but they are generated from this code, never hand-edited.
//! Grain is applied at raster level (perceptual equality), but the function itself is bit-identical.

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
    todo!()
}

/// Grain coverage multiplier (Krita "subtract"-style tooth fill):
/// `lerp(1, smoothstep(1 − level − w, 1 − level + w, h), strength)` with `w = softness`.
/// Peaks catch first; pressing harder (higher `level`) fills valleys.
#[must_use]
pub fn grain_coverage(height: f32, level: f32, softness: f32, strength: f32) -> f32 {
    todo!()
}

/// Bakes one `size × size` tile of [`paper_height`] at `scale` sampled at pixel centers `(x + 0.5, y + 0.5)`,
/// row-major, `round(h·255)` per byte. Clears `out` first, then writes exactly `size²` bytes.
pub fn bake_tile(scale: f32, size: u32, out: &mut Vec<u8>) {
    todo!()
}
