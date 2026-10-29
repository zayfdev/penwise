//! Item invariants ([`Item::validate`]): everything [`crate::item::Scene::apply`] and the geometry derivers
//! rely on, including the extent caps that keep hostile remote values out of the spatial index.
//!
//! Owner: slice a.

use super::{
    Affine, Item, ItemKind, MAX_STROKE_SAMPLES, Paint, SampleParam, ShapeItem, ShapeKind,
    StrokeItem,
};
use crate::bounds::Bounds;
use crate::edit::EditError;
use crate::sample::POSITION_LIMIT_PX;
use crate::smoothing::SmootherState;

/// Most vertices a [`ShapeKind::Polygon`] may have (regular n-gons use ≤ 64; the cap bounds outline work
/// for decoded remote shapes).
pub const MAX_POLYGON_POINTS: usize = 1024;

impl Item {
    /// Checks the invariants [`crate::item::Scene::apply`] relies on; never panics. Rules:
    /// - every float finite (transform, sizes, radii, polygon points, font size, paint opacity in `0..=1`);
    /// - transform: translation and linear coefficients within `±2²⁰` ([`POSITION_LIMIT_PX`]);
    /// - stroke samples on the quantization grid (`s.quantize() == s`, bitwise), at least one sample, and the
    ///   stored window `sample_offset + len ≤` [`MAX_STROKE_SAMPLES`]; [`super::Trim`] only on outline-family
    ///   strokes, `own < visible.len()`, ranges sorted, disjoint, `start ≤ end`, and the own range inside
    ///   the stored samples (`sample_offset ..` window);
    /// - erase path non-empty, radius `> 0`, transform identity;
    /// - shape brush of the outline family; `size` components in `0 ..= 2²⁰`; `RoundedRect` radius `≥ 0`;
    ///   `Polygon` `3 ..=` [`MAX_POLYGON_POINTS`] points;
    /// - fill: ≥ 1 polygon, every ring ≥ 3 points;
    /// - text: `font_size` in `(0, 2²⁰]`, size components in `0 ..= 2²⁰`;
    /// - brushes: [`crate::BrushParams::is_valid`] (every float finite and in range, `opacity ∈ 0..=1`,
    ///   `0 < size_min ≤ size_max`); erase radius `≤ 2²⁰`;
    /// - trim warm state: [`crate::smoothing::SmootherState::is_valid`];
    /// - extent: the local data box and its page image under the transform lie within `±2²⁰` px (the
    ///   codec's position limit), and a stroke's page half-width (`size_max ×`
    ///   [`Affine::width_scale`]) is at most `2²⁰`.
    /// - image: `w`, `h` in `(0, 2²⁰]`;
    /// - frame: `project` non-empty and ≤ 64 bytes, `route` starts with `/` and is ≤ 2048 bytes, `css_w`/
    ///   `css_h` in `1 ..= 2²⁰`.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming the first offending field (fields checked in the order above;
    /// `"extent"` last).
    pub fn validate(&self) -> Result<(), EditError> {
        let transform_ok = if matches!(self.kind, ItemKind::Erase(_) | ItemKind::Mark(_)) {
            self.transform == Affine::IDENTITY
        } else {
            transform_ok(&self.transform)
        };
        check(transform_ok, "transform")?;
        match &self.kind {
            ItemKind::Stroke(s) => validate_stroke(s),
            ItemKind::Erase(e) => {
                check(!e.path.is_empty() && all_finite(&e.path), "path")?;
                check(
                    e.radius.is_finite() && e.radius > 0.0 && e.radius <= POSITION_LIMIT_PX,
                    "radius",
                )
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
            ItemKind::Image(i) => check(
                [i.w, i.h]
                    .iter()
                    .all(|v| *v > 0.0 && *v <= POSITION_LIMIT_PX),
                "image_size",
            ),
            ItemKind::Frame(f) => f.check(),
            ItemKind::Mark(m) => m.check(),
            ItemKind::Text(t) => {
                let font_ok = t.font_size.is_finite() && t.font_size > 0.0;
                check(font_ok && t.font_size <= POSITION_LIMIT_PX, "font_size")?;
                check(size_ok(t.size), "text_box")?;
                check(paint_ok(t.paint), "paint")
            }
        }?;
        check(self.extent_ok(), "extent")
    }

    /// Local data box and its page image within `±2²⁰` px; stroke page half-width within `2²⁰`.
    fn extent_ok(&self) -> bool {
        let local = local_box(&self.kind);
        if !within_limit(&local) || !within_limit(&self.transform.map_bounds(&local)) {
            return false;
        }
        let ItemKind::Stroke(s) = &self.kind else {
            return true;
        };
        let width = f64::from(self.transform.width_scale()) * f64::from(s.params.size_max.abs());
        width <= f64::from(POSITION_LIMIT_PX)
    }
}

/// Box of the item's local data (before stroke widths): samples, erase path ± radius, shape box (polygon
/// vertices scaled by `size`), fill outer rings, text box.
fn local_box(kind: &ItemKind) -> Bounds {
    let mut b = Bounds::EMPTY;
    match kind {
        ItemKind::Stroke(s) => s.samples.iter().for_each(|p| b.include_point(p.x, p.y)),
        ItemKind::Erase(e) => {
            for p in &e.path {
                b.include_point(p[0] - e.radius, p[1] - e.radius);
                b.include_point(p[0] + e.radius, p[1] + e.radius);
            }
        }
        ItemKind::Shape(s) => {
            let [hw, hh] = [s.size[0] * 0.5, s.size[1] * 0.5];
            b.include_point(-hw, -hh);
            b.include_point(hw, hh);
            if let ShapeKind::Polygon { points } = &s.kind {
                for p in points {
                    b.include_point(p[0] * s.size[0], p[1] * s.size[1]);
                }
            }
        }
        ItemKind::Fill(f) => f.polygons.iter().for_each(|p| b = b.union(&p.bounds())),
        ItemKind::Text(t) => {
            b.include_point(0.0, 0.0);
            b.include_point(t.size[0], t.size[1]);
        }
        ItemKind::Image(i) => {
            b.include_point(0.0, 0.0);
            b.include_point(i.w, i.h);
        }
        ItemKind::Frame(f) => {
            let [w, h] = f.page_size();
            b.include_point(0.0, 0.0);
            b.include_point(w, h);
        }
        // Marks carry no ink of their own.
        ItemKind::Mark(_) => {}
    }
    b
}

/// Every coordinate finite and within `±2²⁰` px (an empty box passes: it has no extent).
fn within_limit(b: &Bounds) -> bool {
    b.is_empty()
        || [b.min_x, b.min_y, b.max_x, b.max_y]
            .iter()
            .all(|v| v.is_finite() && v.abs() <= POSITION_LIMIT_PX)
}

fn transform_ok(t: &Affine) -> bool {
    t.is_finite()
        && [t.a, t.b, t.c, t.d, t.e, t.f]
            .iter()
            .all(|v| v.abs() <= POSITION_LIMIT_PX)
}

fn check(ok: bool, field: &'static str) -> Result<(), EditError> {
    if ok {
        Ok(())
    } else {
        Err(EditError::InvalidValue { field })
    }
}

fn all_finite(points: &[[f32; 2]]) -> bool {
    points.iter().all(|p| p[0].is_finite() && p[1].is_finite())
}

fn size_ok(size: [f32; 2]) -> bool {
    size.iter()
        .all(|v| v.is_finite() && *v >= 0.0 && *v <= POSITION_LIMIT_PX)
}

fn paint_ok(paint: Paint) -> bool {
    (0.0..=1.0).contains(&paint.opacity)
}

fn validate_stroke(s: &StrokeItem) -> Result<(), EditError> {
    check(s.params.is_valid(), "brush")?;
    if let Some(binding) = &s.binding {
        binding.check()?;
    }
    let on_grid = |x: &crate::sample::Sample| {
        let q = x.quantize();
        q.x.to_bits() == x.x.to_bits()
            && q.y.to_bits() == x.y.to_bits()
            && q.pressure.to_bits() == x.pressure.to_bits()
            && q.tilt.to_bits() == x.tilt.to_bits()
            && q.azimuth.to_bits() == x.azimuth.to_bits()
            && q.t_us == x.t_us
    };
    let n = u64::try_from(s.samples.len()).unwrap_or(u64::MAX);
    let window_ok = u64::from(s.sample_offset()).saturating_add(n) <= u64::from(MAX_STROKE_SAMPLES);
    check(
        n > 0 && window_ok && s.samples.iter().all(on_grid),
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
            let first = u64::from(trim.sample_offset) << SampleParam::FRAC_BITS;
            let last =
                (u64::from(trim.sample_offset) + n - 1).saturating_mul(1 << SampleParam::FRAC_BITS);
            u64::from(own.start.0) >= first && u64::from(own.end.0) <= last
        });
    let warm_ok = trim.warm.as_ref().is_none_or(SmootherState::is_valid);
    check(outline && ordered && own_ok && warm_ok, "trim")
}

fn validate_shape(s: &ShapeItem) -> Result<(), EditError> {
    let outline = matches!(s.brush.family, crate::brush::BrushFamily::Outline(_));
    check(outline && s.brush.is_valid(), "brush")?;
    check(size_ok(s.size), "shape_size")?;
    let kind_ok = match &s.kind {
        ShapeKind::RoundedRect { radius } => radius.is_finite() && *radius >= 0.0,
        ShapeKind::Polygon { points } => {
            (3..=MAX_POLYGON_POINTS).contains(&points.len()) && all_finite(points)
        }
        ShapeKind::Rect | ShapeKind::Ellipse | ShapeKind::Line | ShapeKind::Arrow { .. } => true,
    };
    check(kind_ok, "shape_kind")
}
