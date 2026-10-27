//! A whole [`Scene`] → pixels: every live item of the chosen layers in paint order, with precise-erase trims
//! (already in the stroke geometry) and masks applied.
//!
//! Strokes and shapes go through the same renderers as [`super::render_strokes`]; fills are filled polygons
//! (even-odd, so holes cut); images are drawn from an [`AssetSource`]; erase items cut their target
//! (destination-out on the target alone, so ink drawn later over the area stays). Text is not drawn: layout and
//! shaping belong to the host.

use std::collections::BTreeMap;

use ::tiny_skia::{
    FillRule, FilterQuality, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

use super::stamps::CoverageBuffer;
use super::{RasterError, Viewport, render_one};
use crate::bounds::Bounds;
use crate::brush::{BlendMode, Color};
use crate::geometry::{StrokeGeometry, StrokeStyle};
use crate::item::{
    Affine, Item, ItemGeometry, ItemId, ItemKind, LayerSet, Paint as FillPaint, Scene,
};

/// Most pixels a rendered region may have (2²⁸, as [`CoverageBuffer`]).
const MAX_PIXELS: f64 = 268_435_456.0;

/// Decoded pictures by asset hash ([`crate::item::ImageItem::asset`]).
pub trait AssetSource {
    /// The decoded picture, `None` when the asset is unknown (the item is then skipped).
    fn image(&self, asset: &[u8; 32]) -> Option<&Pixmap>;
}

impl AssetSource for () {
    fn image(&self, _asset: &[u8; 32]) -> Option<&Pixmap> {
        None
    }
}

impl AssetSource for BTreeMap<[u8; 32], Pixmap> {
    fn image(&self, asset: &[u8; 32]) -> Option<&Pixmap> {
        self.get(asset)
    }
}

/// Renders the live items of `layers` whose bounds meet `region` (page px) into a new transparent pixmap of
/// `ceil(region size · scale)` pixels, in [`Scene::render_order`], each item followed by its masks.
///
/// # Errors
/// [`RasterError::InvalidViewport`] for a non-finite or non-positive `scale` or a non-finite region origin;
/// [`RasterError::InvalidSize`] for an empty `region` or more than 2²⁸ pixels.
pub fn render_scene(
    scene: &Scene,
    layers: LayerSet,
    region: &Bounds,
    scale: f32,
    assets: &dyn AssetSource,
) -> Result<Pixmap, RasterError> {
    if !(scale.is_finite() && scale > 0.0) {
        return Err(RasterError::InvalidViewport);
    }
    let mut canvas = scene_pixmap(region, scale)?;
    render_scene_onto(&mut canvas, scene, layers, region, scale, assets)?;
    Ok(canvas)
}

/// [`render_scene`] onto an existing `canvas` (made by [`scene_pixmap`] for the same `region` and `scale`, then
/// filled with the paper): multiply strokes (highlighter) multiply over what is already there, exactly as the
/// tablet and browser paint them over their paper.
///
/// # Errors
/// [`RasterError::InvalidViewport`] as [`render_scene`].
pub fn render_scene_onto(
    canvas: &mut Pixmap,
    scene: &Scene,
    layers: LayerSet,
    region: &Bounds,
    scale: f32,
    assets: &dyn AssetSource,
) -> Result<(), RasterError> {
    let viewport = Viewport {
        origin_x: region.min_x,
        origin_y: region.min_y,
        scale,
    };
    if !(scale.is_finite() && scale > 0.0) {
        return Err(RasterError::InvalidViewport);
    }
    viewport.check()?;
    let mut scratch = None;
    let mut coverage = CoverageBuffer::default();
    let cull = region.inflate(2.0 / scale);
    for id in scene.render_order(layers) {
        let (Some(item), Some(geometry)) = (scene.get(id), scene.geometry(id)) else {
            continue;
        };
        if !scene.bounds(id).is_some_and(|b| b.intersects(&cull)) {
            continue;
        }
        let masks = page_masks(scene, id);
        let mut ctx = ItemPainter {
            viewport: &viewport,
            coverage: &mut coverage,
            assets,
        };
        if masks.is_empty() {
            ctx.draw(canvas, item, geometry)?;
        } else {
            let layer = scratch.get_or_insert_with(|| Pixmap::new(canvas.width(), canvas.height()));
            let Some(layer) = layer.as_mut() else {
                continue;
            };
            layer.fill(::tiny_skia::Color::TRANSPARENT);
            ctx.draw(layer, item, geometry)?;
            for (path, radius) in &masks {
                erase_capsules(layer, path, *radius, &viewport);
            }
            let blend = if item_blend(item) == BlendMode::Multiply {
                ::tiny_skia::BlendMode::Multiply
            } else {
                ::tiny_skia::BlendMode::SourceOver
            };
            let paint = PixmapPaint {
                blend_mode: blend,
                ..PixmapPaint::default()
            };
            canvas.draw_pixmap(0, 0, layer.as_ref(), &paint, Transform::identity(), None);
        }
    }
    Ok(())
}

/// Transparent pixmap covering `region` at `scale`: `ceil(region size · scale)` pixels per side.
///
/// # Errors
/// [`RasterError::InvalidSize`] for an empty `region` or more than 2²⁸ pixels.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "dimensions are checked finite, positive and ≤ 2²⁸ pixels before the cast"
)]
pub fn scene_pixmap(region: &Bounds, scale: f32) -> Result<Pixmap, RasterError> {
    let dim = |extent: f32| (f64::from(extent) * f64::from(scale)).ceil();
    let (w, h) = (dim(region.width()), dim(region.height()));
    let bad = RasterError::InvalidSize {
        width: w.clamp(0.0, f64::from(u32::MAX)) as u32,
        height: h.clamp(0.0, f64::from(u32::MAX)) as u32,
    };
    if region.is_empty() || !(w >= 1.0 && h >= 1.0) || w * h > MAX_PIXELS {
        return Err(bad);
    }
    Pixmap::new(w as u32, h as u32).ok_or(bad)
}

fn item_blend(item: &Item) -> BlendMode {
    match &item.kind {
        ItemKind::Stroke(s) => s.params.blend,
        ItemKind::Shape(s) => s.brush.blend,
        _ => BlendMode::Normal,
    }
}

/// `(page path, radius)` of `target`'s live non-empty masks, ascending mask id.
fn page_masks(scene: &Scene, target: ItemId) -> Vec<(Vec<[f32; 2]>, f32)> {
    scene
        .masks(target)
        .into_iter()
        .filter_map(|m| match scene.geometry(m) {
            Some(ItemGeometry::Erase { path, radius }) if !path.is_empty() => {
                Some((path.clone(), *radius))
            }
            _ => None,
        })
        .collect()
}

/// Destination-out of the capsule chain (circles of `radius` along `path`, joined).
fn erase_capsules(layer: &mut Pixmap, path: &[[f32; 2]], radius: f32, viewport: &Viewport) {
    let pts: Vec<[f32; 2]> = path.iter().map(|p| viewport.map(p[0], p[1])).collect();
    let r = radius * viewport.scale;
    let mut builder = PathBuilder::new();
    let [x0, y0] = pts[0];
    if pts.len() == 1 {
        builder.push_circle(x0, y0, r);
    } else {
        builder.move_to(x0, y0);
        for p in &pts[1..] {
            builder.line_to(p[0], p[1]);
        }
    }
    let Some(shape) = builder.finish() else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color(::tiny_skia::Color::BLACK);
    paint.blend_mode = ::tiny_skia::BlendMode::DestinationOut;
    paint.anti_alias = true;
    if pts.len() == 1 {
        layer.fill_path(
            &shape,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    } else {
        let stroke = Stroke {
            width: 2.0 * r,
            line_cap: ::tiny_skia::LineCap::Round,
            line_join: ::tiny_skia::LineJoin::Round,
            ..Stroke::default()
        };
        layer.stroke_path(&shape, &paint, &stroke, Transform::identity(), None);
    }
}

/// Per-render drawing state.
struct ItemPainter<'a> {
    viewport: &'a Viewport,
    coverage: &'a mut CoverageBuffer,
    assets: &'a dyn AssetSource,
}

impl ItemPainter<'_> {
    /// Draws one item's geometry onto `target`.
    fn draw(
        &mut self,
        target: &mut Pixmap,
        item: &Item,
        geometry: &ItemGeometry,
    ) -> Result<(), RasterError> {
        match geometry {
            ItemGeometry::Stroke { geometry, .. } => {
                render_one(geometry, self.viewport, target, self.coverage)
            }
            ItemGeometry::Shape(shape) => {
                if let Some((color, interior)) = &shape.fill {
                    let style = StrokeStyle {
                        color: *color,
                        opacity: 1.0,
                        blend: BlendMode::Normal,
                    };
                    let fill = StrokeGeometry::Outline {
                        style,
                        outline: interior.clone(),
                    };
                    render_one(&fill, self.viewport, target, self.coverage)?;
                }
                render_one(&shape.stroke, self.viewport, target, self.coverage)
            }
            ItemGeometry::Fill { polygons, paint } => {
                fill_polygons(target, polygons, *paint, self.viewport);
                Ok(())
            }
            ItemGeometry::Image { .. } => {
                self.draw_image(target, item);
                Ok(())
            }
            ItemGeometry::Erase { .. } | ItemGeometry::Text { .. } => Ok(()),
        }
    }

    fn draw_image(&self, target: &mut Pixmap, item: &Item) {
        let ItemKind::Image(image) = &item.kind else {
            return;
        };
        let Some(pixels) = self.assets.image(&image.asset) else {
            return;
        };
        let (iw, ih) = (pixels.width(), pixels.height());
        if iw == 0 || ih == 0 {
            return;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "picture dimensions are far below 2^24"
        )]
        let fit = Affine::scale(image.w / iw as f32, image.h / ih as f32);
        let s = self.viewport.scale;
        let to_pixels = Affine {
            a: s,
            b: 0.0,
            c: 0.0,
            d: s,
            e: -self.viewport.origin_x * s,
            f: -self.viewport.origin_y * s,
        };
        let m = fit.then(&item.transform).then(&to_pixels);
        let paint = PixmapPaint {
            quality: FilterQuality::Bilinear,
            ..PixmapPaint::default()
        };
        let none: Option<&Mask> = None;
        target.draw_pixmap(
            0,
            0,
            pixels.as_ref(),
            &paint,
            Transform::from_row(m.a, m.b, m.c, m.d, m.e, m.f),
            none,
        );
    }
}

/// Fills each polygon (outer ring plus holes, even-odd) with `paint`.
fn fill_polygons(
    target: &mut Pixmap,
    polygons: &[crate::item::Polygon],
    paint: FillPaint,
    viewport: &Viewport,
) {
    let alpha = crate::float::clamp01(
        f32::from(paint.color.a) / 255.0 * crate::float::clamp01(paint.opacity),
    );
    let Color { r, g, b, .. } = paint.color;
    let Some(color) = ::tiny_skia::Color::from_rgba(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        alpha,
    ) else {
        return;
    };
    let mut sk = Paint::default();
    sk.set_color(color);
    sk.anti_alias = true;
    for polygon in polygons {
        let mut builder = PathBuilder::new();
        for ring in std::iter::once(&polygon.outer).chain(&polygon.holes) {
            let Some(([x0, y0], rest)) = ring
                .split_first()
                .map(|(p, r)| (viewport.map(p[0], p[1]), r))
            else {
                continue;
            };
            builder.move_to(x0, y0);
            for p in rest {
                let [x, y] = viewport.map(p[0], p[1]);
                builder.line_to(x, y);
            }
            builder.close();
        }
        if let Some(path) = builder.finish() {
            target.fill_path(&path, &sk, FillRule::EvenOdd, Transform::identity(), None);
        }
    }
}
