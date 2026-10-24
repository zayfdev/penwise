//! Stroke geometry: the output every surface renders and every golden hashes.
//!
//! Owner: contract.
//!
//! All coordinates are page px (y down), `f32`. Geometry must be bit-identical on native and wasm; the
//! canonical byte form below is what golden tests hash ([`crate::hash::fnv1a64`]).

use crate::bounds::Bounds;
use crate::brush::{BlendMode, Color, GrainParams};
use crate::float::{canon, max};

/// Closed polygon(s), filled with the **nonzero** rule. Contour `k` is
/// `points[contour_ends[k-1] .. contour_ends[k]]` (first starts at 0); each contour is implicitly closed.
/// Perfect-freehand outlines and nib ribbons are a single contour (`contour_ends == [points.len()]`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outline {
    pub points: Vec<[f32; 2]>,
    /// Exclusive end index of each contour, strictly increasing, last == `points.len()`.
    pub contour_ends: Vec<u32>,
}

/// One stamp of a stamp brush, in stroke order. Rasterizers draw an elliptical `MyPaint` falloff mask.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dab {
    /// Center, page px.
    pub x: f32,
    pub y: f32,
    /// Semi-axis along `angle`, px (≥ `radius_y`).
    pub radius_x: f32,
    /// Semi-axis perpendicular to `angle`, px (`> 0`).
    pub radius_y: f32,
    /// Rotation of the `radius_x` axis, radians, page axes.
    pub angle: f32,
    /// Per-dab alpha `0..=1` (spacing-normalized flow × jitter × tilt × velocity factors).
    pub opacity: f32,
    /// Falloff hardness `(0, 1]` (§3.4); renderers apply the 1 px minimum fade.
    pub hardness: f32,
    /// Grain modulation strength `0..=1` (0 = untextured).
    pub grain_strength: f32,
    /// Grain fill level `lerp(0.15, 1, P)` (§3.7): how deep the dab reaches into paper valleys.
    pub grain_level: f32,
}

/// Compositing info shared by both families.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    /// Straight `sRGB` color.
    pub color: Color,
    /// Stroke-level opacity cap `0..=1`, applied once per stroke.
    pub opacity: f32,
    /// [`BlendMode::Multiply`] strokes render in the under-ink band (before every normal stroke of the layer).
    pub blend: BlendMode,
}

/// Finished geometry of one stroke.
#[derive(Debug, Clone, PartialEq)]
pub enum StrokeGeometry {
    Outline {
        style: StrokeStyle,
        outline: Outline,
    },
    /// `grain` is the brush's canvas-space paper grain (rasterizers sample `stamp::texture`).
    Stamps {
        style: StrokeStyle,
        grain: Option<GrainParams>,
        dabs: Vec<Dab>,
    },
}

/// Live outline: left/right point prefixes that will not change, plus a tentative tail recomputed per push.
/// Polygon = `left ++ end_cap ++ reverse(right) ++ start_cap` with `left = stable_left ++ tentative_left`
/// and `right = stable_right ++ tentative_right` (see [`OutlineView::write_polygon`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineView<'a> {
    pub stable_left: &'a [[f32; 2]],
    pub stable_right: &'a [[f32; 2]],
    pub tentative_left: &'a [[f32; 2]],
    pub tentative_right: &'a [[f32; 2]],
    pub start_cap: &'a [[f32; 2]],
    pub end_cap: &'a [[f32; 2]],
}

/// Live view of a stroke in progress (ink-rendering.md §4.3). Borrowed from the builder; valid until the
/// next `push`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeometryView<'a> {
    Outline {
        style: StrokeStyle,
        outline: OutlineView<'a>,
    },
    /// `stable` dabs never change and may be cached (e.g. in a bitmap); `tentative` dabs are replaced on every
    /// push and on finish.
    Stamps {
        style: StrokeStyle,
        grain: Option<GrainParams>,
        stable: &'a [Dab],
        tentative: &'a [Dab],
    },
}

/// A NaN or infinity reached the canonical-bytes boundary (a bug in the engine that produced it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("non-finite float #{index} in stroke geometry")]
pub struct NonFiniteError {
    /// Zero-based index of the offending float in canonical output order.
    pub index: u32,
}

impl OutlineView<'_> {
    /// Clears `out` and writes the live polygon (single contour, nonzero fill).
    pub fn write_polygon(&self, out: &mut Vec<[f32; 2]>) {
        out.clear();
        out.extend_from_slice(self.stable_left);
        out.extend_from_slice(self.tentative_left);
        out.extend_from_slice(self.end_cap);
        out.extend(self.tentative_right.iter().rev());
        out.extend(self.stable_right.iter().rev());
        out.extend_from_slice(self.start_cap);
    }
}

impl Outline {
    /// Bounding box of all points (empty outline → [`Bounds::EMPTY`]).
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::EMPTY;
        for p in &self.points {
            b.include_point(p[0], p[1]);
        }
        b
    }
}

impl StrokeGeometry {
    /// Compositing info.
    #[must_use]
    pub fn style(&self) -> &StrokeStyle {
        match self {
            Self::Outline { style, .. } | Self::Stamps { style, .. } => style,
        }
    }

    /// Conservative bounds: outline points; for dabs, each center ± `max(radius_x, radius_y)`.
    #[must_use]
    pub fn bounds(&self) -> Bounds {
        match self {
            Self::Outline { outline, .. } => outline.bounds(),
            Self::Stamps { dabs, .. } => {
                let mut b = Bounds::EMPTY;
                for d in dabs {
                    let r = max(d.radius_x, d.radius_y);
                    b.include_point(d.x - r, d.y - r);
                    b.include_point(d.x + r, d.y + r);
                }
                b
            }
        }
    }

    /// Canonical little-endian bytes for golden hashing, layout v1:
    /// - `u8` tag (0 outline, 1 stamps); style: `r g b a` u8, `opacity` f32, `blend.code()` u8.
    /// - Outline: `u32` contour count, each contour end `u32`, `u32` point count, then `x, y` f32 per point.
    /// - Stamps: `u8` grain flag (0/1) then `scale, strength, softness` f32 if 1; `u32` dab count; per dab
    ///   `x, y, radius_x, radius_y, angle, opacity, hardness, grain_strength, grain_level` f32.
    ///
    /// Every f32 goes through [`canon`] (−0 → +0) and is written as `to_bits().to_le_bytes()`.
    ///
    /// # Errors
    /// [`NonFiniteError`] at the first NaN/±inf, by float index in this output order.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, NonFiniteError> {
        let mut w = CanonicalWriter {
            out: Vec::new(),
            written: 0,
        };
        match self {
            Self::Outline { style, outline } => {
                w.out.push(0);
                w.style(style)?;
                w.count(outline.contour_ends.len());
                for &end in &outline.contour_ends {
                    w.out.extend_from_slice(&end.to_le_bytes());
                }
                w.count(outline.points.len());
                for p in &outline.points {
                    w.floats(p)?;
                }
            }
            Self::Stamps { style, grain, dabs } => {
                w.out.push(1);
                w.style(style)?;
                match grain {
                    None => w.out.push(0),
                    Some(g) => {
                        w.out.push(1);
                        w.floats(&[g.scale, g.strength, g.softness])?;
                    }
                }
                w.count(dabs.len());
                for d in dabs {
                    w.floats(&[
                        d.x,
                        d.y,
                        d.radius_x,
                        d.radius_y,
                        d.angle,
                        d.opacity,
                        d.hardness,
                        d.grain_strength,
                        d.grain_level,
                    ])?;
                }
            }
        }
        Ok(w.out)
    }
}

struct CanonicalWriter {
    out: Vec<u8>,
    written: u32,
}

impl CanonicalWriter {
    fn floats(&mut self, values: &[f32]) -> Result<(), NonFiniteError> {
        for &v in values {
            let c = canon(v).ok_or(NonFiniteError {
                index: self.written,
            })?;
            self.out.extend_from_slice(&c.to_bits().to_le_bytes());
            self.written = self.written.wrapping_add(1);
        }
        Ok(())
    }

    /// Lengths are written as `u32` (never `usize`); geometry never approaches 2³² elements (dab cap 200 k).
    #[allow(
        clippy::cast_possible_truncation,
        reason = "element counts are capped far below u32::MAX"
    )]
    fn count(&mut self, n: usize) {
        self.out.extend_from_slice(&(n as u32).to_le_bytes());
    }

    fn style(&mut self, s: &StrokeStyle) -> Result<(), NonFiniteError> {
        self.out
            .extend_from_slice(&[s.color.r, s.color.g, s.color.b, s.color.a]);
        self.floats(&[s.opacity])?;
        self.out.push(s.blend.code());
        Ok(())
    }
}
