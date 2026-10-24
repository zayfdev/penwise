//! Brush sheet: renders every contact stroke of a `.pwrec` fixture once per preset, one row per preset in
//! `Preset::ALL` order (row 1 = ballpoint … row 13 = shading), on a white page. The row legend goes to stdout.
//!
//! `cargo run -p penwise-core --features raster --example brush_sheet -- <in.pwrec> <out.png>`
//!
//! Owner: slice e.

use penwise_core::codec::decode_pwrec;
use penwise_core::codec::pwrec::{Recording, RunKind};
use penwise_core::raster::png::encode_png;
use penwise_core::raster::stamps::CoverageBuffer;
use penwise_core::raster::tiny_skia::{Color as SkColor, Paint, Pixmap, Rect, Transform};
use penwise_core::raster::{Viewport, render_strokes};
use penwise_core::{Bounds, Preset, Sample, StrokeBuilder, StrokeGeometry};
use std::error::Error;
use std::io::Write;

/// Page px around the strokes in each row (fits the largest brush radius, airbrush 45 px).
const MARGIN: f32 = 60.0;
/// Sheets wider than this are scaled down.
const MAX_WIDTH_PX: f32 = 1600.0;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: brush_sheet <in.pwrec> <out.png>".into());
    };
    let recording = decode_pwrec(&std::fs::read(input)?)?;
    let strokes = contact_samples(&recording)?;
    let bounds = strokes.iter().flatten().fold(Bounds::EMPTY, |mut b, s| {
        b.include_point(s.x, s.y);
        b
    });
    if bounds.is_empty() {
        return Err("recording has no contact samples".into());
    }
    let layout = Layout::new(&bounds)?;
    let mut pixmap =
        Pixmap::new(layout.width_px, layout.row_px * rows()).ok_or("sheet too large")?;
    pixmap.fill(SkColor::WHITE);
    let mut scratch = CoverageBuffer::default();
    let mut legend = std::io::stdout().lock();
    for (row, preset) in (0u32..).zip(Preset::ALL) {
        let params = preset.params();
        let geometry: Vec<StrokeGeometry> = (0u64..)
            .zip(&strokes)
            .map(|(seed, samples)| StrokeBuilder::build(&params, seed, samples))
            .collect();
        render_strokes(&geometry, &layout.viewport(row), &mut pixmap, &mut scratch)?;
        separator(&mut pixmap, row * layout.row_px);
        writeln!(legend, "row {:>2}: {}", row + 1, preset.name())?;
    }
    std::fs::write(output, encode_png(&pixmap)?)?;
    writeln!(legend, "wrote {output}")?;
    Ok(())
}

fn rows() -> u32 {
    u32::try_from(Preset::ALL.len()).unwrap_or(u32::MAX)
}

/// Quantized samples of every contact run (hover runs skipped).
fn contact_samples(recording: &Recording) -> Result<Vec<Vec<Sample>>, Box<dyn Error>> {
    let ranges = recording.digitizer_ranges()?;
    let mut strokes = Vec::new();
    for run in recording
        .strokes
        .iter()
        .filter(|r| r.kind == RunKind::Contact)
    {
        let mut samples = Vec::new();
        run.to_samples(&ranges, &mut samples);
        if !samples.is_empty() {
            strokes.push(samples);
        }
    }
    Ok(strokes)
}

/// Row geometry: every row shows the same page rect (stroke bounds + margin) at one scale.
struct Layout {
    origin_x: f32,
    origin_y: f32,
    scale: f32,
    width_px: u32,
    row_px: u32,
}

impl Layout {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "sizes are checked finite and positive and capped before the casts"
    )]
    fn new(bounds: &Bounds) -> Result<Self, Box<dyn Error>> {
        let page_w = bounds.width() + 2.0 * MARGIN;
        let page_h = bounds.height() + 2.0 * MARGIN;
        let scale = if page_w > MAX_WIDTH_PX {
            MAX_WIDTH_PX / page_w
        } else {
            1.0
        };
        let (w, h) = ((page_w * scale).ceil(), (page_h * scale).ceil());
        if !(w.is_finite() && h.is_finite() && w <= MAX_WIDTH_PX + 1.0 && h <= 4096.0) {
            return Err("recording bounds too large for a sheet".into());
        }
        Ok(Self {
            origin_x: bounds.min_x - MARGIN,
            origin_y: bounds.min_y - MARGIN,
            scale,
            width_px: w as u32,
            row_px: h as u32,
        })
    }

    /// Viewport placing the page rect at pixel row `row`.
    #[allow(
        clippy::cast_precision_loss,
        reason = "pixel offsets are far below 2^24"
    )]
    fn viewport(&self, row: u32) -> Viewport {
        Viewport {
            origin_x: self.origin_x,
            origin_y: self.origin_y - (row * self.row_px) as f32 / self.scale,
            scale: self.scale,
        }
    }
}

/// 1 px light-gray line at the top of a row (row 0 has none).
#[allow(
    clippy::cast_precision_loss,
    reason = "pixel offsets are far below 2^24"
)]
fn separator(pixmap: &mut Pixmap, y: u32) {
    if y == 0 {
        return;
    }
    let Some(rect) = Rect::from_xywh(0.0, y as f32, pixmap.width() as f32, 1.0) else {
        return;
    };
    let mut paint = Paint::default();
    paint.set_color_rgba8(200, 200, 205, 255);
    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
}
