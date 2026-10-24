//! `BrushParams`: every tunable of a brush (PLAN "Brushes & tools", ink-rendering.md §1–§5).
//!
//! Owner: contract.
//!
//! Units: sizes are **diameters in page px** (340 dpi ⇒ 13.39 px/mm); angles in radians; fractions `0..=1`.
//! Both families' sub-params are always present; the engine of the other family ignores them. The stroke
//! codec serializes every field, so adding one is a stroke-format version bump.

use super::{BlendMode, BrushFamily, Color};
use crate::dynamics::Dynamics;
use crate::smoothing::SmoothingParams;

/// Complete brush definition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushParams {
    pub family: BrushFamily,
    pub color: Color,
    /// Stroke-level opacity cap `0..=1` (outline: applied once to the whole polygon; stamp: §3.5 cap on the
    /// per-stroke coverage buffer).
    pub opacity: f32,
    pub blend: BlendMode,
    /// Diameter at curved pressure 0 (px). Outline radius never goes below `size_min / 2` before tapers.
    pub size_min: f32,
    /// Diameter at curved pressure 1 (px).
    pub size_max: f32,
    /// Stamp size response `D = size_min + (size_max − size_min)·P^size_gamma`. Outline brushes are linear in
    /// pressure (perfect-freehand thinning) and ignore it.
    pub size_gamma: f32,
    pub dynamics: Dynamics,
    pub smoothing: SmoothingParams,
    pub outline: OutlineParams,
    pub stamp: StampParams,
}

/// End cap shape of an outline stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapStyle {
    /// Half-disc (perfect-freehand `cap: true`).
    Round,
    /// Butt end (perfect-freehand `cap: false`; nib brushes' natural end).
    Flat,
}

impl CapStyle {
    /// Frozen byte tag: Round 0, Flat 1.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Round => 0,
            Self::Flat => 1,
        }
    }

    /// Inverse of [`Self::code`].
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Round),
            1 => Some(Self::Flat),
            _ => None,
        }
    }
}

/// Flat/elliptic nib model (§2.1 calligraphy & highlighter, §2.2 fountain).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NibParams {
    /// Nib axis angle φ, radians, page axes (y down).
    pub angle: f32,
    /// Fountain ellipse minor/major ratio `a` (0.5–0.7); unused by flat nibs.
    pub aspect: f32,
    /// Flat-nib thickness `t`, px; unused by the fountain ellipse.
    pub thickness: f32,
    /// 1 = fixed nib angle, 0 = nib always ⟂ stroke (`φ_eff = lerp_angle(θ + π/2, φ, fixation)`).
    pub fixation: f32,
}

impl NibParams {
    /// Neutral nib (round brushes ignore it).
    pub const NONE: Self = Self {
        angle: 0.0,
        aspect: 1.0,
        thickness: 0.0,
        fixation: 1.0,
    };
}

/// Outline-family params.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineParams {
    /// perfect-freehand `smoothing` (outline point decimation, `minDist = size·smoothing`); default 0.5.
    pub smoothing: f32,
    /// Start taper length, px of arc (0 = none). Numeric only: `taper: true` is not supported (§1.4).
    pub taper_start: f32,
    /// End taper length, px of arc (0 = none).
    pub taper_end: f32,
    pub cap_start: CapStyle,
    pub cap_end: CapStyle,
    pub nib: NibParams,
}

/// Paper grain (canvas-space "texturized" placement, §3.7).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrainParams {
    /// Texture scale (1 = fine pencil tooth, 3 = coarse crayon).
    pub scale: f32,
    /// `grain_strength`: 0 = no grain, 1 = full grain modulation.
    pub strength: f32,
    /// Smoothstep edge half-width `w` (pencil 0.15, crayon 0.05).
    pub softness: f32,
}

/// Particle placement inside a spray area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SprayDistribution {
    /// Uniform over the disk (`ρ = R·sqrt(f)`).
    Uniform,
    /// Box–Muller Gaussian (`libm::logf`/`sincosf`), denser center, clipped to the area radius.
    Gaussian,
}

impl SprayDistribution {
    /// Frozen byte tag: Uniform 0, Gaussian 1.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Uniform => 0,
            Self::Gaussian => 1,
        }
    }

    /// Inverse of [`Self::code`].
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Uniform),
            1 => Some(Self::Gaussian),
            _ => None,
        }
    }
}

/// Spray particles (§3.6, Krita spray model): each spray "dab" of diameter `D` emits
/// `N = round(particles_max · P)` hard particles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SprayParams {
    pub particles_max: u32,
    /// Particle diameter range, px (`lerp(min, max, f)` per particle).
    pub particle_size_min: f32,
    pub particle_size_max: f32,
    pub distribution: SprayDistribution,
}

/// Per-dab random variation, each `0..=1` (§3.6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jitter {
    /// `D·(1 − size·f)`.
    pub size: f32,
    /// `alpha·(1 − opacity·f)`.
    pub opacity: f32,
    /// `angle += angle·(2f − 1)·π`.
    pub angle: f32,
}

impl Jitter {
    /// No jitter.
    pub const NONE: Self = Self {
        size: 0.0,
        opacity: 0.0,
        angle: 0.0,
    };
}

/// Stamp-family params.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StampParams {
    /// Alpha accumulated per one diameter of travel (spacing-normalized, §3.3).
    pub flow: f32,
    /// `k_flow`: `flow_dab = flow · lerp(1, P, flow_pressure)`.
    pub flow_pressure: f32,
    /// Dab hardness `h` in `(0, 1]` (`MyPaint` two-segment falloff, §3.4).
    pub hardness: f32,
    /// Dab spacing as a fraction of the diameter; effective step clamped to ≥ 0.5 px.
    pub spacing: f32,
    /// Position scatter radius as a fraction of the diameter (uniform disk).
    pub scatter: f32,
    pub jitter: Jitter,
    /// Temporal dab emission while stationary, dabs/s from sample timestamps (airbrush only; 0 = off).
    pub dabs_per_second: f32,
    /// Paper grain; `None` = untextured.
    pub grain: Option<GrainParams>,
    /// Spray particles; `None` = one dab per spacing step.
    pub spray: Option<SprayParams>,
}

impl StampParams {
    /// Neutral stamp params (outline presets carry these; never used by the outline engine).
    pub const NONE: Self = Self {
        flow: 1.0,
        flow_pressure: 0.0,
        hardness: 1.0,
        spacing: 0.1,
        scatter: 0.0,
        jitter: Jitter::NONE,
        dabs_per_second: 0.0,
        grain: None,
        spray: None,
    };
}

impl OutlineParams {
    /// Round caps, no taper, neutral nib, smoothing 0.5 (stamp presets carry these; never used).
    pub const ROUND: Self = Self {
        smoothing: 0.5,
        taper_start: 0.0,
        taper_end: 0.0,
        cap_start: CapStyle::Round,
        cap_end: CapStyle::Round,
        nib: NibParams::NONE,
    };
}
