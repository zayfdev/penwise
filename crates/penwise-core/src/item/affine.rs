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
        Self {
            e: dx,
            f: dy,
            ..Self::IDENTITY
        }
    }

    /// Scale by `(sx, sy)` about the origin (negative values mirror).
    #[must_use]
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            d: sy,
            ..Self::IDENTITY
        }
    }

    /// Rotation by `angle` radians about the origin, clockwise on screen (y down): `a = cos, b = sin,
    /// c = −sin, d = cos`, with `(sin, cos)` from `libm::sincos(f64::from(angle))` rounded to `f32`.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "documented single rounding to f32"
    )]
    pub fn rotate(angle: f32) -> Self {
        let (s, c) = libm::sincos(f64::from(angle));
        let (s, c) = (s as f32, c as f32);
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            e: 0.0,
            f: 0.0,
        }
    }

    /// `next ∘ self`: apply `self` first, then `next`. Composed in `f64`, rounded once.
    #[must_use]
    pub fn then(&self, next: &Affine) -> Affine {
        let s = Wide::of(self);
        let n = Wide::of(next);
        Wide {
            a: n.a * s.a + n.c * s.b,
            b: n.b * s.a + n.d * s.b,
            c: n.a * s.c + n.c * s.d,
            d: n.b * s.c + n.d * s.d,
            e: n.a * s.e + n.c * s.f + n.e,
            f: n.b * s.e + n.d * s.f + n.f,
        }
        .narrow()
    }

    /// `self` conjugated to act about `anchor`: `Tr(anchor) · self · Tr(−anchor)` (anchor in the space
    /// `self` acts on, so the anchor is a fixed point when `self` has no translation).
    #[must_use]
    pub fn about(&self, anchor: [f32; 2]) -> Affine {
        let s = Wide::of(self);
        let (x, y) = (f64::from(anchor[0]), f64::from(anchor[1]));
        Wide {
            e: s.e + x - (s.a * x + s.c * y),
            f: s.f + y - (s.b * x + s.d * y),
            ..s
        }
        .narrow()
    }

    /// Inverse, or `None` when any component is non-finite or `|det| < 1e-12` (in `f64`).
    #[must_use]
    pub fn invert(&self) -> Option<Affine> {
        if !self.is_finite() {
            return None;
        }
        let s = Wide::of(self);
        let det = s.a * s.d - s.b * s.c;
        if det.abs() < 1e-12 {
            return None;
        }
        let inv = Wide {
            a: s.d / det,
            b: -s.b / det,
            c: -s.c / det,
            d: s.a / det,
            e: (s.c * s.f - s.d * s.e) / det,
            f: (s.b * s.e - s.a * s.f) / det,
        }
        .narrow();
        inv.is_finite().then_some(inv)
    }

    /// Maps a point (translation applied).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "documented single rounding to f32"
    )]
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        let s = Wide::of(self);
        let (x, y) = (f64::from(p[0]), f64::from(p[1]));
        [
            (s.a * x + s.c * y + s.e) as f32,
            (s.b * x + s.d * y + s.f) as f32,
        ]
    }

    /// Maps a vector (translation ignored).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "documented single rounding to f32"
    )]
    pub fn apply_vector(&self, v: [f32; 2]) -> [f32; 2] {
        let s = Wide::of(self);
        let (x, y) = (f64::from(v[0]), f64::from(v[1]));
        [(s.a * x + s.c * y) as f32, (s.b * x + s.d * y) as f32]
    }

    /// Determinant `a·d − b·c` (computed in `f64`).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "documented single rounding to f32"
    )]
    pub fn det(&self) -> f32 {
        Wide::of(self).det() as f32
    }

    /// Width factor for ink strokes under this transform: `sqrt(|det|)` (area-preserving average scale).
    /// Shapes do not use it: their brush width is applied unscaled after the transform.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "documented single rounding to f32"
    )]
    pub fn width_scale(&self) -> f32 {
        Wide::of(self).det().abs().sqrt() as f32
    }

    /// True when the linear part is a similarity (rotation × uniform scale, optionally mirrored): columns
    /// orthogonal and of equal length within a relative `1e-5`. Rounded-rect corners are built in page space
    /// only for similarities (editing-algorithms.md §5).
    #[must_use]
    pub fn is_similarity(&self) -> bool {
        if !self.is_finite() {
            return false;
        }
        let s = Wide::of(self);
        let len_x = (s.a * s.a + s.b * s.b).sqrt();
        let len_y = (s.c * s.c + s.d * s.d).sqrt();
        let longest = if len_x > len_y { len_x } else { len_y };
        if longest <= 0.0 {
            return false;
        }
        let dot = s.a * s.c + s.b * s.d;
        (len_x - len_y).abs() <= 1e-5 * longest && dot.abs() <= 1e-5 * len_x * len_y
    }

    /// True when every component is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.e, self.f]
            .iter()
            .all(|v| v.is_finite())
    }

    /// Bitwise equality of all six components (`to_bits`), the comparison used by undo staleness checks.
    #[must_use]
    pub fn bit_eq(&self, other: &Affine) -> bool {
        let lhs = [self.a, self.b, self.c, self.d, self.e, self.f];
        let rhs = [other.a, other.b, other.c, other.d, other.e, other.f];
        lhs.iter()
            .zip(rhs.iter())
            .all(|(l, r)| l.to_bits() == r.to_bits())
    }

    /// Axis-aligned bounds of `b`'s four mapped corners (empty stays empty).
    #[must_use]
    pub fn map_bounds(&self, b: &Bounds) -> Bounds {
        if b.is_empty() {
            return Bounds::EMPTY;
        }
        let mut out = Bounds::EMPTY;
        for corner in [
            [b.min_x, b.min_y],
            [b.max_x, b.min_y],
            [b.min_x, b.max_y],
            [b.max_x, b.max_y],
        ] {
            let [x, y] = self.apply(corner);
            out.include_point(x, y);
        }
        out
    }
}

/// `f64` working copy of an [`Affine`] (composition happens here; one rounding back to `f32`).
#[derive(Clone, Copy)]
struct Wide {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Wide {
    fn of(t: &Affine) -> Self {
        Self {
            a: f64::from(t.a),
            b: f64::from(t.b),
            c: f64::from(t.c),
            d: f64::from(t.d),
            e: f64::from(t.e),
            f: f64::from(t.f),
        }
    }

    fn det(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    #[allow(
        clippy::cast_possible_truncation,
        reason = "documented single rounding to f32"
    )]
    fn narrow(self) -> Affine {
        Affine {
            a: self.a as f32,
            b: self.b as f32,
            c: self.c as f32,
            d: self.d as f32,
            e: self.e as f32,
            f: self.f as f32,
        }
    }
}

#[cfg(test)]
mod tests;
