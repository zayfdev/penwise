//! 2×3 affine transform shared by every item kind (editing-algorithms.md §5).
//!
//! Owner: slice a.
//!
//! Determinism: stored as `f32`; composition, inversion and point mapping are computed in `f64` with only
//! `+ − × ÷ sqrt` and rounded once to `f32`, so results are bit-identical on every target. Angles go through
//! `libm` (`sincos`, `atan2`) only.

use crate::bounds::Bounds;

/// `page = [a c e; b d f] · [x y 1]ᵀ`, i.e. `x' = a·x + c·y + e`, `y' = b·x + d·y + f` (column-vector
/// convention, same field names as SVG/canvas `matrix(a, b, c, d, e, f)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    /// Identity.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Translation by `(dx, dy)`.
    #[must_use]
    pub fn translate(dx: f32, dy: f32) -> Self {
        todo!()
    }

    /// Scale by `(sx, sy)` about the origin (negative values mirror).
    #[must_use]
    pub fn scale(sx: f32, sy: f32) -> Self {
        todo!()
    }

    /// Rotation by `angle` radians about the origin, clockwise on screen (y down): `a = cos, b = sin,
    /// c = −sin, d = cos`, with `(sin, cos)` from `libm::sincos(f64::from(angle))` rounded to `f32`.
    #[must_use]
    pub fn rotate(angle: f32) -> Self {
        todo!()
    }

    /// `next ∘ self`: apply `self` first, then `next`. Composed in `f64`, rounded once.
    #[must_use]
    pub fn then(&self, next: &Affine) -> Affine {
        todo!()
    }

    /// `self` conjugated to act about `anchor`: `Tr(anchor) · self · Tr(−anchor)` (anchor in the space
    /// `self` acts on, so the anchor is a fixed point when `self` has no translation).
    #[must_use]
    pub fn about(&self, anchor: [f32; 2]) -> Affine {
        todo!()
    }

    /// Inverse, or `None` when any component is non-finite or `|det| < 1e-12` (in `f64`).
    #[must_use]
    pub fn invert(&self) -> Option<Affine> {
        todo!()
    }

    /// Maps a point (translation applied).
    #[must_use]
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        todo!()
    }

    /// Maps a vector (translation ignored).
    #[must_use]
    pub fn apply_vector(&self, v: [f32; 2]) -> [f32; 2] {
        todo!()
    }

    /// Determinant `a·d − b·c` (computed in `f64`).
    #[must_use]
    pub fn det(&self) -> f32 {
        todo!()
    }

    /// Width factor for ink strokes under this transform: `sqrt(|det|)` (area-preserving average scale).
    /// Shapes do not use it: their brush width is applied unscaled after the transform.
    #[must_use]
    pub fn width_scale(&self) -> f32 {
        todo!()
    }

    /// True when the linear part is a similarity (rotation × uniform scale, optionally mirrored): columns
    /// orthogonal and of equal length within a relative `1e-5`. Rounded-rect corners are built in page space
    /// only for similarities (editing-algorithms.md §5).
    #[must_use]
    pub fn is_similarity(&self) -> bool {
        todo!()
    }

    /// True when every component is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        todo!()
    }

    /// Bitwise equality of all six components (`to_bits`), the comparison used by undo staleness checks.
    #[must_use]
    pub fn bit_eq(&self, other: &Affine) -> bool {
        todo!()
    }

    /// Axis-aligned bounds of `b`'s four mapped corners (empty stays empty).
    #[must_use]
    pub fn map_bounds(&self, b: &Bounds) -> Bounds {
        todo!()
    }
}
