//! Page → PNG through the ink engine, so agents see exactly what the tablet drew.

use penwise_core::brush::{Color, Preset};
use penwise_core::builder::StrokeBuilder;
use penwise_core::geometry::StrokeGeometry;
use penwise_core::raster::png::encode_png;
use penwise_core::raster::{CoverageBuffer, Viewport, render_strokes};
use penwise_core::sample::Sample;
use tiny_skia::Pixmap;

use crate::state::StrokeMsg;

/// Page size in page px: the Tab S9 is 1600×2560.
pub const PAGE_WIDTH: f32 = 1600.0;
pub const PAGE_HEIGHT: f32 = 2560.0;
const SAMPLE_STRIDE: usize = 5;
const MIN_SCALE: f32 = 0.05;
const MAX_SCALE: f32 = 4.0;

/// Renders every stroke on a white page at `scale` pixels per page px.
pub fn page_png(strokes: &[StrokeMsg], scale: f32) -> Result<Vec<u8>, String> {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        1.0
    };
    let width = pixel_span(PAGE_WIDTH, scale);
    let height = pixel_span(PAGE_HEIGHT, scale);
    let mut pixmap = Pixmap::new(width, height).ok_or("page size out of range")?;
    pixmap.fill(tiny_skia::Color::WHITE);

    let geometries: Vec<StrokeGeometry> = strokes.iter().map(geometry_of).collect();
    let mut scratch = CoverageBuffer::default();
    render_strokes(
        &geometries,
        &Viewport {
            origin_x: 0.0,
            origin_y: 0.0,
            scale,
        },
        &mut pixmap,
        &mut scratch,
    )
    .map_err(|error| error.to_string())?;
    encode_png(&pixmap).map_err(|error| error.to_string())
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
