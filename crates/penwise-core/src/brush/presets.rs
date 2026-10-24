//! The 13 built-in presets, from the ink-rendering.md §5 preset table (stamp presets retuned at Gate C for
//! visibility at normal writing pressure `c(p) ≈ 0.6`). Data, frozen by the geometry goldens: changing a value
//! moves pixels and requires re-blessing.
//!
//! Owner: contract.

mod helpers;

use super::params::{BrushParams, GrainParams, NibParams, SprayDistribution, SprayParams};
use super::{BlendMode, Color, OutlineKind, StampKind};
use crate::dynamics::{DEG, PressureCurve};
use helpers::{dabs, flat_nib, outline, stamp, tilt, velocity};

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
                dabs(0.6, 0.5, 0.45, 0.06),
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

/// Pressure-driven width 1.5–10 (linear curve, `k_v` 0.15): elliptic nib a 0.55 at φ 45°, end taper 6 px.
const fn fountain() -> BrushParams {
    let mut p = outline(OutlineKind::Fountain, [1.5, 10.0], 1.0, 0.15, 6.0);
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

/// Graphite 3–6 px, flow 1 (`k_flow` 0.4), grain 0.7 (w 0.15, scale 1), tilt side shading A 4 K 3,
/// `k_v` 0.1 on alpha.
const fn pencil() -> BrushParams {
    let mut p = stamp(
        StampKind::Pencil,
        [3.0, 6.0],
        0.5,
        dabs(1.0, 0.4, 0.8, 0.08),
    );
    p.stamp.grain = Some(GrainParams {
        scale: 1.0,
        strength: 0.7,
        softness: 0.15,
    });
    p.color = Color::GRAPHITE;
    p.opacity = 0.95;
    p.dynamics.tilt = tilt(4.0, 3.0, 0.3);
    p.dynamics.velocity = velocity(0.0, 0.1);
    p
}

/// Waxy 8–14 px, fine dense tooth (scale 1, strength 0.85, w 0.25), scatter 0.03, tilt K 1.5.
const fn crayon() -> BrushParams {
    let mut p = stamp(
        StampKind::Crayon,
        [8.0, 14.0],
        0.7,
        dabs(0.8, 0.3, 0.9, 0.06),
    );
    p.stamp.grain = Some(GrainParams {
        scale: 1.0,
        strength: 0.85,
        softness: 0.25,
    });
    p.stamp.scatter = 0.03;
    p.dynamics.tilt = tilt(1.0, 1.5, 0.8);
    p
}

/// Soft 16–60 px build-up, flow 0.45 (`k_flow` 0.7), hardness 0.4, 60 dabs/s while stationary.
const fn airbrush() -> BrushParams {
    let mut p = stamp(
        StampKind::Airbrush,
        [16.0, 60.0],
        0.7,
        dabs(0.45, 0.7, 0.4, 0.05),
    );
    p.stamp.dabs_per_second = 60.0;
    p
}

/// Spray areas 24–56 px, up to 36 opaque particles of 1–2.5 px, Gaussian disk.
const fn spray() -> BrushParams {
    let mut p = stamp(
        StampKind::Spray,
        [24.0, 56.0],
        1.0,
        dabs(1.0, 0.0, 1.0, 0.15),
    );
    p.stamp.spray = Some(SprayParams {
        particles_max: 36,
        particle_size_min: 1.0,
        particle_size_max: 2.5,
        distribution: SprayDistribution::Gaussian,
    });
    p
}

/// Graphite side shading 16–56 px, soft (h 0.45), opacity 0.75, light grain; tilt from 20°: K 2.5 A 1.8,
/// full alpha (wider and stronger when tilted).
const fn shading() -> BrushParams {
    let mut p = stamp(
        StampKind::Shading,
        [16.0, 56.0],
        0.6,
        dabs(0.7, 0.6, 0.45, 0.08),
    );
    p.stamp.grain = Some(GrainParams {
        scale: 1.0,
        strength: 0.4,
        softness: 0.3,
    });
    p.color = Color::GRAPHITE;
    p.opacity = 0.75;
    p.dynamics.tilt = tilt(1.8, 2.5, 0.3);
    p.dynamics.tilt.theta0 = 20.0 * DEG;
    p.dynamics.tilt.side_alpha = 1.0;
    p
}
