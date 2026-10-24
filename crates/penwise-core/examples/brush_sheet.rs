//! Brush sheet: renders every contact stroke of a `.pwrec` fixture once per preset, one row per preset in
//! `Preset::ALL` order (row 1 = ballpoint … row 13 = shading), each row cropped to its ink, on a white page.
//! Layout is `penwise_core::raster::sheet::render_brush_sheet` (shared with the `raster_brush_sheet` golden).
//! The row legend goes to stdout.
//!
//! `cargo run -p penwise-core --features raster --example brush_sheet -- <in.pwrec> <out.png> [--rotate cw|ccw|none]`
//!
//! `--rotate` turns every row a quarter turn (default `none`), for handwriting recorded with the tablet sideways.
//!
//! Owner: slice e.

use penwise_core::Preset;
use penwise_core::codec::decode_pwrec;
use penwise_core::raster::png::encode_png;
use penwise_core::raster::sheet::{Rotation, SheetOptions, contact_strokes, render_brush_sheet};
use std::error::Error;
use std::io::Write;

const USAGE: &str = "usage: brush_sheet <in.pwrec> <out.png> [--rotate cw|ccw|none]";

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output, rotation) = parse(&args)?;
    let recording = decode_pwrec(&std::fs::read(input)?)?;
    let strokes = contact_strokes(&recording)?;
    if strokes.is_empty() {
        return Err("recording has no contact samples".into());
    }
    let options = SheetOptions {
        rotation,
        ..SheetOptions::DEFAULT
    };
    let sheet = render_brush_sheet(&strokes, &options)?;
    std::fs::write(output, encode_png(&sheet)?)?;
    let mut legend = std::io::stdout().lock();
    for (row, preset) in (1u32..).zip(Preset::ALL) {
        writeln!(legend, "row {row:>2}: {}", preset.name())?;
    }
    writeln!(
        legend,
        "wrote {output} ({}x{})",
        sheet.width(),
        sheet.height()
    )?;
    Ok(())
}

/// `<in> <out>` plus an optional `--rotate cw|ccw|none` anywhere after them.
fn parse(args: &[String]) -> Result<(&str, &str, Rotation), Box<dyn Error>> {
    let mut paths = Vec::new();
    let mut rotation = Rotation::None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--rotate" {
            rotation = match it.next().map(String::as_str) {
                Some("cw") => Rotation::Cw,
                Some("ccw") => Rotation::Ccw,
                Some("none") => Rotation::None,
                _ => return Err(USAGE.into()),
            };
        } else {
            paths.push(arg.as_str());
        }
    }
    let [input, output] = paths.as_slice() else {
        return Err(USAGE.into());
    };
    Ok((input, output, rotation))
}
