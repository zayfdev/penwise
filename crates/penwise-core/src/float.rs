//! Deterministic float primitives shared by every slice (core-determinism.md §1, rules 2–3).
//!
//! Owner: contract.
//!
//! `f32::min`/`max` may return either operand when they compare equal (±0), and that differs between x86 and
//! wasm, so core uses these comparison-defined helpers instead (`clippy.toml` bans the std ones). Only
//! IEEE-exact operations appear here (`+ − × ÷`, comparisons), so results are bit-identical on every target.

/// Smaller of `a` and `b`; returns `a` on ties (including `+0.0` vs `-0.0`) and when `b` is NaN.
#[must_use]
#[inline]
pub fn min(a: f32, b: f32) -> f32 {
    if b < a { b } else { a }
}

/// Larger of `a` and `b`; returns `a` on ties (including `+0.0` vs `-0.0`) and when `b` is NaN.
#[must_use]
#[inline]
pub fn max(a: f32, b: f32) -> f32 {
    if b > a { b } else { a }
}

/// Clamps `x` into `[0, 1]`; NaN maps to `0.0`.
#[must_use]
#[inline]
pub fn clamp01(x: f32) -> f32 {
    if x > 0.0 {
        if x < 1.0 { x } else { 1.0 }
    } else {
        0.0
    }
}

/// Linear interpolation `a + (b − a)·t` (no FMA; same op order on every target).
#[must_use]
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Hermite smoothstep: 0 for `x ≤ e0`, 1 for `x ≥ e1`, `t²(3 − 2t)` with `t = (x − e0)/(e1 − e0)` between.
/// If `e1 ≤ e0` it degrades to a step at `e0` (`x < e0` → 0, else 1). NaN `x` → 0.
#[must_use]
#[inline]
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    if e1 <= e0 {
        return if x >= e0 { 1.0 } else { 0.0 };
    }
    let t = clamp01((x - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

/// Canonical output float: `Some(x + 0.0)` (maps `-0.0` to `+0.0`) when finite, `None` for NaN/±inf.
/// Every float that reaches serialized output or a hash goes through this boundary (rule 3).
#[must_use]
#[inline]
pub fn canon(x: f32) -> Option<f32> {
    if x.is_finite() { Some(x + 0.0) } else { None }
}
