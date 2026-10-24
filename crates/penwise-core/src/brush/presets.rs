//! The 13 built-in presets, from the ink-rendering.md §5 preset table. Data, frozen by the geometry goldens:
//! changing a value moves pixels and requires re-blessing.
//!
//! Owner: contract.

use super::params::{
    BrushParams, CapStyle, GrainParams, NibParams, OutlineParams, SprayDistribution, SprayParams,
    StampParams,
};
use super::{BlendMode, BrushFamily, Color, OutlineKind, StampKind};
use crate::dynamics::{DEG, Dynamics, PressureCurve, TiltResponse, VelocityResponse};
use crate::smoothing::SmoothingParams;

/// Built-in brush presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    Ballpoint,
    Fineliner,
    Fountain,
    Calligraphy,
    Marker,
    Highlighter,
    Pencil,
    Crayon,
    Airbrush,
    Spray,
    Soft,
    Solid,
    Shading,
}

const fn curve(gamma: f32) -> PressureCurve {
    PressureCurve {
        floor: 0.02,
        saturation: 0.90,
        gamma,
    }
}

const fn velocity(size: f32, opacity: f32) -> VelocityResponse {
    VelocityResponse {
        size,
        opacity,
        ..VelocityResponse::NONE
    }
}

const fn tilt(max_aspect: f32, max_radius_scale: f32, side_hardness: f32) -> TiltResponse {
    TiltResponse {
        max_aspect,
        max_radius_scale,
        side_hardness,
        side_alpha: 0.5,
        ..TiltResponse::NONE
    }
}

/// Round-capped outline brush with the given sizes, curve and tapers; ink color, opaque, normal blend.
const fn outline(
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
const fn stamp(
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

const fn flat_nib(angle_deg: f32, thickness: f32) -> OutlineParams {
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

const fn dabs(flow: f32, flow_pressure: f32, hardness: f32, spacing: f32) -> StampParams {
    StampParams {
        flow,
        flow_pressure,
        hardness,
        spacing,
        ..StampParams::NONE
    }
}

impl Preset {
    /// Every preset in declaration order (golden test order).
    pub const ALL: [Preset; 13] = [
        Self::Ballpoint,
        Self::Fineliner,
        Self::Fountain,
        Self::Calligraphy,
        Self::Marker,
        Self::Highlighter,
        Self::Pencil,
        Self::Crayon,
        Self::Airbrush,
        Self::Spray,
        Self::Soft,
        Self::Solid,
        Self::Shading,
    ];

    /// Stable lowercase name (golden summary lines, UI ids).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ballpoint => "ballpoint",
            Self::Fineliner => "fineliner",
            Self::Fountain => "fountain",
            Self::Calligraphy => "calligraphy",
            Self::Marker => "marker",
            Self::Highlighter => "highlighter",
            Self::Pencil => "pencil",
            Self::Crayon => "crayon",
            Self::Airbrush => "airbrush",
            Self::Spray => "spray",
            Self::Soft => "soft",
            Self::Solid => "solid",
            Self::Shading => "shading",
        }
    }

    /// The preset's full parameter set (ink-rendering.md §5 table).
    #[must_use]
    pub const fn params(self) -> BrushParams {
        match self {
            Self::Ballpoint => outline(OutlineKind::Ballpoint, [2.4, 5.0], 0.75, 0.1, 2.0),
            Self::Fineliner => outline(OutlineKind::Fineliner, [3.8, 4.6], 1.0, 0.0, 0.0),
            Self::Fountain => fountain(),
            Self::Calligraphy => calligraphy(),
            Self::Marker => marker(),
            Self::Highlighter => highlighter(),
            Self::Pencil => pencil(),
            Self::Crayon => crayon(),
            Self::Airbrush => airbrush(),
            Self::Spray => spray(),
            Self::Soft => stamp(
                StampKind::Soft,
                [12.0, 48.0],
                1.0,
                dabs(0.15, 0.6, 0.25, 0.08),
            ),
            Self::Solid => stamp(
                StampKind::Solid,
                [6.0, 24.0],
                1.0,
                dabs(1.0, 0.0, 0.95, 0.06),
            ),
            Self::Shading => shading(),
        }
    }
}

/// Pressure + mild direction: elliptic nib a 0.55 at φ 45°, end taper 6 px.
const fn fountain() -> BrushParams {
    let mut p = outline(OutlineKind::Fountain, [2.5, 9.0], 0.8, 0.25, 6.0);
    p.outline.nib = NibParams {
        angle: 45.0 * DEG,
        aspect: 0.55,
        thickness: 0.0,
        fixation: 1.0,
    };
    p
}

/// Flat nib W 10–22, t 1.5, φ 40°, flat caps.
const fn calligraphy() -> BrushParams {
    let mut p = outline(OutlineKind::Calligraphy, [10.0, 22.0], 0.75, 0.0, 0.0);
    p.outline = flat_nib(40.0, 1.5);
    p
}

/// Round broad marker 14–18 (thinning 0.125), flat opacity 0.9.
const fn marker() -> BrushParams {
    let mut p = outline(OutlineKind::Marker, [14.0, 18.0], 1.0, 0.0, 0.0);
    p.opacity = 0.9;
    p
}

/// Chisel W 26, t 8, φ 90°, pressure off, `#FFE14D` at 0.35, multiply.
const fn highlighter() -> BrushParams {
    let mut p = outline(OutlineKind::Highlighter, [26.0, 26.0], 1.0, 0.0, 0.0);
    p.color = Color::HIGHLIGHTER;
    p.opacity = 0.35;
    p.blend = BlendMode::Multiply;
    p.dynamics.pressure = PressureCurve::CONSTANT;
    p.outline = flat_nib(90.0, 8.0);
    p
}

/// Graphite 3–6 px, grain 0.7 (w 0.15, scale 1), tilt side shading A 4 K 3, `k_v` 0.15 on alpha.
const fn pencil() -> BrushParams {
    let mut p = stamp(
        StampKind::Pencil,
        [3.0, 6.0],
        0.5,
        dabs(0.35, 0.85, 0.6, 0.10),
    );
    p.stamp.grain = Some(GrainParams {
        scale: 1.0,
        strength: 0.7,
        softness: 0.15,
    });
    p.color = Color::GRAPHITE;
    p.opacity = 0.9;
    p.dynamics.tilt = tilt(4.0, 3.0, 0.3);
    p.dynamics.velocity = velocity(0.0, 0.15);
    p
}

/// Waxy 8–14 px, coarse grain (scale 3, strength 0.9, w 0.05), scatter 0.05, tilt K 1.5.
const fn crayon() -> BrushParams {
    let mut p = stamp(
        StampKind::Crayon,
        [8.0, 14.0],
        0.7,
        dabs(0.6, 0.5, 0.8, 0.08),
    );
    p.stamp.grain = Some(GrainParams {
        scale: 3.0,
        strength: 0.9,
        softness: 0.05,
    });
    p.stamp.scatter = 0.05;
    p.dynamics.tilt = tilt(1.0, 1.5, 0.8);
    p
}

/// Soft 30–90 px build-up, pressure → flow, 60 dabs/s while stationary.
const fn airbrush() -> BrushParams {
    let mut p = stamp(
        StampKind::Airbrush,
        [30.0, 90.0],
        0.3,
        dabs(0.05, 1.0, 0.05, 0.05),
    );
    p.stamp.dabs_per_second = 60.0;
    p
}

/// Spray areas 40–80 px, up to 12 particles of 1–2.5 px, uniform disk.
const fn spray() -> BrushParams {
    let mut p = stamp(
        StampKind::Spray,
        [40.0, 80.0],
        1.0,
        dabs(0.6, 0.0, 1.0, 0.25),
    );
    p.stamp.spray = Some(SprayParams {
        particles_max: 12,
        particle_size_min: 1.0,
        particle_size_max: 2.5,
        distribution: SprayDistribution::Uniform,
    });
    p
}

/// Wide 40–160 px, very soft, 0.3 opacity cap, light grain, tilt K 2 A 1.5.
const fn shading() -> BrushParams {
    let mut p = stamp(
        StampKind::Shading,
        [40.0, 160.0],
        0.5,
        dabs(0.06, 0.8, 0.1, 0.10),
    );
    p.stamp.grain = Some(GrainParams {
        scale: 1.0,
        strength: 0.3,
        softness: 0.15,
    });
    p.opacity = 0.3;
    p.dynamics.tilt = tilt(1.5, 2.0, 0.1);
    p
}
