//! Render data for one scene item as a flat `f32` array (what `SceneHandle` hands the tablet's renderer).
//!
//! Layout: `[multiply, partCount, part…]`, `multiply` is 1 when the item composites with the multiply blend.
//! Every part is `[kind, len, payload × len]`; draw the parts in order, then apply the `MASK` parts as
//! destination-out on the item alone (core `Scene::masks`). Colours are `r g b a` in `0..=255`.
//!
//! | kind | payload |
//! |---|---|
//! | 0 `OUTLINE` (nonzero fill) | `r g b a, opacity, nContours, contourEnds × n (points), points x y …` |
//! | 1 `STAMPS` | `r g b a, opacity, grainScale (0 = none), grainStrength, grainSoftness, nDabs, dabs × 9` (`x y rx ry angle opacity hardness grainStrength grainLevel`) |
//! | 2 `EVEN_ODD` (fill items) | as `OUTLINE`, but fill with the even-odd rule |
//! | 3 `MASK` | `width, nPoints, points x y …` (round caps/joins, destination-out) |
//! | 4 `TEXT` | `8 corner floats (tl tr br bl), r g b a, opacity, fontSize, boxW, boxH` (local box; the text itself via `item_text`) |
//! | 5 `IMAGE` | `8 corner floats, 32 asset-hash bytes as floats` |
//! | 6 `SHAPE_FILL` | as `OUTLINE`, but always drawn with the Normal blend, whatever `multiply` says (a shape's interior; core fills it Normal and only the outline strokes multiply) |
//!
//! `STAMPS` dabs are drawn exactly as core `raster::stamps` does (per-dab grain, minimum fade): the formulas live
//! in that module's docs and are mirrored by the tablet painter (`StampRaster.kt`) and the browser (`stamps.ts`),
//! which both replay `crates/penwise-core/tests/goldens/stamp_coverage.txt` generated from core.

use penwise_core::brush::{BlendMode, Color};
use penwise_core::geometry::{Outline, StrokeGeometry};
use penwise_core::item::{ItemGeometry, ItemId, ItemKind, Scene};

/// Content of live text item `id`; `None` for anything else.
#[must_use]
pub fn item_text(scene: &Scene, id: ItemId) -> Option<&str> {
    match &scene.get(id)?.kind {
        ItemKind::Text(t) => Some(&t.content),
        _ => None,
    }
}

/// Part kinds (see the module table).
pub mod kind {
    pub const OUTLINE: f32 = 0.0;
    pub const STAMPS: f32 = 1.0;
    pub const EVEN_ODD: f32 = 2.0;
    pub const MASK: f32 = 3.0;
    pub const TEXT: f32 = 4.0;
    pub const IMAGE: f32 = 5.0;
    pub const SHAPE_FILL: f32 = 6.0;
}

/// Floats per dab.
pub const DAB_FLOATS: usize = 9;

/// The array for live item `id`, `None` when the item is unknown, dead or an erase item (masks are parts of
/// their target).
#[must_use]
pub fn item_floats(scene: &Scene, id: ItemId) -> Option<Vec<f32>> {
    let item = scene.get(id)?;
    let geometry = scene.geometry(id)?;
    let mut parts: Vec<Vec<f32>> = Vec::new();
    let mut multiply = false;
    match geometry {
        ItemGeometry::Stroke { geometry, .. } => {
            multiply = geometry.style().blend == BlendMode::Multiply;
            parts.push(stroke_part(geometry));
        }
        ItemGeometry::Shape(shape) => {
            multiply = shape.stroke.style().blend == BlendMode::Multiply;
            if let Some((color, outline)) = &shape.fill {
                parts.push(outline_part(kind::SHAPE_FILL, *color, 1.0, outline));
            }
            parts.push(stroke_part(&shape.stroke));
        }
        ItemGeometry::Fill { polygons, paint } => {
            let rings: Vec<&Vec<[f32; 2]>> = polygons
                .iter()
                .flat_map(|p| std::iter::once(&p.outer).chain(p.holes.iter()))
                .collect();
            let mut outline = Outline::default();
            for ring in rings {
                outline.points.extend_from_slice(ring);
                outline
                    .contour_ends
                    .push(u32::try_from(outline.points.len()).unwrap_or(u32::MAX));
            }
            parts.push(outline_part(
                kind::EVEN_ODD,
                paint.color,
                paint.opacity,
                &outline,
            ));
        }
        ItemGeometry::Text { corners, paint } => {
            let mut p = corners.as_flattened().to_vec();
            p.extend(rgba(paint.color));
            p.push(paint.opacity);
            let (font_size, size) = match &item.kind {
                ItemKind::Text(t) => (t.font_size, t.size),
                _ => (0.0, [0.0; 2]),
            };
            p.push(font_size);
            p.extend(size);
            parts.push(part(kind::TEXT, p));
        }
        ItemGeometry::Image { corners } => {
            let mut p = corners.as_flattened().to_vec();
            if let ItemKind::Image(image) = &item.kind {
                p.extend(image.asset.iter().map(|&b| f32::from(b)));
            }
            parts.push(part(kind::IMAGE, p));
        }
        // A frame is a live view the tint layer places itself, not drawable ink; masks are parts of their target.
        ItemGeometry::Frame { .. } | ItemGeometry::Erase { .. } | ItemGeometry::Mark => {
            return None;
        }
    }
    for mask in scene.masks(id) {
        if let Some(ItemGeometry::Erase { path, radius }) = scene.geometry(mask)
            && !path.is_empty()
        {
            let mut p = vec![2.0 * radius, count(path.len())];
            p.extend_from_slice(path.as_flattened());
            parts.push(part(kind::MASK, p));
        }
    }
    let mut out = vec![f32::from(u8::from(multiply)), count(parts.len())];
    for p in parts {
        out.extend(p);
    }
    Some(out)
}

fn part(kind: f32, payload: Vec<f32>) -> Vec<f32> {
    let mut out = vec![kind, count(payload.len())];
    out.extend(payload);
    out
}

fn count(n: usize) -> f32 {
    // Counts are far below 2^24, exactly representable.
    #[allow(clippy::cast_precision_loss, reason = "counts below 2^24")]
    let v = n as f32;
    v
}

fn rgba(c: Color) -> [f32; 4] {
    [c.r, c.g, c.b, c.a].map(f32::from)
}

fn outline_part(kind: f32, color: Color, opacity: f32, outline: &Outline) -> Vec<f32> {
    let mut p = rgba(color).to_vec();
    p.push(opacity);
    p.push(count(outline.contour_ends.len()));
    p.extend(outline.contour_ends.iter().map(|&e| {
        #[allow(clippy::cast_precision_loss, reason = "point counts below 2^24")]
        let v = e as f32;
        v
    }));
    p.extend_from_slice(outline.points.as_flattened());
    part(kind, p)
}

fn stroke_part(geometry: &StrokeGeometry) -> Vec<f32> {
    match geometry {
        StrokeGeometry::Outline { style, outline } => {
            outline_part(kind::OUTLINE, style.color, style.opacity, outline)
        }
        StrokeGeometry::Stamps { style, grain, dabs } => {
            let mut p = rgba(style.color).to_vec();
            p.push(style.opacity);
            let (scale, strength, softness) =
                grain.map_or((0.0, 0.0, 0.0), |g| (g.scale, g.strength, g.softness));
            p.extend([scale, strength, softness, count(dabs.len())]);
            for d in dabs {
                p.extend([
                    d.x,
                    d.y,
                    d.radius_x,
                    d.radius_y,
                    d.angle,
                    d.opacity,
                    d.hardness,
                    d.grain_strength,
                    d.grain_level,
                ]);
            }
            part(kind::STAMPS, p)
        }
    }
}
