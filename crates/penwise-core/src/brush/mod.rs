//! Brush model: family dispatch, color, blend mode, params and the 13 built-in presets (PLAN "Brushes & tools").
//!
//! Owner: contract.
//!
//! Custom brushes are [`BrushParams`] values, not code. The `code`/`from_code` byte tags below are part of the
//! stored stroke format (`codec::stroke`) and are frozen: never renumber, only append.

mod params;
mod presets;

pub use params::{
    BrushParams, CapStyle, GrainParams, Jitter, NibParams, OutlineParams, SprayDistribution,
    SprayParams, StampParams,
};
pub use presets::Preset;

/// Straight (non-premultiplied) `sRGB` color, 8 bits per channel. The effective stroke alpha is
/// `a / 255 · BrushParams::opacity`; renderers premultiply at composite time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Default ink (near-black blue-black).
    pub const INK: Self = Self::rgb(20, 20, 30);
    /// Graphite gray (pencil).
    pub const GRAPHITE: Self = Self::rgb(60, 60, 68);
    /// Highlighter yellow `#FFE14D`.
    pub const HIGHLIGHTER: Self = Self::rgb(0xFF, 0xE1, 0x4D);

    /// Opaque color.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// How a stroke composites onto what is below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// Source-over.
    Normal,
    /// `C = lerp(C_dst, C_dst·C_src, α)`; such strokes also render in the under-ink band (highlighter).
    Multiply,
}

/// Outline-family brush kinds (one filled nonzero polygon per stroke).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutlineKind {
    Ballpoint,
    Fineliner,
    Fountain,
    Calligraphy,
    Marker,
    Highlighter,
}

/// Stamp-family brush kinds (dabs placed along the path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StampKind {
    Pencil,
    Crayon,
    Airbrush,
    Spray,
    Soft,
    Solid,
    Shading,
}

/// Brush family and kind; selects the geometry engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BrushFamily {
    Outline(OutlineKind),
    Stamp(StampKind),
}

impl BlendMode {
    /// Frozen byte tag: Normal 0, Multiply 1.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Multiply => 1,
        }
    }

    /// Inverse of [`Self::code`]; `None` for unknown tags.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Normal),
            1 => Some(Self::Multiply),
            _ => None,
        }
    }
}

impl BrushFamily {
    /// Frozen byte tag: high nibble family (0 outline, 1 stamp), low nibble kind in declaration order
    /// (`Outline(Ballpoint)` = `0x00` … `Outline(Highlighter)` = `0x05`, `Stamp(Pencil)` = `0x10` …
    /// `Stamp(Shading)` = `0x16`).
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Outline(k) => match k {
                OutlineKind::Ballpoint => 0x00,
                OutlineKind::Fineliner => 0x01,
                OutlineKind::Fountain => 0x02,
                OutlineKind::Calligraphy => 0x03,
                OutlineKind::Marker => 0x04,
                OutlineKind::Highlighter => 0x05,
            },
            Self::Stamp(k) => match k {
                StampKind::Pencil => 0x10,
                StampKind::Crayon => 0x11,
                StampKind::Airbrush => 0x12,
                StampKind::Spray => 0x13,
                StampKind::Soft => 0x14,
                StampKind::Solid => 0x15,
                StampKind::Shading => 0x16,
            },
        }
    }

    /// Inverse of [`Self::code`]; `None` for unknown tags.
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0x00 => Self::Outline(OutlineKind::Ballpoint),
            0x01 => Self::Outline(OutlineKind::Fineliner),
            0x02 => Self::Outline(OutlineKind::Fountain),
            0x03 => Self::Outline(OutlineKind::Calligraphy),
            0x04 => Self::Outline(OutlineKind::Marker),
            0x05 => Self::Outline(OutlineKind::Highlighter),
            0x10 => Self::Stamp(StampKind::Pencil),
            0x11 => Self::Stamp(StampKind::Crayon),
            0x12 => Self::Stamp(StampKind::Airbrush),
            0x13 => Self::Stamp(StampKind::Spray),
            0x14 => Self::Stamp(StampKind::Soft),
            0x15 => Self::Stamp(StampKind::Solid),
            0x16 => Self::Stamp(StampKind::Shading),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests;
