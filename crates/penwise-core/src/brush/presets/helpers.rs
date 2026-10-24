//! `const` constructors shared by the preset table: pressure curve, velocity/tilt responses, outline and stamp
//! skeletons, flat nib and dab params. Pure data builders; the values live in `presets.rs`.
//!
//! Owner: contract.

use super::super::params::{BrushParams, CapStyle, NibParams, OutlineParams, StampParams};
use super::super::{BlendMode, BrushFamily, Color, OutlineKind, StampKind};
use crate::dynamics::{DEG, Dynamics, PressureCurve, TiltResponse, VelocityResponse};
use crate::smoothing::SmoothingParams;

/// Pressure curve with the shared floor 0.02 / saturation 0.90 and the given gamma.
pub(super) const fn curve(gamma: f32) -> PressureCurve {
    PressureCurve {
        floor: 0.02,
        saturation: 0.90,
        gamma,
    }
}

/// Velocity response with the default speed band and the given `k_v` on size and alpha.
pub(super) const fn velocity(size: f32, opacity: f32) -> VelocityResponse {
    VelocityResponse {
        size,
        opacity,
        ..VelocityResponse::NONE
    }
}

/// Tilt side shading with the default 45°–65° band and side alpha 0.5.
pub(super) const fn tilt(
    max_aspect: f32,
    max_radius_scale: f32,
    side_hardness: f32,
) -> TiltResponse {
    TiltResponse {
        max_aspect,
        max_radius_scale,
        side_hardness,
        side_alpha: 0.5,
        ..TiltResponse::NONE
    }
}

/// Round-capped outline brush with the given sizes, curve and tapers; ink color, opaque, normal blend.
pub(super) const fn outline(
    kind: OutlineKind,
    size: [f32; 2],
    gamma: f32,
    k_v: f32,
    taper_end: f32,
) -> BrushParams {
    BrushParams {
        family: BrushFamily::Outline(kind),
        color: Color::INK,
        opacity: 1.0,
        blend: BlendMode::Normal,
        size_min: size[0],
        size_max: size[1],
        size_gamma: 1.0,
        dynamics: Dynamics {
            pressure: curve(gamma),
            velocity: velocity(k_v, 0.0),
            tilt: TiltResponse::NONE,
        },
        smoothing: SmoothingParams::DEFAULT,
        outline: OutlineParams {
            taper_end,
            ..OutlineParams::ROUND
        },
        stamp: StampParams::NONE,
    }
}

/// Stamp brush with sizes, size gamma and stamp params; ink color, opaque cap, default curve, no velocity/tilt.
pub(super) const fn stamp(
    kind: StampKind,
    size: [f32; 2],
    size_gamma: f32,
    stamp: StampParams,
) -> BrushParams {
    BrushParams {
        family: BrushFamily::Stamp(kind),
        color: Color::INK,
        opacity: 1.0,
        blend: BlendMode::Normal,
        size_min: size[0],
        size_max: size[1],
        size_gamma,
        dynamics: Dynamics {
            pressure: PressureCurve::DEFAULT,
            velocity: VelocityResponse::NONE,
            tilt: TiltResponse::NONE,
        },
        smoothing: SmoothingParams::DEFAULT,
        outline: OutlineParams::ROUND,
        stamp,
    }
}

/// Fixed flat nib at `angle_deg` with thickness `t` px and flat caps.
pub(super) const fn flat_nib(angle_deg: f32, thickness: f32) -> OutlineParams {
    OutlineParams {
        cap_start: CapStyle::Flat,
        cap_end: CapStyle::Flat,
        nib: NibParams {
            angle: angle_deg * DEG,
            aspect: 1.0,
            thickness,
            fixation: 1.0,
        },
        ..OutlineParams::ROUND
    }
}

/// Plain dab params: flow, `k_flow`, hardness, spacing; no scatter/jitter/grain/spray.
pub(super) const fn dabs(
    flow: f32,
    flow_pressure: f32,
    hardness: f32,
    spacing: f32,
) -> StampParams {
    StampParams {
        flow,
        flow_pressure,
        hardness,
        spacing,
        ..StampParams::NONE
    }
}
