//! Page items: the M2 scene model (PLAN "Data model" → Item) and the in-memory page [`Scene`].
//!
//! Owner: slice a (this file, `id.rs`, `layer.rs`, `affine.rs`, `paint.rs`, `text.rs`, `geometry.rs`,
//! `scene.rs`, `index.rs`); slice b owns `stroke.rs` and `erase.rs`; slice c owns `fill.rs`; slice d owns
//! `shape.rs`. Contract: `docs/contracts/m2-editing.md`.
//!
//! Every item kind lives in its own **local frame** mapped to page px by [`Item::transform`] (a 2×3 affine).
//! New items are created with [`Affine::IDENTITY`] and page-px data, so local = page until a transform edit.
//! Data is the source of truth; geometry ([`ItemGeometry`]) is always re-derived (PLAN "Data model").
mod affine;
mod erase;
mod fill;
mod geometry;
mod id;
mod index;
mod layer;
mod paint;
mod scene;
mod shape;
mod stroke;
mod text;

pub use affine::Affine;
pub use erase::EraseItem;
pub use fill::{FillItem, Polygon};
pub use geometry::{DeriveContext, ItemGeometry, RenderBand, RenderKey};
pub use id::{IdSource, ItemId};
pub use layer::{Layer, LayerSet};
pub use paint::Paint;
pub use scene::{PageKind, Scene};
pub use shape::{ArrowHead, ShapeItem, ShapeKind};
pub use stroke::{
    CONTEXT_SAMPLES, CentrePoint, Centreline, SampleParam, SampleRange, StrokeItem, Trim,
    intersect_ranges,
};
pub use text::TextItem;

/// Gap between consecutive z values handed out by [`Scene::next_z`], so a later "insert between" can pick a
/// midpoint without renumbering. z is a plain `i64` (not a fractional index): concurrent adds that pick the same
/// z are ordered by [`ItemId`] (ULID, so roughly by creation time), which is deterministic everywhere.
pub const Z_STEP: i64 = 1 << 16;

/// One page item. `id`, `kind` tag and immutable kind data never change after `Add`; `layer`, `z`,
/// `transform` and the mutable kind fields listed in [`crate::edit::Field`] change through `Update` edits
/// carrying absolute values (LWW per field in `penwise-proto`).
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub layer: Layer,
    /// Global paint/hit order key together with `id`: items sort by `(z, id)` ascending (bottom → top)
    /// within each [`RenderBand`].
    pub z: i64,
    /// Local → page. Stroke widths are applied **after** the transform (see [`StrokeItem::build_geometry`],
    /// [`crate::shape::shape_geometry`]). Ignored for [`ItemKind::Erase`] (masks use their target's transform);
    /// stored as identity there.
    pub transform: Affine,
    pub kind: ItemKind,
}

/// Kind-specific data (PLAN "Data model": stroke, erase, shape, fill, text; image is later).
#[derive(Debug, Clone, PartialEq)]
pub enum ItemKind {
    /// Pen stroke: brush params + seed + raw samples (+ visible-range [`Trim`] for precise-erase pieces).
    Stroke(StrokeItem),
    /// Precise-eraser mask attached to one stamp stroke or shape (destination-out over its target only).
    Erase(EraseItem),
    /// Rectangle, rounded rectangle, ellipse, line, arrow or polygon.
    Shape(ShapeItem),
    /// Fill-bucket region: polygons with holes.
    Fill(FillItem),
    /// Recognized or typed text.
    Text(TextItem),
}

/// Stable kind tags (frozen: stored in the item codec; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKindTag {
    Stroke,
    Erase,
    Shape,
    Fill,
    Text,
}

impl ItemKindTag {
    /// Byte tag: `Stroke = 0, Erase = 1, Shape = 2, Fill = 3, Text = 4`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Stroke => 0,
            Self::Erase => 1,
            Self::Shape => 2,
            Self::Fill => 3,
            Self::Text => 4,
        }
    }

    /// Inverse of [`ItemKindTag::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Stroke),
            1 => Some(Self::Erase),
            2 => Some(Self::Shape),
            3 => Some(Self::Fill),
            4 => Some(Self::Text),
            _ => None,
        }
    }
}

impl ItemKind {
    /// The kind's tag.
    #[must_use]
    pub fn tag(&self) -> ItemKindTag {
        match self {
            Self::Stroke(_) => ItemKindTag::Stroke,
            Self::Erase(_) => ItemKindTag::Erase,
            Self::Shape(_) => ItemKindTag::Shape,
            Self::Fill(_) => ItemKindTag::Fill,
            Self::Text(_) => ItemKindTag::Text,
        }
    }
}

impl Item {
    /// Checks the invariants [`Scene::apply`] relies on; never panics. Rules:
    /// - every float finite (transform, sizes, radii, polygon points, font size, paint opacity in `0..=1`);
    /// - stroke samples on the quantization grid (`s.quantize() == s`, bitwise) and at least one sample;
    ///   [`Trim`] only on outline-family strokes, `own < visible.len()`, ranges sorted, disjoint,
    ///   `start ≤ end`, and the own range inside the stored samples (`sample_offset ..` window);
    /// - erase path non-empty, radius `> 0`, transform identity;
    /// - shape brush of the outline family; `size` components `≥ 0`; `RoundedRect` radius `≥ 0`; `Polygon`
    ///   ≥ 3 points;
    /// - fill: ≥ 1 polygon, every ring ≥ 3 points;
    /// - text: `font_size > 0`, size components `≥ 0`.
    ///
    /// # Errors
    /// [`crate::edit::EditError::InvalidValue`] naming the first offending field (fields checked in the
    /// order above).
    pub fn validate(&self) -> Result<(), crate::edit::EditError> {
        let transform_ok = if matches!(self.kind, ItemKind::Erase(_)) {
            self.transform == Affine::IDENTITY
        } else {
            self.transform.is_finite()
        };
        check(transform_ok, "transform")?;
        match &self.kind {
            ItemKind::Stroke(s) => validate_stroke(s),
            ItemKind::Erase(e) => {
                check(!e.path.is_empty() && all_finite(&e.path), "path")?;
                check(e.radius.is_finite() && e.radius > 0.0, "radius")
            }
            ItemKind::Shape(s) => validate_shape(s),
            ItemKind::Fill(f) => {
                let ring_ok = |r: &Vec<[f32; 2]>| r.len() >= 3 && all_finite(r);
                let polygons_ok = !f.polygons.is_empty()
                    && f.polygons
                        .iter()
                        .all(|p| ring_ok(&p.outer) && p.holes.iter().all(ring_ok));
                check(polygons_ok, "polygons")?;
                check(paint_ok(f.paint), "paint")
            }
            ItemKind::Text(t) => {
                check(t.font_size.is_finite() && t.font_size > 0.0, "font_size")?;
                check(size_ok(t.size), "text_box")?;
                check(paint_ok(t.paint), "paint")
            }
        }
    }
}

fn check(ok: bool, field: &'static str) -> Result<(), crate::edit::EditError> {
    if ok {
        Ok(())
    } else {
        Err(crate::edit::EditError::InvalidValue { field })
    }
}

fn all_finite(points: &[[f32; 2]]) -> bool {
    points.iter().all(|p| p[0].is_finite() && p[1].is_finite())
}

fn size_ok(size: [f32; 2]) -> bool {
    size.iter().all(|v| v.is_finite() && *v >= 0.0)
}

fn paint_ok(paint: Paint) -> bool {
    (0.0..=1.0).contains(&paint.opacity)
}

fn brush_ok(brush: &crate::brush::BrushParams) -> bool {
    [brush.size_min, brush.size_max, brush.opacity]
        .iter()
        .all(|v| v.is_finite())
}

fn validate_stroke(s: &StrokeItem) -> Result<(), crate::edit::EditError> {
    check(brush_ok(&s.params), "brush")?;
    let on_grid = |x: &crate::sample::Sample| {
        let q = x.quantize();
        q.x.to_bits() == x.x.to_bits()
            && q.y.to_bits() == x.y.to_bits()
            && q.pressure.to_bits() == x.pressure.to_bits()
            && q.tilt.to_bits() == x.tilt.to_bits()
            && q.azimuth.to_bits() == x.azimuth.to_bits()
            && q.t_us == x.t_us
    };
    check(
        !s.samples.is_empty() && s.samples.iter().all(on_grid),
        "samples",
    )?;
    let Some(trim) = &s.trim else { return Ok(()) };
    let outline = matches!(s.params.family, crate::brush::BrushFamily::Outline(_));
    let ranges = &trim.visible;
    let ordered =
        ranges.iter().all(|r| r.start <= r.end) && ranges.windows(2).all(|w| w[0].end < w[1].start);
    let own_ok = usize::try_from(trim.own)
        .ok()
        .and_then(|i| ranges.get(i))
        .is_some_and(|own| {
            // Stored window in param space: [offset << 12, (offset + n − 1) << 12] (n ≥ 1 checked above).
            let n = u64::try_from(s.samples.len()).unwrap_or(u64::from(u32::MAX));
            let first = u64::from(trim.sample_offset) << SampleParam::FRAC_BITS;
            let last =
                (u64::from(trim.sample_offset) + n - 1).saturating_mul(1 << SampleParam::FRAC_BITS);
            u64::from(own.start.0) >= first && u64::from(own.end.0) <= last
        });
    check(outline && ordered && own_ok, "trim")
}

fn validate_shape(s: &ShapeItem) -> Result<(), crate::edit::EditError> {
    let outline = matches!(s.brush.family, crate::brush::BrushFamily::Outline(_));
    check(outline && brush_ok(&s.brush), "brush")?;
    check(size_ok(s.size), "shape_size")?;
    let kind_ok = match &s.kind {
        ShapeKind::RoundedRect { radius } => radius.is_finite() && *radius >= 0.0,
        ShapeKind::Polygon { points } => points.len() >= 3 && all_finite(points),
        ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::Line | ShapeKind::Arrow { .. } => true,
    };
    check(kind_ok, "shape_kind")
}

#[cfg(test)]
mod tests;
