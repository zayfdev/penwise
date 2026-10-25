//! Page → PNG through the ink engine, so agents see exactly what the tablet drew.

use penwise_core::brush::{Color, Preset};
use penwise_core::builder::StrokeBuilder;
use penwise_core::geometry::StrokeGeometry;
use penwise_core::raster::png::encode_png;
use penwise_core::raster::{CoverageBuffer, Viewport, render_strokes};
use penwise_core::sample::Sample;
use tiny_skia::Pixmap;

use crate::library::{BACKGROUND_SPACING_PX, Background, PageKind, PageMeta, StrokeMsg};

/// Smallest infinite-page render, in page px.
const MIN_INFINITE_WIDTH: f32 = 1200.0;
const MIN_INFINITE_HEIGHT: f32 = 900.0;
/// Breathing room around an infinite page's content, in page px.
const INFINITE_PADDING: f32 = 200.0;
/// Largest side of any rendered PNG, in pixels.
const MAX_SIDE_PX: f32 = 8000.0;
/// Largest side of an ink crop handed to a vision model: bigger costs seconds per pixel, not clarity.
pub const OCR_MAX_SIDE: f32 = 1400.0;
const RULED_COLOR: (u8, u8, u8) = (0xC9, 0xD6, 0xE8);
const GRID_COLOR: (u8, u8, u8) = (0xDC, 0xE3, 0xEC);
const DOT_COLOR: (u8, u8, u8) = (0xC6, 0xCE, 0xDA);
const SAMPLE_STRIDE: usize = 5;
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 4.0;

/// Renders one page at `scale` pixels per page px, background first.
pub fn page_png(strokes: &[StrokeMsg], scale: f32, page: &PageMeta) -> Result<Vec<u8>, String> {
    let mut scale = if scale.is_finite() && scale > 0.0 {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        1.0
    };
    let (origin_x, origin_y, page_width, page_height) = extent(strokes, page.kind);
    let longest = page_width.max(page_height);
    if longest * scale > MAX_SIDE_PX {
        scale = MAX_SIDE_PX / longest;
    }
    let width = pixel_span(page_width, scale);
    let height = pixel_span(page_height, scale);
    let mut pixmap = Pixmap::new(width, height).ok_or("page size out of range")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    draw_background(&mut pixmap, page.background, scale);

    let geometries: Vec<StrokeGeometry> = strokes.iter().map(geometry_of).collect();
    let mut scratch = CoverageBuffer::default();
    render_strokes(
        &geometries,
        &Viewport {
            origin_x,
            origin_y,
            scale,
        },
        &mut pixmap,
        &mut scratch,
    )
    .map_err(|error| error.to_string())?;
    encode_png(&pixmap).map_err(|error| error.to_string())
}

/// Page origin and size in page px: fixed pages start at 0,0; infinite ones wrap their content.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "sample coordinates are finite page px; the bounds are only used to size a buffer"
)]
fn extent(strokes: &[StrokeMsg], kind: PageKind) -> (f32, f32, f32, f32) {
    match kind {
        PageKind::Fixed { width, height } => (0.0, 0.0, width, height),
        PageKind::Infinite => {
            let mut bounds: Option<(f32, f32, f32, f32)> = None;
            for stroke in strokes {
                for pair in stroke.samples.chunks_exact(SAMPLE_STRIDE) {
                    let (x, y) = (pair[0], pair[1]);
                    if !x.is_finite() || !y.is_finite() {
                        continue;
                    }
                    bounds = Some(match bounds {
                        None => (x, y, x, y),
                        Some((min_x, min_y, max_x, max_y)) => {
                            (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                        }
                    });
                }
            }
            let (min_x, min_y, max_x, max_y) = bounds.unwrap_or((0.0, 0.0, 0.0, 0.0));
            let origin_x = min_x - INFINITE_PADDING;
            let origin_y = min_y - INFINITE_PADDING;
            let width = (max_x - origin_x + INFINITE_PADDING).max(MIN_INFINITE_WIDTH);
            let height = (max_y - origin_y + INFINITE_PADDING).max(MIN_INFINITE_HEIGHT);
            (origin_x, origin_y, width, height)
        }
    }
}

/// Just the ink: the strokes' bounds, padded, at up to [`OCR_MAX_SIDE`] pixels per side.
///
/// Handwriting fills a corner of a page at most, and a full page render is both slower to run through a vision
/// model and worse to read. Returns `None` for a page with no ink.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "bounds are finite page px and the result is a pixel count"
)]
pub fn ink_png(strokes: &[StrokeMsg], scale: f32, padding: f32) -> Result<Option<Vec<u8>>, String> {
    let Some((min_x, min_y, max_x, max_y)) = ink_bounds(strokes) else {
        return Ok(None);
    };
    let width = (max_x - min_x + padding * 2.0).max(1.0);
    let height = (max_y - min_y + padding * 2.0).max(1.0);
    let longest = width.max(height);
    let mut scale = if scale.is_finite() && scale > 0.0 {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        1.0
    };
    if longest * scale > OCR_MAX_SIDE {
        scale = OCR_MAX_SIDE / longest;
    }
    let origin_x = min_x - padding;
    let origin_y = min_y - padding;
    let mut pixmap = Pixmap::new(pixel_span(width, scale), pixel_span(height, scale))
        .ok_or("page size out of range")?;
    pixmap.fill(tiny_skia::Color::WHITE);
    let geometries: Vec<StrokeGeometry> = strokes.iter().map(geometry_of).collect();
    let mut scratch = CoverageBuffer::default();
    render_strokes(
        &geometries,
        &Viewport {
            origin_x,
            origin_y,
            scale,
        },
        &mut pixmap,
        &mut scratch,
    )
    .map_err(|error| error.to_string())?;
    Ok(Some(
        encode_png(&pixmap).map_err(|error| error.to_string())?,
    ))
}

/// Bounding box of every finite sample, or `None` when nothing is drawable.
fn ink_bounds(strokes: &[StrokeMsg]) -> Option<(f32, f32, f32, f32)> {
    let mut bounds: Option<(f32, f32, f32, f32)> = None;
    for stroke in strokes {
        for pair in stroke.samples.chunks_exact(SAMPLE_STRIDE) {
            let (x, y) = (pair[0], pair[1]);
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
            });
        }
    }
    bounds
}

/// Horizontal rules, a square grid or a dot grid under the strokes, at `scale`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "pixel positions come from a clamped scale and a positive canvas size"
)]
fn draw_background(pixmap: &mut Pixmap, background: Background, scale: f32) {
    let spacing = BACKGROUND_SPACING_PX * scale;
    if background == Background::Blank || spacing < 4.0 {
        return;
    }
    let width = pixmap.width() as f32;
    let height = pixmap.height() as f32;
    let paint = |rgb: (u8, u8, u8)| {
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::from_rgba8(rgb.0, rgb.1, rgb.2, 0xFF));
        paint.anti_alias = true;
        paint
    };
    match background {
        Background::Ruled => {
            let paint = paint(RULED_COLOR);
            let mut y = spacing;
            while y < height {
                if let Some(line) = tiny_skia::Rect::from_xywh(0.0, y, width, 1.0) {
                    pixmap.fill_rect(line, &paint, tiny_skia::Transform::identity(), None);
                }
                y += spacing;
            }
        }
        Background::Grid => {
            let paint = paint(GRID_COLOR);
            let mut x = spacing;
            while x < width {
                if let Some(line) = tiny_skia::Rect::from_xywh(x, 0.0, 1.0, height) {
                    pixmap.fill_rect(line, &paint, tiny_skia::Transform::identity(), None);
                }
                x += spacing;
            }
            let mut y = spacing;
            while y < height {
                if let Some(line) = tiny_skia::Rect::from_xywh(0.0, y, width, 1.0) {
                    pixmap.fill_rect(line, &paint, tiny_skia::Transform::identity(), None);
                }
                y += spacing;
            }
        }
        Background::Dots => {
            let paint = paint(DOT_COLOR);
            let mut y = spacing;
            while y < height {
                let mut x = spacing;
                while x < width {
                    if let Some(dot) = tiny_skia::Rect::from_xywh(x - 1.5, y - 1.5, 3.0, 3.0) {
                        pixmap.fill_rect(dot, &paint, tiny_skia::Transform::identity(), None);
                    }
                    x += spacing;
                }
                y += spacing;
            }
        }
        Background::Blank => {}
    }
}

/// Page px to pixel count: rounded, at least 1, and finite because `scale` is clamped.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to (0, 4] times a page dimension, so the value is a small positive integer"
)]
fn pixel_span(page: f32, scale: f32) -> u32 {
    (page * scale).round().max(1.0) as u32
}

/// Rebuilds one stored stroke; its stored colour replaces the preset's, opacity and blend stay the brush's.
fn geometry_of(stroke: &StrokeMsg) -> StrokeGeometry {
    let params = params_for(stroke);
    let seed = stroke.seed.parse::<u64>().unwrap_or(0);
    let mut builder = StrokeBuilder::new(&params, seed);
    let count = (stroke.samples.len() / SAMPLE_STRIDE).min(stroke.times.len());
    let mut batch = Vec::with_capacity(count);
    for (i, time) in stroke.times.iter().take(count).enumerate() {
        let o = i * SAMPLE_STRIDE;
        batch.push(Sample {
            x: stroke.samples[o],
            y: stroke.samples[o + 1],
            pressure: stroke.samples[o + 2],
            tilt: stroke.samples[o + 3],
            azimuth: stroke.samples[o + 4],
            t_us: u32::try_from((*time).max(0)).unwrap_or(0),
        });
    }
    builder.push(&batch);
    let mut geometry = builder.finish();
    set_color(&mut geometry, stroke.argb);
    geometry
}

fn params_for(stroke: &StrokeMsg) -> penwise_core::brush::BrushParams {
    let index = (stroke.preset as usize).min(Preset::ALL.len().saturating_sub(1));
    let mut params = Preset::ALL[index].params();
    if stroke.scale.is_finite() && stroke.scale > 0.0 {
        params.size_min *= stroke.scale;
        params.size_max *= stroke.scale;
    }
    params
}

#[allow(
    clippy::cast_sign_loss,
    reason = "the colour word is manipulated as u32; the sign bit is the alpha channel"
)]
fn set_color(geometry: &mut StrokeGeometry, argb: i32) {
    let bits = argb as u32;
    let color = Color {
        r: byte(bits >> 16),
        g: byte(bits >> 8),
        b: byte(bits),
        a: byte(bits >> 24),
    };
    match geometry {
        StrokeGeometry::Outline { style, .. } | StrokeGeometry::Stamps { style, .. } => {
            style.color = color;
        }
    }
}

/// One colour channel from a shifted ARGB word.
#[allow(
    clippy::cast_possible_truncation,
    reason = "masked to 8 bits before the cast"
)]
fn byte(shifted: u32) -> u8 {
    (shifted & 0xFF) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(x: f32, y: f32) -> StrokeMsg {
        StrokeMsg {
            layer: "ink".to_owned(),
            preset: 0,
            argb: -16_777_216,
            scale: 1.0,
            seed: "1".to_owned(),
            samples: vec![x, y, 0.5, 0.0, 0.0, x + 60.0, y + 20.0, 0.5, 0.0, 0.0],
            times: vec![0, 8_000],
        }
    }

    #[test]
    fn an_ink_crop_is_smaller_than_the_page_and_keeps_margins() {
        let strokes = vec![stroke(400.0, 300.0), stroke(420.0, 700.0)];
        let png = ink_png(&strokes, 1.0, 48.0).unwrap().unwrap();
        let full = page_png(&strokes, 1.0, &page_fixture()).unwrap();
        assert!(png.len() < full.len(), "the crop is the smaller image");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']), "a real PNG");
    }

    #[test]
    fn a_page_without_ink_has_no_crop() {
        assert!(ink_png(&[], 1.0, 48.0).unwrap().is_none());
        let mut broken = stroke(10.0, 10.0);
        broken.samples.clear();
        broken.times.clear();
        assert!(ink_png(&[broken], 1.0, 48.0).unwrap().is_none());
    }

    #[test]
    fn the_crop_is_capped_for_the_model() {
        let strokes = vec![stroke(0.0, 0.0), stroke(20_000.0, 20_000.0)];
        let png = ink_png(&strokes, 2.0, 48.0).unwrap().unwrap();
        // A 20k-px span at scale 2 would be 40k px wide; the cap keeps the long side at OCR_MAX_SIDE.
        assert!(
            png.len() < 4_000_000,
            "capped image stays small ({} bytes)",
            png.len()
        );
    }

    fn page_fixture() -> PageMeta {
        PageMeta {
            id: "pg_test".to_owned(),
            title: "Test".to_owned(),
            kind: PageKind::landscape(),
            background: Background::Blank,
        }
    }
}
