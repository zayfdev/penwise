//! Per-dab dynamics: size, flow → alpha, scatter/jitter, tilt side shading, spray bursts (§3.3, §3.6, §3.8).
//!
//! Owner: slice c.
//!
//! Slice-internal (`pub(super)`); slice c may reshape these items. Randomness only via `super::prng`;
//! transcendentals via `libm`.

use super::spacing::DabSite;
use crate::brush::BrushParams;
use crate::geometry::Dab;

/// Diameter `D = size_min + (size_max − size_min)·P^size_gamma` with `P = point.size_pressure`
/// (`P == 0` → `size_min`, `P == 1` → `size_max` without calling `powf`), never below `size_min`.
#[must_use]
pub(super) fn diameter(params: &BrushParams, size_pressure: f32) -> f32 {
    todo!()
}

/// Spacing-normalized dab alpha: `flow_dab = flow · lerp(1, P, flow_pressure)` with `P = pressure`,
/// `alpha = 1 − (1 − flow_dab)^spacing` (`libm::powf`; `flow_dab ≥ 1` → 1, `≤ 0` → 0).
#[must_use]
pub(super) fn dab_alpha(params: &BrushParams, pressure: f32) -> f32 {
    todo!()
}

/// Builds the dab for `site` (non-spray brushes):
/// 1. `D = diameter(..)`; size jitter `D·(1 − jitter.size·f2)`; `r = D/2`.
/// 2. Tilt (`s = dynamics.tilt.side(tilt)`, skipped when `tilt.is_none()`): `radius_y = r·lerp(1, K, s)`,
///    `radius_x = radius_y·lerp(1, A, s)`, `angle = α + π/2` where `α = atan2` of `Sample::lean_vector`
///    for the site azimuth, center `+= 0.5·radius_y·lean·s`, `hardness = lerp(h, side_hardness, s)`,
///    alpha `×= lerp(1, side_alpha, s)`, `grain_strength = lerp(g, 1, s)`.
/// 3. Alpha `= dab_alpha(..) · alpha_factor · (1 − jitter.opacity·f3)`; angle `+= jitter.angle·(2f4 − 1)·π`.
/// 4. Scatter: `ρ = scatter·D·sqrt(f0)`, `ψ = 2π·f1`, center `+= ρ(cos ψ, sin ψ)`.
/// 5. `grain_level = lerp(0.15, 1, pressure)`; `grain_strength = 0` when the brush has no grain.
///
/// `fN` = `prng::rand_unit(seed, site.index, channel N)`.
#[must_use]
pub(super) fn make_dab(params: &BrushParams, site: &DabSite, seed: u64) -> Dab {
    todo!()
}

/// Spray burst for `site`: `N = round(particles_max · pressure)` (capped at `prng::MAX_PARTICLES`) hard
/// (`hardness = 1`) round particles of diameter `lerp(particle_size_min, particle_size_max, f)`, placed in the
/// disk of diameter `D` uniformly (`ρ = R·sqrt(f)`) or Gaussian (Box–Muller, σ = R/2, clipped to `R`), each
/// with alpha `dab_alpha(..)`. Appends to `out` in particle order; uses `prng::rand_particle_unit`.
pub(super) fn spray_burst(params: &BrushParams, site: &DabSite, seed: u64, out: &mut Vec<Dab>) {
    todo!()
}
