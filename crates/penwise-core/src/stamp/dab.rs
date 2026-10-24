//! Per-dab dynamics: size, flow → alpha, scatter/jitter, tilt side shading, spray bursts (§3.3, §3.6, §3.8).
//!
//! Owner: slice c.
//!
//! Slice-internal (`pub(super)`); slice c may reshape these items. Randomness only via `super::prng`;
//! transcendentals via `libm`.

use core::f32::consts::{FRAC_PI_2, PI, TAU};

use super::prng::{self, MAX_PARTICLES, channel};
use super::spacing::DabSite;
use crate::brush::{BrushParams, SprayDistribution};
use crate::dynamics::InkPoint;
use crate::float::{clamp01, lerp, max, min};
use crate::geometry::Dab;

/// Smallest emitted semi-axis, px (keeps `radius_y > 0` under full size jitter or zero sizes).
const MIN_RADIUS: f32 = 0.05;
/// Smallest emitted hardness (the falloff needs `h > 0`).
const MIN_HARDNESS: f32 = 1.0 / 1024.0;
/// Grain fill level at zero pressure (§3.7).
const GRAIN_LEVEL_MIN: f32 = 0.15;

/// Diameter `D = size_min + (size_max − size_min)·P^size_gamma` with `P = point.size_pressure`
/// (`P == 0` → `size_min`, `P == 1` → `size_max` without calling `powf`), never below `size_min`.
#[must_use]
pub(super) fn diameter(params: &BrushParams, size_pressure: f32) -> f32 {
    let p = clamp01(size_pressure);
    let d = if p <= 0.0 {
        params.size_min
    } else if p >= 1.0 {
        params.size_max
    } else {
        params.size_min + (params.size_max - params.size_min) * libm::powf(p, params.size_gamma)
    };
    // Operand order: a NaN `d` yields `size_min`.
    max(params.size_min, d)
}

/// Spacing reference diameter at `point`: [`diameter`] times the tilt radius growth `lerp(1, K, s)` (the minor
/// axis of an elliptical dab, §3.8), so elongated side-shading dabs never leave gaps.
#[must_use]
pub(super) fn reference_diameter(params: &BrushParams, point: &InkPoint) -> f32 {
    let d = diameter(params, point.size_pressure);
    let tilt = &params.dynamics.tilt;
    if tilt.is_none() {
        return d;
    }
    d * lerp(1.0, tilt.max_radius_scale, tilt.side(point.tilt))
}

/// Spacing-normalized dab alpha: `flow_dab = flow · lerp(1, P, flow_pressure)` with `P = pressure`,
/// `alpha = 1 − (1 − flow_dab)^spacing` (`libm::powf`; `flow_dab ≥ 1` → 1, `≤ 0` → 0). `spacing` is the
/// effective spacing fraction of the site ([`DabSite::spacing`], i.e. after the `MIN_STEP_PX` clamp), so the
/// alpha accumulated per diameter of travel equals `flow_dab` for any spacing.
#[must_use]
pub(super) fn dab_alpha(params: &BrushParams, pressure: f32, spacing: f32) -> f32 {
    let st = &params.stamp;
    let flow = st.flow * lerp(1.0, clamp01(pressure), st.flow_pressure);
    if flow >= 1.0 {
        return 1.0;
    }
    if flow.is_nan() || flow <= 0.0 {
        return 0.0;
    }
    clamp01(1.0 - libm::powf(1.0 - flow, spacing))
}

/// Builds the dab for `site` (non-spray brushes):
/// 1. `D = diameter(..)`; size jitter `D·(1 − jitter.size·f2)`; `r = D/2`.
/// 2. Tilt (`s = dynamics.tilt.side(tilt)`, skipped when `tilt.is_none()`): `radius_y = r·lerp(1, K, s)`,
///    `radius_x = radius_y·lerp(1, A, s)`, `angle = α + π/2` where `α = atan2` of `Sample::lean_vector`
///    for the site azimuth, center `+= 0.5·radius_y·lean·s`, `hardness = lerp(h, side_hardness, s)`,
///    alpha `×= lerp(1, side_alpha, s)`, `grain_strength = lerp(g, 1, s)`.
/// 3. Alpha `= dab_alpha(..) · alpha_factor · (1 − jitter.opacity·f3)`; angle `+= jitter.angle·(2f4 − 1)·π`.
/// 4. Scatter: `ρ = scatter·D·sqrt(f0)`, `ψ = 2π·f1`, center `+= ρ(cos ψ, sin ψ)` (`D` before jitter).
/// 5. `grain_level = lerp(0.15, 1, pressure)`; `grain_strength = 0` when the brush has no grain.
///
/// `fN` = `prng::rand_unit(seed, site.index, channel N)`. Output is sanitized: `radius_x ≥ radius_y > 0`,
/// `hardness ∈ (0, 1]`, alpha and grain values in `0..=1`.
#[must_use]
pub(super) fn make_dab(params: &BrushParams, site: &DabSite, seed: u64) -> Dab {
    let p = &site.point;
    let st = &params.stamp;
    let rand = |ch: u32| prng::rand_unit(seed, site.index, ch);
    let d = diameter(params, p.size_pressure);
    let r = 0.5 * d * (1.0 - st.jitter.size * rand(channel::SIZE));
    let mut dab = Dab {
        x: p.x,
        y: p.y,
        radius_x: r,
        radius_y: r,
        angle: 0.0,
        opacity: dab_alpha(params, p.pressure, site.spacing) * p.alpha_factor,
        hardness: st.hardness,
        grain_strength: st.grain.map_or(0.0, |g| g.strength),
        grain_level: lerp(GRAIN_LEVEL_MIN, 1.0, clamp01(p.pressure)),
    };
    apply_tilt(params, p, &mut dab);
    dab.opacity *= 1.0 - st.jitter.opacity * rand(channel::OPACITY);
    dab.angle += st.jitter.angle * (2.0 * rand(channel::ANGLE) - 1.0) * PI;
    if st.scatter > 0.0 {
        let rho = st.scatter * d * rand(channel::SCATTER_RADIUS).sqrt();
        let (sin, cos) = libm::sincosf(TAU * rand(channel::SCATTER_ANGLE));
        dab.x += rho * cos;
        dab.y += rho * sin;
    }
    sanitize(dab)
}

/// Tilt side shading (§3.8): elliptical, larger, softer, lighter dab shifted toward the lean.
fn apply_tilt(params: &BrushParams, p: &InkPoint, dab: &mut Dab) {
    let tilt = &params.dynamics.tilt;
    if tilt.is_none() {
        return;
    }
    let s = tilt.side(p.tilt);
    if s.is_nan() || s <= 0.0 {
        return;
    }
    let ry = dab.radius_y * lerp(1.0, tilt.max_radius_scale, s);
    dab.radius_y = ry;
    dab.radius_x = ry * lerp(1.0, tilt.max_aspect, s);
    // `Sample::lean_vector`: page-space lean direction for this azimuth.
    let (sin, cos) = libm::sincosf(p.azimuth);
    let lean = [sin, -cos];
    dab.angle = libm::atan2f(lean[1], lean[0]) + FRAC_PI_2;
    dab.x += 0.5 * ry * lean[0] * s;
    dab.y += 0.5 * ry * lean[1] * s;
    dab.hardness = lerp(dab.hardness, tilt.side_hardness, s);
    dab.opacity *= lerp(1.0, tilt.side_alpha, s);
    if params.stamp.grain.is_some() {
        dab.grain_strength = lerp(dab.grain_strength, 1.0, s);
    }
}

/// Clamps a dab into its documented ranges.
fn sanitize(dab: Dab) -> Dab {
    let radius_y = max(MIN_RADIUS, dab.radius_y);
    Dab {
        radius_y,
        radius_x: max(radius_y, dab.radius_x),
        opacity: clamp01(dab.opacity),
        hardness: max(MIN_HARDNESS, clamp01(dab.hardness)),
        grain_strength: clamp01(dab.grain_strength),
        grain_level: clamp01(dab.grain_level),
        ..dab
    }
}

/// Spray burst for `site`: `N = round(particles_max · pressure)` (capped at `prng::MAX_PARTICLES`) hard
/// (`hardness = 1`) round particles of diameter `lerp(particle_size_min, particle_size_max, f)`, placed in the
/// disk of diameter `D` uniformly (`ρ = R·sqrt(f)`) or Gaussian (Box–Muller, σ = R/2, clipped to `R`), each
/// with alpha `dab_alpha(..) · alpha_factor`. Appends to `out` in particle order; uses
/// `prng::rand_particle_unit`. No-op when the brush has no spray params.
pub(super) fn spray_burst(params: &BrushParams, site: &DabSite, seed: u64, out: &mut Vec<Dab>) {
    let Some(spray) = params.stamp.spray else {
        return;
    };
    let p = &site.point;
    let pressure = clamp01(p.pressure);
    let area_r = 0.5 * diameter(params, p.size_pressure);
    let opacity = clamp01(dab_alpha(params, pressure, site.spacing) * p.alpha_factor);
    let grain_strength = params.stamp.grain.map_or(0.0, |g| clamp01(g.strength));
    let grain_level = lerp(GRAIN_LEVEL_MIN, 1.0, pressure);
    for i in 0..particle_count(spray.particles_max, pressure) {
        let rand = |ch: u32| prng::rand_particle_unit(seed, site.index, i, ch);
        let size = lerp(
            spray.particle_size_min,
            spray.particle_size_max,
            rand(channel::PARTICLE_SIZE),
        );
        let rho = particle_offset(spray.distribution, area_r, rand(channel::SCATTER_RADIUS));
        let (sin, cos) = libm::sincosf(TAU * rand(channel::SCATTER_ANGLE));
        let r = max(MIN_RADIUS, 0.5 * size);
        out.push(Dab {
            x: p.x + rho * cos,
            y: p.y + rho * sin,
            radius_x: r,
            radius_y: r,
            angle: 0.0,
            opacity,
            hardness: 1.0,
            grain_strength,
            grain_level,
        });
    }
}

/// `round(particles_max · pressure)`, capped at [`MAX_PARTICLES`].
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "non-negative product, saturating cast, capped at 64 right after"
)]
fn particle_count(particles_max: u32, pressure: f32) -> u32 {
    let n = (particles_max as f32 * clamp01(pressure)).round() as u64;
    u32::try_from(n.min(MAX_PARTICLES)).unwrap_or(0)
}

/// Radial particle offset in an area of radius `area_r` from uniform `u ∈ [0, 1)`.
fn particle_offset(distribution: SprayDistribution, area_r: f32, u: f32) -> f32 {
    match distribution {
        SprayDistribution::Uniform => area_r * u.sqrt(),
        SprayDistribution::Gaussian => {
            // Polar Box–Muller: radius of a 2-D normal with σ = R/2; `1 − u ∈ (0, 1]` keeps `logf` finite.
            let z = (-2.0 * libm::logf(1.0 - u)).sqrt();
            min(0.5 * area_r * z, area_r)
        }
    }
}
